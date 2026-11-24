use crate::api::AppState;
use crate::auth::Verifier;
use crate::config::Config;
use crate::db::PgPoolOptions;
use crate::engine::Engine;
use crate::error::ServerError;
use crate::io::{Deps, CATALOG};
use crate::push::{Pusher, Signer};
use crate::rows::ServerClock;
use crate::vault::Vault;
use std::sync::Arc;
use troupekit_core::clock::{Clock, SystemClock};
use troupekit_providers::reqwest_transport::ReqwestTransport;
use troupekit_providers::Catalog;
use troupekit_tools::{GuardedHttp, HttpLimits, Transport};

/// Connections one engine instance may hold. The Supabase session pooler
/// allows 15 clients in all, and Vercel runs several instances at once, so
/// each keeps few and drops idle ones after 10 seconds.
pub const MAX_DB_CONNECTIONS: u32 = 3;

/// Builds the engine state from `config`: the Postgres pool, vault,
/// catalog, real transports, optional APNs pusher and JWT verifier.
/// Shared by the `aroh-server` bin and the Vercel entry, so every host
/// runs the same engine. Errors name the failing setting or connection.
pub async fn app(config: Config) -> Result<AppState, ServerError> {
    let pool = PgPoolOptions::new()
        .max_connections(MAX_DB_CONNECTIONS)
        .min_connections(0)
        .idle_timeout(std::time::Duration::from_secs(10))
        .acquire_timeout(std::time::Duration::from_secs(20))
        .connect(&config.database_url)
        .await?;
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
        use crate::fake::{body_text, json_reply, FnTransport, Script};
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
