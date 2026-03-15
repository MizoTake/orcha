use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::core::cycle::Phase;
use crate::core::profile::ProfileName;
use crate::markdown::frontmatter::{self, Document};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerifyStatus {
    Pass,
    Fail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusFrontmatter {
    pub run_id: String,
    pub profile: ProfileName,
    pub cycle: u32,
    pub phase: Phase,
    pub last_update: String,
    pub budget: Budget,
    pub locks: Locks,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify_status: Option<VerifyStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Budget {
    pub paid_calls_used: u32,
    pub paid_calls_limit: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Locks {
    pub writer: Option<String>,
    pub active_task: Option<String>,
}

/// Full parsed status.md.
#[derive(Debug, Clone)]
pub struct StatusFile {
    pub frontmatter: StatusFrontmatter,
    pub content: String,
}

impl StatusFile {
    /// Load status.md from disk.
    pub async fn load(path: &Path) -> anyhow::Result<Self> {
        let raw = tokio::fs::read_to_string(path).await?;
        Self::from_str(&raw)
    }

    /// Parse from string.
    pub fn from_str(input: &str) -> anyhow::Result<Self> {
        let doc: Document<StatusFrontmatter> = frontmatter::parse(input)?;
        Ok(StatusFile {
            frontmatter: doc.frontmatter,
            content: doc.content,
        })
    }

    /// Save status.md to disk.
    pub async fn save(&self, path: &Path) -> anyhow::Result<()> {
        let doc = Document {
            frontmatter: self.frontmatter.clone(),
            content: self.content.clone(),
        };
        let output = frontmatter::serialize(&doc)?;
        tokio::fs::write(path, output).await?;
        Ok(())
    }

    /// Advance to the next phase within the current cycle.
    pub fn advance_phase(&mut self) {
        if let Some(next) = self.frontmatter.phase.next() {
            self.frontmatter.phase = next;
        }
        self.frontmatter.last_update = chrono::Utc::now().to_rfc3339();
    }

    /// Start a new cycle (back to Briefing).
    pub fn start_new_cycle(&mut self) {
        self.frontmatter.cycle += 1;
        self.frontmatter.phase = Phase::Briefing;
        self.frontmatter.last_update = chrono::Utc::now().to_rfc3339();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_status() -> &'static str {
        "---\nrun_id: test-001\nprofile: cheap_checkpoints\ncycle: 1\nphase: plan\nlast_update: '2025-01-01T00:00:00Z'\nbudget:\n  paid_calls_used: 0\n  paid_calls_limit: 10\nlocks:\n  writer: null\n  active_task: null\n---\n\n## Goal\n\nBuild the thing.\n\n## Latest Notes\n\nInitialized.\n"
    }

    #[test]
    fn parse_status() {
        let status = StatusFile::from_str(sample_status()).unwrap();
        assert_eq!(status.frontmatter.run_id, "test-001");
        assert_eq!(status.frontmatter.profile, ProfileName::CheapCheckpoints);
        assert_eq!(status.frontmatter.cycle, 1);
        assert_eq!(status.frontmatter.phase, Phase::Plan);
    }

    #[test]
    fn advance_phase() {
        let mut status = StatusFile::from_str(sample_status()).unwrap();
        assert_eq!(status.frontmatter.phase, Phase::Plan);
        status.advance_phase();
        assert_eq!(status.frontmatter.phase, Phase::Impl);
    }

    #[test]
    fn start_new_cycle_increments_cycle_and_resets_to_briefing() {
        let mut status = StatusFile::from_str(sample_status()).unwrap();
        assert_eq!(status.frontmatter.cycle, 1);
        assert_eq!(status.frontmatter.phase, Phase::Plan);

        status.start_new_cycle();

        assert_eq!(status.frontmatter.cycle, 2);
        assert_eq!(status.frontmatter.phase, Phase::Briefing);
    }

    #[test]
    fn start_new_cycle_increments_monotonically() {
        let mut status = StatusFile::from_str(sample_status()).unwrap();
        status.start_new_cycle();
        status.start_new_cycle();
        assert_eq!(status.frontmatter.cycle, 3);
    }

    #[test]
    fn verify_status_defaults_to_none_for_existing_files() {
        // Status files written before verify_status was added must still parse OK.
        let status = StatusFile::from_str(sample_status()).unwrap();
        assert_eq!(status.frontmatter.verify_status, None);
    }

    #[test]
    fn verify_status_roundtrips_pass_and_fail() {
        let mut status = StatusFile::from_str(sample_status()).unwrap();

        status.frontmatter.verify_status = Some(VerifyStatus::Pass);
        let serialized = crate::markdown::frontmatter::serialize(&crate::markdown::frontmatter::Document {
            frontmatter: status.frontmatter.clone(),
            content: status.content.clone(),
        })
        .unwrap();
        let reparsed = StatusFile::from_str(&serialized).unwrap();
        assert_eq!(reparsed.frontmatter.verify_status, Some(VerifyStatus::Pass));

        status.frontmatter.verify_status = Some(VerifyStatus::Fail);
        let serialized = crate::markdown::frontmatter::serialize(&crate::markdown::frontmatter::Document {
            frontmatter: status.frontmatter.clone(),
            content: status.content.clone(),
        })
        .unwrap();
        let reparsed = StatusFile::from_str(&serialized).unwrap();
        assert_eq!(reparsed.frontmatter.verify_status, Some(VerifyStatus::Fail));
    }

    #[test]
    fn verify_status_none_is_not_serialized() {
        // None should be omitted from the YAML output (skip_serializing_if).
        let status = StatusFile::from_str(sample_status()).unwrap();
        assert_eq!(status.frontmatter.verify_status, None);
        let serialized = crate::markdown::frontmatter::serialize(&crate::markdown::frontmatter::Document {
            frontmatter: status.frontmatter.clone(),
            content: status.content.clone(),
        })
        .unwrap();
        assert!(!serialized.contains("verify_status"));
    }

}
