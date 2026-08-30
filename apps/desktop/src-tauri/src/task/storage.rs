use std::collections::HashSet;
use std::fs;
use std::fs::File;
use std::fs::OpenOptions;
use std::io;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

use super::types::MAX_EVENTS;
use super::types::TaskRecord;

const STORAGE_VERSION: u32 = 1;
const MAX_STORAGE_BYTES: u64 = 2 * 1024 * 1024;
pub(super) const MAX_STORED_TASKS: usize = 100;
pub(super) const MAX_IDEMPOTENCY_KEY_BYTES: usize = 128;
const PROJECT_ID_PREFIX: &str = "project-v1-";
const PROJECT_ID_DIGEST_BYTES: usize = 64;
static UNIQUE_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StoredRunKey {
    pub(super) idempotency_key: String,
    pub(super) run_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StoredTask {
    pub(super) idempotency_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) project_id: Option<String>,
    pub(super) record: TaskRecord,
    pub(super) run_keys: Vec<StoredRunKey>,
}

#[derive(Deserialize, Serialize)]
struct StorageDocument {
    version: u32,
    tasks: Vec<StoredTask>,
}

#[derive(Deserialize)]
struct StorageEnvelope {
    version: u32,
}

#[derive(Clone)]
pub(super) struct TaskStore {
    path: PathBuf,
    replacer: Arc<dyn FileReplacer>,
}

impl TaskStore {
    pub(super) fn new(path: PathBuf) -> Self {
        Self::with_replacer(path, Arc::new(PlatformFileReplacer))
    }

    fn with_replacer(path: PathBuf, replacer: Arc<dyn FileReplacer>) -> Self {
        Self { path, replacer }
    }

    pub(super) fn load(&self) -> Result<Vec<StoredTask>, StorageError> {
        let Some(document) = self.read_document()? else {
            return Ok(vec![]);
        };
        validate_tasks(&document.tasks)?;
        Ok(document.tasks)
    }

    pub(super) fn save(&self, tasks: &[StoredTask]) -> Result<(), StorageError> {
        let tasks = normalize_tasks(tasks.to_vec());
        validate_tasks(&tasks)?;
        if let Some(document) = self.read_document()? {
            validate_tasks(&document.tasks)?;
        }
        let mut contents = serde_json::to_vec_pretty(&StorageDocument {
            version: STORAGE_VERSION,
            tasks,
        })
        .map_err(|_| StorageError::Write)?;
        contents.push(b'\n');
        if contents.len() as u64 > MAX_STORAGE_BYTES {
            return Err(StorageError::InvalidData);
        }
        let parent = self.path.parent().ok_or(StorageError::Write)?;
        fs::create_dir_all(parent).map_err(|_| StorageError::Write)?;
        let temporary_path = unique_neighbor_path(&self.path, "tmp");
        let result = (|| {
            let mut temporary_file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary_path)
                .map_err(|_| StorageError::Write)?;
            temporary_file
                .write_all(&contents)
                .map_err(|_| StorageError::Write)?;
            temporary_file.flush().map_err(|_| StorageError::Write)?;
            temporary_file.sync_all().map_err(|_| StorageError::Write)?;
            drop(temporary_file);
            self.replacer
                .replace(&temporary_path, &self.path)
                .map_err(|_| StorageError::Write)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary_path);
        }
        result
    }

    fn read_document(&self) -> Result<Option<StorageDocument>, StorageError> {
        let file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(StorageError::Read),
        };
        if file.metadata().map_err(|_| StorageError::Read)?.len() > MAX_STORAGE_BYTES {
            return Err(StorageError::Read);
        }
        let mut contents = Vec::new();
        file.take(MAX_STORAGE_BYTES + 1)
            .read_to_end(&mut contents)
            .map_err(|_| StorageError::Read)?;
        let envelope = match serde_json::from_slice::<StorageEnvelope>(&contents) {
            Ok(envelope) => envelope,
            Err(_) => return self.quarantine_and_load_empty(),
        };
        if envelope.version != STORAGE_VERSION {
            return Err(StorageError::UnsupportedVersion);
        }
        match serde_json::from_slice(&contents) {
            Ok(document) => Ok(Some(document)),
            Err(_) => self.quarantine_and_load_empty(),
        }
    }

    fn quarantine_and_load_empty(&self) -> Result<Option<StorageDocument>, StorageError> {
        fs::rename(&self.path, unique_neighbor_path(&self.path, "corrupt"))
            .map_err(|_| StorageError::Read)?;
        Ok(None)
    }
}

pub(super) fn normalize_tasks(mut tasks: Vec<StoredTask>) -> Vec<StoredTask> {
    tasks.truncate(MAX_STORED_TASKS);
    tasks
}

pub(super) fn valid_idempotency_key(key: &str) -> bool {
    !key.trim().is_empty() && key.len() <= MAX_IDEMPOTENCY_KEY_BYTES
}

pub(super) fn valid_project_id(project_id: &str) -> bool {
    project_id
        .strip_prefix(PROJECT_ID_PREFIX)
        .is_some_and(|digest| {
            digest.len() == PROJECT_ID_DIGEST_BYTES
                && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
}

fn validate_tasks(tasks: &[StoredTask]) -> Result<(), StorageError> {
    if tasks.len() > MAX_STORED_TASKS {
        return Err(StorageError::InvalidData);
    }
    let mut task_ids = HashSet::new();
    let mut task_keys = HashSet::new();
    for task in tasks {
        task.record
            .validate()
            .map_err(|_| StorageError::InvalidData)?;
        if !task_ids.insert(&task.record.id)
            || !valid_idempotency_key(&task.idempotency_key)
            || task
                .project_id
                .as_deref()
                .is_some_and(|project_id| !valid_project_id(project_id))
            || !task_keys.insert(&task.idempotency_key)
            || task.run_keys.len() > MAX_EVENTS
        {
            return Err(StorageError::InvalidData);
        }
        let run_ids = task
            .record
            .runs
            .iter()
            .map(|run| run.id.as_str())
            .collect::<HashSet<_>>();
        let mut idempotency_keys = HashSet::new();
        for run_key in &task.run_keys {
            if !valid_idempotency_key(&run_key.idempotency_key)
                || !idempotency_keys.insert(&run_key.idempotency_key)
                || !run_ids.contains(run_key.run_id.as_str())
            {
                return Err(StorageError::InvalidData);
            }
        }
    }
    Ok(())
}

fn unique_neighbor_path(path: &Path, marker: &str) -> PathBuf {
    let sequence = UNIQUE_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let file_name = path
        .file_name()
        .map_or_else(|| "tasks-v1.json".into(), |name| name.to_string_lossy());
    path.with_file_name(format!(
        "{file_name}.{marker}-{}-{sequence}",
        std::process::id()
    ))
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(super) enum StorageError {
    #[error("tasks could not be loaded")]
    Read,
    #[error("tasks use an unsupported storage version")]
    UnsupportedVersion,
    #[error("tasks contain invalid bounded data")]
    InvalidData,
    #[error("tasks could not be saved")]
    Write,
}

/// Atomically installs a completed sibling task file at its destination.
///
/// Implementations must preserve the current destination if replacement fails.
trait FileReplacer: Send + Sync {
    fn replace(&self, source: &Path, destination: &Path) -> io::Result<()>;
}

#[derive(Debug)]
struct PlatformFileReplacer;

#[cfg(windows)]
impl FileReplacer for PlatformFileReplacer {
    fn replace(&self, source: &Path, destination: &Path) -> io::Result<()> {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::REPLACEFILE_WRITE_THROUGH;
        use windows_sys::Win32::Storage::FileSystem::ReplaceFileW;

        if !destination.exists() {
            return fs::rename(source, destination);
        }
        let source = source
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let destination = destination
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        // SAFETY: Both path buffers are null-terminated and remain alive for the call. The other
        // pointer arguments are optional according to the Windows API contract.
        let replaced = unsafe {
            ReplaceFileW(
                destination.as_ptr(),
                source.as_ptr(),
                std::ptr::null(),
                REPLACEFILE_WRITE_THROUGH,
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        if replaced == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

#[cfg(not(windows))]
impl FileReplacer for PlatformFileReplacer {
    fn replace(&self, source: &Path, destination: &Path) -> io::Result<()> {
        fs::rename(source, destination)?;
        if let Some(parent) = destination.parent() {
            let _ = File::open(parent).and_then(|directory| directory.sync_all());
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "storage_tests.rs"]
mod tests;
