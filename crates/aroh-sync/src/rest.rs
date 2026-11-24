use crate::config::Config;
use crate::{check, http};
use aroh_domain::schema::TableDef;
use aroh_store::codec::WireRow;
use aroh_store::sync::Remote;
use std::future::Future;

/// The Supabase side of `aroh_store::sync`: PostgREST calls authed with the
/// signed-in user's JWT, so row-level security limits every read and write
/// to that user. The `aroh_lww` trigger keeps the newer stamp on upsert.
pub struct PgRest {
    client: reqwest::Client,
    config: Config,
    token: String,
}

impl PgRest {
    /// Builds the remote for one sync. Input: the project and a valid
    /// access token (`auth::valid_session`). The token lives only here.
    pub fn new(config: Config, token: String) -> PgRest {
        assert!(!token.is_empty(), "sync needs an access token");
        PgRest { client: http(), config, token }
    }
}

/// The upsert URL of `table`: conflict on the `(user_id, id)` primary key.
/// Input: the project origin and the table. Output: the URL.
pub fn push_url(base: &str, table: &TableDef) -> String {
    format!("{base}/rest/v1/{}?on_conflict=user_id,id", table.name)
}

/// The page URL of `table`: rows with `server_seq` above `after`, oldest
/// first, at most `limit`. Input: origin, table, cursor, page size.
pub fn pull_url(base: &str, table: &TableDef, after: i64, limit: usize) -> String {
    assert!(after >= 0 && limit > 0);
    format!("{base}/rest/v1/{}?select=*&server_seq=gt.{after}&order=server_seq.asc&limit={limit}", table.name)
}

fn remote_error(e: crate::Error) -> aroh_store::Error {
    aroh_store::Error::Remote(e.to_string())
}

impl Remote for PgRest {
    fn push(&mut self, table: &'static TableDef, rows: &[WireRow]) -> impl Future<Output = aroh_store::Result<()>> + Send {
        let request = self
            .client
            .post(push_url(&self.config.url, table))
            .header("apikey", &self.config.anon_key)
            .bearer_auth(&self.token)
            .header("Prefer", "resolution=merge-duplicates,return=minimal")
            .json(rows);
        async move {
            let response = request.send().await.map_err(|e| remote_error(e.into()))?;
            check(response).await.map_err(remote_error)?;
            Ok(())
        }
    }

    fn pull(&mut self, table: &'static TableDef, after: i64, limit: usize) -> impl Future<Output = aroh_store::Result<Vec<WireRow>>> + Send {
        let request = self
            .client
            .get(pull_url(&self.config.url, table, after, limit))
            .header("apikey", &self.config.anon_key)
            .bearer_auth(&self.token);
        async move {
            let response = request.send().await.map_err(|e| remote_error(e.into()))?;
            let rows: Vec<WireRow> = check(response).await.map_err(remote_error)?.json().await.map_err(|e| remote_error(e.into()))?;
            Ok(rows)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_pull_urls() {
        let goals = aroh_domain::schema::table("goals").unwrap();
        assert_eq!(push_url("http://localhost:54321", goals), "http://localhost:54321/rest/v1/goals?on_conflict=user_id,id");
        assert_eq!(
            pull_url("https://x.supabase.co", goals, 41, 500),
            "https://x.supabase.co/rest/v1/goals?select=*&server_seq=gt.41&order=server_seq.asc&limit=500"
        );
    }
}
