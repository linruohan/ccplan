<div align="center">

# ccplan

**The day planner your AI agent can run.**
Write a plain-text plan — or have an agent write it — and ccplan turns it into native OS
notifications, status tracking, and time-triggered commands. No daemon, no account, no cloud.

[![CI](https://github.com/ankitkpandey1/ccplan/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/ankitkpandey1/ccplan/actions/workflows/ci.yml)
[![release](https://img.shields.io/github/v/release/ankitkpandey1/ccplan?sort=semver)](https://github.com/ankitkpandey1/ccplan/releases/latest)
[![license](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)

</div>

---

## Set up with your AI agent

ccplan is built to be driven by an agent. **Copy the block below and paste it to Claude Code (or any
coding agent).** It installs ccplan, loads the skill, and starts planning your day — you run nothing
yourself.

```text
Install and use ccplan (an agent-driven CLI day planner) for me, then plan my day.

1. Install the binary.
   macOS/Linux:  curl --proto '=https' --tlsv1.2 -LsSf https://github.com/ankitkpandey1/ccplan/releases/latest/download/ccplan-installer.sh | sh
   Windows:      powershell -c "irm https://github.com/ankitkpandey1/ccplan/releases/latest/download/ccplan-installer.ps1 | iex"

2. Install the ccplan agent skill so you know exactly how to use it.
   mkdir -p ~/.claude/skills/ccplan
   curl --proto '=https' --tlsv1.2 -fsSL https://raw.githubusercontent.com/ankitkpandey1/ccplan/main/skills/ccplan/SKILL.md -o ~/.claude/skills/ccplan/SKILL.md

3. Confirm it works: run `ccplan --version` and `ccplan doctor`.

4. Read ~/.claude/skills/ccplan/SKILL.md, then ask me about my day and author it with ccplan.
```

That's it — the agent installs the binary, drops the [skill](skills/ccplan/SKILL.md) into place, and
follows its recipe (authoring, exit codes, JSON contract). Prefer to drive it yourself? Read on.

---

## What it is

ccplan plans your day as a list of **time blocks**. Each block can **alert you** with a desktop
notification, be **marked done or skipped**, and optionally **run a command** when it starts — kick
off a sync, open a doc, start a build. The plan is one plain-text [TOML](#the-plan-file) file you can
hand-edit or let an agent author end to end.

|  | Agent can fill it | Desktop alerts | Mark done | Run a command at a time | Cross-platform |
|---|:---:|:---:|:---:|:---:|:---:|
| Google Tasks | ✗ | partial | ✓ | ✗ | ✓ |
| Calendar / CalDAV | ~ | ✓ | ~ | ✗ | ~ |
| Taskwarrior | ✓ | via glue | ✓ | ✗ (fires on *edit*, not at a *time*) | ✓ |
| cron / systemd / launchd | ✓ | — | — | ✓ | per-OS |
| **ccplan** | **✓** | **✓** | **✓** | **✓** | **✓** |

ccplan is the one tool that is agent-authorable, time-triggered, can both **notify and execute**,
runs on Linux/macOS/Windows, and keeps a human-readable plain-text plan as its source of truth.

---

## Install

```sh
# macOS / Linux
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/ankitkpandey1/ccplan/releases/latest/download/ccplan-installer.sh | sh

# Windows (PowerShell)
powershell -c "irm https://github.com/ankitkpandey1/ccplan/releases/latest/download/ccplan-installer.ps1 | iex"
```

Every release ships signed-checksum archives for Linux (x64/ARM64) and macOS (Intel/Apple Silicon),
plus a Windows `.zip` and `.msi` — download any directly from the
[latest release](https://github.com/ankitkpandey1/ccplan/releases/latest), or
[build from source](#build-from-source).

---

## Quickstart

```sh
ccplan add --title "Focus time"      --start 11:00 --end 11:30
ccplan add --title "Agentic sync-up" --start 11:30 --duration 30m --notify 2m
ccplan add --title "Standup"         --start 16:25 --duration 5m

ccplan show              # the day so far
ccplan apply             # hand the schedule to the OS — alerts now fire on their own

ccplan now               # what's active right now
ccplan next              # what's coming up
ccplan done focus-time   # mark a block complete (id is auto-slugged from the title)

ccplan remind "Stretch" --in 25m   # one-shot: alert 25 minutes from now, applied immediately
```

At 11:00 you get a "Focus time" notification, at 11:30 the sync-up alert fires, and so on — even if
`ccplan` itself isn't running.

---

## The plan file

One [TOML](https://toml.io) file per day under your OS data dir
(`~/.local/share/ccplan/plans/YYYY-MM-DD.toml` on Linux). Edit it by hand or let an agent write it;
`ccplan apply` re-validates it before scheduling.

```toml
date     = "2026-06-08"
timezone = "Asia/Kolkata"          # frozen at author time; all times resolve against this

[[block]]
id     = "focus-1"                 # stable, unique within the day
title  = "Focus time"
start  = "11:00"                   # local wall-clock
end    = "11:30"                   # OR  duration = "30m"  (exactly one)
notify = "5m"                      # lead-time notification before start
tags   = ["deep-work"]
status = "pending"                 # pending | active | done | skipped | missed | expired

[[block]]
id       = "sync-1"
title    = "Agentic sync-up"
start    = "11:30"
duration = "30m"
notify   = "2m"
run      = ["/home/me/bin/sync.sh", "--fast"]   # argv vector (no shell) — must be allow-listed
status   = "pending"
```

An agent authors the whole day in one shot by piping TOML into `ccplan set --from -`, then runs
`ccplan apply`. Reads round-trip as JSON (`ccplan show --json`).

> **Why TOML, not YAML?** TOML is unambiguous to hand-edit (no significant whitespace, no
> "`no` → false" surprises) and maps cleanly to a list of blocks. Its Rust implementation is
> maintained and audited, unlike the archived/advisory-carrying YAML crates — better for agents and
> for supply-chain hygiene.

---

## CLI reference

**Authoring**

| Command | Purpose |
|---|---|
| `ccplan set --from <file\|-> [--override-history]` | Replace the whole day. Terminal blocks (done/skipped/missed/expired) are always kept; changing them needs `--override-history`. |
| `ccplan add --title T --start 11:00 [--end\|--duration] [--notify] [--run …] [--id]` | Add or update one block. |
| `ccplan remind "T" --in 30m [--id]` | One-shot reminder: add a zero-lead block at now+duration and apply it in one step. |
| `ccplan edit <id> [--start …] [--title …] …` | Patch a non-terminal block. |
| `ccplan rm <id>` | Remove a pending block. |
| `ccplan done <id>` / `ccplan skip <id>` | Mark a block complete / skipped. |
| `ccplan snooze <id> --by 10m` | Push a non-terminal block later and re-apply (refused if it would cross midnight). |
| `ccplan template save\|apply <name> [--date]` / `ccplan template list` | Capture a day shape once, then stamp it onto any date (statuses reset, then applied). |
| `ccplan clear --yes` | Archive the day and remove its triggers (`--purge` to delete instead). |

**Reading** — all support `--json`

| Command | Returns |
|---|---|
| `ccplan show` | The full day. |
| `ccplan now` | Array of blocks active right now. |
| `ccplan next` | Array of the next upcoming block(s). |
| `ccplan agenda` | Remaining blocks with countdowns. |
| `ccplan log [--date <d>] [--since <rfc3339>]` | The fire ledger — what the scheduler actually did (notify/activate/missed/close). |

`ccplan watch [--every <dur>]` is a live, auto-refreshing view of the agenda for leaving open in a
terminal (human-only, no `--json`; default refresh `30s`; Ctrl-C or Enter to quit).

**System**

| Command | Purpose |
|---|---|
| `ccplan apply [--dry-run]` | Reconcile OS triggers to match the plan (idempotent). |
| `ccplan status` | Scheduler health: tracked vs. live OS triggers. |
| `ccplan doctor` | Check the native scheduler + notifier are usable; print fixes. |
| `ccplan completions <shell>` | Print shell completions (bash/zsh/fish/powershell). |

`ccplan fire …` exists but is internal — it's what the OS invokes when a block triggers.

**Exit codes:** `0` ok · `2` usage/validation · `3` not found · `4` scheduler failure ·
`5` automation refused · `6` history conflict (needs `--override-history`). No command is ever
interactive; destructive ones require an explicit flag (`--yes`, `--override-history`).

---

## MCP server

`ccplan mcp` starts a synchronous [Model Context Protocol](https://modelcontextprotocol.io)
server over stdio (JSON-RPC 2.0, newline-delimited). Wire it into any MCP host:

```json
{
  "mcpServers": {
    "ccplan": {
      "command": "ccplan",
      "args": ["mcp"]
    }
  }
}
```

**Exposed tools** (16 total):

| Tool | What it does |
|---|---|
| `ccplan_plan_day` | Replace the whole day from a JSON blocks array |
| `ccplan_apply` | Reconcile OS triggers to match the current plan |
| `ccplan_show_plan` | Return the full plan as JSON |
| `ccplan_list_now` | Blocks active right now (`[]` if none) |
| `ccplan_list_next` | Next upcoming block(s) (`[]` if none) |
| `ccplan_show_agenda` | Remaining blocks with countdowns |
| `ccplan_add_block` | Add or update one block |
| `ccplan_add_reminder` | One-shot relative reminder (add + apply) |
| `ccplan_mark_block` | Mark a block done or skipped |
| `ccplan_edit_block` | Patch title, time, notify, or run on a non-terminal block |
| `ccplan_remove_block` | Remove a pending block |
| `ccplan_snooze_block` | Push a non-terminal block later by a duration and re-apply |
| `ccplan_save_template` | Save the plan for a date as a named, reusable day template |
| `ccplan_list_templates` | List saved template names |
| `ccplan_apply_template` | Instantiate a template onto a date (statuses reset) and apply |
| `ccplan_fire_log` | Read the fire ledger — what fired while you were away (`[]` if none) |

**Close the loop.** `ccplan_fire_log` is the read side of the agent loop: the scheduler fires
blocks on real OS time, and the agent calls `ccplan_fire_log` (optionally `since` the last time it
looked) to see what actually happened — what notified, what `run:` activated, what was missed — and
re-plans from there. Each entry is `{ ts, date, id, event, outcome, detail }`. It's read-only: it
observes history and never fires anything.

`fire`, `mcp`, and `completions` are **never** exposed as MCP tools. No tool can modify
`automation.enabled` or the allowlist. When a `run:` command is stored but would not execute
(automation disabled or executable not allowlisted), the tool result includes a `WARNING` line.

---

## Configuration

`~/.config/ccplan/config.toml` (Linux paths shown):

```toml
grace = "90s"                      # how late a trigger may fire before it counts as missed

[automation]
enabled = false                    # per-block `run:` is OFF by default
timeout = "5m"                     # max runtime per `run:` command
allowed_executables = [            # `run:`'s argv[0] must be an absolute path on this list
  "/home/me/bin/sync.sh",
]

[notify]
default_lead = "5m"                # lead applied to a block that omits its own `notify`
```

---

## How it works

```
 you / an agent          ccplan apply              native OS scheduler         ccplan fire
 ───────────────▶  plan.toml  ───────────▶  systemd / launchd / Task Sched ──────▶  notify + run
   (CLI or edit)   (source of truth)        (one one-shot trigger per event)        + lifecycle
```

1. **Store** — the per-day TOML plan is the single source of truth.
2. **Compile** — `apply` reconciles the plan into native one-shot OS triggers, one per event
   (`notify` / `start` / `end`). Each embeds a schedule **`rev`** (a hash of the block's timing), so
   re-planned or deleted triggers go inert.
3. **Fire** — when a trigger fires, the OS runs `ccplan fire …`, which checks it's still current and
   on time (a durable ledger guarantees it acts **at most once**), then notifies, runs the command if
   any, and advances the block's status.

There is **no background daemon** — ccplan leans on each OS's own scheduler, so alerts fire even when
ccplan isn't running.

---

## Security model

`run:` lets a block execute a command, so the plan file is a **trust boundary**. ccplan defends it:

- Automation is **off by default**; you opt in via config.
- A command's executable must be an **absolute path on an allowlist** — nothing else runs.
- Commands run as an **argv vector with no shell** (no `sh -c`) — no shell-injection surface.
- The plan file must be **owned by you and not world-writable**, or ccplan refuses to run its commands.
- Every fire is **logged**, runs **at most once**, and is **time-bounded**.

See [`SECURITY.md`](SECURITY.md) for the threat model and how to report issues.

---

## Platform support

| | Linux | macOS | Windows |
|---|:---:|:---:|:---:|
| Scheduling | systemd `--user` timers | launchd LaunchAgents | Task Scheduler |
| Notifications | libnotify / D-Bus | NSUserNotification | wscript MsgBox |

A graphical login session is required for notifications (the usual desktop case). Headless use still
schedules and runs automation; `ccplan doctor` tells you what's available.

---

## Build from source

Requires a recent stable Rust toolchain (edition 2024; see `rust-toolchain.toml`).

```sh
git clone https://github.com/ankitkpandey1/ccplan
cd ccplan
cargo build --release
./target/release/ccplan --help
```

Design notes and invariants live in [`DESIGN.md`](DESIGN.md).

---

## Contributing

Contributions welcome — see [`CONTRIBUTING.md`](CONTRIBUTING.md) and the coding standard in
[`CONVENTIONS.md`](CONVENTIONS.md). In short: conventional-commit messages; `cargo fmt` and
`cargo clippy -- -D warnings` clean; no `unsafe`; tests for every change; the coverage gate stays
at 100%.

## License

Licensed under either [Apache License 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your
option. Unless you state otherwise, any contribution you submit shall be dual-licensed as above,
without additional terms.
