use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;
use std::time::Instant;

use tauri::AppHandle;
use tauri_plugin_shell::ShellExt;
use tauri_plugin_shell::process::CommandChild;
use tauri_plugin_shell::process::CommandEvent;

use crate::app_server::wire::JsonLineDecoder;
use crate::app_server::wire::MAX_JSON_LINE_BYTES;
use crate::app_server::wire::WireError;

const MAX_DIAGNOSTIC_CHARS: usize = 512;
const SIDECAR_NAME: &str = "codex-app-server";

pub(super) trait ProcessLauncher: Send {
    fn launch(&mut self) -> Result<ProcessTransport, String>;
}

pub(super) trait ProcessControl: Send + Sync {
    fn write(&self, message: &str) -> Result<(), String>;
    fn terminate(&self) -> Result<(), String>;
}

pub(super) enum TransportEvent {
    Stdout(Vec<u8>),
    Error(String),
    Terminated(Option<i32>),
}

#[derive(Debug)]
pub(super) enum TransportReadError {
    Timeout,
    InvalidMessage(WireError),
    Transport(String),
    Terminated(Option<i32>),
    Closed,
}

pub(super) struct ProcessTransport {
    control: Arc<dyn ProcessControl>,
    events: Receiver<TransportEvent>,
    decoder: JsonLineDecoder,
    ready_lines: VecDeque<String>,
}

impl ProcessTransport {
    pub(super) fn new(control: Arc<dyn ProcessControl>, events: Receiver<TransportEvent>) -> Self {
        Self {
            control,
            events,
            decoder: JsonLineDecoder::new(MAX_JSON_LINE_BYTES),
            ready_lines: VecDeque::new(),
        }
    }

    pub(super) fn control(&self) -> Arc<dyn ProcessControl> {
        self.control.clone()
    }

    pub(super) fn receive_line(&mut self, timeout: Duration) -> Result<String, TransportReadError> {
        if let Some(line) = self.ready_lines.pop_front() {
            return Ok(line);
        }

        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(TransportReadError::Timeout);
            }

            match self.events.recv_timeout(remaining) {
                Ok(TransportEvent::Stdout(bytes)) => {
                    let lines = self
                        .decoder
                        .push(&bytes)
                        .map_err(TransportReadError::InvalidMessage)?;
                    self.ready_lines.extend(lines);
                    if let Some(line) = self.ready_lines.pop_front() {
                        return Ok(line);
                    }
                }
                Ok(TransportEvent::Error(message)) => {
                    return Err(TransportReadError::Transport(message));
                }
                Ok(TransportEvent::Terminated(code)) => {
                    return Err(TransportReadError::Terminated(code));
                }
                Err(RecvTimeoutError::Timeout) => return Err(TransportReadError::Timeout),
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(TransportReadError::Closed);
                }
            }
        }
    }
}

pub(super) struct TauriProcessLauncher {
    app_handle: AppHandle,
    codex_home: PathBuf,
}

impl TauriProcessLauncher {
    pub(super) fn new(app_handle: AppHandle, codex_home: PathBuf) -> Self {
        Self {
            app_handle,
            codex_home,
        }
    }
}

impl ProcessLauncher for TauriProcessLauncher {
    fn launch(&mut self) -> Result<ProcessTransport, String> {
        std::fs::create_dir_all(&self.codex_home)
            .map_err(|error| format!("failed to create Codex home: {error}"))?;

        let command = self
            .app_handle
            .shell()
            .sidecar(SIDECAR_NAME)
            .map_err(|error| format!("failed to resolve bundled sidecar: {error}"))?
            .env("CODEX_HOME", &self.codex_home);
        let (mut events, child) = command
            .spawn()
            .map_err(|error| format!("failed to spawn bundled sidecar: {error}"))?;
        let (event_sender, event_receiver) = mpsc::channel();

        let _event_forwarder = tauri::async_runtime::spawn(async move {
            while let Some(event) = events.recv().await {
                let transport_event = match event {
                    CommandEvent::Stdout(bytes) => TransportEvent::Stdout(bytes),
                    CommandEvent::Stderr(bytes) => {
                        log_diagnostic("stderr", &String::from_utf8_lossy(&bytes));
                        continue;
                    }
                    CommandEvent::Error(message) => TransportEvent::Error(message),
                    CommandEvent::Terminated(payload) => TransportEvent::Terminated(payload.code),
                    _ => continue,
                };

                if event_sender.send(transport_event).is_err() {
                    break;
                }
            }
        });

        let control = Arc::new(TauriProcessControl {
            child: Mutex::new(Some(child)),
        });
        Ok(ProcessTransport::new(control, event_receiver))
    }
}

struct TauriProcessControl {
    child: Mutex<Option<CommandChild>>,
}

impl ProcessControl for TauriProcessControl {
    fn write(&self, message: &str) -> Result<(), String> {
        self.child
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_mut()
            .ok_or_else(|| "App Server process is no longer running".to_string())?
            .write(message.as_bytes())
            .map_err(|error| format!("failed to write to App Server stdin: {error}"))
    }

    fn terminate(&self) -> Result<(), String> {
        let child = self
            .child
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(child) = child {
            child
                .kill()
                .map_err(|error| format!("failed to terminate App Server: {error}"))?;
        }
        Ok(())
    }
}

pub(crate) fn log_diagnostic(context: &str, detail: &str) {
    let detail = detail
        .chars()
        .take(MAX_DIAGNOSTIC_CHARS)
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    eprintln!("Rivloom App Server {context}: {detail}");
}

#[cfg(test)]
#[path = "transport_tests.rs"]
mod tests;
