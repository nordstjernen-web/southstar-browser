# Pure-Rust shaping and fonts

Status: step 1 (shaping) is implemented as an option: `-Drust-shaper=true`
(cargo feature `rust-shaper` on `southstar-text`) shapes with HarfRust, and
`NS_TEXT_SHAPER=harfbuzz` switches back at runtime. On Windows it matched
HarfBuzz exactly in 97% of a 22,000-case corpus, with no glyph or cluster
differences; the rest are small positioning differences in fallback spaces,
variable fonts, fallback mark placement and vertical origins. The bidi half
of step 4 is done: `rust/bidi` replaced FriBiDi (see "Bidi" below). The
other steps are not started.

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
| WOFF2 | existing `rust/font` decoder plus a Brotli decoder | Already Rust except Brotli. |
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

## Decision

Step 1 is available behind `-Drust-shaper=true`, and step 4 is done. Next,
in order of risk removed per unit of work: `skrifa` for metrics and
outlines of web fonts (steps 2 and 5), then color glyphs (step 3).
