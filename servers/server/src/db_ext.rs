//! SurrealDB result conversion utilities.
//!
//! SurrealDB 3.0 uses native typed values (`Value::Datetime`, `Value::RecordId`,
//! etc.) for SCHEMAFULL table results. The built-in `serde_json::Value` `SurrealValue`
//! implementation cannot convert these types (it only handles `None`, `Null`, `Bool`,
//! `Number`, `String`, `Object`, `Array`).
//!
//! This module provides helper functions that:
//! 1. Take query results as SurrealDB native `Value` (which always succeeds)
//! 2. Recursively convert to `serde_json::Value` (handling `Datetime`, `RecordId`, etc.)
//! 3. Deserialize into Rust structs via `serde_json::from_value`

use serde::de::DeserializeOwned;
use surrealdb::IndexedResults;
use surrealdb::types::{Number, RecordIdKey, Value};

use crate::error::{AppError, Result};

/// Format a SurrealDB `RecordId` as `"table:key"` string.
pub fn record_id_to_string(rid: &surrealdb::types::RecordId) -> String {
    let table = rid.table.as_str();
    let key = match &rid.key {
        RecordIdKey::Number(n) => n.to_string(),
        RecordIdKey::String(s) => s.clone(),
        _ => format!("{:?}", rid.key),
    };
    format!("{table}:{key}")
}

/// Convert a SurrealDB native `Value` to a `serde_json::Value`.
///
/// Handles types that SurrealDB's built-in `serde_json::Value` SurrealValue
/// implementation rejects: `Datetime` → ISO-8601 string, `RecordId` → `"table:key"`
/// string, `Duration` → string, etc.
pub fn value_to_json(value: Value) -> serde_json::Value {
    match value {
        Value::None | Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(b),
        Value::Number(n) => match n {
            Number::Int(i) => serde_json::Value::Number(i.into()),
            Number::Float(f) => serde_json::Number::from_f64(f)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
            Number::Decimal(d) => serde_json::Value::String(d.to_string()),
        },
        Value::String(s) => serde_json::Value::String(s),
        Value::Datetime(dt) => {
            // Use chrono's RFC-3339 formatting via Deref<Target=DateTime<Utc>>.
            serde_json::Value::String(dt.to_rfc3339())
        }
        Value::RecordId(rid) => serde_json::Value::String(record_id_to_string(&rid)),
        Value::Object(obj) => {
            let map = obj
                .into_inner()
                .into_iter()
                .map(|(k, v)| (k, value_to_json(v)))
                .collect();
            serde_json::Value::Object(map)
        }
        Value::Array(arr) => serde_json::Value::Array(arr.into_iter().map(value_to_json).collect()),
        // Duration, Geometry, File, Bytes, Uuid, etc. → debug string fallback.
        other => serde_json::Value::String(format!("{other:?}")),
    }
}

/// Take a single optional result from a SurrealDB query response.
///
/// The result is converted from SurrealDB native types to JSON, then
/// deserialized into `T`. Returns `Ok(None)` if the query returned no rows.
pub fn take_one<T: DeserializeOwned>(
    response: &mut IndexedResults,
    index: usize,
) -> Result<Option<T>> {
    let value: Value = response.take(index).map_err(AppError::Db)?;
    match &value {
        Value::None | Value::Null => return Ok(None),
        Value::Array(arr) if arr.is_empty() => return Ok(None),
        _ => {}
    }

    // If the result is an array, extract the first element.
    let single = match value {
        Value::Array(mut arr) => {
            if arr.is_empty() {
                return Ok(None);
            }
            arr.swap_remove(0)
        }
        v => v,
    };

    if matches!(&single, Value::None | Value::Null) {
        return Ok(None);
    }

    let json = value_to_json(single);
    serde_json::from_value(json)
        .map(Some)
        .map_err(|e| AppError::Internal(format!("deserialize error: {e}")))
}

/// Take multiple results from a SurrealDB query response.
///
/// Each row is converted from SurrealDB native types to JSON, then
/// deserialized into `T`.
pub fn take_many<T: DeserializeOwned>(
    response: &mut IndexedResults,
    index: usize,
) -> Result<Vec<T>> {
    let value: Value = response.take(index).map_err(AppError::Db)?;
    match value {
        Value::None | Value::Null => Ok(vec![]),
        Value::Array(arr) => arr
            .into_iter()
            .filter(|v| !matches!(v, Value::None | Value::Null))
            .map(|v| {
                let json = value_to_json(v);
                serde_json::from_value(json)
                    .map_err(|e| AppError::Internal(format!("deserialize error: {e}")))
            })
            .collect(),
        v => {
            let json = value_to_json(v);
            serde_json::from_value(json)
                .map(|item| vec![item])
                .map_err(|e| AppError::Internal(format!("deserialize error: {e}")))
        }
    }
}
