pub mod adapter;
pub mod kimi_batch;
pub mod ollama;
pub mod openai;

use adapter::{LlmAdapter, LlmError};
use crate::config::LlmConfig;

/// Create an LLM adapter based on the provider type in the config.
///
/// Supported providers:
/// - `"openai"` — OpenAI-compatible API (ChatGPT, GPT-4, etc.)
/// - `"anthropic"` — _Not yet implemented_
/// - `"ollama"` — Local Ollama instance
/// - `"custom"` — Treated as OpenAI-compatible for now
pub fn create_adapter(config: &LlmConfig) -> Result<Box<dyn LlmAdapter>, LlmError> {
    match config.provider.to_lowercase().as_str() {
        "openai" | "custom" => Ok(Box::new(openai::OpenAiAdapter::new(config.clone()))),
        "ollama" => Ok(Box::new(ollama::OllamaAdapter::new(config.clone()))),
        other => Err(LlmError::Config(format!(
            "Unsupported LLM provider: '{}'. Supported values: openai, ollama, custom",
            other
        ))),
    }
}
