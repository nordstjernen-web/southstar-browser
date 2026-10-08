//! Southstar — the --inspect and --inspect-at reports: matched elements, their boxes, edges, key computed values and ancestry.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_dom::{Node, children};
use southstar_layout::BoxRef;

use crate::ffi::{self, Selectors, StyleTable, Value, fmt_g};
use crate::nonempty;
use crate::text::truncated;

const LABEL_CAP: usize = 320;
const VALUE_CAP: usize = 160;
const GROW_CAP: usize = 32;
const MAX_DEPTH: u32 = 512;

const BOX_PROPS: [&CStr; 20] = [
    c"display",
    c"position",
    c"box-sizing",
    c"width",
    c"height",
    c"min-width",
    c"max-width",
    c"min-height",
    c"max-height",
    c"flex-direction",
    c"justify-content",
    c"align-items",
    c"align-self",
    c"flex-grow",
    c"flex-shrink",
    c"flex-basis",
    c"gap",
    c"font-size",
    c"color",
    c"background-color",
];

const UNRENDERED_PROPS: [&CStr; 4] = [c"display", c"visibility", c"position", c"content"];

pub fn report(
    layout: Option<BoxRef>,
    doc: Option<Node>,
    styles: StyleTable,
    selector: Option<&CStr>,
    at: Option<&CStr>,
) -> Option<Vec<u8>> {
    let (selector, at) = (nonempty(selector), nonempty(at));
    if selector.is_none() && at.is_none() {
        return None;
    }
    let mut out = Vec::new();
    if let Some(selector) = selector {
        inspect(layout, doc, styles, selector, &mut out);
    }
    if let Some(at) = at {
        match ffi::scan_point(at.to_bytes()) {
            Some((x, y)) => inspect_at(layout, x, y, &mut out),
            None => out.extend_from_slice(
                &[
                    b"inspect-at: bad coordinate '",
                    at.to_bytes(),
                    b"' (expected X,Y)\n",
                ]
                .concat(),
            ),
        }
    }
    Some(out)
}

fn unit_name(unit: u32) -> &'static [u8] {
    match unit {
        0 => b"px",
        1 => b"em",
        2 => b"rem",
        3 => b"%",
        5 => b"vw",
        6 => b"vh",
        7 => b"vmin",
        8 => b"vmax",
        _ => b"",
    }
}

fn value_str(v: Value, cap: usize) -> Vec<u8> {
    let s = match v {
        Value::Keyword(k) => k.map_or(b"?".to_vec(), |k| k.to_bytes().to_vec()),
        Value::Length(v, unit) => [fmt_g(v).as_bytes(), unit_name(unit)].concat(),
        Value::Calc(pct, px) => format!("calc({}% + {}px)", fmt_g(pct), fmt_g(px)).into_bytes(),
        Value::Color([r, g, b, a]) => format!("rgba({r}, {g}, {b}, {a})").into_bytes(),
        Value::Url(u) => [&b"url("[..], u.map_or(&b""[..], CStr::to_bytes), b")"].concat(),
        Value::Other => b"(set)".to_vec(),
    };
    truncated(s, cap)
}

fn node_label(n: Option<Node>) -> Vec<u8> {
    let element = n
        .filter(|n| n.is_element())
        .and_then(|n| n.name().map(|name| (n, name)));
    let Some((n, name)) = element else {
        let fallback: &[u8] = if n.is_some_and(Node::is_text) {
            b"#text"
        } else {
            b"(anonymous)"
        };
        return fallback.to_vec();
    };
    let mut s = name.to_bytes().to_vec();
    if let Some(id) = n.attr(c"id").filter(|id| !id.is_empty()) {
        s.push(b'#');
        s.extend_from_slice(id.to_bytes());
    }
    if let Some(class) = n.attr(c"class").filter(|c| !c.is_empty()) {
        for part in class.to_bytes().split(|b| b" \t\r\n".contains(b)) {
            if !part.is_empty() {
                s.push(b'.');
                s.extend_from_slice(part);
            }
        }
    }
    truncated(s, LABEL_CAP)
}

fn box_label(b: BoxRef) -> Vec<u8> {
    node_label(ffi::box_dom(b))
}

fn find_box<'a>(root: Option<BoxRef<'a>>, dom: Node) -> Option<BoxRef<'a>> {
    let root = root?;
    if ffi::box_dom(root).is_some_and(|d| d.as_ptr() == dom.as_ptr()) {
        return Some(root);
    }
    southstar_layout::children(root).find_map(|c| find_box(Some(c), dom))
}

fn prop_line(out: &mut Vec<u8>, style: ffi::StyleRef, name: &CStr) {
    let Some(v) = style.value(name) else {
        return;
    };
    let label = String::from_utf8_lossy(name.to_bytes());
    out.extend_from_slice(format!("    {label:<15} ").as_bytes());
    out.extend_from_slice(&value_str(v, VALUE_CAP));
    out.push(b'\n');
}

fn edges_line(out: &mut Vec<u8>, title: &str, e: southstar_layout::Edges) {
    out.extend_from_slice(
        format!(
            "    {title:<15} T{} R{} B{} L{}\n",
            fmt_g(e.top),
            fmt_g(e.right),
            fmt_g(e.bottom),
            fmt_g(e.left)
        )
        .as_bytes(),
    );
}

fn ancestry(b: BoxRef<'_>) -> Vec<BoxRef<'_>> {
    core::iter::successors(Some(b), |p| p.parent()).collect()
}

fn print_box(b: BoxRef, out: &mut Vec<u8>) {
    out.extend_from_slice(&[b"  <", box_label(b).as_slice(), b">\n"].concat());
    let (m, p, br) = (b.margin(), b.padding(), b.border());
    let bx = b.x() + m.left;
    let by = b.y() + m.top;
    let bw = b.content_width() + p.left + p.right + br.left + br.right;
    let bh = b.content_height() + p.top + p.bottom + br.top + br.bottom;
    out.extend_from_slice(
        format!(
            "    content         {} x {}  (at {},{})\n",
            fmt_g(b.content_width()),
            fmt_g(b.content_height()),
            fmt_g(b.x()),
            fmt_g(b.y())
        )
        .as_bytes(),
    );
    out.extend_from_slice(
        format!(
            "    border-box      {} x {}  (at {},{})\n",
            fmt_g(bw),
            fmt_g(bh),
            fmt_g(bx),
            fmt_g(by)
        )
        .as_bytes(),
    );
    edges_line(out, "margin", m);
    edges_line(out, "border", br);
    edges_line(out, "padding", p);

    if let Some(style) = ffi::box_style(b) {
        for name in BOX_PROPS {
            prop_line(out, style, name);
        }
    }

    if b.first_child().is_some() {
        out.extend_from_slice(b"    children:\n");
        for (idx, c) in southstar_layout::children(b).enumerate() {
            let grow = ffi::box_style(c)
                .and_then(|s| s.value(c"flex-grow"))
                .map_or(Vec::new(), |v| {
                    let mut grow = b"  grow=".to_vec();
                    grow.extend_from_slice(&value_str(v, GROW_CAP));
                    truncated(grow, GROW_CAP)
                });
            out.extend_from_slice(
                format!(
                    "      [{idx}] at {},{}  {} x {}  <",
                    fmt_g(c.x()),
                    fmt_g(c.y()),
                    fmt_g(c.content_width()),
                    fmt_g(c.content_height())
                )
                .as_bytes(),
            );
            out.extend_from_slice(&box_label(c));
            out.push(b'>');
            out.extend_from_slice(&grow);
            out.push(b'\n');
        }
    }

    out.extend_from_slice(b"    path            ");
    let chain = ancestry(b);
    for (i, p) in chain.iter().rev().enumerate() {
        out.extend_from_slice(&box_label(*p));
        if i + 1 < chain.len() {
            out.extend_from_slice(b" > ");
        }
    }
    out.push(b'\n');
}

fn collect_matches<'a>(n: Option<Node<'a>>, sels: &Selectors, out: &mut Vec<Node<'a>>, depth: u32) {
    let Some(n) = n.filter(|_| depth < MAX_DEPTH) else {
        return;
    };
    if n.is_element() && n.name().is_some() && sels.any_matches(n) {
        out.push(n);
    }
    for c in children(n) {
        collect_matches(Some(c), sels, out, depth + 1);
    }
}

fn inspect(
    layout: Option<BoxRef>,
    doc: Option<Node>,
    styles: StyleTable,
    selector: &CStr,
    out: &mut Vec<u8>,
) {
    let Some(sels) = Selectors::parse(selector) else {
        out.extend_from_slice(
            &[
                b"inspect: could not parse selector '",
                selector.to_bytes(),
                b"'\n",
            ]
            .concat(),
        );
        return;
    };
    let mut matches = Vec::new();
    collect_matches(doc, &sels, &mut matches, 0);
    out.extend_from_slice(b"inspect: '");
    out.extend_from_slice(selector.to_bytes());
    out.extend_from_slice(format!("' matched {} element(s)\n", matches.len()).as_bytes());
    for (i, el) in matches.iter().enumerate() {
        out.extend_from_slice(format!("\n--- match {} ---\n", i + 1).as_bytes());
        if let Some(b) = find_box(layout, *el) {
            print_box(b, out);
            continue;
        }
        out.extend_from_slice(&[b"  <", node_label(Some(*el)).as_slice(), b">\n"].concat());
        out.extend_from_slice(b"    (not in layout: display:none or non-rendered)\n");
        match styles.get(*el) {
            Some(style) => {
                for name in UNRENDERED_PROPS {
                    prop_line(out, style, name);
                }
            }
            None => out.extend_from_slice(b"    (no style-table entry)\n"),
        }
    }
}

fn inspect_at(layout: Option<BoxRef>, x: f64, y: f64, out: &mut Vec<u8>) {
    let hit = ffi::hit_test(layout, x, y);
    out.extend_from_slice(format!("inspect-at: {},{}\n", fmt_g(x), fmt_g(y)).as_bytes());
    let Some(hit) = hit.filter(|h| ffi::box_dom(*h).is_some()) else {
        out.extend_from_slice(b"  (no element at point)\n");
        return;
    };
    out.extend_from_slice(b"  stack (innermost first):\n");
    for (i, p) in ancestry(hit).into_iter().enumerate() {
        out.extend_from_slice(format!("    {:width$}<", "", width = i * 2).as_bytes());
        out.extend_from_slice(&box_label(p));
        out.extend_from_slice(
            format!(
                ">  at {},{}  {} x {}\n",
                fmt_g(p.x()),
                fmt_g(p.y()),
                fmt_g(p.content_width()),
                fmt_g(p.content_height())
            )
            .as_bytes(),
        );
    }
    out.push(b'\n');
    print_box(hit, out);
}
