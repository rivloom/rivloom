use super::*;
use pretty_assertions::assert_eq;
use rcgen::{CertificateParams, KeyPair, date_time_ymd};
use rustls::pki_types::PrivatePkcs8KeyDer;
use std::net::TcpListener;
use std::thread;

enum Validity {
    Current,
    Expired,
}

fn identity(validity: Validity) -> (ServerTls, Vec<u8>, [u8; 32]) {
    let mut params = CertificateParams::new(vec!["localhost".into()]).unwrap();
    if matches!(validity, Validity::Expired) {
        params.not_before = date_time_ymd(/*year*/ 1999, /*month*/ 1, /*day*/ 1);
        params.not_after = date_time_ymd(/*year*/ 2000, /*month*/ 1, /*day*/ 1);
    }
    let key = KeyPair::generate().unwrap();
    let cert = params.self_signed(&key).unwrap().der().to_vec();
    let pin = Sha256::digest(&cert).into();
    let server = ServerTls::new(
        vec![CertificateDer::from(cert.clone())],
        PrivatePkcs8KeyDer::from(key.serialize_der()).into(),
    )
    .unwrap();
    (server, cert, pin)
}

#[test]
fn pinned_tls13_exchanges_a_maximum_bounded_frame() {
    let (server, root, pin) = identity(Validity::Current);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let peer = Peer::new(
        listener.local_addr().unwrap(),
        "localhost".into(),
        root,
        pin,
    )
    .unwrap();
    let handle = thread::spawn(move || {
        let mut channel = TlsChannel::accept(listener.accept().unwrap().0, &server).unwrap();
        let bytes = channel.receive().unwrap();
        assert_eq!(bytes.len(), MAX_CONTROL_BYTES);
        channel.send(&bytes).unwrap();
    });
    let mut client = TlsChannel::connect(&peer).unwrap();
    let bytes = vec![b'x'; MAX_CONTROL_BYTES];
    client.send(&bytes).unwrap();
    assert_eq!(client.receive().unwrap().as_slice(), bytes);
    handle.join().unwrap();
}

#[test]
fn invalid_root_name_pin_expiry_and_alpn_never_deliver_application_data() {
    enum Failure {
        Root,
        Name,
        Pin,
        Expired,
        Alpn,
    }
    for failure in [
        Failure::Root,
        Failure::Name,
        Failure::Pin,
        Failure::Expired,
        Failure::Alpn,
    ] {
        let validity = if matches!(failure, Failure::Expired) {
            Validity::Expired
        } else {
            Validity::Current
        };
        let (mut server, mut root, mut pin) = identity(validity);
        let mut name = "localhost".to_owned();
        match failure {
            Failure::Root => root = identity(Validity::Current).1,
            Failure::Name => name = "untrusted.example".into(),
            Failure::Pin => pin = [0; 32],
            Failure::Alpn => {
                Arc::get_mut(&mut server.config).unwrap().alpn_protocols = vec![b"other/1".to_vec()]
            }
            Failure::Expired => {}
        }
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let peer = Peer::new(listener.local_addr().unwrap(), name, root, pin).unwrap();
        let handle = thread::spawn(move || {
            TlsChannel::accept(listener.accept().unwrap().0, &server)
                .and_then(|mut channel| channel.receive())
                .is_ok()
        });
        assert!(TlsChannel::connect(&peer).is_err());
        assert!(!handle.join().unwrap());
    }
}

#[test]
fn plaintext_peer_cannot_trigger_a_fallback_connection() {
    let (_, root, pin) = identity(Validity::Current);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let peer = Peer::new(
        listener.local_addr().unwrap(),
        "localhost".into(),
        root,
        pin,
    )
    .unwrap();
    let handle = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut bytes = [0; 4096];
        let count = socket.read(&mut bytes).unwrap();
        socket
            .write_all(b"HTTP/1.1 200 OK\r\n\r\nplaintext")
            .unwrap();
        assert_eq!(bytes[0], 22); // TLS handshake, never an application request.
        assert!(count > 5);
    });
    assert!(TlsChannel::connect(&peer).is_err());
    handle.join().unwrap();
}

#[test]
fn oversized_and_truncated_frames_close_the_channel_without_partial_delivery() {
    for length in [MAX_CONTROL_BYTES + 1, 10] {
        let (server, root, pin) = identity(Validity::Current);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let peer = Peer::new(
            listener.local_addr().unwrap(),
            "localhost".into(),
            root,
            pin,
        )
        .unwrap();
        let handle = thread::spawn(move || {
            let mut channel = TlsChannel::accept(listener.accept().unwrap().0, &server).unwrap();
            channel
                .connection
                .writer()
                .write_all(&(length as u32).to_be_bytes())
                .unwrap();
            channel.connection.writer().write_all(b"bad").unwrap();
            channel.flush_tls().unwrap();
        });
        let mut client = TlsChannel::connect(&peer).unwrap();
        assert!(client.receive().is_err());
        assert_eq!(client.receive(), Err(TlsError::Connection));
        assert_eq!(client.send(b"must-not-send"), Err(TlsError::Connection));
        handle.join().unwrap();
    }
}

#[test]
fn transport_deadlines_and_wire_budgets_are_not_reset_by_partial_reads() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut writer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let mut io = DeadlineTcp::new(listener.accept().unwrap().0);
    writer.write_all(b"ab").unwrap();
    io.remaining = 1;
    let mut bytes = [0; 2];
    assert_eq!(io.read(&mut bytes).unwrap(), 1);
    assert!(io.read(&mut bytes).is_err());
    io.reset();
    io.deadline = Instant::now();
    assert_eq!(
        io.read(&mut bytes).unwrap_err().kind(),
        io::ErrorKind::TimedOut
    );
}

#[test]
fn invalid_or_public_endpoints_are_rejected_before_connecting() {
    let (_, root, pin) = identity(Validity::Current);
    for (address, name) in [
        ("8.8.8.8:443", "localhost"),
        ("0.0.0.0:0", "localhost"),
        ("127.0.0.1:443", "http://localhost"),
    ] {
        assert!(Peer::new(address.parse().unwrap(), name.into(), root.clone(), pin).is_err());
    }
    assert!(
        Peer::new(
            "127.0.0.1:443".parse().unwrap(),
            "localhost".into(),
            vec![],
            pin
        )
        .is_err()
    );
}
