# southstar-unicode

Unicode data and algorithms for text layout, in safe Rust with no
dependencies. Used by [Southstar](https://github.com/nordstjernen-web/southstar-browser)'s
text engine.

**`unicode`**: the character properties line breaking, itemization and
shaping need:
- general category
- line break class (UAX #14)
- script
- East Asian width
- canonical combining class
- case mapping
- decomposition

It follows GLib's semantics and script numbering, so it can stand in for
the GLib calls Pango-derived code makes.

**`bidi`**: the Unicode Bidirectional Algorithm (UAX #9), written from the
specification:
- bidi classes, paired brackets and mirrored glyphs
- a paragraph's embedding levels

It passes all of `BidiTest.txt` and `BidiCharacterTest.txt` from the Unicode
Character Database.

Both modules' tables are generated from the UCD by scripts in the repository,
and are regenerated for a new Unicode version rather than edited by hand.

Part of the browser that `cargo install southstar` builds. The API follows
the browser's needs and is not stable.

License: `LicenseRef-NSL-1.0 OR GPL-3.0-or-later`.
