use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CostMode {
    /// Use pre-calculated costUSD when present, otherwise calculate from tokens.
    Auto,
    /// Always calculate from tokens (ignore costUSD).
    Calculate,
    /// Always use pre-calculated costUSD; show 0 when missing.
    Display,
}

#[derive(Debug, Parser)]
#[command(name = "rcusage", version, about = "Claude Code usage analyzer (Rust port of ccusage)")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Clone, Parser)]
pub struct CommonArgs {
    /// Cost calculation mode.
    #[arg(long, value_enum, default_value = "auto", global = true)]
    pub mode: CostMode,

    /// Use the embedded LiteLLM pricing snapshot instead of fetching online.
    #[arg(long, global = true)]
    pub offline: bool,

    /// Emit JSON instead of a formatted table.
    #[arg(long, global = true)]
    pub json: bool,

    /// IANA timezone name for date grouping (defaults to system timezone).
    #[arg(long, global = true)]
    pub timezone: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Show usage aggregated by day.
    Daily(CommonArgs),
    /// Show usage aggregated by month.
    Monthly(CommonArgs),
    /// Show usage aggregated by Claude Code session (project + sessionId).
    Session(CommonArgs),
    /// Show 5-hour billing blocks.
    Blocks(CommonArgs),
    /// Open the interactive TUI dashboard.
    Tui(CommonArgs),
}
