use aroh_server::api::{openapi_yaml, router};
use aroh_server::boot;
use aroh_server::config::Config;
use aroh_server::engine::Engine;
use aroh_server::error::ServerError;
use aroh_server::push::INTERVAL_SECS;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::watch;
use troupekit_host::WorkOptions;

const USAGE: &str = "usage: aroh-server serve | worker | openapi";

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
    let state = boot::app(config).await?;
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
