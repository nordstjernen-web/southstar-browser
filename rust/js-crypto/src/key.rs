//! Southstar — CryptoKey objects: a host object over a stored key with its type, extractable, usages and algorithm members.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::rc::Rc;

use southstar_js_engine::{Attributes, Scope, Value};
use southstar_webcrypto::{KeyType, StoredKey};

use crate::algorithm::usages_to_array;
use crate::{get, set, set_str};

const ALL: Attributes = Attributes {
    writable: true,
    enumerable: true,
    configurable: true,
};

#[derive(Clone)]
struct KeyHandle(Rc<StoredKey>);

pub(crate) fn stored(scope: &mut Scope<'_>, value: &Value) -> Option<Rc<StoredKey>> {
    if !value.is_object() {
        return None;
    }
    scope.host_data::<KeyHandle>(value).map(|handle| handle.0)
}

pub(crate) fn text(bytes: Option<&[u8]>) -> Option<&str> {
    bytes.and_then(|bytes| core::str::from_utf8(bytes).ok())
}

pub(crate) fn is_rsa(algo: Option<&str>) -> bool {
    matches!(algo, Some("RSASSA-PKCS1-v1_5" | "RSA-PSS" | "RSA-OAEP"))
}

fn prototype(scope: &mut Scope<'_>) -> Option<Value> {
    let global = scope.global();
    let ctor = get(scope, &global, "CryptoKey");
    if !ctor.is_object() {
        return None;
    }
    let proto = get(scope, &ctor, "prototype");
    proto.is_object().then_some(proto)
}

fn own(scope: &mut Scope<'_>, object: &Value, key: &str, value: Value) {
    let _ = scope.define(object, key, value, ALL);
}

pub(crate) fn make_key(scope: &mut Scope<'_>, key: StoredKey) -> Value {
    let key = Rc::new(key);
    let proto = prototype(scope);
    let object = scope.new_host_object(proto.as_ref(), KeyHandle(key.clone()));
    let kind = match key.kind {
        KeyType::Private => "private",
        KeyType::Public => "public",
        KeyType::Secret => "secret",
    };
    let kind = scope.string(kind);
    own(scope, &object, "type", kind);
    own(
        scope,
        &object,
        "extractable",
        Value::boolean(key.extractable),
    );
    let usages = usages_to_array(scope, key.usages);
    own(scope, &object, "usages", usages);

    let algo = text(key.algo.as_deref());
    let algorithm = scope.new_object();
    set_str(scope, &algorithm, "name", algo.unwrap_or_default());
    if let Some(hash) = text(key.hash.as_deref()) {
        let hash_object = scope.new_object();
        set_str(scope, &hash_object, "name", hash);
        set(scope, &algorithm, "hash", hash_object);
    }
    if let Some(curve) = text(key.curve.as_deref()) {
        set_str(scope, &algorithm, "namedCurve", curve);
    }
    if key.kind == KeyType::Secret && key.bits != 0 {
        set(scope, &algorithm, "length", Value::int(key.bits));
    }
    if is_rsa(algo) && key.bits != 0 {
        set(scope, &algorithm, "modulusLength", Value::int(key.bits));
    }
    own(scope, &object, "algorithm", algorithm);
    object
}
