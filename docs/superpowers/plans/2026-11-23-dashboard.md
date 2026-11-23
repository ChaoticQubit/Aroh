# Dashboard Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The Dashboard tab at `/dashboard` (Q69), matching the approved pod 30 mockup, fed by one Rust aggregation over the full local history.

**Architecture:** A pure `aroh_domain::dashboard::dashboard` turns goals, actions, series, trackers, entries and plugin targets into per-goal raw counts. `aroh_app::dashboard::read` reads the live rows; the Tauri command `app_dashboard` adds plugin field targets and the device zone. The SvelteKit route joins the result with the snapshot (titles, category slots), filters by chip and sums the shown goals for the tiles. Chart geometry lives in a tested TS module.

**Tech Stack:** Rust (chrono, chrono-tz, rrule), Tauri 2, SvelteKit with Svelte 5 runes, Tailwind tokens, vitest, Kiri containers.

## Global Constraints

- Decisions: `docs/agent-decisions.md`, section "Dashboard (pod 34)". Binding: Q69; design: Q41, Q70.
- No code comments except 3-5 line docstrings above declarations. No em dashes.
- Status words and colours for done, missed and skipped come from `apps/aroh/src/lib/ui/status.ts`.
- Do not change `--cat-*` or `--series-*`; add `--chart-*` and `--chart-cat-*` only.
- Do not touch the shell, top bar, plan card or goal screen.
- Conventional commits, no Co-Authored-By trailer, stage explicit paths only (never `git add -A`: `graphify-out` is an untracked symlink).

---

### Task 1: Domain aggregation (`crates/aroh-domain/src/dashboard.rs`)

**Files:**
- Create: `crates/aroh-domain/src/dashboard.rs` (module, constants, types, functions, `#[cfg(test)] mod tests`)
- Modify: `crates/aroh-domain/src/lib.rs` (add `pub mod dashboard;`)

**Interfaces (produces):**
- `pub const WEEKS: usize = 6; pub const TREND_WEEKS: usize = 7; pub const ON_TRACK_SHARE: f64 = 0.85; pub const AHEAD_DAYS: i64 = 7;`
- `pub enum Pace { Ahead, OnTrack, Behind }` (serde snake_case)
- `pub struct Counts { pub done: u32, pub due: u32 }`
- `pub struct Trend { pub tracker_id: String, pub label: String, pub unit: Option<String>, pub points: Vec<Option<f64>>, pub target: Option<f64> }`
- `pub struct GoalStats { goal_id, done, due, plan_end: Option<NaiveDate>, finish: Option<NaiveDate>, pace: Pace, streak: u32, best_streak: u32, week: Counts, month: Counts, minutes: Vec<i64>, trend: Option<Trend> }`
- `pub struct Dashboard { pub weeks: Vec<NaiveDate>, pub trend_weeks: Vec<NaiveDate>, pub goals: Vec<GoalStats> }`
- `pub struct Rows<'a> { goals, actions, series, trackers, entries: &'a [..], targets: &'a BTreeMap<String, BTreeMap<String, f64>> }` (targets keyed by tracker id, then field key)
- `pub fn shown(goal: &Goal) -> bool` (not draft, not archived, not Routines)
- `pub fn is_due(a: &Action, now) -> bool`
- `pub fn pace(done, due, finish: Option<NaiveDate>, deadline: Option<NaiveDate>) -> Pace`
- `pub fn last_occurrence(series: &ActionSeries) -> Result<Option<DateTime<Utc>>>` (None when unbounded)
- `pub fn projected_finish(actions: &[&Action], series_ends: &[Option<DateTime<Utc>>], owed: u32, zone) -> (Option<NaiveDate>, Option<NaiveDate>)` (plan end, finish)
- `pub fn streaks(actions: &[&Action], now, zone) -> (u32, u32)`
- `pub fn week_starts(now, zone, n) -> Vec<NaiveDate>` (Mondays, oldest first)
- `pub fn weekly_minutes(actions: &[&Action], weeks: &[NaiveDate], zone) -> Vec<i64>`
- `pub fn dashboard(rows: &Rows, now, zone) -> Dashboard`

- [ ] **Step 1: Write failing tests** in `mod tests`: `shown_hides_draft_archived_and_routines`; `due_counts_ended_or_decided_actions`; `pace_rules` (85 % boundary, finish after deadline is Behind, Ahead needs nothing owed and 7 days, no deadline never Ahead); `bounded_series_last_occurrence_and_unbounded_none`; `finish_slips_by_owed_sessions_at_average_gap` (plus single action gap 1 day, no actions None, unbounded series None); `streak_skips_empty_and_skipped_days_and_open_today`; `best_streak_is_longest_run`; `weekly_minutes_buckets_by_local_week` (Berlin 23:30 Sunday UTC belongs to next week locally); `dashboard_goal_order_completion_and_trend`.
- [ ] **Step 2: Run** `HOME=/Users/chaoticqubit kiri build -f Dockerfile.test -t aroh-test . && HOME=/Users/chaoticqubit kiri run --rm aroh-test cargo test -p aroh-domain dashboard`. Expected: compile errors (functions missing).
- [ ] **Step 3: Implement** the functions above.
- [ ] **Step 4: Run** the same command. Expected: all `dashboard::tests` pass.
- [ ] **Step 5: Commit** `feat(domain): dashboard aggregation for goals`.

### Task 2: App read and Tauri command

**Files:**
- Create: `crates/aroh-app/src/dashboard.rs` with `pub fn read(conn, user_id, targets, now, zone) -> Result<Dashboard>` (store errors only)
- Modify: `crates/aroh-app/src/lib.rs` (`pub mod dashboard;`)
- Test: `crates/aroh-app/tests/dashboard.rs` (`read_skips_tombstones_and_reads_full_history`: an action 120 days old counts, a tombstoned goal is absent)
- Create: `apps/aroh/src-tauri/src/commands/dashboard.rs` with `#[tauri::command] pub fn app_dashboard(state) -> Result<Dashboard, String>`: parses each enabled plugin row with `aroh_plugins::parse`, maps tracker rows of that goal by name to `BundleTracker::targets`, calls `read` with `Utc::now()` and `crate::zone()`.
- Modify: `apps/aroh/src-tauri/src/commands/mod.rs` (`pub mod dashboard;`), `apps/aroh/src-tauri/src/lib.rs` (one handler line).

- [ ] Steps: failing test, run (red), implement, run (green), `cargo check -p aroh` on the host for the Tauri crate, commit `feat(app): app_dashboard command`.

### Task 3: TS chart helpers and view model

**Files:**
- Create: `apps/aroh/src/lib/app/dashboard.ts`: types mirroring the Rust result; `dashboard()` client call; `dashGoals(s, d)` joining titles, category slot, series slot (1-6 by stable order, else 0 = Other); `filterGoals(goals, chip)`; `chips(goals)`; `totals(goals)` (done, due, week and month rates, max streak and best).
- Create: `apps/aroh/src/lib/ui/components/dashboard/geometry.ts`: `stackWeek(series, i, y)` (bottom up, 2 px gap), `barScale(totals)` (via `niceScale` from `chart.ts`, from zero), `sparkline(points, target, w, h)` (one point centred, flat series mid-height, nulls skipped).
- Test: `apps/aroh/src/lib/app/dashboard.test.ts`, `apps/aroh/src/lib/ui/components/dashboard/geometry.test.ts`.
- [ ] Steps: failing tests, `pnpm vitest run <files>` (red), implement, green, commit `feat(dashboard): view model and chart geometry`.

### Task 4: Route and components

**Files:**
- Create: `apps/aroh/src/routes/dashboard/+page.svelte`
- Create under `apps/aroh/src/lib/ui/components/dashboard/`: `DashboardScreen.svelte` (chips, tiles, sections, empty state), `GoalPace.svelte` (goal list), `WeeklyMinutes.svelte` (stacked bars, Goal/Category segmented, tap readout, legend), `TrackerTrends.svelte` (sparklines grid).
- Modify: `apps/aroh/src/lib/ui/tokens/tokens.css` (add `--chart-1..6`, `--chart-cat-1..6` light and dark).
- [ ] Steps: build, `pnpm check` clean, commit `feat(dashboard): dashboard screen`.

### Task 5: Verify in the simulator

- [ ] Merge master (`git merge master`), keep this route on any add/add conflict.
- [ ] Simulator `aroh-p34` on port 1440 with seeded long data; screenshots light and dark to `docs/screenshots/pod34/`; restore `gen/apple`.
- [ ] `e2e/pod31.sh` overflow guard on `/dashboard`.
- [ ] `graphify update .`; commit `docs(screenshots): pod 34 dashboard`.
