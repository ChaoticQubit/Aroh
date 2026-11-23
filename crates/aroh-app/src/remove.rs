//! Deleting a goal (Q68): tombstones of the goal and every row tied to it, or with
//! `keep_actions` its actions and series moved onto the built-in Routines goal, plus the
//! tick's settling of rows that reach a device after their goal was deleted.

use crate::location::ensure_routines;
use crate::{invalid, Result};
use aroh_domain::hlc::Hlc;
use aroh_domain::rows::{Action, ActionSeries, Budget, Goal, GoalFile, GoalLink, Persona, PlaceLink, Plugin, Proposal, Record, Tracker, TrackerEntry};
use aroh_domain::seed::routines_id;
use aroh_store::repo;
use rusqlite::Connection;
use std::collections::BTreeSet;

fn tombstone_all<R: Record>(conn: &Connection, clock: &mut Hlc, filter: &[(&str, &str)]) -> Result<Vec<R>> {
    let rows: Vec<R> = repo::list(conn, filter)?;
    for row in &rows {
        repo::delete::<R>(conn, clock, row.id())?;
    }
    Ok(rows)
}

/// Deletes goal `goal_id` in one transaction. Without `keep_actions` its actions and series
/// are tombstoned; with it they, and place links naming a kept series, move to Routines.
/// Either way the goal, links, files (text cleared), personas, plugins, trackers and their
/// entries, proposals, budget and other place links are tombstoned; profile facts and usage stay.
/// Output: storage paths of the goal's stored files. Errors for Routines or a missing goal.
pub fn delete_goal(conn: &Connection, clock: &mut Hlc, goal_id: &str, keep_actions: bool) -> Result<Vec<String>> {
    if goal_id == routines_id() {
        return Err(invalid("the Routines goal cannot be deleted"));
    }
    let tx = conn.unchecked_transaction()?;
    let goal: Goal = repo::get(&tx, goal_id)?;
    let by_goal = [("goal_id", goal_id)];
    let mut paths = Vec::new();
    for mut file in repo::list::<GoalFile>(&tx, &by_goal)? {
        paths.extend(file.storage_path.clone());
        file.extracted_text = None;
        file.deleted = true;
        repo::put(&tx, clock, &mut file)?;
    }
    tombstone_all::<GoalLink>(&tx, clock, &[("from_goal_id", goal_id)])?;
    tombstone_all::<GoalLink>(&tx, clock, &[("to_goal_id", goal_id)])?;
    tombstone_all::<Persona>(&tx, clock, &by_goal)?;
    tombstone_all::<Plugin>(&tx, clock, &by_goal)?;
    for tracker in tombstone_all::<Tracker>(&tx, clock, &by_goal)? {
        tombstone_all::<TrackerEntry>(&tx, clock, &[("tracker_id", tracker.id.as_str())])?;
    }
    tombstone_all::<Proposal>(&tx, clock, &by_goal)?;
    tombstone_all::<Budget>(&tx, clock, &by_goal)?;
    let kept: BTreeSet<String> = if keep_actions {
        let routines = ensure_routines(&tx, clock, &goal.user_id)?.id;
        let mut kept = BTreeSet::new();
        for mut series in repo::list::<ActionSeries>(&tx, &by_goal)? {
            series.goal_id = routines.clone();
            repo::put(&tx, clock, &mut series)?;
            kept.insert(series.id);
        }
        for mut action in repo::list::<Action>(&tx, &by_goal)? {
            action.goal_id = routines.clone();
            repo::put(&tx, clock, &mut action)?;
        }
        for mut link in repo::list::<PlaceLink>(&tx, &by_goal)? {
            if link.series_id.as_ref().is_some_and(|s| kept.contains(s)) {
                link.goal_id = routines.clone();
                repo::put(&tx, clock, &mut link)?;
            }
        }
        kept
    } else {
        tombstone_all::<Action>(&tx, clock, &by_goal)?;
        tombstone_all::<ActionSeries>(&tx, clock, &by_goal)?.into_iter().map(|s| s.id).collect()
    };
    tombstone_all::<PlaceLink>(&tx, clock, &by_goal)?;
    repo::delete::<Goal>(&tx, clock, goal_id)?;
    assert!(repo::list::<Action>(&tx, &by_goal)?.is_empty(), "no live action keeps a deleted goal");
    assert!(keep_actions || kept.iter().all(|id| repo::get::<ActionSeries>(&tx, id).is_err()));
    tx.commit()?;
    Ok(paths)
}

fn deleted_goals(conn: &Connection, user_id: &str) -> Result<BTreeSet<String>> {
    let mut stmt = conn.prepare("SELECT id FROM goals WHERE user_id = ?1 AND deleted = 1")?;
    let ids = stmt.query_map([user_id], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<BTreeSet<_>>>()?;
    Ok(ids)
}

/// Settles live series and actions of `user_id` whose goal is a tombstone, rows that arrived
/// after the delete (a server occurrence, another device's write). An action follows its live
/// series' goal, an action of a gone series is tombstoned, everything else moves to Routines,
/// so no owner row is lost. Output: rows changed. Runs in every tick, before materializing.
pub fn settle_orphans(conn: &Connection, clock: &mut Hlc, user_id: &str) -> Result<usize> {
    let dead = deleted_goals(conn, user_id)?;
    if dead.is_empty() {
        return Ok(0);
    }
    let tx = conn.unchecked_transaction()?;
    let mut routines: Option<String> = None;
    let mut home = |tx: &Connection, clock: &mut Hlc| -> Result<String> {
        if routines.is_none() {
            routines = Some(ensure_routines(tx, clock, user_id)?.id);
        }
        Ok(routines.clone().expect("set above"))
    };
    let mut changed = 0;
    for mut series in repo::list::<ActionSeries>(&tx, &[("user_id", user_id)])?.into_iter().filter(|s| dead.contains(&s.goal_id)) {
        series.goal_id = home(&tx, clock)?;
        repo::put(&tx, clock, &mut series)?;
        changed += 1;
    }
    for mut action in repo::list::<Action>(&tx, &[("user_id", user_id)])?.into_iter().filter(|a| dead.contains(&a.goal_id)) {
        match action.series_id.as_deref().map(|id| repo::get::<ActionSeries>(&tx, id)) {
            Some(Ok(series)) => {
                action.goal_id = series.goal_id;
                repo::put(&tx, clock, &mut action)?;
            }
            Some(Err(aroh_store::Error::NotFound(_))) => repo::delete::<Action>(&tx, clock, &action.id)?,
            Some(Err(e)) => return Err(e.into()),
            None => {
                action.goal_id = home(&tx, clock)?;
                repo::put(&tx, clock, &mut action)?;
            }
        }
        changed += 1;
    }
    tx.commit()?;
    Ok(changed)
}
