//! Conversion between SQLite values and wire JSON by column kind, shared by the
//! repositories and the sync engine.

use crate::{bad_row, Result};
use aroh_domain::schema::{Kind, TableDef};
use rusqlite::types::Value as Sql;
use serde_json::{Map, Value as Json};

/// A row as wire JSON, keyed by column name.
pub type WireRow = Map<String, Json>;

/// Converts one wire value to its SQLite form for a column of `kind`.
/// Times are normalized to the canonical RFC 3339 form, JSON is stored as text.
/// Errors with `InvalidRow` when the value does not fit the kind.
pub fn to_sql(kind: Kind, value: &Json) -> Result<Sql> {
    Ok(match (kind, value) {
        (_, Json::Null) => Sql::Null,
        (Kind::Text, Json::String(s)) => Sql::Text(s.clone()),
        (Kind::Int, Json::Number(n)) => Sql::Integer(n.as_i64().ok_or_else(|| bad_row("integer out of range"))?),
        (Kind::Bool, Json::Bool(b)) => Sql::Integer(i64::from(*b)),
        (Kind::Json, v) => Sql::Text(v.to_string()),
        (Kind::Time, Json::String(s)) => Sql::Text(aroh_domain::time::normalize(s)?),
        (kind, v) => return Err(bad_row(format!("{v} is not a {kind:?} value"))),
    })
}

/// Converts one SQLite value back to wire JSON for a column of `kind`.
/// Errors with `InvalidRow` when the stored value does not fit the kind.
pub fn from_sql(kind: Kind, value: Sql) -> Result<Json> {
    Ok(match (kind, value) {
        (_, Sql::Null) => Json::Null,
        (Kind::Text | Kind::Time, Sql::Text(s)) => Json::String(s),
        (Kind::Int, Sql::Integer(i)) => Json::from(i),
        (Kind::Bool, Sql::Integer(0)) => Json::Bool(false),
        (Kind::Bool, Sql::Integer(1)) => Json::Bool(true),
        (Kind::Json, Sql::Text(s)) => serde_json::from_str(&s).map_err(|e| bad_row(format!("stored json: {e}")))?,
        (kind, v) => return Err(bad_row(format!("stored {v:?} is not a {kind:?} value"))),
    })
}

/// Converts every registry column of `row` to SQLite values, in column order.
/// Keys outside the registry are ignored; a missing column is an error.
pub fn row_to_sql(table: &TableDef, row: &WireRow) -> Result<Vec<Sql>> {
    table
        .columns
        .iter()
        .map(|c| {
            let value = row.get(c.name).ok_or_else(|| bad_row(format!("{} row without {}", table.name, c.name)))?;
            to_sql(c.kind, value)
        })
        .collect()
}

/// Reads one SQLite result row selected as `table.columns` in order into wire JSON.
pub fn row_from_sql(table: &TableDef, row: &rusqlite::Row<'_>) -> Result<WireRow> {
    let mut out = WireRow::new();
    for (i, c) in table.columns.iter().enumerate() {
        out.insert(c.name.to_string(), from_sql(c.kind, row.get::<_, Sql>(i)?)?);
    }
    assert_eq!(out.len(), table.columns.len());
    Ok(out)
}

/// Normalizes a wire row the way storage would: registry columns only, times in
/// canonical form, then validates it with the table's domain check.
/// Output: the row exactly as a later read returns it.
pub fn normalize(table: &TableDef, row: &WireRow) -> Result<WireRow> {
    let values = row_to_sql(table, row)?;
    let mut out = WireRow::new();
    for (c, v) in table.columns.iter().zip(values) {
        out.insert(c.name.to_string(), from_sql(c.kind, v)?);
    }
    (table.check)(&Json::Object(out.clone()))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn values_round_trip_by_kind() {
        let cases = [
            (Kind::Text, json!("hi")),
            (Kind::Int, json!(42)),
            (Kind::Bool, json!(true)),
            (Kind::Bool, json!(false)),
            (Kind::Json, json!({"a": [1, 2]})),
            (Kind::Time, json!("2026-11-21T09:00:00Z")),
            (Kind::Text, Json::Null),
        ];
        for (kind, value) in cases {
            assert_eq!(from_sql(kind, to_sql(kind, &value).unwrap()).unwrap(), value, "{kind:?}");
        }
        assert_eq!(to_sql(Kind::Time, &json!("2026-11-21T10:00:00+01:00")).unwrap(), Sql::Text("2026-11-21T09:00:00Z".into()));
        assert!(to_sql(Kind::Bool, &json!(1)).is_err());
        assert!(to_sql(Kind::Int, &json!("1")).is_err());
        assert!(to_sql(Kind::Text, &json!({"x": 1})).is_err());
        assert!(to_sql(Kind::Time, &json!("noon")).is_err());
        assert!(from_sql(Kind::Bool, Sql::Integer(2)).is_err());
    }
}
