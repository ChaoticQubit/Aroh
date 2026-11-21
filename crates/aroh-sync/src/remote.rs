use rusqlite::Connection;

use crate::auth::{check, http};
use crate::config::Config;
use crate::store::{self, Row, Table, TABLES};
use crate::Error;

/// Rows per push batch and per pull page.
pub const PAGE: usize = 500;

/// Counts of rows one sync moved.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Moved {
    pub pushed: usize,
    pub pulled: usize,
}

/// Pushes then pulls every synced table.
/// Input: the open database, the project config, a valid GoTrue access
/// token and a callback receiving every remote HLC.
/// Output: rows pushed and rows written locally.
pub async fn sync_all(
    conn: &mut Connection,
    config: &Config,
    token: &str,
    on_remote: &mut (dyn FnMut(&str) + Send),
) -> Result<Moved, Error> {
    assert!(!token.is_empty());
    store::ensure_state(conn)?;
    let client = http();
    let mut moved = Moved::default();
    for table in &TABLES {
        moved.pushed += push(conn, &client, config, token, table).await?;
    }
    for table in &TABLES {
        moved.pulled += pull(conn, &client, config, token, table, on_remote).await?;
    }
    Ok(moved)
}

async fn push(
    conn: &mut Connection,
    client: &reqwest::Client,
    config: &Config,
    token: &str,
    table: &Table,
) -> Result<usize, Error> {
    let key = format!("push:{}", table.name);
    let mut pushed = 0;
    loop {
        let cursor = store::get_state(conn, &key)?.unwrap_or_default();
        let rows = store::pending(conn, table, &cursor, PAGE)?;
        let Some(last) = rows.last().and_then(|r| r["updated_at"].as_str()).map(str::to_string) else {
            return Ok(pushed);
        };
        assert!(last > cursor);
        let response = client
            .post(format!("{}/rest/v1/{}?on_conflict=user_id,{}", config.url, table.name, table.pk))
            .header("apikey", &config.anon_key)
            .bearer_auth(token)
            .header("Prefer", "resolution=merge-duplicates,return=minimal")
            .json(&rows)
            .send()
            .await?;
        check(response).await?;
        store::set_state(conn, &key, &last)?;
        pushed += rows.len();
        if rows.len() < PAGE {
            return Ok(pushed);
        }
    }
}

async fn pull(
    conn: &mut Connection,
    client: &reqwest::Client,
    config: &Config,
    token: &str,
    table: &Table,
    on_remote: &mut (dyn FnMut(&str) + Send),
) -> Result<usize, Error> {
    let key = format!("pull:{}", table.name);
    let mut pulled = 0;
    loop {
        let cursor: i64 = match store::get_state(conn, &key)? {
            Some(v) => v.parse().map_err(|_| Error::InvalidRow(format!("bad cursor {key}")))?,
            None => 0,
        };
        let response = client
            .get(format!(
                "{}/rest/v1/{}?select=*&server_seq=gt.{cursor}&order=server_seq.asc&limit={PAGE}",
                config.url, table.name
            ))
            .header("apikey", &config.anon_key)
            .bearer_auth(token)
            .send()
            .await?;
        let rows: Vec<Row> = check(response).await?.json().await?;
        if rows.len() > PAGE {
            return Err(Error::InvalidRow("page larger than requested".into()));
        }
        let Some(last) = rows.last().and_then(|r| r.get("server_seq")).and_then(|s| s.as_i64()) else {
            return Ok(pulled);
        };
        if last <= cursor {
            return Err(Error::InvalidRow("server_seq did not advance".into()));
        }
        pulled += store::apply_page(conn, table, &rows, last, on_remote)?;
        if rows.len() < PAGE {
            return Ok(pulled);
        }
    }
}
