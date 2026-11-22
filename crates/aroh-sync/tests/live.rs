use aroh_domain::hlc::Hlc;
use aroh_domain::rows::{samples, Goal};
use aroh_store::{db, repo, sync::sync_all};
use aroh_sync::auth::sign_up_session;
use aroh_sync::rest::PgRest;
use aroh_sync::{Config, Session};
use rusqlite::Connection;

fn config() -> Config {
    let url = std::env::var("AROH_E2E_SUPABASE_URL").expect("AROH_E2E_SUPABASE_URL is set; run e2e/up.sh");
    let key = std::env::var("AROH_E2E_ANON_KEY").expect("AROH_E2E_ANON_KEY is set");
    Config::new(&url, &key).unwrap()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64
}

async fn account(config: &Config) -> Session {
    let email = format!("live-{}@aroh.test", aroh_domain::new_id());
    sign_up_session(config, &email, "live-password-1").await.unwrap()
}

struct Device {
    conn: Connection,
    clock: Hlc,
}

impl Device {
    fn new(node: &str) -> Device {
        Device { conn: db::open_in_memory().unwrap(), clock: Hlc::with_wall_clock(node, now_ms()) }
    }

    async fn sync(&mut self, config: &Config, session: &Session) -> aroh_store::sync::Moved {
        let mut remote = PgRest::new(config.clone(), session.access_token.clone());
        sync_all(&mut self.conn, &mut remote, &session.user_id, &mut self.clock).await.unwrap()
    }

    fn goal(&self, id: &str) -> Option<(String, bool)> {
        self.conn.query_row("SELECT title, deleted FROM goals WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?))).ok()
    }
}

#[tokio::test]
#[ignore = "needs the e2e stack: e2e/up.sh"]
async fn two_devices_converge_through_postgrest() {
    let config = config();
    let me = account(&config).await;
    let other = account(&config).await;
    let mut a = Device::new("00000000000000aa");
    let mut b = Device::new("00000000000000bb");

    let mut goal = Goal { user_id: me.user_id.clone(), category_id: None, ..samples::goal() };
    goal.id = aroh_domain::new_id();
    repo::put(&a.conn, &mut a.clock, &mut goal).unwrap();
    assert_eq!(a.sync(&config, &me).await.pushed, 1);
    assert_eq!(b.sync(&config, &me).await.pulled, 1);
    assert_eq!(b.goal(&goal.id), Some((goal.title.clone(), false)));

    let mut on_b = repo::get::<Goal>(&b.conn, &goal.id).unwrap();
    on_b.title = "Edited on B".into();
    repo::put(&b.conn, &mut b.clock, &mut on_b).unwrap();
    let mut on_a = repo::get::<Goal>(&a.conn, &goal.id).unwrap();
    on_a.title = "Edited on A, later".into();
    a.clock.set_wall_clock(now_ms() + 5_000);
    repo::put(&a.conn, &mut a.clock, &mut on_a).unwrap();
    for _ in 0..2 {
        b.sync(&config, &me).await;
        a.sync(&config, &me).await;
    }
    assert_eq!(a.goal(&goal.id), b.goal(&goal.id));
    assert_eq!(a.goal(&goal.id).unwrap().0, "Edited on A, later", "the later stamp wins");

    b.clock.set_wall_clock(now_ms() + 10_000);
    repo::delete::<Goal>(&b.conn, &mut b.clock, &goal.id).unwrap();
    b.sync(&config, &me).await;
    a.sync(&config, &me).await;
    assert_eq!(a.goal(&goal.id).unwrap().1, true, "the tombstone reaches A");

    let mut stranger = Device::new("00000000000000cc");
    let moved = stranger.sync(&config, &other).await;
    assert_eq!(moved.pulled, 0, "row-level security hides the first account");
    assert_eq!(stranger.goal(&goal.id), None);
}
