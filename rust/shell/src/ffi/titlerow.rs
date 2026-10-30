//! Southstar — the title bar row widget: a box whose custom layout places its children.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;
use std::ffi::CString;

use southstar_gtk::sys::{self as gtk_sys, GraphenePoint, GtkAllocation};
use southstar_gtk::{self as gtk, CustomLayout, WeakWidget, Widget};

use crate::titlerow::{self, Widths};

struct TitleRow {
    row: WeakWidget,
    start_controls: Widget,
    tabstrip: Widget,
    full_name: Widget,
    short_name: Widget,
    end_controls: Widget,
}

fn natural_width(w: &Widget) -> i32 {
    w.measure(gtk::ORIENTATION_HORIZONTAL, -1).1
}

fn visible_width(w: &Widget) -> Option<i32> {
    w.is_visible().then(|| natural_width(w))
}

impl TitleRow {
    fn widths(&self) -> Widths {
        let (tabs_min, tabs_nat) = self.tabstrip.measure(gtk::ORIENTATION_HORIZONTAL, -1);
        Widths {
            start_controls: visible_width(&self.start_controls),
            end_controls: visible_width(&self.end_controls),
            tabs_min,
            tabs_nat,
            full_name: natural_width(&self.full_name),
            short_name: natural_width(&self.short_name),
        }
    }

    fn children(&self) -> [&Widget; 5] {
        [
            &self.start_controls,
            &self.tabstrip,
            &self.full_name,
            &self.short_name,
            &self.end_controls,
        ]
    }
}

fn place(row: &Widget, child: &Widget, x: i32, width: i32, height: i32) {
    let x = titlerow::mirrored_x(row.is_rtl(), row.width(), x, width);
    child.size_allocate(
        GtkAllocation {
            x,
            y: 0,
            width,
            height,
        },
        -1,
    );
}

fn centred_name_x(row: &Widget, width: i32, name_width: i32) -> i32 {
    let (mut origin, mut span) = (0, width);
    if let Some(header) = row.ancestor(gtk::HeaderBar::static_type())
        && let Some(p) = row.compute_point(&header, GraphenePoint::default())
    {
        span = header.width();
        origin = p.x as i32;
        if row.is_rtl() {
            origin = span - origin - width;
        }
    }
    titlerow::centred_name_x(span, origin, name_width)
}

impl CustomLayout for TitleRow {
    fn measure(&self, orientation: gtk::GtkOrientation, _for_size: c_int) -> (c_int, c_int) {
        if orientation == gtk::ORIENTATION_VERTICAL {
            return self
                .children()
                .into_iter()
                .fold((0, 0), |(minimum, natural), child| {
                    let (min, nat) = child.measure(gtk::ORIENTATION_VERTICAL, -1);
                    (minimum.max(min), natural.max(nat))
                });
        }
        titlerow::horizontal_size(&self.widths())
    }

    fn allocate(&self, width: c_int, height: c_int, _baseline: c_int) {
        let Some(row) = self.row.upgrade() else {
            return;
        };
        let p = titlerow::place(&self.widths(), width);
        if let Some(s) = p.start_controls {
            place(&row, &self.start_controls, s.x, s.width, height);
        }
        if let Some(s) = p.end_controls {
            place(&row, &self.end_controls, s.x, s.width, height);
        }
        place(&row, &self.tabstrip, p.tabs.x, p.tabs.width, height);
        let name_x = titlerow::name_x(&p, centred_name_x(&row, width, p.name_width));
        self.full_name.set_child_visible(p.full);
        self.short_name.set_child_visible(!p.full);
        let name = if p.full {
            &self.full_name
        } else {
            &self.short_name
        };
        place(&row, name, name_x, p.name_width, height);
    }
}

fn controls(row: &Widget, side: gtk_sys::GtkPackType) -> Widget {
    let controls = gtk::WindowControls::new(side);
    controls.object().bind_property(
        c"empty",
        controls.object(),
        c"visible",
        gtk_sys::BINDING_SYNC_CREATE | gtk_sys::BINDING_INVERT_BOOLEAN,
    );
    controls.set_parent(row);
    controls.into()
}

fn name(row: &Widget, text: &str) -> Widget {
    let text = CString::new(text).unwrap_or_default();
    let label = gtk::Label::new(&text);
    label.add_css_class(c"ns-brand-title");
    label.set_can_target(false);
    label.set_parent(row);
    label.into()
}

pub(super) fn ns_title_row_new(tabstrip: &Widget) -> Widget {
    let row: Widget = gtk::Box::new(gtk::ORIENTATION_HORIZONTAL, 0).into();
    row.set_hexpand(true);
    let start_controls = controls(&row, gtk_sys::PACK_START);
    tabstrip.set_parent(&row);
    let full_name = name(&row, &titlerow::full_name());
    let short_name = name(&row, titlerow::SHORT_NAME);
    let end_controls = controls(&row, gtk_sys::PACK_END);
    gtk::set_custom_layout(
        &row,
        TitleRow {
            row: row.downgrade(),
            start_controls,
            tabstrip: tabstrip.clone(),
            full_name,
            short_name,
            end_controls,
        },
    );
    row
}
