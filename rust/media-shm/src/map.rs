//! Southstar — opening a media helper's named shared memory and reading the video ring and audio clock in it while the helper writes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::marker::PhantomData;
use core::mem::{offset_of, size_of};
use core::ptr::NonNull;
use core::sync::atomic::{AtomicU32, Ordering};
use std::ffi::CString;

use crate::{
    AUDIO_CLOCK_SLOTS, AudioClockHdr, AudioClockSlot, VIDEO_RING_SLOTS, VideoRingHdr, VideoRingSlot,
};

pub struct SharedMemory {
    base: NonNull<u8>,
    len: usize,
    #[cfg(windows)]
    handle: os::Handle,
}

#[derive(Clone, Copy)]
struct View<'a> {
    base: NonNull<u8>,
    len: usize,
    _map: PhantomData<&'a SharedMemory>,
}

#[derive(Clone, Copy)]
pub struct VideoRing<'a>(View<'a>);

#[derive(Clone, Copy)]
pub struct AudioClock<'a>(View<'a>);

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Geometry {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub frame_bytes: u32,
}

#[derive(Clone, Copy)]
pub struct RingSlot {
    pub sequence: u32,
    pub generation: u32,
    pub pts: f64,
}

#[derive(Clone, Copy)]
pub struct ClockSample {
    pub flags: u32,
    pub monotonic_us: i64,
    pub position: f64,
}

#[derive(Clone, Copy)]
pub struct TokenSample {
    pub flags: u32,
    pub monotonic_us: i64,
    pub position: f64,
    pub token: [u8; 64],
}

pub struct Frame<'a> {
    pixels: NonNull<u8>,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    _map: PhantomData<&'a SharedMemory>,
}

impl SharedMemory {
    pub fn open(name: &[u8], writable: bool) -> Option<SharedMemory> {
        let name = CString::new(name).ok()?;
        os::open(&name, writable)
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn view(&self, header: usize) -> Option<View<'_>> {
        (self.len >= header).then_some(View {
            base: self.base,
            len: self.len,
            _map: PhantomData,
        })
    }

    pub fn video_ring(&self) -> Option<VideoRing<'_>> {
        self.view(size_of::<VideoRingHdr>()).map(VideoRing)
    }

    pub fn audio_clock(&self) -> Option<AudioClock<'_>> {
        self.view(size_of::<AudioClockHdr>()).map(AudioClock)
    }
}

impl Drop for SharedMemory {
    fn drop(&mut self) {
        os::close(self);
    }
}

impl<'a> View<'a> {
    fn read<T: Copy>(&self, offset: usize) -> T {
        assert!(offset + size_of::<T>() <= self.len);
        let field = self.base.as_ptr().wrapping_add(offset).cast::<T>();
        // SAFETY: offset names a field of the header, which lies inside the mapping (checked when the view was made) at its natural alignment; the helper may write it concurrently, so it is copied out once, volatile.
        unsafe { field.read_volatile() }
    }

    fn atomic(&self, offset: usize) -> &'a AtomicU32 {
        assert!(offset + size_of::<AtomicU32>() <= self.len);
        let field = self.base.as_ptr().wrapping_add(offset).cast::<AtomicU32>();
        // SAFETY: an aligned u32 inside the mapping, which outlives 'a; other processes touch it only atomically.
        unsafe { &*field }
    }
}

fn ring_slot(index: u32, field: usize) -> usize {
    assert!(index < VIDEO_RING_SLOTS);
    offset_of!(VideoRingHdr, slots) + index as usize * size_of::<VideoRingSlot>() + field
}

fn clock_slot(index: u32, field: usize) -> usize {
    assert!(index < AUDIO_CLOCK_SLOTS);
    offset_of!(AudioClockHdr, slots) + index as usize * size_of::<AudioClockSlot>() + field
}

impl<'a> VideoRing<'a> {
    pub fn magic(&self) -> u32 {
        self.0.read(offset_of!(VideoRingHdr, magic))
    }

    pub fn version(&self) -> u32 {
        self.0.read(offset_of!(VideoRingHdr, version))
    }

    pub fn nslots(&self) -> u32 {
        self.0.read(offset_of!(VideoRingHdr, nslots))
    }

    pub fn geometry(&self) -> Geometry {
        Geometry {
            width: self.0.read(offset_of!(VideoRingHdr, width)),
            height: self.0.read(offset_of!(VideoRingHdr, height)),
            stride: self.0.read(offset_of!(VideoRingHdr, stride)),
            frame_bytes: self.0.read(offset_of!(VideoRingHdr, frame_bytes)),
        }
    }

    pub fn mapped_len(&self) -> usize {
        self.0.len
    }

    pub fn published(&self) -> &'a AtomicU32 {
        self.0.atomic(offset_of!(VideoRingHdr, published))
    }

    pub fn released(&self) -> &'a AtomicU32 {
        self.0.atomic(offset_of!(VideoRingHdr, released))
    }

    pub fn generation(&self) -> &'a AtomicU32 {
        self.0.atomic(offset_of!(VideoRingHdr, generation))
    }

    pub fn clock(&self) -> Option<ClockSample> {
        let sequence = self.0.atomic(offset_of!(VideoRingHdr, clock_sequence));
        let seq1 = sequence.load(Ordering::Acquire);
        if seq1 == 0 || seq1 & 1 != 0 {
            return None;
        }
        let sample = ClockSample {
            flags: self.0.read(offset_of!(VideoRingHdr, clock_flags)),
            monotonic_us: self.0.read(offset_of!(VideoRingHdr, clock_monotonic_us)),
            position: self.0.read(offset_of!(VideoRingHdr, clock_position)),
        };
        (sequence.load(Ordering::Acquire) == seq1).then_some(sample)
    }

    pub fn slot(&self, index: u32) -> RingSlot {
        RingSlot {
            sequence: self
                .0
                .atomic(ring_slot(index, offset_of!(VideoRingSlot, sequence)))
                .load(Ordering::Acquire),
            generation: self
                .0
                .read(ring_slot(index, offset_of!(VideoRingSlot, generation))),
            pts: self
                .0
                .read(ring_slot(index, offset_of!(VideoRingSlot, pts))),
        }
    }

    pub fn frame(&self, slot: u32, geometry: Geometry) -> Option<Frame<'a>> {
        let Geometry {
            width,
            height,
            stride,
            frame_bytes,
        } = geometry;
        let fits = slot < VIDEO_RING_SLOTS
            && width > 0
            && height > 0
            && u64::from(stride) >= u64::from(width) * 4
            && u64::from(stride) * u64::from(height) <= u64::from(frame_bytes)
            && size_of::<VideoRingHdr>() + (slot as usize + 1) * frame_bytes as usize <= self.0.len;
        let offset = size_of::<VideoRingHdr>() + slot as usize * frame_bytes as usize;
        let pixels = NonNull::new(self.0.base.as_ptr().wrapping_add(offset))?;
        fits.then_some(Frame {
            pixels,
            width,
            height,
            stride,
            _map: PhantomData,
        })
    }
}

impl Frame<'_> {
    pub fn as_ptr(&self) -> *mut u8 {
        self.pixels.as_ptr()
    }

    pub fn copy_rows(&self, dst: &mut [u8], dst_stride: usize) {
        let row = self.width as usize * 4;
        for y in 0..self.height as usize {
            let Some(out) = dst.get_mut(y * dst_stride..y * dst_stride + row) else {
                return;
            };
            let src = self.pixels.as_ptr().wrapping_add(y * self.stride as usize);
            // SAFETY: frame() checked that height rows of stride bytes, each holding width * 4 pixel bytes, lie inside the mapping; out is a distinct slice of exactly that length.
            unsafe { core::ptr::copy_nonoverlapping(src, out.as_mut_ptr(), out.len()) };
        }
    }
}

impl AudioClock<'_> {
    pub fn magic(&self) -> u32 {
        self.0.read(offset_of!(AudioClockHdr, magic))
    }

    pub fn version(&self) -> u32 {
        self.0.read(offset_of!(AudioClockHdr, version))
    }

    pub fn nslots(&self) -> u32 {
        self.0.read(offset_of!(AudioClockHdr, nslots))
    }

    pub fn sample(&self, index: u32) -> Option<TokenSample> {
        let sequence = self
            .0
            .atomic(clock_slot(index, offset_of!(AudioClockSlot, sequence)));
        let seq1 = sequence.load(Ordering::Acquire);
        if seq1 == 0 || seq1 & 1 != 0 {
            return None;
        }
        let sample = TokenSample {
            flags: self
                .0
                .read(clock_slot(index, offset_of!(AudioClockSlot, flags))),
            monotonic_us: self
                .0
                .read(clock_slot(index, offset_of!(AudioClockSlot, monotonic_us))),
            position: self
                .0
                .read(clock_slot(index, offset_of!(AudioClockSlot, position))),
            token: self
                .0
                .read(clock_slot(index, offset_of!(AudioClockSlot, token))),
        };
        (sequence.load(Ordering::Acquire) == seq1).then_some(sample)
    }
}

#[cfg(unix)]
mod os {
    use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
    use core::ptr::{self, NonNull};
    use std::fs::File;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

    use super::SharedMemory;

    const O_RDONLY: c_int = 0;
    const O_RDWR: c_int = 2;
    const PROT_READ: c_int = 1;
    const PROT_WRITE: c_int = 2;
    const MAP_SHARED: c_int = 1;

    unsafe extern "C" {
        fn shm_open(name: *const c_char, oflag: c_int, ...) -> c_int;
        fn mmap(
            addr: *mut c_void,
            len: usize,
            prot: c_int,
            flags: c_int,
            fd: c_int,
            offset: i64,
        ) -> *mut c_void;
        fn munmap(addr: *mut c_void, len: usize) -> c_int;
    }

    pub(super) fn open(name: &CStr, writable: bool) -> Option<SharedMemory> {
        let flags = if writable { O_RDWR } else { O_RDONLY };
        // SAFETY: name is NUL-terminated; shm_open opens an existing object and returns a new descriptor or -1.
        let fd = unsafe { shm_open(name.as_ptr(), flags, 0 as c_uint) };
        if fd < 0 {
            return None;
        }
        // SAFETY: fd is the descriptor shm_open just returned, owned by nothing else.
        let file = File::from(unsafe { OwnedFd::from_raw_fd(fd) });
        let len = usize::try_from(file.metadata().ok()?.len()).ok()?;
        let prot = if writable {
            PROT_READ | PROT_WRITE
        } else {
            PROT_READ
        };
        // SAFETY: a new shared mapping of the whole object at an address the kernel picks; it stays valid after the descriptor closes.
        let base = unsafe { mmap(ptr::null_mut(), len, prot, MAP_SHARED, file.as_raw_fd(), 0) };
        if base as isize == -1 {
            return None;
        }
        Some(SharedMemory {
            base: NonNull::new(base.cast())?,
            len,
        })
    }

    pub(super) fn close(map: &mut SharedMemory) {
        // SAFETY: base and len describe the mapping open() made, unmapped once as the SharedMemory drops.
        unsafe { munmap(map.base.as_ptr().cast(), map.len) };
    }
}

#[cfg(windows)]
mod os {
    use core::ffi::{CStr, c_char, c_int, c_void};
    use core::ptr::{self, NonNull};

    use super::SharedMemory;

    pub(super) type Handle = *mut c_void;

    const FILE_MAP_READ: u32 = 4;
    const FILE_MAP_ALL_ACCESS: u32 = 0xF001F;

    #[repr(C)]
    struct MemoryBasicInformation {
        base_address: *mut c_void,
        allocation_base: *mut c_void,
        allocation_protect: u32,
        partition_id: u16,
        region_size: usize,
        state: u32,
        protect: u32,
        kind: u32,
    }

    unsafe extern "system" {
        fn OpenFileMappingA(access: u32, inherit: c_int, name: *const c_char) -> Handle;
        fn MapViewOfFile(
            mapping: Handle,
            access: u32,
            offset_high: u32,
            offset_low: u32,
            bytes: usize,
        ) -> *mut c_void;
        fn VirtualQuery(
            address: *const c_void,
            info: *mut MemoryBasicInformation,
            length: usize,
        ) -> usize;
        fn UnmapViewOfFile(address: *const c_void) -> c_int;
        fn CloseHandle(handle: Handle) -> c_int;
    }

    fn region_size(base: *mut c_void) -> usize {
        let mut info = MemoryBasicInformation {
            base_address: ptr::null_mut(),
            allocation_base: ptr::null_mut(),
            allocation_protect: 0,
            partition_id: 0,
            region_size: 0,
            state: 0,
            protect: 0,
            kind: 0,
        };
        // SAFETY: info is a writable MEMORY_BASIC_INFORMATION of the size passed; VirtualQuery only fills it.
        let filled = unsafe { VirtualQuery(base, &mut info, size_of::<MemoryBasicInformation>()) };
        if filled == 0 { 0 } else { info.region_size }
    }

    fn close_handle(handle: Handle) {
        // SAFETY: handle is the mapping handle OpenFileMappingA returned, closed exactly once.
        unsafe { CloseHandle(handle) };
    }

    fn unmap(base: *mut c_void) {
        // SAFETY: base is a view MapViewOfFile returned, unmapped exactly once.
        unsafe { UnmapViewOfFile(base) };
    }

    pub(super) fn open(name: &CStr, writable: bool) -> Option<SharedMemory> {
        let access = if writable {
            FILE_MAP_ALL_ACCESS
        } else {
            FILE_MAP_READ
        };
        // SAFETY: name is NUL-terminated; the call returns a new handle or null.
        let handle = unsafe { OpenFileMappingA(access, 0, name.as_ptr()) };
        if handle.is_null() {
            return None;
        }
        // SAFETY: handle is a live file mapping; mapping all of it returns a new view or null.
        let base = unsafe { MapViewOfFile(handle, access, 0, 0, 0) };
        let Some(view) = NonNull::new(base.cast::<u8>()) else {
            close_handle(handle);
            return None;
        };
        Some(SharedMemory {
            base: view,
            len: region_size(base),
            handle,
        })
    }

    pub(super) fn close(map: &mut SharedMemory) {
        unmap(map.base.as_ptr().cast());
        close_handle(map.handle);
    }
}
