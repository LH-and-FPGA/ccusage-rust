//! Aggregation: turn flat per-entry records into daily / monthly / session /
//! 5-hour-block summaries. All counters use u64 for tokens, f64 for USD.

use jiff::tz::TimeZone;
use std::collections::BTreeMap;

use crate::loader::LoadedEntry;

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Totals {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    pub total_cost_usd: f64,
}

impl Totals {
    pub fn add(&mut self, e: &LoadedEntry, cost: f64) {
        let u = e.entry.usage();
        self.input_tokens += u.input_tokens;
        self.output_tokens += u.output_tokens;
        self.cache_creation_tokens += u.cache_creation_input_tokens;
        self.cache_read_tokens += u.cache_read_input_tokens;
        self.total_cost_usd += cost;
    }
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens
            + self.output_tokens
            + self.cache_creation_tokens
            + self.cache_read_tokens
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Bucket {
    pub key: String,
    pub totals: Totals,
    pub models: Vec<String>,
}

fn date_in_tz(ts: jiff::Timestamp, tz: &TimeZone, fmt: &str) -> String {
    let zoned = ts.to_zoned(tz.clone());
    zoned.strftime(fmt).to_string()
}

fn group_by<F: Fn(&LoadedEntry) -> String>(
    entries: &[(LoadedEntry, f64)],
    key: F,
) -> Vec<Bucket> {
    let mut map: BTreeMap<String, (Totals, Vec<String>)> = BTreeMap::new();
    for (e, cost) in entries {
        let k = key(e);
        let slot = map.entry(k).or_default();
        slot.0.add(e, *cost);
        if let Some(m) = e.entry.display_model() {
            if !slot.1.iter().any(|x| x == m) {
                slot.1.push(m.to_string());
            }
        }
    }
    map.into_iter()
        .map(|(key, (totals, mut models))| {
            models.sort();
            Bucket { key, totals, models }
        })
        .collect()
}

pub fn daily(entries: &[(LoadedEntry, f64)], tz: &TimeZone) -> Vec<Bucket> {
    group_by(entries, |e| date_in_tz(e.entry.timestamp, tz, "%Y-%m-%d"))
}

pub fn monthly(entries: &[(LoadedEntry, f64)], tz: &TimeZone) -> Vec<Bucket> {
    group_by(entries, |e| date_in_tz(e.entry.timestamp, tz, "%Y-%m"))
}

pub fn session(entries: &[(LoadedEntry, f64)]) -> Vec<Bucket> {
    group_by(entries, |e| format!("{}/{}", e.project, e.session_id))
}
