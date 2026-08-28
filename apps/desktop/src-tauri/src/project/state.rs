use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use super::service::{ProjectSelection, ProjectService, ProjectServiceError, ResolvedProject};
use super::storage::RecentProjectStore;
use super::types::LocalProject;

pub(crate) struct ProjectState {
    service: Mutex<ProjectService>,
}

impl ProjectState {
    pub(crate) fn new(settings_file: PathBuf) -> Self {
        Self {
            service: Mutex::new(ProjectService::new(RecentProjectStore::new(settings_file))),
        }
    }

    pub(crate) fn list_recent(&self) -> Result<Vec<LocalProject>, ProjectServiceError> {
        self.service()?.list_recent()
    }

    pub(crate) fn select_project(
        &self,
        selected_path: Option<PathBuf>,
    ) -> Result<Option<ProjectSelection>, ProjectServiceError> {
        self.service()?.select_project(selected_path)
    }

    pub(crate) fn lookup_project(
        &self,
        project_id: &str,
    ) -> Result<ResolvedProject, ProjectServiceError> {
        self.service()?.lookup_project(project_id)
    }

    pub(crate) fn remove_recent(&self, project_id: &str) -> Result<(), ProjectServiceError> {
        self.service()?.remove_recent(project_id)
    }

    fn service(&self) -> Result<MutexGuard<'_, ProjectService>, ProjectServiceError> {
        self.service
            .lock()
            .map_err(|_| ProjectServiceError::Storage)
    }
}
