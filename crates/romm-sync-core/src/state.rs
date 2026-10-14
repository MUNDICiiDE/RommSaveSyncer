use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use romm_sync_planner::{GameKey, PlanAction, PlannedItem};
use rusqlite::{Connection, OpenFlags, params};
use serde::Serialize;

use crate::{CoreError, io_error};

pub const STATE_SCHEMA_VERSION: u32 = 1;

const MIGRATION_1: &str = r"
CREATE TABLE IF NOT EXISTS devices (
    device_id TEXT PRIMARY KEY,
    server_origin TEXT NOT NULL,
    display_name TEXT NOT NULL,
    last_seen_ms INTEGER
);
CREATE TABLE IF NOT EXISTS game_mappings (
    platform TEXT NOT NULL,
    rom_id TEXT NOT NULL,
    slot TEXT NOT NULL,
    adapter_id TEXT NOT NULL,
    local_root TEXT NOT NULL,
    identity_state TEXT NOT NULL,
    PRIMARY KEY (platform, rom_id, slot)
);
CREATE TABLE IF NOT EXISTS save_groups (
    platform TEXT NOT NULL,
    rom_id TEXT NOT NULL,
    slot TEXT NOT NULL,
    content_hash TEXT,
    integrity_hash TEXT,
    file_manifest_json TEXT NOT NULL,
    observed_ms INTEGER NOT NULL,
    PRIMARY KEY (platform, rom_id, slot)
);
CREATE TABLE IF NOT EXISTS remote_saves (
    platform TEXT NOT NULL,
    rom_id TEXT NOT NULL,
    slot TEXT NOT NULL,
    content_hash TEXT,
    remote_revision TEXT,
    origin_device TEXT,
    observed_ms INTEGER NOT NULL,
    PRIMARY KEY (platform, rom_id, slot)
);
CREATE TABLE IF NOT EXISTS sync_baselines (
    platform TEXT NOT NULL,
    rom_id TEXT NOT NULL,
    slot TEXT NOT NULL,
    local_content_hash TEXT NOT NULL,
    remote_content_hash TEXT NOT NULL,
    remote_revision TEXT NOT NULL,
    confirmed_ms INTEGER NOT NULL,
    PRIMARY KEY (platform, rom_id, slot)
);
CREATE TABLE IF NOT EXISTS retry_queue (
    retry_id INTEGER PRIMARY KEY AUTOINCREMENT,
    platform TEXT NOT NULL,
    rom_id TEXT NOT NULL,
    slot TEXT NOT NULL,
    operation TEXT NOT NULL,
    attempt_count INTEGER NOT NULL DEFAULT 0,
    not_before_ms INTEGER NOT NULL,
    last_error TEXT,
    UNIQUE (platform, rom_id, slot, operation)
);
CREATE TABLE IF NOT EXISTS plans (
    plan_id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_ms INTEGER NOT NULL,
    status TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS plan_items (
    plan_id INTEGER NOT NULL REFERENCES plans(plan_id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL,
    platform TEXT NOT NULL,
    rom_id TEXT NOT NULL,
    slot TEXT NOT NULL,
    action TEXT NOT NULL,
    reason TEXT NOT NULL,
    has_preconditions INTEGER NOT NULL,
    PRIMARY KEY (plan_id, ordinal)
);
CREATE TABLE IF NOT EXISTS conflicts (
    conflict_id INTEGER PRIMARY KEY AUTOINCREMENT,
    platform TEXT NOT NULL,
    rom_id TEXT NOT NULL,
    slot TEXT NOT NULL,
    reason TEXT NOT NULL,
    archive_path TEXT,
    created_ms INTEGER NOT NULL,
    resolved_ms INTEGER
);
CREATE TABLE IF NOT EXISTS history (
    history_id INTEGER PRIMARY KEY AUTOINCREMENT,
    platform TEXT NOT NULL,
    rom_id TEXT NOT NULL,
    slot TEXT NOT NULL,
    event TEXT NOT NULL,
    result TEXT NOT NULL,
    reason TEXT,
    bytes INTEGER NOT NULL DEFAULT 0,
    occurred_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS execution_journal (
    journal_id INTEGER PRIMARY KEY AUTOINCREMENT,
    platform TEXT NOT NULL,
    rom_id TEXT NOT NULL,
    slot TEXT NOT NULL,
    action TEXT NOT NULL,
    state TEXT NOT NULL,
    root_path TEXT NOT NULL,
    mount_identity TEXT NOT NULL,
    backup_path TEXT,
    stage_path TEXT,
    last_error TEXT,
    created_ms INTEGER NOT NULL,
    updated_ms INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS history_game_idx
    ON history(platform, rom_id, slot, occurred_ms);
CREATE INDEX IF NOT EXISTS journal_state_idx
    ON execution_journal(state);
";

pub struct StateStore {
    connection: Connection,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JournalEntry {
    pub journal_id: i64,
    pub key: GameKey,
    pub action: String,
    pub state: JournalState,
    pub root_path: PathBuf,
    pub mount_identity: String,
    pub backup_path: Option<PathBuf>,
    pub stage_path: Option<PathBuf>,
    pub last_error: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JournalState {
    Prepared,
    Staged,
    BackedUp,
    Replacing,
    Committed,
    RolledBack,
    Recovered,
    Rejected,
    Failed,
}

impl JournalState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::Staged => "staged",
            Self::BackedUp => "backed_up",
            Self::Replacing => "replacing",
            Self::Committed => "committed",
            Self::RolledBack => "rolled_back",
            Self::Recovered => "recovered",
            Self::Rejected => "rejected",
            Self::Failed => "failed",
        }
    }

    fn parse(value: &str) -> Result<Self, CoreError> {
        match value {
            "prepared" => Ok(Self::Prepared),
            "staged" => Ok(Self::Staged),
            "backed_up" => Ok(Self::BackedUp),
            "replacing" => Ok(Self::Replacing),
            "committed" => Ok(Self::Committed),
            "rolled_back" => Ok(Self::RolledBack),
            "recovered" => Ok(Self::Recovered),
            "rejected" => Ok(Self::Rejected),
            "failed" => Ok(Self::Failed),
            _ => Err(CoreError::DatabaseIntegrity(format!(
                "unknown journal state {value:?}"
            ))),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct StatusSummary {
    pub schema_version: u32,
    pub pending_retries: u64,
    pub open_conflicts: u64,
    pub unfinished_executions: u64,
    pub history_entries: u64,
}

impl StateStore {
    /// Opens a writable state database, applying transactional migrations first.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot be backed up, opened, migrated,
    /// configured, or verified.
    pub fn open(path: &Path) -> Result<Self, CoreError> {
        prepare_migration_backup(path)?;
        let connection = Connection::open(path)?;
        let mut store = Self { connection };
        store.configure()?;
        store.migrate()?;
        store.verify_integrity()?;
        Ok(store)
    }

    /// Opens a migrated in-memory database for isolated execution and tests.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite configuration or migration fails.
    pub fn open_in_memory() -> Result<Self, CoreError> {
        let connection = Connection::open_in_memory()?;
        let mut store = Self { connection };
        store.configure()?;
        store.migrate()?;
        Ok(store)
    }

    /// Opens an existing database without creating, migrating, or writing it.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot be opened read-only or fails
    /// its integrity check.
    pub fn open_read_only(path: &Path) -> Result<Self, CoreError> {
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let store = Self { connection };
        let version = store.schema_version()?;
        if version != STATE_SCHEMA_VERSION {
            return Err(CoreError::DatabaseIntegrity(format!(
                "schema version {version} is not supported; expected {STATE_SCHEMA_VERSION}"
            )));
        }
        store.verify_integrity()?;
        Ok(store)
    }

    /// Returns the applied schema version.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot read the version pragma.
    pub fn schema_version(&self) -> Result<u32, CoreError> {
        self.connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(CoreError::from)
    }

    /// Stores a complete plan and its stable reason codes in one transaction.
    ///
    /// # Errors
    ///
    /// Returns an error when serialization or the database transaction fails.
    pub fn record_plan(&mut self, items: &[PlannedItem]) -> Result<i64, CoreError> {
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "INSERT INTO plans(created_ms, status) VALUES (?1, 'planned')",
            [now_ms()?],
        )?;
        let plan_id = transaction.last_insert_rowid();
        for (ordinal, item) in items.iter().enumerate() {
            transaction.execute(
                "INSERT INTO plan_items(
                    plan_id, ordinal, platform, rom_id, slot, action, reason, has_preconditions
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    plan_id,
                    i64::try_from(ordinal).map_err(|_| CoreError::DatabaseIntegrity(
                        "plan contains too many items".to_owned()
                    ))?,
                    item.key.platform.slug(),
                    item.key.rom_id,
                    item.key.slot.0,
                    action_name(item.action),
                    item.reason.as_str(),
                    i64::from(item.preconditions.is_some()),
                ],
            )?;
        }
        transaction.commit()?;
        Ok(plan_id)
    }

    /// Creates a durable prepared journal before mutation starts.
    ///
    /// # Errors
    ///
    /// Returns an error when the journal row cannot be inserted.
    pub fn begin_execution(
        &self,
        key: &GameKey,
        action: PlanAction,
        root: &romm_sync_planner::RootSnapshot,
    ) -> Result<i64, CoreError> {
        let now = now_ms()?;
        self.connection.execute(
            "INSERT INTO execution_journal(
                platform, rom_id, slot, action, state, root_path, mount_identity,
                created_ms, updated_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            params![
                key.platform.slug(),
                key.rom_id,
                key.slot.0,
                action_name(action),
                JournalState::Prepared.as_str(),
                root.resolved_path,
                root.mount_identity,
                now,
            ],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    /// Advances a journal entry and records its recovery metadata.
    ///
    /// # Errors
    ///
    /// Returns an error when the journal entry cannot be updated.
    pub fn update_journal(
        &self,
        journal_id: i64,
        state: JournalState,
        backup_path: Option<&Path>,
        stage_path: Option<&Path>,
        last_error: Option<&str>,
    ) -> Result<(), CoreError> {
        self.connection.execute(
            "UPDATE execution_journal
             SET state = ?2,
                 backup_path = COALESCE(?3, backup_path),
                 stage_path = COALESCE(?4, stage_path),
                 last_error = ?5,
                 updated_ms = ?6
             WHERE journal_id = ?1",
            params![
                journal_id,
                state.as_str(),
                backup_path.map(|path| path.to_string_lossy().into_owned()),
                stage_path.map(|path| path.to_string_lossy().into_owned()),
                last_error,
                now_ms()?,
            ],
        )?;
        Ok(())
    }

    /// Lists non-terminal executions that require startup recovery.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed journal data or a failed database query.
    pub fn unfinished_journals(&self) -> Result<Vec<JournalEntry>, CoreError> {
        let mut statement = self.connection.prepare(
            "SELECT journal_id, platform, rom_id, slot, action, state, root_path,
                    mount_identity, backup_path, stage_path, last_error
             FROM execution_journal
             WHERE state IN ('prepared', 'staged', 'backed_up', 'replacing')
             ORDER BY journal_id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, Option<String>>(9)?,
                row.get::<_, Option<String>>(10)?,
            ))
        })?;

        let mut entries = Vec::new();
        for row in rows {
            let (
                id,
                platform,
                rom_id,
                slot,
                action,
                journal_state,
                root,
                mount,
                backup,
                staging_path,
                error,
            ) = row?;
            entries.push(JournalEntry {
                journal_id: id,
                key: GameKey {
                    platform: parse_platform(&platform)?,
                    rom_id,
                    slot: romm_sync_planner::SlotId(slot),
                },
                action,
                state: JournalState::parse(&journal_state)?,
                root_path: PathBuf::from(root),
                mount_identity: mount,
                backup_path: backup.map(PathBuf::from),
                stage_path: staging_path.map(PathBuf::from),
                last_error: error,
            });
        }
        Ok(entries)
    }

    /// Appends an immutable execution or recovery history event.
    ///
    /// # Errors
    ///
    /// Returns an error when the history row cannot be inserted.
    pub fn record_history(
        &self,
        key: &GameKey,
        event: &str,
        result: &str,
        reason: Option<&str>,
        bytes: u64,
    ) -> Result<(), CoreError> {
        self.connection.execute(
            "INSERT INTO history(
                platform, rom_id, slot, event, result, reason, bytes, occurred_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                key.platform.slug(),
                key.rom_id,
                key.slot.0,
                event,
                result,
                reason,
                i64::try_from(bytes).unwrap_or(i64::MAX),
                now_ms()?,
            ],
        )?;
        Ok(())
    }

    /// Persists the last verified local/remote synchronization baseline.
    ///
    /// # Errors
    ///
    /// Returns an error when the baseline cannot be stored.
    pub fn upsert_baseline(
        &self,
        key: &GameKey,
        local_content_hash: &str,
        remote_content_hash: &str,
        remote_revision: &str,
    ) -> Result<(), CoreError> {
        self.connection.execute(
            "INSERT INTO sync_baselines(
                platform, rom_id, slot, local_content_hash, remote_content_hash,
                remote_revision, confirmed_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(platform, rom_id, slot) DO UPDATE SET
                local_content_hash = excluded.local_content_hash,
                remote_content_hash = excluded.remote_content_hash,
                remote_revision = excluded.remote_revision,
                confirmed_ms = excluded.confirmed_ms",
            params![
                key.platform.slug(),
                key.rom_id,
                key.slot.0,
                local_content_hash,
                remote_content_hash,
                remote_revision,
                now_ms()?,
            ],
        )?;
        Ok(())
    }

    /// Returns aggregate read-only status counts for the CLI.
    ///
    /// # Errors
    ///
    /// Returns an error when any status query fails.
    pub fn status(&self) -> Result<StatusSummary, CoreError> {
        Ok(StatusSummary {
            schema_version: self.schema_version()?,
            pending_retries: self.count("SELECT COUNT(*) FROM retry_queue")?,
            open_conflicts: self
                .count("SELECT COUNT(*) FROM conflicts WHERE resolved_ms IS NULL")?,
            unfinished_executions: self.count(
                "SELECT COUNT(*) FROM execution_journal
                 WHERE state IN ('prepared', 'staged', 'backed_up', 'replacing')",
            )?,
            history_entries: self.count("SELECT COUNT(*) FROM history")?,
        })
    }

    fn configure(&self) -> Result<(), CoreError> {
        self.connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = FULL;",
        )?;
        Ok(())
    }

    fn migrate(&mut self) -> Result<(), CoreError> {
        let version = self.schema_version()?;
        if version > STATE_SCHEMA_VERSION {
            return Err(CoreError::DatabaseIntegrity(format!(
                "database schema {version} is newer than supported {STATE_SCHEMA_VERSION}"
            )));
        }
        if version == 0 {
            let transaction = self.connection.transaction()?;
            transaction.execute_batch(MIGRATION_1)?;
            transaction.pragma_update(None, "user_version", STATE_SCHEMA_VERSION)?;
            transaction.commit()?;
        }
        Ok(())
    }

    fn verify_integrity(&self) -> Result<(), CoreError> {
        let result: String = self
            .connection
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        if result == "ok" {
            Ok(())
        } else {
            Err(CoreError::DatabaseIntegrity(result))
        }
    }

    fn count(&self, query: &str) -> Result<u64, CoreError> {
        let value: i64 = self.connection.query_row(query, [], |row| row.get(0))?;
        u64::try_from(value)
            .map_err(|_| CoreError::DatabaseIntegrity("negative row count".to_owned()))
    }
}

fn prepare_migration_backup(path: &Path) -> Result<(), CoreError> {
    if !path.exists() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| io_error("create directory", parent, error))?;
        }
        return Ok(());
    }

    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version >= STATE_SCHEMA_VERSION {
        return Ok(());
    }
    let backup = migration_backup_path(path, version);
    if !backup.exists() {
        connection.backup(rusqlite::MAIN_DB, &backup, None)?;
    }
    Ok(())
}

#[must_use]
pub fn migration_backup_path(path: &Path, version: u32) -> PathBuf {
    let file_name = path
        .file_name()
        .map_or_else(|| "state.db".into(), |name| name.to_string_lossy());
    path.with_file_name(format!("{file_name}.pre-migration-v{version}.bak"))
}

fn now_ms() -> Result<i64, CoreError> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| CoreError::Rejected(format!("system clock before Unix epoch: {error}")))?;
    i64::try_from(elapsed.as_millis())
        .map_err(|_| CoreError::Rejected("system clock value exceeds i64".to_owned()))
}

const fn action_name(action: PlanAction) -> &'static str {
    match action {
        PlanAction::Upload => "upload",
        PlanAction::Download => "download",
        PlanAction::NoOp => "no_op",
        PlanAction::Conflict => "conflict",
        PlanAction::Defer => "defer",
        PlanAction::Unsupported => "unsupported",
        PlanAction::Error => "error",
    }
}

fn parse_platform(value: &str) -> Result<romm_sync_planner::Platform, CoreError> {
    romm_sync_planner::Platform::ALL
        .into_iter()
        .find(|platform| platform.slug() == value)
        .ok_or_else(|| CoreError::DatabaseIntegrity(format!("unknown platform {value:?}")))
}
