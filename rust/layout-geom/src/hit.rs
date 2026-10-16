//! Southstar — whether a style opts a box out of hit testing and whether a node is a form control a hit lands on.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::Node;
use southstar_style::{PropId, StyleRef};

pub fn node_is_form_hit_target(n: Option<Node<'_>>) -> bool {
    n.filter(|n| n.is_element())
        .and_then(Node::name)
        .is_some_and(|name| {
            matches!(
                name.to_bytes(),
                b"button" | b"input" | b"select" | b"textarea"
            )
        })
}

pub fn style_blocks_hit_testing(s: Option<StyleRef<'_>>) -> bool {
    let Some(s) = s else {
        return false;
    };
    s.get(PropId::PointerEvents)
        .is_some_and(|v| v.is_keyword(c"none"))
        || s.keyword_of(PropId::Visibility)
            .is_some_and(|v| v == c"hidden" || v == c"collapse")
}
