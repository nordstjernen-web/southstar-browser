//! Southstar — painting a page into embedder buffers: cairo contexts over pixel memory, the paint state, layer plans and their viewport captures.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr::{self, NonNull};

use southstar_glib::{self as glib, GArray, GBoolean, GHashTable};
use southstar_layout::{BoxRef, NsBox, Style};

use super::NsBrowser;

const FORMAT_ARGB32: c_int = 0;
const STATUS_SUCCESS: c_int = 0;
const ANTIALIAS_FAST: c_int = 4;
pub const VP_STICKY: c_int = 2;

#[repr(C)]
pub struct LayerPlan {
    dynamic: GBoolean,
    _kinds: *mut GHashTable,
    vp: *mut GArray,
}

#[repr(C)]
pub struct VpCapture {
    b: *const NsBox,
    kind: c_int,
    _rel: [f64; 6],
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(core::mem::size_of::<VpCapture>() == 64);

#[repr(C)]
#[derive(Default)]
pub struct StickyY {
    has_top: GBoolean,
    has_bottom: GBoolean,
    top_start: f64,
    top_cap: f64,
    bottom_start: f64,
    bottom_cap: f64,
}

#[repr(C)]
#[derive(Default)]
pub struct VpLayerInfo {
    pub kind: c_int,
    pub top: f64,
    pub bottom: f64,
    pub x_offset: f64,
    sticky: StickyY,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(core::mem::size_of::<VpLayerInfo>() == 72);

type UpperFn = unsafe extern "C" fn(index: c_int, data: *mut c_void) -> *mut c_void;

unsafe extern "C" {
    fn cairo_image_surface_create(format: c_int, width: c_int, height: c_int) -> *mut c_void;
    fn cairo_image_surface_create_for_data(
        data: *mut u8,
        format: c_int,
        width: c_int,
        height: c_int,
        stride: c_int,
    ) -> *mut c_void;
    fn cairo_image_surface_get_data(surface: *mut c_void) -> *mut u8;
    fn cairo_image_surface_get_stride(surface: *mut c_void) -> c_int;
    fn cairo_surface_status(surface: *mut c_void) -> c_int;
    fn cairo_surface_destroy(surface: *mut c_void);
    fn cairo_surface_flush(surface: *mut c_void);
    fn cairo_surface_reference(surface: *mut c_void) -> *mut c_void;
    fn cairo_surface_write_to_png(surface: *mut c_void, filename: *const c_char) -> c_int;
    fn cairo_create(target: *mut c_void) -> *mut c_void;
    fn cairo_destroy(cr: *mut c_void);
    fn cairo_get_target(cr: *mut c_void) -> *mut c_void;
    fn cairo_set_tolerance(cr: *mut c_void, tolerance: f64);
    fn cairo_set_antialias(cr: *mut c_void, antialias: c_int);
    fn cairo_rectangle(cr: *mut c_void, x: f64, y: f64, w: f64, h: f64);
    fn cairo_clip(cr: *mut c_void);
    fn cairo_scale(cr: *mut c_void, sx: f64, sy: f64);
    fn cairo_translate(cr: *mut c_void, tx: f64, ty: f64);
    fn ns_paint(cr: *mut c_void, root: *const NsBox, highlight: *const c_char);
    fn ns_paint_with_selection(
        cr: *mut c_void,
        root: *const NsBox,
        highlight: *const c_char,
        sel: *const c_void,
    );
    fn ns_paint_set_search(case_sensitive: GBoolean, active: *const NsBox);
    fn ns_paint_set_caret_visible(visible: GBoolean);
    fn ns_paint_plan_layers(cr: *mut c_void, root: *const NsBox, plan: *mut LayerPlan);
    fn ns_paint_doc_layers(
        cr: *mut c_void,
        upper: UpperFn,
        data: *mut c_void,
        root: *const NsBox,
        highlight: *const c_char,
        sel: *const c_void,
        plan: *const LayerPlan,
    ) -> GBoolean;
    fn ns_paint_vp_layer(
        cr: *mut c_void,
        root: *const NsBox,
        layer: *const VpCapture,
        vp_x: f64,
        vp_y: f64,
        highlight: *const c_char,
        sel: *const c_void,
    );
    fn ns_paint_canvas_color(root: *const NsBox, rgba: *mut f64) -> GBoolean;
    fn ns_video_cache_flush_composites(cache: *mut c_void, now_us: i64);
    fn ns_video_cache_set_page_coords(cache: *mut c_void, on: GBoolean);
    fn ns_box_set_hit_viewport(scroll_x: f64, scroll_y: f64);
    fn ns_box_is_fixed(b: *const NsBox) -> GBoolean;
    fn ns_box_hit_offset(b: *const NsBox, dx: *mut f64, dy: *mut f64);
    fn ns_box_subtree_extent_y(b: *const NsBox, top: *mut f64, bottom: *mut f64) -> GBoolean;
    fn ns_box_sticky_y_model(b: *const NsBox, viewport_h: f64, out: *mut StickyY) -> GBoolean;
    fn ns_box_sticky_offset(
        b: *const NsBox,
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        dx: *mut f64,
        dy: *mut f64,
    );
    fn ns_box_scroll_snap_viewport(
        root: *mut NsBox,
        style: *const Style,
        viewport_w: f64,
        viewport_h: f64,
        max_x: f64,
        max_y: f64,
        prev_x: f64,
        prev_y: f64,
        x: *mut f64,
        y: *mut f64,
    ) -> GBoolean;
    fn ns_css_prop_id(name: *const c_char) -> c_int;
    fn ns_style_keyword(style: *const Style, prop: c_int) -> *const c_char;
}

pub struct Surface(NonNull<c_void>);

impl Surface {
    pub fn image(width: c_int, height: c_int) -> Option<Surface> {
        let surf =
            NonNull::new(unsafe { cairo_image_surface_create(FORMAT_ARGB32, width, height) })?;
        if unsafe { cairo_surface_status(surf.as_ptr()) } != STATUS_SUCCESS {
            unsafe { cairo_surface_destroy(surf.as_ptr()) };
            return None;
        }
        Some(Surface(surf))
    }

    pub fn flush(&self) {
        unsafe { cairo_surface_flush(self.0.as_ptr()) };
    }

    pub fn write_png(&self, path: &CStr) {
        unsafe { cairo_surface_write_to_png(self.0.as_ptr(), path.as_ptr()) };
    }

    pub fn copy_to_rgba(&self, out: &PixelBuf) {
        let src = unsafe { cairo_image_surface_get_data(self.0.as_ptr()) };
        let src_stride = unsafe { cairo_image_surface_get_stride(self.0.as_ptr()) } as usize;
        for y in 0..out.height as usize {
            for x in 0..out.width as usize {
                let px = unsafe {
                    src.add(y * src_stride + x * 4)
                        .cast::<u32>()
                        .read_unaligned()
                };
                let rgba = [
                    (px >> 16) as u8,
                    (px >> 8) as u8,
                    px as u8,
                    (px >> 24) as u8,
                ];
                unsafe {
                    ptr::copy_nonoverlapping(
                        rgba.as_ptr(),
                        out.data.add(y * out.stride as usize + x * 4),
                        4,
                    )
                };
            }
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe { cairo_surface_destroy(self.0.as_ptr()) };
    }
}

fn tolerance(scale: f64) -> f64 {
    if scale > 0.0 { 0.5 / scale } else { 0.5 }
}

pub struct Cairo(NonNull<c_void>);

impl Cairo {
    pub fn on_surface(surface: &Surface, width: c_int, height: c_int, scale: f64) -> Cairo {
        let cr =
            Cairo(NonNull::new(unsafe { cairo_create(surface.0.as_ptr()) }).expect("cairo_create"));
        cr.setup(width, height, scale);
        cr
    }

    fn for_data(
        data: *mut u8,
        width: c_int,
        height: c_int,
        stride: c_int,
        scale: f64,
    ) -> Option<Cairo> {
        let surf = unsafe {
            cairo_image_surface_create_for_data(data, FORMAT_ARGB32, width, height, stride)
        };
        if unsafe { cairo_surface_status(surf) } != STATUS_SUCCESS {
            unsafe { cairo_surface_destroy(surf) };
            return None;
        }
        let cr = unsafe { cairo_create(surf) };
        unsafe { cairo_surface_destroy(surf) };
        let cr = Cairo(NonNull::new(cr)?);
        cr.setup(width, height, scale);
        Some(cr)
    }

    pub fn on_buffer(buf: &PixelBuf, scale: f64) -> Option<Cairo> {
        Cairo::for_data(buf.data, buf.width, buf.height, buf.stride, scale)
    }

    fn setup(&self, width: c_int, height: c_int, scale: f64) {
        unsafe {
            cairo_set_tolerance(self.raw(), tolerance(scale));
            cairo_set_antialias(self.raw(), ANTIALIAS_FAST);
            cairo_rectangle(self.raw(), 0.0, 0.0, f64::from(width), f64::from(height));
            cairo_clip(self.raw());
        }
    }

    fn raw(&self) -> *mut c_void {
        self.0.as_ptr()
    }

    pub fn scale(&self, s: f64) {
        unsafe { cairo_scale(self.raw(), s, s) };
    }

    pub fn translate(&self, x: f64, y: f64) {
        unsafe { cairo_translate(self.raw(), x, y) };
    }

    pub fn destroy(self) {
        unsafe { cairo_destroy(self.raw()) };
    }

    pub fn finish(self) -> Surface {
        let surf = unsafe { cairo_surface_reference(cairo_get_target(self.raw())) };
        unsafe {
            cairo_destroy(self.raw());
            cairo_surface_flush(surf);
        }
        Surface(NonNull::new(surf).expect("cairo target"))
    }

    pub fn paint(&self, b: &NsBrowser, layout: BoxRef<'_>) {
        let sel = b.selection_if_range();
        unsafe {
            if sel.is_null() {
                ns_paint(self.raw(), layout.as_ptr(), b.search_query.ptr());
            } else {
                ns_paint_with_selection(self.raw(), layout.as_ptr(), b.search_query.ptr(), sel);
            }
        }
    }

    pub fn plan_layers(&self, layout: BoxRef<'_>, plan: &Plan<'_>) {
        unsafe { ns_paint_plan_layers(self.raw(), layout.as_ptr(), plan.0) };
    }

    pub fn vp_layer(
        &self,
        b: &NsBrowser,
        layout: BoxRef<'_>,
        cap: &VpCapture,
        vp_x: f64,
        vp_y: f64,
    ) {
        unsafe {
            ns_paint_vp_layer(
                self.raw(),
                layout.as_ptr(),
                cap,
                vp_x,
                vp_y,
                b.search_query.ptr(),
                b.selection_if_range(),
            )
        };
    }
}

#[derive(Clone, Copy)]
pub struct PixelBuf {
    data: *mut u8,
    pub width: c_int,
    pub height: c_int,
    pub stride: c_int,
}

impl PixelBuf {
    pub unsafe fn new(data: *mut u8, width: c_int, height: c_int, stride: c_int) -> PixelBuf {
        PixelBuf {
            data,
            width,
            height,
            stride,
        }
    }

    pub fn clear(&self) {
        unsafe { ptr::write_bytes(self.data, 0, self.stride as usize * self.height as usize) };
    }
}

pub struct Plan<'a>(*mut LayerPlan, core::marker::PhantomData<&'a LayerPlan>);

impl<'a> Plan<'a> {
    pub unsafe fn from_ptr(p: *const LayerPlan) -> Option<Plan<'a>> {
        (!p.is_null()).then(|| Plan(p.cast_mut(), core::marker::PhantomData))
    }

    pub fn dynamic(&self) -> bool {
        unsafe { (*self.0).dynamic != 0 }
    }

    pub fn len(&self) -> usize {
        unsafe { (*(*self.0).vp).len as usize }
    }

    pub fn capture(&self, index: c_int) -> Option<&'a VpCapture> {
        let index = usize::try_from(index).ok().filter(|&i| i < self.len())?;
        Some(unsafe { &*(*(*self.0).vp).data.cast::<VpCapture>().add(index) })
    }

    pub fn raw(&self) -> *const LayerPlan {
        self.0
    }
}

impl VpCapture {
    pub fn kind(&self) -> c_int {
        self.kind
    }

    pub fn box_ref(&self) -> Option<BoxRef<'_>> {
        unsafe { BoxRef::from_ptr(self.b) }
    }
}

struct DocTile {
    bufs: *const *mut u8,
    upper: Vec<Option<Cairo>>,
    used: *mut GBoolean,
    width: c_int,
    height: c_int,
    stride: c_int,
    scroll_x: c_int,
    tile_y: c_int,
    scale: f64,
}

impl DocTile {
    fn transform(&self, cr: &Cairo) {
        cr.translate(0.0, -f64::from(self.tile_y));
        cr.scale(self.scale);
        cr.translate(-f64::from(self.scroll_x), 0.0);
    }
}

unsafe extern "C" fn doc_tile_upper(index: c_int, data: *mut c_void) -> *mut c_void {
    let tile = unsafe { &mut *data.cast::<DocTile>() };
    let i = index as usize;
    if let Some(cr) = &tile.upper[i] {
        return cr.raw();
    }
    let buf = unsafe { *tile.bufs.add(i + 1) };
    unsafe { ptr::write_bytes(buf, 0, tile.stride as usize * tile.height as usize) };
    let cr = Cairo::for_data(buf, tile.width, tile.height, tile.stride, tile.scale);
    if let Some(cr) = &cr {
        tile.transform(cr);
    }
    unsafe { *tile.used.add(i) = glib::boolean(cr.is_some()) };
    let raw = cr.as_ref().map_or(ptr::null_mut(), Cairo::raw);
    tile.upper[i] = cr;
    raw
}

pub struct TileTarget {
    pub bufs: *const *mut u8,
    pub used: *mut GBoolean,
    pub width: c_int,
    pub height: c_int,
    pub stride: c_int,
    pub scroll_x: c_int,
    pub tile_y: c_int,
    pub scale: f64,
}

pub fn render_doc_tile(
    b: &NsBrowser,
    layout: BoxRef<'_>,
    plan: &Plan<'_>,
    target: &TileTarget,
    begin: impl Fn(),
    end: impl Fn(),
) -> c_int {
    let n_upper = plan.len();
    let mut tile = DocTile {
        bufs: target.bufs,
        upper: (0..n_upper.max(1)).map(|_| None).collect(),
        used: target.used,
        width: target.width,
        height: target.height,
        stride: target.stride,
        scroll_x: target.scroll_x,
        tile_y: target.tile_y,
        scale: target.scale,
    };
    for i in 0..n_upper {
        unsafe { *target.used.add(i) = 0 };
    }
    let first = unsafe { *target.bufs };
    let Some(cr) = Cairo::for_data(
        first,
        target.width,
        target.height,
        target.stride,
        target.scale,
    ) else {
        return -1;
    };
    tile.transform(&cr);
    begin();
    let ok = unsafe {
        ns_paint_doc_layers(
            cr.raw(),
            doc_tile_upper,
            ptr::from_mut(&mut tile).cast(),
            layout.as_ptr(),
            b.search_query.ptr(),
            b.selection_if_range(),
            plan.raw(),
        )
    };
    end();
    for upper in tile.upper.drain(..).take(n_upper).flatten() {
        drop(upper.finish());
    }
    drop(cr.finish());
    if ok != 0 { 0 } else { -2 }
}

pub fn set_search(case_sensitive: bool, active: *const NsBox) {
    unsafe { ns_paint_set_search(glib::boolean(case_sensitive), active) };
}

pub fn set_caret_visible(visible: bool) {
    unsafe { ns_paint_set_caret_visible(glib::boolean(visible)) };
}

pub fn canvas_color(layout: BoxRef<'_>, rgba: &mut [f64; 4]) -> bool {
    unsafe { ns_paint_canvas_color(layout.as_ptr(), rgba.as_mut_ptr()) != 0 }
}

pub fn set_hit_viewport(x: f64, y: f64) {
    unsafe { ns_box_set_hit_viewport(x, y) };
}

pub fn box_is_fixed(b: BoxRef<'_>) -> bool {
    unsafe { ns_box_is_fixed(b.as_ptr()) != 0 }
}

pub fn box_hit_offset(b: BoxRef<'_>) -> (f64, f64) {
    let (mut dx, mut dy) = (0.0, 0.0);
    unsafe { ns_box_hit_offset(b.as_ptr(), &mut dx, &mut dy) };
    (dx, dy)
}

pub fn subtree_extent_y(b: BoxRef<'_>, out: &mut VpLayerInfo) -> bool {
    unsafe { ns_box_subtree_extent_y(b.as_ptr(), &mut out.top, &mut out.bottom) != 0 }
}

pub fn sticky_y_model(b: BoxRef<'_>, viewport_h: f64, out: &mut VpLayerInfo) -> bool {
    unsafe { ns_box_sticky_y_model(b.as_ptr(), viewport_h, &mut out.sticky) != 0 }
}

pub fn sticky_offset(b: BoxRef<'_>, x0: f64, y0: f64, x1: f64, y1: f64) -> (f64, f64) {
    let (mut dx, mut dy) = (0.0, 0.0);
    unsafe { ns_box_sticky_offset(b.as_ptr(), x0, y0, x1, y1, &mut dx, &mut dy) };
    (dx, dy)
}

pub fn scroll_snap_viewport(
    layout: BoxRef<'_>,
    style: *const Style,
    viewport: (f64, f64),
    max: (f64, f64),
    prev: (f64, f64),
    pos: &mut (f64, f64),
) -> bool {
    unsafe {
        ns_box_scroll_snap_viewport(
            layout.as_ptr().cast_mut(),
            style,
            viewport.0,
            viewport.1,
            max.0,
            max.1,
            prev.0,
            prev.1,
            &mut pos.0,
            &mut pos.1,
        ) != 0
    }
}

pub fn snap_type_is_set(style: *const Style) -> bool {
    static PROP: std::sync::OnceLock<c_int> = std::sync::OnceLock::new();
    let prop = *PROP.get_or_init(|| unsafe { ns_css_prop_id(c"scroll-snap-type".as_ptr()) });
    let kw = unsafe { ns_style_keyword(style, prop) };
    !kw.is_null() && unsafe { CStr::from_ptr(kw) }.to_bytes() != b"none"
}

impl NsBrowser {
    pub fn selection_if_range(&self) -> *const c_void {
        if self.selection_has_range() {
            self.selection.get().cast_const().cast()
        } else {
            ptr::null()
        }
    }

    pub fn flush_video_composites(&self, now_us: i64) {
        unsafe { ns_video_cache_flush_composites(self.videos.get(), now_us) };
    }

    pub fn set_video_page_coords(&self, on: bool) {
        unsafe { ns_video_cache_set_page_coords(self.videos.get(), glib::boolean(on)) };
    }

    pub fn paint_search(&self) {
        set_search(self.search_case.get(), self.search_active.0.get());
    }

    pub fn style_for(&self, node: southstar_dom::Node<'_>) -> *const Style {
        let styles = self.styles.get();
        if styles.is_null() {
            return ptr::null();
        }
        unsafe { glib::g_hash_table_lookup(styles, node.as_ptr().cast()) }
            .cast_const()
            .cast()
    }
}
