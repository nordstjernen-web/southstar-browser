//! Southstar bidi — one paragraph's embedding levels: rules P2–P3, X1–X10, W1–W7, N0–N2, I1–I2 and L1 of UAX #9.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::{BidiClass, Bracket};

const MAX_DEPTH: u8 = 125;
const MAX_BRACKET_PAIRS: usize = 63;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParagraphDirection {
    Ltr,
    Rtl,
    WeakLtr,
    WeakRtl,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Resolved {
    pub base_level: u8,
    pub max_level: u8,
}

#[derive(Clone, Copy)]
struct StatusEntry {
    level: u8,
    override_class: Option<BidiClass>,
    isolate: bool,
}

struct Paragraph<'a> {
    original: &'a [BidiClass],
    brackets: &'a [Option<Bracket>],
    classes: Vec<BidiClass>,
    matching_pdi: Vec<Option<usize>>,
    base_level: u8,
}

pub fn resolve_paragraph(
    original: &[BidiClass],
    brackets: &[Option<Bracket>],
    direction: ParagraphDirection,
    levels: &mut [u8],
) -> Resolved {
    let len = original.len().min(levels.len());
    let original = &original[..len];
    let levels = &mut levels[..len];
    let matching_pdi = match_isolates(original);
    let base_level = match direction {
        ParagraphDirection::Ltr => 0,
        ParagraphDirection::Rtl => 1,
        ParagraphDirection::WeakLtr | ParagraphDirection::WeakRtl => {
            match first_strong(original, &matching_pdi, 0, len) {
                Some(BidiClass::L) => 0,
                Some(_) => 1,
                None => u8::from(direction == ParagraphDirection::WeakRtl),
            }
        }
    };
    let mut paragraph = Paragraph {
        original,
        brackets,
        classes: original.to_vec(),
        matching_pdi,
        base_level,
    };
    paragraph.explicit_levels(levels);
    let explicit = levels.to_vec();
    for sequence in paragraph.isolating_run_sequences(&explicit) {
        paragraph.resolve_sequence(&sequence, &explicit, levels);
    }
    let max_level = levels
        .iter()
        .zip(original)
        .filter(|(_, class)| !class.is_removed_by_x9())
        .map(|(&level, _)| level)
        .max()
        .unwrap_or(base_level)
        .max(base_level);
    paragraph.level_removed(levels);
    paragraph.reset_whitespace(levels);
    Resolved {
        base_level,
        max_level,
    }
}

fn match_isolates(original: &[BidiClass]) -> Vec<Option<usize>> {
    let mut matching = vec![None; original.len()];
    let mut open = Vec::new();
    for (i, class) in original.iter().enumerate() {
        match class {
            BidiClass::LRI | BidiClass::RLI | BidiClass::FSI => open.push(i),
            BidiClass::PDI => {
                if let Some(initiator) = open.pop() {
                    matching[initiator] = Some(i);
                }
            }
            BidiClass::B => open.clear(),
            _ => {}
        }
    }
    matching
}

fn first_strong(
    original: &[BidiClass],
    matching_pdi: &[Option<usize>],
    start: usize,
    end: usize,
) -> Option<BidiClass> {
    let mut i = start;
    while i < end {
        match original[i] {
            BidiClass::L => return Some(BidiClass::L),
            BidiClass::R | BidiClass::AL => return Some(BidiClass::R),
            BidiClass::B => return None,
            BidiClass::LRI | BidiClass::RLI | BidiClass::FSI => match matching_pdi[i] {
                Some(pdi) => i = pdi,
                None => return None,
            },
            _ => {}
        }
        i += 1;
    }
    None
}

fn next_level(level: u8, rtl: bool) -> u8 {
    if rtl {
        (level + 1) | 1
    } else {
        (level + 2) & !1
    }
}

fn strong_direction(class: BidiClass) -> Option<BidiClass> {
    match class {
        BidiClass::L => Some(BidiClass::L),
        BidiClass::R | BidiClass::AL | BidiClass::EN | BidiClass::AN => Some(BidiClass::R),
        _ => None,
    }
}

fn is_neutral_or_isolate(class: BidiClass) -> bool {
    matches!(
        class,
        BidiClass::B | BidiClass::S | BidiClass::WS | BidiClass::ON
    ) || class.is_isolate_control()
}

fn direction_of_level(level: u8) -> BidiClass {
    if level & 1 == 1 {
        BidiClass::R
    } else {
        BidiClass::L
    }
}

impl Paragraph<'_> {
    fn explicit_levels(&mut self, levels: &mut [u8]) {
        let initial = StatusEntry {
            level: self.base_level,
            override_class: None,
            isolate: false,
        };
        let mut stack = vec![initial];
        let mut overflow_isolates = 0usize;
        let mut overflow_embeddings = 0usize;
        let mut valid_isolates = 0usize;
        let original = self.original;
        for (i, &class) in original.iter().enumerate() {
            let last = stack.last().copied().unwrap_or(initial);
            levels[i] = last.level;
            match class {
                BidiClass::RLE | BidiClass::LRE | BidiClass::RLO | BidiClass::LRO => {
                    let rtl = matches!(class, BidiClass::RLE | BidiClass::RLO);
                    let level = next_level(last.level, rtl);
                    if level <= MAX_DEPTH && overflow_isolates == 0 && overflow_embeddings == 0 {
                        let override_class = match class {
                            BidiClass::RLO => Some(BidiClass::R),
                            BidiClass::LRO => Some(BidiClass::L),
                            _ => None,
                        };
                        stack.push(StatusEntry {
                            level,
                            override_class,
                            isolate: false,
                        });
                    } else if overflow_isolates == 0 {
                        overflow_embeddings += 1;
                    }
                }
                BidiClass::RLI | BidiClass::LRI | BidiClass::FSI => {
                    if let Some(override_class) = last.override_class {
                        self.classes[i] = override_class;
                    }
                    let rtl = match class {
                        BidiClass::RLI => true,
                        BidiClass::LRI => false,
                        _ => {
                            let end = self.matching_pdi[i].unwrap_or(self.original.len());
                            first_strong(self.original, &self.matching_pdi, i + 1, end)
                                == Some(BidiClass::R)
                        }
                    };
                    let level = next_level(last.level, rtl);
                    if level <= MAX_DEPTH && overflow_isolates == 0 && overflow_embeddings == 0 {
                        valid_isolates += 1;
                        stack.push(StatusEntry {
                            level,
                            override_class: None,
                            isolate: true,
                        });
                    } else {
                        overflow_isolates += 1;
                    }
                }
                BidiClass::PDI => {
                    if overflow_isolates > 0 {
                        overflow_isolates -= 1;
                    } else if valid_isolates > 0 {
                        overflow_embeddings = 0;
                        while stack.last().is_some_and(|entry| !entry.isolate) {
                            stack.pop();
                        }
                        stack.pop();
                        valid_isolates -= 1;
                    }
                    let last = stack.last().copied().unwrap_or(initial);
                    levels[i] = last.level;
                    if let Some(override_class) = last.override_class {
                        self.classes[i] = override_class;
                    }
                }
                BidiClass::PDF if overflow_isolates == 0 => {
                    if overflow_embeddings > 0 {
                        overflow_embeddings -= 1;
                    } else if !last.isolate && stack.len() >= 2 {
                        stack.pop();
                    }
                }
                BidiClass::B => {
                    levels[i] = self.base_level;
                    stack.truncate(1);
                    overflow_isolates = 0;
                    overflow_embeddings = 0;
                    valid_isolates = 0;
                }
                BidiClass::PDF | BidiClass::BN => {}
                _ => {
                    if let Some(override_class) = last.override_class {
                        self.classes[i] = override_class;
                    }
                }
            }
        }
    }

    fn isolating_run_sequences(&self, levels: &[u8]) -> Vec<Vec<usize>> {
        let mut runs: Vec<Vec<usize>> = Vec::new();
        let mut previous: Option<usize> = None;
        for (i, class) in self.original.iter().enumerate() {
            if class.is_removed_by_x9() {
                continue;
            }
            match (runs.last_mut(), previous) {
                (Some(run), Some(p))
                    if levels[p] == levels[i] && !self.original[p].is_isolate_initiator() =>
                {
                    run.push(i);
                }
                _ => runs.push(vec![i]),
            }
            previous = Some(i);
        }
        let mut assigned = vec![false; runs.len()];
        let mut sequences = Vec::new();
        for start in 0..runs.len() {
            if assigned[start] {
                continue;
            }
            assigned[start] = true;
            let level = levels[runs[start][0]];
            let mut sequence = runs[start].clone();
            let mut current = start;
            while runs[current]
                .last()
                .is_some_and(|&last| self.original[last].is_isolate_initiator())
            {
                let Some(next) = (current + 1..runs.len()).find(|&j| {
                    !assigned[j]
                        && levels[runs[j][0]] == level
                        && self.classes[runs[j][0]] == BidiClass::PDI
                }) else {
                    break;
                };
                assigned[next] = true;
                sequence.extend_from_slice(&runs[next]);
                current = next;
            }
            sequences.push(sequence);
        }
        sequences
    }

    fn neighbour_level(&self, levels: &[u8], from: usize, forward: bool) -> u8 {
        let mut i = from;
        loop {
            if forward {
                i += 1;
                if i >= self.original.len() {
                    return self.base_level;
                }
            } else {
                if i == 0 {
                    return self.base_level;
                }
                i -= 1;
            }
            if !self.original[i].is_removed_by_x9() {
                return levels[i];
            }
        }
    }

    fn resolve_sequence(&mut self, sequence: &[usize], explicit: &[u8], levels: &mut [u8]) {
        let (Some(&first), Some(&last)) = (sequence.first(), sequence.last()) else {
            return;
        };
        let level = explicit[first];
        let sos = direction_of_level(level.max(self.neighbour_level(explicit, first, false)));
        let eos_neighbour = if self.original[last].is_isolate_initiator() {
            self.base_level
        } else {
            self.neighbour_level(explicit, last, true)
        };
        let eos = direction_of_level(level.max(eos_neighbour));
        let mut types: Vec<BidiClass> = sequence.iter().map(|&i| self.classes[i]).collect();
        resolve_weak(&mut types, sos, eos);
        self.resolve_brackets(sequence, &mut types, level, sos);
        resolve_neutral(&mut types, sos, eos, direction_of_level(level));
        for (&i, &class) in sequence.iter().zip(&types) {
            self.classes[i] = class;
            levels[i] = implicit_level(level, class);
        }
    }

    fn resolve_brackets(
        &self,
        sequence: &[usize],
        types: &mut [BidiClass],
        level: u8,
        sos: BidiClass,
    ) {
        let mut stack: Vec<(u32, usize)> = Vec::new();
        let mut pairs = Vec::new();
        for (k, &i) in sequence.iter().enumerate() {
            if types[k] != BidiClass::ON {
                continue;
            }
            let Some(bracket) = self.brackets.get(i).copied().flatten() else {
                continue;
            };
            if bracket.opening {
                if stack.len() == MAX_BRACKET_PAIRS {
                    break;
                }
                stack.push((bracket.pair, k));
            } else if let Some(position) = stack.iter().rposition(|&(pair, _)| pair == bracket.pair)
            {
                pairs.push((stack[position].1, k));
                stack.truncate(position);
            }
        }
        if pairs.is_empty() {
            return;
        }
        pairs.sort_unstable();
        let embedding = direction_of_level(level);
        for (open, close) in pairs {
            let mut found_embedding = false;
            let mut found_opposite = false;
            for &class in &types[open + 1..close] {
                match strong_direction(class) {
                    Some(direction) if direction == embedding => {
                        found_embedding = true;
                        break;
                    }
                    Some(_) => found_opposite = true,
                    None => {}
                }
            }
            let resolved = if found_embedding {
                embedding
            } else if found_opposite {
                let context = types[..open]
                    .iter()
                    .rev()
                    .find_map(|&class| strong_direction(class))
                    .unwrap_or(sos);
                if context == embedding {
                    embedding
                } else {
                    context
                }
            } else {
                continue;
            };
            for bracket in [open, close] {
                types[bracket] = resolved;
                for k in bracket + 1..sequence.len() {
                    if self.original[sequence[k]] != BidiClass::NSM {
                        break;
                    }
                    types[k] = resolved;
                }
            }
        }
    }

    fn level_removed(&self, levels: &mut [u8]) {
        for i in 0..self.original.len() {
            if self.original[i].is_removed_by_x9() {
                levels[i] = if i == 0 {
                    self.base_level
                } else {
                    levels[i - 1]
                };
            }
        }
    }

    fn reset_whitespace(&self, levels: &mut [u8]) {
        let mut trailing = true;
        for i in (0..self.original.len()).rev() {
            let class = self.original[i];
            if matches!(class, BidiClass::B | BidiClass::S) {
                levels[i] = self.base_level;
                trailing = true;
            } else if trailing
                && (class == BidiClass::WS
                    || class.is_isolate_control()
                    || class.is_removed_by_x9())
            {
                levels[i] = self.base_level;
            } else {
                trailing = false;
            }
        }
    }
}

fn resolve_weak(types: &mut [BidiClass], sos: BidiClass, eos: BidiClass) {
    let len = types.len();
    for k in 0..len {
        if types[k] == BidiClass::NSM {
            types[k] = match k.checked_sub(1).map(|p| types[p]) {
                None => sos,
                Some(previous) if previous.is_isolate_control() => BidiClass::ON,
                Some(previous) => previous,
            };
        }
    }
    let mut last_strong = sos;
    for class in types.iter_mut() {
        match *class {
            BidiClass::L | BidiClass::R | BidiClass::AL => last_strong = *class,
            BidiClass::EN if last_strong == BidiClass::AL => *class = BidiClass::AN,
            _ => {}
        }
    }
    for class in types.iter_mut() {
        if *class == BidiClass::AL {
            *class = BidiClass::R;
        }
    }
    for k in 1..len.saturating_sub(1) {
        let (previous, next) = (types[k - 1], types[k + 1]);
        match types[k] {
            BidiClass::ES if previous == BidiClass::EN && next == BidiClass::EN => {
                types[k] = BidiClass::EN;
            }
            BidiClass::CS if previous == next && previous.is_number() => types[k] = previous,
            _ => {}
        }
    }
    let mut k = 0;
    while k < len {
        if types[k] != BidiClass::ET {
            k += 1;
            continue;
        }
        let end = types[k..]
            .iter()
            .position(|&class| class != BidiClass::ET)
            .map_or(len, |offset| k + offset);
        let before = k.checked_sub(1).map_or(sos, |p| types[p]);
        let after = if end < len { types[end] } else { eos };
        if before == BidiClass::EN || after == BidiClass::EN {
            types[k..end].fill(BidiClass::EN);
        }
        k = end;
    }
    for class in types.iter_mut() {
        if matches!(*class, BidiClass::ES | BidiClass::ET | BidiClass::CS) {
            *class = BidiClass::ON;
        }
    }
    let mut last_strong = sos;
    for class in types.iter_mut() {
        match *class {
            BidiClass::L | BidiClass::R => last_strong = *class,
            BidiClass::EN if last_strong == BidiClass::L => *class = BidiClass::L,
            _ => {}
        }
    }
}

fn resolve_neutral(types: &mut [BidiClass], sos: BidiClass, eos: BidiClass, embedding: BidiClass) {
    let len = types.len();
    let mut k = 0;
    while k < len {
        if !is_neutral_or_isolate(types[k]) {
            k += 1;
            continue;
        }
        let end = types[k..]
            .iter()
            .position(|&class| !is_neutral_or_isolate(class))
            .map_or(len, |offset| k + offset);
        let before = k
            .checked_sub(1)
            .and_then(|p| strong_direction(types[p]))
            .unwrap_or(sos);
        let after = if end < len {
            strong_direction(types[end]).unwrap_or(eos)
        } else {
            eos
        };
        let resolved = if before == after { before } else { embedding };
        types[k..end].fill(resolved);
        k = end;
    }
}

fn implicit_level(level: u8, class: BidiClass) -> u8 {
    if level & 1 == 0 {
        match class {
            BidiClass::R => level + 1,
            BidiClass::AN | BidiClass::EN => level + 2,
            _ => level,
        }
    } else {
        match class {
            BidiClass::L | BidiClass::EN | BidiClass::AN => level + 1,
            _ => level,
        }
    }
}
