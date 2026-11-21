use aroh_domain::schema::SYNCED;
use aroh_store::db;
use std::collections::BTreeMap;

const POSTGRES: &str = include_str!("../../../supabase/migrations/20261121000000_rebuild.sql");

fn postgres_tables() -> BTreeMap<String, Vec<String>> {
    let mut tables = BTreeMap::new();
    let mut current: Option<(String, Vec<String>)> = None;
    for line in POSTGRES.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("create table public.") {
            let name = rest.trim_end_matches(" (").to_string();
            current = Some((name, Vec::new()));
        } else if line == ");" {
            if let Some((name, columns)) = current.take() {
                tables.insert(name, columns);
            }
        } else if let Some((_, columns)) = current.as_mut() {
            let first = line.split_whitespace().next().unwrap_or("");
            if !["primary", "check", "unique", "constraint", ""].contains(&first) {
                columns.push(first.to_string());
            }
        }
    }
    tables
}

#[test]
fn sqlite_columns_match_the_registry() {
    let conn = db::open_in_memory().unwrap();
    for table in SYNCED.iter() {
        let mut stmt = conn.prepare(&format!("SELECT name FROM pragma_table_info('{}') ORDER BY cid", table.name)).unwrap();
        let columns: Vec<String> = stmt.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
        assert_eq!(columns, table.names(), "{}", table.name);
    }
}

#[test]
fn postgres_columns_match_the_registry_plus_server_seq() {
    let tables = postgres_tables();
    assert_eq!(tables.len(), 25, "18 synced and 7 server-only tables");
    for table in SYNCED.iter() {
        let mut expected: Vec<String> = table.names().iter().map(|c| c.to_string()).collect();
        expected.push("server_seq".into());
        assert_eq!(tables.get(table.name), Some(&expected), "{}", table.name);
    }
    for server_only in ["jobs", "runs", "run_checkpoints", "run_events", "knowledge_chunks", "memory_logs", "provider_credentials"] {
        assert!(tables[server_only].contains(&"user_id".to_string()), "{server_only}");
    }
}
