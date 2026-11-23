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
use troupekit_core::clock::FixedClock;
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
    clock: Arc<FixedClock>,
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
    let verifier = Verifier::new(Some(SECRET), None, "authenticated", clock.clone());
    let state = AppState { engine: Arc::new(Engine::new(deps)), verifier: Arc::new(verifier) };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, router(state)).await.unwrap() });
    Api { base, http: ReqwestTransport::new().unwrap(), pool, hlc: ServerClock::default(), clock, seen: Mutex::new(Vec::new()) }
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
            reminders: true,
            use_calendars: true,
            run_limit_minutes: None,
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

/// The fitness intake a coach asks (Q59): runs a week first, then the daily
/// minutes cap and the body, activity, diet and injury facts the
/// `coach_questions` eval needs for a fitness goal.
fn coach_intake() -> Value {
    let q = |id: &str, text: &str| json!({"id": id, "text": text, "kind": "text", "choices": [], "unit": null, "min": null, "max": null, "lasting": null, "known": null});
    json!({"questions": [
        {"id": "weekly_runs", "text": "How many runs a week?", "kind": "number", "choices": [], "unit": "runs", "min": 0, "max": 7, "lasting": null, "known": null},
        {"id": "daily_minutes", "text": "How many minutes a day can you give this?", "kind": "number", "choices": [], "unit": "minutes per day", "min": 5, "max": 480, "lasting": null, "known": null},
        q("weight_kg", "What is your body weight now?"),
        q("height_cm", "How tall are you?"),
        q("activity", "How active are you on a normal day?"),
        q("diet", "Do you eat meat, or are you vegetarian or vegan?"),
        q("injuries", "Any injuries or pain to plan around?"),
    ]})
}

fn plan() -> Value {
    json!({
        "summary": "Three easy runs a week and new shoes.",
        "timezone": "Europe/Berlin", "daily_minutes": null, "needed_minutes": null,
        "items": [
            {"title": "Easy run", "start": "2026-11-23T07:00:00", "minutes": 30, "rrule": "FREQ=WEEKLY;BYDAY=MO,WE,FR", "notes": "Run 1 min, walk 2 min, eight rounds"},
            {"title": "Buy running shoes", "start": "2026-11-24T18:00:00", "minutes": 30, "rrule": null, "notes": "Gait check"}
        ],
        "trackers": [{"name": "Runs", "fields": [
            {"key": "distance_km", "label": "Distance", "kind": "number", "unit": "km"},
            {"key": "felt_good", "label": "Felt good", "kind": "bool", "unit": null}
        ], "cadence": null, "rules": null}]
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
            return sse_text(&coach_intake().to_string());
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
async fn locked_actions_are_fixed_for_every_proposal() {
    let ids = Arc::new(Mutex::new((String::new(), String::new())));
    let seen = ids.clone();
    let models = FnTransport::new(move |req| {
        let body = body_text(req);
        if !body.contains("You are Adaptation analyst") {
            return json_reply(400, json!({}));
        }
        let (fixed, free) = seen.lock().unwrap().clone();
        let target = if body.contains("is a fixed action") { free } else { fixed };
        let change = json!([{"op": "move", "action_id": target, "title": "Easy run", "start": "2026-11-23T18:00:00", "minutes": 30}]);
        sse_text(&json!({"summary": "Move it.", "size": "small", "changes": change}).to_string())
    });
    let api = api(models, offline()).await;
    let (u, t) = api.signed_up().await;
    let goal = api.goal(u, Autonomy::Auto).await;
    let locked = api.action(u, goal, "2026-11-23T06:00:00Z", true).await;
    let free = api.action(u, goal, "2026-11-21T07:00:00Z", false).await;
    *ids.lock().unwrap() = (locked.id.clone(), free.id.clone());
    let (_, started) = api.call(Method::Post, &format!("/v1/goals/{goal}/replan"), Some(&t), Some(json!({"reason": "Busy", "timezone": "Europe/Berlin"}))).await;
    let run = started["run_id"].as_str().unwrap();
    api.tick().await;
    let view = api.run(&t, run).await;
    assert_eq!(view["status"]["status"], "done", "{view}");
    let start_of = |id: String| {
        let pool = api.pool.clone();
        async move {
            let start: DateTime<Utc> = db::query_scalar("select start_at from public.actions where id::text = $1").bind(id).fetch_one(&pool).await.unwrap();
            start
        }
    };
    assert_eq!(start_of(locked.id.clone()).await.to_rfc3339(), "2026-11-23T06:00:00+00:00", "a fixed action is never moved");
    assert_eq!(start_of(free.id.clone()).await.to_rfc3339(), "2026-11-23T17:00:00+00:00", "the repair round moved a flexible action instead");
    let diffs: Vec<Value> = db::query_scalar("select diff from public.proposals where goal_id::text = $1").bind(goal.to_string()).fetch_all(&api.pool).await.unwrap();
    assert_eq!(diffs.len(), 1);
    assert!(!diffs[0].to_string().contains(&locked.id), "nothing on a fixed action reaches a proposal");
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
    let body = json!({"vendor": "mistral", "key": "test-MISTRAL-key-0001", "model": "mistral-large-latest", "is_default": true});
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

async fn busy_block(api: &Api, user: Uuid, title: &str, start: &str, minutes: i64) {
    let start: DateTime<Utc> = start.parse().unwrap();
    let mut b = aroh_domain::rows::BusyBlock {
        id: Uuid::now_v7().to_string(),
        user_id: user.to_string(),
        calendar_id: "work".into(),
        event_id: format!("evt-{title}"),
        title: title.into(),
        start_at: start,
        end_at: start + chrono::Duration::minutes(minutes),
        all_day: false,
        group_name: Some("Work".into()),
        day_kind: None,
        updated_at: String::new(),
        deleted: false,
    };
    put_row(&api.pool, &api.hlc, user, &mut b).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn planner_repairs_a_plan_over_busy_time() {
    let models = FnTransport::new(|req| {
        let body = body_text(req);
        if body.contains("You are Intake interviewer") {
            return sse_text(&coach_intake().to_string());
        }
        if body.contains("You are Goal researcher") {
            return sse_text(&json!({"summary": "Easy runs.", "sources": []}).to_string());
        }
        if body.contains("You are Goal planner") {
            let start = if body.contains("overlaps busy") { "2026-11-23T07:00:00" } else { "2026-11-23T09:15:00" };
            let plan = json!({"summary": "One run.", "timezone": "Europe/Berlin", "trackers": [], "daily_minutes": null, "needed_minutes": null,
                "items": [{"title": "Easy run", "start": start, "minutes": 30, "rrule": null, "notes": "Run 1 min, walk 2 min, eight rounds"}]});
            return sse_text(&plan.to_string());
        }
        json_reply(400, json!({}))
    });
    let api = api(models.clone(), offline()).await;
    let (u, t) = api.signed_up().await;
    busy_block(&api, u, "Standup", "2026-11-23T08:00:00Z", 30).await;
    busy_block(&api, u, "Team sync", "2026-11-23T08:15:00Z", 30).await;
    let (_, started) = api.call(Method::Post, "/v1/goals", Some(&t), Some(json!({"title": "Run a 5k", "timezone": "Europe/Berlin"}))).await;
    let (goal, run) = (started["goal_id"].as_str().unwrap().to_string(), started["run_id"].as_str().unwrap().to_string());
    api.tick().await;
    api.answer(&t, &run, "questions", json!([{"id": "weekly_runs", "answer": "3"}])).await;
    api.tick().await;
    let view = api.run(&t, &run).await;
    assert_eq!(view["waiting"][0]["node"], "approve", "{view}");
    assert_eq!(view["waiting"][0]["data"]["plan"]["items"][0]["start"], "2026-11-23T07:00:00");
    let planner: Vec<String> = models.sent().iter().map(body_text).filter(|b| b.contains("You are Goal planner")).collect();
    assert_eq!(planner.len(), 2, "one repair round");
    assert!(planner[0].contains("Standup") && planner[0].contains("2026-11-23T09:00:00"), "{}", planner[0]);
    assert!(planner[1].contains("overlaps busy `Standup`"), "{}", planner[1]);
    api.answer(&t, &run, "approve", json!(true)).await;
    api.tick().await;
    let start: DateTime<Utc> = db::query_scalar("select start_at from public.actions where goal_id::text = $1").bind(&goal).fetch_one(&api.pool).await.unwrap();
    assert_eq!(start.to_rfc3339(), "2026-11-23T06:00:00+00:00");
}

fn triage_event(id: &str, title: &str, start: &str, rrule: Option<&str>) -> Value {
    let start: DateTime<Utc> = start.parse().unwrap();
    json!({"event_id": id, "calendar_id": "cal-work", "calendar": "Work", "title": title,
        "start_at": start, "end_at": start + chrono::Duration::minutes(30), "rrule": rrule})
}

fn triage_models() -> Arc<FnTransport> {
    FnTransport::new(|req| {
        if body_text(req).contains("You are Calendar triage") {
            let groups = json!({"groups": [
                {"name": "Work meetings", "kind": "work", "keep": true, "reason": "Meetings.", "events": ["e1", "e3"]},
                {"name": "Noise", "kind": "noise", "keep": false, "reason": "Holidays.", "events": ["e2", "e9"]}
            ], "days": [{"event": "e5", "kind": "away"}, {"event": "e1", "kind": "busy"}]});
            return sse_text(&groups.to_string());
        }
        json_reply(400, json!({}))
    })
}

async fn rules(api: &Api, user: Uuid) -> Vec<(Option<String>, Option<String>, String, String)> {
    db::query_as("select series_key, pattern, group_name, decision from public.calendar_rules where user_id = $1 and not deleted order by group_name, series_key, pattern")
        .bind(user)
        .fetch_all(&api.pool)
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn triage_groups_decides_and_writes_rules() {
    let models = triage_models();
    let api = api(models.clone(), offline()).await;
    let (u, t) = api.signed_up().await;
    let mut trip = triage_event("trip", "Trip", "2026-11-26T23:00:00Z", Some("FREQ=YEARLY"));
    trip["end_at"] = json!("2026-11-29T23:00:00Z");
    trip["all_day"] = json!(true);
    let events = json!([
        triage_event("standup", "Standup", "2026-11-23T08:00:00Z", Some("FREQ=DAILY")),
        triage_event("holiday", "Public holiday", "2026-11-23T09:00:00Z", None),
        triage_event("review", "Design Review", "2026-11-24T13:00:00Z", None),
        triage_event("dentist", "Dentist", "2026-11-25T15:00:00Z", None),
        trip.clone()
    ]);
    let body = json!({"timezone": "Europe/Berlin", "events": events});
    let (status, started) = api.call(Method::Post, "/v1/calendar/triage", Some(&t), Some(body.clone())).await;
    assert_eq!(status, 202, "{started}");
    assert_eq!(started["goal_id"], Value::Null);
    let run = started["run_id"].as_str().unwrap().to_string();
    let (status, _) = api.call(Method::Post, "/v1/calendar/triage", Some(&t), Some(body.clone())).await;
    assert_eq!(status, 409, "one open triage at a time");
    api.tick().await;
    let view = api.run(&t, &run).await;
    assert_eq!(view["waiting"][0]["node"], "decide", "{view}");
    let groups = view["waiting"][0]["data"]["sorted"]["groups"].as_array().unwrap().clone();
    let names: Vec<&str> = groups.iter().map(|g| g["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Work meetings", "Noise", "Other"]);
    assert_eq!(groups[2]["events"], json!(["e4"]), "all-day events get a day pick, never a group");
    let data = &view["waiting"][0]["data"];
    assert_eq!(data["sorted"]["days"], json!([{"event": "e5", "kind": "away"}]), "only all-day events get a kind");
    assert_eq!((data["events"][4]["all_day"].clone(), data["events"][4]["minutes"].clone()), (json!(true), json!(1440)));
    assert_eq!(data["refs"][4]["event_id"], "trip", "the app maps the key back to its event");
    let mut chosen = aroh_domain::calendar::day_rule(&u.to_string(), "cal-work", "trip", "Trip", "All day", aroh_domain::rows::DayKind::Away);
    put_row(&api.pool, &api.hlc, u, &mut chosen).await.unwrap();
    assert_eq!(view["waiting"][0]["data"]["events"][0]["start"], "2026-11-23T09:00:00");
    let sent = body_text(&models.sent()[0]);
    assert!(sent.contains("Standup") && !sent.contains("cal-work") && !sent.contains("standup\""), "ids stay on the server");
    let decisions = json!([{"group": "Work meetings", "keep": true}, {"group": "Noise", "keep": false}, {"group": "Other", "keep": true}]);
    api.answer(&t, &run, "decide", decisions).await;
    api.tick().await;
    assert_eq!(api.run(&t, &run).await["status"]["status"], "done");
    let s = |v: &str| Some(v.to_string());
    let expected = vec![
        (s("trip"), None, "All day".to_string(), "keep".to_string()),
        (None, s("public holiday"), "Noise".to_string(), "ignore".to_string()),
        (None, s("dentist"), "Other".to_string(), "keep".to_string()),
        (s("standup"), None, "Work meetings".to_string(), "keep".to_string()),
        (None, s("design review"), "Work meetings".to_string(), "keep".to_string()),
    ];
    assert_eq!(rules(&api, u).await, expected);
    let kind: Option<String> = db::query_scalar("select day_kind from public.calendar_rules where user_id = $1 and series_key = 'trip'")
        .bind(u)
        .fetch_one(&api.pool)
        .await
        .unwrap();
    assert_eq!(kind.as_deref(), Some("away"), "a triage answer keeps the owner's day kind");

    let (_, again) = api.call(Method::Post, "/v1/calendar/triage", Some(&t), Some(json!({"timezone": "UTC", "events": [triage_event("standup", "Standup", "2026-11-30T08:00:00Z", Some("FREQ=DAILY"))]}))).await;
    let again = again["run_id"].as_str().unwrap().to_string();
    api.tick().await;
    api.answer(&t, &again, "decide", json!([{"group": "Work meetings", "keep": false}])).await;
    api.tick().await;
    let after = rules(&api, u).await;
    assert_eq!(after.len(), 5, "the same key overwrites");
    assert!(after.contains(&(s("standup"), None, "Work meetings".to_string(), "ignore".to_string())));
    let (_, other) = api.signed_up().await;
    assert_eq!(api.call(Method::Get, &format!("/v1/runs/{run}"), Some(&other), None).await.0, 404);
    let theirs: i64 = db::query_scalar("select count(*) from public.calendar_rules where user_id <> $1").bind(u).fetch_one(&api.pool).await.unwrap();
    assert_eq!(theirs, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn an_all_day_only_triage_has_no_group_and_writes_day_rules() {
    let api = api(triage_models(), offline()).await;
    let (u, t) = api.signed_up().await;
    let mut trip = triage_event("trip", "Trip", "2026-11-26T23:00:00Z", None);
    trip["end_at"] = json!("2026-11-29T23:00:00Z");
    trip["all_day"] = json!(true);
    let (status, started) = api.call(Method::Post, "/v1/calendar/triage", Some(&t), Some(json!({"timezone": "UTC", "events": [trip]}))).await;
    assert_eq!(status, 202, "{started}");
    let run = started["run_id"].as_str().unwrap().to_string();
    api.tick().await;
    let view = api.run(&t, &run).await;
    assert_eq!(view["waiting"][0]["data"]["sorted"]["groups"], json!([]), "{view}");
    api.answer(&t, &run, "decide", json!([])).await;
    api.tick().await;
    assert_eq!(api.run(&t, &run).await["status"]["status"], "done");
    let s = |v: &str| Some(v.to_string());
    assert_eq!(rules(&api, u).await, vec![(s("trip"), None, "All day".to_string(), "keep".to_string())]);
    let kind: Option<String> = db::query_scalar("select day_kind from public.calendar_rules where user_id = $1").bind(u).fetch_one(&api.pool).await.unwrap();
    assert_eq!(kind, None, "the kind waits for the owner's tap");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn triage_rejects_bad_requests() {
    let api = api(triage_models(), offline()).await;
    let (_, t) = api.signed_up().await;
    let one = triage_event("a", "A", "2026-11-23T08:00:00Z", None);
    let mut backwards = one.clone();
    backwards["end_at"] = json!("2026-11-23T07:00:00Z");
    let many: Vec<Value> = (0..301).map(|i| triage_event(&format!("e{i}"), "x", "2026-11-23T08:00:00Z", None)).collect();
    for body in [
        json!({"timezone": "UTC", "events": []}),
        json!({"timezone": "UTC", "events": many}),
        json!({"timezone": "UTC", "events": [backwards]}),
        json!({"timezone": "Mars/Base", "events": [one.clone()]}),
        json!({"timezone": "UTC", "events": [one], "extra": 1}),
    ] {
        let (status, err) = api.call(Method::Post, "/v1/calendar/triage", Some(&t), Some(body)).await;
        assert_eq!(status, 400, "{err}");
    }
    assert_eq!(api.call(Method::Post, "/v1/calendar/triage", None, Some(json!({}))).await.0, 401);
}

fn music_plan() -> Value {
    json!({
        "summary": "Learn Clair de Lune section by section, then raise the tempo.",
        "timezone": "Europe/Berlin", "daily_minutes": null, "needed_minutes": null,
        "items": [
            {"title": "Piano practice: bars 1 to 14", "start": "2026-11-23T19:00:00", "minutes": 30, "rrule": "FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR,SA", "notes": "Hands separately, slow"},
            {"title": "Record a run-through", "start": "2026-11-29T11:00:00", "minutes": 20, "rrule": null, "notes": "Play it through twice, slow"}
        ],
        "trackers": [
            {"name": "Practice", "fields": [{"key": "minutes", "label": "Practice", "kind": "number", "unit": "min"}], "cadence": {"freq": "daily", "weekdays": [], "at": "20:00"}, "rules": null},
            {"name": "Tempo", "fields": [{"key": "bpm", "label": "Tempo", "kind": "number", "unit": "bpm"}], "cadence": null, "rules": null},
            {"name": "Pieces", "fields": [{"key": "piece", "label": "Piece or passage", "kind": "text", "unit": null}, {"key": "progress", "label": "Progress", "kind": "number", "unit": "%"}], "cadence": null, "rules": null}
        ]
    })
}

fn music_models() -> Arc<FnTransport> {
    FnTransport::new(move |req| {
        let body = body_text(req);
        if !body.contains("\"stream\":true") {
            return json_reply(200, json!({"id": "msg_k", "type": "message", "role": "assistant", "content": [{"type": "text", "text": "OK"}],
                "stop_reason": "max_tokens", "usage": {"input_tokens": 5, "output_tokens": 1}}));
        }
        if body.contains("You are Plugin builder") && body.contains("Music practice") {
            return builder_reply(aroh_plugins::template("music").unwrap());
        }
        if body.contains("You are Intake interviewer") {
            return sse_text(&json!({"questions": [{"id": "level", "text": "How long have you played piano?", "kind": "text", "choices": [], "unit": null, "min": null, "max": null, "lasting": null, "known": null}, {"id": "daily_minutes", "text": "Minutes a day to practise?", "kind": "number", "choices": [], "unit": "minutes per day", "min": 5, "max": 480, "lasting": null, "known": null}]}).to_string());
        }
        if body.contains("You are Instrument teacher") {
            return sse_text(&json!({"summary": "Learn it in four sections, slow practice first.", "sources": [{"title": "Clair de Lune guide", "url": "https://example.org/clair"}]}).to_string());
        }
        if body.contains("You are Practice coach") {
            return sse_text(&music_plan().to_string());
        }
        Reply { status: 400, content_type: "application/json", body: json!({"error": "unexpected request"}).to_string() }
    })
}

impl Api {
    async fn run_personas(&self, run: &str) -> Vec<String> {
        let map: Value = db::query_scalar("select personas from public.runs where id::text = $1").bind(run).fetch_one(&self.pool).await.unwrap();
        map.as_object().unwrap().keys().cloned().collect()
    }

    async fn goal_category(&self, goal: &str) -> Option<String> {
        db::query_scalar("select category_id::text from public.goals where id::text = $1").bind(goal).fetch_one(&self.pool).await.unwrap()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn a_music_goal_runs_with_the_music_team_category_and_template() {
    let models = music_models();
    let api = api(models.clone(), offline()).await;
    let (u, t) = api.signed_up().await;
    let body = json!({"title": "Learn Clair de Lune on piano by March", "deadline": "2027-03-31", "timezone": "Europe/Berlin"});
    let (status, started) = api.call(Method::Post, "/v1/goals", Some(&t), Some(body)).await;
    assert_eq!(status, 202, "{started}");
    let (goal, run) = (started["goal_id"].as_str().unwrap().to_string(), started["run_id"].as_str().unwrap().to_string());
    assert_eq!(api.run_personas(&run).await, ["instrument-teacher", "intake-interviewer", "plugin-builder", "practice-coach"]);
    let (_, runs) = api.call(Method::Get, &format!("/v1/runs?goal_id={goal}"), Some(&t), None).await;
    assert_eq!((runs[0]["run_id"].as_str(), &runs[0]["personas"]), (Some(run.as_str()), &json!(["instrument-teacher", "intake-interviewer", "plugin-builder", "practice-coach"])), "{runs}");
    assert_eq!(api.goal_category(&goal).await, aroh_server::teams::seeded_id(u, "hobby-skill"));

    api.tick().await;
    api.answer(&t, &run, "questions", json!([{"id": "level", "answer": "Two years"}, {"id": "daily_minutes", "answer": "30"}])).await;
    api.tick().await;
    let view = api.run(&t, &run).await;
    assert_eq!(view["waiting"][0]["node"], "approve", "{view}");
    assert_eq!(view["waiting"][0]["data"]["plan"]["trackers"][1]["name"], "Tempo");
    api.answer(&t, &run, "approve", json!(true)).await;
    api.tick().await;
    let view = api.run(&t, &run).await;
    assert_eq!(view["status"]["status"], "done", "{view}");
    assert_eq!(api.count("select count(*) from public.plugins where goal_id::text = $1 and enabled and version = 1 and name = 'music'", &goal).await, 1);
    assert_eq!(api.count("select count(*) from public.trackers where goal_id::text = $1", &goal).await, 3);
    let calls: Vec<String> = models.sent().iter().map(body_text).filter(|b| b.contains("\"stream\":true")).collect();
    assert_eq!(calls.len(), 4, "intake, research, plan, plugin");
    assert!(calls.iter().all(|b| !b.contains("You are Goal planner") && !b.contains("You are Goal researcher")));

    let learning = aroh_server::teams::seeded_id(u, "learning").unwrap();
    let (status, started) = api.call(Method::Post, "/v1/goals", Some(&t), Some(json!({"title": "Run a 5k", "category_id": learning, "timezone": "UTC"}))).await;
    assert_eq!(status, 202, "{started}");
    assert_eq!(api.run_personas(started["run_id"].as_str().unwrap()).await, ["intake-interviewer", "plugin-builder", "practice-planner", "tutor"]);
    let (_, started) = api.call(Method::Post, "/v1/goals", Some(&t), Some(json!({"title": "Ship my side project", "timezone": "UTC"}))).await;
    assert_eq!(api.goal_category(started["goal_id"].as_str().unwrap()).await, None, "the general team leaves the category open");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn persona_library_lists_copies_and_edits_and_runs_use_the_copy() {
    let api = api(music_models(), offline()).await;
    let (_u, t) = api.signed_up().await;
    let (status, list) = api.call(Method::Get, "/v1/personas", Some(&t), None).await;
    assert_eq!(status, 200, "{list}");
    let entries = list.as_array().unwrap();
    assert_eq!(entries.len(), 15);
    assert!(entries.iter().all(|p| p["source"] == "builtin" && p["builtin"] == true));
    let coach = entries.iter().find(|p| p["name"] == "practice-coach").unwrap();
    assert_eq!((coach["display_name"].as_str(), coach["teams"].clone()), (Some("Practice coach"), json!(["music"])));
    let shipped = coach["toml"].as_str().unwrap().to_string();

    let copy = |name: &str| format!("/v1/personas/{name}/copy");
    let (status, copied) = api.call(Method::Post, &copy("practice-coach"), Some(&t), None).await;
    assert_eq!((status, copied["source"].as_str(), copied["toml"].as_str()), (201, Some("library"), Some(shipped.as_str())), "{copied}");
    assert_eq!(api.call(Method::Post, &copy("practice-coach"), Some(&t), None).await.0, 409);
    assert_eq!(api.call(Method::Post, &copy("nobody"), Some(&t), None).await.0, 404);
    let edit = |name: &str, toml: &str| (format!("/v1/personas/{name}"), json!({"toml": toml}));
    let (path, body) = edit("budget-coach", aroh_server::workflows::shipped("budget-coach").unwrap());
    assert_eq!(api.call(Method::Put, &path, Some(&t), Some(body)).await.0, 404, "copy first");

    let edited = shipped.replace("15 to 45 minutes", "20 to 40 minutes");
    assert_ne!(edited, shipped);
    for (bad, why) in [
        (edited.replace("name = \"practice-coach\"", "name = \"other-coach\""), "name"),
        (edited.replace("model = \"default\"", "model = \"default\"\ntools = [\"send_mail\"]"), "send_mail"),
        (edited.replace("evals = [", "evals = [\"vibes(items)\", "), "vibes"),
        (edited.replace("summary = \"string\"", "summary = \"int\""), "output"),
        ("name = 3".to_string(), "practice-coach.toml"),
    ] {
        let (path, body) = edit("practice-coach", &bad);
        let (status, err) = api.call(Method::Put, &path, Some(&t), Some(body)).await;
        assert_eq!(status, 422, "{why}: {err}");
        assert!(err["error"]["message"].as_str().unwrap().contains(why), "{why}: {err}");
    }
    let (path, body) = edit("practice-coach", &edited);
    let (status, saved) = api.call(Method::Put, &path, Some(&t), Some(body)).await;
    assert_eq!((status, saved["toml"].as_str(), saved["source"].as_str()), (200, Some(edited.as_str()), Some("library")), "{saved}");

    assert_eq!(api.call(Method::Post, &copy("budget-coach"), Some(&t), None).await.0, 201);
    let unsafe_edit: String = aroh_server::workflows::shipped("budget-coach").unwrap().lines().filter(|l| !l.contains("Safety:")).collect::<Vec<_>>().join("\n");
    let (path, body) = edit("budget-coach", &unsafe_edit);
    let (status, err) = api.call(Method::Put, &path, Some(&t), Some(body)).await;
    assert_eq!(status, 422, "{err}");
    assert!(err["error"]["message"].as_str().unwrap().contains("Safety:"), "{err}");

    let (_, list) = api.call(Method::Get, "/v1/personas", Some(&t), None).await;
    let sources: Vec<(String, String)> = list.as_array().unwrap().iter().filter(|p| p["source"] == "library").map(|p| (p["name"].as_str().unwrap().into(), p["toml"].as_str().unwrap().into())).collect();
    assert_eq!(sources.len(), 2);
    assert!(sources.contains(&("practice-coach".into(), edited.clone())));

    let (_v, tv) = api.signed_up().await;
    let (_, other) = api.call(Method::Get, "/v1/personas", Some(&tv), None).await;
    assert!(other.as_array().unwrap().iter().all(|p| p["source"] == "builtin"));
    let (path, body) = edit("practice-coach", &edited);
    assert_eq!(api.call(Method::Put, &path, Some(&tv), Some(body)).await.0, 404);

    let (status, started) = api.call(Method::Post, "/v1/goals", Some(&t), Some(json!({"title": "Learn Clair de Lune on piano by March", "timezone": "UTC"}))).await;
    assert_eq!(status, 202, "{started}");
    let run = started["run_id"].as_str().unwrap();
    let text: String = db::query_scalar("select personas->>'practice-coach' from public.runs where id::text = $1").bind(run).fetch_one(&api.pool).await.unwrap();
    assert_eq!(text, edited, "the run snapshots the owner's library copy");
    assert_eq!(api.call(Method::Get, "/v1/personas", None, None).await.0, 401);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn persona_fields_edit_the_copy_and_keep_safety_rules() {
    let api = api(music_models(), offline()).await;
    let (_u, t) = api.signed_up().await;
    let (_, list) = api.call(Method::Get, "/v1/personas", Some(&t), None).await;
    let budget = list.as_array().unwrap().iter().find(|p| p["name"] == "budget-coach").unwrap().clone();
    let safety = "Safety: no individual investment, tax or legal advice; refer to a licensed financial professional for those.";
    assert_eq!(budget["locked"], json!([safety]));
    assert_eq!(budget["guardrails"].as_array().unwrap().len(), 4);
    assert!(budget["instructions"].as_str().unwrap().starts_with("1. Search the web"));
    let tutor = list.as_array().unwrap().iter().find(|p| p["name"] == "tutor").unwrap();
    assert_eq!(tutor["locked"], json!([]));

    let path = "/v1/personas/budget-coach";
    assert_eq!(api.call(Method::Put, path, Some(&t), Some(json!({"display_name": "Money mentor"}))).await.0, 404, "copy first");
    assert_eq!(api.call(Method::Post, "/v1/personas/budget-coach/copy", Some(&t), None).await.0, 201);
    let instructions = "Say \"hi\" with a \\ backslash\nthen a second line.\n";
    let rules = json!([safety, "Keep it short."]);
    let edit = json!({"display_name": "Money mentor", "role": "A warm, plain-spoken coach.", "instructions": instructions, "guardrails": rules});
    let (status, saved) = api.call(Method::Put, path, Some(&t), Some(edit)).await;
    assert_eq!(status, 200, "{saved}");
    assert_eq!((saved["display_name"].as_str(), saved["source"].as_str()), (Some("Money mentor"), Some("library")));
    let (_, list) = api.call(Method::Get, "/v1/personas", Some(&t), None).await;
    let listed = list.as_array().unwrap().iter().find(|p| p["name"] == "budget-coach").unwrap();
    assert_eq!((listed["display_name"].as_str(), listed["role"].as_str()), (Some("Money mentor"), Some("A warm, plain-spoken coach.")));
    assert_eq!((listed["instructions"].as_str(), &listed["guardrails"], &listed["locked"]), (Some(instructions), &rules, &json!([safety])));

    let (status, err) = api.call(Method::Put, path, Some(&t), Some(json!({"guardrails": ["Keep it short."]}))).await;
    assert_eq!(status, 422, "{err}");
    assert_eq!(err["error"]["code"], "setup");
    assert_eq!(err["error"]["message"].as_str().unwrap(), format!("setup: invalid_persona: budget-coach.toml: guardrails: keep `{safety}`"));
    for bad in [json!({}), json!({"toml": "name = 3", "role": "x"}), json!({"style": "calm"})] {
        assert_eq!(api.call(Method::Put, path, Some(&t), Some(bad.clone())).await.0, 400, "{bad}");
    }
    let whole = aroh_server::workflows::shipped("budget-coach").unwrap();
    let (status, saved) = api.call(Method::Put, path, Some(&t), Some(json!({"toml": whole}))).await;
    assert_eq!((status, saved["toml"].as_str(), saved["display_name"].as_str()), (200, Some(whole), Some("Budget coach")), "{saved}");
}

/// The six intake questions of the owner's stuck build 1 run, with the unit and slider bounds new runs carry.
fn reading_intake() -> Value {
    let q = |id: &str, kind: &str, text: &str, choices: Value, unit: Value, min: Value, max: Value| {
        json!({"id": id, "kind": kind, "text": text, "choices": choices, "unit": unit, "min": min, "max": max, "lasting": null, "known": null})
    };
    json!({"questions": [
        q("chapters_done", "number", "How many chapters of The Joy of X have you finished?", json!([]), json!("chapters"), json!(0), json!(30)),
        q("daily_minutes", "number", "How many minutes a day can you read?", json!([]), json!("minutes per day"), json!(0), json!(180)),
        q("preferred_reading_time", "choice", "When do you prefer to read?", json!(["Early morning", "Midday", "Evening", "Late night", "It varies"]), Value::Null, Value::Null, Value::Null),
        q("reading_days", "number", "How many days this week can you read?", json!([]), json!("days"), json!(1), json!(7)),
        q("book_format", "choice", "What format are you reading the book in?", json!(["Print book", "E-reader", "Phone or tablet", "Audiobook"]), Value::Null, Value::Null, Value::Null),
        q("busy_days_limits", "text", "Any days you cannot read this week?", json!([]), Value::Null, Value::Null, Value::Null),
    ]})
}

fn reading_models() -> Arc<FnTransport> {
    FnTransport::new(reading_reply)
}

/// The reading flow's model replies: intake questions, the tutor's
/// research and the planner's plan; 400 for anything else.
fn reading_reply(req: &HttpRequest) -> Reply {
    {
        let body = body_text(req);
        if body.contains("You are Intake interviewer") {
            return sse_text(&reading_intake().to_string());
        }
        if body.contains("You are Tutor") {
            return sse_text(&json!({"summary": "The Joy of X has 30 chapters and 336 pages.", "sources": [{"title": "Publisher", "url": "https://example.org/joy-of-x"}]}).to_string());
        }
        if body.contains("You are Practice planner") {
            return sse_text(&plan().to_string());
        }
        Reply { status: 400, content_type: "application/json", body: json!({"error": "unexpected request"}).to_string() }
    }
}

impl Api {
    async fn reading_goal(&self, t: &str) -> String {
        let (status, started) = self.call(Method::Post, "/v1/goals", Some(t), Some(json!({"title": "Finish reading The Joy of X by next Sunday", "timezone": "UTC"}))).await;
        assert_eq!(status, 202, "{started}");
        let run = started["run_id"].as_str().unwrap().to_string();
        self.tick().await;
        let view = self.run(t, &run).await;
        assert_eq!(view["waiting"][0]["node"], "questions", "{view}");
        run
    }

    async fn post_answers(&self, t: &str, run: &str, answer: Value) -> u16 {
        self.call(Method::Post, &format!("/v1/runs/{run}/answers"), Some(t), Some(json!({"node": "questions", "answer": answer}))).await.0
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn questions_take_string_answers_and_skips_and_old_runs_still_submit() {
    let models = reading_models();
    let api = api(models.clone(), offline()).await;
    let (_u, t) = api.signed_up().await;

    let run = api.reading_goal(&t).await;
    let view = api.run(&t, &run).await;
    let asked = &view["waiting"][0]["data"]["intake"]["questions"];
    assert_eq!((asked[0]["unit"].as_str(), asked[0]["min"].as_f64(), asked[0]["max"].as_f64()), (Some("chapters"), Some(0.0), Some(30.0)));
    assert_eq!(api.post_answers(&t, &run, json!([{"id": "chapters_done", "answer": 4}])).await, 409, "numbers are posted as strings");
    let answers = json!([
        {"id": "chapters_done", "answer": "4"},
        {"id": "daily_minutes", "answer": "30"},
        {"id": "preferred_reading_time", "answer": null},
        {"id": "reading_days", "answer": "5"},
        {"id": "book_format", "answer": null},
        {"id": "busy_days_limits", "answer": null},
    ]);
    assert_eq!(api.post_answers(&t, &run, answers).await, 202);
    api.tick().await;
    let view = api.run(&t, &run).await;
    assert_eq!(view["waiting"][0]["node"], "approve", "a skipped question is never asked again: {view}");
    let planner = models.sent().iter().map(body_text).find(|b| b.contains("You are Practice planner")).expect("the planner ran");
    assert!(planner.contains("What format are you reading the book in?"), "the planner reads the question texts");
    assert!(planner.contains("sensible default"), "the planner is told what to do with skipped questions");

    let all_skipped = api.reading_goal(&t).await;
    let skips: Vec<Value> = ["chapters_done", "daily_minutes", "preferred_reading_time", "reading_days", "book_format", "busy_days_limits"]
        .iter()
        .map(|id| json!({"id": id, "answer": null}))
        .collect();
    assert_eq!(api.post_answers(&t, &all_skipped, json!(skips)).await, 202, "skipping every question is allowed");
    api.tick().await;
    assert_eq!(api.run(&t, &all_skipped).await["waiting"][0]["node"], "approve");

    let old = api.reading_goal(&t).await;
    db::query("update public.run_checkpoints set state = jsonb_set(state, '{workflow,state,answers,items,properties,answer,type}', '\"string\"') where run_id::text = $1")
        .bind(&old)
        .execute(&api.pool)
        .await
        .unwrap();
    let mixed = json!([
        {"id": "chapters_done", "answer": "4"},
        {"id": "daily_minutes", "answer": null},
        {"id": "preferred_reading_time", "answer": "Evening"},
        {"id": "reading_days", "answer": "5"},
        {"id": "book_format", "answer": "Print book"},
        {"id": "busy_days_limits", "answer": "none"},
    ]);
    assert_eq!(api.post_answers(&t, &old, mixed).await, 202, "a run snapshotted before skips existed still takes a skip");
    let state: Value = db::query_scalar("select state from public.run_checkpoints where run_id::text = $1 order by step desc limit 1")
        .bind(&old)
        .fetch_one(&api.pool)
        .await
        .unwrap();
    assert_eq!(state["state"]["answers"][1], json!({"id": "daily_minutes", "answer": "skipped"}));
}

/// Reading models where each planner call takes `ms` of active work on the
/// clock in `clock`, once the test has set it, so a run reaches its limit.
fn slow_reading_models(clock: Arc<std::sync::OnceLock<Arc<FixedClock>>>, ms: i64) -> Arc<FnTransport> {
    FnTransport::new(move |req| {
        if let Some(c) = clock.get().filter(|_| body_text(req).contains("You are Practice planner")) {
            c.advance(ms);
        }
        reading_reply(req)
    })
}

/// How many model calls `models` got whose prompt contains `marker`.
fn calls_to(models: &FnTransport, marker: &str) -> usize {
    models.sent().iter().filter(|r| body_text(r).contains(marker)).count()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn a_run_stops_at_its_active_time_limit_and_the_owner_can_stop_and_retry_it() {
    let slot = Arc::new(std::sync::OnceLock::new());
    let models = slow_reading_models(slot.clone(), 70_000);
    let api = api(models.clone(), offline()).await;
    let (_u, t) = api.signed_up().await;
    assert_eq!(api.call(Method::Get, "/v1/settings/run-limit", Some(&t), None).await, (200, json!({"minutes": 5})), "five minutes by default");
    for bad in [json!(0), json!(121), json!(2.5)] {
        assert_eq!(api.call(Method::Put, "/v1/settings/run-limit", Some(&t), Some(json!({"minutes": bad}))).await.0, 400, "{bad}");
    }
    assert_eq!(api.call(Method::Put, "/v1/settings/run-limit", Some(&t), Some(json!({"minutes": 1}))).await, (200, json!({"minutes": 1})));
    assert_eq!(api.call(Method::Get, "/v1/settings/run-limit", Some(&t), None).await.1, json!({"minutes": 1}));
    slot.set(api.clock.clone()).unwrap();

    let run = api.reading_goal(&t).await;
    api.clock.advance(3_600_000);
    let skips: Vec<Value> = ["chapters_done", "daily_minutes", "preferred_reading_time", "reading_days", "book_format", "busy_days_limits"]
        .iter()
        .map(|id| json!({"id": id, "answer": null}))
        .collect();
    assert_eq!(api.post_answers(&t, &run, json!(skips)).await, 202);
    api.tick().await;
    let view = api.run(&t, &run).await;
    assert_eq!(view["status"]["status"], "failed", "an hour of waiting never counts, a 70 s model call does: {view}");
    assert_eq!(view["status"]["reason"], json!({"kind": "time_limit", "limit_ms": 60_000}));
    assert!(view["waiting"].as_array().unwrap().is_empty(), "{view}");
    let calls = models.sent().len();
    api.tick().await;
    assert_eq!(models.sent().len(), calls, "a stopped run calls no more models");

    let other = user_token(user(&api.pool).await);
    assert_eq!(api.call(Method::Post, &format!("/v1/runs/{run}/retry"), Some(&other), None).await.0, 404, "only the owner retries");
    assert_eq!(api.call(Method::Put, "/v1/settings/run-limit", Some(&t), Some(json!({"minutes": 10}))).await.0, 200);
    assert_eq!(api.call(Method::Post, &format!("/v1/runs/{run}/retry"), Some(&t), None).await.0, 202);
    api.tick().await;
    let view = api.run(&t, &run).await;
    assert_eq!(view["waiting"][0]["node"], "approve", "a retried run continues where it stopped: {view}");
    assert_eq!(calls_to(&models, "You are Practice planner"), 1, "without asking the planner again");
    assert_eq!(api.call(Method::Post, &format!("/v1/runs/{run}/retry"), Some(&t), None).await.0, 409, "only a stopped run retries");

    assert_eq!(api.call(Method::Post, &format!("/v1/runs/{run}/cancel"), Some(&other), None).await.0, 404, "only the owner stops a run");
    assert_eq!(api.call(Method::Post, &format!("/v1/runs/{run}/cancel"), Some(&t), None).await.0, 202);
    assert_eq!(api.call(Method::Post, &format!("/v1/runs/{run}/cancel"), Some(&t), None).await.0, 202, "stopping twice is fine");
    let view = api.run(&t, &run).await;
    assert_eq!((&view["status"]["status"], &view["status"]["reason"]), (&json!("failed"), &json!({"kind": "cancelled"})), "{view}");
    let (status, _) = api.call(Method::Post, &format!("/v1/runs/{run}/answers"), Some(&t), Some(json!({"node": "approve", "answer": true}))).await;
    assert_eq!(status, 409, "a stopped run takes no answer");
    assert_eq!(api.call(Method::Post, &format!("/v1/runs/{run}/retry"), Some(&t), None).await.0, 202);
    api.tick().await;
    assert_eq!(api.run(&t, &run).await["waiting"][0]["node"], "approve", "a cancelled run retried waits where it was");
}


/// Facts of `user` as `(topic, fact, goal_id, blocked)`, live rows only, by topic.
async fn facts(api: &Api, user: Uuid) -> Vec<(Option<String>, String, Option<String>, bool)> {
    db::query_as("select topic, fact, goal_id::text, blocked from public.preferences where user_id = $1 and not deleted and confirmed order by topic")
        .bind(user)
        .fetch_all(&api.pool)
        .await
        .unwrap()
}

/// Models for the profile flow: the intake marks lasting questions, and once the profile
/// holds the minutes fact it asks that one as a known question under a made-up id, which
/// the server resolves by topic.
fn profile_models() -> Arc<FnTransport> {
    FnTransport::new(move |req| {
        let body = body_text(req);
        if body.contains("You are Intake interviewer") {
            let minutes = if body.contains("Free minutes a day: 30 min") {
                json!({"id": "minutes", "text": "Minutes a day?", "kind": "text", "choices": [], "unit": null, "min": null, "max": null, "known": {"fact_id": "a-made-up-id", "value": "30 min", "source": "your answer on Run a 5k"}, "lasting": {"topic": "daily_minutes", "label": "Free minutes a day"}})
            } else {
                json!({"id": "minutes", "text": "Minutes a day?", "kind": "text", "choices": [], "unit": null, "min": null, "max": null, "lasting": {"topic": "daily_minutes", "label": "Free minutes a day"}, "known": null})
            };
            let questions = json!([
                minutes,
                {"id": "runs_done", "text": "Runs done so far?", "kind": "text", "choices": [], "unit": null, "min": null, "max": null, "lasting": null, "known": null},
                {"id": "when", "text": "When do you run?", "kind": "text", "choices": [], "unit": null, "min": null, "max": null, "lasting": {"topic": "time_of_day", "label": "Running time of day"}, "known": null},
                {"id": "weight", "text": "Your weight?", "kind": "text", "choices": [], "unit": null, "min": null, "max": null, "lasting": {"topic": "weight", "label": "Body weight"}, "known": null},
                {"id": "body", "text": "How tall are you, how active are you, what do you eat, and any injuries?", "kind": "text", "choices": [], "unit": null, "min": null, "max": null, "lasting": null, "known": null},
            ]);
            return sse_text(&json!({"questions": questions}).to_string());
        }
        if body.contains("You are Goal researcher") {
            return sse_text(&json!({"summary": "Easy runs.", "sources": []}).to_string());
        }
        if body.contains("You are Goal planner") {
            let plan = json!({"summary": "One run.", "timezone": "UTC", "trackers": [], "daily_minutes": null, "needed_minutes": null,
                "items": [{"title": "Easy run", "start": "2026-11-23T09:15:00", "minutes": 30, "rrule": null, "notes": "Run 1 min, walk 2 min, eight rounds"}]});
            return sse_text(&plan.to_string());
        }
        json_reply(400, json!({}))
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn lasting_answers_become_facts_and_known_questions_update_them() {
    let models = profile_models();
    let api = api(models.clone(), offline()).await;
    let (u, t) = api.signed_up().await;
    let mut blocked = aroh_domain::rows::Preference {
        id: Uuid::now_v7().to_string(),
        user_id: u.to_string(),
        fact: "Body weight".into(),
        source: "answer".into(),
        noted_at: utc(T0),
        confirmed: true,
        topic: Some("weight".into()),
        goal_id: None,
        origin: aroh_domain::rows::PreferenceOrigin::Answer,
        blocked: true,
        updated_at: String::new(),
        deleted: false,
    };
    put_row(&api.pool, &api.hlc, u, &mut blocked).await.unwrap();

    let (_, started) = api.call(Method::Post, "/v1/goals", Some(&t), Some(json!({"title": "Run a 5k", "timezone": "UTC"}))).await;
    let (goal, run) = (started["goal_id"].as_str().unwrap().to_string(), started["run_id"].as_str().unwrap().to_string());
    api.tick().await;
    let answers = json!([{"id": "minutes", "answer": "30 min"}, {"id": "runs_done", "answer": "2"}, {"id": "when", "answer": null}, {"id": "weight", "answer": "70 kg"}, {"id": "body", "answer": null}]);
    api.answer(&t, &run, "questions", answers).await;
    api.tick().await;
    assert_eq!(api.run(&t, &run).await["waiting"][0]["node"], "approve");
    assert_eq!(
        facts(&api, u).await,
        vec![
            (Some("daily_minutes".into()), "Free minutes a day: 30 min".into(), Some(goal.clone()), false),
            (Some("weight".into()), "Body weight".into(), None, true),
        ],
        "a lasting answer is kept; one-off, skipped and blocked ones are not"
    );
    let intake = models.sent().iter().map(body_text).find(|b| b.contains("You are Intake interviewer")).expect("the intake ran");
    assert!(intake.contains("Body weight"), "the intake is told the blocked topics");

    let (_, started) = api.call(Method::Post, "/v1/goals", Some(&t), Some(json!({"title": "Run a 10k", "timezone": "UTC"}))).await;
    let run = started["run_id"].as_str().unwrap().to_string();
    api.tick().await;
    let view = api.run(&t, &run).await;
    assert_eq!(view["waiting"][0]["data"]["intake"]["questions"][0]["known"]["value"], "30 min", "the intake read the profile: {view}");
    api.clock.advance(86_400_000);
    api.answer(&t, &run, "questions", json!([{"id": "minutes", "answer": "45 min"}, {"id": "runs_done", "answer": null}, {"id": "when", "answer": null}, {"id": "weight", "answer": null}, {"id": "body", "answer": null}])).await;
    api.tick().await;
    assert_eq!(api.run(&t, &run).await["waiting"][0]["node"], "approve");
    let kept = facts(&api, u).await;
    assert_eq!(kept.len(), 2, "an edited known answer updates its fact, never adds one: {kept:?}");
    assert_eq!(kept[0].1, "Free minutes a day: 45 min");
    let noted: DateTime<Utc> = db::query_scalar("select noted_at from public.preferences where topic = 'daily_minutes'").fetch_one(&api.pool).await.unwrap();
    assert!(noted >= utc(T0 + 86_400_000), "the fact is dated at the new answer: {noted}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn a_run_waiting_before_the_profile_node_resumes_on_its_own_graph() {
    let models = reading_models();
    let api = api(models.clone(), offline()).await;
    let (u, t) = api.signed_up().await;
    let run = api.reading_goal(&t).await;
    let (step, mut state): (i64, Value) = db::query_as("select step, state from public.run_checkpoints where run_id::text = $1 order by step desc limit 1")
        .bind(&run)
        .fetch_one(&api.pool)
        .await
        .unwrap();
    let wf = &mut state["workflow"];
    wf["nodes"].as_array_mut().unwrap().retain(|n| n["id"] != "save_facts");
    wf["edges"].as_array_mut().unwrap().retain(|e| e["from"] != "save_facts");
    for e in wf["edges"].as_array_mut().unwrap().iter_mut().filter(|e| e["to"] == "save_facts") {
        e["to"] = json!("plan");
    }
    wf["state"].as_object_mut().unwrap().remove("facts_saved");
    wf["state"]["profile"] = json!({"type": "array", "items": {"type": "string"}});
    state["state"]["profile"] = json!(["Prefers mornings"]);
    db::query("update public.run_checkpoints set state = $3 where run_id::text = $1 and step = $2").bind(&run).bind(step).bind(&state).execute(&api.pool).await.unwrap();

    let answers = json!([
        {"id": "chapters_done", "answer": "4"},
        {"id": "daily_minutes", "answer": "30"},
        {"id": "preferred_reading_time", "answer": "Evening"},
        {"id": "reading_days", "answer": "5"},
        {"id": "book_format", "answer": null},
        {"id": "busy_days_limits", "answer": null},
    ]);
    assert_eq!(api.post_answers(&t, &run, answers).await, 202);
    api.tick().await;
    let view = api.run(&t, &run).await;
    assert_eq!(view["waiting"][0]["node"], "approve", "a run deployed onto the new server keeps its graph: {view}");
    let planner = models.sent().iter().map(body_text).find(|b| b.contains("You are Practice planner")).expect("the planner ran");
    assert!(planner.contains("Prefers mornings"), "the old string profile still reaches the planner");
    assert!(facts(&api, u).await.is_empty(), "the old graph keeps no facts");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn a_goal_run_limit_overrides_the_settings_limit() {
    let slot = Arc::new(std::sync::OnceLock::new());
    let models = slow_reading_models(slot.clone(), 70_000);
    let api = api(models.clone(), offline()).await;
    let (_u, t) = api.signed_up().await;
    assert_eq!(api.call(Method::Put, "/v1/settings/run-limit", Some(&t), Some(json!({"minutes": 1}))).await.0, 200);
    slot.set(api.clock.clone()).unwrap();
    let body = json!({"title": "Finish reading The Joy of X by next Sunday", "timezone": "UTC", "run_limit_minutes": 10, "reminders": false});
    let (status, started) = api.call(Method::Post, "/v1/goals", Some(&t), Some(body)).await;
    assert_eq!(status, 202, "{started}");
    let goal = started["goal_id"].as_str().unwrap().to_string();
    let run = started["run_id"].as_str().unwrap().to_string();
    let stored: (Option<i32>, bool, bool) = db::query_as("select run_limit_minutes, reminders, use_calendars from public.goals where id::text = $1")
        .bind(&goal)
        .fetch_one(&api.pool)
        .await
        .unwrap();
    assert_eq!(stored, (Some(10), false, true));
    api.tick().await;
    let skips: Vec<Value> = ["chapters_done", "daily_minutes", "preferred_reading_time", "reading_days", "book_format", "busy_days_limits"]
        .iter()
        .map(|id| json!({"id": id, "answer": null}))
        .collect();
    assert_eq!(api.post_answers(&t, &run, json!(skips)).await, 202);
    api.tick().await;
    let view = api.run(&t, &run).await;
    assert_eq!(view["waiting"][0]["node"], "approve", "a 70 s call fits the goal's 10 min, not the 1 min default: {view}");
    assert!(view["research"]["summary"].is_string(), "the card shows what research found: {view}");
    let answers = view["answers"].as_array().unwrap();
    assert_eq!(answers.len(), 6, "{view}");
    assert!(answers.iter().all(|a| a["answer"].is_null() && a["text"].is_string()), "{view}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn a_goal_that_skips_calendars_plans_without_busy_time() {
    let models = FnTransport::new(|req| {
        let body = body_text(req);
        if body.contains("You are Intake interviewer") {
            return sse_text(&coach_intake().to_string());
        }
        if body.contains("You are Goal researcher") {
            return sse_text(&json!({"summary": "Easy runs.", "sources": []}).to_string());
        }
        if body.contains("You are Goal planner") {
            let plan = json!({"summary": "One run.", "timezone": "Europe/Berlin", "trackers": [], "daily_minutes": null, "needed_minutes": null,
                "items": [{"title": "Easy run", "start": "2026-11-23T09:15:00", "minutes": 30, "rrule": null, "notes": "Run 1 min, walk 2 min, eight rounds"}]});
            return sse_text(&plan.to_string());
        }
        json_reply(400, json!({}))
    });
    let api = api(models.clone(), offline()).await;
    let (u, t) = api.signed_up().await;
    busy_block(&api, u, "Standup", "2026-11-23T08:00:00Z", 30).await;
    let body = json!({"title": "Run a 5k", "timezone": "Europe/Berlin", "use_calendars": false});
    let (_, started) = api.call(Method::Post, "/v1/goals", Some(&t), Some(body)).await;
    let run = started["run_id"].as_str().unwrap().to_string();
    api.tick().await;
    api.answer(&t, &run, "questions", json!([{"id": "weekly_runs", "answer": "3"}])).await;
    api.tick().await;
    assert_eq!(api.run(&t, &run).await["waiting"][0]["node"], "approve");
    let planner: Vec<String> = models.sent().iter().map(body_text).filter(|b| b.contains("You are Goal planner")).collect();
    assert_eq!(planner.len(), 1, "no busy time, so no repair round");
    assert!(!planner[0].contains("Standup"), "{}", planner[0]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn attached_text_and_links_reach_research_from_the_database() {
    let models = FnTransport::new(|req| {
        let body = body_text(req);
        if body.contains("You are Goal researcher") {
            return sse_text(&json!({"summary": "Easy runs.", "sources": []}).to_string());
        }
        if body.contains("You are Intake interviewer") {
            return sse_text(&json!({"questions": []}).to_string());
        }
        json_reply(400, json!({}))
    });
    let api = api(models.clone(), offline()).await;
    let (u, t) = api.signed_up().await;
    let goal = Uuid::now_v7().to_string();
    let file = |name: &str, kind, path: Option<String>, url: Option<&str>, text: Option<&str>, status| aroh_domain::rows::GoalFile {
        id: Uuid::now_v7().to_string(),
        user_id: u.to_string(),
        goal_id: goal.clone(),
        name: name.into(),
        kind,
        storage_path: path,
        url: url.map(str::to_string),
        mime: None,
        size_bytes: 10,
        ingest_status: status,
        extracted_text: text.map(str::to_string),
        updated_at: String::new(),
        deleted: false,
    };
    use aroh_domain::rows::{FileKind, IngestStatus};
    for mut f in [
        file("training-log.txt", FileKind::Document, Some(format!("goals/{goal}/a-training-log.txt")), None, Some("Ran 3 km on Monday"), IngestStatus::Done),
        file("example.org/couch-to-5k", FileKind::Link, None, Some("https://example.org/couch-to-5k"), None, IngestStatus::Pending),
        file("scan.pdf", FileKind::Document, Some(format!("goals/{goal}/b-scan.pdf")), None, None, IngestStatus::Failed),
    ] {
        put_row(&api.pool, &api.hlc, u, &mut f).await.unwrap();
    }
    let body = json!({"goal_id": goal, "title": "Run a 5k", "timezone": "Europe/Berlin"});
    let (status, _) = api.call(Method::Post, "/v1/goals", Some(&t), Some(body)).await;
    assert_eq!(status, 202);
    api.tick().await;
    let research = models.sent().iter().map(body_text).find(|b| b.contains("You are Goal researcher")).expect("research ran");
    assert!(research.contains("Ran 3 km on Monday") && research.contains("training-log.txt"), "{research}");
    assert!(research.contains("https://example.org/couch-to-5k"));
    assert!(research.contains("scan.pdf"), "a file that could not be read still shows by name");
    assert!(research.contains("untrusted"), "file text is fenced as data");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn research_runs_first_and_without_search_the_interviewer_asks_for_what_it_could_not_find() {
    let models = FnTransport::new(|req| {
        let body = body_text(req);
        if body.contains("You are Tutor") && !body.contains("tool_result") {
            return sse_tool("web_search", json!({"query": "The Joy of X chapters"}));
        }
        if body.contains("You are Tutor") {
            return sse_text(&json!({"summary": "Search was not available, so the book's chapter count is unknown.", "sources": []}).to_string());
        }
        reading_reply(req)
    });
    let api = api(models.clone(), offline()).await;
    let (_u, t) = api.signed_up().await;
    let run = api.reading_goal(&t).await;
    let bodies: Vec<String> = models.sent().iter().map(body_text).collect();
    let first = |marker: &str| bodies.iter().position(|b| b.contains(marker)).unwrap_or_else(|| panic!("no call to {marker}"));
    assert!(first("You are Tutor") < first("You are Intake interviewer"), "research runs before the interview");
    assert!(bodies.iter().any(|b| b.contains("You are Tutor") && b.contains("web search is not configured")), "with no search key the tool says so instead of failing");
    let intake = &bodies[first("You are Intake interviewer")];
    assert!(intake.contains("chapter count is unknown"), "the interviewer reads the research, so it can ask for what search could not find");
    assert_eq!(api.run(&t, &run).await["waiting"][0]["node"], "questions");
}

/// Models for the limits flow (Q64): a reading intake with the daily cap, and
/// a planner whose 20 minutes a day need until Dec 15 for a Dec 5 target.
fn limits_models() -> Arc<FnTransport> {
    FnTransport::new(|req| {
        let body = body_text(req);
        if body.contains("You are Intake interviewer") {
            return sse_text(&json!({"questions": [
                {"id": "chapters_done", "text": "How many chapters have you read so far?", "kind": "number", "choices": [], "unit": "chapters", "min": 0, "max": 30, "lasting": null, "known": null},
                {"id": "daily_minutes", "text": "How many minutes a day can you read?", "kind": "number", "choices": [], "unit": "minutes per day", "min": 5, "max": 480, "lasting": null, "known": null}
            ]}).to_string());
        }
        if body.contains("You are Tutor") {
            return sse_text(&json!({"summary": "The book has 22 chapters of about 20 minutes each.", "sources": []}).to_string());
        }
        if body.contains("You are Practice planner") {
            return sse_text(&json!({"summary": "One chapter a day.", "timezone": "UTC", "trackers": [], "daily_minutes": 20, "needed_minutes": 440,
                "items": [{"title": "Read", "start": "2026-11-24T20:00:00", "minutes": 20, "rrule": "FREQ=DAILY", "notes": "Next chapter"}]}).to_string());
        }
        json_reply(400, json!({}))
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AROH_TEST_PG"]
async fn a_plan_late_for_its_date_asks_the_owner_before_approval() {
    let api = api(limits_models(), offline()).await;
    let (_, t) = api.signed_up().await;
    let body = json!({"title": "Finish reading The Joy of X", "deadline": "2026-12-05", "timezone": "UTC"});
    let (status, started) = api.call(Method::Post, "/v1/goals", Some(&t), Some(body)).await;
    assert_eq!(status, 202, "{started}");
    let run = started["run_id"].as_str().unwrap().to_string();
    api.tick().await;
    api.answer(&t, &run, "questions", json!([{"id": "chapters_done", "answer": "0"}, {"id": "daily_minutes", "answer": "20"}])).await;
    api.tick().await;
    let view = api.run(&t, &run).await;
    assert_eq!(view["waiting"][0]["node"], "limit_choice", "{view}");
    let card = &view["waiting"][0]["data"]["limits"]["questions"];
    assert_eq!(card[0]["text"], "At 20 min/day you finish on Dec 15, 10 days after your target. What should change?", "{card}");
    assert_eq!(card[0]["choices"], json!(["Move the date to Dec 15", "Raise the daily minutes", "Lower the target"]));
    api.answer(&t, &run, "limit_choice", json!([{"id": "limit_choice", "answer": "Move the date to Dec 15"}, {"id": "new_minutes", "answer": null}])).await;
    api.tick().await;
    let view = api.run(&t, &run).await;
    assert_eq!(view["waiting"][0]["node"], "approve", "the moved date fits: {view}");
}
