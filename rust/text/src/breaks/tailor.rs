//! Southstar text — script-specific break tailoring for Arabic, the Indic scripts, Thai and Catalan Latin.
//! Copyright 1999 Red Hat Software; 2006 Behdad Esfahbod; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

#[cfg(feature = "thai")]
use crate::ffi::thai;
use crate::script::Script;
use crate::types::LogAttr;
use crate::unicode;

const ALEF_WITH_MADDA_ABOVE: u32 = 0x0622;
const YEH_WITH_HAMZA_ABOVE: u32 = 0x0626;
const ALEF: u32 = 0x0627;
const WAW: u32 = 0x0648;
const YEH: u32 = 0x064A;
const MADDAH_ABOVE: u32 = 0x0653;
const HAMZA_ABOVE: u32 = 0x0654;
const HAMZA_BELOW: u32 = 0x0655;

fn break_arabic(text: &str, attrs: &mut [LogAttr]) {
    let mut prev_wc = 0u32;
    for (i, character) in text.chars().enumerate() {
        let this_wc = u32::from(character);
        if (ALEF_WITH_MADDA_ABOVE..=YEH_WITH_HAMZA_ABOVE).contains(&this_wc)
            || (prev_wc == ALEF && (MADDAH_ABOVE..=HAMZA_BELOW).contains(&this_wc))
            || (this_wc == HAMZA_ABOVE && (prev_wc == WAW || prev_wc == YEH))
        {
            attrs[i + 1].set_backspace_deletes_character(false);
        }
        prev_wc = this_wc;
    }
}

fn is_composite_with_brahmi_nukta(c: u32) -> bool {
    (0x09DC..=0x09DF).contains(&c)
        || (0x0958..=0x095F).contains(&c)
        || c == 0x0931
        || (0x0959..=0x095F).contains(&c)
        || (0x0CC7..=0x0CC8).contains(&c)
        || (0x0CCA..=0x0CCB).contains(&c)
        || matches!(
            c,
            0x0BCA | 0x0BCB | 0x0BCC | 0x0C47 | 0x0C48 | 0x0B48 | 0x0B4B | 0x0B4C
        )
        || (0x0A59..=0x0A5C).contains(&c)
        || matches!(c, 0x0A5E | 0x0A33 | 0x0A36)
}

fn is_split_matra_brahmi(c: u32) -> bool {
    c == 0x09CB || c == 0x09CC || (0x0D4A..=0x0D4C).contains(&c)
}

fn not_cursor_position(attr: &mut LogAttr) {
    if !attr.is_mandatory_break() {
        attr.set_is_cursor_position(false);
        attr.set_is_char_break(false);
        attr.set_is_line_break(false);
        attr.set_is_mandatory_break(false);
    }
}

fn break_indic(text: &str, script: Script, attrs: &mut [LogAttr]) {
    let chars: Vec<u32> = text.chars().map(u32::from).collect();
    let mut prev_wc = 0u32;
    let mut is_conjunct = false;

    for (i, &this_wc) in chars.iter().enumerate() {
        if is_composite_with_brahmi_nukta(this_wc) || is_split_matra_brahmi(this_wc) {
            attrs[i + 1].set_backspace_deletes_character(false);
        }

        let next_wc = chars.get(i + 1).copied().unwrap_or(0);
        let next_next_wc = chars.get(i + 2).copied().unwrap_or(0);

        if script == Script::SINHALA {
            if (this_wc == 0x0DCA && next_wc == 0x200D) || (this_wc == 0x200D && next_wc == 0x0DCA)
            {
                not_cursor_position(&mut attrs[i]);
                not_cursor_position(&mut attrs[i + 1]);
                is_conjunct = true;
            } else if is_conjunct
                && (prev_wc == 0x200D || prev_wc == 0x0DCA)
                && (0x0D9A..=0x0DC6).contains(&this_wc)
            {
                not_cursor_position(&mut attrs[i]);
                is_conjunct = false;
            } else if !is_conjunct && prev_wc == 0x0DCA && this_wc != 0x200D {
                attrs[i].set_is_cursor_position(true);
            }
        } else if prev_wc != 0 && (this_wc == 0x200D || this_wc == 0x200C) {
            not_cursor_position(&mut attrs[i]);
            if next_wc != 0 {
                not_cursor_position(&mut attrs[i + 1]);
                if next_next_wc != 0
                    && matches!(
                        next_wc,
                        0x09CD
                            | 0x0ACD
                            | 0x094D
                            | 0x0CCD
                            | 0x0D4D
                            | 0x0B4D
                            | 0x0A4D
                            | 0x0BCD
                            | 0x0C4D
                    )
                {
                    not_cursor_position(&mut attrs[i + 2]);
                }
            }
        }

        prev_wc = this_wc;
    }
}

#[cfg(not(feature = "thai"))]
fn break_thai(_text: &str, _attrs: &mut [LogAttr]) {}

#[cfg(feature = "thai")]
fn break_thai(text: &str, attrs: &mut [LogAttr]) {
    let mut tis_text: Vec<u8> = text
        .chars()
        .map(|ch| thai::uni2tis(u32::from(ch)))
        .collect();
    tis_text.push(0);

    let mut break_points = vec![0; tis_text.len()];
    let found = thai::find_breaks(&tis_text, &mut break_points);

    for &point in &break_points[..found] {
        let Ok(index) = usize::try_from(point) else {
            continue;
        };
        let attr = &mut attrs[index];
        if !attr.is_line_break() {
            attr.set_is_char_break(true);
            attr.set_is_line_break(true);
        }
        if !(attr.is_word_start() || attr.is_word_end()) {
            attr.set_is_word_start(true);
            attr.set_is_word_end(true);
        }
    }
}

fn break_latin(
    paragraph: &str,
    start: usize,
    end: usize,
    language: Option<&str>,
    attrs: &mut [LogAttr],
) {
    let Some(language) = language else {
        return;
    };
    if !language
        .as_bytes()
        .get(..3)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"ca-"))
    {
        return;
    }

    let mut prev_wc = 0u32;
    for (i, (offset, character)) in paragraph[start..end].char_indices().enumerate() {
        let wc = u32::from(character);
        if wc == 0x00b7 {
            let middle_next = unicode::char_at(paragraph, start + offset + character.len_utf8());
            if unicode::to_lower(middle_next) == u32::from(b'l')
                && unicode::to_lower(prev_wc) == u32::from(b'l')
            {
                attrs[i].set_is_word_end(false);
                attrs[i + 1].set_is_word_start(false);
            }
        }
        prev_wc = wc;
    }
}

pub(super) fn break_script(
    paragraph: &str,
    start: usize,
    end: usize,
    script: Script,
    language: Option<&str>,
    attrs: &mut [LogAttr],
) -> bool {
    let text = &paragraph[start..end];
    match script {
        Script::ARABIC => break_arabic(text, attrs),
        Script::DEVANAGARI
        | Script::BENGALI
        | Script::GURMUKHI
        | Script::GUJARATI
        | Script::ORIYA
        | Script::TAMIL
        | Script::TELUGU
        | Script::KANNADA
        | Script::MALAYALAM
        | Script::SINHALA => break_indic(text, script, attrs),
        Script::THAI => break_thai(text, attrs),
        Script::LATIN => break_latin(paragraph, start, end, language, attrs),
        _ => return false,
    }
    true
}
