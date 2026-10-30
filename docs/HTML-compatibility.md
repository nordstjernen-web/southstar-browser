# HTML compatibility

How Southstar tracks the **WHATWG HTML Living Standard**
(<https://html.spec.whatwg.org/>).

This is a section-by-section walk through the entire spec, from §1
*Introduction* to §16 *Obsolete features*, recording how Southstar
behaves against each. It is measured against the **spec text**, not
against any other browser — Southstar is a Rust / GTK 4
implementation, networking through its own HTTP client (`rust/http`; no
libcurl), with no upstream engine. It is a living map,
not a guarantee; the browser's runtime behaviour is the source of
truth. Re-check any row by running the browser against a page that
exercises the feature.

This document focuses on the HTML spec proper and summarises adjacent
CSS, DOM, networking, media, and security surfaces where the spec
references them.

Snapshot: **1.0.5-dev** (after the 1.0.4 release), 2026-10-30 (rev 36).

§1–§16 row tally (counted across the section tables below): **144 ✅
implemented · 31 🟡 partial · 0 ❌ absent · 7 🚫 absent by design** (the
mixed ✅/🟡 image-decode row counts as partial).

**Legend:** ✅ implemented · 🟡 partial / stubbed · ❌ absent ·
🚫 absent by design (a project non-goal — see
[Design constraints](#design-constraints-the-spec-we-deliberately-do-not-implement)).

**Engine map** (the files referenced throughout):

| Concern | Source |
|---------|--------|
| HTML tokenizer + tree construction | `rust/html5`, built into the DOM by `rust/html-parser` |
| Parse dispatch, charset decode | `rust/browser/src/open.rs`; charset decode in `rust/html-util` (`decode_body`) |
| DOM tree + node APIs | `rust/dom`, exposed to script by the `rust/js-*` binding crates (`rust/js-node`, `rust/js-document`, `rust/js-elements`, …) |
| WHATWG URL | `rust/url` (`southstar_url::bytes`, called directly by every crate) |
| CSS cascade / selectors | `rust/css` (with `rust/css-syntax`, `rust/css-media`, `rust/css-color`); computed styles read through `rust/style` |
| Layout (block/inline/flex/grid/table) | `rust/layout` (box tree), `rust/layout-build`, `rust/layout-block`, `rust/layout-inline`, `rust/layout-flex`, `rust/layout-grid`, `rust/layout-table`, `rust/layout-position`, `rust/layout-float`, `rust/layout-sizing` |
| Paint (cairo) / text | `rust/paint`; text layout and shaping in `rust/text` (the Rust port of the Pango fork) over `rust/bidi` and `rust/unicode`; web fonts in `rust/font` |
| JavaScript (Boa, interpreter) | `rust/js-*`, `rust/js-engine`, `third_party/boa_engine` |
| Networking | `rust/http` (HTTP/1.1, HTTP/2, rustls TLS) over `rust/http-wire`; redirects, HSTS, referrer and cookies in `rust/net` (cookie jar rules in `rust/netstore`); HTTP cache in `rust/http-cache` |
| Images / media | `rust/image`, `rust/image-decoders`, `rust/raster`, `rust/svg`, `rust/video`, `rust/plmpeg`, `rust/webvtt`, `rust/webaudio`, `rust/audio-helper`, `rust/video-helper` |
| Security (CSP/SOP/sandbox) | `rust/csp`; CORS in `rust/js-net/src/preflight.rs`; iframe sandbox in `rust/js-frames/src/sandbox.rs`; process sandbox in `rust/sandbox` |

---

## The renderer: out-of-process (IPC)

Southstar has a single renderer architecture. The GTK app
is a thin shell (`rust/shell`, tab view in `rust/shell/src/procview`) that spawns
one sandboxed `southstar-renderer` process per tab
(`rust/renderer-host`) and drive it over a control channel +
shared-memory framebuffer (`rust/renderer-client`). The engine
(`rust/css`, the `rust/layout*` crates, the `rust/js-*` crates, `rust/dom`,
`rust/paint`, `rust/net`, images) runs entirely inside the sandboxed child (Linux
Landlock + seccomp); the UI process only blits frames and forwards
input. `./southstar` (the entry point is `main` in `rust/shell/src/ffi/appmain.rs`) launches
this shell; `--headless`/`--dump`/`--eval`/`--inspect`/`--act` run the
same engine in-process without a display. The earlier in-process GTK
renderer has been removed.

Every §1–§16 row below describes that one engine. The browser-chrome
features layered on top by the GTK shell are tracked here:

| Browser-chrome feature | GTK | Notes |
|------------------------|:--:|------|
| Page render · scroll · resize reflow | ✅ | whole shared-memory frames (`/render`, `/viewport` requests) |
| Per-tab process isolation / sandbox | ✅ | each tab's engine runs in a Landlock+seccomp child |
| Click activation (`pointerdown`→`mousedown`→`pointerup`→`mouseup`→`click`) | ✅ | full sequence in `ns_browser_click` (`rust/browser/src/input.rs`) |
| `:hover` CSS restyle + pointer move/over/out JS events | ✅ | `ns_browser_hover` tracks the hovered element and restyles (`set_hover_node` in `rust/css/src/ffi/matcher.rs`) |
| Keyboard input → JS + form/`contenteditable` editing | ✅ | `/key` requests drive the shared text-entry machinery |
| Form submit (GET query + POST body) + navigation | ✅ | stashes a pending POST and re-`/open`s |
| Text selection (drag-select, select-all, copy) | ✅ | `/select` requests |
| Continuous CSS animation / `requestAnimationFrame` loop | ✅ | renderer frame loop while `ns_browser_animating()` is true |
| Per-page zoom | ✅ | renderer-side `scale` |
| Find-in-page | ✅ | Ctrl+F search bar (live match count, next/prev with wrap) over an IPC `/find` request → `ns_browser_find`, reusing the box-tree match search (`rust/layout-query/src/find.rs`) with all matches painted and the active one emphasized |
| Context menu (right-click) | ✅ | open/copy link, back/forward/reload, copy page address, select-all/copy selection, save as PDF/image |
| DevTools / console | ✅ | F12 / Ctrl+Shift+J panel: streams `console.log`/`warn`/`error`/`info`/`debug` (`/console` poll) and evaluates JS in the live page (`/eval` → `ns_browser_eval`). No DOM/network inspector |
| Save / print / PDF export | ✅ | full-page PDF or PNG (context menu / Ctrl+P / Ctrl+S) via an `/export` request → `ns_browser_render_image`; the sandboxed renderer writes to its runtime dir and the shell copies to the chosen path |
| External audio/video player handoff | ✅ | clicking a `<video>`/`<audio>` in an undecodable format resolves the media URL (`/media` → `ns_browser_media_at`) and reports it over the renderer protocol, for an embedder to hand to a player of its own; the GTK shell launches nothing |
| App menu · About | ✅ | toolbar menu button with About, Settings, and bookmarks (it links the engine's config/bookmarks store: `rust/config`, and `Bookmarks::load`/`add` in `rust/bookmarks`) |

---

## §1 Introduction

Informative; nothing to implement. Southstar targets the HTML5
*standards* processing model — documents are parsed and laid out in
standards mode (see [§13](#13-the-html-syntax)).

## §2 Common infrastructure

| Topic | Status | Notes |
|-------|:--:|------|
| WHATWG URL parsing & serialisation | ✅ | `rust/url` (`southstar_url::bytes`: `resolve`, `parts`, `host_from`, `origin_from`) |
| IDN / Punycode | ✅ | UTS #46 ToASCII in `rust/url`, over tables generated from the Unicode Character Database |
| Origin & same-origin/same-site | ✅ | `same_origin` (`rust/url/src/bytes.rs`), `is_same_site` (`rust/net/src/url.rs`) |
| Character encodings → UTF-8 | ✅ | uchardet detection in `decode_body` (`rust/html-util`), `g_convert` to UTF-8, Latin-1 last-resort |
| Content-type sniffing | 🟡 | charset sniffing delegated to uchardet; no full MIME sniffing standard |
| Reflected content attributes / IDL | ✅ | typed reflection in `rust/js-reflect`: string, URL (resolved to absolute), boolean (presence), `long`/`unsigned long` with defaults and spec clamping (e.g. `colSpan` → [1,1000], `rowSpan` → [0,65534]), numeric `progress`/`meter` range getters (`value`/`max`/`position`, `min`/`low`/`high`/`optimum`), and **enumerated** attributes canonicalised to known keywords with missing-/invalid-value defaults (`type`, `loading`, `decoding`, `method`, `crossOrigin`, `referrerPolicy`, `draggable` true/false/auto) |
| Microsyntaxes (numbers, dates/times, colours, tokens) | 🟡 | integer/non-negative-integer parsing drives reflection; date/month/week/time/local-date-time parsing & serialisation back the form `valueAsNumber`/`valueAsDate` APIs (`rust/datetime`, `rust/js-forms/src/value.rs`); the legacy-colour-value algorithm drives presentational hints (`bgcolor`/`text`/`<font color>`, `legacy_color` in `rust/css/src/hints.rs`); space/comma-separated tokens handled |
| `DOMTokenList` (`classList`, `relList`) | ✅ | generic backing-attribute token list in `rust/js-attrs/src/token_list.rs`; ASCII-whitespace tokenisation per spec; empty/whitespace tokens throw `SyntaxError`/`InvalidCharacterError`; `relList` reflects `rel` live; full iterator protocol — `values()`, `keys()`, `entries()`, `forEach`, and `Symbol.iterator`, so `for (const t of el.classList)`, `[...el.relList]`, and `Array.from(...)` all work |
| `DOMStringMap` (`dataset`) | ✅ | native live map over `data-*` attributes (read/write/delete/enumerate) with camelCase↔dash conversion; invalid names (`-` + lowercase) throw `SyntaxError` |

## §3 Semantics, structure and APIs of HTML documents

| Topic | Status | Notes |
|-------|:--:|------|
| DOM tree construction | ✅ | `rust/html5` (`rust/html-parser`); spec-faithful even for malformed input |
| `Document`, `documentElement`, `body`, `head` | ✅ | exposed in `rust/js-document` |
| `document.title` | ✅ | reflects `<title>` |
| `HTMLElement.innerText` / `outerText` | ✅ | layout-aware getter (not a `textContent` alias): the rendered-text walk in `rust/js-text/src/inner_text.rs` skips `display:none` subtrees and non-rendered elements (`script`/`style`/`head`/…), collapses runs of ASCII whitespace under normal `white-space` while preserving them under the `pre` family, turns `<br>` and block-box boundaries into newlines, and keeps `visibility:hidden` text per spec; when the element itself is not rendered the getter falls back to `textContent`. The setter splits on `\n` into text nodes and `<br>` elements |
| `getElementById` | ✅ | id index (`rust/dom/src/index.rs`) is a self-healing cache: a hit is validated (still connected, id unchanged) and any miss/stale entry falls back to an authoritative tree walk that repairs the cache, so the spec's first-in-tree-order element is returned even after DOM mutation, `cloneNode`+`replaceChild`, or transient duplicate ids (the pattern QUnit's fixture reset exercises) |
| DOM tree accessors (`getElementsBy{Id,TagName,TagNameNS,ClassName,Name}`, `forms`/`images`/`links`/`scripts`) | ✅ | `getElementsByTagNameNS` applies the spec namespace filter against each element's **actual** namespace (`by_tag_ns` in `rust/js-collections/src/live.rs`): empty string normalised to `null` (matches only null-namespace elements, of which there are none in an HTML document), `*` matches any namespace, and a concrete URI (`…/xhtml`, `…/svg`, `…/Math/MathML`) matches the elements in that namespace by local name — so a parsed inline `<svg>` is found by the SVG namespace and not by the XHTML one; exposed on both `Document` and `Element` |
| `HTMLCollection` / `NodeList` exotic objects | ✅ | the live collections returned by `children`, `childNodes`, `getElementsByTagName`/`ClassName`/`Name`, `links`, `forms`/`images`, and `form.elements` are **WebIDL legacy platform objects** (`rust/js-collections/src/live.rs`, `rust/js-collections/src/class.rs`), not decorated `Array`s: `Object.prototype.toString` reports `[object HTMLCollection]` / `[object NodeList]`, `length` is a non-own prototype getter (so `getOwnPropertyNames` lists only the indices, plus the supported named properties for an `HTMLCollection`), indexed and named entries are read-only and configurable, and the legacy `[[DefineOwnProperty]]`/`[[Delete]]` rules hold (assigning over or deleting an index / supported name fails — strict mode throws). An `HTMLCollection` is iterable via `Symbol.iterator` but, per spec, carries no `values`/`entries`/`keys`/`forEach`; a `NodeList` carries the full iterable surface. Array-index-shaped names beyond `2^32−2` resolve as named properties, not indices |
| `document.currentScript` | ✅ | the running classic `<script>` element during execution; `null` for module scripts (`rust/js-document`) |
| `document.compatMode` | ✅ | reflects the parser's quirks state (`BackCompat` in quirks mode, else `CSS1Compat`); plumbed from the parser's quirks mode via the document node flags |
| Void / raw-text / escapable element classes | ✅ | void set (`area base br col embed hr img input link meta param source track wbr`) in `is_void` (`rust/html-util`) |
| Quirks / limited-quirks / no-quirks | 🟡 | DOCTYPE consumed; `document.compatMode` now reflects the mode, but quirks-specific layout deltas are still not applied |
| `dir` global attribute (`Element.dir`, `document.dir`) | ✅ | reflected as an enumerated IDL attribute (canonicalised to `ltr`/`rtl`/`auto`/`""`); drives Pango base direction and the start/end resolution of `text-align` (an unset `text-align` in an RTL context resolves to right). The CSS `direction` (inherited) and `unicode-bidi` properties are parsed and honoured, so author `direction: rtl` reaches base-direction parity with the `dir` attribute and `unicode-bidi: bidi-override`/`isolate`/`isolate-override`/`plaintext` use real Unicode bidi controls (see §4.5). `dir=auto` (and the `bdi` default) computes its directionality from the first strong character of the element's content — feeding both the laid-out base direction (a `dir=auto` block of Hebrew/Arabic lays out RTL) and the `:dir()` selector (`node_dir` in `rust/css/src/element_state.rs`), ignoring digits/punctuation and descendant subtrees that carry their own `dir` |
| `lang`, `translate`, `accessKey` global attributes | ✅ | reflected (`translate` resolves the inherited yes/no translation mode); the nearest-ancestor `lang` (falling back to `xml:lang`) feeds Pango text shaping and hyphenation |

### §4.2 Document metadata

| Element | Status | Notes |
|---------|:--:|------|
| `title` | ✅ | window/tab title; `title.text` IDL attribute returns/sets the child text content per spec |
| `base` (`href`, `target`) | ✅ | `href` feeds URL resolution through `rust/url` |
| `link rel="stylesheet"` | ✅ | fetched and cascaded (`rust/css`) |
| `link rel="icon"` | ✅ | favicon fetched as image |
| `link rel="preload"`/`prefetch` | ✅ | fetched early into the disk cache by the speculative-preload scanner (`rust/engine/src/fetch.rs`); no `as=`-based prioritization |
| `link rel="preconnect"`/`dns-prefetch` | ✅ | warm the origin's DNS + TLS connection early (`southstar_net::preconnect_async` in `rust/net`) |
| `meta charset` | ✅ | feeds charset decode |
| `meta name="viewport"` | ✅ | ignored, as in desktop browsers: the layout viewport is the window |
| `meta http-equiv` (CSP, refresh, etc.) | ✅ | CSP/Referrer-Policy honoured where reflected; **declarative refresh is applied**: the `refresh` directive and the HTTP `Refresh` response header are parsed per the WHATWG shared-declarative-refresh steps (digit time, `;`/`,` separators, optional `url`/`=` keyword, quoted/whitespace-trimmed URL — `parse_refresh` in `rust/net/src/url.rs`), armed on document open (`arm_declarative_refresh` in `rust/browser/src/build.rs`, header first then the first `<meta>` in tree order), and after the timeout the navigation is handed to the shell through the pending-nav channel (`ns_browser_take_pending_nav` → the renderer's `X-Nav` header); an armed refresh keeps `ns_browser_animating` true so the shell's frame loop stays alive to deliver it |
| `meta name="referrer"` | ✅ | Referrer-Policy applied by `referer_for` (`rust/net/src/url.rs`) |
| `style` (inline sheet) | ✅ | parsed and cascaded by `rust/css` |

## §4.3 Sections · §4.4 Grouping · §4.5 Text-level semantics

All flow/sectioning/grouping/phrasing elements parse into the DOM and
lay out through the generic block/inline engine in `rust/layout-block`
and `rust/layout-inline`.
They have **no element-specific code path** — their appearance comes
from a built-in **UA stylesheet** (`rust/css/src/ua.css`, embedded by
`rust/css/src/ffi/ua.rs`) plus author CSS. This matches the spec model, where these
elements are defined in terms of [§15 Rendering](#15-rendering) UA CSS.

| Group | Examples | Status |
|-------|----------|:--:|
| Sectioning | `body article section nav aside header footer address h1`–`h6` `hgroup` | ✅ via UA CSS |
| Grouping | `p hr pre blockquote ol ul menu li dl dt dd figure figcaption main div` | ✅ via UA CSS |
| Obsolete preformatted | `xmp listing plaintext` | ✅ render as block monospace `white-space: pre` per §15 (previously hidden) |
| Text-level | `a em strong small s cite dfn abbr code var samp kbd b i u mark span` | ✅ via UA CSS; `abbr[title]` carries the spec dotted underline |
| Dedicated inline handling | `q` `sub` `sup` `br` `wbr` | ✅ explicit in the box-tree build (`rust/layout-build/src/collect.rs`) |
| Ruby annotations | `ruby rt rp` | 🟡 parsed; rendered inline without ruby positioning |
| `data` / `time` | | ✅ reflected `data.value` and `time.dateTime` |
| `bdi` / `bdo` | | ✅ both wrap their content in Unicode bidi formatting controls at layout time (`rust/layout-build/src/collect.rs`), so the text layout (`rust/text` over `rust/bidi`) lays them out per UAX#9. `bdo` (UA rule `unicode-bidi: bidi-override`) emits the override pair (LRO/RLO … PDF) keyed on the resolved direction, forcing the run's direction. `bdi` (UA rule `unicode-bidi: isolate`, defaulting to `dir=auto`) emits an isolate (LRI/RLI, or **FSI** for the auto/plaintext case … PDI), so its content's directionality cannot reorder the surrounding text and vice-versa. The general `unicode-bidi` values `isolate`, `isolate-override`, and `plaintext` are honoured the same way |

The UA stylesheet defines the heading scale, list markers, default
margins/padding, form-control baselines, and sets non-rendered
elements (`head title meta link style script noscript template`) to
`display:none`.

## §4.6 Links · §4.7 Edits

| Topic | Status | Notes |
|-------|:--:|------|
| `a href` navigation | ✅ | link ranges tracked in layout; navigation dispatched from `rust/browser/src/input.rs` |
| `target` | ✅ | stored on the link range |
| `rel` (`noopener`/`noreferrer`/`nofollow`) | 🟡 | parsed; `noreferrer` interacts with Referrer-Policy; `noopener` semantics limited (single browsing context) |
| `download` | 🟡 | recognised; save flow limited |
| `HTMLHyperlinkElementUtils` (`href`/`protocol`/`host`/`hostname`/`port`/`pathname`/`search`/`hash`/`origin`) | ✅ | typed URL decomposition via `part_get` / `part_set` (`rust/js-dom-factory/src/hyperlink.rs`); `href` resolves to absolute on read, parses on write |
| `a.text` (descendant text content) | ✅ | the element's descendant text content, per spec |
| `ins` / `del` | ✅ | full HTMLModElement: styled by UA CSS (`ins` green underline, `del` red strike-through), and the element's only two IDL attributes are reflected per spec — `cite` as a **URL** (resolved to an absolute URL on read via the shared URL-reflection path, `attr_get` in `rust/js-reflect/src/strings.rs`; `getAttribute` still returns the literal) and `dateTime` as a plain string. `ins`/`del` define no further behaviour in the spec |

## §4.8 Embedded content

| Element | Status | Notes |
|---------|:--:|------|
| `img` | ✅ | layout + decode pipeline |
| `img srcset` / `sizes` | ✅ | descriptor parsing (`srcset::select` in `rust/layout/src/srcset.rs`) + `sizes` evaluation (`southstar_css::sizes_resolve`); width & density descriptors selected by viewport/density. `HTMLImageElement.currentSrc` reflects the actually-selected source (resolved to an absolute URL via the shared `img_chosen_url` (`rust/layout/src/boxes/image_source.rs`) so it always matches the image the engine renders, including `<picture>` selection) |
| `picture` / `source` | ✅ | `picture_source` (`rust/layout/src/image_source.rs`) matches `media`/`type` |
| `img loading="lazy"` | ✅ | fetch/decode deferred until the image scrolls near the viewport (`rust/engine/src/fetch.rs`) |
| Decode pipeline | ✅/🟡 | the chain in `rust/image`: ICO (`rust/image-decoders`) → Rust raster decoders (`rust/raster` over `png`, `gif`, `zune-jpeg` and an in-tree BMP decoder: PNG/APNG, GIF, BMP, JPEG) → WebP via libwebp (lossy VP8 + lossless VP8L + animated via `WebPAnimDecoder`, `rust/image-decoders/src/ffi/libwebp.rs`) → AVIF via libavif (`rust/image-decoders/src/ffi/libavif.rs`, if built). SVG renders in-engine (`rust/svg`). Nothing follows: an unsupported format fails to decode |
| `iframe` | 🟡 | `src`/`srcdoc` load; a **srcless or `about:blank`** frame, when connected, runs the load algorithm — a real same-origin `about:blank` content document is created and a `load` event fires (`load_now` in `rust/js-frames/src/load.rs`), so script that waits on `iframe.onload` proceeds; `sandbox` parsed **and enforced** — scripts, forms, popups, modals, and same-origin (cookie/storage) gated per the token list, restrictions inherited by nested frames (`effective` in `rust/js-frames/src/sandbox.rs`) |
| `iframe srcdoc` | 🟡 | attribute and DOM reflection; embedded rendering still limited |
| `embed` / `object` | 🚫 | no NPAPI/PPAPI plugin dispatch |
| `video` | 🟡 | plays **inline** for MPEG-1 (`.mpg`/`.mpeg`/`.m1v`, always) and VP9/VP8 WebM (`.webm`, with FFmpeg libav), honouring `autoplay`/`loop`/`muted`/`poster`, play/pause, seeking, volume, `played` and timed events. `MediaSource`/`SourceBuffer` streams use the same standards path: YouTube- and Vimeo-style WebM segments are accumulated in the renderer, video is decoded in `southstar-video`, audio in `southstar-audio`, and old prefix segments are evicted when the page calls `SourceBuffer.remove()`. `SourceBuffer.buffered` exposes the retained demux timestamp range after eviction. Unsupported codecs retain the poster + play overlay. See [media.md](media.md) |
| `audio` | 🟡 | MP3 (always) and, when FFmpeg libav is built in, Opus/Vorbis (`.opus`/`.webm`/`.ogg`) play via the unsandboxed `southstar-audio` helper; other codecs hand the source URL to the system media player. See [media.md](media.md) |
| `track` (captions) | 🟡 | parsed; `kind`/`src`/`srclang`/`label`/`default` reflected via the standard typed-reflection path. **Rendered**: a `<track default>` whose `kind` is `subtitles`/`captions` (the missing-value default) is fetched, its WebVTT parsed into timed cues (`southstar_webvtt::parse` in `rust/webvtt` — `[HH:]MM:SS.mmm` timings, cue-setting/identifier/`NOTE` skipping, `<…>` tag and entity stripping), and the cue active at the video's current time is painted as centred captions over the bottom of the inline video (`paint_video_caption` in `rust/paint`). Only the `default` track auto-shows (per the spec's initial mode); JS `TextTrack.mode` switching and cue positioning settings (`line`/`position`/`align`) are not wired |
| `map` / `area` (client-side image maps) | ✅ | `<img usemap>` clicks are hit-tested against the referenced `<map>`'s `<area>` elements — `rect`/`circle`/`poly`/`default` shapes in image-local coordinates — and the first matching area's `href` is navigated (`resolve` in `rust/dom/src/image_map.rs`, wired into the GUI and headless click paths) |
| `img ismap` (server-side image maps) | ✅ | clicking an `<img ismap>` nested in an `<a href>` appends the click position relative to the image's top-left corner as a `?x,y` suffix to the link URL before navigating (GUI path in `rust/browser`, headless click path in `rust/headless`); coordinates are clamped to non-negative |
| MathML | ✅ | presentation MathML is laid out and painted over `rust/text`/cairo (`rust/mathml`), embedded inline on the surrounding text baseline through the replaced-element media-box path (`rust/layout-inline`, `rust/paint`): `mrow`, the token elements `mi`/`mn`/`mo`/`ms`/`mtext` (with `mi` auto-italicising single letters and `mo` operator spacing), `msup`/`msub`/`msubsup`, `mfrac` (with rule), `msqrt`/`mroot` (drawn radical), `munder`/`mover`/`munderover`, `mtable`/`mtr`/`mtd`, `mspace`, `mphantom` (reserves its contents' metrics without painting), `mfenced` (synthesises the `open`/`close` fences and `separators`), and `semantics` (renders its first presentation child). Content MathML and `annotation`/`annotation-xml` payloads are not rendered |

## §4.9 Tabular data

`table caption colgroup col thead tbody tfoot tr td th` are laid out
by the table code in `rust/layout-table`, driven off the CSS
`display:table | table-caption | table-row | table-cell | …` family.

| Feature | Status | Notes |
|---------|:--:|------|
| Basic grid layout | ✅ | rows/cells sized from content |
| `colspan` / `rowspan` | ✅ | parsed and applied; row-spanning cells expand the covered row group when their content is taller than the initially measured rows |
| `thead`/`tbody`/`tfoot` grouping | 🟡 | header rows rendered first and footer rows last regardless of source order (`collect_rows` in `rust/layout-tree/src/table.rs`); not repeated across fragments |
| `caption` | ✅ | rendered as a table caption; `caption-side:top/bottom` supported |
| Cell `vertical-align` (`valign`) | ✅ | `vertical-align: top/middle/bottom` on table cells positions content within the row's resolved height (`rust/layout-table`); the legacy `valign` attribute maps onto it. `baseline` (the default) is approximated as `top` |
| `col`/`colgroup` width hints | ✅ | `<col>`/`<colgroup>` `width` (length or percentage, honouring `span`) and explicit per-cell `width` now seed column widths in **auto** layout too, not just `table-layout:fixed` (`rust/layout-table`): a column with an explicit width is pinned (clamped to its min-content), flexible columns absorb the remaining width, and when all columns are pinned the leftover is distributed proportionally |
| Fixed vs auto `table-layout` | 🟡 | `table-layout:fixed` uses `col`/`colgroup` hints, first-row cell widths, then equal remaining columns; auto layout remains an approximation |
| `border-spacing` / `cellspacing` | ✅ | inherited `border-spacing` (one or two lengths) consumed by the auto and fixed table layout (`border_spacing` in `rust/layout-table/src/table.rs`): horizontal spacing sits before/between/after columns and is reserved out of the available width, vertical spacing before/between/after rows; the UA sheet defaults `table` to `border-spacing: 2px` so plain tables get the spec inter-cell gap, and the legacy `cellspacing` attribute maps onto it |
| Border-collapse model | ✅ | `border-collapse: separate` (default) vs `collapse` honoured: collapse forces `border-spacing` to zero **and** de-duplicates shared interior edges to a single grid line via a cell-occupancy grid (`collapse_borders` in `rust/layout-table/src/table.rs`, honouring colspan/rowspan) |
| Legacy `frame` / `rules` attributes | ✅ | mapped as presentational hints (`presentational_hints` in `rust/css/src/hints.rs`): `frame` picks the rendered outer-border sides (`void`/`above`/`below`/`hsides`/`vsides`/`lhs`/`rhs`/`box`/`border`), `rules` draws the matching internal cell borders (`none`/`all`/`cols`/`rows`; `groups` approximated as `all`) and collapses the table borders |

## §4.10 Forms

A first-class, functional surface: `rust/layout-build` and
`rust/layout-content` render the controls; `rust/js-forms` and `rust/forms`
wire up submission, `FormData`, and constraint validation.

| Control / feature | Status |
|-------------------|:--:|
| `input` text/password/search/email/url/tel/number | ✅ (editable with a rendered caret and a highlighted text selection; keyboard caret/selection navigation — arrows, Home/End, Shift+move to extend, Ctrl+A select-all, Ctrl+C/X copy/cut — and selection-replacing edits. **Rendering & CSS** (`rust/layout-build/src/controls.rs`): the author `color`, `font-size`, and `font-family` of the control apply to the value text and field metrics; an unfocused value longer than the field is clipped according to `text-align` (`start`/`left` show the head, `center` shows a middle window, and `end`/`right` show the tail), while a focused field scrolls to keep the caret in view; disabled controls render their text greyed. Author `border`/`background`/`background-image`/`border-radius`/`padding` are honoured via the CSS-chrome path. The **`::placeholder`** pseudo-element is supported (parsed in `rust/css`, resolved like the other pseudo-elements): placeholder text takes the pseudo's `color`, `font-style`, and `font-weight`, falling back to a muted UA grey (`#757575`) when unstyled; the legacy `::-webkit-input-placeholder` / `::-moz-placeholder` / `::-ms-input-placeholder` aliases map to it. When the control is positioned, a flex/grid item, or a `display:block` box, it gets a real layout box and the field chrome fills the assigned width (so e.g. a `flex:1` or `width:100%` search input fills the bar instead of staying at its intrinsic `size` width — `paint_inline` in `rust/paint`). Short values are padded into the same `text-align` position, and values/placeholders stay on a single line) |
| `input` checkbox/radio | ✅ |
| `input` button/submit/reset | ✅ |
| `input type=number` | ✅ (Up/Down arrow keys step by `step`, default 1, clamped to `min`/`max`, firing input/change; non-numeric keystrokes filtered out; no rendered spin buttons) |
| `maxlength` while editing | ✅ (user typing/paste cannot grow a text input or textarea past `maxlength`; programmatic `.value` unrestricted per spec) |
| `input` file/color/range | ✅ (`<input type=file>.files` returns a live `FileList` of `File` objects after the user picks — each carries `name`, `size`, `type` from `southstar_mime::guess` (`rust/mime`), and `Blob`-shaped bytes, so `await file.text()` / `await file.arrayBuffer()` work and `new FormData()` over the input serialises the bytes through the shared multipart path) |
| `input` date/time/datetime-local/month/week | 🟡 (text-style entry; no native picker) |
| `textarea` | ✅ (multi-line: newlines preserved as line breaks, height from `rows`, border box grows to enclose content; caret and text selection rendered; an empty textarea shows its `placeholder` in the UA grey / author `::placeholder` colour, and the control's `color`/`font-size`/`font-family` apply to the value — shared with the `<input>` path via `emit_control_text_style` in `rust/layout-content/src/controls.rs`) |
| `select` / `option` / `optgroup` | 🟡 (rendered; DOM options/selectedOptions collections, add/remove, spec-compliant `option.text` — descendant text minus script subtrees, ASCII-whitespace-stripped + collapsed — `option.label` (label attr, falling back to `option.text`), `optgroup.label` reflected, `option.value` (value attr, falling back to `option.text`), single/multiple `value`; the select popup and form submission both consult `option.label` / `option.value` so legacy markup with extra whitespace or inline children now submits the same string a browser would. **Fully interactive** via an engine-rendered inline picker: a plain select opens on click to an inline option list and commits the clicked option; a `multiple`/`size>1` select renders as a listbox whose options are individually clickable (plain click selects, `Ctrl`-click toggles a `multiple` selection — `dropdown_click` in `rust/browser/src/input.rs`, per-option boxes built by `listbox` in `rust/layout-build/src/controls.rs`); and a focused select takes **keyboard** input (`select_key`): Arrow Up/Down step the selection, Home/End jump to the first/last enabled option, printable keys do type-ahead, and Enter/Space/Escape open/close the dropdown (`select_step`/`select_edge`/`select_typeahead` in `rust/js-forms/src/ffi/mod.rs`, `toggle_option` in `rust/js-forms/src/select.rs`), each firing `input`/`change`. Uses an inline picker, not a native OS popup. Explicit deselection is honoured per spec: `select.selectedIndex = -1`, or setting `select.value` to a string no option carries, deselects every option and reports `selectedIndex === -1` / `value === ""` instead of snapping back to the first option — the contract jQuery's `val()` setter relies on) |
| `datalist` | 🟡 (autocomplete suggestions rendered: a focused `<input list=…>` shows the referenced `<datalist>`'s matching `<option>`s as an inline dropdown — substring-filtered against the current value, case-insensitive, capped at 8 (`emit_datalist_suggestions` in `rust/layout-content/src/select.rs`); clicking a suggestion fills the field and fires `input`/`change` while keeping focus (`datalist_click` in `rust/browser/src/input.rs`), typing re-filters, and `Escape` dismisses. No keyboard highlight navigation through the suggestions) |
| `button` (submit/reset/button) | ✅ |
| `output` | ✅ (full HTMLOutputElement: `value` and `defaultValue` track the value-mode flag — setting `value` switches to value mode while `defaultValue` preserves/serves the markup default, and a form reset restores the default value; `type` returns `"output"`, `htmlFor` is a live `DOMTokenList`, `labels`/`form`/`name` reflect, and the element is correctly barred from constraint validation so `willValidate` is `false` and `checkValidity()` stays `true` even with `setCustomValidity` set, in `rust/js-forms`; native reset-button activation routes through `form_reset` from `rust/browser/src/input.rs`) |
| `progress` | ✅ (determinate + indeterminate rendered bars; numeric `value`, `max`, `position` IDL getters) |
| `meter` | ✅ (spec min/max/value/low/high/optimum gauge algorithm; optimum/suboptimal/less-good regions reflected in rendering and numeric IDL getters) |
| `fieldset` / `legend` | ✅ (full HTMLFieldSetElement: UA-styled bordered group with rendered `<legend>`; `type` returns `"fieldset"`, `name`/`disabled`/`form` reflect, and `elements` is a live `HTMLFormControlsCollection` of the listed elements (`input`/`select`/`textarea`/`button`/`output`/nested `fieldset`) rooted at the fieldset with indexed and `namedItem` access (`collect_listed` in `rust/js-forms/src/lib.rs`); a `disabled` fieldset disables and greys its descendant controls and excludes them from submission, and the element is barred from constraint validation — `willValidate` is `false`, `checkValidity()`/`reportValidity()` return `true`, and `validationMessage` is empty even when `setCustomValidity` set `validity.customError`) |
| Constraint validation (`required` `readonly` `pattern` `min` `max` `step` `minlength` `maxlength`) | ✅ (the full barrier API: `willValidate`, a live `validity` `ValidityState` with every flag — `valueMissing`/`typeMismatch`/`patternMismatch`/`rangeUnderflow`/`rangeOverflow`/`stepMismatch`/`tooShort`/`tooLong`/`badInput`/`customError`/`valid` — `validationMessage`, `setCustomValidity`, element and form-wide `checkValidity()`/`reportValidity()`, the non-bubbling cancelable `invalid` event, and the `:valid`/`:invalid`/`:required`/`:optional` pseudo-classes. Native submission (and `requestSubmit()`) blocks on an invalid control and fires `invalid`; `novalidate`/`formnovalidate` suppress interactive submission validation without affecting the scripting API. Validity is computed for required text/select/checkbox/radio, email lists with `multiple`, URL-parser-backed `type=url`, pattern/type/length, numeric and temporal `min`/`max`/`step`, and custom validity) |
| `FormData` (`append`/`set`/`entries`/…) | ✅ | `append(name, blob, filename)` honours the third-argument filename per spec |
| Form ownership / successful controls | ✅ (`form="id"` owners, disabled fieldsets, default checkbox/radio `"on"` values, multi-select values, form.elements named lookup/RadioNodeList, associated submit/reset activation with cancelable reset events) |
| Submission, `application/x-www-form-urlencoded` | ✅ (HTML `+` space encoding; `requestSubmit()` validates/fires `SubmitEvent` with `submitter`, while `submit()` bypasses both) |
| Submission, `multipart/form-data` | ✅ (full UTF-8 serialiser: native form submit (`rust/forms`, encoded by `rust/net/src/forms.rs`), and `fetch`/`XMLHttpRequest` bodies of `FormData` and `URLSearchParams` (`nsr_js_form_data_serialize` in `rust/js-forms/src/ffi/mod.rs` / `usp_serialize` in `rust/js-runtime/src/api.rs`) — CSPRNG boundary, per-entry `Content-Disposition`, Blob/File parts get a `Content-Type` from `blob.type` (default `application/octet-stream`) and the entry's filename (or `blob` if unspecified), name/filename quoted per WHATWG (only LF/CR/`"` escaped), and `URLSearchParams` bodies auto-pick `application/x-www-form-urlencoded;charset=UTF-8` — caller-set `Content-Type` always wins) |
| `formaction`/`formmethod`/`formenctype` overrides | ✅ |

## §4.11 Interactive elements

| Element | Status | Notes |
|---------|:--:|------|
| `details` / `summary` | ✅ | UA-styled disclosure widget with a marker; the `<summary>`'s **activation behavior** toggles its parent `<details>` across every click path — scripted `summary.click()` (`activation_behavior` in `rust/js-focus/src/activation.rs`), the renderer/GUI pointer-click path (`activate_summary`, reached from `ns_browser_release_click` through `rust/browser/src/input.rs`, honouring `preventDefault()`), and the `open` IDL/attribute setters. Each toggle dispatches the spec's `ToggleEvent` pair — a `beforetoggle` then a `toggle`, carrying `oldState`/`newState` ∈ `"open"`/`"closed"` (`toggle_open` in `rust/js-top-layer/src/details.rs`); opening one `<details name="X">` closes the rest of the group per the exclusive-accordion rule; fragment/hash navigation into skipped details content sets `open` before scrolling. CSS open/close *animation* (`::details-content`/`interpolate-size`) is not supported — a rendering nicety, not part of the element's behaviour |
| `dialog` | ✅ | `open`/`show()`/`showModal()`/`close(result)` and `returnValue` implemented (`rust/js-top-layer/src/dialog.rs`); `method="dialog"` forms close the dialog with the submitter's value; `requestClose(returnValue?)` (and an Escape press on the topmost open modal) fires a cancelable `cancel` event and, if not prevented, closes the dialog and fires `close` — matching the spec's close-watcher semantics. `showModal()` now puts the dialog in the top layer (painted on top of an author `::backdrop` fill, `rust/paint`), moves focus to its `autofocus`/first focusable descendant, traps focus by making the rest of the document inert, and restores focus to the opener on close |
| `popover` attribute | 🟡 | open/closed state, `showPopover`/`hidePopover`/`togglePopover`, `popovertarget` activation, and target/action reflection; open/close transitions dispatch the spec `ToggleEvent` pair — a `beforetoggle` (cancelable on open, so `preventDefault()` keeps the popover closed) then a `toggle`, each with `oldState`/`newState`; limited top-layer behaviour |

## §4.12 Scripting

| Topic | Status | Notes |
|-------|:--:|------|
| `script` inline / external | ✅ | `run_scripts_in_doc` (`rust/js-page/src/run.rs`); `script.text` returns/sets the child text content per spec |
| `async` / `defer` | ✅ | `defer` delays to end of parse |
| `type="module"` / `nomodule` | 🟡 | modules detected and run; full module graph/`import` resolution limited |
| Engine | ✅ | Boa (pure Rust, patched fork in `third_party/boa_engine`, **interpreter only, no JIT** — W^X holds); per-call eval budget plus a 60 s absolute execution monitor that halts a runaway page (armed on the outermost JS entry, enforced in the Boa fork's interrupt handler — `rust/js-runtime/src/budget.rs`, `interrupt.rs`) |
| `noscript` | ✅ | `display:none` when JS enabled |
| `template` (`.content`) | ✅ | `template.content` is a `DocumentFragment` (snapshot clone of the parsed children); descendants of a `<template>` are hidden from the document's tree-walk: `document.querySelectorAll`, `getElementsByTagName`/`ClassName`/`Name`, `getElementById`, the CSS selector engine, and the id/class/tag indexes all stop at a `<template>` and don't descend into its children. The child accessors on the template element — `firstChild`, `lastChild`, `firstElementChild`, `lastElementChild`, `children`, `childNodes`, `childElementCount`, `hasChildNodes()` — all report empty/null, matching the spec model where template content lives in the content fragment rather than as children of the template element. Only `template.content`-rooted queries see the parsed nodes |
| `slot` / shadow projection | 🟡 | `attachShadow` + slot assignment (bounded) |
| `canvas` 2D context | ✅ | full Cairo-backed `CanvasRenderingContext2D` (paths, text, `drawImage`, gradients/patterns, `get/putImageData`, compositing, shadows) |
| `canvas` WebGL / WebGL2 context | 🟡 | `getContext("webgl"/"webgl2")` maps a pragmatic WebGL 1 / 2 core directly onto OpenGL ES via the toolkit-independent GL context (`rust/glctx`) + libepoxy (`rust/webgl`). Enabled by default, globally switchable in Settings, and announced in the status bar when used. The supported extension set is deliberately small; data-transfer entry points are bounds-checked and zero-initialised. |
| `canvas` WebGPU context | 🟡 | experimental, off at runtime by default. Built whenever wgpu-native is present (the `webgpu` feature is `auto`) and enabled at runtime with `--enable-webgpu` (or `NS_WEBGPU_ALLOW=1`); `navigator.gpu` + `getContext("webgpu")` cover most of the **render and compute** path: WGSL shaders (naga), bind groups / uniforms / samplers / textures (incl. `copyExternalImageToTexture`), render & depth pipelines, MSAA, **compute pipelines** + storage buffers, and texture-to-canvas output — enough that **three.js's `WebGPURenderer` renders on it**, including GPU-compute examples (`webgpu_compute_birds`). Real timestamp queries, storage textures, and some PBR feature paths remain. |
| `OffscreenCanvas` | 🟡 | constructs; no worker thread |

## §4.13 Custom elements

| Topic | Status | Notes |
|-------|:--:|------|
| `customElements.define` / `get` | ✅ | `rust/js-custom-elements`; `define()` upgrades already-connected matching elements **synchronously**, so `connectedCallback` runs before the next statement (an in-construction deferral counter keeps the explicit upgrade from being suppressed during the initial inline-script run) |
| Autonomous custom elements | ✅ | name validation (hyphen required); `customElements.define(name, ctor)` |
| Customized built-in elements (`define(..., {extends})` / `is=`) | ✅ | `customElements.define(name, ctor, {extends})` records the extended local name; a matching `<button is="…">` — from markup or `document.createElement("button", {is})` — is upgraded through the same reaction machinery as autonomous elements: constructor/`super()` adoption, `connectedCallback`/`disconnectedCallback`, `observedAttributes` + `attributeChangedCallback`, and `instanceof` against both the custom class and the extended interface. An `extends` that is itself a valid custom-element name throws `NotSupportedError` (`class_for_node` in `rust/js-custom-elements/src/reactions.rs` / `define` in `rust/js-custom-elements/src/registry.rs`). The built-in interface prototypes (`HTMLButtonElement.prototype` …) are chained into the DOM prototype hierarchy, so a customized built-in keeps the extended element's own members (`value`, …) alongside every `Element`/`Node` method |
| Lifecycle (`connected`/`disconnected`/`adopted`/`attributeChanged`) callbacks | ✅ | |
| `observedAttributes` | ✅ | |
| Shadow DOM (`attachShadow`, slots) | 🟡 | see §4.12 |

## §4.14–4.16 Idioms · disabled elements · selector matching

| Topic | Status | Notes |
|-------|:--:|------|
| Common idioms (`rel` keywords, etc.) | 🟡 | as above |
| `disabled` / inert disabling | ✅ | the §4.16 "actually disabled" concept is fully applied to every disabled-capable control (`button`/`input`/`select`/`textarea`/`optgroup`/`option`/`fieldset`), with `fieldset` and `optgroup` propagating disabledness to descendants: a disabled element matches `:disabled` (not `:enabled`), is barred from focus (`focus()` leaves `document.activeElement` unchanged) and from click activation (a synthetic `.click()` fires no `click` event), is omitted from `FormData`/form submission, and is barred from constraint validation (`willValidate` is `false`). `inert` is the orthogonal subtree-disabling mechanism — see its own row above |
| Matching elements via Selectors/CSS | ✅ | rich selector engine (`rust/css`) with broad structural, state, and functional selector coverage, including the HTML-connected state pseudo-classes `:default`, `:indeterminate`, `:in-range`, `:out-of-range`, `:blank`, `:target-within`, and `:modal`. The Selectors API (`querySelector`/`querySelectorAll`/`matches`/`closest`) throws a `SyntaxError` for an unparseable or non-standard selector — a dangling combinator (`div >`), an unknown pseudo-class (jQuery's `:radio`/`:first`/`:eq`/`:contains`, etc.), or an empty list — via `SelectorList::parse` (`rust/css/src/ffi/query.rs`) and `selector_syntax_error` (`rust/js-collections/src/query.rs`). Standard-but-still-unimplemented pseudo-classes (`:fullscreen`, `:autofill`, …) and the forgiving `:is()`/`:where()` interiors do **not** throw. This is the contract jQuery/Sizzle rely on to fall back to their own engine, so their non-CSS pseudos now resolve correctly |

## §5 Microdata

✅ **Implemented.** The full microdata DOM API lives in
`rust/js-html/src/microdata.rs`:

- `element.itemScope` / `itemId` — reflected (get + set).
- `element.itemType` / `itemProp` / `itemRef` — live `DOMTokenList`s.
- `element.itemValue` — element-type-aware getter **and** setter (`meta`
  content, `src`/`href`/`data` resolved to absolute URLs, `data` value,
  `time` datetime, nested item → the element itself, else text); setting
  on an item throws `InvalidAccessError`.
- `element.properties` — an `HTMLPropertiesCollection` crawled per the
  spec's properties algorithm (honours `itemref`, stops at nested items,
  results in tree order), with `.names`, `.namedItem(name)`, named access
  (`coll.foo`), and `PropertyNodeList.getValues()`.
- `document.getItems(typeNames)` — top-level items filtered by type.

No JSON/RDF serialisation beyond this API (not part of the spec's DOM
surface).

## §6 User interaction

| Topic | Status | Notes |
|-------|:--:|------|
| `hidden` attribute | ✅ | plain `hidden` maps to `display:none`; `hidden="until-found"` maps (via the UA stylesheet) to the real `content-visibility: hidden` — its subtree is laid out (so its text stays in the box tree and is findable) and the element is size-contained (collapses to a zero-height box, `rust/layout-block/src/height.rs`) but its contents are skipped while painting (`box_is_hidden` in `rust/paint`). `HTMLElement.hidden` follows the spec's enumerated getter/setter for `"until-found"`; fragment/hash navigation runs the ancestor reveal path before scrolling, removing `hidden="until-found"` ancestors and opening skipped `<details>` ancestors, with `beforematch` fired for same-document hidden-until-found reveals — removing the attribute drops the containment and reveals the content. The `content-visibility` property is also honoured directly (`auto` is treated as always-rendered, skipping only the lazy-render optimisation) |
| `inert` attribute | ✅ | excludes the subtree from focus (`focus()`, sequential navigation) and click activation, and an open modal dialog makes the rest of the document inert (`set_active_modal` → `effectively_inert` in `rust/dom/src/tree.rs`) |
| Event dispatch / cancellation | ✅ | full capture → at-target → bubble propagation with `eventPhase`, `currentTarget`, `stopPropagation()`/`stopImmediatePropagation()`, `once`/`signal` listener removal, and inline `return false`; `preventDefault()` honours `event.cancelable` and is suppressed (a no-op) for `{passive:true}` listeners; `composedPath()` returns the live propagation path (target → ancestors → document → window) during dispatch and an empty array otherwise. A primary activation fires the full UI-Events button sequence `pointerdown`→`mousedown`→`pointerup`→`mouseup`→`click` (`ns_browser_click`, `rust/browser/src/input.rs`) |
| Pointer hover (`:hover`, `mousemove`/`mouseover`/`mouseout`) | ✅ | the shell sends the hovered point to the out-of-process renderer each time the pointer moves (a `/hover` request → `ns_browser_hover` in `rust/browser`): it hit-tests the DOM, sets the CSS `:hover` state on the element under the pointer and its ancestors so `:hover` rules restyle and repaint, and fires the `pointermove`/`mousemove` and, on element transitions, `pointerover`/`mouseover`/`pointerout`/`mouseout`/`pointerenter`/`mouseenter`/`pointerleave`/`mouseleave` listeners. Restyle work is gated to pages that actually use `:hover` (`has_hover_rules` in `rust/css/src/ffi/sheet.rs`) and skipped while a text selection is in progress. The thin GTK client (`rust/shell/src/procview`) drives this on every mouse-move and re-renders when the renderer reports a visual change |
| `contenteditable` | ✅ | in-page plaintext editing: a `contenteditable` host (`true`, the empty string, or `plaintext-only`) is focusable by click or Tab and edits as a single plaintext run — on focus its content is flattened to text, then the shared text-entry machinery (`editable_value`/`set_editable_value` in `rust/dom/src/controls.rs`) drives a rendered caret, text selection, keyboard caret navigation (arrows, Home/End), insertion, Backspace/Delete, Enter→newline, and copy/cut/paste. Newlines in the host render as forced line breaks (`rust/layout-build/src/collect.rs`); `beforeinput`/`input` fire and `document.activeElement`/`:focus` stay in sync. Rich inline structure is not preserved across an edit (the plaintext model); there is no per-range rich-text formatting |
| `tabindex` / focus order | ✅ | sequential focus navigation (`sequential_focus_target` in `rust/js-focus`) honours `tabindex` ordering — positive values first in ascending order, then `0`/auto in tree order, negative excluded — skipping disabled/inert/hidden controls; Tab / Shift+Tab walk it (`rust/browser/src/input.rs`), and `focus()`/`blur()` route through the canonical `set_focus` (`rust/js-focus`), keeping `document.activeElement` and `:focus` in sync |
| `accesskey` | ✅ | reflected as `HTMLElement.accessKey`, and bound: pressing the access-key modifier (Alt, the platform combo) plus one of the element's space-separated `accesskey` characters focuses the element and runs its activation behaviour. The GTK shell forwards `Alt`+key to the engine (`on_key` in `rust/shell/src/ffi/procview_input.rs`); `ns_browser_key_full` (`find_accesskey` in `rust/browser/src/input.rs`) finds the first non-inert element whose `accesskey` matches (case-insensitive) and calls `activate_element` (`rust/js-focus`), which fires the click activation — so an `accesskey` button runs its `onclick`, a checkbox toggles, a link navigates, and a text field is focused |
| `spellcheck` | ✅ | the enumerated content attribute and the boolean `HTMLElement.spellcheck` IDL attribute are spec-correct (`spellcheck_used` in `rust/dom/src/controls.rs`): `true`/`""` → `true`, `false` → `false`, and the default/invalid state inherits the nearest ancestor's value, falling back to `true` at the root; the setter writes `"true"`/`"false"` while `getAttribute` keeps the literal. **On-screen checking is performed** when built against the optional Enchant library (`rust/spellcheck`): editable text (text `input`/`textarea` and `contenteditable`) with spell-checking enabled has its misspelled words drawn with a red wavy underline (`Underline::Error` in `rust/text`), gated per element by the used `spellcheck` value. Dictionaries are loaded before the renderer seals its sandbox; if Enchant or a dictionary is absent the attribute model still works and nothing is flagged |
| `autocapitalize` / `enterkeyhint` | ✅ | proper **enumerated** IDL reflection (`rust/js-reflect/src/strings.rs`): `enterKeyHint` canonicalises to its known keywords (`enter`/`done`/`go`/`next`/`previous`/`search`/`send`) case-insensitively with missing/invalid → `""`; `autocapitalize` maps the `off`/`none` and `on`/`sentences` aliases to their canonical state, returns `""` for the default/invalid state, and — for the form-associated controls (`input`/`textarea`/`select`/`button`/`output`/`fieldset`) — inherits the form owner's own value per spec; `getAttribute` still returns the literal. Behaviourally advisory (hints for an on-screen keyboard), which is the complete behaviour on a desktop UA with a physical keyboard |
| Drag and drop (`DataTransfer`, drag events) | ✅ | script-created `DataTransfer`, `DataTransferItemList`, `DataTransferItem` (`kind`/`type`, `getAsString`, `getAsFile`), and `DragEvent.dataTransfer` are exposed for feature detection and synthetic events; native GTK drag gestures dispatch the HTML `dragstart` → `dragenter`/`dragover`/`dragleave` → `drop` → `dragend` sequence for `draggable=true` elements, links, and images, with shared `dataTransfer` state and default URL data for links/images. Drop follows the browser rule that the target must accept the drag by cancelling `dragenter` or `dragover`. **External/native OS file drags are bridged into the page**: a `GtkDropTarget` on the view accepts a dropped `GdkFileList`, forwards the paths and drop point to the renderer (`/dropfiles` → `ns_browser_drop_files` in `rust/browser`), which builds a `DataTransfer` whose `items`/`files` carry real `File` objects (`add_file` in `rust/js-input/src/transfer.rs`) and dispatches `dragenter`/`dragover`/`drop` at the hit-tested target — so a page's drop handler receives `event.dataTransfer.files` exactly as in other browsers |

## §7 Loading web pages

| Topic | Status | Notes |
|-------|:--:|------|
| Browsing context / `Window` | ✅ | single top-level context per tab; each tab's engine runs in its own sandboxed renderer process |
| `WindowProxy` / named access | 🟡 | basic |
| Origins & same-origin policy | ✅ | enforced for fetch/XHR/storage/cookies |
| `iframe` sandboxing | ✅ | token list enforced: `allow-scripts`, `allow-forms`, `allow-same-origin`, `allow-popups`, `allow-modals`, `allow-top-navigation*`, `allow-downloads`, etc. parsed; scripts/forms/popups/modals and opaque-origin (cookie/storage) restrictions applied and inherited by nested frames |
| Cross-origin (`postMessage`) | ✅ | `window.postMessage(message, targetOrigin, transfer)` — see §9 for the full behaviour. `targetOrigin` is enforced: `"*"`/`"/"` always match, a specific origin delivers only when it equals the document's origin, otherwise the message is silently dropped |
| Session history & navigation | ✅ | back/forward |
| History API (`pushState`/`replaceState`/`popstate`) | ✅ | same-origin checked, `popstate` dispatched |
| `location` (all members) | ✅ | `href`/`protocol`/`host`/`hostname`/`port`/`pathname`/`search`/`hash` |
| `Navigation` API (`navigation.navigate`) | 🟡 | `window.navigation` is a live `EventTarget` backed by the same-document history stack (`rust/js-window/src/navigation.rs`): `currentEntry` / `entries()` expose `NavigationHistoryEntry` objects (`url`, stable `key`/`id`, `index`, `sameDocument`, `getState()` returning a structured clone), and `canGoBack`/`canGoForward` reflect the stack position. `navigate(url, {state, history, info})` fires a cancelable `navigate` event carrying `navigationType`, `canIntercept` (true for a same-origin destination — scheme/host/port compared), a `destination` with `getState()`, and a working `intercept({handler})`: an intercepted same-origin navigation commits synchronously (pushes/replaces the entry, updates `currentEntry`, fires `currententrychange`) and resolves `committed`, then awaits the handlers via `Promise.all` before firing `navigatesuccess`/`navigateerror` and settling `finished`; a non-intercepted navigation falls back to a real load. `reload()`, `back()`, `forward()`, `traverseTo(key)` and `updateCurrentEntry({state})` all drive the stack and fire `currententrychange`; `onnavigate`/`onnavigatesuccess`/`onnavigateerror`/`oncurrententrychange` are honoured. Not yet: `NavigationHistoryEntry` identity caching / `dispose`, `NavigateEvent.signal`/`formData`/`downloadRequest`, and `navigation.activation`/`transition` |
| `BroadcastChannel` | ✅ | `new BroadcastChannel(name)` (name required, else `TypeError`; `name` read-only; `instanceof` via the real prototype), `postMessage`, `close`, `onmessage`/`addEventListener`. A posted message is `structuredClone`d and delivered asynchronously (microtask) as a `MessageEvent` (`data`, page `origin`, `source:null`, empty `ports`) to every *other* open channel with the same name, excluding the sender and closed channels (`post_message` in `rust/js-workers/src/broadcast.rs`). Cross-document reach is bounded by the shared single-runtime model — channels in one runtime see each other, matching the per-origin contract within a page |
| Application cache / offline | 🚫 | obsolete; not implemented |

## §8 Web application APIs

| API | Status | Notes |
|-----|:--:|------|
| `Window` / `Navigator` (`userAgent`, `platform`) | ✅ | `navigator` chains to the non-constructible `Navigator.prototype`; `history` and `performance` likewise use their exposed Web IDL prototypes, so brand and `instanceof` checks agree with their global interfaces |
| `setTimeout` / `setInterval` / `clearX` | ✅ | |
| `queueMicrotask` | ✅ | |
| `requestAnimationFrame` / `cancelAnimationFrame` | ✅ | tied to repaint |
| `requestIdleCallback` | ✅ | |
| Promise rejection events | ✅ | unhandled promises dispatch cancelable `unhandledrejection` with `promise`/`reason`; a later handler dispatches `rejectionhandled`, and `preventDefault()` suppresses default console reporting |
| `MutationObserver` / `IntersectionObserver` / `ResizeObserver` | ✅ | callbacks fire for real: mutations via the microtask checkpoint; intersection/resize on layout changes **and** as an initial observation scheduled when `observe()` is first called (so virtualized lists, lazy-load and read-receipt patterns get their first callback without waiting for a reflow). An actively-observing `IntersectionObserver` is kept alive while it has targets, so dropping the JS reference (common with React refs) does not GC it out from under the observation; `disconnect()` / removing all targets releases it |
| `atob` / `btoa` | ✅ | |
| `crypto.getRandomValues` / `crypto.randomUUID` | ✅ | CSPRNG-backed |
| `crypto.subtle` (Web Cryptography) | ✅ | full SubtleCrypto over OpenSSL libcrypto (`rust/webcrypto`, bindings in `rust/js-crypto`): `digest`, `generateKey`, `importKey`, `exportKey`, `sign`, `verify`, `encrypt`, `decrypt`, `deriveBits`, `deriveKey`. Algorithms: HMAC; AES-GCM/CBC/CTR; RSASSA-PKCS1-v1_5, RSA-PSS, RSA-OAEP; ECDSA and ECDH on P-256/384/521; PBKDF2; HKDF. Key formats `raw`/`jwk`/`spki`/`pkcs8`; ECDSA uses the raw r‖s signature encoding. Verified against NIST AES-GCM, RFC 6070 PBKDF2 and RFC 5869 HKDF vectors |
| `structuredClone` (§2.7) | ✅ | true serialize/deserialize in `rust/js-clone`: cycles & shared references, `Map`/`Set`/`Date`/`RegExp`, `ArrayBuffer`/typed arrays/`DataView`, `Blob`/`File`, `Error` subtypes (name/message/stack), `undefined`; `DataCloneError` for functions/symbols. `structuredClone` itself does not honour a transfer list, but `Worker.postMessage(value, [buffers])` **does** transfer `ArrayBuffer`s — the bytes are serialized to the receiver and the source buffers are detached (a non-transferable entry throws `DataCloneError`). |
| `document.implementation.createHTMLDocument(title?)` | ✅ | builds a real inert HTML document (`<html><head><title></head><body>`) via the HTML parser and exposes the document factory methods (`createElement`/`createElementNS`/`createTextNode`/`createComment`/`createDocumentFragment`/`importNode`/`adoptNode`) plus `documentElement`/`head`/`body`/`title` (`create_html_document` in `rust/js-dom-factory/src/implementation.rs`). Nodes it creates are real and adoptable into the main document, which is what jQuery's `$.parseHTML` / `buildFragment` use to parse markup off-document |
| `DOMParser` / `XMLSerializer` | ✅ | The returned `Document` carries the live spec accessors — `documentElement`, `body`, `head`, `title`, `nodeType` (`= 9`) — set up by `parse_from_string` (`rust/js-dom-factory/src/constructors.rs`); `text/html` parses through the full HTML document parser (auto-wraps `<html><head><body>`); MIME types with `xml` or `svg` parse through the fragment parser so the supplied root (e.g. `<svg>`) becomes `documentElement` rather than being wrapped; malformed XML input yields a document whose root is a `<parsererror>` element (the contract `jQuery.parseXML` relies on to throw) |
| `fetch` / `Response` body | ✅ | binary-safe: response bytes are attached as an `ArrayBuffer` on `_bodyBuffer` and the body consumers (`text` / `json` / `blob` / `arrayBuffer` / `bytes` / `formData`) read from it through `TextDecoder` / `Uint8Array`, so non-UTF-8 bytes survive round-tripping (PNG, MP4, etc.) instead of being mangled by JS-string conversion |
| `Response.body` / `Request.body` (`ReadableStream`) | ✅ | `body` is a readable stream: `getReader()` yields the bytes as a single `Uint8Array` chunk then closes, and `body.pipeThrough(new DecompressionStream(...))` works. A `Request`/`Response` can also be **constructed from** a `ReadableStream` body — the consumers (`text`/`arrayBuffer`/…) drain it. Bodyless requests report `body === null`. The stream is single-chunk (not incremental network delivery) and not a real `tee`, so `clone()` of a stream-backed body shares the underlying stream |
| `Request`/`Response` body extraction + `Content-Type` | ✅ | the `Request`/`Response` constructors serialize every body type and infer the default `Content-Type` (unless one is given): string → `text/plain;charset=UTF-8`; `URLSearchParams` → `application/x-www-form-urlencoded;charset=UTF-8`; `Blob` → its `type`; `FormData` → `multipart/form-data` with a CSPRNG boundary (with per-part `Content-Disposition`/filenames); `ArrayBuffer`/typed array → raw bytes, no type. The serialized bytes are what `text()`/`arrayBuffer()` read and what `fetch()` sends, so both `fetch(url, {body})` **and** `fetch(new Request(url, {body}))` upload attachments and form posts correctly |
| `XMLHttpRequest.responseType` | ✅ | `""`/`"text"`, `"json"`, `"arraybuffer"` (`ArrayBuffer`), `"blob"` (`Blob`, `type` from `Content-Type`), `"document"` (delegates to `DOMParser`, XML mode picked when the response is `*xml*` or SVG); `responseText` stays populated regardless for compatibility |
| `AbortSignal` + `fetch(url, {signal})` | ✅ | `AbortSignal.abort(reason)` and `AbortController().abort(reason)` cancel an in-flight `fetch` — the in-flight request is cancelled through its cancellable (`on_signal_abort` in `rust/js-net/src/fetch.rs`), the promise is rejected with the signal's `reason` (or a synthetic `AbortError`), and the abort listener is cleaned up when the state is freed. A pre-aborted signal rejects synchronously without touching the network. `AbortSignal.timeout(ms)` schedules a real main-loop timeout (`static_timeout` in `rust/js-net/src/abort.rs`) that flips the signal and fires `abort` with a `TimeoutError`. `AbortSignal.any([s1, s2, …])` returns a composite signal that aborts as soon as any input does, copying the firing signal's `reason` (and is immediately aborted if any input is already aborted) |
| `navigator.sendBeacon(url, body?)` | ✅ | fire-and-forget HTTP POST through the regular fetch pipeline (`send_beacon` in `rust/js-net/src/beacon.rs`); body type is inferred and the Content-Type chosen accordingly — string → `text/plain;charset=UTF-8`, `Blob`/`File` → `blob.type`, `URLSearchParams` → urlencoded, `FormData` → multipart with CSPRNG boundary, `ArrayBuffer`/typed array → raw bytes. Same-origin policy and the rest of the fetch security pipeline apply. Returns `false` on non-string URL or non-`http(s)` scheme; otherwise `true` |
| `navigator.clipboard.writeText(text)` | 🟡 | in a windowed context, writes through the GTK clipboard (`gdk_clipboard_set_text`) and resolves; in headless / no-clipboard contexts rejects with `NotAllowedError`. Other clipboard methods (`readText`/`read`/`write`) still reject — read access is intentionally not exposed |
| Cookie Store API | 🟡 | `cookieStore` exposes promise-based `get`/`getAll`/`set`/`delete` over the document cookie jar and chains to the non-constructible `CookieStore.prototype`; change events and service-worker exposure are not implemented |
| `navigator.*` device APIs (geolocation, sensors, …) | 🚫 | by design |

## §9 Communication

| API | Status | Notes |
|-----|:--:|------|
| `MessageEvent` | ✅ | |
| `WebSocket` (`ws://`/`wss://`) | ✅ | in-tree RFC 6455 client over `rust/http` (`rust/websocket`: frame codec and handshake checks, text/binary, fragmentation, ping/pong, close, subprotocol negotiation; bindings in `rust/js-net/src/socket.rs`) |
| Cross-document `postMessage` | ✅ | `window.postMessage` `structuredClone`s the payload and delivers it asynchronously (task) as a fully-formed `MessageEvent` — `data` (the clone), `origin` (sender origin), `source` (sender `WindowProxy`), `lastEventId` `""`, empty `ports` — to both `onmessage` and `addEventListener('message')`. `targetOrigin` is honoured (`"*"`/`"/"`/exact-origin match, else dropped). The `MessageEvent` constructor itself now reflects every dictionary member (`data`/`origin`/`lastEventId`/`source`/`ports`) with spec defaults (`message_event_ctor` in `rust/js-events/src/ctors.rs`). Delivery between *separate* browsing contexts is bounded by the shared single-runtime model (one window object per tab), so messaging is realised within a browsing context rather than isolated per child frame |
| `MessageChannel` / `MessagePort` | ✅ | `new MessageChannel()` entangles `port1`/`port2` (both chain to the real `MessageChannel`/`MessagePort` prototypes, so `instanceof` works). `postMessage` `structuredClone`s the payload and delivers it asynchronously (microtask) to the entangled port as a `MessageEvent`; the port message queue starts disabled, so messages are buffered until the receiving port is enabled — assigning `onmessage` enables it (and flushes the backlog) and `start()` enables it explicitly, matching the spec rule that `addEventListener('message')` without `start()` receives nothing. `close()` detaches the port. Cross-agent port *transfer* is out of scope under the shared single-runtime model |
| Server-sent events (`EventSource`) | ✅ | streaming client over `rust/http` (`rust/eventsource`, bindings in `rust/js-net/src/socket.rs`): `onopen`/`onmessage`/`onerror`, named events via `addEventListener`, multi-line `data`, `id` → `lastEventId`, `retry`, automatic reconnection with `Last-Event-ID`, and `close()`; HTTPS pages refuse `http:` streams and `connect-src` CSP is enforced |

## §10 Web workers · §11 Worklets

✅ **Dedicated workers are implemented** (classic and module). `new
Worker(url, options)` creates a dedicated worker in its own JavaScript
runtime on its own thread; `{type: 'module'}` evaluates the entry script
as an ES module with static and dynamic `import` resolved through the
same module loader the main realm uses (`importScripts` throws a
`TypeError` there, per spec). Supported surface: same-origin worker
script loading, `worker.postMessage()`, worker-global `postMessage()`,
`message` / `messageerror` / `error` events, `onmessage`,
`addEventListener`, worker-thread timers, `close()`, `terminate()`,
worker `location`, `navigator`, `performance.now()`, `URL`,
`URLSearchParams`, `TextEncoder` / `TextDecoder`, `atob` / `btoa`,
`structuredClone`, synchronous classic `importScripts()`, `XMLHttpRequest`,
`fetch()` with `Request` / `Response` (the worker runs the async fetch
on its own event loop and reads the body back through the standard
consumers — verified with a `data:` round-trip inside a worker), and
`crypto` (`getRandomValues`, `randomUUID`, and the full `crypto.subtle`
SubtleCrypto surface — verified by computing `SHA-256` inside a worker).

Worker script creation is guarded by mixed-content checks and CSP
`worker-src` with the expected fallback through `child-src`,
`script-src`, and `default-src`. Message payloads cross runtimes as a
wire graph (`rust/js-clone`) turned into bytes by the engine's object
serializer (`Scope::write_object` in
`rust/js-engine/src/boa/serialize.rs`); functions and SharedArrayBuffer are
rejected. A `postMessage`
transfer list transfers its `ArrayBuffer`s — they are serialized to the
receiver and then detached in the sender (`Scope::detach_array_buffer` in `rust/js-engine/src/boa.rs`), and a
non-transferable entry throws `DataCloneError`.

✅ **Service Workers** are supported, including `FetchEvent` interception.
`navigator.serviceWorker` exposes `register()` / `getRegistration()` /
`getRegistrations()` / `ready` / `controller` plus the `controllerchange`
and `message` events. `register()` fetches and runs the worker script in a
real `ServiceWorkerGlobalScope` (its own thread) — same-origin,
mixed-content, and CSP `worker-src` rules apply — then drives the
lifecycle: the `install` and `activate` `ExtendableEvent`s fire (with
`waitUntil`), the worker's `state` transitions `installing → installed →
activating → activated` (each firing `statechange`), and on activation
`controller` is set and `ready` resolves. The scope provides
`self.registration`, `self.clients` (`claim` / `matchAll` / `get`),
`self.skipWaiting()`, `caches`, and the `FetchEvent` /
`ExtendableMessageEvent` interfaces, and `postMessage` works in both
directions (SW↔page). **Network interception works** for `fetch()` from a
controlled page: when an activated worker's scope covers the request URL,
the request is dispatched to the worker as a `FetchEvent` (with a real
`request` and `respondWith`); if the handler calls `respondWith(r)` the
page's `fetch` resolves with that response (the `Response` is read to bytes
on the worker thread and reconstructed on the page — `rust/js-workers/src/service.rs`), and `respondWith` accepts a `Response`, a `Promise<Response>`,
or `caches.match(...)`. If the handler does not respond, the request falls
through to the normal network path. Registrations live for the session
rather than persisting across loads, and navigations/subresources fetched
by the engine are not yet routed through the worker (only page `fetch()`
is intercepted).

The **Cache API** (`caches` / `CacheStorage` / `Cache`) is a working
in-memory store (in `rust/js-runtime/js/polyfills.js`). `caches.open(name)`
returns a real `Cache`; `put(request, response)` reads the response body to
an `ArrayBuffer` and stores it with status/headers/url (rejecting a used
body, a `206` response, or a non-`GET` request per spec), and `match` /
`matchAll` reconstruct a fresh `Response` (honouring `ignoreSearch`).
`add` / `addAll` fetch then store, `delete` / `keys` work, and the
top-level `caches.has` / `delete` / `keys` / `match` operate across the
named stores. The cache lives for the session and is per-runtime — the
Service Worker thread has its own, which it can populate and serve from in
its `FetchEvent` handler (`event.respondWith(caches.match(event.request))`).

🚫 `SharedWorker`, worklets, transferable `MessagePort`s, and nested
workers are not implemented yet.

## §12 Web storage

| API | Status | Notes |
|-----|:--:|------|
| `localStorage` | ✅ | persistent, origin-partitioned, **no quota enforced** |
| `sessionStorage` | ✅ | per-session |
| `storage` event | ✅ | the `StorageEvent` interface is complete — the constructor reflects every dictionary member (`key`/`oldValue`/`newValue`/`url`/`storageArea`) with spec defaults (`key`/`oldValue`/`newValue` `null`, `url` `""`, `storageArea` `null`), the legacy `initStorageEvent(type, bubbles, cancelable, key, oldValue, newValue, url, storageArea)` is provided (`storage_event_ctor` in `rust/js-events/src/ctors.rs`), and `onstorage`/`addEventListener('storage')` deliver dispatched events with intact fields. A storage area mutation does **not** fire a `storage` event on the window that made the change (per spec), and with one browsing context per tab there is no other same-origin context to notify — so same-tab mutations correctly fire nothing |
| IndexedDB | 🟡 | SQLite-backed, origin-partitioned; supports databases, version upgrades, object stores, indexes, key ranges, cursors, requests/events, and value persistence through the engine's structured serialization (`write_object`/`read_object` in `rust/js-engine/src/boa/serialize.rs`: Dates, Maps, Sets, typed arrays, shared references and cycles round-trip). Transaction rollback/blocking semantics and cross-context versionchange coordination are simplified |

## §13 The HTML syntax

The heart of spec conformance. Tokenisation and tree construction —
the bulk of §13.2 — are done by **`rust/html5`**, Southstar's
from-scratch WHATWG-conformant Rust implementation
(`rust/html-parser` is its DOM sink). Because the parser is a conformant engine, the
DOM tree Southstar builds for a given byte stream is spec-faithful
**even for malformed input**; the compatibility gaps elsewhere in this
document are in *rendering and behaviour*, not parsing.

| Topic | Status |
|-------|:--:|
| Tokenizer state machine (§13.2.5) | ✅ |
| Tree construction / insertion modes (§13.2.6) | ✅ |
| Foreign content (SVG/MathML namespaces) | ✅ (SVG parsed to DOM; MathML parsed to DOM **and laid out** — see §4.8; per-element namespace set at parse time — the tree sink in `rust/html-parser` flags SVG elements `FLAG_SVG_NS` and MathML elements `FLAG_FOREIGN_NS` (`rust/dom`) with the MathML URI, so a parsed inline `<svg>`/`<math>` reports the correct `namespaceURI`/`localName`/`tagName`, satisfies `instanceof SVGElement`-style namespace checks, and is matched by `getElementsByTagNameNS`; `<foreignObject>` content correctly reverts to the XHTML namespace) |
| Adoption agency / error recovery | ✅ |
| Named & numeric character references | ✅ |
| Fragment parsing (`innerHTML`) | ✅ |
| HTML parsing methods (`setHTML`, `setHTMLUnsafe`, `appendHTML`/`prependHTML`/`beforeHTML`/`afterHTML`/`replaceWithHTML` and their `Unsafe` variants) | ✅ (`rust/js-html/src/positional.rs`: parsed with the receiver, the parent, a shadow root's host or template contents as context; declarative shadow roots attach; the spec's `HierarchyRequestError` cases; safe variants are no-ops on HTML/SVG `script` and drop script-capable markup; `runScripts` for the `Unsafe` variants. The `sanitizer` option takes a `SanitizerConfig` or a `Sanitizer` — element/attribute allow and remove lists, `replaceWithChildrenElements`, `comments`, `dataAttributes` — but not the Sanitizer API's built-in safe-default allowlist; Trusted Types parser options are not implemented. WPT `domparsing/positional-methods.html` 480/480) |
| Serialisation (`outerHTML`) | ✅ |
| `document.write` | ✅ (`document.write`/`writeln` insert parsed markup at the script's position during parsing; `document.open()` blanks the document — clearing all nodes and rebuilding a fresh `<html><head><body>` (with the tag/id/class indices rebuilt) — and returns the document; `document.close()` flushes any pending write. A `write` after the document has finished loading (`readyState` `complete`) triggers an implicit `open()` first, exactly as the spec requires, so a late `document.write` replaces the page rather than appending — `open`/`write_common` in `rust/js-document/src/write.rs`) |

## §14 The XML syntax

🟡 **Partial.** XML, XHTML and SVG resources are parsed by a real
namespace-aware XML parser (`southstar_xml::parse_document`, `rust/xml`), not the HTML
parser. A recursive-descent tokeniser builds the live `rust/dom` DOM
following the same conventions as `createElementNS` — qualified name,
`data-nd-ns-uri` / `data-nd-ns-prefix`, the SVG / foreign / keep-case
flags — resolving each element's and attribute's namespace from the
in-scope `xmlns` / `xmlns:prefix` declarations, and handling elements,
attributes, text, `CDATASection`s, comments, processing instructions,
`DOCTYPE`, the predefined entities plus numeric character references,
and self-closing tags. This drives two paths:

- `DOMParser.parseFromString(s, "application/xml" | "text/xml" |
  "application/xhtml+xml" | "image/svg+xml")` returns a real XML
  document whose `documentElement` is the parsed root with correct
  `namespaceURI` / `localName` / `prefix`.
- An `<iframe>` whose response is `application/xhtml+xml`,
  `text/xml` / `application/xml`, or `image/svg+xml` is parsed as XML,
  its `contentDocument` is an `XMLDocument` (so `createElement` keeps
  case and resolves the HTML-versus-null namespace per spec, and
  `createCDATASection` / `createProcessingInstruction` work), and
  well-formedness is enforced — a malformed XHTML frame falls back to a
  parse-error page.

Still partial: the top-level *navigation* path for a page served as
`application/xhtml+xml` is still handled by the HTML parser (only
embedded frames and `DOMParser` use the XML path); XML `Name`
validation is ASCII-pragmatic rather than the full Unicode production;
and only the five predefined entities (plus `&nbsp;`) and numeric
references are recognised — DTD-declared entity sets are not.

## §15 Rendering

Layout lives in `rust/layout` and the `rust/layout-*` crates (block/inline
flow, flexbox, grid, positioning, floats, stacking); painting is cairo
(`rust/paint`) with text from `rust/text`. The UA stylesheet
(`rust/css/src/ua.css`) supplies the default rendering the spec mandates for each
element.

CSS support (abridged):

- ✅ Box model, `display` (block/inline/flex/grid/table/table-caption/
  list-item/flow-root), sizing incl. `aspect-ratio` and the intrinsic
  `width` keywords `min-content` / `max-content` / `fit-content` on
  block boxes (shrink-to-fit via the same content-measurement used by
  tables; `getComputedStyle` reports the resolved pixel width),
  margins/padding, borders,
  `border-radius`, outline, `box-shadow`, `text-shadow`.
- ✅ Flexbox and Grid (incl. `fr`, `minmax()`, `auto-fit/fill`, `gap`,
  `grid-auto-flow` — `dense` back-filling and `column` flow with
  `grid-auto-columns`-sized implicit columns — and the legacy
  `grid-gap`/`grid-row-gap`/`grid-column-gap` aliases).
- ✅ `quotes` — author quote pairs (CSS string escapes decoded) drive
  `<q>` nesting and `open-quote`/`close-quote` generated content;
  `quotes: none` suppresses the marks.
- ✅ Positioning (`static/relative/absolute/fixed/sticky`), `float`/
  `clear`, `z-index` stacking contexts.
- ✅ CSS Values & Units viewport/container/logical unit subset including
  `em`/`rem`, `sv*`/`lv*`/`dv*`, `vi`/`vb`, `cqi`/`cqb`, `cap`, `ic`,
  and `Q`.
- ✅ Typography, CSS Color 4 `rgb()`/`hsl()`/`hwb()` space/slash syntax
  plus `lab()`/`lch()`/`oklab()`/`oklch()` conversion to sRGB, and
  CSS Color 5 `color-mix(in srgb, ...)`,
  backgrounds (incl. linear/radial/conic gradients and external
  stylesheet-relative `url(...)` resources),
  transforms (2D/3D), transitions/animations (`opacity`/`transform`/
  `color`/`background-color`), inline font/style/stretch ranges measured
  for wrapping and line height, `object-fit`, `mask-image`,
  `accent-color`, `caret-color`, `tab-size`, `pointer-events`, custom properties + `calc()` and
  the Values 4 length math subset (`round()`/`mod()`/`rem()`/`abs()`).
- ✅ `border-image` — the `border-image-source`/`-slice`/`-width`/`-outset`/
  `-repeat` longhands, the `border-image` shorthand and the
  `-webkit-border-image` alias, with the `border` shorthand resetting them.
  `paint_border_image` in `rust/paint` nine-slices a `url()` raster image or
  a gradient rasterized to the border-image area, honouring `fill`, number
  and percentage slices, `auto`/length/percentage/number widths, outsets and
  the `stretch`/`repeat`/`round`/`space` tiling modes on both axes; it
  replaces the element's border style while it renders, and falls back to
  that style until the image loads. Block-level boxes only.
- ✅ `text-indent` — indents the first formatted line of a block's
  inline content (length or percentage of the content width), including
  large negative sprite-hiding indents used with clipped inline-block
  logos (`text_indent_px` in `rust/layout-geom`).
- ✅ `text-decoration` — all three lines (`underline`, `overline`,
  `line-through`, singly or combined, also settable through the
  `text-decoration-line` longhand), all five `text-decoration-style`
  keywords (`solid`/`double`/`wavy` via the matching Pango line
  attributes, and `dotted`/`dashed` drawn as a custom dashed cairo stroke
  — for the underline, line-through *and* overline of a run — since Pango
  has no dotted/dashed line style) and `text-decoration-color`, at paint
  time (`paint_inline` / `paint_inline_dashed_decorations` in
  `rust/paint`). This makes the UA `abbr[title]` dotted underline and
  Firefox's `text-decoration` reftests render correctly. Decorations
  apply both
  to inline elements and to block-level boxes that propagate the property
  to their inline content (`rust/layout-build`); `none` clears inherited
  decorations.
- ✅ `white-space`: `normal`/`nowrap` collapse, `pre`/`pre-wrap`/
  `break-spaces` preserve, and `pre-line` collapses runs of spaces/tabs
  while preserving newlines as forced breaks (`white_space_mode` in
  `rust/layout-build/src/collect.rs`); `text-wrap`/`text-wrap-mode` map `nowrap` to the same
  no-wrap path and wrapping keywords to normal wrapping. CSS `hyphens`
  is inherited; `auto` enables Pango inserted hyphens using the element
  language, while `manual`/`none` keep automatic insertion disabled.
- ✅ Multi-column layout: `column-count`/`column-width`/`columns`, gaps,
  rules, balanced block-child distribution, and plain inline text runs
  fragmented by wrapped line while preserving links and text-style ranges.
- ✅ Selectors: type/class/id, combinators, attribute, structural &
  state pseudo-classes including `:any-link`, `:lang()`, `:dir()`,
  `:open`, `:popover-open`, `:modal`, `:default`, `:indeterminate`,
  `:in-range`, `:out-of-range`, `:blank`, `:target-within`,
  `:nth-last-*`, `:only-of-type`, and
  `:nth-child(... of selector)`,
  `:is()/:where()/:has()` (bounded),
  `::before/::after`, `::first-letter`, `::first-line`, `::placeholder`.
  The dynamic `:hover` pseudo-class is live in the out-of-process
  renderer: the element under the pointer and its ancestors match
  `:hover` and the page restyles/repaints as the pointer moves
  (`set_hover_node` in `rust/css/src/ffi/matcher.rs`, `ns_browser_hover` in
  `rust/browser`). The dynamic `:active` pseudo-class is live
  too: a primary-button press sets the active element
  (`set_active_node` in `rust/css/src/ffi/matcher.rs`, set in `ns_browser_click`) so the pressed
  element and its ancestors match `:active` and the page restyles; the
  button release clears it through a dedicated renderer `release`
  message (`ns_browser_release`, wired to the GTK click gesture's
  `released` signal). Restyle work is
  gated to pages whose stylesheets actually contain `:active` rules
  (`has_active_rules` in `rust/css/src/ffi/sheet.rs`).
- ✅ At-rules: `@media`, `@font-face`, `@keyframes` (subset),
  `@supports` (now evaluated, incl. `selector()`), `@scope`
  (selector-form roots/limits, relative scoped selectors, `:scope`,
  and scope proximity), `@property`
  (registers custom-property `initial-value` and the `inherits`
  descriptor — registered properties resolve to their initial value
  when unset, and `inherits: false` properties do not inherit from the
  parent; `syntax` is parsed but not yet type-validated).
- ✅ Logical sizing/spacing aliases and shorthands including
  `margin-inline/block`, `padding-inline/block`, `inset-inline/block`,
  `border-inline/block`, logical border width/style/color pairs, and
  logical corner-radius aliases.
- ✅ Media Queries: width/height/orientation, range syntax, aspect-ratio,
  resolution, color-gamut, scripting, display-mode, update,
  dynamic-range, overflow-block/inline, grid/monochrome/color, and user
  preference features including reduced data and inverted colors.
- ✅ `filter` (grayscale/sepia/invert/brightness/contrast/saturate/blur/drop-shadow),
  `clip-path` (basic shapes), `mix-blend-mode`, `-webkit-line-clamp`.
- ✅ `:focus`, `:focus-within`, `:focus-visible`, `:placeholder-shown`,
  `:checked`, `:valid`, `:invalid`.
- ✅ `::selection` — author `background-color`/`color` honoured for the
  on-page text selection (`rust/selection`, pseudo style resolved in
  `rust/css`).
- ✅ `::first-line` — the first formatted line of a block adopts the
  pseudo's `color`, `background-color`, `font-size`/`weight`/`style`/
  `family`, `font-kerning`, `font-variant: small-caps`,
  `font-variant-ligatures`, `font-feature-settings`,
  `font-variation-settings`, and `text-decoration: underline`. The
  first line's extent is taken from the wrapped Pango
  layout at paint time (`apply_first_line_attrs` in `rust/paint`), so
  it tracks the actual wrap width; cascades correctly with other
  `::first-line` rules and composes with `::first-letter`.
- ✅ `::marker` — author `color`/`font-size` honoured for list-item
  markers (`paint_marker` in `rust/paint`); `display:none` hides the
  marker, and string / `counter(list-item)` `content` overrides the
  generated bullet/number. `list-style-image` paints a fetched `url(...)`
  image in place of the bullet (scaled down to the font size when
  larger), losing only to a `::marker` `content` override per spec.
- ✅ `user-select: none` — the subtree's text is skipped by drag
  selection, select-all, highlight painting, and clipboard collection
  (`rust/selection`); the `-webkit-`/`-moz-` prefixed forms alias to it.
- 🟡 `::backdrop` — author `background`/`background-color` painted as a
  full-viewport fill behind a modal dialog's top-layer box
  (`paint_top_layer` in `rust/paint`); only the modal-`dialog` top
  layer is covered (popovers not yet), and only the backdrop's
  background paints.
- 🟡 `subgrid` — a grid item that is itself a grid container with
  `grid-template-columns: subgrid` adopts its parent grid's spanned
  column tracks (sizes, positions, and gap), so its own children align
  to the outer column grid (`layout_grid` in
  `rust/layout-grid/src/layout.rs`); honours partial spans
  (`grid-column: 1/3`) and falls back to a single auto column when
  there is no parent grid. Row-axis `grid-template-rows: subgrid`
  adopts concrete parent row tracks (`grid-template-rows` / fixed
  `grid-auto-rows`) and the parent row gap; fully auto-sized parent
  rows still fall back to the regular auto-row path.
- 🟡 Bidirectional text: `direction` (inherited, `ltr`/`rtl`) feeds the
  Pango base direction and the start/end resolution of `text-align`. All
  `unicode-bidi` values are honoured by emitting the matching Unicode bidi
  formatting controls around the element's content (override LRO/RLO…PDF
  for `bidi-override`; isolates LRI/RLI/FSI…PDI for `isolate`/`plaintext`,
  combined for `isolate-override`; UA rules give `bdo` override and `bdi`
  isolate) so `rust/bidi` performs real UAX#9 isolation/override. The residual
  gap is exposing the computed *used* direction of `dir=auto` elements to
  `:dir()`.
- 🟡 Writing modes: `writing-mode` and `text-orientation` are parsed and inherited
  (`writing_mode_of` / `text_orientation` in `rust/css/src/style_query.rs`); a
  block/inline-block container in `vertical-rl` / `vertical-lr` (and the
  `sideways-*` aliases) lays its text out in vertical columns — measured with the
  inline and block axes swapped (`inline_layout` in `rust/layout-inline/src/layout.rs`) and painted in
  `paint_inline` (`rust/paint`). Two orientations render:
  `text-orientation: mixed` (the default) and `sideways` set each column's glyphs
  rotated 90° **clockwise** (per line, so horizontal-script/Latin runs read
  top-to-bottom by turning the page clockwise — the spec-correct default); columns
  progress right-to-left for `vertical-rl` and left-to-right for `vertical-lr`.
  `text-orientation: upright` stacks every character upright (grapheme per row).
  Remaining: under `mixed`, upright scripts (CJK) are not yet kept upright while
  Latin rotates — the whole run rotates, so `mixed` currently matches `sideways`
  for mixed-script text; plus automatic wrapping into columns at the block height
  (columns form only at explicit line breaks) and vertical layout of
  form/replaced boxes.

Replaced elements (`img`/`video`/`canvas`) are sized via the media-box
path in `rust/layout-sizing`.

## §16 Obsolete features

| Element | Status | Notes |
|---------|:--:|------|
| `frame` / `frameset` | 🚫 | `display:none`; not rendered |
| `applet` | 🚫 | no Java; `display:none` |
| `marquee` | 🚫 | `display:none`; not rendered |
| `basefont` `noembed` `isindex` | 🚫 | `display:none` / inert |
| Presentational attributes (`align`, `bgcolor`, `<font>`) | 🟡 | a subset mapped to CSS where common (`presentational_hints` in `rust/css/src/hints.rs`): `bgcolor`/`text`/`<font color/face/size>`, `width`/`height`, `hspace`/`vspace`, `<hr align/color/size/noshade>` (`color` drives the rule's `background-color`, `noshade` renders a solid grey bar), `<img align>` (left/right float plus top/middle/bottom `vertical-align`), `<textarea wrap=off>`→`white-space: pre`, `<table frame/rules>` (see §4.9), and the table family — `cellspacing`→`border-spacing`, `cellpadding`→cell `padding`, `<td/th align/valign/nowrap>`, `<table align=left/right>`→`float` and `align=center`→auto side margins (centred by `rust/layout-table`), and **`<table border=N>`** maps the legacy grid: an outer table border plus a `1px solid` border on every `td`/`th` (the classic `border="1"` grid), while `border="0"` stays gridless. The legacy **list attributes** are honoured in `rust/paint`: `<ol start>`/`<ol reversed>`/`<li value>` drive ordinal numbering (`list_item_ordinal` — start seeds the first number, reversed counts down, a `value` resets the running count), and `<ol type>`/`<ul type>`/`<li type>` map to `list-style-type` as a presentational hint (`presentational_hints` in `rust/css/src/hints.rs`, applied at the presentational-hint origin, `ORIGIN_PRESENTATIONAL` in `rust/css/src/cascade.rs`, so it overrides the UA default but loses to author CSS): ol/li `1`/`a`/`A`/`i`/`I` → decimal / lower- and upper-alpha / lower- and upper-roman (case-sensitive), ul/li `disc`/`circle`/`square` (case-insensitive) |

---

## Design constraints (the spec we deliberately do not implement)

These are project non-goals (see `CLAUDE.md` / `README.md`), not
defects, and will not be added:

- AI-style web APIs. (WebGL is supported and enabled by default; see §4.12.
  **WebGPU** is experimental: built over
  wgpu-native whenever that library is present and enabled at runtime with
  `--enable-webgpu`; `navigator.gpu` plus a working `getContext("webgpu")`
  render/compute path.)
- Shared Workers and Worklets. (Service Worker `FetchEvent` interception
  of page `fetch()` **is** supported — see §10/§11; what remains is routing
  the engine's navigations/subresources through the worker.)
- **WebRTC** and **EME** (DRM media). MSE itself is implemented for inline
  segmented playback.
- Plugin content (`embed`/`object` via NPAPI/PPAPI).
- A broad vendored codec collection. MPEG-1/MP2 and MP3 are in-tree; WebM
  playback uses optional system-provided FFmpeg components and the dedicated
  audio/video helper processes described in [`media.md`](media.md).
- **JIT** (interpreter-only, to preserve W^X process-wide).
- Telemetry, crash reporters, update pingers, "studies".
- gettext / `.po` localisation tooling. (UI strings *are* translated to
  the OS language through the in-tree `rust/i18n` catalogue lookup over
  `data/i18n/*.lang`; English is the source and fallback.)

## Highest-leverage HTML/CSS gaps for real-world sites

Ordered by how often they block ordinary browsing:

1. Remaining row-axis `subgrid` sizing edge cases — concrete parent
   rows are adopted; fully auto-sized parent rows still use the regular
   auto-row path.
2. Remaining `table-layout` edge cases (advanced auto-layout sizing and shared-edge border collapsing; inter-cell `border-spacing`/`border-collapse` spacing is now honoured).

(`subgrid`, `multipart/form-data` for `fetch`/`XHR`
bodies, the Microdata DOM API, table captions with `caption-side`,
`::selection`, `::marker`, `:focus-visible`, `:focus-within`,
sequential focus navigation (`tabindex`/`inert`), the modal-dialog
top layer / focus trap, `::backdrop`, native HTML drag-and-drop gesture
dispatch, and single-text-run multi-column fragmentation have since been
implemented.)

## Site compatibility

Top-level URLs, request policy, DOM construction and media discovery all use
the generic engine paths. Southstar does not rewrite selected hosts to
mobile variants, scrape private player JSON, inject per-site CSS, or synthesize
site-specific DOM. Compatibility fixes belong in the relevant HTML, CSS,
JavaScript, networking or media implementation.

## How to re-check this document

There is **no automated conformance suite** (by project policy).
Validate any row by running the browser against a page that exercises
the feature and observing the result. Treat this file as a living map
and update it whenever behaviour changes.

Interaction can be scripted in headless mode with `--act`, a
`;`-separated list of `click X,Y`, `drag X,Y X,Y`, `type TEXT`,
`key NAME`, and `wait N`
(`Enter`/`Backspace`/`Delete`/`Left`/`Right`/`Up`/`Down`/`Home`/`End`)
steps run after the page settles, before the dump. `wait` settles the main
loop for `N` milliseconds. The clicks drive the
real hit-test, focus, checkbox/radio toggling, `<label>`→control
activation, `<summary>` disclosure of `<details>`, link navigation,
native HTML drag-and-drop dispatch, and JS `click`/`input`/`change`
dispatch; `type` honours `maxlength` and filters `type=number` input,
and `Up`/`Down` step a focused number input — so e.g.

```sh
southstar --headless --url=FILE --viewport=800 \
  --dump=png:out.png --act='click 120,180; type hello; key Enter; wait 500'
```

renders the result of focusing a control, typing, and pressing Enter —
useful for exercising forms and `contenteditable` without a window.

Layout can be inspected — like a browser's "Inspect" panel — with
`--inspect=SELECTOR` and `--inspect-at=X,Y`:

```sh
southstar --headless --url=URL --inspect='.cdx-search-input'
southstar --headless --url=URL --inspect-at=400,18
```

`--inspect` matches a CSS selector and, for each match, prints the box
model (content / border-box / margin / border / padding in px), the key
computed styles (`display`, `position`, `box-sizing`, sizing, the flex
properties, `font-size`), the laid-out children with their geometry and
`flex-grow`, and the full DOM path; elements that match but were not laid
out (e.g. `display:none`) are reported as such. `--inspect-at` hit-tests
the point and prints the element stack there (innermost first) followed
by the hit element's details. Used alone, either flag suppresses the
default text dump; combine with `--dump=` to get both.
