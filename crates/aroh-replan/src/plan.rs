use crate::model::{ActionStatus, DiffOp, Infeasible, Op, PlanInput, Proposal, Slot};
use crate::parsed::{self, format_time, PAction, PGoal};
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
    goals: BTreeMap<String, PGoal>,
    missed: BTreeSet<&'a str>,
}

impl Ctx<'_> {
    /// Deadline of a live goal, or None when the goal is unknown.
    fn goal_deadline(&self, goal_id: &str) -> Option<DateTime<Utc>> {
        self.goals.get(goal_id).map(|g| deadline(g, &self.input.availability))
    }

    /// True when the action blocks time: planned and not a missed one.
    fn occupies(&self, action: &PAction) -> bool {
        action.status == ActionStatus::Planned && !self.missed.contains(action.id.as_str())
    }
}

/// Spreads each goal's missed minutes over its remaining days and returns
/// the diff plus minutes that fit nowhere. Created actions get fresh uuid v7
/// ids; everything else is deterministic. Output always passes validate().
pub fn plan(input: &PlanInput) -> Proposal {
    let ctx = Ctx {
        input,
        goals: parsed::goals(&input.goals).into_iter().map(|g| (g.id.clone(), g)).collect(),
        missed: input.missed_ids.iter().map(String::as_str).collect(),
    };
    let mut current = parsed::actions(&input.actions);
    let mut need: BTreeMap<String, i64> = BTreeMap::new();
    for action in &current {
        if ctx.missed.contains(action.id.as_str()) && ctx.goals.contains_key(&action.goal_id) {
            *need.entry(action.goal_id.clone()).or_default() += action.est_minutes.max(0);
        }
    }
    let mut order: Vec<&PGoal> = need.keys().map(|id| &ctx.goals[id]).collect();
    order.sort_by_key(|g| (deadline(g, &input.availability), g.id.clone()));

    let mut infeasible = Vec::new();
    for goal in order {
        let minutes = need[&goal.id];
        let placed = place_goal(&ctx, &mut current, goal, minutes);
        assert!(placed <= minutes);
        if placed < minutes {
            infeasible.push(Infeasible { goal_id: goal.id.clone(), unplaced_minutes: minutes - placed });
        }
    }
    let proposal = Proposal { diff: diff_of(&current), infeasible };
    debug_assert_eq!(crate::validate(input, &proposal.diff), Ok(()));
    proposal
}

/// Places up to `minutes` of a goal's work into `current`; returns minutes placed.
fn place_goal(ctx: &Ctx, current: &mut Vec<PAction>, goal: &PGoal, minutes: i64) -> i64 {
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
fn capacity(ctx: &Ctx, current: &[PAction], day: NaiveDate, goal: &PGoal, limit: i64) -> i64 {
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

/// New placement of one item: working index (None for a created action), start, end, estimate.
type Placed = (Option<usize>, DateTime<Utc>, DateTime<Utc>, i64);

/// Lays out `day` with the goal grown by `extra` minutes. Returns a
/// placement for every item, or None when something does not fit.
fn layout(ctx: &Ctx, current: &[PAction], day: NaiveDate, goal: &PGoal, extra: i64) -> Option<Vec<Placed>> {
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
        placed.push((item.index, start, end, item.est_minutes));
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
fn commit(current: &mut Vec<PAction>, layout: Vec<Placed>, goal: &PGoal) {
    for (index, start_at, end_at, est_minutes) in layout {
        match index {
            Some(i) => {
                current[i].start_at = start_at;
                current[i].end_at = end_at;
                current[i].est_minutes = est_minutes;
            }
            None => current.push(PAction {
                id: aroh_core::new_id(),
                goal_id: goal.id.clone(),
                title: format!("Catch up: {}", goal.title),
                start_at,
                end_at,
                est_minutes,
                status: ActionStatus::Planned,
                original: None,
            }),
        }
    }
}

/// Diff from each action's stored slot to its working slot, sorted by new start then id.
fn diff_of(current: &[PAction]) -> Vec<DiffOp> {
    let mut diff: Vec<(DateTime<Utc>, DiffOp)> = Vec::new();
    for action in current {
        let after = Slot { start_at: format_time(action.start_at), end_at: format_time(action.end_at), est_minutes: action.est_minutes };
        let entry = match &action.original {
            None => Some(DiffOp {
                action_id: action.id.clone(),
                op: Op::Create,
                before: None,
                after: Some(after),
                goal_id: Some(action.goal_id.clone()),
                title: Some(action.title.clone()),
            }),
            Some(before) => {
                let times = parsed::parse_slot(before).expect("parsed on input");
                let changed = times != (action.start_at, action.end_at) || before.est_minutes != action.est_minutes;
                let op = if before.est_minutes == action.est_minutes { Op::Move } else { Op::Resize };
                changed.then(|| DiffOp { action_id: action.id.clone(), op, before: Some(before.clone()), after: Some(after), goal_id: None, title: None })
            }
        };
        diff.extend(entry.map(|e| (action.start_at, e)));
    }
    diff.sort_by(|a, b| (a.0, &a.1.action_id).cmp(&(b.0, &b.1.action_id)));
    diff.into_iter().map(|(_, e)| e).collect()
}
