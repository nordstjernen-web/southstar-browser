# CSS status

Southstar's CSS support, laid out on the same template as Blitz's
[CSS status page](https://blitz.is/status/css): the same sections and the
same rows, so the two engines can be read side by side. Each row was checked
against the engine source (`rust/css` parses and cascades, `rust/layout-*`
lays out, `rust/paint` paints); a property that is parsed and reported by
`getComputedStyle` but has no effect on layout or paint counts as not
supported. [CSS-compatibility.md](CSS-compatibility.md) is the module-by-module
map with the details, and [HTML-compatibility.md](HTML-compatibility.md) covers
HTML.

Snapshot: 2026-11-15. Of the template's 254 rows, 186 are ✅, 15 ⚠️ and 53 ❌
(Blitz's own page shows about 136 ✅ on the same rows).

**Legend:** ✅ supported · ⚠️ partial · 🚧 in progress · ❌ not supported
(❌ *parsed* means the value is accepted, cascaded and serialized but not
rendered).

## Core

| Feature | Status | Notes |
|---------|:--:|-------|
| `display: inline` | ✅ | |
| `display: block` | ✅ | also `flow-root`, `list-item` |
| `display: inline-block` | ✅ | |
| `display: flex` | ✅ | |
| `display: grid` | ✅ | |
| `display: table` | ✅ | the full `table-*` family; `table-layout: auto` widths are an approximation |
| `display: grid-lanes` | ❌ | |
| `display: none` | ✅ | |
| `display: contents` | ✅ | |
| `position: static` | ✅ | |
| `position: relative` | ✅ | |
| `position: absolute` | ✅ | an inline element can be the containing block |
| `position: fixed` | ✅ | composited as a viewport layer |
| `position: sticky` | ✅ | |
| `overflow: visible` | ✅ | |
| `overflow: clip` | ✅ | |
| `overflow: hidden` | ✅ | |
| `overflow: scroll` | ✅ | working scrollbars and hit-testing |
| `overflow: auto` | ✅ | |
| `z-index` | ✅ | stacking contexts per CSS 2.1 Appendix E |
| `box-sizing` | ✅ | |
| `cursor` (keyword) | ✅ | resolved at the hovered point, `auto` per UI 4 |
| `cursor` (custom image) | ❌ | `url(...)` falls back to its keyword |
| `content` string | ✅ | also `attr()` and quotes |
| `content` image | ✅ | a single `url()`, inline, inline-block or block, on inline and block hosts |
| `content` gradient | ❌ | |
| `content` counter | ✅ | `counter()` / `counters()` |
| `float` | ✅ | |
| `clear` | ✅ | |
| `order` | ✅ | |
| `zoom` | ❌ | parsed; the browser's page zoom is separate |
| `all` | ✅ | |
| `overflow-clip-margin` | ❌ | parsed |

## Visibility & isolation

| Feature | Status | Notes |
|---------|:--:|-------|
| `opacity` | ✅ | |
| `visibility` | ✅ | `visible` / `hidden` / `collapse` |
| `contain: layout` | ❌ | parsed |
| `contain: paint` | ✅ | clips to the padding box (also `content`, `strict`) |
| `contain: style` | ❌ | parsed |
| `contain: size` | ✅ | height ignores the contents, from `contain-intrinsic-size` or zero (also `strict`) |
| `contain: inline-size` | ❌ | parsed; container queries use `container-type` |
| `isolation` | ✅ | `isolate` paints the box as a group, so descendant blend modes stay inside it |
| `content-visibility` | ⚠️ | `hidden` works; `auto` does not skip off-screen content |
| `contain-intrinsic-size` | ⚠️ | the height used by size containment; `auto` is accepted and ignored, the width is not used |

## Clipping & masking

| Feature | Status | Notes |
|---------|:--:|-------|
| `clip-path` | ⚠️ | `inset()`, `circle()`, `ellipse()`, `polygon()`, `xywh()`, `rect()` and geometry boxes; `path()` / `shape()` are not drawn |
| `mask-image` | ✅ | `url()` and linear/radial gradients; conic masks are skipped |
| `mask-position` | ✅ | |
| `mask-repeat` | ✅ | |
| `mask-size` | ✅ | |
| `mask-composite` | ✅ | |
| `mask-type` | ❌ | parsed |
| `mask-mode` | ❌ | parsed; masks are alpha masks |
| `mask-origin` | ❌ | parsed |
| `mask-clip` | ✅ | |

## Sizing & positioning

| Feature | Status | Notes |
|---------|:--:|-------|
| `width` / `height` | ✅ | `min-content`, `max-content`, `fit-content`, `calc-size()` |
| `min-width` / `min-height` | ✅ | |
| `max-width` / `max-height` | ✅ | |
| `inset` | ✅ | and the logical `inset-*` |
| `gap` | ✅ | flex, grid and multicol |
| `aspect-ratio` | ✅ | |
| `object-fit` | ✅ | |
| `object-position` | ✅ | |

## Padding, margin & borders

| Feature | Status | Notes |
|---------|:--:|-------|
| `padding` | ✅ | |
| `margin` | ✅ | margin collapsing, auto margins in flex |
| `border-radius` | ✅ | per-corner, elliptical |
| `border-color` | ✅ | |
| `border-width` | ✅ | |
| `border-style` | ✅ | |
| `border-image` | ⚠️ | nine-slice with `url()` or gradient sources, every repeat mode; not on inline boxes |
| `corner-shape` | ❌ | |

## Font

| Feature | Status | Notes |
|---------|:--:|-------|
| `@font-face` | ✅ | WOFF/WOFF2, `unicode-range`; web fonts take the pure-Rust font path |
| `font-size` | ✅ | |
| `font-family` | ✅ | |
| `font-weight` | ✅ | |
| `font-style` | ✅ | |
| `font-display` | ❌ | |
| `font-stretch` | ✅ | |
| `font-feature-settings` | ✅ | |
| `font-variant-ligatures` | ✅ | |
| `font-variant-numeric` | ⚠️ | parsed; reachable through `font-feature-settings` |
| `font-variant-caps` | ⚠️ | `small-caps` renders (synthesized when the font lacks them); other values parsed |
| `font-variant-east-asian` | ❌ | parsed |
| `font-variant-position` | ❌ | parsed |
| `font-variant-alternates` | ❌ | parsed |
| `font-variant-emoji` | ❌ | parsed |
| `font-variation-settings` | ✅ | |
| `font-optical-sizing` | ❌ | parsed |
| `font-synthesis` | ❌ | parsed |
| `font-kerning` | ✅ | |
| `font-size-adjust` | ❌ | parsed |

## Text direction

| Feature | Status | Notes |
|---------|:--:|-------|
| `direction` | ✅ | drives bidi, `text-align: start/end`, flex row reversal |
| `unicode-bidi` | ⚠️ | `isolate`, `bidi-override`, `isolate-override`, `plaintext` reach the UAX #9 resolver; overrides across element boundaries are partial |
| `writing-mode` | ⚠️ | vertical text runs are measured and painted; orthogonal-flow box layout is limited |
| `text-orientation` | ⚠️ | `upright` stacks glyphs, `mixed` / `sideways` rotate the run |

## Text

| Feature | Status | Notes |
|---------|:--:|-------|
| `color` | ✅ | Color 4/5: `oklch()`, `color-mix()`, relative colours, `light-dark()` |
| `text-align` | ✅ | |
| `line-height` | ✅ | |
| `text-decoration` | ✅ | shorthand |
| `vertical-align` | ✅ | |
| `white-space-collapse` | ✅ | |
| `text-wrap-mode` | ✅ | |
| `text-transform` | ✅ | |
| `letter-spacing` | ✅ | |
| `text-overflow` | ✅ | `ellipsis` |
| `word-break` | ✅ | |
| `line-clamp` | ✅ | and `-webkit-line-clamp` |
| `overflow-wrap` / `word-wrap` | ✅ | |
| `text-indent` | ✅ | |
| `text-wrap-style` | ✅ | `balance` (up to ten lines) and `pretty` (no lone last word) |
| `text-decoration-line` | ✅ | |
| `text-underline-offset` | ✅ | lengths, em and percentages of the decorated text's font size |
| `hyphens` | ✅ | `auto` hyphenates by the element's language |
| `text-decoration-color` | ✅ | |
| `text-decoration-style` | ✅ | solid, double, dotted, dashed, wavy |
| `text-decoration-thickness` | ✅ | also in the `text-decoration` shorthand |
| `tab-size` | ✅ | |
| `word-spacing` | ✅ | |
| `-webkit-text-fill-color` | ✅ | |
| `text-decoration-skip-ink` | ❌ | parsed |
| `quotes` | ✅ | |
| `text-underline-position` | ❌ | parsed |
| `line-break` | ❌ | parsed; breaking follows UAX #14 with the Thai/Arabic/Indic tailorings |
| `-webkit-text-stroke` | ❌ | |
| `box-decoration-break` | ❌ | parsed |
| `text-align-last` | ❌ | |
| `paint-order` | ⚠️ | SVG text and shapes only |
| `text-justify` | ❌ | parsed |
| `text-decoration-inset` | ❌ | |

## Backgrounds

| Feature | Status | Notes |
|---------|:--:|-------|
| `background-color` | ✅ | |
| `background-image` | ✅ | multiple layers; linear, radial and conic gradients and their `repeating-*` forms |
| `background-position` | ✅ | |
| `background-size` | ✅ | |
| `background-repeat` | ✅ | |
| `background-clip` | ✅ | `border-box` / `padding-box` / `content-box` / `text` |
| `background-origin` | ✅ | |
| `background-attachment: scroll` | ✅ | |
| `background-attachment: fixed` | ✅ | |
| `background-attachment: local` | ❌ | |

## Flexbox

| Feature | Status | Notes |
|---------|:--:|-------|
| `flex-direction` | ✅ | |
| `flex-wrap` | ✅ | |
| `flex-shrink` | ✅ | |
| `flex-grow` | ✅ | |
| `flex-basis` | ✅ | |
| `-webkit-box-*` | ⚠️ | `display: -webkit-box` lays out as flex and `-webkit-box-orient` is honoured with `-webkit-line-clamp`; the other legacy box properties are not |

## Grid

| Feature | Status | Notes |
|---------|:--:|-------|
| `grid-template-*`: px / % / fr | ✅ | |
| `grid-template-*`: `min-content` / `max-content` / `auto` | ✅ | |
| `grid-template-*`: `fit-content()` | ✅ | also `minmax()`, `repeat()`, `auto-fill` / `auto-fit` |
| `grid-template-*`: named lines | ✅ | |
| `grid-template-*`: `subgrid` | ⚠️ | columns adopt the parent's tracks; rows only when the parent's rows are concrete |
| `grid-row` / `grid-column` | ✅ | |
| `grid-area` | ✅ | |
| `grid-template-areas` | ✅ | |
| `grid-auto-flow` | ✅ | `row`, `column`, `dense` |
| `grid-auto-rows` / `grid-auto-columns` | ✅ | |

## Box alignment

| Feature | Status | Notes |
|---------|:--:|-------|
| `align-content` / `justify-content` | ✅ | |
| `align-items` / `justify-items` | ✅ | flex baseline alignment is approximated |
| `align-self` / `justify-self` | ✅ | |

## 2D transforms

| Feature | Status | Notes |
|---------|:--:|-------|
| `transform` | ✅ | |
| `transform-origin` | ✅ | |
| `scale` | ✅ | |
| `rotate` | ✅ | |
| `translate` | ✅ | |
| `transform-box` | ❌ | parsed |

## 3D transforms

| Feature | Status | Notes |
|---------|:--:|-------|
| `transform` (3D functions) | ✅ | `matrix3d()`, `perspective()`, `rotate3d()`, … painted by `rust/paint/src/three_d.rs` |
| `transform-origin` (z) | ✅ | |
| `scale` (3D) | ✅ | |
| `rotate` (3D) | ✅ | |
| `translate` (3D) | ✅ | |
| `backface-visibility` | ✅ | |
| `transform-style` | ✅ | `preserve-3d` |
| `perspective` | ✅ | |
| `transform-box` | ❌ | parsed |
| `perspective-origin` | ✅ | |

## Transitions & animations

| Feature | Status | Notes |
|---------|:--:|-------|
| Transitions | ✅ | honours `prefers-reduced-motion` |
| Animations | ✅ | `@keyframes` on any animatable property, every timing function, direction and fill mode |

## Composition & effects

| Feature | Status | Notes |
|---------|:--:|-------|
| `box-shadow` | ✅ | inset and multiple shadows |
| `filter` | ✅ | every filter function on any element; `url()` SVG filters are not applied |
| `outline` | ✅ | with `outline-offset` |
| `text-shadow` | ✅ | |
| `backdrop-filter` | ✅ | colour filters and `blur()` over the page behind the border box, with `-webkit-` alias |
| `mix-blend-mode` | ✅ | |
| `background-blend-mode` | ✅ | |

## Lists

| Feature | Status | Notes |
|---------|:--:|-------|
| `list-style-type` | ✅ | disc/circle/square, decimal, alpha, roman, greek, `none` |
| `list-style-position` | ✅ | |
| `counter-increment` | ✅ | |
| `counter-reset` | ✅ | |
| `list-style-image` | ✅ | |

## Tables

| Feature | Status | Notes |
|---------|:--:|-------|
| `border-collapse` | ✅ | shared edges de-duplicated; conflict resolution is simplified |
| `border-spacing` | ✅ | |
| `table-layout` | ⚠️ | `fixed` per spec; `auto` approximated |
| `caption-side` | ✅ | |
| `empty-cells` | ❌ | parsed |

## SVG

Applies to inline and image SVG, rendered in-engine by `rust/svg`, where
these are read from presentation attributes and from CSS.

| Feature | Status | Notes |
|---------|:--:|-------|
| `fill` | ✅ | |
| `stroke` | ✅ | |
| `stroke-width` | ✅ | |
| `stroke-dashoffset` | ✅ | |
| `stroke-dasharray` | ✅ | |
| `fill-opacity` | ✅ | |
| `stroke-linecap` | ✅ | |
| `stroke-miterlimit` | ✅ | |
| `stroke-opacity` | ✅ | |
| `stroke-linejoin` | ✅ | |
| `fill-rule` | ✅ | |

## Interaction & pointer events

| Feature | Status | Notes |
|---------|:--:|-------|
| `pointer-events` | ✅ | |
| `user-select: auto` | ✅ | |
| `user-select: text` | ✅ | |
| `user-select: none` | ✅ | skipped by drag selection, select-all, highlight and copy |
| `user-select: all` | ❌ | behaves as `auto` |
| `touch-action` | ❌ | parsed |
| `caret-color` | ✅ | |
| `-webkit-user-drag` | ❌ | |
| `accent-color` | ✅ | |
| `-webkit-tap-highlight-color` | ❌ | parsed |

## Scrollbar presentation

| Feature | Status | Notes |
|---------|:--:|-------|
| `scrollbar-width` | ✅ | |
| `scrollbar-color` | ✅ | |
| `scrollbar-gutter` | ❌ | parsed; scrollbars are overlays and take no space |

## Scroll behaviour

| Feature | Status | Notes |
|---------|:--:|-------|
| `scroll-behavior` | ✅ | `smooth` glides script, `scrollIntoView` and anchor scrolls of the page; also `behavior: 'smooth'` in `scrollTo`/`scrollBy` |
| `overscroll-behavior` | ❌ | parsed |
| `scroll-snap-type` | ✅ | |
| `scroll-snap-align` | ✅ | |
| `scroll-margin` | ✅ | |
| `scroll-padding` | ✅ | |
| `scroll-snap-stop` | ❌ | parsed |
| `scroll-boundary-behavior` | ❌ | |
| `scroll-boundary-behavior-y` | ❌ | |

## User agent hints

| Feature | Status | Notes |
|---------|:--:|-------|
| `-webkit-font-smoothing` | ❌ | parsed |
| `appearance` | ⚠️ | `none` drops native form-control styling; other values parsed |
| `will-change` | ❌ | parsed |
| `text-rendering` | ❌ | parsed |
| `color-scheme` | ⚠️ | the browser's light/dark setting drives `prefers-color-scheme` and `light-dark()`; the property itself does not switch UA colours |
| `text-size-adjust` | ❌ | parsed |
| `forced-color-adjust` | ❌ | parsed |
| `overflow-anchor` | ❌ | parsed |
| `image-rendering` | ✅ | `pixelated` / `crisp-edges` |

## Miscellaneous / other

| Feature | Status | Notes |
|---------|:--:|-------|
| `resize` | ❌ | parsed |
| `container-type` | ✅ | `@container` size and style queries, container units |
| `speak` | ❌ | |
| `container` | ✅ | shorthand with `container-name` |
| `field-sizing` | ❌ | parsed |

## Multi-column layout

| Feature | Status | Notes |
|---------|:--:|-------|
| `column-count` | ✅ | balanced columns |
| `column-width` | ✅ | |
| `column-fill` | ❌ | parsed; columns are always balanced |
| `column-rule` | ✅ | |
| `column-span` | ❌ | parsed |

## Anchor positioning

| Feature | Status | Notes |
|---------|:--:|-------|
| `anchor-name` | ✅ | `rust/layout-position/src/anchor.rs` |
| `position-anchor` | ✅ | `anchor()` and `anchor-size()` |
| `position-area` | ✅ | |
| `position-try` | ✅ | `@position-try` rules and `position-try-fallbacks` |
| `position-visibility` | ❌ | parsed |
| `anchor-scope` | ✅ | |

## @page

| Feature | Status | Notes |
|---------|:--:|-------|
| `size` | ✅ | named sizes, lengths, `portrait` / `landscape` ([printing.md](printing.md)) |
| `break-inside` | ✅ | while paginating for print |
| `page-break-inside` | ✅ | |
| `break-after` | ✅ | |
| `break-before` | ✅ | |

## Beyond the template

Supported features the Blitz template does not list: CSS Nesting, `@layer`,
`@scope`, `@container`, `@property`, `@supports` with `selector()`,
`:has()` and the rest of Selectors 4, custom properties, `calc()` and the
Values 4 math functions, Color 4/5, `::marker`, `::first-letter`,
`::first-line`, `::selection`, the top layer with `::backdrop`, the CSSOM
(`document.styleSheets`, `insertRule`, constructable stylesheets).
