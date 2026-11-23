use aroh_app::{local, remove, tick};
use aroh_domain::hlc::Hlc;
use aroh_domain::lww::{remote_wins, Version};
use aroh_domain::rows::{samples, Action, ActionLog, ActionSeries, Goal, GoalFile, PlaceLink, Preference, Usage};
use aroh_domain::schema::{Kind, TableDef, SYNCED};
use aroh_domain::seed::{routines_id, SEED_STAMP};
use aroh_store::codec::WireRow;
use aroh_store::sync::{apply_page, sync_all, Remote};
use aroh_store::{db, repo, Error, Result};
use chrono::{DateTime, Duration, TimeZone, Utc};
use chrono_tz::Tz;
use rusqlite::Connection;
use std::collections::BTreeMap;
use std::future::{ready, Future};
use std::sync::{Arc, Mutex};

const ZONE: Tz = chrono_tz::Europe::Berlin;
const USER: &str = samples::USER;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 23, 6, 0, 0).unwrap()
}

fn store(node: &str) -> (Connection, Hlc) {
    let mut conn = db::open_in_memory().unwrap();
    let mut clock = Hlc::new(node);
    for (table, row) in SYNCED.iter().zip(samples::all()) {
        let serde_json::Value::Object(mut row) = row else { unreachable!() };
        postgres_times(table, &mut row);
        row.insert("server_seq".into(), 1.into());
        apply_page(&mut conn, table, USER, &[row], &mut clock).unwrap();
    }
    (conn, clock)
}

fn live_count(conn: &Connection, table: &str, column: &str, value: &str) -> i64 {
    conn.query_row(&format!("SELECT count(*) FROM {table} WHERE {column} = ?1 AND deleted = 0"), [value], |r| r.get(0)).unwrap()
}

fn goal() -> String {
    samples::goal_id()
}

#[test]
fn delete_all_tombstones_every_row_of_the_goal_and_keeps_facts_and_spend() {
    let (conn, mut clock) = store("00000000000000d1");
    let file: GoalFile = repo::list(&conn, &[("goal_id", goal().as_str())]).unwrap().remove(0);
    let paths = remove::delete_goal(&conn, &mut clock, &goal(), false).unwrap();
    assert_eq!(paths, vec![file.storage_path.clone().unwrap()]);
    assert!(matches!(repo::get::<Goal>(&conn, &goal()), Err(Error::NotFound(_))));
    for (table, column) in [
        ("goal_files", "goal_id"),
        ("personas", "goal_id"),
        ("plugins", "goal_id"),
        ("trackers", "goal_id"),
        ("action_series", "goal_id"),
        ("actions", "goal_id"),
        ("action_logs", "goal_id"),
        ("proposals", "goal_id"),
        ("budgets", "goal_id"),
        ("place_links", "goal_id"),
        ("goal_links", "from_goal_id"),
    ] {
        assert_eq!(live_count(&conn, table, column, &goal()), 0, "{table}");
    }
    assert_eq!(conn.query_row("SELECT count(*) FROM tracker_entries WHERE deleted = 0", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
    let text: Option<String> = conn.query_row("SELECT extracted_text FROM goal_files WHERE id = ?1", [&file.id], |r| r.get(0)).unwrap();
    assert_eq!(text, None, "a deleted file keeps no text");
    assert_eq!(repo::list::<Preference>(&conn, &[]).unwrap().len(), 1, "profile facts stay");
    assert_eq!(repo::list::<Usage>(&conn, &[]).unwrap().len(), 1, "spend history stays");
    assert!(repo::list::<Goal>(&conn, &[("id", routines_id().as_str())]).unwrap().is_empty(), "delete all makes no Routines goal");
}

#[test]
fn keep_actions_moves_actions_series_and_their_place_links_to_routines() {
    let (conn, mut clock) = store("00000000000000d1");
    let series: Vec<ActionSeries> = repo::list(&conn, &[("goal_id", goal().as_str())]).unwrap();
    let actions: Vec<Action> = repo::list(&conn, &[("goal_id", goal().as_str())]).unwrap();
    assert!(!series.is_empty() && !actions.is_empty());
    remove::delete_goal(&conn, &mut clock, &goal(), true).unwrap();
    assert!(matches!(repo::get::<Goal>(&conn, &goal()), Err(Error::NotFound(_))));
    let routines = routines_id();
    for s in &series {
        assert_eq!(repo::get::<ActionSeries>(&conn, &s.id).unwrap().goal_id, routines);
    }
    for a in &actions {
        let kept = repo::get::<Action>(&conn, &a.id).unwrap();
        assert_eq!((kept.goal_id.as_str(), kept.title.as_str(), kept.start_at), (routines.as_str(), a.title.as_str(), a.start_at));
    }
    let links: Vec<PlaceLink> = repo::list(&conn, &[]).unwrap();
    assert!(links.iter().all(|l| l.goal_id == routines), "a link naming a kept series follows it");
    let logs: Vec<ActionLog> = repo::list(&conn, &[]).unwrap();
    assert!(!logs.is_empty() && logs.iter().all(|l| l.goal_id == routines), "session logs follow their actions");
    assert_eq!(repo::get::<Goal>(&conn, &routines).unwrap().title, "Routines");
    for table in ["goal_files", "personas", "plugins", "trackers", "proposals", "budgets"] {
        assert_eq!(live_count(&conn, table, "goal_id", &goal()), 0, "{table}");
    }
    assert_eq!(repo::list::<Preference>(&conn, &[]).unwrap().len(), 1);
}

#[test]
fn routines_cannot_be_deleted_and_a_missing_goal_is_not_found() {
    let (conn, mut clock) = store("00000000000000d1");
    remove::delete_goal(&conn, &mut clock, &goal(), true).unwrap();
    let err = remove::delete_goal(&conn, &mut clock, &routines_id(), false).unwrap_err();
    assert!(err.to_string().contains("Routines"), "{err}");
    assert!(repo::get::<Goal>(&conn, &routines_id()).is_ok());
    assert!(remove::delete_goal(&conn, &mut clock, &goal(), false).is_err(), "deleting twice fails");
}

#[test]
fn file_removals_wait_on_the_device_until_cleared() {
    let conn = db::open_in_memory().unwrap();
    assert!(local::pending_removals(&conn).unwrap().is_empty());
    local::queue_removals(&conn, &["goals/g/a.pdf".to_string(), "goals/g/b.png".to_string()]).unwrap();
    local::queue_removals(&conn, &["goals/g/a.pdf".to_string()]).unwrap();
    assert_eq!(local::pending_removals(&conn).unwrap(), vec!["goals/g/a.pdf", "goals/g/b.png"]);
    local::clear_removals(&conn, &["goals/g/a.pdf".to_string()]).unwrap();
    assert_eq!(local::pending_removals(&conn).unwrap(), vec!["goals/g/b.png"]);
}

#[derive(Default)]
struct Server {
    seq: i64,
    rows: BTreeMap<(&'static str, String), (i64, WireRow)>,
}

impl Server {
    fn put(&mut self, table: &'static str, row: WireRow) {
        self.seq += 1;
        self.rows.insert((table, row["id"].as_str().unwrap().to_string()), (self.seq, row));
    }
}

struct MemoryRemote(Arc<Mutex<Server>>);

fn version(row: &WireRow) -> Version {
    Version { updated_at: row["updated_at"].as_str().unwrap().to_string(), deleted: row["deleted"].as_bool().unwrap() }
}

fn postgres_times(table: &TableDef, row: &mut WireRow) {
    for column in table.columns.iter().filter(|c| c.kind == Kind::Time) {
        if let Some(text) = row[column.name].as_str() {
            row.insert(column.name.into(), text.replace('Z', "+00:00").into());
        }
    }
}

impl Remote for MemoryRemote {
    fn push(&mut self, table: &'static TableDef, rows: &[WireRow]) -> impl Future<Output = Result<()>> + Send {
        let mut server = self.0.lock().unwrap();
        for row in rows {
            let local = server.rows.get(&(table.name, row["id"].as_str().unwrap().to_string())).map(|(_, r)| version(r));
            if remote_wins(local.as_ref(), &version(row)) {
                server.put(table.name, row.clone());
            }
        }
        ready(Ok(()))
    }

    fn pull(&mut self, table: &'static TableDef, after: i64, limit: usize) -> impl Future<Output = Result<Vec<WireRow>>> + Send {
        let server = self.0.lock().unwrap();
        let mut rows: Vec<(i64, WireRow)> = server.rows.iter().filter(|((t, _), (seq, _))| *t == table.name && *seq > after).map(|(_, v)| v.clone()).collect();
        rows.sort_by_key(|(seq, _)| *seq);
        let page = rows
            .into_iter()
            .take(limit)
            .map(|(seq, mut row)| {
                postgres_times(table, &mut row);
                row.insert("server_seq".into(), seq.into());
                row
            })
            .collect();
        ready(Ok(page))
    }
}

fn sync(conn: &mut Connection, clock: &mut Hlc, server: &Arc<Mutex<Server>>) {
    let mut remote = MemoryRemote(server.clone());
    let run = |conn: &mut Connection, clock: &mut Hlc, remote: &mut MemoryRemote| {
        tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(sync_all(conn, remote, USER, clock)).unwrap();
    };
    run(conn, clock, &mut remote);
    tick::tick(conn, clock, USER, now(), ZONE).unwrap();
    run(conn, clock, &mut remote);
}

fn rows_of(conn: &Connection) -> Vec<(String, String, bool, Option<String>)> {
    let mut out = Vec::new();
    for table in SYNCED.iter().filter(|t| t.column("goal_id").is_some()) {
        let mut stmt = conn.prepare(&format!("SELECT id, deleted, goal_id FROM {} ORDER BY id", table.name)).unwrap();
        let rows = stmt.query_map([], |r| Ok((table.name.to_string(), r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
        out.extend(rows.map(|r| r.unwrap()));
    }
    out
}

fn late_occurrence(series: &ActionSeries, days: i64) -> WireRow {
    let from = now() + Duration::days(days);
    let action = aroh_domain::recur::materialize(series, from, 7, &Default::default()).unwrap().remove(0);
    assert_eq!(action.updated_at, SEED_STAMP);
    let serde_json::Value::Object(row) = serde_json::to_value(action).unwrap() else { unreachable!() };
    row
}

fn converges(keep: bool) {
    let server = Arc::new(Mutex::new(Server::default()));
    for (table, row) in SYNCED.iter().zip(samples::all()) {
        let serde_json::Value::Object(row) = row else { unreachable!() };
        server.lock().unwrap().put(table.name, row);
    }
    let mut a = db::open_in_memory().unwrap();
    let mut clock_a = Hlc::new("00000000000000a1");
    sync(&mut a, &mut clock_a, &server);
    let mut b = db::open_in_memory().unwrap();
    let mut clock_b = Hlc::new("00000000000000b1");
    sync(&mut b, &mut clock_b, &server);
    let series: ActionSeries = repo::list(&a, &[("goal_id", goal().as_str())]).unwrap().remove(0);
    remove::delete_goal(&a, &mut clock_a, &goal(), keep).unwrap();
    server.lock().unwrap().put("actions", late_occurrence(&series, 20));
    sync(&mut a, &mut clock_a, &server);
    sync(&mut b, &mut clock_b, &server);
    sync(&mut a, &mut clock_a, &server);
    assert_eq!(rows_of(&a), rows_of(&b), "both devices hold the same rows");
    assert_eq!(live_count(&b, "goals", "id", &goal()), 0);
    assert_eq!(live_count(&b, "actions", "goal_id", &goal()), 0, "no live action keeps the deleted goal");
    let kept = live_count(&b, "actions", "goal_id", &routines_id());
    if keep {
        assert!(kept > 0 && live_count(&b, "action_series", "goal_id", &routines_id()) > 0);
    } else {
        assert_eq!(kept, 0);
        assert_eq!(live_count(&b, "action_series", "id", &series.id), 0, "the tick never brings the series back");
    }
}

#[test]
fn a_second_device_converges_after_delete_all() {
    converges(false);
}

#[test]
fn a_second_device_converges_after_keep_actions() {
    converges(true);
}
