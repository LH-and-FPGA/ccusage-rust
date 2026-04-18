//! LiteLLM pricing fetcher with offline fallback.
//!
//! - `--offline`: load embedded snapshot (`assets/litellm_pricing.json`).
//! - default: try fetching the live URL once; on failure fall back to embedded.
//!
//! Mirrors the calculation logic from `packages/internal/src/pricing.ts`.

use anyhow::Result;
use once_cell::sync::OnceCell;
use serde::Deserialize;
use std::collections::HashMap;

const LITELLM_URL: &str =
    "https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json";

const EMBEDDED: &str = include_str!("../assets/litellm_pricing.json");

/// Subset of LiteLLM model entry fields we use for cost.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ModelPricing {
    #[serde(default)]
    pub input_cost_per_token: Option<f64>,
    #[serde(default)]
    pub output_cost_per_token: Option<f64>,
    #[serde(default)]
    pub cache_creation_input_token_cost: Option<f64>,
    #[serde(default)]
    pub cache_read_input_token_cost: Option<f64>,

    #[serde(default)]
    pub input_cost_per_token_above_200k_tokens: Option<f64>,
    #[serde(default)]
    pub output_cost_per_token_above_200k_tokens: Option<f64>,
    #[serde(default)]
    pub cache_creation_input_token_cost_above_200k_tokens: Option<f64>,
    #[serde(default)]
    pub cache_read_input_token_cost_above_200k_tokens: Option<f64>,

    #[serde(default)]
    pub provider_specific_entry: Option<HashMap<String, serde_json::Value>>,
}

impl ModelPricing {
    /// Get the `fast` speed multiplier from `provider_specific_entry`, default 1.
    pub fn fast_multiplier(&self) -> f64 {
        self.provider_specific_entry
            .as_ref()
            .and_then(|m| m.get("fast"))
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0)
    }
}

pub struct PricingTable {
    inner: HashMap<String, ModelPricing>,
}

static EMBEDDED_TABLE: OnceCell<HashMap<String, ModelPricing>> = OnceCell::new();

fn load_embedded() -> &'static HashMap<String, ModelPricing> {
    EMBEDDED_TABLE.get_or_init(|| {
        // The top-level "sample_spec" key holds metadata, not a real model.
        let mut map: HashMap<String, ModelPricing> =
            serde_json::from_str(EMBEDDED).unwrap_or_default();
        map.remove("sample_spec");
        map
    })
}

fn fetch_remote() -> Result<HashMap<String, ModelPricing>> {
    let body: HashMap<String, ModelPricing> =
        ureq::get(LITELLM_URL).call()?.into_json()?;
    Ok(body)
}

impl PricingTable {
    pub fn load(offline: bool) -> Self {
        if offline {
            return Self {
                inner: load_embedded().clone(),
            };
        }
        match fetch_remote() {
            Ok(map) => Self { inner: map },
            Err(_) => Self {
                inner: load_embedded().clone(),
            },
        }
    }

    /// Look up a model by name. Tries exact match first, then a couple of
    /// common provider-prefix variants used by Bedrock / Vertex routing.
    pub fn get(&self, model: &str) -> Option<&ModelPricing> {
        if let Some(p) = self.inner.get(model) {
            return Some(p);
        }
        let candidates = [
            format!("anthropic/{model}"),
            format!("anthropic.{model}"),
        ];
        for c in &candidates {
            if let Some(p) = self.inner.get(c) {
                return Some(p);
            }
        }
        None
    }
}
