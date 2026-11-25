//! The engine side of the encrypted diary (Q85): the server copy of the
//! diary key while the agents read toggle is on, the last seven days of
//! summaries a run may read, and how diary runs store proposal summaries.

use crate::db;
use crate::error::ServerError;
use crate::io::Deps;
use crate::rows::parse_row;
use crate::tools::ToolEnv;
use aroh_diary::{AgentDay, Card, DiaryKey, Sealed, Table};
use aroh_domain::rows::DiaryDay;
use chrono::{Duration, NaiveDate};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use troupekit_tools::ToolError;
use utoipa::ToSchema;
use uuid::Uuid;

/// Days of diary a run reads, today included.
pub const DIARY_DAYS: i64 = 7;

/// The plaintext summary a diary run's proposal carries; the real summary is
/// sealed with the diary key so only the owner's devices read it.
pub const NEUTRAL_SUMMARY: &str = "A plan change that used your diary. Open it on your iPhone to read why.";

/// Body of `PUT /v1/diary/key`: the diary key version and its 32 bytes as
/// standard base64. No Debug, so the key never reaches a log line.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SetDiaryKey {
    pub version: i64,
    pub key: String,
}

/// Whether agents may read the diary, and the server copy's key version.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct DiaryKeyStatus {
    pub on: bool,
    pub version: Option<i64>,
}

/// The newest key version among `user`'s live diary entries and cards: the
/// version the owner's devices write with. None while there are no rows.
pub async fn rows_version(deps: &Deps, user: Uuid) -> Result<Option<i64>, ServerError> {
    Ok(db::query_scalar(
        "select max(v) from (select max(key_version) v from public.diary_entries where user_id = $1 and not deleted
         union all select max(key_version) from public.diary_days where user_id = $1 and not deleted) t",
    )
    .bind(user)
    .fetch_one(&deps.pool)
    .await?)
}

/// Stores `user`'s diary key from `req` in the vault (toggle on).
/// Returns the status, `BadRequest` for a key that is not 32 bytes of base64
/// or a version outside 1 to 1000000 (never echoing the key), or `Conflict`
/// when diary rows already use another version than the device sent.
pub async fn set_key(deps: &Deps, user: Uuid, req: SetDiaryKey) -> Result<DiaryKeyStatus, ServerError> {
    let key = DiaryKey::from_base64(req.version, &req.key).map_err(|e| ServerError::BadRequest(e.to_string()))?;
    if let Some(rows) = rows_version(deps, user).await? {
        if rows != key.version() {
            return Err(ServerError::Conflict(format!("diary rows use key version {rows}, not {}", key.version())));
        }
    }
    let version = deps.vault.diary_set(user, &key).await?;
    Ok(DiaryKeyStatus { on: true, version: Some(version) })
}

/// Deletes `user`'s server copy of the diary key (toggle off).
/// Returns true when there was one.
pub async fn delete_key(deps: &Deps, user: Uuid) -> Result<bool, ServerError> {
    deps.vault.diary_delete(user).await
}

/// Reads whether `user`'s agents read toggle is on, without decrypting.
pub async fn status(deps: &Deps, user: Uuid) -> Result<DiaryKeyStatus, ServerError> {
    let version = deps.vault.diary_version(user).await?;
    Ok(DiaryKeyStatus { on: version.is_some(), version })
}

/// The diary a run of `user` may read: summary, mood, energy and notes for
/// tomorrow of each structured day from `today` back six days, opened in
/// memory with the server copy of the key. None when the toggle is off, when
/// the server key is not the version the diary rows use, or no day opens; rows
/// that are not the day's own card id are skipped, so at most seven days are read.
pub async fn agent_days(deps: &Deps, user: Uuid, today: NaiveDate) -> Result<Option<Vec<AgentDay>>, ServerError> {
    let Some(key) = deps.vault.diary_get(user).await? else {
        return Ok(None);
    };
    if rows_version(deps, user).await?.is_some_and(|v| v != key.version()) {
        eprintln!("aroh: diary key version differs from the diary rows; the run gets no diary");
        return Ok(None);
    }
    let from = (today - Duration::days(DIARY_DAYS - 1)).format("%Y-%m-%d").to_string();
    let to = today.format("%Y-%m-%d").to_string();
    let rows: Vec<Value> = db::query_scalar(
        "select to_jsonb(t) - 'server_seq' from public.diary_days t
         where t.user_id = $1 and not t.deleted and t.day >= $2 and t.day <= $3 order by t.day",
    )
    .bind(user)
    .bind(from)
    .bind(to)
    .fetch_all(&deps.pool)
    .await?;
    let owner = user.to_string();
    let mut days = Vec::new();
    for value in rows {
        let row: DiaryDay = parse_row(value)?;
        if row.id != aroh_diary::day_id(&owner, &row.day) || days.len() == DIARY_DAYS as usize {
            continue;
        }
        let sealed = Sealed { key_version: row.key_version, nonce: row.nonce, ciphertext: row.ciphertext };
        let Ok(plain) = aroh_diary::open(&key, &owner, Table::Days, &row.id, &sealed) else { continue };
        let Ok(card) = Card::from_bytes(&plain) else { continue };
        days.push(card.for_agents());
    }
    Ok(if days.is_empty() { None } else { Some(days) })
}

/// The summary columns of a proposal written by `env`'s run: as given for an
/// ordinary run; for a diary run the neutral line, with the real summary
/// sealed by the diary key for row `id` (none when the key was taken back).
/// Returns `(summary, sealed_summary)`.
pub fn proposal_summary(env: &ToolEnv, id: &str, summary: &str) -> Result<(String, Option<Value>), ToolError> {
    if !env.sealed {
        return Ok((summary.to_string(), None));
    }
    let Some(key) = &env.diary_key else {
        return Ok((NEUTRAL_SUMMARY.to_string(), None));
    };
    let sealed = aroh_diary::seal(key, &env.user.to_string(), Table::Proposals, id, summary.as_bytes())
        .map_err(|e| ToolError::Failed(format!("sealing the proposal summary: {e}")))?;
    Ok((NEUTRAL_SUMMARY.to_string(), Some(json!(sealed))))
}

/// Seals `value` of a diary run's proposal `id` under `table` with the diary key.
/// Returns `{"sealed": {key_version, nonce, ciphertext}}`, or `{"withheld": true}`
/// when the key was taken back during the run; the plaintext is never stored.
fn sealed_json(env: &ToolEnv, table: Table, id: &str, value: &Value) -> Result<Value, ToolError> {
    assert!(env.sealed, "only a diary run seals its proposal text");
    let Some(key) = &env.diary_key else {
        return Ok(json!({"withheld": true}));
    };
    let plain = serde_json::to_vec(value).expect("JSON serializes");
    let sealed = aroh_diary::seal(key, &env.user.to_string(), table, id, &plain)
        .map_err(|e| ToolError::Failed(format!("sealing the proposal: {e}")))?;
    Ok(json!({"sealed": sealed}))
}

/// The `diff` column of proposal `id`: as given for an ordinary run, sealed
/// with the diary key for a diary run (the device opens it for display).
pub fn proposal_diff(env: &ToolEnv, id: &str, diff: Value) -> Result<Value, ToolError> {
    if !env.sealed {
        return Ok(diff);
    }
    sealed_json(env, Table::ProposalDiffs, id, &diff)
}

/// Field `part` of proposal `id`'s diff (a card chat's `request` or `after`):
/// as given for an ordinary run, sealed with the diary key under row id
/// `{id}/{part}` for a diary run, so the device opens each part on its own.
pub fn proposal_part(env: &ToolEnv, id: &str, part: &str, value: Value) -> Result<Value, ToolError> {
    if !env.sealed {
        return Ok(value);
    }
    sealed_json(env, Table::ProposalDiffs, &format!("{id}/{part}"), &value)
}
