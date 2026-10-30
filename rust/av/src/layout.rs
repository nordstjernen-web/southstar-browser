//! Southstar — compile-time lookup in a `name=value,…` layout string meson computes from C headers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub const fn lookup(layout: &str, key: &str) -> i64 {
    let bytes = layout.as_bytes();
    let key = key.as_bytes();
    let mut start = 0;
    while start < bytes.len() {
        let mut end = start;
        while end < bytes.len() && bytes[end] != b',' {
            end += 1;
        }
        if entry_matches(bytes, start, end, key) {
            return parse_int(bytes, start + key.len() + 1, end);
        }
        start = end + 1;
    }
    panic!("layout value missing; reconfigure the build with meson")
}

const fn entry_matches(bytes: &[u8], start: usize, end: usize, key: &[u8]) -> bool {
    if end - start <= key.len() || bytes[start + key.len()] != b'=' {
        return false;
    }
    let mut i = 0;
    while i < key.len() {
        if bytes[start + i] != key[i] {
            return false;
        }
        i += 1;
    }
    true
}

const fn parse_int(bytes: &[u8], mut at: usize, end: usize) -> i64 {
    let negative = at < end && bytes[at] == b'-';
    if negative {
        at += 1;
    }
    let mut value: i64 = 0;
    while at < end {
        value = value * 10 + (bytes[at] - b'0') as i64;
        at += 1;
    }
    if negative { -value } else { value }
}

pub struct Field<S, T> {
    offset: usize,
    marker: core::marker::PhantomData<(*const S, T)>,
}

impl<S, T: Copy> Field<S, T> {
    pub const fn at(offset: i64) -> Self {
        Field {
            offset: offset as usize,
            marker: core::marker::PhantomData,
        }
    }

    pub const fn offset(&self) -> usize {
        self.offset
    }

    pub fn ptr(&self, base: *mut S) -> *mut T {
        base.cast::<u8>().wrapping_add(self.offset).cast::<T>()
    }

    /// # Safety
    ///
    /// `base` must point to a live `S` laid out as the installed headers describe, so that a `T` lies at this field's offset.
    pub unsafe fn get(&self, base: *const S) -> T {
        let field = self.ptr(base.cast_mut()).cast_const();
        // SAFETY: the caller guarantees a live S at base, so field addresses its T; it may be unaligned, hence read_unaligned.
        unsafe { field.read_unaligned() }
    }

    /// # Safety
    ///
    /// `base` must point to a live `S` laid out as the installed headers describe and writable for a `T` at this field's offset.
    pub unsafe fn set(&self, base: *mut S, value: T) {
        let field = self.ptr(base);
        // SAFETY: the caller guarantees a live, writable S at base, so field addresses its T; it may be unaligned, hence write_unaligned.
        unsafe { field.write_unaligned(value) }
    }
}
