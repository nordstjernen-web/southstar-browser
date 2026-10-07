//! Southstar — the C ABI of spell checking, as declared in src/spellcheck.h, and the Enchant calls behind it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_char;
use southstar_glib::{self as glib, GBoolean, TRUE};

#[cfg(feature = "enchant")]
mod enchant {
    use core::ffi::{CStr, c_char, c_int};
    use core::ptr;
    use std::ffi::CString;
    use std::sync::{Mutex, MutexGuard};

    use southstar_glib as glib;

    #[repr(C)]
    struct EnchantBroker {
        _private: [u8; 0],
    }

    #[repr(C)]
    struct EnchantDict {
        _private: [u8; 0],
    }

    unsafe extern "C" {
        fn enchant_broker_init() -> *mut EnchantBroker;
        fn enchant_broker_dict_exists(broker: *mut EnchantBroker, tag: *const c_char) -> c_int;
        fn enchant_broker_request_dict(
            broker: *mut EnchantBroker,
            tag: *const c_char,
        ) -> *mut EnchantDict;
        fn enchant_dict_check(dict: *mut EnchantDict, word: *const c_char, len: isize) -> c_int;
        fn enchant_dict_suggest(
            dict: *mut EnchantDict,
            word: *const c_char,
            len: isize,
            out_n_suggs: *mut usize,
        ) -> *mut *mut c_char;
        fn enchant_dict_free_string_list(dict: *mut EnchantDict, string_list: *mut *mut c_char);
    }

    struct Dict {
        handle: *mut EnchantDict,
        tag: Vec<u8>,
    }

    struct Spell {
        broker: *mut EnchantBroker,
        dicts: Vec<Dict>,
    }

    unsafe impl Send for Spell {}

    static SPELL: Mutex<Spell> = Mutex::new(Spell {
        broker: ptr::null_mut(),
        dicts: Vec::new(),
    });

    fn lock() -> MutexGuard<'static, Spell> {
        SPELL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    impl Spell {
        fn try_load(&mut self, tag: &[u8]) {
            if tag.is_empty() || self.dicts.iter().any(|dict| dict.tag == tag) {
                return;
            }
            let Ok(c_tag) = CString::new(tag) else {
                return;
            };
            unsafe {
                if enchant_broker_dict_exists(self.broker, c_tag.as_ptr()) == 0 {
                    return;
                }
                let handle = enchant_broker_request_dict(self.broker, c_tag.as_ptr());
                if !handle.is_null() {
                    self.dicts.push(Dict {
                        handle,
                        tag: tag.to_vec(),
                    });
                }
            }
        }

        fn warm_up(&self) {
            for dict in &self.dicts {
                unsafe {
                    enchant_dict_check(dict.handle, c"test".as_ptr(), 4);
                    let mut n = 0usize;
                    let suggestions = enchant_dict_suggest(dict.handle, c"teh".as_ptr(), 3, &mut n);
                    if !suggestions.is_null() {
                        enchant_dict_free_string_list(dict.handle, suggestions);
                    }
                }
            }
        }
    }

    pub(super) fn init() {
        let mut spell = lock();
        if !spell.broker.is_null() {
            return;
        }
        spell.broker = unsafe { enchant_broker_init() };
        if spell.broker.is_null() {
            return;
        }
        let mut names = unsafe { glib::g_get_language_names() };
        while !names.is_null() && !unsafe { *names }.is_null() {
            let name = unsafe { CStr::from_ptr(*names) }.to_bytes();
            names = unsafe { names.add(1) };
            if let Some(tag) = crate::normalize_lang(name) {
                spell.try_load(&tag);
            }
        }
        for tag in [b"en_US".as_slice(), b"en_GB", b"en"] {
            spell.try_load(tag);
        }
        spell.warm_up();
    }

    pub(super) fn available() -> bool {
        !lock().dicts.is_empty()
    }

    pub(super) fn word_ok(word: &[u8], lang: Option<&[u8]>) -> bool {
        let spell = lock();
        let tags: Vec<&[u8]> = spell.dicts.iter().map(|dict| dict.tag.as_slice()).collect();
        let Some(index) = crate::pick(&tags, lang) else {
            return true;
        };
        if word.is_empty() {
            return true;
        }
        let rc = unsafe {
            enchant_dict_check(
                spell.dicts[index].handle,
                word.as_ptr().cast(),
                word.len() as isize,
            )
        };
        rc <= 0
    }
}

#[cfg(not(feature = "enchant"))]
mod enchant {
    pub(super) fn init() {}

    pub(super) fn available() -> bool {
        false
    }

    pub(super) fn word_ok(_word: &[u8], _lang: Option<&[u8]>) -> bool {
        true
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_spell_init() {
    enchant::init();
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_spell_available() -> GBoolean {
    glib::boolean(enchant::available())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_spell_word_ok(
    word: *const c_char,
    len: isize,
    lang: *const c_char,
) -> GBoolean {
    if word.is_null() {
        return TRUE;
    }
    let word = if len < 0 {
        unsafe { glib::bytes(word) }.unwrap_or_default()
    } else {
        unsafe { glib::slice(word.cast(), len as usize) }
    };
    let lang = unsafe { glib::bytes(lang) };
    glib::boolean(enchant::word_ok(word, lang))
}
