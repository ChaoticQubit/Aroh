use crate::model::{ActionStatus, DiffOp, Infeasible, Op, PlanInput, Proposal, Slot};
use crate::parsed::{self, format_time, PAction, PGoal};
use crate::time::{day_windows, deadline, inside_window, local_date, Span};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use std::collections::{BTreeMap, BTreeSet};

/// Upper bound on days scanned per goal, so a far target date stays cheap.
const DAYS_MAX: i64 = 3660;

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

/// Moves each goal's missed minutes to its earliest free days and returns
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

/// Shortest piece of work placed on one day, unless the whole segment is shorter.
const CHUNK_MIN: i64 = 25;

/// One run of a goal's work in queue order. `index` is the working index of
/// a movable planned action, or None for a missed action's minutes.
struct Segment {
    index: Option<usize>,
    title: String,
    minutes: i64,
    first_day: Option<NaiveDate>,
}

/// Part of a segment placed on one day; `first` marks the segment's first piece.
#[derive(Clone, Copy)]
struct Piece {
    segment: usize,
    minutes: i64,
    first: bool,
}

/// Back-to-back pieces laid out as one action. `host` is the segment whose
/// existing action the block becomes; None means a created action.
struct Block {
    host: Option<usize>,
    pieces: Vec<Piece>,
}

impl Block {
    fn minutes(&self) -> i64 {
        self.pieces.iter().map(|p| p.minutes).sum()
    }
}

/// Places up to `minutes` of the goal's missed work and returns minutes placed.
/// The missed work goes first and the goal's movable actions follow in order,
/// each day filled as far as it holds, so a full day pushes the goal's later
/// actions forward a day, up to the deadline. Minutes that do not fit are
/// cut from the missed work, never from the existing plan.
fn place_goal(ctx: &Ctx, current: &mut Vec<PAction>, goal: &PGoal, minutes: i64) -> i64 {
    let availability = &ctx.input.availability;
    let first = local_date(ctx.input.now, availability);
    let days: Vec<NaiveDate> = (0..DAYS_MAX)
        .map(|n| first + Duration::days(n))
        .take_while(|d| *d <= goal.target_date)
        .collect();
    let mut missed: Vec<&PAction> = current
        .iter()
        .filter(|a| a.goal_id == goal.id && ctx.missed.contains(a.id.as_str()) && a.est_minutes > 0)
        .collect();
    missed.sort_by(|a, b| (a.start_at, &a.id).cmp(&(b.start_at, &b.id)));
    let missed: Vec<(String, i64)> = missed.iter().map(|a| (a.title.clone(), a.est_minutes)).collect();
    assert_eq!(missed.iter().map(|m| m.1).sum::<i64>(), minutes);
    let mut queued: Vec<usize> = (0..current.len())
        .filter(|i| {
            let a = &current[*i];
            a.goal_id == goal.id
                && ctx.occupies(a)
                && a.start_at >= ctx.input.now
                && a.end_at > a.start_at
                && inside_window(a.start_at, a.end_at, availability)
                && days.contains(&local_date(a.start_at, availability))
        })
        .collect();
    queued.sort_by(|a, b| (current[*a].start_at, &current[*a].id).cmp(&(current[*b].start_at, &current[*b].id)));
    let exclude: BTreeSet<usize> = queued.iter().copied().collect();

    let mut cut = 0;
    loop {
        let mut segments: Vec<Segment> = Vec::new();
        let mut keep = minutes - cut;
        for (title, est) in &missed {
            let take = keep.min(*est);
            keep -= take;
            if take > 0 {
                segments.push(Segment { index: None, title: title.clone(), minutes: take, first_day: None });
            }
        }
        for index in &queued {
            let a = &current[*index];
            let first_day = Some(local_date(a.start_at, availability));
            segments.push(Segment { index: Some(*index), title: a.title.clone(), minutes: (a.end_at - a.start_at).num_minutes(), first_day });
        }
        let (plans, overflow) = fill(ctx, current, goal, &exclude, &segments, &days);
        if overflow == 0 {
            for (day, blocks) in plans {
                commit(ctx, current, goal, &exclude, &segments, day, &blocks);
            }
            return minutes - cut;
        }
        assert!(overflow > 0);
        if cut >= minutes {
            return 0;
        }
        cut = (cut + overflow).min(minutes);
    }
}

/// Fills days in order from the segment queue. Returns each day's blocks
/// and the minutes left unplaced after the last day.
fn fill(ctx: &Ctx, current: &[PAction], goal: &PGoal, exclude: &BTreeSet<usize>, segments: &[Segment], days: &[NaiveDate]) -> (Vec<(NaiveDate, Vec<Block>)>, i64) {
    let mut remaining: Vec<i64> = segments.iter().map(|s| s.minutes).collect();
    let mut next = 0;
    let mut plans = Vec::new();
    for day in days {
        if next == segments.len() {
            break;
        }
        let mut pieces: Vec<Piece> = Vec::new();
        while next < segments.len() && segments[next].first_day.is_none_or(|d| d <= *day) {
            let rest = remaining[next];
            let first = rest == segments[next].minutes;
            let fits = |m: i64| {
                let mut trial = pieces.clone();
                trial.push(Piece { segment: next, minutes: m, first });
                layout(ctx, current, *day, goal, exclude, segments, &blocks_of(segments, &trial)).is_some()
            };
            let take = if fits(rest) { rest } else { largest_fit(rest, fits) };
            if take == 0 {
                break;
            }
            pieces.push(Piece { segment: next, minutes: take, first });
            remaining[next] -= take;
            if remaining[next] > 0 {
                break;
            }
            next += 1;
        }
        assert!(pieces.iter().all(|p| p.minutes >= CHUNK_MIN.min(segments[p.segment].minutes)));
        if !pieces.is_empty() {
            plans.push((*day, blocks_of(segments, &pieces)));
        }
    }
    assert!(remaining.iter().zip(segments).all(|(r, s)| (0..=s.minutes).contains(r)));
    (plans, remaining.iter().sum())
}

/// Largest split of `rest` minutes that fits, keeping both parts at least
/// CHUNK_MIN long; 0 when no such split fits.
fn largest_fit(rest: i64, fits: impl Fn(i64) -> bool) -> i64 {
    let (mut lo, mut hi) = (CHUNK_MIN, rest - CHUNK_MIN);
    if hi < lo || !fits(lo) {
        return 0;
    }
    while lo < hi {
        let mid = lo + (hi - lo + 1) / 2;
        if fits(mid) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    lo
}

/// Groups one day's pieces into blocks: each first piece of an existing
/// action starts its own block, and other pieces join the next block that
/// day, else the last one, else a created block.
fn blocks_of(segments: &[Segment], pieces: &[Piece]) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();
    let mut pending: Vec<Piece> = Vec::new();
    for piece in pieces {
        pending.push(*piece);
        if piece.first && segments[piece.segment].index.is_some() {
            blocks.push(Block { host: Some(piece.segment), pieces: std::mem::take(&mut pending) });
        }
    }
    if !pending.is_empty() {
        match blocks.last_mut() {
            Some(last) => last.pieces.extend(pending),
            None => blocks.push(Block { host: None, pieces: pending }),
        }
    }
    blocks
}

/// Placement of one other action on the day: working index, start, end.
type Placed = (usize, DateTime<Utc>, DateTime<Utc>);

/// Lays out `day` with the goal's queued actions replaced by `blocks`.
/// Returns new slots for the day's other movable actions and one span per
/// block, or None when something does not fit.
fn layout(
    ctx: &Ctx,
    current: &[PAction],
    day: NaiveDate,
    goal: &PGoal,
    exclude: &BTreeSet<usize>,
    segments: &[Segment],
    blocks: &[Block],
) -> Option<(Vec<Placed>, Vec<Span>)> {
    let availability = &ctx.input.availability;
    let now = ctx.input.now;
    let windows: Vec<Span> = day_windows(day, availability)
        .into_iter()
        .filter(|w| w.1 > now)
        .map(|w| (w.0.max(now), w.1))
        .collect();
    let mut items: Vec<(Option<usize>, DateTime<Utc>, i64, DateTime<Utc>)> = Vec::new();
    let mut occupied: Vec<Span> = Vec::new();
    for (index, action) in current.iter().enumerate() {
        if !ctx.occupies(action) || exclude.contains(&index) {
            continue;
        }
        let item_deadline = ctx.goal_deadline(&action.goal_id);
        let movable = action.start_at >= now
            && local_date(action.start_at, availability) == day
            && inside_window(action.start_at, action.end_at, availability);
        match item_deadline {
            Some(limit) if movable => items.push((Some(index), action.start_at, (action.end_at - action.start_at).num_minutes(), limit)),
            _ => occupied.push((action.start_at, action.end_at)),
        }
    }
    items.sort_by(|a, b| (a.1, a.0.map(|x| &current[x].id)).cmp(&(b.1, b.0.map(|x| &current[x].id))));
    let goal_deadline = deadline(goal, availability);
    for block in blocks {
        let unchanged = match (block.host, block.pieces.as_slice()) {
            (Some(host), [only]) if only.minutes == segments[host].minutes && segments[host].first_day == Some(day) => {
                Some(current[segments[host].index.expect("host is existing")].start_at)
            }
            _ => None,
        };
        let lower = match unchanged {
            Some(start) => start,
            None => windows.first()?.0,
        };
        items.push((None, lower, block.minutes(), goal_deadline));
    }
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by_key(|i| items[*i].1);
    let mut others = Vec::new();
    let mut spans = vec![None; blocks.len()];
    let first_block = items.len() - blocks.len();
    for i in order {
        let (index, lower, length, limit) = items[i];
        let start = earliest_fit(&windows, &occupied, lower, length, limit)?;
        let end = start + Duration::minutes(length);
        occupied.push((start, end));
        match index {
            Some(index) => others.push((index, start, end)),
            None => spans[i - first_block] = Some((start, end)),
        }
    }
    Some((others, spans.into_iter().map(|s| s.expect("every block placed")).collect()))
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

/// Writes one day's layout into the working actions: other actions get
/// their new slots, each hosted block resizes its action and each unhosted
/// block becomes a created action.
fn commit(ctx: &Ctx, current: &mut Vec<PAction>, goal: &PGoal, exclude: &BTreeSet<usize>, segments: &[Segment], day: NaiveDate, blocks: &[Block]) {
    let (others, spans) = layout(ctx, current, day, goal, exclude, segments, blocks).expect("filled layout fits");
    for (index, start_at, end_at) in others {
        current[index].start_at = start_at;
        current[index].end_at = end_at;
    }
    for (block, (start_at, end_at)) in blocks.iter().zip(spans) {
        let length = block.minutes();
        assert_eq!((end_at - start_at).num_minutes(), length);
        match block.host {
            Some(host) => {
                let action = &mut current[segments[host].index.expect("host is existing")];
                action.est_minutes += length - segments[host].minutes;
                assert!(action.est_minutes >= 0);
                action.start_at = start_at;
                action.end_at = end_at;
            }
            None => {
                let lead = &segments[block.pieces[0].segment];
                let title = match lead.index {
                    None => format!("Catch up: {}", lead.title),
                    Some(_) => format!("{} (continued)", lead.title),
                };
                current.push(PAction {
                    id: aroh_core::new_id(),
                    goal_id: goal.id.clone(),
                    title,
                    start_at,
                    end_at,
                    est_minutes: length,
                    status: ActionStatus::Planned,
                    original: None,
                });
            }
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
