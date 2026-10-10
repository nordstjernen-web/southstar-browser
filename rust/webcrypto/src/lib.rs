//! Southstar — SubtleCrypto primitives over OpenSSL libcrypto: digests, HMAC, AES, RSA, ECDSA/ECDH, Ed25519/X25519, PBKDF2 and HKDF.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

use core::ffi::CStr;
use ffi::openssl::{
    self, AesMode, Cipher, CipherCtx, Md, Padding, Param, Pkey, PkeyRef, Selection,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum KeyType {
    Secret,
    Public,
    Private,
}

pub struct Key<'a> {
    pub kind: KeyType,
    pub algo: &'a [u8],
    pub hash: Option<&'a [u8]>,
    pub curve: Option<&'a [u8]>,
    pub bits: i32,
    pub raw: Option<&'a [u8]>,
    pub pkey: Option<PkeyRef<'a>>,
}

pub struct Params<'a> {
    pub iv: Option<&'a [u8]>,
    pub aad: Option<&'a [u8]>,
    pub tag_bits: i32,
    pub label: Option<&'a [u8]>,
    pub salt: Option<&'a [u8]>,
    pub info: Option<&'a [u8]>,
    pub iterations: i32,
    pub kdf_hash: Option<&'a [u8]>,
    pub sign_hash: Option<&'a [u8]>,
    pub peer: Option<Key<'a>>,
    pub counter_bits: i32,
    pub pss_salt_len: i32,
}

pub struct NewKey<'a> {
    pub kind: KeyType,
    pub algo: Option<&'a [u8]>,
    pub hash: Option<&'a [u8]>,
    pub curve: Option<&'a [u8]>,
    pub bits: i32,
    pub extractable: bool,
    pub usages: u32,
    pub raw: Option<Vec<u8>>,
    pub pkey: Option<Pkey>,
}

impl Drop for NewKey<'_> {
    fn drop(&mut self) {
        if let Some(raw) = self.raw.as_mut() {
            openssl::cleanse(raw);
        }
    }
}

pub struct StoredKey {
    pub kind: KeyType,
    pub algo: Option<Vec<u8>>,
    pub hash: Option<Vec<u8>>,
    pub curve: Option<Vec<u8>>,
    pub bits: i32,
    pub extractable: bool,
    pub usages: u32,
    raw: Option<Vec<u8>>,
    pkey: Option<Pkey>,
}

impl StoredKey {
    pub fn view(&self) -> Key<'_> {
        Key {
            kind: self.kind,
            algo: self.algo.as_deref().unwrap_or_default(),
            hash: self.hash.as_deref(),
            curve: self.curve.as_deref(),
            bits: self.bits,
            raw: self.raw.as_deref(),
            pkey: self.pkey.as_ref().map(Pkey::as_ref),
        }
    }

    pub fn raw(&self) -> Option<&[u8]> {
        self.raw.as_deref()
    }
}

impl From<NewKey<'_>> for StoredKey {
    fn from(mut key: NewKey<'_>) -> StoredKey {
        StoredKey {
            kind: key.kind,
            algo: key.algo.map(<[u8]>::to_vec),
            hash: key.hash.map(<[u8]>::to_vec),
            curve: key.curve.map(<[u8]>::to_vec),
            bits: key.bits,
            extractable: key.extractable,
            usages: key.usages,
            raw: key.raw.take().filter(|raw| !raw.is_empty()),
            pkey: key.pkey.take(),
        }
    }
}

impl Drop for StoredKey {
    fn drop(&mut self) {
        if let Some(raw) = self.raw.as_mut() {
            openssl::cleanse(raw);
        }
    }
}

pub struct KeyAttrs<'a> {
    pub algo: Option<&'a [u8]>,
    pub hash: Option<&'a [u8]>,
    pub curve: Option<&'a [u8]>,
    pub extractable: bool,
    pub usages: u32,
}

impl<'a> KeyAttrs<'a> {
    fn key(&self, kind: KeyType, bits: i32) -> NewKey<'a> {
        NewKey {
            kind,
            algo: self.algo,
            hash: self.hash,
            curve: self.curve,
            bits,
            extractable: self.extractable,
            usages: self.usages,
            raw: None,
            pkey: None,
        }
    }

    fn with_pkey(&self, kind: KeyType, pkey: Pkey) -> NewKey<'a> {
        let mut key = self.key(kind, pkey.as_ref().bits());
        key.pkey = Some(pkey);
        key
    }
}

pub type Result<T> = core::result::Result<T, String>;

fn plain<T>(message: &str) -> Result<T> {
    Err(message.to_owned())
}

fn failed<T>(prefix: &str) -> Result<T> {
    Err(openssl::error_message(prefix))
}

pub fn eq_caseless(a: &[u8], b: &str) -> bool {
    a.eq_ignore_ascii_case(b.as_bytes())
}

type MdCtor = fn() -> Option<Md>;

fn md(hash: Option<&[u8]>) -> Option<Md> {
    let hash = hash?;
    let table: [(&str, MdCtor); 7] = [
        ("SHA-1", Md::sha1),
        ("SHA-256", Md::sha256),
        ("SHA-384", Md::sha384),
        ("SHA-512", Md::sha512),
        ("SHA3-256", Md::sha3_256),
        ("SHA3-384", Md::sha3_384),
        ("SHA3-512", Md::sha3_512),
    ];
    table
        .into_iter()
        .find(|(web, _)| eq_caseless(hash, web))
        .and_then(|(_, get)| get())
}

pub fn md_name(hash: &[u8]) -> Option<&'static CStr> {
    [
        ("SHA-1", c"SHA1"),
        ("SHA-256", c"SHA256"),
        ("SHA-384", c"SHA384"),
        ("SHA-512", c"SHA512"),
    ]
    .into_iter()
    .find(|(web, _)| eq_caseless(hash, web))
    .map(|(_, ossl)| ossl)
}

pub fn curve_group(curve: &[u8]) -> Option<&'static CStr> {
    [
        ("P-256", c"prime256v1"),
        ("P-384", c"secp384r1"),
        ("P-521", c"secp521r1"),
    ]
    .into_iter()
    .find(|(web, _)| eq_caseless(curve, web))
    .map(|(_, group)| group)
}

pub fn curve_order_bytes(curve: &[u8]) -> usize {
    [("P-256", 32), ("P-384", 48), ("P-521", 66)]
        .into_iter()
        .find(|(web, _)| eq_caseless(curve, web))
        .map_or(0, |(_, order)| order)
}

pub fn okp_name(algo: &[u8]) -> Option<&'static CStr> {
    [("Ed25519", c"ED25519"), ("X25519", c"X25519")]
        .into_iter()
        .find(|(web, _)| eq_caseless(algo, web))
        .map(|(_, ossl)| ossl)
}

fn opt_okp_name(algo: Option<&[u8]>) -> Option<&'static CStr> {
    algo.and_then(okp_name)
}

pub(crate) fn opt_curve_order(curve: Option<&[u8]>) -> usize {
    curve.map_or(0, curve_order_bytes)
}

pub fn is_symmetric(algo: &[u8]) -> bool {
    eq_caseless(algo, "HMAC")
        || algo
            .get(..3)
            .is_some_and(|prefix| eq_caseless(prefix, "AES"))
        || eq_caseless(algo, "PBKDF2")
        || eq_caseless(algo, "HKDF")
}

pub fn is_aes(algo: &[u8]) -> bool {
    algo.get(..4)
        .is_some_and(|prefix| eq_caseless(prefix, "AES-"))
}

pub fn valid_gcm_tag_bits(bits: i32) -> bool {
    matches!(bits, 32 | 64 | 96 | 104 | 112 | 120 | 128)
}

pub fn ctr_blocks_before_wrap(counter: &[u8; 16], counter_bits: i32) -> Option<u64> {
    let low = counter[8..]
        .iter()
        .fold(0u64, |acc, &byte| (acc << 8) | u64::from(byte));
    if counter_bits >= 64 {
        let all_high_set =
            (64..counter_bits).all(|bit| (counter[15 - (bit / 8) as usize] >> (bit % 8)) & 1 != 0);
        if !all_high_set {
            return None;
        }
        return Some(if low == 0 {
            u64::MAX
        } else {
            (!low).wrapping_add(1)
        });
    }
    let mask = (1u64 << counter_bits) - 1;
    Some(mask - (low & mask) + 1)
}

pub fn ctr_restart(counter: &[u8; 16], counter_bits: i32) -> [u8; 16] {
    let mut restart = *counter;
    for bit in 0..counter_bits {
        restart[15 - (bit / 8) as usize] &= !(1u8 << (bit % 8));
    }
    restart
}

pub fn digest(hash: Option<&[u8]>, data: &[u8]) -> Option<Vec<u8>> {
    openssl::digest(md(hash)?, data)
}

pub fn generate_secret<'a>(attrs: &KeyAttrs<'a>, length_bits: i32) -> Result<NewKey<'a>> {
    if length_bits <= 0 || length_bits % 8 != 0 || length_bits > 4096 {
        return plain("OperationError: invalid key length");
    }
    let mut buf = vec![0u8; length_bits as usize / 8];
    if !openssl::rand_bytes(&mut buf) {
        openssl::cleanse(&mut buf);
        return failed("OperationError: RNG failure");
    }
    let mut key = attrs.key(KeyType::Secret, length_bits);
    key.curve = None;
    key.raw = Some(buf);
    Ok(key)
}

pub fn generate_keypair<'a>(
    attrs: &KeyAttrs<'a>,
    modulus_bits: i32,
    pubexp: u32,
) -> Result<(NewKey<'a>, NewKey<'a>)> {
    let algo = attrs.algo.unwrap_or_default();
    let is_ec = eq_caseless(algo, "ECDSA") || eq_caseless(algo, "ECDH");
    let pkey = if let Some(okp) = okp_name(algo) {
        match openssl::keygen_named(okp) {
            Some(pkey) => pkey,
            None => return failed("OperationError: keygen"),
        }
    } else if is_ec {
        let Some(group) = attrs.curve.and_then(curve_group) else {
            return plain("NotSupportedError: curve");
        };
        match openssl::keygen_ec(group) {
            Some(pkey) => pkey,
            None => return failed("OperationError: EC keygen"),
        }
    } else {
        if !(256..=16384).contains(&modulus_bits) {
            return plain("OperationError: invalid modulus length");
        }
        let exponent = if pubexp != 0 { pubexp } else { 65537 };
        match openssl::keygen_rsa(modulus_bits, exponent) {
            Some(pkey) => pkey,
            None => return failed("OperationError: RSA keygen"),
        }
    };
    let bits = pkey.as_ref().bits();
    let Some(private_pkey) = pkey.as_ref().dup() else {
        return failed("OperationError: key duplication");
    };
    let mut public = attrs.key(KeyType::Public, bits);
    public.extractable = true;
    public.pkey = Some(pkey);
    let mut private = attrs.key(KeyType::Private, bits);
    private.pkey = Some(private_pkey);
    Ok((public, private))
}

pub fn import_raw<'a>(
    format: Option<&[u8]>,
    data: &[u8],
    attrs: &KeyAttrs<'a>,
) -> Result<NewKey<'a>> {
    let algo = attrs.algo.unwrap_or_default();
    let raw = format == Some(b"raw");
    if raw && is_symmetric(algo) {
        let aes = algo
            .get(..3)
            .is_some_and(|prefix| eq_caseless(prefix, "AES"));
        if aes && !matches!(data.len(), 16 | 24 | 32) {
            return plain("DataError: invalid AES key length");
        }
        let mut key = attrs.key(KeyType::Secret, data.len() as i32 * 8);
        key.curve = None;
        key.raw = Some(data.to_vec());
        return Ok(key);
    }
    if raw && (eq_caseless(algo, "ECDSA") || eq_caseless(algo, "ECDH")) {
        let Some(group) = attrs.curve.and_then(curve_group) else {
            return plain("NotSupportedError: curve");
        };
        let params = [Param::Utf8(c"group", group), Param::Octets(c"pub", data)];
        return match openssl::pkey_fromdata(c"EC", Selection::Public, &params) {
            Some(pkey) => Ok(attrs.with_pkey(KeyType::Public, pkey)),
            None => failed("DataError: EC raw import"),
        };
    }
    if raw && let Some(okp) = okp_name(algo) {
        let Some(pkey) = Pkey::raw_public(okp, data) else {
            return failed("DataError: raw import");
        };
        let mut key = attrs.with_pkey(KeyType::Public, pkey);
        key.curve = None;
        return Ok(key);
    }
    let (pkey, kind) = match format {
        Some(b"spki") => (Pkey::from_spki(data), KeyType::Public),
        Some(b"pkcs8") => (Pkey::from_pkcs8(data), KeyType::Private),
        _ => (None, KeyType::Public),
    };
    match pkey {
        Some(pkey) => Ok(attrs.with_pkey(kind, pkey)),
        None => failed("DataError: key import"),
    }
}

pub struct RsaJwk<'a> {
    pub n: Option<&'a [u8]>,
    pub e: Option<&'a [u8]>,
    pub d: Option<&'a [u8]>,
    pub p: Option<&'a [u8]>,
    pub q: Option<&'a [u8]>,
    pub dp: Option<&'a [u8]>,
    pub dq: Option<&'a [u8]>,
    pub qi: Option<&'a [u8]>,
}

fn present(bytes: Option<&[u8]>) -> bool {
    bytes.is_some_and(|bytes| !bytes.is_empty())
}

pub fn import_rsa_jwk<'a>(jwk: &RsaJwk<'_>, attrs: &KeyAttrs<'a>) -> Result<NewKey<'a>> {
    let private = present(jwk.d);
    let mut params = vec![Param::Bn(c"n", jwk.n), Param::Bn(c"e", jwk.e)];
    if private {
        params.push(Param::Bn(c"d", jwk.d));
        if jwk.p.is_some() && jwk.q.is_some() {
            params.push(Param::Bn(c"rsa-factor1", jwk.p));
            params.push(Param::Bn(c"rsa-factor2", jwk.q));
            if jwk.dp.is_some() && jwk.dq.is_some() && jwk.qi.is_some() {
                params.push(Param::Bn(c"rsa-exponent1", jwk.dp));
                params.push(Param::Bn(c"rsa-exponent2", jwk.dq));
                params.push(Param::Bn(c"rsa-coefficient1", jwk.qi));
            }
        }
    }
    let selection = if private {
        Selection::Keypair
    } else {
        Selection::Public
    };
    match openssl::pkey_fromdata(c"RSA", selection, &params) {
        Some(pkey) => {
            let kind = if private {
                KeyType::Private
            } else {
                KeyType::Public
            };
            let mut key = attrs.with_pkey(kind, pkey);
            key.curve = None;
            Ok(key)
        }
        None => failed("DataError: RSA JWK import"),
    }
}

pub fn import_ec_jwk<'a>(
    x: Option<&[u8]>,
    y: Option<&[u8]>,
    d: Option<&[u8]>,
    attrs: &KeyAttrs<'a>,
) -> Result<NewKey<'a>> {
    let group = attrs.curve.and_then(curve_group);
    let order = opt_curve_order(attrs.curve);
    let (Some(group), Some(x), Some(y)) = (group, x, y) else {
        return plain("DataError: EC JWK");
    };
    if order == 0 {
        return plain("DataError: EC JWK");
    }
    if x.len() > order || y.len() > order {
        return plain("DataError: EC JWK coordinate too large");
    }
    let mut point = vec![0u8; 1 + 2 * order];
    point[0] = 0x04;
    point[1 + order - x.len()..1 + order].copy_from_slice(x);
    point[1 + 2 * order - y.len()..].copy_from_slice(y);
    let private = present(d);
    let mut params = vec![Param::Utf8(c"group", group), Param::Octets(c"pub", &point)];
    if private {
        params.push(Param::Bn(c"priv", d));
    }
    let selection = if private {
        Selection::Keypair
    } else {
        Selection::Public
    };
    match openssl::pkey_fromdata(c"EC", selection, &params) {
        Some(pkey) => {
            let kind = if private {
                KeyType::Private
            } else {
                KeyType::Public
            };
            let mut key = attrs.with_pkey(kind, pkey);
            key.hash = None;
            Ok(key)
        }
        None => failed("DataError: EC JWK import"),
    }
}

pub fn import_okp_jwk<'a>(
    x: Option<&[u8]>,
    d: Option<&[u8]>,
    attrs: &KeyAttrs<'a>,
) -> Result<NewKey<'a>> {
    let okp = opt_okp_name(attrs.curve);
    let private = present(d);
    let Some(okp) = okp.filter(|_| private || present(x)) else {
        return plain("DataError: OKP JWK");
    };
    let pkey = if private {
        Pkey::raw_private(okp, d.unwrap_or_default())
    } else {
        Pkey::raw_public(okp, x.unwrap_or_default())
    };
    let Some(pkey) = pkey else {
        return failed("DataError: OKP JWK import");
    };
    let kind = if private {
        KeyType::Private
    } else {
        KeyType::Public
    };
    let mut key = attrs.with_pkey(kind, pkey);
    key.hash = None;
    key.curve = None;
    Ok(key)
}

pub struct OkpJwk {
    pub x: Vec<u8>,
    pub d: Option<Vec<u8>>,
}

pub fn export_okp_jwk(key: &Key<'_>) -> Result<OkpJwk> {
    let Some(pkey) = key.pkey.filter(|_| okp_name(key.algo).is_some()) else {
        return plain("OperationError: OKP export");
    };
    let Some(x) = pkey.raw_public_key() else {
        return failed("OperationError: OKP export");
    };
    if key.kind != KeyType::Private {
        return Ok(OkpJwk { x, d: None });
    }
    match pkey.raw_private_key() {
        Some(d) => Ok(OkpJwk { x, d: Some(d) }),
        None => failed("OperationError: OKP export"),
    }
}

pub fn export_raw(format: Option<&[u8]>, key: &Key<'_>) -> Result<Vec<u8>> {
    let raw = format == Some(b"raw");
    if raw && let Some(bytes) = key.raw {
        return Ok(bytes.to_vec());
    }
    let Some(pkey) = key.pkey else {
        return plain("NotSupportedError: export");
    };
    if raw && okp_name(key.algo).is_some() {
        return pkey
            .raw_public_key()
            .map_or_else(|| failed("OperationError: raw export"), Ok);
    }
    if raw && (key.algo == b"ECDSA" || key.algo == b"ECDH") {
        return match pkey.encoded_public_key() {
            Some(bytes) if !bytes.is_empty() => Ok(bytes),
            _ => failed("OperationError: raw export"),
        };
    }
    match format {
        Some(b"spki") => pkey
            .spki()
            .map_or_else(|| failed("OperationError: spki export"), Ok),
        Some(b"pkcs8") => pkey
            .pkcs8()
            .map_or_else(|| failed("OperationError: pkcs8 export"), Ok),
        _ => plain("NotSupportedError: export format"),
    }
}

pub struct RsaJwkExport {
    pub n: Option<Vec<u8>>,
    pub e: Option<Vec<u8>>,
    pub d: Option<Vec<u8>>,
    pub p: Option<Vec<u8>>,
    pub q: Option<Vec<u8>>,
    pub dp: Option<Vec<u8>>,
    pub dq: Option<Vec<u8>>,
    pub qi: Option<Vec<u8>>,
}

pub fn export_rsa_jwk(key: &Key<'_>) -> Result<RsaJwkExport> {
    let Some(pkey) = key.pkey else {
        return plain("export");
    };
    let private = key.kind == KeyType::Private;
    let secret = |name: &CStr| {
        if private { pkey.bn_param(name) } else { None }
    };
    let jwk = RsaJwkExport {
        n: pkey.bn_param(c"n"),
        e: pkey.bn_param(c"e"),
        d: secret(c"d"),
        p: secret(c"rsa-factor1"),
        q: secret(c"rsa-factor2"),
        dp: secret(c"rsa-exponent1"),
        dq: secret(c"rsa-exponent2"),
        qi: secret(c"rsa-coefficient1"),
    };
    if jwk.n.is_some() && jwk.e.is_some() {
        Ok(jwk)
    } else {
        plain("export")
    }
}

pub struct EcJwk {
    pub x: Vec<u8>,
    pub y: Vec<u8>,
    pub d: Option<Vec<u8>>,
}

pub fn export_ec_jwk(key: &Key<'_>) -> Result<EcJwk> {
    let Some(pkey) = key.pkey else {
        return plain("export");
    };
    let order = opt_curve_order(key.curve);
    if order == 0 {
        return plain("OperationError: EC export");
    }
    let encoded = pkey.encoded_public_key();
    let Some(point) = encoded.filter(|enc| enc.len() == 1 + 2 * order && enc[0] == 0x04) else {
        return plain("OperationError: EC export");
    };
    let x = point[1..1 + order].to_vec();
    let y = point[1 + order..].to_vec();
    if key.kind != KeyType::Private {
        return Ok(EcJwk { x, y, d: None });
    }
    let Some(mut raw) = pkey.bn_param(c"priv") else {
        return plain("OperationError: EC export");
    };
    if raw.len() < order {
        let mut padded = vec![0u8; order];
        padded[order - raw.len()..].copy_from_slice(&raw);
        openssl::cleanse(&mut raw);
        raw = padded;
    }
    Ok(EcJwk { x, y, d: Some(raw) })
}

fn hmac(key: &Key<'_>, data: &[u8]) -> Result<Vec<u8>> {
    let Some(name) = key.hash.and_then(md_name) else {
        return plain("NotSupportedError: hash");
    };
    openssl::hmac(name, key.raw, data).map_or_else(|| failed("OperationError: HMAC"), Ok)
}

fn padding(key: &Key<'_>, params: &Params<'_>) -> Padding {
    match key.algo {
        b"RSA-PSS" => Padding::Pss(if params.pss_salt_len >= 0 {
            Some(params.pss_salt_len)
        } else {
            md(params.sign_hash.or(key.hash)).map(Md::size)
        }),
        b"RSASSA-PKCS1-v1_5" => Padding::Pkcs1,
        _ => Padding::Default,
    }
}

pub fn sign(key: &Key<'_>, params: &Params<'_>, data: &[u8]) -> Result<Vec<u8>> {
    if key.algo == b"HMAC" {
        return hmac(key, data);
    }
    if key.algo == b"Ed25519" {
        let Some(pkey) = key.pkey else {
            return plain("NotSupportedError: sign");
        };
        return openssl::digest_sign(pkey, None, &Padding::Default, data)
            .map_or_else(|| failed("OperationError: sign"), Ok);
    }
    let digest = md(params.sign_hash.or(key.hash));
    let (Some(digest), Some(pkey)) = (digest, key.pkey) else {
        return plain("NotSupportedError: sign");
    };
    let Some(mut der) = openssl::digest_sign(pkey, Some(digest), &padding(key, params), data)
    else {
        return failed("OperationError: sign");
    };
    if key.algo != b"ECDSA" {
        return Ok(der);
    }
    let order = opt_curve_order(key.curve);
    if order == 0 {
        return failed("OperationError: unsupported ECDSA curve");
    }
    let raw = openssl::ecdsa_der_to_raw(&der, order);
    der.clear();
    raw.map_or_else(|| failed("OperationError: ECDSA encode"), Ok)
}

pub fn verify(key: &Key<'_>, params: &Params<'_>, sig: &[u8], data: &[u8]) -> Result<bool> {
    if key.algo == b"HMAC" {
        let mut mac = hmac(key, data)?;
        let ok = openssl::constant_time_eq(&mac, sig);
        openssl::cleanse(&mut mac);
        return Ok(ok);
    }
    if key.algo == b"Ed25519" {
        let Some(pkey) = key.pkey else {
            return plain("NotSupportedError: verify");
        };
        return openssl::digest_verify(pkey, None, &Padding::Default, sig, data)
            .map_or_else(|| failed("OperationError: verify"), Ok);
    }
    let digest = md(params.sign_hash.or(key.hash));
    let (Some(digest), Some(pkey)) = (digest, key.pkey) else {
        return plain("NotSupportedError: verify");
    };
    let der;
    let sig = if key.algo == b"ECDSA" {
        let order = opt_curve_order(key.curve);
        if order == 0 {
            return plain("OperationError: unsupported ECDSA curve");
        }
        let Some(converted) = openssl::ecdsa_raw_to_der(sig, order) else {
            return plain("OperationError: ECDSA decode");
        };
        der = converted;
        &der[..]
    } else {
        sig
    };
    openssl::digest_verify(pkey, Some(digest), &padding(key, params), sig, data)
        .map_or_else(|| failed("OperationError: verify"), Ok)
}

fn aes_mode(algo: &[u8]) -> Option<AesMode> {
    match algo {
        b"AES-GCM" => Some(AesMode::Gcm),
        b"AES-CBC" => Some(AesMode::Cbc),
        b"AES-CTR" => Some(AesMode::Ctr),
        b"AES-KW" => Some(AesMode::Kw),
        _ => None,
    }
}

const SLACK: usize = 32;

fn aes_ctr(key: &[u8], cipher: Cipher, params: &Params<'_>, data: &[u8]) -> Result<Vec<u8>> {
    let bits = params.counter_bits;
    if !(1..=128).contains(&bits) {
        return failed("OperationError: invalid AES-CTR length");
    }
    let mut counter = [0u8; 16];
    counter.copy_from_slice(params.iv.unwrap_or_default());
    let blocks = (data.len() as u64).div_ceil(16);
    let before_wrap = if bits < 128 {
        ctr_blocks_before_wrap(&counter, bits)
    } else {
        None
    };
    let wraps = before_wrap.is_some_and(|before| blocks > before);
    if wraps && bits < 64 && blocks > (1u64 << bits) {
        return failed("OperationError: AES-CTR counter would repeat");
    }
    let head = match before_wrap {
        Some(before) if wraps => before as usize * 16,
        _ => data.len(),
    };
    let mut out = vec![0u8; data.len() + SLACK];
    let mut ok = data.len() <= i32::MAX as usize
        && openssl::aes_ctr_pass(cipher, key, &counter, &data[..head], &mut out);
    if ok && wraps {
        let restart = ctr_restart(&counter, bits);
        ok = openssl::aes_ctr_pass(cipher, key, &restart, &data[head..], &mut out[head..]);
    }
    if !ok {
        openssl::cleanse(&mut out);
        return failed("OperationError: AES-CTR");
    }
    out.truncate(data.len());
    Ok(out)
}

fn aes(key: &Key<'_>, params: &Params<'_>, data: &[u8], encrypt: bool) -> Result<Vec<u8>> {
    let mode = aes_mode(key.algo);
    let cipher = mode.and_then(|mode| Cipher::aes(mode, key.bits));
    let (Some(cipher), Some(secret)) = (cipher, key.raw) else {
        return plain("NotSupportedError: AES");
    };
    let gcm = key.algo == b"AES-GCM";
    let kw = key.algo == b"AES-KW";
    let iv_len = params.iv.map_or(0, <[u8]>::len);
    if gcm {
        if iv_len == 0 {
            return plain("OperationError: invalid AES-GCM IV");
        }
    } else if kw {
        if !data.len().is_multiple_of(8) || data.len() < if encrypt { 16 } else { 24 } {
            return plain("OperationError: invalid AES-KW length");
        }
    } else {
        if params.iv.is_none() || iv_len != 16 {
            return plain("OperationError: invalid AES IV length");
        }
        if key.algo == b"AES-CTR" {
            return aes_ctr(secret, cipher, params, data);
        }
    }
    let tag_bits = if gcm {
        if params.tag_bits > 0 {
            params.tag_bits
        } else {
            128
        }
    } else {
        0
    };
    if gcm && !valid_gcm_tag_bits(tag_bits) {
        return plain("OperationError: invalid AES-GCM tag length");
    }
    let tag_len = tag_bits as usize / 8;
    let fail = || {
        failed(if encrypt {
            "OperationError: encrypt"
        } else {
            "OperationError: decrypt"
        })
    };
    let aad_len = params.aad.map_or(0, <[u8]>::len);
    let limit = i32::MAX as usize;
    let Some(ctx) = CipherCtx::new() else {
        return fail();
    };
    if data.len() > limit - 64 || aad_len > limit || iv_len > limit {
        return fail();
    }
    let mut out = vec![0u8; data.len() + SLACK + tag_len];
    let produced = (|| {
        if kw {
            ctx.allow_wrap();
        }
        if !ctx.init_cipher(cipher, encrypt) || (gcm && !ctx.set_iv_len(iv_len)) {
            return None;
        }
        if !ctx.init_key(secret, if kw { None } else { params.iv }, encrypt) {
            return None;
        }
        let mut ciphertext = data;
        let mut tag = &[][..];
        if gcm && !encrypt {
            let body = ciphertext.len().checked_sub(tag_len)?;
            (ciphertext, tag) = ciphertext.split_at(body);
        }
        if gcm && aad_len > 0 && !ctx.aad(params.aad.unwrap_or_default()) {
            return None;
        }
        let mut produced = 0;
        if !ciphertext.is_empty() {
            produced = ctx.update(&mut out, ciphertext)?;
        }
        if gcm && !encrypt && !ctx.set_tag(tag) {
            return None;
        }
        produced += ctx.finish(&mut out[produced..])?;
        if gcm && encrypt {
            if !ctx.get_tag(&mut out[produced..produced + tag_len]) {
                return None;
            }
            produced += tag_len;
        }
        Some(produced)
    })();
    match produced {
        Some(produced) => {
            openssl::cleanse(&mut out[produced..]);
            out.truncate(produced);
            Ok(out)
        }
        None => {
            openssl::cleanse(&mut out);
            fail()
        }
    }
}

fn rsa_oaep(key: &Key<'_>, params: &Params<'_>, data: &[u8], encrypt: bool) -> Result<Vec<u8>> {
    let (Some(digest), Some(pkey)) = (md(key.hash), key.pkey) else {
        return plain("NotSupportedError: RSA-OAEP");
    };
    openssl::rsa_oaep(pkey, digest, params.label, data, encrypt).map_or_else(
        || {
            failed(if encrypt {
                "OperationError: encrypt"
            } else {
                "OperationError: decrypt"
            })
        },
        Ok,
    )
}

pub fn encrypt(key: &Key<'_>, params: &Params<'_>, data: &[u8]) -> Result<Vec<u8>> {
    if is_aes(key.algo) {
        return aes(key, params, data, true);
    }
    if key.algo == b"RSA-OAEP" {
        return rsa_oaep(key, params, data, true);
    }
    plain("NotSupportedError: encrypt")
}

pub fn decrypt(key: &Key<'_>, params: &Params<'_>, data: &[u8]) -> Result<Vec<u8>> {
    if is_aes(key.algo) {
        return aes(key, params, data, false);
    }
    if key.algo == b"RSA-OAEP" {
        return rsa_oaep(key, params, data, false);
    }
    plain("NotSupportedError: decrypt")
}

fn ecdh(key: &Key<'_>, params: &Params<'_>, length_bits: i32) -> Result<Vec<u8>> {
    let peer = params.peer.as_ref().and_then(|peer| peer.pkey);
    let (Some(pkey), Some(peer)) = (key.pkey, peer) else {
        return plain("InvalidAccessError: ECDH");
    };
    let Some(mut out) = openssl::derive(pkey, peer) else {
        return failed("OperationError: ECDH");
    };
    if length_bits > 0 {
        let want = length_bits as usize / 8;
        if want > out.len() {
            openssl::cleanse(&mut out);
            return failed("OperationError: ECDH length");
        }
        openssl::cleanse(&mut out[want..]);
        out.truncate(want);
    }
    Ok(out)
}

fn pbkdf2(key: &Key<'_>, params: &Params<'_>, length_bits: i32) -> Result<Vec<u8>> {
    let digest = md(params.kdf_hash);
    let Some(digest) = digest.filter(|_| length_bits > 0) else {
        return plain("OperationError: PBKDF2");
    };
    if params.iterations <= 0 {
        return plain("OperationError: PBKDF2 iterations must be positive");
    }
    let limit = i32::MAX as usize;
    if key.raw.map_or(0, <[u8]>::len) > limit || params.salt.map_or(0, <[u8]>::len) > limit {
        return plain("OperationError: PBKDF2 input too large");
    }
    let mut out = vec![0u8; length_bits as usize / 8];
    if !openssl::pbkdf2(key.raw, params.salt, params.iterations, digest, &mut out) {
        openssl::cleanse(&mut out);
        return failed("OperationError: PBKDF2");
    }
    Ok(out)
}

fn hkdf(key: &Key<'_>, params: &Params<'_>, length_bits: i32) -> Result<Vec<u8>> {
    let name = params.kdf_hash.and_then(md_name);
    let Some(name) = name.filter(|_| length_bits > 0) else {
        return plain("OperationError: HKDF");
    };
    let mut out = vec![0u8; length_bits as usize / 8];
    let salt = params.salt.unwrap_or_default();
    let info = params.info.unwrap_or_default();
    if !openssl::hkdf(name, key.raw, salt, info, &mut out) {
        openssl::cleanse(&mut out);
        return failed("OperationError: HKDF");
    }
    Ok(out)
}

pub fn derive_bits(key: &Key<'_>, params: &Params<'_>, length_bits: i32) -> Result<Vec<u8>> {
    if length_bits > 1 << 20 {
        return plain("OperationError: deriveBits length too large");
    }
    match key.algo {
        b"ECDH" | b"X25519" => ecdh(key, params, length_bits),
        b"PBKDF2" => pbkdf2(key, params, length_bits),
        b"HKDF" => hkdf(key, params, length_bits),
        _ => plain("NotSupportedError: deriveBits"),
    }
}
