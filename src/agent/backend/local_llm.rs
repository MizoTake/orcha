use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use tokio::process::Command;
use tokio::time::MissedTickBehavior;

use crate::agent::{Agent, AgentContext, AgentKind, AgentResponse};
use crate::config::AppConfig;

/// Local LLM agent using OpenAI-compatible API (Ollama, LM Studio, etc.)
pub struct LocalLlmAgent {
    client: Client,
    endpoint: String,
    model: String,
}

impl LocalLlmAgent {
    const READY_CHECK_INTERVAL: Duration = Duration::from_secs(60);
    const READY_CHECK_TIMEOUT: Duration = Duration::from_secs(8);

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

    fn completions_url(&self) -> String {
        format!("{}/chat/completions", self.endpoint.trim_end_matches('/'))
    }

    async fn send_completion(
        &self,
        url: &str,
        body: &serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .client
            .post(url)
            .json(body)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Local LLM request failed: {}", e))?;

        let status = resp.status();
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Local LLM HTTP {}: {}", status, body_text);
        }

        let json: serde_json::Value = resp.json().await?;
        Ok(json)
    }

    async fn lmstudio_ready_status(&self) -> Option<bool> {
        let host = lmstudio_host_from_endpoint(&self.endpoint);
        let mut command = Command::new("lms");
        command.arg("ps").arg("--json");
        if let Some(host) = host {
            command.arg("--host").arg(host);
        }

        let output = tokio::time::timeout(Self::READY_CHECK_TIMEOUT, command.output())
            .await
            .ok()?
            .ok()?;
        if !output.status.success() {
            return None;
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let parsed: serde_json::Value = serde_json::from_str(&stdout).ok()?;
        infer_lmstudio_ready_from_ps(&parsed)
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

        let url = self.completions_url();
        let mut attempt = 1u32;
        let json: serde_json::Value = loop {
            let send_future = self.send_completion(&url, &body);
            tokio::pin!(send_future);

            let mut ready_tick = tokio::time::interval(Self::READY_CHECK_INTERVAL);
            ready_tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
            ready_tick.tick().await;

            let mut retry_due_to_ready = false;
            let completion_result = loop {
                tokio::select! {
                    result = &mut send_future => {
                        break Some(result);
                    }
                    _ = ready_tick.tick() => {
                        if let Some(true) = self.lmstudio_ready_status().await {
                            attempt = attempt.saturating_add(1);
                            println!(
                                "  ... local_llm watchdog: LM Studio Ready detected; re-sending request (attempt {})",
                                attempt
                            );
                            retry_due_to_ready = true;
                            break None;
                        }
                    }
                }
            };

            if retry_due_to_ready {
                continue;
            }

            if let Some(result) = completion_result {
                break result?;
            }
        };
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

fn lmstudio_host_from_endpoint(endpoint: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(endpoint).ok()?;
    let host = parsed.host_str()?.to_string();
    let port = parsed.port_or_known_default()?;
    Some(format!("{host}:{port}"))
}

#[derive(Default)]
struct ReadySignals {
    saw_indicator: bool,
    busy_detected: bool,
}

fn infer_lmstudio_ready_from_ps(value: &serde_json::Value) -> Option<bool> {
    let mut signals = ReadySignals::default();
    collect_ready_signals(value, None, &mut signals);
    if signals.busy_detected {
        return Some(false);
    }
    if signals.saw_indicator {
        return Some(true);
    }
    None
}

fn collect_ready_signals(
    value: &serde_json::Value,
    current_key: Option<&str>,
    signals: &mut ReadySignals,
) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                collect_ready_signals(child, Some(key), signals);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_ready_signals(item, current_key, signals);
            }
        }
        serde_json::Value::String(text) => {
            if let Some(key) = current_key {
                update_signal_from_key_value(key, text, signals);
            }
        }
        serde_json::Value::Number(num) => {
            if let Some(key) = current_key {
                update_signal_from_key_number(key, num, signals);
            }
        }
        serde_json::Value::Bool(value) => {
            if let Some(key) = current_key {
                update_signal_from_key_bool(key, *value, signals);
            }
        }
        _ => {}
    }
}

fn normalized_key(raw: &str) -> String {
    raw.to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

fn update_signal_from_key_value(key: &str, value: &str, signals: &mut ReadySignals) {
    let key = normalized_key(key);
    let status = value.trim().to_ascii_lowercase();

    if key.contains("generationstatus") {
        signals.saw_indicator = true;
        match status.as_str() {
            "ready" | "idle" | "none" | "stopped" => {}
            _ => signals.busy_detected = true,
        }
    }
}

fn update_signal_from_key_number(
    key: &str,
    value: &serde_json::Number,
    signals: &mut ReadySignals,
) {
    let key = normalized_key(key);
    if key.contains("queuedpredictionrequests") {
        signals.saw_indicator = true;
        if value.as_i64().unwrap_or(0) > 0 {
            signals.busy_detected = true;
        }
    }
}

fn update_signal_from_key_bool(key: &str, value: bool, signals: &mut ReadySignals) {
    let key = normalized_key(key);
    if key.contains("isgenerating") || key.contains("isprocessing") {
        signals.saw_indicator = true;
        if value {
            signals.busy_detected = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn infer_ready_when_generation_status_is_ready() {
        let payload = json!({
            "models": [
                {
                    "generation_status": "ready",
                    "queued_prediction_requests": 0
                }
            ]
        });
        assert_eq!(super::infer_lmstudio_ready_from_ps(&payload), Some(true));
    }

    #[test]
    fn infer_not_ready_when_generation_status_is_busy() {
        let payload = json!({
            "models": [
                {
                    "generation_status": "generating",
                    "queued_prediction_requests": 0
                }
            ]
        });
        assert_eq!(super::infer_lmstudio_ready_from_ps(&payload), Some(false));
    }

    #[test]
    fn infer_not_ready_when_queue_has_pending_requests() {
        let payload = json!({
            "models": [
                {
                    "generation_status": "ready",
                    "queued_prediction_requests": 2
                }
            ]
        });
        assert_eq!(super::infer_lmstudio_ready_from_ps(&payload), Some(false));
    }

    #[test]
    fn infer_none_when_no_supported_fields_exist() {
        let payload = json!({
            "models": [
                {
                    "id": "qwen/qwen3",
                    "state": "loaded"
                }
            ]
        });
        assert_eq!(super::infer_lmstudio_ready_from_ps(&payload), None);
    }

    #[test]
    fn parse_host_from_openai_endpoint() {
        let host = super::lmstudio_host_from_endpoint("http://localhost:1234/v1");
        assert_eq!(host.as_deref(), Some("localhost:1234"));
    }
}
