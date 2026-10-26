//! Southstar text — HarfBuzz's font-table answers read through read-fonts: glyph classes, ligature carets, OpenType metrics, font extents and GSUB feature support.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use harfrust::{Language, Script, Tag};
use read_fonts::TableProvider;
use read_fonts::tables::gdef::CaretValue;
use read_fonts::tables::layout::DeviceOrVariationIndex;
use read_fonts::tables::variations::{DeltaSetIndex, ItemVariationStore};
use read_fonts::types::GlyphId16;

use crate::ffi::harfbuzz::FontExtents;
use crate::rust_shaper::{ShaperFace, hb_roundf};
use crate::types::Glyph;

const GDEF_BLOCKLIST: [(usize, usize, usize); 39] = [
    (442, 2874, 42038),
    (430, 2874, 40662),
    (442, 2874, 39116),
    (430, 2874, 39374),
    (490, 3046, 41638),
    (478, 3046, 41902),
    (898, 12554, 46470),
    (910, 12566, 47732),
    (928, 23298, 59332),
    (940, 23310, 60732),
    (964, 23836, 60072),
    (976, 23832, 61456),
    (994, 24474, 60336),
    (1006, 24470, 61740),
    (1006, 24576, 61346),
    (1018, 24572, 62828),
    (1006, 24576, 61352),
    (1018, 24572, 62834),
    (832, 7324, 47162),
    (844, 7302, 45474),
    (180, 13054, 7254),
    (192, 12638, 7254),
    (192, 12690, 7254),
    (188, 248, 3852),
    (188, 264, 3426),
    (1058, 47032, 11818),
    (1046, 47030, 12600),
    (1058, 71796, 16770),
    (1046, 71790, 17862),
    (1046, 71788, 17112),
    (1058, 71794, 17514),
    (1330, 109904, 57938),
    (1330, 109904, 58972),
    (1004, 59092, 14836),
    (588, 5078, 14418),
    (588, 5078, 14238),
    (894, 17162, 33960),
    (894, 17154, 34472),
    (816, 7868, 17052),
];

const NO_SCRIPT_INDEX: u16 = 0xFFFF;
const DEFAULT_LANGUAGE_INDEX: u16 = 0xFFFF;
const MAX_TAGS: usize = 3;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Axis {
    X,
    Y,
}

fn tag(name: &[u8; 4]) -> Tag {
    Tag::new(name)
}

fn em_mult(value: i16, mult: i64) -> i32 {
    ((i64::from(value) * mult + 32768) >> 16) as i32
}

fn fixed_mult(scale: i32, upem: u16) -> i64 {
    let upem = i64::from(upem.max(1));
    if scale < 0 {
        -((-i64::from(scale)) << 16) / upem
    } else {
        (i64::from(scale) << 16) / upem
    }
}

fn fix_ascender_descender(value: f32, metric: &[u8; 4]) -> f32 {
    match metric {
        b"hasc" | b"vasc" => f64::from(value).abs() as f32,
        b"hdsc" | b"vdsc" => -(f64::from(value).abs() as f32),
        _ => value,
    }
}

fn axis_factor(coord: i32, start: i32, peak: i32, end: i32) -> f32 {
    if peak == 0 || coord == peak || start > peak || peak > end || (start < 0 && end > 0) {
        return 1.;
    }
    if coord <= start || end <= coord {
        0.
    } else if coord < peak {
        (coord - start) as f32 / (peak - start) as f32
    } else {
        (end - coord) as f32 / (end - peak) as f32
    }
}

fn float_delta(
    store: &ItemVariationStore<'_>,
    index: DeltaSetIndex,
    coords: &[read_fonts::types::F2Dot14],
) -> f32 {
    if coords.is_empty() {
        return 0.;
    }
    let Some(Ok(data)) = store.item_variation_data().get(usize::from(index.outer)) else {
        return 0.;
    };
    let Ok(region_list) = store.variation_region_list() else {
        return 0.;
    };
    let regions = region_list.variation_regions();
    let mut delta = 0f32;
    for (region_index, value) in data
        .region_indexes()
        .iter()
        .zip(data.delta_set(index.inner))
    {
        let Ok(region) = regions.get(usize::from(region_index.get())) else {
            continue;
        };
        let mut scalar = 1f32;
        for (axis, coordinates) in region.region_axes().iter().enumerate() {
            let coord = coords.get(axis).map_or(0, |c| i32::from(c.to_bits()));
            let factor = axis_factor(
                coord,
                i32::from(coordinates.start_coord().to_bits()),
                i32::from(coordinates.peak_coord().to_bits()),
                i32::from(coordinates.end_coord().to_bits()),
            );
            if factor == 0. {
                scalar = 0.;
                break;
            }
            scalar *= factor;
        }
        delta += scalar * value as f32;
    }
    delta
}

impl ShaperFace {
    fn upem(&self) -> u16 {
        self.font().units_per_em()
    }

    fn scale(&self, axis: Axis) -> i32 {
        match axis {
            Axis::X => self.x_scale,
            Axis::Y => self.y_scale,
        }
    }

    fn em_scale(&self, value: i16, axis: Axis) -> i32 {
        em_mult(value, fixed_mult(self.scale(axis), self.upem()))
    }

    fn em_scalef(&self, value: f32, axis: Axis) -> i32 {
        let mult = self.scale(axis) as f32 / f32::from(self.upem());
        hb_roundf(value * mult)
    }

    fn mvar_delta(&self, metric: &[u8; 4]) -> f32 {
        let coords = self.font().normalized_coords();
        if coords.is_empty() {
            return 0.;
        }
        let Ok(mvar) = self.font().tables().mvar() else {
            return 0.;
        };
        let records = mvar.value_records();
        let Ok(found) = records.binary_search_by(|record| record.value_tag().cmp(&tag(metric)))
        else {
            return 0.;
        };
        let record = &records[found];
        mvar.item_variation_store()
            .and_then(Result::ok)
            .map_or(0., |store| {
                float_delta(
                    &store,
                    DeltaSetIndex {
                        outer: record.delta_set_outer_index(),
                        inner: record.delta_set_inner_index(),
                    },
                    coords,
                )
            })
    }

    fn gdef_usable(&self) -> Option<read_fonts::tables::gdef::Gdef<'_>> {
        let tables = self.font().tables();
        let gdef = tables.gdef().ok()?;
        let gsub_len = tables.gsub().map_or(0, |t| t.offset_data().len());
        let gpos_len = tables.gpos().map_or(0, |t| t.offset_data().len());
        let lengths = (gdef.offset_data().len(), gsub_len, gpos_len);
        (!GDEF_BLOCKLIST.contains(&lengths)).then_some(gdef)
    }

    pub(crate) fn glyph_class(&self, glyph: Glyph) -> i32 {
        let Ok(gid) = u16::try_from(glyph) else {
            return 0;
        };
        self.gdef_usable()
            .and_then(|gdef| gdef.glyph_class_def())
            .and_then(Result::ok)
            .map_or(0, |classes| classes.get(GlyphId16::new(gid)) as i32)
    }

    pub(crate) fn ligature_caret(
        &self,
        horizontal: bool,
        glyph: Glyph,
        start_offset: u32,
    ) -> (u32, u32, i32) {
        let axis = if horizontal { Axis::X } else { Axis::Y };
        let Some(gdef) = self.gdef_usable() else {
            return (0, 0, 0);
        };
        let Some(Ok(list)) = gdef.lig_caret_list() else {
            return (0, 0, 0);
        };
        let Some(index) = u16::try_from(glyph)
            .ok()
            .and_then(|gid| list.coverage().ok()?.get(GlyphId16::new(gid)))
        else {
            return (0, 0, 0);
        };
        let Ok(lig_glyph) = list.lig_glyphs().get(index as usize) else {
            return (0, 0, 0);
        };
        let carets = lig_glyph.caret_values();
        let total = carets.len() as u32;
        let Some(caret) = (start_offset < total)
            .then(|| carets.get(start_offset as usize).ok())
            .flatten()
        else {
            return (total, 0, 0);
        };
        let value = match caret {
            CaretValue::Format1(format) => self.em_scale(format.coordinate(), axis),
            CaretValue::Format2(_) => 0,
            CaretValue::Format3(format) => {
                let delta = match format.device() {
                    Ok(DeviceOrVariationIndex::VariationIndex(index)) => self.variation_delta(
                        &gdef,
                        DeltaSetIndex {
                            outer: index.delta_set_outer_index(),
                            inner: index.delta_set_inner_index(),
                        },
                        axis,
                    ),
                    _ => 0,
                };
                self.em_scale(format.coordinate(), axis) + delta
            }
        };
        (total, 1, value)
    }

    fn variation_delta(
        &self,
        gdef: &read_fonts::tables::gdef::Gdef<'_>,
        index: DeltaSetIndex,
        axis: Axis,
    ) -> i32 {
        let coords = self.font().normalized_coords();
        if coords.is_empty() {
            return 0;
        }
        gdef.item_var_store()
            .and_then(Result::ok)
            .map_or(0, |store| {
                self.em_scalef(float_delta(&store, index, coords), axis)
            })
    }

    fn metric_value(&self, metric: &[u8; 4], value: i16, axis: Axis) -> i32 {
        let raw = f32::from(value) + self.mvar_delta(metric);
        self.em_scalef(fix_ascender_descender(raw, metric), axis)
    }

    fn horizontal_metric(&self, metric: &[u8; 4]) -> Option<i32> {
        let tables = self.font().tables();
        let os2 = tables.os2().ok().filter(|os2| {
            os2.us_weight_class() != 0
                || os2.us_width_class() != 0
                || os2.us_first_char_index() != 0
                || os2.us_last_char_index() != 0
        });
        let hhea = tables.hhea().ok().filter(|hhea| hhea.version().major != 0);
        let typo = os2.as_ref().filter(|os2| {
            os2.fs_selection()
                .contains(read_fonts::tables::os2::SelectionFlags::USE_TYPO_METRICS)
        });
        let from_os2 = typo.map(|os2| match metric {
            b"hasc" => os2.s_typo_ascender(),
            b"hdsc" => os2.s_typo_descender(),
            _ => os2.s_typo_line_gap(),
        });
        let from_hhea = hhea.map(|hhea| match metric {
            b"hasc" => hhea.ascender().to_i16(),
            b"hdsc" => hhea.descender().to_i16(),
            _ => hhea.line_gap().to_i16(),
        });
        from_os2
            .or(from_hhea)
            .map(|value| self.metric_value(metric, value, Axis::Y))
    }

    pub(crate) fn ot_metric(&self, metric_tag: u32) -> Option<i32> {
        let metric = metric_tag.to_be_bytes();
        let tables = self.font().tables();
        let os2 = tables.os2().ok().filter(|os2| {
            os2.us_weight_class() != 0
                || os2.us_width_class() != 0
                || os2.us_first_char_index() != 0
                || os2.us_last_char_index() != 0
        });
        let scaled = |value: i16, axis: Axis| {
            let raw = f32::from(value) + self.mvar_delta(&metric);
            self.em_scalef(raw, axis)
        };
        match &metric {
            b"hasc" | b"hdsc" | b"hlgp" => self.horizontal_metric(&metric),
            b"xhgt" | b"cpht" => {
                let os2 = tables.os2().ok().filter(|os2| os2.version() >= 2)?;
                let (x_height, cap_height) = (os2.sx_height()?, os2.s_cap_height()?);
                (x_height != 0 || cap_height != 0).then(|| {
                    scaled(
                        if &metric == b"xhgt" {
                            x_height
                        } else {
                            cap_height
                        },
                        Axis::Y,
                    )
                })
            }
            b"sbxs" => os2.map(|os2| scaled(os2.y_subscript_x_size(), Axis::X)),
            b"sbys" => os2.map(|os2| scaled(os2.y_subscript_y_size(), Axis::Y)),
            b"sbxo" => os2.map(|os2| scaled(os2.y_subscript_x_offset(), Axis::X)),
            b"sbyo" => os2.map(|os2| scaled(os2.y_subscript_y_offset(), Axis::Y)),
            b"spxs" => os2.map(|os2| scaled(os2.y_superscript_x_size(), Axis::X)),
            b"spys" => os2.map(|os2| scaled(os2.y_superscript_y_size(), Axis::Y)),
            b"spxo" => os2.map(|os2| scaled(os2.y_superscript_x_offset(), Axis::X)),
            b"spyo" => os2.map(|os2| scaled(os2.y_superscript_y_offset(), Axis::Y)),
            b"hcla" => os2.map(|os2| scaled(os2.us_win_ascent() as i16, Axis::Y)),
            b"hcld" => os2.map(|os2| scaled(os2.us_win_descent() as i16, Axis::Y)),
            b"strs" => os2.map(|os2| scaled(os2.y_strikeout_size(), Axis::Y)),
            b"stro" => os2.map(|os2| scaled(os2.y_strikeout_position(), Axis::Y)),
            b"unds" | b"undo" => {
                let post = tables
                    .post()
                    .ok()
                    .filter(|post| post.version().to_major_minor() != (0, 0))?;
                let value = if &metric == b"unds" {
                    post.underline_thickness().to_i16()
                } else {
                    post.underline_position().to_i16()
                };
                Some(scaled(value, Axis::Y))
            }
            _ => None,
        }
    }

    fn font_h_extents(&self) -> Option<FontExtents> {
        let found = (
            self.horizontal_metric(b"hasc"),
            self.horizontal_metric(b"hdsc"),
            self.horizontal_metric(b"hlgp"),
        );
        match found {
            (Some(ascender), Some(descender), Some(line_gap)) => {
                let shift = if self.y_scale < 0 {
                    -self.y_strength
                } else {
                    self.y_strength
                };
                Some(FontExtents::new(ascender + shift, descender, line_gap))
            }
            _ => None,
        }
    }

    pub(crate) fn h_extents(&self) -> FontExtents {
        self.font_h_extents().unwrap_or_default()
    }

    pub(crate) fn extents_ltr(&self) -> FontExtents {
        if let Some(extents) = self.font_h_extents() {
            return extents;
        }
        let ascender = (self.y_scale as f64 * 0.8) as i32;
        FontExtents::new(ascender, ascender - self.y_scale, 0)
    }

    pub(crate) fn gsub_features_supported(
        &self,
        script_tag: u32,
        language: Option<&str>,
        features: &[u32],
    ) -> bool {
        let Ok(gsub) = self.font().tables().gsub() else {
            return features.is_empty();
        };
        let Ok(script_list) = gsub.script_list() else {
            return features.is_empty();
        };
        let Ok(feature_list) = gsub.feature_list() else {
            return features.is_empty();
        };
        let script = Script::from_iso15924_tag(Tag::from_u32(script_tag));
        let language = language.and_then(|lang| lang.parse::<Language>().ok());
        let (script_tags, language_tags) = Script::tags_for_language(script, language.as_ref());
        let records = script_list.script_records();
        let find_script = |wanted: Tag| {
            records
                .iter()
                .position(|record| record.script_tag() == wanted)
        };
        let script_index = script_tags
            .as_slice()
            .iter()
            .take(MAX_TAGS)
            .find_map(|&wanted| find_script(wanted))
            .or_else(|| find_script(tag(b"DFLT")))
            .or_else(|| find_script(tag(b"dflt")))
            .or_else(|| find_script(tag(b"latn")))
            .map_or(NO_SCRIPT_INDEX, |index| index as u16);
        if script_index == NO_SCRIPT_INDEX {
            return features.is_empty();
        }
        let Some(Ok(script)) = records
            .get(usize::from(script_index))
            .map(|record| record.script(script_list.offset_data()))
        else {
            return features.is_empty();
        };
        let lang_records = script.lang_sys_records();
        let find_language = |wanted: Tag| {
            lang_records
                .iter()
                .position(|record| record.lang_sys_tag() == wanted)
        };
        let language_index = language_tags
            .as_slice()
            .iter()
            .take(MAX_TAGS)
            .find_map(|&wanted| find_language(wanted))
            .or_else(|| find_language(tag(b"dflt")))
            .map_or(DEFAULT_LANGUAGE_INDEX, |index| index as u16);
        let lang_sys = if language_index == DEFAULT_LANGUAGE_INDEX {
            script.default_lang_sys().and_then(Result::ok)
        } else {
            lang_records
                .get(usize::from(language_index))
                .and_then(|record| record.lang_sys(script.offset_data()).ok())
        };
        let Some(lang_sys) = lang_sys else {
            return features.is_empty();
        };
        let feature_records = feature_list.feature_records();
        features.iter().all(|&feature| {
            lang_sys.feature_indices().iter().any(|index| {
                feature_records
                    .get(usize::from(index.get()))
                    .is_some_and(|record| record.feature_tag() == Tag::from_u32(feature))
            })
        })
    }
}
