//! Registry of synced tables: the one column list that SQLite, Postgres and the
//! sync wire are checked against.

use crate::rows;
use crate::Result;

/// How a column's value is stored and sent.
/// SQLite stores `Bool` as 0/1, `Json` as text and `Time` as canonical RFC 3339 text;
/// the wire carries JSON bool, nested JSON and RFC 3339 strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Text,
    Int,
    Bool,
    Json,
    Time,
}

/// One column of a synced table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Column {
    pub name: &'static str,
    pub kind: Kind,
}

/// One synced table: its name, columns in storage order, and `check`, which
/// deserializes a wire row (registry columns only) into its row struct and validates it.
#[derive(Debug)]
pub struct TableDef {
    pub name: &'static str,
    pub columns: &'static [Column],
    pub check: fn(&serde_json::Value) -> Result<()>,
}

impl TableDef {
    /// Returns the column called `name`, if the table has one.
    pub fn column(&self, name: &str) -> Option<&Column> {
        self.columns.iter().find(|c| c.name == name)
    }

    /// Returns the column names in storage order.
    pub fn names(&self) -> Vec<&'static str> {
        self.columns.iter().map(|c| c.name).collect()
    }
}

/// Every synced table, in push and pull order: parents before children.
pub static SYNCED: [&TableDef; 21] = [
    &rows::CATEGORIES,
    &rows::GOALS,
    &rows::GOAL_LINKS,
    &rows::GOAL_FILES,
    &rows::PERSONAS,
    &rows::PLUGINS,
    &rows::TRACKERS,
    &rows::TRACKER_ENTRIES,
    &rows::ACTION_SERIES,
    &rows::ACTIONS,
    &rows::ACTION_LOGS,
    &rows::PROPOSALS,
    &rows::PREFERENCES,
    &rows::BUSY_BLOCKS,
    &rows::CALENDAR_RULES,
    &rows::PROVIDER_SETTINGS,
    &rows::SEARCH_SETTINGS,
    &rows::BUDGETS,
    &rows::USAGE,
    &rows::PLACES,
    &rows::PLACE_LINKS,
];

/// Returns the synced table called `name`, if any.
pub fn table(name: &str) -> Option<&'static TableDef> {
    SYNCED.iter().copied().find(|t| t.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn every_table_has_the_sync_columns_and_unique_names() {
        let names: BTreeSet<&str> = SYNCED.iter().map(|t| t.name).collect();
        assert_eq!(names.len(), SYNCED.len());
        for t in SYNCED.iter() {
            let cols = t.names();
            assert_eq!(&cols[..2], &["id", "user_id"], "{}", t.name);
            assert_eq!(&cols[cols.len() - 2..], &["updated_at", "deleted"], "{}", t.name);
            let unique: BTreeSet<&str> = cols.iter().copied().collect();
            assert_eq!(unique.len(), cols.len(), "{}", t.name);
            assert_eq!(t.column("deleted").unwrap().kind, Kind::Bool);
            assert!(table(t.name).is_some());
        }
        assert!(table("provider_credentials").is_none());
    }
}
