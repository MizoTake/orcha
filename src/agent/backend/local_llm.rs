use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;

use crate::agent::{Agent, AgentContext, AgentKind, AgentResponse};
use crate::config::AppConfig;

/// Local LLM agent using OpenAI-compatible API (Ollama, LM Studio, etc.)
pub struct LocalLlmAgent {
    client: Client,
    endpoint: String,
    model: String,
}

impl LocalLlmAgent {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            client: Client::new(),
            endpoint: config.local_llm_endpoint.clone(),
            model: config.local_llm_model.clone(),
        }
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
impl Agent for LocalLlmAgent {
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

        let url = format!("{}/chat/completions", self.endpoint.trim_end_matches('/'));
        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Local LLM request failed: {}", e))?;

        let status = resp.status();
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Local LLM HTTP {}: {}", status, body_text);
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
            is_paid: false,
        })
    }

    fn kind(&self) -> AgentKind {
        AgentKind::LocalLlm
    }
}
