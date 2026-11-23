use crate::engine::Engine;
use crate::error::ServerError;
use crate::learning::{self, EditEvent};
use crate::rows::{get_row, list_rows, utc};
use crate::db;
use aroh_domain::hlc;
use aroh_domain::calendar::local_days;
use aroh_domain::recur::DEFAULT_WINDOW_DAYS;
use aroh_domain::rows::{Autonomy, CadenceFreq, Goal, GoalStatus, RuleOp, Tracker, TrackerEntry, TrackerField};
use aroh_domain::time::MISSED_GRACE_MIN;
use chrono::{DateTime, Datelike, Duration, LocalResult, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use crate::workflows::LEARN_PREFERENCE;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use troupekit_core::ids::RunId;
use utoipa::ToSchema;
use uuid::Uuid;

/// How often one process scans for triggers, in milliseconds.
/// Worker mode ticks every second and cron once a minute; dedupe keeps
/// concurrent scans of several processes correct.
pub const SCAN_EVERY_MS: i64 = 30_000;

/// How far back the scan derives missed actions, in hours.
/// Older history never fires, so a first deploy does not replan the past.
/// Matches the app tick's notion of missed otherwise.
pub const MISSED_LOOKBACK_HOURS: i64 = 48;

const BUSY_STATES: &str = "('queued', 'running', 'checkpointed', 'waiting_human', 'waiting_budget')";

/// What started an adaptation: action events, a tracker rule, the weekly
/// review, repeated owner edits or an Away all-day event. Stored as
/// `adaptations.source` and shown as the insight's kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Actions,
    Rule,
    Review,
    Edits,
    Calendar,
}

impl Source {
    /// The `adaptations.source` text of this source.
    /// One of `actions`, `rule`, `review`, `edits`, `calendar`.
    /// Returns a static string.
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Actions => "actions",
            Source::Rule => "rule",
            Source::Review => "review",
            Source::Edits => "edits",
            Source::Calendar => "calendar",
        }
    }
}

/// One breached rule of a tracker: its index in `rules` and the reason
/// the replan run gets, in the owner's words.
/// Produced by `breaches`.
#[derive(Debug, Clone, PartialEq)]
pub struct Breach {
    pub index: usize,
    pub reason: String,
}

fn number(entry: &TrackerEntry, field: &str) -> Option<f64> {
    entry.data.get(field).and_then(Value::as_f64).filter(|v| v.is_finite())
}

fn label(tracker: &Tracker, field: &str) -> (String, String) {
    let f: Option<&TrackerField> = tracker.fields.iter().find(|f| f.key == field);
    let name = f.map(|f| f.label.clone()).unwrap_or_else(|| field.to_string());
    let unit = f.and_then(|f| f.unit.clone()).map(|u| format!(" {u}")).unwrap_or_default();
    (name, unit)
}

fn cadence_days(tracker: &Tracker, from: NaiveDate, to: NaiveDate) -> Vec<NaiveDate> {
    let Some(cadence) = &tracker.cadence else { return vec![] };
    from.iter_days()
        .take_while(|d| *d <= to)
        .filter(|d| cadence.freq == CadenceFreq::Daily || cadence.weekdays.contains(&d.weekday()))
        .collect()
}

/// Local instant of `date` at `at` in `tz`; a time skipped by a clock
/// change moves to the next valid instant, an ambiguous one takes the
/// earlier. Returns `None` only when no instant exists within 3 hours.
pub fn local_instant(date: NaiveDate, at: NaiveTime, tz: Tz) -> Option<DateTime<Utc>> {
    let mut wall: NaiveDateTime = date.and_time(at);
    for _ in 0..=12 {
        match tz.from_local_datetime(&wall) {
            LocalResult::Single(t) => return Some(t.with_timezone(&Utc)),
            LocalResult::Ambiguous(early, _) => return Some(early.with_timezone(&Utc)),
            LocalResult::None => wall += Duration::minutes(15),
        }
    }
    None
}

fn missed_check_ins(tracker: &Tracker, entries: &[TrackerEntry], window_days: u32, now: DateTime<Utc>, tz: Tz) -> usize {
    let Some(cadence) = &tracker.cadence else { return 0 };
    let Ok(at) = NaiveTime::parse_from_str(&cadence.at, "%H:%M") else { return 0 };
    let since = hlc::parse(&tracker.updated_at).map(|(ms, _)| utc(ms as i64)).unwrap_or(now);
    let start = now - Duration::days(i64::from(window_days));
    let logged: Vec<NaiveDate> = entries.iter().map(|e| e.logged_at.with_timezone(&tz).date_naive()).collect();
    let today = now.with_timezone(&tz).date_naive();
    cadence_days(tracker, start.with_timezone(&tz).date_naive(), today)
        .into_iter()
        .filter(|d| {
            let Some(slot) = local_instant(*d, at, tz) else { return false };
            slot >= start && slot >= since && slot + Duration::minutes(MISSED_GRACE_MIN) <= now && !logged.contains(d)
        })
        .count()
}

/// Evaluates every rule of `tracker` at `now` over the tracker's
/// `entries` (any order): average above or below, flat within a
/// tolerance, or missed check-ins. Returns the breached rules.
pub fn breaches(tracker: &Tracker, entries: &[TrackerEntry], now: DateTime<Utc>, tz: Tz) -> Vec<Breach> {
    let mut out = Vec::new();
    for (index, rule) in tracker.rules.iter().enumerate() {
        let start = now - Duration::days(i64::from(rule.window_days));
        let mut window: Vec<&TrackerEntry> = entries.iter().filter(|e| e.logged_at > start && e.logged_at <= now && !e.deleted).collect();
        window.sort_by_key(|e| e.logged_at);
        let (name, unit) = label(tracker, &rule.field);
        let values: Vec<(DateTime<Utc>, f64)> = window.iter().filter_map(|e| number(e, &rule.field).map(|v| (e.logged_at, v))).collect();
        let reason = match rule.op {
            RuleOp::Above | RuleOp::Below if !values.is_empty() => {
                let avg = values.iter().map(|(_, v)| v).sum::<f64>() / values.len() as f64;
                let crossed = if rule.op == RuleOp::Above { avg > rule.value } else { avg < rule.value };
                let side = if rule.op == RuleOp::Above { "above" } else { "below" };
                crossed.then(|| {
                    format!("{}: average {name} {avg:.1}{unit} over {} days is {side} {}{unit}.", tracker.name, rule.window_days, rule.value)
                })
            }
            RuleOp::Flat if values.len() >= 2 => {
                let span = values[values.len() - 1].0 - values[0].0;
                let (min, max) = values.iter().fold((f64::MAX, f64::MIN), |(lo, hi), (_, v)| (lo.min(*v), hi.max(*v)));
                (span >= Duration::days(i64::from(rule.window_days) - 1) && max - min <= rule.value).then(|| {
                    format!("{}: {name} stayed between {min:.1} and {max:.1}{unit} for {} days.", tracker.name, rule.window_days)
                })
            }
            RuleOp::Missed => {
                let missed = missed_check_ins(tracker, &window.iter().map(|e| (*e).clone()).collect::<Vec<_>>(), rule.window_days, now, tz);
                (missed as f64 >= rule.value)
                    .then(|| format!("{}: {missed} check-ins missed in the last {} days.", tracker.name, rule.window_days))
            }
            _ => None,
        };
        if let Some(reason) = reason {
            out.push(Breach { index, reason });
        }
    }
    out
}

/// Window key of action-event replans: one per goal and UTC day.
/// Events that miss their day's window wait for the next one.
/// Returns `actions:<YYYY-MM-DD>`.
pub fn actions_window(now: DateTime<Utc>) -> String {
    format!("actions:{}", now.date_naive())
}

/// Window key of rule `index` of `tracker_id`: one per rule window of
/// `window_days`, so a rule that stays true fires once per window.
/// Returns `rule:<tracker>:<index>:<bucket>`.
pub fn rule_window(tracker_id: &str, index: usize, window_days: u32, now: DateTime<Utc>) -> String {
    assert!(window_days >= 1, "rules have a window");
    let bucket = now.timestamp().div_euclid(86_400 * i64::from(window_days));
    format!("rule:{tracker_id}:{index}:{bucket}")
}

/// Window key of the weekly review whose slot falls on local `date`.
/// One review per goal and slot.
/// Returns `review:<YYYY-MM-DD>`.
pub fn review_window(date: NaiveDate) -> String {
    format!("review:{date}")
}

/// Window key of an Away adaptation over local dates `first` to `last`:
/// one per goal and trip, so a later scan claims nothing new.
/// Returns `away:<first>:<last>`.
pub fn away_window(first: NaiveDate, last: NaiveDate) -> String {
    assert!(first <= last, "a trip has at least one day");
    format!("away:{first}:{last}")
}

/// The replan reason of an Away event titled `title` over `days` (local,
/// in order): names the dates and asks to adapt by the goal's category.
/// Returns one paragraph for the adaptation analyst.
pub fn away_reason(title: &str, days: &[NaiveDate]) -> String {
    let (first, last) = (days.first().expect("a trip has days"), days.last().expect("a trip has days"));
    let when = if first == last { first.format("%a %-d %b").to_string() } else { format!("{} to {}", first.format("%a %-d %b"), last.format("%a %-d %b")) };
    format!(
        "Away ({title}): {when}. Adapt this goal's actions on those days to its category, for example a no-equipment workout instead of the gym, practice that needs an instrument paused and moved to after the trip, habits continue."
    )
}

/// The first weekly review slot after `after`: weekday `weekday`
/// (0 = Sunday) at local `at` (`HH:MM`) in `tz`.
/// Returns `Err` for a bad weekday or time.
pub fn next_review(weekday: u8, at: &str, tz: Tz, after: DateTime<Utc>) -> Result<DateTime<Utc>, String> {
    if weekday > 6 {
        return Err(format!("weekday {weekday} is not 0 to 6"));
    }
    let time = NaiveTime::parse_from_str(at, "%H:%M").map_err(|_| format!("`{at}` is not HH:MM"))?;
    let today = after.with_timezone(&tz).date_naive();
    for d in 0..=8 {
        let date = today + Duration::days(d);
        if date.weekday().num_days_from_sunday() != u32::from(weekday) {
            continue;
        }
        if let Some(slot) = local_instant(date, time, tz).filter(|s| *s > after) {
            return Ok(slot);
        }
    }
    unreachable!("a weekday recurs within 8 days")
}

/// One pending status event of an action: kind (`missed`, `skipped`,
/// `done`), the action's title and span, and when it was done.
/// Read from `action_events.after`.
#[derive(Debug, Clone, PartialEq)]
pub struct StatusEvent {
    pub id: i64,
    pub kind: String,
    pub title: String,
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
    pub done_at: Option<DateTime<Utc>>,
}

impl StatusEvent {
    /// Whether this event asks for adaptation: missed and skipped always,
    /// done only when done more than the grace after the action ended.
    /// Returns false for any other kind.
    pub fn counts(&self) -> bool {
        match self.kind.as_str() {
            "missed" | "skipped" => true,
            "done" => self.done_at.is_some_and(|d| d > self.end_at + Duration::minutes(MISSED_GRACE_MIN)),
            _ => false,
        }
    }
}

/// The replan reason for `events` in local time of `tz`, such as
/// `Missed: Easy run (Sat 07:00). Done late: Core (Sun 18:00).`
/// Events that do not count are left out.
pub fn event_reason(events: &[StatusEvent], tz: Tz) -> String {
    let mut parts = Vec::new();
    for e in events.iter().filter(|e| e.counts()) {
        let word = match e.kind.as_str() {
            "missed" => "Missed",
            "skipped" => "Skipped",
            _ => "Done late",
        };
        parts.push(format!("{word}: {} ({}).", e.title, e.start_at.with_timezone(&tz).format("%a %H:%M")));
    }
    parts.join(" ")
}

fn time_of(v: &Value, key: &str) -> Option<DateTime<Utc>> {
    v.get(key).and_then(Value::as_str).and_then(|t| DateTime::parse_from_rfc3339(t).ok()).map(|t| t.with_timezone(&Utc))
}

fn zone_or_utc(name: &str) -> Tz {
    name.parse().unwrap_or(Tz::UTC)
}

/// Inserts `missed` events for live planned actions of active goals that
/// ended in the lookback window but more than the grace ago; only goal
/// `only` when given. Existing events are kept. Returns rows added.
pub async fn derive_missed(pool: &db::PgPool, now: DateTime<Utc>, only: Option<(Uuid, Uuid)>) -> Result<u64, ServerError> {
    let cut = now - Duration::minutes(MISSED_GRACE_MIN);
    let from = now - Duration::hours(MISSED_LOOKBACK_HOURS);
    let done = db::query(
        "insert into public.action_events (user_id, goal_id, action_id, kind, after)
         select a.user_id, a.goal_id, a.id, 'missed',
           jsonb_build_object('title', a.title, 'start_at', a.start_at, 'end_at', a.end_at, 'status', 'missed', 'done_at', null)
         from public.actions a join public.goals g on g.user_id = a.user_id and g.id = a.goal_id
         where not a.deleted and not g.deleted and g.status = 'active' and a.status = 'planned'
           and a.end_at < $1 and a.end_at >= $2
           and ($3::uuid is null or (a.user_id = $3 and a.goal_id = $4))
         on conflict (user_id, action_id, kind) where kind in ('missed', 'skipped') do nothing",
    )
    .bind(cut)
    .bind(from)
    .bind(only.map(|o| o.0))
    .bind(only.map(|o| o.1))
    .execute(pool)
    .await?;
    Ok(done.rows_affected())
}

/// Marks every pending status event of `user`'s goal `goal` handled by
/// `run`, after deriving the goal's missed events, so a replan covers
/// what was past due when it started. Returns the events claimed.
pub async fn claim_status_events(pool: &db::PgPool, user: Uuid, goal: Uuid, run: RunId, now: DateTime<Utc>) -> Result<u64, ServerError> {
    derive_missed(pool, now, Some((user, goal))).await?;
    let done = db::query(
        "update public.action_events set handled = true, handled_by = $3
         where user_id = $1 and goal_id = $2 and not handled and kind in ('missed', 'skipped', 'done')",
    )
    .bind(user)
    .bind(goal)
    .bind(run.0)
    .execute(pool)
    .await?;
    Ok(done.rows_affected())
}

/// The owner's timezone for `goal`, from its review schedule, else UTC.
/// Server-started runs have no request zone, so this is the source.
/// Returns the zone.
pub async fn goal_zone(pool: &db::PgPool, user: Uuid, goal: Uuid) -> Result<Tz, ServerError> {
    let name: Option<String> = db::query_scalar("select timezone from public.review_schedules where user_id = $1 and goal_id = $2")
        .bind(user)
        .bind(goal)
        .fetch_optional(pool)
        .await?;
    Ok(name.as_deref().map(zone_or_utc).unwrap_or(Tz::UTC))
}

async fn busy(pool: &db::PgPool, user: Uuid, goal: Uuid) -> Result<bool, ServerError> {
    Ok(db::query_scalar(&format!(
        "select exists(select 1 from public.runs where user_id = $1 and goal_id = $2 and workflow = 'replan' and state in {BUSY_STATES})"
    ))
    .bind(user)
    .bind(goal)
    .fetch_one(pool)
    .await?)
}

async fn active_goal(pool: &db::PgPool, user: Uuid, goal: Uuid) -> Result<Option<Goal>, ServerError> {
    Ok(get_row::<Goal>(pool, user, &goal.to_string()).await?.filter(|g| g.status == GoalStatus::Active))
}

/// Inserts the `adaptations` row of a trigger unless its window was used.
/// The unique `(user_id, goal_id, window_key)` makes concurrent scans
/// start one run. Returns the new row id, or `None` when deduped.
pub async fn claim_window(
    pool: &db::PgPool,
    user: Uuid,
    goal: Option<Uuid>,
    source: Source,
    window: &str,
    reason: &str,
    now: DateTime<Utc>,
) -> Result<Option<Uuid>, ServerError> {
    let id = Uuid::now_v7();
    let inserted: Option<Uuid> = db::query_scalar(
        "insert into public.adaptations (id, user_id, goal_id, source, window_key, reason, created_at)
         values ($1, $2, $3, $4, $5, $6, $7) on conflict (user_id, goal_id, window_key) do nothing returning id",
    )
    .bind(id)
    .bind(user)
    .bind(goal)
    .bind(source.as_str())
    .bind(window)
    .bind(reason)
    .bind(now)
    .fetch_optional(pool)
    .await?;
    Ok(inserted)
}

async fn settle(pool: &db::PgPool, adaptation: Uuid, started: &Result<RunId, ServerError>) -> Result<(), ServerError> {
    let (run, error) = match started {
        Ok(r) => (Some(r.0), None),
        Err(e) => (None, Some(e.to_string())),
    };
    db::query("update public.adaptations set run_id = $2, error = $3 where id = $1")
        .bind(adaptation)
        .bind(run)
        .bind(error)
        .execute(pool)
        .await?;
    Ok(())
}

impl Engine {
    /// Runs one trigger scan when `SCAN_EVERY_MS` passed since this
    /// process's last one: action events, tracker rules, due reviews and
    /// repeated edits. Returns how many runs it started.
    pub async fn scan_if_due(&self) -> Result<usize, ServerError> {
        let now_ms = self.deps().clock.now_ms();
        if !self.claim_scan(now_ms, SCAN_EVERY_MS) {
            return Ok(0);
        }
        self.scan(utc(now_ms)).await
    }

    /// One trigger scan at `now`. Per-goal start failures are stored on
    /// the adaptation row and never fail the scan.
    /// Returns how many runs it started.
    pub async fn scan(&self, now: DateTime<Utc>) -> Result<usize, ServerError> {
        let pool = self.deps().pool.clone();
        self.stop_deleted_goals(&pool).await?;
        derive_missed(&pool, now, None).await?;
        let mut started = 0;
        started += self.scan_events(&pool, now).await?;
        started += self.scan_rules(&pool, now).await?;
        started += self.scan_reviews(&pool, now).await?;
        started += self.scan_edits(&pool, now).await?;
        started += self.scan_away(&pool, now).await?;
        Ok(started)
    }

    /// Stops what the server still does for goals the owner deleted (Q68), whichever device
    /// sent the tombstone: cancels their unfinished runs, turns off their weekly reviews and
    /// marks their pending action events handled. Returns how many runs it cancelled.
    pub async fn stop_deleted_goals(&self, pool: &db::PgPool) -> Result<usize, ServerError> {
        let runs: Vec<(Uuid, Uuid)> = db::query_as(
            "select r.user_id, r.id from public.runs r join public.goals g on g.user_id = r.user_id and g.id = r.goal_id
             where g.deleted and r.state not in ('done', 'failed') order by r.id",
        )
        .fetch_all(pool)
        .await?;
        for (user, run) in &runs {
            self.cancel(*user, *run).await?;
        }
        db::query(
            "update public.review_schedules s set enabled = false from public.goals g
             where g.user_id = s.user_id and g.id = s.goal_id and g.deleted and s.enabled",
        )
        .execute(pool)
        .await?;
        db::query(
            "update public.action_events e set handled = true from public.goals g
             where g.user_id = e.user_id and g.id = e.goal_id and g.deleted and not e.handled",
        )
        .execute(pool)
        .await?;
        Ok(runs.len())
    }

    async fn trigger_replan(&self, user: Uuid, goal: &Goal, source: Source, window: &str, reason: &str, now: DateTime<Utc>) -> Result<bool, ServerError> {
        let pool = &self.deps().pool;
        let goal_id = Uuid::parse_str(&goal.id).map_err(|_| ServerError::Db("bad goal id".into()))?;
        let Some(adaptation) = claim_window(pool, user, Some(goal_id), source, window, reason, now).await? else {
            return Ok(false);
        };
        let tz = goal_zone(pool, user, goal_id).await?;
        let asked = Goal { autonomy: if source == Source::Calendar { Autonomy::Ask } else { goal.autonomy }, ..goal.clone() };
        let result = self.replan(user, &asked, reason, tz).await;
        settle(pool, adaptation, &result).await?;
        Ok(result.is_ok())
    }

    async fn scan_away(&self, pool: &db::PgPool, now: DateTime<Utc>) -> Result<usize, ServerError> {
        let until = now + Duration::days(i64::from(DEFAULT_WINDOW_DAYS));
        let trips: Vec<(Uuid, String, DateTime<Utc>, DateTime<Utc>)> = db::query_as(
            "select user_id, title, start_at, end_at from public.busy_blocks
             where not deleted and all_day and day_kind = 'away' and end_at > $1 and start_at < $2 order by user_id, start_at, id",
        )
        .bind(now)
        .bind(until)
        .fetch_all(pool)
        .await?;
        let mut started = 0;
        for (user, title, start, end) in trips {
            let goals: Vec<Uuid> = db::query_scalar(
                "select id from public.goals where user_id = $1 and not deleted and status = 'active' order by id",
            )
            .bind(user)
            .fetch_all(pool)
            .await?;
            for goal_id in goals {
                let tz = goal_zone(pool, user, goal_id).await?;
                let today = now.with_timezone(&tz).date_naive();
                let days: Vec<NaiveDate> = local_days(start, end, tz).into_iter().filter(|d| *d >= today).collect();
                let Some((first, last)) = days.first().zip(days.last()) else { continue };
                let (from, to) = (local_instant(*first, NaiveTime::MIN, tz), local_instant(*last + Duration::days(1), NaiveTime::MIN, tz));
                let (Some(from), Some(to)) = (from, to) else { continue };
                let planned: i64 = db::query_scalar(
                    "select count(*) from public.actions where user_id = $1 and goal_id = $2 and not deleted and status = 'planned'
                     and start_at >= $3 and start_at < $4",
                )
                .bind(user)
                .bind(goal_id)
                .bind(from.max(now))
                .bind(to)
                .fetch_one(pool)
                .await?;
                if planned == 0 || busy(pool, user, goal_id).await? {
                    continue;
                }
                let Some(goal) = active_goal(pool, user, goal_id).await? else { continue };
                let reason = away_reason(&title, &days);
                if self.trigger_replan(user, &goal, Source::Calendar, &away_window(*first, *last), &reason, now).await? {
                    started += 1;
                }
            }
        }
        Ok(started)
    }

    async fn scan_events(&self, pool: &db::PgPool, now: DateTime<Utc>) -> Result<usize, ServerError> {
        let rows: Vec<(i64, Uuid, Uuid, String, Value, Option<String>, Option<bool>)> = db::query_as(
            "select e.id, e.user_id, e.goal_id, e.kind, e.after, a.status, a.deleted
             from public.action_events e left join public.actions a on a.user_id = e.user_id and a.id = e.action_id
             where not e.handled and e.kind in ('missed', 'skipped', 'done') order by e.id",
        )
        .fetch_all(pool)
        .await?;
        let mut per_goal: BTreeMap<(Uuid, Uuid), Vec<StatusEvent>> = BTreeMap::new();
        let mut stale = Vec::new();
        for (id, user, goal, kind, after, status, deleted) in rows {
            let event = StatusEvent {
                id,
                title: after.get("title").and_then(Value::as_str).unwrap_or("Action").to_string(),
                start_at: time_of(&after, "start_at").unwrap_or(now),
                end_at: time_of(&after, "end_at").unwrap_or(now),
                done_at: time_of(&after, "done_at"),
                kind: kind.clone(),
            };
            let current = match kind.as_str() {
                "missed" => matches!(status.as_deref(), Some("planned" | "missed")),
                other => status.as_deref() == Some(other),
            };
            if !event.counts() || !current || deleted != Some(false) {
                stale.push(id);
            } else {
                per_goal.entry((user, goal)).or_default().push(event);
            }
        }
        let mut started = 0;
        for ((user, goal_id), events) in per_goal {
            let Some(goal) = active_goal(pool, user, goal_id).await? else {
                stale.extend(events.iter().map(|e| e.id));
                continue;
            };
            if busy(pool, user, goal_id).await? {
                continue;
            }
            let tz = goal_zone(pool, user, goal_id).await?;
            let reason = event_reason(&events, tz);
            assert!(!reason.is_empty(), "only counting events reach a reason");
            if self.trigger_replan(user, &goal, Source::Actions, &actions_window(now), &reason, now).await? {
                started += 1;
            }
        }
        db::query("update public.action_events set handled = true where id = any($1)").bind(&stale).execute(pool).await?;
        Ok(started)
    }

    async fn scan_rules(&self, pool: &db::PgPool, now: DateTime<Utc>) -> Result<usize, ServerError> {
        let trackers: Vec<Value> = db::query_scalar(
            "select to_jsonb(t) - 'server_seq' from public.trackers t join public.goals g on g.user_id = t.user_id and g.id = t.goal_id
             where not t.deleted and not g.deleted and g.status = 'active' and jsonb_array_length(t.rules) > 0 order by t.user_id, t.id",
        )
        .fetch_all(pool)
        .await?;
        let mut started = 0;
        for value in trackers {
            let tracker: Tracker = crate::rows::parse_row(value)?;
            let user = Uuid::parse_str(&tracker.user_id).map_err(|_| ServerError::Db("bad user id".into()))?;
            let goal_id = Uuid::parse_str(&tracker.goal_id).map_err(|_| ServerError::Db("bad goal id".into()))?;
            let Some(goal) = active_goal(pool, user, goal_id).await? else { continue };
            let entries = list_rows::<TrackerEntry>(pool, user, &[("tracker_id", tracker.id.clone())], false).await?;
            let tz = goal_zone(pool, user, goal_id).await?;
            for breach in breaches(&tracker, &entries, now, tz) {
                if busy(pool, user, goal_id).await? {
                    break;
                }
                let window = rule_window(&tracker.id, breach.index, tracker.rules[breach.index].window_days, now);
                if self.trigger_replan(user, &goal, Source::Rule, &window, &breach.reason, now).await? {
                    started += 1;
                }
            }
        }
        Ok(started)
    }

    async fn scan_reviews(&self, pool: &db::PgPool, now: DateTime<Utc>) -> Result<usize, ServerError> {
        let due: Vec<(Uuid, Uuid, i16, String, String, DateTime<Utc>)> = db::query_as(
            "select user_id, goal_id, weekday, at, timezone, next_at from public.review_schedules
             where enabled and next_at <= $1 order by next_at",
        )
        .bind(now)
        .fetch_all(pool)
        .await?;
        let mut started = 0;
        for (user, goal_id, weekday, at, zone, slot) in due {
            let tz = zone_or_utc(&zone);
            let next = next_review(u8::try_from(weekday).unwrap_or(0), &at, tz, now).map_err(ServerError::Db)?;
            if let Some(goal) = active_goal(pool, user, goal_id).await? {
                if busy(pool, user, goal_id).await? {
                    continue;
                }
                let reason = self.review_reason(pool, user, goal_id, now).await?;
                let window = review_window(slot.with_timezone(&tz).date_naive());
                if self.trigger_replan(user, &goal, Source::Review, &window, &reason, now).await? {
                    started += 1;
                }
            }
            db::query("update public.review_schedules set next_at = $3 where user_id = $1 and goal_id = $2")
                .bind(user)
                .bind(goal_id)
                .bind(next)
                .execute(pool)
                .await?;
        }
        Ok(started)
    }

    async fn review_reason(&self, pool: &db::PgPool, user: Uuid, goal: Uuid, now: DateTime<Utc>) -> Result<String, ServerError> {
        let cut = now - Duration::minutes(MISSED_GRACE_MIN);
        let (done, missed, skipped, ahead): (i64, i64, i64, i64) = db::query_as(
            "select count(*) filter (where status = 'done' and end_at >= $3),
                    count(*) filter (where (status = 'missed' or (status = 'planned' and end_at < $4)) and end_at >= $3),
                    count(*) filter (where status = 'skipped' and end_at >= $3),
                    count(*) filter (where status = 'planned' and start_at >= $5 and start_at < $5 + interval '7 days')
             from public.actions where user_id = $1 and goal_id = $2 and not deleted",
        )
        .bind(user)
        .bind(goal)
        .bind(now - Duration::days(7))
        .bind(cut)
        .bind(now)
        .fetch_one(pool)
        .await?;
        Ok(format!(
            "Weekly review of the last 7 days: {done} done, {missed} missed, {skipped} skipped; {ahead} planned for the next 7 days. Keep the plan unless it needs a change."
        ))
    }

    async fn scan_edits(&self, pool: &db::PgPool, now: DateTime<Utc>) -> Result<usize, ServerError> {
        let rows: Vec<(i64, Uuid, Uuid, String, Option<Value>, Value)> = db::query_as(
            "select id, user_id, goal_id, kind, before, after from public.action_events
             where not handled and kind in ('move', 'resize', 'delete') order by user_id, id",
        )
        .fetch_all(pool)
        .await?;
        let mut zones: BTreeMap<(Uuid, Uuid), Tz> = BTreeMap::new();
        let mut per_user: BTreeMap<Uuid, Vec<EditEvent>> = BTreeMap::new();
        for (id, user, goal, kind, before, after) in rows {
            let tz = match zones.get(&(user, goal)) {
                Some(tz) => *tz,
                None => {
                    let tz = goal_zone(pool, user, goal).await?;
                    zones.insert((user, goal), tz);
                    tz
                }
            };
            if let Some(edit) = EditEvent::from_row(id, &kind, before.as_ref(), &after, tz) {
                per_user.entry(user).or_default().push(edit);
            }
        }
        let mut started = 0;
        for (user, edits) in per_user {
            for group in learning::similar_groups(&edits) {
                let ids: Vec<i64> = group.edits.iter().map(|e| e.id).collect();
                if learning::preference_known(pool, user, &group.key).await? {
                    db::query("update public.action_events set handled = true where id = any($1)").bind(&ids).execute(pool).await?;
                    continue;
                }
                let window = format!("edits:{}:{}", group.key, now.date_naive());
                let reason = learning::reason(&group);
                let Some(adaptation) = claim_window(pool, user, None, Source::Edits, &window, &reason, now).await? else { continue };
                let result = self.learn(user, &group).await;
                settle(pool, adaptation, &result).await?;
                let by = result.as_ref().ok().map(|r| r.0);
                db::query("update public.action_events set handled = true, handled_by = $2 where id = any($1)")
                    .bind(&ids)
                    .bind(by)
                    .execute(pool)
                    .await?;
                if result.is_ok() {
                    started += 1;
                }
            }
        }
        Ok(started)
    }
}

/// A goal's weekly review: weekday (0 = Sunday), local `at` (`HH:MM`),
/// the owner's zone, whether it runs, and the next slot in UTC.
/// Answer of `GET` and `PUT /v1/goals/{goal_id}/review`.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct ReviewSchedule {
    pub goal_id: Uuid,
    pub weekday: u8,
    pub at: String,
    pub timezone: String,
    pub enabled: bool,
    pub next_at: DateTime<Utc>,
}

/// Body of `PUT /v1/goals/{goal_id}/review`: weekday 0 to 6 (0 = Sunday),
/// local `HH:MM`, on or off, and the owner's IANA zone.
/// The next slot is computed from these.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SetReview {
    pub weekday: u8,
    pub at: String,
    pub enabled: bool,
    pub timezone: String,
}

/// Body of `PUT /v1/timezone`: the IANA zone the owner's device moved to.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SetTimezone {
    pub timezone: String,
}

/// Answer of `PUT /v1/timezone`: how many weekly reviews now run in the zone.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TimezoneSet {
    pub reviews: usize,
}

const DEFAULT_REVIEW_WEEKDAY: u8 = 0;
const DEFAULT_REVIEW_AT: &str = "18:00";

impl Engine {
    /// Creates `user`'s weekly review of `goal` (Sunday 18:00 in `tz`) when
    /// it has none, else moves it to zone `tz` when that changed.
    /// Every owner request with a zone calls it.
    pub(crate) async fn ensure_review(&self, user: Uuid, goal: Uuid, tz: Tz) -> Result<(), ServerError> {
        let now = utc(self.deps().clock.now_ms());
        let pool = &self.deps().pool;
        let current: Option<(i16, String, String)> =
            db::query_as("select weekday, at, timezone from public.review_schedules where user_id = $1 and goal_id = $2")
                .bind(user)
                .bind(goal)
                .fetch_optional(pool)
                .await?;
        let (weekday, at) = match &current {
            Some((_, _, zone)) if zone == tz.name() => return Ok(()),
            Some((w, a, _)) => (u8::try_from(*w).expect("weekday is checked by the table"), a.clone()),
            None => (DEFAULT_REVIEW_WEEKDAY, DEFAULT_REVIEW_AT.to_string()),
        };
        let next = next_review(weekday, &at, tz, now).map_err(ServerError::Db)?;
        db::query(
            "insert into public.review_schedules (user_id, goal_id, weekday, at, timezone, enabled, next_at)
             values ($1, $2, $3, $4, $5, true, $6)
             on conflict (user_id, goal_id) do update set timezone = excluded.timezone, next_at = excluded.next_at",
        )
        .bind(user)
        .bind(goal)
        .bind(i16::from(weekday))
        .bind(&at)
        .bind(tz.name())
        .bind(next)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Moves every weekly review of `user` to the IANA zone `timezone` (the owner
    /// travelled), keeping each weekday and local time, through [`Self::ensure_review`].
    /// Returns how many reviews the user has, or `BadRequest` for an unknown zone.
    pub async fn set_timezone(&self, user: Uuid, timezone: &str) -> Result<usize, ServerError> {
        let tz = crate::engine::parse_tz(timezone)?;
        let goals: Vec<Uuid> = db::query_scalar("select goal_id from public.review_schedules where user_id = $1 order by goal_id")
            .bind(user)
            .fetch_all(&self.deps().pool)
            .await?;
        for goal in &goals {
            self.ensure_review(user, *goal, tz).await?;
        }
        Ok(goals.len())
    }

    /// Reads `user`'s weekly review of `goal`.
    /// Returns `NotFound` when the goal has none or is another user's.
    /// Reviews are created with the goal's first run.
    pub async fn review(&self, user: Uuid, goal: Uuid) -> Result<ReviewSchedule, ServerError> {
        let row: Option<(i16, String, String, bool, DateTime<Utc>)> = db::query_as(
            "select weekday, at, timezone, enabled, next_at from public.review_schedules where user_id = $1 and goal_id = $2",
        )
        .bind(user)
        .bind(goal)
        .fetch_optional(&self.deps().pool)
        .await?;
        let (weekday, at, timezone, enabled, next_at) = row.ok_or_else(|| ServerError::NotFound(format!("review of goal {goal}")))?;
        Ok(ReviewSchedule { goal_id: goal, weekday: u8::try_from(weekday).expect("weekday is checked by the table"), at, timezone, enabled, next_at })
    }

    /// Sets `user`'s weekly review of `goal` from `req` and computes its
    /// next slot. Returns `BadRequest` for a bad weekday, time or zone and
    /// `NotFound` for a goal the user does not have.
    pub async fn set_review(&self, user: Uuid, goal: Uuid, req: SetReview) -> Result<ReviewSchedule, ServerError> {
        let tz: Tz = req.timezone.parse().map_err(|_| ServerError::BadRequest(format!("unknown timezone `{}`", req.timezone)))?;
        let now = utc(self.deps().clock.now_ms());
        let next = next_review(req.weekday, &req.at, tz, now).map_err(ServerError::BadRequest)?;
        let pool = &self.deps().pool;
        get_row::<Goal>(pool, user, &goal.to_string()).await?.ok_or_else(|| ServerError::NotFound(format!("goal {goal}")))?;
        db::query(
            "insert into public.review_schedules (user_id, goal_id, weekday, at, timezone, enabled, next_at)
             values ($1, $2, $3, $4, $5, $6, $7)
             on conflict (user_id, goal_id) do update set weekday = excluded.weekday, at = excluded.at,
               timezone = excluded.timezone, enabled = excluded.enabled, next_at = excluded.next_at",
        )
        .bind(user)
        .bind(goal)
        .bind(i16::from(req.weekday))
        .bind(&req.at)
        .bind(tz.name())
        .bind(req.enabled)
        .bind(next)
        .execute(pool)
        .await?;
        self.review(user, goal).await
    }

    /// Starts the learning run of `user`'s similar edits `group`; the
    /// preference analyst proposes one preference for the Inbox.
    /// Returns the run, or `Setup` when no provider key is usable.
    pub(crate) async fn learn(&self, user: Uuid, group: &learning::EditGroup) -> Result<RunId, ServerError> {
        let (edits, pattern) = learning::run_input(group);
        let mut input = serde_json::Map::new();
        input.insert("edits".into(), edits);
        input.insert("pattern".into(), pattern);
        self.start(user, None, LEARN_PREFERENCE, input).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aroh_domain::rows::{Cadence, EntrySource, FieldKind, TrackerRule};
    use serde_json::json;

    const BERLIN: Tz = chrono_tz::Europe::Berlin;

    fn at(text: &str) -> DateTime<Utc> {
        text.parse().unwrap()
    }

    fn tracker(rules: Vec<TrackerRule>, cadence: Option<Cadence>) -> Tracker {
        Tracker {
            id: "0190f2a0-0000-7000-8000-000000000007".into(),
            user_id: "0190f2a0-0000-7000-8000-00000000000a".into(),
            goal_id: "0190f2a0-0000-7000-8000-000000000091".into(),
            plugin_id: None,
            name: "Body".into(),
            fields: vec![
                TrackerField { key: "weight".into(), label: "Weight".into(), kind: FieldKind::Number, unit: Some("kg".into()), choices: vec![], health: None },
            ],
            cadence,
            rules,
            updated_at: "1790000000000-0000-00000000000000d1".into(),
            deleted: false,
        }
    }

    fn entry(when: &str, weight: f64) -> TrackerEntry {
        TrackerEntry {
            id: Uuid::now_v7().to_string(),
            user_id: "0190f2a0-0000-7000-8000-00000000000a".into(),
            tracker_id: "0190f2a0-0000-7000-8000-000000000007".into(),
            data: json!({"weight": weight}).as_object().unwrap().clone(),
            source: EntrySource::CheckIn,
            raw_text: None,
            logged_at: at(when),
            updated_at: "1795000000000-0000-00000000000000d1".into(),
            deleted: false,
        }
    }

    fn rule(op: RuleOp, value: f64, window_days: u32) -> TrackerRule {
        TrackerRule { field: "weight".into(), op, value, window_days }
    }

    #[test]
    fn above_below_flat_missed_rules_evaluate() {
        let now = at("2026-11-22T12:00:00Z");
        let daily: Vec<TrackerEntry> = (0..14).map(|d| entry(&format!("2026-11-{:02}T19:00:00Z", 8 + d), 82.0 + (d % 2) as f64 * 0.2)).collect();
        let t = tracker(vec![rule(RuleOp::Above, 81.0, 7), rule(RuleOp::Below, 81.0, 7), rule(RuleOp::Flat, 0.5, 14), rule(RuleOp::Flat, 0.1, 14)], None);
        let hits = breaches(&t, &daily, now, BERLIN);
        assert_eq!(hits.iter().map(|b| b.index).collect::<Vec<_>>(), [0, 2]);
        assert_eq!(hits[1].reason, "Body: Weight stayed between 82.0 and 82.2 kg for 14 days.");
        let short: Vec<TrackerEntry> = daily[10..].to_vec();
        assert!(breaches(&tracker(vec![rule(RuleOp::Flat, 0.5, 14)], None), &short, now, BERLIN).is_empty());
        assert!(breaches(&tracker(vec![rule(RuleOp::Above, 81.0, 7)], None), &[], now, BERLIN).is_empty());

        let cadence = Some(Cadence { freq: CadenceFreq::Daily, weekdays: vec![], at: "20:00".into() });
        let t = tracker(vec![rule(RuleOp::Missed, 3.0, 7)], cadence);
        let some = vec![entry("2026-11-16T19:00:00Z", 82.0), entry("2026-11-18T19:00:00Z", 82.0), entry("2026-11-20T19:00:00Z", 82.0)];
        let hits = breaches(&t, &some, now, BERLIN);
        assert_eq!(hits.len(), 1, "15, 17, 19 and 21 have no check-in");
        assert_eq!(hits[0].reason, "Body: 4 check-ins missed in the last 7 days.");
        let mut newer = t.clone();
        newer.updated_at = format!("{}-0000-00000000000000d1", at("2026-11-20T12:00:00Z").timestamp_millis());
        assert!(breaches(&newer, &some, now, BERLIN).is_empty(), "only 21 is missed since the tracker exists");
    }

    #[test]
    fn next_review_slot_across_dst() {
        let sunday_noon = at("2026-11-22T12:00:00Z");
        assert_eq!(next_review(0, "18:00", BERLIN, sunday_noon).unwrap(), at("2026-11-22T17:00:00Z"));
        assert_eq!(next_review(0, "18:00", BERLIN, at("2026-11-22T17:00:00Z")).unwrap(), at("2026-11-29T17:00:00Z"));
        assert_eq!(next_review(1, "09:30", BERLIN, sunday_noon).unwrap(), at("2026-11-23T08:30:00Z"));
        assert_eq!(next_review(0, "02:30", BERLIN, at("2027-03-27T12:00:00Z")).unwrap(), at("2027-03-28T01:00:00Z"));
        assert_eq!(next_review(0, "18:00", BERLIN, at("2027-03-27T12:00:00Z")).unwrap(), at("2027-03-28T16:00:00Z"));
        assert!(next_review(7, "18:00", BERLIN, sunday_noon).is_err());
        assert!(next_review(0, "25:00", BERLIN, sunday_noon).is_err());
    }

    #[test]
    fn window_keys_dedupe_per_goal_and_window() {
        let a = at("2026-11-22T01:00:00Z");
        let b = at("2026-11-22T23:00:00Z");
        assert_eq!(actions_window(a), actions_window(b));
        assert_ne!(actions_window(a), actions_window(b + Duration::hours(2)));
        assert_eq!(rule_window("t", 0, 14, a), rule_window("t", 0, 14, a + Duration::days(1)));
        assert_ne!(rule_window("t", 0, 14, a), rule_window("t", 0, 14, a + Duration::days(14)));
        assert_ne!(rule_window("t", 0, 14, a), rule_window("t", 1, 14, a));
        assert_eq!(review_window(NaiveDate::from_ymd_opt(2026, 11, 22).unwrap()), "review:2026-11-22");
    }

    fn status(kind: &str, end: &str, done: Option<&str>) -> StatusEvent {
        let end_at = at(end);
        StatusEvent { id: 1, kind: kind.into(), title: "Easy run".into(), start_at: end_at - Duration::minutes(30), end_at, done_at: done.map(at) }
    }

    #[test]
    fn away_window_and_reason_name_the_dates() {
        let d = |day: u32| NaiveDate::from_ymd_opt(2026, 11, day).unwrap();
        assert_eq!(away_window(d(27), d(29)), "away:2026-11-27:2026-11-29");
        let reason = away_reason("Vacation", &[d(27), d(28), d(29)]);
        assert!(reason.starts_with("Away (Vacation): Fri 27 Nov to Sun 29 Nov. Adapt"), "{reason}");
        assert!(reason.contains("no-equipment workout") && reason.contains("habits continue"));
        assert!(away_reason("Trip", &[d(28)]).starts_with("Away (Trip): Sat 28 Nov. "));
        assert_eq!(Source::Calendar.as_str(), "calendar");
    }

    #[test]
    fn done_late_only_after_grace() {
        assert!(status("missed", "2026-11-21T07:30:00Z", None).counts());
        assert!(status("skipped", "2026-11-21T07:30:00Z", None).counts());
        assert!(!status("done", "2026-11-21T07:30:00Z", Some("2026-11-21T07:40:00Z")).counts());
        assert!(!status("done", "2026-11-21T07:30:00Z", Some("2026-11-21T07:45:00Z")).counts());
        assert!(status("done", "2026-11-21T07:30:00Z", Some("2026-11-21T07:46:00Z")).counts());
        assert!(!status("move", "2026-11-21T07:30:00Z", None).counts());
    }

    #[test]
    fn event_reason_names_actions_in_local_time() {
        let events = vec![
            status("missed", "2026-11-21T06:30:00Z", None),
            status("done", "2026-11-21T08:30:00Z", Some("2026-11-21T08:35:00Z")),
            status("done", "2026-11-22T17:30:00Z", Some("2026-11-22T19:00:00Z")),
        ];
        assert_eq!(event_reason(&events, BERLIN), "Missed: Easy run (Sat 07:00). Done late: Easy run (Sun 18:00).");
    }
}
