# southstar-url

A [WHATWG URL Standard](https://url.spec.whatwg.org/) parser, written from
the spec with no crates.io dependencies. It is the URL parser of the
[Southstar](https://github.com/nordstjernen-web/southstar-browser) web
browser, which does not use Servo's `url` or `idna`.

- **Parsing:** the basic URL parser, including parsing against a base URL.
- **Hosts:** the host parser with UTS #46 IDNA processing, Punycode and NFC,
  and IPv4 and IPv6 addresses.
- **Serialization, origins and host extraction.**
- **The URL API setters:** `href`, `protocol`, `username`, `password`,
  `host`, `hostname`, `port`, `pathname`, `search` and `hash`, with the
  standard's state overrides.
- **An absolute-URL check.**

The functions work on bytes (`southstar_url::bytes`). The Unicode tables for
IDNA and NFC are generated from the Unicode Character Database by a script
in the repository, not edited by hand.

Part of the browser that `cargo install southstar` builds. The API follows
the browser's needs and is not stable.

License: `LicenseRef-NSL-1.0 OR GPL-3.0-or-later`.
