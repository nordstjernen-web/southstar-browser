# southstar-http

The HTTP client of [Southstar](https://github.com/nordstjernen-web/southstar-browser),
written from scratch in Rust. It replaced libcurl and libnghttp2 in the
browser.

- **HTTP/1.1 and HTTP/2** with its own framing, flow control and HPACK. The
  HPACK static table and Huffman code are generated from RFC 7541.
- **Multiplexing:** HTTP/2 connections are pooled per `scheme://host:port`
  and carry concurrent requests. One I/O thread per connection drives a
  sans-I/O connection state machine, and requests wait on their own stream's
  events.
- **TLS through [rustls](https://github.com/rustls/rustls) over ring:** TLS
  1.3 and 1.2, ALPN and session resumption. Key exchange prefers the hybrid
  post-quantum X25519MLKEM768. Roots come from a CA bundle, the Windows root
  store or `SSL_CERT_DIR`.
- **Proxies and FTP:** HTTP CONNECT, SOCKS4/4a/5/5h and no-proxy matching,
  plus FTP downloads and the HTTP/1.1 upgrade WebSocket uses.
- **Content decoding:** gzip and deflate through zlib, and `br`/`zstd` through
  libbrotlidec/libzstd when present.
- **Waiting:** sockets are non-blocking after connect, and every wait is a
  `poll()`.

The `http_wire` module holds the wire formats with no I/O and no unsafe code:
HTTP/1.1 heads and chunked bodies, HTTP/2 frames, HPACK and the sans-I/O
client connection. It can be tested on its own.

Not supported: HTTP/3, DNS over HTTPS, Encrypted Client Hello.

Part of the browser that `cargo install southstar` builds. The API follows
the browser's needs and is not stable.

License: `LicenseRef-NSL-1.0 OR GPL-3.0-or-later`.
