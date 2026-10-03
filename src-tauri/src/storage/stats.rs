use std::collections::HashMap;

use rusqlite::Connection;

use crate::error::{AppError, AppResult};
use crate::models::{StorageSnapshot, StorageStats, StorageTableStats, TableStats};
use crate::state::AppState;

use super::paths;
use super::TABLE_DESCRIPTORS;

pub fn get_storage_stats(state: &AppState) -> AppResult<StorageStats> {
    let storage = state
        .storage
        .read()
        .map_err(|_| AppError::new("STATE_LOCK", "Storage lock failed"))?;
    Ok(paths::stats(&storage))
}

pub fn get_table_stats(state: &AppState) -> AppResult<StorageTableStats> {
    let storage = state
        .storage
        .read()
        .map_err(|_| AppError::new("STATE_LOCK", "Storage lock failed"))?;
    let connection = storage
        .db
        .lock()
        .map_err(|_| AppError::new("STORAGE_LOCK", "Database lock failed"))?;
    stats_for_connection(&connection, paths::main_file_size(&storage.current_path))
}

pub fn get_storage_snapshot(state: &AppState) -> AppResult<StorageSnapshot> {
    let storage = state
        .storage
        .read()
        .map_err(|_| AppError::new("STATE_LOCK", "Storage lock failed"))?;
    let connection = storage
        .db
        .lock()
        .map_err(|_| AppError::new("STORAGE_LOCK", "Database lock failed"))?;
    let storage_stats = paths::stats(&storage);
    let table_stats =
        stats_for_connection(&connection, paths::main_file_size(&storage.current_path))?;
    Ok(StorageSnapshot {
        storage: storage_stats,
        table_stats,
    })
}

fn stats_for_connection(
    connection: &Connection,
    total_file_bytes: u64,
) -> AppResult<StorageTableStats> {
    let mut row_counts = HashMap::with_capacity(TABLE_DESCRIPTORS.len());
    for descriptor in TABLE_DESCRIPTORS {
        let escaped = descriptor.name.replace('"', "\"\"");
        let row_count: u64 = connection
            .query_row(&format!("SELECT COUNT(*) FROM \"{escaped}\""), [], |row| {
                row.get::<_, i64>(0)
            })?
            .max(0) as u64;
        row_counts.insert(descriptor.name, row_count);
    }

    let exact_sizes = try_dbstat(connection);
    let total_rows = row_counts.values().copied().sum::<u64>();
    let mut tables = Vec::with_capacity(TABLE_DESCRIPTORS.len());
    for descriptor in TABLE_DESCRIPTORS {
        let row_count = row_counts.get(descriptor.name).copied().unwrap_or_default();
        let (size_bytes, estimated) = match &exact_sizes {
            Some(sizes) => (
                sizes.get(descriptor.name).copied().unwrap_or_default(),
                false,
            ),
            None if total_rows > 0 => (
                (total_file_bytes.saturating_mul(row_count)) / total_rows,
                true,
            ),
            None => (0, true),
        };
        tables.push(TableStats {
            name: descriptor.name.to_string(),
            label_key: descriptor.label_key.to_string(),
            clearable: descriptor.clearable,
            row_count,
            size_bytes,
            estimated,
        });
    }

    Ok(StorageTableStats {
        total_bytes: tables.iter().map(|table| table.size_bytes).sum(),
        tables,
    })
}

fn try_dbstat(connection: &Connection) -> Option<HashMap<&'static str, u64>> {
    let placeholders = std::iter::repeat("?")
        .take(TABLE_DESCRIPTORS.len())
        .collect::<Vec<_>>()
        .join(",");
    let mut statement = connection
        .prepare(&format!(
            "SELECT name, COALESCE(SUM(pgsize), 0) FROM dbstat WHERE name IN ({placeholders}) GROUP BY name"
        ))
        .ok()?;
    let names = TABLE_DESCRIPTORS.iter().map(|descriptor| descriptor.name);
    let mut rows = statement.query(rusqlite::params_from_iter(names)).ok()?;
    let mut sizes = HashMap::with_capacity(TABLE_DESCRIPTORS.len());
    while let Some(row) = rows.next().ok()? {
        let name: String = row.get(0).ok()?;
        let bytes: u64 = row.get::<_, i64>(1).ok()?.max(0) as u64;
        if let Some(descriptor) = TABLE_DESCRIPTORS
            .iter()
            .find(|descriptor| descriptor.name == name)
        {
            sizes.insert(descriptor.name, bytes);
        }
    }
    (sizes.len() == TABLE_DESCRIPTORS.len()).then_some(sizes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn table_sizes_are_explicitly_estimated_not_fabricated() {
        let connection = Connection::open_in_memory().expect("db");
        connection
            .execute_batch(
                "CREATE TABLE app_config(id INTEGER PRIMARY KEY);
                 CREATE TABLE user_preferences(key TEXT PRIMARY KEY);",
            )
            .expect("tables");
        let stats = stats_for_connection(&connection, 100).expect("stats");
        assert_eq!(stats.tables.len(), 2);
        assert!(stats
            .tables
            .iter()
            .all(|table| table.estimated || table.size_bytes > 0));
        assert!(stats
            .tables
            .iter()
            .any(|table| table.name == "app_config" && !table.clearable));
        assert!(stats
            .tables
            .iter()
            .any(|table| table.name == "user_preferences" && table.clearable));
    }

    #[test]
    fn table_total_is_sum_of_table_sizes() {
        let connection = Connection::open_in_memory().expect("db");
        connection
            .execute_batch(
                "CREATE TABLE app_config(id INTEGER PRIMARY KEY);
                 CREATE TABLE user_preferences(key TEXT PRIMARY KEY);
                 INSERT INTO user_preferences(key) VALUES ('showLogo');",
            )
            .expect("tables");
        let stats = stats_for_connection(&connection, 100).expect("stats");
        assert_eq!(
            stats.total_bytes,
            stats
                .tables
                .iter()
                .map(|table| table.size_bytes)
                .sum::<u64>(),
        );
    }
}
