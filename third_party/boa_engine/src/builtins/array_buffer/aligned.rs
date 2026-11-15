//! 64-byte aligned byte storage for `ArrayBuffer` and `SharedArrayBuffer` data blocks.

use std::{
    collections::TryReserveError,
    fmt,
    ops::{Deref, DerefMut},
    slice,
};

use portable_atomic::AtomicU8;

const BLOCK: usize = 64;

#[derive(Clone, Copy)]
#[repr(C, align(64))]
struct Block([u8; BLOCK]);

#[repr(C, align(64))]
struct AtomicBlock([AtomicU8; BLOCK]);

const fn blocks_for(len: usize) -> usize {
    len.div_ceil(BLOCK)
}

/// A growable byte buffer whose data starts on a 64-byte boundary.
#[derive(Clone, Default)]
pub struct AlignedVec {
    blocks: Vec<Block>,
    len: usize,
}

impl AlignedVec {
    /// Creates an empty buffer.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            blocks: Vec::new(),
            len: 0,
        }
    }

    /// Reserves room for exactly `additional` more bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the capacity overflows or the allocation fails.
    pub fn try_reserve_exact(&mut self, additional: usize) -> Result<(), TryReserveError> {
        let wanted = blocks_for(self.len.saturating_add(additional));
        self.blocks
            .try_reserve_exact(wanted.saturating_sub(self.blocks.len()))
    }

    /// Resizes the buffer to `new_len` bytes, filling new bytes with `value`.
    pub fn resize(&mut self, new_len: usize, value: u8) {
        let old_len = self.len;
        self.blocks.resize(blocks_for(new_len), Block([0; BLOCK]));
        self.len = new_len;
        if new_len > old_len {
            self[old_len..].fill(value);
        }
    }

    /// Creates a buffer holding a copy of `bytes`.
    #[must_use]
    pub fn from_slice(bytes: &[u8]) -> Self {
        let mut vec = Self::new();
        vec.resize(bytes.len(), 0);
        vec.copy_from_slice(bytes);
        vec
    }

    /// Releases capacity beyond the current length.
    pub fn shrink_to_fit(&mut self) {
        self.blocks.shrink_to_fit();
    }
}

impl FromIterator<u8> for AlignedVec {
    fn from_iter<I: IntoIterator<Item = u8>>(iter: I) -> Self {
        Self::from_slice(&iter.into_iter().collect::<Vec<u8>>())
    }
}

impl Deref for AlignedVec {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        // SAFETY: `blocks` is contiguous `repr(C)` storage of `blocks.len() * BLOCK` initialized
        // bytes and `len` never exceeds that, so the first `len` bytes are a valid `[u8]`.
        unsafe { slice::from_raw_parts(self.blocks.as_ptr().cast::<u8>(), self.len) }
    }
}

impl DerefMut for AlignedVec {
    fn deref_mut(&mut self) -> &mut [u8] {
        // SAFETY: as in `deref`, and the exclusive borrow of `self` makes the slice unique.
        unsafe { slice::from_raw_parts_mut(self.blocks.as_mut_ptr().cast::<u8>(), self.len) }
    }
}

impl fmt::Debug for AlignedVec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

/// A fixed-size block of zeroed atomic bytes whose data starts on a 64-byte boundary.
#[derive(Default)]
pub(crate) struct SharedBlock {
    blocks: Box<[AtomicBlock]>,
    len: usize,
}

impl SharedBlock {
    /// Allocates `len` zeroed bytes.
    pub(crate) fn zeroed(len: usize) -> Result<Self, TryReserveError> {
        let count = blocks_for(len);
        let mut blocks = Vec::new();
        blocks.try_reserve_exact(count)?;
        blocks.extend((0..count).map(|_| AtomicBlock(std::array::from_fn(|_| AtomicU8::new(0)))));
        Ok(Self {
            blocks: blocks.into_boxed_slice(),
            len,
        })
    }
}

impl Deref for SharedBlock {
    type Target = [AtomicU8];

    fn deref(&self) -> &[AtomicU8] {
        // SAFETY: `blocks` is contiguous `repr(C)` storage of `blocks.len() * BLOCK` atomic
        // bytes and `len` never exceeds that.
        unsafe { slice::from_raw_parts(self.blocks.as_ptr().cast::<AtomicU8>(), self.len) }
    }
}

impl fmt::Debug for SharedBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SharedBlock")
            .field("len", &self.len)
            .finish()
    }
}
