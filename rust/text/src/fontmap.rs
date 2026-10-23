//! Southstar text — the fontconfig font map: font families and faces, pattern matching, and the caches that turn descriptions into fonts and fontsets.
//! Copyright 2000-2003 Red Hat Software, 2000 Tor Lillqvist, 2005 Keith Packard; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use std::cell::OnceCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::CString;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, Weak};

use crate::context::Context;
use crate::coverage::Coverage;
use crate::ffi::fontconfig::{
    self, FC_CHARCELL, FC_COLOR, FC_DPI, FC_DUAL, FC_FAMILY, FC_FILE, FC_FONT_FEATURES,
    FC_FONT_VARIATIONS, FC_FONT_WRAPPER, FC_FONTFORMAT, FC_INDEX, FC_LANG, FC_MATRIX, FC_MONO,
    FC_ORDER, FC_PIXEL_SIZE, FC_PRGNAME, FC_PROPORTIONAL, FC_SIZE, FC_SLANT, FC_SLANT_ITALIC,
    FC_SLANT_OBLIQUE, FC_SLANT_ROMAN, FC_SPACING, FC_STYLE, FC_VARIABLE, FC_WEIGHT,
    FC_WEIGHT_MEDIUM, FC_WIDTH, FaceOrder, FontSet, PANGO_FC_GRAVITY, PANGO_FC_VERSION, Pattern,
    PatternBuilder,
};
use crate::ffi::harfbuzz::HbFace;
use crate::font::{Font, FontCoverage};
use crate::font_description::{FontDescription, c_double_to_int};
use crate::fontset::{Fontset, Patterns};
use crate::language::Language;
use crate::matrix::{Matrix, matrix_get_font_scale_factor};
use crate::types::{Gravity, SCALE, Stretch, Style, Variant, Weight, font_mask};
use crate::utils::version;

pub use crate::ffi::cairo::FontOptions;

const FONTSET_CACHE_SIZE: usize = 256;
const DEFAULT_DPI: f64 = 96.0;

#[derive(Clone)]
pub struct FontMap(pub(crate) Arc<FontMapShared>);

pub(crate) struct FontMapShared {
    state: Mutex<FontMapState>,
}

struct FontMapState {
    serial: u32,
    dpi: f64,
    fontset_hash: HashMap<FontsetKey, Fontset>,
    fontset_cache: VecDeque<Fontset>,
    font_hash: HashMap<FontKey, Weak<crate::font::FontInner>>,
    patterns_hash: HashMap<usize, Weak<Patterns>>,
    pattern_hash: HashSet<UniquePattern>,
    face_data: HashMap<(Vec<u8>, i32), Arc<FaceData>>,
    families: Option<Vec<FontFamily>>,
    fonts: Option<Arc<FontSet>>,
}

struct UniquePattern(Pattern);

impl PartialEq for UniquePattern {
    fn eq(&self, other: &UniquePattern) -> bool {
        self.0.equal(&other.0)
    }
}

impl Eq for UniquePattern {}

impl core::hash::Hash for UniquePattern {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.0.hash_value().hash(state);
    }
}

pub(crate) struct FaceData {
    filename: CString,
    id: i32,
    coverage: OnceLock<Option<FontCoverage>>,
    languages: OnceLock<Option<Vec<Language>>>,
    hb_face: OnceLock<HbFace>,
}

impl FaceData {
    pub(crate) fn coverage(&self, font_pattern: &Pattern) -> FontCoverage {
        let cached = self.coverage.get_or_init(|| {
            font_pattern
                .charset()
                .map(|charset| FontCoverage::new(Coverage::from_charset(charset)))
        });
        match cached {
            Some(coverage) => coverage.clone(),
            None => FontCoverage::new(Coverage::new()),
        }
    }

    pub(crate) fn languages(&self, font_pattern: &Pattern) -> Option<Vec<Language>> {
        self.languages
            .get_or_init(|| {
                font_pattern.languages().map(|languages| {
                    languages
                        .iter()
                        .map(|language| Language::from_string(language))
                        .collect()
                })
            })
            .clone()
    }

    pub(crate) fn hb_face(&self) -> HbFace {
        self.hb_face
            .get_or_init(|| HbFace::from_file(&self.filename, self.id))
            .clone()
    }
}

fn matrix_bits(matrix: &Matrix) -> [u64; 4] {
    [
        matrix.xx.to_bits(),
        matrix.xy.to_bits(),
        matrix.yx.to_bits(),
        matrix.yy.to_bits(),
    ]
}

#[derive(Clone)]
pub(crate) struct FontKey {
    pub(crate) pattern: Pattern,
    pub(crate) matrix: Matrix,
    pub(crate) context_key: Option<FontOptions>,
    pub(crate) variations: Option<String>,
    pub(crate) features: Option<String>,
}

impl PartialEq for FontKey {
    fn eq(&self, other: &FontKey) -> bool {
        self.pattern.id() == other.pattern.id()
            && self.variations == other.variations
            && self.features == other.features
            && matrix_bits(&self.matrix) == matrix_bits(&other.matrix)
            && self.context_key == other.context_key
    }
}

impl Eq for FontKey {}

impl core::hash::Hash for FontKey {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.pattern.id().hash(state);
        matrix_bits(&self.matrix).hash(state);
        self.variations.hash(state);
        self.features.hash(state);
        self.context_key.hash(state);
    }
}

#[derive(Clone)]
pub(crate) struct FontsetKey {
    pub(crate) language: Option<Language>,
    pub(crate) desc: FontDescription,
    pub(crate) matrix: Matrix,
    pub(crate) pixelsize: i32,
    pub(crate) resolution: f64,
    pub(crate) context_key: Option<FontOptions>,
    pub(crate) variations: Option<String>,
    pub(crate) features: Option<String>,
}

impl PartialEq for FontsetKey {
    fn eq(&self, other: &FontsetKey) -> bool {
        self.language == other.language
            && self.pixelsize == other.pixelsize
            && self.resolution == other.resolution
            && self.variations == other.variations
            && self.features == other.features
            && self.desc.equal(&other.desc)
            && matrix_bits(&self.matrix) == matrix_bits(&other.matrix)
            && self.context_key == other.context_key
    }
}

impl Eq for FontsetKey {}

impl core::hash::Hash for FontsetKey {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.language.hash(state);
        self.desc.hash().hash(state);
        matrix_bits(&self.matrix).hash(state);
        self.pixelsize.hash(state);
        self.resolution.to_bits().hash(state);
        self.variations.hash(state);
        self.features.hash(state);
        self.context_key.hash(state);
    }
}

fn context_matrix(context: Option<&Context>) -> Matrix {
    let mut matrix = context
        .and_then(Context::matrix)
        .unwrap_or(Matrix::IDENTITY);
    matrix.x0 = 0.0;
    matrix.y0 = 0.0;
    matrix
}

pub(crate) fn split_list(text: &str) -> Vec<&str> {
    if text.is_empty() {
        Vec::new()
    } else {
        text.split(',').collect()
    }
}

pub(crate) fn gravity_nick(gravity: Gravity) -> &'static str {
    match gravity {
        Gravity::South => "south",
        Gravity::East => "east",
        Gravity::North => "north",
        Gravity::West => "west",
        Gravity::Auto => "auto",
    }
}

pub(crate) fn gravity_from_nick(nick: &str) -> Gravity {
    match nick {
        "east" => Gravity::East,
        "north" => Gravity::North,
        "west" => Gravity::West,
        "auto" => Gravity::Auto,
        _ => Gravity::South,
    }
}

fn maybe_add_feature(out: &mut String, features: Option<&str>, feature: &str) {
    if features.is_some_and(|features| features.contains(&feature[..4])) {
        return;
    }
    if !out.is_empty() {
        out.push(',');
    }
    out.push_str(feature);
}

pub fn combine_features(variant: Variant, features: Option<&str>) -> String {
    let mut out = String::new();
    match variant {
        Variant::SmallCaps => maybe_add_feature(&mut out, features, "smcp=1"),
        Variant::AllSmallCaps => {
            maybe_add_feature(&mut out, features, "smcp=1");
            maybe_add_feature(&mut out, features, "c2sc=1");
        }
        Variant::PetiteCaps => maybe_add_feature(&mut out, features, "pcap=1"),
        Variant::AllPetiteCaps => {
            maybe_add_feature(&mut out, features, "pcap=1");
            maybe_add_feature(&mut out, features, "c2pc=1");
        }
        Variant::Unicase => maybe_add_feature(&mut out, features, "unic=1"),
        Variant::TitleCaps => maybe_add_feature(&mut out, features, "titl=1"),
        Variant::Normal => {}
    }
    if let Some(features) = features {
        if !out.is_empty() {
            out.push(',');
        }
        out.push_str(features);
    }
    out
}

pub fn features_get_variant(features: Option<&str>) -> Variant {
    let Some(features) = features.filter(|f| !f.is_empty()) else {
        return Variant::Normal;
    };
    let mut variant = Variant::Normal;
    let mut all_caps = false;
    for token in features.split(',') {
        match token {
            "smcp=1" => {
                variant = if all_caps {
                    Variant::AllSmallCaps
                } else {
                    Variant::SmallCaps
                }
            }
            "c2sc=1" => {
                if variant == Variant::SmallCaps {
                    variant = Variant::AllSmallCaps;
                } else {
                    all_caps = true;
                }
            }
            "pcap=1" => {
                variant = if all_caps {
                    Variant::AllPetiteCaps
                } else {
                    Variant::PetiteCaps
                }
            }
            "c2pc=1" => {
                if variant == Variant::PetiteCaps {
                    variant = Variant::AllPetiteCaps;
                } else {
                    all_caps = true;
                }
            }
            "unic=1" => variant = Variant::Unicase,
            "titl=1" => variant = Variant::TitleCaps,
            _ => {}
        }
    }
    variant
}

fn slant_to_fc(style: Style) -> i32 {
    match style {
        Style::Normal => FC_SLANT_ROMAN,
        Style::Italic => FC_SLANT_ITALIC,
        Style::Oblique => FC_SLANT_OBLIQUE,
    }
}

fn slant_to_style(slant: i32) -> Style {
    match slant {
        FC_SLANT_ITALIC => Style::Italic,
        FC_SLANT_OBLIQUE => Style::Oblique,
        _ => Style::Normal,
    }
}

fn width_to_stretch(width: i32) -> Stretch {
    match width {
        fontconfig::FC_WIDTH_ULTRACONDENSED => Stretch::UltraCondensed,
        fontconfig::FC_WIDTH_EXTRACONDENSED => Stretch::ExtraCondensed,
        fontconfig::FC_WIDTH_CONDENSED => Stretch::Condensed,
        fontconfig::FC_WIDTH_SEMICONDENSED => Stretch::SemiCondensed,
        fontconfig::FC_WIDTH_SEMIEXPANDED => Stretch::SemiExpanded,
        fontconfig::FC_WIDTH_EXPANDED => Stretch::Expanded,
        fontconfig::FC_WIDTH_EXTRAEXPANDED => Stretch::ExtraExpanded,
        fontconfig::FC_WIDTH_ULTRAEXPANDED => Stretch::UltraExpanded,
        _ => Stretch::Normal,
    }
}

pub(crate) fn font_description_from_pattern(
    pattern: &Pattern,
    include_size: bool,
) -> FontDescription {
    let mut desc = FontDescription::new();
    let family = pattern.get_string(FC_FAMILY, 0).unwrap_or_default();
    desc.set_family(Some(&family));

    let style = pattern
        .get_integer(FC_SLANT, 0)
        .map_or(Style::Normal, slant_to_style);
    desc.set_style(style);

    let weight = pattern
        .get_double(FC_WEIGHT, 0)
        .map_or(Weight::NORMAL, |d| {
            Weight(c_double_to_int(fontconfig::weight_to_opentype(d)))
        });
    desc.set_weight(weight);

    let stretch = pattern
        .get_integer(FC_WIDTH, 0)
        .map_or(Stretch::Normal, width_to_stretch);
    desc.set_stretch(stretch);

    let mut features: Option<String> = None;
    for i in 0..32 {
        let Some(feature) = pattern.get_string(FC_FONT_FEATURES, i) else {
            break;
        };
        let combined = features.get_or_insert_with(String::new);
        if !combined.is_empty() {
            combined.push(',');
        }
        combined.push_str(&feature);
    }
    match features {
        Some(features) => {
            desc.set_variant(features_get_variant(Some(&features)));
            desc.set_features(Some(&features));
        }
        None => desc.set_variant(Variant::Normal),
    }

    if include_size && let Some(size) = pattern.get_double(FC_SIZE, 0) {
        let scale_factor = match pattern.get_matrix(FC_MATRIX, 0) {
            Some(fc_matrix) => Matrix {
                xx: fc_matrix.xx,
                xy: fc_matrix.xy,
                yx: fc_matrix.yx,
                yy: fc_matrix.yy,
                ..Matrix::IDENTITY
            }
            .font_scale_factor(),
            None => 1.0,
        };
        let scaled_size = scale_factor * size * f64::from(SCALE);
        desc.set_size(c_double_to_int(scaled_size));
    }

    if let Some(nick) = pattern.get_string(PANGO_FC_GRAVITY, 0) {
        desc.set_gravity(gravity_from_nick(&nick));
    }

    if let Some(variations) = pattern.get_string(FC_FONT_VARIATIONS, 0)
        && !variations.is_empty()
    {
        desc.set_variations(Some(&variations));
    }
    desc
}

fn is_supported_font_format(pattern: &Pattern) -> bool {
    if pattern.get_cstr(FC_FILE, 0).is_none() {
        return false;
    }
    if let Some(wrapper) = pattern.get_cstr(FC_FONT_WRAPPER, 0) {
        return wrapper.to_bytes() == b"SFNT";
    }
    let Some(format) = pattern.get_cstr(FC_FONTFORMAT, 0) else {
        return false;
    };
    let format = format.to_bytes();
    format.eq_ignore_ascii_case(b"TrueType") || format.eq_ignore_ascii_case(b"CFF")
}

fn filter_by_format(sets: [Option<FontSet>; 2]) -> FontSet {
    let mut result = FontSet::new();
    for (order, set) in sets.iter().enumerate() {
        let Some(set) = set else {
            continue;
        };
        for pattern in set.patterns() {
            if !is_supported_font_format(&pattern) {
                continue;
            }
            result.push(pattern.with_order(order as i32));
        }
    }
    result
}

fn is_alias_family(name: &str) -> bool {
    match name.as_bytes().first() {
        Some(b'c' | b'C') => name.eq_ignore_ascii_case("cursive"),
        Some(b'f' | b'F') => name.eq_ignore_ascii_case("fantasy"),
        Some(b'm' | b'M') => name.eq_ignore_ascii_case("monospace"),
        Some(b's' | b'S') => {
            name.eq_ignore_ascii_case("sans")
                || name.eq_ignore_ascii_case("serif")
                || name.eq_ignore_ascii_case("system-ui")
        }
        _ => false,
    }
}

fn make_pattern(key: &FontsetKey) -> PatternBuilder {
    let description = &key.desc;
    let mut pattern = PatternBuilder::new();
    pattern.add_integer(PANGO_FC_VERSION, version());
    pattern.add_double(
        FC_WEIGHT,
        fontconfig::weight_from_opentype(f64::from(description.weight().0)),
    );
    pattern.add_integer(FC_SLANT, slant_to_fc(description.style()));
    pattern.add_double(FC_WIDTH, f64::from(description.width().0) / 10.0);
    pattern.add_bool(FC_VARIABLE, fontconfig::FC_DONT_CARE);
    pattern.add_double(FC_DPI, key.resolution);
    pattern.add_double(
        FC_SIZE,
        f64::from(key.pixelsize) * (72. / 1024. / key.resolution),
    );
    pattern.add_double(FC_PIXEL_SIZE, f64::from(key.pixelsize) / 1024.);
    pattern.add_integer(FC_ORDER, 1000);
    pattern.add_bool(FC_COLOR, description.color() as i32);

    if let Some(variations) = key.variations.as_deref() {
        pattern.add_string(FC_FONT_VARIATIONS, variations);
    }
    if let Some(family) = description.family() {
        for name in split_list(family) {
            pattern.add_string(FC_FAMILY, name);
        }
    }
    if let Some(language) = key.language {
        pattern.add_string(FC_LANG, language.as_str());
    }
    let gravity = description.gravity();
    if gravity != Gravity::South {
        pattern.add_string(PANGO_FC_GRAVITY, gravity_nick(gravity));
    }
    if let Some(prgname) = fontconfig::prgname() {
        pattern.add_string(FC_PRGNAME, &prgname);
    }
    let combined = combine_features(description.variant(), key.features.as_deref());
    for feature in split_list(&combined) {
        pattern.add_string(FC_FONT_FEATURES, feature);
    }
    pattern
}

fn lock(shared: &FontMapShared) -> MutexGuard<'_, FontMapState> {
    shared
        .state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

thread_local! {
    static DEFAULT_FONT_MAP: OnceCell<FontMap> = const { OnceCell::new() };
}

impl Default for FontMap {
    fn default() -> FontMap {
        FontMap::new()
    }
}

impl PartialEq for FontMap {
    fn eq(&self, other: &FontMap) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for FontMap {}

impl core::fmt::Debug for FontMap {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "FontMap({:p})", Arc::as_ptr(&self.0))
    }
}

impl FontMap {
    pub fn new() -> FontMap {
        fontconfig::init();
        FontMap(Arc::new(FontMapShared {
            state: Mutex::new(FontMapState::new()),
        }))
    }

    pub fn default_for_thread() -> FontMap {
        DEFAULT_FONT_MAP.with(|map| map.get_or_init(FontMap::new).clone())
    }

    pub(crate) fn from_shared(shared: Arc<FontMapShared>) -> FontMap {
        FontMap(shared)
    }

    pub(crate) fn downgrade(&self) -> Weak<FontMapShared> {
        Arc::downgrade(&self.0)
    }

    pub fn ptr_eq(&self, other: &FontMap) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    pub fn create_context(&self) -> Context {
        let mut context = Context::new();
        context.set_font_map(Some(self.clone()));
        context
    }

    pub fn serial(&self) -> u32 {
        lock(&self.0).serial
    }

    pub fn changed(&self) {
        let mut state = lock(&self.0);
        state.serial = state.serial.wrapping_add(1);
        if state.serial == 0 {
            state.serial = 1;
        }
    }

    pub fn resolution(&self) -> f64 {
        lock(&self.0).dpi
    }

    pub(crate) fn resolution_for(&self, context: Option<&Context>) -> f64 {
        let map_dpi = self.resolution();
        match context {
            Some(context) => {
                let dpi = context.resolution();
                if dpi <= 0.0 { map_dpi } else { dpi }
            }
            None => map_dpi,
        }
    }

    pub fn cache_clear(&self) {
        let old = {
            let mut state = lock(&self.0);
            let dpi = state.dpi;
            let serial = state.serial;
            let old = core::mem::replace(&mut *state, FontMapState::new());
            state.dpi = dpi;
            state.serial = serial;
            old
        };
        drop(old);
        self.ensure_families();
        self.changed();
    }

    pub fn config_changed(&self) {
        self.cache_clear();
    }

    pub(crate) fn config_fonts(&self) -> Arc<FontSet> {
        let mut state = lock(&self.0);
        state.config_fonts()
    }

    fn ensure_families(&self) -> Vec<FontFamily> {
        let fonts = {
            let mut state = lock(&self.0);
            if let Some(families) = state.families.as_ref() {
                return families.clone();
            }
            state.config_fonts()
        };
        let empty = PatternBuilder::new().finish();
        let objects = [
            FC_FAMILY,
            FC_SPACING,
            FC_STYLE,
            FC_WEIGHT,
            FC_WIDTH,
            FC_SLANT,
            FC_VARIABLE,
            FC_FONTFORMAT,
        ];
        let listed = fonts.list(&empty, &objects);
        let mut builders: Vec<FamilyBuilder> = Vec::new();
        let mut by_name: HashMap<String, usize> = HashMap::new();
        for pattern in listed.as_ref().map(FontSet::patterns).unwrap_or_default() {
            let name = pattern.get_string(FC_FAMILY, 0).unwrap_or_default();
            let mut index = by_name.get(&name).copied();
            if !is_alias_family(&name) && index.is_none() {
                let spacing = pattern
                    .get_integer(FC_SPACING, 0)
                    .unwrap_or(FC_PROPORTIONAL);
                builders.push(FamilyBuilder::new(&name, spacing, false));
                by_name.insert(name.clone(), builders.len() - 1);
                index = Some(builders.len() - 1);
            }
            if let Some(index) = index {
                let family = &mut builders[index];
                if pattern.get_bool(FC_VARIABLE, 0) == Some(true) {
                    family.variable = true;
                }
                family.patterns.push(pattern);
            }
        }
        builders.push(FamilyBuilder::new("Sans", FC_PROPORTIONAL, false));
        builders.push(FamilyBuilder::new("Serif", FC_PROPORTIONAL, false));
        builders.push(FamilyBuilder::new("Monospace", FC_MONO, false));
        builders.push(FamilyBuilder::new("System-ui", FC_PROPORTIONAL, false));
        builders.sort_by(|a, b| a.name.as_bytes().cmp(b.name.as_bytes()));
        let weak = self.downgrade();
        let families: Vec<FontFamily> = builders
            .into_iter()
            .map(|builder| builder.finish(weak.clone()))
            .collect();

        let mut state = lock(&self.0);
        if let Some(existing) = state.families.as_ref() {
            return existing.clone();
        }
        state.families = Some(families.clone());
        families
    }

    pub fn list_families(&self) -> Vec<FontFamily> {
        self.ensure_families()
    }

    pub fn family(&self, name: &str) -> Option<FontFamily> {
        self.ensure_families()
            .into_iter()
            .find(|family| family.name() == name)
    }

    pub fn load_font(&self, context: Option<&Context>, desc: &FontDescription) -> Option<Font> {
        let language = context.and_then(Context::language);
        let fontset = self.load_fontset(context, desc, language)?;
        let mut first = None;
        fontset.foreach(|font| {
            first = Some(font.clone());
            true
        });
        first
    }

    pub fn load_fontset(
        &self,
        context: Option<&Context>,
        desc: &FontDescription,
        language: Option<Language>,
    ) -> Option<Fontset> {
        let key = self.fontset_key(context, desc, language);
        let mut evicted = Vec::new();
        let fontset = {
            let mut state = lock(&self.0);
            let fontset = match state.fontset_hash.get(&key) {
                Some(fontset) => fontset.clone(),
                None => {
                    let patterns = state.patterns_for(self, &key, context)?;
                    let fontset = Fontset::new(self.downgrade(), key.clone(), patterns);
                    state.fontset_hash.insert(key, fontset.clone());
                    fontset
                }
            };
            state.touch_fontset(&fontset, &mut evicted);
            fontset
        };
        drop(evicted);
        Some(fontset)
    }

    fn fontset_key(
        &self,
        context: Option<&Context>,
        desc: &FontDescription,
        language: Option<Language>,
    ) -> FontsetKey {
        let language = match language {
            Some(language) => Some(language),
            None => context.and_then(Context::language),
        };
        let resolution = self.resolution_for(context);
        let mut size = f64::from(desc.size());
        if !desc.size_is_absolute() {
            size = size * resolution / 72.;
        }
        let scale = matrix_get_font_scale_factor(context.and_then(Context::matrix).as_ref());
        let pixelsize = c_double_to_int(0.5 + scale * size);
        let mut key_desc = desc.clone();
        key_desc.unset_fields(font_mask::SIZE | font_mask::VARIATIONS | font_mask::FEATURES);
        FontsetKey {
            language,
            desc: key_desc,
            matrix: context_matrix(context),
            pixelsize,
            resolution,
            context_key: context.map(Context::merged_font_options),
            variations: desc.variations().map(str::to_owned),
            features: desc.features().map(str::to_owned),
        }
    }

    pub(crate) fn new_font(&self, key: &FontsetKey, matched: &Pattern) -> Option<Font> {
        let mut state = lock(&self.0);
        let pattern = state.uniquify(matched);
        let font_key = FontKey {
            pattern,
            matrix: key.matrix,
            context_key: key.context_key.clone(),
            variations: key.variations.clone(),
            features: key.features.clone(),
        };
        if let Some(font) = state
            .font_hash
            .get(&font_key)
            .and_then(Weak::upgrade)
            .map(Font::from_inner)
        {
            return Some(font);
        }
        let font = Font::new(self.downgrade(), font_key.clone());
        if state.font_hash.len() >= 1024 && state.font_hash.len().is_power_of_two() {
            state.font_hash.retain(|_, font| font.strong_count() > 0);
        }
        state.font_hash.insert(font_key, font.downgrade());
        Some(font)
    }

    pub(crate) fn face_data(&self, font_pattern: &Pattern) -> Option<Arc<FaceData>> {
        let filename = font_pattern.get_cstr(FC_FILE, 0)?.to_owned();
        let id = font_pattern.get_integer(FC_INDEX, 0)?;
        let mut state = lock(&self.0);
        let key = (filename.as_bytes().to_vec(), id);
        let data = state.face_data.entry(key).or_insert_with(|| {
            Arc::new(FaceData {
                filename,
                id,
                coverage: OnceLock::new(),
                languages: OnceLock::new(),
                hb_face: OnceLock::new(),
            })
        });
        Some(data.clone())
    }
}

impl FontMapState {
    fn new() -> FontMapState {
        FontMapState {
            serial: 1,
            dpi: DEFAULT_DPI,
            fontset_hash: HashMap::new(),
            fontset_cache: VecDeque::new(),
            font_hash: HashMap::new(),
            patterns_hash: HashMap::new(),
            pattern_hash: HashSet::new(),
            face_data: HashMap::new(),
            families: None,
            fonts: None,
        }
    }

    fn config_fonts(&mut self) -> Arc<FontSet> {
        if let Some(fonts) = self.fonts.as_ref() {
            return fonts.clone();
        }
        let sets = [
            FontSet::config_fonts(fontconfig::SET_SYSTEM),
            FontSet::config_fonts(fontconfig::SET_APPLICATION),
        ];
        let fonts = Arc::new(filter_by_format(sets));
        self.fonts = Some(fonts.clone());
        fonts
    }

    fn uniquify(&mut self, pattern: &Pattern) -> Pattern {
        let candidate = UniquePattern(pattern.clone());
        if let Some(existing) = self.pattern_hash.get(&candidate) {
            return existing.0.clone();
        }
        self.pattern_hash.insert(candidate);
        pattern.clone()
    }

    fn patterns_for(
        &mut self,
        map: &FontMap,
        key: &FontsetKey,
        context: Option<&Context>,
    ) -> Option<Arc<Patterns>> {
        let mut pattern = make_pattern(key);
        pattern.config_substitute();
        if context.is_some()
            && let Some(options) = key.context_key.as_ref()
        {
            options.substitute_into(&mut pattern);
        }
        pattern.default_substitute();
        let pattern = self.uniquify(&pattern.finish());
        if let Some(existing) = self
            .patterns_hash
            .get(&pattern.id())
            .and_then(Weak::upgrade)
        {
            return Some(existing);
        }
        let fonts = self.config_fonts();
        let patterns = Arc::new(Patterns::new(map.downgrade(), pattern.clone(), fonts));
        self.patterns_hash
            .insert(pattern.id(), Arc::downgrade(&patterns));
        Some(patterns)
    }

    fn touch_fontset(&mut self, fontset: &Fontset, evicted: &mut Vec<Fontset>) {
        if let Some(position) = self
            .fontset_cache
            .iter()
            .position(|cached| cached.ptr_eq(fontset))
        {
            if position == 0 {
                return;
            }
            if let Some(cached) = self.fontset_cache.remove(position) {
                self.fontset_cache.push_front(cached);
            }
            return;
        }
        if self.fontset_cache.len() == FONTSET_CACHE_SIZE
            && let Some(oldest) = self.fontset_cache.pop_back()
        {
            if let Some(removed) = self.fontset_hash.remove(oldest.key()) {
                evicted.push(removed);
            }
            evicted.push(oldest);
        }
        self.fontset_cache.push_front(fontset.clone());
    }
}

struct FamilyBuilder {
    name: String,
    spacing: i32,
    variable: bool,
    patterns: Vec<Pattern>,
}

impl FamilyBuilder {
    fn new(name: &str, spacing: i32, variable: bool) -> FamilyBuilder {
        FamilyBuilder {
            name: name.to_owned(),
            spacing,
            variable,
            patterns: Vec::new(),
        }
    }

    fn finish(self, fontmap: Weak<FontMapShared>) -> FontFamily {
        FontFamily(Arc::new(FamilyInner {
            fontmap,
            name: self.name,
            spacing: self.spacing,
            variable: self.variable,
            patterns: self.patterns,
            faces: OnceLock::new(),
        }))
    }
}

#[derive(Clone)]
pub struct FontFamily(Arc<FamilyInner>);

struct FamilyInner {
    fontmap: Weak<FontMapShared>,
    name: String,
    spacing: i32,
    variable: bool,
    patterns: Vec<Pattern>,
    faces: OnceLock<Vec<FontFace>>,
}

#[derive(Clone)]
pub struct FontFace(Arc<FaceInner>);

struct FaceInner {
    family: Weak<FamilyInner>,
    style: String,
    pattern: Option<Pattern>,
    fake: bool,
    regular: bool,
}

impl core::fmt::Debug for FontFamily {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "FontFamily({:?})", self.0.name)
    }
}

impl core::fmt::Debug for FontFace {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "FontFace({:?})", self.0.style)
    }
}

fn face_order_of(pattern: Option<&Pattern>) -> (i32, i32) {
    let weight = pattern
        .and_then(|p| p.get_integer(FC_WEIGHT, 0))
        .unwrap_or(FC_WEIGHT_MEDIUM);
    let slant = pattern
        .and_then(|p| p.get_integer(FC_SLANT, 0))
        .unwrap_or(FC_SLANT_ROMAN);
    (slant, weight)
}

struct FaceSpec {
    style: String,
    pattern: Option<Pattern>,
    fake: bool,
    regular: bool,
}

impl FaceSpec {
    fn new(style: &str, pattern: Option<Pattern>, fake: bool) -> FaceSpec {
        FaceSpec {
            style: style.to_owned(),
            pattern,
            fake,
            regular: false,
        }
    }
}

impl FontFamily {
    pub fn name(&self) -> &str {
        &self.0.name
    }

    pub fn is_monospace(&self) -> bool {
        matches!(self.0.spacing, FC_MONO | FC_DUAL | FC_CHARCELL)
    }

    pub fn is_variable(&self) -> bool {
        self.0.variable
    }

    pub fn font_map(&self) -> Option<FontMap> {
        self.0.fontmap.upgrade().map(FontMap::from_shared)
    }

    fn faces(&self) -> &[FontFace] {
        self.0.faces.get_or_init(|| self.build_faces())
    }

    fn build_faces(&self) -> Vec<FontFace> {
        let mut specs: Vec<FaceSpec> = Vec::new();
        if is_alias_family(&self.0.name) {
            specs.push(FaceSpec::new("Regular", None, true));
            specs.push(FaceSpec::new("Bold", None, true));
            specs.push(FaceSpec::new("Italic", None, true));
            specs.push(FaceSpec::new("Bold Italic", None, true));
            specs[0].regular = true;
        } else {
            let mut has_regular = false;
            let mut has_italic = false;
            let mut has_bold = false;
            let mut has_bold_italic = false;
            let mut regular_weight = 0;
            let mut regular_idx: Option<usize> = None;
            for pattern in &self.0.patterns {
                let weight = pattern
                    .get_integer(FC_WEIGHT, 0)
                    .unwrap_or(FC_WEIGHT_MEDIUM);
                let slant = pattern.get_integer(FC_SLANT, 0).unwrap_or(FC_SLANT_ROMAN);
                if pattern.get_bool(FC_VARIABLE, 0).unwrap_or(false) {
                    continue;
                }
                let font_style = pattern.get_string(FC_STYLE, 0);
                if font_style.as_deref() == Some("Regular") {
                    regular_weight = FC_WEIGHT_MEDIUM;
                    regular_idx = Some(specs.len());
                }
                let style = if weight <= FC_WEIGHT_MEDIUM {
                    if slant == FC_SLANT_ROMAN {
                        has_regular = true;
                        if weight > regular_weight {
                            regular_weight = weight;
                            regular_idx = Some(specs.len());
                        }
                        "Regular"
                    } else {
                        has_italic = true;
                        "Italic"
                    }
                } else if slant == FC_SLANT_ROMAN {
                    has_bold = true;
                    "Bold"
                } else {
                    has_bold_italic = true;
                    "Bold Italic"
                };
                let name = font_style.unwrap_or_else(|| style.to_owned());
                specs.push(FaceSpec::new(&name, Some(pattern.clone()), false));
            }
            if has_regular {
                if !has_italic {
                    specs.push(FaceSpec::new("Italic", None, true));
                }
                if !has_bold {
                    specs.push(FaceSpec::new("Bold", None, true));
                }
            }
            if (has_regular || has_italic || has_bold) && !has_bold_italic {
                specs.push(FaceSpec::new("Bold Italic", None, true));
            }
            if let Some(index) = regular_idx {
                specs[index].regular = true;
            }
            let mut order: Vec<FaceOrder> = specs
                .iter()
                .enumerate()
                .map(|(index, spec)| {
                    let (slant, weight) = face_order_of(spec.pattern.as_ref());
                    FaceOrder {
                        slant,
                        weight,
                        index: index as u32,
                    }
                })
                .collect();
            fontconfig::sort_faces_like_qsort(&mut order);
            let mut slots: Vec<Option<FaceSpec>> = specs.into_iter().map(Some).collect();
            specs = order
                .iter()
                .filter_map(|entry| slots[entry.index as usize].take())
                .collect();
        }
        let family = Arc::downgrade(&self.0);
        specs
            .into_iter()
            .map(|spec| {
                FontFace(Arc::new(FaceInner {
                    family: family.clone(),
                    style: spec.style,
                    pattern: spec.pattern,
                    fake: spec.fake,
                    regular: spec.regular,
                }))
            })
            .collect()
    }

    pub fn list_faces(&self) -> Vec<FontFace> {
        if self.0.fontmap.strong_count() == 0 {
            return Vec::new();
        }
        self.faces().to_vec()
    }

    pub fn face(&self, name: Option<&str>) -> Option<FontFace> {
        self.faces()
            .iter()
            .find(|face| match name {
                Some(name) => face.face_name() == name,
                None => face.0.regular,
            })
            .cloned()
    }
}

impl FontFace {
    pub fn face_name(&self) -> &str {
        &self.0.style
    }

    pub fn is_synthesized(&self) -> bool {
        self.0.fake
    }

    pub fn family(&self) -> Option<FontFamily> {
        self.0.family.upgrade().map(FontFamily)
    }

    pub fn describe(&self) -> FontDescription {
        let Some(family) = self.0.family.upgrade() else {
            return FontDescription::new();
        };
        if self.0.fake {
            let (bold, italic) = match self.0.style.as_str() {
                "Regular" => (false, false),
                "Bold" => (true, false),
                "Italic" => (false, true),
                _ => (true, true),
            };
            let mut desc = FontDescription::new();
            desc.set_family(Some(&family.name));
            desc.set_style(if italic { Style::Italic } else { Style::Normal });
            desc.set_weight(if bold { Weight::BOLD } else { Weight::NORMAL });
            return desc;
        }
        match self.0.pattern.as_ref() {
            Some(pattern) => font_description_from_pattern(pattern, false),
            None => FontDescription::new(),
        }
    }

    pub fn list_sizes(&self) -> Vec<i32> {
        let Some(family) = self.0.family.upgrade() else {
            return Vec::new();
        };
        let Some(fontmap) = family.fontmap.upgrade().map(FontMap::from_shared) else {
            return Vec::new();
        };
        let mut pattern = PatternBuilder::new();
        pattern.add_string(FC_FAMILY, &family.name);
        pattern.add_string(FC_STYLE, &self.0.style);
        let pattern = pattern.finish();
        let fonts = fontmap.config_fonts();
        let Some(listed) = fonts.list(&pattern, &[FC_PIXEL_SIZE]) else {
            return Vec::new();
        };
        let mut sizes = Vec::new();
        let mut dpi = -1.0;
        for font in listed.patterns() {
            let mut j = 0;
            while let Some(size) = font.get_double(FC_PIXEL_SIZE, j) {
                if dpi < 0.0 {
                    dpi = fontmap.resolution_for(None);
                }
                sizes.push(c_double_to_int(f64::from(SCALE) * size * 72.0 / dpi));
                j += 1;
            }
        }
        sizes.sort_unstable();
        sizes
    }
}
