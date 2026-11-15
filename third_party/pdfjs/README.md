# pdf.js for Southstar

Mozilla's [pdf.js](https://github.com/mozilla/pdf.js) renders the PDF documents
Southstar opens. This directory holds files from the prebuilt npm package
pdfjs-dist 6.4.299, unmodified, as `scripts/update-pdfjs.py` extracts them:

- `build/pdf.min.mjs` and `build/pdf.worker.min.mjs` — the library and its
  worker, from the package's `legacy/build` (the build that avoids the newest
  JavaScript and web platform features);
- `cmaps/` — the CMaps for CJK and other CID-keyed fonts;
- `standard_fonts/` — the standard 14 font substitutes (Foxit and Liberation);
- `wasm/` — the OpenJPEG, JBIG2 and QCMS WebAssembly decoders;
- `iccs/` — the CMYK ICC profile;
- `LICENSE` — pdf.js's Apache License 2.0. The fonts, decoders and profile
  carry their own licence files next to them.

Not taken from the package: the modern `build/`, source maps, type
declarations, the scripting sandbox and its QuickJS build (PDF JavaScript is
never run), the image decoders' non-WebAssembly fallbacks and the `web/`
viewer components. Southstar's own viewer page around the library is
`rust/pdf-viewer`.

This crate is the one bundled Mozilla project in Southstar. Its files are not
edited: update them with `python3 scripts/update-pdfjs.py VERSION`, which
checks the tarball against the registry's integrity hash and regenerates
`src/files.rs`. `src/lib.rs` is Southstar's, and only looks the files up.
