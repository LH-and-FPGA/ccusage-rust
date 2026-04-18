# rcusage

Rust port of [ccusage](https://github.com/ryoppippi/ccusage). Reads Claude Code's local JSONL logs and reports token / cost usage as a daily, monthly, session, or 5-hour-block breakdown.

Single static binary, no Node toolchain required.

## Install

```sh
cargo install --path .
# or
cargo build --release
./target/release/rcusage --help
```

## Usage

```sh
rcusage daily            # group by day
rcusage monthly          # group by month
rcusage session          # group by project + sessionId
rcusage blocks           # 5-hour billing windows

# Common flags (all subcommands)
--mode auto|calculate|display   # cost source (default: auto)
--offline                        # use embedded LiteLLM pricing snapshot
--json                           # emit JSON instead of a table
--timezone <IANA>                # override system timezone for date grouping
```

### Cost modes

| mode | behavior |
|---|---|
| `auto` (default) | use the entry's pre-computed `costUSD` if present, else compute from tokens |
| `calculate` | always compute from tokens (ignore `costUSD`) |
| `display` | always use `costUSD`; missing values count as 0 |

### Data sources

Reads from, in order:

1. `$CLAUDE_CONFIG_DIR` (comma-separated list)
2. `$XDG_CONFIG_HOME/claude` (or `~/.config/claude`)
3. `~/.claude`

A path counts as valid only if it contains a `projects/` subdirectory.

### Pricing data

Pulls per-token rates from LiteLLM's [`model_prices_and_context_window.json`](https://github.com/BerriAI/litellm). A snapshot is embedded at build time (`assets/litellm_pricing.json`) and used as a fallback if the network fetch fails or `--offline` is set.

Tiered pricing (the 200k-token threshold for Claude 1M-context models) and the `fast`-speed multiplier are both honored.

## How it differs from ccusage

- **Subset.** Only the four core report commands. No `statusline`, no MCP server, no Codex / OpenCode log support, no `--instances` project breakdown, no `--jq` filter.
- **Single binary.** Embeds the pricing snapshot, so `--offline` works with zero network access.
- **Streaming parser.** Line-by-line JSON parsing keeps memory flat regardless of log size.

Output numbers should match `ccusage` for the same data and mode. If they don't, that's a bug — please open an issue.

## Project layout

```
src/
├── main.rs        # clap entry, dispatches subcommands
├── cli.rs         # argument structs
├── paths.rs       # Claude data directory discovery
├── schema.rs      # serde structs for one JSONL line
├── loader.rs      # walk + earliest-timestamp sort + stream + dedup
├── pricing.rs     # LiteLLM fetch + embedded snapshot
├── cost.rs        # tiered pricing + 3 cost modes
├── aggregate.rs   # daily / monthly / session group-by
├── blocks.rs      # 5-hour block + gap detection
└── output.rs      # table + JSON renderers
assets/
└── litellm_pricing.json    # offline pricing snapshot
```

## License

MIT. Original ccusage by [@ryoppippi](https://github.com/ryoppippi).
