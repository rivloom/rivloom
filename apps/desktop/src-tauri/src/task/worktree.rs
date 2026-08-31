use std::ffi::OsString;
use std::fs;
use std::io;
use std::io::Read;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use std::time::Instant;

use sha2::Digest;
use sha2::Sha256;
use thiserror::Error;

use crate::project::ResolvedProject;

const MAX_RUN_ID_BYTES: usize = 128;
const MAX_GIT_OUTPUT_BYTES: u64 = 4 * 1024;
const GIT_COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) struct TaskWorktreeManager {
    managed_root: PathBuf,
    git_program: PathBuf,
}

impl TaskWorktreeManager {
    pub(crate) fn new(managed_root: PathBuf) -> Self {
        Self {
            managed_root,
            git_program: PathBuf::from("git"),
        }
    }

    pub(crate) fn create(
        &self,
        project: &ResolvedProject,
        run_id: &str,
    ) -> Result<TaskWorktree, WorktreeError> {
        if run_id.trim().is_empty() || run_id.len() > MAX_RUN_ID_BYTES {
            return Err(WorktreeError::InvalidRequest);
        }
        let repository =
            dunce::canonicalize(project.cwd()).map_err(|_| WorktreeError::NotRepository)?;
        let reported_root = git_line(
            &self.git_program,
            &repository,
            &["rev-parse".into(), "--show-toplevel".into()],
        )
        .map_err(map_repository_error)?;
        let reported_root =
            dunce::canonicalize(reported_root).map_err(|_| WorktreeError::NotRepository)?;
        if reported_root != repository {
            return Err(WorktreeError::NotRepository);
        }
        let baseline_commit = git_line(
            &self.git_program,
            &repository,
            &[
                "rev-parse".into(),
                "--verify".into(),
                "HEAD^{commit}".into(),
            ],
        )
        .map_err(map_repository_error)?;
        if !valid_commit(&baseline_commit) {
            return Err(WorktreeError::NotRepository);
        }
        if !self.managed_root.is_absolute() {
            return Err(WorktreeError::InvalidRequest);
        }
        let root_parent = self
            .managed_root
            .parent()
            .ok_or(WorktreeError::InvalidRequest)?;
        let root_name = self
            .managed_root
            .file_name()
            .ok_or(WorktreeError::InvalidRequest)?;
        let root_parent =
            dunce::canonicalize(root_parent).map_err(|_| WorktreeError::CreateFailed)?;
        let intended_root = root_parent.join(root_name);
        if paths_overlap(&intended_root, &repository) {
            return Err(WorktreeError::InvalidRequest);
        }
        match fs::create_dir(&intended_root) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(WorktreeError::CreateFailed),
        }
        let managed_root =
            dunce::canonicalize(&intended_root).map_err(|_| WorktreeError::CreateFailed)?;
        if paths_overlap(&managed_root, &repository) {
            return Err(WorktreeError::InvalidRequest);
        }
        let directory_name = format!("run-{:x}", Sha256::digest(run_id.as_bytes()));
        let path = managed_root.join(&directory_name);
        match fs::symlink_metadata(&path) {
            Ok(_) => return Err(WorktreeError::DestinationExists),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => return Err(WorktreeError::CreateFailed),
        }
        run_git(
            &self.git_program,
            &repository,
            &[
                "worktree".into(),
                "add".into(),
                "--detach".into(),
                path.as_os_str().to_owned(),
                baseline_commit.clone().into(),
            ],
            MAX_GIT_OUTPUT_BYTES,
        )
        .map_err(map_create_error)?;
        let canonical_path = dunce::canonicalize(&path).map_err(|_| WorktreeError::CreateFailed)?;
        if canonical_path != path || !exact_target(&managed_root, &canonical_path, &directory_name)
        {
            return Err(WorktreeError::CreateFailed);
        }
        let cwd = canonical_path
            .to_str()
            .ok_or(WorktreeError::InvalidRequest)?
            .to_string();
        Ok(TaskWorktree {
            managed_root,
            repository,
            path: canonical_path,
            directory_name,
            cwd,
            baseline_commit,
            git_program: self.git_program.clone(),
        })
    }
}

pub(crate) struct TaskWorktree {
    managed_root: PathBuf,
    repository: PathBuf,
    path: PathBuf,
    directory_name: String,
    cwd: String,
    baseline_commit: String,
    git_program: PathBuf,
}

impl TaskWorktree {
    pub(crate) fn cwd(&self) -> &str {
        &self.cwd
    }

    pub(crate) fn baseline_commit(&self) -> &str {
        &self.baseline_commit
    }

    pub(crate) fn cleanup(&self) -> WorktreeCleanup {
        if !self.paths_still_match() {
            return WorktreeCleanup::Retained {
                reason: WorktreeCleanupFailure::InvalidTarget,
            };
        }
        let result = run_git(
            &self.git_program,
            &self.repository,
            &[
                "worktree".into(),
                "remove".into(),
                "--force".into(),
                self.path.as_os_str().to_owned(),
            ],
            MAX_GIT_OUTPUT_BYTES,
        );
        match result {
            Err(GitCommandError::Unavailable) => WorktreeCleanup::Retained {
                reason: WorktreeCleanupFailure::GitUnavailable,
            },
            Err(
                GitCommandError::Failed
                | GitCommandError::OutputTooLarge
                | GitCommandError::TimedOut,
            ) => WorktreeCleanup::Retained {
                reason: WorktreeCleanupFailure::GitRejected,
            },
            Ok(_) if target_present(&self.path) => WorktreeCleanup::Retained {
                reason: WorktreeCleanupFailure::GitRejected,
            },
            Ok(_) => WorktreeCleanup::Removed,
        }
    }

    pub(super) fn git_output(
        &self,
        args: &[OsString],
        max_output_bytes: u64,
    ) -> Result<Vec<u8>, GitCommandError> {
        if !self.paths_still_match() {
            return Err(GitCommandError::Failed);
        }
        run_git(&self.git_program, &self.path, args, max_output_bytes)
    }

    fn paths_still_match(&self) -> bool {
        dunce::canonicalize(&self.managed_root).is_ok_and(|root| root == self.managed_root)
            && dunce::canonicalize(&self.path).is_ok_and(|path| path == self.path)
            && exact_target(&self.managed_root, &self.path, &self.directory_name)
    }
}

fn exact_target(managed_root: &std::path::Path, path: &std::path::Path, name: &str) -> bool {
    path == managed_root.join(name) && path.parent() == Some(managed_root)
}

fn paths_overlap(left: &std::path::Path, right: &std::path::Path) -> bool {
    left.starts_with(right) || right.starts_with(left)
}

fn target_present(path: &std::path::Path) -> bool {
    !matches!(
        fs::symlink_metadata(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound
    )
}

fn valid_commit(commit: &str) -> bool {
    matches!(commit.len(), 40 | 64) && commit.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn git_line(
    program: &std::path::Path,
    cwd: &std::path::Path,
    args: &[OsString],
) -> Result<String, GitCommandError> {
    let output = run_git(program, cwd, args, MAX_GIT_OUTPUT_BYTES)?;
    let output = output.strip_suffix(b"\n").unwrap_or(&output);
    let output = output.strip_suffix(b"\r").unwrap_or(output);
    String::from_utf8(output.to_vec()).map_err(|_| GitCommandError::Failed)
}

fn run_git(
    program: &std::path::Path,
    cwd: &std::path::Path,
    args: &[OsString],
    max_output_bytes: u64,
) -> Result<Vec<u8>, GitCommandError> {
    let mut child = Command::new(program)
        .arg("-C")
        .arg(cwd)
        .args(args)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_OBJECT_DIRECTORY")
        .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| GitCommandError::Unavailable)?;
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(GitCommandError::Failed);
    };
    let (sender, receiver) = mpsc::sync_channel(1);
    let reader = thread::Builder::new()
        .name("rivloom-git-output".to_string())
        .spawn(move || {
            let mut output = Vec::new();
            let result = stdout
                .take(max_output_bytes.saturating_add(1))
                .read_to_end(&mut output);
            let _ = sender.send((result, output));
        });
    let Ok(reader) = reader else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(GitCommandError::Failed);
    };
    let deadline = Instant::now() + GIT_COMMAND_TIMEOUT;
    let mut captured = None;
    loop {
        if captured.is_none()
            && let Ok((read_result, output)) = receiver.try_recv()
        {
            if read_result.is_err() || output.len() as u64 > max_output_bytes {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Err(if read_result.is_err() {
                    GitCommandError::Failed
                } else {
                    GitCommandError::OutputTooLarge
                });
            }
            captured = Some(output);
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let output = match captured {
                    Some(output) => output,
                    None => match receiver.recv() {
                        Ok((Ok(_), output)) if output.len() as u64 <= max_output_bytes => output,
                        Ok((Ok(_), _)) => {
                            let _ = reader.join();
                            return Err(GitCommandError::OutputTooLarge);
                        }
                        Ok((Err(_), _)) | Err(_) => {
                            let _ = reader.join();
                            return Err(GitCommandError::Failed);
                        }
                    },
                };
                let _ = reader.join();
                return if status.success() {
                    Ok(output)
                } else {
                    Err(GitCommandError::Failed)
                };
            }
            Ok(None) => {}
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Err(GitCommandError::Failed);
            }
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            return Err(GitCommandError::TimedOut);
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn map_repository_error(error: GitCommandError) -> WorktreeError {
    match error {
        GitCommandError::Unavailable => WorktreeError::GitUnavailable,
        GitCommandError::Failed | GitCommandError::OutputTooLarge | GitCommandError::TimedOut => {
            WorktreeError::NotRepository
        }
    }
}

fn map_create_error(error: GitCommandError) -> WorktreeError {
    match error {
        GitCommandError::Unavailable => WorktreeError::GitUnavailable,
        GitCommandError::Failed | GitCommandError::OutputTooLarge | GitCommandError::TimedOut => {
            WorktreeError::CreateFailed
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum GitCommandError {
    Unavailable,
    Failed,
    OutputTooLarge,
    TimedOut,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(crate) enum WorktreeError {
    #[error("task worktree request is invalid")]
    InvalidRequest,
    #[error("Git is unavailable")]
    GitUnavailable,
    #[error("registered project is not an exact Git repository root")]
    NotRepository,
    #[error("task worktree destination already exists")]
    DestinationExists,
    #[error("task worktree could not be created")]
    CreateFailed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WorktreeCleanup {
    Removed,
    Retained { reason: WorktreeCleanupFailure },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WorktreeCleanupFailure {
    EvidenceIncomplete,
    InvalidTarget,
    GitUnavailable,
    GitRejected,
}

#[cfg(test)]
#[path = "worktree_tests.rs"]
mod tests;
