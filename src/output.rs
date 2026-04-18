//! Table + JSON rendering.

use comfy_table::{presets::UTF8_FULL, Attribute, Cell, Color, ContentArrangement, Table};
use serde::Serialize;
use std::io::IsTerminal;

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

/// Colors are enabled iff stdout is a TTY and `NO_COLOR` is not set.
/// (Honors the de-facto https://no-color.org/ standard.)
fn use_color() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    std::io::stdout().is_terminal()
}

/// Pick a color for a cost value: dim < $1, default < $10, yellow < $100, red ≥ $100.
fn cost_color(v: f64) -> Color {
    if v < 1.0 {
        Color::DarkGrey
    } else if v < 10.0 {
        Color::Green
    } else if v < 100.0 {
        Color::Yellow
    } else {
        Color::Red
    }
}

fn header_cell(s: &str, color: bool) -> Cell {
    let c = Cell::new(s);
    if color {
        c.fg(Color::Cyan).add_attribute(Attribute::Bold)
    } else {
        c
    }
}

fn key_cell(s: &str, color: bool) -> Cell {
    let c = Cell::new(s);
    if color {
        c.fg(Color::Cyan)
    } else {
        c
    }
}

fn token_cell(n: u64, color: bool) -> Cell {
    let c = Cell::new(fmt_int(n));
    if color && n > 0 {
        c.fg(Color::Yellow)
    } else {
        c
    }
}

fn cost_cell(v: f64, color: bool) -> Cell {
    let c = Cell::new(fmt_usd(v));
    if color {
        c.fg(cost_color(v)).add_attribute(Attribute::Bold)
    } else {
        c
    }
}

/// Pick a stable-ish color for a given model name so distinct models stand out
/// from each other but the same model keeps the same color across rows.
fn color_for_model(name: &str) -> Color {
    let palette = [
        Color::Magenta,
        Color::Blue,
        Color::Green,
        Color::Cyan,
        Color::Red,
        Color::Yellow,
    ];
    let h: u32 = name
        .bytes()
        .fold(2166136261u32, |acc, b| acc.wrapping_mul(16777619) ^ b as u32);
    palette[(h as usize) % palette.len()]
}

fn models_cell(ms: &[String], color: bool) -> Cell {
    if !color || ms.is_empty() {
        return Cell::new(join_models(ms));
    }
    let mut out = String::new();
    for (i, m) in ms.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        // ANSI: bold + 256-color foreground per model.
        let (r, g, b) = match color_for_model(m) {
            Color::Magenta => (5, 1, 5),
            Color::Blue => (1, 3, 5),
            Color::Green => (1, 5, 1),
            Color::Cyan => (1, 5, 5),
            Color::Red => (5, 1, 1),
            Color::Yellow => (5, 5, 1),
            _ => (5, 5, 5),
        };
        let code = 16 + 36 * r + 6 * g + b;
        out.push_str(&format!("\x1b[1;38;5;{code}m{m}\x1b[0m"));
    }
    Cell::new(out)
}

fn total_label_cell(color: bool) -> Cell {
    let c = Cell::new("TOTAL");
    if color {
        c.fg(Color::White).add_attribute(Attribute::Bold)
    } else {
        c.add_attribute(Attribute::Bold)
    }
}

fn title(s: &str, color: bool) -> String {
    if color {
        // bold + cyan via raw ANSI (comfy_table Cells don't apply outside the table)
        format!("\x1b[1;36m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

pub fn render_table(title_text: &str, key_header: &str, buckets: &[Bucket]) -> String {
    let color = use_color();
    let mut t = Table::new();
    t.load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            header_cell(key_header, color),
            header_cell("Input", color),
            header_cell("Output", color),
            header_cell("Cache Create", color),
            header_cell("Cache Read", color),
            header_cell("Total Tokens", color),
            header_cell("Cost (USD)", color),
            header_cell("Models", color),
        ]);

    let mut grand = crate::aggregate::Totals::default();
    for b in buckets {
        grand.input_tokens += b.totals.input_tokens;
        grand.output_tokens += b.totals.output_tokens;
        grand.cache_creation_tokens += b.totals.cache_creation_tokens;
        grand.cache_read_tokens += b.totals.cache_read_tokens;
        grand.total_cost_usd += b.totals.total_cost_usd;
        t.add_row(vec![
            key_cell(&b.key, color),
            token_cell(b.totals.input_tokens, color),
            token_cell(b.totals.output_tokens, color),
            token_cell(b.totals.cache_creation_tokens, color),
            token_cell(b.totals.cache_read_tokens, color),
            token_cell(b.totals.total_tokens(), color),
            cost_cell(b.totals.total_cost_usd, color),
            models_cell(&b.models, color),
        ]);
    }

    let bold_int = |n: u64| {
        let mut c = Cell::new(fmt_int(n)).add_attribute(Attribute::Bold);
        if color {
            c = c.fg(Color::Yellow);
        }
        c
    };

    t.add_row(vec![
        total_label_cell(color),
        bold_int(grand.input_tokens),
        bold_int(grand.output_tokens),
        bold_int(grand.cache_creation_tokens),
        bold_int(grand.cache_read_tokens),
        bold_int(grand.total_tokens()),
        cost_cell(grand.total_cost_usd, color),
        Cell::new(""),
    ]);

    format!("{}\n{t}", title(title_text, color))
}

pub fn render_blocks(blocks: &[SessionBlock]) -> String {
    let color = use_color();
    let mut t = Table::new();
    t.load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            header_cell("Start", color),
            header_cell("End", color),
            header_cell("Status", color),
            header_cell("Total Tokens", color),
            header_cell("Cost (USD)", color),
            header_cell("Models", color),
        ]);
    for b in blocks {
        let (status, status_color) = if b.is_gap {
            ("gap", Color::DarkGrey)
        } else if b.is_active {
            ("active", Color::Green)
        } else {
            ("closed", Color::White)
        };
        let status_cell = if color {
            Cell::new(status).fg(status_color).add_attribute(Attribute::Bold)
        } else {
            Cell::new(status)
        };
        t.add_row(vec![
            key_cell(&b.start.to_string(), color),
            key_cell(&b.end.to_string(), color),
            status_cell,
            token_cell(b.totals.total_tokens(), color),
            cost_cell(b.totals.total_cost_usd, color),
            models_cell(&b.models, color),
        ]);
    }
    format!("{}\n{t}", title("5-hour blocks", color))
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
