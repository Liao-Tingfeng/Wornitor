use super::adapter::{
    AnalysisImage, AnalysisResult, ConnectionStatus, LlmAdapter, LlmError, ReportContext, UsageInfo,
};
use crate::config::LlmConfig;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::{Deserialize, Serialize, de::Error as SerdeError};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Types for the Ollama Chat API
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    stream: bool,
    options: Option<OllamaOptions>,
}

#[derive(Serialize)]
struct OllamaMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    images: Option<Vec<String>>,
}

#[derive(Serialize)]
struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<u32>,
}

#[derive(Deserialize)]
struct ChatResponse {
    message: OllamaResponseMessage,
    #[serde(default)]
    done: bool,
}

#[derive(Deserialize)]
struct OllamaResponseMessage {
    content: String,
}

#[derive(Deserialize)]
struct OllamaErrorBody {
    #[serde(default)]
    error: String,
}

#[derive(Deserialize)]
struct ListModelsResponse {
    models: Vec<OllamaModelInfo>,
}

#[derive(Deserialize)]
struct OllamaModelInfo {
    name: String,
}

// ---------------------------------------------------------------------------
// Ollama adapter
// ---------------------------------------------------------------------------

pub struct OllamaAdapter {
    config: LlmConfig,
    client: reqwest::Client,
}

impl OllamaAdapter {
    pub fn new(config: LlmConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to create reqwest Client for Ollama");

        Self { config, client }
    }

    fn build_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers
    }

    /// Build the full API URL from the base configuration.
    fn api_url(&self, path: &str) -> String {
        let base = self.config.base_url.trim_end_matches('/');
        format!("{}{}", base, path)
    }

    async fn chat(
        &self,
        content: String,
        images: Option<Vec<String>>,
        num_predict: u32,
        temperature: f32,
    ) -> Result<String, LlmError> {
        let url = self.api_url("/api/chat");

        let message = OllamaMessage {
            role: "user".to_string(),
            content,
            images,
        };

        let request = ChatRequest {
            model: self.config.model.clone(),
            messages: vec![message],
            stream: false,
            options: Some(OllamaOptions {
                temperature: Some(temperature),
                num_predict: Some(num_predict),
            }),
        };

        let headers = self.build_headers();
        let response = self
            .client
            .post(&url)
            .headers(headers)
            .json(&request)
            .send()
            .await
            .map_err(LlmError::from_reqwest_error)?;

        let status = response.status();

        if status.is_success() {
            let chat_resp: ChatResponse = response.json().await.map_err(LlmError::from_reqwest_error)?;
            return Ok(chat_resp.message.content);
        }

        // Extract error
        let error_message = response
            .json::<OllamaErrorBody>()
            .await
            .ok()
            .map(|b| b.error)
            .unwrap_or_else(|| "Unknown Ollama error".to_string());

        Err(LlmError::ApiError {
            status: status.as_u16(),
            message: error_message,
        })
    }
}

#[async_trait::async_trait]
impl LlmAdapter for OllamaAdapter {
    async fn analyze_screenshots(
        &self,
        images: &[AnalysisImage],
        prompt: &str,
    ) -> Result<(AnalysisResult, Option<UsageInfo>), LlmError> {
        // Ollama expects images as a plain base64 string array (no data URI prefix).
        let image_data: Vec<String> = images.iter().map(|img| img.data.clone()).collect();

        let response_text = self
            .chat(
                prompt.to_string(),
                Some(image_data),
                256,
                0.1,
            )
            .await?;

        // Try to parse the response as JSON — some Ollama models may wrap in markdown
        let cleaned = response_text
            .trim()
            .strip_prefix("```json")
            .or_else(|| response_text.trim().strip_prefix("```"))
            .and_then(|s| s.strip_suffix("```"))
            .map(|s| s.trim())
            .unwrap_or(response_text.trim());

        let result: AnalysisResult = serde_json::from_str(cleaned).map_err(|e| {
            LlmError::Parse(serde::de::Error::custom(format!(
                "Failed to parse AnalysisResult from Ollama response: {e}. Raw: {response_text}"
            )))
        })?;

        if !(0.0..=1.0).contains(&result.confidence) {
            return Err(LlmError::Parse(serde::de::Error::custom(
                "confidence must be between 0.0 and 1.0".to_string(),
            )));
        }

        // Ollama local responses don't include token usage
        Ok((result, None))
    }

    async fn generate_report(
        &self,
        context: &ReportContext,
        prompt: &str,
    ) -> Result<(String, Option<UsageInfo>), LlmError> {
        let context_json =
            serde_json::to_string_pretty(&context).expect("ReportContext serialization should not fail");
        let full_prompt = format!("{}\n\nContext:\n{}", prompt, context_json);

        let content = self.chat(full_prompt, None, 1024, 0.3).await?;
        // Ollama local responses don't include token usage
        Ok((content, None))
    }

    async fn test_connection(&self) -> Result<ConnectionStatus, LlmError> {
        let url = self.api_url("/api/tags");

        let headers = self.build_headers();

        match self.client.get(&url).headers(headers).send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    let models: ListModelsResponse = resp.json().await.map_err(LlmError::from_reqwest_error)?;
                    let model_found = models.models.iter().any(|m| m.name == self.config.model);

                    Ok(ConnectionStatus {
                        success: true,
                        message: if model_found {
                            format!("Connected. Model '{}' is available.", self.config.model)
                        } else {
                            format!(
                                "Connected, but model '{}' not found locally. Pull it with: ollama pull {}",
                                self.config.model, self.config.model
                            )
                        },
                        model: Some(self.config.model.clone()),
                    })
                } else {
                    let status = resp.status();
                    let body = resp.json::<OllamaErrorBody>().await.ok();
                    let msg = body.map(|b| b.error).unwrap_or_else(|| "Unknown".to_string());
                    Ok(ConnectionStatus {
                        success: false,
                        message: format!("Ollama error ({}): {}", status.as_u16(), msg),
                        model: None,
                    })
                }
            }
            Err(e) => {
                let message = if e.is_timeout() {
                    "Connection timed out. Is Ollama running?".to_string()
                } else if e.is_connect() {
                    format!(
                        "Cannot connect to {}. Make sure Ollama is started.",
                        self.config.base_url
                    )
                } else {
                    format!("Network error: {}", e)
                };

                Ok(ConnectionStatus {
                    success: false,
                    message,
                    model: None,
                })
            }
        }
    }

    async fn list_models(&self) -> Result<Vec<String>, LlmError> {
        let url = self.api_url("/api/tags");

        let headers = self.build_headers();
        let resp = self
            .client
            .get(&url)
            .headers(headers)
            .send()
            .await
            .map_err(LlmError::from_reqwest_error)?;

        if !resp.status().is_success() {
            return Err(LlmError::ApiError {
                status: resp.status().as_u16(),
                message: "Failed to list Ollama models".to_string(),
            });
        }

        let body: ListModelsResponse = resp.json().await.map_err(LlmError::from_reqwest_error)?;
        Ok(body.models.into_iter().map(|m| m.name).collect())
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn provider_name(&self) -> &str {
        "ollama"
    }
}
