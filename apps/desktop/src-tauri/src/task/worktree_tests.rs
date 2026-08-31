use std::fs;
use std::path::Path;
use std::process::Command;

use pretty_assertions::assert_eq;

use super::*;
use crate::project::ProjectState;

#[test]
fn creates_a_detached_worktree_without_touching_the_users_checkout() {
    let fixture = RepositoryFixture::new();
    fs::write(&fixture.file, "user change\n").unwrap();
    let manager = TaskWorktreeManager::new(fixture.managed_root.clone());

    let worktree = manager.create(&fixture.project, "run/../1").unwrap();

    assert_eq!(worktree.baseline_commit(), fixture.baseline);
    assert_eq!(
        git_output(Path::new(worktree.cwd()), &["rev-parse", "HEAD"]),
        fixture.baseline
    );
    assert_eq!(
        fs::read_to_string(Path::new(worktree.cwd()).join("tracked.txt")).unwrap(),
        "baseline\n"
    );
    assert_eq!(fs::read_to_string(&fixture.file).unwrap(), "user change\n");
    assert_eq!(
        dunce::canonicalize(Path::new(worktree.cwd()))
            .unwrap()
            .parent(),
        Some(
            dunce::canonicalize(&fixture.managed_root)
                .unwrap()
                .as_path()
        )
    );
    assert!(!worktree.cwd().contains("run/../1"));
    assert_eq!(worktree.cleanup(), WorktreeCleanup::Removed);
    assert!(!Path::new(worktree.cwd()).exists());
    assert_eq!(fs::read_to_string(&fixture.file).unwrap(), "user change\n");
}

#[test]
fn non_git_projects_and_existing_destinations_are_rejected() {
    let temp_dir = tempfile::tempdir().unwrap();
    let project_path = temp_dir.path().join("plain-project");
    fs::create_dir(&project_path).unwrap();
    let state = ProjectState::new(temp_dir.path().join("recent-projects-v1.json"));
    let selection = state.select_project(Some(project_path)).unwrap().unwrap();
    let project = state.lookup_project(&selection.project.id).unwrap();
    let manager = TaskWorktreeManager::new(temp_dir.path().join("managed"));

    assert_eq!(
        manager.create(&project, "run-1").err(),
        Some(WorktreeError::NotRepository)
    );

    let fixture = RepositoryFixture::new();
    let manager = TaskWorktreeManager::new(fixture.managed_root.clone());
    let worktree = manager.create(&fixture.project, "run-1").unwrap();
    assert_eq!(
        manager.create(&fixture.project, "run-1").err(),
        Some(WorktreeError::DestinationExists)
    );
    assert_eq!(worktree.cleanup(), WorktreeCleanup::Removed);
}

#[test]
fn cleanup_failures_retain_the_worktree_for_local_diagnosis() {
    let fixture = RepositoryFixture::new();
    let manager = TaskWorktreeManager::new(fixture.managed_root.clone());
    let mut worktree = manager.create(&fixture.project, "run-1").unwrap();
    worktree.git_program = fixture.temp_dir.path().join("missing-git");

    assert_eq!(
        worktree.cleanup(),
        WorktreeCleanup::Retained {
            reason: WorktreeCleanupFailure::GitUnavailable,
        }
    );
    assert!(Path::new(worktree.cwd()).exists());
}

#[test]
fn a_managed_root_inside_the_repository_is_rejected_before_creation() {
    let fixture = RepositoryFixture::new();
    let unsafe_root = fixture.repository.join("must-not-be-created");
    let manager = TaskWorktreeManager::new(unsafe_root.clone());

    assert_eq!(
        manager.create(&fixture.project, "run-1").err(),
        Some(WorktreeError::InvalidRequest)
    );
    assert!(!unsafe_root.exists());
}

#[test]
fn cleanup_refuses_every_path_outside_the_exact_managed_root() {
    let fixture = RepositoryFixture::new();
    let manager = TaskWorktreeManager::new(fixture.managed_root.clone());
    let mut worktree = manager.create(&fixture.project, "run-1").unwrap();
    worktree.path = fixture.repository.clone();
    worktree.cwd = fixture.repository.to_string_lossy().into_owned();

    assert_eq!(
        worktree.cleanup(),
        WorktreeCleanup::Retained {
            reason: WorktreeCleanupFailure::InvalidTarget,
        }
    );
    assert!(fixture.repository.exists());
    assert!(fixture.file.exists());
}

struct RepositoryFixture {
    temp_dir: tempfile::TempDir,
    repository: PathBuf,
    file: PathBuf,
    managed_root: PathBuf,
    project: ResolvedProject,
    baseline: String,
}

impl RepositoryFixture {
    fn new() -> Self {
        let temp_dir = tempfile::tempdir().unwrap();
        let repository = temp_dir.path().join("repository");
        let managed_root = temp_dir.path().join("managed-worktrees");
        fs::create_dir(&repository).unwrap();
        git(&repository, &["init"]);
        git(
            &repository,
            &["config", "user.email", "tests@rivloom.local"],
        );
        git(&repository, &["config", "user.name", "Rivloom Tests"]);
        git(&repository, &["config", "core.autocrlf", "false"]);
        let file = repository.join("tracked.txt");
        fs::write(&file, "baseline\n").unwrap();
        git(&repository, &["add", "tracked.txt"]);
        git(&repository, &["commit", "-m", "baseline"]);
        let baseline = git_output(&repository, &["rev-parse", "HEAD"]);
        let state = ProjectState::new(temp_dir.path().join("recent-projects-v1.json"));
        let selection = state
            .select_project(Some(repository.clone()))
            .unwrap()
            .unwrap();
        let project = state.lookup_project(&selection.project.id).unwrap();
        Self {
            temp_dir,
            repository,
            file,
            managed_root,
            project,
            baseline,
        }
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

fn git_output(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}
