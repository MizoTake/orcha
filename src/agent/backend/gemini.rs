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
