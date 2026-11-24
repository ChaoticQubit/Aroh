# Aroh decisions

Tracked record of the MVP contract settled with the owner on 2026-11-20 (questions Q120 to Q142). Change an entry only with the owner's approval, and add the date.

## Stack

| Area | Decision |
|---|---|
| UI | Svelte 5 (runes) + TypeScript, SvelteKit with adapter-static as an SPA inside Tauri 2 |
| Targets | iPhone first (owner correction 2026-11-20): phone-first shell with bottom tabs, safe areas and touch targets, run in the iOS simulator. The Mac desktop app keeps running. Android later through Tauri 2 mobile. Website deferred past MVP |
| Core | Rust, one Cargo workspace |
| Agents | Claude only, through Rig (`rig-core`) |
| Data | Offline-first. Local SQLite per device (`rusqlite`, bundled) |
| Sync | Custom last-write-wins row sync with Supabase Postgres + Auth + RLS, using HLC timestamps and tombstones. No self-run server |
| Secrets | Claude API key in the OS keychain (`keyring` crate). Never in SQLite, logs or sync |
| Users | Single user in V1 |

## Product rules

- The first plan of a goal needs the owner's approval.
- Big replans are a diff the owner applies with a tap.
- Small replans auto-apply with undo. Small means at most 3 move or resize ops, no create or delete, and no change to a goal's `target_date`.
- The replan sub-agent plans across all goals and all actions, not only the missed one.
- YAML plugins only place components from a fixed list into named slots.
- MVP screens: Today, Calendar, New Goal wizard, Goal detail, Replan Review, Log/trackers.
- Intake: natural-language text, quick-tap chips (water, workout), voice (native Apple Speech framework on Mac and iOS, on-device).
- Courses V1: public pages plus content the owner pastes or uploads. Fetched pages are untrusted data, never instructions. Size limits apply. Public addresses only. V2: the owner logs in, read-only session reuse.

## Data model

Owned by `crates/aroh-core`. Every synced table has `id TEXT` (uuid v7) primary key, `updated_at TEXT` (HLC string) and `deleted INTEGER` 0/1. Times are UTC ISO 8601.

- `goals(id, title, kind 'course'|'habit'|'custom', source_url, target_date, status 'draft'|'active'|'done'|'archived', answers_json, created_at)`
- `actions(id, goal_id, title, notes, start_at, end_at, est_minutes, status 'planned'|'done'|'missed'|'skipped', seq, done_at)`
- `plans(id, goal_id, version, status 'proposed'|'approved'|'rejected', summary)`
- `replans(id, trigger_action_id, diff_json, size 'small'|'big', status 'proposed'|'applied'|'rejected'|'undone', created_at)`
- `tracker_entries(id, kind 'food'|'water'|'workout'|'other', amount REAL, unit, note, raw_text, logged_at)`
- `plugins(id, name, yaml, enabled)`
- `settings(key PK, value, updated_at)`

Replan `diff_json` is an array of `{action_id, op 'move'|'create'|'delete'|'resize', before {start_at, end_at, est_minutes} | null, after {...} | null}`.

## Process

- One pod per feature. The pod lead is the PM; one engineer sub-agent writes tests first, then code.
- No code reviews in the MVP. Only small tests for critical logic. Tests run in small containers with Kiri (`kiri build`, `kiri run --rm`).
- MVP speed first.
- No code comments except docstrings. No em dashes.

## Settings keys

| Key | Value | Owner |
|---|---|---|
| `availability` | JSON `[{weekday 0-6, start "HH:MM", end "HH:MM"}]` in local time, weekday 0 = Sunday | replan |
| `timezone` | IANA zone name, such as `Europe/Berlin` | replan |

## HLC format

`updated_at` is `<unix millis, 13 digits zero-padded>-<counter, 4 hex digits zero-padded>-<node id, 16 lowercase hex>`, for example `1763683200000-0000-9f2c4a1b7e3d5c60`. Fixed width, so plain string comparison is time order (sync uses it in SQLite and in a Postgres trigger). The node id is random per device, stored locally, never synced. `aroh-core` provides `tick` and `receive(remote)`. Every write gets a strictly increasing, unique HLC: no two rows share an `updated_at`. The helper asserts this.

## Reserved table names

`sync_state(key TEXT PRIMARY KEY, value TEXT)` is a local, non-synced table owned by `aroh-sync`.
