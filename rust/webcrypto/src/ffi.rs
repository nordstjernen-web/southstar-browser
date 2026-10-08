//! Southstar — the C ABI of SubtleCrypto, as declared in src/webcrypto.h.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub(crate) mod openssl;

use core::ffi::{c_char, c_int, c_void};
use core::ptr;
use southstar_glib::{self as glib, FALSE, GBoolean, TRUE, g_free, g_malloc, g_malloc0};

use crate::{KeyAttrs, KeyType, NewKey, Params, Result, RsaJwk};
use openssl::{Pkey, PkeyRef};

#[repr(C)]
pub struct CryptoKey {
    kind: c_int,
    algo: *mut c_char,
    hash: *mut c_char,
    curve: *mut c_char,
    bits: c_int,
    extractable: GBoolean,
    usages: u32,
    raw: *mut u8,
    raw_len: usize,
    pkey: *mut c_void,
    refcount: c_int,
}

#[repr(C)]
pub struct CryptoParams {
    iv: *const u8,
    iv_len: usize,
    aad: *const u8,
    aad_len: usize,
    tag_bits: c_int,
    label: *const u8,
    label_len: usize,
    salt: *const u8,
    salt_len: usize,
    info: *const u8,
    info_len: usize,
    iterations: c_int,
    kdf_hash: *const c_char,
    sign_hash: *const c_char,
    peer: *mut CryptoKey,
    counter_bits: c_int,
    pss_salt_len: c_int,
}

unsafe fn opt_slice<'a>(p: *const u8, len: usize) -> Option<&'a [u8]> {
    if p.is_null() {
        None
    } else if len == 0 {
        Some(&[])
    } else {
        Some(unsafe { core::slice::from_raw_parts(p, len) })
    }
}

unsafe fn text<'a>(p: *const c_char) -> Option<&'a [u8]> {
    unsafe { glib::bytes(p) }
}

fn kind_of(kind: c_int) -> KeyType {
    match kind {
        1 => KeyType::Public,
        2 => KeyType::Private,
        _ => KeyType::Secret,
    }
}

fn kind_code(kind: KeyType) -> c_int {
    match kind {
        KeyType::Secret => 0,
        KeyType::Public => 1,
        KeyType::Private => 2,
    }
}

unsafe fn view<'a>(k: *const CryptoKey) -> crate::Key<'a> {
    let k = unsafe { &*k };
    crate::Key {
        kind: kind_of(k.kind),
        algo: unsafe { text(k.algo) }.unwrap_or_default(),
        hash: unsafe { text(k.hash) },
        curve: unsafe { text(k.curve) },
        bits: k.bits,
        raw: unsafe { opt_slice(k.raw, k.raw_len) }.filter(|raw| !raw.is_empty()),
        pkey: unsafe { PkeyRef::from_ptr(k.pkey) },
    }
}

unsafe fn params<'a>(p: *const CryptoParams) -> Params<'a> {
    let p = unsafe { &*p };
    Params {
        iv: unsafe { opt_slice(p.iv, p.iv_len) },
        aad: unsafe { opt_slice(p.aad, p.aad_len) },
        tag_bits: p.tag_bits,
        label: unsafe { opt_slice(p.label, p.label_len) },
        salt: unsafe { opt_slice(p.salt, p.salt_len) },
        info: unsafe { opt_slice(p.info, p.info_len) },
        iterations: p.iterations,
        kdf_hash: unsafe { text(p.kdf_hash) },
        sign_hash: unsafe { text(p.sign_hash) },
        peer: (!p.peer.is_null()).then(|| unsafe { view(p.peer) }),
        counter_bits: p.counter_bits,
        pss_salt_len: p.pss_salt_len,
    }
}

fn to_glib(mut bytes: Vec<u8>) -> *mut u8 {
    let out = unsafe { g_malloc(bytes.len().max(1)) }.cast::<u8>();
    unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len()) };
    openssl::cleanse(&mut bytes);
    out
}

fn strdup(bytes: Option<&[u8]>) -> *mut c_char {
    bytes.map_or(ptr::null_mut(), glib::strdup)
}

fn into_key(mut key: NewKey<'_>) -> *mut CryptoKey {
    let (raw, raw_len) = match key.raw.take() {
        Some(raw) if !raw.is_empty() => {
            let len = raw.len();
            (to_glib(raw), len)
        }
        _ => (ptr::null_mut(), 0),
    };
    let out = unsafe { g_malloc0(core::mem::size_of::<CryptoKey>()) }.cast::<CryptoKey>();
    let fields = CryptoKey {
        kind: kind_code(key.kind),
        algo: strdup(key.algo),
        hash: strdup(key.hash),
        curve: strdup(key.curve),
        bits: key.bits,
        extractable: glib::boolean(key.extractable),
        usages: key.usages,
        raw,
        raw_len,
        pkey: key.pkey.take().map_or(ptr::null_mut(), Pkey::into_raw),
        refcount: 1,
    };
    unsafe { out.write(fields) };
    out
}

unsafe fn set_err(err: *mut *mut c_char, message: &str) {
    if !err.is_null() && unsafe { (*err).is_null() } {
        unsafe { *err = glib::strdup(message.as_bytes()) };
    }
}

unsafe fn set_len(out_len: *mut usize, len: usize) {
    if !out_len.is_null() {
        unsafe { *out_len = len };
    }
}

unsafe fn bytes_result(
    result: Result<Vec<u8>>,
    out_len: *mut usize,
    err: *mut *mut c_char,
) -> *mut u8 {
    match result {
        Ok(bytes) => {
            unsafe { set_len(out_len, bytes.len()) };
            to_glib(bytes)
        }
        Err(message) => {
            unsafe { set_err(err, &message) };
            ptr::null_mut()
        }
    }
}

unsafe fn key_result(result: Result<NewKey<'_>>, err: *mut *mut c_char) -> *mut CryptoKey {
    match result {
        Ok(key) => into_key(key),
        Err(message) => {
            unsafe { set_err(err, &message) };
            ptr::null_mut()
        }
    }
}

unsafe fn attrs<'a>(
    algo: *const c_char,
    hash: *const c_char,
    curve: *const c_char,
    extractable: GBoolean,
    usages: u32,
) -> KeyAttrs<'a> {
    KeyAttrs {
        algo: unsafe { text(algo) },
        hash: unsafe { text(hash) },
        curve: unsafe { text(curve) },
        extractable: extractable != FALSE,
        usages,
    }
}

unsafe fn put(out: *mut *mut u8, out_len: *mut usize, bytes: Option<Vec<u8>>) {
    let (p, len) = match bytes {
        Some(bytes) => {
            let len = bytes.len();
            (to_glib(bytes), len)
        }
        None => (ptr::null_mut(), 0),
    };
    unsafe {
        *out = p;
        *out_len = len;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_crypto_key_unref(k: *mut CryptoKey) {
    if k.is_null() {
        return;
    }
    let key = unsafe { &mut *k };
    key.refcount -= 1;
    if key.refcount > 0 {
        return;
    }
    unsafe {
        g_free(key.algo.cast());
        g_free(key.hash.cast());
        g_free(key.curve.cast());
        if !key.raw.is_null() {
            openssl::cleanse(core::slice::from_raw_parts_mut(key.raw, key.raw_len));
            g_free(key.raw.cast());
        }
        drop(Pkey::from_owned(key.pkey));
        g_free(k.cast());
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_crypto_digest(
    hash: *const c_char,
    data: *const u8,
    len: usize,
    out_len: *mut usize,
) -> *mut u8 {
    let data = unsafe { glib::slice(data, len) };
    match crate::digest(unsafe { text(hash) }, data) {
        Some(digest) => {
            unsafe { set_len(out_len, digest.len()) };
            to_glib(digest)
        }
        None => ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_crypto_generate_secret(
    algo: *const c_char,
    hash: *const c_char,
    length_bits: c_int,
    extractable: GBoolean,
    usages: u32,
    err: *mut *mut c_char,
) -> *mut CryptoKey {
    let attrs = unsafe { attrs(algo, hash, ptr::null(), extractable, usages) };
    unsafe { key_result(crate::generate_secret(&attrs, length_bits), err) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_crypto_generate_keypair(
    algo: *const c_char,
    hash: *const c_char,
    curve: *const c_char,
    modulus_bits: c_int,
    pubexp: u32,
    extractable: GBoolean,
    usages: u32,
    public: *mut *mut CryptoKey,
    private: *mut *mut CryptoKey,
    err: *mut *mut c_char,
) -> GBoolean {
    unsafe {
        *public = ptr::null_mut();
        *private = ptr::null_mut();
    }
    let attrs = unsafe { attrs(algo, hash, curve, extractable, usages) };
    match crate::generate_keypair(&attrs, modulus_bits, pubexp) {
        Ok((public_key, private_key)) => {
            unsafe {
                *public = into_key(public_key);
                *private = into_key(private_key);
            }
            TRUE
        }
        Err(message) => {
            unsafe { set_err(err, &message) };
            FALSE
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_crypto_import_raw(
    format: *const c_char,
    data: *const u8,
    len: usize,
    algo: *const c_char,
    hash: *const c_char,
    curve: *const c_char,
    extractable: GBoolean,
    usages: u32,
    err: *mut *mut c_char,
) -> *mut CryptoKey {
    let attrs = unsafe { attrs(algo, hash, curve, extractable, usages) };
    let data = unsafe { glib::slice(data, len) };
    unsafe { key_result(crate::import_raw(text(format), data, &attrs), err) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_crypto_import_rsa_jwk(
    n: *const u8,
    n_len: usize,
    e: *const u8,
    e_len: usize,
    d: *const u8,
    d_len: usize,
    p: *const u8,
    p_len: usize,
    q: *const u8,
    q_len: usize,
    dp: *const u8,
    dp_len: usize,
    dq: *const u8,
    dq_len: usize,
    qi: *const u8,
    qi_len: usize,
    algo: *const c_char,
    hash: *const c_char,
    extractable: GBoolean,
    usages: u32,
    err: *mut *mut c_char,
) -> *mut CryptoKey {
    let jwk = unsafe {
        RsaJwk {
            n: opt_slice(n, n_len),
            e: opt_slice(e, e_len),
            d: opt_slice(d, d_len),
            p: opt_slice(p, p_len),
            q: opt_slice(q, q_len),
            dp: opt_slice(dp, dp_len),
            dq: opt_slice(dq, dq_len),
            qi: opt_slice(qi, qi_len),
        }
    };
    let attrs = unsafe { attrs(algo, hash, ptr::null(), extractable, usages) };
    unsafe { key_result(crate::import_rsa_jwk(&jwk, &attrs), err) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_crypto_import_ec_jwk(
    curve: *const c_char,
    x: *const u8,
    x_len: usize,
    y: *const u8,
    y_len: usize,
    d: *const u8,
    d_len: usize,
    algo: *const c_char,
    extractable: GBoolean,
    usages: u32,
    err: *mut *mut c_char,
) -> *mut CryptoKey {
    let attrs = unsafe { attrs(algo, ptr::null(), curve, extractable, usages) };
    let (x, y, d) = unsafe {
        (
            opt_slice(x, x_len),
            opt_slice(y, y_len),
            opt_slice(d, d_len),
        )
    };
    unsafe { key_result(crate::import_ec_jwk(x, y, d, &attrs), err) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_crypto_import_okp_jwk(
    curve: *const c_char,
    x: *const u8,
    x_len: usize,
    d: *const u8,
    d_len: usize,
    algo: *const c_char,
    extractable: GBoolean,
    usages: u32,
    err: *mut *mut c_char,
) -> *mut CryptoKey {
    let attrs = unsafe { attrs(algo, ptr::null(), curve, extractable, usages) };
    let (x, d) = unsafe { (opt_slice(x, x_len), opt_slice(d, d_len)) };
    unsafe { key_result(crate::import_okp_jwk(x, d, &attrs), err) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_crypto_export_raw(
    format: *const c_char,
    k: *const CryptoKey,
    out_len: *mut usize,
    err: *mut *mut c_char,
) -> *mut u8 {
    let key = unsafe { view(k) };
    unsafe { bytes_result(crate::export_raw(text(format), &key), out_len, err) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_crypto_export_rsa_jwk(
    k: *const CryptoKey,
    n: *mut *mut u8,
    n_len: *mut usize,
    e: *mut *mut u8,
    e_len: *mut usize,
    d: *mut *mut u8,
    d_len: *mut usize,
    p: *mut *mut u8,
    p_len: *mut usize,
    q: *mut *mut u8,
    q_len: *mut usize,
    dp: *mut *mut u8,
    dp_len: *mut usize,
    dq: *mut *mut u8,
    dq_len: *mut usize,
    qi: *mut *mut u8,
    qi_len: *mut usize,
    err: *mut *mut c_char,
) -> GBoolean {
    let key = unsafe { view(k) };
    match crate::export_rsa_jwk(&key) {
        Ok(jwk) => {
            unsafe {
                put(n, n_len, jwk.n);
                put(e, e_len, jwk.e);
                put(d, d_len, jwk.d);
                put(p, p_len, jwk.p);
                put(q, q_len, jwk.q);
                put(dp, dp_len, jwk.dp);
                put(dq, dq_len, jwk.dq);
                put(qi, qi_len, jwk.qi);
            }
            TRUE
        }
        Err(message) => {
            if key.pkey.is_some() {
                unsafe {
                    for (out, out_len) in [
                        (n, n_len),
                        (e, e_len),
                        (d, d_len),
                        (p, p_len),
                        (q, q_len),
                        (dp, dp_len),
                        (dq, dq_len),
                        (qi, qi_len),
                    ] {
                        put(out, out_len, None);
                    }
                }
            }
            unsafe { set_err(err, &message) };
            FALSE
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_crypto_export_ec_jwk(
    k: *const CryptoKey,
    x: *mut *mut u8,
    x_len: *mut usize,
    y: *mut *mut u8,
    y_len: *mut usize,
    d: *mut *mut u8,
    d_len: *mut usize,
    err: *mut *mut c_char,
) -> GBoolean {
    let key = unsafe { view(k) };
    let result = crate::export_ec_jwk(&key);
    if key.pkey.is_some() && crate::opt_curve_order(key.curve) > 0 {
        unsafe {
            put(x, x_len, None);
            put(y, y_len, None);
            put(d, d_len, None);
        }
    }
    match result {
        Ok(jwk) => {
            unsafe {
                put(x, x_len, Some(jwk.x));
                put(y, y_len, Some(jwk.y));
                put(d, d_len, jwk.d);
            }
            TRUE
        }
        Err(message) => {
            unsafe { set_err(err, &message) };
            FALSE
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_crypto_export_okp_jwk(
    k: *const CryptoKey,
    x: *mut *mut u8,
    x_len: *mut usize,
    d: *mut *mut u8,
    d_len: *mut usize,
    err: *mut *mut c_char,
) -> GBoolean {
    unsafe {
        put(x, x_len, None);
        put(d, d_len, None);
    }
    let key = unsafe { view(k) };
    match crate::export_okp_jwk(&key) {
        Ok(jwk) => {
            unsafe {
                put(x, x_len, Some(jwk.x));
                put(d, d_len, jwk.d);
            }
            TRUE
        }
        Err(message) => {
            unsafe { set_err(err, &message) };
            FALSE
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_crypto_sign(
    k: *const CryptoKey,
    p: *const CryptoParams,
    data: *const u8,
    len: usize,
    out_len: *mut usize,
    err: *mut *mut c_char,
) -> *mut u8 {
    let (key, params) = unsafe { (view(k), params(p)) };
    let data = unsafe { glib::slice(data, len) };
    unsafe { bytes_result(crate::sign(&key, &params, data), out_len, err) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_crypto_verify(
    k: *const CryptoKey,
    p: *const CryptoParams,
    sig: *const u8,
    sig_len: usize,
    data: *const u8,
    len: usize,
    err: *mut *mut c_char,
) -> c_int {
    let (key, params) = unsafe { (view(k), params(p)) };
    let (sig, data) = unsafe { (glib::slice(sig, sig_len), glib::slice(data, len)) };
    match crate::verify(&key, &params, sig, data) {
        Ok(valid) => c_int::from(valid),
        Err(message) => {
            unsafe { set_err(err, &message) };
            -1
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_crypto_encrypt(
    k: *const CryptoKey,
    p: *const CryptoParams,
    data: *const u8,
    len: usize,
    out_len: *mut usize,
    err: *mut *mut c_char,
) -> *mut u8 {
    let (key, params) = unsafe { (view(k), params(p)) };
    let data = unsafe { glib::slice(data, len) };
    unsafe { bytes_result(crate::encrypt(&key, &params, data), out_len, err) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_crypto_decrypt(
    k: *const CryptoKey,
    p: *const CryptoParams,
    data: *const u8,
    len: usize,
    out_len: *mut usize,
    err: *mut *mut c_char,
) -> *mut u8 {
    let (key, params) = unsafe { (view(k), params(p)) };
    let data = unsafe { glib::slice(data, len) };
    unsafe { bytes_result(crate::decrypt(&key, &params, data), out_len, err) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_crypto_derive_bits(
    k: *const CryptoKey,
    p: *const CryptoParams,
    length_bits: c_int,
    out_len: *mut usize,
    err: *mut *mut c_char,
) -> *mut u8 {
    let (key, params) = unsafe { (view(k), params(p)) };
    unsafe { bytes_result(crate::derive_bits(&key, &params, length_bits), out_len, err) }
}
