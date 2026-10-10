//! Southstar — importing key material from raw, spki, pkcs8 and JSON Web Key forms, and exporting keys as JWK.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};
use southstar_webcrypto::{self as webcrypto, KeyAttrs, RsaJwk, StoredKey};

use crate::algorithm::{Algorithm, canonical, string_prop, usages_to_array};
use crate::key::{is_rsa, text};
use crate::{base64url, set, set_str};

pub(crate) enum KeyData {
    Jwk(Value),
    Bytes(Vec<u8>),
}

fn field(scope: &mut Scope<'_>, jwk: &Value, name: &str) -> Option<Vec<u8>> {
    string_prop(scope, jwk, name).map(|text| base64url::decode(&text))
}

fn attrs<'a>(
    algorithm: &'a Algorithm,
    hash: Option<&'a str>,
    curve: Option<&'a str>,
    extractable: bool,
    usages: u32,
) -> KeyAttrs<'a> {
    KeyAttrs {
        algo: Some(algorithm.name.as_bytes()),
        hash: hash.map(str::as_bytes),
        curve: curve.map(str::as_bytes),
        extractable,
        usages,
    }
}

fn import_jwk(
    scope: &mut Scope<'_>,
    jwk: &Value,
    algorithm: &Algorithm,
    extractable: bool,
    usages: u32,
) -> webcrypto::Result<StoredKey> {
    let kty = string_prop(scope, jwk, "kty");
    let key = match kty.as_deref() {
        Some("oct") => {
            let k = field(scope, jwk, "k").unwrap_or_default();
            let attrs = attrs(algorithm, algorithm.hash, None, extractable, usages);
            webcrypto::import_raw(Some(b"raw"), &k, &attrs)?.into()
        }
        Some("RSA") => {
            let names = ["n", "e", "d", "p", "q", "dp", "dq", "qi"];
            let [n, e, d, p, q, dp, dq, qi] = names.map(|name| field(scope, jwk, name));
            let rsa = RsaJwk {
                n: n.as_deref(),
                e: e.as_deref(),
                d: d.as_deref(),
                p: p.as_deref(),
                q: q.as_deref(),
                dp: dp.as_deref(),
                dq: dq.as_deref(),
                qi: qi.as_deref(),
            };
            let attrs = attrs(algorithm, algorithm.hash, None, extractable, usages);
            webcrypto::import_rsa_jwk(&rsa, &attrs)?.into()
        }
        Some("EC") => {
            let curve = match string_prop(scope, jwk, "crv") {
                Some(crv) => canonical(&crv),
                None => algorithm.curve,
            };
            let [x, y, d] = ["x", "y", "d"].map(|name| field(scope, jwk, name));
            let attrs = attrs(algorithm, None, curve, extractable, usages);
            webcrypto::import_ec_jwk(x.as_deref(), y.as_deref(), d.as_deref(), &attrs)?.into()
        }
        Some("OKP") => {
            let curve = match string_prop(scope, jwk, "crv") {
                Some(crv) => canonical(&crv),
                None => Some(algorithm.name),
            };
            let [x, d] = ["x", "d"].map(|name| field(scope, jwk, name));
            let attrs = attrs(algorithm, None, curve, extractable, usages);
            webcrypto::import_okp_jwk(x.as_deref(), d.as_deref(), &attrs)?.into()
        }
        _ => return Err("DataError: unsupported jwk kty".to_owned()),
    };
    Ok(key)
}

pub(crate) fn import(
    scope: &mut Scope<'_>,
    format: &str,
    data: KeyData,
    algorithm: &Algorithm,
    extractable: bool,
    usages: u32,
) -> webcrypto::Result<StoredKey> {
    if format == "jwk" {
        let jwk = match data {
            KeyData::Jwk(jwk) => jwk,
            KeyData::Bytes(_) => Value::undefined(),
        };
        return import_jwk(scope, &jwk, algorithm, extractable, usages);
    }
    let bytes = match data {
        KeyData::Bytes(bytes) => bytes,
        KeyData::Jwk(value) => crate::algorithm::buffer_bytes(scope, &value).unwrap_or_default(),
    };
    let attrs = attrs(
        algorithm,
        algorithm.hash,
        algorithm.curve,
        extractable,
        usages,
    );
    Ok(webcrypto::import_raw(Some(format.as_bytes()), &bytes, &attrs)?.into())
}

fn put(scope: &mut Scope<'_>, object: &Value, key: &str, bytes: &[u8]) {
    set_str(scope, object, key, &base64url::encode(bytes));
}

fn rsa_alg(algo: Option<&str>, hash: Option<&str>) -> Option<String> {
    let suffix = match hash {
        Some("SHA-1") => Some("1"),
        Some("SHA-256") => Some("256"),
        Some("SHA-384") => Some("384"),
        Some("SHA-512") => Some("512"),
        _ => None,
    };
    match algo {
        Some("RSA-OAEP") if hash == Some("SHA-1") => Some("RSA-OAEP".to_owned()),
        Some("RSA-OAEP") => Some(format!("RSA-OAEP-{}", suffix.unwrap_or("256"))),
        Some("RSASSA-PKCS1-v1_5") => suffix.map(|s| format!("RS{s}")),
        Some("RSA-PSS") => suffix.map(|s| format!("PS{s}")),
        _ => None,
    }
}

pub(crate) fn export(scope: &mut Scope<'_>, key: &StoredKey) -> webcrypto::Result<Value> {
    let object = scope.new_object();
    set(scope, &object, "ext", Value::boolean(key.extractable));
    let ops = usages_to_array(scope, key.usages);
    set(scope, &object, "key_ops", ops);
    if let Some(raw) = key.raw() {
        set_str(scope, &object, "kty", "oct");
        put(scope, &object, "k", raw);
        return Ok(object);
    }
    let view = key.view();
    let algo = text(key.algo.as_deref());
    let hash = text(key.hash.as_deref());
    if is_rsa(algo) {
        let jwk = webcrypto::export_rsa_jwk(&view)?;
        set_str(scope, &object, "kty", "RSA");
        if let Some(alg) = rsa_alg(algo, hash) {
            set_str(scope, &object, "alg", &alg);
        }
        put(scope, &object, "n", jwk.n.as_deref().unwrap_or_default());
        put(scope, &object, "e", jwk.e.as_deref().unwrap_or_default());
        if jwk.d.is_some() {
            let fields = [
                ("d", &jwk.d),
                ("p", &jwk.p),
                ("q", &jwk.q),
                ("dp", &jwk.dp),
                ("dq", &jwk.dq),
                ("qi", &jwk.qi),
            ];
            for (name, bytes) in fields {
                if let Some(bytes) = bytes {
                    put(scope, &object, name, bytes);
                }
            }
        }
        return Ok(object);
    }
    if matches!(algo, Some("ECDSA" | "ECDH")) {
        let jwk = webcrypto::export_ec_jwk(&view)?;
        set_str(scope, &object, "kty", "EC");
        let curve = text(key.curve.as_deref()).unwrap_or_default();
        set_str(scope, &object, "crv", curve);
        put(scope, &object, "x", &jwk.x);
        put(scope, &object, "y", &jwk.y);
        if let Some(d) = &jwk.d {
            put(scope, &object, "d", d);
        }
        return Ok(object);
    }
    if let Some(algo @ ("Ed25519" | "X25519")) = algo {
        let jwk = webcrypto::export_okp_jwk(&view)?;
        set_str(scope, &object, "kty", "OKP");
        set_str(scope, &object, "crv", algo);
        put(scope, &object, "x", &jwk.x);
        if let Some(d) = &jwk.d {
            put(scope, &object, "d", d);
        }
        return Ok(object);
    }
    Err("NotSupportedError: jwk export".to_owned())
}

pub(crate) fn serialize(
    scope: &mut Scope<'_>,
    format: &str,
    key: &StoredKey,
) -> webcrypto::Result<Vec<u8>> {
    if format != "jwk" {
        return webcrypto::export_raw(Some(format.as_bytes()), &key.view());
    }
    let jwk = export(scope, key)?;
    let serialize_error = || "OperationError: jwk serialize".to_owned();
    let text = scope.json_stringify(&jwk).map_err(|_| serialize_error())?;
    scope
        .to_string(&text)
        .map(String::into_bytes)
        .map_err(|_| serialize_error())
}

pub(crate) fn json_key_data(scope: &mut Scope<'_>, plain: &[u8]) -> webcrypto::Result<KeyData> {
    scope
        .parse_json(plain, "<unwrap>")
        .map(KeyData::Jwk)
        .map_err(|_| "DataError: wrapped jwk is not valid JSON".to_owned())
}
