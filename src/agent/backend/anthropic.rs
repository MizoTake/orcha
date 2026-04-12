use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;

use crate::agent::{Agent, AgentContext, AgentKind, AgentResponse};
use crate::config::AppConfig;

/// Anthropic Claude agent.
pub struct AnthropicAgent {
    client: Client,
    api_key: String,
    model: String,
}

impl AnthropicAgent {
    pub fn new(config: &AppConfig) -> anyhow::Result<Self> {
        let api_key = config
            .anthropic_api_key
            .clone()
            .ok_or_else(|| anyhow::anyhow!("ANTHROPIC_API_KEY not set"))?;

        Ok(Self {
            client: Client::new(),
            api_key,
            model: config.anthropic_model.clone(),
        })
    }

    fn build_prompt(&self, context: &AgentContext) -> String {
        let mut prompt = String::new();
        for file in &context.context_files {
            prompt.push_str(&format!("--- {} ---\n{}\n\n", file.name, file.content));
        }
        prompt.push_str("--- Instruction ---\n");
        prompt.push_str(&context.instruction);
        prompt
    }
}

#[async_trait]
impl Agent for AnthropicAgent {
    async fn respond(&self, context: &AgentContext) -> anyhow::Result<AgentResponse> {
        let prompt = self.build_prompt(context);

        let body = json!({
            "model": &self.model,
            "max_tokens": 4096,
            "messages": [
                {
                    "role": "user",
                    "content": prompt
                }
            ],
            "system": "You are an AI assistant working as part of an orchestrated development team. Follow instructions precisely and return well-structured markdown."
        });

        let resp = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Anthropic request failed: {}", e))?;

        let status = resp.status();
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Anthropic HTTP {}: {}", status, body_text);
        }

        let json: serde_json::Value = resp.json().await?;
        let content = json["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let input_tokens = json["usage"]["input_tokens"].as_u64().unwrap_or(0);
        let output_tokens = json["usage"]["output_tokens"].as_u64().unwrap_or(0);

        Ok(AgentResponse {
            content,
            model_used: self.model.clone(),
            tokens_used: Some(input_tokens + output_tokens),
            is_paid: true,
        })
    }

    fn kind(&self) -> AgentKind {
        AgentKind::Claude
    }
}

#[cfg(test)]
mod tests {
    use crate::agent::{Agent, AgentContext, ContextFile};
    use crate::config::AppConfig;
    use crate::machine_config::{LocalLlmCliConfig, ProviderMode};

    use super::AnthropicAgent;

    #[test]
    fn new_requires_api_key() {
        let err = AnthropicAgent::new(&base_config()).err().expect("missing api key should fail");
        assert!(err.to_string().contains("ANTHROPIC_API_KEY not set"));
    }

    #[test]
    fn build_prompt_includes_context_files_and_instruction() {
        let mut config = base_config();
        config.anthropic_api_key = Some("sk-ant".into());
        config.anthropic_model = "claude-test".into();
        let agent = AnthropicAgent::new(&config).expect("agent should be created");
        let context = AgentContext {
            context_files: vec![
                ContextFile {
                    name: "status.md".into(),
                    content: "Cycle 3".into(),
                },
                ContextFile {
                    name: "task.md".into(),
                    content: "Implement logging".into(),
                },
            ],
            role: "reviewer".into(),
            instruction: "Review the latest changes.".into(),
        };

        let prompt = agent.build_prompt(&context);
        assert!(prompt.contains("--- status.md ---"));
        assert!(prompt.contains("Cycle 3"));
        assert!(prompt.contains("--- task.md ---"));
        assert!(prompt.contains("Implement logging"));
        assert!(prompt.contains("--- Instruction ---"));
        assert!(prompt.contains("Review the latest changes."));
    }

    #[test]
    fn kind_is_claude() {
        let mut config = base_config();
        config.anthropic_api_key = Some("sk-ant".into());
        let agent = AnthropicAgent::new(&config).expect("agent should be created");
        assert_eq!(agent.kind(), crate::agent::AgentKind::Claude);
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
