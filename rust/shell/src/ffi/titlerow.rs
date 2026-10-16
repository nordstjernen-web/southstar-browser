//! Southstar — the title bar row widget: a box whose custom layout places its children.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_gtk_ffi::{self as gtk, GtkWidget};

use crate::titlerow::{self, Widths};

const PARTS_KEY: &CStr = c"ns-title-row";

struct Parts {
    start_controls: *mut GtkWidget,
    tabstrip: *mut GtkWidget,
    full_name: *mut GtkWidget,
    short_name: *mut GtkWidget,
    end_controls: *mut GtkWidget,
}

unsafe fn parts<'a>(row: *mut GtkWidget) -> &'a Parts {
    unsafe { &*gtk::g_object_get_data(row.cast(), PARTS_KEY.as_ptr()).cast::<Parts>() }
}

unsafe extern "C" fn free_parts(data: *mut c_void) {
    drop(unsafe { Box::from_raw(data.cast::<Parts>()) });
}

unsafe fn measure_width(w: *mut GtkWidget) -> (i32, i32) {
    let (mut minimum, mut natural) = (0, 0);
    unsafe {
        gtk::gtk_widget_measure(
            w,
            gtk::ORIENTATION_HORIZONTAL,
            -1,
            &mut minimum,
            &mut natural,
            ptr::null_mut(),
            ptr::null_mut(),
        )
    };
    (minimum, natural)
}

unsafe fn natural_width(w: *mut GtkWidget) -> i32 {
    unsafe { measure_width(w) }.1
}

unsafe fn visible_width(w: *mut GtkWidget) -> Option<i32> {
    (unsafe { gtk::gtk_widget_get_visible(w) } != 0).then(|| unsafe { natural_width(w) })
}

unsafe fn widths(r: &Parts) -> Widths {
    unsafe {
        let (tabs_min, tabs_nat) = measure_width(r.tabstrip);
        Widths {
            start_controls: visible_width(r.start_controls),
            end_controls: visible_width(r.end_controls),
            tabs_min,
            tabs_nat,
            full_name: natural_width(r.full_name),
            short_name: natural_width(r.short_name),
        }
    }
}

unsafe extern "C" fn measure(
    row: *mut GtkWidget,
    orientation: gtk::GtkOrientation,
    _for_size: c_int,
    minimum: *mut c_int,
    natural: *mut c_int,
    _minimum_baseline: *mut c_int,
    _natural_baseline: *mut c_int,
) {
    unsafe {
        let r = parts(row);
        *minimum = 0;
        *natural = 0;
        if orientation == gtk::ORIENTATION_VERTICAL {
            for child in [
                r.start_controls,
                r.tabstrip,
                r.full_name,
                r.short_name,
                r.end_controls,
            ] {
                let (mut min, mut nat) = (0, 0);
                gtk::gtk_widget_measure(
                    child,
                    gtk::ORIENTATION_VERTICAL,
                    -1,
                    &mut min,
                    &mut nat,
                    ptr::null_mut(),
                    ptr::null_mut(),
                );
                *minimum = (*minimum).max(min);
                *natural = (*natural).max(nat);
            }
            return;
        }
        let (min, nat) = titlerow::horizontal_size(&widths(r));
        *minimum = min;
        *natural = nat;
    }
}

unsafe fn is_rtl(row: *mut GtkWidget) -> bool {
    unsafe { gtk::gtk_widget_get_direction(row) == gtk::TEXT_DIR_RTL }
}

unsafe fn place(row: *mut GtkWidget, child: *mut GtkWidget, x: i32, width: i32, height: i32) {
    unsafe {
        let x = titlerow::mirrored_x(is_rtl(row), gtk::gtk_widget_get_width(row), x, width);
        let allocation = gtk::GtkAllocation {
            x,
            y: 0,
            width,
            height,
        };
        gtk::gtk_widget_size_allocate(child, &allocation, -1);
    }
}

unsafe fn centred_name_x(row: *mut GtkWidget, width: i32, name_width: i32) -> i32 {
    let (mut origin, mut span) = (0, width);
    unsafe {
        let header = gtk::gtk_widget_get_ancestor(row, gtk::gtk_header_bar_get_type());
        let mut p = gtk::GraphenePoint::default();
        if !header.is_null()
            && gtk::gtk_widget_compute_point(row, header, &gtk::GraphenePoint::default(), &mut p)
                != 0
        {
            span = gtk::gtk_widget_get_width(header);
            origin = p.x as i32;
            if is_rtl(row) {
                origin = span - origin - width;
            }
        }
    }
    titlerow::centred_name_x(span, origin, name_width)
}

unsafe extern "C" fn allocate(row: *mut GtkWidget, width: c_int, height: c_int, _baseline: c_int) {
    unsafe {
        let r = parts(row);
        let p = titlerow::place(&widths(r), width);
        if let Some(s) = p.start_controls {
            place(row, r.start_controls, s.x, s.width, height);
        }
        if let Some(s) = p.end_controls {
            place(row, r.end_controls, s.x, s.width, height);
        }
        place(row, r.tabstrip, p.tabs.x, p.tabs.width, height);
        let name_x = titlerow::name_x(&p, centred_name_x(row, width, p.name_width));
        gtk::gtk_widget_set_child_visible(r.full_name, c_int::from(p.full));
        gtk::gtk_widget_set_child_visible(r.short_name, c_int::from(!p.full));
        let name = if p.full { r.full_name } else { r.short_name };
        place(row, name, name_x, p.name_width, height);
    }
}

unsafe fn controls(row: *mut GtkWidget, side: gtk::GtkPackType) -> *mut GtkWidget {
    unsafe {
        let controls = gtk::gtk_window_controls_new(side);
        gtk::g_object_bind_property(
            controls.cast(),
            c"empty".as_ptr(),
            controls.cast(),
            c"visible".as_ptr(),
            gtk::BINDING_SYNC_CREATE | gtk::BINDING_INVERT_BOOLEAN,
        );
        gtk::gtk_widget_set_parent(controls, row);
        controls
    }
}

unsafe fn name(row: *mut GtkWidget, text: &str) -> *mut GtkWidget {
    let text = CString::new(text).unwrap_or_default();
    unsafe {
        let label = gtk::gtk_label_new(text.as_ptr());
        gtk::gtk_widget_add_css_class(label, c"ns-brand-title".as_ptr());
        gtk::gtk_widget_set_can_target(label, 0);
        gtk::gtk_widget_set_parent(label, row);
        label
    }
}

pub(super) fn ns_title_row_new(tabstrip: *mut GtkWidget) -> *mut GtkWidget {
    unsafe {
        let row = gtk::gtk_box_new(gtk::ORIENTATION_HORIZONTAL, 0);
        gtk::gtk_widget_set_layout_manager(
            row,
            gtk::gtk_custom_layout_new(None, Some(measure), Some(allocate)),
        );
        gtk::gtk_widget_set_hexpand(row, 1);
        let start_controls = controls(row, gtk::PACK_START);
        gtk::gtk_widget_set_parent(tabstrip, row);
        let full_name = name(row, &titlerow::full_name());
        let short_name = name(row, titlerow::SHORT_NAME);
        let end_controls = controls(row, gtk::PACK_END);
        let parts = Box::new(Parts {
            start_controls,
            tabstrip,
            full_name,
            short_name,
            end_controls,
        });
        gtk::g_object_set_data_full(
            row.cast(),
            PARTS_KEY.as_ptr(),
            Box::into_raw(parts).cast(),
            Some(free_parts),
        );
        row
    }
}
