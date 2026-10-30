# southstar-dom

The Document Object Model of [Southstar](https://github.com/nordstjernen-web/southstar-browser),
and the parsers that build it.

The `dom` module's design aims for a tree that safe Rust can walk and
relink:
- **Nodes live in a per-thread store of slots that are never deallocated**,
  so a `Node<'a>` is a plain shared reference and its tree links are `Cell`s.
- **Each node owns its data:** its name, text, attributes and indexes, with no
  string arena.
- **Detached subtrees:** a subtree with no parent is owned by a `Tree`, which
  frees it on drop unless it was given a parent.
- **Long-lived references:** anything that keeps a node past the current call
  holds a `WeakNode`. It returns `None` once the node is gone, even if its
  slot has been reused.

The other modules:
- `html_parser` builds the DOM through
  [southstar-html5](https://crates.io/crates/southstar-html5). It then
  attaches declarative shadow roots and extracts standards-based media
  metadata (OpenGraph, JSON-LD).
- `xml` is the namespaced XML and XHTML parser behind `DOMParser`,
  `XMLHttpRequest` and XHTML pages.
- `html_util` holds escaping, number and charset parsing, response-body
  decoding (with uchardet), and the JSON, XML and image viewer documents.

Part of the browser that `cargo install southstar` builds. The API follows
the browser's needs and is not stable.

License: `LicenseRef-NSL-1.0 OR GPL-3.0-or-later`.
