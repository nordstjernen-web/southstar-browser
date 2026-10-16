//! Southstar — painting a page into embedder buffers: cairo contexts over pixel memory, the paint state, layer plans and their viewport captures.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::OnceCell;
use core::ffi::{CStr, c_char, c_int, c_void};
use core::marker::PhantomData;
use core::ptr::{self, NonNull};

use southstar_glib::{self as glib, GBoolean, GPtrArray};
use southstar_layout::{BoxRef, NsBox, Style};

use southstar_cairo::ContextRef;
use southstar_engine::Sheet;
use southstar_selection::NsSelection;

use super::{NsBrowser, PrintSetup};

const FORMAT_ARGB32: c_int = 0;
const STATUS_SUCCESS: c_int = 0;
const ANTIALIAS_FAST: c_int = 4;
pub const VP_STICKY: c_int = 2;

#[repr(C)]
#[derive(Default)]
pub struct StickyY {
    pub has_top: GBoolean,
    pub has_bottom: GBoolean,
    pub top_start: f64,
    pub top_cap: f64,
    pub bottom_start: f64,
    pub bottom_cap: f64,
}

#[repr(C)]
#[derive(Default)]
pub struct VpLayerInfo {
    pub kind: c_int,
    pub top: f64,
    pub bottom: f64,
    pub x_offset: f64,
    pub sticky: StickyY,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(core::mem::size_of::<VpLayerInfo>() == 72);

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

    fn cr(&self) -> ContextRef<'_> {
        // SAFETY: the context is live until destroy or finish consumes this value, which the borrow of self outlasts.
        unsafe { ContextRef::from_raw(self.raw()) }.expect("live cairo context")
    }

    pub fn paint(&self, b: &NsBrowser, layout: BoxRef<'_>) {
        let sel = b.selection_if_range();
        let query = b.search_query.get();
        southstar_paint::paint(self.cr(), layout, query.as_deref(), sel.as_ref());
    }

    pub fn plan_layers(&self, layout: BoxRef<'_>, plan: &mut LayerPlan) {
        southstar_paint::plan_layers(self.cr(), layout, plan);
    }

    pub fn vp_layer(
        &self,
        b: &NsBrowser,
        layout: BoxRef<'_>,
        cap: &VpCapture,
        vp_x: f64,
        vp_y: f64,
    ) {
        let query = b.search_query.get();
        southstar_paint::vp_layer(
            self.cr(),
            layout,
            cap,
            (vp_x, vp_y),
            query.as_deref(),
            b.selection_if_range().as_ref(),
        );
    }
}

pub struct PixelBuf<'a> {
    data: *mut u8,
    pub width: c_int,
    pub height: c_int,
    pub stride: c_int,
    _pixels: PhantomData<&'a mut [u8]>,
}

impl<'a> PixelBuf<'a> {
    pub unsafe fn new(data: *mut u8, width: c_int, height: c_int, stride: c_int) -> PixelBuf<'a> {
        PixelBuf {
            data,
            width,
            height,
            stride,
            _pixels: PhantomData,
        }
    }

    pub fn from_slice(
        pixels: &'a mut [u8],
        width: c_int,
        height: c_int,
        stride: c_int,
    ) -> Option<PixelBuf<'a>> {
        let row_bytes = usize::try_from(stride).ok()?;
        let width_bytes = usize::try_from(width).ok()?.checked_mul(4)?;
        let needed = usize::try_from(height).ok()?.checked_mul(row_bytes)?;
        if width_bytes > row_bytes || needed > pixels.len() {
            return None;
        }
        Some(PixelBuf {
            data: pixels.as_mut_ptr(),
            width,
            height,
            stride,
            _pixels: PhantomData,
        })
    }

    pub fn clear(&self) {
        unsafe { ptr::write_bytes(self.data, 0, self.stride as usize * self.height as usize) };
    }
}

pub struct PrintPages(Option<Vec<Sheet>>);

impl PrintPages {
    fn sheets(&self) -> &[Sheet] {
        self.0.as_deref().unwrap_or_default()
    }

    pub fn len(&self) -> usize {
        self.sheets().len()
    }

    pub fn is_empty(&self) -> bool {
        self.sheets().is_empty()
    }

    pub fn sheet(&self, index: usize) -> Option<NonNull<c_void>> {
        self.sheets().get(index).map(Sheet::as_ptr)
    }

    pub fn into_raw(self) -> *mut GPtrArray {
        let Some(sheets) = self.0 else {
            return ptr::null_mut();
        };
        // SAFETY: g_ptr_array_new has no preconditions.
        let array = unsafe { glib::g_ptr_array_new() };
        for sheet in sheets {
            // SAFETY: `array` is the live array made above, and it takes over the sheet's surface reference.
            unsafe { glib::g_ptr_array_add(array, sheet.into_raw().as_ptr()) };
        }
        array
    }
}

struct DocTile {
    bufs: *const *mut u8,
    upper: Vec<OnceCell<Cairo>>,
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

impl DocTile {
    fn upper(&self, index: c_int) -> Option<ContextRef<'_>> {
        let i = index as usize;
        if let Some(cr) = self.upper[i].get() {
            return Some(cr.cr());
        }
        // SAFETY: the embedder passes one buffer per viewport layer after the document's own, and `i` is below the plan's layer count.
        let buf = unsafe { *self.bufs.add(i + 1) };
        // SAFETY: each layer buffer holds `stride * height` writable bytes.
        unsafe { ptr::write_bytes(buf, 0, self.stride as usize * self.height as usize) };
        let cr = Cairo::for_data(buf, self.width, self.height, self.stride, self.scale);
        if let Some(cr) = &cr {
            self.transform(cr);
        }
        // SAFETY: the embedder's used flags hold one entry per viewport layer.
        unsafe { *self.used.add(i) = glib::boolean(cr.is_some()) };
        let cr = cr?;
        Some(self.upper[i].get_or_init(|| cr).cr())
    }
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
    plan: &LayerPlan,
    target: &TileTarget,
    begin: impl Fn(),
    end: impl Fn(),
) -> c_int {
    let n_upper = plan.viewport_layers();
    let tile = DocTile {
        bufs: target.bufs,
        upper: (0..n_upper.max(1)).map(|_| OnceCell::new()).collect(),
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
    let query = b.search_query.get();
    let sel = b.selection_if_range();
    let ok = southstar_paint::doc_layers(
        cr.cr(),
        &mut |index| tile.upper(index),
        layout,
        query.as_deref(),
        sel.as_ref(),
        plan,
    );
    end();
    for upper in tile
        .upper
        .into_iter()
        .take(n_upper)
        .filter_map(OnceCell::into_inner)
    {
        drop(upper.finish());
    }
    drop(cr.finish());
    if ok { 0 } else { -2 }
}

pub use southstar_paint::{LayerPlan, VpCapture, set_caret_visible, set_search};

pub fn canvas_color(layout: BoxRef<'_>, rgba: &mut [f64; 4]) -> bool {
    let (found, color) = southstar_paint::canvas_color(layout);
    *rgba = color;
    found
}

pub use southstar_layout_hit::{
    hit_offset as box_hit_offset, is_fixed as box_is_fixed, set_hit_viewport,
};

pub fn subtree_extent_y(b: BoxRef<'_>, out: &mut VpLayerInfo) -> bool {
    let extent = southstar_layout_query::subtree_extent_y(b);
    out.top = extent.top;
    out.bottom = extent.bottom;
    extent.exact
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
    // SAFETY: `style` is null or an entry of the page's style table, which outlives this call.
    let Some(style) = (unsafe { southstar_style::StyleRef::from_ptr(style) }) else {
        return false;
    };
    let port = southstar_layout_query::SnapViewport {
        size: viewport,
        max,
        prev,
    };
    match southstar_layout_query::scroll_snap_viewport(layout, style, &port, *pos) {
        Some(snapped) => {
            *pos = snapped;
            true
        }
        None => false,
    }
}

pub fn snap_type_is_set(style: *const Style) -> bool {
    static PROP: std::sync::OnceLock<c_int> = std::sync::OnceLock::new();
    let prop = *PROP.get_or_init(|| super::css_prop_id(c"scroll-snap-type"));
    super::style_keyword_raw(style, prop).is_some_and(|kw| kw.to_bytes() != b"none")
}

impl NsBrowser {
    pub fn print_recordings(&self, setup: &PrintSetup) -> PrintPages {
        PrintPages(
            self.layout()
                .map(|layout| southstar_engine::print_sheets(layout, setup)),
        )
    }

    pub fn selection_if_range(&self) -> Option<NsSelection> {
        self.selection_has_range().then(|| self.selection.get())
    }

    pub fn flush_video_composites(&self, now_us: i64) {
        if let Some(videos) = self.videos() {
            videos.flush_composites(now_us);
        }
    }

    pub fn set_video_page_coords(&self, on: bool) {
        if let Some(videos) = self.videos() {
            videos.set_page_coords(on);
        }
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
