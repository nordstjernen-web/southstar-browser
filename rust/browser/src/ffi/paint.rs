//! Southstar — painting a page into embedder buffers: cairo contexts over pixel memory, the paint state, layer plans and their viewport captures.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::Cell;
use core::ffi::{CStr, c_int};
use core::marker::PhantomData;
use core::ptr;

use southstar_css::RetainedStyle;
use southstar_glib::{self as glib, GBoolean};
use southstar_layout::BoxRef;

use southstar_cairo::{Antialias, Context, ContextRef, Format};
use southstar_engine::Sheet;
use southstar_selection::NsSelection;

use super::{NsBrowser, PrintSetup};

pub const VP_STICKY: c_int = 2;

pub use southstar_layout_position::StickyY;

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

pub struct Surface<'a> {
    surface: southstar_cairo::Surface,
    _pixels: PhantomData<&'a [Cell<u8>]>,
}

impl Surface<'static> {
    pub fn image(width: c_int, height: c_int) -> Option<Surface<'static>> {
        let surface = southstar_cairo::Surface::image(Format::Argb32, width, height)?;
        Some(Surface {
            surface,
            _pixels: PhantomData,
        })
    }
}

impl<'a> Surface<'a> {
    fn for_cells(
        pixels: &'a [Cell<u8>],
        width: c_int,
        height: c_int,
        stride: c_int,
    ) -> Option<Surface<'a>> {
        let row_bytes = usize::try_from(stride).ok()?;
        let width_bytes = usize::try_from(width).ok()?.checked_mul(4)?;
        let needed = usize::try_from(height).ok()?.checked_mul(row_bytes)?;
        if width_bytes > row_bytes || needed > pixels.len() {
            return None;
        }
        let data = pixels.as_ptr().cast_mut().cast::<u8>();
        // SAFETY: `data` addresses `pixels`, which holds at least `stride * height` bytes, is borrowed for 'a by the returned Surface and every Cairo drawing on it, and is only written through cairo while they live; Cell makes those writes legal behind the shared borrow.
        let surface = unsafe {
            southstar_cairo::Surface::for_data(data, Format::Argb32, width, height, stride)
        };
        surface.as_ref().status().is_ok().then_some(Surface {
            surface,
            _pixels: PhantomData,
        })
    }

    pub fn flush(&self) {
        self.surface.as_ref().flush();
    }

    pub fn write_png(&self, path: &CStr) {
        let _ = self.surface.as_ref().write_to_png(path);
    }

    pub fn copy_to_rgba(&self, out: &PixelBuf) {
        self.surface.as_ref().with_data(|src, src_stride| {
            let rows = (out.height as usize).min(src.len() / src_stride.max(1));
            for y in 0..rows {
                for x in 0..out.width as usize {
                    let at = y * src_stride + x * 4;
                    let px = u32::from_ne_bytes([src[at], src[at + 1], src[at + 2], src[at + 3]]);
                    let rgba = [
                        (px >> 16) as u8,
                        (px >> 8) as u8,
                        px as u8,
                        (px >> 24) as u8,
                    ];
                    let row = y * out.stride as usize + x * 4;
                    for (cell, byte) in out.pixels[row..row + 4].iter().zip(rgba) {
                        cell.set(byte);
                    }
                }
            }
        });
    }
}

fn tolerance(scale: f64) -> f64 {
    if scale > 0.0 { 0.5 / scale } else { 0.5 }
}

pub struct Cairo<'a> {
    context: Context,
    _pixels: PhantomData<&'a [Cell<u8>]>,
}

impl<'a> Cairo<'a> {
    pub fn on_surface(surface: &Surface<'a>, width: c_int, height: c_int, scale: f64) -> Cairo<'a> {
        let cr = Cairo {
            context: Context::new(surface.surface.as_ref()),
            _pixels: PhantomData,
        };
        cr.setup(width, height, scale);
        cr
    }

    fn for_data(
        pixels: &'a [Cell<u8>],
        width: c_int,
        height: c_int,
        stride: c_int,
        scale: f64,
    ) -> Option<Cairo<'a>> {
        let surface = Surface::for_cells(pixels, width, height, stride)?;
        Some(Cairo::on_surface(&surface, width, height, scale))
    }

    pub fn on_buffer(buf: &PixelBuf<'a>, scale: f64) -> Option<Cairo<'a>> {
        Cairo::for_data(buf.pixels, buf.width, buf.height, buf.stride, scale)
    }

    fn setup(&self, width: c_int, height: c_int, scale: f64) {
        let cr = self.context.as_ref();
        cr.set_tolerance(tolerance(scale));
        cr.set_antialias(Antialias::Fast);
        cr.rectangle(0.0, 0.0, f64::from(width), f64::from(height));
        cr.clip();
    }

    pub fn scale(&self, s: f64) {
        self.context.as_ref().scale(s, s);
    }

    pub fn translate(&self, x: f64, y: f64) {
        self.context.as_ref().translate(x, y);
    }

    pub fn destroy(self) {}

    pub fn finish(self) -> Surface<'a> {
        let surface = self.context.as_ref().target().reference();
        drop(self.context);
        surface.as_ref().flush();
        Surface {
            surface,
            _pixels: PhantomData,
        }
    }

    fn cr(&self) -> ContextRef<'_> {
        self.context.as_ref()
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
    pixels: &'a [Cell<u8>],
    pub width: c_int,
    pub height: c_int,
    pub stride: c_int,
}

impl<'a> PixelBuf<'a> {
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
            pixels: Cell::from_mut(&mut pixels[..needed]).as_slice_of_cells(),
            width,
            height,
            stride,
        })
    }

    pub fn clear(&self) {
        clear(self.pixels);
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

    pub fn sheet(&self, index: usize) -> Option<southstar_cairo::SurfaceRef<'_>> {
        let sheet = self.sheets().get(index)?;
        // SAFETY: the sheet holds a reference to its live recording surface for as long as `self` lives, and print pages stay on the thread that recorded them.
        unsafe { southstar_cairo::SurfaceRef::from_raw(sheet.as_ptr().as_ptr()) }
    }

    pub fn into_surfaces(self) -> Vec<southstar_cairo::Surface> {
        self.0
            .unwrap_or_default()
            .into_iter()
            // SAFETY: each sheet hands over its one reference to a live recording surface.
            .filter_map(|sheet| unsafe {
                southstar_cairo::Surface::from_raw_full(sheet.into_raw().as_ptr())
            })
            .collect()
    }
}

fn clear(pixels: &[Cell<u8>]) {
    for pixel in pixels {
        pixel.set(0);
    }
}

struct DocTile<'t, 'a> {
    target: &'t TileTarget<'a>,
}

impl<'a> DocTile<'_, 'a> {
    fn transform(&self, cr: &Cairo) {
        cr.translate(0.0, -f64::from(self.target.tile_y));
        cr.scale(self.target.scale);
        cr.translate(-f64::from(self.target.scroll_x), 0.0);
    }

    fn for_buffer<'p>(&self, pixels: &'p [Cell<u8>]) -> Option<Cairo<'p>> {
        let t = self.target;
        Cairo::for_data(pixels, t.width, t.height, t.stride, t.scale)
    }

    fn upper(&self, index: usize) -> Option<Cairo<'a>> {
        let cr = self.for_buffer(self.target.bufs[index + 1])?;
        self.transform(&cr);
        Some(cr)
    }
}

pub struct TileTarget<'a> {
    pub bufs: Vec<&'a [Cell<u8>]>,
    pub used: &'a [Cell<GBoolean>],
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
    target: &TileTarget<'_>,
    begin: impl Fn(),
    end: impl Fn(),
) -> c_int {
    let n_upper = plan.viewport_layers();
    let tile = DocTile { target };
    for used in &target.used[..n_upper] {
        used.set(0);
    }
    let Some(cr) = tile.for_buffer(target.bufs[0]) else {
        return -1;
    };
    tile.transform(&cr);
    begin();
    let query = b.search_query.get();
    let sel = b.selection_if_range();
    let upper: Vec<Option<Cairo>> = (0..n_upper).map(|index| tile.upper(index)).collect();
    let upper_refs: Vec<Option<ContextRef<'_>>> =
        upper.iter().map(|cr| cr.as_ref().map(Cairo::cr)).collect();
    let (ok, used) = southstar_paint::doc_layers(
        cr.cr(),
        &upper_refs,
        layout,
        query.as_deref(),
        sel.as_ref(),
        plan,
    );
    end();
    drop(upper_refs);
    for ((upper, used), flag) in upper.into_iter().zip(used).zip(&target.used[..n_upper]) {
        let Some(upper) = upper else {
            continue;
        };
        if used {
            flag.set(glib::boolean(true));
            drop(upper.finish());
        } else {
            upper.destroy();
        }
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
    let (model, ok) = southstar_layout_position::sticky_y_model(b, viewport_h);
    out.sticky = model;
    ok
}

pub use southstar_layout_position::sticky_offset;

pub fn scroll_snap_viewport(
    layout: BoxRef<'_>,
    style: &RetainedStyle,
    viewport: (f64, f64),
    max: (f64, f64),
    prev: (f64, f64),
    pos: &mut (f64, f64),
) -> bool {
    let style = southstar_style::StyleRef::retained(style);
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

pub fn snap_type_is_set(style: &RetainedStyle) -> bool {
    static PROP: std::sync::OnceLock<c_int> = std::sync::OnceLock::new();
    let prop = *PROP.get_or_init(|| super::css_prop_id(c"scroll-snap-type"));
    super::retained_keyword(style, prop).is_some_and(|kw| kw.to_bytes() != b"none")
}

impl NsBrowser {
    pub fn print_recordings(&self, setup: &PrintSetup) -> PrintPages {
        PrintPages(
            self.layout()
                .map(|layout| southstar_engine::print_sheets(layout, setup)),
        )
    }

    pub fn selection_if_range(&self) -> Option<NsSelection> {
        self.selection_has_range().then(|| self.selection.0.get())
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
        set_search(
            self.search_case.get(),
            self.search_active.get().map_or(ptr::null(), BoxRef::as_ptr),
        );
    }

    pub fn style_for(&self, node: southstar_dom::Node<'_>) -> Option<RetainedStyle> {
        self.styles.borrow().as_ref()?.retained(node.as_ptr())
    }
}
