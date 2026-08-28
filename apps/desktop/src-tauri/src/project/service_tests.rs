use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::{fs, io};

use pretty_assertions::assert_eq;
use serde_json::json;
use tempfile::TempDir;

#[cfg(windows)]
use super::path_to_utf8;
use super::{
    DirectoryReadability, PersistenceWarning, ProjectClock, ProjectService, ProjectServiceError,
    project_id_for_path,
};
use crate::project::storage::{RecentProjectStore, StoredProject};
use crate::project::types::{LocalProject, ProjectAvailability};

#[test]
fn canceled_selection_does_not_change_recent_projects() {
    let temp_dir = tempfile::tempdir().unwrap();
    let service = service(&temp_dir, Arc::new(AlwaysReadable), [10]);

    assert_eq!(service.select_project(None).unwrap(), None);
    assert_eq!(service.list_recent().unwrap(), Vec::<LocalProject>::new());
}

#[test]
fn selection_returns_a_canonical_absolute_directory() {
    let temp_dir = tempfile::tempdir().unwrap();
    let project_dir = create_project_dir(&temp_dir, "workspace");
    let selected = project_dir.join("..").join("workspace");
    let service = service(&temp_dir, Arc::new(AlwaysReadable), [100]);

    let result = service.select_project(Some(selected)).unwrap().unwrap();

    let canonical = dunce::canonicalize(&project_dir).unwrap();
    assert_eq!(
        result.project,
        LocalProject {
            id: project_id_for_path(&canonical).unwrap(),
            path: canonical.to_str().unwrap().to_string(),
            name: "workspace".to_string(),
            last_opened_at: 100,
            availability: ProjectAvailability::Available,
        }
    );
    assert_eq!(result.warning, None);
    assert!(Path::new(&result.project.path).is_absolute());
}

#[test]
fn regular_files_are_rejected_as_projects() {
    let temp_dir = tempfile::tempdir().unwrap();
    let file = temp_dir.path().join("file.txt");
    fs::write(&file, "not a directory").unwrap();
    let service = service(&temp_dir, Arc::new(AlwaysReadable), [10]);

    assert_eq!(
        service.select_project(Some(file)),
        Err(ProjectServiceError::NotDirectory)
    );
}

#[cfg(unix)]
#[test]
fn symbolic_links_are_normalized_to_their_target() {
    use std::os::unix::fs::symlink;

    let temp_dir = tempfile::tempdir().unwrap();
    let target = create_project_dir(&temp_dir, "target");
    let link = temp_dir.path().join("link");
    symlink(&target, &link).unwrap();
    let service = service(&temp_dir, Arc::new(AlwaysReadable), [10]);

    let selected = service.select_project(Some(link)).unwrap().unwrap();

    assert_eq!(selected.project.path, target.to_str().unwrap());
}

#[cfg(windows)]
#[test]
fn windows_project_identity_is_case_insensitive() {
    let temp_dir = tempfile::tempdir().unwrap();
    let project_dir = create_project_dir(&temp_dir, "RivloomCase");
    let alternate_case = temp_dir.path().join("rivloomcase");
    let service = service(&temp_dir, Arc::new(AlwaysReadable), [100, 200]);

    let first = select_project(&service, project_dir);
    let second = select_project(&service, alternate_case);

    assert_eq!(first.id, second.id);
    assert_eq!(service.list_recent().unwrap().len(), 1);
}

#[cfg(windows)]
#[test]
fn windows_distinct_unicode_directories_do_not_share_an_identity() {
    let temp_dir = tempfile::tempdir().unwrap();
    let dotted_capital_i = create_project_dir(&temp_dir, "\u{130}");
    let lowercase_i_with_dot = create_project_dir(&temp_dir, "i\u{307}");
    let service = service(&temp_dir, Arc::new(AlwaysReadable), [100, 200]);

    let first = select_project(&service, dotted_capital_i);
    let second = select_project(&service, lowercase_i_with_dot);

    assert_ne!(first.id, second.id);
    assert_eq!(service.list_recent().unwrap().len(), 2);
}

#[cfg(not(windows))]
#[test]
fn unix_project_identity_is_case_sensitive() {
    assert_ne!(
        project_id_for_path(Path::new("/work/Rivloom")).unwrap(),
        project_id_for_path(Path::new("/work/rivloom")).unwrap()
    );
}

#[cfg(target_os = "macos")]
#[test]
fn macos_filesystem_equivalent_unicode_paths_share_an_identity() {
    let temp_dir = tempfile::tempdir().unwrap();
    let composed = create_project_dir(&temp_dir, "Caf\u{e9}");
    let decomposed = temp_dir.path().join("Cafe\u{301}");
    let service = service(&temp_dir, Arc::new(AlwaysReadable), [100, 200]);

    let first = select_project(&service, composed);
    let second = select_project(&service, decomposed);

    assert_eq!(first.id, second.id);
    assert_eq!(service.list_recent().unwrap().len(), 1);
}

#[cfg(unix)]
#[test]
fn non_utf8_directories_are_rejected_without_lossy_conversion() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let temp_dir = tempfile::tempdir().unwrap();
    let directory = temp_dir.path().join(OsString::from_vec(vec![b'p', 0xff]));
    fs::create_dir(&directory).unwrap();
    let service = service(&temp_dir, Arc::new(AlwaysReadable), [10]);

    assert_eq!(
        service.select_project(Some(directory)),
        Err(ProjectServiceError::NonUnicodePath)
    );
}

#[cfg(windows)]
#[test]
fn non_utf8_windows_paths_are_rejected_without_lossy_conversion() {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    let path = PathBuf::from(OsString::from_wide(&[0xd800]));

    assert_eq!(
        path_to_utf8(&path),
        Err(ProjectServiceError::NonUnicodePath)
    );
}

#[test]
fn reopening_updates_recency_without_duplicating_the_project() {
    let temp_dir = tempfile::tempdir().unwrap();
    let project_dir = create_project_dir(&temp_dir, "workspace");
    let service = service(&temp_dir, Arc::new(AlwaysReadable), [100, 200]);

    service.select_project(Some(project_dir.clone())).unwrap();
    service.select_project(Some(project_dir)).unwrap();

    let projects = service.list_recent().unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].last_opened_at, 200);
}

#[test]
fn removing_a_recent_project_is_idempotent() {
    let temp_dir = tempfile::tempdir().unwrap();
    let project_dir = create_project_dir(&temp_dir, "workspace");
    let service = service(&temp_dir, Arc::new(AlwaysReadable), [100]);
    let project = select_project(&service, project_dir);

    service.remove_recent(&project.id).unwrap();
    service.remove_recent(&project.id).unwrap();

    assert_eq!(service.list_recent().unwrap(), Vec::<LocalProject>::new());
}

#[test]
fn failed_removal_keeps_the_project_retryable_until_it_is_persisted() {
    let temp_dir = tempfile::tempdir().unwrap();
    let project_dir = create_project_dir(&temp_dir, "workspace");
    let service = service(&temp_dir, Arc::new(AlwaysReadable), [100]);
    let project = select_project(&service, project_dir);
    let valid_contents = fs::read(store_path(&temp_dir)).unwrap();
    fs::write(
        store_path(&temp_dir),
        serde_json::to_vec(&json!({"version": 2, "entries": []})).unwrap(),
    )
    .unwrap();

    assert_eq!(
        service.remove_recent(&project.id),
        Err(ProjectServiceError::Storage)
    );
    assert_eq!(service.list_recent().unwrap(), vec![project.clone()]);

    fs::write(store_path(&temp_dir), valid_contents).unwrap();
    service.remove_recent(&project.id).unwrap();
    let reloaded = ProjectService::with_dependencies(
        RecentProjectStore::new(store_path(&temp_dir)),
        Arc::new(AlwaysReadable),
        Arc::new(SequenceClock::new([])),
    );
    assert_eq!(reloaded.list_recent().unwrap(), Vec::<LocalProject>::new());
}

#[test]
fn unreadable_projects_fail_closed_before_downstream_work() {
    let temp_dir = tempfile::tempdir().unwrap();
    let project_dir = create_project_dir(&temp_dir, "workspace");
    let checks = Arc::new(AtomicUsize::new(0));
    let service = service(
        &temp_dir,
        Arc::new(AlwaysUnreadable {
            checks: Arc::clone(&checks),
        }),
        [100],
    );

    assert_eq!(
        service.select_project(Some(project_dir)),
        Err(ProjectServiceError::Unreadable)
    );
    assert_eq!(checks.load(Ordering::SeqCst), 1);
    assert_eq!(service.list_recent().unwrap(), Vec::<LocalProject>::new());
}

#[test]
fn saved_missing_directories_remain_visible_as_unavailable() {
    let temp_dir = tempfile::tempdir().unwrap();
    let missing = temp_dir.path().join("moved-workspace");
    let path = missing.to_str().unwrap().to_string();
    let stored = StoredProject {
        id: project_id_for_path(&missing).unwrap(),
        path: path.clone(),
        name: "moved-workspace".to_string(),
        last_opened_at: 100,
    };
    RecentProjectStore::new(store_path(&temp_dir))
        .save(std::slice::from_ref(&stored))
        .unwrap();
    let service = service(&temp_dir, Arc::new(AlwaysReadable), [200]);

    assert_eq!(
        service.list_recent().unwrap(),
        vec![LocalProject {
            id: stored.id,
            path,
            name: stored.name,
            last_opened_at: stored.last_opened_at,
            availability: ProjectAvailability::Missing,
        }]
    );
}

#[test]
fn lookup_revalidates_the_registered_directory() {
    let temp_dir = tempfile::tempdir().unwrap();
    let project_dir = create_project_dir(&temp_dir, "workspace");
    let service = service(&temp_dir, Arc::new(AlwaysReadable), [100]);
    let project = select_project(&service, project_dir.clone());
    fs::remove_dir(project_dir).unwrap();

    assert_eq!(
        service.lookup_project(&project.id),
        Err(ProjectServiceError::InvalidPath)
    );
}

#[test]
fn lookup_rejects_a_registered_id_with_a_tampered_path() {
    let temp_dir = tempfile::tempdir().unwrap();
    let first_dir = create_project_dir(&temp_dir, "first");
    let second_dir = create_project_dir(&temp_dir, "second");
    let first_id = project_id_for_path(&dunce::canonicalize(&first_dir).unwrap()).unwrap();
    let second_path = dunce::canonicalize(&second_dir).unwrap();
    RecentProjectStore::new(store_path(&temp_dir))
        .save(&[StoredProject {
            id: first_id.clone(),
            path: second_path.to_str().unwrap().to_string(),
            name: "second".to_string(),
            last_opened_at: 100,
        }])
        .unwrap();
    let service = service(&temp_dir, Arc::new(AlwaysReadable), []);

    assert_eq!(
        service.lookup_project(&first_id),
        Err(ProjectServiceError::Unavailable)
    );
}

#[test]
fn saved_unreadable_projects_are_visible_but_cannot_be_resolved() {
    let temp_dir = tempfile::tempdir().unwrap();
    let project_dir = create_project_dir(&temp_dir, "workspace");
    let canonical = dunce::canonicalize(&project_dir).unwrap();
    let stored = StoredProject {
        id: project_id_for_path(&canonical).unwrap(),
        path: canonical.to_str().unwrap().to_string(),
        name: "workspace".to_string(),
        last_opened_at: 100,
    };
    RecentProjectStore::new(store_path(&temp_dir))
        .save(std::slice::from_ref(&stored))
        .unwrap();
    let service = service(
        &temp_dir,
        Arc::new(AlwaysUnreadable {
            checks: Arc::new(AtomicUsize::new(0)),
        }),
        [],
    );
    let stored_id = stored.id.clone();

    assert_eq!(
        service.list_recent().unwrap(),
        vec![LocalProject {
            id: stored.id,
            path: stored.path,
            name: stored.name,
            last_opened_at: stored.last_opened_at,
            availability: ProjectAvailability::Unreadable,
        }]
    );
    assert_eq!(
        service.lookup_project(&stored_id),
        Err(ProjectServiceError::Unreadable)
    );
}

#[test]
fn unknown_project_ids_cannot_authorize_a_path() {
    let temp_dir = tempfile::tempdir().unwrap();
    let project_dir = create_project_dir(&temp_dir, "workspace");
    let service = service(&temp_dir, Arc::new(AlwaysReadable), [100]);
    service.select_project(Some(project_dir)).unwrap();

    assert_eq!(
        service.lookup_project("project-v1-not-registered"),
        Err(ProjectServiceError::NotFound)
    );
}

#[test]
fn save_failure_returns_the_project_with_a_nonfatal_warning() {
    let temp_dir = tempfile::tempdir().unwrap();
    let project_dir = create_project_dir(&temp_dir, "workspace");
    let parent_file = temp_dir.path().join("not-a-directory");
    fs::write(&parent_file, "content").unwrap();
    let service = ProjectService::with_dependencies(
        RecentProjectStore::new(parent_file.join("recent-projects-v1.json")),
        Arc::new(AlwaysReadable),
        Arc::new(SequenceClock::new([100])),
    );

    let selected = service.select_project(Some(project_dir)).unwrap().unwrap();

    assert_eq!(
        selected.project.availability,
        ProjectAvailability::Available
    );
    assert_eq!(
        selected.warning,
        Some(PersistenceWarning::RecentProjectsNotSaved)
    );
    assert_eq!(
        service
            .lookup_project(&selected.project.id)
            .unwrap()
            .project,
        selected.project
    );
}

fn service(
    temp_dir: &TempDir,
    readability: Arc<dyn DirectoryReadability>,
    times: impl IntoIterator<Item = i64>,
) -> ProjectService {
    ProjectService::with_dependencies(
        RecentProjectStore::new(store_path(temp_dir)),
        readability,
        Arc::new(SequenceClock::new(times)),
    )
}

fn store_path(temp_dir: &TempDir) -> PathBuf {
    temp_dir.path().join("settings/recent-projects-v1.json")
}

fn create_project_dir(temp_dir: &TempDir, name: &str) -> PathBuf {
    let path = temp_dir.path().join(name);
    fs::create_dir(&path).unwrap();
    path
}

fn select_project(service: &ProjectService, path: PathBuf) -> LocalProject {
    service.select_project(Some(path)).unwrap().unwrap().project
}

#[derive(Debug)]
struct AlwaysReadable;

impl DirectoryReadability for AlwaysReadable {
    fn check(&self, _path: &Path) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Debug)]
struct AlwaysUnreadable {
    checks: Arc<AtomicUsize>,
}

impl DirectoryReadability for AlwaysUnreadable {
    fn check(&self, _path: &Path) -> io::Result<()> {
        self.checks.fetch_add(1, Ordering::SeqCst);
        Err(io::Error::new(io::ErrorKind::PermissionDenied, "denied"))
    }
}

#[derive(Debug)]
struct SequenceClock {
    times: Mutex<VecDeque<i64>>,
}

impl SequenceClock {
    fn new(times: impl IntoIterator<Item = i64>) -> Self {
        Self {
            times: Mutex::new(times.into_iter().collect()),
        }
    }
}

impl ProjectClock for SequenceClock {
    fn now_unix_seconds(&self) -> i64 {
        self.times.lock().unwrap().pop_front().unwrap()
    }
}
