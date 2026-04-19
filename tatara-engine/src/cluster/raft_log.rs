use std::fmt::Debug;
use std::ops::RangeBounds;
use std::path::Path;
use std::sync::Arc;

use openraft::anyerror::AnyError;
use openraft::storage::RaftLogStorage;
use openraft::{
    Entry, LogId, LogState, OptionalSend, RaftLogReader, StorageError, StorageIOError, Vote,
};
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use tokio::sync::Mutex;

use super::raft_sm::TypeConfig;
use tatara_core::cluster::types::NodeId;

const LOG_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("raft_log");
const META_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("raft_meta");

const VOTE_KEY: &str = "vote";
const PURGE_KEY: &str = "last_purged";

fn io_read_logs<E: std::error::Error + 'static>(e: &E) -> StorageError<NodeId> {
    StorageIOError::<NodeId>::read_logs(AnyError::new(e)).into()
}

fn io_write_logs<E: std::error::Error + 'static>(e: &E) -> StorageError<NodeId> {
    StorageIOError::<NodeId>::write_logs(AnyError::new(e)).into()
}

fn io_read_vote<E: std::error::Error + 'static>(e: &E) -> StorageError<NodeId> {
    StorageIOError::<NodeId>::read_vote(AnyError::new(e)).into()
}

fn io_write_vote<E: std::error::Error + 'static>(e: &E) -> StorageError<NodeId> {
    StorageIOError::<NodeId>::write_vote(AnyError::new(e)).into()
}

/// Raft log storage backed by redb (pure Rust embedded KV store).
pub struct LogStore {
    db: Arc<Database>,
    /// Serialization lock — redb supports only one writer at a time.
    write_lock: Arc<Mutex<()>>,
}

impl Clone for LogStore {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            write_lock: self.write_lock.clone(),
        }
    }
}

impl LogStore {
    pub fn new(path: &Path) -> anyhow::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let db = Database::create(path)?;

        // Ensure tables exist
        let write_txn = db.begin_write()?;
        write_txn.open_table(LOG_TABLE)?;
        write_txn.open_table(META_TABLE)?;
        write_txn.commit()?;

        Ok(Self {
            db: Arc::new(db),
            write_lock: Arc::new(Mutex::new(())),
        })
    }
}

impl RaftLogReader<TypeConfig> for LogStore {
    async fn try_get_log_entries<RB: RangeBounds<u64> + Clone + Debug + OptionalSend>(
        &mut self,
        range: RB,
    ) -> Result<Vec<Entry<TypeConfig>>, StorageError<NodeId>> {
        let read_txn = self.db.begin_read().map_err(|e| io_read_logs(&e))?;
        let table = read_txn
            .open_table(LOG_TABLE)
            .map_err(|e| io_read_logs(&e))?;

        let mut entries = Vec::new();
        let iter = table.range(range).map_err(|e| io_read_logs(&e))?;

        for item in iter {
            let (_, value) = item.map_err(|e| io_read_logs(&e))?;
            let entry: Entry<TypeConfig> =
                serde_json::from_slice(value.value()).map_err(|e| io_read_logs(&e))?;
            entries.push(entry);
        }

        Ok(entries)
    }
}

impl RaftLogStorage<TypeConfig> for LogStore {
    type LogReader = LogStore;

    async fn get_log_state(&mut self) -> Result<LogState<TypeConfig>, StorageError<NodeId>> {
        let read_txn = self.db.begin_read().map_err(|e| io_read_logs(&e))?;

        // Check for persisted purge state
        let meta_table = read_txn
            .open_table(META_TABLE)
            .map_err(|e| io_read_logs(&e))?;
        let last_purged = match meta_table.get(PURGE_KEY).map_err(|e| io_read_logs(&e))? {
            Some(guard) => {
                let log_id: LogId<NodeId> =
                    serde_json::from_slice(guard.value()).map_err(|e| io_read_logs(&e))?;
                Some(log_id)
            }
            None => None,
        };

        let table = read_txn
            .open_table(LOG_TABLE)
            .map_err(|e| io_read_logs(&e))?;

        let last = table.last().map_err(|e| io_read_logs(&e))?;

        let last_log_id = match last {
            Some(entry) => {
                let bytes = entry.1.value();
                let log_entry: Entry<TypeConfig> =
                    serde_json::from_slice(bytes).map_err(|e| io_read_logs(&e))?;
                Some(log_entry.log_id)
            }
            None => last_purged,
        };

        Ok(LogState {
            last_purged_log_id: last_purged,
            last_log_id,
        })
    }

    async fn get_log_reader(&mut self) -> Self::LogReader {
        self.clone()
    }

    async fn save_vote(&mut self, vote: &Vote<NodeId>) -> Result<(), StorageError<NodeId>> {
        let _lock = self.write_lock.lock().await;
        let bytes = serde_json::to_vec(vote).map_err(|e| io_write_vote(&e))?;

        let write_txn = self.db.begin_write().map_err(|e| io_write_vote(&e))?;
        {
            let mut table = write_txn
                .open_table(META_TABLE)
                .map_err(|e| io_write_vote(&e))?;
            table
                .insert(VOTE_KEY, bytes.as_slice())
                .map_err(|e| io_write_vote(&e))?;
        }
        write_txn.commit().map_err(|e| io_write_vote(&e))?;

        Ok(())
    }

    async fn read_vote(&mut self) -> Result<Option<Vote<NodeId>>, StorageError<NodeId>> {
        let read_txn = self.db.begin_read().map_err(|e| io_read_vote(&e))?;
        let table = read_txn
            .open_table(META_TABLE)
            .map_err(|e| io_read_vote(&e))?;

        match table.get(VOTE_KEY).map_err(|e| io_read_vote(&e))? {
            Some(guard) => {
                let vote: Vote<NodeId> =
                    serde_json::from_slice(guard.value()).map_err(|e| io_read_vote(&e))?;
                Ok(Some(vote))
            }
            None => Ok(None),
        }
    }

    async fn append<I>(
        &mut self,
        entries: I,
        callback: openraft::storage::LogFlushed<TypeConfig>,
    ) -> Result<(), StorageError<NodeId>>
    where
        I: IntoIterator<Item = Entry<TypeConfig>> + OptionalSend,
    {
        let _lock = self.write_lock.lock().await;
        let write_txn = self.db.begin_write().map_err(|e| io_write_logs(&e))?;
        {
            let mut table = write_txn
                .open_table(LOG_TABLE)
                .map_err(|e| io_write_logs(&e))?;

            for entry in entries {
                let index = entry.log_id.index;
                let bytes = serde_json::to_vec(&entry).map_err(|e| io_write_logs(&e))?;
                table
                    .insert(index, bytes.as_slice())
                    .map_err(|e| io_write_logs(&e))?;
            }
        }
        write_txn.commit().map_err(|e| io_write_logs(&e))?;

        callback.log_io_completed(Ok(()));
        Ok(())
    }

    async fn truncate(&mut self, log_id: LogId<NodeId>) -> Result<(), StorageError<NodeId>> {
        let _lock = self.write_lock.lock().await;
        let write_txn = self.db.begin_write().map_err(|e| io_write_logs(&e))?;
        {
            let mut table = write_txn
                .open_table(LOG_TABLE)
                .map_err(|e| io_write_logs(&e))?;

            // Remove all entries from log_id.index onwards
            let to_remove: Vec<u64> = table
                .range(log_id.index..)
                .map_err(|e| io_write_logs(&e))?
                .map(|entry| entry.unwrap().0.value())
                .collect();

            for idx in to_remove {
                table.remove(idx).map_err(|e| io_write_logs(&e))?;
            }
        }
        write_txn.commit().map_err(|e| io_write_logs(&e))?;

        Ok(())
    }

    async fn purge(&mut self, log_id: LogId<NodeId>) -> Result<(), StorageError<NodeId>> {
        let _lock = self.write_lock.lock().await;
        let write_txn = self.db.begin_write().map_err(|e| io_write_logs(&e))?;
        {
            let mut table = write_txn
                .open_table(LOG_TABLE)
                .map_err(|e| io_write_logs(&e))?;

            // Remove all entries up to and including log_id.index
            let to_remove: Vec<u64> = table
                .range(..=log_id.index)
                .map_err(|e| io_write_logs(&e))?
                .map(|entry| entry.unwrap().0.value())
                .collect();

            for idx in to_remove {
                table.remove(idx).map_err(|e| io_write_logs(&e))?;
            }

            // Persist the purge point
            let mut meta = write_txn
                .open_table(META_TABLE)
                .map_err(|e| io_write_logs(&e))?;
            let purge_bytes = serde_json::to_vec(&log_id).map_err(|e| io_write_logs(&e))?;
            meta.insert(PURGE_KEY, purge_bytes.as_slice())
                .map_err(|e| io_write_logs(&e))?;
        }
        write_txn.commit().map_err(|e| io_write_logs(&e))?;

        Ok(())
    }
}
