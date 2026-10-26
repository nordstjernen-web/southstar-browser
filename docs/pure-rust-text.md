# Pure-Rust shaping and fonts

Status: in a `-Drust-shaper=true` build (cargo feature `rust-shaper` on
`southstar-text`) a font can take the Rust path, chosen by
`NS_RUST_FONTS=off|web|all` (`web` = fonts loaded through `@font-face`;
`NS_TEXT_SHAPER=harfbuzz` means off). A Rust-backed font is shaped by HarfRust,
answers layout's table queries (glyph classes, ligature carets, OpenType
metrics, font extents, GSUB feature support) from `read-fonts` exactly as
HarfBuzz 14.2 does, and is drawn from `skrifa` outlines through a cairo user
font (see "Drawing" below), so neither FreeType nor HarfBuzz reads it at render
time. HarfRust matched HarfBuzz exactly in 97% of a 22,000-case corpus, with
no glyph or cluster differences. Step 4 is done: `rust/bidi` replaced
FriBiDi and `rust/unicode` GLib's Unicode data. Color glyphs (COLR v0 and
v1, CBDT and sbix) are painted in Rust too (see "Color glyphs"). Indexing web
fonts without FreeType is not done yet.

This note describes a possible later step for Southstar's text stack:
replacing the C libraries under text layout — HarfBuzz, FreeType,
fontconfig, FriBiDi — with Rust crates, so that untrusted font files and
page text are handled by memory-safe code all the way down.

It follows the port of web-text-lib (the Pango fork Southstar used to
build from `third_party/web-text-lib`) to the Rust crate `rust/text`. That
port, now done, is faithful: it keeps Pango's algorithms and keeps calling the same C
libraries, so pages render the same. This proposal changes what sits
underneath.

## Why

Porting Pango's own logic (attributes, line breaking, itemization,
paragraph layout, caches) to Rust makes that logic memory-safe. The code
that parses hostile input is elsewhere:

| Input | Parsed today by | Language |
|---|---|---|
| `@font-face` web fonts (TrueType/OpenType tables, after WOFF/WOFF2 decoding) | FreeType, HarfBuzz | C, C++ |
| Glyph outlines, hinting bytecode, color glyph tables (COLR, CBDT, sbix, SVG) | FreeType, cairo | C |
| Shaping tables (GSUB, GPOS, GDEF, morx, kerx, AAT) | HarfBuzz | C++ |
| Bidirectional text | `rust/bidi` (was FriBiDi) | Rust |
| System font configuration and fallback | fontconfig | C |

Web fonts are attacker-controlled binary files. FreeType and HarfBuzz are
heavily fuzzed and well maintained, but they remain the largest memory-unsafe
surface that a page can reach through text. Moving them to Rust is the step
that removes that surface; the faithful Pango port alone does not.

## Candidate crates

All of these are outside the Servo/Firefox exclusion (D1) and carry
licences `deny.toml` already accepts (MIT, Apache-2.0, or both). Each new
crate still needs the usual `cargo vet` record and a reason in its commit.

| Job | Candidates | Notes |
|---|---|---|
| Font table parsing | `read-fonts` / `skrifa` (Google's fontations) | Used by Chrome for font loading; zero-copy table access, glyph outlines, metrics, hinting (TrueType interpreter and autohinter), variable fonts. |
| Shaping | `harfrust` (the HarfBuzz project's Rust port) or `rustybuzz` | Both are ports of HarfBuzz's shaper. HarfRust sits on `read-fonts`, so one parser serves shaping and outlines. |
| Rasterization | cairo (draw `skrifa` outlines as paths) or `swash`'s scaler / `zeno` | Keeping cairo for painting limits the change to outline extraction; color glyphs need COLR v0/v1 and bitmap (CBDT/sbix) handling. |
| Bidi | in-tree `rust/bidi` (done) | `unicode-bidi` comes from the Servo project and is excluded by D1, so it is an in-tree implementation written from UAX #9 and tested against Unicode's reference implementation and FriBiDi. |
| WOFF, WOFF2 | `rust/woff2` (done) | WOFF 1.0 is decoded in Rust (zlib through `miniz_oxide`), WOFF2 in Rust except the Brotli stream (libbrotlidec). FreeType no longer reads the containers. |
| System fonts | fontconfig (keep) or `fontique` | Enumeration and fallback have no parsing risk worth the churn; fontconfig can stay on Linux/BSD. |

Unicode property data (line break classes, scripts, general category) came
from GLib. It now comes from `rust/unicode`, generated tables in the style of
`rust/url/src/tables.rs` (from the Unicode Character Database), which also
removed the dependency on the system GLib's Unicode version.

## Shape of the change

The Rust `rust/text` crate keeps Pango's structure: a font map hands out
fonts, a font answers coverage, metrics and glyph extents, and the shaper
turns an item into a glyph string. The proposal replaces the implementations
behind those seams, one at a time:

1. **Shaping.** Replace `hb_shape` with HarfRust on the same font bytes.
   Fonts are still opened by FreeType for outlines. Compare glyph ids,
   advances and offsets against HarfBuzz over a corpus of fonts and scripts.
2. **Metrics and outlines.** Replace FreeType with `skrifa` for metrics,
   glyph extents and outlines, painting the outlines through cairo paths
   (or a glyph cache of rasterized masks). Hinting is the main source of
   visible differences.
3. **Color glyphs.** COLR v0/v1 and bitmap emoji through `skrifa`, painted
   with cairo. Needed before FreeType can leave the renderer.
4. **Bidi and Unicode data.** In-tree UAX #9 and generated property tables,
   verified against FriBiDi and GLib. Done: `rust/bidi` and
   `rust/unicode` (see "Bidi" and "Unicode properties" below).
5. **Web fonts first, system fonts later.** Untrusted `@font-face` fonts can
   switch to the Rust path before system fonts do, since they carry the
   risk; system fonts keep the C path until the Rust one matches it.

Each step is a pull request with a differential check against the C
library it replaces, plus old-vs-new screenshots of the render tests.

## Risks and costs

- **Rendering changes.** Different hinting and anti-aliasing will move
  pixels on every page. Glyph positions should match HarfBuzz exactly for
  OpenType; AAT and some edge cases may not.
- **Coverage.** HarfRust and `skrifa` are newer than HarfBuzz and FreeType.
  Gaps (rare scripts, AAT fonts, Type 1/CFF quirks) would need fixing
  upstream or keeping the C path as a fallback.
- **Performance.** Shaping and outline speed are close to the C libraries
  in published comparisons, but the glyph caches built around cairo and
  FreeType would need rework.
- **Dependencies.** Several new crates.io crates in the font path, each
  needing a vet record and a reason.
- **Platforms.** macOS and Windows font lookup still go through fontconfig
  in this build; moving off it is a separate decision.

## Bidi

`rust/bidi` (`southstar-bidi`) replaced FriBiDi. It implements UAX #9 from
the specification: P2–P3, X1–X10, W1–W7, N0 with the BD16 bracket pairs,
N1–N2, I1–I2 and L1, in safe Rust with no dependencies. Its tables
(Bidi_Class with the `@missing` defaults, paired brackets folded through
canonical singletons, Bidi_Mirroring_Glyph) are generated from the Unicode
18.0 UCD by `scripts/gen-bidi-tables.py`.

Verification:

- **Conformance.** All 770,241 BidiTest.txt and 91,707
  BidiCharacterTest.txt cases pass (`SOUTHSTAR_UCD_DIR` enables them; the
  Linux CI downloads both files).
- **Reference implementation.** On 5,000,000 random paragraphs (all 23
  classes, brackets including canonical and compatibility forms, nesting
  past the 125-level limit, all four paragraph directions) the levels match
  Unicode's C reference implementation (BidiReferenceC 18.0.0) everywhere
  it reports a level. Where an isolate initiator overflows the depth limit
  the specification's BD13 is silent; `rust/bidi` follows the reference
  there, ending a level run at every isolate initiator.
- **FriBiDi.** FriBiDi 1.0.16 disagreed with the reference on 36,075 of
  those paragraphs, about half of them in bracket pairs. It also pairs
  compatibility brackets (U+FF08, U+FE59, …) with their ASCII forms,
  ignores embeddings after a mid-text paragraph separator, and its tables
  stop at Unicode 16. Characters removed by X9 take the level of the
  character before them, as FriBiDi does, so itemization is unchanged.
- **Rendering.** The 53 render-test fixtures, a bidi stress page and nine
  Hebrew, Arabic, Persian, Urdu and English Wikipedia articles render
  pixel-identically with FriBiDi and with `rust/bidi`, apart from content
  that also differs between two runs of the same binary.

## Unicode properties

`rust/unicode` (`southstar-unicode`) replaced GLib's Unicode database under
text layout: general category, line break class, script (in GLib's
GUnicodeScript numbering, which `Script` stores, with newer scripts
appended), East Asian width, zero width, combining class, simple and
special case mappings, and single-step canonical decomposition. The tables
come from the UCD through `scripts/gen-unicode-tables.py`.

They are generated from Unicode 17.0, the version GLib 2.88 carries, and an
exhaustive comparison against GLib (every code point, every function
`rust/text` calls) differs only in line break classes of unassigned code
points from U+33500 to U+3FFFD, where GLib's table ends and the UCD's `ID`
default now applies. Case conversion of strings ignores the process
locale; GLib applied Turkic and Lithuanian rules when the operating system
used those languages. Moving to Unicode 18 changes the line break class of
soft hyphen (BA to HH) and of figure and en dashes (HH to BA), so it should
land together with the UAX #14 rules of that version.

The other crates that called GLib's Unicode functions (`text-transform` in
layout and `innerText`, case folding for `<select>` and `window.find()`,
`:dir()`, MathML, history search and the shell's input filter) use
`rust/unicode` too, which added CaseFolding.txt and the alphabetic, control
and printable tests, each checked against GLib over every code point.

## Web fonts

Untrusted `@font-face` bytes reached C in four places: FreeType read the WOFF
and WOFF2 containers in `rust/font`, `FcConfigAppFontAddFile` and
`FcFreeTypeQueryAll` parsed the font to index it, and cairo-ft (FreeType)
and HarfBuzz parse it again to draw and shape it. The order of work:

1. **Containers (done).** WOFF 1.0 is decoded by `rust/woff2` and WOFF2 only
   by the Rust decoder. Over 487 WOFF files made from the Windows fonts and
   110 WOFF files from Google Fonts the SFNT output is byte-identical to the
   FreeType path; over 358 WOFF2 files from Google Fonts every glyph
   (outline, on-curve flags, bounding box, overlap bit, instructions,
   components) and every other table matches fontTools' decoder, and 120 of
   those fonts render pixel-identically in the browser. The FreeType path
   produced `loca` tables pointing past the end of `glyf` in 355 of the 358
   (it truncated FreeType's own reconstruction to the original table
   length); FreeType tolerated it.
2. **Index.** Build the fontconfig patterns in Rust from `read-fonts`
   (charset from `cmap`, languages through `FcLangGetCharSet`) instead of
   `FcConfigAppFontAddFile` and `FcFreeTypeQueryAll`, checked field by field
   against fontconfig's own patterns.
3. **Draw.** Web fonts get a Rust backend in `rust/text`: `skrifa` metrics and
   outlines drawn through a cairo user font (the hex-box path already uses
   one) and HarfRust shaping, first behind a switch for old/new screenshots.

## Drawing

`rust/text/src/rust_glyphs.rs` loads a Rust-backed font's outlines with
`skrifa` the way cairo-ft loads them through FreeType, and cairo paints them as
paths through a user font:

- Hinting follows the cairo font options and the fontconfig pattern
  (`hinting`, `hintstyle`, `antialias`, `autohint`) as cairo-ft merges them.
  Slight hinting of TrueType outlines uses the automatic hinter, as FreeType's
  light mode does; CFF uses its own hinter, and fonts without hints the
  automatic one.
- Synthetic bold (`embolden`) is a port of `FT_Outline_EmboldenXY` with
  FreeType's strength (ppem × 2730 / 1024 / 64 pixels) and metric adjustments.
- With metrics hinting off, as the browser lays out, glyph extents are those
  cairo-ft reports: the control box of the outline at the em size, grid-fitted
  when hinting is on, and the linear advance. Over every installed face at
  three sizes (901,896 glyphs) 96.1% of the extents equal cairo-ft's exactly;
  the rest differ by about one font unit, apart from color glyphs.
- Subpixel positioning is cairo's own (four phases), the same as for cairo-ft
  glyphs; rasterization is cairo's, so edges differ from FreeType's by a few
  levels of coverage.

On the render tests with every font on the Rust path the pixel differences
are anti-aliasing only; on a page of Google web fonts, GOV.UK and MDN they
are under 0.3% of the pixels and invisible at normal size.

## Color glyphs

`rust/text/src/rust_color.rs` paints color glyphs of Rust-backed fonts from
the cairo user font's color callback:

- COLR v0 and v1 through `skrifa`'s paint traversal: transforms, glyph and
  box clips, solid fills from CPAL palette 0 or the text color (cairo's
  foreground marker, so cached glyphs follow the color), linear and radial
  gradients as cairo patterns (radial color lines truncated where a radius
  would turn negative), sweep gradients as mesh patterns of 2° slices, and
  layers composited with the cairo operator of each composite mode. A paint
  graph `skrifa` rejects (a cycle, too deep) leaves what was painted and
  balances the cairo state.
- CBDT and sbix PNG strikes decoded by the `png` crate (already a dependency
  of `rust/raster`) under a 4096-pixel side limit, and uncompressed BGRA
  strikes, placed by their bearings and scaled from the strike's ppem.
- A glyph that is not a color glyph, or whose bitmap does not decode, falls
  back to its outline. SVG-in-OpenType glyphs are drawn from their outlines.

Noto Color Emoji (CBDT and COLR v1), Segoe UI Emoji and the COLR v1 and sbix
test fonts of googlefonts/color-fonts render within anti-aliasing of
cairo-ft. The COLR v1 test glyphs that crashed the renderer with a stack
overflow in the cairo-ft path render on the Rust path.

## Decision

Step 1 is available behind `-Drust-shaper=true`, and step 4 is done. Next,
in order of risk removed per unit of work: `skrifa` for metrics and
outlines of web fonts (steps 2 and 5), then color glyphs (step 3).
