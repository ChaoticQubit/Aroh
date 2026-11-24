//! Per-user limits of the hosted server (Q75), consent to send data to the AI provider,
//! and account deletion (App Store 5.1.1(v)). The database enforces the goal and file caps
//! for direct writes too; this module gives the engine the same answers before it writes.

use crate::db::{self, PgPool};
use crate::error::ServerError;
use crate::workflows;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use troupekit_core::http::{read_limited, HttpRequest, Method, Transport};
use utoipa::ToSchema;
use uuid::Uuid;

/// Most goals a user may hold open (draft, active or paused) at once.
pub const MAX_OPEN_GOALS: i64 = 5;

/// Most agent runs a user may start in any 24 hours.
pub const MAX_RUNS_PER_DAY: i64 = 10;

/// The workflows the owner starts by hand, the only runs `MAX_RUNS_PER_DAY` counts
/// and refuses. System runs (fill, reviews, triggers, learning, log intake, replans)
/// never count and are never refused by the cap.
pub const OWNER_STARTED: [&str; 3] = [workflows::NEW_GOAL, workflows::RUN_AGAIN, workflows::CARD_CHAT];

/// Largest single attachment, in bytes; the bucket and `goal_files_quota` hold the same.
pub const MAX_FILE_BYTES: i64 = 10 * 1024 * 1024;

/// Most stored attachments per user; the storage insert policy holds the same.
pub const MAX_FILES: i64 = 20;

/// Most declared attachment bytes per user; `goal_files_quota` holds the same.
pub const MAX_FILE_TOTAL_BYTES: i64 = 50 * 1024 * 1024;

/// Most object names one Storage API removal names.
pub const REMOVE_BATCH: usize = 1000;

/// The built-in Routines goal (`aroh_domain::seed::routines_id`), which never counts
/// toward `MAX_OPEN_GOALS`; the `goals_quota` trigger skips the same id.
pub const ROUTINES_GOAL: &str = "00000000-0000-7d9f-866c-860554389e92";

const BUCKET: &str = "goal-files";
const OPEN: &str = "status in ('draft', 'active', 'paused') and not deleted and id <> '00000000-0000-7d9f-866c-860554389e92'";

/// A user's account as `GET /v1/account` reports it: when they allowed data to go to the
/// AI provider (none when not), and their use of each limit next to the limit.
/// `runs_today` counts the owner-started runs of the last 24 hours.
#[derive(Debug, Serialize, ToSchema)]
pub struct AccountView {
    pub ai_consent_at: Option<DateTime<Utc>>,
    pub open_goals: i64,
    pub max_open_goals: i64,
    pub runs_today: i64,
    pub max_runs_per_day: i64,
    pub files: i64,
    pub max_files: i64,
    pub file_bytes: i64,
    pub max_file_bytes: i64,
    pub max_file_total_bytes: i64,
}

/// Body of `PUT /v1/account/consent`: `ai` true records consent now, false withdraws it.
/// Without consent the engine starts no agent run.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SetConsent {
    pub ai: bool,
}

/// The Supabase Storage API the purge removes a deleted account's files through, so the
/// files leave the bucket and not only the table (a SQL delete orphans them). Built from
/// `AROH_SUPABASE_URL` and `AROH_SUPABASE_SECRET_KEY`; `Debug` never prints the key.
pub struct StorageApi {
    url: String,
    secret_key: String,
    transport: Arc<dyn Transport>,
}

impl std::fmt::Debug for StorageApi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StorageApi").field("url", &self.url).finish_non_exhaustive()
    }
}

impl StorageApi {
    /// Builds the client for the project at `url` (no trailing slash needed) with the
    /// project's `secret_key` (sent on the `apikey` header only), through `transport`.
    /// Returns the client.
    pub fn new(url: &str, secret_key: &str, transport: Arc<dyn Transport>) -> StorageApi {
        assert!(!url.is_empty() && !secret_key.is_empty(), "storage needs a URL and a key");
        StorageApi { url: url.trim_end_matches('/').to_string(), secret_key: secret_key.to_string(), transport }
    }

    /// Removes the objects `names` of the goal-files bucket.
    /// A name with no object is not an error, so a retry is safe.
    /// Errors on a network failure or a non-2xx answer.
    pub async fn remove(&self, names: &[String]) -> Result<(), ServerError> {
        assert!(names.len() <= REMOVE_BATCH, "callers send at most one batch");
        let body = serde_json::to_vec(&serde_json::json!({ "prefixes": names })).expect("names serialize");
        let req = HttpRequest {
            method: Method::Delete,
            url: format!("{}/storage/v1/object/{BUCKET}", self.url),
            headers: vec![("content-type".into(), "application/json".into()), ("apikey".into(), self.secret_key.clone())],
            body: Some(body),
            accept: vec![],
        };
        let mut res = self.transport.send(req).await.map_err(|e| ServerError::Config(format!("storage: {e}")))?;
        if !(200..300).contains(&res.status) {
            let (text, _) = read_limited(res.body.as_mut(), 2048).await.unwrap_or_default();
            return Err(ServerError::Config(format!("storage: status {} {}", res.status, String::from_utf8_lossy(&text))));
        }
        Ok(())
    }
}

fn refused(code: &'static str, message: String) -> ServerError {
    ServerError::Refused { code, message }
}

async fn consent_at(pool: &PgPool, user: Uuid) -> Result<Option<DateTime<Utc>>, ServerError> {
    Ok(db::query_scalar("select granted_at from public.ai_consents where user_id = $1").bind(user).fetch_optional(pool).await?)
}

async fn runs_in_window(pool: &PgPool, user: Uuid) -> Result<(i64, Option<DateTime<Utc>>), ServerError> {
    Ok(db::query_as(
        "select count(*), min(created_at) from public.runs
           where user_id = $1 and created_at > now() - interval '24 hours' and workflow = any($2)",
    )
    .bind(user)
    .bind(&OWNER_STARTED[..])
    .fetch_one(pool)
    .await?)
}

async fn open_goals(pool: &PgPool, user: Uuid) -> Result<i64, ServerError> {
    Ok(db::query_scalar(&format!("select count(*) from public.goals where user_id = $1 and {OPEN}")).bind(user).fetch_one(pool).await?)
}

async fn storage_present(pool: &PgPool) -> Result<bool, ServerError> {
    Ok(db::query_scalar("select to_regclass('storage.objects') is not null").fetch_one(pool).await?)
}

/// The names of `user`'s objects in the goal-files bucket, oldest name first.
/// Empty when the database has no storage schema (a self-host without storage).
/// Errors only when the database does.
pub async fn stored_files(pool: &PgPool, user: Uuid) -> Result<Vec<String>, ServerError> {
    if !storage_present(pool).await? {
        return Ok(Vec::new());
    }
    Ok(db::query_scalar(
        "select name from storage.objects where bucket_id = $1 and (storage.foldername(name))[1] = $2 order by name",
    )
    .bind(BUCKET)
    .bind(user.to_string())
    .fetch_all(pool)
    .await?)
}

/// Whether `user` asked to delete their account and has not signed in since (Q76).
/// Such an account is disabled: no runs, no data served.
/// Errors only when the database does.
pub async fn disabled(pool: &PgPool, user: Uuid) -> Result<bool, ServerError> {
    Ok(db::query_scalar("select exists (select 1 from public.account_deletions where user_id = $1)").bind(user).fetch_one(pool).await?)
}

/// Refuses an agent run of `user` while the account is disabled (`account_disabled`),
/// or without AI consent (`consent_required`); a run of an `OWNER_STARTED` `workflow`
/// also past `MAX_RUNS_PER_DAY` of those in 24 hours (`daily_run_limit`). Every run
/// start goes through it; continuing a stopped run does not.
pub async fn check_run(pool: &PgPool, user: Uuid, workflow: &str) -> Result<(), ServerError> {
    if disabled(pool, user).await? {
        return Err(refused("account_disabled", "This account is being deleted, so agents do not run.".into()));
    }
    if consent_at(pool, user).await?.is_none() {
        return Err(refused(
            "consent_required",
            "Agents send your goal data to the AI provider. Allow it in Settings before agents can run.".into(),
        ));
    }
    if !OWNER_STARTED.contains(&workflow) {
        return Ok(());
    }
    let (count, oldest) = runs_in_window(pool, user).await?;
    if count >= MAX_RUNS_PER_DAY {
        let free = oldest.map(|t| (t + Duration::hours(24)).format("%H:%M UTC").to_string()).unwrap_or_else(|| "tomorrow".into());
        return Err(refused(
            "daily_run_limit",
            format!("You have used your {MAX_RUNS_PER_DAY} agent runs for the last 24 hours. The next one frees up at {free}."),
        ));
    }
    Ok(())
}

/// Refuses a new open goal of `user` at `MAX_OPEN_GOALS` (`goal_limit`), before the
/// engine writes the draft; the `goals_quota` trigger refuses the same for direct writes.
pub async fn check_new_goal(pool: &PgPool, user: Uuid) -> Result<(), ServerError> {
    if open_goals(pool, user).await? >= MAX_OPEN_GOALS {
        return Err(refused("goal_limit", format!("You have {MAX_OPEN_GOALS} active goals. Archive one to add another.")));
    }
    Ok(())
}

/// Reads `user`'s consent and use of every limit.
/// Returns the view.
/// Errors only when the database does.
pub async fn view(pool: &PgPool, user: Uuid) -> Result<AccountView, ServerError> {
    let (runs_today, _) = runs_in_window(pool, user).await?;
    let (files, file_bytes): (i64, i64) = db::query_as(
        "select count(*), coalesce(sum(size_bytes), 0)::bigint from public.goal_files where user_id = $1 and not deleted and storage_path is not null",
    )
    .bind(user)
    .fetch_one(pool)
    .await?;
    Ok(AccountView {
        ai_consent_at: consent_at(pool, user).await?,
        open_goals: open_goals(pool, user).await?,
        max_open_goals: MAX_OPEN_GOALS,
        runs_today,
        max_runs_per_day: MAX_RUNS_PER_DAY,
        files,
        max_files: MAX_FILES,
        file_bytes,
        max_file_bytes: MAX_FILE_BYTES,
        max_file_total_bytes: MAX_FILE_TOTAL_BYTES,
    })
}

/// Records `user`'s consent at `now` when `ai`, else withdraws it.
/// Recording again keeps the first time.
/// Errors only when the database does.
pub async fn set_consent(pool: &PgPool, user: Uuid, ai: bool, now: DateTime<Utc>) -> Result<(), ServerError> {
    if ai {
        db::query("insert into public.ai_consents (user_id, granted_at) values ($1, $2) on conflict (user_id) do nothing")
            .bind(user)
            .bind(now)
            .execute(pool)
            .await?;
    } else {
        db::query("delete from public.ai_consents where user_id = $1").bind(user).execute(pool).await?;
    }
    Ok(())
}

/// Days between a deletion request and the purge (Q76); signing in before cancels it.
pub const DELETION_GRACE_DAYS: i64 = 15;

/// Answer of `DELETE /v1/account`: when everything of the account is deleted unless the
/// user signs in again before then.
#[derive(Debug, Serialize, ToSchema)]
pub struct DeletionScheduled {
    pub purge_after: DateTime<Utc>,
}

/// Disables `user`'s account at `now` and schedules the purge `DELETION_GRACE_DAYS` later
/// (`aroh_request_account_deletion`): devices and sessions go, queued runs fail, data is
/// no longer served. A repeat keeps the first date. Returns the purge date.
pub async fn request_deletion(pool: &PgPool, user: Uuid, now: DateTime<Utc>) -> Result<DeletionScheduled, ServerError> {
    let purge_after: DateTime<Utc> = db::query_scalar("select public.aroh_request_account_deletion($1, $2, make_interval(days => $3::int))")
        .bind(user)
        .bind(now)
        .bind(i32::try_from(DELETION_GRACE_DAYS).expect("small"))
        .fetch_one(pool)
        .await?;
    assert!(purge_after > now - Duration::days(DELETION_GRACE_DAYS + 1), "a purge date follows a request");
    Ok(DeletionScheduled { purge_after })
}

async fn purge(pool: &PgPool, storage: Option<&StorageApi>, user: Uuid) -> Result<(), ServerError> {
    let names = stored_files(pool, user).await?;
    if !names.is_empty() {
        let Some(storage) = storage else {
            return Err(ServerError::Config(format!("{} stored files and no AROH_SUPABASE_SECRET_KEY to remove them", names.len())));
        };
        for batch in names.chunks(REMOVE_BATCH) {
            storage.remove(batch).await?;
        }
    }
    db::query("select public.aroh_delete_account($1)").bind(user).execute(pool).await?;
    Ok(())
}

/// Purges every account whose grace ended by `now`, at most 20 per call: stored files
/// through `storage`, then every row and the auth user (`aroh_delete_account`). An
/// account that fails stays pending for the next tick. Returns purged count and errors.
pub async fn purge_due(pool: &PgPool, storage: Option<&StorageApi>, now: DateTime<Utc>) -> Result<(usize, Vec<String>), ServerError> {
    let due: Vec<Uuid> = db::query_scalar("select user_id from public.account_deletions where purge_after <= $1 order by purge_after limit 20")
        .bind(now)
        .fetch_all(pool)
        .await?;
    let mut purged = 0;
    let mut errors = Vec::new();
    for user in due {
        match purge(pool, storage, user).await {
            Ok(()) => purged += 1,
            Err(e) => errors.push(format!("{user}: {}: {e}", e.status().1)),
        }
    }
    Ok((purged, errors))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_routines_goal_is_the_seeded_one_and_never_counts() {
        assert_eq!(ROUTINES_GOAL, aroh_domain::seed::routines_id());
        assert!(OPEN.ends_with(&format!("id <> '{ROUTINES_GOAL}'")));
        let migration = include_str!("../../../supabase/migrations/20261202000000_public_accounts.sql");
        assert_eq!(migration.matches(ROUTINES_GOAL).count(), 2, "goals_quota skips the Routines goal");
    }
}
