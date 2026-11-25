//! Server push (Q22): device token registration and APNs delivery of reminders, check-ins,
//! proposals and runs waiting for the owner (Q83). The device schedules the next 24 h locally,
//! so a reminder is pushed only when the device has not seen its row or its fire time lies
//! beyond the local schedule.

use crate::db::{self, PgPool};
use crate::error::ServerError;
use crate::rows::{list_rows, utc};
use aroh_domain::remind::{self, Planned, PlannedKind};
use aroh_domain::rows::{Action, Goal, Proposal, Tracker};
use troupekit_graph::{HumanAsk, Stop};
use chrono::{DateTime, Duration, Utc};
use chrono_tz::Tz;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use troupekit_core::clock::Clock;
use troupekit_core::http::{read_limited, HttpRequest, Transport};
use troupekit_core::ids::RunId;
use utoipa::ToSchema;
use uuid::Uuid;

/// How far back a dispatch pass looks for reminders whose fire time passed.
pub const LOOKBACK_MINUTES: i64 = 5;

/// How long after creation a proposal is still pushed.
pub const PROPOSAL_HOURS: i64 = 24;

/// How long after a run starts waiting for its owner it is still pushed.
pub const RUN_HOURS: i64 = 6;

/// Title prefix of a waiting run that has no goal, such as a calendar triage.
pub const NO_GOAL_NAME: &str = "Calendar";

/// How long after a run fails it is still pushed.
pub const FAIL_HOURS: i64 = 2;

/// Body of a failed-run push. Static: never the error, which can quote model or diary text.
pub const FAIL_BODY: &str = "Tap to see what happened.";

/// Title of a failed-run push for a run that has no goal.
pub const NO_GOAL_FAIL_TITLE: &str = "A run stopped";

/// Seconds between dispatch passes in worker mode.
pub const INTERVAL_SECS: u64 = 30;

const JWT_REFRESH_MS: i64 = 50 * 60 * 1000;
const LEDGER_DAYS: i64 = 7;
const REPLY_MAX: usize = 4096;

/// APNs environment of a device's build: `sandbox` for debug builds, `production` for
/// TestFlight and App Store builds. Picks the APNs host the push goes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ApnsEnv {
    Sandbox,
    Production,
}

impl ApnsEnv {
    fn host(self) -> &'static str {
        match self {
            ApnsEnv::Sandbox => "api.sandbox.push.apple.com",
            ApnsEnv::Production => "api.push.apple.com",
        }
    }
}

/// Push platform of a device; only iOS for now (FCM comes with Android, Q22).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Ios,
}

/// APNs token-based auth settings: the .p8 signing key, its key id, the team id and the
/// topic (the app's bundle id). `Debug` never prints the key.
pub struct ApnsConfig {
    key: EncodingKey,
    pub key_id: String,
    pub team_id: String,
    pub topic: String,
}

impl std::fmt::Debug for ApnsConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApnsConfig").field("key", &"[redacted]").field("key_id", &self.key_id).field("team_id", &self.team_id).field("topic", &self.topic).finish()
    }
}

fn check_ident(name: &str, value: &str) -> Result<(), ServerError> {
    if value.len() == 10 && value.chars().all(|c| c.is_ascii_alphanumeric()) {
        Ok(())
    } else {
        Err(ServerError::Config(format!("{name} must be 10 letters or digits")))
    }
}

impl ApnsConfig {
    /// Reads `AROH_APNS_KEY` (PEM text of the .p8, `\n` escapes allowed), `AROH_APNS_KEY_ID`,
    /// `AROH_APNS_TEAM_ID` and `AROH_APNS_TOPIC` from `vars`. All unset turns push off
    /// (`None`); a partial set or a bad value is a `Config` error that never echoes the key.
    pub fn from_vars(vars: &BTreeMap<String, String>) -> Result<Option<ApnsConfig>, ServerError> {
        let names = ["AROH_APNS_KEY", "AROH_APNS_KEY_ID", "AROH_APNS_TEAM_ID", "AROH_APNS_TOPIC"];
        let values: Vec<Option<&String>> = names.iter().map(|n| vars.get(*n).filter(|v| !v.trim().is_empty())).collect();
        if values.iter().all(Option::is_none) {
            return Ok(None);
        }
        if let Some(i) = values.iter().position(Option::is_none) {
            return Err(ServerError::Config(format!("{} is not set; set all APNs variables or none", names[i])));
        }
        let [pem, key_id, team_id, topic] = [values[0], values[1], values[2], values[3]].map(|v| v.expect("checked above").trim().to_string());
        check_ident("AROH_APNS_KEY_ID", &key_id)?;
        check_ident("AROH_APNS_TEAM_ID", &team_id)?;
        if topic.is_empty() || topic.len() > 155 || !topic.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') {
            return Err(ServerError::Config("AROH_APNS_TOPIC must be a bundle id".into()));
        }
        let key = EncodingKey::from_ec_pem(pem.replace("\\n", "\n").as_bytes())
            .map_err(|_| ServerError::Config("AROH_APNS_KEY is not a PEM EC private key".into()))?;
        Ok(Some(ApnsConfig { key, key_id, team_id, topic }))
    }
}

#[derive(Serialize)]
struct Claims<'a> {
    iss: &'a str,
    iat: i64,
}

/// Signs the APNs provider token (ES256 JWT with `kid` and `iss`) and reuses it for
/// 50 minutes, inside Apple's 20 to 60 minute window.
pub struct Signer {
    config: ApnsConfig,
    clock: Arc<dyn Clock>,
    cached: Mutex<Option<(i64, String)>>,
}

impl Signer {
    /// Builds a signer over `config` reading time from `clock`.
    /// Nothing is signed until the first `token` call.
    /// Returns the signer.
    pub fn new(config: ApnsConfig, clock: Arc<dyn Clock>) -> Signer {
        Signer { config, clock, cached: Mutex::new(None) }
    }

    /// The current provider token, signed anew when the cached one is 50 minutes old.
    /// Errors with `Crypto` when signing fails; the message carries no key material.
    /// Returns the compact JWT.
    pub fn token(&self) -> Result<String, ServerError> {
        let now = self.clock.now_ms();
        let mut cached = self.cached.lock().expect("signer lock");
        if let Some((at, token)) = cached.as_ref() {
            if now - at < JWT_REFRESH_MS && now >= *at {
                return Ok(token.clone());
            }
        }
        let mut header = Header::new(Algorithm::ES256);
        header.kid = Some(self.config.key_id.clone());
        let claims = Claims { iss: &self.config.team_id, iat: now / 1000 };
        let token = jsonwebtoken::encode(&header, &claims, &self.config.key).map_err(|_| ServerError::Crypto("apns token signing failed".into()))?;
        *cached = Some((now, token.clone()));
        Ok(token)
    }

    /// The topic every push names, the app's bundle id.
    /// Same value as `AROH_APNS_TOPIC`.
    /// Returns it borrowed.
    pub fn topic(&self) -> &str {
        &self.config.topic
    }
}

/// Body of `PUT /v1/devices/{device_id}`: the APNs token (hex), the build's APNs
/// environment, the device's IANA zone, the lowest pull cursor of `actions` and
/// `trackers`, and how far its local notification schedule reaches.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RegisterDevice {
    pub platform: Platform,
    pub token: String,
    pub environment: ApnsEnv,
    pub zone: String,
    pub synced_seq: i64,
    pub scheduled_until: DateTime<Utc>,
}

/// A registered device as the API reports it. The token is never echoed.
/// `scheduled_until` and `synced_seq` are what the device last reported.
/// Times are UTC.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DeviceInfo {
    pub device_id: Uuid,
    pub platform: Platform,
    pub environment: ApnsEnv,
    pub zone: String,
    pub synced_seq: i64,
    pub scheduled_until: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
struct Device {
    id: Uuid,
    user_id: Uuid,
    token: String,
    environment: ApnsEnv,
    zone: String,
    synced_seq: i64,
    scheduled_until: DateTime<Utc>,
    created_at: DateTime<Utc>,
}

fn check_device(req: &RegisterDevice) -> Result<Tz, ServerError> {
    let hex = req.token.len() % 2 == 0 && (64..=200).contains(&req.token.len()) && req.token.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase());
    if !hex {
        return Err(ServerError::BadRequest("token must be 64 to 200 lowercase hex characters".into()));
    }
    if req.synced_seq < 0 {
        return Err(ServerError::BadRequest("synced_seq must not be negative".into()));
    }
    if req.zone.len() > 64 {
        return Err(ServerError::BadRequest("zone is not an IANA time zone".into()));
    }
    req.zone.parse::<Tz>().map_err(|_| ServerError::BadRequest("zone is not an IANA time zone".into()))
}

/// Registers or refreshes device `device_id` for `user` at `now`. A token held by another
/// device row (another user's included) moves here. Errors with `BadRequest` for a bad
/// token or zone and `Conflict` when the device id belongs to another user.
pub async fn register(pool: &PgPool, user: Uuid, device_id: Uuid, req: RegisterDevice, now: DateTime<Utc>) -> Result<DeviceInfo, ServerError> {
    check_device(&req)?;
    let mut tx = pool.begin().await?;
    db::query("delete from public.devices where token = $1 and id <> $2").bind(&req.token).bind(device_id).execute(&mut *tx).await?;
    let changed = db::query(
        "insert into public.devices (id, user_id, platform, token, environment, zone, synced_seq, scheduled_until, created_at, updated_at)
         values ($1, $2, 'ios', $3, $4, $5, $6, $7, $8, $8)
         on conflict (id) do update set token = excluded.token, environment = excluded.environment, zone = excluded.zone,
           synced_seq = excluded.synced_seq, scheduled_until = excluded.scheduled_until, updated_at = excluded.updated_at
         where public.devices.user_id = excluded.user_id",
    )
    .bind(device_id)
    .bind(user)
    .bind(&req.token)
    .bind(match req.environment {
        ApnsEnv::Sandbox => "sandbox",
        ApnsEnv::Production => "production",
    })
    .bind(&req.zone)
    .bind(req.synced_seq)
    .bind(req.scheduled_until)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    if changed.rows_affected() != 1 {
        return Err(ServerError::Conflict(format!("device {device_id} belongs to another account")));
    }
    tx.commit().await?;
    Ok(DeviceInfo { device_id, platform: req.platform, environment: req.environment, zone: req.zone, synced_seq: req.synced_seq, scheduled_until: req.scheduled_until })
}

/// Deletes device `device_id` of `user`, for sign-out. Its push ledger goes with it.
/// Returns false when no such device of `user` exists.
pub async fn unregister(pool: &PgPool, user: Uuid, device_id: Uuid) -> Result<bool, ServerError> {
    let done = db::query("delete from public.devices where id = $1 and user_id = $2").bind(device_id).bind(user).execute(pool).await?;
    Ok(done.rows_affected() == 1)
}

/// What one push announces: a reminder or check-in from the shared planner, a proposal,
/// a run waiting at human node `node` since `waiting_since`, or a run that failed at
/// `failed_at`. A run's `step` is its checkpoint step; its `goal_id` is `None` when the
/// run has no goal.
#[derive(Debug, Clone, PartialEq)]
pub enum Notice {
    Reminder(Planned),
    Proposal { id: String, goal_id: String, title: String, body: String, created_at: DateTime<Utc> },
    Run { run_id: Uuid, goal_id: Option<String>, node: String, step: u64, title: String, body: String, waiting_since: DateTime<Utc> },
    RunFailed { run_id: Uuid, goal_id: Option<String>, step: u64, title: String, failed_at: DateTime<Utc> },
}

/// One APNs push ready to send: ledger key, collapse id, expiry and JSON payload.
/// The collapse id of a reminder equals the device's local notification id.
#[derive(Debug, Clone, PartialEq)]
pub struct Push {
    pub key: String,
    pub collapse_id: String,
    pub expires_at: DateTime<Utc>,
    pub payload: Value,
}

/// Builds the push for `notice`: `aps` alert with title, body, default sound and the
/// kind as thread, plus `aroh {kind, id, goal_id}` (and `node` for a run) to route a tap.
/// A run's ledger key names run, node and wait step, so each new wait pushes once; its
/// collapse id is the run id (64-byte cap).
/// A failed run's key names run and step, so a retried run that fails again pushes again.
/// Output: the push; reminders expire after an hour, runs after `RUN_HOURS`, failed runs
/// after `FAIL_HOURS`, proposals a day.
pub fn build(notice: &Notice) -> Push {
    let alert = |kind: &str, title: &str, body: &str, aroh: Value| {
        json!({"aps": {"alert": {"title": title, "body": body}, "sound": "default", "thread-id": kind}, "aroh": aroh})
    };
    match notice {
        Notice::Reminder(p) => {
            let kind = match p.kind {
                PlannedKind::Action => "reminder",
                PlannedKind::CheckIn => "check_in",
            };
            Push {
                key: format!("{kind}:{}:{}", p.id, p.fire_at.timestamp()),
                collapse_id: p.id.to_string(),
                expires_at: p.fire_at + Duration::hours(1),
                payload: alert(kind, &p.title, &p.body, json!({"kind": kind, "id": p.row_id})),
            }
        }
        Notice::Proposal { id, goal_id, title, body, created_at } => Push {
            key: format!("proposal:{id}"),
            collapse_id: id.clone(),
            expires_at: *created_at + Duration::hours(PROPOSAL_HOURS),
            payload: alert("proposal", title, body, json!({"kind": "proposal", "id": id, "goal_id": goal_id})),
        },
        Notice::Run { run_id, goal_id, node, step, title, body, waiting_since } => {
            let push = Push {
                key: format!("run:{run_id}:{node}:{step}"),
                collapse_id: run_id.to_string(),
                expires_at: *waiting_since + Duration::hours(RUN_HOURS),
                payload: alert("run", title, body, json!({"kind": "run", "id": run_id, "goal_id": goal_id, "node": node})),
            };
            assert!((1..=200).contains(&push.key.len()), "push_sent keys are 1 to 200 characters");
            assert!(push.collapse_id.len() <= 64, "APNs caps the collapse id at 64 bytes");
            push
        }
        Notice::RunFailed { run_id, goal_id, step, title, failed_at } => {
            let push = Push {
                key: format!("fail:{run_id}:{step}"),
                collapse_id: run_id.to_string(),
                expires_at: *failed_at + Duration::hours(FAIL_HOURS),
                payload: alert("run_failed", title, FAIL_BODY, json!({"kind": "run_failed", "id": run_id, "goal_id": goal_id})),
            };
            assert!((1..=200).contains(&push.key.len()), "push_sent keys are 1 to 200 characters");
            assert!(push.collapse_id.len() <= 64, "APNs caps the collapse id at 64 bytes");
            push
        }
    }
}

/// Title of the push for a failed run of goal `goal` (`None`: the run has none) stopped by
/// `stop`, if it was stopped from outside a step. Returns `None` for a run its owner
/// cancelled, which never pushes; a time limit or an error does. Names only the goal title.
pub fn fail_title(goal: Option<&str>, stop: Option<&Stop>) -> Option<String> {
    if matches!(stop, Some(Stop::Cancelled)) {
        return None;
    }
    Some(match goal {
        Some(goal) => remind::truncate(format!("{goal}: the run stopped")),
        None => NO_GOAL_FAIL_TITLE.to_string(),
    })
}

/// Title and body of the push for a run of goal `goal` (`NO_GOAL_NAME` when the run has
/// none) waiting at `ask`: a plan to approve, or answers to give. Echoes no answers.
/// Returns `None` for a replan change to approve, whose proposal row pushes instead.
pub fn run_text(goal: &str, ask: &HumanAsk) -> Option<(String, String)> {
    let has = |key: &str| ask.data.get(key).is_some_and(|v| !v.is_null());
    let (what, body) = match ask.node.as_str() {
        "approve" if has("plan") => ("plan ready to approve", "Tap to review the plan."),
        "approve" if has("proposal_record") => return None,
        _ => ("your answers are needed", "Tap to answer in Aroh."),
    };
    Some((remind::truncate(format!("{goal}: {what}")), remind::truncate(body.to_string())))
}

/// The APNs request for `push` to `token` in `env`, authed with provider token `jwt`.
/// Headers: topic, alert push type, priority 10, collapse id and expiration.
/// Output: an HTTP/2 POST to `/3/device/<token>`.
pub fn request(push: &Push, token: &str, env: ApnsEnv, topic: &str, jwt: &str) -> HttpRequest {
    let headers = vec![
        ("authorization".to_string(), format!("bearer {jwt}")),
        ("apns-topic".to_string(), topic.to_string()),
        ("apns-push-type".to_string(), "alert".to_string()),
        ("apns-priority".to_string(), "10".to_string()),
        ("apns-collapse-id".to_string(), push.collapse_id.clone()),
        ("apns-expiration".to_string(), push.expires_at.timestamp().to_string()),
        ("content-type".to_string(), "application/json".to_string()),
    ];
    HttpRequest::post(format!("https://{}/3/device/{token}", env.host()), headers, push.payload.to_string().into_bytes())
}

/// Whether the server should push a reminder whose row has `seq` and fires at `fire_at`
/// to a device that synced up to `synced_seq` and scheduled locally until
/// `scheduled_until`. True when the device never saw the row or will not show it itself.
pub fn should_push(seq: Option<i64>, fire_at: DateTime<Utc>, synced_seq: i64, scheduled_until: DateTime<Utc>) -> bool {
    seq.is_none_or(|s| s > synced_seq) || fire_at > scheduled_until
}

/// Counts of one dispatch pass: pushes sent, skipped because an earlier pass sent them,
/// failed (retried next pass) and devices removed because APNs no longer knows their token.
/// `enabled` is false when no APNs key is configured.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, ToSchema)]
pub struct PushSummary {
    pub enabled: bool,
    pub sent: usize,
    pub skipped: usize,
    pub failed: usize,
    pub removed: usize,
}

enum Sent {
    Ok,
    Gone,
    Retry,
}

/// Sends due pushes to every registered device through an injected transport, so tests
/// use a fake APNs. Each push is claimed in `push_sent` before it is sent, so concurrent
/// passes never send one twice.
pub struct Pusher {
    pool: PgPool,
    vault: crate::vault::Vault,
    transport: Arc<dyn Transport>,
    signer: Signer,
    clock: Arc<dyn Clock>,
}

impl Pusher {
    /// Builds a pusher over `pool` (opening sealed run checkpoints with `vault`),
    /// sending through `transport` with `signer`.
    /// `clock` decides which reminders are due.
    /// Returns the pusher.
    pub fn new(pool: PgPool, vault: crate::vault::Vault, transport: Arc<dyn Transport>, signer: Signer, clock: Arc<dyn Clock>) -> Pusher {
        Pusher { pool, vault, transport, signer, clock }
    }

    async fn devices(&self) -> Result<Vec<Device>, ServerError> {
        let rows = db::query_scalar::<Value>("select to_jsonb(d) from public.devices d order by d.user_id, d.id").fetch_all(&self.pool).await?;
        rows.into_iter().map(|v| serde_json::from_value(v).map_err(|e| ServerError::Db(format!("devices: {e}")))).collect()
    }

    async fn seqs(&self, user: Uuid) -> Result<HashMap<String, i64>, ServerError> {
        let rows: Vec<(String, i64)> = db::query_as(
            "select id::text, server_seq from public.actions where user_id = $1 union all select id::text, server_seq from public.trackers where user_id = $1",
        )
        .bind(user)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().collect())
    }

    /// The run notices of `user` at `now`: one per human node of each run that entered
    /// `waiting_human` within `RUN_HOURS`, keyed by its checkpoint step, titled from `titles`. Runs of a deleted goal and
    /// runs whose checkpoint fails to load are skipped, so one bad run never stops a pass.
    async fn waiting_runs(&self, user: Uuid, titles: &HashMap<&str, &str>, now: DateTime<Utc>) -> Result<Vec<Notice>, ServerError> {
        let rows: Vec<(Uuid, Option<Uuid>, DateTime<Utc>)> =
            db::query_as("select id, goal_id, updated_at from public.runs where user_id = $1 and state = 'waiting_human' and updated_at > $2 order by created_at, id")
                .bind(user)
                .bind(now - Duration::hours(RUN_HOURS))
                .fetch_all(&self.pool)
                .await?;
        let mut out = Vec::new();
        for (run_id, goal_id, waiting_since) in rows {
            let goal = match goal_id {
                None => NO_GOAL_NAME,
                Some(g) => match titles.get(g.to_string().as_str()) {
                    Some(title) => title,
                    None => continue,
                },
            };
            let run = match crate::engine::latest_run(&self.pool, &self.vault, RunId(run_id)).await {
                Ok(run) => run,
                Err(_) => {
                    eprintln!("aroh-server: push skips run {run_id}, its checkpoint did not load");
                    continue;
                }
            };
            for ask in run.pending_human() {
                let Some((title, body)) = run_text(goal, &ask) else { continue };
                out.push(Notice::Run { run_id, goal_id: goal_id.map(|g| g.to_string()), node: ask.node, step: run.steps(), title, body, waiting_since });
            }
        }
        Ok(out)
    }

    /// The failed-run notices of `user` at `now`: one per run that turned `failed` within
    /// `FAIL_HOURS`, keyed by its checkpoint step, titled from `titles`. Runs of a deleted goal,
    /// runs the owner cancelled and runs whose checkpoint fails to load are skipped.
    async fn failed_runs(&self, user: Uuid, titles: &HashMap<&str, &str>, now: DateTime<Utc>) -> Result<Vec<Notice>, ServerError> {
        let rows: Vec<(Uuid, Option<Uuid>, DateTime<Utc>)> =
            db::query_as("select id, goal_id, updated_at from public.runs where user_id = $1 and state = 'failed' and updated_at > $2 order by created_at, id")
                .bind(user)
                .bind(now - Duration::hours(FAIL_HOURS))
                .fetch_all(&self.pool)
                .await?;
        let mut out = Vec::new();
        for (run_id, goal_id, failed_at) in rows {
            let goal = match goal_id {
                None => None,
                Some(g) => match titles.get(g.to_string().as_str()) {
                    Some(title) => Some(*title),
                    None => continue,
                },
            };
            let run = match crate::engine::latest_run(&self.pool, &self.vault, RunId(run_id)).await {
                Ok(run) => run,
                Err(_) => {
                    eprintln!("aroh-server: push skips failed run {run_id}, its checkpoint did not load");
                    continue;
                }
            };
            let Some(title) = fail_title(goal, run.stopped()) else { continue };
            out.push(Notice::RunFailed { run_id, goal_id: goal_id.map(|g| g.to_string()), step: run.steps(), title, failed_at });
        }
        Ok(out)
    }

    async fn notices(&self, user: Uuid, devices: &[&Device], now: DateTime<Utc>) -> Result<Vec<(Uuid, Notice)>, ServerError> {
        let goals = list_rows::<Goal>(&self.pool, user, &[], false).await?;
        let actions = list_rows::<Action>(&self.pool, user, &[("status", "planned".into())], false).await?;
        let trackers = list_rows::<Tracker>(&self.pool, user, &[], false).await?;
        let proposals = list_rows::<Proposal>(&self.pool, user, &[("status", "proposed".into())], false).await?;
        let seqs = self.seqs(user).await?;
        let titles: HashMap<&str, &str> = goals.iter().map(|g| (g.id.as_str(), g.title.as_str())).collect();
        let runs = self.waiting_runs(user, &titles, now).await?;
        let failed = self.failed_runs(user, &titles, now).await?;
        let mut out = Vec::new();
        for device in devices {
            let zone: Tz = device.zone.parse().unwrap_or(Tz::UTC);
            let since = now - Duration::minutes(LOOKBACK_MINUTES);
            for p in remind::plan(&goals, &actions, &trackers, since, zone).into_iter().filter(|p| p.fire_at <= now) {
                if should_push(seqs.get(&p.row_id).copied(), p.fire_at, device.synced_seq, device.scheduled_until) {
                    out.push((device.id, Notice::Reminder(p)));
                }
            }
            for proposal in proposals.iter().filter(|p| p.created_at > now - Duration::hours(PROPOSAL_HOURS) && p.created_at >= device.created_at) {
                let goal = titles.get(proposal.goal_id.as_str()).copied().unwrap_or("your goal");
                out.push((
                    device.id,
                    Notice::Proposal {
                        id: proposal.id.clone(),
                        goal_id: proposal.goal_id.clone(),
                        title: remind::truncate(format!("Plan change: {goal}")),
                        body: remind::truncate(proposal.summary.clone()),
                        created_at: proposal.created_at,
                    },
                ));
            }
            for run in runs.iter().filter(|r| matches!(r, Notice::Run { waiting_since, .. } if *waiting_since >= device.created_at)) {
                out.push((device.id, run.clone()));
            }
            for run in failed.iter().filter(|r| matches!(r, Notice::RunFailed { failed_at, .. } if *failed_at >= device.created_at)) {
                out.push((device.id, run.clone()));
            }
        }
        Ok(out)
    }

    async fn claim(&self, device: Uuid, key: &str, now: DateTime<Utc>) -> Result<bool, ServerError> {
        let done = db::query("insert into public.push_sent (device_id, key, sent_at) values ($1, $2, $3) on conflict do nothing")
            .bind(device)
            .bind(key)
            .bind(now)
            .execute(&self.pool)
            .await?;
        Ok(done.rows_affected() == 1)
    }

    async fn release(&self, device: Uuid, key: &str) -> Result<(), ServerError> {
        db::query("delete from public.push_sent where device_id = $1 and key = $2").bind(device).bind(key).execute(&self.pool).await?;
        Ok(())
    }

    async fn send(&self, device: &Device, push: &Push) -> Result<Sent, ServerError> {
        let req = request(push, &device.token, device.environment, self.signer.topic(), &self.signer.token()?);
        let mut res = match self.transport.send(req).await {
            Ok(res) => res,
            Err(_) => return Ok(Sent::Retry),
        };
        let (body, _) = read_limited(res.body.as_mut(), REPLY_MAX).await.unwrap_or_default();
        let reason = serde_json::from_slice::<Value>(&body).ok().and_then(|v| v["reason"].as_str().map(str::to_string)).unwrap_or_default();
        Ok(match res.status {
            200 => Sent::Ok,
            410 => Sent::Gone,
            400 if reason == "BadDeviceToken" => Sent::Gone,
            status => {
                eprintln!("aroh-server: push to device {} answered {status} {reason}", device.id);
                Sent::Retry
            }
        })
    }

    /// One dispatch pass at the clock's now: per device, due reminders the device will not
    /// show itself, fresh proposals, runs that started waiting for the owner within
    /// `RUN_HOURS` and runs that failed within `FAIL_HOURS`, each claimed then sent.
    /// Gone tokens delete the device; failures release the claim. Returns the pass's counts.
    pub async fn dispatch_once(&self) -> Result<PushSummary, ServerError> {
        let now = utc(self.clock.now_ms());
        let mut summary = PushSummary { enabled: true, ..PushSummary::default() };
        db::query("delete from public.push_sent where sent_at < $1").bind(now - Duration::days(LEDGER_DAYS)).execute(&self.pool).await?;
        let devices = self.devices().await?;
        let by_id: HashMap<Uuid, &Device> = devices.iter().map(|d| (d.id, d)).collect();
        let mut users: Vec<Uuid> = devices.iter().map(|d| d.user_id).collect();
        users.dedup();
        for user in users {
            let mine: Vec<&Device> = devices.iter().filter(|d| d.user_id == user).collect();
            for (device_id, notice) in self.notices(user, &mine, now).await? {
                let device = by_id[&device_id];
                let push = build(&notice);
                if !self.claim(device.id, &push.key, now).await? {
                    summary.skipped += 1;
                    continue;
                }
                match self.send(device, &push).await? {
                    Sent::Ok => summary.sent += 1,
                    Sent::Gone => {
                        if unregister(&self.pool, device.user_id, device.id).await? {
                            summary.removed += 1;
                        }
                    }
                    Sent::Retry => {
                        self.release(device.id, &push.key).await?;
                        summary.failed += 1;
                    }
                }
            }
        }
        Ok(summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;
    use ring::rand::SystemRandom;
    use ring::signature::{EcdsaKeyPair, KeyPair, UnparsedPublicKey, ECDSA_P256_SHA256_FIXED, ECDSA_P256_SHA256_FIXED_SIGNING};
    use troupekit_core::clock::FixedClock;

    fn key_pem() -> (String, Vec<u8>) {
        let rng = SystemRandom::new();
        let der = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &rng).unwrap();
        let pair = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, der.as_ref(), &rng).unwrap();
        let b64 = base64::engine::general_purpose::STANDARD.encode(der.as_ref());
        let lines: Vec<&str> = b64.as_bytes().chunks(64).map(|c| std::str::from_utf8(c).unwrap()).collect();
        (format!("-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n", lines.join("\n")), pair.public_key().as_ref().to_vec())
    }

    fn vars(pem: &str) -> BTreeMap<String, String> {
        BTreeMap::from([
            ("AROH_APNS_KEY".to_string(), pem.to_string()),
            ("AROH_APNS_KEY_ID".to_string(), "ABC123DEFG".to_string()),
            ("AROH_APNS_TEAM_ID".to_string(), "TEAM456789".to_string()),
            ("AROH_APNS_TOPIC".to_string(), "io.aroh.app".to_string()),
        ])
    }

    fn at(h: u32, m: u32) -> DateTime<Utc> {
        use chrono::TimeZone;
        Utc.with_ymd_and_hms(2026, 11, 22, h, m, 0).unwrap()
    }

    fn check_in() -> Planned {
        Planned {
            id: 1234567,
            fire_at: at(20, 0),
            title: "Check-in: Weight".into(),
            body: "Six pack by Feb 1".into(),
            kind: PlannedKind::CheckIn,
            row_id: "0190f2a0-0000-7000-8000-000000000007".into(),
        }
    }

    #[test]
    fn config_is_all_or_nothing_and_never_prints_the_key() {
        let (pem, _) = key_pem();
        assert!(ApnsConfig::from_vars(&BTreeMap::new()).unwrap().is_none());
        let mut partial = vars(&pem);
        partial.remove("AROH_APNS_TEAM_ID");
        assert!(ApnsConfig::from_vars(&partial).unwrap_err().to_string().contains("AROH_APNS_TEAM_ID"));
        let mut bad = vars("-----BEGIN PRIVATE KEY-----\nnot-a-key\n-----END PRIVATE KEY-----");
        let err = ApnsConfig::from_vars(&bad).unwrap_err().to_string();
        assert!(!err.contains("not-a-key"));
        bad = vars(&pem.replace('\n', "\\n"));
        let config = ApnsConfig::from_vars(&bad).unwrap().unwrap();
        assert!(!format!("{config:?}").contains("PRIVATE"));
        assert_eq!(config.topic, "io.aroh.app");
    }

    #[test]
    fn provider_token_verifies_and_is_reused_for_fifty_minutes() {
        let (pem, public) = key_pem();
        let clock = Arc::new(FixedClock::new(1_795_348_800_000));
        let signer = Signer::new(ApnsConfig::from_vars(&vars(&pem)).unwrap().unwrap(), clock.clone());
        let token = signer.token().unwrap();
        let parts: Vec<&str> = token.split('.').collect();
        let decode = |s: &str| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(s).unwrap();
        let header: Value = serde_json::from_slice(&decode(parts[0])).unwrap();
        assert_eq!((header["alg"].as_str(), header["kid"].as_str()), (Some("ES256"), Some("ABC123DEFG")));
        let claims: Value = serde_json::from_slice(&decode(parts[1])).unwrap();
        assert_eq!(claims, json!({"iss": "TEAM456789", "iat": 1_795_348_800}));
        let signed = format!("{}.{}", parts[0], parts[1]);
        UnparsedPublicKey::new(&ECDSA_P256_SHA256_FIXED, &public).verify(signed.as_bytes(), &decode(parts[2])).unwrap();
        clock.advance(49 * 60 * 1000);
        assert_eq!(signer.token().unwrap(), token);
        clock.advance(2 * 60 * 1000);
        assert_ne!(signer.token().unwrap(), token);
    }

    #[test]
    fn reminder_and_proposal_pushes_carry_ids_and_headers() {
        let push = build(&Notice::Reminder(check_in()));
        assert_eq!(push.key, format!("check_in:1234567:{}", at(20, 0).timestamp()));
        assert_eq!(push.collapse_id, "1234567");
        assert_eq!(push.expires_at, at(21, 0));
        let req = request(&push, &"ab".repeat(32), ApnsEnv::Sandbox, "io.aroh.app", "jwt-value");
        assert_eq!(req.url, format!("https://api.sandbox.push.apple.com/3/device/{}", "ab".repeat(32)));
        assert_eq!(req.header("authorization"), Some("bearer jwt-value"));
        assert_eq!(req.header("apns-topic"), Some("io.aroh.app"));
        assert_eq!(req.header("apns-push-type"), Some("alert"));
        assert_eq!(req.header("apns-collapse-id"), Some("1234567"));
        assert_eq!(req.header("apns-expiration"), Some(at(21, 0).timestamp().to_string().as_str()));
        let proposal = build(&Notice::Proposal { id: "p1".into(), goal_id: "g1".into(), title: "Plan change: Run".into(), body: "Move the long run".into(), created_at: at(9, 0) });
        assert_eq!(proposal.payload["aroh"], json!({"kind": "proposal", "id": "p1", "goal_id": "g1"}));
        assert_eq!(proposal.payload["aps"]["thread-id"], "proposal");
        let action = Planned { kind: PlannedKind::Action, ..check_in() };
        assert_eq!(build(&Notice::Reminder(action)).payload["aroh"]["kind"], "reminder");
    }

    fn ask(node: &str, data: Value) -> HumanAsk {
        HumanAsk { node: node.into(), ask: troupekit_graph::Ask::Approval, prompt: None, data: data.as_object().unwrap().clone() }
    }

    #[test]
    fn run_pushes_name_the_run_node_and_goal() {
        let run_id = Uuid::parse_str("0190f2a0-0000-7000-8000-000000000009").unwrap();
        let notice = |goal_id: Option<String>| Notice::Run {
            run_id,
            goal_id,
            node: "questions".into(),
            step: 7,
            title: "Gym: your answers are needed".into(),
            body: "Tap to answer in Aroh.".into(),
            waiting_since: at(9, 0),
        };
        let push = build(&notice(Some("g1".into())));
        assert_eq!(push.key, format!("run:{run_id}:questions:7"));
        let longest = Notice::Run { run_id, goal_id: None, node: "n".repeat(64), step: u64::MAX, title: String::new(), body: String::new(), waiting_since: at(9, 0) };
        assert!(build(&longest).key.len() <= 200);
        assert_eq!(push.collapse_id, run_id.to_string());
        assert!(push.collapse_id.len() <= 64);
        assert_eq!(push.expires_at, at(15, 0));
        assert_eq!(push.payload["aroh"], json!({"kind": "run", "id": run_id.to_string(), "goal_id": "g1", "node": "questions"}));
        assert_eq!(push.payload["aps"]["thread-id"], "run");
        assert_eq!(push.payload["aps"]["alert"], json!({"title": "Gym: your answers are needed", "body": "Tap to answer in Aroh."}));
        assert_eq!(build(&notice(None)).payload["aroh"]["goal_id"], Value::Null);

        let plan = run_text("Gym", &ask("approve", json!({"plan": {"summary": "Three runs"}}))).unwrap();
        assert_eq!(plan, ("Gym: plan ready to approve".to_string(), "Tap to review the plan.".to_string()));
        assert_eq!(run_text("Gym", &ask("approve", json!({"proposal_record": {"summary": "Move it"}}))), None);
        let questions = run_text(NO_GOAL_NAME, &ask("questions", json!({"intake": {"questions": []}}))).unwrap();
        assert_eq!(questions.0, "Calendar: your answers are needed");
        assert_eq!(run_text("Gym", &ask("limit_choice", json!({"limits": {}}))).unwrap().0, "Gym: your answers are needed");
        assert!(run_text(&"x".repeat(300), &ask("change", json!({}))).unwrap().0.chars().count() <= 120);
    }

    #[test]
    fn failed_runs_push_once_per_step_without_the_error() {
        let run_id = Uuid::parse_str("0190f2a0-0000-7000-8000-000000000009").unwrap();
        let error = "node `write`: provider said: secret diary words";
        let title = fail_title(Some("Gym"), None).unwrap();
        assert_eq!(title, "Gym: the run stopped");
        let notice = Notice::RunFailed { run_id, goal_id: Some("g1".into()), step: 12, title, failed_at: at(9, 0) };
        let push = build(&notice);
        assert_eq!(push.key, format!("fail:{run_id}:12"));
        assert_eq!(push.collapse_id, run_id.to_string());
        assert_eq!(push.expires_at, at(11, 0));
        assert_eq!(push.payload["aroh"], json!({"kind": "run_failed", "id": run_id.to_string(), "goal_id": "g1"}));
        assert_eq!(push.payload["aps"]["thread-id"], "run_failed");
        assert_eq!(push.payload["aps"]["alert"], json!({"title": "Gym: the run stopped", "body": FAIL_BODY}));
        assert!(!push.payload.to_string().contains(error));
        let again = Notice::RunFailed { run_id, goal_id: None, step: 14, title: fail_title(None, None).unwrap(), failed_at: at(9, 0) };
        assert_ne!(build(&again).key, push.key);
        assert_eq!(build(&again).payload["aroh"]["goal_id"], Value::Null);
        assert_eq!(build(&again).payload["aps"]["alert"]["title"], NO_GOAL_FAIL_TITLE);

        assert_eq!(fail_title(Some("Gym"), Some(&Stop::Cancelled)), None);
        assert_eq!(fail_title(Some("Gym"), Some(&Stop::TimeLimit { limit_ms: 300_000 })).unwrap(), "Gym: the run stopped");
        assert!(fail_title(Some(&"x".repeat(300)), None).unwrap().chars().count() <= 120);
    }

    #[test]
    fn covered_reminders_are_left_to_the_device() {
        let until = at(20, 0);
        assert!(should_push(None, at(9, 0), 100, until));
        assert!(should_push(Some(101), at(9, 0), 100, until));
        assert!(should_push(Some(50), at(20, 1), 100, until));
        assert!(!should_push(Some(100), at(20, 0), 100, until));
    }

    #[test]
    fn the_simulator_payload_is_what_the_server_sends() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../e2e/pod13/payload.json");
        let committed: Value = serde_json::from_str(&std::fs::read_to_string(path).expect("e2e/pod13/payload.json exists")).unwrap();
        assert_eq!(committed, build(&Notice::Reminder(check_in())).payload);
    }

    #[test]
    fn bad_device_registrations_are_refused() {
        let ok = RegisterDevice { platform: Platform::Ios, token: "ab".repeat(32), environment: ApnsEnv::Sandbox, zone: "Europe/Berlin".into(), synced_seq: 0, scheduled_until: at(9, 0) };
        assert!(check_device(&ok).is_ok());
        assert!(check_device(&RegisterDevice { token: "AB".repeat(32), ..ok.clone() }).is_err());
        assert!(check_device(&RegisterDevice { token: "ab".repeat(20), ..ok.clone() }).is_err());
        assert!(check_device(&RegisterDevice { zone: "Mars/Olympus".into(), ..ok.clone() }).is_err());
        assert!(check_device(&RegisterDevice { synced_seq: -1, ..ok }).is_err());
    }
}
