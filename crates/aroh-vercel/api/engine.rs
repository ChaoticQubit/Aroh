use aroh_server::api::router;
use aroh_server::boot;
use aroh_server::config::Config;
use std::collections::BTreeMap;
use tower::ServiceBuilder;
use vercel_runtime::axum::VercelLayer;
use vercel_runtime::Error;

/// Vercel entry of the engine in step mode: builds the engine once per
/// instance from the process environment and serves the full router
/// through Vercel's function bridge. No tick loop runs here; pg_cron and
/// the jobs trigger call `/v1/tick`, `/v1/push/tick` and `/v1/hooks/enqueue`.
#[tokio::main]
async fn main() -> Result<(), Error> {
    let vars: BTreeMap<String, String> = std::env::vars().collect();
    let state = boot::app(Config::from_vars(&vars)?).await?;
    let app = ServiceBuilder::new().layer(VercelLayer::new()).service(router(state));
    vercel_runtime::run(app).await
}
