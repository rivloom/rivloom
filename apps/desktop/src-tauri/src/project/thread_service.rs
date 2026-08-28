use std::sync::Arc;

use thiserror::Error;

use super::protocol::{
    MAX_CURSOR_BYTES, MAX_PAGE_THREADS, MAX_THREAD_ID_BYTES, ThreadProtocolError, list_params,
    parse_list_response, parse_read_response, parse_start_response, read_params, start_params,
};
use super::service::ResolvedProject;
use super::types::{ProjectThread, ProjectThreadPage};
use crate::app_server::{ConnectionControl, ConnectionError};

pub(crate) const MAX_ACCUMULATED_THREADS: usize = 500;

pub(crate) struct ThreadService;

impl ThreadService {
    pub(crate) fn list_threads(
        project: &ResolvedProject,
        connection: Arc<dyn ConnectionControl>,
        cursor: Option<&str>,
        accumulated: usize,
    ) -> Result<ProjectThreadPage, ThreadServiceError> {
        if accumulated > MAX_ACCUMULATED_THREADS
            || cursor.is_some_and(|cursor| cursor.len() > MAX_CURSOR_BYTES)
        {
            return Err(ThreadServiceError::InvalidRequest);
        }
        if accumulated == MAX_ACCUMULATED_THREADS {
            return Ok(ProjectThreadPage {
                data: Vec::new(),
                next_cursor: None,
            });
        }
        let limit = MAX_PAGE_THREADS.min(MAX_ACCUMULATED_THREADS - accumulated);
        let response = connection
            .request("thread/list", list_params(project.cwd(), cursor, limit))
            .map_err(map_connection_error)?;
        let mut page = parse_list_response(response, project.cwd(), limit)?;
        if accumulated + page.data.len() == MAX_ACCUMULATED_THREADS {
            page.next_cursor = None;
        }
        Ok(page)
    }

    pub(crate) fn start_thread(
        project: &ResolvedProject,
        connection: Arc<dyn ConnectionControl>,
    ) -> Result<ProjectThread, ThreadServiceError> {
        let response = connection
            .request("thread/start", start_params(project.cwd()))
            .map_err(map_connection_error)?;
        parse_start_response(response, project.cwd()).map_err(Into::into)
    }

    pub(crate) fn read_thread(
        project: &ResolvedProject,
        connection: Arc<dyn ConnectionControl>,
        thread_id: &str,
    ) -> Result<ProjectThread, ThreadServiceError> {
        if thread_id.is_empty() || thread_id.len() > MAX_THREAD_ID_BYTES {
            return Err(ThreadServiceError::InvalidRequest);
        }
        let response = connection
            .request("thread/read", read_params(thread_id))
            .map_err(map_connection_error)?;
        parse_read_response(response, project.cwd()).map_err(Into::into)
    }
}

fn map_connection_error(error: ConnectionError) -> ThreadServiceError {
    match error {
        ConnectionError::Disconnected => ThreadServiceError::Disconnected,
        ConnectionError::Serialize
        | ConnectionError::WriteFailed
        | ConnectionError::Timeout
        | ConnectionError::TooManyPending
        | ConnectionError::Remote { .. }
        | ConnectionError::RequestIdExhausted => ThreadServiceError::RequestFailed,
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(crate) enum ThreadServiceError {
    #[error("the App Server connection is unavailable")]
    Disconnected,
    #[error("the App Server request failed")]
    RequestFailed,
    #[error("the thread request is invalid")]
    InvalidRequest,
    #[error("the App Server returned an invalid thread response")]
    InvalidResponse,
    #[error("the thread does not belong to the selected project")]
    ProjectMismatch,
}

impl From<ThreadProtocolError> for ThreadServiceError {
    fn from(error: ThreadProtocolError) -> Self {
        match error {
            ThreadProtocolError::InvalidResponse => Self::InvalidResponse,
            ThreadProtocolError::CwdMismatch => Self::ProjectMismatch,
        }
    }
}

#[cfg(test)]
#[path = "thread_service_tests.rs"]
mod tests;
