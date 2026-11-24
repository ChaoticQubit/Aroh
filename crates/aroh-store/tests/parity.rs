use aroh_domain::schema::SYNCED;
use aroh_store::db;
use std::collections::BTreeMap;

const POSTGRES: &str = include_str!("../../../supabase/migrations/20261121000000_rebuild.sql");
const LATER: &[&str] = &[
    include_str!("../../../supabase/migrations/20261124000000_day_kind.sql"),
    include_str!("../../../supabase/migrations/20261127000000_goal_redesign.sql"),
    include_str!("../../../supabase/migrations/20261127010000_goal_files_text.sql"),
    include_str!("../../../supabase/migrations/20261128000000_action_cards.sql"),
    include_str!("../../../supabase/migrations/20261203000000_run_provenance.sql"),
];
const CREATED: &[&str] = &[
    include_str!("../../../supabase/migrations/20261126010000_places.sql"),
    include_str!("../../../supabase/migrations/20261128000000_action_cards.sql"),
];

fn added_columns(tables: &mut BTreeMap<String, Vec<String>>) {
    for sql in LATER {
        let mut table: Option<String> = None;
        for line in sql.lines().map(str::trim) {
            if let Some(rest) = line.strip_prefix("alter table public.") {
                table = Some(rest.split_whitespace().next().unwrap_or("").to_string());
            }
            if let (Some(t), Some(rest)) = (&table, line.strip_prefix("add column ")) {
                let column = rest.split_whitespace().next().unwrap_or("").to_string();
                if let Some(columns) = tables.get_mut(t) {
                    columns.push(column);
                }
            }
        }
    }
}

fn postgres_tables() -> BTreeMap<String, Vec<String>> {
    let mut tables = BTreeMap::new();
    let mut current: Option<(String, Vec<String>)> = None;
    for line in std::iter::once(POSTGRES).chain(CREATED.iter().copied()).flat_map(str::lines).map(str::trim) {
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
    added_columns(&mut tables);
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
    assert_eq!(tables.len(), 28, "21 synced and 7 server-only tables");
    for table in SYNCED.iter() {
        let mut expected: Vec<String> = table.names().iter().map(|c| c.to_string()).collect();
        expected.push("server_seq".into());
        expected.sort();
        let mut found = tables.get(table.name).cloned().unwrap_or_default();
        found.sort();
        assert_eq!(found, expected, "{}", table.name);
    }
    for server_only in ["jobs", "runs", "run_checkpoints", "run_events", "knowledge_chunks", "memory_logs", "provider_credentials"] {
        assert!(tables[server_only].contains(&"user_id".to_string()), "{server_only}");
    }
}
