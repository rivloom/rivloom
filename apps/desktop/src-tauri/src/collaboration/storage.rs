use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::brain::{Brain, BrainError, MAX_BRAIN_BYTES, OwnerProfile, PresenceReset};
use super::credential::{IssuedCredential, SecretToken};

const MAX_FILE_BYTES: usize = MAX_BRAIN_BYTES * 2 + 1024;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Document {
    version: u32,
    // Keep the original JSON text so duplicate authority keys reach the strict Brain decoder.
    payload: String,
    sha256: [u8; 32],
}

/// One writer for an app-owned local directory. Never share snapshots with peers or call Runtime
/// operations inside a transaction. Drop releases the OS lock; the lock marker is never unlinked.
pub(super) struct BrainStore {
    path: PathBuf,
    _lock: File,
    brain: Brain,
    disk_hash: [u8; 32],
    poisoned: bool,
    replacer: Arc<dyn FileReplacer>,
}

enum OpenMode {
    Create,
    Existing,
}

impl BrainStore {
    pub(super) fn create(
        directory: PathBuf,
        brain_id: String,
        owner: OwnerProfile<'_>,
        now: i64,
    ) -> Result<(Self, IssuedCredential), StorageError> {
        let (brain, credential) = Brain::bootstrap(brain_id, owner, now)?;
        let (path, lock) = lock_directory(&directory, OpenMode::Create)?;
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Ok(_) | Err(_) => return Err(StorageError::Existing),
        }
        let bytes = document(&brain)?;
        let store = Self {
            path,
            _lock: lock,
            brain,
            disk_hash: Sha256::digest(&bytes).into(),
            poisoned: false,
            replacer: Arc::new(PlatformFileReplacer),
        };
        store.write(&bytes)?;
        Ok((store, credential))
    }

    pub(super) fn open(directory: PathBuf, brain_id: &str, now: i64) -> Result<Self, StorageError> {
        let (path, lock) = lock_directory(&directory, OpenMode::Existing)?;
        let bytes = read(&path)?;
        let document: Document =
            serde_json::from_slice(&bytes).map_err(|_| StorageError::Invalid)?;
        if document.version != 1
            || document.payload.len() > MAX_BRAIN_BYTES
            || <[u8; 32]>::from(Sha256::digest(document.payload.as_bytes())) != document.sha256
        {
            return Err(StorageError::Invalid);
        }
        let brain = Brain::decode(document.payload.as_bytes(), brain_id)?;
        let mut store = Self {
            path,
            _lock: lock,
            brain,
            disk_hash: Sha256::digest(&bytes).into(),
            poisoned: false,
            replacer: Arc::new(PlatformFileReplacer),
        };
        store.transact(now, |brain| {
            brain.reset_presence(now, PresenceReset::Restart)
        })?;
        Ok(store)
    }

    pub(super) fn brain(&self) -> Result<&Brain, StorageError> {
        if self.poisoned {
            return Err(StorageError::Unavailable);
        }
        Ok(&self.brain)
    }

    /// Stage only in-memory operations. No externally visible effects are allowed in the closure.
    /// A returned secret/ack is released only after the full snapshot has been committed.
    pub(super) fn transact<R>(
        &mut self,
        now: i64,
        operation: impl FnOnce(&mut Brain) -> Result<R, BrainError>,
    ) -> Result<R, StorageError> {
        self.brain()?;
        let current = read(&self.path);
        if !current.is_ok_and(|bytes| <[u8; 32]>::from(Sha256::digest(bytes)) == self.disk_hash) {
            self.poisoned = true;
            return Err(StorageError::Changed);
        }
        let baseline = self.brain.encode()?;
        let mut candidate = Brain::decode(&baseline, self.brain.brain_id())?;
        candidate.observe(now)?;
        let staged = operation(&mut candidate)
            .map_err(StorageError::from)
            .and_then(|value| Ok((value, document(&candidate)?)));
        let (result, bytes) = match staged {
            Ok((value, bytes)) => (Ok(value), bytes),
            Err(error) => {
                candidate = Brain::decode(&baseline, self.brain.brain_id())?;
                // Rejected operations and invalid candidates both retain trusted observed time.
                candidate.observe(now)?;
                (Err(error), document(&candidate)?)
            }
        };
        let hash = Sha256::digest(&bytes).into();
        if hash != self.disk_hash && self.write(&bytes).is_err() {
            self.poisoned = true;
            return Err(StorageError::Write);
        }
        self.disk_hash = hash;
        self.brain = candidate;
        result
    }

    fn write(&self, bytes: &[u8]) -> Result<(), StorageError> {
        let nonce = SecretToken::generate().map_err(BrainError::from)?;
        let temporary = self
            .path
            .with_file_name(format!("brain-{}.tmp", nonce.expose_secret()));
        let result = (|| {
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&temporary)?;
            file.write_all(bytes)?;
            file.sync_all()?;
            drop(file);
            self.replacer.replace(&temporary, &self.path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result.map_err(|_| StorageError::Write)
    }
}

fn lock_directory(directory: &Path, mode: OpenMode) -> Result<(PathBuf, File), StorageError> {
    if !directory.is_absolute() {
        return Err(StorageError::Invalid);
    }
    if matches!(mode, OpenMode::Create) {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(directory).map_err(|_| StorageError::Write)?;
    }
    let metadata = fs::symlink_metadata(directory).map_err(|_| StorageError::Read)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(StorageError::Invalid);
    }
    let directory = fs::canonicalize(directory).map_err(|_| StorageError::Read)?;
    let lock_path = directory.join("brain-v1.lock");
    if matches!(mode, OpenMode::Existing) {
        let metadata = fs::symlink_metadata(&lock_path).map_err(|_| StorageError::Read)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(StorageError::Invalid);
        }
    }
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(true)
        .create_new(matches!(mode, OpenMode::Create));
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let lock = options
        .open(lock_path)
        .map_err(|_| StorageError::Existing)?;
    fs2::FileExt::try_lock_exclusive(&lock).map_err(|_| StorageError::Locked)?;
    Ok((directory.join("brain-v1.json"), lock))
}

fn document(brain: &Brain) -> Result<Vec<u8>, StorageError> {
    let payload = String::from_utf8(brain.encode()?).map_err(|_| StorageError::Invalid)?;
    let sha256 = Sha256::digest(payload.as_bytes()).into();
    let bytes = serde_json::to_vec(&Document {
        version: 1,
        payload,
        sha256,
    })
    .map_err(|_| StorageError::Invalid)?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err(StorageError::Invalid);
    }
    Ok(bytes)
}

fn read(path: &Path) -> Result<Vec<u8>, StorageError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| StorageError::Read)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAX_FILE_BYTES as u64
    {
        return Err(StorageError::Invalid);
    }
    let file = File::open(path).map_err(|_| StorageError::Read)?;
    let mut bytes = Vec::new();
    file.take((MAX_FILE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| StorageError::Read)?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err(StorageError::Invalid);
    }
    Ok(bytes)
}

/// Install one complete, synced sibling snapshot. An error may follow a completed replacement;
/// callers must stop serving and reopen/validate storage instead of assuming rollback succeeded.
trait FileReplacer: Send + Sync {
    fn replace(&self, source: &Path, destination: &Path) -> io::Result<()>;
}

struct PlatformFileReplacer;

#[cfg(windows)]
impl FileReplacer for PlatformFileReplacer {
    fn replace(&self, source: &Path, destination: &Path) -> io::Result<()> {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        };
        let source: Vec<_> = source.as_os_str().encode_wide().chain([0]).collect();
        let destination: Vec<_> = destination.as_os_str().encode_wide().chain([0]).collect();
        // SAFETY: Both paths are NUL-terminated and live throughout the call. Temporary and target
        // are siblings; do not allow cross-volume copy/delete or delayed reboot operations.
        if unsafe {
            MoveFileExW(
                source.as_ptr(),
                destination.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

#[cfg(not(windows))]
impl FileReplacer for PlatformFileReplacer {
    fn replace(&self, source: &Path, destination: &Path) -> io::Result<()> {
        fs::rename(source, destination)?;
        File::open(
            destination
                .parent()
                .ok_or_else(|| io::Error::other("Invalid storage directory"))?,
        )?
        .sync_all()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(super) enum StorageError {
    #[error("Brain storage could not be read")]
    Read,
    #[error("Brain storage commit could not be confirmed")]
    Write,
    #[error("Brain storage contains invalid bounded data")]
    Invalid,
    #[error("Brain storage already exists or is unavailable")]
    Existing,
    #[error("Brain storage writer lock is unavailable")]
    Locked,
    #[error("Brain storage changed outside this writer")]
    Changed,
    #[error("Brain storage must be reopened after a failed commit")]
    Unavailable,
    #[error(transparent)]
    State(#[from] BrainError),
}

#[cfg(test)]
#[path = "storage_tests.rs"]
mod tests;
