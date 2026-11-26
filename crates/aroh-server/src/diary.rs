//! The engine side of the encrypted diary (Q85, Q94): the server copy of the
//! diary key, structuring a day in memory, the agents read toggle, the last
//! seven days of summaries a run may read, and how diary runs store proposal
//! summaries.

use crate::db;
use crate::error::ServerError;
use crate::io::Deps;
use crate::rows::parse_row;
use crate::tools::ToolEnv;
use aroh_diary::{AgentDay, Card, DiaryKey, Sealed, StructureRequest, StructuredDay, Table};
use aroh_domain::rows::DiaryDay;
use chrono::{Duration, NaiveDate};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use troupekit_core::message::{ContentPart, Message};
use troupekit_providers::CompletionRequest;
use troupekit_tools::ToolError;
use utoipa::ToSchema;
use uuid::Uuid;

/// Most output tokens of one structuring turn.
const STRUCTURE_MAX_TOKENS: u32 = 16_000;

/// Largest body of `POST /v1/diary/structure`, in bytes: 200 sealed entries
/// fit, and it stays under the hosting platform's request cap.
pub const STRUCTURE_BODY_MAX: usize = 4 * 1024 * 1024;

/// Days of diary a run reads, today included.
pub const DIARY_DAYS: i64 = 7;

/// The plaintext summary a diary run's proposal carries; the real summary is
/// sealed with the diary key so only the owner's devices read it.
pub const NEUTRAL_SUMMARY: &str = "A plan change that used your diary. Open it on your iPhone to read why.";

/// Body of `PUT /v1/diary/key`: the diary key version and its 32 bytes as
/// standard base64, and `agents` to set whether planning agents may read the
/// diary (omitted keeps the current choice, off for a first key). No Debug,
/// so the key never reaches a log line.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SetDiaryKey {
    pub version: i64,
    pub key: String,
    #[serde(default)]
    pub agents: Option<bool>,
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

/// Stores `user`'s diary key from `req` in the vault and, when `req.agents`
/// is given, sets the agents read toggle. Returns the status, `BadRequest`
/// for a key that is not 32 bytes of base64 or a version outside 1 to 1000000
/// (never echoing the key), or `Conflict` when diary rows already use another
/// version than the device sent.
pub async fn set_key(deps: &Deps, user: Uuid, req: SetDiaryKey) -> Result<DiaryKeyStatus, ServerError> {
    let key = DiaryKey::from_base64(req.version, &req.key).map_err(|e| ServerError::BadRequest(e.to_string()))?;
    if let Some(rows) = rows_version(deps, user).await? {
        if rows != key.version() {
            return Err(ServerError::Conflict(format!("diary rows use key version {rows}, not {}", key.version())));
        }
    }
    deps.vault.diary_set(user, &key).await?;
    if let Some(on) = req.agents {
        assert!(deps.vault.diary_agents(user, on).await?, "the key row was just written");
    }
    status(deps, user).await
}

/// Reads whether `user`'s agents read toggle is on and which key version the
/// server holds, without decrypting.
pub async fn status(deps: &Deps, user: Uuid) -> Result<DiaryKeyStatus, ServerError> {
    let state = deps.vault.diary_state(user).await?;
    Ok(DiaryKeyStatus { on: state.is_some_and(|s| s.1), version: state.map(|s| s.0) })
}

/// Structures one day of `user` from `req` (Q94): opens the sealed entries
/// in memory with the server copy of the diary key, calls the model once with
/// the diary structure prompt, and seals the card for the day's row. Returns
/// ciphertext and tracker numbers only; nothing is stored or logged but the
/// call's usage. Errors: `BadRequest` for a malformed request, `diary_key_missing`
/// (409) when the server holds no key or another version than the entries, a
/// refusal when an entry does not open, and the errors of `io::one_turn`.
pub async fn structure(deps: &Deps, user: Uuid, req: &StructureRequest) -> Result<StructuredDay, ServerError> {
    req.check().map_err(|e| ServerError::BadRequest(e.to_string()))?;
    let missing = || ServerError::Refused { code: "diary_key_missing", message: "The engine needs your current diary key. Open the diary on your iPhone and try again.".into() };
    let key = deps.vault.diary_get(user).await?.ok_or_else(missing)?;
    if req.entries.iter().any(|e| e.sealed.key_version != key.version()) {
        return Err(missing());
    }
    let owner = user.to_string();
    let entries = req.open(&key, &owner).map_err(|_| ServerError::BadRequest("An entry of this day could not be opened with your diary key.".into()))?;
    let prompt = aroh_diary::prompt(&req.day, &entries, &req.fields).map_err(|e| ServerError::BadRequest(e.to_string()))?;
    drop(entries);
    let mut call = CompletionRequest::user(prompt.user.as_str(), STRUCTURE_MAX_TOKENS);
    call.system = Some(prompt.system.clone());
    drop(prompt);
    let reply = crate::io::one_turn(deps, user, &call).await;
    zero_messages(&mut call.messages);
    let reply = zeroize::Zeroizing::new(reply?);
    let now = crate::rows::utc(deps.clock.now_ms());
    req.answer(&key, &owner, &reply, now)
        .map_err(|_| ServerError::Refused { code: "model_failed", message: "The AI provider's answer could not be read as a diary card. Try again.".into() })
}

/// Overwrites the text of `messages` in place before they are dropped, so the
/// diary words of a structuring call do not linger in freed memory.
fn zero_messages(messages: &mut [Message]) {
    use zeroize::Zeroize;
    for m in messages.iter_mut() {
        for part in m.parts.iter_mut() {
            if let ContentPart::Text { text } = part {
                text.zeroize();
            }
        }
    }
}

/// The diary a run of `user` may read: summary, mood, energy and notes for
/// tomorrow of each structured day from `today` back six days, opened in
/// memory with the server copy of the key. None when the agents read toggle
/// is off, when the server key is not the version the diary rows use, or no
/// day opens; rows that are not the day's own card id are skipped, so at most
/// seven days are read.
pub async fn agent_days(deps: &Deps, user: Uuid, today: NaiveDate) -> Result<Option<Vec<AgentDay>>, ServerError> {
    if !deps.vault.diary_state(user).await?.is_some_and(|s| s.1) {
        return Ok(None);
    }
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
