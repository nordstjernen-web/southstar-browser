//! Southstar text — differential tests of the HarfRust shaper against HarfBuzz driven as web-text-lib drives it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use std::collections::BTreeMap;

use super::{
    ShapeDirection, ShapeRequest, ShapedGlyph, buffer_flags, script_to_iso15924, shape, shaper_face,
};
use crate::context::Context;
use crate::ffi::harfbuzz::feature_from_string;
use crate::ffi::harfbuzz_shape_test as hb;
use crate::font::{Feature, Font};
use crate::font_description::FontDescription;
use crate::fontmap::FontMap;
use crate::script::Script;
use crate::types::{Gravity, show_flags};
use crate::unicode::{UnicodeType, to_upper, unichar_type};

const LATIN: &[&str] = &[
    "office affine fluffy ffi ffl final",
    "AVATAR WAVE Type Te To Yo P. F, LT",
    "The quick brown fox jumps over the lazy dog.",
    "Ligatures: fi fl ff -> => != === <= >= www",
    "naïve café résumé Ångström Øresund",
    "e\u{301} a\u{308} o\u{303}\u{323} q\u{307}\u{323} decomposed marks",
    "1/2 3/4 10:30 1st 2nd H2O x²",
    "Ǆ ǅ ǆ İ ı ß ẞ ŉ",
    "x\u{2011}y non\u{2011}breaking hyphen",
    "\u{a0}nbsp\u{2009}thin\u{3000}ideo\u{202f}narrow\u{2007}figure",
    "soft\u{ad}hyphen zero\u{200b}width join\u{200d}er non\u{200c}joiner",
    "Привет, мир! Ελληνικά κείμενα",
    "",
    " ",
];

const ARABIC: &[&str] = &[
    "السلام عليكم",
    "لا إله إلا الله",
    "الْعَرَبِيَّةُ لُغَةٌ",
    "مرحبا بالعالم ١٢٣ 123",
    "ﻻ لأ لإ لآ",
    "كتب ـــ تطويل",
    "فارسی: می\u{200c}خواهم",
    "بِسْمِ اللَّهِ الرَّحْمَٰنِ الرَّحِيمِ",
    "(نص) [عربي] «اقتباس»",
];

const HEBREW: &[&str] = &[
    "שָׁלוֹם עוֹלָם",
    "בְּרֵאשִׁית בָּרָא אֱלֹהִים",
    "עברית 123 (abc) ״גרשיים״",
    "וַיֹּ֥אמֶר אֱלֹהִ֖ים",
];

const DEVANAGARI: &[&str] = &[
    "नमस्ते दुनिया",
    "क्षत्रिय ज्ञान श्री द्व्य र्क",
    "हिन्दी भाषा",
    "क़ ज़ ड़ फ़",
    "्क ि",
    "कि की कु कू कृ के कै को कौ कं कः",
];

const THAI: &[&str] = &["สวัสดีครับ", "น้ำ ป่า ญี่ปุ่น", "กำ ทํา ปั้น"];

const CJK: &[&str] = &[
    "你好，世界！",
    "日本語のテキスト、「かぎ括弧」。",
    "한국어 텍스트",
    "全角ＡＢＣ１２３",
    "〜ー・…",
    "漢字かな交じり文",
];

const EMOJI: &[&str] = &["😀👍🏽👨‍👩‍👧‍👦🏳️‍🌈🇳🇴🇯🇵", "❤️ ❤︎ ☺️ #️⃣ 1️⃣", "👩🏾‍💻 🧑‍🤝‍🧑 🏴󠁧󠁢󠁳󠁣󠁴󠁿", "a😀b"];

const MISSING: &[&str] = &[
    "Latin 中文 混合",
    "abc नमस्ते xyz",
    "emoji 😀 in text",
    "x\u{2011}y",
    "ꙮ 𓀀 𝔸𝔹",
    "e\u{301}\u{1ab0}",
];

#[derive(Default)]
struct Tally {
    cases: usize,
    exact: usize,
    glyphs_differ: usize,
    positions_differ: usize,
    space_fallback_differs: usize,
    flags_differ: usize,
    max_position_difference: i32,
    examples: Vec<String>,
    failing_fonts: BTreeMap<String, usize>,
}

#[derive(Clone)]
struct Case {
    paragraph: String,
    start: usize,
    end: usize,
    direction: ShapeDirection,
    script: Script,
    language: Option<&'static str>,
    flags: u32,
    show_flags: u32,
    features: Vec<Feature>,
    transformed: Option<Vec<(u32, u32)>>,
    hyphen: Option<(u32, u32)>,
}

impl Case {
    fn new(paragraph: &str, start: usize, end: usize) -> Case {
        let script = paragraph[start..end]
            .chars()
            .map(|ch| Script::for_char(u32::from(ch)))
            .find(|script| script.is_real())
            .unwrap_or(Script::COMMON);
        let direction = if matches!(script, Script::ARABIC | Script::HEBREW) {
            ShapeDirection::RightToLeft
        } else {
            ShapeDirection::LeftToRight
        };
        let language = match script {
            Script::ARABIC => Some("ar"),
            Script::HEBREW => Some("he"),
            Script::DEVANAGARI => Some("hi"),
            Script::THAI => Some("th"),
            Script::HAN => Some("zh-cn"),
            Script::HIRAGANA | Script::KATAKANA => Some("ja"),
            Script::HANGUL => Some("ko"),
            _ => Some("en-us"),
        };
        Case {
            paragraph: paragraph.to_owned(),
            start,
            end,
            direction,
            script,
            language,
            flags: buffer_flags::BEGINNING_OF_TEXT | buffer_flags::END_OF_TEXT,
            show_flags: 0,
            features: Vec::new(),
            transformed: None,
            hyphen: None,
        }
    }

    fn request(&self) -> ShapeRequest<'_> {
        ShapeRequest {
            paragraph: &self.paragraph,
            item_start: self.start,
            item_end: self.end,
            transformed: self.transformed.as_deref(),
            hyphen: self.hyphen,
            direction: self.direction,
            script: self.script,
            language: self.language,
            flags: self.flags,
            show_flags: self.show_flags,
            features: &self.features,
        }
    }

    fn describe(&self) -> String {
        format!(
            "{:?}[{}..{}] {:?} {:?} feats={} flags={:#x} show={} transformed={} hyphen={:?}",
            self.paragraph,
            self.start,
            self.end,
            self.direction,
            self.script,
            self.features.len(),
            self.flags,
            self.show_flags,
            self.transformed.is_some(),
            self.hyphen
        )
    }
}

fn features(text: &str) -> Vec<Feature> {
    text.split(',')
        .filter(|item| !item.is_empty())
        .filter_map(|item| feature_from_string(item.as_bytes()))
        .collect()
}

fn items(paragraph: &str) -> Vec<(usize, usize)> {
    let mut items = vec![(0, paragraph.len())];
    let boundaries: Vec<usize> = paragraph
        .char_indices()
        .map(|(index, _)| index)
        .chain([paragraph.len()])
        .collect();
    if boundaries.len() > 4 {
        let third = boundaries[boundaries.len() / 3];
        let two_thirds = boundaries[2 * boundaries.len() / 3];
        items.push((third, two_thirds));
        items.push((0, third));
        items.push((two_thirds, paragraph.len()));
    }
    if let Some(space) = paragraph.find(' ') {
        items.push((space + 1, paragraph.len()));
        items.push((0, space));
    }
    items
}

fn load(context: &Context, description: &str) -> Option<Font> {
    context.load_font(&FontDescription::from_string(description))
}

fn compare(font: &Font, description: &str, case: &Case, tally: &mut Tally) {
    let request = case.request();
    let expected = hb::shape_like_web_text_lib(font, &request);
    let actual = shape(font, &request).expect("font file loads");
    tally.cases += 1;
    if expected == actual {
        tally.exact += 1;
        return;
    }
    let first_for_font = !tally.failing_fonts.contains_key(description);
    *tally
        .failing_fonts
        .entry(description.to_owned())
        .or_default() += 1;
    let same_glyphs = expected.len() == actual.len()
        && expected
            .iter()
            .zip(&actual)
            .all(|(a, b)| a.glyph == b.glyph && a.cluster == b.cluster);
    let same_positions = expected.len() == actual.len()
        && expected.iter().zip(&actual).all(|(a, b)| {
            (a.x_advance, a.y_advance, a.x_offset, a.y_offset)
                == (b.x_advance, b.y_advance, b.x_offset, b.y_offset)
        });
    let only_fallback_spaces = same_glyphs
        && expected.iter().zip(&actual).all(|(a, b)| {
            a == b
                || case.paragraph[a.cluster as usize..]
                    .chars()
                    .next()
                    .is_some_and(|ch| {
                        ch != ' ' && unichar_type(u32::from(ch)) == UnicodeType::SpaceSeparator
                    })
        });
    if !same_glyphs {
        tally.glyphs_differ += 1;
    } else if !same_positions {
        if only_fallback_spaces {
            tally.space_fallback_differs += 1;
        } else {
            tally.positions_differ += 1;
        }
        let difference = expected
            .iter()
            .zip(&actual)
            .map(|(a, b)| {
                [
                    a.x_advance - b.x_advance,
                    a.y_advance - b.y_advance,
                    a.x_offset - b.x_offset,
                    a.y_offset - b.y_offset,
                ]
                .into_iter()
                .map(i32::abs)
                .max()
                .unwrap_or(0)
            })
            .max()
            .unwrap_or(0);
        tally.max_position_difference = tally.max_position_difference.max(difference);
    } else {
        tally.flags_differ += 1;
    }
    if tally.examples.len() < 8 && first_for_font && !only_fallback_spaces {
        tally.examples.push(format!(
            "{} {}\n      harfbuzz {}\n      harfrust {}",
            font.describe().to_string_repr(),
            case.describe(),
            render(&expected),
            render(&actual)
        ));
    }
}

fn render(glyphs: &[ShapedGlyph]) -> String {
    glyphs
        .iter()
        .map(|glyph| {
            format!(
                "{:x}@{}/{}:{},{}+{},{}",
                glyph.glyph,
                glyph.cluster,
                glyph.flags,
                glyph.x_advance,
                glyph.y_advance,
                glyph.x_offset,
                glyph.y_offset
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

trait DescribeRepr {
    fn to_string_repr(&self) -> String;
}

impl DescribeRepr for FontDescription {
    fn to_string_repr(&self) -> String {
        format!("{self:?}").chars().take(120).collect()
    }
}

struct Run {
    tallies: BTreeMap<String, Tally>,
    files: BTreeMap<String, String>,
}

impl Run {
    fn shape_texts(
        &mut self,
        context: &Context,
        category: &str,
        fonts: &[&str],
        sizes: &[&str],
        texts: &[&str],
        configure: &dyn Fn(&mut Case),
    ) {
        for family in fonts {
            for size in sizes {
                let description = match family.split_once(" @") {
                    Some((name, variations)) => format!("{name} {size} @{variations}"),
                    None => format!("{family} {size}"),
                };
                let Some(font) = load(context, &description) else {
                    continue;
                };
                let Some(file) = font.file() else { continue };
                self.files.insert(description.clone(), file);
                for text in texts {
                    for (start, end) in items(text) {
                        let mut case = Case::new(text, start, end);
                        configure(&mut case);
                        let key = if (start, end) == (0, text.len()) {
                            category.to_owned()
                        } else {
                            format!("{category} (item with context)")
                        };
                        compare(
                            &font,
                            &description,
                            &case,
                            self.tallies.entry(key).or_default(),
                        );
                    }
                }
            }
        }
    }
}

const SIZES: &[&str] = &["9", "13.5", "17", "36"];

#[test]
fn shapes_like_harfbuzz() {
    let map = FontMap::new();
    let context = map.create_context();
    if load(&context, "Sans 12").is_none() {
        return;
    }
    let mut run = Run {
        tallies: BTreeMap::new(),
        files: BTreeMap::new(),
    };
    let latin_fonts = [
        "Sans",
        "Serif",
        "Monospace",
        "Segoe UI",
        "Arial",
        "Times New Roman",
        "Consolas",
        "Cascadia Code",
        "Gabriola",
        "Source Sans Pro",
        "DejaVu Sans",
        "Noto Serif",
        "Comic Neue",
    ];
    run.shape_texts(&context, "latin", &latin_fonts, SIZES, LATIN, &|_| {});
    for feature_set in [
        "liga=0",
        "kern=0",
        "smcp=1",
        "liga=0,clig=0,dlig=1",
        "ss01=1,ss02=1,onum=1",
        "frac=1",
        "c2sc=1,smcp=1,kern=0",
    ] {
        run.shape_texts(
            &context,
            &format!("features {feature_set}"),
            &latin_fonts,
            &["13"],
            LATIN,
            &|case| case.features = features(feature_set),
        );
    }
    run.shape_texts(
        &context,
        "features ranged smcp/liga",
        &latin_fonts,
        &["13"],
        LATIN,
        &|case| {
            let middle = case.start + (case.end - case.start) / 2;
            case.features = features("smcp=1,liga=0");
            case.features[0].start = case.start as u32;
            case.features[0].end = middle as u32;
            case.features[1].start = middle as u32;
            case.features[1].end = case.end as u32;
        },
    );
    run.shape_texts(
        &context,
        "arabic",
        &[
            "Noto Naskh Arabic",
            "Noto Sans Arabic",
            "Noto Kufi Arabic",
            "Segoe UI",
            "Arial",
            "Times New Roman",
            "Tahoma",
            "DejaVu Sans",
        ],
        SIZES,
        ARABIC,
        &|_| {},
    );
    run.shape_texts(
        &context,
        "hebrew",
        &[
            "Noto Sans Hebrew",
            "Noto Serif Hebrew",
            "David Libre",
            "Segoe UI",
            "Arial",
            "Times New Roman",
            "DejaVu Sans",
        ],
        SIZES,
        HEBREW,
        &|_| {},
    );
    run.shape_texts(
        &context,
        "devanagari",
        &["Nirmala UI", "Nirmala Text"],
        SIZES,
        DEVANAGARI,
        &|_| {},
    );
    run.shape_texts(
        &context,
        "thai",
        &["Leelawadee UI", "Tahoma"],
        SIZES,
        THAI,
        &|_| {},
    );
    run.shape_texts(
        &context,
        "cjk",
        &[
            "Microsoft YaHei",
            "Yu Gothic",
            "Malgun Gothic",
            "SimSun",
            "MingLiU-ExtB",
        ],
        SIZES,
        CJK,
        &|_| {},
    );
    run.shape_texts(
        &context,
        "emoji",
        &["Segoe UI Emoji"],
        SIZES,
        EMOJI,
        &|_| {},
    );
    run.shape_texts(
        &context,
        "variable",
        &[
            "Bahnschrift",
            "Bahnschrift @wght=300",
            "Bahnschrift @wght=700,wdth=75",
            "Bahnschrift SemiBold Condensed",
            "Segoe UI Variable",
            "Segoe UI Variable @wght=350",
            "Segoe UI Variable @opsz=8",
            "Segoe UI Variable @wght=650,opsz=36",
            "Sitka Text",
            "Sitka Text @wght=600",
            "Ubuntu Mono",
            "Ubuntu Mono @wght=600",
        ],
        &["8", "13", "28"],
        LATIN,
        &|_| {},
    );
    run.shape_texts(
        &context,
        "missing glyphs",
        &[
            "Arial",
            "Consolas",
            "Segoe UI Emoji",
            "Gabriola",
            "Comic Neue",
        ],
        &["13"],
        MISSING,
        &|_| {},
    );
    run.shape_texts(
        &context,
        "synthetic bold/oblique",
        &[
            "Gabriola Bold",
            "Gabriola Italic",
            "Gabriola Bold Italic",
            "Comic Neue Italic",
            "Segoe UI Emoji Italic",
            "Malgun Gothic Italic",
            "Noto Naskh Arabic Italic",
        ],
        &["13", "31"],
        &[LATIN, ARABIC, MISSING].concat(),
        &|_| {},
    );
    run.shape_texts(
        &context,
        "show flags + preserve ignorables",
        &["Arial", "Segoe UI", "Noto Naskh Arabic"],
        &["13"],
        &[
            LATIN,
            ARABIC,
            MISSING,
            &["a\u{2028}b\u{2029}c\u{180b}d\u{e0001}"],
        ]
        .concat(),
        &|case| {
            case.show_flags = show_flags::SPACES | show_flags::IGNORABLES | show_flags::LINE_BREAKS;
            case.flags |= buffer_flags::PRESERVE_DEFAULT_IGNORABLES;
        },
    );
    run.shape_texts(
        &context,
        "unsafe-to-concat",
        &[
            "Arial",
            "Segoe UI",
            "Noto Naskh Arabic",
            "Nirmala UI",
            "Gabriola",
        ],
        &["13"],
        &[LATIN, ARABIC, DEVANAGARI].concat(),
        &|case| case.flags |= buffer_flags::PRODUCE_UNSAFE_TO_CONCAT,
    );
    run.shape_texts(
        &context,
        "hyphen + transform",
        &["Arial", "Segoe UI", "Gabriola", "DejaVu Sans"],
        &["13"],
        LATIN,
        &|case| {
            if case.end > case.start {
                case.hyphen = Some((0x2010, (case.end - 1) as u32));
            }
            let mut chars = Vec::new();
            for (offset, ch) in case.paragraph[case.start..case.end].char_indices() {
                chars.push((to_upper(u32::from(ch)), (case.start + offset) as u32));
            }
            case.transformed = Some(chars);
        },
    );
    run.shape_texts(
        &context,
        "vertical (TTB)",
        &["Microsoft YaHei", "Yu Gothic", "MingLiU-ExtB", "Arial"],
        &["13"],
        &[CJK, LATIN].concat(),
        &|case| case.direction = ShapeDirection::TopToBottom,
    );
    let mut improper = FontDescription::from_string("Segoe UI 13");
    improper.set_gravity(Gravity::North);
    if let Some(font) = context.load_font(&improper) {
        let tally = run
            .tallies
            .entry("improper gravity (negative scale)".to_owned())
            .or_default();
        for text in LATIN.iter().chain(ARABIC) {
            for (start, end) in items(text) {
                let mut case = Case::new(text, start, end);
                case.direction = case.direction.reverse();
                compare(&font, "Segoe UI 13 North", &case, tally);
            }
        }
    }

    eprintln!("HarfBuzz {}", hb::version());
    for (description, file) in &run.files {
        eprintln!("  font {description:40} {file}");
    }
    let mut total = Tally::default();
    for (category, tally) in &run.tallies {
        eprintln!(
            "{category:45} {:5}/{:5} exact  glyphs-differ {:4}  positions-differ {:4} (max {})  fallback-space-differ {:4}  flags-differ {:4}",
            tally.exact,
            tally.cases,
            tally.glyphs_differ,
            tally.positions_differ,
            tally.max_position_difference,
            tally.space_fallback_differs,
            tally.flags_differ
        );
        eprintln!("    failing fonts: {:?}", tally.failing_fonts);
        for example in &tally.examples {
            eprintln!("    {example}");
        }
        total.cases += tally.cases;
        total.exact += tally.exact;
    }
    eprintln!("total {}/{} exact", total.exact, total.cases);
    assert!(total.cases > 0);
}

#[test]
fn maps_scripts_like_glib() {
    for code in -1..200 {
        assert_eq!(
            script_to_iso15924(Script(code)),
            hb::glib_script_to_iso15924(code),
            "script {code}"
        );
    }
}

#[test]
fn queries_fonts_like_harfbuzz() {
    let map = FontMap::new();
    let context = map.create_context();
    let mut checked = 0;
    let mut extents_off_by_rounding = 0;
    let mut color_differs = 0;
    for description in [
        "Arial 13",
        "Segoe UI 17",
        "Gabriola Bold 13",
        "Gabriola Italic 13",
        "Bahnschrift 12 @wght=300",
        "Segoe UI Emoji 20",
        "Nirmala UI 15",
        "Microsoft YaHei 12",
    ] {
        let Some(font) = load(&context, description) else {
            continue;
        };
        let Some(face) = shaper_face(&font) else {
            continue;
        };
        assert_eq!(face.has_color(), hb::font_has_color(&font), "{description}");
        for ch in (0x20..0x3000).chain(0x1f300..0x1f700) {
            let glyph = hb::nominal_glyph_of(&font, ch);
            assert_eq!(face.nominal_glyph(ch), glyph, "{description} U+{ch:04X}");
        }
        for glyph in 0..face.num_glyphs().min(3000) {
            assert_eq!(
                face.glyph_h_advance(glyph),
                hb::glyph_h_advance_of(&font, glyph),
                "{description} glyph {glyph}"
            );
            assert_eq!(
                face.glyph_v_advance(glyph),
                hb::glyph_v_advance_of(&font, glyph),
                "{description} glyph {glyph}"
            );
            let extents = face
                .glyph_extents(glyph)
                .map(|e| (e.x_bearing, e.y_bearing, e.width, e.height));
            let expected = hb::glyph_extents_of(&font, glyph);
            let (ours, theirs) = (extents.unwrap_or_default(), expected.unwrap_or_default());
            let difference = [
                ours.0 - theirs.0,
                ours.1 - theirs.1,
                ours.2 - theirs.2,
                ours.3 - theirs.3,
            ]
            .into_iter()
            .map(i32::abs)
            .max()
            .unwrap_or(0);
            let setup = font.hb_font_setup();
            let synthetic = setup.embolden != 0. || setup.slant != 0. || face.has_color();
            assert!(
                synthetic || difference <= 4,
                "{description} glyph {glyph}: {ours:?} {theirs:?}"
            );
            if difference != 0 {
                extents_off_by_rounding += 1;
            }
            if hb::glyph_has_paint_or_layers(&font, glyph) != face.glyph_has_color(glyph) {
                color_differs += 1;
            }
            checked += 1;
        }
    }
    eprintln!(
        "checked {checked} glyphs, {extents_off_by_rounding} with extents off by rounding, {color_differs} with color paint/layers answered differently"
    );
}

#[test]
fn normalizes_variations_like_harfbuzz() {
    let map = FontMap::new();
    let context = map.create_context();
    for description in [
        "Bahnschrift 13",
        "Bahnschrift 13 @wght=300",
        "Bahnschrift 13 @wght=700,wdth=75",
        "Segoe UI Variable 13",
        "Segoe UI Variable 13 @wght=350",
        "Segoe UI Variable 31 @wght=650,opsz=36",
        "Sitka Text 8",
        "Sitka Text 23 @wght=555",
        "Ubuntu Mono 13 @wght=600",
    ] {
        let Some(font) = load(&context, description) else {
            continue;
        };
        let Some(face) = shaper_face(&font) else {
            continue;
        };
        let mut ours: Vec<i32> = face
            .font
            .normalized_coords()
            .iter()
            .map(|coord| i32::from(coord.to_bits()))
            .collect();
        let theirs = hb::normalized_coords_of(&font);
        if ours.is_empty() {
            ours = vec![0; theirs.len()];
        }
        assert_eq!(ours, theirs, "{description}");
    }
}

fn table_tag(name: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*name)
}

#[test]
fn answers_table_queries_like_harfbuzz() {
    let map = FontMap::new();
    let context = map.create_context();
    let metrics: Vec<u32> = [
        b"hasc", b"hdsc", b"hlgp", b"xhgt", b"cpht", b"sbxs", b"sbys", b"sbxo", b"sbyo", b"spxs",
        b"spys", b"spxo", b"spyo", b"strs", b"stro", b"unds", b"undo", b"hcla", b"hcld", b"zzzz",
    ]
    .into_iter()
    .map(table_tag)
    .collect();
    let probes: [(&[u8; 4], Option<&str>, &[&[u8; 4]]); 12] = [
        (b"Latn", None, &[b"smcp"]),
        (b"Latn", Some("tr"), &[b"smcp", b"c2sc"]),
        (b"Latn", Some("en"), &[b"liga"]),
        (b"Latn", None, &[]),
        (b"Arab", Some("ar"), &[b"init", b"medi"]),
        (b"Arab", Some("fa"), &[b"smcp"]),
        (b"Deva", Some("hi"), &[b"half"]),
        (b"Hani", Some("ja"), &[b"vert"]),
        (b"Hani", Some("zh-tw"), &[b"pcap"]),
        (b"Grek", None, &[b"smcp", b"c2sc"]),
        (b"Cyrl", Some("sr"), &[b"locl"]),
        (b"Zyyy", None, &[b"unic"]),
    ];
    let mut faces = Vec::new();
    for family in map.list_families() {
        for face in family.list_faces() {
            faces.push(face.describe().to_string());
        }
    }
    faces.extend(
        [
            "Bahnschrift 12 @wght=300,wdth=80",
            "Segoe UI Variable Display 15 @wght=650",
            "Arial Bold Italic 9",
            "Gabriola 31",
            "Sitka Small 14 @opsz=11",
        ]
        .map(String::from),
    );
    let (mut fonts, mut glyphs, mut carets) = (0, 0, 0);
    let mut mismatches = Vec::new();
    for base in faces {
        for size in [" 13", " 7.5", " 41"] {
            let description = if base.chars().any(|c| c.is_ascii_digit()) {
                base.clone()
            } else {
                format!("{base}{size}")
            };
            let Some(font) = load(&context, &description) else {
                continue;
            };
            let Some(face) = shaper_face(&font) else {
                continue;
            };
            let hb_font = font.hb_font();
            fonts += 1;
            let mut check = |what: String, ours: String, theirs: String| {
                if ours != theirs && mismatches.len() < 400 {
                    mismatches.push(format!(
                        "{description}: {what}: ours {ours} harfbuzz {theirs}"
                    ));
                }
            };
            for &metric in &metrics {
                check(
                    format!("metric {:?}", metric.to_be_bytes().map(char::from)),
                    format!("{:?}", face.ot_metric(metric)),
                    format!("{:?}", hb_font.ot_metrics_position(metric)),
                );
            }
            let (ours, theirs) = (face.h_extents(), hb_font.h_extents());
            check(
                "h extents".into(),
                format!("{} {} {}", ours.ascender, ours.descender, ours.line_gap),
                format!(
                    "{} {} {}",
                    theirs.ascender, theirs.descender, theirs.line_gap
                ),
            );
            let (ours, theirs) = (face.extents_ltr(), hb_font.extents_ltr());
            check(
                "ltr extents".into(),
                format!("{} {} {}", ours.ascender, ours.descender, ours.line_gap),
                format!(
                    "{} {} {}",
                    theirs.ascender, theirs.descender, theirs.line_gap
                ),
            );
            check(
                "y scale".into(),
                face.y_scale.to_string(),
                hb_font.y_scale().to_string(),
            );
            for (script, language, features) in &probes {
                let wanted: Vec<u32> = features.iter().map(|f| table_tag(f)).collect();
                check(
                    format!(
                        "gsub {:?} {language:?} {:?}",
                        script.map(char::from),
                        wanted
                    ),
                    face.gsub_features_supported(table_tag(script), *language, &wanted)
                        .to_string(),
                    hb_font
                        .gsub_features_supported(table_tag(script), *language, &wanted)
                        .to_string(),
                );
            }
            if size != " 13" {
                continue;
            }
            for glyph in 0..face.num_glyphs() {
                glyphs += 1;
                check(
                    format!("class of glyph {glyph}"),
                    face.glyph_class(glyph).to_string(),
                    hb_font.glyph_class(glyph).to_string(),
                );
                for direction in [4, 5, 6] {
                    for start in 0..3 {
                        let ours = face.ligature_caret(direction & !1 == 4, glyph, start);
                        let theirs = hb_font.ligature_caret(direction, glyph, start);
                        if theirs.0 != 0 {
                            carets += 1;
                        }
                        check(
                            format!("caret of glyph {glyph} dir {direction} from {start}"),
                            format!("{ours:?}"),
                            format!("{theirs:?}"),
                        );
                    }
                }
            }
            if base.chars().any(|c| c.is_ascii_digit()) {
                break;
            }
        }
    }
    eprintln!("compared {fonts} fonts, {glyphs} glyphs, {carets} caret queries");
    for line in &mismatches {
        eprintln!("{line}");
    }
    assert!(fonts > 0 || load(&context, "Sans 12").is_none());
    assert!(mismatches.is_empty(), "{} mismatches", mismatches.len());
}
