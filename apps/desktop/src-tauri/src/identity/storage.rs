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

use super::types::RivloomIdentity;

const STORAGE_VERSION: u32 = 1;
const MAX_STORAGE_BYTES: u64 = 16 * 1024;
static UNIQUE_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Deserialize, Serialize)]
struct StorageDocument {
    version: u32,
    identity: RivloomIdentity,
}

#[derive(Deserialize)]
struct StorageEnvelope {
    version: u32,
}

#[derive(Clone)]
pub(crate) struct IdentityStore {
    path: PathBuf,
    replacer: Arc<dyn FileReplacer>,
}

impl IdentityStore {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self::with_replacer(path, Arc::new(PlatformFileReplacer))
    }

    fn with_replacer(path: PathBuf, replacer: Arc<dyn FileReplacer>) -> Self {
        Self { path, replacer }
    }

    pub(crate) fn load(&self) -> Result<Option<RivloomIdentity>, StorageError> {
        self.read_document()
            .map(|document| document.map(|document| document.identity))
    }

    pub(crate) fn save(&self, identity: &RivloomIdentity) -> Result<(), StorageError> {
        if let Some(document) = self.read_document()?
            && document.version != STORAGE_VERSION
        {
            return Err(StorageError::UnsupportedVersion);
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
            let document = StorageDocument {
                version: STORAGE_VERSION,
                identity: identity.clone(),
            };
            serde_json::to_writer_pretty(&mut temporary_file, &document)
                .map_err(|_| StorageError::Write)?;
            temporary_file
                .write_all(b"\n")
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

fn unique_neighbor_path(path: &Path, marker: &str) -> PathBuf {
    let sequence = UNIQUE_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let file_name = path
        .file_name()
        .map_or_else(|| "identity-v1.json".into(), |name| name.to_string_lossy());
    path.with_file_name(format!(
        "{file_name}.{marker}-{}-{sequence}",
        std::process::id()
    ))
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(crate) enum StorageError {
    #[error("identity could not be loaded")]
    Read,
    #[error("identity uses an unsupported storage version")]
    UnsupportedVersion,
    #[error("identity could not be saved")]
    Write,
}

/// Atomically installs a completed sibling identity file at its destination.
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
