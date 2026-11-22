use aroh_server::api::{openapi_yaml, router, AppState};
use aroh_server::auth::Verifier;
use aroh_server::config::Config;
use aroh_server::db::PgPoolOptions;
use aroh_server::engine::Engine;
use aroh_server::error::ServerError;
use aroh_server::io::{Deps, CATALOG};
use aroh_server::push::{Pusher, Signer, INTERVAL_SECS};
use aroh_server::rows::ServerClock;
use aroh_server::vault::Vault;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::watch;
use troupekit_core::clock::{Clock, SystemClock};
use troupekit_host::WorkOptions;
use troupekit_providers::reqwest_transport::ReqwestTransport;
use troupekit_providers::Catalog;
use troupekit_tools::{GuardedHttp, HttpLimits, Transport};

const USAGE: &str = "usage: aroh-server serve | worker | openapi";

async fn state(config: Config) -> Result<AppState, ServerError> {
    let pool = PgPoolOptions::new().max_connections(10).connect(&config.database_url).await?;
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let http: Arc<dyn Transport> = Arc::new(ReqwestTransport::new().map_err(|e| ServerError::Config(e.to_string()))?);
    let (models, web) = transports(http.clone())?;
    let deps = Deps {
        vault: Vault::new(pool.clone(), Arc::new(config.master_key)),
        catalog: Catalog::load(CATALOG).map_err(|e| ServerError::Config(e.to_string()))?,
        models,
        web,
        clock: clock.clone(),
        hlc: Arc::new(ServerClock::default()),
        allow_custom_base_url: config.allow_custom_base_url,
        search_toml: config.search_toml,
        pool,
    };
    let pusher = config.apns.map(|apns| {
        eprintln!("aroh-server: push on, topic {}", apns.topic);
        Arc::new(Pusher::new(deps.pool.clone(), http.clone(), Signer::new(apns, clock.clone()), clock.clone()))
    });
    let jwks = config.jwks_url.map(|url| (url, http));
    let verifier = Verifier::new(config.jwt_secret.as_deref(), jwks, &config.jwt_audience, clock);
    let mut engine = Engine::new(Arc::new(deps));
    if let Some(pusher) = pusher {
        engine = engine.with_push(pusher);
    }
    Ok(AppState { engine: Arc::new(engine), verifier: Arc::new(verifier) })
}

/// The model and web transports: real ones, or with the `fake-transports`
/// feature and `AROH_FAKE_SCRIPT` set, scripted models (the file is read
/// per request, so a driver can rewrite it) and a web that answers 404.
fn transports(http: Arc<dyn Transport>) -> Result<(Arc<dyn Transport>, Arc<dyn Transport>), ServerError> {
    #[cfg(feature = "fake-transports")]
    if let Ok(path) = std::env::var("AROH_FAKE_SCRIPT") {
        use aroh_server::fake::{body_text, json_reply, FnTransport, Script};
        eprintln!("aroh-server: fake transports on, script {path}");
        let models: Arc<dyn Transport> = FnTransport::new(move |req| {
            let script = std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| Script::from_json(&t));
            match script {
                Ok(script) => script.answer(&body_text(req)),
                Err(e) => json_reply(500, serde_json::json!({"error": e})),
            }
        });
        let web: Arc<dyn Transport> = FnTransport::new(|_| json_reply(404, serde_json::json!({})));
        return Ok((models, web));
    }
    Ok((http, Arc::new(GuardedHttp::new(HttpLimits::default()))))
}

/// Worker mode: one push dispatch pass every [`INTERVAL_SECS`] while push is on,
/// until `shutdown` turns true. Errors are logged and the loop goes on.
async fn push_loop(engine: Arc<Engine>, mut shutdown: watch::Receiver<bool>) {
    let Some(pusher) = engine.push() else { return };
    while !*shutdown.borrow() {
        if let Err(e) = pusher.dispatch_once().await {
            eprintln!("aroh-server: push pass failed: {e}");
        }
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(INTERVAL_SECS)) => {}
            changed = shutdown.changed() => {
                if changed.is_err() {
                    return;
                }
            }
        }
    }
}

fn shutdown_signal() -> watch::Receiver<bool> {
    let (tx, rx) = watch::channel(false);
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        let _ = tx.send(true);
    });
    rx
}

async fn run(mode: &str) -> Result<(), ServerError> {
    if mode == "openapi" {
        print!("{}", openapi_yaml());
        return Ok(());
    }
    let vars: BTreeMap<String, String> = std::env::vars().collect();
    let config = Config::from_vars(&vars)?;
    let bind = config.bind.clone();
    let state = state(config).await?;
    if mode != "serve" && mode != "worker" {
        return Err(ServerError::Config(USAGE.into()));
    }
    let listener = tokio::net::TcpListener::bind(&bind).await.map_err(|e| ServerError::Config(format!("bind {bind}: {e}")))?;
    let stop = shutdown_signal();
    let mut api_stop = stop.clone();
    let api = axum::serve(listener, router(state.clone())).with_graceful_shutdown(async move {
        let _ = api_stop.wait_for(|s| *s).await;
    });
    if mode == "serve" {
        return api.await.map_err(|e| ServerError::Config(format!("serve: {e}")));
    }
    let engine = state.engine.clone();
    let (served, (), ()) = tokio::join!(api, engine.work(WorkOptions::default(), stop.clone()), push_loop(engine.clone(), stop));
    served.map_err(|e| ServerError::Config(format!("serve: {e}")))
}

#[tokio::main]
async fn main() {
    let mode = std::env::args().nth(1).unwrap_or_default();
    if let Err(e) = run(&mode).await {
        eprintln!("aroh-server: {e}");
        std::process::exit(2);
    }
}
