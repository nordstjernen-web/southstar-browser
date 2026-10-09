//! Southstar — cascade layer order: the layers a set of style sheets declares, and every dotted prefix of them, ranked so sibling layers keep the order they were first declared in and a layer's sublayers come before the layer itself.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cmp::Ordering;
use std::collections::HashMap;

fn prefixes(name: &[u8]) -> impl Iterator<Item = &[u8]> {
    name.iter()
        .enumerate()
        .filter(|&(_, &c)| c == b'.')
        .map(|(i, _)| &name[..i])
        .chain(core::iter::once(name))
}

fn segment_end(name: &[u8], from: usize) -> Option<usize> {
    name[from..]
        .iter()
        .position(|&c| c == b'.')
        .map(|i| from + i)
}

fn layer_order(first: &HashMap<&[u8], usize>, a: &[u8], b: &[u8]) -> Ordering {
    let first_index = |prefix: &[u8]| first.get(prefix).copied().unwrap_or(usize::MAX);
    let (mut ap, mut bp) = (0, 0);
    loop {
        let ad = segment_end(a, ap);
        let bd = segment_end(b, bp);
        let ae = ad.unwrap_or(a.len());
        let be = bd.unwrap_or(b.len());
        if a[ap..ae] != b[bp..be] {
            return first_index(&a[..ae])
                .cmp(&first_index(&b[..be]))
                .then_with(|| a.cmp(b));
        }
        match (ad, bd) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Greater,
            (Some(_), None) => return Ordering::Less,
            (Some(ad), Some(bd)) => {
                ap = ad + 1;
                bp = bd + 1;
            }
        }
    }
}

pub(crate) fn ranks<'a>(declared: impl Iterator<Item = &'a [u8]>) -> Vec<(&'a [u8], usize)> {
    let mut seen: HashMap<&[u8], usize> = HashMap::new();
    for name in declared {
        let next = seen.len();
        seen.entry(name).or_insert(next);
    }
    let mut first: HashMap<&[u8], usize> = HashMap::new();
    for (&name, &rank) in &seen {
        for prefix in prefixes(name) {
            first
                .entry(prefix)
                .and_modify(|r| *r = (*r).min(rank))
                .or_insert(rank);
        }
    }
    let mut names: Vec<&[u8]> = first.keys().copied().collect();
    names.sort_by(|a, b| layer_order(&first, a, b));
    names.into_iter().enumerate().map(|(i, n)| (n, i)).collect()
}
