//! Southstar — the NsIcon paintable behind icons.h: shell icons from the resource bundle, drawn by the SVG renderer.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_double, c_int, c_uint, c_void};
use core::ptr;
use std::collections::HashMap;
use std::ffi::CString;
use std::sync::{Mutex, OnceLock};

use southstar_gtk_ffi::{
    self as gtk, Cairo, GObject, GObjectClass, GdkPaintable, GdkPaintableFlags,
    GdkPaintableInterface, GdkRgba, GtkButton, GtkEntry, GtkImage, GtkMenuButton, GtkSnapshot,
    GtkSymbolicPaintableInterface, GtkWidget,
};

use crate::icons;

#[repr(C)]
struct NsNode {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Default)]
struct SvgSize {
    width: c_double,
    height: c_double,
    ratio: c_double,
    has_width: c_int,
    has_height: c_int,
    has_ratio: c_int,
}

unsafe extern "C" {
    fn ns_html_parse(input: *const c_char, len: isize) -> *mut NsNode;
    fn ns_node_free(node: *mut NsNode);
    fn ns_svg_document_root(doc: *const NsNode) -> *const NsNode;
    fn ns_svg_intrinsic_size(svg: *const NsNode, out: *mut SvgSize);
    fn ns_svg_render_node(
        cr: *mut Cairo,
        svg: *const NsNode,
        width: c_double,
        height: c_double,
        styles: *mut c_void,
        inherited: *const c_void,
    );
}

#[repr(C)]
struct NsIcon {
    parent_instance: GObject,
    doc: *mut NsNode,
    root: *const NsNode,
    width: c_int,
    height: c_int,
    symbolic: c_int,
}

fn icon_type() -> gtk::GType {
    static TYPE: OnceLock<gtk::GType> = OnceLock::new();
    *TYPE.get_or_init(|| unsafe {
        let ty = gtk::g_type_register_static_simple(
            gtk::g_object_get_type(),
            c"NsIcon".as_ptr(),
            size_of::<GObjectClass>() as c_uint,
            Some(class_init),
            size_of::<NsIcon>() as c_uint,
            Some(instance_init),
            0,
        );
        let paintable = gtk::GInterfaceInfo {
            interface_init: Some(paintable_init),
            interface_finalize: None,
            interface_data: ptr::null_mut(),
        };
        gtk::g_type_add_interface_static(ty, gtk::gdk_paintable_get_type(), &paintable);
        let symbolic = gtk::GInterfaceInfo {
            interface_init: Some(symbolic_paintable_init),
            interface_finalize: None,
            interface_data: ptr::null_mut(),
        };
        gtk::g_type_add_interface_static(ty, gtk::gtk_symbolic_paintable_get_type(), &symbolic);
        ty
    })
}

unsafe extern "C" fn class_init(klass: *mut c_void, _data: *mut c_void) {
    unsafe { (*klass.cast::<GObjectClass>()).finalize = Some(finalize) };
}

unsafe extern "C" fn instance_init(instance: *mut gtk::GTypeInstance, _klass: *mut c_void) {
    let icon = instance.cast::<NsIcon>();
    unsafe {
        (*icon).width = icons::DEFAULT_PX;
        (*icon).height = icons::DEFAULT_PX;
    }
}

unsafe extern "C" fn finalize(object: *mut GObject) {
    unsafe {
        let icon = object.cast::<NsIcon>();
        if !(*icon).doc.is_null() {
            ns_node_free((*icon).doc);
        }
        let parent = gtk::g_type_class_peek(gtk::g_object_get_type()).cast::<GObjectClass>();
        if let Some(parent_finalize) = (*parent).finalize {
            parent_finalize(object);
        }
    }
}

unsafe extern "C" fn paintable_init(iface: *mut c_void, _data: *mut c_void) {
    let iface = iface.cast::<GdkPaintableInterface>();
    unsafe {
        (*iface).snapshot = Some(snapshot);
        (*iface).get_flags = Some(get_flags);
        (*iface).get_intrinsic_width = Some(intrinsic_width);
        (*iface).get_intrinsic_height = Some(intrinsic_height);
    }
}

unsafe extern "C" fn symbolic_paintable_init(iface: *mut c_void, _data: *mut c_void) {
    unsafe {
        (*iface.cast::<GtkSymbolicPaintableInterface>()).snapshot_symbolic = Some(snapshot_symbolic)
    };
}

unsafe fn draw(
    icon: *const NsIcon,
    snapshot: *mut GtkSnapshot,
    width: f64,
    height: f64,
    color: Option<&GdkRgba>,
) {
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    let bounds = gtk::GrapheneRect::new(0.0, 0.0, width as f32, height as f32);
    unsafe {
        let root = (*icon).root;
        let cr = gtk::gtk_snapshot_append_cairo(snapshot, &bounds);
        match color {
            Some(color) => {
                gtk::cairo_push_group(cr);
                ns_svg_render_node(cr, root, width, height, ptr::null_mut(), ptr::null());
                let shape = gtk::cairo_pop_group(cr);
                gtk::cairo_set_source_rgba(
                    cr,
                    f64::from(color.red),
                    f64::from(color.green),
                    f64::from(color.blue),
                    f64::from(color.alpha),
                );
                gtk::cairo_mask(cr, shape);
                gtk::cairo_pattern_destroy(shape);
            }
            None => ns_svg_render_node(cr, root, width, height, ptr::null_mut(), ptr::null()),
        }
        gtk::cairo_destroy(cr);
    }
}

unsafe extern "C" fn snapshot(
    paintable: *mut GdkPaintable,
    snapshot: *mut GtkSnapshot,
    width: c_double,
    height: c_double,
) {
    unsafe { draw(paintable.cast(), snapshot, width, height, None) };
}

unsafe extern "C" fn snapshot_symbolic(
    paintable: *mut GdkPaintable,
    snapshot: *mut GtkSnapshot,
    width: c_double,
    height: c_double,
    colors: *const GdkRgba,
    n_colors: usize,
) {
    unsafe {
        let icon = paintable.cast::<NsIcon>();
        let foreground = ((*icon).symbolic != 0 && n_colors > 0).then(|| &*colors);
        draw(icon, snapshot, width, height, foreground);
    }
}

unsafe extern "C" fn get_flags(_paintable: *mut GdkPaintable) -> GdkPaintableFlags {
    gtk::PAINTABLE_STATIC_SIZE | gtk::PAINTABLE_STATIC_CONTENTS
}

unsafe extern "C" fn intrinsic_width(paintable: *mut GdkPaintable) -> c_int {
    unsafe { (*paintable.cast::<NsIcon>()).width }
}

unsafe extern "C" fn intrinsic_height(paintable: *mut GdkPaintable) -> c_int {
    unsafe { (*paintable.cast::<NsIcon>()).height }
}

unsafe fn icon_from_resource(name: &[u8]) -> *mut NsIcon {
    let Ok(path) = CString::new(icons::resource_path(name)) else {
        return ptr::null_mut();
    };
    unsafe {
        let bytes = gtk::g_resources_lookup_data(path.as_ptr(), 0, ptr::null_mut());
        if bytes.is_null() {
            return ptr::null_mut();
        }
        let mut len = 0usize;
        let data = gtk::g_bytes_get_data(bytes, &mut len);
        let doc = ns_html_parse(data.cast(), len as isize);
        gtk::g_bytes_unref(bytes);
        let root = if doc.is_null() {
            ptr::null()
        } else {
            ns_svg_document_root(doc)
        };
        if root.is_null() {
            if !doc.is_null() {
                ns_node_free(doc);
            }
            return ptr::null_mut();
        }
        let icon = gtk::g_object_new(icon_type(), ptr::null()).cast::<NsIcon>();
        (*icon).doc = doc;
        (*icon).root = root;
        (*icon).symbolic = c_int::from(icons::is_symbolic(name));
        let mut size = SvgSize::default();
        ns_svg_intrinsic_size(root, &mut size);
        (*icon).width = icons::intrinsic_px(size.has_width != 0, size.width);
        (*icon).height = icons::intrinsic_px(size.has_height != 0, size.height);
        icon
    }
}

static ICONS: Mutex<Option<HashMap<Vec<u8>, usize>>> = Mutex::new(None);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_icon_lookup(name: *const c_char) -> *mut GdkPaintable {
    if name.is_null() {
        return ptr::null_mut();
    }
    let name = unsafe { CStr::from_ptr(name) }.to_bytes();
    if name.is_empty() {
        return ptr::null_mut();
    }
    let mut icons = ICONS.lock().unwrap_or_else(|e| e.into_inner());
    let icons = icons.get_or_insert_with(HashMap::new);
    if let Some(&icon) = icons.get(name) {
        return icon as *mut GdkPaintable;
    }
    let icon = unsafe { icon_from_resource(name) };
    if icon.is_null() {
        return ptr::null_mut();
    }
    icons.insert(name.to_vec(), icon as usize);
    icon.cast()
}

unsafe fn centered_image(paintable: *mut GdkPaintable) -> *mut GtkWidget {
    unsafe {
        let image = gtk::gtk_image_new_from_paintable(paintable);
        gtk::gtk_widget_set_halign(image, gtk::ALIGN_CENTER);
        gtk::gtk_widget_set_valign(image, gtk::ALIGN_CENTER);
        image
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_icon_image_new(name: *const c_char) -> *mut GtkWidget {
    unsafe {
        let paintable = ns_icon_lookup(name);
        if paintable.is_null() {
            gtk::gtk_image_new_from_icon_name(name)
        } else {
            centered_image(paintable)
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_icon_image_set(image: *mut GtkImage, name: *const c_char) {
    unsafe {
        let paintable = ns_icon_lookup(name);
        if paintable.is_null() {
            gtk::gtk_image_set_from_icon_name(image, name);
        } else {
            gtk::gtk_image_set_from_paintable(image, paintable);
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_icon_button_new(name: *const c_char) -> *mut GtkWidget {
    unsafe {
        let button = gtk::gtk_button_new();
        ns_icon_button_set(button.cast(), name);
        button
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_icon_button_set(button: *mut GtkButton, name: *const c_char) {
    unsafe {
        let paintable = ns_icon_lookup(name);
        if paintable.is_null() {
            gtk::gtk_button_set_icon_name(button, name);
            return;
        }
        let child = gtk::gtk_button_get_child(button);
        if gtk::g_type_check_instance_is_a(child.cast(), gtk::gtk_image_get_type()) != 0 {
            gtk::gtk_image_set_from_paintable(child.cast(), paintable);
        } else {
            gtk::gtk_button_set_child(button, centered_image(paintable));
        }
        gtk::gtk_widget_add_css_class(button.cast(), c"image-button".as_ptr());
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_icon_menu_button_set(button: *mut GtkMenuButton, name: *const c_char) {
    unsafe {
        let paintable = ns_icon_lookup(name);
        if paintable.is_null() {
            gtk::gtk_menu_button_set_icon_name(button, name);
        } else {
            gtk::gtk_menu_button_set_child(button, centered_image(paintable));
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_icon_entry_set(
    entry: *mut GtkEntry,
    pos: gtk::GtkEntryIconPosition,
    name: *const c_char,
) {
    unsafe {
        let paintable = ns_icon_lookup(name);
        if paintable.is_null() {
            gtk::gtk_entry_set_icon_from_icon_name(entry, pos, name);
        } else {
            gtk::gtk_entry_set_icon_from_paintable(entry, pos, paintable);
        }
    }
}

unsafe fn pixbuf_loads_svg() -> bool {
    let mut found = false;
    unsafe {
        let formats = gtk::gdk_pixbuf_get_formats();
        let mut l = formats;
        while !l.is_null() && !found {
            let format = gtk::gdk_pixbuf_format_get_name((*l).data.cast());
            found = !format.is_null() && CStr::from_ptr(format).to_bytes() == b"svg";
            gtk::g_free(format.cast());
            l = (*l).next;
        }
        gtk::g_slist_free(formats);
    }
    found
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_icon_install_window_icon(name: *const c_char) {
    unsafe {
        if pixbuf_loads_svg() {
            gtk::gtk_window_set_default_icon_name(name);
        }
    }
}
