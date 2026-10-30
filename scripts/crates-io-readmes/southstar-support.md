# southstar-support

The storage and support code behind the
[Southstar](https://github.com/nordstjernen-web/southstar-browser) web
browser's network stack and page APIs.

**What the browser remembers**
- `http_cache`: the on-disk HTTP cache. A SQLite index of responses, keyed by
  URL, partition and `Vary`, with bodies stored as files.
- `netstore`: the rules behind stored network state, with no I/O: the cookie
  jar (RFC 6265 matching, Netscape format), the HSTS host list,
  `Strict-Transport-Security` and HTTP dates.
- `history`: browsing history in SQLite and the `about:history` page.
- `bytecode_cache`: compiled JavaScript kept in memory and on disk, keyed by
  source hash.

**What pages are checked against**
- `csp`: Content-Security-Policy parsing and its allow/deny checks.
- `safebrowsing`: a local phishing and malware host blocklist with its warning
  page. It needs no network service.

**Formats and primitives**
- `intl_core`: BCP 47 locale identifiers, the CLDR data behind them, and
  collation for `Intl`. It uses no ICU.
- `webcrypto`: the `crypto.subtle` primitives over OpenSSL libcrypto: digests,
  HMAC, AES-GCM/CBC/CTR/KW, RSA, ECDSA/ECDH, Ed25519/X25519, PBKDF2 and HKDF.
- `image_decoders`: the image chain. PNG/APNG, GIF, JPEG and BMP in safe Rust,
  ICO and CUR containers, and WebP (still and animated) over libwebp.
- `pdf`: PDF documents shown as pages of rendered images, through poppler.

Part of the browser that `cargo install southstar` builds. The API follows
the browser's needs and is not stable.

License: `LicenseRef-NSL-1.0 OR GPL-3.0-or-later`.
