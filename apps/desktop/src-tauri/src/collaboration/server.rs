use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::host::Host;
use super::tls::{ServerTls, TlsChannel, private_address};
use super::wire::{Request, WireError};

/// Explicitly started private-interface listener. Stop/drop shuts sockets and joins every worker.
pub(super) struct Server {
    pub(super) address: SocketAddr,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Server {
    pub(super) fn start(
        host: Arc<Host>,
        tls: ServerTls,
        address: SocketAddr,
    ) -> Result<Self, WireError> {
        if !private_address(address.ip()) {
            return Err(WireError::Invalid);
        }
        let listener = TcpListener::bind(address).map_err(|_| WireError::Unavailable)?;
        listener
            .set_nonblocking(true)
            .map_err(|_| WireError::Unavailable)?;
        let address = listener.local_addr().map_err(|_| WireError::Unavailable)?;
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let tls = Arc::new(tls);
        let worker = thread::Builder::new()
            .name("rivloom-brain-listener".into())
            .spawn(move || {
                let mut workers: Vec<(TcpStream, JoinHandle<()>)> = Vec::new();
                while !stopping.load(Ordering::SeqCst) {
                    let mut index = 0;
                    while index < workers.len() {
                        if workers[index].1.is_finished() {
                            let (_, worker) = workers.swap_remove(index);
                            let _ = worker.join();
                        } else {
                            index += 1;
                        }
                    }
                    let socket = match listener.accept() {
                        Ok((socket, _)) => socket,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(20));
                            continue;
                        }
                        Err(_) => break,
                    };
                    if workers.len() >= 16 {
                        continue;
                    }
                    let Ok(mut session) = host.session() else {
                        continue;
                    };
                    let Ok(control) = socket.try_clone() else {
                        continue;
                    };
                    let tls = tls.clone();
                    let stopping = stopping.clone();
                    if let Ok(worker) = thread::Builder::new()
                        .name("rivloom-brain-peer".into())
                        .spawn(move || {
                            let Ok(mut channel) = TlsChannel::accept(socket, &tls) else {
                                return;
                            };
                            while !stopping.load(Ordering::SeqCst) {
                                let Ok(bytes) = channel.receive() else {
                                    break;
                                };
                                let Ok(request) = Request::decode(&bytes) else {
                                    break;
                                };
                                drop(bytes);
                                let Ok(time) = now() else {
                                    break;
                                };
                                let response = session.handle(request, time);
                                let Ok(bytes) = response.encode() else {
                                    break;
                                };
                                if channel.send(&bytes).is_err() || session.closed() {
                                    break;
                                }
                            }
                            channel.close();
                        })
                    {
                        workers.push((control, worker));
                    }
                }
                // Wake blocked handshake/frame readers before joining. No detached worker outlives this loop.
                for (socket, _) in &workers {
                    let _ = socket.shutdown(Shutdown::Both);
                }
                for (_, worker) in workers {
                    let _ = worker.join();
                }
            })
            .map_err(|_| WireError::Unavailable)?;
        Ok(Self {
            address,
            stop,
            worker: Some(worker),
        })
    }

    pub(super) fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop();
    }
}

pub(super) fn now() -> Result<i64, WireError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|time| i64::try_from(time.as_secs()).ok())
        .ok_or(WireError::Unavailable)
}

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
