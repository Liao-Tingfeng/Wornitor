//! Kimi Batch API adapter.
//!
//! Implements the batch inference pattern:
//!   1. Submit a batch of chat completion requests → get a batch_id
//!   2. Poll batch status periodically
//!   3. Fetch results when completed
//!
//! Compatible with both Kimi and OpenAI Batch APIs (POST /v1/batch/create, etc.).

use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::config::LlmConfig;
use crate::llm::adapter::{AnalysisResult, LlmError};

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

/// Request body for POST /v1/batch/create
#[derive(Debug, Clone, Serialize)]
pub struct BatchCreateRequest {
    pub model: String,
    /// JSONL lines — each line is a complete chat completion request.
    pub input: Vec<BatchInputItem>,
    /// Expected completion window (e.g. "24h"). Kimi supports up to 24h.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_window: Option<String>,
}

/// One line in the batch input — mirrors a single chat completion request.
#[derive(Debug, Clone, Serialize)]
pub struct BatchInputItem {
    /// Client-defined unique ID (returned in the result for correlation).
    pub custom_id: String,
    /// HTTP method — always "POST" for chat completions.
    pub method: String,
    /// Endpoint path — "/v1/chat/completions".
    pub url: String,
    /// The actual chat completion request body.
    pub body: BatchRequestBody,
}

/// The request body for a single chat completion within the batch.
#[derive(Debug, Clone, Serialize)]
pub struct BatchRequestBody {
    pub model: String,
    pub messages: Vec<BatchMessage>,
    pub max_tokens: u32,
}

/// A single message in a batch request.
#[derive(Debug, Clone, Serialize)]
pub struct BatchMessage {
    pub role: String,
    pub content: Vec<BatchContentPart>,
}

/// A content part — either text or image_url.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum BatchContentPart {
    Text { r#type: String, text: String },
    ImageUrl {
        r#type: String,
        #[serde(rename = "image_url")]
        image_url: BatchImageUrl,
    },
}

/// image_url content part.
#[derive(Debug, Clone, Serialize)]
pub struct BatchImageUrl {
    pub url: String, // "data:image/jpeg;base64,..."
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// Response from POST /v1/batch/create
#[derive(Debug, Clone, Deserialize)]
pub struct BatchCreateResponse {
    pub id: String,
    pub status: String,
    #[serde(default)]
    pub request_counts: Option<BatchRequestCounts>,
    #[serde(default)]
    pub errors: Option<Vec<BatchError>>,
    #[serde(default)]
    pub output_file: Option<String>,
}

/// Response from GET /v1/batch/:id/retrieve
#[derive(Debug, Clone, Deserialize)]
pub struct BatchRetrieveResponse {
    pub id: String,
    pub status: String,
    #[serde(default)]
    pub output_file: Option<String>,
    #[serde(default)]
    pub errors: Option<Vec<BatchError>>,
    #[serde(default)]
    pub request_counts: Option<BatchRequestCounts>,
}

/// Request counts for a batch.
#[derive(Debug, Clone, Deserialize)]
pub struct BatchRequestCounts {
    pub total: u32,
    pub completed: u32,
    pub failed: u32,
}

/// An error reported by the batch API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchError {
    pub code: String,
    pub message: String,
}

/// One line in the batch output file (JSONL).
#[derive(Debug, Clone, Deserialize)]
pub struct BatchCompletionResult {
    pub id: String,
    pub custom_id: String,
    pub response: BatchCompletionResponse,
    #[serde(default)]
    pub error: Option<BatchError>,
}

/// The response body for one completion within the batch.
#[derive(Debug, Clone, Deserialize)]
pub struct BatchCompletionResponse {
    pub status_code: u16,
    pub request_id: String,
    pub body: BatchCompletionBody,
}

/// The parsed chat completion response body.
#[derive(Debug, Clone, Deserialize)]
pub struct BatchCompletionBody {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<BatchChoice>,
    #[serde(default)]
    pub usage: Option<BatchUsage>,
}

/// One choice in a batch completion response.
#[derive(Debug, Clone, Deserialize)]
pub struct BatchChoice {
    pub index: u32,
    pub message: BatchResponseMessage,
    pub finish_reason: String,
}

/// The response message from a batch completion.
#[derive(Debug, Clone, Deserialize)]
pub struct BatchResponseMessage {
    pub role: String,
    pub content: String,
}

/// Token usage for one batch completion.
#[derive(Debug, Clone, Deserialize)]
pub struct BatchUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

/// Adapter for submitting and polling Kimi (OpenAI-compatible) batch jobs.
pub struct KimiBatchAdapter {
    config: LlmConfig,
    client: reqwest::Client,
}

impl KimiBatchAdapter {
    /// Create a new KimiBatchAdapter from the given config.
    pub fn new(config: LlmConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();
        Self { config, client }
    }

    /// Build common request headers (Authorization, Content-Type).
    fn build_headers(&self) -> Result<reqwest::header::HeaderMap, LlmError> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            "application/json".parse().unwrap(),
        );
        if let Some(ref key) = self.config.api_key {
            let auth_value = format!("Bearer {}", key);
            headers.insert(
                reqwest::header::AUTHORIZATION,
                reqwest::header::HeaderValue::from_str(&auth_value)
                    .map_err(|e| LlmError::Config(format!("Invalid API key header: {e}")))?,
            );
        }
        Ok(headers)
    }

    /// Submit a batch of analysis requests.
    /// Returns the batch_id on success.
    pub async fn submit_batch(&self, items: Vec<BatchInputItem>) -> Result<String, LlmError> {
        let base_url = self.config.base_url.trim_end_matches('/');
        let url = format!("{}/v1/batch/create", base_url);

        let request = BatchCreateRequest {
            model: self.config.model.clone(),
            input: items,
            completion_window: Some("24h".to_string()),
        };

        let response = self
            .client
            .post(&url)
            .headers(self.build_headers()?)
            .json(&request)
            .send()
            .await
            .map_err(LlmError::from_reqwest_error)?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            eprintln!("[KIMI_BATCH] create failed (HTTP {status}): {text}");
            return Err(LlmError::ApiError {
                status,
                message: text,
            });
        }

        let body: BatchCreateResponse = response
            .json()
            .await
            .map_err(LlmError::from_reqwest_error)?;

        eprintln!(
            "[KIMI_BATCH] Batch created: id={}, status={}",
            body.id, body.status
        );

        Ok(body.id)
    }

    /// Check batch status by batch_id.
    pub async fn retrieve_batch(&self, batch_id: &str) -> Result<BatchRetrieveResponse, LlmError> {
        let base_url = self.config.base_url.trim_end_matches('/');
        let url = format!("{}/v1/batch/{}", base_url, batch_id);

        let response = self
            .client
            .get(&url)
            .headers(self.build_headers()?)
            .send()
            .await
            .map_err(LlmError::from_reqwest_error)?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            return Err(LlmError::ApiError {
                status,
                message: text,
            });
        }

        let body: BatchRetrieveResponse = response
            .json()
            .await
            .map_err(LlmError::from_reqwest_error)?;

        Ok(body)
    }

    /// Fetch completed batch results from the output_file URL or path.
    ///
    /// `output_file` may be a full URL (e.g. from Kimi) or a relative path
    /// (from OpenAI-compatible APIs). Returns parsed batch results in order.
    pub async fn fetch_results(
        &self,
        output_file: &str,
    ) -> Result<Vec<BatchCompletionResult>, LlmError> {
        let base_url = self.config.base_url.trim_end_matches('/');
        let url = if output_file.starts_with("http://") || output_file.starts_with("https://") {
            output_file.to_string()
        } else {
            format!("{}/{}", base_url, output_file)
        };

        let response = self
            .client
            .get(&url)
            .headers(self.build_headers()?)
            .send()
            .await
            .map_err(LlmError::from_reqwest_error)?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            return Err(LlmError::ApiError {
                status,
                message: text,
            });
        }

        let text = response.text().await.map_err(LlmError::from_reqwest_error)?;

        // Parse JSONL — each line is one result
        let results: Vec<BatchCompletionResult> = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str::<BatchCompletionResult>(l).ok())
            .collect();

        eprintln!(
            "[KIMI_BATCH] Fetched {} results from {}",
            results.len(),
            output_file
        );

        Ok(results)
    }

    /// Parse the raw JSON content from a `BatchChoice` into an `AnalysisResult`.
    ///
    /// The LLM is expected to respond with valid JSON matching `AnalysisResult`.
    pub fn parse_analysis_from_choice(
        choice: &BatchChoice,
    ) -> Result<AnalysisResult, LlmError> {
        let content = &choice.message.content;
        // Strip any markdown code fences if present
        let cleaned = content
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();
        let result: AnalysisResult = serde_json::from_str(cleaned)?;
        Ok(result)
    }

    /// Extract token usage from a batch completion body.
    pub fn extract_usage(body: &BatchCompletionBody) -> Option<crate::llm::adapter::UsageInfo> {
        body.usage.as_ref().map(|u| crate::llm::adapter::UsageInfo {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
        })
    }

    /// Build a `BatchInputItem` for screenshot analysis.
    ///
    /// `custom_id` should be unique per item (e.g. the screenshot UUID).
    pub fn build_analysis_item(
        custom_id: &str,
        model: &str,
        base64_image: &str,
        prompt: &str,
        max_tokens: u32,
    ) -> BatchInputItem {
        let messages = vec![
            // System message via user role — some batch APIs don't support system role
            BatchMessage {
                role: "user".to_string(),
                content: vec![
                    BatchContentPart::Text {
                        r#type: "text".to_string(),
                        text: prompt.to_string(),
                    },
                    BatchContentPart::ImageUrl {
                        r#type: "image_url".to_string(),
                        image_url: BatchImageUrl {
                            url: format!("data:image/jpeg;base64,{}", base64_image),
                        },
                    },
                ],
            },
        ];

        BatchInputItem {
            custom_id: custom_id.to_string(),
            method: "POST".to_string(),
            url: "/v1/chat/completions".to_string(),
            body: BatchRequestBody {
                model: model.to_string(),
                messages,
                max_tokens,
            },
        }
    }
}
