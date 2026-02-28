use serde::{Deserialize, Serialize};

/// Application configuration resolved from environment variables and .env file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    // Local LLM (OpenAI-compatible)
    pub local_llm_endpoint: String,
    pub local_llm_model: String,

    // Anthropic (Claude)
    pub anthropic_api_key: Option<String>,
    pub anthropic_model: String,

    // Google Gemini
    pub gemini_api_key: Option<String>,
    pub gemini_model: String,

    // OpenAI / Codex
    pub openai_api_key: Option<String>,
    pub codex_model: String,
}

impl AppConfig {
    /// Load configuration from environment variables.
    /// Call `dotenvy::dotenv().ok()` before this to load .env file.
    pub fn from_env() -> Self {
        Self {
            local_llm_endpoint: std::env::var("LOCAL_LLM_ENDPOINT")
                .unwrap_or_else(|_| "http://localhost:11434/v1".to_string()),
            local_llm_model: std::env::var("LOCAL_LLM_MODEL")
                .unwrap_or_else(|_| "llama3.2".to_string()),

            anthropic_api_key: std::env::var("ANTHROPIC_API_KEY").ok(),
            anthropic_model: std::env::var("ANTHROPIC_MODEL")
                .unwrap_or_else(|_| "claude-sonnet-4-20250514".to_string()),

            gemini_api_key: std::env::var("GEMINI_API_KEY").ok(),
            gemini_model: std::env::var("GEMINI_MODEL")
                .unwrap_or_else(|_| "gemini-2.0-flash".to_string()),

            openai_api_key: std::env::var("OPENAI_API_KEY").ok(),
            codex_model: std::env::var("CODEX_MODEL")
                .unwrap_or_else(|_| "gpt-4.1".to_string()),
        }
    }

    pub fn has_anthropic(&self) -> bool {
        self.anthropic_api_key.as_ref().is_some_and(|k| !k.is_empty())
    }

    pub fn has_gemini(&self) -> bool {
        self.gemini_api_key.as_ref().is_some_and(|k| !k.is_empty())
    }

    pub fn has_openai(&self) -> bool {
        self.openai_api_key.as_ref().is_some_and(|k| !k.is_empty())
    }
}
