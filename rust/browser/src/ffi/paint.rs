//! Southstar — painting a page into embedder buffers: cairo contexts over pixel memory, the paint state, layer plans and their viewport captures.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::marker::PhantomData;
use core::mem::ManuallyDrop;
use core::ptr::{self, NonNull};

use southstar_glib::{self as glib, GArray, GBoolean, GHashTable, GPtrArray};
use southstar_layout::{BoxRef, NsBox, Style};

use super::{NsBrowser, PrintSetup};

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
    fn ns_paint_layer_plan_init(plan: *mut LayerPlan);
    fn ns_paint_layer_plan_clear(plan: *mut LayerPlan);
    fn ns_engine_print_recordings(root: *const NsBox, setup: *const PrintSetup) -> *mut GPtrArray;
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
        let query = b.search_query.get();
        let query = query.as_deref().map_or(ptr::null(), CStr::as_ptr);
        unsafe {
            if sel.is_null() {
                ns_paint(self.raw(), layout.as_ptr(), query);
            } else {
                ns_paint_with_selection(self.raw(), layout.as_ptr(), query, sel);
            }
        }
    }

    pub fn plan_layers(&self, layout: BoxRef<'_>, plan: &mut LayerPlan) {
        unsafe { ns_paint_plan_layers(self.raw(), layout.as_ptr(), plan) };
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
        unsafe {
            ns_paint_vp_layer(
                self.raw(),
                layout.as_ptr(),
                cap,
                vp_x,
                vp_y,
                query.as_deref().map_or(ptr::null(), CStr::as_ptr),
                b.selection_if_range(),
            )
        };
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

impl LayerPlan {
    pub fn new() -> LayerPlan {
        let mut plan = LayerPlan {
            dynamic: 0,
            _kinds: ptr::null_mut(),
            vp: ptr::null_mut(),
        };
        // SAFETY: `plan` is a live, writable plan whose tables the paint layer creates here.
        unsafe { ns_paint_layer_plan_init(&mut plan) };
        plan
    }

    pub fn viewport_layers(&self) -> usize {
        // SAFETY: a plan's layer array is null or the GArray its init created, alive until it is cleared.
        unsafe { self.vp.as_ref() }.map_or(0, |vp| vp.len as usize)
    }

    pub(crate) fn dynamic(&self) -> bool {
        self.dynamic != 0
    }

    pub(crate) fn capture(&self, index: c_int) -> Option<&VpCapture> {
        let index = usize::try_from(index)
            .ok()
            .filter(|&i| i < self.viewport_layers())?;
        // SAFETY: `index` is below the layer array's length and its elements are VpCapture records.
        Some(unsafe { &*(*self.vp).data.cast::<VpCapture>().add(index) })
    }
}

impl Default for LayerPlan {
    fn default() -> LayerPlan {
        LayerPlan::new()
    }
}

impl Drop for LayerPlan {
    fn drop(&mut self) {
        // SAFETY: only plans made by LayerPlan::new are owned by Rust, and this frees their tables once.
        unsafe { ns_paint_layer_plan_clear(self) };
    }
}

pub struct PrintPages(Option<NonNull<GPtrArray>>);

impl PrintPages {
    fn sheets(&self) -> &[*mut c_void] {
        let Some(array) = self.0 else {
            return &[];
        };
        // SAFETY: the array came from ns_engine_print_recordings and lives until this value drops.
        let array = unsafe { array.as_ref() };
        if array.len == 0 {
            return &[];
        }
        // SAFETY: a non-empty pointer array holds `len` sheet pointers at `pdata`.
        unsafe { core::slice::from_raw_parts(array.pdata, array.len as usize) }
    }

    pub fn len(&self) -> usize {
        self.sheets().len()
    }

    pub fn is_empty(&self) -> bool {
        self.sheets().is_empty()
    }

    pub fn sheet(&self, index: usize) -> Option<NonNull<c_void>> {
        self.sheets().get(index).copied().and_then(NonNull::new)
    }

    pub fn into_raw(self) -> *mut GPtrArray {
        ManuallyDrop::new(self)
            .0
            .map_or(ptr::null_mut(), NonNull::as_ptr)
    }
}

impl Drop for PrintPages {
    fn drop(&mut self) {
        for &sheet in self.sheets() {
            // SAFETY: each sheet is a cairo recording surface whose one reference this array holds.
            unsafe { cairo_surface_destroy(sheet) };
        }
        if let Some(array) = self.0 {
            // SAFETY: the array is owned here and freed once, after the sheets it held.
            unsafe { glib::g_ptr_array_free(array.as_ptr(), glib::TRUE) };
        }
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
    plan: &LayerPlan,
    target: &TileTarget,
    begin: impl Fn(),
    end: impl Fn(),
) -> c_int {
    let n_upper = plan.viewport_layers();
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
    let query = b.search_query.get();
    let ok = unsafe {
        ns_paint_doc_layers(
            cr.raw(),
            doc_tile_upper,
            ptr::from_mut(&mut tile).cast(),
            layout.as_ptr(),
            query.as_deref().map_or(ptr::null(), CStr::as_ptr),
            b.selection_if_range(),
            plan,
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
        // SAFETY: the layout root is null or this page's live box tree, and `setup` is a valid page setup.
        PrintPages(NonNull::new(unsafe {
            ns_engine_print_recordings(self.layout.get(), setup)
        }))
    }

    pub fn selection_if_range(&self) -> *const c_void {
        if self.selection_has_range() {
            self.selection.get().cast_const().cast()
        } else {
            ptr::null()
        }
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
