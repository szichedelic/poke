# poke

Agent attention aggregator. Detects AI coding agents waiting for your input across tmux sessions and lets you quickly switch to them.

## The Problem

You're running multiple Claude Code and Codex sessions across tmux panes — some local, some in happy sessions, some on remote hosts. Agents block waiting for your response (questions, approvals, choices) and you lose track of which ones need you.

## How It Works

poke uses two detection methods:

- **Structured detection** (primary): Claude Code hooks write status events when the agent needs input. Reliable, zero false positives.
- **Tmux scraping** (fallback): Regex pattern matching on pane output for agents without hook support (Codex, ad-hoc sessions).

Results are deduplicated and presented through multiple interfaces.

## Install

```bash
cargo install --path .
```

## Setup

Configure Claude Code integration (one-time):

```bash
poke init
```

This adds a notification hook to `~/.claude/settings.json` that writes status events when Claude Code needs your attention.

## Usage

### Quick check

```bash
poke
```

Shows a table of agents waiting for input. Select one to switch to it.

```
 # │ Agent       │ Waiting For │ Context                          │ Since
───┼─────────────┼─────────────┼──────────────────────────────────┼────────
 1 │ claude-code │ approval    │ wants to edit src/main.rs        │ 3m ago
 2 │ claude-code │ question    │ "should I split this into two…"  │ 8m ago
 3 │ codex       │ choice      │ pick an option (1/2/3)           │ 12m ago

Select [1-3, q to quit]:
```

### Tmux status bar

Add to `.tmux.conf` for ambient awareness:

```
set -g status-right "#(poke count)"
```

Shows `● 3` when agents need you, nothing when all clear.

### Watch mode

```bash
poke watch
```

Live-updating TUI. Arrow keys to navigate, Enter to switch, `q` to quit.

### Quick response

For simple yes/no approval prompts:

```bash
poke respond --pane %42 --answer y
```

### JSON output

```bash
poke list --json
```

## Configuration

Config lives at `~/.poke/config.toml`:

```toml
# Watch mode refresh interval
scan_interval_secs = 3

# Delete stale event files after this long
stale_timeout_secs = 300

# Tmux status bar format
status_format = "● {count}"
status_empty = ""

[detectors]
structured = true
scraping = true

[scraping]
include_sessions = []
exclude_sessions = ["scratch", "music"]
```

### Custom patterns

Extend scraping detection via `~/.poke/patterns.toml`:

```toml
[[patterns]]
name = "my-agent-prompt"
agent = "my-agent"
waiting_type = "question"
regex = 'my-agent>\s*$'
```

Patterns use [Rust regex syntax](https://docs.rs/regex/latest/regex/#syntax).

## Architecture

```
~/.poke/
├── config.toml      # User config
├── events/          # Structured status files from hooks
│   └── pct42.json   # One per tmux pane with a waiting agent
└── patterns.toml    # Custom scraping patterns
```

Detection flow:
1. Structured detector reads event files from `~/.poke/events/`
2. Scraper runs `tmux capture-pane` and matches against patterns
3. Aggregator deduplicates by pane (structured wins), filters to waiting-only
4. Display layer renders results as table, TUI, count, or JSON

## License

MIT
