use std::fs;
use std::path::Path;
use std::process::Command;

use pretty_assertions::assert_eq;
use sha2::Digest;
use sha2::Sha256;

use super::*;
use crate::project::ProjectState;
use crate::task::worktree::TaskWorktreeManager;

#[test]
fn complete_patch_includes_tracked_and_new_files_with_hash_and_baseline() {
    let fixture = ArtifactFixture::new();
    fs::write(fixture.worktree_path().join("tracked.txt"), "changed\n").unwrap();
    fs::write(fixture.worktree_path().join("new.txt"), "new\n").unwrap();

    let artifact = PatchArtifact::collect(&fixture.worktree).unwrap();

    let patch = artifact.patch.as_ref().unwrap();
    assert_eq!(artifact.state, PatchArtifactState::Complete);
    assert_eq!(artifact.baseline_commit, fixture.worktree.baseline_commit());
    assert_eq!(artifact.limit_bytes, MAX_PATCH_BYTES);
    assert_eq!(artifact.byte_count, Some(patch.len() as u64));
    assert_eq!(
        artifact.sha256,
        Some(format!("{:x}", Sha256::digest(patch.as_bytes())))
    );
    assert!(patch.contains("a/tracked.txt"));
    assert!(patch.contains("b/new.txt"));
    let serialized = serde_json::to_string(&artifact).unwrap();
    assert!(!serialized.contains(fixture.worktree.cwd()));
    assert!(!serialized.contains(&fixture.temp_dir.path().to_string_lossy().into_owned()));
}

#[test]
fn unchanged_worktree_has_a_verifiable_empty_patch() {
    let fixture = ArtifactFixture::new();

    let artifact = PatchArtifact::collect(&fixture.worktree).unwrap();

    assert_eq!(artifact.state, PatchArtifactState::Empty);
    assert_eq!(artifact.byte_count, Some(0));
    assert_eq!(artifact.sha256, Some(format!("{:x}", Sha256::digest([]))));
    assert_eq!(artifact.patch, Some(String::new()));
}

#[test]
fn oversized_patch_is_explicit_and_never_partially_returned() {
    let fixture = ArtifactFixture::new();
    fs::write(
        fixture.worktree_path().join("large.txt"),
        "x".repeat(MAX_PATCH_BYTES as usize + 1024),
    )
    .unwrap();

    let artifact = PatchArtifact::collect(&fixture.worktree).unwrap();

    assert_eq!(artifact.state, PatchArtifactState::TooLarge);
    assert_eq!(artifact.byte_count, None);
    assert_eq!(artifact.sha256, None);
    assert_eq!(artifact.patch, None);
}

#[test]
fn invalid_utf8_keeps_exact_metadata_but_not_unrenderable_content() {
    let fixture = ArtifactFixture::new();
    fs::write(fixture.worktree_path().join("bytes.txt"), [0x80, b'\n']).unwrap();

    let artifact = PatchArtifact::collect(&fixture.worktree).unwrap();

    assert_eq!(artifact.state, PatchArtifactState::UnsupportedEncoding);
    assert!(artifact.byte_count.is_some_and(|bytes| bytes > 0));
    assert!(artifact.sha256.is_some());
    assert_eq!(artifact.patch, None);
}

struct ArtifactFixture {
    temp_dir: tempfile::TempDir,
    worktree: crate::task::worktree::TaskWorktree,
}

impl ArtifactFixture {
    fn new() -> Self {
        let temp_dir = tempfile::tempdir().unwrap();
        let repository = temp_dir.path().join("repository");
        fs::create_dir(&repository).unwrap();
        git(&repository, &["init"]);
        git(
            &repository,
            &["config", "user.email", "tests@rivloom.local"],
        );
        git(&repository, &["config", "user.name", "Rivloom Tests"]);
        git(&repository, &["config", "core.autocrlf", "false"]);
        git(
            &repository,
            &[
                "config",
                "diff.external",
                "rivloom-external-diff-must-not-run",
            ],
        );
        git(
            &repository,
            &[
                "config",
                "diff.rivloom-test.textconv",
                "rivloom-textconv-must-not-run",
            ],
        );
        fs::write(
            repository.join(".gitattributes"),
            "*.txt diff=rivloom-test\n",
        )
        .unwrap();
        fs::write(repository.join("tracked.txt"), "baseline\n").unwrap();
        git(&repository, &["add", ".gitattributes", "tracked.txt"]);
        git(&repository, &["commit", "-m", "baseline"]);
        let state = ProjectState::new(temp_dir.path().join("recent-projects-v1.json"));
        let selection = state.select_project(Some(repository)).unwrap().unwrap();
        let project = state.lookup_project(&selection.project.id).unwrap();
        let worktree = TaskWorktreeManager::new(temp_dir.path().join("managed-worktrees"))
            .create(&project, "artifact-run")
            .unwrap();
        Self { temp_dir, worktree }
    }

    fn worktree_path(&self) -> &Path {
        Path::new(self.worktree.cwd())
    }
}

fn git(cwd: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .status()
        .unwrap();
    assert!(status.success());
}
