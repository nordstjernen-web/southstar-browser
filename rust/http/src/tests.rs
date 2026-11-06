//! Southstar — unit tests for the HTTP client's TLS key exchange (the X25519MLKEM768 hybrid and the configured group list), its proxy choice from the environment, FTP URL parsing, the zstd window cap and gzip and deflate body decoding.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use ml_kem::{EncapsulationKey768, KeyExport};
use ring::agreement::{self, EphemeralPrivateKey, UnparsedPublicKey};
use ring::rand::SystemRandom;
use rustls::NamedGroup;
use rustls::crypto::SupportedKxGroup;

use crate::x25519_mlkem768::{CIPHERTEXT_LEN, SEED_LEN, X25519_LEN, X25519MlKem768};
use crate::{ftp, proxy};

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

#[test]
fn ftp_url_parts_are_decoded() {
    let location = ftp::parse(b"ftp://me:p%40ss@Example.org:2121/pub/a%20b/file.txt").unwrap();
    assert_eq!(location.host, "example.org");
    assert_eq!(location.port, 2121);
    assert_eq!(location.user, b"me");
    assert_eq!(location.password, b"p@ss");
    assert_eq!(location.dirs, vec![b"pub".to_vec(), b"a b".to_vec()]);
    assert_eq!(location.file.as_deref(), Some(&b"file.txt"[..]));
}

#[test]
fn ftp_url_refuses_line_breaks_in_commands() {
    let urls: [&[u8]; 6] = [
        b"ftp://example.org/x%0D%0ADELE%20y",
        b"ftp://example.org/dir%0A/file",
        b"ftp://us%0Der@example.org/file",
        b"ftp://user:pa%0Ass@example.org/file",
        b"ftp://example.org/fi%00le",
        b"ftp://exa\r\nmple.org/file",
    ];
    for url in urls {
        assert!(
            ftp::parse(url).is_none(),
            "{}",
            String::from_utf8_lossy(url)
        );
    }
}

#[cfg(feature = "zstd")]
fn zstd_frame(window_descriptor: u8) -> Vec<u8> {
    vec![
        0x28,
        0xb5,
        0x2f,
        0xfd,
        0x00,
        window_descriptor,
        0x09,
        0x00,
        0x00,
        b'a',
    ]
}

#[cfg(feature = "zstd")]
fn zstd_decode(frame: &[u8]) -> Option<Vec<u8>> {
    let mut zstd = crate::ffi::codec::Zstd::new()?;
    let mut out = Vec::new();
    let mut emit = |chunk: &[u8]| {
        out.extend_from_slice(chunk);
        true
    };
    zstd.feed(frame, &mut emit).ok()?;
    Some(out)
}

#[cfg(feature = "zstd")]
#[test]
fn zstd_window_is_capped_at_8_mib() {
    assert_eq!(
        zstd_decode(&zstd_frame(13 << 3)).as_deref(),
        Some(&b"a"[..])
    );
    assert_eq!(zstd_decode(&zstd_frame((13 << 3) | 1)), None);
    assert_eq!(zstd_decode(&zstd_frame(17 << 3)), None);
}

#[test]
fn doh_cache_stays_bounded() {
    use std::collections::HashMap;
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::{Duration, Instant};

    let now = Instant::now();
    let addr = vec![IpAddr::V4(Ipv4Addr::LOCALHOST)];
    let mut cache = HashMap::new();
    for i in 0..5000u64 {
        let expires = now + Duration::from_secs(60 + i);
        crate::doh::remember(&mut cache, format!("h{i}.test"), addr.clone(), expires, now);
    }
    assert_eq!(cache.len(), 1024);
    assert!(cache.contains_key("h4999.test"));
    assert!(!cache.contains_key("h0.test"));

    let later = now + Duration::from_secs(100_000);
    crate::doh::remember(
        &mut cache,
        "fresh.test".to_owned(),
        addr,
        later + Duration::from_secs(60),
        later,
    );
    assert_eq!(cache.len(), 1);
}

fn sample_body() -> Vec<u8> {
    (0..20_000u32)
        .flat_map(|i| format!("line {i} of a compressible body\n").into_bytes())
        .collect()
}

fn encode(encoding: crate::decode::Encoding, body: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let level = flate2::Compression::default();
    match encoding {
        crate::decode::Encoding::Gzip => {
            let mut encoder = flate2::write::GzEncoder::new(Vec::new(), level);
            encoder.write_all(body).unwrap();
            encoder.finish().unwrap()
        }
        _ => {
            let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), level);
            encoder.write_all(body).unwrap();
            encoder.finish().unwrap()
        }
    }
}

fn raw_deflate(body: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut encoder =
        flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(body).unwrap();
    encoder.finish().unwrap()
}

fn decode_in_chunks(
    encoding: crate::decode::Encoding,
    data: &[u8],
    chunk: usize,
) -> Result<Vec<u8>, ()> {
    let mut decoder = crate::decode::Decoder::new(&[encoding]).ok_or(())?;
    let mut out = Vec::new();
    for piece in data.chunks(chunk.max(1)) {
        decoder
            .feed(piece, &mut |bytes: &[u8]| {
                out.extend_from_slice(bytes);
                true
            })
            .map_err(|_| ())?;
    }
    decoder
        .finish(&mut |bytes: &[u8]| {
            out.extend_from_slice(bytes);
            true
        })
        .map_err(|_| ())?;
    Ok(out)
}

#[test]
fn gzip_body_decodes_whole_and_byte_by_byte() {
    let body = sample_body();
    let gzip = encode(crate::decode::Encoding::Gzip, &body);
    assert_eq!(
        decode_in_chunks(crate::decode::Encoding::Gzip, &gzip, gzip.len()),
        Ok(body.clone())
    );
    assert_eq!(
        decode_in_chunks(crate::decode::Encoding::Gzip, &gzip, 1),
        Ok(body)
    );
}

#[test]
fn concatenated_gzip_members_decode_in_order() {
    let mut both = encode(crate::decode::Encoding::Gzip, b"first member, ");
    both.extend(encode(crate::decode::Encoding::Gzip, b"second member"));
    assert_eq!(
        decode_in_chunks(crate::decode::Encoding::Gzip, &both, 7),
        Ok(b"first member, second member".to_vec())
    );
}

#[test]
fn deflate_accepts_zlib_wrapped_and_raw_streams() {
    let body = sample_body();
    let zlib = encode(crate::decode::Encoding::Deflate, &body);
    assert_eq!(
        decode_in_chunks(crate::decode::Encoding::Deflate, &zlib, 1000),
        Ok(body.clone())
    );
    let raw = raw_deflate(&body);
    assert_eq!(
        decode_in_chunks(crate::decode::Encoding::Deflate, &raw, 3),
        Ok(body)
    );
}

#[test]
fn corrupt_gzip_is_an_error() {
    let mut gzip = encode(crate::decode::Encoding::Gzip, &sample_body());
    let middle = gzip.len() / 2;
    gzip[middle..middle + 16].fill(0xff);
    assert_eq!(
        decode_in_chunks(crate::decode::Encoding::Gzip, &gzip, 4096),
        Err(())
    );
}

#[test]
fn inflate_rejects_unsupported_window_bits() {
    assert!(crate::Inflate::new(15).is_some());
    assert!(crate::Inflate::new(-15).is_some());
    assert!(crate::Inflate::new(31).is_some());
    assert!(crate::Inflate::new(47).is_some());
    assert!(crate::Inflate::new(48).is_none());
    assert!(crate::Inflate::new(7).is_none());
}
