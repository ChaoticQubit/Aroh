mod common;

use aroh_domain::rows::{Action, ActionStatus, Autonomy, Goal, GoalStatus, SearchSetting, Tracker, TrackerField, FieldKind};
use aroh_server::api::{openapi_yaml, router, AppState};
use aroh_server::auth::Verifier;
use aroh_server::db::{self, PgPool};
use aroh_server::engine::Engine;
use aroh_server::rows::{put_row, utc, ServerClock};
use chrono::{DateTime, Utc};
use aroh_server::fake::{body_text, json_reply, sse_text, sse_tool, FnTransport, Reply};
use common::{deps, offline, pg, user, T0};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use troupekit_core::http::{read_limited, HttpRequest, Method, Transport};
use troupekit_providers::reqwest_transport::ReqwestTransport;
use uuid::Uuid;

const SECRET: &str = "engine-test-jwt-secret-0123456789abcdef";
const MODEL_KEY: &str = "sk-ant-test-MODELKEY-4321";
const BRAVE_KEY: &str = "BSA-test-BRAVEKEY-8765";

struct Api {
    base: String,
    http: ReqwestTransport,
    pool: PgPool,
    hlc: ServerClock,
    seen: Mutex<Vec<String>>,
}

fn token(claims: Value) -> String {
    encode(&Header::new(Algorithm::HS256), &claims, &EncodingKey::from_secret(SECRET.as_bytes())).unwrap()
}

fn user_token(user: Uuid) -> String {
    token(json!({"sub": user, "role": "authenticated", "aud": "authenticated", "exp": 4_000_000_000u64}))
}

fn service_token() -> String {
    token(json!({"role": "service_role", "exp": 4_000_000_000u64}))
}

async fn api(models: Arc<FnTransport>, web: Arc<FnTransport>) -> Api {
    let pool = pg().await;
    let (deps, clock) = deps(&pool, models, web);
    let verifier = Verifier::new(Some(SECRET), None, "authenticated", clock);
    let state = AppState { engine: Arc::new(Engine::new(deps)), verifier: Arc::new(verifier) };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, router(state)).await.unwrap() });
    Api { base, http: ReqwestTransport::new().unwrap(), pool, hlc: ServerClock::default(), seen: Mutex::new(Vec::new()) }
}

impl Api {
    async fn call(&self, method: Method, path: &str, auth: Option<&str>, body: Option<Value>) -> (u16, Value) {
        let mut headers = vec![("content-type".to_string(), "application/json".to_string())];
        if let Some(t) = auth {
            headers.push(("authorization".into(), format!("Bearer {t}")));
        }
        let req = HttpRequest {
            method,
            url: format!("{}{path}", self.base),
            headers,
            body: body.map(|b| b.to_string().into_bytes()),
            accept: vec![],
        };
        let mut res = self.http.send(req).await.unwrap();
        let (bytes, _) = read_limited(res.body.as_mut(), 1 << 20).await.unwrap();
        let text = String::from_utf8(bytes).unwrap();
        self.seen.lock().unwrap().push(text.clone());
        (res.status, serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    async fn tick(&self) -> Value {
        let (status, report) = self.call(Method::Post, "/v1/tick", Some(&service_token()), None).await;
        assert_eq!(status, 200, "{report}");
        assert_eq!(report["errors"], json!([]), "{report}");
        report
    }

    async fn run(&self, t: &str, run_id: &str) -> Value {
        let (status, run) = self.call(Method::Get, &format!("/v1/runs/{run_id}"), Some(t), None).await;
        assert_eq!(status, 200, "{run}");
        run
    }

    async fn answer(&self, t: &str, run_id: &str, node: &str, answer: Value) {
        let (status, body) =
            self.call(Method::Post, &format!("/v1/runs/{run_id}/answers"), Some(t), Some(json!({"node": node, "answer": answer}))).await;
        assert_eq!(status, 202, "{body}");
    }

    async fn signed_up(&self) -> (Uuid, String) {
        let u = user(&self.pool).await;
        let t = user_token(u);
        let body = json!({"vendor": "anthropic", "key": MODEL_KEY, "model": "claude-sonnet-5-5", "label": "Main", "is_default": true});
        let (status, info) = self.call(Method::Put, &format!("/v1/keys/{}", Uuid::now_v7()), Some(&t), Some(body)).await;
        assert_eq!(status, 200, "{info}");
        assert_eq!(info["last4"], "4321");
        (u, t)
    }

    async fn goal(&self, user: Uuid, autonomy: Autonomy) -> Uuid {
        let id = Uuid::now_v7();
        let mut g = Goal {
            id: id.to_string(),
            user_id: user.to_string(),
            category_id: None,
            title: "Run a 5k".into(),
            context: None,
            deadline: None,
            status: GoalStatus::Active,
            autonomy,
            created_at: utc(T0),
            updated_at: String::new(),
            deleted: false,
        };
        put_row(&self.pool, &self.hlc, user, &mut g).await.unwrap();
        id
    }

    async fn action(&self, user: Uuid, goal: Uuid, start: &str, locked: bool) -> Action {
        let start: DateTime<Utc> = start.parse().unwrap();
        let mut a = Action {
            id: Uuid::now_v7().to_string(),
            user_id: user.to_string(),
            goal_id: goal.to_string(),
            series_id: None,
            occurrence: None,
            title: "Easy run".into(),
            notes: None,
            start_at: start,
            end_at: start + chrono::Duration::minutes(30),
            status: ActionStatus::Planned,
            locked,
            done_at: None,
            updated_at: String::new(),
            deleted: false,
        };
        put_row(&self.pool, &self.hlc, user, &mut a).await.unwrap();
        a
    }

    async fn count(&self, sql: &str, id: &str) -> i64 {
        db::query_scalar(sql).bind(id).fetch_one(&self.pool).await.unwrap()
    }

    async fn assert_no_secret_stored(&self, secrets: &[&str]) {
        let tables: Vec<String> =
            db::query_scalar("select table_name::text from information_schema.tables where table_schema = 'public' and table_type = 'BASE TABLE'")
                .fetch_all(&self.pool)
                .await
                .unwrap();
        assert!(tables.len() >= 26);
        for t in tables {
            for s in secrets {
                let n: i64 = db::query_scalar(&format!("select count(*) from public.{t} r where to_jsonb(r)::text like '%' || $1 || '%'"))
                    .bind(s)
                    .fetch_one(&self.pool)
                    .await
                    .unwrap();
                assert_eq!(n, 0, "{s} stored in {t}");
            }
        }
        for body in self.seen.lock().unwrap().iter() {
            for s in secrets {
                assert!(!body.contains(s), "{s} in a response");
            }
        }
    }
}

fn plan() -> Value {
    json!({
        "summary": "Three easy runs a week and new shoes.",
        "timezone": "Europe/Berlin",
        "items": [
            {"title": "Easy run", "start": "2026-11-23T07:00:00", "minutes": 30, "rrule": "FREQ=WEEKLY;BYDAY=MO,WE,FR", "notes": null},
            {"title": "Buy running shoes", "start": "2026-11-24T18:00:00", "minutes": 45, "rrule": null, "notes": "Gait check"}
        ],
        "trackers": [{"name": "Runs", "fields": [
            {"key": "distance_km", "label": "Distance", "kind": "number", "unit": "km"},
            {"key": "felt_good", "label": "Felt good", "kind": "bool", "unit": null}
        ], "cadence": null}]
    })
}

const RUNS_PLUGIN: &str = r#"
aroh_plugin = 1
name = "running"
title = "Running"

[[trackers]]
name = "Runs"

[[trackers.fields]]
key = "distance_km"
label = "Distance"
kind = "number"
unit = "km"
target = 5

[[trackers.fields]]
key = "felt_good"
label = "Felt good"
kind = "bool"

[[trackers]]
name = "Sleep"

[[trackers.fields]]
key = "hours"
label = "Sleep"
kind = "number"

[[sections]]
title = "Progress"

[[sections.items]]
kind = "target_vs_actual"
label = "Longest run"
unit = "count"
target = { bind = "target", tracker = "Runs", field = "distance_km" }
actual = { bind = "latest", tracker = "Runs", field = "distance_km" }

[[sections.items]]
kind = "bar_chart"
label = "Sleep"
unit = "count"
points = { bind = "series", tracker = "Sleep", field = "hours" }

[[sections]]
title = "Upcoming"

[[sections.items]]
kind = "action_list"
label = "Next runs"
actions = { bind = "actions", when = "next", limit = 3 }
"#;

const REPAIR: &str = "Your answer failed these checks";

fn builder_reply(bundle: &str) -> Reply {
    sse_text(&json!({"bundle_toml": bundle}).to_string())
}

fn goal_models() -> Arc<FnTransport> {
    goal_models_with(|_| builder_reply(RUNS_PLUGIN))
}

fn goal_models_with(builder: impl Fn(&str) -> Reply + Send + Sync + 'static) -> Arc<FnTransport> {
    FnTransport::new(move |req| {
        let body = body_text(req);
        if body.contains("You are Plugin builder") {
            return builder(&body);
        }
        if !body.contains("\"stream\":true") {
            return json_reply(200, json!({"id": "msg_k", "type": "message", "role": "assistant", "content": [{"type": "text", "text": "OK"}],
                "stop_reason": "max_tokens", "usage": {"input_tokens": 5, "output_tokens": 1}}));
        }
        if body.contains("You are Intake interviewer") {
            return sse_text(&json!({"questions": [{"id": "weekly_runs", "text": "How many runs a week?", "kind": "number", "choices": []}]}).to_string());
        }
        if body.contains("You are Goal researcher") {
            if body.contains("tool_result") {
                return sse_text(&json!({"summary": "Walk-run intervals three times a week.", "sources": [{"title": "C25K", "url": "https://example.org/c25k"}]}).to_string());
            }
            return sse_tool("web_search", json!({"query": "couch to 5k plan"}));
        }
        if body.contains("You are Goal planner") {
            return sse_text(&plan().to_string());
        }
        Reply { status: 400, content_type: "application/json", body: json!({"error": "unexpected request"}).to_string() }
    })
}

fn brave() -> Arc<FnTransport> {
    FnTransport::new(|req| {
        if req.url.starts_with("https://api.search.brave.com/res/v1/web/search") {
            return json_reply(200, json!({"web": {"results": [{"title": "C25K", "url": "https://example.org/c25k", "description": "Couch to 5k"}]}}));
        }
        json_reply(404, json!({}))
    })
}

#[test]
fn openapi_document_is_committed() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/engine-api.yaml");
    let committed = std::fs::read_to_string(path).expect("docs/engine-api.yaml exists");
    assert_eq!(committed, openapi_yaml(), "regenerate with `aroh-server openapi > docs/engine-api.yaml`");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn new_goal_to_written_actions() {
    let models = goal_models();
    let web = brave();
    let api = api(models.clone(), web.clone()).await;
    let (u, t) = api.signed_up().await;
    let search_id = Uuid::now_v7();
    let mut search = SearchSetting {
        id: search_id.to_string(),
        user_id: u.to_string(),
        label: "Brave".into(),
        provider: "brave".into(),
        base_url: None,
        priority: 0,
        enabled: true,
        updated_at: String::new(),
        deleted: false,
    };
    put_row(&api.pool, &api.hlc, u, &mut search).await.unwrap();
    let (status, _) = api.call(Method::Put, &format!("/v1/keys/{search_id}"), Some(&t), Some(json!({"vendor": "brave", "key": BRAVE_KEY}))).await;
    assert_eq!(status, 200);
    let (status, keys) = api.call(Method::Get, "/v1/keys", Some(&t), None).await;
    assert_eq!((status, keys.as_array().unwrap().len()), (200, 2));

    let (status, started) = api.call(Method::Post, "/v1/goals", Some(&t), Some(json!({"title": "Run a 5k", "timezone": "Europe/Berlin"}))).await;
    assert_eq!(status, 202, "{started}");
    let (goal, run) = (started["goal_id"].as_str().unwrap().to_string(), started["run_id"].as_str().unwrap().to_string());

    api.tick().await;
    let view = api.run(&t, &run).await;
    assert_eq!(view["status"]["status"], "waiting_human", "{view}");
    assert_eq!(view["waiting"][0]["node"], "questions");
    assert_eq!(view["waiting"][0]["data"]["intake"]["questions"][0]["id"], "weekly_runs");
    api.answer(&t, &run, "questions", json!([{"id": "weekly_runs", "answer": "3"}])).await;
    let (status, _) = api.call(Method::Post, &format!("/v1/runs/{run}/answers"), Some(&t), Some(json!({"node": "questions", "answer": []}))).await;
    assert_eq!(status, 409, "a node answers once");

    api.tick().await;
    let view = api.run(&t, &run).await;
    assert_eq!(view["waiting"][0]["node"], "approve", "{view}");
    assert_eq!(view["waiting"][0]["data"]["plan"]["items"][0]["title"], "Easy run");
    api.answer(&t, &run, "approve", json!(false)).await;
    api.tick().await;
    assert_eq!(api.run(&t, &run).await["waiting"][0]["node"], "change");
    api.answer(&t, &run, "change", json!("Keep it as is.")).await;
    api.tick().await;
    assert_eq!(api.run(&t, &run).await["waiting"][0]["node"], "approve");
    api.answer(&t, &run, "approve", json!(true)).await;
    api.tick().await;

    let view = api.run(&t, &run).await;
    assert_eq!(view["status"]["status"], "done", "{view}");
    assert!(view["events"].as_array().unwrap().len() > 5);
    let after = api.run(&t, &format!("{run}?after_seq=3")).await;
    assert!(after["events"].as_array().unwrap().iter().all(|e| e["seq"].as_u64().unwrap() > 3));
    assert_eq!(api.count("select count(*) from public.actions where goal_id::text = $1 and not deleted", &goal).await, 7);
    assert_eq!(api.count("select count(*) from public.action_series where goal_id::text = $1", &goal).await, 1);
    assert_eq!(api.count("select count(*) from public.trackers where goal_id::text = $1", &goal).await, 2, "Runs from the plan, Sleep from the plugin");
    assert_eq!(api.count("select count(*) from public.plugins where goal_id::text = $1 and enabled and version = 1", &goal).await, 1);    let status: String = db::query_scalar("select status from public.goals where id::text = $1").bind(&goal).fetch_one(&api.pool).await.unwrap();
    assert_eq!(status, "active");
    let one_off: DateTime<Utc> = db::query_scalar("select start_at from public.actions where goal_id::text = $1 and series_id is null")
        .bind(&goal)
        .fetch_one(&api.pool)
        .await
        .unwrap();
    assert_eq!(one_off.to_rfc3339(), "2026-11-24T17:00:00+00:00");
    let model_calls = models.sent().iter().filter(|r| body_text(r).contains("\"stream\":true")).count();
    assert_eq!(model_calls, 6, "intake, research twice, plan, revise, plugin");
    assert_eq!(api.count("select count(*) from public.usage where run_id::text = $1", &run).await, 6);
    assert!(models.sent().iter().all(|r| r.header("x-api-key") == Some(MODEL_KEY)));
    let searches = web.sent();
    assert_eq!(searches.len(), 1);
    assert_eq!(searches[0].header("X-Subscription-Token"), Some(BRAVE_KEY));
    let (status, runs) = api.call(Method::Get, &format!("/v1/runs?state=done&goal_id={goal}"), Some(&t), None).await;
    assert_eq!((status, runs[0]["run_id"].as_str()), (200, Some(run.as_str())));
    let (_, none) = api.call(Method::Get, &format!("/v1/runs?goal_id={}", Uuid::now_v7()), Some(&t), None).await;
    assert_eq!(none, json!([]));

    let setting = keys.as_array().unwrap().iter().find(|k| k["vendor"] == "anthropic").unwrap()["setting_id"].as_str().unwrap().to_string();
    let (status, test) = api.call(Method::Post, &format!("/v1/keys/{setting}/test"), Some(&t), None).await;
    assert_eq!((status, test["ok"].as_bool()), (200, Some(true)), "{test}");
    api.assert_no_secret_stored(&[MODEL_KEY, BRAVE_KEY]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn users_never_see_each_other_and_roles_are_enforced() {
    let api = api(goal_models(), offline()).await;
    let (_a, ta) = api.signed_up().await;
    let (_b, tb) = api.signed_up().await;
    let (_, started) = api.call(Method::Post, "/v1/goals", Some(&ta), Some(json!({"title": "Run a 5k", "timezone": "UTC"}))).await;
    let run = started["run_id"].as_str().unwrap();
    assert_eq!(api.call(Method::Get, &format!("/v1/runs/{run}"), Some(&tb), None).await.0, 404);
    assert_eq!(api.call(Method::Get, "/v1/runs", Some(&tb), None).await.1, json!([]));
    assert_eq!(api.call(Method::Get, "/v1/keys", Some(&tb), None).await.1.as_array().unwrap().len(), 1);
    let answer = json!({"node": "questions", "answer": []});
    assert_eq!(api.call(Method::Post, &format!("/v1/runs/{run}/answers"), Some(&tb), Some(answer)).await.0, 404);
    assert_eq!(api.call(Method::Get, "/v1/keys", None, None).await.0, 401);
    assert_eq!(api.call(Method::Get, "/v1/keys", Some("garbage"), None).await.0, 401);
    assert_eq!(api.call(Method::Post, "/v1/tick", Some(&ta), None).await.0, 403);
    assert_eq!(api.call(Method::Get, "/v1/keys", Some(&service_token()), None).await.0, 403);
    assert_eq!(api.call(Method::Post, "/v1/hooks/enqueue", Some(&service_token()), Some(json!({"type": "INSERT"}))).await.0, 200);
    let no_key = user(&api.pool).await;
    let (status, err) = api.call(Method::Post, "/v1/goals", Some(&user_token(no_key)), Some(json!({"title": "x", "timezone": "UTC"}))).await;
    assert_eq!((status, err["error"]["code"].as_str()), (422, Some("setup")), "{err}");
    let (status, err) = api.call(Method::Put, &format!("/v1/keys/{}", Uuid::now_v7()), Some(&ta), Some(json!({"vendor": "anthropic", "key": MODEL_KEY}))).await;
    assert_eq!(status, 422, "{err}");
    let (status, _) = api.call(Method::Post, "/v1/goals", Some(&ta), Some(json!({"title": "x", "timezone": "Mars/Base"}))).await;
    assert_eq!(status, 400);
    api.assert_no_secret_stored(&[MODEL_KEY]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn replan_applies_small_changes_and_asks_when_it_must() {
    let missed_id = Arc::new(Mutex::new(String::new()));
    let id = missed_id.clone();
    let models = FnTransport::new(move |req| {
        let body = body_text(req);
        let target = id.lock().unwrap().clone();
        if body.contains("You are Adaptation analyst") {
            let change = json!([{"op": "move", "action_id": target, "title": "Easy run", "start": "2026-11-23T18:00:00", "minutes": 30}]);
            return sse_text(&json!({"summary": "Move the missed run to Monday evening.", "size": "small", "changes": change}).to_string());
        }
        json_reply(400, json!({}))
    });
    let api = api(models, offline()).await;
    let (u, t) = api.signed_up().await;
    let balanced = api.goal(u, Autonomy::Balanced).await;
    let missed = api.action(u, balanced, "2026-11-21T07:00:00Z", false).await;
    *missed_id.lock().unwrap() = missed.id.clone();
    let reason = json!({"reason": "I missed Saturday's run.", "timezone": "Europe/Berlin"});
    let (status, started) = api.call(Method::Post, &format!("/v1/goals/{balanced}/replan"), Some(&t), Some(reason.clone())).await;
    assert_eq!(status, 202, "{started}");
    api.tick().await;
    let run = started["run_id"].as_str().unwrap();
    assert_eq!(api.run(&t, run).await["status"]["status"], "done");
    let moved: DateTime<Utc> = db::query_scalar("select start_at from public.actions where id::text = $1").bind(&missed.id).fetch_one(&api.pool).await.unwrap();
    assert_eq!(moved.to_rfc3339(), "2026-11-23T17:00:00+00:00");
    let (status, undo): (String, Value) = db::query_as("select status, undo from public.proposals where goal_id::text = $1")
        .bind(balanced.to_string())
        .fetch_one(&api.pool)
        .await
        .unwrap();
    assert_eq!(status, "applied");
    assert_eq!(undo["changes"][0]["before"]["start_at"], "2026-11-21T07:00:00Z");

    let asks = api.goal(u, Autonomy::Ask).await;
    let other = api.action(u, asks, "2026-11-21T08:00:00Z", false).await;
    *missed_id.lock().unwrap() = other.id.clone();
    let (_, started) = api.call(Method::Post, &format!("/v1/goals/{asks}/replan"), Some(&t), Some(reason)).await;
    let run = started["run_id"].as_str().unwrap();
    api.tick().await;
    let view = api.run(&t, run).await;
    assert_eq!(view["waiting"][0]["node"], "approve", "{view}");
    let record = &view["waiting"][0]["data"]["proposal_record"];
    assert_eq!(record["changes"][0]["before"]["start_at"], "2026-11-21T08:00:00Z");
    assert_eq!(record["changes"][0]["after"]["start_at"], "2026-11-23T17:00:00Z");
    api.answer(&t, run, "approve", json!(false)).await;
    api.tick().await;
    assert_eq!(api.run(&t, run).await["status"]["status"], "done");
    let status: String = db::query_scalar("select status from public.proposals where goal_id::text = $1").bind(asks.to_string()).fetch_one(&api.pool).await.unwrap();
    assert_eq!(status, "rejected");
    let kept: DateTime<Utc> = db::query_scalar("select start_at from public.actions where id::text = $1").bind(&other.id).fetch_one(&api.pool).await.unwrap();
    assert_eq!(kept.to_rfc3339(), "2026-11-21T08:00:00+00:00");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn locked_actions_force_approval_and_are_never_moved() {
    let locked_id = Arc::new(Mutex::new(String::new()));
    let id = locked_id.clone();
    let models = FnTransport::new(move |req| {
        let target = id.lock().unwrap().clone();
        let change = json!([{"op": "move", "action_id": target, "title": "Easy run", "start": "2026-11-23T18:00:00", "minutes": 30}]);
        if body_text(req).contains("You are Adaptation analyst") {
            return sse_text(&json!({"summary": "Move it.", "size": "small", "changes": change}).to_string());
        }
        json_reply(400, json!({}))
    });
    let api = api(models, offline()).await;
    let (u, t) = api.signed_up().await;
    let goal = api.goal(u, Autonomy::Auto).await;
    let locked = api.action(u, goal, "2026-11-23T06:00:00Z", true).await;
    *locked_id.lock().unwrap() = locked.id.clone();
    let (_, started) = api.call(Method::Post, &format!("/v1/goals/{goal}/replan"), Some(&t), Some(json!({"reason": "Busy", "timezone": "Europe/Berlin"}))).await;
    let run = started["run_id"].as_str().unwrap();
    api.tick().await;
    let view = api.run(&t, run).await;
    assert_eq!(view["waiting"][0]["data"]["proposal_record"]["touches_locked"], true, "{view}");
    api.answer(&t, run, "approve", json!(true)).await;
    api.tick().await;
    let kept: DateTime<Utc> = db::query_scalar("select start_at from public.actions where id::text = $1").bind(&locked.id).fetch_one(&api.pool).await.unwrap();
    assert_eq!(kept.to_rfc3339(), "2026-11-23T06:00:00+00:00");
    let view = api.run(&t, run).await;
    assert_eq!(view["status"]["status"], "done");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn free_text_log_becomes_tracker_entries() {
    let tracker_id = Arc::new(Mutex::new(String::new()));
    let id = tracker_id.clone();
    let models = FnTransport::new(move |req| {
        let target = id.lock().unwrap().clone();
        if body_text(req).contains("You are Log intake") {
            let entries = json!({"entries": [{"tracker_id": target, "values": [{"key": "distance_km", "value": "5.2"}, {"key": "felt_good", "value": "true"}]}]});
            return sse_text(&entries.to_string());
        }
        json_reply(400, json!({}))
    });
    let api = api(models, offline()).await;
    let (u, t) = api.signed_up().await;
    let goal = api.goal(u, Autonomy::Balanced).await;
    let field = |key: &str, kind: FieldKind| TrackerField { key: key.into(), label: key.into(), kind, unit: None, choices: vec![], health: None };
    let mut tracker = Tracker {
        id: Uuid::now_v7().to_string(),
        user_id: u.to_string(),
        goal_id: goal.to_string(),
        plugin_id: None,
        name: "Runs".into(),
        fields: vec![field("distance_km", FieldKind::Number), field("felt_good", FieldKind::Bool)],
        cadence: None,
        rules: vec![],
        updated_at: String::new(),
        deleted: false,
    };
    put_row(&api.pool, &api.hlc, u, &mut tracker).await.unwrap();
    *tracker_id.lock().unwrap() = tracker.id.clone();
    let (status, started) = api.call(Method::Post, "/v1/logs", Some(&t), Some(json!({"text": "Ran 5.2 km, felt great", "timezone": "Europe/Berlin"}))).await;
    assert_eq!(status, 202, "{started}");
    api.tick().await;
    assert_eq!(api.run(&t, started["run_id"].as_str().unwrap()).await["status"]["status"], "done");
    let (data, source): (Value, String) = db::query_as("select data, source from public.tracker_entries where tracker_id::text = $1")
        .bind(&tracker.id)
        .fetch_one(&api.pool)
        .await
        .unwrap();
    assert_eq!(data, json!({"distance_km": 5.2, "felt_good": true}));
    assert_eq!(source, "log");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn unverified_prices_pause_before_any_call() {
    let models = goal_models();
    let api = api(models.clone(), offline()).await;
    let u = user(&api.pool).await;
    let t = user_token(u);
    let body = json!({"vendor": "openai", "key": "sk-proj-test-OPENAI-0001", "model": "gpt-6.2-sol", "is_default": true});
    assert_eq!(api.call(Method::Put, &format!("/v1/keys/{}", Uuid::now_v7()), Some(&t), Some(body)).await.0, 200);
    let (status, started) = api.call(Method::Post, "/v1/goals", Some(&t), Some(json!({"title": "Run a 5k", "timezone": "UTC"}))).await;
    assert_eq!(status, 202, "{started}");
    api.tick().await;
    let view = api.run(&t, started["run_id"].as_str().unwrap()).await;
    assert_eq!(view["status"]["status"], "waiting_budget", "{view}");
    assert!(models.sent().is_empty());
}

impl Api {
    /// Runs a new-goal flow to its end: questions answered, plan approved,
    /// then the plugin step. Returns the goal id, run id and final run view.
    /// Ticks without asserting on errors, so a failed run can be inspected.
    async fn approved_goal(&self, t: &str) -> (String, String, Value) {
        let (status, started) = self.call(Method::Post, "/v1/goals", Some(t), Some(json!({"title": "Run a 5k", "timezone": "Europe/Berlin"}))).await;
        assert_eq!(status, 202, "{started}");
        let (goal, run) = (started["goal_id"].as_str().unwrap().to_string(), started["run_id"].as_str().unwrap().to_string());
        self.tick().await;
        self.answer(t, &run, "questions", json!([{"id": "weekly_runs", "answer": "3"}])).await;
        self.tick().await;
        self.answer(t, &run, "approve", json!(true)).await;
        self.call(Method::Post, "/v1/tick", Some(&service_token()), None).await;
        let view = self.run(t, &run).await;
        (goal, run, view)
    }
}

fn builder_calls(models: &FnTransport) -> usize {
    models.sent().iter().filter(|r| body_text(r).contains("You are Plugin builder")).count()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn a_bad_generated_plugin_is_repaired_once_then_stored() {
    let models = goal_models_with(|body| {
        if body.contains(REPAIR) {
            assert!(body.contains("plugin.toml: sections[0].items[0].points.tracker"), "the repair names the field");
            return builder_reply(RUNS_PLUGIN);
        }
        builder_reply(&RUNS_PLUGIN.replace("tracker = \"Sleep\", field = \"hours\"", "tracker = \"Slep\", field = \"hours\"").replacen("[[sections.items]]\nkind = \"target_vs_actual\"", "[[sections.items]]\nkind = \"bar_chart\"\nlabel = \"x\"\nunit = \"count\"\npoints = { bind = \"series\", tracker = \"Slep\", field = \"hours\" }\n\n[[sections.items]]\nkind = \"target_vs_actual\"", 1))
    });
    let api = api(models.clone(), offline()).await;
    let (_, t) = api.signed_up().await;
    let (goal, _, view) = api.approved_goal(&t).await;
    assert_eq!(view["status"]["status"], "done", "{view}");
    assert_eq!(builder_calls(&models), 2, "one call and one repair");
    let (status, list) = api.call(Method::Get, &format!("/v1/goals/{goal}/plugins"), Some(&t), None).await;
    assert_eq!(status, 200, "{list}");
    assert_eq!((list[0]["version"].as_i64(), list[0]["enabled"].as_bool(), list[0]["name"].as_str()), (Some(1), Some(true), Some("running")));
    assert!(!list[0]["bundle_toml"].as_str().unwrap().contains("Slep"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn a_plugin_invalid_twice_fails_the_run_and_keeps_the_plan() {
    let models = goal_models_with(|_| builder_reply("aroh_plugin = 1\nname = \"broken\"\ntitle = \"Broken\""));
    let api = api(models.clone(), offline()).await;
    let (_, t) = api.signed_up().await;
    let (goal, _, view) = api.approved_goal(&t).await;
    assert_eq!(view["status"]["status"], "failed", "{view}");
    assert!(view["status"].to_string().contains("sections: missing"), "{view}");
    assert_eq!(builder_calls(&models), 2);
    assert!(api.count("select count(*) from public.actions where goal_id::text = $1", &goal).await > 0, "the approved plan is written");
    assert_eq!(api.count("select count(*) from public.plugins where goal_id::text = $1", &goal).await, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn plugin_endpoints_store_versions_and_switch_them() {
    let api = api(goal_models(), offline()).await;
    let (u, t) = api.signed_up().await;
    let goal = api.goal(u, Autonomy::Balanced).await;
    let path = format!("/v1/goals/{goal}/plugins");
    let fitness = aroh_plugins::template("fitness").unwrap();
    let (status, v1) = api.call(Method::Put, &path, Some(&t), Some(json!({"bundle_toml": fitness}))).await;
    assert_eq!(status, 201, "{v1}");
    assert_eq!((v1["version"].as_i64(), v1["enabled"].as_bool()), (Some(1), Some(true)));
    assert_eq!(api.count("select count(*) from public.trackers where goal_id::text = $1", &goal.to_string()).await, 4, "template trackers added");
    let (status, v2) = api.call(Method::Put, &path, Some(&t), Some(json!({"bundle_toml": fitness.replace("Next sessions", "Coming up")}))).await;
    assert_eq!((status, v2["version"].as_i64()), (201, Some(2)), "{v2}");
    let (_, draft) = api.call(Method::Put, &path, Some(&t), Some(json!({"bundle_toml": fitness, "enable": false}))).await;
    assert_eq!((draft["version"].as_i64(), draft["enabled"].as_bool()), (Some(3), Some(false)));
    assert_eq!(api.count("select count(*) from public.trackers where goal_id::text = $1", &goal.to_string()).await, 4, "trackers are matched by name");
    let (status, list) = api.call(Method::Get, &path, Some(&t), None).await;
    assert_eq!(status, 200);
    let summary: Vec<(i64, bool)> = list.as_array().unwrap().iter().map(|p| (p["version"].as_i64().unwrap(), p["enabled"].as_bool().unwrap())).collect();
    assert_eq!(summary, [(3, false), (2, true), (1, false)]);
    let v1_id = v1["id"].as_str().unwrap();
    let (status, back) = api.call(Method::Post, &format!("/v1/plugins/{v1_id}/enable"), Some(&t), None).await;
    assert_eq!((status, back["enabled"].as_bool()), (200, Some(true)), "{back}");
    let (status, one) = api.call(Method::Get, &format!("/v1/plugins/{}", v2["id"].as_str().unwrap()), Some(&t), None).await;
    assert_eq!((status, one["enabled"].as_bool()), (200, Some(false)));
    assert_eq!(api.count("select count(*) from public.plugins where goal_id::text = $1 and enabled", &goal.to_string()).await, 1);

    let (status, bad) = api.call(Method::Put, &path, Some(&t), Some(json!({"bundle_toml": fitness.replace("kind = \"countdown\"", "kind = \"gauge\"")}))).await;
    assert_eq!(status, 422, "{bad}");
    assert!(bad["error"]["message"].as_str().unwrap().contains("plugin.toml: sections[0].items[0].kind: unknown kind `gauge`"), "{bad}");
    let (status, _) = api.call(Method::Put, &format!("/v1/goals/{}/plugins", Uuid::now_v7()), Some(&t), Some(json!({"bundle_toml": fitness}))).await;
    assert_eq!(status, 404);

    let (_, other) = api.signed_up().await;
    assert_eq!(api.call(Method::Get, &path, Some(&other), None).await.0, 404);
    assert_eq!(api.call(Method::Get, &format!("/v1/plugins/{v1_id}"), Some(&other), None).await.0, 404);
    assert_eq!(api.call(Method::Post, &format!("/v1/plugins/{v1_id}/enable"), Some(&other), None).await.0, 404);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn device_routes_register_and_delete_and_push_tick_needs_the_service_role() {
    let api = api(offline(), offline()).await;
    let u = user(&api.pool).await;
    let t = user_token(u);
    let device = Uuid::now_v7();
    let token = "a1".repeat(32);
    let body = json!({"platform": "ios", "token": token, "environment": "sandbox", "zone": "Europe/Berlin", "synced_seq": 12, "scheduled_until": "2026-11-23T12:00:00Z"});
    let (status, info) = api.call(Method::Put, &format!("/v1/devices/{device}"), Some(&t), Some(body.clone())).await;
    assert_eq!(status, 200, "{info}");
    assert_eq!(info["synced_seq"], 12);
    assert!(!info.to_string().contains(&token));
    assert_eq!(api.call(Method::Put, &format!("/v1/devices/{device}"), None, Some(body.clone())).await.0, 401);
    let mut bad = body;
    bad["zone"] = json!("Mars/Olympus");
    assert_eq!(api.call(Method::Put, &format!("/v1/devices/{device}"), Some(&t), Some(bad)).await.0, 400);
    assert_eq!(api.call(Method::Post, "/v1/push/tick", Some(&t), None).await.0, 403);
    let (status, summary) = api.call(Method::Post, "/v1/push/tick", Some(&service_token()), None).await;
    assert_eq!((status, summary["enabled"].clone()), (200, json!(false)));
    assert_eq!(api.call(Method::Delete, &format!("/v1/devices/{device}"), Some(&t), None).await.0, 204);
    assert_eq!(api.call(Method::Delete, &format!("/v1/devices/{device}"), Some(&t), None).await.0, 404);
}
