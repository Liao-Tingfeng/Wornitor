use super::adapter::{
    estimate_cost, AnalysisImage, AnalysisResult, ConnectionStatus, LlmAdapter, LlmError,
    ReportContext, UsageInfo,
};
use crate::config::LlmConfig;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize, de::Error as SerdeError};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Types for the OpenAI Chat Completions API
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<ResponseFormat>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum MessageContent {
    Text(String),
    Multi(Vec<ContentPart>),
}

#[derive(Serialize)]
struct Message {
    role: String,
    content: MessageContent,
}

#[derive(Serialize)]
#[serde(untagged)]
enum ContentPart {
    Text { r#type: String, text: String },
    Image {
        r#type: String,
        image_url: ImageUrl,
    },
}

#[derive(Serialize)]
struct ImageUrl {
    url: String,
    detail: String,
}

#[derive(Serialize)]
struct ResponseFormat {
    r#type: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct Choice {
    message: ChoiceMessage,
}

#[derive(Deserialize)]
struct ChoiceMessage {
    content: Option<String>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct Usage {
    prompt_tokens: u32,
    completion_tokens: u32,
    #[serde(default)]
    total_tokens: Option<u32>,
}

#[derive(Deserialize)]
struct ApiErrorBody {
    #[serde(default)]
    error: Option<ApiErrorDetail>,
}

#[derive(Deserialize)]
struct ApiErrorDetail {
    #[serde(default)]
    message: String,
    #[serde(rename = "type", default)]
    error_type: String,
}

#[derive(Deserialize)]
struct ModelsResponse {
    data: Vec<ModelInfo>,
}

#[derive(Deserialize)]
struct ModelInfo {
    id: String,
}

// ---------------------------------------------------------------------------
// OpenAI adapter
// ---------------------------------------------------------------------------

pub struct OpenAiAdapter {
    config: LlmConfig,
    client: reqwest::Client,
}

impl OpenAiAdapter {
    pub fn new(config: LlmConfig) -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(60))
            .build()
            .expect("Failed to create reqwest Client for OpenAI");

        Self { config, client }
    }

    fn build_headers(&self) -> Result<HeaderMap, LlmError> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        if let Some(ref key) = self.config.api_key {
            let bearer = format!("Bearer {}", key);
            let mut hv = HeaderValue::from_str(&bearer)
                .map_err(|e| LlmError::Config(format!("Invalid API key header: {e}")))?;
            hv.set_sensitive(true);
            headers.insert(AUTHORIZATION, hv);
        }

        Ok(headers)
    }

    async fn chat_completion(
        &self,
        messages: Vec<Message>,
        max_tokens: u32,
        json_mode: bool,
    ) -> Result<(String, Option<UsageInfo>), LlmError> {
        let base_url = self.config.base_url.trim_end_matches('/');
        let url = format!("{}/chat/completions", base_url);

        let mut request = ChatRequest {
            model: self.config.model.clone(),
            messages,
            max_tokens,
            temperature: None,
            response_format: None,
        };

        // Some LLM providers (e.g. Kimi) don't support response_format=json_object.
        // Instead, the prompt already instructs JSON output.

        let headers = self.build_headers()?;
        let mut attempt: u32 = 0;
        let max_retries: u32 = 3;

        loop {
            let response = self
                .client
                .post(&url)
                .headers(headers.clone())
                .json(&request)
                .send()
                .await
                .map_err(LlmError::from_reqwest_error)?;

            let status = response.status();

            if status.is_success() {
                let chat_resp: ChatResponse = response.json().await.map_err(LlmError::from_reqwest_error)?;
                let choice = chat_resp
                    .choices
                    .into_iter()
                    .next()
                    .ok_or_else(|| LlmError::Parse(serde_json::from_str::<serde_json::Value>("{}").unwrap_err()))?;

                let content = choice
                    .message
                    .content
                    .ok_or_else(|| LlmError::ApiError {
                        status: status.as_u16(),
                        message: "Empty response content".to_string(),
                    })?;

                // Convert usage to public type and log cost
                let usage = chat_resp.usage.map(|u| UsageInfo {
                    prompt_tokens: u.prompt_tokens,
                    completion_tokens: u.completion_tokens,
                    total_tokens: u.total_tokens.unwrap_or(u.prompt_tokens + u.completion_tokens),
                });

                if let Some(ref u) = usage {
                    let cost = estimate_cost(&self.config.model, u);
                    eprintln!(
                        "[LLM-COST] model={}, prompt_tokens={}, completion_tokens={}, total_tokens={}, estimated_cost=¥{:.6}",
                        self.config.model, u.prompt_tokens, u.completion_tokens, u.total_tokens, cost
                    );
                }

                return Ok((content, usage));
            }

            if status == http_status::TOO_MANY_REQUESTS && attempt < max_retries {
                let retry_after = response
                    .headers()
                    .get("Retry-After")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(2u64.pow(attempt + 1));

                tokio::time::sleep(Duration::from_secs(retry_after)).await;
                attempt += 1;
                continue;
            }

            // Try to extract an error message from the body
            let error_message = if status.is_client_error() || status.is_server_error() {
                response
                    .json::<ApiErrorBody>()
                    .await
                    .ok()
                    .and_then(|b| b.error)
                    .map(|e| e.message)
                    .unwrap_or_else(|| "Unknown API error".to_string())
            } else {
                "Unknown error".to_string()
            };

            return Err(LlmError::ApiError {
                status: status.as_u16(),
                message: error_message,
            });
        }
    }
}

#[async_trait::async_trait]
impl LlmAdapter for OpenAiAdapter {
    async fn analyze_screenshots(
        &self,
        images: &[AnalysisImage],
        prompt: &str,
    ) -> Result<(AnalysisResult, Option<UsageInfo>), LlmError> {
        let mut content_parts: Vec<ContentPart> = Vec::with_capacity(1 + images.len());

        // Text prompt first
        content_parts.push(ContentPart::Text {
            r#type: "text".to_string(),
            text: prompt.to_string(),
        });

        // Append each image
        for img in images {
            // The OpenAI API expects: "data:{media_type};base64,{data}"
            let url = format!("data:{};base64,{}", img.media_type, img.data);
            content_parts.push(ContentPart::Image {
                r#type: "image_url".to_string(),
                image_url: ImageUrl {
                    url,
                    detail: "low".to_string(),
                },
            });
        }

        let message = Message {
            role: "user".to_string(),
            content: MessageContent::Multi(content_parts),
        };

        let (raw, usage) = self
            .chat_completion(vec![message], self.config.max_tokens, true)
            .await?;

        let result: AnalysisResult = serde_json::from_str(&raw).map_err(|e| {
            LlmError::Parse(serde::de::Error::custom(format!(
                "Failed to parse AnalysisResult from LLM response: {e}. Raw: {raw}"
            )))
        })?;

        // Validate confidence range
        if !(0.0..=1.0).contains(&result.confidence) {
            return Err(LlmError::Parse(serde::de::Error::custom(
                "confidence must be between 0.0 and 1.0".to_string(),
            )));
        }

        Ok((result, usage))
    }

    async fn generate_report(
        &self,
        context: &ReportContext,
        prompt: &str,
    ) -> Result<(String, Option<UsageInfo>), LlmError> {
        let context_json =
            serde_json::to_string_pretty(&context).expect("ReportContext serialization should not fail");

        let full_prompt = format!("{}\n\nContext:\n{}", prompt, context_json);

        let message = Message {
            role: "user".to_string(),
            content: MessageContent::Text(full_prompt),
        };

        self.chat_completion(vec![message], self.config.max_tokens, false).await
    }

    async fn test_connection(&self) -> Result<ConnectionStatus, LlmError> {
        let base_url = self.config.base_url.trim_end_matches('/');
        let url = format!("{}/models", base_url);

        let headers = self.build_headers()?;

        match self.client.get(&url).headers(headers).send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    let models: ModelsResponse = resp.json().await.map_err(LlmError::from_reqwest_error)?;
                    let model_found = models.data.iter().any(|m| m.id == self.config.model);
                    Ok(ConnectionStatus {
                        success: true,
                        message: if model_found {
                            format!("Connected. Model '{}' is available.", self.config.model)
                        } else {
                            format!(
                                "Connected, but model '{}' not found in available models list.",
                                self.config.model
                            )
                        },
                        model: Some(self.config.model.clone()),
                    })
                } else {
                    let status = resp.status();
                    let body = resp.json::<ApiErrorBody>().await.ok();
                    let msg = body
                        .and_then(|b| b.error)
                        .map(|e| e.message)
                        .unwrap_or_else(|| "Connection failed".to_string());
                    Ok(ConnectionStatus {
                        success: false,
                        message: format!("API error ({}): {}", status.as_u16(), msg),
                        model: None,
                    })
                }
            }
            Err(e) => Ok(ConnectionStatus {
                success: false,
                message: format!("Network error: {}", e),
                model: None,
            }),
        }
    }

    async fn list_models(&self) -> Result<Vec<String>, LlmError> {
        let base_url = self.config.base_url.trim_end_matches('/');
        let url = format!("{}/models", base_url);

        let headers = self.build_headers()?;
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
                message: "Failed to list models".to_string(),
            });
        }

        let models: ModelsResponse = resp.json().await.map_err(LlmError::from_reqwest_error)?;
        Ok(models.data.into_iter().map(|m| m.id).collect())
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn provider_name(&self) -> &str {
        &self.config.provider
    }
}

// Small helper: HTTP status constants
mod http_status {
    pub const TOO_MANY_REQUESTS: u16 = 429;
}
