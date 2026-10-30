//! Southstar — the shell icon paintable: icons from the resource bundle, drawn by the SVG renderer.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::RefCell;
use core::ffi::{CStr, c_int};
use core::mem::ManuallyDrop;
use std::collections::HashMap;
use std::ffi::CString;

use southstar_cairo::ContextRef;
use southstar_dom::{Tree, WeakNode};
use southstar_gtk::sys::{self as gtk_sys, GdkRgba};
use southstar_gtk::{self as gtk, Paintable, PaintableImpl};

use crate::icons;

struct SvgIcon {
    _doc: Tree,
    root: WeakNode,
    width: c_int,
    height: c_int,
    symbolic: bool,
}

impl PaintableImpl for SvgIcon {
    fn intrinsic_width(&self) -> c_int {
        self.width
    }

    fn intrinsic_height(&self) -> c_int {
        self.height
    }

    fn snapshot(&self, cr: ContextRef<'_>, width: f64, height: f64, colors: &[GdkRgba]) {
        let Some(root) = self.root.get() else {
            return;
        };
        let canvas = southstar_svg::Canvas::from(cr);
        match colors.first().filter(|_| self.symbolic) {
            Some(color) => {
                cr.push_group();
                southstar_svg::render_root(canvas, root, width, height);
                let shape = cr.pop_group();
                cr.set_source_rgba(
                    f64::from(color.red),
                    f64::from(color.green),
                    f64::from(color.blue),
                    f64::from(color.alpha),
                );
                cr.mask(&shape);
            }
            None => southstar_svg::render_root(canvas, root, width, height),
        }
    }
}

fn icon_from_resource(name: &[u8]) -> Option<Paintable> {
    let path = CString::new(icons::resource_path(name)).ok()?;
    let data = gtk::resource_bytes(&path)?;
    let doc = southstar_html_parser::parse_document(&data, true);
    let root = southstar_svg::find_root(doc.root())?;
    let size = southstar_svg::intrinsic_size(Some(root));
    Some(Paintable::new(SvgIcon {
        root: root.downgrade(),
        _doc: doc,
        width: icons::intrinsic_px(size.has_width, size.width),
        height: icons::intrinsic_px(size.has_height, size.height),
        symbolic: icons::is_symbolic(name),
    }))
}

thread_local! {
    static ICONS: ManuallyDrop<RefCell<HashMap<Vec<u8>, Paintable>>> =
        ManuallyDrop::new(RefCell::new(HashMap::new()));
}

fn lookup(name: &CStr) -> Option<Paintable> {
    let name = name.to_bytes();
    if name.is_empty() {
        return None;
    }
    ICONS.with(|icons| {
        if let Some(icon) = icons.borrow().get(name) {
            return Some(icon.clone());
        }
        let icon = icon_from_resource(name)?;
        icons.borrow_mut().insert(name.to_vec(), icon.clone());
        Some(icon)
    })
}

fn centered_image(paintable: &Paintable) -> gtk::Image {
    let image = gtk::Image::from_paintable(paintable);
    image.set_halign(gtk_sys::ALIGN_CENTER);
    image.set_valign(gtk_sys::ALIGN_CENTER);
    image
}

pub(super) fn image_new(name: &CStr) -> gtk::Image {
    match lookup(name) {
        Some(paintable) => centered_image(&paintable),
        None => gtk::Image::from_icon_name(name),
    }
}

pub(super) fn image_set(image: &gtk::Image, name: &CStr) {
    match lookup(name) {
        Some(paintable) => image.set_from_paintable(&paintable),
        None => image.set_from_icon_name(name),
    }
}

pub(super) fn button_new(name: &CStr) -> gtk::Button {
    let button = gtk::Button::new();
    button_set(&button, name);
    button
}

pub(super) fn button_set(button: &gtk::Button, name: &CStr) {
    let Some(paintable) = lookup(name) else {
        button.set_icon_name(name);
        return;
    };
    match button.child().as_ref().and_then(gtk::Image::cast) {
        Some(image) => image.set_from_paintable(&paintable),
        None => button.set_child(&centered_image(&paintable)),
    }
    button.add_css_class(c"image-button");
}

pub(super) fn menu_button_set(button: &gtk::MenuButton, name: &CStr) {
    match lookup(name) {
        Some(paintable) => button.set_child(&centered_image(&paintable)),
        None => button.set_icon_name(name),
    }
}

pub(super) fn entry_set(entry: &gtk::Entry, pos: gtk_sys::GtkEntryIconPosition, name: &CStr) {
    match lookup(name) {
        Some(paintable) => entry.set_icon_from_paintable(pos, &paintable),
        None => entry.set_icon_from_icon_name(pos, name),
    }
}

pub(super) fn install_window_icon(name: &CStr) {
    if gtk::pixbuf_loads_format(b"svg") {
        gtk::Window::set_default_icon_name(name);
    }
}
