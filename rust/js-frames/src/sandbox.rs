//! Southstar — iframe sandbox flags: the sandbox attribute's tokens and the flags in force for a node, intersected over every sandboxed frame above it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::{Node, ancestors_and_self};

pub(crate) const ACTIVE: u32 = 1 << 0;
pub(crate) const ALLOW_SCRIPTS: u32 = 1 << 1;
pub(crate) const ALLOW_FORMS: u32 = 1 << 2;
pub(crate) const ALLOW_SAME_ORIGIN: u32 = 1 << 3;

const TOKENS: &[(&[u8], u32)] = &[
    (b"allow-scripts", ALLOW_SCRIPTS),
    (b"allow-forms", ALLOW_FORMS),
    (b"allow-same-origin", ALLOW_SAME_ORIGIN),
    (b"allow-popups", 1 << 4),
    (b"allow-modals", 1 << 5),
    (b"allow-top-navigation", 1 << 6),
    (b"allow-top-navigation-by-user-activation", 1 << 7),
    (b"allow-downloads", 1 << 8),
    (b"allow-popups-to-escape-sandbox", 1 << 9),
    (b"allow-pointer-lock", 1 << 10),
    (b"allow-presentation", 1 << 11),
    (b"allow-orientation-lock", 1 << 12),
    (b"allow-storage-access-by-user-activation", 1 << 14),
];

fn parse_tokens(value: &[u8]) -> u32 {
    value
        .split(|byte| byte.is_ascii_whitespace() || *byte == 0x0b)
        .filter(|token| !token.is_empty())
        .filter_map(|token| {
            TOKENS
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(token))
                .map(|(_, flag)| *flag)
        })
        .fold(ACTIVE, |flags, flag| flags | flag)
}

pub(crate) fn effective(node: Node<'_>) -> u32 {
    let mut flags: Option<u32> = None;
    for frame in ancestors_and_self(node) {
        if frame.element_name() != Some(b"iframe") {
            continue;
        }
        let Some(value) = frame.attr(c"sandbox") else {
            continue;
        };
        let parsed = parse_tokens(value.to_bytes());
        flags = Some(flags.map_or(parsed, |flags| flags & parsed));
    }
    flags.map_or(0, |flags| flags | ACTIVE)
}

pub(crate) fn origin_is_opaque(node: Node<'_>) -> bool {
    let flags = effective(node);
    flags & ACTIVE != 0 && flags & ALLOW_SAME_ORIGIN == 0
}

pub(crate) fn blocks_forms(node: Node<'_>) -> bool {
    let flags = effective(node);
    flags & ACTIVE != 0 && flags & ALLOW_FORMS == 0
}
