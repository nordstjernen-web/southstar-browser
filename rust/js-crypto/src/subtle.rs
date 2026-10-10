//! Southstar — the crypto.subtle methods: digest, key generation, import, export, sign, verify, encryption, derivation and key wrapping.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::rc::Rc;

use southstar_js_engine::{ObjectKind, Scope, Value};
use southstar_webcrypto::{self as webcrypto, KeyAttrs, StoredKey};

use crate::algorithm::{
    self, Algorithm, USAGE_DECRYPT, USAGE_DERIVE_BITS, USAGE_DERIVE_KEY, USAGE_ENCRYPT, USAGE_SIGN,
    USAGE_UNWRAP, USAGE_VERIFY, USAGE_WRAP, buffer_bytes, hmac_default_bits, usages_from_array,
};
use crate::jwk::{self, KeyData};
use crate::key::{make_key, stored};
use crate::{JsResult, arg, c_string, get, rejection};

struct Capability {
    promise: Value,
    resolve: Value,
    reject: Value,
}

impl Capability {
    fn new(scope: &mut Scope<'_>) -> JsResult<Capability> {
        let (promise, resolve, reject) = scope.new_promise()?;
        Ok(Capability {
            promise,
            resolve,
            reject,
        })
    }

    fn resolve(self, scope: &mut Scope<'_>, value: Value) -> JsResult {
        let _ = scope.call(&self.resolve, &Value::undefined(), &[value]);
        Ok(self.promise)
    }

    fn resolve_bytes(self, scope: &mut Scope<'_>, bytes: &[u8]) -> JsResult {
        match scope.new_array_buffer(bytes) {
            Ok(buffer) => self.resolve(scope, buffer),
            Err(_) => self.reject(scope, "OperationError"),
        }
    }

    fn reject(self, scope: &mut Scope<'_>, message: &str) -> JsResult {
        let error = rejection(scope, message);
        let _ = scope.call(&self.reject, &Value::undefined(), &[error]);
        Ok(self.promise)
    }

    fn settle_bytes(self, scope: &mut Scope<'_>, result: webcrypto::Result<Vec<u8>>) -> JsResult {
        match result {
            Ok(bytes) => self.resolve_bytes(scope, &bytes),
            Err(message) => self.reject(scope, &message),
        }
    }

    fn settle_key(self, scope: &mut Scope<'_>, result: webcrypto::Result<StoredKey>) -> JsResult {
        match result {
            Ok(key) => {
                let key = make_key(scope, key);
                self.resolve(scope, key)
            }
            Err(message) => self.reject(scope, &message),
        }
    }
}

fn data_arg(scope: &mut Scope<'_>, args: &[Value], index: usize) -> Vec<u8> {
    buffer_bytes(scope, &arg(args, index)).unwrap_or_default()
}

fn text_arg(scope: &mut Scope<'_>, args: &[Value], index: usize) -> Option<String> {
    c_string(scope, &arg(args, index))
}

fn op_check(key: &StoredKey, algorithm: &str, need: u32) -> Option<&'static str> {
    if key.usages & need == 0 {
        return Some("InvalidAccessError: key does not permit this operation");
    }
    match key.algo.as_deref() {
        Some(algo) if !algo.eq_ignore_ascii_case(algorithm.as_bytes()) => {
            Some("InvalidAccessError: key algorithm does not match")
        }
        _ => None,
    }
}

fn key_attrs(
    algorithm: &Algorithm,
    curve: Option<&'static str>,
    extractable: bool,
    usages: u32,
) -> KeyAttrs<'static> {
    KeyAttrs {
        algo: Some(algorithm.name.as_bytes()),
        hash: algorithm.hash.map(str::as_bytes),
        curve: curve.map(str::as_bytes),
        extractable,
        usages,
    }
}

fn keyed(scope: &mut Scope<'_>, args: &[Value]) -> Option<(Algorithm, Rc<StoredKey>)> {
    let algorithm = algorithm::parse(scope, &arg(args, 0));
    let key = stored(scope, &arg(args, 1));
    algorithm.zip(key)
}

fn cipher(
    key: &StoredKey,
    algorithm: &Algorithm,
    data: &[u8],
    encrypt: bool,
) -> webcrypto::Result<Vec<u8>> {
    let params = algorithm.cipher_params();
    let view = key.view();
    if encrypt {
        webcrypto::encrypt(&view, &params, data)
    } else {
        webcrypto::decrypt(&view, &params, data)
    }
}

fn derive(key: &StoredKey, algorithm: &Algorithm, length_bits: i32) -> webcrypto::Result<Vec<u8>> {
    webcrypto::derive_bits(&key.view(), &algorithm.derive_params(), length_bits)
}

pub(crate) fn generate_key(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let capability = Capability::new(scope)?;
    if args.len() < 3 {
        return capability.reject(scope, "generateKey: 3 arguments required");
    }
    let Some(algorithm) = algorithm::parse(scope, &args[0]) else {
        return capability.reject(scope, "NotSupportedError: algorithm");
    };
    let extractable = scope.to_bool(&args[1]);
    let usages = usages_from_array(scope, &args[2]);

    if webcrypto::is_symmetric(algorithm.name.as_bytes()) {
        let mut bits = algorithm.length;
        if algorithm.name == "HMAC" && bits <= 0 {
            bits = hmac_default_bits(algorithm.hash);
        }
        let attrs = key_attrs(&algorithm, None, extractable, usages);
        let result = webcrypto::generate_secret(&attrs, bits).map(StoredKey::from);
        return capability.settle_key(scope, result);
    }
    let attrs = key_attrs(&algorithm, algorithm.curve, extractable, usages);
    match webcrypto::generate_keypair(&attrs, algorithm.modulus_bits, algorithm.pubexp) {
        Ok((public, private)) => {
            let mut public = StoredKey::from(public);
            let mut private = StoredKey::from(private);
            public.usages = usages & (USAGE_ENCRYPT | USAGE_VERIFY | USAGE_WRAP);
            private.usages = usages
                & (USAGE_DECRYPT
                    | USAGE_SIGN
                    | USAGE_DERIVE_KEY
                    | USAGE_DERIVE_BITS
                    | USAGE_UNWRAP);
            let pair = scope.new_object();
            let public = make_key(scope, public);
            crate::set(scope, &pair, "publicKey", public);
            let private = make_key(scope, private);
            crate::set(scope, &pair, "privateKey", private);
            capability.resolve(scope, pair)
        }
        Err(message) => capability.reject(scope, &message),
    }
}

pub(crate) fn import_key(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let capability = Capability::new(scope)?;
    if args.len() < 5 {
        return capability.reject(scope, "importKey: 5 arguments required");
    }
    let format = text_arg(scope, args, 0);
    let algorithm = format
        .as_ref()
        .and_then(|_| algorithm::parse(scope, &args[2]));
    let (Some(format), Some(algorithm)) = (format, algorithm) else {
        return capability.reject(scope, "NotSupportedError: algorithm");
    };
    let extractable = scope.to_bool(&args[3]);
    let usages = usages_from_array(scope, &args[4]);
    let data = KeyData::Jwk(args[1].clone());
    let result = jwk::import(scope, &format, data, &algorithm, extractable, usages);
    capability.settle_key(scope, result)
}

pub(crate) fn export_key(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let capability = Capability::new(scope)?;
    if args.len() < 2 {
        return capability.reject(scope, "exportKey: 2 arguments required");
    }
    let format = text_arg(scope, args, 0);
    let key = stored(scope, &args[1]);
    let (Some(format), Some(key)) = (format, key) else {
        return capability.reject(scope, "InvalidAccessError: key");
    };
    if !key.extractable {
        return capability.reject(scope, "InvalidAccessError: not extractable");
    }
    if format == "jwk" {
        return match jwk::export(scope, &key) {
            Ok(object) => capability.resolve(scope, object),
            Err(message) => capability.reject(scope, &message),
        };
    }
    let result = webcrypto::export_raw(Some(format.as_bytes()), &key.view());
    capability.settle_bytes(scope, result)
}

pub(crate) fn sign(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let capability = Capability::new(scope)?;
    if args.len() < 3 {
        return capability.reject(scope, "sign: 3 arguments required");
    }
    let Some((algorithm, key)) = keyed(scope, args) else {
        return capability.reject(scope, "InvalidAccessError: key");
    };
    if let Some(error) = op_check(&key, algorithm.name, USAGE_SIGN) {
        return capability.reject(scope, error);
    }
    let data = data_arg(scope, args, 2);
    let result = webcrypto::sign(&key.view(), &algorithm.sign_params(), &data);
    capability.settle_bytes(scope, result)
}

pub(crate) fn verify(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let capability = Capability::new(scope)?;
    if args.len() < 4 {
        return capability.reject(scope, "verify: 4 arguments required");
    }
    let Some((algorithm, key)) = keyed(scope, args) else {
        return capability.reject(scope, "InvalidAccessError: key");
    };
    if let Some(error) = op_check(&key, algorithm.name, USAGE_VERIFY) {
        return capability.reject(scope, error);
    }
    let signature = data_arg(scope, args, 2);
    let data = data_arg(scope, args, 3);
    match webcrypto::verify(&key.view(), &algorithm.sign_params(), &signature, &data) {
        Ok(valid) => capability.resolve(scope, Value::boolean(valid)),
        Err(message) => capability.reject(scope, &message),
    }
}

fn encrypt_or_decrypt(scope: &mut Scope<'_>, args: &[Value], encrypt: bool) -> JsResult {
    let capability = Capability::new(scope)?;
    if args.len() < 3 {
        return capability.reject(scope, "3 arguments required");
    }
    let Some((algorithm, key)) = keyed(scope, args) else {
        return capability.reject(scope, "InvalidAccessError: key");
    };
    let need = if encrypt {
        USAGE_ENCRYPT
    } else {
        USAGE_DECRYPT
    };
    if let Some(error) = op_check(&key, algorithm.name, need) {
        return capability.reject(scope, error);
    }
    let data = data_arg(scope, args, 2);
    let result = cipher(&key, &algorithm, &data, encrypt);
    capability.settle_bytes(scope, result)
}

pub(crate) fn encrypt(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    encrypt_or_decrypt(scope, args, true)
}

pub(crate) fn decrypt(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    encrypt_or_decrypt(scope, args, false)
}

pub(crate) fn derive_bits(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let capability = Capability::new(scope)?;
    if args.len() < 2 {
        return capability.reject(scope, "deriveBits: 2 arguments required");
    }
    let Some((algorithm, key)) = keyed(scope, args) else {
        return capability.reject(scope, "InvalidAccessError: key");
    };
    let length = arg(args, 2);
    let length = if length.is_number() {
        scope.to_int32(&length).unwrap_or(0)
    } else {
        0
    };
    let result = derive(&key, &algorithm, length);
    capability.settle_bytes(scope, result)
}

pub(crate) fn derive_key(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let capability = Capability::new(scope)?;
    if args.len() < 5 {
        return capability.reject(scope, "deriveKey: 5 arguments required");
    }
    let algorithm = algorithm::parse(scope, &args[0]);
    let derived = algorithm
        .as_ref()
        .and_then(|_| algorithm::parse(scope, &args[2]));
    let key = stored(scope, &args[1]);
    let (Some(algorithm), Some(derived), Some(key)) = (algorithm, derived, key) else {
        return capability.reject(scope, "InvalidAccessError: key");
    };
    let extractable = scope.to_bool(&args[3]);
    let usages = usages_from_array(scope, &args[4]);
    let mut bits = derived.length;
    if bits <= 0 && derived.name == "HMAC" {
        bits = hmac_default_bits(derived.hash);
    }
    if bits <= 0 {
        bits = 256;
    }
    let mut secret = match derive(&key, &algorithm, bits) {
        Ok(secret) => secret,
        Err(message) => return capability.reject(scope, &message),
    };
    let attrs = key_attrs(&derived, None, extractable, usages);
    let result = webcrypto::import_raw(Some(b"raw"), &secret, &attrs).map(StoredKey::from);
    secret.fill(0);
    capability.settle_key(scope, result)
}

pub(crate) fn wrap_key(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let capability = Capability::new(scope)?;
    if args.len() < 4 {
        return capability.reject(scope, "wrapKey: 4 arguments required");
    }
    let format = text_arg(scope, args, 0);
    let key = stored(scope, &args[1]);
    let wrapping = stored(scope, &args[2]);
    let ready = format.is_some() && key.is_some() && wrapping.is_some();
    let algorithm = if ready {
        algorithm::parse(scope, &args[3])
    } else {
        None
    };
    let (Some(format), Some(key), Some(wrapping), Some(algorithm)) =
        (format, key, wrapping, algorithm)
    else {
        return capability.reject(scope, "InvalidAccessError: key");
    };
    let mut error = op_check(&wrapping, algorithm.name, USAGE_WRAP);
    if error.is_none() && !key.extractable {
        error = Some("InvalidAccessError: key is not extractable");
    }
    if let Some(error) = error {
        return capability.reject(scope, error);
    }
    let mut serialized = match jwk::serialize(scope, &format, &key) {
        Ok(bytes) => bytes,
        Err(message) => return capability.reject(scope, &message),
    };
    let result = cipher(&wrapping, &algorithm, &serialized, true);
    serialized.fill(0);
    capability.settle_bytes(scope, result)
}

pub(crate) fn unwrap_key(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let capability = Capability::new(scope)?;
    if args.len() < 7 {
        return capability.reject(scope, "unwrapKey: 7 arguments required");
    }
    let format = text_arg(scope, args, 0);
    let unwrapping = stored(scope, &args[2]);
    let ready = format.is_some() && unwrapping.is_some();
    let unwrap_algorithm = if ready {
        algorithm::parse(scope, &args[3])
    } else {
        None
    };
    let key_algorithm = if unwrap_algorithm.is_some() {
        algorithm::parse(scope, &args[4])
    } else {
        None
    };
    let (Some(format), Some(unwrapping), Some(unwrap_algorithm), Some(key_algorithm)) =
        (format, unwrapping, unwrap_algorithm, key_algorithm)
    else {
        return capability.reject(scope, "InvalidAccessError: key");
    };
    if let Some(error) = op_check(&unwrapping, unwrap_algorithm.name, USAGE_UNWRAP) {
        return capability.reject(scope, error);
    }
    let extractable = scope.to_bool(&args[5]);
    let usages = usages_from_array(scope, &args[6]);
    let wrapped = data_arg(scope, args, 1);
    let mut plain = match cipher(&unwrapping, &unwrap_algorithm, &wrapped, false) {
        Ok(plain) => plain,
        Err(message) => return capability.reject(scope, &message),
    };
    let data = if format == "jwk" {
        jwk::json_key_data(scope, &plain)
    } else {
        Ok(KeyData::Bytes(plain.clone()))
    };
    plain.fill(0);
    let result = data
        .and_then(|data| jwk::import(scope, &format, data, &key_algorithm, extractable, usages));
    capability.settle_key(scope, result)
}

fn digest_name(name: &str) -> Option<&'static str> {
    let names = [
        ("SHA-1", "SHA1"),
        ("SHA-256", "SHA256"),
        ("SHA-384", "SHA384"),
        ("SHA-512", "SHA512"),
        ("SHA3-256", ""),
        ("SHA3-384", ""),
        ("SHA3-512", ""),
    ];
    names
        .into_iter()
        .find(|(web, alias)| {
            name.eq_ignore_ascii_case(web)
                || (!alias.is_empty() && name.eq_ignore_ascii_case(alias))
        })
        .map(|(web, _)| web)
}

pub(crate) fn digest(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let capability = Capability::new(scope)?;
    if args.len() < 2 {
        return capability.reject(scope, "digest: 2 arguments required");
    }
    let name = if args[0].is_string() {
        c_string(scope, &args[0])
    } else if args[0].is_object() {
        let name = get(scope, &args[0], "name");
        if name.is_undefined() {
            return capability.reject(scope, "TypeError: algorithm name is required");
        }
        c_string(scope, &name)
    } else {
        None
    };
    let Some(hash) = name.as_deref().and_then(digest_name) else {
        return capability.reject(scope, "NotSupportedError: unsupported digest algorithm");
    };
    let buffer_like = matches!(
        scope.object_kind(&args[1]),
        Some(ObjectKind::ArrayBuffer | ObjectKind::TypedArray(_) | ObjectKind::DataView)
    );
    if !buffer_like {
        return capability.reject(scope, "digest: data must be ArrayBuffer or typed array");
    }
    let data = buffer_bytes(scope, &args[1]).unwrap_or_default();
    match webcrypto::digest(Some(hash.as_bytes()), &data) {
        Some(digest) => capability.resolve_bytes(scope, &digest),
        None => capability.reject(scope, "OperationError: digest failed"),
    }
}
