//! Southstar — normalizing a SubtleCrypto algorithm argument and key usage lists.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::rc::Rc;

use southstar_js_engine::{Scope, Value};
use southstar_webcrypto::{Params, StoredKey};

use crate::get;

pub(crate) const USAGE_ENCRYPT: u32 = 1 << 0;
pub(crate) const USAGE_DECRYPT: u32 = 1 << 1;
pub(crate) const USAGE_SIGN: u32 = 1 << 2;
pub(crate) const USAGE_VERIFY: u32 = 1 << 3;
pub(crate) const USAGE_DERIVE_KEY: u32 = 1 << 4;
pub(crate) const USAGE_DERIVE_BITS: u32 = 1 << 5;
pub(crate) const USAGE_WRAP: u32 = 1 << 6;
pub(crate) const USAGE_UNWRAP: u32 = 1 << 7;

const USAGE_NAMES: [(u32, &str); 8] = [
    (USAGE_ENCRYPT, "encrypt"),
    (USAGE_DECRYPT, "decrypt"),
    (USAGE_SIGN, "sign"),
    (USAGE_VERIFY, "verify"),
    (USAGE_DERIVE_KEY, "deriveKey"),
    (USAGE_DERIVE_BITS, "deriveBits"),
    (USAGE_WRAP, "wrapKey"),
    (USAGE_UNWRAP, "unwrapKey"),
];

const CANONICAL_NAMES: [&str; 21] = [
    "RSASSA-PKCS1-v1_5",
    "RSA-PSS",
    "RSA-OAEP",
    "AES-GCM",
    "AES-CBC",
    "AES-CTR",
    "AES-KW",
    "HMAC",
    "ECDSA",
    "ECDH",
    "PBKDF2",
    "HKDF",
    "Ed25519",
    "X25519",
    "SHA-1",
    "SHA-256",
    "SHA-384",
    "SHA-512",
    "P-256",
    "P-384",
    "P-521",
];

pub(crate) fn canonical(name: &str) -> Option<&'static str> {
    CANONICAL_NAMES
        .into_iter()
        .find(|known| known.eq_ignore_ascii_case(name))
}

pub(crate) fn hmac_default_bits(hash: Option<&str>) -> i32 {
    match hash {
        Some("SHA-384" | "SHA-512") => 1024,
        _ => 512,
    }
}

pub(crate) struct Algorithm {
    pub(crate) name: &'static str,
    pub(crate) hash: Option<&'static str>,
    pub(crate) curve: Option<&'static str>,
    pub(crate) modulus_bits: i32,
    pub(crate) pubexp: u32,
    pub(crate) length: i32,
    pub(crate) salt_len_pss: i32,
    pub(crate) iterations: i32,
    pub(crate) tag_bits: i32,
    pub(crate) iv: Option<Vec<u8>>,
    pub(crate) aad: Option<Vec<u8>>,
    pub(crate) label: Option<Vec<u8>>,
    pub(crate) salt: Option<Vec<u8>>,
    pub(crate) info: Option<Vec<u8>>,
    pub(crate) counter: Option<Vec<u8>>,
    pub(crate) peer: Option<Rc<StoredKey>>,
}

impl Algorithm {
    fn named(name: &'static str) -> Algorithm {
        Algorithm {
            name,
            hash: None,
            curve: None,
            modulus_bits: 0,
            pubexp: 0,
            length: 0,
            salt_len_pss: -1,
            iterations: 0,
            tag_bits: 128,
            iv: None,
            aad: None,
            label: None,
            salt: None,
            info: None,
            counter: None,
            peer: None,
        }
    }

    pub(crate) fn cipher_params(&self) -> Params<'_> {
        let mut params = empty_params();
        params.iv = self.iv.as_deref();
        params.aad = self.aad.as_deref();
        params.tag_bits = self.tag_bits;
        params.label = self.label.as_deref();
        if self.name == "AES-CTR" {
            params.iv = self.counter.as_deref();
            params.counter_bits = self.length;
        }
        params
    }

    pub(crate) fn sign_params(&self) -> Params<'_> {
        let mut params = empty_params();
        params.sign_hash = self.hash.map(str::as_bytes);
        params.pss_salt_len = self.salt_len_pss;
        params
    }

    pub(crate) fn derive_params(&self) -> Params<'_> {
        let mut params = empty_params();
        params.peer = self.peer.as_deref().map(StoredKey::view);
        params.salt = self.salt.as_deref();
        params.info = self.info.as_deref();
        params.iterations = self.iterations;
        params.kdf_hash = self.hash.map(str::as_bytes);
        params
    }
}

fn empty_params<'a>() -> Params<'a> {
    Params {
        iv: None,
        aad: None,
        tag_bits: 0,
        label: None,
        salt: None,
        info: None,
        iterations: 0,
        kdf_hash: None,
        sign_hash: None,
        peer: None,
        counter_bits: 0,
        pss_salt_len: 0,
    }
}

pub(crate) fn string_prop(scope: &mut Scope<'_>, object: &Value, key: &str) -> Option<String> {
    let value = get(scope, object, key);
    if value.is_string() {
        scope.to_string(&value).ok()
    } else {
        None
    }
}

fn int_prop(scope: &mut Scope<'_>, object: &Value, key: &str, fallback: i32) -> i32 {
    let value = get(scope, object, key);
    if value.is_number() {
        scope.to_int32(&value).unwrap_or(0)
    } else {
        fallback
    }
}

pub(crate) fn buffer_bytes(scope: &mut Scope<'_>, value: &Value) -> Option<Vec<u8>> {
    scope.buffer_source_bytes(value)
}

fn buffer_prop(scope: &mut Scope<'_>, object: &Value, key: &str) -> Option<Vec<u8>> {
    let value = get(scope, object, key);
    if value.is_undefined() || value.is_null() {
        None
    } else {
        buffer_bytes(scope, &value)
    }
}

fn canonical_value(scope: &mut Scope<'_>, value: &Value) -> Option<&'static str> {
    scope
        .to_string(value)
        .ok()
        .and_then(|name| canonical(&name))
}

pub(crate) fn parse(scope: &mut Scope<'_>, value: &Value) -> Option<Algorithm> {
    if value.is_string() {
        return canonical_value(scope, value).map(Algorithm::named);
    }
    if !value.is_object() {
        return None;
    }
    let name = get(scope, value, "name");
    let name = if name.is_string() {
        canonical_value(scope, &name)
    } else {
        None
    };
    let mut algorithm = Algorithm::named(name?);

    let hash = get(scope, value, "hash");
    if hash.is_string() {
        algorithm.hash = canonical_value(scope, &hash);
    } else if hash.is_object() {
        algorithm.hash = string_prop(scope, &hash, "name").and_then(|name| canonical(&name));
    }
    algorithm.curve = string_prop(scope, value, "namedCurve").and_then(|name| canonical(&name));

    algorithm.modulus_bits = int_prop(scope, value, "modulusLength", 0);
    algorithm.length = int_prop(scope, value, "length", 0);
    algorithm.iterations = int_prop(scope, value, "iterations", 0);
    algorithm.salt_len_pss = int_prop(scope, value, "saltLength", -1);
    algorithm.tag_bits = int_prop(scope, value, "tagLength", 128);

    let exponent = buffer_prop(scope, value, "publicExponent")
        .unwrap_or_default()
        .iter()
        .fold(0u32, |acc, &b| (acc << 8) | u32::from(b));
    algorithm.pubexp = if exponent != 0 { exponent } else { 65537 };

    algorithm.iv = buffer_prop(scope, value, "iv");
    algorithm.aad = buffer_prop(scope, value, "additionalData");
    algorithm.label = buffer_prop(scope, value, "label");
    algorithm.salt = buffer_prop(scope, value, "salt");
    algorithm.info = buffer_prop(scope, value, "info");
    algorithm.counter = buffer_prop(scope, value, "counter");

    let public = get(scope, value, "public");
    algorithm.peer = crate::key::stored(scope, &public);
    Some(algorithm)
}

pub(crate) fn usages_from_array(scope: &mut Scope<'_>, value: &Value) -> u32 {
    if !value.is_object() {
        return 0;
    }
    let length = get(scope, value, "length");
    let length = scope.to_number(&length).unwrap_or(0.0);
    let length = southstar_js_engine::int64_modulo(length) as u32;
    let mut usages = 0;
    for index in 0..length {
        let entry = scope
            .get_index(value, index)
            .unwrap_or_else(|_| Value::undefined());
        let Ok(name) = scope.to_string(&entry) else {
            continue;
        };
        if let Some((bit, _)) = USAGE_NAMES.iter().find(|(_, known)| *known == name) {
            usages |= bit;
        }
    }
    usages
}

pub(crate) fn usages_to_array(scope: &mut Scope<'_>, usages: u32) -> Value {
    let array = scope.new_array();
    let mut index = 0;
    for (bit, name) in USAGE_NAMES {
        if usages & bit != 0 {
            let name = scope.string(name);
            let _ = scope.set_index(&array, index, name);
            index += 1;
        }
    }
    array
}
