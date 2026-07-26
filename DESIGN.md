# ccplan — Design Doc

**App / binary name:** `ccplan`  ·  **Repo:** `ccplan`
**Author:** Ankit Pandey <itsankitkp@gmail.com>
**Implementation language:** Rust · **Targets:** Linux, macOS, Windows (cross-platform CLI)

---

## 1. Problem

I want to plan my day as a set of time blocks and have a coding agent (Claude Code, or any other
agent on any OS) do the planning *for* me — "fill up my day" — by editing a single source of truth.
Once a day is planned, the plan must actually *do things*:

- **Alert me** at the right times (desktop notifications), with optional lead time.
- Let me **mark blocks done / skipped** as the day progresses.
- **Run automation** when a block fires — e.g. start a sync script at "Agentic sync-up", kick off a
  build before standup, open a doc at "Focus time".

Existing tools each miss at least one of these:

| Tool | Agent can fill it | Desktop alerts | Mark done | Run a command at a time | Cross-platform |
|------|:---:|:---:|:---:|:---:|:---:|
| Google Tasks | ✗ (no real CLI) | partial | ✓ | ✗ | ✓ |
| Calendar / CalDAV (khal) | ~ | ✓ | ~ | ✗ | ~ |
| Taskwarrior | ✓ | via glue | ✓ | ✗ (hooks fire on *edit*, not at a *time*) | ✓ |
| cron / systemd / launchd | ✓ | — | — | ✓ | per-OS |
| **ccplan** | ✓ | ✓ | ✓ | ✓ | ✓ |

The gap is one tool that is **agent-authorable**, **time-triggered**, can both **notify** and
**execute**, runs **anywhere an agent runs**, and is backed by a plain-text plan an agent and a
human can both edit.

## 2. Context & constraints

- **Cross-platform CLI.** Runs on Linux, macOS, and Windows. No assumption of a specific desktop,
  init system, or notification daemon — those sit behind backend abstractions (§6).
- **Rust.** Single self-contained binary per platform; the binary is also what the OS scheduler
  invokes on fire, so a fast, dependency-free executable matters.
- **Agent-driven.** Primary author is an agent translating natural language into CLI calls. The CLI
  contract *is* the integration surface.
- **Local, offline, single-user, zero-config.** No account, no network, no daemon to babysit where
  the OS can schedule for us.
- The primary development environment is Ubuntu 26.04 / GNOME 50 / Wayland — one supported target,
  not the design center.

## 3. Goals

- **G1 — Agent-first CLI.** Non-interactive, machine-readable commands an agent can use to author
  and mutate a single day's time-blocked plan reliably. Authoring the *whole day in one shot*
  (read TOML/JSON from stdin) is first-class.
- **G2 — Plain-text source of truth.** A human-readable, diff-friendly plan file, editable by CLI
  or by hand/agent, whose schedule derives a stable **rev** that triggers are keyed to (§6.3).
- **G3 — Time-triggered notifications.** Notifications at block start and optional per-block lead
  times, on every target OS.
- **G4 — Status tracking.** `done` / `skipped`; auto-detect `missed` and `expired`; query current
  and next blocks.
- **G5 — Per-block automation.** A block may carry a `run:` command (argv vector) executed when the
  block fires. Opt-in, allowlisted, logged, safe by default (§9).
- **G6 — Idempotent & reproducible.** Derived state (OS triggers, notifications) is regenerated from
  the plan by a single `apply`; re-running converges. Safe to call repeatedly by an agent.
- **G7 — Portable scheduling & notifications.** A backend abstraction maps the same trigger/notify
  contract onto each OS's native scheduler and notifier (§6.1) — first-class on Linux, macOS, and Windows.
- **G8 — Local, offline, zero-config.** Works with no network and no account.

## 4. Non-goals

- **NG1** — Multi-day project management, GTD, dependencies, recurring/repeating tasks. One day at a time.
- **NG2** — Cloud sync, multi-device, mobile, accounts.
- **NG3** — Calendar integration (CalDAV / Google / Online Accounts). Possible future *import*.
- **NG4** — A GUI. (A read-only TUI/`watch` view is a possible future addition; not core.)
- **NG5** — A general-purpose job scheduler / cron replacement. `run:` is scoped to "this block, today."
- **NG6** — Sandboxing/containment of `run:` commands. We mitigate (§9) but do not sandbox.
- **NG7** — Multi-user / shared / team plans.
- **NG8** — Catch-up replay of alerts/automation missed while the machine was off/asleep. Stale
  automation is worse than missed automation (Inv-6).
- **NG9** — No shell execution. `run:` is an argv vector only; there is no `sh -c` / shell-string
  mode. This deliberately keeps the highest-risk path out of the design (§9).

## 5. Users & primary flows

**Persona A — the agent.** Authors/edits the plan. Needs deterministic, scriptable, JSON-out
commands and no interactive prompts.
**Persona B — the human.** Glances at the plan, marks things done, tweaks a block, gets nudged.

Flows:
1. *Plan my day (agent).* "Plan two focus blocks, lunch at 2, standup at 4:25, run my sync script
   before the agentic sync-up." → agent composes a plan doc → `ccplan set --from -` → `ccplan apply`.
2. *During the day (human).* 11:00 notification "Focus time". 11:30 the sync script auto-runs, a
   notification confirms. Finished early → `ccplan done focus-1`.
3. *Re-plan (agent).* "Push everything after lunch back 30m." → `ccplan show --json` → rewrite times
   → `ccplan apply`. Triggers reconverge; retimed blocks get a new schedule **rev**, stale triggers
   self-void (§6.3).
4. *Status query.* `ccplan now --json` / `ccplan next --json` to drive a status line or further automation.

## 6. Design overview

```
                author/edit                compile (apply)                 fire (per OS scheduler)
  agent/human ─────────────▶ plan file ─────────────────────▶  Scheduler  ─────────────▶ ccplan fire
  (CLI or direct edit)      (TOML +      (idempotent          backend                    --date --id
                             schedule     reconciler)         (systemd /                 --event --rev
                             rev)                             launchd /                  --at …)
                                                              schtasks)                        │
                                                                                               ├─▶ Notifier backend
                                                                                               └─▶ run: (argv) automation
```

Three responsibilities, deliberately separated:
1. **Store** — a per-day TOML plan file. Canonical. Each block's schedule derives a **rev** (§6.3).
2. **Compile** — `ccplan apply` reconciles OS-level triggers to match the plan. Derived, disposable,
   reproducible, idempotent.
3. **Fire** — `ccplan fire …` is what a trigger invokes: it validates identity + freshness against a
   durable ledger, then (per event type) notifies / runs / reconciles lifecycle, and records the outcome.

### 6.1 Backend abstractions (cross-platform core)

Two traits isolate all OS-specific behavior; the reconciler and lifecycle logic are platform-agnostic.

**`Scheduler`** — create/cancel/list time-triggers, keyed by a portable trigger identity (§6.3).
| OS | Backend | Trigger object | Identity / namespace |
|----|---------|----------------|----------------------|
| Linux | `systemd --user` transient timers (`systemd-run --user --on-calendar=…`) | one timer+service per event | unit `ccplan-<date>-<idhash>-<rev>-<event>.timer` (id hashed + `systemd-escape`d) |
| macOS | launchd LaunchAgents | one plist per event (`StartCalendarInterval`), `launchctl bootstrap` | label `io.ccplan.<date>-<idhash>-<rev>-<event>` (the shared trigger-id token after the `io.ccplan.` prefix, so all three backends derive names from one `backend_id`) |
| Windows | Task Scheduler (`schtasks.exe /Create /XML`) | one task per event | path `\ccplan\<date>-<idhash>-<rev>-<event>` |

A **native scheduler is required on every platform** — there is no resident daemon or polling
fallback. Native schedulers may deliver a trigger late (after sleep/DST); the freshness gate at
`fire` (§7) decides whether to act, so backends are treated as best-effort *delivery* and `fire` is
authoritative. Every backend MUST: (a) realize exactly one trigger per `(date, id, event)`; (b)
embed the schedule **rev** and `scheduled_at` in the fire invocation; (c) be reconcilable purely by
listing/removing within its own namespace prefix; (d) clean up its own fired and orphaned triggers
on the next reconcile.

> **Scheduler precision & one-shot semantics differ per OS — they are NOT uniform.** systemd timers
> set `AccuracySec=1s` (≈second precision); the Windows TimeTrigger uses a second-precision
> `StartBoundary`. **launchd `StartCalendarInterval` is minute-granular (its `Second` key is not
> honored) and recurring (no year field)**, so a macOS notification may fire up to ~59 s late and the
> one-shot guarantee depends on the `fire` path booting its own agent out and deleting its plist as
> its last step — which MUST run on *every* `fire`, including a no-op/stale fire. Each backend MUST
> document its real precision; `fire`'s freshness gate (§7) is authoritative over best-effort
> delivery. A per-platform conformance test MUST assert the exact identity grammar in the table above.

**`Notifier`** — emit a desktop notification. Backends: Linux `libnotify`/`notify-send` (D-Bus);
macOS `osascript`/`UserNotifications`; Windows `wscript` MsgBox. There is no silent no-op notification
mode: if the platform's notification capability is unavailable, `doctor` reports it and `apply`
warns loudly (it does not fail planning), so a misconfigured notifier is always surfaced rather than
hidden behind a fallback.

> **Linux notification-environment requirement.** A notification raised from a systemd user timer
> only reaches the session if the user manager has the session bus/display env. `apply` MUST
> `systemctl --user import-environment DBUS_SESSION_BUS_ADDRESS DISPLAY WAYLAND_DISPLAY XAUTHORITY`,
> and the `fire` path MUST re-derive `DBUS_SESSION_BUS_ADDRESS` from `/run/user/$UID/bus` if absent.
> A failed notification is logged and non-fatal (§11). macOS/Windows agents run in the GUI session
> context and don't need this, but the Notifier trait still treats notification failure as non-fatal.

### 6.2 Storage layout (OS dirs via the `directories` crate)

```
<data>/ccplan/                     # Linux ~/.local/share, macOS ~/Library/Application Support, Win %APPDATA%
  plans/YYYY-MM-DD.toml            # canonical plan per day
  archive/YYYY-MM-DD.toml          # plans retired by `clear` (terminal history preserved, §8)
  log/fire.log                     # append-only human record of every fire (notify + run outcome)
<config>/ccplan/config.toml        # automation toggle, allowlist, defaults, grace, timeouts
<state>/ccplan/triggers.json       # which trigger units we own + the rev each was built from
<state>/ccplan/fired.json          # durable fired-event ledger for at-most-once firing (§6.4)
```

### 6.3 Plan schema, schedule revision & trigger identity

The plan file is **TOML** (not YAML): the maintained, supply-chain-clean (`serde_yaml` is archived;
its forks are unmaintained or carry a RUSTSEC advisory — `cargo-deny` would reject them), and the
least error-prone format for an agent or human to edit by hand (no significant whitespace, no
"Norway problem"). A day maps cleanly to an array-of-tables.

```toml
# plans/2026-06-08.toml
date     = "2026-06-08"
timezone = "Asia/Kolkata"          # resolved & frozen at author time

[[block]]
id     = "focus-1"                 # stable, unique within the day; agent-referencable
title  = "Focus time"
start  = "11:00"                   # local wall-clock for `date`
end    = "11:30"                   # OR `duration = "30m"`  (exactly one of end/duration)
notify = "0m"                      # lead time before start (default from config)
tags   = ["deep-work"]
status = "pending"                 # pending | active | done | skipped | missed | expired  (§7)

[[block]]
id       = "sync-1"
title    = "Agentic sync-up"
start    = "11:30"
duration = "30m"
notify   = "2m"
run      = ["/usr/local/bin/sync.sh", "--fast"]   # argv vector — no shell (NG9, §9)
status   = "pending"
```

**Schedule `rev` (the critical contract).** `rev` is a hash computed over **only the
trigger-affecting fields** of a block — its identity and timing: `id`, resolved `start`,
resolved `end`/`duration`, and `notify` lead. It deliberately **excludes mutable `status`,
history, `title`, `tags`, and even `run`** (the latter three are read fresh from the plan at fire
time, so they don't need to invalidate a trigger). `rev` is *computed*, not hand-maintained; `apply`
recomputes it from the file every run.

Consequences:
- **Lifecycle writes never invalidate triggers.** `fire(start)` flipping `pending → active`, or a
  user `done`, changes `status` but not the schedule, so an already-scheduled `end` trigger's
  embedded `rev` stays valid. (This is the bug the contract exists to prevent.)
- **Retiming a block changes its `rev`**, voiding its old triggers and scheduling new ones.
- **Trigger identity = `(date, id, event, rev)`**, `event ∈ {notify, start, end}`. A fired trigger
  whose embedded `rev` ≠ the block's current schedule `rev` (or whose block is gone) is **stale**:
  `fire` no-ops and the backend removes it. This handles re-plans, deleted blocks, and reused ids
  across days. A hand edit to timing is caught because `apply` recomputes `rev` and sees the drift.

**Other field rules:** `id` unique per day (auto-slugged from title if omitted); exactly one of
`end`/`duration`; times resolve to absolute instants via `date`+`timezone`; `run` is an argv array
whose `argv[0]` must be an allowlisted absolute path (§9); **unknown fields are rejected everywhere —
on both read and write** (`#[serde(deny_unknown_fields)]`), so an agent gets a hard error instead of
silent drift; overlaps allowed (query semantics in §8).

**`notify` lead & the no-double-notify rule.** When omitted, `notify` takes `config.notify.default_lead`
(default **`5m`**). A lead of `0m` — or any lead whose resulting notify instant coincides with the
block's `start` — does **not** schedule a separate `notify` trigger: the `start` event already
notifies, so `apply` emits the `notify` trigger only when `notify_at < start`. A block is therefore
never double-notified at a single instant (Inv-16).

### 6.4 Fired-event ledger (at-most-once)

A fresh `rev` proves a trigger is *current* but does not prevent **double execution** from scheduler
retries, duplicate invocations, or two racing `fire` processes. The durable ledger `fired.json`
closes that gap. Its key is `(date, id, event, rev, scheduled_at)`. `fire` performs an **atomic
check-and-set** (under the same lockfile as writes) *before* sending a notification or running
automation: if the key is already present, `fire` is a no-op; otherwise it records the key, then
acts. This guarantees at-most-once notification and at-most-once automation per scheduled occurrence
(Inv-14).

## 7. Block lifecycle (state machine)

```
                 apply schedules                    fire(start)            user marks
   pending ───────────────────────▶ pending ──────────────────▶ active ───────────────▶ done
      │   (now > start+grace, still pending)                       │   user marks ─────▶ skipped
      │                                                            │   fire(end) while active:
      └──────────────────▶ missed  ◀──────────────────────────────┘     ├─ auto_done_on_end → done
              (start window elapsed, never fired)                        └─ else            → expired
```

States:
- **pending** — authored, not started.
- **active** — `fire(start)` ran (notified / executed) within the start grace window.
- **done** — completed (user, or `fire(end)` when `config.auto_done_on_end = true`). Terminal.
- **skipped** — user explicitly skipped. Terminal.
- **missed** — `now > start + grace` and the block never fired (e.g. machine asleep). Terminal.
- **expired** — block became `active`, reached its `end`, and was never closed (auto-done off).
  Terminal; distinguishes "started but you never closed it" from "never happened" (`missed`) and
  "you finished it" (`done`).

**Event-specific reconciliation.** Overdue triggers are *not* handled generically — each event has
its own rule, because a missed lead notification must never mark a block `missed` before its start:

| Event | On time (`now ≤ target + grace`) | Overdue (`now > target + grace`) |
|-------|----------------------------------|-----------------------------------|
| `notify` | Send the lead notification. | **No-op only.** Never changes status. |
| `start` | Mark `active`, notify, run automation (if enabled). | If still `pending` → `missed`. No notify/run. |
| `end` | If `active` → `done` (auto) or `expired`. | Same: `active` → `done`/`expired`; already-terminal → no-op. |

If a machine is off so no `end` ever fires, the next `apply`/query reconciles overdue `active`
blocks to `expired`. `done`/`skipped`/`missed`/`expired` blocks and `fire.log` are immutable history
(Inv-7).

**Read/write contract.** Query commands (`show`/`now`/`next`/`agenda`) reconcile overdue blocks **in
memory only** — they compute the reconciled view for display but **never persist and never take the
write lock**. The plan file changes solely on `apply`, `fire`, or an explicit mutation command
(`set`/`add`/`edit`/`rm`/`done`/`skip`/`clear`). This keeps reads side-effect-free, non-blocking, and
safe to run concurrently with a writer (Inv-18). Every mutation command is a **single locked
read-modify-write transaction**: it acquires the store lock, then loads → mutates → writes under that
held lock, so two concurrent mutations can never lose each other's blocks (Inv-17).

## 8. CLI surface

Binary **`ccplan`** (optional short alias `plan`). Agent-safe conventions are themselves
requirements (Inv-2):
- **Never interactive.** No prompts. Destructive ops need an explicit flag, not a TTY answer.
- **`--json` on every read**, stable documented schema. **Reads that can match multiple blocks
  return a JSON array** (Inv-11).
- **Deterministic exit codes:** `0` ok · `2` usage/validation · `3` not found · `4` scheduler/apply
  failure · `5` automation refused (policy/allowlist) · `6` history-conflict (needs override).
- **Whole-plan I/O over stdin/stdout** for one-shot agent authoring.

Authoring / mutation:
| Command | Purpose |
|---|---|
| `ccplan set --from <file\|-> [--date D] [--override-history]` | Replace the day's plan from TOML/JSON. **Terminal blocks (done/skipped/missed/expired) are always retained**, carried forward by `id` *even if the incoming plan omits them*; only pending/active blocks are replaced. Modifying or dropping a terminal block, or reusing a terminal `id`, requires `--override-history` (else exit `6`). Validates, writes atomically. |
| `ccplan add --title T --start 11:00 [--end\|--duration] [--notify] [--run …] [--id]` | Upsert one block (same `id` ⇒ update a non-terminal block). |
| `ccplan remind "T" --in 30m [--id]` | Sugar over add+apply: resolve `now + duration` (clock TZ, minute granularity) into a zero-lead block — so only the `start` event notifies, exactly at the target (Inv-16) — then auto-apply. `--in` reuses `DurationSpec` (max 24h, capping rollover to tomorrow). Same upsert/terminal-history rules as `add`. |
| `ccplan edit <id> --start … --title …` | Patch a non-terminal block. |
| `ccplan rm <id>` | Remove a pending block. |
| `ccplan done <id>` / `ccplan skip <id>` | Status transition. |
| `ccplan clear [--date D] --yes` | Retire the day: **archive the plan** to `archive/` (preserving terminal history), then run the same reconciler `apply` uses to remove that day's triggers. Requires `--yes`. `--purge` instead of archiving permanently deletes — the sole explicit exception to immutable history (Inv-7/12). |

Reads / queries (all `--json`):
| Command | Returns |
|---|---|
| `ccplan show [--date D]` | The full day (object). |
| `ccplan now` | **Array** of all blocks active at this instant (overlaps ⇒ ≥1). |
| `ccplan next` | **Array** of the next block(s) by soonest future start (ties ⇒ >1). |
| `ccplan agenda` | Remaining blocks with countdowns (array). |

Lifecycle / system:
| Command | Purpose |
|---|---|
| `ccplan apply [--date D] [--dry-run]` | Reconcile OS triggers to the plan via the Scheduler backend. Idempotent. Scheduler mutations go through this one reconciler (invoked by `apply` and by `clear`). Imports session env on Linux (§6.1). |
| `ccplan fire --date D --id ID --event notify\|start\|end --rev R --at <rfc3339>` | **Internal**, invoked by a trigger. Checks identity + freshness against the ledger (§6.4), then applies the event-specific rule (§7); records to `fire.log`. Not for humans. |
| `ccplan status` | Scheduler health: live triggers, their rev vs current, drift, last fires. |
| `ccplan doctor` | Verify the native Scheduler/Notifier backend (systemd user instance / launchd / Task Scheduler; notification capability; timezone) and print fixes. |

**Agent integration.** An `AGENTS.md` documents the canonical recipe (`set --from -` → `apply`;
query via `--json`). No separate API — the CLI contract is the surface.

## 9. Automation (`run:`) & security

`run:` executes a command when a block fires. The plan file is therefore a **trust boundary**:
whoever writes the plan can cause code to run at a scheduled time — and the agent writes the plan.
This is the highest-risk part of the design. Controls (defense in depth):

- **Opt-in, globally and per-run.** Automation is **off by default** (`config.automation.enabled:
  false`). A block with `run:` under disabled automation ⇒ exit `5`, visible, never silent.
- **Allowlist of absolute executables.** `config.automation.allowed_executables` is a list of
  **absolute paths**. `run:`'s `argv[0]` must be an absolute path present in the allowlist, or `fire`
  refuses (exit `5`, logged). Ownership + argv-only alone is *not* sufficient.
- **argv vector, no shell ever (NG9).** `run:` is exec'd as an argv vector (no `sh -c`), eliminating
  shell-metacharacter injection. There is no shell-string mode.
- **Single execution path.** `fire` only ever runs the command **as stored in the on-disk plan**
  (vetted against the allowlist). No code path execs a string passed on the command line (Inv-8).
- **Ownership & perms check.** Refuse if the plan file is not owned by the invoking user or is
  group/world-writable.
- **At-most-once.** The fired-event ledger (§6.4) guarantees automation runs at most once per
  scheduled occurrence even under scheduler retries or racing processes (Inv-14).
- **Logged & attributable.** Every fire (argv, exit status, stdout/stderr tail, timestamp, rev)
  appends to `log/fire.log`. `--dry-run` prints what *would* run.
- **Bounded.** Per-run timeout (config; default 5m); runaway commands killed and logged.
- Out of scope (NG6): sandboxing, egress control, capability dropping.

## 10. Invariants

- **Inv-1 (Single source of truth).** The plan file is canonical; triggers/notifications/caches are
  derived and fully reproducible via `apply`. Delete all derived state, re-`apply` ⇒ equivalent system.
- **Inv-2 (Agent-safe CLI).** Non-interactive; reads support `--json` with a stable schema; exit
  codes stable and meaningful; no command blocks on a TTY.
- **Inv-3 (Idempotence).** `apply` is convergent; `add`/`set` are upserts, not duplicators.
- **Inv-4 (Stable identity).** Each block has a `day`-unique, edit-stable `id`.
- **Inv-5 (Unambiguous time).** Every block resolves to an absolute instant via `date` + frozen
  `timezone` + wall-clock; DST/offset resolved at author time.
- **Inv-6 (Fresh-fire only; no stale replay).** A trigger acts **iff** `(date,id,event,rev)` matches
  the block's current schedule and the event-specific freshness rule (§7) holds. Otherwise it is a
  no-op, reconciled per event type. No catch-up replay (NG8).
- **Inv-7 (Immutable history).** `done`/`skipped`/`missed`/`expired` blocks and `fire.log` are
  append-only and never silently destroyed. `set` retains terminal blocks even when omitted from the
  incoming plan; the only ways to alter/remove them are `set --override-history` and `clear --purge`.
- **Inv-8 (One execution path).** Automation runs only commands present in the on-disk plan, vetted
  by §9 (allowlist, ownership, argv-only). No execution of command-line-passed strings.
- **Inv-9 (Atomic, non-destructive writes).** Mutations are write-temp-then-rename under a lockfile;
  a crash mid-write never corrupts/truncates the plan; concurrent writers are detected (one fails clean).
- **Inv-10 (Self-contained namespace).** The tool manages only triggers under its own
  `ccplan-`/`io.ccplan`/`\ccplan` namespace; `clear` touches exactly that day's file and its triggers.
- **Inv-11 (Array query semantics).** Because overlaps are allowed, `now`/`next`/`agenda` return JSON
  **arrays**; an empty result is `[]`, never an error.
- **Inv-12 (Explicit destruction).** Every irreversible op (`clear --purge`, `set --override-history`)
  requires an explicit flag; none infer intent from a TTY.
- **Inv-13 (Lifecycle closure).** No block remains `active` past its `end`: `fire(end)` or the next
  reconcile transitions it to `done`/`expired`.
- **Inv-14 (At-most-once fire).** Per `(date,id,event,rev,scheduled_at)`, notification and automation
  each run at most once, enforced by the durable ledger's atomic check-and-set (§6.4).
- **Inv-15 (Schedule-only rev).** `rev` is a pure function of trigger-affecting fields (`id`, timing,
  notify lead) and excludes status/history/title/tags/run, so lifecycle and content edits never
  invalidate live triggers (§6.3).
- **Inv-16 (No double-notify).** At most one notification is emitted per block per fire instant. When
  the `notify` lead places the notify instant at (or after) `start`, no separate `notify` trigger is
  scheduled — only the `start` event notifies (§6.3).
- **Inv-17 (Transactional mutation).** Every mutating command performs its load → mutate → write as a
  single transaction under the held store lock, so concurrent mutations are serialized and no
  committed block is ever lost to a stale in-memory read (§7, extends Inv-9).
- **Inv-18 (Side-effect-free reads).** Query commands never persist state and never take the write
  lock; reconcile-on-query is computed in memory. Only `apply`/`fire`/explicit mutations write (§7).

## 11. Failure modes & handling

| Situation | Behavior |
|---|---|
| Machine asleep/off at fire time | Next `apply`/`doctor`/query reconciles per event: overdue `notify` ignored; `pending` past start+grace → `missed`; overdue `active` → `expired` (Inv-6/13). No late replay. |
| Stale trigger fires after a re-plan | `rev` mismatch ⇒ `fire` no-ops, backend removes the orphan (Inv-6, §6.3). |
| Duplicate / retried fire, racing processes | Ledger check-and-set ⇒ second invocation no-ops before notifying/running (Inv-14, §6.4). |
| Notification capability unavailable | Surfaced up front: `doctor` reports it, `apply` warns loudly (never silently degrades). At fire time a send failure is logged and non-fatal — `fire` still records and (if enabled) runs automation (§6.1). |
| Scheduler backend unavailable | `apply` exits `4` with `doctor` guidance; file ops still work — planning degrades to "no alerts" rather than failing. |
| `run:` not on allowlist / automation off | Exit `5`, logged; block still follows its own lifecycle. |
| `run:` fails / times out | Logged with exit status; non-zero surfaced in `status`; lifecycle unaffected. |
| Invalid plan (bad time, dup id, both end+duration, unknown field) | `set`/`add` reject with exit `2`; **`apply` re-validates and refuses to schedule an invalid plan** (covers hand edits, §12). On-disk plan unchanged on reject. |
| Two writers at once | Lockfile; second fails clean (Inv-9). |
| `id` reused / terminal block dropped by `set` | Without `--override-history` ⇒ exit `6`; terminal block otherwise retained. |
| Clock/timezone change mid-day | Times are frozen absolute instants; `doctor` warns if system tz ≠ plan tz. |

## 12. Implementation notes

- **Language: Rust, edition 2024.** Single self-contained binary per OS; the binary is the
  scheduler's fire target, so startup must be fast. Pinned, current (2026) stack:
  | Concern | Crate | Notes |
  |---|---|---|
  | CLI | `clap` 4 (derive) + `clap_complete` + `clap_mangen` | completions + man page via `build.rs` |
  | Plan/config format | `toml` 1 + `serde` 1 | maintained; agent-editable (§6.3). **Not** `serde_yaml`/`serde_yml` (archived/advisory). |
  | JSON output | `serde_json` 1 | machine output for agents |
  | Date/time + tz/DST | `jiff` 0.2 (`serde`) | best-in-class IANA-zone + DST disambiguation; bundles tzdb on Windows |
  | OS dirs | `directories` 5 | XDG / macOS / Windows data·config·state dirs |
  | Hashing (`rev`, id) | `blake3` | short schedule-rev + id hashing |
  | Notifications | `notify-rust` 4 | Linux D-Bus / macOS / Windows wscript MsgBox |
  | File locking | `fs2` + temp-then-rename | atomic, locked writes (Inv-9) |
  | macOS plist | `plist` 1 | authors the LaunchAgent plist (maintained) |

  No archived/unmaintained crates and no `unsafe` (`#![forbid(unsafe_code)]`): Windows scheduling
  shells out to `schtasks.exe` with an XML task rather than the archived `planif` or `windows`-rs COM.
- **Testability architecture (drives 100% coverage).** Split into a **library crate** (all logic)
  plus a **~10-line binary shim** (`main`: parse args → `lib::run(cli, out, &ctx)` → `ExitCode`).
  All side effects are injected as traits on a `Context { clock, scheduler, notifier, fs_root }`:
  `Clock` (jiff has no clock-mocking — `now()` *must* be injected), `Scheduler`, `Notifier`. Tests
  use `FixedClock` + recording fakes and never touch real systemd/launchd/Task Scheduler. Only the
  **side-effecting backend methods** (the actual `Command`/native-API calls) and the `main` shim are
  excluded from coverage, via `#[cfg_attr(coverage_nightly, coverage(off))]` on **those methods**. The
  **pure helpers inside the platform modules** — name/identity formatting, XML/plist building,
  command-output parsing, calendar-time formatting — are *not* excluded; they live in coverage-on
  submodules with unit tests. A **module-scope** `#![cfg_attr(coverage_nightly, coverage(off))]` is
  forbidden: it hides untested logic behind a green 100% (the exact failure mode the coverage gate
  exists to prevent).
- **Scheduler backends (per-OS, behind one trait).** Linux: shell out to `systemd-run --user
  --on-calendar=…` (transient one-shot timers auto-clean via `RemainAfterElapse=no`; set
  `AccuracySec=1s`; pass `--setenv=DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$UID/bus` so
  notifications reach the session). macOS: author a `~/Library/LaunchAgents` plist (`plist` crate) +
  `launchctl bootstrap`; since `StartCalendarInterval` recurs, the one-shot `fire` path **boots
  itself out** and deletes its plist as its last step. Windows: shell out to `schtasks.exe /Create
  /XML` (a TimeTrigger with second-precision `StartBoundary`) under a `\ccplan\` folder, with
  `EndBoundary` + `DeleteExpiredTaskAfter` for auto-cleanup; run the fire path as a
  `windows_subsystem = "windows"` shim so no console flashes.
- **Writer model.** The CLI is the recommended writer; hand/agent edits are allowed but `apply`
  re-validates and **rejects** an invalid/unsafe plan before scheduling. All CLI writes are atomic +
  locked (Inv-9). `rev` is recomputed from the file on every `apply`, so untracked timing edits are
  detected as trigger drift.
- **Trigger naming.** `id` is hashed (short blake3) and OS-escaped (`systemd-escape`, launchd label
  rules, Task Scheduler path rules) → valid, collision-free names encoding `date` + `event`.
  `triggers.json` records owned units + their `rev` for diff-based reconcile and orphan cleanup.
- **Grace window** is configurable (`config.grace`, default ~90s) and applied per the §7 event table.
- **Quality gates.** `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `RUSTFLAGS="--cfg coverage_nightly" cargo +nightly llvm-cov --fail-under-lines 100` (the cfg makes
  the documented `coverage(off)` exclusions compile + apply),
  `cargo deny check` — all green in CI on a Linux/macOS/Windows matrix.

## 13. Build sequence

Platform-parallel: the core is OS-agnostic, then all three native backends are first-class together,
then automation and the operational surface.

- **Phase 0 — Core CLI / schema / store (OS-agnostic):** computed schedule `rev`;
  `set`/`show`/`add`/`edit`/`rm`/`done`/`skip`/`clear` with archive over the TOML store (atomic
  locked writes, validation, ids); `now`/`next`/`agenda` (arrays). No OS triggers yet (G1, G2, G4,
  Inv-2/4/5/7/9/11/15).
- **Phase 1 — Native scheduling & notifications (Linux, macOS, Windows together):**
  `Scheduler`/`Notifier` traits + the systemd, launchd, and Task Scheduler backends; `apply` ⇄
  triggers; `fire` with identity/freshness/ledger and the event-specific rules; missed/expired
  reconcile; `doctor` per backend (G3, G6, G7, Inv-1/3/6/13/14).
- **Phase 2 — Automation:** `run:` argv + allowlist + ownership/timeout + `fire.log` + `--dry-run`
  (G5, Inv-8).
- **Phase 3 — Operational surface:** `status`, login/boot re-`apply` per OS, `AGENTS.md`/skill recipe,
  per-OS install/packaging.
- **Later:** notification actions (Done/Snooze), status-line/top-bar integration, calendar *import*,
  daily templates.

## 14. Alternatives considered

- **Taskwarrior + glue.** Great CLI/hooks, but hooks fire on data change, not at a wall-clock time;
  time-triggered alerts/automation need an external scheduler anyway, and the task model fights the
  time-blocked day shape. Inspiration, not the core.
- **Pure OS timers, no plan file.** Free firing/automation but no human/agent day model, status, or
  "show my day." It becomes our compile *target*, not the product.
- **CalDAV/khal backend.** Real calendar + reminders + phone sync, but no command execution (fails
  G5) and heavier moving parts. Reserved as a future *import* source (NG3).
- **Single resident daemon everywhere.** Simplest cross-platform firing, but a process to supervise
  and keep alive across sessions on every OS, duplicating what the OS scheduler already does
  reliably. Rejected: ccplan requires the native scheduler on each platform and ships no daemon.
