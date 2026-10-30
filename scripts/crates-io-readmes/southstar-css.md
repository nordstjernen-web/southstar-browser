# southstar-css

The CSS engine of [Southstar](https://github.com/nordstjernen-web/southstar-browser),
written from the CSS specifications without Servo's `cssparser` or
`selectors`.

The `css` module is the style engine:
- **Values:** lengths and `calc()`, colors, gradients, transforms, grid and
  font values, shadows, animations and transitions, counters and lists, and
  border images.
- **Rules:** the style sheet parser with its at-rules, nesting, `@supports`,
  container queries and cascade layers.
- **Selectors:** parsing and matching, the element state behind
  pseudo-classes, and shadow-tree scoping.
- **The cascade:** custom properties, `var()` and `attr()` substitution,
  registered properties and presentational hints.
- **Computed style:** computed-style queries, style sharing and incremental
  restyle.

The smaller modules each follow one specification:

| Module | Specification |
| --- | --- |
| `css_syntax` | CSS Syntax 3: the tokenizer and component values |
| `css_color` | CSS Color 4/5: color spaces, `color-mix()`, relative colors |
| `css_media` | Media Queries 4: parsing and evaluation |
| `css_prop_syntax` | Properties and Values API: the `<syntax>` grammar of `@property` |
| `css_grammar` | keyword-shaped values against a value grammar, serialized canonically |
| `selector_text` | CSSOM selector serialization |

Part of the browser that `cargo install southstar` builds. The API follows
the browser's needs and is not stable.

License: `LicenseRef-NSL-1.0 OR GPL-3.0-or-later`.
