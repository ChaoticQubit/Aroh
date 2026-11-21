# Pod 8: aroh-domain, aroh-store, Supabase Schema and Sync

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The shared Aroh data layer of the rebuild: row types, HLC, validation and recurrence in `aroh-domain`; SQLite migrations, repositories and the LWW sync engine in `aroh-store`; the new Supabase schema with RLS for every table, proved by a Postgres test.

**Architecture:** `aroh-domain` has no I/O and is used by both engines (app and server). It owns one registry of the 18 synced tables (`schema::SYNCED`): table name, columns and each column's kind. Every other copy of the column list (row structs, SQLite DDL, Postgres DDL) is checked against that registry by one parity test. `aroh-store` converts rows between SQLite values and the sync wire (JSON) by column kind, so the repositories and the sync engine share one codec. The sync engine is the MVP `aroh-sync` merge and page logic, generalized to the registry and driven through an async `Remote` trait; the PostgREST adapter, auth and keychain stay in `aroh-sync` until pod 10 wires the new app engine. Postgres mirrors the registry with real types (uuid, boolean, jsonb, timestamptz), keeps the MVP LWW trigger and adds per-user RLS on all 25 tables.

**Tech Stack:** Rust 2021, serde, serde_json, thiserror 2, uuid (v5, v7), chrono, chrono-tz, rrule 0.14, rusqlite 0.37 bundled, tokio (tests). Postgres 17 with pgvector (`pgvector/pgvector:pg17`) for the schema test.

## Global Constraints

- No code comments; docstrings only, directly above a declaration, 3 to 5 lines. SQL has no docstring form, so SQL files carry no comments. No em dashes.
- `assert!` for internal invariants that should be impossible; input validation returns `Err`.
- One copy of every helper: anything both engines need (HLC, LWW rule, registry, validation, recurrence, seed list, time format) lives in `aroh-domain`.
- `aroh-domain` and `aroh-store` never depend on `aroh-core` or `aroh-sync`. `aroh-core` re-exports the moved HLC so old callers keep compiling. Old crates stay until pod 10 replaces the screens.
- Tests run in Kiri: `HOME=/Users/chaoticqubit kiri build -f Dockerfile.test -t aroh-test-p8 .` then `HOME=/Users/chaoticqubit kiri run --rm aroh-test-p8`. Schema test: `KIRI_HOME=/Users/chaoticqubit sh supabase/tests/run.sh`.
- Branch `feat/aroh-domain-store` in its own worktree. Conventional commits. Commit, never push.

## Decisions

| Topic | Decision | Reason |
|---|---|---|
| Ids | Text uuid v7 on device, `uuid` on Postgres. Primary key `(user_id, id)` on Postgres, `id` on SQLite. | Composite key stops one tenant squatting another tenant's id (MVP test kept). |
| Wire format | Sync rows are the serde JSON of the row structs: booleans as JSON bool, JSON columns nested, times RFC 3339 UTC. | Matches PostgREST on boolean, jsonb and timestamptz columns, so the server schema uses real types. |
| Column kinds | `Text`, `Int`, `Bool`, `Json`, `Time`. SQLite stores Bool as 0/1, Json as text, Time as canonical RFC 3339 text. Pulled times are normalized to the canonical form. | One codec for repositories and sync. |
| Push filter | Push only rows whose HLC node is this device's node, scoped to the signed-in `user_id`. Cursors keyed `push:<user>:<table>` and `pull:<user>:<table>`. | Fixes the MVP re-push of pulled rows, keeps server-written tables (usage) read only for devices, and stops a foreign row from blocking pushes. |
| Derived rows | Materialized occurrences get a deterministic v7-shaped id (v5 hash of series id and occurrence) and carry the series row's `updated_at`. Seeded categories get a deterministic id from user id and slug and stamp `0000000000000-0000-0000000000000000`. | Two devices (or the server engine) derive identical rows, so LWW collapses them; any real edit or tombstone has a newer stamp and wins, so a deleted occurrence is never resurrected. No server unique constraint that would fail a whole push batch. |
| Who seeds | Both engines call `aroh_domain::seed::categories(user_id)` and insert with insert-or-ignore. | Seed rows are derived, never pushed (zero node). |
| Foreign keys | None between synced tables on either side. Server-only tables use foreign keys among themselves. | Pages arrive per table, in any order across devices. |
| Server LWW | `public.aroh_lww()` before insert or update: skips an older update, skips an equal stamp unless it turns a live row into a tombstone, sets `server_seq`, and takes a per-user transaction advisory lock first. | MVP rule, plus the lock makes a user's `server_seq` order match commit order, closing the MVP pull gap for one user. |
| Old schema | The rebuild migration drops the seven MVP tables, `sync_lww()` and `sync_server_seq`. `supabase/tests/rls.sql` is replaced by `rls_rebuild.sql`, which ports its LWW assertions. | Names collide (`goals`, `actions`, `tracker_entries`, `plugins`). MVP server sync stops working until pod 10, local MVP use is unaffected. |
| Client grants | Synced tables: select, insert, update, delete own rows, except `usage` (select only). `jobs`, `runs`, `run_events`: select own (Inbox progress, Realtime). `run_checkpoints`, `knowledge_chunks`, `memory_logs`, `provider_credentials`: no client access. `service_role` keeps full access. | Least privilege; pod 9 widens if a client write path is needed. |
| Embeddings | `extensions.vector(384)` with HNSW cosine index and a generated `tsvector` for hybrid retrieval. | Pod 7 confirmed multilingual-e5-small, 384 dims. A model change needs a re-embed job plus a migration (Q13). |
| Recurrence | `action_series` stores the RRULE line, a local start (`YYYY-MM-DDTHH:MM:SS`) and an IANA timezone. Window default 14 days, cap 500 occurrences per window, sub-daily FREQ rejected, COUNT at most 10000. "This and following" bounds the old series with UTC UNTIL one second before the split, gives the new series a fresh id and the remaining COUNT, tombstones the old series' planned unlocked occurrences from the split on, and leaves locked or done ones untouched. | Q34; RRULEs come from agents, so they are bounded. |
| HTTP | The PostgREST `Remote` adapter, auth and keychain are not in this pod. | Keeps `keyring` and `reqwest` out of a crate the server engine links; the adapter lands with the app engine wiring. |

## Synced tables (single column source)

Every synced table starts with `id`, `user_id` and ends with `updated_at` (HLC text), `deleted` (bool). Kinds: T text, I int, B bool, J json, D time. `?` marks nullable.

| Table | Columns between `user_id` and `updated_at` |
|---|---|
| categories | name T, color T (`#rrggbb`), seeded B |
| goals | category_id T?, title T, context T?, deadline T? (`YYYY-MM-DD`), status T (draft, active, paused, done, archived), autonomy T (ask, balanced, auto), created_at D |
| goal_links | from_goal_id T, to_goal_id T, kind T (parent, depends_on, supports, conflicts), confirmed B |
| goal_files | goal_id T, name T, kind T (document, image, audio, video, link), storage_path T?, url T?, mime T?, size_bytes I, ingest_status T (pending, processing, done, failed) |
| personas | goal_id T?, name T, toml T |
| plugins | goal_id T?, name T, version I, bundle_toml T, enabled B |
| trackers | goal_id T, plugin_id T?, name T, fields J, cadence J?, rules J |
| tracker_entries | tracker_id T, data J (values keyed by field key; `values` is reserved in SQL), source T (check_in, log, health), raw_text T?, logged_at D |
| action_series | goal_id T, title T, notes T?, rrule T, dtstart_local T, timezone T, duration_minutes I |
| actions | goal_id T, series_id T?, occurrence D?, title T, notes T?, start_at D, end_at D, status T (planned, done, missed, skipped), locked B, done_at D? |
| proposals | goal_id T, run_id T?, summary T, diff J, size T (small, big), status T (proposed, applied, rejected, undone), undo J?, created_at D |
| preferences | fact T, source T, noted_at D, confirmed B |
| busy_blocks | calendar_id T, event_id T, title T, start_at D, end_at D, all_day B, group_name T? |
| calendar_rules | calendar_id T?, series_key T?, pattern T?, group_name T, decision T (keep, ignore) |
| provider_settings | label T, vendor T, model T, base_url T?, is_default B |
| search_settings | label T, provider T, base_url T?, priority I, enabled B |
| budgets | scope T (call, run, goal_month, user_month), goal_id T?, limit_usd_micros I |
| usage | goal_id T?, run_id T?, vendor T, model T, input_tokens I, output_tokens I, cost_usd_micros I, occurred_at D |

Server-only: `jobs`, `runs`, `run_checkpoints` (primary key `run_id, step`), `run_events` (unique `run_id, seq`), `knowledge_chunks` (primary key `user_id, id`), `memory_logs` (primary key `user_id, log, position`), `provider_credentials` (unique `user_id, setting_id`). Each has `user_id uuid not null references auth.users on delete cascade`.

## File Structure

| Path | Responsibility |
|---|---|
| `Cargo.toml` (root) | new workspace deps: `aroh-domain`, `aroh-store`, chrono, chrono-tz, rrule; uuid `v5` |
| `crates/aroh-domain/src/lib.rs` | `Error`, `Result`, `new_id` |
| `.../src/hlc.rs` | HLC moved from `aroh-core` unchanged, plus `node()` accessor; tests moved with it |
| `.../src/lww.rs` | `Version`, `remote_wins` moved from `aroh-sync/src/merge.rs` with tests |
| `.../src/time.rs` | canonical RFC 3339 format and normalization |
| `.../src/schema.rs` | `Kind`, `Column`, `TableDef`, `SYNCED` registry |
| `.../src/rows.rs` | row structs, enums, `Record` trait, validation |
| `.../src/recur.rs` | window materializer, split, deterministic occurrence ids |
| `.../src/seed.rs` | seeded categories |
| `crates/aroh-core/src/hlc.rs` | deleted; `lib.rs` re-exports `aroh_domain::hlc` |
| `crates/aroh-store/src/lib.rs` | `Error`, `Result` |
| `.../src/db.rs` | open, migrate, SQLite DDL for the 18 tables plus `sync_state` |
| `.../src/codec.rs` | SQLite value to wire JSON and back, by column kind |
| `.../src/repo.rs` | generic `put`, `get`, `list`, `delete`; `materialize`, `split_series` |
| `.../src/sync.rs` | `Remote` trait, cursors, `pending`, `apply_page`, `sync_all` |
| `.../tests/repo.rs`, `.../tests/sync.rs`, `.../tests/parity.rs` | integration tests against in-memory SQLite |
| `supabase/migrations/20261121000000_rebuild.sql` | new schema, trigger, RLS, grants |
| `supabase/tests/stub_auth.sql`, `run.sh` | add `extensions` schema and `service_role`; pgvector image; run `rls_rebuild.sql` |
| `supabase/tests/rls_rebuild.sql` | tenant isolation, credentials denial, LWW trigger, RLS sweep |

---

### Task 1: aroh-domain scaffold, HLC and LWW moved

**Files:** create `crates/aroh-domain/{Cargo.toml,src/lib.rs,src/hlc.rs,src/lww.rs}`; modify root `Cargo.toml`, `crates/aroh-core/{Cargo.toml,src/lib.rs}`, `crates/aroh-core/tests/core.rs`; delete `crates/aroh-core/src/hlc.rs`.

- [ ] Move the two HLC tests (`hlc_orders_causally`, `hlc_string_order_matches_time_order`) from `aroh-core/tests/core.rs` into `aroh-domain/src/hlc.rs` `#[cfg(test)]`, and the six `merge.rs` tests into `lww.rs`. Run them: they fail to compile (no crate).
- [ ] Move `hlc.rs` with `Error::InvalidHlc` into `aroh-domain`, add `pub fn node(&self) -> &str`. Copy `remote_wins` and `Version` into `lww.rs` (the `aroh-sync` copy stays until pod 10 deletes that crate). `aroh-core` gets `pub use aroh_domain::hlc;` and loses its `InvalidHlc` variant.
- [ ] Run Kiri tests: all pass, `aroh-core` and `aroh-sync` tests still pass.
- [ ] Commit `feat(aroh-domain): crate with hlc and lww rule moved from mvp`.

### Task 2: registry, row structs and validation

**Files:** `src/time.rs`, `src/schema.rs`, `src/rows.rs`, `src/seed.rs`.

- [ ] Tests first: every `SYNCED` table starts with `id, user_id` and ends with `updated_at, deleted`; names unique; a sample of every struct serializes to exactly the registry's column keys; each validation rule below returns `Err` for a bad value and `Ok` for the sample; seed ids are stable for one user and differ across users.
- [ ] `Record` trait: `const TABLE: &'static TableDef`, `id`, `user_id`, `updated_at`, `set_updated_at`, `deleted`, `set_deleted`, `validate`. `TableDef` holds `check: fn(&serde_json::Value) -> Result<()>` that deserializes a wire row into its struct and validates it.
- [ ] Validation: uuid ids, HLC stamp, non-empty bounded text (titles 200 chars, TOML 64 KiB), `#rrggbb` colors, link not to itself, file `storage_path` under `goals/<goal_id>/` without `..` (non-link) and `https` `url` (link), tracker field keys unique and rules naming existing fields, `actions.end_at > start_at`, series RRULE (see Task 3), budget `goal_id` present iff scope `goal_month` and limit positive, calendar rule with at least one matcher, `base_url` `https` or `http` on localhost only, non-negative counters, entry values naming tracker fields (`validate_entry(&Tracker, &TrackerEntry)`).
- [ ] Commit `feat(aroh-domain): synced table registry, row types and validation`.

### Task 3: recurrence

**Files:** `src/recur.rs`.

- [ ] Tests first: daily 07:00 Europe/Berlin across 2026-10-25 keeps local 07:00 and moves UTC from 05:00 to 06:00, and across 2027-03-28 the reverse; window of 14 days yields 14 daily rows, end exclusive; a second run with the first run's occurrences returns nothing; a tombstoned occurrence key is not re-created; ids are deterministic and valid v7; split of COUNT=10 at the 4th occurrence gives old COUNT-bounded by UNTIL (3 occurrences) and new COUNT=7; split keeps UNTIL rules; sub-daily FREQ, bad timezone, DTSTART inside the rule and COUNT above 10000 return `Err`; a nonexistent local start time is handled without panic (behavior asserted from rrule 0.14).
- [ ] `materialize(series, from: DateTime<Utc>, days: u32, existing: &BTreeSet<DateTime<Utc>>) -> Result<Vec<Action>>`, `split(series, at: DateTime<Utc>, new_id) -> Result<(ActionSeries, ActionSeries)>`, `occurrence_id(series_id, at) -> String`, `DEFAULT_WINDOW_DAYS = 14`.
- [ ] Commit `feat(aroh-domain): rrule window materializer and series split`.

### Task 4: aroh-store migrations, codec and repositories

**Files:** `crates/aroh-store/{Cargo.toml,src/lib.rs,src/db.rs,src/codec.rs,src/repo.rs}`, `tests/repo.rs`, `tests/parity.rs`.

- [ ] Tests first: fresh migration creates every table, re-running is a no-op, a newer `user_version` errors; `pragma table_info` of each table equals the registry columns; the Postgres migration's `create table` column lists equal registry columns plus `server_seq`; put, get, list (equality filter on registry columns only), delete round trip for a goal and an action with JSON and bool columns; `put` rejects an invalid row and stamps a strictly greater HLC; `materialize` stores rows idempotently; `split_series` tombstones planned unlocked future occurrences and keeps locked and done ones.
- [ ] Commit `feat(aroh-store): sqlite migrations, codec and repositories`.

### Task 5: LWW sync engine

**Files:** `src/sync.rs`, `tests/sync.rs`.

- [ ] Tests first, with a test-only `MemoryRemote` that applies the server LWW rule and a global `server_seq`: two devices converge on edits, tombstones and concurrent edits to one row; a pulled row is not pushed back; rows of another `user_id` are rejected; a failed page commits neither rows nor cursor; pulled `+00:00` times are normalized; seeded and materialized rows are not pushed by a non-author device.
- [ ] `Remote` trait with `push` and `pull` returning `impl Future + Send`; `pending`, `apply_page`, `sync_all(conn, remote, user_id, clock)`; page size 500.
- [ ] Commit `feat(aroh-store): lww sync engine for every synced table`.

### Task 6: Supabase rebuild schema and RLS test

**Files:** `supabase/migrations/20261121000000_rebuild.sql`, `supabase/tests/{stub_auth.sql,run.sh,rls_rebuild.sql}`; delete `supabase/tests/rls.sql`.

- [ ] Test first: `rls_rebuild.sql` seeds one row per table for user A as the owner role; as B, every table shows no A rows and refuses update, delete and a forged insert with A's `user_id`; as A, synced tables, `jobs`, `runs`, `run_events` and `usage` are readable, `usage` refuses insert, `provider_credentials`, `run_checkpoints`, `knowledge_chunks`, `memory_logs` refuse select; LWW trigger assertions ported from `rls.sql`; malformed HLC refused; every public table has row level security; anon reads nothing. Run: fails (no schema).
- [ ] Write the migration; run `run.sh`: `rls: ok`.
- [ ] Commit `feat(supabase): rebuild schema with per-user rls`.

### Task 7: Report

DONE line: `DONE feat/aroh-domain-store <base>..<tip> VERIFIED` with Kiri test counts and the RLS result.
