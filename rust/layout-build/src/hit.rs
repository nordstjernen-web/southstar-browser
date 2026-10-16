//! Southstar — which form control of an inline run a point falls on.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_int, c_uint};

use southstar_dom::Node;
use southstar_layout::{BoxRef, InlineAttr, inline_kind};
use southstar_layout_content::controls::keyword_is;
use southstar_layout_geom::text_indent_px;
use southstar_paint::ffi::pango::{
    ALIGN_CENTER, ALIGN_LEFT, ALIGN_RIGHT, AttrList, DIRECTION_RTL, ELLIPSIZE_END, SCALE,
};
use southstar_style::{PropId, StyleRef};

use crate::ffi;

fn is_form_hit(k: c_uint) -> bool {
    matches!(
        k,
        inline_kind::INPUT_FIELD
            | inline_kind::INPUT_FIELD_FOCUSED
            | inline_kind::BUTTON
            | inline_kind::CHECKBOX
            | inline_kind::CHECKBOX_CHECKED
            | inline_kind::RADIO
            | inline_kind::RADIO_CHECKED
    )
}

fn is_button_hit(k: c_uint) -> bool {
    matches!(
        k,
        inline_kind::BUTTON
            | inline_kind::CHECKBOX
            | inline_kind::CHECKBOX_CHECKED
            | inline_kind::RADIO
            | inline_kind::RADIO_CHECKED
    )
}

fn hit_candidate(r: &InlineAttr, parent_style: Option<StyleRef<'_>>) -> bool {
    if !is_form_hit(r.kind) || r.dom_ptr().is_null() {
        return false;
    }
    let rs = ffi::style(r.style()).or(parent_style);
    !ffi::style_blocks_hit_testing(rs)
}

pub fn inline_box_form_hit<'a>(
    b: BoxRef<'a>,
    local_x: f64,
    local_y: f64,
    parent_style: Option<StyleRef<'_>>,
) -> Option<Node<'a>> {
    let attrs = b.attrs();
    let atomics = b.inline_atomics().unwrap_or(&[]);
    if attrs.is_empty() && atomics.is_empty() {
        return None;
    }
    let text = b.text().filter(|t| !t.is_empty())?;
    let ps = parent_style.map_or(core::ptr::null(), StyleRef::as_ptr);
    let layout = ffi::new_layout(ps);
    layout.set_width((b.content_width() * f64::from(SCALE)) as c_int);
    layout.set_wrap(ffi::wrap_mode_for(ps));
    if atomics.is_empty() {
        ffi::apply_css_line_spacing(&layout, ps);
    }
    let ti = if b.inline_split_tail() {
        0.0
    } else {
        text_indent_px(parent_style, b.content_width())
    };
    if ti > 0.0 {
        layout.set_indent((ti * f64::from(SCALE)) as c_int);
    }
    if keyword_is(
        parent_style.and_then(|s| s.get(PropId::TextOverflow)),
        c"ellipsis",
    ) {
        layout.set_ellipsize(ELLIPSIZE_END);
    }
    layout.set_text(text);
    let i18n = AttrList::new();
    ffi::apply_inline_attrs(&layout, &i18n, b, ps);
    let ta = parent_style.and_then(|s| s.get(PropId::TextAlign));
    let rtl = layout.context().base_dir() == DIRECTION_RTL;
    if keyword_is(ta, c"center") {
        layout.set_alignment(ALIGN_CENTER);
    } else if keyword_is(ta, c"right")
        || (keyword_is(ta, c"end") && !rtl)
        || (keyword_is(ta, c"start") && rtl)
        || (ta.is_none() && rtl)
    {
        layout.set_alignment(ALIGN_RIGHT);
    } else if keyword_is(ta, c"justify") {
        layout.set_justify(true);
    } else {
        layout.set_alignment(ALIGN_LEFT);
    }
    ffi::start_align_overflow(&layout);

    let mut button_hit = None;
    let mut field_hit = None;
    let mut atomic_hit = None;
    let scale = f64::from(SCALE);
    if let Some(index) = layout
        .xy_to_index_inside((local_x * scale) as c_int, (local_y * scale) as c_int)
        .filter(|&i| i >= 0)
    {
        let idx = index as usize;
        for a in atomics {
            let Some(ab) = a.box_ref() else {
                continue;
            };
            if idx < a.byte_off() || idx >= a.byte_off() + 3 {
                continue;
            }
            if let Some(dom) = ab.dom().filter(|d| ffi::node_is_form_hit_target(Some(*d))) {
                atomic_hit = Some(dom.as_ptr());
            }
        }
        for r in attrs {
            if !hit_candidate(r, parent_style) || idx < r.start || idx >= r.start + r.len {
                continue;
            }
            if is_button_hit(r.kind) {
                button_hit = Some(r.dom_ptr());
            } else if field_hit.is_none() {
                field_hit = Some(r.dom_ptr());
            }
        }
    }
    for r in attrs {
        if !hit_candidate(r, parent_style) {
            continue;
        }
        let r0 = layout.index_to_pos(r.start as c_int);
        let r1 = layout.index_to_pos((r.start + r.len - 1) as c_int);
        let sized = r.box_w > 0.0 || r.box_h > 0.0;
        let bleed_x = if sized { 0.0 } else { 10.0 };
        let bleed_y = if sized { 0.0 } else { 5.0 };
        let mut x0 = f64::from(r0.x) / scale - bleed_x;
        let mut y0 = f64::from(r0.y) / scale - bleed_y;
        let mut x1 = f64::from(r1.x + r1.width) / scale + bleed_x;
        let mut y1 = f64::from(r0.y + r0.height) / scale + bleed_y;
        let css_w = ffi::inline_attr_control_width(r, b);
        if css_w > 0.0 {
            x0 = f64::from(r0.x) / scale;
            x1 = x0 + css_w;
        }
        if r.box_h > 0.0 {
            let cy = (y0 + y1) / 2.0;
            y0 = cy - r.box_h / 2.0;
            y1 = cy + r.box_h / 2.0;
        }
        if local_x < x0 || local_x > x1 || local_y < y0 || local_y > y1 {
            continue;
        }
        if is_button_hit(r.kind) {
            button_hit = Some(r.dom_ptr());
        } else if field_hit.is_none() {
            field_hit = Some(r.dom_ptr());
        }
    }
    let hit = atomic_hit
        .or(button_hit.map(|p| p.cast()))
        .or(field_hit.map(|p| p.cast()))?;
    ffi::node(hit)
}
