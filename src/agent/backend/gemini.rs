use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;

use crate::agent::{Agent, AgentContext, AgentKind, AgentResponse};
use crate::config::AppConfig;

/// Google Gemini agent.
pub struct GeminiAgent {
    client: Client,
    api_key: String,
    model: String,
}

impl GeminiAgent {
    pub fn new(config: &AppConfig) -> anyhow::Result<Self> {
        let api_key = config
            .gemini_api_key
            .clone()
            .ok_or_else(|| anyhow::anyhow!("GEMINI_API_KEY not set"))?;

        Ok(Self {
            client: Client::new(),
            api_key,
            model: config.gemini_model.clone(),
        })
    }

    fn build_prompt(&self, context: &AgentContext) -> String {
        let mut prompt = String::new();
        prompt.push_str("You are an AI assistant working as part of an orchestrated development team. Follow instructions precisely and return well-structured markdown.\n\n");
        for file in &context.context_files {
            prompt.push_str(&format!("--- {} ---\n{}\n\n", file.name, file.content));
        }
        prompt.push_str("--- Instruction ---\n");
        prompt.push_str(&context.instruction);
        prompt
    }
}

#[async_trait]
impl Agent for GeminiAgent {
    async fn respond(&self, context: &AgentContext) -> anyhow::Result<AgentResponse> {
        let prompt = self.build_prompt(context);

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.model, self.api_key
        );

        let body = json!({
            "contents": [
                {
                    "parts": [
                        { "text": prompt }
                    ]
                }
            ],
            "generationConfig": {
                "temperature": 0.3,
                "maxOutputTokens": 4096
            }
        });

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Gemini request failed: {}", e))?;

        let status = resp.status();
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Gemini HTTP {}: {}", status, body_text);
        }

        let json: serde_json::Value = resp.json().await?;
        let content = json["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let tokens = json["usageMetadata"]["totalTokenCount"].as_u64();

        Ok(AgentResponse {
            content,
            model_used: self.model.clone(),
            tokens_used: tokens,
            is_paid: true,
        })
    }

    fn kind(&self) -> AgentKind {
        AgentKind::Gemini
    }
}

#[cfg(test)]
mod tests {
    use crate::agent::{Agent, AgentContext, ContextFile};
    use crate::config::AppConfig;
    use crate::machine_config::{LocalLlmCliConfig, ProviderMode};

    use super::GeminiAgent;

    #[test]
    fn new_requires_api_key() {
        let err = GeminiAgent::new(&base_config()).err().expect("missing api key should fail");
        assert!(err.to_string().contains("GEMINI_API_KEY not set"));
    }

    #[test]
    fn build_prompt_includes_system_context_files_and_instruction() {
        let mut config = base_config();
        config.gemini_api_key = Some("sk-gemini".into());
        config.gemini_model = "gemini-test".into();
        let agent = GeminiAgent::new(&config).expect("agent should be created");
        let context = AgentContext {
            context_files: vec![
                ContextFile {
                    name: "status.md".into(),
                    content: "Cycle 5".into(),
                },
                ContextFile {
                    name: "review.md".into(),
                    content: "Must-fix: add validation".into(),
                },
            ],
            role: "fixer".into(),
            instruction: "Resolve the must-fix items.".into(),
        };

        let prompt = agent.build_prompt(&context);
        assert!(prompt.starts_with("You are an AI assistant working as part of an orchestrated development team."));
        assert!(prompt.contains("--- status.md ---"));
        assert!(prompt.contains("Cycle 5"));
        assert!(prompt.contains("--- review.md ---"));
        assert!(prompt.contains("Must-fix: add validation"));
        assert!(prompt.contains("--- Instruction ---"));
        assert!(prompt.contains("Resolve the must-fix items."));
    }

    #[test]
    fn kind_is_gemini() {
        let mut config = base_config();
        config.gemini_api_key = Some("sk-gemini".into());
        let agent = GeminiAgent::new(&config).expect("agent should be created");
        assert_eq!(agent.kind(), crate::agent::AgentKind::Gemini);
    }

    fn base_config() -> AppConfig {
        AppConfig {
            local_llm_mode: ProviderMode::Http,
            local_llm_endpoint: String::new(),
            local_llm_model: String::new(),
            local_llm_cli: LocalLlmCliConfig::default(),
            anthropic_api_key: None,
            anthropic_model: String::new(),
            anthropic_mode: ProviderMode::Http,
            anthropic_cli: LocalLlmCliConfig::default(),
            gemini_api_key: None,
            gemini_model: String::new(),
            gemini_mode: ProviderMode::Http,
            gemini_cli: LocalLlmCliConfig::default(),
            openai_api_key: None,
            codex_model: String::new(),
            openai_mode: ProviderMode::Http,
            openai_cli: LocalLlmCliConfig::default(),
        }
    }
}
