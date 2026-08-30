use std::ffi::OsString;

use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;
use thiserror::Error;

use super::worktree::GitCommandError;
use super::worktree::TaskWorktree;

pub(crate) const MAX_PATCH_BYTES: u64 = 512 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum PatchArtifactState {
    Empty,
    Complete,
    TooLarge,
    UnsupportedEncoding,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PatchArtifact {
    pub(crate) baseline_commit: String,
    pub(crate) state: PatchArtifactState,
    pub(crate) limit_bytes: u64,
    pub(crate) byte_count: Option<u64>,
    pub(crate) sha256: Option<String>,
    pub(crate) patch: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PatchArtifactMetadata {
    pub(crate) baseline_commit: String,
    pub(crate) state: PatchArtifactState,
    pub(crate) limit_bytes: u64,
    pub(crate) byte_count: Option<u64>,
    pub(crate) sha256: Option<String>,
}

impl PatchArtifact {
    pub(crate) fn collect(worktree: &TaskWorktree) -> Result<Self, ArtifactError> {
        worktree
            .git_output(
                &[
                    "add".into(),
                    "--intent-to-add".into(),
                    "--".into(),
                    ".".into(),
                ],
                4 * 1024,
            )
            .map_err(|_| ArtifactError::CollectionFailed)?;
        let baseline_commit = worktree.baseline_commit().to_string();
        let output = worktree.git_output(
            &[
                "diff".into(),
                "--binary".into(),
                "--full-index".into(),
                "--no-color".into(),
                "--no-ext-diff".into(),
                "--no-textconv".into(),
                "--no-renames".into(),
                "--src-prefix=a/".into(),
                "--dst-prefix=b/".into(),
                OsString::from(&baseline_commit),
                "--".into(),
            ],
            MAX_PATCH_BYTES,
        );
        let output = match output {
            Ok(output) => output,
            Err(GitCommandError::OutputTooLarge) => {
                return Ok(Self {
                    baseline_commit,
                    state: PatchArtifactState::TooLarge,
                    limit_bytes: MAX_PATCH_BYTES,
                    byte_count: None,
                    sha256: None,
                    patch: None,
                });
            }
            Err(
                GitCommandError::Unavailable | GitCommandError::Failed | GitCommandError::TimedOut,
            ) => return Err(ArtifactError::CollectionFailed),
        };
        let byte_count = output.len() as u64;
        let sha256 = format!("{:x}", Sha256::digest(&output));
        match String::from_utf8(output) {
            Ok(patch) => Ok(Self {
                baseline_commit,
                state: if patch.is_empty() {
                    PatchArtifactState::Empty
                } else {
                    PatchArtifactState::Complete
                },
                limit_bytes: MAX_PATCH_BYTES,
                byte_count: Some(byte_count),
                sha256: Some(sha256),
                patch: Some(patch),
            }),
            Err(_) => Ok(Self {
                baseline_commit,
                state: PatchArtifactState::UnsupportedEncoding,
                limit_bytes: MAX_PATCH_BYTES,
                byte_count: Some(byte_count),
                sha256: Some(sha256),
                patch: None,
            }),
        }
    }

    pub(super) fn is_valid(&self) -> bool {
        let metadata = PatchArtifactMetadata {
            baseline_commit: self.baseline_commit.clone(),
            state: self.state,
            limit_bytes: self.limit_bytes,
            byte_count: self.byte_count,
            sha256: self.sha256.clone(),
        };
        if !metadata.is_valid() {
            return false;
        }
        match self.state {
            PatchArtifactState::Empty => self.patch.as_deref() == Some(""),
            PatchArtifactState::Complete => self.patch.as_ref().is_some_and(|patch| {
                !patch.is_empty()
                    && self.byte_count == Some(patch.len() as u64)
                    && self.sha256.as_deref() == Some(&sha256(patch.as_bytes()))
            }),
            PatchArtifactState::TooLarge | PatchArtifactState::UnsupportedEncoding => {
                self.patch.is_none()
            }
        }
    }

    pub(super) fn metadata(&self) -> Option<PatchArtifactMetadata> {
        self.is_valid().then(|| PatchArtifactMetadata {
            baseline_commit: self.baseline_commit.clone(),
            state: self.state,
            limit_bytes: self.limit_bytes,
            byte_count: self.byte_count,
            sha256: self.sha256.clone(),
        })
    }
}

impl PatchArtifactMetadata {
    pub(super) fn is_valid(&self) -> bool {
        if self.limit_bytes != MAX_PATCH_BYTES || !valid_hex(&self.baseline_commit, &[40, 64]) {
            return false;
        }
        match self.state {
            PatchArtifactState::Empty => {
                self.byte_count == Some(0) && self.sha256.as_deref() == Some(&sha256([]))
            }
            PatchArtifactState::Complete | PatchArtifactState::UnsupportedEncoding => {
                self.byte_count
                    .is_some_and(|bytes| bytes > 0 && bytes <= self.limit_bytes)
                    && self
                        .sha256
                        .as_deref()
                        .is_some_and(|hash| valid_hex(hash, &[64]))
            }
            PatchArtifactState::TooLarge => self.byte_count.is_none() && self.sha256.is_none(),
        }
    }
}

fn sha256(bytes: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(bytes.as_ref()))
}

fn valid_hex(value: &str, lengths: &[usize]) -> bool {
    lengths.contains(&value.len()) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(crate) enum ArtifactError {
    #[error("Patch artifact could not be collected")]
    CollectionFailed,
}

#[cfg(test)]
#[path = "artifact_tests.rs"]
mod tests;
