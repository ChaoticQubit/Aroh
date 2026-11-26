use aroh_app::diary::Keyring;
use aroh_app::notebook::{self, Block};
use aroh_app::{local, sample};
use aroh_diary::{DiaryKey, Sealed, Table};
use aroh_domain::hlc::Hlc;
use aroh_domain::lww::{remote_wins, Version};
use aroh_domain::rows::{Action, Page, PageBlock, PageBlockHistory, PageKind};
use aroh_domain::schema::{Kind, TableDef, SYNCED};
use aroh_store::codec::WireRow;
use aroh_store::sync::{self, sync_all, Remote};
use aroh_store::{db, repo, Result};
use chrono::{DateTime, TimeZone, Utc};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::future::{ready, Future};
use std::sync::{Arc, Mutex};

const ACCOUNT: &str = "0190f2a0-0000-7000-8000-0000000000ac";
const OTHER: &str = "0190f2a0-0000-7000-8000-0000000000ad";
const DAY: &str = "2026-11-26";
const SECRET_TITLE: &str = "PAGE-SENTINEL trip to Lisbon";
const SECRET_TEXT: &str = "BLOCK-SENTINEL call the landlord";
const WALL: u64 = 1_764_000_000_000;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 26, 20, 15, 0).unwrap()
}

fn device(node: &str, wall: u64) -> (Connection, Hlc) {
    let mut conn = db::open_in_memory().unwrap();
    local::adopt(&mut conn, ACCOUNT).unwrap();
    (conn, Hlc::with_wall_clock(node, wall))
}

fn seeded() -> (Connection, Hlc) {
    let mut conn = db::open_in_memory().unwrap();
    let user = local::user_id(&conn).unwrap();
    let mut clock = Hlc::with_wall_clock("00000000000000d1", WALL);
    assert!(sample::seed(&conn, &mut clock, &user, now(), chrono_tz::Europe::Berlin).unwrap());
    local::adopt(&mut conn, ACCOUNT).unwrap();
    (conn, clock)
}

fn ring() -> Keyring {
    Keyring::new(vec![DiaryKey::from_bytes(1, &[7u8; 32]).unwrap()]).unwrap()
}

fn uuid(n: u32) -> String {
    format!("0190f2a0-0000-7000-8000-{n:012x}")
}

fn para(id: &str, order: &str, text: &str) -> Block {
    Block { id: id.into(), kind: "paragraph".into(), order: order.into(), content: json!({"type": "paragraph", "content": [{"type": "text", "text": text}]}) }
}

fn link(id: &str, order: &str, kind: &str, target: &str) -> Block {
    Block {
        id: id.into(),
        kind: "paragraph".into(),
        order: order.into(),
        content: json!({"type": "paragraph", "content": [{"type": "text", "text": "see "}, {"type": "pageLink", "attrs": {"target_kind": kind, "target": target}}]}),
    }
}

fn texts(blocks: &[Block]) -> Vec<String> {
    blocks.iter().map(|b| b.content["content"][0]["text"].as_str().unwrap_or_default().to_string()).collect()
}

#[test]
fn seal_open_round_trip_and_wrong_owner_refusal() {
    let (conn, mut clock) = device("00000000000000a1", WALL);
    let keys = ring();
    let page = notebook::create(&conn, &mut clock, &keys, None, SECRET_TITLE, now()).unwrap();
    let row: Page = repo::get(&conn, &page.id).unwrap();
    let sealed = Sealed { key_version: row.key_version, nonce: row.nonce.clone(), ciphertext: row.ciphertext.clone() };
    let plain = aroh_diary::open(keys.newest(), ACCOUNT, Table::Pages, &row.id, &sealed).unwrap();
    assert!(String::from_utf8(plain.to_vec()).unwrap().contains(SECRET_TITLE));
    assert_eq!(aroh_diary::open(keys.newest(), OTHER, Table::Pages, &row.id, &sealed), Err(aroh_diary::Error::Crypto));
    assert_eq!(aroh_diary::open(keys.newest(), ACCOUNT, Table::PageBlocks, &row.id, &sealed), Err(aroh_diary::Error::Crypto));
    assert_eq!(notebook::get(&conn, &mut clock, &keys, "page", &page.id, now()).unwrap().title, SECRET_TITLE);
    let stranger = Keyring::new(vec![DiaryKey::from_bytes(1, &[9u8; 32]).unwrap()]).unwrap();
    assert!(notebook::get(&conn, &mut clock, &stranger, "page", &page.id, now()).unwrap_err().to_string().contains("locked"));
}

#[test]
fn notebook_refuses_without_an_account() {
    let conn = db::open_in_memory().unwrap();
    let mut clock = Hlc::with_wall_clock("00000000000000a1", WALL);
    assert!(notebook::create(&conn, &mut clock, &ring(), None, "x", now()).unwrap_err().to_string().contains("sign in"));
}

#[test]
fn deterministic_day_page_id() {
    let id = notebook::page_id(ACCOUNT, PageKind::Day, DAY);
    assert_eq!(id, notebook::page_id(ACCOUNT, PageKind::Day, DAY));
    assert_ne!(id, notebook::page_id(ACCOUNT, PageKind::Day, "2026-11-27"));
    assert_ne!(id, notebook::page_id(OTHER, PageKind::Day, DAY));
    assert_ne!(id, notebook::page_id(ACCOUNT, PageKind::Goal, DAY));
    let (a, mut ca) = device("00000000000000a1", WALL);
    let (b, mut cb) = device("00000000000000b2", WALL);
    let on_a = notebook::get(&a, &mut ca, &ring(), "day", DAY, now()).unwrap();
    let on_b = notebook::get(&b, &mut cb, &ring(), "day", DAY, now()).unwrap();
    assert_eq!((on_a.id.clone(), on_b.id), (id.clone(), id.clone()));
    assert_eq!((on_a.kind, on_a.target.as_str(), on_a.title.as_str()), (PageKind::Day, DAY, "Thursday, November 26, 2026"));
    assert_eq!(notebook::get(&a, &mut ca, &ring(), "day", DAY, now()).unwrap().id, id, "a second get finds the same page");
    assert_eq!(repo::list::<Page>(&a, &[]).unwrap().len(), 1);
    assert!(notebook::get(&a, &mut ca, &ring(), "day", "2026-02-30", now()).is_err());
}

#[test]
fn action_and_goal_pages_take_their_titles() {
    let (conn, mut clock) = seeded();
    let action = repo::list::<Action>(&conn, &[]).unwrap().remove(0);
    let page = notebook::get(&conn, &mut clock, &ring(), "action", &action.id, now()).unwrap();
    assert_eq!((page.kind, page.target.clone(), page.title.clone()), (PageKind::Action, action.id.clone(), action.title.clone()));
    let goal: aroh_domain::rows::Goal = repo::get(&conn, &action.goal_id).unwrap();
    let page = notebook::get(&conn, &mut clock, &ring(), "goal", &goal.id, now()).unwrap();
    assert_eq!((page.kind, page.title), (PageKind::Goal, goal.title));
    assert!(notebook::get(&conn, &mut clock, &ring(), "goal", &uuid(999), now()).is_err());
}

#[test]
fn save_diffs_blocks_and_keeps_unchanged_stamps() {
    let (conn, mut clock) = device("00000000000000a1", WALL);
    let keys = ring();
    let page = notebook::create(&conn, &mut clock, &keys, None, "Plans", now()).unwrap();
    let view = notebook::save(&conn, &mut clock, &keys, &page.id, vec![para(&uuid(2), "a1", "second"), para(&uuid(1), "a0", "first")], now()).unwrap();
    assert_eq!(texts(&view.blocks), vec!["first", "second"], "blocks come back in order");
    let stamp = |id: &str| repo::get::<PageBlock>(&conn, id).unwrap().updated_at;
    let before = (stamp(&uuid(1)), stamp(&uuid(2)));
    notebook::save(&conn, &mut clock, &keys, &page.id, vec![para(&uuid(1), "a0", "first"), para(&uuid(2), "a1", "second, edited")], now()).unwrap();
    assert_eq!(stamp(&uuid(1)), before.0, "an unchanged block is not rewritten");
    assert_ne!(stamp(&uuid(2)), before.1);
    let view = notebook::save(&conn, &mut clock, &keys, &page.id, vec![para(&uuid(2), "a1", "second, edited")], now()).unwrap();
    assert_eq!(texts(&view.blocks), vec!["second, edited"]);
    assert!(repo::get::<PageBlock>(&conn, &uuid(1)).is_err(), "a removed block is tombstoned");
    assert!(notebook::save(&conn, &mut clock, &keys, &page.id, vec![para(&uuid(3), "a0", "x"), para(&uuid(3), "a1", "y")], now()).is_err());
    assert!(notebook::save(&conn, &mut clock, &keys, &page.id, vec![para("not-a-uuid", "a0", "x")], now()).is_err());
    let other = notebook::create(&conn, &mut clock, &keys, None, "Other", now()).unwrap();
    assert!(notebook::save(&conn, &mut clock, &keys, &other.id, vec![para(&uuid(2), "a0", "stolen")], now()).is_err(), "a block of another page is refused");
}

#[test]
fn links_and_backlinks() {
    let (conn, mut clock) = device("00000000000000a1", WALL);
    let keys = ring();
    let target = notebook::create(&conn, &mut clock, &keys, None, "Target", now()).unwrap();
    let source = notebook::create(&conn, &mut clock, &keys, None, "Source", now()).unwrap();
    let blocks = vec![link(&uuid(1), "a0", "page", &target.id), link(&uuid(2), "a1", "day", DAY), para(&uuid(3), "a2", "plain")];
    notebook::save(&conn, &mut clock, &keys, &source.id, blocks, now()).unwrap();
    let extracted = notebook::links(&json!({"type": "bulletList", "content": [{"type": "listItem", "content": [{"type": "pageLink", "attrs": {"target_kind": "goal", "target": "g1"}}]}, {"type": "pageLink", "attrs": {"target_kind": "nope", "target": "x"}}]}));
    assert_eq!(extracted, vec![("goal".to_string(), "g1".to_string())]);
    let view = notebook::get(&conn, &mut clock, &keys, "page", &target.id, now()).unwrap();
    assert_eq!(view.backlinks.iter().map(|b| (b.id.clone(), b.kind, b.title.clone())).collect::<Vec<_>>(), vec![(source.id.clone(), PageKind::Free, "Source".to_string())]);
    let day = notebook::get(&conn, &mut clock, &keys, "day", DAY, now()).unwrap();
    assert_eq!(day.backlinks.iter().map(|b| b.id.clone()).collect::<Vec<_>>(), vec![source.id.clone()]);
    notebook::save(&conn, &mut clock, &keys, &source.id, vec![para(&uuid(3), "a2", "plain")], now()).unwrap();
    assert!(notebook::get(&conn, &mut clock, &keys, "page", &target.id, now()).unwrap().backlinks.is_empty(), "a removed link drops the backlink");
}

#[test]
fn move_rejects_cycles_and_delete_cascades() {
    let (conn, mut clock) = device("00000000000000a1", WALL);
    let keys = ring();
    let a = notebook::create(&conn, &mut clock, &keys, None, "A", now()).unwrap();
    let b = notebook::create(&conn, &mut clock, &keys, Some(&a.id), "B", now()).unwrap();
    let c = notebook::create(&conn, &mut clock, &keys, Some(&b.id), "C", now()).unwrap();
    notebook::save(&conn, &mut clock, &keys, &c.id, vec![para(&uuid(1), "a0", "deep")], now()).unwrap();
    assert!(notebook::move_page(&conn, &mut clock, &keys, &a.id, Some(&c.id)).unwrap_err().to_string().contains("cycle"));
    assert!(notebook::move_page(&conn, &mut clock, &keys, &a.id, Some(&a.id)).is_err());
    assert_eq!(notebook::get(&conn, &mut clock, &keys, "page", &a.id, now()).unwrap().children.iter().map(|c| c.title.clone()).collect::<Vec<_>>(), vec!["B"]);
    let moved = notebook::move_page(&conn, &mut clock, &keys, &c.id, None).unwrap();
    assert_eq!(moved.parent_id, None);
    notebook::move_page(&conn, &mut clock, &keys, &c.id, Some(&b.id)).unwrap();
    notebook::delete(&conn, &mut clock, &a.id).unwrap();
    assert!(notebook::tree(&conn, &keys).unwrap().is_empty());
    assert!(repo::get::<PageBlock>(&conn, &uuid(1)).is_err(), "subpage blocks are tombstoned");
    assert!(notebook::get(&conn, &mut clock, &keys, "page", &c.id, now()).is_err());
}

#[test]
fn tree_and_search() {
    let (conn, mut clock) = device("00000000000000a1", WALL);
    let keys = ring();
    let a = notebook::create(&conn, &mut clock, &keys, None, "Lisbon trip", now()).unwrap();
    let b = notebook::create(&conn, &mut clock, &keys, Some(&a.id), "Packing", now()).unwrap();
    notebook::save(&conn, &mut clock, &keys, &b.id, vec![para(&uuid(1), "a0", "Bring the Lisbon guide and a charger")], now()).unwrap();
    let tree = notebook::tree(&conn, &keys).unwrap();
    assert_eq!(tree.len(), 2);
    assert!(tree.iter().any(|r| r.id == b.id && r.parent_id.as_deref() == Some(a.id.as_str()) && r.title == "Packing"));
    assert!(DateTime::parse_from_rfc3339(&tree[0].updated_at).is_ok());
    let hits = notebook::search(&conn, &keys, "lisbon").unwrap();
    assert_eq!(hits.iter().map(|h| h.id.clone()).collect::<Vec<_>>(), vec![a.id.clone(), b.id.clone()], "title hits come first");
    assert!(hits[1].snippet.contains("Lisbon guide"));
    assert!(notebook::search(&conn, &keys, "charger").unwrap().iter().all(|h| h.id == b.id));
    assert!(notebook::search(&conn, &keys, "absent words").unwrap().is_empty());
}

#[test]
fn nothing_plaintext_in_a_synced_row() {
    let (conn, mut clock) = device("00000000000000a1", WALL);
    let keys = ring();
    let page = notebook::create(&conn, &mut clock, &keys, None, SECRET_TITLE, now()).unwrap();
    notebook::save(&conn, &mut clock, &keys, &page.id, vec![para(&uuid(1), "a0", SECRET_TEXT), link(&uuid(2), "a1", "day", "2027-03-14")], now()).unwrap();
    notebook::get(&conn, &mut clock, &keys, "day", "2027-03-14", now()).unwrap();
    let mut seen = 0;
    for table in SYNCED.iter() {
        let rows = sync::pending(&conn, table, ACCOUNT, clock.node(), "", 500).unwrap();
        seen += rows.len();
        let wire = serde_json::to_string(&rows).unwrap();
        for secret in ["PAGE-SENTINEL", "BLOCK-SENTINEL", "Lisbon", "landlord", "2027-03-14", "Sunday", "paragraph", "pageLink"] {
            assert!(!wire.contains(secret), "{} would sync {secret} in plaintext", table.name);
        }
    }
    assert!(seen >= 4, "the page, the day page and both blocks are pending");
}

#[derive(Default)]
struct Server {
    seq: i64,
    rows: BTreeMap<(&'static str, String), (i64, WireRow)>,
}

struct MemoryRemote(Arc<Mutex<Server>>);

fn version(row: &WireRow) -> Version {
    Version { updated_at: row["updated_at"].as_str().unwrap().to_string(), deleted: row["deleted"].as_bool().unwrap() }
}

impl Remote for MemoryRemote {
    fn push(&mut self, table: &'static TableDef, rows: &[WireRow]) -> impl Future<Output = Result<()>> + Send {
        let mut server = self.0.lock().unwrap();
        for row in rows {
            let key = (table.name, row["id"].as_str().unwrap().to_string());
            if remote_wins(server.rows.get(&key).map(|(_, r)| version(r)).as_ref(), &version(row)) {
                server.seq += 1;
                let seq = server.seq;
                server.rows.insert(key, (seq, row.clone()));
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
                for column in table.columns.iter().filter(|c| c.kind == Kind::Time) {
                    if let Some(text) = row[column.name].as_str() {
                        row.insert(column.name.into(), text.replace('Z', "+00:00").into());
                    }
                }
                row.insert("server_seq".into(), seq.into());
                row
            })
            .collect();
        ready(Ok(page))
    }
}

fn sync_device(conn: &mut Connection, clock: &mut Hlc, server: &Arc<Mutex<Server>>) {
    let mut remote = MemoryRemote(server.clone());
    tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(sync_all(conn, &mut remote, ACCOUNT, clock)).unwrap();
}

fn block_text(view: &notebook::PageView, id: &str) -> String {
    view.blocks.iter().find(|b| b.id == id).map(|b| b.content["content"][0]["text"].as_str().unwrap().to_string()).unwrap_or_default()
}

#[test]
fn per_block_merge_keeps_disjoint_edits_and_history_keeps_the_loser() {
    let server = Arc::new(Mutex::new(Server::default()));
    let keys = ring();
    let (mut a, mut ca) = device("00000000000000a1", WALL);
    let (mut b, mut cb) = device("00000000000000b2", WALL);
    let page = notebook::get(&a, &mut ca, &keys, "day", DAY, now()).unwrap();
    notebook::save(&a, &mut ca, &keys, &page.id, vec![para(&uuid(1), "a0", "one"), para(&uuid(2), "a1", "two")], now()).unwrap();
    sync_device(&mut a, &mut ca, &server);
    sync_device(&mut b, &mut cb, &server);
    let on_b = notebook::get(&b, &mut cb, &keys, "day", DAY, now()).unwrap();
    assert_eq!(texts(&on_b.blocks), vec!["one", "two"]);

    ca.set_wall_clock(WALL + 1_000);
    cb.set_wall_clock(WALL + 2_000);
    notebook::save(&a, &mut ca, &keys, &page.id, vec![para(&uuid(1), "a0", "one by A"), para(&uuid(2), "a1", "two")], now()).unwrap();
    notebook::save(&b, &mut cb, &keys, &page.id, vec![para(&uuid(1), "a0", "one"), para(&uuid(2), "a1", "two by B")], now()).unwrap();
    sync_device(&mut a, &mut ca, &server);
    sync_device(&mut b, &mut cb, &server);
    sync_device(&mut a, &mut ca, &server);
    for (conn, clock) in [(&a, &mut ca), (&b, &mut cb)] {
        let view = notebook::get(conn, clock, &keys, "day", DAY, now()).unwrap();
        assert_eq!(texts(&view.blocks), vec!["one by A", "two by B"], "edits to different blocks both survive");
    }
    let kept = |conn: &Connection, id: &str| notebook::history(conn, &keys, id).unwrap().iter().map(|h| h.content["content"][0]["text"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    assert_eq!(kept(&a, &uuid(2)), vec!["two"], "a version this device wrote that another device replaced is kept");
    assert!(kept(&b, &uuid(2)).is_empty() && kept(&a, &uuid(1)).is_empty());

    ca.set_wall_clock(WALL + 3_000);
    cb.set_wall_clock(WALL + 4_000);
    notebook::save(&a, &mut ca, &keys, &page.id, vec![para(&uuid(1), "a0", "A wins?"), para(&uuid(2), "a1", "two by B")], now()).unwrap();
    notebook::save(&b, &mut cb, &keys, &page.id, vec![para(&uuid(1), "a0", "B is newer"), para(&uuid(2), "a1", "two by B")], now()).unwrap();
    sync_device(&mut a, &mut ca, &server);
    sync_device(&mut b, &mut cb, &server);
    sync_device(&mut a, &mut ca, &server);
    sync_device(&mut b, &mut cb, &server);
    sync_device(&mut a, &mut ca, &server);
    sync_device(&mut b, &mut cb, &server);
    for (conn, clock) in [(&a, &mut ca), (&b, &mut cb)] {
        let view = notebook::get(conn, clock, &keys, "day", DAY, now()).unwrap();
        assert_eq!(block_text(&view, &uuid(1)), "B is newer", "the newest HLC wins the same block");
    }
    for conn in [&a, &b] {
        let history = notebook::history(conn, &keys, &uuid(1)).unwrap();
        assert_eq!(history.iter().map(|h| h.content["content"][0]["text"].as_str().unwrap().to_string()).collect::<Vec<_>>(), vec!["A wins?"], "history keeps the loser");
    }
    assert_eq!(repo::list::<PageBlockHistory>(&b, &[("block_id", uuid(1).as_str())]).unwrap().len(), 1, "the loser is kept once");
}

#[test]
fn history_is_capped_per_block() {
    let (conn, mut clock) = device("00000000000000a1", WALL);
    let keys = ring();
    let page = notebook::create(&conn, &mut clock, &keys, None, "Capped", now()).unwrap();
    for i in 0..25u64 {
        clock.set_wall_clock(WALL + i * 1_000);
        notebook::save(&conn, &mut clock, &keys, &page.id, vec![para(&uuid(1), "a0", &format!("v{i}"))], now()).unwrap();
        let block: PageBlock = repo::get(&conn, &uuid(1)).unwrap();
        aroh_store::notebook::keep_history(&conn, &mut clock, &block).unwrap();
    }
    let history = notebook::history(&conn, &keys, &uuid(1)).unwrap();
    assert_eq!(history.len(), aroh_store::notebook::HISTORY_KEEP);
    assert_eq!(history[0].content["content"][0]["text"], Value::from("v24"), "newest first");
}

#[test]
fn links_rebuild_after_sync_and_a_page_delete_keeps_no_history() {
    let server = Arc::new(Mutex::new(Server::default()));
    let keys = ring();
    let (mut a, mut ca) = device("00000000000000a1", WALL);
    let (mut b, mut cb) = device("00000000000000b2", WALL + 500);
    let target = notebook::create(&a, &mut ca, &keys, None, "Target", now()).unwrap();
    sync_device(&mut a, &mut ca, &server);
    sync_device(&mut b, &mut cb, &server);
    let source = notebook::create(&b, &mut cb, &keys, None, "Source", now()).unwrap();
    notebook::save(&b, &mut cb, &keys, &source.id, vec![link(&uuid(1), "a0", "page", &target.id), para(&uuid(2), "a1", "kept by B")], now()).unwrap();
    sync_device(&mut b, &mut cb, &server);
    sync_device(&mut a, &mut ca, &server);
    let view = notebook::get(&a, &mut ca, &keys, "page", &target.id, now()).unwrap();
    assert_eq!(view.backlinks.iter().map(|l| l.id.clone()).collect::<Vec<_>>(), vec![source.id.clone()], "links are rebuilt after a sync");

    ca.set_wall_clock(WALL + 5_000);
    notebook::delete(&a, &mut ca, &source.id).unwrap();
    sync_device(&mut a, &mut ca, &server);
    sync_device(&mut b, &mut cb, &server);
    assert!(notebook::get(&b, &mut cb, &keys, "page", &source.id, now()).is_err(), "the delete reached B");
    assert!(repo::list::<PageBlockHistory>(&b, &[]).unwrap().is_empty(), "a page delete keeps no history");
    assert!(notebook::get(&b, &mut cb, &keys, "page", &target.id, now()).unwrap().backlinks.is_empty(), "the deleted page's links are gone");
}
