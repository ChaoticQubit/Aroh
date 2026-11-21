use aroh_server::api::{openapi_yaml, router, AppState};
use aroh_server::auth::Verifier;
use aroh_server::config::Config;
use aroh_server::db::PgPoolOptions;
use aroh_server::engine::Engine;
use aroh_server::error::ServerError;
use aroh_server::io::{Deps, CATALOG};
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
    let deps = Deps {
        vault: Vault::new(pool.clone(), Arc::new(config.master_key)),
        catalog: Catalog::load(CATALOG).map_err(|e| ServerError::Config(e.to_string()))?,
        models: http.clone(),
        web: Arc::new(GuardedHttp::new(HttpLimits::default())),
        clock: clock.clone(),
        hlc: Arc::new(ServerClock::default()),
        allow_custom_base_url: config.allow_custom_base_url,
        search_toml: config.search_toml,
        pool,
    };
    let jwks = config.jwks_url.map(|url| (url, http));
    let verifier = Verifier::new(config.jwt_secret.as_deref(), jwks, &config.jwt_audience, clock);
    Ok(AppState { engine: Arc::new(Engine::new(Arc::new(deps))), verifier: Arc::new(verifier) })
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
    match mode {
        "serve" => {
            let listener = tokio::net::TcpListener::bind(&bind).await.map_err(|e| ServerError::Config(format!("bind {bind}: {e}")))?;
            let mut stop = shutdown_signal();
            axum::serve(listener, router(state))
                .with_graceful_shutdown(async move {
                    let _ = stop.wait_for(|s| *s).await;
                })
                .await
                .map_err(|e| ServerError::Config(format!("serve: {e}")))
        }
        "worker" => {
            state.engine.work(WorkOptions::default(), shutdown_signal()).await;
            Ok(())
        }
        _ => Err(ServerError::Config(USAGE.into())),
    }
}

#[tokio::main]
async fn main() {
    let mode = std::env::args().nth(1).unwrap_or_default();
    if let Err(e) = run(&mode).await {
        eprintln!("aroh-server: {e}");
        std::process::exit(2);
    }
}
