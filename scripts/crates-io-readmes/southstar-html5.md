# southstar-html5

An HTML5 parser written from the [WHATWG HTML Standard](https://html.spec.whatwg.org/multipage/parsing.html),
with no dependencies. It is the parser of the
[Southstar](https://github.com/nordstjernen-web/southstar-browser) web
browser, which does not use html5ever or any other browser engine's code.

**Tokenizer**
- every tokenizer state, including the processing-instruction states
- runs over the whole newline-normalized input
- named character references come from a table generated from WHATWG's
  `entities.json`

**Tree builder**
- every insertion mode
- the adoption agency algorithm, foster parenting and `<template>`
- foreign content (SVG and MathML) and quirks mode
- fragment parsing with a context element
- the relaxed `<select>` parsing

**Output.** The tree builder builds through a `TreeSink` trait, so it creates
whatever tree the caller wants: Southstar's DOM, or a test harness's simple
nodes. Script source positions come from the tokenizer.

**No global state.** Several documents can be parsed at once on different
threads, which Southstar does to decode SVG images in parallel.

Part of the browser that `cargo install southstar` builds. The API follows
the browser's needs and is not stable.

License: `LicenseRef-NSL-1.0 OR GPL-3.0-or-later`.
