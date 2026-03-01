use std::path::Path;

use chrono::Utc;
use tokio::io::AsyncWriteExt;

/// Read the contents of a handoff markdown file.
pub async fn read_handoff(path: &Path) -> anyhow::Result<String> {
    if path.exists() {
        Ok(tokio::fs::read_to_string(path).await?)
    } else {
        Ok(String::new())
    }
}

/// Append a message to a handoff markdown file.
pub async fn append_handoff(path: &Path, from: &str, message: &str) -> anyhow::Result<()> {
    let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
    let entry = format!("\n---\n\n**{}** ({})\n\n{}\n", from, timestamp, message);

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await?;

    file.write_all(entry.as_bytes()).await?;
    Ok(())
}

/// Clear a handoff file (after processing).
pub async fn clear_handoff(path: &Path) -> anyhow::Result<()> {
    if path.exists() {
        let heading = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(to_heading)
            .unwrap_or_else(|| "Inbox".to_string());
        let content = format!("# {}\n\nNo pending messages.\n", heading);
        tokio::fs::write(path, content).await?;
    }
    Ok(())
}

fn to_heading(stem: &str) -> String {
    stem
        .split(['_', '-'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => {
                    let mut word = String::new();
                    word.extend(first.to_uppercase());
                    word.push_str(chars.as_str());
                    word
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
