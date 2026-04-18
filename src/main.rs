use anyhow::Result;
use clap::Parser;
use jiff::tz::TimeZone;

mod aggregate;
mod blocks;
mod cli;
mod cost;
mod loader;
mod output;
mod paths;
mod pricing;
mod schema;
mod tui;

use cli::{Cli, Command, CommonArgs};

fn resolve_tz(arg: &Option<String>) -> Result<TimeZone> {
    match arg {
        Some(name) => Ok(TimeZone::get(name)?),
        None => Ok(TimeZone::system()),
    }
}

fn load_priced(args: &CommonArgs) -> Result<Vec<(loader::LoadedEntry, f64)>> {
    let claude_paths = paths::discover()?;
    let entries = loader::load_all(&claude_paths)?;
    let table = pricing::PricingTable::load(args.offline);
    Ok(entries
        .into_iter()
        .map(|e| {
            let c = cost::cost_for_entry(&e.entry, args.mode, &table);
            (e, c)
        })
        .collect())
}

fn run_daily(args: CommonArgs) -> Result<()> {
    let priced = load_priced(&args)?;
    let tz = resolve_tz(&args.timezone)?;
    let buckets = aggregate::daily(&priced, &tz);
    if args.json {
        let report = output::report_for_buckets("daily", &buckets);
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("{}", output::render_table("Daily usage", "Date", &buckets));
    }
    Ok(())
}

fn run_monthly(args: CommonArgs) -> Result<()> {
    let priced = load_priced(&args)?;
    let tz = resolve_tz(&args.timezone)?;
    let buckets = aggregate::monthly(&priced, &tz);
    if args.json {
        let report = output::report_for_buckets("monthly", &buckets);
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("{}", output::render_table("Monthly usage", "Month", &buckets));
    }
    Ok(())
}

fn run_session(args: CommonArgs) -> Result<()> {
    let priced = load_priced(&args)?;
    let buckets = aggregate::session(&priced);
    if args.json {
        let report = output::report_for_buckets("session", &buckets);
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "{}",
            output::render_table("Session usage", "Project / Session", &buckets)
        );
    }
    Ok(())
}

fn run_blocks(args: CommonArgs) -> Result<()> {
    let priced = load_priced(&args)?;
    let blocks = blocks::identify(&priced);
    if args.json {
        println!("{}", serde_json::to_string_pretty(&blocks)?);
    } else {
        println!("{}", output::render_blocks(&blocks));
    }
    Ok(())
}

fn run_tui(args: CommonArgs) -> Result<()> {
    let priced = load_priced(&args)?;
    let tz = resolve_tz(&args.timezone)?;
    let daily = aggregate::daily(&priced, &tz);
    let monthly = aggregate::monthly(&priced, &tz);
    let session = aggregate::session(&priced);
    let blocks_v = blocks::identify(&priced);
    tui::run(daily, monthly, session, blocks_v)
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Daily(a) => run_daily(a),
        Command::Monthly(a) => run_monthly(a),
        Command::Session(a) => run_session(a),
        Command::Blocks(a) => run_blocks(a),
        Command::Tui(a) => run_tui(a),
    }
}
