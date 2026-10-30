# southstar-text

The text layout engine of [Southstar](https://github.com/nordstjernen-web/southstar-browser):
a Rust port of web-text-lib, the Pango fork Southstar used to build from C.

It keeps Pango's structure and results:
- **attribute lists**
- **line breaking:** UAX #14, plus the Thai, Arabic and Indic tailorings
- **itemization** by script, font and bidi level
- **fonts:** a fontconfig font map with FreeType and cairo fonts
- **shaping** through HarfBuzz
- **paragraph layout** with ellipsizing and justification
- **a cairo renderer**

The port was checked against the C library case by case and is
pixel-identical on Southstar's render tests. There is no GObject: `Layout`,
`Font`, `FontMap` and `Context` are plain Rust types. Nothing collides with
the system Pango that GTK loads in the same process.

**Caches.** Pango keeps no cache that outlives a layout, so this crate adds
process-wide ones: shaped glyph strings, itemizations, line breaks and font
metrics. A run is cached only when its shaping can't depend on the text
around it.

**Web fonts in pure Rust.** With the `rust-shaper` feature (the default),
downloaded `@font-face` fonts never reach FreeType, HarfBuzz or cairo-ft:
- [HarfRust](https://crates.io/crates/harfrust) shapes them;
- [read-fonts](https://crates.io/crates/read-fonts) answers table queries;
- [skrifa](https://crates.io/crates/skrifa) draws outlines and color glyphs
  (COLR, CBDT, sbix).

Bidi and Unicode data come from
[southstar-unicode](https://crates.io/crates/southstar-unicode).

License: `LGPL-2.0-or-later`, because it is a translation of LGPL code.
