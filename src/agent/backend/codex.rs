use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;

use crate::agent::{Agent, AgentContext, AgentKind, AgentResponse};
use crate::config::AppConfig;

/// OpenAI Codex/GPT agent.
pub struct CodexAgent {
    client: Client,
    api_key: String,
    model: String,
}

impl CodexAgent {
    pub fn new(config: &AppConfig) -> anyhow::Result<Self> {
        let api_key = config
            .openai_api_key
            .clone()
            .ok_or_else(|| anyhow::anyhow!("OPENAI_API_KEY not set"))?;

        Ok(Self {
            client: Client::new(),
            api_key,
            model: config.codex_model.clone(),
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
impl Agent for CodexAgent {
    async fn respond(&self, context: &AgentContext) -> anyhow::Result<AgentResponse> {
        let prompt = self.build_prompt(context);

        let body = json!({
            "model": &self.model,
            "messages": [
                {
                    "role": "system",
                    "content": "You are an AI assistant working as part of an orchestrated development team. Follow instructions precisely and return well-structured markdown."
                },
                {
                    "role": "user",
                    "content": prompt
                }
            ],
            "temperature": 0.3,
            "max_tokens": 4096
        });

        let resp = self
            .client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("OpenAI request failed: {}", e))?;

        let status = resp.status();
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI HTTP {}: {}", status, body_text);
        }

        let json: serde_json::Value = resp.json().await?;
        let content = json["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let tokens = json["usage"]["total_tokens"].as_u64();

        Ok(AgentResponse {
            content,
            model_used: self.model.clone(),
            tokens_used: tokens,
            is_paid: true,
        })
    }

    fn kind(&self) -> AgentKind {
        AgentKind::Codex
    }
}

#[cfg(test)]
mod tests {
    use crate::agent::{Agent, AgentContext, ContextFile};
    use crate::config::AppConfig;
    use crate::machine_config::{LocalLlmCliConfig, ProviderMode};

    use super::CodexAgent;

    #[test]
    fn new_requires_api_key() {
        let err = CodexAgent::new(&base_config()).err().expect("missing api key should fail");
        assert!(err.to_string().contains("OPENAI_API_KEY not set"));
    }

    #[test]
    fn build_prompt_includes_context_files_and_instruction() {
        let mut config = base_config();
        config.openai_api_key = Some("sk-openai".into());
        config.codex_model = "gpt-test".into();
        let agent = CodexAgent::new(&config).expect("agent should be created");
        let context = AgentContext {
            context_files: vec![
                ContextFile {
                    name: "goal.md".into(),
                    content: "Ship the feature".into(),
                },
                ContextFile {
                    name: "task.md".into(),
                    content: "Add regression tests".into(),
                },
            ],
            role: "implementer".into(),
            instruction: "Implement the assigned task.".into(),
        };

        let prompt = agent.build_prompt(&context);
        assert!(prompt.contains("--- goal.md ---"));
        assert!(prompt.contains("Ship the feature"));
        assert!(prompt.contains("--- task.md ---"));
        assert!(prompt.contains("Add regression tests"));
        assert!(prompt.contains("--- Instruction ---"));
        assert!(prompt.contains("Implement the assigned task."));
    }

    #[test]
    fn kind_is_codex() {
        let mut config = base_config();
        config.openai_api_key = Some("sk-openai".into());
        let agent = CodexAgent::new(&config).expect("agent should be created");
        assert_eq!(agent.kind(), crate::agent::AgentKind::Codex);
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
