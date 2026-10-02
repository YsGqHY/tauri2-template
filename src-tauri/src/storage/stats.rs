use rusqlite::Connection;

use crate::error::AppResult;
use crate::models::{StorageStats, StorageTableStats, TableStats};
use crate::state::AppState;

use super::paths;
use super::CLEARABLE_TABLES;

pub fn get_storage_stats(state: &AppState) -> AppResult<StorageStats> {
    let storage = state
        .storage
        .read()
        .map_err(|_| crate::error::AppError::new("STATE_LOCK", "Storage lock failed"))?;
    Ok(paths::stats(&storage))
}

pub fn get_table_stats(state: &AppState) -> AppResult<StorageTableStats> {
    let storage = state
        .storage
        .read()
        .map_err(|_| crate::error::AppError::new("STATE_LOCK", "Storage lock failed"))?;
    let connection = storage
        .db
        .lock()
        .map_err(|_| crate::error::AppError::new("STORAGE_LOCK", "Database lock failed"))?;
    stats_for_connection(&connection, paths::size(&storage.current_path))
}

fn stats_for_connection(connection: &Connection, total_bytes: u64) -> AppResult<StorageTableStats> {
    let mut statement = connection.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )?;
    let names = statement.query_map([], |row| row.get::<_, String>(0))?;
    let mut tables = Vec::new();
    for name in names {
        let name = name?;
        let escaped = name.replace('"', "\"\"");
        let row_count: u64 = connection
            .query_row(&format!("SELECT COUNT(*) FROM \"{escaped}\""), [], |row| {
                row.get::<_, i64>(0)
            })?
            .max(0) as u64;
        tables.push(TableStats {
            label_key: format!("storage.table.{name}"),
            clearable: CLEARABLE_TABLES.contains(&name.as_str()),
            name,
            row_count,
            size_bytes: 0,
            estimated: true,
        });
    }
    Ok(StorageTableStats {
        total_bytes,
        tables,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn table_sizes_are_explicitly_estimated_not_fabricated() {
        let connection = Connection::open_in_memory().expect("db");
        connection
            .execute_batch("CREATE TABLE one(value TEXT); CREATE TABLE two(value TEXT);")
            .expect("tables");
        let stats = stats_for_connection(&connection, 100).expect("stats");
        assert!(stats
            .tables
            .iter()
            .all(|table| table.estimated && table.size_bytes == 0));
    }
}
