//! Table + JSON rendering.

use comfy_table::{presets::UTF8_FULL, Cell, ContentArrangement, Table};
use serde::Serialize;

use crate::aggregate::Bucket;
use crate::blocks::SessionBlock;

fn fmt_int(n: u64) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(len + len / 3);
    for (i, b) in bytes.iter().enumerate() {
        let from_end = len - i;
        if i != 0 && from_end % 3 == 0 {
            out.push(',');
        }
        out.push(*b as char);
    }
    out
}

fn fmt_usd(v: f64) -> String {
    format!("${v:.2}")
}

fn join_models(ms: &[String]) -> String {
    ms.join(", ")
}

pub fn render_table(title: &str, key_header: &str, buckets: &[Bucket]) -> String {
    let mut t = Table::new();
    t.load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new(key_header),
            Cell::new("Input"),
            Cell::new("Output"),
            Cell::new("Cache Create"),
            Cell::new("Cache Read"),
            Cell::new("Total Tokens"),
            Cell::new("Cost (USD)"),
            Cell::new("Models"),
        ]);

    let mut grand = crate::aggregate::Totals::default();
    for b in buckets {
        grand.input_tokens += b.totals.input_tokens;
        grand.output_tokens += b.totals.output_tokens;
        grand.cache_creation_tokens += b.totals.cache_creation_tokens;
        grand.cache_read_tokens += b.totals.cache_read_tokens;
        grand.total_cost_usd += b.totals.total_cost_usd;
        t.add_row(vec![
            Cell::new(&b.key),
            Cell::new(fmt_int(b.totals.input_tokens)),
            Cell::new(fmt_int(b.totals.output_tokens)),
            Cell::new(fmt_int(b.totals.cache_creation_tokens)),
            Cell::new(fmt_int(b.totals.cache_read_tokens)),
            Cell::new(fmt_int(b.totals.total_tokens())),
            Cell::new(fmt_usd(b.totals.total_cost_usd)),
            Cell::new(join_models(&b.models)),
        ]);
    }

    t.add_row(vec![
        Cell::new("TOTAL"),
        Cell::new(fmt_int(grand.input_tokens)),
        Cell::new(fmt_int(grand.output_tokens)),
        Cell::new(fmt_int(grand.cache_creation_tokens)),
        Cell::new(fmt_int(grand.cache_read_tokens)),
        Cell::new(fmt_int(grand.total_tokens())),
        Cell::new(fmt_usd(grand.total_cost_usd)),
        Cell::new(""),
    ]);

    format!("{title}\n{t}")
}

pub fn render_blocks(blocks: &[SessionBlock]) -> String {
    let mut t = Table::new();
    t.load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("Start"),
            Cell::new("End"),
            Cell::new("Status"),
            Cell::new("Total Tokens"),
            Cell::new("Cost (USD)"),
            Cell::new("Models"),
        ]);
    for b in blocks {
        let status = if b.is_gap {
            "gap"
        } else if b.is_active {
            "active"
        } else {
            "closed"
        };
        t.add_row(vec![
            Cell::new(b.start.to_string()),
            Cell::new(b.end.to_string()),
            Cell::new(status),
            Cell::new(fmt_int(b.totals.total_tokens())),
            Cell::new(fmt_usd(b.totals.total_cost_usd)),
            Cell::new(join_models(&b.models)),
        ]);
    }
    format!("5-hour blocks\n{t}")
}

#[derive(Serialize)]
pub struct BucketReport<'a> {
    pub kind: &'a str,
    pub buckets: &'a [Bucket],
    pub totals: crate::aggregate::Totals,
}

pub fn report_for_buckets<'a>(kind: &'a str, buckets: &'a [Bucket]) -> BucketReport<'a> {
    let mut totals = crate::aggregate::Totals::default();
    for b in buckets {
        totals.input_tokens += b.totals.input_tokens;
        totals.output_tokens += b.totals.output_tokens;
        totals.cache_creation_tokens += b.totals.cache_creation_tokens;
        totals.cache_read_tokens += b.totals.cache_read_tokens;
        totals.total_cost_usd += b.totals.total_cost_usd;
    }
    BucketReport { kind, buckets, totals }
}
