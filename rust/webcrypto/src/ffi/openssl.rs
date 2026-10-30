//! Southstar — safe wrappers over the OpenSSL libcrypto EVP, BIGNUM and OSSL_PARAM calls SubtleCrypto needs.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_long, c_uint, c_ulong, c_void};
use core::marker::PhantomData;
use core::ptr::{self, NonNull};

macro_rules! opaque {
    ($($name:ident),* $(,)?) => {
        $(
            #[repr(C)]
            pub struct $name {
                _private: [u8; 0],
            }
        )*
    };
}

opaque!(
    EvpMd,
    EvpMdCtx,
    EvpPkey,
    EvpPkeyCtx,
    EvpCipher,
    EvpCipherCtx,
    EvpMac,
    EvpMacCtx,
    EvpKdf,
    EvpKdfCtx,
    Bignum,
    ParamBld,
    Pkcs8PrivKeyInfo,
    EcdsaSig,
    Engine,
);

#[repr(C)]
#[derive(Clone, Copy)]
struct OsslParam {
    key: *const c_char,
    data_type: c_uint,
    data: *mut c_void,
    data_size: usize,
    return_size: usize,
}

const EVP_PKEY_PUBLIC_KEY: c_int = 0x86;
const EVP_PKEY_KEYPAIR: c_int = 0x87;
const EVP_CIPHER_CTX_FLAG_WRAP_ALLOW: c_int = 0x1;
const EVP_CTRL_AEAD_SET_IVLEN: c_int = 0x9;
const EVP_CTRL_AEAD_GET_TAG: c_int = 0x10;
const EVP_CTRL_AEAD_SET_TAG: c_int = 0x11;
const RSA_PKCS1_PADDING: c_int = 1;
const RSA_PKCS1_OAEP_PADDING: c_int = 4;
const RSA_PKCS1_PSS_PADDING: c_int = 6;
const AES_BLOCK: usize = 16;

unsafe extern "C" {
    safe fn ERR_peek_last_error() -> c_ulong;
    fn ERR_error_string_n(e: c_ulong, buf: *mut c_char, len: usize);
    safe fn ERR_clear_error();
    fn OPENSSL_cleanse(ptr: *mut c_void, len: usize);
    fn CRYPTO_free(ptr: *mut c_void, file: *const c_char, line: c_int);
    fn CRYPTO_memdup(
        data: *const c_void,
        siz: usize,
        file: *const c_char,
        line: c_int,
    ) -> *mut c_void;
    fn CRYPTO_memcmp(a: *const c_void, b: *const c_void, len: usize) -> c_int;
    fn RAND_bytes(buf: *mut u8, num: c_int) -> c_int;

    safe fn EVP_sha1() -> *const EvpMd;
    safe fn EVP_sha256() -> *const EvpMd;
    safe fn EVP_sha384() -> *const EvpMd;
    safe fn EVP_sha512() -> *const EvpMd;
    safe fn EVP_sha3_256() -> *const EvpMd;
    safe fn EVP_sha3_384() -> *const EvpMd;
    safe fn EVP_sha3_512() -> *const EvpMd;
    fn EVP_MD_get_size(md: *const EvpMd) -> c_int;
    safe fn EVP_MD_CTX_new() -> *mut EvpMdCtx;
    fn EVP_MD_CTX_free(ctx: *mut EvpMdCtx);
    fn EVP_DigestInit_ex(ctx: *mut EvpMdCtx, md: *const EvpMd, engine: *mut Engine) -> c_int;
    fn EVP_DigestUpdate(ctx: *mut EvpMdCtx, data: *const c_void, len: usize) -> c_int;
    fn EVP_DigestFinal_ex(ctx: *mut EvpMdCtx, md: *mut u8, len: *mut c_uint) -> c_int;
    fn EVP_DigestSignInit(
        ctx: *mut EvpMdCtx,
        pctx: *mut *mut EvpPkeyCtx,
        md: *const EvpMd,
        engine: *mut Engine,
        pkey: *mut EvpPkey,
    ) -> c_int;
    fn EVP_DigestSign(
        ctx: *mut EvpMdCtx,
        sig: *mut u8,
        siglen: *mut usize,
        tbs: *const u8,
        tbslen: usize,
    ) -> c_int;
    fn EVP_DigestVerifyInit(
        ctx: *mut EvpMdCtx,
        pctx: *mut *mut EvpPkeyCtx,
        md: *const EvpMd,
        engine: *mut Engine,
        pkey: *mut EvpPkey,
    ) -> c_int;
    fn EVP_DigestVerify(
        ctx: *mut EvpMdCtx,
        sig: *const u8,
        siglen: usize,
        tbs: *const u8,
        tbslen: usize,
    ) -> c_int;

    fn EVP_PKEY_free(pkey: *mut EvpPkey);
    fn EVP_PKEY_dup(pkey: *mut EvpPkey) -> *mut EvpPkey;
    fn EVP_PKEY_get_bits(pkey: *const EvpPkey) -> c_int;
    fn EVP_PKEY_get_raw_public_key(pkey: *const EvpPkey, out: *mut u8, len: *mut usize) -> c_int;
    fn EVP_PKEY_get_raw_private_key(pkey: *const EvpPkey, out: *mut u8, len: *mut usize) -> c_int;
    fn EVP_PKEY_get1_encoded_public_key(pkey: *mut EvpPkey, out: *mut *mut u8) -> usize;
    fn EVP_PKEY_get_bn_param(
        pkey: *const EvpPkey,
        name: *const c_char,
        bn: *mut *mut Bignum,
    ) -> c_int;
    fn EVP_PKEY_new_raw_public_key_ex(
        libctx: *mut c_void,
        keytype: *const c_char,
        propq: *const c_char,
        key: *const u8,
        keylen: usize,
    ) -> *mut EvpPkey;
    fn EVP_PKEY_new_raw_private_key_ex(
        libctx: *mut c_void,
        keytype: *const c_char,
        propq: *const c_char,
        key: *const u8,
        keylen: usize,
    ) -> *mut EvpPkey;
    fn d2i_PUBKEY(a: *mut *mut EvpPkey, pp: *mut *const u8, length: c_long) -> *mut EvpPkey;
    fn i2d_PUBKEY(a: *const EvpPkey, pp: *mut *mut u8) -> c_int;
    fn d2i_PKCS8_PRIV_KEY_INFO(
        a: *mut *mut Pkcs8PrivKeyInfo,
        pp: *mut *const u8,
        length: c_long,
    ) -> *mut Pkcs8PrivKeyInfo;
    fn i2d_PKCS8_PRIV_KEY_INFO(a: *const Pkcs8PrivKeyInfo, pp: *mut *mut u8) -> c_int;
    fn PKCS8_PRIV_KEY_INFO_free(a: *mut Pkcs8PrivKeyInfo);
    fn EVP_PKCS82PKEY(p8: *const Pkcs8PrivKeyInfo) -> *mut EvpPkey;
    fn EVP_PKEY2PKCS8(pkey: *const EvpPkey) -> *mut Pkcs8PrivKeyInfo;

    fn EVP_PKEY_CTX_new_from_name(
        libctx: *mut c_void,
        name: *const c_char,
        propq: *const c_char,
    ) -> *mut EvpPkeyCtx;
    fn EVP_PKEY_CTX_new_from_pkey(
        libctx: *mut c_void,
        pkey: *mut EvpPkey,
        propq: *const c_char,
    ) -> *mut EvpPkeyCtx;
    fn EVP_PKEY_CTX_free(ctx: *mut EvpPkeyCtx);
    fn EVP_PKEY_keygen_init(ctx: *mut EvpPkeyCtx) -> c_int;
    fn EVP_PKEY_generate(ctx: *mut EvpPkeyCtx, pkey: *mut *mut EvpPkey) -> c_int;
    fn EVP_PKEY_CTX_set_group_name(ctx: *mut EvpPkeyCtx, name: *const c_char) -> c_int;
    fn EVP_PKEY_CTX_set_rsa_keygen_bits(ctx: *mut EvpPkeyCtx, bits: c_int) -> c_int;
    fn EVP_PKEY_CTX_set1_rsa_keygen_pubexp(ctx: *mut EvpPkeyCtx, pubexp: *mut Bignum) -> c_int;
    fn EVP_PKEY_CTX_set_rsa_padding(ctx: *mut EvpPkeyCtx, pad_mode: c_int) -> c_int;
    fn EVP_PKEY_CTX_set_rsa_pss_saltlen(ctx: *mut EvpPkeyCtx, saltlen: c_int) -> c_int;
    fn EVP_PKEY_CTX_set_rsa_oaep_md(ctx: *mut EvpPkeyCtx, md: *const EvpMd) -> c_int;
    fn EVP_PKEY_CTX_set_rsa_mgf1_md(ctx: *mut EvpPkeyCtx, md: *const EvpMd) -> c_int;
    fn EVP_PKEY_CTX_set0_rsa_oaep_label(
        ctx: *mut EvpPkeyCtx,
        label: *mut c_void,
        llen: c_int,
    ) -> c_int;
    fn EVP_PKEY_fromdata_init(ctx: *mut EvpPkeyCtx) -> c_int;
    fn EVP_PKEY_fromdata(
        ctx: *mut EvpPkeyCtx,
        pkey: *mut *mut EvpPkey,
        selection: c_int,
        params: *mut OsslParam,
    ) -> c_int;
    fn EVP_PKEY_encrypt_init(ctx: *mut EvpPkeyCtx) -> c_int;
    fn EVP_PKEY_decrypt_init(ctx: *mut EvpPkeyCtx) -> c_int;
    fn EVP_PKEY_encrypt(
        ctx: *mut EvpPkeyCtx,
        out: *mut u8,
        outlen: *mut usize,
        input: *const u8,
        inlen: usize,
    ) -> c_int;
    fn EVP_PKEY_decrypt(
        ctx: *mut EvpPkeyCtx,
        out: *mut u8,
        outlen: *mut usize,
        input: *const u8,
        inlen: usize,
    ) -> c_int;
    fn EVP_PKEY_derive_init(ctx: *mut EvpPkeyCtx) -> c_int;
    fn EVP_PKEY_derive_set_peer(ctx: *mut EvpPkeyCtx, peer: *mut EvpPkey) -> c_int;
    fn EVP_PKEY_derive(ctx: *mut EvpPkeyCtx, key: *mut u8, keylen: *mut usize) -> c_int;

    safe fn BN_new() -> *mut Bignum;
    fn BN_clear_free(bn: *mut Bignum);
    fn BN_set_word(bn: *mut Bignum, w: c_ulong) -> c_int;
    fn BN_bin2bn(s: *const u8, len: c_int, ret: *mut Bignum) -> *mut Bignum;
    fn BN_bn2bin(bn: *const Bignum, to: *mut u8) -> c_int;
    fn BN_bn2binpad(bn: *const Bignum, to: *mut u8, tolen: c_int) -> c_int;
    fn BN_num_bits(bn: *const Bignum) -> c_int;

    safe fn OSSL_PARAM_BLD_new() -> *mut ParamBld;
    fn OSSL_PARAM_BLD_free(bld: *mut ParamBld);
    fn OSSL_PARAM_BLD_push_utf8_string(
        bld: *mut ParamBld,
        key: *const c_char,
        buf: *const c_char,
        bsize: usize,
    ) -> c_int;
    fn OSSL_PARAM_BLD_push_octet_string(
        bld: *mut ParamBld,
        key: *const c_char,
        buf: *const c_void,
        bsize: usize,
    ) -> c_int;
    fn OSSL_PARAM_BLD_push_BN(bld: *mut ParamBld, key: *const c_char, bn: *const Bignum) -> c_int;
    fn OSSL_PARAM_BLD_to_param(bld: *mut ParamBld) -> *mut OsslParam;
    fn OSSL_PARAM_free(params: *mut OsslParam);

    fn EVP_MAC_fetch(
        libctx: *mut c_void,
        algorithm: *const c_char,
        properties: *const c_char,
    ) -> *mut EvpMac;
    fn EVP_MAC_free(mac: *mut EvpMac);
    fn EVP_MAC_CTX_new(mac: *mut EvpMac) -> *mut EvpMacCtx;
    fn EVP_MAC_CTX_free(ctx: *mut EvpMacCtx);
    fn EVP_MAC_init(
        ctx: *mut EvpMacCtx,
        key: *const u8,
        keylen: usize,
        params: *const OsslParam,
    ) -> c_int;
    fn EVP_MAC_update(ctx: *mut EvpMacCtx, data: *const u8, datalen: usize) -> c_int;
    fn EVP_MAC_final(ctx: *mut EvpMacCtx, out: *mut u8, outl: *mut usize, outsize: usize) -> c_int;

    fn EVP_KDF_fetch(
        libctx: *mut c_void,
        algorithm: *const c_char,
        properties: *const c_char,
    ) -> *mut EvpKdf;
    fn EVP_KDF_free(kdf: *mut EvpKdf);
    fn EVP_KDF_CTX_new(kdf: *mut EvpKdf) -> *mut EvpKdfCtx;
    fn EVP_KDF_CTX_free(ctx: *mut EvpKdfCtx);
    fn EVP_KDF_derive(
        ctx: *mut EvpKdfCtx,
        key: *mut u8,
        keylen: usize,
        params: *const OsslParam,
    ) -> c_int;
    fn PKCS5_PBKDF2_HMAC(
        pass: *const c_char,
        passlen: c_int,
        salt: *const u8,
        saltlen: c_int,
        iter: c_int,
        digest: *const EvpMd,
        keylen: c_int,
        out: *mut u8,
    ) -> c_int;

    fn d2i_ECDSA_SIG(sig: *mut *mut EcdsaSig, pp: *mut *const u8, len: c_long) -> *mut EcdsaSig;
    fn i2d_ECDSA_SIG(sig: *const EcdsaSig, pp: *mut *mut u8) -> c_int;
    safe fn ECDSA_SIG_new() -> *mut EcdsaSig;
    fn ECDSA_SIG_free(sig: *mut EcdsaSig);
    fn ECDSA_SIG_get0(sig: *const EcdsaSig, pr: *mut *const Bignum, ps: *mut *const Bignum);
    fn ECDSA_SIG_set0(sig: *mut EcdsaSig, r: *mut Bignum, s: *mut Bignum) -> c_int;

    safe fn EVP_aes_128_gcm() -> *const EvpCipher;
    safe fn EVP_aes_192_gcm() -> *const EvpCipher;
    safe fn EVP_aes_256_gcm() -> *const EvpCipher;
    safe fn EVP_aes_128_cbc() -> *const EvpCipher;
    safe fn EVP_aes_192_cbc() -> *const EvpCipher;
    safe fn EVP_aes_256_cbc() -> *const EvpCipher;
    safe fn EVP_aes_128_ctr() -> *const EvpCipher;
    safe fn EVP_aes_192_ctr() -> *const EvpCipher;
    safe fn EVP_aes_256_ctr() -> *const EvpCipher;
    safe fn EVP_aes_128_wrap() -> *const EvpCipher;
    safe fn EVP_aes_192_wrap() -> *const EvpCipher;
    safe fn EVP_aes_256_wrap() -> *const EvpCipher;
    safe fn EVP_CIPHER_CTX_new() -> *mut EvpCipherCtx;
    fn EVP_CIPHER_CTX_free(ctx: *mut EvpCipherCtx);
    fn EVP_CIPHER_CTX_set_flags(ctx: *mut EvpCipherCtx, flags: c_int);
    fn EVP_CIPHER_CTX_ctrl(
        ctx: *mut EvpCipherCtx,
        kind: c_int,
        arg: c_int,
        ptr: *mut c_void,
    ) -> c_int;
    fn EVP_CipherInit_ex(
        ctx: *mut EvpCipherCtx,
        cipher: *const EvpCipher,
        engine: *mut Engine,
        key: *const u8,
        iv: *const u8,
        enc: c_int,
    ) -> c_int;
    fn EVP_CipherUpdate(
        ctx: *mut EvpCipherCtx,
        out: *mut u8,
        outl: *mut c_int,
        input: *const u8,
        inl: c_int,
    ) -> c_int;
    fn EVP_CipherFinal_ex(ctx: *mut EvpCipherCtx, out: *mut u8, outl: *mut c_int) -> c_int;
    fn EVP_EncryptInit_ex(
        ctx: *mut EvpCipherCtx,
        cipher: *const EvpCipher,
        engine: *mut Engine,
        key: *const u8,
        iv: *const u8,
    ) -> c_int;
    fn EVP_EncryptUpdate(
        ctx: *mut EvpCipherCtx,
        out: *mut u8,
        outl: *mut c_int,
        input: *const u8,
        inl: c_int,
    ) -> c_int;
    fn EVP_EncryptFinal_ex(ctx: *mut EvpCipherCtx, out: *mut u8, outl: *mut c_int) -> c_int;
}

const FILE: &CStr = c"rust/webcrypto";
const OSSL_PARAM_UTF8_STRING: c_uint = 4;
const OSSL_PARAM_OCTET_STRING: c_uint = 5;
const OSSL_PARAM_UNMODIFIED: usize = usize::MAX;

fn opt_ptr(bytes: Option<&[u8]>) -> *const u8 {
    bytes.map_or(ptr::null(), <[u8]>::as_ptr)
}

fn c_len(len: usize) -> Option<c_int> {
    c_int::try_from(len).ok()
}

/// Copies an OpenSSL-allocated buffer into a `Vec`, optionally wipes it, and frees it.
///
/// # Safety
///
/// `p` must point to `len` initialized bytes allocated by OpenSSL's allocator that
/// nothing else owns or references; the buffer is freed before this returns.
unsafe fn take_openssl_bytes(p: *mut u8, len: usize, wipe: bool) -> Vec<u8> {
    // SAFETY: the caller guarantees `p` holds `len` initialized bytes that it alone owns.
    let bytes = unsafe { core::slice::from_raw_parts_mut(p, len) };
    let copy = bytes.to_vec();
    if wipe {
        cleanse(bytes);
    }
    // SAFETY: the caller guarantees OpenSSL allocated `p`, and `bytes` is not used past here.
    unsafe { CRYPTO_free(p.cast(), FILE.as_ptr(), 0) };
    copy
}

pub fn error_message(prefix: &str) -> String {
    let code = ERR_peek_last_error();
    let message = if code != 0 {
        let mut buf = [0u8; 256];
        // SAFETY: `buf` is writable for `buf.len()` bytes and OpenSSL writes at most that many.
        unsafe { ERR_error_string_n(code, buf.as_mut_ptr().cast(), buf.len()) };
        let text = CStr::from_bytes_until_nul(&buf)
            .map(|text| text.to_string_lossy().into_owned())
            .unwrap_or_default();
        format!("{prefix}: {text}")
    } else {
        prefix.to_owned()
    };
    ERR_clear_error();
    message
}

pub fn clear_errors() {
    ERR_clear_error();
}

pub fn cleanse(buf: &mut [u8]) {
    // SAFETY: `buf` is a live, exclusively borrowed region of exactly `buf.len()` bytes.
    unsafe { OPENSSL_cleanse(buf.as_mut_ptr().cast(), buf.len()) };
}

pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    // SAFETY: both slices are readable for `a.len()` bytes, as `b.len()` equals it here.
    a.len() == b.len()
        && unsafe { CRYPTO_memcmp(a.as_ptr().cast(), b.as_ptr().cast(), a.len()) } == 0
}

pub fn rand_bytes(buf: &mut [u8]) -> bool {
    c_len(buf.len()).is_some_and(|len| {
        // SAFETY: `buf` is exclusively borrowed and `len` equals its length.
        unsafe { RAND_bytes(buf.as_mut_ptr(), len) == 1 }
    })
}

impl OsslParam {
    fn utf8(key: &'static CStr, value: &CStr) -> OsslParam {
        OsslParam {
            key: key.as_ptr(),
            data_type: OSSL_PARAM_UTF8_STRING,
            data: value.as_ptr().cast_mut().cast(),
            data_size: value.to_bytes().len(),
            return_size: OSSL_PARAM_UNMODIFIED,
        }
    }

    fn octets(key: &'static CStr, bytes: Option<&[u8]>) -> OsslParam {
        OsslParam {
            key: key.as_ptr(),
            data_type: OSSL_PARAM_OCTET_STRING,
            data: opt_ptr(bytes).cast_mut().cast(),
            data_size: bytes.map_or(0, <[u8]>::len),
            return_size: OSSL_PARAM_UNMODIFIED,
        }
    }

    fn end() -> OsslParam {
        OsslParam {
            key: ptr::null(),
            data_type: 0,
            data: ptr::null_mut(),
            data_size: 0,
            return_size: 0,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Md(*const EvpMd);

impl Md {
    fn from(md: *const EvpMd) -> Option<Md> {
        (!md.is_null()).then_some(Md(md))
    }

    pub fn sha1() -> Option<Md> {
        Md::from(EVP_sha1())
    }

    pub fn sha256() -> Option<Md> {
        Md::from(EVP_sha256())
    }

    pub fn sha384() -> Option<Md> {
        Md::from(EVP_sha384())
    }

    pub fn sha512() -> Option<Md> {
        Md::from(EVP_sha512())
    }

    pub fn sha3_256() -> Option<Md> {
        Md::from(EVP_sha3_256())
    }

    pub fn sha3_384() -> Option<Md> {
        Md::from(EVP_sha3_384())
    }

    pub fn sha3_512() -> Option<Md> {
        Md::from(EVP_sha3_512())
    }

    pub fn size(self) -> i32 {
        // SAFETY: an `Md` only holds a non-null static digest returned by `EVP_sha*`.
        unsafe { EVP_MD_get_size(self.0) }
    }
}

fn md_ptr(md: Option<Md>) -> *const EvpMd {
    md.map_or(ptr::null(), |md| md.0)
}

struct MdCtx(NonNull<EvpMdCtx>);

impl Drop for MdCtx {
    fn drop(&mut self) {
        // SAFETY: the context came from `EVP_MD_CTX_new` and is owned only by this value.
        unsafe { EVP_MD_CTX_free(self.0.as_ptr()) };
    }
}

impl MdCtx {
    fn new() -> Option<MdCtx> {
        NonNull::new(EVP_MD_CTX_new()).map(MdCtx)
    }

    fn ptr(&self) -> *mut EvpMdCtx {
        self.0.as_ptr()
    }

    fn digest_init(&self, md: Md) -> bool {
        // SAFETY: the context is live and `md` is a static digest.
        unsafe { EVP_DigestInit_ex(self.ptr(), md.0, ptr::null_mut()) == 1 }
    }

    fn digest_update(&self, data: &[u8]) -> bool {
        // SAFETY: the context is live and `data` is readable for `data.len()` bytes.
        unsafe { EVP_DigestUpdate(self.ptr(), data.as_ptr().cast(), data.len()) == 1 }
    }

    fn digest_final(&self, out: &mut [u8]) -> Option<usize> {
        if out.len() < EVP_MAX_MD_SIZE {
            return None;
        }
        let mut n: c_uint = 0;
        // SAFETY: the context is live and `out` holds at least `EVP_MAX_MD_SIZE` writable bytes.
        let ok = unsafe { EVP_DigestFinal_ex(self.ptr(), out.as_mut_ptr(), &mut n) == 1 };
        ok.then_some(n as usize)
    }

    fn sign_init(
        &self,
        md: Option<Md>,
        pkey: PkeyRef<'_>,
        verify: bool,
    ) -> Option<Option<PkeyCtxRef<'_>>> {
        let init = if verify {
            EVP_DigestVerifyInit
        } else {
            EVP_DigestSignInit
        };
        let mut pctx = ptr::null_mut();
        // SAFETY: the context and key are live, `md` is null or a static digest, and `pctx` is a valid out-pointer.
        let ok = unsafe {
            init(
                self.ptr(),
                &mut pctx,
                md_ptr(md),
                ptr::null_mut(),
                pkey.ptr(),
            )
        } > 0;
        ok.then(|| NonNull::new(pctx).map(|pctx| PkeyCtxRef(pctx, PhantomData)))
    }

    fn sign(&self, out: Option<&mut [u8]>, n: &mut usize, data: &[u8]) -> bool {
        let out = out.map_or(ptr::null_mut(), |out| {
            *n = out.len();
            out.as_mut_ptr()
        });
        // SAFETY: the context was initialised for signing, `out` is null or writable for `*n` bytes, and `data` is readable.
        unsafe { EVP_DigestSign(self.ptr(), out, n, data.as_ptr(), data.len()) > 0 }
    }

    fn verify(&self, sig: &[u8], data: &[u8]) -> c_int {
        // SAFETY: the context was initialised for verifying and both slices are readable for their lengths.
        unsafe {
            EVP_DigestVerify(
                self.ptr(),
                sig.as_ptr(),
                sig.len(),
                data.as_ptr(),
                data.len(),
            )
        }
    }
}

const EVP_MAX_MD_SIZE: usize = 64;

pub fn digest(md: Md, data: &[u8]) -> Option<Vec<u8>> {
    let ctx = MdCtx::new()?;
    let size = usize::try_from(md.size()).ok()?;
    let mut out = vec![0u8; size.max(EVP_MAX_MD_SIZE)];
    if !(ctx.digest_init(md) && ctx.digest_update(data)) {
        return None;
    }
    let n = ctx.digest_final(&mut out)?;
    out.truncate(n);
    Some(out)
}

pub struct Pkey(NonNull<EvpPkey>);

impl Drop for Pkey {
    fn drop(&mut self) {
        // SAFETY: the key is owned only by this value and every borrow of it has ended.
        unsafe { EVP_PKEY_free(self.0.as_ptr()) };
    }
}

#[derive(Clone, Copy)]
pub struct PkeyRef<'a>(NonNull<EvpPkey>, PhantomData<&'a Pkey>);

struct Pkcs8(NonNull<Pkcs8PrivKeyInfo>);

impl Drop for Pkcs8 {
    fn drop(&mut self) {
        // SAFETY: the structure is owned only by this value.
        unsafe { PKCS8_PRIV_KEY_INFO_free(self.0.as_ptr()) };
    }
}

impl Pkcs8 {
    fn from_der(der: &[u8]) -> Option<Pkcs8> {
        let mut p = der.as_ptr();
        // SAFETY: `p` points at `der`, readable for the `der.len()` bytes passed as its length.
        let p8 = unsafe { d2i_PKCS8_PRIV_KEY_INFO(ptr::null_mut(), &mut p, der.len() as c_long) };
        NonNull::new(p8).map(Pkcs8)
    }

    fn from_pkey(pkey: PkeyRef<'_>) -> Option<Pkcs8> {
        // SAFETY: the key is live for the call.
        NonNull::new(unsafe { EVP_PKEY2PKCS8(pkey.ptr()) }).map(Pkcs8)
    }

    fn to_pkey(&self) -> Option<Pkey> {
        // SAFETY: the structure is live for the call and the returned key is a new reference.
        Pkey::from_raw(unsafe { EVP_PKCS82PKEY(self.0.as_ptr()) })
    }

    fn to_der(&self) -> Option<Vec<u8>> {
        let mut der = ptr::null_mut();
        // SAFETY: the structure is live and `der` is a valid out-pointer OpenSSL allocates into.
        let n = unsafe { i2d_PKCS8_PRIV_KEY_INFO(self.0.as_ptr(), &mut der) };
        if n <= 0 {
            return None;
        }
        // SAFETY: on success `der` holds `n` bytes OpenSSL allocated for this caller alone.
        Some(unsafe { take_openssl_bytes(der, n as usize, true) })
    }
}

impl Pkey {
    fn from_raw(p: *mut EvpPkey) -> Option<Pkey> {
        NonNull::new(p).map(Pkey)
    }

    pub fn as_ref(&self) -> PkeyRef<'_> {
        PkeyRef(self.0, PhantomData)
    }

    pub fn from_spki(der: &[u8]) -> Option<Pkey> {
        let mut p = der.as_ptr();
        // SAFETY: `p` points at `der`, readable for the `der.len()` bytes passed as its length.
        Pkey::from_raw(unsafe { d2i_PUBKEY(ptr::null_mut(), &mut p, der.len() as c_long) })
    }

    pub fn from_pkcs8(der: &[u8]) -> Option<Pkey> {
        Pkcs8::from_der(der)?.to_pkey()
    }

    pub fn raw_public(keytype: &CStr, key: &[u8]) -> Option<Pkey> {
        // SAFETY: `keytype` is NUL-terminated and `key` is readable for `key.len()` bytes.
        Pkey::from_raw(unsafe {
            EVP_PKEY_new_raw_public_key_ex(
                ptr::null_mut(),
                keytype.as_ptr(),
                ptr::null(),
                key.as_ptr(),
                key.len(),
            )
        })
    }

    pub fn raw_private(keytype: &CStr, key: &[u8]) -> Option<Pkey> {
        // SAFETY: `keytype` is NUL-terminated and `key` is readable for `key.len()` bytes.
        Pkey::from_raw(unsafe {
            EVP_PKEY_new_raw_private_key_ex(
                ptr::null_mut(),
                keytype.as_ptr(),
                ptr::null(),
                key.as_ptr(),
                key.len(),
            )
        })
    }
}

impl<'a> PkeyRef<'a> {
    fn ptr(self) -> *mut EvpPkey {
        self.0.as_ptr()
    }

    pub fn bits(self) -> i32 {
        // SAFETY: the borrowed key is live.
        unsafe { EVP_PKEY_get_bits(self.ptr()) }
    }

    pub fn dup(self) -> Option<Pkey> {
        // SAFETY: the borrowed key is live and the copy is a new, separately owned key.
        Pkey::from_raw(unsafe { EVP_PKEY_dup(self.ptr()) })
    }

    fn raw_key(self, public: bool) -> Option<Vec<u8>> {
        let get = if public {
            EVP_PKEY_get_raw_public_key
        } else {
            EVP_PKEY_get_raw_private_key
        };
        let mut n = 0usize;
        // SAFETY: the key is live; a null buffer asks OpenSSL only for the length into `n`.
        if unsafe { get(self.ptr(), ptr::null_mut(), &mut n) } <= 0 || n == 0 {
            return None;
        }
        let mut out = vec![0u8; n];
        // SAFETY: the key is live and `out` is writable for the `n` bytes reported in `n`.
        if unsafe { get(self.ptr(), out.as_mut_ptr(), &mut n) } <= 0 {
            cleanse(&mut out);
            return None;
        }
        out.truncate(n);
        Some(out)
    }

    pub fn raw_public_key(self) -> Option<Vec<u8>> {
        self.raw_key(true)
    }

    pub fn raw_private_key(self) -> Option<Vec<u8>> {
        self.raw_key(false)
    }

    pub fn encoded_public_key(self) -> Option<Vec<u8>> {
        let mut buf = ptr::null_mut();
        // SAFETY: the key is live and `buf` is a valid out-pointer OpenSSL allocates into.
        let n = unsafe { EVP_PKEY_get1_encoded_public_key(self.ptr(), &mut buf) };
        if buf.is_null() {
            return None;
        }
        // SAFETY: a non-null `buf` holds the `n` bytes OpenSSL allocated for this caller alone.
        Some(unsafe { take_openssl_bytes(buf, n, false) })
    }

    pub fn spki(self) -> Option<Vec<u8>> {
        let mut der = ptr::null_mut();
        // SAFETY: the key is live and `der` is a valid out-pointer OpenSSL allocates into.
        let n = unsafe { i2d_PUBKEY(self.ptr(), &mut der) };
        if n <= 0 {
            return None;
        }
        // SAFETY: on success `der` holds `n` bytes OpenSSL allocated for this caller alone.
        Some(unsafe { take_openssl_bytes(der, n as usize, false) })
    }

    pub fn pkcs8(self) -> Option<Vec<u8>> {
        Pkcs8::from_pkey(self)?.to_der()
    }

    pub fn bn_param(self, name: &CStr) -> Option<Vec<u8>> {
        let mut bn = ptr::null_mut();
        // SAFETY: the key is live, `name` is NUL-terminated and `bn` is a valid out-pointer.
        if unsafe { EVP_PKEY_get_bn_param(self.ptr(), name.as_ptr(), &mut bn) } <= 0 {
            return None;
        }
        Some(Bn(NonNull::new(bn)?).as_ref().to_vec())
    }
}

#[derive(Clone, Copy)]
struct PkeyCtxRef<'a>(NonNull<EvpPkeyCtx>, PhantomData<&'a ()>);

impl PkeyCtxRef<'_> {
    fn ptr(self) -> *mut EvpPkeyCtx {
        self.0.as_ptr()
    }

    fn set_rsa_padding(self, mode: c_int) -> bool {
        // SAFETY: the borrowed context is live.
        unsafe { EVP_PKEY_CTX_set_rsa_padding(self.ptr(), mode) > 0 }
    }

    fn set_rsa_pss_saltlen(self, salt: c_int) -> bool {
        // SAFETY: the borrowed context is live.
        unsafe { EVP_PKEY_CTX_set_rsa_pss_saltlen(self.ptr(), salt) > 0 }
    }
}

struct PkeyCtx(NonNull<EvpPkeyCtx>);

impl Drop for PkeyCtx {
    fn drop(&mut self) {
        // SAFETY: the context is owned only by this value and every borrow of it has ended.
        unsafe { EVP_PKEY_CTX_free(self.0.as_ptr()) };
    }
}

impl PkeyCtx {
    fn by_name(name: &CStr) -> Option<PkeyCtx> {
        // SAFETY: `name` is NUL-terminated; null library context and properties select the defaults.
        NonNull::new(unsafe {
            EVP_PKEY_CTX_new_from_name(ptr::null_mut(), name.as_ptr(), ptr::null())
        })
        .map(PkeyCtx)
    }

    fn for_key(pkey: PkeyRef<'_>) -> Option<PkeyCtx> {
        // SAFETY: the key is live; the new context takes its own reference to it.
        NonNull::new(unsafe {
            EVP_PKEY_CTX_new_from_pkey(ptr::null_mut(), pkey.ptr(), ptr::null())
        })
        .map(PkeyCtx)
    }

    fn ptr(&self) -> *mut EvpPkeyCtx {
        self.0.as_ptr()
    }

    fn as_ref(&self) -> PkeyCtxRef<'_> {
        PkeyCtxRef(self.0, PhantomData)
    }

    fn keygen_init(&self) -> bool {
        // SAFETY: the context is live.
        unsafe { EVP_PKEY_keygen_init(self.ptr()) > 0 }
    }

    fn generate(&self) -> Option<Pkey> {
        let mut pkey = ptr::null_mut();
        // SAFETY: the context is live and `pkey` is a valid out-pointer for the new key.
        if unsafe { EVP_PKEY_generate(self.ptr(), &mut pkey) } <= 0 {
            return None;
        }
        Pkey::from_raw(pkey)
    }

    fn fromdata(&self, selection: c_int, params: &BuiltParams) -> Option<Pkey> {
        // SAFETY: the context is live.
        if unsafe { EVP_PKEY_fromdata_init(self.ptr()) } <= 0 {
            return None;
        }
        let mut pkey = ptr::null_mut();
        // SAFETY: the context is live, `pkey` is a valid out-pointer and `params` is a built parameter array.
        let ok = unsafe { EVP_PKEY_fromdata(self.ptr(), &mut pkey, selection, params.0) } > 0;
        let pkey = Pkey::from_raw(pkey);
        if ok { pkey } else { None }
    }
}

pub fn keygen_named(name: &CStr) -> Option<Pkey> {
    let ctx = PkeyCtx::by_name(name)?;
    if !ctx.keygen_init() {
        return None;
    }
    ctx.generate()
}

pub fn keygen_ec(group: &CStr) -> Option<Pkey> {
    let ctx = PkeyCtx::by_name(c"EC")?;
    if !ctx.keygen_init() {
        return None;
    }
    // SAFETY: the context is live and `group` is NUL-terminated.
    if unsafe { EVP_PKEY_CTX_set_group_name(ctx.ptr(), group.as_ptr()) } <= 0 {
        return None;
    }
    ctx.generate()
}

struct Bn(NonNull<Bignum>);

impl Drop for Bn {
    fn drop(&mut self) {
        // SAFETY: the number is owned only by this value and every borrow of it has ended.
        unsafe { BN_clear_free(self.0.as_ptr()) };
    }
}

#[derive(Clone, Copy)]
struct BnRef<'a>(NonNull<Bignum>, PhantomData<&'a ()>);

impl Bn {
    fn word(w: u32) -> Option<Bn> {
        let bn = Bn(NonNull::new(BN_new())?);
        // SAFETY: the number is live and exclusively owned.
        (unsafe { BN_set_word(bn.0.as_ptr(), c_ulong::from(w)) } != 0).then_some(bn)
    }

    fn from_bytes(bytes: &[u8]) -> Option<Bn> {
        let len = c_len(bytes.len())?;
        // SAFETY: `bytes` is readable for `len` bytes and a null target asks for a new number.
        NonNull::new(unsafe { BN_bin2bn(bytes.as_ptr(), len, ptr::null_mut()) }).map(Bn)
    }

    fn into_raw(self) -> *mut Bignum {
        let p = self.0.as_ptr();
        core::mem::forget(self);
        p
    }

    fn as_ref(&self) -> BnRef<'_> {
        BnRef(self.0, PhantomData)
    }

    fn ptr(bn: &Option<Bn>) -> *const Bignum {
        bn.as_ref().map_or(ptr::null(), |bn| bn.0.as_ptr())
    }
}

impl BnRef<'_> {
    fn to_vec(self) -> Vec<u8> {
        // SAFETY: the borrowed number is live.
        let bits = unsafe { BN_num_bits(self.0.as_ptr()) };
        let mut out = vec![0u8; ((bits + 7) / 8) as usize];
        // SAFETY: the number is live and `out` holds the `BN_num_bytes` bytes it writes.
        unsafe { BN_bn2bin(self.0.as_ptr(), out.as_mut_ptr()) };
        out
    }

    fn to_padded(self, out: &mut [u8]) -> bool {
        c_len(out.len()).is_some_and(|len| {
            // SAFETY: the number is live and `out` is writable for the `len` bytes requested.
            unsafe { BN_bn2binpad(self.0.as_ptr(), out.as_mut_ptr(), len) >= 0 }
        })
    }
}

pub fn keygen_rsa(modulus_bits: i32, pubexp: u32) -> Option<Pkey> {
    let ctx = PkeyCtx::by_name(c"RSA");
    let e = Bn::word(pubexp)?;
    let ctx = ctx?;
    if !ctx.keygen_init() {
        return None;
    }
    // SAFETY: the context is live.
    if unsafe { EVP_PKEY_CTX_set_rsa_keygen_bits(ctx.ptr(), modulus_bits) } <= 0 {
        return None;
    }
    // SAFETY: the context and `e` are live; OpenSSL copies the exponent.
    if unsafe { EVP_PKEY_CTX_set1_rsa_keygen_pubexp(ctx.ptr(), e.0.as_ptr()) } <= 0 {
        return None;
    }
    ctx.generate()
}

pub enum Param<'a> {
    Utf8(&'static CStr, &'static CStr),
    Octets(&'static CStr, &'a [u8]),
    Bn(&'static CStr, Option<&'a [u8]>),
}

pub enum Selection {
    Public,
    Keypair,
}

struct ParamBuilder(NonNull<ParamBld>);

impl Drop for ParamBuilder {
    fn drop(&mut self) {
        // SAFETY: the builder is owned only by this value.
        unsafe { OSSL_PARAM_BLD_free(self.0.as_ptr()) };
    }
}

impl ParamBuilder {
    fn new() -> Option<ParamBuilder> {
        NonNull::new(OSSL_PARAM_BLD_new()).map(ParamBuilder)
    }

    fn push_utf8(&self, key: &'static CStr, value: &'static CStr) {
        // SAFETY: the builder is live and both strings are NUL-terminated statics.
        unsafe {
            OSSL_PARAM_BLD_push_utf8_string(self.0.as_ptr(), key.as_ptr(), value.as_ptr(), 0)
        };
    }

    fn push_octets(&self, key: &'static CStr, bytes: &[u8]) {
        // SAFETY: the builder is live, `key` is NUL-terminated and OpenSSL copies `bytes` before returning.
        unsafe {
            OSSL_PARAM_BLD_push_octet_string(
                self.0.as_ptr(),
                key.as_ptr(),
                bytes.as_ptr().cast(),
                bytes.len(),
            )
        };
    }

    fn push_bn(&self, key: &'static CStr, bn: &Option<Bn>) {
        // SAFETY: the builder is live, `key` is NUL-terminated and the number, if any, outlives `to_param`.
        unsafe { OSSL_PARAM_BLD_push_BN(self.0.as_ptr(), key.as_ptr(), Bn::ptr(bn)) };
    }

    fn to_param(&self) -> BuiltParams {
        // SAFETY: the builder is live and every number pushed into it is still alive.
        BuiltParams(unsafe { OSSL_PARAM_BLD_to_param(self.0.as_ptr()) })
    }
}

struct BuiltParams(*mut OsslParam);

impl Drop for BuiltParams {
    fn drop(&mut self) {
        // SAFETY: the array is null or came from `OSSL_PARAM_BLD_to_param` and is owned only by this value.
        unsafe { OSSL_PARAM_free(self.0) };
    }
}

pub fn pkey_fromdata(keytype: &CStr, selection: Selection, params: &[Param<'_>]) -> Option<Pkey> {
    let bld = ParamBuilder::new()?;
    let mut bignums = Vec::new();
    for param in params {
        match *param {
            Param::Utf8(key, value) => bld.push_utf8(key, value),
            Param::Octets(key, bytes) => bld.push_octets(key, bytes),
            Param::Bn(key, bytes) => {
                let bn = bytes.filter(|b| !b.is_empty()).and_then(Bn::from_bytes);
                bld.push_bn(key, &bn);
                bignums.push(bn);
            }
        }
    }
    let ossl_params = bld.to_param();
    let selection = match selection {
        Selection::Public => EVP_PKEY_PUBLIC_KEY,
        Selection::Keypair => EVP_PKEY_KEYPAIR,
    };
    PkeyCtx::by_name(keytype)?.fromdata(selection, &ossl_params)
}

struct Mac(NonNull<EvpMac>);

impl Drop for Mac {
    fn drop(&mut self) {
        // SAFETY: this value owns one reference to the fetched algorithm.
        unsafe { EVP_MAC_free(self.0.as_ptr()) };
    }
}

struct MacCtx(NonNull<EvpMacCtx>);

impl Drop for MacCtx {
    fn drop(&mut self) {
        // SAFETY: the context is owned only by this value.
        unsafe { EVP_MAC_CTX_free(self.0.as_ptr()) };
    }
}

impl MacCtx {
    fn hmac() -> Option<(Mac, MacCtx)> {
        // SAFETY: the name is NUL-terminated; null library context and properties select the defaults.
        let mac = Mac(NonNull::new(unsafe {
            EVP_MAC_fetch(ptr::null_mut(), c"HMAC".as_ptr(), ptr::null())
        })?);
        // SAFETY: `mac` is a live fetched algorithm; the context takes its own reference.
        let ctx = MacCtx(NonNull::new(unsafe { EVP_MAC_CTX_new(mac.0.as_ptr()) })?);
        Some((mac, ctx))
    }

    fn ptr(&self) -> *mut EvpMacCtx {
        self.0.as_ptr()
    }
}

pub fn hmac(digest_name: &CStr, key: Option<&[u8]>, data: &[u8]) -> Option<Vec<u8>> {
    let (_mac, ctx) = MacCtx::hmac()?;
    let params = [OsslParam::utf8(c"digest", digest_name), OsslParam::end()];
    let key_len = key.map_or(0, <[u8]>::len);
    // SAFETY: the context is live, `key` is null or readable for `key_len` bytes and `params` is end-terminated.
    if unsafe { EVP_MAC_init(ctx.ptr(), opt_ptr(key), key_len, params.as_ptr()) } == 0 {
        return None;
    }
    // SAFETY: the context is live and `data` is readable for `data.len()` bytes.
    if unsafe { EVP_MAC_update(ctx.ptr(), data.as_ptr(), data.len()) } == 0 {
        return None;
    }
    let mut n = 0usize;
    // SAFETY: the context is live; a null buffer asks only for the length into `n`.
    if unsafe { EVP_MAC_final(ctx.ptr(), ptr::null_mut(), &mut n, 0) } == 0 {
        return None;
    }
    let mut buf = vec![0u8; n];
    // SAFETY: the context is live and `buf` is writable for `buf.len()` bytes.
    if unsafe { EVP_MAC_final(ctx.ptr(), buf.as_mut_ptr(), &mut n, buf.len()) } == 0 {
        return None;
    }
    buf.truncate(n);
    Some(buf)
}

struct Kdf(NonNull<EvpKdf>);

impl Drop for Kdf {
    fn drop(&mut self) {
        // SAFETY: this value owns one reference to the fetched algorithm.
        unsafe { EVP_KDF_free(self.0.as_ptr()) };
    }
}

struct KdfCtx(NonNull<EvpKdfCtx>);

impl Drop for KdfCtx {
    fn drop(&mut self) {
        // SAFETY: the context is owned only by this value.
        unsafe { EVP_KDF_CTX_free(self.0.as_ptr()) };
    }
}

impl KdfCtx {
    fn hkdf() -> Option<(Kdf, KdfCtx)> {
        // SAFETY: the name is NUL-terminated; null library context and properties select the defaults.
        let kdf = Kdf(NonNull::new(unsafe {
            EVP_KDF_fetch(ptr::null_mut(), c"HKDF".as_ptr(), ptr::null())
        })?);
        // SAFETY: `kdf` is a live fetched algorithm; the context takes its own reference.
        let ctx = KdfCtx(NonNull::new(unsafe { EVP_KDF_CTX_new(kdf.0.as_ptr()) })?);
        Some((kdf, ctx))
    }
}

pub fn hkdf(
    digest_name: &CStr,
    key: Option<&[u8]>,
    salt: &[u8],
    info: &[u8],
    out: &mut [u8],
) -> bool {
    let Some((_kdf, ctx)) = KdfCtx::hkdf() else {
        return false;
    };
    let params = [
        OsslParam::utf8(c"digest", digest_name),
        OsslParam::octets(c"key", key),
        OsslParam::octets(c"salt", Some(salt)),
        OsslParam::octets(c"info", Some(info)),
        OsslParam::end(),
    ];
    // SAFETY: the context is live, `out` is writable for `out.len()` bytes and `params` is end-terminated over live buffers.
    unsafe { EVP_KDF_derive(ctx.0.as_ptr(), out.as_mut_ptr(), out.len(), params.as_ptr()) > 0 }
}

pub fn pbkdf2(
    password: Option<&[u8]>,
    salt: Option<&[u8]>,
    iterations: i32,
    md: Md,
    out: &mut [u8],
) -> bool {
    let (Some(pass_len), Some(salt_len), Some(out_len)) = (
        c_len(password.map_or(0, <[u8]>::len)),
        c_len(salt.map_or(0, <[u8]>::len)),
        c_len(out.len()),
    ) else {
        return false;
    };
    // SAFETY: each buffer is null with length zero or valid for its length, and `md` is a static digest.
    unsafe {
        PKCS5_PBKDF2_HMAC(
            opt_ptr(password).cast(),
            pass_len,
            opt_ptr(salt),
            salt_len,
            iterations,
            md.0,
            out_len,
            out.as_mut_ptr(),
        ) == 1
    }
}

pub enum Padding {
    Default,
    Pkcs1,
    Pss(Option<i32>),
}

fn sign_setup(pctx: Option<PkeyCtxRef<'_>>, padding: &Padding) -> bool {
    match *padding {
        Padding::Default => true,
        Padding::Pkcs1 => pctx.is_some_and(|pctx| pctx.set_rsa_padding(RSA_PKCS1_PADDING)),
        Padding::Pss(salt) => pctx.is_some_and(|pctx| {
            pctx.set_rsa_padding(RSA_PKCS1_PSS_PADDING)
                && salt.is_some_and(|salt| pctx.set_rsa_pss_saltlen(salt))
        }),
    }
}

pub fn digest_sign(
    pkey: PkeyRef<'_>,
    md: Option<Md>,
    padding: &Padding,
    data: &[u8],
) -> Option<Vec<u8>> {
    let ctx = MdCtx::new()?;
    let pctx = ctx.sign_init(md, pkey, false)?;
    let mut n = 0usize;
    if !(sign_setup(pctx, padding) && ctx.sign(None, &mut n, data)) {
        return None;
    }
    let mut out = vec![0u8; n];
    if !ctx.sign(Some(&mut out), &mut n, data) {
        return None;
    }
    out.truncate(n);
    Some(out)
}

pub fn digest_verify(
    pkey: PkeyRef<'_>,
    md: Option<Md>,
    padding: &Padding,
    sig: &[u8],
    data: &[u8],
) -> Option<bool> {
    let ctx = MdCtx::new()?;
    let pctx = ctx.sign_init(md, pkey, true)?;
    if !sign_setup(pctx, padding) {
        return None;
    }
    let verdict = ctx.verify(sig, data);
    if verdict < 0 {
        clear_errors();
    }
    Some(verdict == 1)
}

struct EcdsaSignature(NonNull<EcdsaSig>);

impl Drop for EcdsaSignature {
    fn drop(&mut self) {
        // SAFETY: the signature is owned only by this value and every borrow of it has ended.
        unsafe { ECDSA_SIG_free(self.0.as_ptr()) };
    }
}

impl EcdsaSignature {
    fn from_der(der: &[u8]) -> Option<EcdsaSignature> {
        let mut p = der.as_ptr();
        // SAFETY: `p` points at `der`, readable for the `der.len()` bytes passed as its length.
        let sig = unsafe { d2i_ECDSA_SIG(ptr::null_mut(), &mut p, der.len() as c_long) };
        NonNull::new(sig).map(EcdsaSignature)
    }

    fn components(&self) -> Option<(BnRef<'_>, BnRef<'_>)> {
        let (mut r, mut s) = (ptr::null(), ptr::null());
        // SAFETY: the signature is live and `r` and `s` are valid out-pointers for borrowed numbers.
        unsafe { ECDSA_SIG_get0(self.0.as_ptr(), &mut r, &mut s) };
        let r = NonNull::new(r.cast_mut())?;
        let s = NonNull::new(s.cast_mut())?;
        Some((BnRef(r, PhantomData), BnRef(s, PhantomData)))
    }

    fn set(&self, r: Bn, s: Bn) {
        // SAFETY: the signature is live and takes ownership of the two numbers released here.
        unsafe { ECDSA_SIG_set0(self.0.as_ptr(), r.into_raw(), s.into_raw()) };
    }

    fn to_der(&self) -> Option<Vec<u8>> {
        let mut der = ptr::null_mut();
        // SAFETY: the signature is live and `der` is a valid out-pointer OpenSSL allocates into.
        let n = unsafe { i2d_ECDSA_SIG(self.0.as_ptr(), &mut der) };
        if n <= 0 {
            return None;
        }
        // SAFETY: on success `der` holds `n` bytes OpenSSL allocated for this caller alone.
        Some(unsafe { take_openssl_bytes(der, n as usize, false) })
    }
}

pub fn ecdsa_der_to_raw(der: &[u8], order: usize) -> Option<Vec<u8>> {
    let sig = EcdsaSignature::from_der(der)?;
    let (r, s) = sig.components()?;
    let mut out = vec![0u8; order * 2];
    let (r_out, s_out) = out.split_at_mut(order);
    (r.to_padded(r_out) && s.to_padded(s_out)).then_some(out)
}

pub fn ecdsa_raw_to_der(raw: &[u8], order: usize) -> Option<Vec<u8>> {
    if raw.len() != order * 2 {
        return None;
    }
    let sig = NonNull::new(ECDSA_SIG_new()).map(EcdsaSignature);
    let r = Bn::from_bytes(&raw[..order])?;
    let s = Bn::from_bytes(&raw[order..])?;
    let sig = sig?;
    sig.set(r, s);
    sig.to_der()
}

pub fn rsa_oaep(
    pkey: PkeyRef<'_>,
    md: Md,
    label: Option<&[u8]>,
    data: &[u8],
    encrypt: bool,
) -> Option<Vec<u8>> {
    let ctx = PkeyCtx::for_key(pkey)?;
    let c = ctx.ptr();
    let init = if encrypt {
        EVP_PKEY_encrypt_init
    } else {
        EVP_PKEY_decrypt_init
    };
    // SAFETY: the context is live.
    if unsafe { init(c) } <= 0 || !ctx.as_ref().set_rsa_padding(RSA_PKCS1_OAEP_PADDING) {
        return None;
    }
    // SAFETY: the context is live and `md` is a static digest.
    if unsafe { EVP_PKEY_CTX_set_rsa_oaep_md(c, md.0) } <= 0 {
        return None;
    }
    // SAFETY: the context is live and `md` is a static digest.
    if unsafe { EVP_PKEY_CTX_set_rsa_mgf1_md(c, md.0) } <= 0 {
        return None;
    }
    if let Some(label) = label.filter(|label| !label.is_empty()) {
        let len = c_len(label.len())?;
        // SAFETY: `label` is readable for `label.len()` bytes and `FILE` is NUL-terminated.
        let copy = unsafe { CRYPTO_memdup(label.as_ptr().cast(), label.len(), FILE.as_ptr(), 0) };
        if copy.is_null() {
            return None;
        }
        // SAFETY: the context is live and takes ownership of `copy`, an OpenSSL allocation of `len` bytes, on success.
        if unsafe { EVP_PKEY_CTX_set0_rsa_oaep_label(c, copy, len) } <= 0 {
            // SAFETY: the context refused `copy`, so it is still this function's OpenSSL allocation.
            unsafe { CRYPTO_free(copy, FILE.as_ptr(), 0) };
            return None;
        }
    }
    let run = if encrypt {
        EVP_PKEY_encrypt
    } else {
        EVP_PKEY_decrypt
    };
    let mut n = 0usize;
    // SAFETY: the context is initialised; a null output asks only for the length into `n`.
    if unsafe { run(c, ptr::null_mut(), &mut n, data.as_ptr(), data.len()) } <= 0 {
        return None;
    }
    let mut out = vec![0u8; n];
    // SAFETY: the context is initialised, `out` is writable for the `n` bytes in `n` and `data` is readable.
    if unsafe { run(c, out.as_mut_ptr(), &mut n, data.as_ptr(), data.len()) } <= 0 {
        cleanse(&mut out);
        return None;
    }
    let tail = n.min(out.len());
    cleanse(&mut out[tail..]);
    out.truncate(n);
    Some(out)
}

pub fn derive(pkey: PkeyRef<'_>, peer: PkeyRef<'_>) -> Option<Vec<u8>> {
    let ctx = PkeyCtx::for_key(pkey)?;
    let c = ctx.ptr();
    // SAFETY: the context is live.
    if unsafe { EVP_PKEY_derive_init(c) } <= 0 {
        return None;
    }
    // SAFETY: the context and the peer key are live; the context takes its own reference to the peer.
    if unsafe { EVP_PKEY_derive_set_peer(c, peer.ptr()) } <= 0 {
        return None;
    }
    let mut n = 0usize;
    // SAFETY: the context is initialised; a null output asks only for the length into `n`.
    if unsafe { EVP_PKEY_derive(c, ptr::null_mut(), &mut n) } <= 0 {
        return None;
    }
    let mut out = vec![0u8; n];
    // SAFETY: the context is initialised and `out` is writable for the `n` bytes in `n`.
    if unsafe { EVP_PKEY_derive(c, out.as_mut_ptr(), &mut n) } <= 0 {
        cleanse(&mut out);
        return None;
    }
    let tail = n.min(out.len());
    cleanse(&mut out[tail..]);
    out.truncate(n);
    Some(out)
}

#[derive(Clone, Copy)]
pub struct Cipher(*const EvpCipher);

pub enum AesMode {
    Gcm,
    Cbc,
    Ctr,
    Kw,
}

impl Cipher {
    pub fn aes(mode: AesMode, bits: i32) -> Option<Cipher> {
        let table: [extern "C" fn() -> *const EvpCipher; 3] = match mode {
            AesMode::Gcm => [EVP_aes_128_gcm, EVP_aes_192_gcm, EVP_aes_256_gcm],
            AesMode::Cbc => [EVP_aes_128_cbc, EVP_aes_192_cbc, EVP_aes_256_cbc],
            AesMode::Ctr => [EVP_aes_128_ctr, EVP_aes_192_ctr, EVP_aes_256_ctr],
            AesMode::Kw => [EVP_aes_128_wrap, EVP_aes_192_wrap, EVP_aes_256_wrap],
        };
        let get = match bits {
            128 => table[0],
            192 => table[1],
            256 => table[2],
            _ => return None,
        };
        let cipher = get();
        (!cipher.is_null()).then_some(Cipher(cipher))
    }
}

pub struct CipherCtx(NonNull<EvpCipherCtx>);

impl Drop for CipherCtx {
    fn drop(&mut self) {
        // SAFETY: the context is owned only by this value.
        unsafe { EVP_CIPHER_CTX_free(self.0.as_ptr()) };
    }
}

impl CipherCtx {
    pub fn new() -> Option<CipherCtx> {
        NonNull::new(EVP_CIPHER_CTX_new()).map(CipherCtx)
    }

    fn ptr(&self) -> *mut EvpCipherCtx {
        self.0.as_ptr()
    }

    pub fn allow_wrap(&self) {
        // SAFETY: the context is live.
        unsafe { EVP_CIPHER_CTX_set_flags(self.ptr(), EVP_CIPHER_CTX_FLAG_WRAP_ALLOW) };
    }

    pub fn init_cipher(&self, cipher: Cipher, encrypt: bool) -> bool {
        // SAFETY: the context is live, `cipher` is a static cipher and null key and IV are set later.
        unsafe {
            EVP_CipherInit_ex(
                self.ptr(),
                cipher.0,
                ptr::null_mut(),
                ptr::null(),
                ptr::null(),
                c_int::from(encrypt),
            ) != 0
        }
    }

    pub fn init_key(&self, key: &[u8], iv: Option<&[u8]>, encrypt: bool) -> bool {
        // SAFETY: the context is live with a cipher set, and the caller sizes `key` and `iv` to it.
        unsafe {
            EVP_CipherInit_ex(
                self.ptr(),
                ptr::null(),
                ptr::null_mut(),
                key.as_ptr(),
                opt_ptr(iv),
                c_int::from(encrypt),
            ) != 0
        }
    }

    pub fn set_iv_len(&self, len: usize) -> bool {
        c_len(len).is_some_and(|len| {
            // SAFETY: the context is live and this control takes no pointer argument.
            unsafe {
                EVP_CIPHER_CTX_ctrl(self.ptr(), EVP_CTRL_AEAD_SET_IVLEN, len, ptr::null_mut()) != 0
            }
        })
    }

    pub fn aad(&self, aad: &[u8]) -> bool {
        let mut outl = 0;
        c_len(aad.len()).is_some_and(|len| {
            // SAFETY: the context is live, a null output marks AAD and `aad` is readable for `len` bytes.
            unsafe {
                EVP_CipherUpdate(self.ptr(), ptr::null_mut(), &mut outl, aad.as_ptr(), len) != 0
            }
        })
    }

    pub fn update(&self, out: &mut [u8], input: &[u8]) -> Option<usize> {
        let len = c_len(input.len())?;
        if out.len() < input.len() + AES_BLOCK {
            return None;
        }
        let mut outl = 0;
        // SAFETY: the context is live and `out` has room for `input.len()` plus one block.
        (unsafe { EVP_CipherUpdate(self.ptr(), out.as_mut_ptr(), &mut outl, input.as_ptr(), len) }
            != 0)
            .then_some(outl as usize)
    }

    pub fn finish(&self, out: &mut [u8]) -> Option<usize> {
        if out.len() < AES_BLOCK {
            return None;
        }
        let mut outl = 0;
        // SAFETY: the context is live and `out` has room for the one block a final call can write.
        (unsafe { EVP_CipherFinal_ex(self.ptr(), out.as_mut_ptr(), &mut outl) } != 0)
            .then_some(outl as usize)
    }

    pub fn set_tag(&self, tag: &[u8]) -> bool {
        c_len(tag.len()).is_some_and(|len| {
            // SAFETY: the context is live and OpenSSL only reads the `len` bytes of `tag`.
            unsafe {
                EVP_CIPHER_CTX_ctrl(
                    self.ptr(),
                    EVP_CTRL_AEAD_SET_TAG,
                    len,
                    tag.as_ptr().cast_mut().cast(),
                ) != 0
            }
        })
    }

    pub fn get_tag(&self, tag: &mut [u8]) -> bool {
        c_len(tag.len()).is_some_and(|len| {
            // SAFETY: the context is live and `tag` is writable for `len` bytes.
            unsafe {
                EVP_CIPHER_CTX_ctrl(
                    self.ptr(),
                    EVP_CTRL_AEAD_GET_TAG,
                    len,
                    tag.as_mut_ptr().cast(),
                ) != 0
            }
        })
    }

    fn encrypt_init(&self, cipher: Cipher, key: &[u8], iv: &[u8; 16]) -> bool {
        // SAFETY: the context is live, `cipher` is a static cipher and the caller sizes `key` to it.
        unsafe {
            EVP_EncryptInit_ex(
                self.ptr(),
                cipher.0,
                ptr::null_mut(),
                key.as_ptr(),
                iv.as_ptr(),
            ) != 0
        }
    }

    fn encrypt_update(&self, out: &mut [u8], input: &[u8]) -> Option<usize> {
        let len = c_len(input.len())?;
        if out.len() < input.len() + AES_BLOCK {
            return None;
        }
        let mut outl = 0;
        // SAFETY: the context is live and `out` has room for `input.len()` plus one block.
        (unsafe { EVP_EncryptUpdate(self.ptr(), out.as_mut_ptr(), &mut outl, input.as_ptr(), len) }
            != 0)
            .then_some(outl as usize)
    }

    fn encrypt_final(&self, out: &mut [u8]) -> bool {
        let mut outl = 0;
        if out.len() < AES_BLOCK {
            return false;
        }
        // SAFETY: the context is live and `out` has room for the one block a final call can write.
        unsafe { EVP_EncryptFinal_ex(self.ptr(), out.as_mut_ptr(), &mut outl) != 0 }
    }
}

pub fn aes_ctr_pass(
    cipher: Cipher,
    key: &[u8],
    counter: &[u8; 16],
    input: &[u8],
    out: &mut [u8],
) -> bool {
    let Some(ctx) = CipherCtx::new() else {
        return false;
    };
    if c_len(input.len()).is_none() || out.len() < input.len() + AES_BLOCK {
        return false;
    }
    ctx.encrypt_init(cipher, key, counter)
        && ctx
            .encrypt_update(out, input)
            .and_then(|outl| out.get_mut(outl..))
            .is_some_and(|rest| ctx.encrypt_final(rest))
}
