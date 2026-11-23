use aroh_domain::hlc::Hlc;
use aroh_domain::lww::{remote_wins, Version};
use aroh_domain::rows::{samples, Action, ActionSeries, Category, Goal};
use aroh_domain::schema::{Kind, TableDef, SYNCED};
use aroh_store::codec::WireRow;
use aroh_store::sync::{apply_page, pending, sync_all, Remote};
use aroh_store::{db, repo, Error, Result};
use chrono::{TimeZone, Utc};
use rusqlite::Connection;
use std::collections::BTreeMap;
use std::future::{ready, Future};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Server {
    seq: i64,
    rows: BTreeMap<(&'static str, String, String), (i64, WireRow)>,
}

struct MemoryRemote {
    user: String,
    server: Arc<Mutex<Server>>,
}

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
        let mut server = self.server.lock().unwrap();
        for row in rows {
            if row["user_id"].as_str() != Some(self.user.as_str()) {
                return ready(Err(Error::Remote("new row violates row-level security policy".into())));
            }
            let key = (table.name, self.user.clone(), row["id"].as_str().unwrap().to_string());
            let local = server.rows.get(&key).map(|(_, r)| version(r));
            if remote_wins(local.as_ref(), &version(row)) {
                server.seq += 1;
                let seq = server.seq;
                server.rows.insert(key, (seq, row.clone()));
            }
        }
        ready(Ok(()))
    }

    fn pull(&mut self, table: &'static TableDef, after: i64, limit: usize) -> impl Future<Output = Result<Vec<WireRow>>> + Send {
        let server = self.server.lock().unwrap();
        let mut rows: Vec<(i64, WireRow)> = server
            .rows
            .iter()
            .filter(|((t, user, _), (seq, _))| *t == table.name && *user == self.user && *seq > after)
            .map(|(_, (seq, row))| (*seq, row.clone()))
            .collect();
        rows.sort_by_key(|(seq, _)| *seq);
        rows.truncate(limit);
        let out = rows
            .into_iter()
            .map(|(seq, mut row)| {
                postgres_times(table, &mut row);
                row.insert("server_seq".into(), seq.into());
                row
            })
            .collect();
        ready(Ok(out))
    }
}

struct Device {
    conn: Connection,
    clock: Hlc,
    remote: MemoryRemote,
}

impl Device {
    fn new(node: &str, server: &Arc<Mutex<Server>>, wall: u64) -> Self {
        Device {
            conn: db::open_in_memory().unwrap(),
            clock: Hlc::with_wall_clock(node, wall),
            remote: MemoryRemote { user: samples::USER.into(), server: server.clone() },
        }
    }

    async fn sync(&mut self) -> aroh_store::sync::Moved {
        sync_all(&mut self.conn, &mut self.remote, samples::USER, &mut self.clock).await.unwrap()
    }

    fn pending(&self, table: &TableDef) -> Vec<WireRow> {
        pending(&self.conn, table, samples::USER, self.clock.node(), "", 500).unwrap()
    }
}

fn table(name: &str) -> &'static TableDef {
    aroh_domain::schema::table(name).unwrap()
}

#[tokio::test]
async fn two_devices_converge_on_edits_and_tombstones() {
    let server = Arc::new(Mutex::new(Server::default()));
    let mut a = Device::new("00000000000000aa", &server, 1_763_683_300_000);
    let mut b = Device::new("00000000000000bb", &server, 1_763_683_300_000);

    let mut goal = samples::goal();
    repo::put(&a.conn, &mut a.clock, &mut goal).unwrap();
    assert_eq!(a.sync().await.pushed, 1);
    assert_eq!(b.sync().await.pulled, 1);
    assert_eq!(repo::get::<Goal>(&b.conn, &goal.id).unwrap(), goal);
    assert!(b.pending(table("goals")).is_empty(), "pulled rows are not pushed back");

    let mut on_b = repo::get::<Goal>(&b.conn, &goal.id).unwrap();
    on_b.title = "Edited on B".into();
    repo::put(&b.conn, &mut b.clock, &mut on_b).unwrap();
    a.clock.set_wall_clock(1_763_683_400_000);
    repo::delete::<Goal>(&a.conn, &mut a.clock, &goal.id).unwrap();

    for _ in 0..2 {
        b.sync().await;
        a.sync().await;
    }
    let dump = |d: &Device| d.conn.query_row("SELECT title, deleted, updated_at FROM goals", [], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?))).unwrap();
    assert_eq!(dump(&a), dump(&b));
    assert_eq!(dump(&a).1, 1, "the later tombstone wins");
}

#[tokio::test]
async fn every_sample_row_round_trips_through_sync() {
    let server = Arc::new(Mutex::new(Server::default()));
    let mut a = Device::new("00000000000000aa", &server, 1_763_683_300_000);
    let mut b = Device::new("00000000000000bb", &server, 1_763_683_300_000);
    let stamp = a.clock.tick();
    for (table, row) in SYNCED.iter().zip(samples::all()) {
        let mut row = row.as_object().unwrap().clone();
        row.insert("updated_at".into(), stamp.clone().into());
        let sql = format!("INSERT INTO {} ({}) VALUES ({})", table.name, table.names().join(", "), (1..=table.columns.len()).map(|i| format!("?{i}")).collect::<Vec<_>>().join(", "));
        a.conn.execute(&sql, rusqlite::params_from_iter(aroh_store::codec::row_to_sql(table, &row).unwrap())).unwrap();
    }
    assert_eq!(a.sync().await.pushed, SYNCED.len());
    assert_eq!(b.sync().await.pulled, SYNCED.len());
    for table in SYNCED.iter() {
        assert_eq!(b.pending(table).len(), 0);
        let read = |d: &Device| pending(&d.conn, table, samples::USER, "00000000000000aa", "", 500).unwrap();
        assert_eq!(read(&a), read(&b), "{}", table.name);
    }
    let start: String = b.conn.query_row("SELECT start_at FROM actions", [], |r| r.get(0)).unwrap();
    assert_eq!(start, "2026-11-23T09:00:00Z", "pulled times are normalized");
}

#[tokio::test]
async fn derived_rows_collapse_and_are_not_pushed_by_others() {
    let server = Arc::new(Mutex::new(Server::default()));
    let mut a = Device::new("00000000000000aa", &server, 1_763_683_300_000);
    let mut b = Device::new("00000000000000bb", &server, 1_763_683_300_000);
    for d in [&a, &b] {
        for c in aroh_domain::seed::categories(samples::USER) {
            repo::insert_derived(&d.conn, &c).unwrap();
        }
    }
    assert!(a.pending(table("categories")).is_empty());

    let mut series = samples::series();
    repo::put(&a.conn, &mut a.clock, &mut series).unwrap();
    a.sync().await;
    b.sync().await;
    let from = Utc.with_ymd_and_hms(2026, 11, 23, 0, 0, 0).unwrap();
    let on_a = repo::materialize(&a.conn, &series.id, from, 14).unwrap();
    let on_b = repo::materialize(&b.conn, &series.id, from, 14).unwrap();
    assert_eq!(on_a, on_b);
    assert!(a.pending(table("actions")).is_empty() && b.pending(table("actions")).is_empty());

    repo::delete::<Action>(&a.conn, &mut a.clock, &on_a[0].id).unwrap();
    a.sync().await;
    b.sync().await;
    assert_eq!(repo::list::<Action>(&b.conn, &[]).unwrap().len(), 13);
    assert!(repo::materialize(&b.conn, &series.id, from, 14).unwrap().is_empty());
    assert_eq!(repo::list::<Category>(&b.conn, &[]).unwrap().len(), 6);
    assert!(repo::get::<ActionSeries>(&b.conn, &series.id).is_ok());
}

#[test]
fn sync_future_is_send() {
    fn send<T: Send>(_: T) {}
    let server = Arc::new(Mutex::new(Server::default()));
    let mut d = Device::new("00000000000000aa", &server, 1_763_683_300_000);
    send(sync_all(&mut d.conn, &mut d.remote, samples::USER, &mut d.clock));
}

#[test]
fn foreign_and_bad_pages_commit_nothing() {
    let mut conn = db::open_in_memory().unwrap();
    let mut clock = Hlc::with_wall_clock("00000000000000bb", 1_763_683_300_000);
    let goals = table("goals");
    let good = samples::all()[1].as_object().unwrap().clone();
    let with_seq = |mut r: WireRow, seq: i64| {
        r.insert("server_seq".into(), seq.into());
        r
    };

    let mut foreign = good.clone();
    foreign.insert("user_id".into(), "0190f2a0-0000-7000-8000-00000000000b".into());
    foreign.insert("id".into(), aroh_domain::new_id().into());
    let page = [with_seq(good.clone(), 1), with_seq(foreign, 2)];
    assert!(apply_page(&mut conn, goals, samples::USER, &page, &mut clock).is_err());

    let mut bad = good.clone();
    bad.insert("status".into(), "exploded".into());
    let page = [with_seq(good.clone(), 1), with_seq(bad, 2)];
    assert!(apply_page(&mut conn, goals, samples::USER, &page, &mut clock).is_err());
    assert!(apply_page(&mut conn, goals, samples::USER, std::slice::from_ref(&good), &mut clock).is_err());

    let count: i64 = conn.query_row("SELECT count(*) FROM goals", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 0);
    assert_eq!(aroh_store::sync::get_state(&conn, &format!("pull:{}:goals", samples::USER)).unwrap(), None);

    assert_eq!(apply_page(&mut conn, goals, samples::USER, &[with_seq(good.clone(), 5)], &mut clock).unwrap(), 1);
    assert!(apply_page(&mut conn, goals, samples::USER, &[with_seq(good, 5)], &mut clock).is_err(), "cursor must advance");
    assert!(clock.tick().as_str() > samples::STAMP);
}

const SENTINEL_LAT: i64 = 47_123_457;
const SENTINEL_TEXT: &str = "SENTINEL-RAW-LOCATION";

fn fill_local_tables(conn: &Connection) {
    conn.execute(
        "INSERT INTO visits (id, lat_e6, lon_e6, accuracy_m, arrived_at, left_at, place_id) VALUES (?1, ?2, ?2, 30, '2026-11-20T10:00:00Z', NULL, NULL)",
        rusqlite::params![aroh_domain::new_id(), SENTINEL_LAT],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO place_candidates (id, lat_e6, lon_e6, poi_name, poi_kind, address, state, visit_count, last_at, prompted_at) VALUES (?1, ?2, ?2, ?3, ?3, ?3, 'pending', 3, '2026-11-20T10:00:00Z', NULL)",
        rusqlite::params![aroh_domain::new_id(), SENTINEL_LAT, SENTINEL_TEXT],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO pattern_candidates (id, place_id, title, gap_days, weekday, local_time, duration_minutes, evidence, state, declined_until, created_at) VALUES (?1, ?1, ?2, 7, 6, '10:00', 45, 5, 'pending', NULL, '2026-11-20T10:00:00Z')",
        rusqlite::params![aroh_domain::new_id(), SENTINEL_TEXT],
    )
    .unwrap();
    conn.execute("INSERT INTO nudge_sent (place_id, local_date) VALUES (?1, '2026-11-20')", [SENTINEL_TEXT]).unwrap();
    conn.execute("INSERT INTO place_kinds (place_id, kind) VALUES (?1, ?1)", [SENTINEL_TEXT]).unwrap();
}

#[tokio::test]
async fn raw_visits_never_reach_sync() {
    let server = Arc::new(Mutex::new(Server::default()));
    let mut a = Device::new("00000000000000aa", &server, 1_763_683_300_000);
    fill_local_tables(&a.conn);
    let mut goal = samples::goal();
    repo::put(&a.conn, &mut a.clock, &mut goal).unwrap();
    let mut place = samples::place();
    repo::put(&a.conn, &mut a.clock, &mut place).unwrap();

    for table in SYNCED.iter() {
        assert!(!aroh_store::local::LOCAL_TABLES.contains(&table.name), "{} is local only", table.name);
        let rows = serde_json::to_string(&a.pending(table)).unwrap();
        assert!(!rows.contains(SENTINEL_TEXT) && !rows.contains(&SENTINEL_LAT.to_string()), "{}", table.name);
    }
    assert_eq!(a.sync().await.pushed, 2, "only the goal and the named place push");
    let pushed = serde_json::to_string(&server.lock().unwrap().rows.values().map(|(_, r)| r.clone()).collect::<Vec<_>>()).unwrap();
    assert!(!pushed.contains(SENTINEL_TEXT) && !pushed.contains(&SENTINEL_LAT.to_string()));
    assert!(server.lock().unwrap().rows.keys().all(|(t, _, _)| *t == "goals" || *t == "places"));

    let mut stmt = a.conn.prepare("SELECT name FROM sqlite_master WHERE type = 'table'").unwrap();
    let tables: Vec<String> = stmt.query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect();
    for name in &tables {
        let synced = SYNCED.iter().any(|t| t.name == name);
        let local = aroh_store::local::LOCAL_TABLES.contains(&name.as_str()) || name == "sync_state";
        assert!(synced != local, "{name} must be synced or local, not both or neither");
    }
    for local in aroh_store::local::LOCAL_TABLES {
        assert!(tables.iter().any(|t| t == local), "{local} exists");
    }
}

#[test]
fn visits_older_than_90_days_are_deleted() {
    let conn = db::open_in_memory().unwrap();
    let now = Utc.with_ymd_and_hms(2026, 11, 22, 12, 0, 0).unwrap();
    for (id, arrived) in [("old", "2026-08-23T11:00:00Z"), ("edge", "2026-08-24T13:00:00Z"), ("new", "2026-11-21T10:00:00Z")] {
        conn.execute(
            "INSERT INTO visits (id, lat_e6, lon_e6, accuracy_m, arrived_at, left_at, place_id) VALUES (?1, 1, 1, 30, ?2, NULL, NULL)",
            [id, arrived],
        )
        .unwrap();
    }
    assert_eq!(aroh_store::local::prune_visits(&conn, now).unwrap(), 1);
    let mut stmt = conn.prepare("SELECT id FROM visits ORDER BY id").unwrap();
    let left: Vec<String> = stmt.query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect();
    assert_eq!(left, ["edge", "new"]);
    assert_eq!(aroh_store::local::prune_visits(&conn, now).unwrap(), 0);
}

#[tokio::test]
async fn places_and_links_converge_under_lww() {
    use aroh_domain::rows::{Place, PlaceLink};
    let server = Arc::new(Mutex::new(Server::default()));
    let mut a = Device::new("00000000000000aa", &server, 1_763_683_300_000);
    let mut b = Device::new("00000000000000bb", &server, 1_763_683_300_000);
    let mut place = samples::place();
    repo::put(&a.conn, &mut a.clock, &mut place).unwrap();
    let mut link: PlaceLink = serde_json::from_value(samples::all()[19].clone()).unwrap();
    repo::put(&a.conn, &mut a.clock, &mut link).unwrap();
    assert_eq!(a.sync().await.pushed, 2);
    assert_eq!(b.sync().await.pulled, 2);
    let mut on_a = repo::get::<Place>(&a.conn, &place.id).unwrap();
    on_a.name = "Old gym".into();
    repo::put(&a.conn, &mut a.clock, &mut on_a).unwrap();
    let mut on_b = repo::get::<Place>(&b.conn, &place.id).unwrap();
    on_b.name = "Gym".into();
    b.clock.set_wall_clock(1_763_683_500_000);
    repo::put(&b.conn, &mut b.clock, &mut on_b).unwrap();
    for _ in 0..2 {
        a.sync().await;
        b.sync().await;
    }
    assert_eq!(repo::get::<Place>(&a.conn, &place.id).unwrap().name, "Gym");
    assert_eq!(repo::get::<Place>(&b.conn, &place.id).unwrap(), repo::get::<Place>(&a.conn, &place.id).unwrap());
    assert_eq!(repo::get::<PlaceLink>(&b.conn, &link.id).unwrap().series_id, link.series_id);
}
