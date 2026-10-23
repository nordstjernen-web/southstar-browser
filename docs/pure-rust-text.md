# Pure-Rust shaping and fonts

Status: proposal. Nothing here is implemented yet.

This note describes a possible later step for Southstar's text stack:
replacing the C libraries under text layout — HarfBuzz, FreeType,
fontconfig, FriBiDi — with Rust crates, so that untrusted font files and
page text are handled by memory-safe code all the way down.

It follows the port of web-text-lib (the Pango fork in
`third_party/web-text-lib`) to the Rust crate `rust/text`. That port is
faithful: it keeps Pango's algorithms and keeps calling the same C
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
| Bidirectional text | FriBiDi | C |
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
| Bidi | own port of FriBiDi's UAX #9 path, or `unicode-bidi` | `unicode-bidi` comes from the Servo project and is excluded by D1, so the realistic path is an in-tree implementation tested against FriBiDi. |
| WOFF2 | existing `rust/font` decoder plus a Brotli decoder | Already Rust except Brotli. |
| System fonts | fontconfig (keep) or `fontique` | Enumeration and fallback have no parsing risk worth the churn; fontconfig can stay on Linux/BSD. |

Unicode property data (line break classes, scripts, general category)
currently comes from GLib. It can move to generated tables in the style of
`rust/url/src/tables.rs` (from the Unicode Character Database), which also
removes the dependency on the system GLib's Unicode version.

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
   verified against FriBiDi and GLib.
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

## Decision

Not scheduled. Revisit once the faithful port to `rust/text` has landed
and replaced web-text-lib; it is the base this change builds on.
