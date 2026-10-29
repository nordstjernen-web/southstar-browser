//! Southstar — unit tests for the HTTP client's TLS key exchange (the X25519MLKEM768 hybrid and the configured group list) and its proxy choice from the environment.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use ml_kem::{EncapsulationKey768, KeyExport};
use ring::agreement::{self, EphemeralPrivateKey, UnparsedPublicKey};
use ring::rand::SystemRandom;
use rustls::NamedGroup;
use rustls::crypto::SupportedKxGroup;

use crate::proxy;
use crate::x25519_mlkem768::{CIPHERTEXT_LEN, SEED_LEN, X25519_LEN, X25519MlKem768};

const ENCAPSULATION_KEY_LEN: usize = 1184;

struct Server {
    share: Vec<u8>,
    secret: Vec<u8>,
}

fn server_reply(client_share: &[u8], message: [u8; 32]) -> Server {
    let (encapsulation_key, client_x25519) = client_share.split_at(ENCAPSULATION_KEY_LEN);
    let key = EncapsulationKey768::new(&encapsulation_key.try_into().unwrap()).unwrap();
    assert_eq!(key.to_bytes().as_slice(), encapsulation_key);
    let (ciphertext, post_quantum) = key.encapsulate_deterministic(&message.into());
    let rng = SystemRandom::new();
    let private = EphemeralPrivateKey::generate(&agreement::X25519, &rng).unwrap();
    let public = private.compute_public_key().unwrap();
    let classical = agreement::agree_ephemeral(
        private,
        &UnparsedPublicKey::new(&agreement::X25519, client_x25519),
        <[u8]>::to_vec,
    )
    .unwrap();
    let mut share = ciphertext.to_vec();
    share.extend_from_slice(public.as_ref());
    let mut secret = post_quantum.to_vec();
    secret.extend_from_slice(&classical);
    Server { share, secret }
}

#[test]
fn hybrid_share_is_encapsulation_key_then_x25519() {
    let client = X25519MlKem768.start_with_seed([7; SEED_LEN]).unwrap();
    assert_eq!(client.group(), NamedGroup::X25519MLKEM768);
    assert_eq!(client.pub_key().len(), ENCAPSULATION_KEY_LEN + X25519_LEN);
    let (group, classical) = client.hybrid_component().unwrap();
    assert_eq!(group, NamedGroup::X25519);
    assert_eq!(classical, &client.pub_key()[ENCAPSULATION_KEY_LEN..]);
}

#[test]
fn hybrid_secret_is_ml_kem_then_x25519() {
    let client = X25519MlKem768.start_with_seed([1; SEED_LEN]).unwrap();
    let server = server_reply(client.pub_key(), [9; 32]);
    assert_eq!(server.share.len(), CIPHERTEXT_LEN + X25519_LEN);
    let secret = client.complete(&server.share).unwrap();
    assert_eq!(secret.secret_bytes(), server.secret.as_slice());
}

#[test]
fn same_seed_gives_same_encapsulation_key() {
    let a = X25519MlKem768.start_with_seed([3; SEED_LEN]).unwrap();
    let b = X25519MlKem768.start_with_seed([3; SEED_LEN]).unwrap();
    let c = X25519MlKem768.start_with_seed([4; SEED_LEN]).unwrap();
    assert_eq!(
        a.pub_key()[..ENCAPSULATION_KEY_LEN],
        b.pub_key()[..ENCAPSULATION_KEY_LEN]
    );
    assert_ne!(
        a.pub_key()[..ENCAPSULATION_KEY_LEN],
        c.pub_key()[..ENCAPSULATION_KEY_LEN]
    );
    assert_ne!(
        a.pub_key()[ENCAPSULATION_KEY_LEN..],
        b.pub_key()[ENCAPSULATION_KEY_LEN..]
    );
}

#[test]
fn classical_component_completes_alone() {
    let client = X25519MlKem768.start().unwrap();
    let client_x25519 = client.hybrid_component().unwrap().1.to_vec();
    let rng = SystemRandom::new();
    let private = EphemeralPrivateKey::generate(&agreement::X25519, &rng).unwrap();
    let public = private.compute_public_key().unwrap();
    let expected = agreement::agree_ephemeral(
        private,
        &UnparsedPublicKey::new(&agreement::X25519, &client_x25519),
        <[u8]>::to_vec,
    )
    .unwrap();
    let secret = client.complete_hybrid_component(public.as_ref()).unwrap();
    assert_eq!(secret.secret_bytes(), expected.as_slice());
}

#[test]
fn malformed_server_share_is_refused() {
    let client = X25519MlKem768.start().unwrap();
    assert!(client.complete(&[0; CIPHERTEXT_LEN]).is_err());
    let client = X25519MlKem768.start().unwrap();
    assert!(
        client
            .complete(&[0; CIPHERTEXT_LEN + X25519_LEN + 1])
            .is_err()
    );
    let client = X25519MlKem768.start().unwrap();
    assert!(client.complete(&[0; CIPHERTEXT_LEN + X25519_LEN]).is_err());
}

#[test]
fn hybrid_is_tls13_only_and_client_only() {
    assert!(X25519MlKem768.usable_for_version(rustls::ProtocolVersion::TLSv1_3));
    assert!(!X25519MlKem768.usable_for_version(rustls::ProtocolVersion::TLSv1_2));
    assert!(X25519MlKem768.start_and_complete(&[0; 1216]).is_err());
}

#[test]
fn group_list_follows_the_configured_names() {
    let names = |curves: &[u8]| -> Vec<NamedGroup> {
        crate::tls::provider(curves)
            .kx_groups
            .iter()
            .map(|g| g.name())
            .collect()
    };
    assert_eq!(
        names(b"X25519MLKEM768:X25519:P-256:P-384"),
        [
            NamedGroup::X25519MLKEM768,
            NamedGroup::X25519,
            NamedGroup::secp256r1,
            NamedGroup::secp384r1
        ]
    );
    assert_eq!(
        names(b"secp384r1:bogus:x25519"),
        [NamedGroup::secp384r1, NamedGroup::X25519]
    );
    assert_eq!(names(b"")[0], NamedGroup::X25519MLKEM768);
    assert_eq!(names(b"bogus")[0], NamedGroup::X25519MLKEM768);
}

fn env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<Vec<u8>> + 'a {
    move |name| {
        vars.iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.as_bytes().to_vec())
    }
}

fn proxy_host(url: &str, host: &str, vars: &[(&str, &str)]) -> Option<String> {
    proxy::from_env(url.as_bytes(), host, &env(vars)).map(|p| format!("{}:{}", p.host, p.port))
}

#[test]
fn downloads_use_the_proxy_for_their_scheme() {
    let vars = [
        ("http_proxy", "http://plain.example:8080"),
        ("https_proxy", "http://secure.example:3128"),
    ];
    assert_eq!(
        proxy_host("https://a.example/x.mp3", "a.example", &vars).as_deref(),
        Some("secure.example:3128")
    );
    assert_eq!(
        proxy_host("http://a.example/x.mp3", "a.example", &vars).as_deref(),
        Some("plain.example:8080")
    );
    let only_http = [("http_proxy", "http://plain.example:8080")];
    assert_eq!(
        proxy_host("https://a.example/x.mp3", "a.example", &only_http).as_deref(),
        Some("plain.example:8080")
    );
    let all = [("all_proxy", "socks5h://127.0.0.1:9050")];
    assert_eq!(
        proxy_host("https://a.example/x.mp3", "a.example", &all).as_deref(),
        Some("127.0.0.1:9050")
    );
    assert_eq!(
        proxy_host("https://a.example/x.mp3", "a.example", &[]),
        None
    );
}

#[test]
fn no_proxy_exempts_hosts() {
    let vars = [
        ("all_proxy", "socks5h://127.0.0.1:9050"),
        ("no_proxy", "intranet.example"),
    ];
    assert_eq!(
        proxy_host("https://intranet.example/a.mp3", "intranet.example", &vars),
        None
    );
    assert!(proxy_host("https://other.example/a.mp3", "other.example", &vars).is_some());
}

#[test]
fn an_unparsable_proxy_fails_closed() {
    assert!(proxy::env_valid(&env(&[(
        "http_proxy",
        "http://p.example:80"
    )])));
    assert!(proxy::env_valid(&env(&[])));
    assert!(!proxy::env_valid(&env(&[(
        "all_proxy",
        "bogus://p.example"
    )])));
}

#[test]
fn an_upgrade_request_carries_its_cookies() {
    let request = crate::client::Request {
        url: b"https://live.example/ws",
        https: true,
        host: "live.example",
        port: 443,
        authority: b"live.example",
        path: b"/ws",
        method: b"GET",
        user_agent: Some(b"UA"),
        referer: None,
        cookie: Some(b"session=abc; theme=dark".to_vec()),
        extra_headers: vec![b"Upgrade: websocket"],
        body: b"",
        timeout: std::time::Duration::MAX,
        connect_timeout: std::time::Duration::from_secs(1),
        allow_insecure: false,
        tls: crate::tls::Settings {
            ca_bundle: None,
            curves: Vec::new(),
        },
        proxy: None,
    };
    let head = crate::client::h1_head_upgrade(&request);
    let text = String::from_utf8_lossy(&head);
    assert!(text.contains("\r\nCookie: session=abc; theme=dark\r\n"));
    assert!(text.ends_with("Connection: Upgrade\r\n\r\n"));
}
