//! Cost calculation: tiered (200k threshold) per token type, plus the three
//! cost modes (auto / calculate / display).
//!
//! Direct port of `packages/internal/src/pricing.ts` `calculateCostFromPricing`
//! and `apps/ccusage/src/data-loader.ts` `calculateCostForEntry`.

use crate::cli::CostMode;
use crate::pricing::{ModelPricing, PricingTable};
use crate::schema::{Speed, UsageEntry};

const TIERED_THRESHOLD: u64 = 200_000;

fn tiered(total: u64, base: Option<f64>, above: Option<f64>) -> f64 {
    if total == 0 {
        return 0.0;
    }
    if total > TIERED_THRESHOLD {
        if let Some(above_price) = above {
            let below = TIERED_THRESHOLD as f64;
            let above_count = (total - TIERED_THRESHOLD) as f64;
            let mut cost = above_count * above_price;
            if let Some(base_price) = base {
                cost += below * base_price;
            }
            return cost;
        }
    }
    base.map(|p| total as f64 * p).unwrap_or(0.0)
}

pub fn cost_from_tokens(entry: &UsageEntry, pricing: &ModelPricing) -> f64 {
    let u = entry.usage();
    let input = tiered(
        u.input_tokens,
        pricing.input_cost_per_token,
        pricing.input_cost_per_token_above_200k_tokens,
    );
    let output = tiered(
        u.output_tokens,
        pricing.output_cost_per_token,
        pricing.output_cost_per_token_above_200k_tokens,
    );
    // Cache rates fall back to input rates when missing (per packages/internal/CLAUDE.md).
    let cache_create = tiered(
        u.cache_creation_input_tokens,
        pricing
            .cache_creation_input_token_cost
            .or(pricing.input_cost_per_token),
        pricing
            .cache_creation_input_token_cost_above_200k_tokens
            .or(pricing.input_cost_per_token_above_200k_tokens),
    );
    let cache_read = tiered(
        u.cache_read_input_tokens,
        pricing
            .cache_read_input_token_cost
            .or(pricing.input_cost_per_token),
        pricing
            .cache_read_input_token_cost_above_200k_tokens
            .or(pricing.input_cost_per_token_above_200k_tokens),
    );

    let base = input + output + cache_create + cache_read;

    if matches!(u.speed, Some(Speed::Fast)) {
        base * pricing.fast_multiplier()
    } else {
        base
    }
}

pub fn cost_for_entry(entry: &UsageEntry, mode: CostMode, table: &PricingTable) -> f64 {
    use CostMode::*;
    match mode {
        Display => entry.cost_usd.unwrap_or(0.0),
        Calculate => entry
            .display_model()
            .and_then(|m| table.get(m))
            .map(|p| cost_from_tokens(entry, p))
            .unwrap_or(0.0),
        Auto => {
            if let Some(c) = entry.cost_usd {
                return c;
            }
            entry
                .display_model()
                .and_then(|m| table.get(m))
                .map(|p| cost_from_tokens(entry, p))
                .unwrap_or(0.0)
        }
    }
}
