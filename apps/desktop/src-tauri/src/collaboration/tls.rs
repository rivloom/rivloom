use std::io::{self, Read, Write};
use std::net::{IpAddr, Shutdown, SocketAddr, TcpStream};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use rustls::{
    ClientConfig, ClientConnection, Connection, RootCertStore, ServerConfig, ServerConnection,
};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use super::reconcile::MAX_CONTROL_BYTES;

const ALPN: &[u8] = b"rivloom/1";
const IO_TIMEOUT: Duration = Duration::from_secs(5);

/// Out-of-band trusted root, DNS/IP identity and exact leaf-certificate SHA-256; never TOFU.
pub(super) struct Peer {
    address: SocketAddr,
    name: ServerName<'static>,
    config: Arc<ClientConfig>,
    pin: [u8; 32],
}

impl Peer {
    pub(super) fn new(
        address: SocketAddr,
        name: String,
        root: Vec<u8>,
        pin: [u8; 32],
    ) -> Result<Self, TlsError> {
        if !private_address(address.ip()) || address.port() == 0 || root.len() > 16384 {
            return Err(TlsError::Configuration);
        }
        let name = ServerName::try_from(name).map_err(|_| TlsError::Configuration)?;
        let mut roots = RootCertStore::empty();
        roots
            .add(CertificateDer::from(root))
            .map_err(|_| TlsError::Configuration)?;
        let mut config =
            ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_protocol_versions(&[&rustls::version::TLS13])
                .map_err(|_| TlsError::Configuration)?
                .with_root_certificates(roots)
                .with_no_client_auth();
        config.alpn_protocols = vec![ALPN.to_vec()];
        config.resumption = rustls::client::Resumption::disabled();
        config.enable_early_data = false;
        Ok(Self {
            address,
            name,
            config: Arc::new(config),
            pin,
        })
    }
}

pub(super) struct ServerTls {
    config: Arc<ServerConfig>,
}

impl ServerTls {
    pub(super) fn new(
        certificates: Vec<CertificateDer<'static>>,
        key: PrivateKeyDer<'static>,
    ) -> Result<Self, TlsError> {
        if certificates.is_empty()
            || certificates.len() > 4
            || certificates.iter().any(|cert| cert.len() > 16384)
            || key.secret_der().len() > 16384
        {
            return Err(TlsError::Configuration);
        }
        let mut config =
            ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_protocol_versions(&[&rustls::version::TLS13])
                .map_err(|_| TlsError::Configuration)?
                .with_no_client_auth()
                .with_single_cert(certificates, key)
                .map_err(|_| TlsError::Configuration)?;
        config.alpn_protocols = vec![ALPN.to_vec()];
        config.send_tls13_tickets = 0;
        config.session_storage = Arc::new(rustls::server::NoServerSessionStorage {});
        Ok(Self {
            config: Arc::new(config),
        })
    }
}

/// The only application-byte transport; construction completes TLS and client pin validation first.
pub(super) struct TlsChannel {
    connection: Connection,
    io: DeadlineTcp,
    healthy: bool,
}

impl TlsChannel {
    pub(super) fn connect(peer: &Peer) -> Result<Self, TlsError> {
        let connection = ClientConnection::new(peer.config.clone(), peer.name.clone())
            .map_err(|_| TlsError::Configuration)?;
        let socket = TcpStream::connect_timeout(&peer.address, IO_TIMEOUT)
            .map_err(|_| TlsError::Connection)?;
        let channel = Self::handshake(connection.into(), socket)?;
        let leaf = channel
            .connection
            .peer_certificates()
            .and_then(|certs| certs.first())
            .ok_or(TlsError::Connection)?;
        if <[u8; 32]>::from(Sha256::digest(leaf.as_ref())) != peer.pin {
            return Err(TlsError::Connection);
        }
        Ok(channel)
    }

    pub(super) fn accept(socket: TcpStream, server: &ServerTls) -> Result<Self, TlsError> {
        let connection =
            ServerConnection::new(server.config.clone()).map_err(|_| TlsError::Configuration)?;
        Self::handshake(connection.into(), socket)
    }

    fn handshake(mut connection: Connection, socket: TcpStream) -> Result<Self, TlsError> {
        // Accepted Windows sockets can inherit the listener's nonblocking mode.
        socket
            .set_nonblocking(false)
            .map_err(|_| TlsError::Connection)?;
        connection.set_buffer_limit(Some(MAX_CONTROL_BYTES));
        let mut io = DeadlineTcp::new(socket);
        connection
            .complete_io(&mut io)
            .map_err(|_| TlsError::Connection)?;
        if connection.is_handshaking()
            || connection.alpn_protocol() != Some(ALPN)
            || connection.protocol_version() != Some(rustls::ProtocolVersion::TLSv1_3)
        {
            return Err(TlsError::Connection);
        }
        Ok(Self {
            connection,
            io,
            healthy: true,
        })
    }

    pub(super) fn send(&mut self, bytes: &[u8]) -> Result<(), TlsError> {
        if !self.healthy {
            return Err(TlsError::Connection);
        }
        if bytes.is_empty() || bytes.len() > MAX_CONTROL_BYTES {
            self.close();
            return Err(TlsError::Frame);
        }
        self.io.reset();
        let result = (|| {
            self.connection
                .writer()
                .write_all(&(bytes.len() as u32).to_be_bytes())?;
            self.flush_tls()?;
            for chunk in bytes.chunks(16384) {
                self.connection.writer().write_all(chunk)?;
                self.flush_tls()?;
            }
            Ok::<_, io::Error>(())
        })();
        if result.is_err() {
            self.close();
            return Err(TlsError::Connection);
        }
        Ok(())
    }

    pub(super) fn receive(&mut self) -> Result<Zeroizing<Vec<u8>>, TlsError> {
        if !self.healthy {
            return Err(TlsError::Connection);
        }
        self.io.reset();
        let result = (|| {
            let mut header = [0; 4];
            self.read_plain(&mut header)
                .map_err(|_| TlsError::Connection)?;
            let length = u32::from_be_bytes(header) as usize;
            if length == 0 || length > MAX_CONTROL_BYTES {
                return Err(TlsError::Frame);
            }
            let mut bytes = Zeroizing::new(vec![0; length]);
            self.read_plain(&mut bytes)
                .map_err(|_| TlsError::Connection)?;
            Ok(bytes)
        })();
        if result.is_err() {
            self.close();
        }
        result
    }

    fn read_plain(&mut self, output: &mut [u8]) -> io::Result<()> {
        let mut offset = 0;
        while offset < output.len() {
            match self.connection.reader().read(&mut output[offset..]) {
                Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
                Ok(count) => offset += count,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    self.flush_tls()?;
                    if self.connection.read_tls(&mut self.io)? == 0 {
                        return Err(io::ErrorKind::UnexpectedEof.into());
                    }
                    self.connection
                        .process_new_packets()
                        .map_err(|_| io::ErrorKind::InvalidData)?;
                }
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    fn flush_tls(&mut self) -> io::Result<()> {
        while self.connection.wants_write() {
            if self.connection.write_tls(&mut self.io)? == 0 {
                return Err(io::ErrorKind::WriteZero.into());
            }
        }
        Ok(())
    }

    pub(super) fn close(&mut self) {
        self.healthy = false;
        let _ = self.io.socket.shutdown(Shutdown::Both);
    }
}

/// Each handshake/frame has a total time and wire-byte budget, including trickle/control traffic.
struct DeadlineTcp {
    socket: TcpStream,
    deadline: Instant,
    remaining: usize,
}

impl DeadlineTcp {
    fn new(socket: TcpStream) -> Self {
        Self {
            socket,
            deadline: Instant::now() + IO_TIMEOUT,
            remaining: 256 * 1024,
        }
    }
    fn reset(&mut self) {
        self.deadline = Instant::now() + IO_TIMEOUT;
        self.remaining = 256 * 1024;
    }
    fn budget(&self) -> io::Result<Duration> {
        if self.remaining == 0 {
            return Err(io::ErrorKind::InvalidData.into());
        }
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|time| !time.is_zero())
            .ok_or_else(|| io::ErrorKind::TimedOut.into())
    }
}

impl Read for DeadlineTcp {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.socket.set_read_timeout(Some(self.budget()?))?;
        let length = bytes.len().min(self.remaining);
        let count = self.socket.read(&mut bytes[..length])?;
        self.remaining -= count;
        Ok(count)
    }
}

impl Write for DeadlineTcp {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.socket.set_write_timeout(Some(self.budget()?))?;
        let count = self
            .socket
            .write(&bytes[..bytes.len().min(self.remaining)])?;
        self.remaining -= count;
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.socket.flush()
    }
}

pub(super) fn private_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(ip) => {
            ip.is_loopback()
                || ip.is_private()
                || (ip.octets()[0] == 100 && (64..128).contains(&ip.octets()[1]))
        }
        IpAddr::V6(ip) => ip.is_loopback() || (ip.segments()[0] & 0xfe00 == 0xfc00),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(super) enum TlsError {
    #[error("Invalid trusted collaboration endpoint configuration")]
    Configuration,
    #[error("Secure collaboration connection unavailable")]
    Connection,
    #[error("Invalid bounded collaboration frame")]
    Frame,
}

#[cfg(test)]
#[path = "tls_tests.rs"]
mod tests;
