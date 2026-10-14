//! Southstar — the operating system's trusted root certificates: the Windows ROOT store read through crypt32, nothing elsewhere.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#[cfg(windows)]
mod sys {
    use core::ffi::c_void;

    #[repr(C)]
    struct CertContext {
        encoding: u32,
        encoded: *const u8,
        encoded_len: u32,
        info: *mut c_void,
        store: *mut c_void,
    }

    #[link(name = "crypt32")]
    unsafe extern "system" {
        fn CertOpenSystemStoreW(provider: usize, name: *const u16) -> *mut c_void;
        fn CertEnumCertificatesInStore(
            store: *mut c_void,
            prev: *const CertContext,
        ) -> *const CertContext;
        fn CertCloseStore(store: *mut c_void, flags: u32) -> i32;
    }

    pub fn system() -> Vec<Vec<u8>> {
        let name: Vec<u16> = "ROOT".encode_utf16().chain(Some(0)).collect();
        // SAFETY: `name` is a NUL-terminated UTF-16 string that outlives the call.
        let store = unsafe { CertOpenSystemStoreW(0, name.as_ptr()) };
        if store.is_null() {
            return Vec::new();
        }
        let mut certificates = Vec::new();
        let mut cert: *const CertContext = core::ptr::null();
        loop {
            // SAFETY: `store` is open and `cert` is null or the context the previous call returned, which this call frees.
            cert = unsafe { CertEnumCertificatesInStore(store, cert) };
            // SAFETY: a non-null result is a valid context that lives until the next enumeration call.
            let Some(context) = (unsafe { cert.as_ref() }) else {
                break;
            };
            if context.encoded.is_null() {
                continue;
            }
            // SAFETY: crypt32 guarantees `encoded` points at `encoded_len` bytes of the context's DER encoding.
            let der = unsafe {
                core::slice::from_raw_parts(context.encoded, context.encoded_len as usize)
            };
            certificates.push(der.to_vec());
        }
        // SAFETY: `store` was opened above and the enumeration has released its last context.
        unsafe { CertCloseStore(store, 0) };
        certificates
    }
}

#[cfg(windows)]
pub use sys::system;

#[cfg(not(windows))]
pub fn system() -> Vec<Vec<u8>> {
    Vec::new()
}
