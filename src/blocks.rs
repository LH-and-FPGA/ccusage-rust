//! 5-hour billing blocks. Direct port of
//! `apps/ccusage/src/_session-blocks.ts` `identifySessionBlocks`.

use jiff::{Span, Timestamp};

use crate::aggregate::Totals;
use crate::loader::LoadedEntry;

const SESSION_HOURS: i64 = 5;

#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionBlock {
    pub start: Timestamp,
    pub end: Timestamp,
    pub actual_end: Option<Timestamp>,
    pub is_active: bool,
    pub is_gap: bool,
    pub totals: Totals,
    pub models: Vec<String>,
}

fn floor_to_hour(ts: Timestamp) -> Timestamp {
    let zoned = ts.to_zoned(jiff::tz::TimeZone::UTC);
    let floored = zoned
        .with()
        .minute(0)
        .second(0)
        .subsec_nanosecond(0)
        .build()
        .expect("floor to hour");
    floored.timestamp()
}

fn block_duration() -> Span {
    Span::new().hours(SESSION_HOURS)
}

fn finalize(
    start: Timestamp,
    block_entries: &[&(LoadedEntry, f64)],
    now: Timestamp,
) -> SessionBlock {
    let end = start + block_duration();
    let mut totals = Totals::default();
    let mut models: Vec<String> = Vec::new();
    let mut actual_end: Option<Timestamp> = None;
    for (e, cost) in block_entries {
        totals.add(e, *cost);
        if let Some(m) = e.entry.display_model() {
            if !models.iter().any(|x| x == m) {
                models.push(m.to_string());
            }
        }
        actual_end = Some(actual_end.map_or(e.entry.timestamp, |prev| {
            if e.entry.timestamp > prev {
                e.entry.timestamp
            } else {
                prev
            }
        }));
    }
    let is_active = now < end && actual_end.is_some();
    models.sort();
    SessionBlock {
        start,
        end,
        actual_end,
        is_active,
        is_gap: false,
        totals,
        models,
    }
}

pub fn identify(entries: &[(LoadedEntry, f64)]) -> Vec<SessionBlock> {
    if entries.is_empty() {
        return Vec::new();
    }

    let mut sorted: Vec<&(LoadedEntry, f64)> = entries.iter().collect();
    sorted.sort_by_key(|(e, _)| e.entry.timestamp);

    let now = Timestamp::now();
    let dur = block_duration();
    let mut blocks: Vec<SessionBlock> = Vec::new();
    let mut current_start: Option<Timestamp> = None;
    let mut current: Vec<&(LoadedEntry, f64)> = Vec::new();

    for item in sorted {
        let ts = item.0.entry.timestamp;
        match current_start {
            None => {
                current_start = Some(floor_to_hour(ts));
                current.push(item);
            }
            Some(start) => {
                let last_ts = current
                    .last()
                    .map(|(e, _)| e.entry.timestamp)
                    .expect("non-empty");
                let exceeds_block = ts > (start + dur);
                let exceeds_gap = ts > (last_ts + dur);
                if exceeds_block || exceeds_gap {
                    blocks.push(finalize(start, &current, now));
                    if exceeds_gap {
                        blocks.push(SessionBlock {
                            start: last_ts,
                            end: ts,
                            actual_end: None,
                            is_active: false,
                            is_gap: true,
                            totals: Totals::default(),
                            models: Vec::new(),
                        });
                    }
                    current_start = Some(floor_to_hour(ts));
                    current.clear();
                    current.push(item);
                } else {
                    current.push(item);
                }
            }
        }
    }

    if let Some(start) = current_start {
        if !current.is_empty() {
            blocks.push(finalize(start, &current, now));
        }
    }

    blocks
}
