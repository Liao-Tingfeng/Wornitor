use super::DbError;

/// A single migration step.
pub struct Migration {
    pub version: u32,
    pub name: &'static str,
    pub sql: &'static str,
}

/// Returns all available migrations in order.
pub fn get_migrations() -> Vec<Migration> {
    vec![
        Migration {
            version: 1,
            name: "create_core_tables",
            sql: include_str!("../../migrations/001_init.sql"),
        },
        Migration {
            version: 2,
            name: "add_performance_indexes",
            sql: include_str!("../../migrations/002_add_indexes.sql"),
        },
        Migration {
            version: 3,
            name: "api_key_direct_storage",
            sql: include_str!("../../migrations/003_api_key.sql"),
        },
        Migration {
            version: 4,
            name: "cost_tracking",
            sql: include_str!("../../migrations/004_cost_tracking.sql"),
        },
        Migration {
            version: 5,
            name: "add_use_batch_api",
            sql: include_str!("../../migrations/005_add_use_batch_api.sql"),
        },
        Migration {
            version: 6,
            name: "add_segment_cost",
            sql: include_str!("../../migrations/006_add_segment_cost.sql"),
        },
    ]
}

/// Get the current migration version from the database.
pub fn get_current_version(conn: &rusqlite::Connection) -> Result<u32, DbError> {
    // Ensure the migrations tracking table exists.
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations (
            version  INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )?;

    let current: u32 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM _migrations",
        [],
        |row| row.get(0),
    )?;
    Ok(current)
}

/// Run all pending migrations in order.
pub fn run_migrations(conn: &rusqlite::Connection) -> Result<(), DbError> {
    let current = get_current_version(conn)?;
    let migrations = get_migrations();

    for m in &migrations {
        if m.version <= current {
            continue; // already applied
        }

        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(m.sql)?;
        tx.execute(
            "INSERT INTO _migrations (version) VALUES (?1)",
            rusqlite::params![m.version],
        )?;
        tx.commit()?;

        eprintln!("[DB] Migration v{} applied: {}", m.version, m.name);
    }

    Ok(())
}
