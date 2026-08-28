use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, io};

use serde::Serialize;
use sha2::{Digest, Sha256};
use thiserror::Error;

use super::storage::{RecentProjectStore, StorageError, StoredProject, normalize_projects};
use super::types::{LocalProject, ProjectAvailability};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectSelection {
    pub project: LocalProject,
    pub warning: Option<PersistenceWarning>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum PersistenceWarning {
    RecentProjectsNotSaved,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResolvedProject {
    project: LocalProject,
    path: PathBuf,
}

impl ResolvedProject {
    pub(crate) fn project(&self) -> &LocalProject {
        &self.project
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn cwd(&self) -> &str {
        &self.project.path
    }
}

pub(crate) struct ProjectService {
    store: RecentProjectStore,
    readability: Arc<dyn DirectoryReadability>,
    clock: Arc<dyn ProjectClock>,
    projects: Mutex<Option<Vec<StoredProject>>>,
}

impl ProjectService {
    pub(crate) fn new(store: RecentProjectStore) -> Self {
        Self::with_dependencies(store, Arc::new(ReadDirectoryOnce), Arc::new(SystemClock))
    }

    fn with_dependencies(
        store: RecentProjectStore,
        readability: Arc<dyn DirectoryReadability>,
        clock: Arc<dyn ProjectClock>,
    ) -> Self {
        Self {
            store,
            readability,
            clock,
            projects: Mutex::new(None),
        }
    }

    pub(crate) fn list_recent(&self) -> Result<Vec<LocalProject>, ProjectServiceError> {
        self.cached_projects().map(|projects| {
            projects
                .into_iter()
                .map(|project| self.local_project(project))
                .collect()
        })
    }

    pub(crate) fn select_project(
        &self,
        selected_path: Option<PathBuf>,
    ) -> Result<Option<ProjectSelection>, ProjectServiceError> {
        let Some(selected_path) = selected_path else {
            return Ok(None);
        };
        let (path, path_string) = self.validate_selected_path(&selected_path)?;
        let id = project_id_for_path(&path)?;
        let project = StoredProject {
            id: id.clone(),
            path: path_string,
            name: display_name(&path)?,
            last_opened_at: self.clock.now_unix_seconds(),
        };
        let mut projects = self.cached_projects()?;
        projects.retain(|candidate| candidate.id != id);
        projects.push(project.clone());
        let projects = normalize_projects(projects);
        self.cache_projects(projects.clone())?;
        let warning = self
            .store
            .save(&projects)
            .err()
            .map(|_| PersistenceWarning::RecentProjectsNotSaved);
        Ok(Some(ProjectSelection {
            project: local_project(project, ProjectAvailability::Available),
            warning,
        }))
    }

    pub(crate) fn lookup_project(
        &self,
        project_id: &str,
    ) -> Result<ResolvedProject, ProjectServiceError> {
        let stored = self
            .cached_projects()?
            .into_iter()
            .find(|project| project.id == project_id)
            .ok_or(ProjectServiceError::NotFound)?;
        let registered_path = PathBuf::from(&stored.path);
        let (path, _) = self.validate_selected_path(&registered_path)?;
        if project_id_for_path(&path)? != stored.id {
            return Err(ProjectServiceError::Unavailable);
        }
        Ok(ResolvedProject {
            project: local_project(stored, ProjectAvailability::Available),
            path,
        })
    }

    pub(crate) fn remove_recent(&self, project_id: &str) -> Result<(), ProjectServiceError> {
        let mut projects = self.cached_projects()?;
        let previous_len = projects.len();
        projects.retain(|project| project.id != project_id);
        if projects.len() != previous_len {
            self.store
                .save(&projects)
                .map_err(ProjectServiceError::from)?;
            self.cache_projects(projects)?;
        }
        Ok(())
    }

    fn validate_selected_path(
        &self,
        selected_path: &Path,
    ) -> Result<(PathBuf, String), ProjectServiceError> {
        let path =
            dunce::canonicalize(selected_path).map_err(|_| ProjectServiceError::InvalidPath)?;
        let metadata = fs::metadata(&path).map_err(|_| ProjectServiceError::InvalidPath)?;
        if !metadata.is_dir() {
            return Err(ProjectServiceError::NotDirectory);
        }
        self.readability
            .check(&path)
            .map_err(|_| ProjectServiceError::Unreadable)?;
        let path_string = path_to_utf8(&path)?;
        Ok((path, path_string))
    }

    fn local_project(&self, project: StoredProject) -> LocalProject {
        let path = Path::new(&project.path);
        let availability = match fs::metadata(path) {
            Ok(metadata) if metadata.is_dir() => self
                .readability
                .check(path)
                .map_or(ProjectAvailability::Unreadable, |()| {
                    ProjectAvailability::Available
                }),
            Ok(_) => ProjectAvailability::Unreadable,
            Err(error) if error.kind() == io::ErrorKind::NotFound => ProjectAvailability::Missing,
            Err(_) => ProjectAvailability::Unreadable,
        };
        local_project(project, availability)
    }

    fn cached_projects(&self) -> Result<Vec<StoredProject>, ProjectServiceError> {
        let mut projects = self
            .projects
            .lock()
            .map_err(|_| ProjectServiceError::Storage)?;
        if projects.is_none() {
            *projects = Some(self.store.load().map_err(ProjectServiceError::from)?);
        }
        Ok(projects.as_ref().cloned().unwrap_or_default())
    }

    fn cache_projects(&self, projects: Vec<StoredProject>) -> Result<(), ProjectServiceError> {
        *self
            .projects
            .lock()
            .map_err(|_| ProjectServiceError::Storage)? = Some(projects);
        Ok(())
    }
}

fn local_project(project: StoredProject, availability: ProjectAvailability) -> LocalProject {
    LocalProject {
        id: project.id,
        path: project.path,
        name: project.name,
        last_opened_at: project.last_opened_at,
        availability,
    }
}

fn path_to_utf8(path: &Path) -> Result<String, ProjectServiceError> {
    path.to_str()
        .map(str::to_string)
        .ok_or(ProjectServiceError::NonUnicodePath)
}

fn project_id_for_path(path: &Path) -> Result<String, ProjectServiceError> {
    let path = path_to_utf8(path)?;
    let digest = Sha256::digest(path.as_bytes());
    Ok(format!("project-v1-{digest:x}"))
}

fn display_name(path: &Path) -> Result<String, ProjectServiceError> {
    if let Some(name) = path.file_name() {
        return name
            .to_str()
            .map(str::to_string)
            .ok_or(ProjectServiceError::NonUnicodePath);
    }
    path_to_utf8(path)
}

/// Checks that a validated directory can be opened without enumerating or reading project files.
trait DirectoryReadability: Send + Sync {
    fn check(&self, path: &Path) -> io::Result<()>;
}

#[derive(Debug)]
struct ReadDirectoryOnce;

impl DirectoryReadability for ReadDirectoryOnce {
    fn check(&self, path: &Path) -> io::Result<()> {
        fs::read_dir(path).map(drop)
    }
}

/// Supplies Unix timestamps for recent-project ordering.
trait ProjectClock: Send + Sync {
    fn now_unix_seconds(&self) -> i64;
}

#[derive(Debug)]
struct SystemClock;

impl ProjectClock for SystemClock {
    fn now_unix_seconds(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .try_into()
            .unwrap_or(i64::MAX)
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(crate) enum ProjectServiceError {
    #[error("the selected project path is unavailable")]
    InvalidPath,
    #[error("the selected project path is not a directory")]
    NotDirectory,
    #[error("the selected project path is not readable")]
    Unreadable,
    #[error("the selected project path cannot be represented safely")]
    NonUnicodePath,
    #[error("the requested project is not registered")]
    NotFound,
    #[error("the requested project is unavailable")]
    Unavailable,
    #[error("recent projects are unavailable")]
    Storage,
}

impl From<StorageError> for ProjectServiceError {
    fn from(_error: StorageError) -> Self {
        Self::Storage
    }
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;
