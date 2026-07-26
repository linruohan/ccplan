# Changelog

All notable changes to this project are documented here.

This project follows Keep a Changelog and uses Semantic Versioning.

## [Unreleased]

## [1.2.0] - 2026-06-16

### Added

- **`ccplan mcp`**: synchronous JSON-RPC 2.0 MCP server over stdio. Exposes 16 tools:
  `ccplan_plan_day`, `ccplan_apply`, `ccplan_show_plan`, `ccplan_list_now`, `ccplan_list_next`,
  `ccplan_show_agenda`, `ccplan_add_block`, `ccplan_add_reminder`, `ccplan_mark_block`,
  `ccplan_edit_block`, `ccplan_remove_block`, `ccplan_snooze_block`, `ccplan_save_template`,
  `ccplan_list_templates`, `ccplan_apply_template`, `ccplan_fire_log`. No new runtime dependencies —
  hand-rolled over `serde_json`. Security: `fire`, `mcp`, and `completions` are not exposed as
  tools; no tool can set `automation.enabled` or modify the allowlist; authoring-time `run:`
  warnings fire when automation is disabled or the executable isn't allowlisted.
- **Close the loop**: `ccplan log` (and the `ccplan_fire_log` MCP tool) read the fire ledger — what
  the scheduler actually did (notify/activate/missed/close) — so an agent can see what fired while
  it was away and re-plan. Optional `--date` and `--since <rfc3339>` filters; `--json` for machines.
  Read-only; cannot fire or mutate anything.
- **`ccplan snooze <id> --by <dur>`** (and the `ccplan_snooze_block` MCP tool): push a non-terminal
  block later by a duration and re-apply in one step — react to a fire by sliding the block instead
  of recomputing absolute times. Refused if the slide would cross midnight (no day rollover).
- **`ccplan template save|list|apply`** (and the `ccplan_save_template` / `ccplan_list_templates` /
  `ccplan_apply_template` MCP tools): reusable day templates. Capture a day shape once, then stamp it
  onto any date (every block reset to pending) and apply it in one step. Template names are validated
  as safe slugs (path-traversal guard). Instantiating over a day with terminal history is refused,
  like `set`.
- **`ccplan watch [--every <dur>]`**: a live, auto-refreshing view of the agenda for leaving open in
  a terminal. Read-only (no scheduling, no `--json`); redraws on the given interval (default `30s`,
  max 24h) and quits on Ctrl-C or Enter. Not exposed as an MCP tool — agents poll `ccplan_show_agenda`
  instead.

### Changed

- The fire ledger (`fire.log`) is now newline-delimited JSON (`{ts, date, id, event, outcome,
  detail}`) instead of a free-form text line, and each entry is timestamped. This makes the ledger
  consumable by `ccplan log` / `ccplan_fire_log` rather than write-only.

## [1.1.0] - 2026-06-15

### Added

- **`ccplan remind "<text>" --in <duration>`**: one-shot relative reminder. Sugar over `add` +
  `apply` — it resolves `now + duration` in the clock's time zone (minute granularity), creates a
  zero-lead block so the only alert is the `start` event firing exactly at the target, and
  auto-applies so the OS trigger goes live in one step. `--in` accepts `1h` / `30m` / `1h30m`
  (max 24h); a reminder that crosses midnight lands in the next day's plan. Same upsert and
  terminal-history rules as `add` (`--id` to override the auto-slugged id).

## [1.0.0] - 2026-06-15

The first public release.

### Added

- **CLI**: Full day-planner surface — `set/add/edit/rm`, `done/skip/clear`, `show/now/next/agenda`,
  `apply/fire/status/doctor`, `completions`.  Reads return JSON arrays (`--json`); exits use
  documented codes (0/2/3/4/5/6); no interactive prompts (agent-safe).
- **Whole-plan stdin authoring**: `ccplan set --from -` reads a TOML plan from stdin, enabling
  agents to author an entire day in one shot.
- **TOML plan schema** (`date`, `timezone`, `[[block]]` array-of-tables) with `deny_unknown_fields`
  validation, a `schedule_rev` keyed only on trigger-affecting fields, and immutable terminal
  history.
- **Native scheduler integration**: `systemd --user` transient timers (Linux), LaunchAgent plists
  (macOS), and Task Scheduler XML tasks (Windows).  `apply` reconciles desired vs. live triggers
  idempotently; `fire` is guarded by a durable at-most-once ledger.
- **Notifications**: `notify-rust` (Linux), `osascript` (macOS), `wscript` MsgBox (Windows);
  non-fatal; non-silent on missing capability.
- **`run:` automation**: argv-only (no shell), allowlisted absolute paths, plan-file ownership
  check, configurable timeout, at-most-once via ledger, structured `fire.log` output.
- **DST-correct time resolution** via `jiff` with the Compatible ambiguity strategy.
- **Shell completions** (bash/zsh/fish/PowerShell) generated at build time and via
  `ccplan completions <shell>`.  Man page `ccplan(1)` generated at build time.
- **Agent skill** (`skills/ccplan/SKILL.md`) and canonical recipe (`AGENTS.md`) with an
  agent-onboarding test that runs the documented commands so docs cannot drift from the CLI.
- **Release automation**: `release-plz` release PR management; `cargo-dist` cross-platform binary +
  shell/PowerShell/Homebrew/MSI installers triggered on `v*.*.*` tags.
- **OSS hygiene**: dual `MIT OR Apache-2.0` license, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md` (CC 2.1),
  `SECURITY.md` (run: threat model), issue templates, PR template, Dependabot.
- **Quality gate**: 100% line coverage with `#[coverage(off)]` only on sanctioned OS-IO methods;
  `clippy::pedantic`; `cargo-deny`; anti-gaming guards enforced in CI.

### Reliability

- The fired-event ledger is pruned by `archive`/`purge` so it cannot grow without bound.
- Atomic writes fsync the parent directory (Unix) after the rename, so the atomic replace itself —
  not just the file contents — is durable across a crash.
- `fire --dry-run` is genuinely read-only: it previews the decision without recording the
  at-most-once ledger entry, sending a notification, persisting status, or writing a fire-log entry.

### Security

- The `run:` automation plan-file ownership check resolves the current UID via a safe `getuid`
  syscall wrapper instead of spawning `id -u`, removing a `PATH`-resolved subprocess from the
  security gate of a scheduler-invoked process.
</content>
</invoke>
