//! Wire format for a single JSONL line emitted by Claude Code.
//!
//! Mirrors `apps/ccusage/src/data-loader.ts` `usageDataSchema`. We only model
//! the fields ccusage actually reads; everything else is ignored via serde
//! defaults so unknown / extra fields don't break parsing.

use jiff::Timestamp;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct UsageEntry {
    pub timestamp: Timestamp,
    #[serde(default)]
    pub message: Message,
    #[serde(rename = "costUSD", default)]
    pub cost_usd: Option<f64>,
    #[serde(rename = "requestId", default)]
    pub request_id: Option<String>,
    #[serde(rename = "sessionId", default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(rename = "isApiErrorMessage", default)]
    pub is_api_error_message: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Message {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub usage: Usage,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_creation_input_tokens: u64,
    #[serde(default)]
    pub cache_read_input_tokens: u64,
    #[serde(default)]
    pub speed: Option<Speed>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Speed {
    Standard,
    Fast,
}

impl UsageEntry {
    /// Hash for cross-file dedup. Returns `None` if either id is missing — in
    /// that case the entry is *not* deduplicated (matches ccusage behavior).
    pub fn dedup_key(&self) -> Option<String> {
        let msg_id = self.message.id.as_deref()?;
        let req_id = self.request_id.as_deref()?;
        Some(format!("{msg_id}:{req_id}"))
    }

    pub fn usage(&self) -> &Usage {
        &self.message.usage
    }

    /// True if this entry has zero usage tokens (no billable activity).
    pub fn is_empty(&self) -> bool {
        let u = self.usage();
        u.input_tokens == 0
            && u.output_tokens == 0
            && u.cache_creation_input_tokens == 0
            && u.cache_read_input_tokens == 0
    }

    pub fn display_model(&self) -> Option<&str> {
        self.message
            .model
            .as_deref()
            .filter(|m| !m.is_empty() && *m != "<synthetic>")
    }
}
