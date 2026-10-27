//! Southstar — the text and attributes inline collection writes for form controls: gauges, inputs of every type, buttons, selects and textareas.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_long};

use southstar_dom::{Node, children, controls, select as dom_select, serialize};
use southstar_glib::ascii_strtod;
use southstar_layout::{InlineAttr, inline_kind};
use southstar_layout_content::controls::{
    atoi, emit_color, emit_control_text_style, emit_form_attr_sized, meter_state, pad_spaces,
    progress_fraction, run_at_line_start,
};
use southstar_layout_content::{pseudo, select, text_input};
use southstar_style::{PropId, StyleRef};

use crate::collect::{Collector, LINE_SEPARATOR, NBSP, same};

const BULLET: &[u8] = "\u{2022}".as_bytes();

fn type_is(ty: Option<&[u8]>, names: &[&[u8]]) -> bool {
    ty.is_some_and(|t| names.iter().any(|n| t.eq_ignore_ascii_case(n)))
}

fn file_name(path: &[u8]) -> &[u8] {
    let slash = path.iter().rposition(|&c| c == b'/');
    #[cfg(windows)]
    let slash = match (slash, path.iter().rposition(|&c| c == b'\\')) {
        (None, w) => w,
        (s, Some(w)) if Some(w) > s => Some(w),
        (s, _) => s,
    };
    slash.map_or(path, |i| &path[i + 1..])
}

fn hex_pair(s: &[u8], at: &mut usize) -> Option<u32> {
    while s
        .get(*at)
        .is_some_and(|&c| matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r'))
    {
        *at += 1;
    }
    let start = *at;
    let mut v = 0u32;
    while *at - start < 2
        && let Some(d) = s.get(*at).and_then(|&c| char::from(c).to_digit(16))
    {
        v = v * 16 + d;
        *at += 1;
    }
    (*at > start).then_some(v)
}

fn parse_hex_color(hex: &[u8]) -> [u8; 3] {
    if hex.first() != Some(&b'#') || hex.len() < 7 {
        return [0, 0, 0];
    }
    let mut at = 1;
    match (
        hex_pair(hex, &mut at),
        hex_pair(hex, &mut at),
        hex_pair(hex, &mut at),
    ) {
        (Some(r), Some(g), Some(b)) => [r as u8, g as u8, b as u8],
        _ => [0, 0, 0],
    }
}

impl<'a> Collector<'a> {
    fn form_attr(&mut self, kind: u32, start: usize, dom: Node<'_>) {
        let end = self.out.len();
        emit_form_attr_sized(&mut self.attrs, kind, start, end, Some(dom), self.styles);
    }

    pub(crate) fn gauge(&mut self, n: Node<'_>, is_meter: bool) {
        let start = self.out.len();
        self.push_repeat(NBSP, 12);
        let (frac, rgba) = if is_meter {
            let (frac, quality) = meter_state(Some(n));
            let rgb = match quality {
                0 => [0x2e, 0x9d, 0x54],
                1 => [0xd0, 0xa4, 0x1f],
                _ => [0xc4, 0x43, 0x3c],
            };
            (frac, [rgb[0], rgb[1], rgb[2], 255])
        } else {
            (progress_fraction(Some(n)), [0, 0, 0, 0])
        };
        let kind = if is_meter {
            inline_kind::METER
        } else {
            inline_kind::PROGRESS
        };
        let mut a = InlineAttr::new(kind, start, self.out.len() - start).with_dom(Some(n));
        a.font_size_px = frac;
        [a.r, a.g, a.b, a.a] = rgba;
        self.attrs.push(a);
    }

    pub(crate) fn input(&mut self, n: Node<'_>, s: Option<StyleRef<'_>>) {
        let ty = n.attr(c"type").map(CStr::to_bytes);
        let is_password = type_is(ty, &[b"password"]);
        let is_text = ty.is_none_or(<[u8]>::is_empty)
            || is_password
            || type_is(
                ty,
                &[b"text", b"search", b"email", b"url", b"tel", b"number"],
            );
        if is_text {
            self.text_input(n, s, is_password);
        } else if type_is(ty, &[b"submit", b"button", b"reset"]) {
            let start = self.out.len();
            let v = match n.attr(c"value") {
                Some(v) => v.to_bytes(),
                None if type_is(ty, &[b"submit"]) => b"Submit",
                None if type_is(ty, &[b"reset"]) => b"Reset",
                None => b"Button",
            };
            let lead = pad_spaces(s, false);
            let trail = pad_spaces(s, true);
            self.push(NBSP);
            self.push_repeat(NBSP, c_long::from(lead));
            self.push(v);
            self.push_repeat(NBSP, c_long::from(trail));
            self.push(NBSP);
            self.form_attr(inline_kind::BUTTON, start, n);
            self.push(b" ");
        } else if type_is(ty, &[b"checkbox"]) {
            let checked = controls::is_checked(n);
            let start = self.out.len();
            self.push(if checked { "\u{2611}" } else { "\u{2610}" }.as_bytes());
            let kind = if checked {
                inline_kind::CHECKBOX_CHECKED
            } else {
                inline_kind::CHECKBOX
            };
            self.form_attr(kind, start, n);
            self.push(b" ");
        } else if type_is(ty, &[b"radio"]) {
            let checked = controls::is_checked(n);
            let start = self.out.len();
            self.push(if checked { "\u{25C9}" } else { "\u{25CB}" }.as_bytes());
            let kind = if checked {
                inline_kind::RADIO_CHECKED
            } else {
                inline_kind::RADIO
            };
            self.form_attr(kind, start, n);
            self.push(b" ");
        } else if type_is(ty, &[b"file"]) {
            let start = self.out.len();
            self.push("\u{a0}Choose File\u{a0}".as_bytes());
            self.form_attr(inline_kind::BUTTON, start, n);
            match n.attr(c"data-nd-file-path").filter(|p| !p.is_empty()) {
                Some(path) => {
                    self.push(b" ");
                    self.push(file_name(path.to_bytes()));
                }
                None => self.push(b" (no file chosen)"),
            }
        } else if type_is(ty, &[b"color"]) {
            let hex = n
                .attr(c"value")
                .map(CStr::to_bytes)
                .filter(|v| !v.is_empty())
                .unwrap_or(b"#000000");
            let start = self.out.len();
            self.push(NBSP);
            let swatch_start = self.out.len();
            self.push("\u{25A0}".as_bytes());
            let swatch_end = self.out.len();
            self.push(NBSP);
            self.push(hex);
            self.push(NBSP);
            self.form_attr(inline_kind::INPUT_FIELD, start, n);
            let [r, g, b] = parse_hex_color(hex);
            emit_color(&mut self.attrs, swatch_start, swatch_end, [r, g, b, 255]);
        } else if type_is(ty, &[b"range"]) {
            let num = |name: &CStr, dflt: f64| {
                n.attr(name)
                    .map(CStr::to_bytes)
                    .filter(|v| !v.is_empty())
                    .map_or(dflt, ascii_strtod)
            };
            let vv = num(c"value", 50.0);
            let mnv = num(c"min", 0.0);
            let mut mxv = num(c"max", 100.0);
            if mxv <= mnv {
                mxv = mnv + 1.0;
            }
            let frac = ((vv - mnv) / (mxv - mnv)).clamp(0.0, 1.0);
            let knob_at = (frac * 10.0 + 0.5) as i32;
            let start = self.out.len();
            self.push(NBSP);
            for i in 0..=10 {
                self.push(if i == knob_at { "\u{25CF}" } else { "\u{2500}" }.as_bytes());
            }
            self.push(b" ");
            self.push(southstar_numfmt::g(vv).as_bytes());
            self.push(NBSP);
            self.form_attr(inline_kind::INPUT_FIELD, start, n);
        } else if type_is(
            ty,
            &[b"date", b"datetime-local", b"time", b"month", b"week"],
        ) {
            let start = self.out.len();
            self.push(NBSP);
            match n.attr(c"value").filter(|v| !v.is_empty()) {
                Some(v) => self.push(v.to_bytes()),
                None => self.push(b"____-__-__"),
            }
            self.push(NBSP);
            self.form_attr(inline_kind::INPUT_FIELD, start, n);
        }
    }

    fn text_input(&mut self, n: Node<'_>, s: Option<StyleRef<'_>>, is_password: bool) {
        if !run_at_line_start(&self.out) {
            self.push_repeat(NBSP, 3);
        }
        let start = self.out.len();
        self.push(NBSP);
        self.push_repeat(NBSP, c_long::from(pad_spaces(s, false)));
        let real_value = controls::used_value(n);
        let mut v = real_value;
        let mut is_placeholder = false;
        let mut v_is_real = true;
        if v.is_none_or(|v| v.is_empty()) {
            v = n.attr(c"placeholder");
            v_is_real = false;
            is_placeholder = v.is_some_and(|v| !v.is_empty());
        }
        let focused = same(self.state.focused_input, n);
        let mut val_start = self.out.len();
        let mut disp_end = val_start;
        let mut caret_pos = val_start;
        let mut anchor_pos = val_start;
        let mut caret_byte = self.state.caret_byte;
        let mut anchor_byte = self.state.anchor_byte;
        let real_len = real_value.map(|r| r.to_bytes().len());
        if let Some(len) = real_len
            && caret_byte > len
        {
            caret_byte = len;
        }
        match real_len {
            Some(len) if anchor_byte > len => anchor_byte = len,
            None => anchor_byte = 0,
            _ => {}
        }
        let size = c_long::from(text_input::columns(Some(n)));
        let mut displayed_chars: c_long = 0;
        let v = v.filter(|v| !v.is_empty());
        if let Some(v) = v.filter(|_| is_password && !is_placeholder) {
            let cps = text_input::utf8_len(v.to_bytes());
            let skip_cps = if focused {
                text_input::focused_skip_cps(real_value, caret_byte, cps, size)
            } else {
                text_input::align_skip_cps(s, Some(n), cps, size)
            };
            let shown_cps = (cps - skip_cps).min(size);
            self.push_repeat(BULLET, shown_cps);
            displayed_chars = shown_cps;
            disp_end = self.out.len();
            if focused {
                let cp_before = real_value.map_or(0, |r| {
                    text_input::pointer_to_offset(r.to_bytes(), caret_byte)
                });
                let cp_in_shown = if cp_before > skip_cps {
                    cp_before - skip_cps
                } else {
                    0
                }
                .min(shown_cps);
                caret_pos = val_start + cp_in_shown as usize * 3;
            }
        } else if let Some(v) = v {
            let bytes = v.to_bytes();
            let mut display = bytes;
            let mut skip_bytes = 0;
            let value_cps = text_input::utf8_len(bytes);
            if value_cps > size {
                let skip_cps = if focused && v_is_real {
                    text_input::focused_skip_cps(real_value, caret_byte, value_cps, size)
                } else {
                    text_input::align_skip_cps(s, Some(n), value_cps, size)
                };
                let p = text_input::advance_cps(bytes, skip_cps);
                let q = p + text_input::advance_cps(&bytes[p..], size);
                display = &bytes[p..q];
                if v_is_real {
                    skip_bytes = p;
                }
            }
            self.push(display);
            displayed_chars = text_input::utf8_len(display);
            disp_end = self.out.len();
            let real_nonempty = real_value.is_some_and(|r| !r.is_empty());
            if focused && real_nonempty {
                let caret_in_shown = caret_byte.saturating_sub(skip_bytes).min(display.len());
                caret_pos = val_start + caret_in_shown;
                let anchor_in_shown = anchor_byte.saturating_sub(skip_bytes).min(display.len());
                anchor_pos = val_start + anchor_in_shown;
            } else if focused {
                caret_pos = val_start;
                anchor_pos = val_start;
            }
        } else if focused {
            caret_pos = val_start;
            anchor_pos = val_start;
        }
        let pad = (size - displayed_chars).max(0);
        let (mut left_pad, mut right_pad) = (0, pad);
        if pad > 0 {
            left_pad = text_input::align_offset(s, Some(n), pad);
            right_pad = pad - left_pad;
        }
        if left_pad > 0 {
            let lp: Vec<u8> = NBSP.repeat(left_pad as usize);
            let shift = lp.len();
            self.out.splice(val_start..val_start, lp);
            caret_pos += shift;
            anchor_pos += shift;
            disp_end += shift;
            val_start += shift;
        }
        self.push_repeat(NBSP, right_pad);
        self.push(NBSP);
        let kind = if focused {
            inline_kind::INPUT_FIELD_FOCUSED
        } else {
            inline_kind::INPUT_FIELD
        };
        self.form_attr(kind, start, n);
        let disabled = n.attr(c"disabled").is_some();
        let end = self.out.len();
        emit_control_text_style(
            &mut self.attrs,
            s,
            (start, end),
            (val_start, disp_end),
            is_placeholder,
            disabled,
        );
        self.spellcheck(n, is_placeholder, disabled, val_start, disp_end);
        if focused {
            self.caret_attrs(caret_pos, anchor_pos);
        }
        if focused && self.state.datalist_open {
            select::emit_datalist_suggestions(&mut self.out, &mut self.attrs, self.styles, n);
        }
    }

    fn spellcheck(
        &mut self,
        n: Node<'_>,
        is_placeholder: bool,
        disabled: bool,
        val_start: usize,
        disp_end: usize,
    ) {
        if !is_placeholder && disp_end > val_start && !disabled && controls::spellcheck_used(n) {
            self.attrs.push(
                InlineAttr::new(inline_kind::SPELLCHECK, val_start, disp_end - val_start)
                    .with_dom(Some(n)),
            );
        }
    }

    pub(crate) fn button(&mut self, n: Node<'_>) {
        let mut label = serialize::collect_text(Some(n));
        if label.is_empty() {
            let bs = self.styles.get(n);
            let mut generated = Vec::new();
            for ps in [bs.and_then(StyleRef::before), bs.and_then(StyleRef::after)]
                .into_iter()
                .flatten()
            {
                pseudo::append_content(&mut generated, ps.get(PropId::Content), Some(n), Some(ps));
            }
            label = if !generated.is_empty() {
                generated
            } else {
                [c"aria-label", c"title", c"value"]
                    .iter()
                    .find_map(|a| n.attr(a).filter(|v| !v.is_empty()))
                    .map_or_else(Vec::new, |v| v.to_bytes().to_vec())
            };
        }
        if label.is_empty() {
            return;
        }
        let start = self.out.len();
        self.push(NBSP);
        self.push(&label);
        self.push(NBSP);
        self.form_attr(inline_kind::BUTTON, start, n);
    }

    pub(crate) fn select(&mut self, n: Node<'_>) {
        let multi = n.attr(c"multiple").is_some();
        let size_n = n
            .attr(c"size")
            .map_or(0, |s| controls::parse_int(Some(s), 0, 0, 1000));
        if multi || size_n > 1 {
            self.listbox(n, size_n);
            return;
        }
        let label = dom_select::chosen_option(n)
            .map_or_else(Vec::new, |c| serialize::collect_text(Some(c)));
        let start = self.out.len();
        self.push(NBSP);
        self.push(&label);
        let open = same(self.state.open_select, n);
        self.push(
            if open {
                " \u{25B4}\u{a0}"
            } else {
                " \u{25BE}\u{a0}"
            }
            .as_bytes(),
        );
        self.form_attr(inline_kind::INPUT_FIELD, start, n);
        if !open {
            return;
        }
        for c in children(n) {
            match c.element_name() {
                Some(b"option") => self.open_option(c),
                Some(b"optgroup") => {
                    if let Some(gl) = c.attr(c"label").filter(|l| !l.is_empty()) {
                        self.push("\u{2028}\u{a0}".as_bytes());
                        self.push(gl.to_bytes());
                        self.push(NBSP);
                    }
                    for o in children(c) {
                        if o.element_name() == Some(b"option") {
                            self.open_option(o);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn open_option(&mut self, o: Node<'_>) {
        select::emit_option(&mut self.out, &mut self.attrs, self.styles, o, true);
    }

    fn listbox(&mut self, n: Node<'_>, size_n: i32) {
        let mut shown = 0;
        let cap = if size_n > 0 { size_n } else { 6 };
        let mut first = true;
        let mut c = n.first_child();
        while let Some(child) = c.filter(|_| shown < cap) {
            c = child.next_sibling();
            match child.element_name() {
                Some(b"optgroup") => {
                    if let Some(gl) = child.attr(c"label").filter(|l| !l.is_empty()) {
                        if !first {
                            self.push(LINE_SEPARATOR);
                        }
                        self.push(NBSP);
                        self.push(gl.to_bytes());
                        self.push(NBSP);
                        first = false;
                        shown += 1;
                    }
                    let mut opt = child.first_child();
                    while let Some(o) = opt.filter(|_| shown < cap) {
                        opt = o.next_sibling();
                        if o.element_name() != Some(b"option") {
                            continue;
                        }
                        select::emit_option(&mut self.out, &mut self.attrs, self.styles, o, !first);
                        first = false;
                        shown += 1;
                    }
                }
                Some(b"option") => {
                    select::emit_option(&mut self.out, &mut self.attrs, self.styles, child, !first);
                    first = false;
                    shown += 1;
                }
                _ => {}
            }
        }
        if shown == 0 {
            let start = self.out.len();
            self.push("\u{a0}\u{a0}".as_bytes());
            self.form_attr(inline_kind::INPUT_FIELD, start, n);
        }
    }

    pub(crate) fn textarea(&mut self, n: Node<'_>, s: Option<StyleRef<'_>>) {
        let start = self.out.len();
        let focused = same(self.state.focused_input, n);
        self.push(NBSP);
        let val_start = self.out.len();
        let raw = controls::textarea_value(Some(n));
        let raw = &raw[..raw.iter().position(|&c| c == 0).unwrap_or(raw.len())];
        let value_byte_len = raw.len();
        let any = value_byte_len > 0;
        let (mut caret_pos, mut anchor_pos) = self.push_editable(raw);
        let mut disp_end = self.out.len();
        let mut is_placeholder = false;
        let ta_sized = n.attr(c"rows").is_some() || n.attr(c"cols").is_some();
        if !any {
            if let Some(ph) = n.attr(c"placeholder").filter(|p| !p.is_empty()) {
                for &ch in ph.to_bytes() {
                    match ch {
                        b'\n' => self.push(LINE_SEPARATOR),
                        b'\r' => {}
                        ch => self.out.push(ch),
                    }
                }
                disp_end = self.out.len();
                is_placeholder = true;
            } else if ta_sized {
                let row_lines = atoi(n.attr(c"rows"), 2).clamp(1, 1000);
                for r in 0..row_lines {
                    if r > 0 {
                        self.push(LINE_SEPARATOR);
                    }
                    self.push(NBSP);
                }
            } else {
                self.push_repeat(NBSP, 40);
            }
            if focused {
                caret_pos = val_start;
                anchor_pos = val_start;
            } else {
                caret_pos = 0;
            }
        }
        self.push(NBSP);
        let kind = if focused {
            inline_kind::INPUT_FIELD_FOCUSED
        } else {
            inline_kind::INPUT_FIELD
        };
        self.form_attr(kind, start, n);
        let disabled = n.attr(c"disabled").is_some();
        let end = self.out.len();
        emit_control_text_style(
            &mut self.attrs,
            s,
            (start, end),
            (val_start, disp_end),
            is_placeholder,
            disabled,
        );
        self.spellcheck(n, is_placeholder, disabled, val_start, disp_end);
        if focused {
            self.caret_attrs(caret_pos, anchor_pos);
        }
    }
}
