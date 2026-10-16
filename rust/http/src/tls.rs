//! Southstar — TLS client connections over rustls: the shared client configurations, trust roots, ALPN, certificate checks, session resumption and non-blocking reads and writes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::sync::{Arc, Mutex};

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::client::{ClientSessionMemoryCache, Resumption};
use rustls::crypto::{CryptoProvider, SupportedKxGroup, WebPkiSupportedAlgorithms, ring};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{
    CertificateError, ClientConfig, ClientConnection, DigitallySignedStruct, Error, RootCertStore,
    SignatureScheme,
};

use crate::ffi::{roots, socket};
use crate::x25519_mlkem768::X25519MLKEM768;

pub struct Settings {
    pub ca_bundle: Option<Vec<u8>>,
    pub curves: Vec<u8>,
}

const SESSION_CACHE_SIZE: usize = 256;

type Roots = (Option<Vec<u8>>, Arc<RootCertStore>);

static CONFIGS: Mutex<[Option<Arc<ClientConfig>>; 4]> = Mutex::new([None, None, None, None]);
static SESSIONS: Mutex<Option<Arc<ClientSessionMemoryCache>>> = Mutex::new(None);
static ROOTS: Mutex<Option<Roots>> = Mutex::new(None);

fn config(verify: bool, h2: bool, settings: &Settings) -> Result<Arc<ClientConfig>, Error> {
    let mut slots = CONFIGS.lock().unwrap_or_else(|e| e.into_inner());
    let slot = &mut slots[usize::from(!verify) * 2 + usize::from(!h2)];
    if let Some(config) = slot {
        return Ok(config.clone());
    }
    let provider = Arc::new(provider(&settings.curves));
    let builder = ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()?;
    let mut config = if verify {
        builder
            .with_root_certificates(trust_roots(settings.ca_bundle.as_deref()))
            .with_no_client_auth()
    } else {
        builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(AnyCertificate(
                provider.signature_verification_algorithms,
            )))
            .with_no_client_auth()
    };
    config.alpn_protocols = if h2 {
        vec![b"h2".to_vec(), b"http/1.1".to_vec()]
    } else {
        vec![b"http/1.1".to_vec()]
    };
    config.resumption = if verify {
        Resumption::store(session_cache())
    } else {
        Resumption::disabled()
    };
    let config = Arc::new(config);
    *slot = Some(config.clone());
    Ok(config)
}

fn session_cache() -> Arc<ClientSessionMemoryCache> {
    SESSIONS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(|| Arc::new(ClientSessionMemoryCache::new(SESSION_CACHE_SIZE)))
        .clone()
}

pub(crate) fn provider(curves: &[u8]) -> CryptoProvider {
    let mut provider = ring::default_provider();
    let groups: Vec<_> = curves
        .split(|&c| c == b':')
        .filter_map(key_exchange_group)
        .collect();
    if !groups.is_empty() {
        provider.kx_groups = groups;
    } else {
        provider.kx_groups.insert(0, X25519MLKEM768);
    }
    provider
}

fn key_exchange_group(name: &[u8]) -> Option<&'static dyn SupportedKxGroup> {
    match name.to_ascii_lowercase().as_slice() {
        b"x25519mlkem768" => Some(X25519MLKEM768),
        b"x25519" => Some(ring::kx_group::X25519),
        b"p-256" | b"prime256v1" | b"secp256r1" => Some(ring::kx_group::SECP256R1),
        b"p-384" | b"secp384r1" => Some(ring::kx_group::SECP384R1),
        _ => None,
    }
}

pub fn preload_trust_roots(ca_bundle: Option<&[u8]>) {
    trust_roots(ca_bundle);
}

fn trust_roots(ca_bundle: Option<&[u8]>) -> Arc<RootCertStore> {
    let bundle = ca_bundle
        .filter(|path| !path.is_empty())
        .map(<[u8]>::to_vec);
    let mut cached = ROOTS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((path, store)) = cached.as_ref()
        && *path == bundle
    {
        return store.clone();
    }
    let store = Arc::new(load_trust_roots(bundle.as_deref()));
    *cached = Some((bundle, store.clone()));
    store
}

fn load_trust_roots(bundle: Option<&[u8]>) -> RootCertStore {
    let mut store = RootCertStore::empty();
    match bundle.and_then(|path| std::str::from_utf8(path).ok()) {
        Some(path) => add_pem_file(&mut store, Path::new(path)),
        None => {
            store.add_parsable_certificates(roots::system().into_iter().map(CertificateDer::from));
            if cfg!(unix) {
                add_pem_dir(&mut store);
            }
        }
    }
    store
}

fn add_pem_file(store: &mut RootCertStore, path: &Path) {
    if let Ok(certificates) = CertificateDer::pem_file_iter(path) {
        store.add_parsable_certificates(certificates.filter_map(Result::ok));
    }
}

fn add_pem_dir(store: &mut RootCertStore) {
    let dir = std::env::var_os("SSL_CERT_DIR").unwrap_or_else(|| "/etc/ssl/certs".into());
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.path().is_file() {
            add_pem_file(store, &entry.path());
        }
    }
}

#[derive(Debug)]
struct AnyCertificate(WebPkiSupportedAlgorithms);

impl ServerCertVerifier for AnyCertificate {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.0)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.0)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.supported_schemes()
    }
}

fn certificate_problem(error: &Error) -> Option<String> {
    let Error::InvalidCertificate(problem) = error else {
        return None;
    };
    let text = match problem {
        CertificateError::Expired | CertificateError::ExpiredContext { .. } => {
            "certificate has expired"
        }
        CertificateError::NotValidYet | CertificateError::NotValidYetContext { .. } => {
            "certificate is not yet valid"
        }
        CertificateError::UnknownIssuer => "certificate issuer is unknown or self-signed",
        CertificateError::NotValidForName | CertificateError::NotValidForNameContext { .. } => {
            "certificate is not valid for this host name"
        }
        CertificateError::Revoked => "certificate has been revoked",
        CertificateError::BadSignature => "certificate signature is invalid",
        CertificateError::BadEncoding => "certificate is malformed",
        CertificateError::InvalidPurpose | CertificateError::InvalidPurposeContext { .. } => {
            "certificate is not valid for a TLS server"
        }
        _ => return Some(error.to_string()),
    };
    Some(String::from(text))
}

fn would_block(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
    ) || socket::retryable(error.raw_os_error().unwrap_or(0))
}

fn unbracketed(host: &str) -> &str {
    host.strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host)
}

pub struct Tls {
    conn: ClientConnection,
    tcp: TcpStream,
}

struct Sender<'a>(&'a TcpStream);

impl Write for Sender<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut tcp = self.0;
        tcp.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub enum Io {
    Done(usize),
    WouldBlock,
    Closed,
    Failed,
}

pub enum Handshake {
    Cancelled,
    Failed { certificate: Option<String> },
}

impl Tls {
    pub fn handshake(
        tcp: TcpStream,
        host: &str,
        verify: bool,
        h2: bool,
        settings: &Settings,
        should_abort: &dyn Fn() -> bool,
    ) -> Result<Tls, Handshake> {
        let failed = |certificate| Handshake::Failed { certificate };
        let config = config(verify, h2, settings).map_err(|_| failed(None))?;
        let name = ServerName::try_from(unbracketed(host).to_owned()).map_err(|_| failed(None))?;
        let conn = ClientConnection::new(config, name).map_err(|_| failed(None))?;
        let mut tls = Tls { conn, tcp };
        loop {
            match tls.flush_tls() {
                Ok(true) => {}
                Ok(false) => {
                    if should_abort() {
                        return Err(Handshake::Cancelled);
                    }
                    socket::wait(socket::raw(&tls.tcp), socket::POLLOUT, 250);
                    continue;
                }
                Err(_) => return Err(failed(None)),
            }
            if !tls.conn.is_handshaking() {
                return Ok(tls);
            }
            match tls.conn.read_tls(&mut &tls.tcp) {
                Ok(0) => return Err(failed(None)),
                Ok(_) => {
                    if let Err(error) = tls.conn.process_new_packets() {
                        let _ = tls.flush_tls();
                        return Err(failed(certificate_problem(&error)));
                    }
                }
                Err(error) if would_block(&error) => {
                    if should_abort() {
                        return Err(Handshake::Cancelled);
                    }
                    socket::wait(socket::raw(&tls.tcp), socket::POLLIN, 250);
                }
                Err(_) => return Err(failed(None)),
            }
        }
    }

    pub fn tcp(&self) -> &TcpStream {
        &self.tcp
    }

    pub fn alpn_h2(&self) -> bool {
        self.conn.alpn_protocol() == Some(b"h2")
    }

    pub fn pending(&mut self) -> bool {
        self.conn
            .process_new_packets()
            .is_ok_and(|state| state.plaintext_bytes_to_read() > 0)
    }

    pub fn wants_write(&self) -> bool {
        self.conn.wants_write()
    }

    fn flush_tls(&mut self) -> io::Result<bool> {
        while self.conn.wants_write() {
            match self.conn.write_tls(&mut Sender(&self.tcp)) {
                Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                Ok(_) => {}
                Err(error) if would_block(&error) => return Ok(false),
                Err(error) => return Err(error),
            }
        }
        Ok(true)
    }

    pub fn flush(&mut self) -> Io {
        match self.flush_tls() {
            Ok(true) => Io::Done(0),
            Ok(false) => Io::WouldBlock,
            Err(_) => Io::Failed,
        }
    }

    pub fn read(&mut self, buf: &mut [u8]) -> Io {
        if self.flush_tls().is_err() {
            return Io::Failed;
        }
        loop {
            match self.conn.reader().read(buf) {
                Ok(0) => return Io::Closed,
                Ok(n) => return Io::Done(n),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Io::Closed,
                Err(_) => return Io::Failed,
            }
            match self.conn.read_tls(&mut &self.tcp) {
                Ok(_) => {}
                Err(error) if would_block(&error) => return Io::WouldBlock,
                Err(_) => return Io::Failed,
            }
            if self.conn.process_new_packets().is_err() {
                let _ = self.flush_tls();
                return Io::Failed;
            }
            if self.flush_tls().is_err() {
                return Io::Failed;
            }
        }
    }

    pub fn write(&mut self, buf: &[u8]) -> Io {
        match self.flush_tls() {
            Ok(true) => {}
            Ok(false) => return Io::WouldBlock,
            Err(_) => return Io::Failed,
        }
        let Ok(n) = self.conn.writer().write(buf) else {
            return Io::Failed;
        };
        match self.flush_tls() {
            Ok(_) => Io::Done(n),
            Err(_) => Io::Failed,
        }
    }
}

impl Drop for Tls {
    fn drop(&mut self) {
        self.conn.send_close_notify();
        let _ = self.flush_tls();
    }
}

pub fn shutdown() {
    *CONFIGS.lock().unwrap_or_else(|e| e.into_inner()) = [None, None, None, None];
    SESSIONS.lock().unwrap_or_else(|e| e.into_inner()).take();
}
