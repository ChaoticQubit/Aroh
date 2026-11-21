use crate::model::{Action, ActionStatus, DiffOp, Goal, Infeasible, Op, PlanInput, Proposal, Slot};
use crate::time::{day_windows, deadline, inside_window, local_date, Span};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use std::collections::{BTreeMap, BTreeSet};

/// Upper bound on days scanned per goal, so a far target date stays cheap.
const DAYS_MAX: i64 = 3660;

/// One action placed during a day layout: an index into the working
/// actions, or None for the catch-up action the layout creates.
struct Item {
    index: Option<usize>,
    lower: DateTime<Utc>,
    minutes: i64,
    est_minutes: i64,
    deadline: DateTime<Utc>,
}

/// Shared read-only state of one plan() run.
struct Ctx<'a> {
    input: &'a PlanInput,
    goals: BTreeMap<&'a str, &'a Goal>,
    missed: BTreeSet<&'a str>,
}

impl Ctx<'_> {
    /// Deadline of a live goal, or None when the goal is unknown.
    fn goal_deadline(&self, goal_id: &str) -> Option<DateTime<Utc>> {
        self.goals.get(goal_id).map(|g| deadline(g, &self.input.availability))
    }

    /// True when the action blocks time: planned, live and not a missed one.
    fn occupies(&self, action: &Action) -> bool {
        action.status == ActionStatus::Planned && !action.deleted && !self.missed.contains(action.id.as_str())
    }
}

/// Spreads each goal's missed minutes over its remaining days and returns
/// the diff plus minutes that fit nowhere. Inputs are read only; output is
/// deterministic and always passes validate().
pub fn plan(input: &PlanInput) -> Proposal {
    let ctx = Ctx {
        input,
        goals: input.goals.iter().filter(|g| !g.deleted).map(|g| (g.id.as_str(), g)).collect(),
        missed: input.missed_ids.iter().map(String::as_str).collect(),
    };
    let mut need: BTreeMap<&str, i64> = BTreeMap::new();
    for action in &input.actions {
        if ctx.missed.contains(action.id.as_str()) && !action.deleted && ctx.goals.contains_key(action.goal_id.as_str()) {
            *need.entry(action.goal_id.as_str()).or_default() += action.est_minutes.max(0);
        }
    }
    let mut order: Vec<&Goal> = need.keys().map(|id| ctx.goals[id]).collect();
    order.sort_by_key(|g| (deadline(g, &input.availability), g.id.clone()));

    let mut current = input.actions.clone();
    let mut infeasible = Vec::new();
    for goal in order {
        let minutes = need[goal.id.as_str()];
        let placed = place_goal(&ctx, &mut current, goal, minutes);
        assert!(placed <= minutes);
        if placed < minutes {
            infeasible.push(Infeasible { goal_id: goal.id.clone(), unplaced_minutes: minutes - placed });
        }
    }
    let proposal = Proposal { diff: diff_between(&input.actions, &current), infeasible };
    debug_assert_eq!(crate::validate(input, &proposal.diff), Ok(()));
    proposal
}

/// Places up to `minutes` of a goal's work into `current`; returns minutes placed.
fn place_goal(ctx: &Ctx, current: &mut Vec<Action>, goal: &Goal, minutes: i64) -> i64 {
    let availability = &ctx.input.availability;
    let first = local_date(ctx.input.now, availability);
    let days: Vec<NaiveDate> = (0..DAYS_MAX)
        .map(|n| first + Duration::days(n))
        .take_while(|d| *d <= goal.target_date)
        .collect();
    let caps: Vec<i64> = days.iter().map(|d| capacity(ctx, current, *d, goal, minutes)).collect();
    let alloc = water_fill(minutes, &caps);
    for (day, extra) in days.iter().zip(&alloc) {
        if *extra > 0 {
            let layout = layout(ctx, current, *day, goal, *extra).expect("allocation within capacity");
            commit(current, layout, goal);
        }
    }
    alloc.iter().sum()
}

/// Largest extra minutes (at most `limit`) the goal can take on `day`.
fn capacity(ctx: &Ctx, current: &[Action], day: NaiveDate, goal: &Goal, limit: i64) -> i64 {
    if limit <= 0 || layout(ctx, current, day, goal, 0).is_none() {
        return 0;
    }
    let (mut lo, mut hi) = (0, limit);
    while lo < hi {
        let mid = lo + (hi - lo + 1) / 2;
        if layout(ctx, current, day, goal, mid).is_some() {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    lo
}

/// Splits `total` as evenly as the per-day caps allow; earlier days take
/// the remainder. The result never exceeds a cap or the total.
fn water_fill(total: i64, caps: &[i64]) -> Vec<i64> {
    let mut alloc = vec![0; caps.len()];
    let mut open: Vec<usize> = (0..caps.len()).filter(|i| caps[*i] > 0).collect();
    let mut remaining = total;
    while remaining > 0 && !open.is_empty() {
        let count = open.len() as i64;
        let share = |j: usize| remaining / count + i64::from((j as i64) < remaining % count);
        let full: Vec<usize> = open.iter().enumerate().filter(|(j, i)| share(*j) >= caps[**i]).map(|(_, i)| *i).collect();
        if full.is_empty() {
            for (j, i) in open.iter().enumerate() {
                alloc[*i] = share(j);
            }
            remaining = 0;
        } else {
            for i in &full {
                alloc[*i] = caps[*i];
                remaining -= caps[*i];
            }
            open.retain(|i| !full.contains(i));
        }
    }
    assert!(alloc.iter().sum::<i64>() <= total);
    alloc
}

/// Lays out `day` with the goal grown by `extra` minutes. Returns new
/// slots for every item, or None when something does not fit.
fn layout(ctx: &Ctx, current: &[Action], day: NaiveDate, goal: &Goal, extra: i64) -> Option<Vec<(Option<usize>, Slot)>> {
    let availability = &ctx.input.availability;
    let now = ctx.input.now;
    let windows: Vec<Span> = day_windows(day, availability)
        .into_iter()
        .filter(|w| w.1 > now)
        .map(|w| (w.0.max(now), w.1))
        .collect();
    let mut items = Vec::new();
    let mut occupied: Vec<Span> = Vec::new();
    for (index, action) in current.iter().enumerate() {
        if !ctx.occupies(action) {
            continue;
        }
        let item_deadline = ctx.goal_deadline(&action.goal_id);
        let movable = action.start_at >= now
            && local_date(action.start_at, availability) == day
            && inside_window(action.start_at, action.end_at, availability)
            && item_deadline.is_some();
        if movable {
            items.push(Item {
                index: Some(index),
                lower: action.start_at,
                minutes: (action.end_at - action.start_at).num_minutes(),
                est_minutes: action.est_minutes,
                deadline: item_deadline.expect("checked above"),
            });
        } else {
            occupied.push((action.start_at, action.end_at));
        }
    }
    items.sort_by(|a, b| {
        let key = |i: &Item| (i.lower, i.index.map(|x| current[x].id.clone()));
        key(a).cmp(&key(b))
    });
    if extra > 0 {
        let own = items.iter_mut().find(|i| current[i.index.expect("existing")].goal_id == goal.id);
        match own {
            Some(item) => {
                item.minutes += extra;
                item.est_minutes += extra;
            }
            None => items.push(Item {
                index: None,
                lower: windows.first()?.0,
                minutes: extra,
                est_minutes: extra,
                deadline: deadline(goal, availability),
            }),
        }
    }
    let mut placed = Vec::with_capacity(items.len());
    for item in items {
        let start = earliest_fit(&windows, &occupied, item.lower, item.minutes, item.deadline)?;
        let end = start + Duration::minutes(item.minutes);
        occupied.push((start, end));
        placed.push((item.index, Slot { start_at: start, end_at: end, est_minutes: item.est_minutes }));
    }
    Some(placed)
}

/// Earliest start at or after `lower` where `minutes` fit inside one
/// window, clear of every occupied span, ending by `limit`.
fn earliest_fit(windows: &[Span], occupied: &[Span], lower: DateTime<Utc>, minutes: i64, limit: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let length = Duration::minutes(minutes);
    for window in windows {
        let mut start = window.0.max(lower);
        loop {
            let end = start + length;
            if end > window.1 || end > limit {
                break;
            }
            match occupied.iter().filter(|o| o.0 < end && start < o.1).map(|o| o.1).max() {
                Some(blocked_until) => start = blocked_until,
                None => return Some(start),
            }
        }
    }
    None
}

/// Writes a day layout into the working actions, creating the catch-up action.
fn commit(current: &mut Vec<Action>, layout: Vec<(Option<usize>, Slot)>, goal: &Goal) {
    for (index, slot) in layout {
        match index {
            Some(i) => {
                current[i].start_at = slot.start_at;
                current[i].end_at = slot.end_at;
                current[i].est_minutes = slot.est_minutes;
            }
            None => {
                let op = DiffOp {
                    action_id: format!("{}-replan-{}", goal.id, slot.start_at.format("%Y%m%dT%H%M")),
                    op: Op::Create,
                    before: None,
                    after: Some(slot),
                    goal_id: Some(goal.id.clone()),
                    title: Some(format!("Catch up: {}", goal.title)),
                };
                *current = crate::apply(current, &[op]);
            }
        }
    }
}

/// Diff that turns `before` into `after`, sorted by new start then id.
fn diff_between(before: &[Action], after: &[Action]) -> Vec<DiffOp> {
    let slot = |a: &Action| Slot { start_at: a.start_at, end_at: a.end_at, est_minutes: a.est_minutes };
    let mut diff: Vec<DiffOp> = Vec::new();
    for (old, new) in before.iter().zip(after) {
        assert_eq!(old.id, new.id);
        if slot(old) != slot(new) {
            let op = if old.est_minutes == new.est_minutes { Op::Move } else { Op::Resize };
            diff.push(DiffOp { action_id: old.id.clone(), op, before: Some(slot(old)), after: Some(slot(new)), goal_id: None, title: None });
        }
    }
    for new in &after[before.len()..] {
        diff.push(DiffOp {
            action_id: new.id.clone(),
            op: Op::Create,
            before: None,
            after: Some(slot(new)),
            goal_id: Some(new.goal_id.clone()),
            title: Some(new.title.clone()),
        });
    }
    diff.sort_by(|a, b| (a.after.map(|s| s.start_at), &a.action_id).cmp(&(b.after.map(|s| s.start_at), &b.action_id)));
    diff
}
