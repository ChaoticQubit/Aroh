mod common;

use aroh_server::cutoff::{RoundBudget, RunBudget};
use aroh_server::db;
use aroh_server::io::CATALOG;
use common::{pg, user, T0};
use std::sync::Arc;
use troupekit_core::clock::FixedClock;
use troupekit_core::ids::RunId;
use troupekit_providers::Catalog;

#[tokio::test]
#[ignore = "needs AROH_TEST_PG"]
async fn the_run_budget_counts_what_the_run_already_spent() {
    let pool = pg().await;
    let u = user(&pool).await;
    let run = RunId::new();
    db::query("insert into public.budgets (id, user_id, scope, limit_usd_micros, updated_at) values (gen_random_uuid(), $1, 'run', 1000000, '1763683200001-0000-aaaaaaaaaaaaaaaa')")
        .bind(u)
        .execute(&pool)
        .await
        .expect("budget inserted");
    db::query("insert into public.usage (id, user_id, run_id, vendor, model, input_tokens, output_tokens, cost_usd_micros, occurred_at, updated_at) values (gen_random_uuid(), $1, $2, 'anthropic', 'claude-opus-5-5', 10, 5, 600000, now(), '1763683200002-0000-aaaaaaaaaaaaaaaa')")
        .bind(u)
        .bind(run.0)
        .execute(&pool)
        .await
        .expect("usage inserted");
    let price = Catalog::load(CATALOG).unwrap().price("anthropic/claude-opus-5-5").unwrap();
    let budget = RunBudget { pool, clock: Arc::new(FixedClock::new(T0)), price, user: u, goal: None, run: Some(run) };
    assert!(budget.allows(300_000).await.unwrap(), "600000 spent plus 300000 fits 1000000");
    assert!(!budget.allows(500_000).await.unwrap(), "600000 spent plus 500000 crosses 1000000");
}
