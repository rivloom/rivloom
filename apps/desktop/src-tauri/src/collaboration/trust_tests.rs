use super::super::tls::{ServerTls, TlsChannel};
use super::*;
use pretty_assertions::assert_eq;
use rcgen::{CertificateParams, KeyPair};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use serde_json::json;
use std::net::TcpListener;

fn identity() -> (TrustDescriptor, ServerTls) {
    let key = KeyPair::generate().unwrap();
    let cert = CertificateParams::new(vec!["localhost".into()])
        .unwrap()
        .self_signed(&key)
        .unwrap()
        .der()
        .to_vec();
    let server = ServerTls::new(
        vec![CertificateDer::from(cert.clone())],
        PrivatePkcs8KeyDer::from(key.serialize_der()).into(),
    )
    .unwrap();
    (
        TrustDescriptor::new(
            "brain-1".into(),
            "127.0.0.1:7443".parse().unwrap(),
            "localhost".into(),
            cert,
        )
        .unwrap(),
        server,
    )
}

#[test]
fn only_independently_confirmed_fingerprint_produces_a_usable_peer() {
    let (descriptor, _server) = identity();
    let bytes = descriptor.encode().unwrap();
    assert_eq!(TrustDescriptor::decode(&bytes).unwrap(), descriptor);
    for wrong in [
        "",
        "confirmed",
        &"0".repeat(64),
        &format!("{} ", descriptor.fingerprint()),
    ] {
        assert_eq!(
            TrustedPeer::confirm(&bytes, wrong).err(),
            Some(TrustError::Unconfirmed)
        );
    }
    let trusted = TrustedPeer::confirm(&bytes, &descriptor.fingerprint()).unwrap();
    assert_eq!(trusted.descriptor(), &descriptor);
    assert!(trusted.peer().is_ok());
}

#[test]
fn replacing_a_valid_certificate_cannot_reuse_the_confirmed_fingerprint() {
    let (original, _) = identity();
    let (replacement, _) = identity();
    assert_ne!(original.fingerprint(), replacement.fingerprint());
    assert_eq!(
        TrustedPeer::confirm(&replacement.encode().unwrap(), &original.fingerprint()).err(),
        Some(TrustError::Unconfirmed)
    );
}

#[test]
fn malformed_or_unbounded_descriptors_are_rejected_before_network_io() {
    let (descriptor, _) = identity();
    let valid = serde_json::to_value(&descriptor).unwrap();
    for (field, value) in [
        ("version", json!(2)),
        ("brainId", json!("../brain")),
        ("address", json!("0.0.0.0:7443")),
        ("address", json!("8.8.8.8:7443")),
        ("address", json!("127.0.0.1:0")),
        ("address", json!("https://localhost:7443")),
        ("serverName", json!("https://localhost")),
        ("serverName", json!("x".repeat(254))),
        ("certificateDer", json!([])),
        ("certificateDer", json!([0, 1, 2])),
        ("certificateDer", json!(vec![0; 1025])),
        ("secret", json!("must-not-be-accepted")),
    ] {
        let mut bad = valid.clone();
        bad[field] = value;
        assert_eq!(
            TrustDescriptor::decode(&serde_json::to_vec(&bad).unwrap()).err(),
            Some(TrustError::Invalid),
            "{field}"
        );
    }
    assert_eq!(
        TrustDescriptor::decode(&vec![b' '; MAX_DESCRIPTOR_BYTES + 1]).err(),
        Some(TrustError::Invalid)
    );
    let repeated = String::from_utf8(descriptor.encode().unwrap())
        .unwrap()
        .replacen("{", "{\"version\":1,", 1);
    assert_eq!(
        TrustDescriptor::decode(repeated.as_bytes()).err(),
        Some(TrustError::Invalid)
    );
}

#[test]
fn public_bootstrap_material_contains_no_private_key_or_node_credential() {
    let (descriptor, _) = identity();
    let value = serde_json::from_slice::<serde_json::Value>(&descriptor.encode().unwrap()).unwrap();
    assert_eq!(
        value,
        json!({
            "version":1,"brainId":"brain-1","address":"127.0.0.1:7443",
            "serverName":"localhost","certificateDer":descriptor.certificate_der()
        })
    );
}

#[test]
fn confirmed_descriptor_still_requires_the_live_tls_identity() {
    let (mut descriptor, tls) = identity();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    descriptor.address = listener.local_addr().unwrap();
    let trusted =
        TrustedPeer::confirm(&descriptor.encode().unwrap(), &descriptor.fingerprint()).unwrap();
    let worker = std::thread::spawn(move || {
        let mut channel = TlsChannel::accept(listener.accept().unwrap().0, &tls).unwrap();
        assert_eq!(
            channel.receive().unwrap().as_slice(),
            b"bounded-bootstrap-check"
        );
        channel.send(b"verified").unwrap();
    });
    let mut channel = TlsChannel::connect(&trusted.peer().unwrap()).unwrap();
    channel.send(b"bounded-bootstrap-check").unwrap();
    assert_eq!(channel.receive().unwrap().as_slice(), b"verified");
    worker.join().unwrap();
}
