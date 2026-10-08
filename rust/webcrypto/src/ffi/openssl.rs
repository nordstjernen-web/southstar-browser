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
    fn ERR_peek_last_error() -> c_ulong;
    fn ERR_error_string_n(e: c_ulong, buf: *mut c_char, len: usize);
    fn ERR_clear_error();
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

    fn EVP_sha1() -> *const EvpMd;
    fn EVP_sha256() -> *const EvpMd;
    fn EVP_sha384() -> *const EvpMd;
    fn EVP_sha512() -> *const EvpMd;
    fn EVP_sha3_256() -> *const EvpMd;
    fn EVP_sha3_384() -> *const EvpMd;
    fn EVP_sha3_512() -> *const EvpMd;
    fn EVP_MD_get_size(md: *const EvpMd) -> c_int;
    fn EVP_MD_CTX_new() -> *mut EvpMdCtx;
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

    fn BN_new() -> *mut Bignum;
    fn BN_free(bn: *mut Bignum);
    fn BN_clear_free(bn: *mut Bignum);
    fn BN_set_word(bn: *mut Bignum, w: c_ulong) -> c_int;
    fn BN_bin2bn(s: *const u8, len: c_int, ret: *mut Bignum) -> *mut Bignum;
    fn BN_bn2bin(bn: *const Bignum, to: *mut u8) -> c_int;
    fn BN_bn2binpad(bn: *const Bignum, to: *mut u8, tolen: c_int) -> c_int;
    fn BN_num_bits(bn: *const Bignum) -> c_int;

    fn OSSL_PARAM_BLD_new() -> *mut ParamBld;
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
    fn OSSL_PARAM_construct_utf8_string(
        key: *const c_char,
        buf: *mut c_char,
        bsize: usize,
    ) -> OsslParam;
    fn OSSL_PARAM_construct_octet_string(
        key: *const c_char,
        buf: *mut c_void,
        bsize: usize,
    ) -> OsslParam;
    fn OSSL_PARAM_construct_end() -> OsslParam;

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
    fn ECDSA_SIG_new() -> *mut EcdsaSig;
    fn ECDSA_SIG_free(sig: *mut EcdsaSig);
    fn ECDSA_SIG_get0(sig: *const EcdsaSig, pr: *mut *const Bignum, ps: *mut *const Bignum);
    fn ECDSA_SIG_set0(sig: *mut EcdsaSig, r: *mut Bignum, s: *mut Bignum) -> c_int;

    fn EVP_aes_128_gcm() -> *const EvpCipher;
    fn EVP_aes_192_gcm() -> *const EvpCipher;
    fn EVP_aes_256_gcm() -> *const EvpCipher;
    fn EVP_aes_128_cbc() -> *const EvpCipher;
    fn EVP_aes_192_cbc() -> *const EvpCipher;
    fn EVP_aes_256_cbc() -> *const EvpCipher;
    fn EVP_aes_128_ctr() -> *const EvpCipher;
    fn EVP_aes_192_ctr() -> *const EvpCipher;
    fn EVP_aes_256_ctr() -> *const EvpCipher;
    fn EVP_aes_128_wrap() -> *const EvpCipher;
    fn EVP_aes_192_wrap() -> *const EvpCipher;
    fn EVP_aes_256_wrap() -> *const EvpCipher;
    fn EVP_CIPHER_CTX_new() -> *mut EvpCipherCtx;
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

fn opt_ptr(bytes: Option<&[u8]>) -> *const u8 {
    bytes.map_or(ptr::null(), <[u8]>::as_ptr)
}

fn c_len(len: usize) -> Option<c_int> {
    c_int::try_from(len).ok()
}

unsafe fn openssl_free(p: *mut u8) {
    unsafe { CRYPTO_free(p.cast(), FILE.as_ptr(), 0) };
}

unsafe fn take_openssl_bytes(p: *mut u8, len: usize) -> Vec<u8> {
    let bytes = unsafe { core::slice::from_raw_parts(p, len) }.to_vec();
    unsafe { openssl_free(p) };
    bytes
}

pub fn error_message(prefix: &str) -> String {
    let code = unsafe { ERR_peek_last_error() };
    let message = if code != 0 {
        let mut buf = [0 as c_char; 256];
        unsafe { ERR_error_string_n(code, buf.as_mut_ptr(), buf.len()) };
        let text = unsafe { CStr::from_ptr(buf.as_ptr()) }.to_string_lossy();
        format!("{prefix}: {text}")
    } else {
        prefix.to_owned()
    };
    unsafe { ERR_clear_error() };
    message
}

pub fn clear_errors() {
    unsafe { ERR_clear_error() };
}

pub fn cleanse(buf: &mut [u8]) {
    unsafe { OPENSSL_cleanse(buf.as_mut_ptr().cast(), buf.len()) };
}

pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len()
        && unsafe { CRYPTO_memcmp(a.as_ptr().cast(), b.as_ptr().cast(), a.len()) } == 0
}

pub fn rand_bytes(buf: &mut [u8]) -> bool {
    c_len(buf.len()).is_some_and(|len| unsafe { RAND_bytes(buf.as_mut_ptr(), len) } == 1)
}

#[derive(Clone, Copy)]
pub struct Md(*const EvpMd);

impl Md {
    fn from(md: *const EvpMd) -> Option<Md> {
        (!md.is_null()).then_some(Md(md))
    }

    pub fn sha1() -> Option<Md> {
        Md::from(unsafe { EVP_sha1() })
    }

    pub fn sha256() -> Option<Md> {
        Md::from(unsafe { EVP_sha256() })
    }

    pub fn sha384() -> Option<Md> {
        Md::from(unsafe { EVP_sha384() })
    }

    pub fn sha512() -> Option<Md> {
        Md::from(unsafe { EVP_sha512() })
    }

    pub fn sha3_256() -> Option<Md> {
        Md::from(unsafe { EVP_sha3_256() })
    }

    pub fn sha3_384() -> Option<Md> {
        Md::from(unsafe { EVP_sha3_384() })
    }

    pub fn sha3_512() -> Option<Md> {
        Md::from(unsafe { EVP_sha3_512() })
    }

    pub fn size(self) -> i32 {
        unsafe { EVP_MD_get_size(self.0) }
    }
}

struct MdCtx(NonNull<EvpMdCtx>);

impl MdCtx {
    fn new() -> Option<MdCtx> {
        NonNull::new(unsafe { EVP_MD_CTX_new() }).map(MdCtx)
    }
}

impl Drop for MdCtx {
    fn drop(&mut self) {
        unsafe { EVP_MD_CTX_free(self.0.as_ptr()) };
    }
}

pub fn digest(md: Md, data: &[u8]) -> Option<Vec<u8>> {
    let ctx = MdCtx::new()?;
    let mut out = vec![0u8; usize::try_from(md.size()).ok()?];
    let mut n: c_uint = 0;
    let ok = unsafe {
        EVP_DigestInit_ex(ctx.0.as_ptr(), md.0, ptr::null_mut()) == 1
            && EVP_DigestUpdate(ctx.0.as_ptr(), data.as_ptr().cast(), data.len()) == 1
            && EVP_DigestFinal_ex(ctx.0.as_ptr(), out.as_mut_ptr(), &mut n) == 1
    };
    if !ok {
        return None;
    }
    out.truncate(n as usize);
    Some(out)
}

pub struct Pkey(NonNull<EvpPkey>);

impl Drop for Pkey {
    fn drop(&mut self) {
        unsafe { EVP_PKEY_free(self.0.as_ptr()) };
    }
}

#[derive(Clone, Copy)]
pub struct PkeyRef<'a>(NonNull<EvpPkey>, PhantomData<&'a Pkey>);

impl Pkey {
    fn from_raw(p: *mut EvpPkey) -> Option<Pkey> {
        NonNull::new(p).map(Pkey)
    }

    pub unsafe fn from_owned(p: *mut c_void) -> Option<Pkey> {
        Pkey::from_raw(p.cast())
    }

    pub fn as_ref(&self) -> PkeyRef<'_> {
        PkeyRef(self.0, PhantomData)
    }

    pub fn into_raw(self) -> *mut c_void {
        let p = self.0.as_ptr();
        core::mem::forget(self);
        p.cast()
    }

    pub fn from_spki(der: &[u8]) -> Option<Pkey> {
        let mut p = der.as_ptr();
        Pkey::from_raw(unsafe { d2i_PUBKEY(ptr::null_mut(), &mut p, der.len() as c_long) })
    }

    pub fn from_pkcs8(der: &[u8]) -> Option<Pkey> {
        let mut p = der.as_ptr();
        let p8 = unsafe { d2i_PKCS8_PRIV_KEY_INFO(ptr::null_mut(), &mut p, der.len() as c_long) };
        if p8.is_null() {
            return None;
        }
        let pkey = unsafe { EVP_PKCS82PKEY(p8) };
        unsafe { PKCS8_PRIV_KEY_INFO_free(p8) };
        Pkey::from_raw(pkey)
    }

    pub fn raw_public(keytype: &CStr, key: &[u8]) -> Option<Pkey> {
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
    pub unsafe fn from_ptr(p: *mut c_void) -> Option<PkeyRef<'a>> {
        NonNull::new(p.cast()).map(|p| PkeyRef(p, PhantomData))
    }

    fn ptr(self) -> *mut EvpPkey {
        self.0.as_ptr()
    }

    pub fn bits(self) -> i32 {
        unsafe { EVP_PKEY_get_bits(self.ptr()) }
    }

    pub fn dup(self) -> Option<Pkey> {
        Pkey::from_raw(unsafe { EVP_PKEY_dup(self.ptr()) })
    }

    fn raw_key(
        self,
        get: unsafe extern "C" fn(*const EvpPkey, *mut u8, *mut usize) -> c_int,
    ) -> Option<Vec<u8>> {
        let mut n = 0usize;
        if unsafe { get(self.ptr(), ptr::null_mut(), &mut n) } <= 0 || n == 0 {
            return None;
        }
        let mut out = vec![0u8; n];
        if unsafe { get(self.ptr(), out.as_mut_ptr(), &mut n) } <= 0 {
            cleanse(&mut out);
            return None;
        }
        out.truncate(n);
        Some(out)
    }

    pub fn raw_public_key(self) -> Option<Vec<u8>> {
        self.raw_key(EVP_PKEY_get_raw_public_key)
    }

    pub fn raw_private_key(self) -> Option<Vec<u8>> {
        self.raw_key(EVP_PKEY_get_raw_private_key)
    }

    pub fn encoded_public_key(self) -> Option<Vec<u8>> {
        let mut buf = ptr::null_mut();
        let n = unsafe { EVP_PKEY_get1_encoded_public_key(self.ptr(), &mut buf) };
        if buf.is_null() {
            return None;
        }
        Some(unsafe { take_openssl_bytes(buf, n) })
    }

    pub fn spki(self) -> Option<Vec<u8>> {
        let mut der = ptr::null_mut();
        let n = unsafe { i2d_PUBKEY(self.ptr(), &mut der) };
        if n <= 0 {
            return None;
        }
        Some(unsafe { take_openssl_bytes(der, n as usize) })
    }

    pub fn pkcs8(self) -> Option<Vec<u8>> {
        let p8 = unsafe { EVP_PKEY2PKCS8(self.ptr()) };
        if p8.is_null() {
            return None;
        }
        let mut der = ptr::null_mut();
        let n = unsafe { i2d_PKCS8_PRIV_KEY_INFO(p8, &mut der) };
        unsafe { PKCS8_PRIV_KEY_INFO_free(p8) };
        if n <= 0 {
            return None;
        }
        let bytes = unsafe { core::slice::from_raw_parts(der, n as usize) }.to_vec();
        unsafe {
            OPENSSL_cleanse(der.cast(), n as usize);
            openssl_free(der);
        }
        Some(bytes)
    }

    pub fn bn_param(self, name: &CStr) -> Option<Vec<u8>> {
        let mut bn = ptr::null_mut();
        if unsafe { EVP_PKEY_get_bn_param(self.ptr(), name.as_ptr(), &mut bn) } <= 0 || bn.is_null()
        {
            return None;
        }
        let n = (unsafe { BN_num_bits(bn) } + 7) / 8;
        let mut out = vec![0u8; n as usize];
        unsafe {
            BN_bn2bin(bn, out.as_mut_ptr());
            BN_clear_free(bn);
        }
        Some(out)
    }
}

struct PkeyCtx(NonNull<EvpPkeyCtx>);

impl Drop for PkeyCtx {
    fn drop(&mut self) {
        unsafe { EVP_PKEY_CTX_free(self.0.as_ptr()) };
    }
}

impl PkeyCtx {
    fn by_name(name: &CStr) -> Option<PkeyCtx> {
        NonNull::new(unsafe {
            EVP_PKEY_CTX_new_from_name(ptr::null_mut(), name.as_ptr(), ptr::null())
        })
        .map(PkeyCtx)
    }

    fn for_key(pkey: PkeyRef<'_>) -> Option<PkeyCtx> {
        NonNull::new(unsafe {
            EVP_PKEY_CTX_new_from_pkey(ptr::null_mut(), pkey.ptr(), ptr::null())
        })
        .map(PkeyCtx)
    }

    fn ptr(&self) -> *mut EvpPkeyCtx {
        self.0.as_ptr()
    }

    fn generate(&self) -> Option<Pkey> {
        let mut pkey = ptr::null_mut();
        if unsafe { EVP_PKEY_generate(self.ptr(), &mut pkey) } <= 0 {
            return None;
        }
        Pkey::from_raw(pkey)
    }
}

pub fn keygen_named(name: &CStr) -> Option<Pkey> {
    let ctx = PkeyCtx::by_name(name)?;
    if unsafe { EVP_PKEY_keygen_init(ctx.ptr()) } <= 0 {
        return None;
    }
    ctx.generate()
}

pub fn keygen_ec(group: &CStr) -> Option<Pkey> {
    let ctx = PkeyCtx::by_name(c"EC")?;
    if unsafe { EVP_PKEY_keygen_init(ctx.ptr()) } <= 0
        || unsafe { EVP_PKEY_CTX_set_group_name(ctx.ptr(), group.as_ptr()) } <= 0
    {
        return None;
    }
    ctx.generate()
}

struct Bn(NonNull<Bignum>);

impl Drop for Bn {
    fn drop(&mut self) {
        unsafe { BN_free(self.0.as_ptr()) };
    }
}

impl Bn {
    fn word(w: u32) -> Option<Bn> {
        let bn = Bn(NonNull::new(unsafe { BN_new() })?);
        (unsafe { BN_set_word(bn.0.as_ptr(), c_ulong::from(w)) } != 0).then_some(bn)
    }

    fn from_bytes(bytes: &[u8]) -> Option<Bn> {
        let len = c_len(bytes.len())?;
        NonNull::new(unsafe { BN_bin2bn(bytes.as_ptr(), len, ptr::null_mut()) }).map(Bn)
    }

    fn into_raw(self) -> *mut Bignum {
        let p = self.0.as_ptr();
        core::mem::forget(self);
        p
    }

    fn ptr(bn: &Option<Bn>) -> *const Bignum {
        bn.as_ref().map_or(ptr::null(), |bn| bn.0.as_ptr())
    }
}

pub fn keygen_rsa(modulus_bits: i32, pubexp: u32) -> Option<Pkey> {
    let ctx = PkeyCtx::by_name(c"RSA");
    let e = Bn::word(pubexp)?;
    let ctx = ctx?;
    if unsafe { EVP_PKEY_keygen_init(ctx.ptr()) } <= 0
        || unsafe { EVP_PKEY_CTX_set_rsa_keygen_bits(ctx.ptr(), modulus_bits) } <= 0
        || unsafe { EVP_PKEY_CTX_set1_rsa_keygen_pubexp(ctx.ptr(), e.0.as_ptr()) } <= 0
    {
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

pub fn pkey_fromdata(keytype: &CStr, selection: Selection, params: &[Param<'_>]) -> Option<Pkey> {
    let bld = unsafe { OSSL_PARAM_BLD_new() };
    let mut bignums = Vec::new();
    for param in params {
        match *param {
            Param::Utf8(key, value) => unsafe {
                OSSL_PARAM_BLD_push_utf8_string(bld, key.as_ptr(), value.as_ptr(), 0);
            },
            Param::Octets(key, bytes) => unsafe {
                OSSL_PARAM_BLD_push_octet_string(
                    bld,
                    key.as_ptr(),
                    bytes.as_ptr().cast(),
                    bytes.len(),
                );
            },
            Param::Bn(key, bytes) => {
                let bn = bytes.filter(|b| !b.is_empty()).and_then(Bn::from_bytes);
                unsafe { OSSL_PARAM_BLD_push_BN(bld, key.as_ptr(), Bn::ptr(&bn)) };
                bignums.push(bn);
            }
        }
    }
    let ossl_params = unsafe { OSSL_PARAM_BLD_to_param(bld) };
    let ctx = PkeyCtx::by_name(keytype);
    let selection = match selection {
        Selection::Public => EVP_PKEY_PUBLIC_KEY,
        Selection::Keypair => EVP_PKEY_KEYPAIR,
    };
    let mut pkey = ptr::null_mut();
    let ok = ctx.as_ref().is_some_and(|ctx| unsafe {
        EVP_PKEY_fromdata_init(ctx.ptr()) > 0
            && EVP_PKEY_fromdata(ctx.ptr(), &mut pkey, selection, ossl_params) > 0
    });
    let pkey = Pkey::from_raw(pkey);
    unsafe {
        OSSL_PARAM_free(ossl_params);
        OSSL_PARAM_BLD_free(bld);
    }
    drop(ctx);
    drop(bignums);
    if ok { pkey } else { None }
}

pub fn hmac(digest_name: &CStr, key: Option<&[u8]>, data: &[u8]) -> Option<Vec<u8>> {
    let mac = unsafe { EVP_MAC_fetch(ptr::null_mut(), c"HMAC".as_ptr(), ptr::null()) };
    let ctx = if mac.is_null() {
        ptr::null_mut()
    } else {
        unsafe { EVP_MAC_CTX_new(mac) }
    };
    let params = [
        unsafe {
            OSSL_PARAM_construct_utf8_string(c"digest".as_ptr(), digest_name.as_ptr().cast_mut(), 0)
        },
        unsafe { OSSL_PARAM_construct_end() },
    ];
    let key_len = key.map_or(0, <[u8]>::len);
    let mut out = None;
    let mut n = 0usize;
    if !ctx.is_null()
        && unsafe { EVP_MAC_init(ctx, opt_ptr(key), key_len, params.as_ptr()) } != 0
        && unsafe { EVP_MAC_update(ctx, data.as_ptr(), data.len()) } != 0
        && unsafe { EVP_MAC_final(ctx, ptr::null_mut(), &mut n, 0) } != 0
    {
        let mut buf = vec![0u8; n];
        if unsafe { EVP_MAC_final(ctx, buf.as_mut_ptr(), &mut n, buf.len()) } != 0 {
            buf.truncate(n);
            out = Some(buf);
        }
    }
    unsafe {
        EVP_MAC_CTX_free(ctx);
        EVP_MAC_free(mac);
    }
    out
}

pub fn hkdf(
    digest_name: &CStr,
    key: Option<&[u8]>,
    salt: &[u8],
    info: &[u8],
    out: &mut [u8],
) -> bool {
    let kdf = unsafe { EVP_KDF_fetch(ptr::null_mut(), c"HKDF".as_ptr(), ptr::null()) };
    let ctx = if kdf.is_null() {
        ptr::null_mut()
    } else {
        unsafe { EVP_KDF_CTX_new(kdf) }
    };
    let params = unsafe {
        [
            OSSL_PARAM_construct_utf8_string(
                c"digest".as_ptr(),
                digest_name.as_ptr().cast_mut(),
                0,
            ),
            OSSL_PARAM_construct_octet_string(
                c"key".as_ptr(),
                opt_ptr(key).cast_mut().cast(),
                key.map_or(0, <[u8]>::len),
            ),
            OSSL_PARAM_construct_octet_string(
                c"salt".as_ptr(),
                salt.as_ptr().cast_mut().cast(),
                salt.len(),
            ),
            OSSL_PARAM_construct_octet_string(
                c"info".as_ptr(),
                info.as_ptr().cast_mut().cast(),
                info.len(),
            ),
            OSSL_PARAM_construct_end(),
        ]
    };
    let ok = !ctx.is_null()
        && unsafe { EVP_KDF_derive(ctx, out.as_mut_ptr(), out.len(), params.as_ptr()) } > 0;
    unsafe {
        EVP_KDF_CTX_free(ctx);
        EVP_KDF_free(kdf);
    }
    ok
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

fn sign_setup(pctx: *mut EvpPkeyCtx, padding: &Padding) -> bool {
    match *padding {
        Padding::Default => true,
        Padding::Pkcs1 => unsafe { EVP_PKEY_CTX_set_rsa_padding(pctx, RSA_PKCS1_PADDING) > 0 },
        Padding::Pss(salt) => {
            let padded = unsafe { EVP_PKEY_CTX_set_rsa_padding(pctx, RSA_PKCS1_PSS_PADDING) > 0 };
            padded
                && salt
                    .is_some_and(|salt| unsafe { EVP_PKEY_CTX_set_rsa_pss_saltlen(pctx, salt) > 0 })
        }
    }
}

fn md_ptr(md: Option<Md>) -> *const EvpMd {
    md.map_or(ptr::null(), |md| md.0)
}

pub fn digest_sign(
    pkey: PkeyRef<'_>,
    md: Option<Md>,
    padding: &Padding,
    data: &[u8],
) -> Option<Vec<u8>> {
    let ctx = MdCtx::new()?;
    let mut pctx = ptr::null_mut();
    let mut n = 0usize;
    let ready = unsafe {
        EVP_DigestSignInit(
            ctx.0.as_ptr(),
            &mut pctx,
            md_ptr(md),
            ptr::null_mut(),
            pkey.ptr(),
        ) > 0
    } && sign_setup(pctx, padding)
        && unsafe {
            EVP_DigestSign(
                ctx.0.as_ptr(),
                ptr::null_mut(),
                &mut n,
                data.as_ptr(),
                data.len(),
            ) > 0
        };
    if !ready {
        return None;
    }
    let mut out = vec![0u8; n];
    if unsafe {
        EVP_DigestSign(
            ctx.0.as_ptr(),
            out.as_mut_ptr(),
            &mut n,
            data.as_ptr(),
            data.len(),
        )
    } <= 0
    {
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
    let mut pctx = ptr::null_mut();
    let ready = unsafe {
        EVP_DigestVerifyInit(
            ctx.0.as_ptr(),
            &mut pctx,
            md_ptr(md),
            ptr::null_mut(),
            pkey.ptr(),
        ) > 0
    } && sign_setup(pctx, padding);
    if !ready {
        return None;
    }
    let verdict = unsafe {
        EVP_DigestVerify(
            ctx.0.as_ptr(),
            sig.as_ptr(),
            sig.len(),
            data.as_ptr(),
            data.len(),
        )
    };
    if verdict < 0 {
        clear_errors();
    }
    Some(verdict == 1)
}

pub fn ecdsa_der_to_raw(der: &[u8], order: usize) -> Option<Vec<u8>> {
    let mut p = der.as_ptr();
    let sig = unsafe { d2i_ECDSA_SIG(ptr::null_mut(), &mut p, der.len() as c_long) };
    if sig.is_null() {
        return None;
    }
    let (mut r, mut s) = (ptr::null(), ptr::null());
    unsafe { ECDSA_SIG_get0(sig, &mut r, &mut s) };
    let mut out = vec![0u8; order * 2];
    let ok = !r.is_null()
        && !s.is_null()
        && c_len(order).is_some_and(|len| unsafe {
            BN_bn2binpad(r, out.as_mut_ptr(), len) >= 0
                && BN_bn2binpad(s, out.as_mut_ptr().add(order), len) >= 0
        });
    unsafe { ECDSA_SIG_free(sig) };
    ok.then_some(out)
}

pub fn ecdsa_raw_to_der(raw: &[u8], order: usize) -> Option<Vec<u8>> {
    if raw.len() != order * 2 {
        return None;
    }
    let sig = unsafe { ECDSA_SIG_new() };
    let r = Bn::from_bytes(&raw[..order]);
    let s = Bn::from_bytes(&raw[order..]);
    let (Some(r), Some(s)) = (r, s) else {
        unsafe { ECDSA_SIG_free(sig) };
        return None;
    };
    if sig.is_null() {
        return None;
    }
    unsafe { ECDSA_SIG_set0(sig, r.into_raw(), s.into_raw()) };
    let mut der = ptr::null_mut();
    let n = unsafe { i2d_ECDSA_SIG(sig, &mut der) };
    unsafe { ECDSA_SIG_free(sig) };
    if n <= 0 {
        return None;
    }
    Some(unsafe { take_openssl_bytes(der, n as usize) })
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
    let mut ok = unsafe {
        (if encrypt {
            EVP_PKEY_encrypt_init(c)
        } else {
            EVP_PKEY_decrypt_init(c)
        }) > 0
            && EVP_PKEY_CTX_set_rsa_padding(c, RSA_PKCS1_OAEP_PADDING) > 0
            && EVP_PKEY_CTX_set_rsa_oaep_md(c, md.0) > 0
            && EVP_PKEY_CTX_set_rsa_mgf1_md(c, md.0) > 0
    };
    if let Some(label) = label.filter(|label| !label.is_empty()) {
        if ok {
            let copy = c_len(label.len()).map(|len| {
                (
                    unsafe { CRYPTO_memdup(label.as_ptr().cast(), label.len(), FILE.as_ptr(), 0) },
                    len,
                )
            });
            ok = match copy {
                Some((lbl, len)) if !lbl.is_null() => {
                    let set = unsafe { EVP_PKEY_CTX_set0_rsa_oaep_label(c, lbl, len) } > 0;
                    if !set {
                        unsafe { openssl_free(lbl.cast()) };
                    }
                    set
                }
                _ => false,
            };
        }
    }
    if !ok {
        return None;
    }
    let run = |out: *mut u8, n: &mut usize| unsafe {
        if encrypt {
            EVP_PKEY_encrypt(c, out, n, data.as_ptr(), data.len())
        } else {
            EVP_PKEY_decrypt(c, out, n, data.as_ptr(), data.len())
        }
    };
    let mut n = 0usize;
    if run(ptr::null_mut(), &mut n) <= 0 {
        return None;
    }
    let mut out = vec![0u8; n];
    if run(out.as_mut_ptr(), &mut n) <= 0 {
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
    let mut n = 0usize;
    let ready = unsafe {
        EVP_PKEY_derive_init(c) > 0
            && EVP_PKEY_derive_set_peer(c, peer.ptr()) > 0
            && EVP_PKEY_derive(c, ptr::null_mut(), &mut n) > 0
    };
    if !ready {
        return None;
    }
    let mut out = vec![0u8; n];
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
        let table: [unsafe extern "C" fn() -> *const EvpCipher; 3] = match mode {
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
        let cipher = unsafe { get() };
        (!cipher.is_null()).then_some(Cipher(cipher))
    }
}

pub struct CipherCtx(NonNull<EvpCipherCtx>);

impl Drop for CipherCtx {
    fn drop(&mut self) {
        unsafe { EVP_CIPHER_CTX_free(self.0.as_ptr()) };
    }
}

impl CipherCtx {
    pub fn new() -> Option<CipherCtx> {
        NonNull::new(unsafe { EVP_CIPHER_CTX_new() }).map(CipherCtx)
    }

    fn ptr(&self) -> *mut EvpCipherCtx {
        self.0.as_ptr()
    }

    pub fn allow_wrap(&self) {
        unsafe { EVP_CIPHER_CTX_set_flags(self.ptr(), EVP_CIPHER_CTX_FLAG_WRAP_ALLOW) };
    }

    pub fn init_cipher(&self, cipher: Cipher, encrypt: bool) -> bool {
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
        c_len(len).is_some_and(|len| unsafe {
            EVP_CIPHER_CTX_ctrl(self.ptr(), EVP_CTRL_AEAD_SET_IVLEN, len, ptr::null_mut()) != 0
        })
    }

    pub fn aad(&self, aad: &[u8]) -> bool {
        let mut outl = 0;
        c_len(aad.len()).is_some_and(|len| unsafe {
            EVP_CipherUpdate(self.ptr(), ptr::null_mut(), &mut outl, aad.as_ptr(), len) != 0
        })
    }

    pub fn update(&self, out: &mut [u8], input: &[u8]) -> Option<usize> {
        let len = c_len(input.len())?;
        if out.len() < input.len() + AES_BLOCK {
            return None;
        }
        let mut outl = 0;
        (unsafe { EVP_CipherUpdate(self.ptr(), out.as_mut_ptr(), &mut outl, input.as_ptr(), len) }
            != 0)
            .then_some(outl as usize)
    }

    pub fn finish(&self, out: &mut [u8]) -> Option<usize> {
        if out.len() < AES_BLOCK {
            return None;
        }
        let mut outl = 0;
        (unsafe { EVP_CipherFinal_ex(self.ptr(), out.as_mut_ptr(), &mut outl) } != 0)
            .then_some(outl as usize)
    }

    pub fn set_tag(&self, tag: &[u8]) -> bool {
        c_len(tag.len()).is_some_and(|len| unsafe {
            EVP_CIPHER_CTX_ctrl(
                self.ptr(),
                EVP_CTRL_AEAD_SET_TAG,
                len,
                tag.as_ptr().cast_mut().cast(),
            ) != 0
        })
    }

    pub fn get_tag(&self, tag: &mut [u8]) -> bool {
        c_len(tag.len()).is_some_and(|len| unsafe {
            EVP_CIPHER_CTX_ctrl(
                self.ptr(),
                EVP_CTRL_AEAD_GET_TAG,
                len,
                tag.as_mut_ptr().cast(),
            ) != 0
        })
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
    let Some(len) = c_len(input.len()) else {
        return false;
    };
    if out.len() < input.len() + AES_BLOCK {
        return false;
    }
    let (mut outl, mut finl) = (0, 0);
    unsafe {
        EVP_EncryptInit_ex(
            ctx.ptr(),
            cipher.0,
            ptr::null_mut(),
            key.as_ptr(),
            counter.as_ptr(),
        ) != 0
            && EVP_EncryptUpdate(ctx.ptr(), out.as_mut_ptr(), &mut outl, input.as_ptr(), len) != 0
            && EVP_EncryptFinal_ex(ctx.ptr(), out.as_mut_ptr().add(outl as usize), &mut finl) != 0
    }
}
