//! The device side of the notebook (Q96): pages and blocks sealed with the diary key,
//! day, action and goal pages found by a deterministic id, block saves diffed per block,
//! subpages, the local links index with backlinks, and an in-memory search over opened
//! text. Keys are passed in; nothing here stores them, logs text or calls the network.

use crate::diary::{account, Keyring};
use crate::{invalid, Result};
use aroh_diary::{DiaryKey, Sealed, Table};
use aroh_domain::hlc::{self, Hlc};
use aroh_domain::rows::{Action, Goal, Page, PageBlock, PageBlockHistory, PageKind};
use aroh_store::notebook as store;
use aroh_store::repo;
use chrono::{DateTime, NaiveDate, SecondsFormat, Utc};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use zeroize::Zeroizing;

/// Longest page title, in characters.
pub const TITLE_MAX: usize = 300;
/// Most blocks one page save may hold.
pub const BLOCKS_MAX: usize = 2000;
/// Longest block type and order key, in characters.
pub const KEY_MAX: usize = 128;
/// Longest search query, in characters.
pub const QUERY_MAX: usize = 200;
/// Most search hits returned.
pub const HITS_MAX: usize = 50;
/// Title shown for a page sealed with a key this device lacks.
pub const LOCKED_TITLE: &str = "Locked page";
const LOCKED: &str = "locked: this page was sealed with a diary key this phone lacks";
const LINK_KINDS: [&str; 4] = ["page", "day", "action", "goal"];
const LINK_TARGET_MAX: usize = 200;
const SNIPPET_BEFORE: usize = 40;
const SNIPPET_LEN: usize = 140;

/// One block as the editor sends and reads it: id, type, fractional order key and one
/// ProseMirror node, kept opaque.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub order: String,
    pub content: Value,
}

/// A subpage listed on its parent.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChildView {
    pub id: String,
    pub title: String,
}

/// A page that links to the open page.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BacklinkView {
    pub id: String,
    pub kind: PageKind,
    pub title: String,
}

/// One page for the editor. `target` is the date, action id or goal id, and the page's own
/// id for a free page; `updated_at` is the latest edit to the page or a block, RFC 3339 UTC.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PageView {
    pub id: String,
    pub kind: PageKind,
    pub target: String,
    pub title: String,
    pub parent_id: Option<String>,
    pub blocks: Vec<Block>,
    pub children: Vec<ChildView>,
    pub backlinks: Vec<BacklinkView>,
    pub updated_at: String,
}

/// One row of the page tree.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TreeRow {
    pub id: String,
    pub kind: PageKind,
    pub title: String,
    pub parent_id: Option<String>,
    pub updated_at: String,
}

/// One search hit with a short piece of the matching text.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SearchHit {
    pub id: String,
    pub kind: PageKind,
    pub title: String,
    pub snippet: String,
}

#[derive(Serialize, Deserialize)]
struct PagePayload {
    title: String,
    #[serde(rename = "ref")]
    reference: Option<String>,
}

#[derive(PartialEq, Serialize, Deserialize)]
struct BlockPayload {
    #[serde(rename = "type")]
    kind: String,
    order: String,
    content: Value,
}

/// The id of the day, action or goal page of `reference` for `user`: a uuid v5 of owner,
/// kind and ref, so two devices create the same row and the ref never shows in plaintext.
pub fn page_id(user: &str, kind: PageKind, reference: &str) -> String {
    const NAMESPACE: uuid::Uuid = uuid::Uuid::from_u128(0x3c51_7d0e_8a42_4f19_b6e3_91d0_2a7c_5e84);
    uuid::Uuid::new_v5(&NAMESPACE, format!("{user}/{}/{reference}", kind.as_str()).as_bytes()).hyphenated().to_string()
}

fn sealed_of(version: i64, nonce: &str, ciphertext: &str) -> Sealed {
    Sealed { key_version: version, nonce: nonce.to_string(), ciphertext: ciphertext.to_string() }
}

fn open(ring: &Keyring, user: &str, table: Table, row: &str, sealed: &Sealed) -> Option<Zeroizing<Vec<u8>>> {
    aroh_diary::open(ring.get(sealed.key_version)?, user, table, row, sealed).ok()
}

fn seal(key: &DiaryKey, user: &str, table: Table, row: &str, plain: &[u8]) -> Result<Sealed> {
    aroh_diary::seal(key, user, table, row, plain).map_err(|_| invalid("the page or block is too large to keep"))
}

fn open_page(ring: &Keyring, row: &Page) -> Option<PagePayload> {
    let plain = open(ring, &row.user_id, Table::Pages, &row.id, &sealed_of(row.key_version, &row.nonce, &row.ciphertext))?;
    serde_json::from_slice(&plain).ok()
}

fn open_block(ring: &Keyring, row: &PageBlock) -> Option<BlockPayload> {
    let plain = open(ring, &row.user_id, Table::PageBlocks, &row.id, &sealed_of(row.key_version, &row.nonce, &row.ciphertext))?;
    serde_json::from_slice(&plain).ok()
}

fn title_of(ring: &Keyring, row: &Page) -> String {
    open_page(ring, row).map(|p| p.title).unwrap_or_else(|| LOCKED_TITLE.to_string())
}

fn check_title(title: &str) -> Result<String> {
    let title = title.trim();
    if title.is_empty() || title.chars().count() > TITLE_MAX || title.chars().any(char::is_control) {
        return Err(invalid(format!("a page title is 1 to {TITLE_MAX} characters")));
    }
    Ok(title.to_string())
}

fn check_block(block: &Block) -> Result<()> {
    aroh_domain::rows::check_uuid("block id", &block.id)?;
    let key_ok = |s: &str, extra: &[char]| !s.is_empty() && s.chars().count() <= KEY_MAX && s.chars().all(|c| c.is_ascii_alphanumeric() || extra.contains(&c));
    if !key_ok(&block.kind, &['_', '-']) {
        return Err(invalid("a block type is 1 to 128 letters, digits, _ or -"));
    }
    if !key_ok(&block.order, &[]) {
        return Err(invalid("a block order is 1 to 128 base-62 characters"));
    }
    if !block.content.is_object() {
        return Err(invalid("a block's content is one ProseMirror node"));
    }
    Ok(())
}

fn seal_page(key: &DiaryKey, row: &mut Page, payload: &PagePayload) -> Result<()> {
    let plain = Zeroizing::new(serde_json::to_vec(payload).expect("a page payload serializes"));
    let sealed = seal(key, &row.user_id, Table::Pages, &row.id, &plain)?;
    (row.key_version, row.nonce, row.ciphertext) = (sealed.key_version, sealed.nonce, sealed.ciphertext);
    Ok(())
}

fn seal_block(key: &DiaryKey, row: &mut PageBlock, payload: &BlockPayload) -> Result<()> {
    let plain = Zeroizing::new(serde_json::to_vec(payload).expect("a block payload serializes"));
    let sealed = seal(key, &row.user_id, Table::PageBlocks, &row.id, &plain)?;
    (row.key_version, row.nonce, row.ciphertext) = (sealed.key_version, sealed.nonce, sealed.ciphertext);
    Ok(())
}

fn stamp_time(stamp: &str) -> String {
    let millis = hlc::parse(stamp).map(|(ms, _)| ms).unwrap_or(0);
    let at = DateTime::<Utc>::from_timestamp_millis(i64::try_from(millis).unwrap_or(0)).unwrap_or_default();
    at.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn block_stamps(conn: &Connection) -> Result<BTreeMap<String, String>> {
    let mut stmt = conn.prepare("SELECT page_id, max(updated_at) FROM page_blocks GROUP BY page_id")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn updated_at(row: &Page, stamps: &BTreeMap<String, String>) -> String {
    let newest = stamps.get(&row.id).filter(|s| **s > row.updated_at).unwrap_or(&row.updated_at);
    stamp_time(newest)
}

fn live_pages(conn: &Connection, user: &str) -> Result<Vec<Page>> {
    Ok(repo::list(conn, &[("user_id", user)])?)
}

fn stored_stamp(conn: &Connection, id: &str) -> Result<Option<(String, bool)>> {
    Ok(conn.query_row("SELECT updated_at, deleted FROM pages WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? != 0))).optional()?)
}

/// Parent of every live page, each walked up so a parent that is gone or a loop two
/// devices made by concurrent moves reads as a top-level page.
fn effective_parents(pages: &[Page]) -> BTreeMap<String, Option<String>> {
    let raw: BTreeMap<&str, Option<&str>> = pages.iter().map(|p| (p.id.as_str(), p.parent_id.as_deref())).collect();
    let mut out = BTreeMap::new();
    for page in pages {
        let mut parent = page.parent_id.as_deref().filter(|p| raw.contains_key(p));
        let mut seen = BTreeSet::from([page.id.as_str()]);
        let mut at = parent;
        while let Some(current) = at {
            if !seen.insert(current) {
                parent = None;
                break;
            }
            at = raw.get(current).copied().flatten().filter(|p| raw.contains_key(p));
        }
        assert!(seen.len() <= pages.len() + 1);
        out.insert(page.id.clone(), parent.map(str::to_string));
    }
    out
}

fn default_title(conn: &Connection, kind: PageKind, reference: &str) -> Result<String> {
    Ok(match kind {
        PageKind::Day => NaiveDate::parse_from_str(reference, "%Y-%m-%d").expect("checked before").format("%A, %B %-d, %Y").to_string(),
        PageKind::Action => repo::get::<Action>(conn, reference).map_err(|_| invalid("no such action"))?.title,
        PageKind::Goal => repo::get::<Goal>(conn, reference).map_err(|_| invalid("no such goal"))?.title,
        PageKind::Free => unreachable!("free pages are made by create"),
    })
}

fn check_reference(kind: PageKind, reference: &str) -> Result<()> {
    match kind {
        PageKind::Day => match NaiveDate::parse_from_str(reference, "%Y-%m-%d") {
            Ok(d) if d.format("%Y-%m-%d").to_string() == reference => Ok(()),
            _ => Err(invalid("a day page needs a YYYY-MM-DD date")),
        },
        PageKind::Action | PageKind::Goal => Ok(aroh_domain::rows::check_uuid("target", reference)?),
        PageKind::Free => Err(invalid("a free page is opened by its id")),
    }
}

/// Opens the page for `kind` and `target` (Q96 contract): kind `page` takes a page id of
/// any kind; `day`, `action` and `goal` take the date or id and create the page on first
/// open with the long date, action title or goal title. Every view rebuilds stale links.
/// Errors `Invalid` before sign-in, for a bad target, or `locked` without the key.
pub fn get(conn: &Connection, clock: &mut Hlc, ring: &Keyring, kind: &str, target: &str, now: DateTime<Utc>) -> Result<PageView> {
    let user = account(conn)?;
    let page_kind = match kind {
        "page" => return view(conn, ring, &user, target),
        "day" => PageKind::Day,
        "action" => PageKind::Action,
        "goal" => PageKind::Goal,
        _ => return Err(invalid("kind is page, day, action or goal")),
    };
    check_reference(page_kind, target)?;
    let id = page_id(&user, page_kind, target);
    let existing = stored_stamp(conn, &id)?;
    if existing.as_ref().is_none_or(|(_, deleted)| *deleted) {
        let title = default_title(conn, page_kind, target)?;
        let mut row = Page {
            id: id.clone(),
            user_id: user.clone(),
            kind: page_kind,
            parent_id: None,
            created_at: now,
            key_version: 0,
            nonce: String::new(),
            ciphertext: String::new(),
            updated_at: existing.map(|(stamp, _)| stamp).unwrap_or_default(),
            deleted: false,
        };
        seal_page(ring.newest(), &mut row, &PagePayload { title, reference: Some(target.to_string()) })?;
        repo::put(conn, clock, &mut row)?;
    }
    view(conn, ring, &user, &id)
}

/// Makes a free page titled `title` (blank becomes "Untitled") under `parent_id`.
/// Returns its view, or `Invalid` for a missing parent or a bad title.
pub fn create(conn: &Connection, clock: &mut Hlc, ring: &Keyring, parent_id: Option<&str>, title: &str, now: DateTime<Utc>) -> Result<PageView> {
    let user = account(conn)?;
    let title = if title.trim().is_empty() { "Untitled".to_string() } else { check_title(title)? };
    if let Some(parent) = parent_id {
        repo::get::<Page>(conn, parent).map_err(|_| invalid("no such parent page"))?;
    }
    let mut row = Page {
        id: aroh_domain::new_id(),
        user_id: user.clone(),
        kind: PageKind::Free,
        parent_id: parent_id.map(str::to_string),
        created_at: now,
        key_version: 0,
        nonce: String::new(),
        ciphertext: String::new(),
        updated_at: String::new(),
        deleted: false,
    };
    seal_page(ring.newest(), &mut row, &PagePayload { title, reference: None })?;
    repo::put(conn, clock, &mut row)?;
    view(conn, ring, &user, &row.id)
}

fn live_blocks(conn: &Connection, page_id: &str) -> Result<Vec<PageBlock>> {
    Ok(repo::list(conn, &[("page_id", page_id)])?)
}

/// Saves the full ordered `blocks` of page `page_id`: changed or new blocks are sealed and
/// written, unchanged ones keep their stamp, missing ones are tombstoned, and the page's
/// links are rebuilt, in one transaction. Refuses repeated ids, blocks of another page,
/// and a page with a block this device cannot open (it would be lost).
pub fn save(conn: &Connection, clock: &mut Hlc, ring: &Keyring, page_id: &str, blocks: Vec<Block>, now: DateTime<Utc>) -> Result<PageView> {
    let user = account(conn)?;
    if blocks.len() > BLOCKS_MAX {
        return Err(invalid(format!("a page holds at most {BLOCKS_MAX} blocks")));
    }
    let mut ids = BTreeSet::new();
    for block in &blocks {
        check_block(block)?;
        if !ids.insert(block.id.as_str()) {
            return Err(invalid("a block id appears twice"));
        }
    }
    let tx = conn.unchecked_transaction()?;
    repo::get::<Page>(&tx, page_id).map_err(|_| invalid("no such page"))?;
    let mut stored: BTreeMap<String, (PageBlock, BlockPayload)> = BTreeMap::new();
    for row in live_blocks(&tx, page_id)? {
        let payload = open_block(ring, &row).ok_or_else(|| invalid(LOCKED))?;
        stored.insert(row.id.clone(), (row, payload));
    }
    for block in blocks {
        let payload = BlockPayload { kind: block.kind, order: block.order, content: block.content };
        let mut row = match stored.remove(&block.id) {
            Some((_, old)) if old == payload => continue,
            Some((row, _)) => row,
            None => {
                let other: Option<(String, i64, String)> =
                    tx.query_row("SELECT page_id, deleted, updated_at FROM page_blocks WHERE id = ?1", [&block.id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).optional()?;
                if matches!(&other, Some((page, 0, _)) if page != page_id) {
                    return Err(invalid("a block belongs to another page"));
                }
                PageBlock {
                    id: block.id,
                    user_id: user.clone(),
                    page_id: page_id.to_string(),
                    created_at: now,
                    key_version: 0,
                    nonce: String::new(),
                    ciphertext: String::new(),
                    updated_at: other.map(|(_, _, stamp)| stamp).unwrap_or_default(),
                    deleted: false,
                }
            }
        };
        seal_block(ring.newest(), &mut row, &payload)?;
        repo::put(&tx, clock, &mut row)?;
    }
    for (id, _) in stored {
        repo::delete::<PageBlock>(&tx, clock, &id)?;
    }
    rebuild_links(&tx, ring, page_id)?;
    tx.commit()?;
    view(conn, ring, &user, page_id)
}

/// Renames page `page_id` to `title`, keeping its ref. Returns the view.
pub fn rename(conn: &Connection, clock: &mut Hlc, ring: &Keyring, page_id: &str, title: &str) -> Result<PageView> {
    let user = account(conn)?;
    let title = check_title(title)?;
    let mut row: Page = repo::get(conn, page_id).map_err(|_| invalid("no such page"))?;
    let payload = open_page(ring, &row).ok_or_else(|| invalid(LOCKED))?;
    seal_page(ring.newest(), &mut row, &PagePayload { title, reference: payload.reference })?;
    repo::put(conn, clock, &mut row)?;
    view(conn, ring, &user, page_id)
}

/// Moves page `page_id` under `parent_id`, or to the top with None. Refuses a missing
/// parent and a move under the page itself or one of its subpages (a cycle).
pub fn move_page(conn: &Connection, clock: &mut Hlc, ring: &Keyring, page_id: &str, parent_id: Option<&str>) -> Result<PageView> {
    let user = account(conn)?;
    let mut row: Page = repo::get(conn, page_id).map_err(|_| invalid("no such page"))?;
    if let Some(parent) = parent_id {
        let pages = live_pages(conn, &user)?;
        let raw: BTreeMap<&str, Option<&str>> = pages.iter().map(|p| (p.id.as_str(), p.parent_id.as_deref())).collect();
        if !raw.contains_key(parent) {
            return Err(invalid("no such parent page"));
        }
        let mut seen = BTreeSet::new();
        let mut at = Some(parent);
        while let Some(current) = at {
            if current == page_id {
                return Err(invalid("a page cannot move under itself or its subpages (a cycle)"));
            }
            if !seen.insert(current) {
                break;
            }
            at = raw.get(current).copied().flatten();
        }
    }
    row.parent_id = parent_id.map(str::to_string);
    repo::put(conn, clock, &mut row)?;
    view(conn, ring, &user, page_id)
}

/// Tombstones page `page_id`, every subpage under it, and their blocks and history, and
/// drops their links. Errors `Invalid` when the page is missing.
pub fn delete(conn: &Connection, clock: &mut Hlc, page_id: &str) -> Result<()> {
    let user = account(conn)?;
    let tx = conn.unchecked_transaction()?;
    repo::get::<Page>(&tx, page_id).map_err(|_| invalid("no such page"))?;
    let pages = live_pages(&tx, &user)?;
    let mut doomed = BTreeSet::from([page_id.to_string()]);
    loop {
        let more: Vec<String> = pages.iter().filter(|p| !doomed.contains(&p.id) && p.parent_id.as_ref().is_some_and(|q| doomed.contains(q))).map(|p| p.id.clone()).collect();
        if more.is_empty() {
            break;
        }
        doomed.extend(more);
    }
    assert!(doomed.len() <= pages.len());
    for id in &doomed {
        for block in live_blocks(&tx, id)? {
            repo::delete::<PageBlock>(&tx, clock, &block.id)?;
        }
        for old in repo::list::<PageBlockHistory>(&tx, &[("page_id", id.as_str())])? {
            repo::delete::<PageBlockHistory>(&tx, clock, &old.id)?;
        }
        repo::delete::<Page>(&tx, clock, id)?;
        tx.execute("DELETE FROM page_links WHERE page_id = ?1", [id])?;
        store::clear_dirty(&tx, id)?;
    }
    tx.commit()?;
    Ok(())
}

/// Every live page as a flat tree row: id, kind, title, parent (a lost parent or a loop
/// reads as top level) and the last edit time. Locked pages show `LOCKED_TITLE`.
pub fn tree(conn: &Connection, ring: &Keyring) -> Result<Vec<TreeRow>> {
    let user = account(conn)?;
    let pages = live_pages(conn, &user)?;
    let parents = effective_parents(&pages);
    let stamps = block_stamps(conn)?;
    Ok(pages
        .iter()
        .map(|p| TreeRow { id: p.id.clone(), kind: p.kind, title: title_of(ring, p), parent_id: parents[&p.id].clone(), updated_at: updated_at(p, &stamps) })
        .collect())
}

fn node_text(node: &Value, out: &mut String) {
    if let Some(text) = node.get("text").and_then(Value::as_str) {
        out.push_str(text);
    }
    if let Some(children) = node.get("content").and_then(Value::as_array) {
        for child in children {
            node_text(child, out);
        }
        out.push(' ');
    }
}

fn snippet(text: &str, at: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    let start_char = text[..at].chars().count().saturating_sub(SNIPPET_BEFORE);
    chars.iter().skip(start_char).take(SNIPPET_LEN).collect::<String>().trim().to_string()
}

/// Pages whose title or block text holds `query`, ignoring case: title hits first, then
/// the most recently edited, at most `HITS_MAX`. Scans opened text in memory only.
pub fn search(conn: &Connection, ring: &Keyring, query: &str) -> Result<Vec<SearchHit>> {
    let user = account(conn)?;
    let query = query.trim().to_lowercase();
    if query.is_empty() || query.chars().count() > QUERY_MAX {
        return Err(invalid(format!("a search is 1 to {QUERY_MAX} characters")));
    }
    let stamps = block_stamps(conn)?;
    let mut hits: Vec<(bool, String, SearchHit)> = Vec::new();
    for page in live_pages(conn, &user)? {
        let Some(payload) = open_page(ring, &page) else { continue };
        let mut body = String::new();
        let mut blocks: Vec<(String, String, String)> = Vec::new();
        for row in live_blocks(conn, &page.id)? {
            if let Some(block) = open_block(ring, &row) {
                let mut text = String::new();
                node_text(&block.content, &mut text);
                blocks.push((block.order, row.id, text));
            }
        }
        blocks.sort();
        for (_, _, text) in &blocks {
            body.push_str(text.trim());
            body.push(' ');
        }
        let title_hit = payload.title.to_lowercase().contains(&query);
        let lower = body.to_lowercase();
        let found = lower.find(&query).filter(|at| lower.len() == body.len() && body.is_char_boundary(*at));
        let piece = match found {
            Some(at) => snippet(&body, at),
            None if title_hit || lower.contains(&query) => snippet(&body, 0),
            None => continue,
        };
        hits.push((title_hit, updated_at(&page, &stamps), SearchHit { id: page.id.clone(), kind: page.kind, title: payload.title, snippet: piece }));
    }
    hits.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)).then_with(|| a.2.id.cmp(&b.2.id)));
    Ok(hits.into_iter().take(HITS_MAX).map(|(_, _, h)| h).collect())
}

/// The `pageLink` targets in one ProseMirror node, depth first, deduplicated:
/// `(target_kind, target)` pairs with a known kind and a non-empty target.
pub fn links(content: &Value) -> Vec<(String, String)> {
    fn walk(node: &Value, out: &mut Vec<(String, String)>) {
        if node.get("type").and_then(Value::as_str) == Some("pageLink") {
            let attrs = node.get("attrs");
            let kind = attrs.and_then(|a| a.get("target_kind")).and_then(Value::as_str).unwrap_or("");
            let target = attrs.and_then(|a| a.get("target")).and_then(Value::as_str).unwrap_or("");
            let pair = (kind.to_string(), target.to_string());
            if LINK_KINDS.contains(&kind) && !target.is_empty() && target.chars().count() <= LINK_TARGET_MAX && !out.contains(&pair) {
                out.push(pair);
            }
        }
        if let Some(children) = node.get("content").and_then(Value::as_array) {
            for child in children {
                walk(child, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(content, &mut out);
    out
}

/// Rebuilds the local links of page `page_id` from its live blocks and clears its dirty
/// mark. A missing or tombstoned page keeps no links. A block that does not open leaves
/// the page dirty for a later try.
fn rebuild_links(conn: &Connection, ring: &Keyring, page_id: &str) -> Result<()> {
    conn.execute("DELETE FROM page_links WHERE page_id = ?1", [page_id])?;
    if repo::get::<Page>(conn, page_id).is_err() {
        return store::clear_dirty(conn, page_id).map_err(Into::into);
    }
    let mut complete = true;
    for row in live_blocks(conn, page_id)? {
        let Some(block) = open_block(ring, &row) else {
            complete = false;
            continue;
        };
        for (kind, target) in links(&block.content) {
            conn.execute("INSERT OR IGNORE INTO page_links (page_id, target_kind, target) VALUES (?1, ?2, ?3)", [page_id, &kind, &target])?;
        }
    }
    if complete {
        store::clear_dirty(conn, page_id)?;
    } else {
        store::mark_dirty(conn, page_id)?;
    }
    Ok(())
}

/// Rebuilds the links of every page a sync marked dirty. Runs before each page read.
pub fn settle(conn: &Connection, ring: &Keyring) -> Result<()> {
    let dirty = store::dirty(conn)?;
    if dirty.is_empty() {
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;
    for id in dirty {
        rebuild_links(&tx, ring, &id)?;
    }
    tx.commit()?;
    Ok(())
}

fn view(conn: &Connection, ring: &Keyring, user: &str, id: &str) -> Result<PageView> {
    settle(conn, ring)?;
    let row: Page = repo::get(conn, id).map_err(|_| invalid("no such page"))?;
    let payload = open_page(ring, &row).ok_or_else(|| invalid(LOCKED))?;
    let mut blocks = Vec::new();
    for b in live_blocks(conn, id)? {
        let p = open_block(ring, &b).ok_or_else(|| invalid(LOCKED))?;
        blocks.push(Block { id: b.id, kind: p.kind, order: p.order, content: p.content });
    }
    blocks.sort_by(|a, b| (a.order.as_str(), a.id.as_str()).cmp(&(b.order.as_str(), b.id.as_str())));
    let pages = live_pages(conn, user)?;
    let parents = effective_parents(&pages);
    let by_id: BTreeMap<&str, &Page> = pages.iter().map(|p| (p.id.as_str(), p)).collect();
    let mut children: Vec<(DateTime<Utc>, ChildView)> = pages
        .iter()
        .filter(|p| parents[&p.id].as_deref() == Some(id))
        .map(|p| (p.created_at, ChildView { id: p.id.clone(), title: title_of(ring, p) }))
        .collect();
    children.sort_by(|a, b| (a.0, &a.1.id).cmp(&(b.0, &b.1.id)));
    let target = payload.reference.clone().unwrap_or_else(|| row.id.clone());
    let mut stmt = conn.prepare(
        "SELECT DISTINCT page_id FROM page_links WHERE page_id <> ?1 AND ((target_kind = 'page' AND target = ?1) OR (target_kind = ?2 AND target = ?3))",
    )?;
    let sources = stmt.query_map(rusqlite::params![id, row.kind.as_str(), target], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let mut backlinks: Vec<BacklinkView> = sources
        .iter()
        .filter_map(|s| by_id.get(s.as_str()))
        .map(|p| BacklinkView { id: p.id.clone(), kind: p.kind, title: title_of(ring, p) })
        .collect();
    backlinks.sort_by(|a, b| (a.title.to_lowercase(), &a.id).cmp(&(b.title.to_lowercase(), &b.id)));
    Ok(PageView {
        id: row.id.clone(),
        kind: row.kind,
        target,
        title: payload.title,
        parent_id: parents.get(id).cloned().flatten(),
        blocks,
        children: children.into_iter().map(|(_, c)| c).collect(),
        backlinks,
        updated_at: updated_at(&row, &block_stamps(conn)?),
    })
}

/// The kept older versions of block `block_id` that open with `ring`, newest first.
pub fn history(conn: &Connection, ring: &Keyring, block_id: &str) -> Result<Vec<Block>> {
    let mut rows: Vec<PageBlockHistory> = repo::list(conn, &[("block_id", block_id)])?;
    rows.sort_by(|a, b| (b.created_at, &b.id).cmp(&(a.created_at, &a.id)));
    Ok(rows
        .iter()
        .filter_map(|h| {
            let plain = open(ring, &h.user_id, Table::PageBlocks, &h.block_id, &sealed_of(h.key_version, &h.nonce, &h.ciphertext))?;
            let p: BlockPayload = serde_json::from_slice(&plain).ok()?;
            Some(Block { id: h.block_id.clone(), kind: p.kind, order: p.order, content: p.content })
        })
        .collect())
}

const SEALED_TABLES: [&str; 3] = ["pages", "page_blocks", "page_block_history"];

/// The newest key version any notebook row of the account uses, 0 for none, so a phone
/// never makes a fresh key while pages sealed with another one exist.
pub fn newest_version(conn: &Connection) -> Result<i64> {
    let user = account(conn)?;
    let mut top: i64 = 0;
    for table in SEALED_TABLES {
        let v: Option<i64> = conn.query_row(&format!("SELECT max(key_version) FROM {table} WHERE user_id = ?1"), [&user], |r| r.get(0))?;
        top = top.max(v.unwrap_or(0));
    }
    Ok(top)
}

/// How many live notebook rows of the account are still sealed with key `version`.
pub fn rows_at(conn: &Connection, version: i64) -> Result<usize> {
    let user = account(conn)?;
    let mut n: i64 = 0;
    for table in SEALED_TABLES {
        n += conn.query_row(&format!("SELECT count(*) FROM {table} WHERE user_id = ?1 AND key_version = ?2 AND deleted = 0"), rusqlite::params![user, version], |r| r.get::<_, i64>(0))?;
    }
    Ok(usize::try_from(n).expect("counts are non-negative"))
}

/// Re-encrypts every notebook row of the account at `old`'s version under `new`; history
/// rows keep their block's associated data. Returns how many rows moved.
pub fn rotate(conn: &Connection, clock: &mut Hlc, old: &DiaryKey, new: &DiaryKey) -> Result<usize> {
    assert!(new.version() > old.version(), "rotation moves to a newer key");
    let user = account(conn)?;
    let version = old.version().to_string();
    let filter = [("user_id", user.as_str()), ("key_version", version.as_str())];
    let move_one = |table: Table, id: &str, s: Sealed| aroh_diary::rotate(old, new, &user, table, id, &s).map_err(|e| invalid(e.to_string()));
    let mut moved = 0;
    for mut row in repo::list::<Page>(conn, &filter)? {
        let s = move_one(Table::Pages, &row.id, sealed_of(row.key_version, &row.nonce, &row.ciphertext))?;
        (row.key_version, row.nonce, row.ciphertext) = (s.key_version, s.nonce, s.ciphertext);
        repo::put(conn, clock, &mut row)?;
        moved += 1;
    }
    for mut row in repo::list::<PageBlock>(conn, &filter)? {
        let s = move_one(Table::PageBlocks, &row.id, sealed_of(row.key_version, &row.nonce, &row.ciphertext))?;
        (row.key_version, row.nonce, row.ciphertext) = (s.key_version, s.nonce, s.ciphertext);
        repo::put(conn, clock, &mut row)?;
        moved += 1;
    }
    for mut row in repo::list::<PageBlockHistory>(conn, &filter)? {
        let s = move_one(Table::PageBlocks, &row.block_id, sealed_of(row.key_version, &row.nonce, &row.ciphertext))?;
        (row.key_version, row.nonce, row.ciphertext) = (s.key_version, s.nonce, s.ciphertext);
        repo::put(conn, clock, &mut row)?;
        moved += 1;
    }
    Ok(moved)
}

/// Moves the notebook rows of every older key in `ring` to its newest key.
/// Returns how many rows moved.
pub fn resume_rotation(conn: &Connection, clock: &mut Hlc, ring: &Keyring) -> Result<usize> {
    let newest = ring.newest();
    let mut moved = 0;
    for old in ring.all().iter().filter(|k| k.version() < newest.version()) {
        moved += rotate(conn, clock, old, newest)?;
    }
    Ok(moved)
}
