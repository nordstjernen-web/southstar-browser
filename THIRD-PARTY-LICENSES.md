# Third-party software notices

Southstar links to (and in some cases statically includes) the
following open-source libraries. One of them, ns-pango, is a **modified**
LGPL library; its source and our relink offer are described in its entry
below. Their copyright notices and license
texts are reproduced below. For libraries shipped dynamically in the
release bundles, you are entitled by the LGPL terms to replace them
with modified versions; the binary will continue to function with any
ABI-compatible replacement.

The Southstar source code itself is dual-licensed under the
Nordstjernen Source License v1.0 or the GNU General Public License
version 3 or later, at your option. See `License.md` and `COPYING` for the
project's own license terms.

---

## Statically linked

### The MIT License

> The text the MIT-licensed components below refer to.

Permission is hereby granted, free of charge, to any person obtaining a
copy of this software and associated documentation files (the
"Software"), to deal in the Software without restriction, including
without limitation the rights to use, copy, modify, merge, publish,
distribute, sublicense, and/or sell copies of the Software, and to
permit persons to whom the Software is furnished to do so, subject to
the following conditions:

The above copyright notice and this permission notice shall be included
in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

### Boa (boa_engine) — The Unlicense OR MIT License

> JavaScript engine, vendored in third_party/boa_engine as the base of
> Southstar's JavaScript engine.
> <https://github.com/boa-dev/boa>
>
> Used under the Unlicense, reproduced below; the MIT License is the
> alternative the project offers.

This is free and unencumbered software released into the public domain.

Anyone is free to copy, modify, publish, use, compile, sell, or
distribute this software, either in source code form or as a compiled
binary, for any purpose, commercial or non-commercial, and by any
means.

In jurisdictions that recognize copyright laws, the author or authors
of this software dedicate any and all copyright interest in the
software to the public domain. We make this dedication for the benefit
of the public at large and to the detriment of our heirs and
successors. We intend this dedication to be an overt act of
relinquishment in perpetuity of all present and future rights to this
software under copyright law.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY CLAIM, DAMAGES OR
OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
OTHER DEALINGS IN THE SOFTWARE.

For more information, please refer to <https://unlicense.org>

### rustls, rustls-webpki and ring — ISC License, Apache License 2.0

> TLS for the in-tree HTTP client: rustls (used under its ISC option),
> rustls-webpki for certificate verification, and ring for the
> cryptographic primitives. ring carries code from BoringSSL under the
> Apache License 2.0 (see the OpenSSL entry below for where to read it)
> and under the ISC License; each such file names its license at the top.
> <https://github.com/rustls/rustls>, <https://github.com/rustls/webpki>,
> <https://github.com/briansmith/ring>
>
> Copyright (c) 2016, Joseph Birr-Pixton; Copyright 2015 Brian Smith;
> Copyright 2015-2025 Brian Smith.

Permission to use, copy, modify, and/or distribute this software for any
purpose with or without fee is hereby granted, provided that the above
copyright notice and this permission notice appear in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY
SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION
OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN
CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.

### RustCrypto ml-kem — MIT License

> ML-KEM-768 (FIPS 203) for the X25519MLKEM768 post-quantum TLS key
> exchange, with the RustCrypto crates it builds on (sha3, keccak, digest,
> hybrid-array, module-lattice, kem). Used under the MIT License above.
> <https://github.com/RustCrypto/KEMs>
>
> Copyright (c) 2024-2026 RustCrypto Developers.

### pl_mpeg — MIT License

> Single-file MPEG-1 video and MP2 audio decoder and MPEG-PS demuxer.
> Ported to Rust in `rust/plmpeg`; the license text is kept at
> `rust/plmpeg/LICENSE-pl_mpeg`.
> <https://github.com/phoboslab/pl_mpeg>
>
> Copyright (c) 2019 Dominic Szablewski

Licensed under the MIT License. See the MIT License text above for the
license text (same license).

### minimp3 — CC0 1.0 Universal (public domain dedication)

> Single-file MP3 decoder, ported to Rust as `rust/minimp3` and used by
> the `southstar-audio` helper.
> <https://github.com/lieff/minimp3>

To the extent possible under law, the authors have dedicated all
copyright and related and neighboring rights to this software to the
public domain worldwide. This software is distributed without any
warranty. See <http://creativecommons.org/publicdomain/zero/1.0/>.

---

### ns-pango — GNU LGPL 2.1 or later (modified Pango, statically linked)

> Text itemization, shaping and line breaking, with a cache of shaped runs
> that Pango itself does not keep.
> <https://github.com/nordstjernen-web/ns-pango>
>
> Copyright the GNU Project and contributors, and Northstar contributors
> for the modifications.

**This is a modified copy of Pango.** It adds a process-wide cache of
shaped glyph strings and of context font metrics, renames every exported
symbol so it can share a process with the system Pango that GTK loads, and
removes the backends and tooling a browser does not use. It remains
licensed under the GNU Lesser General Public License version 2.1 or, at
your option, any later version, and its complete corresponding source --
including every modification -- is published at the URL above. The full
license text is available at:

  <https://www.gnu.org/licenses/old-licenses/lgpl-2.1.html>

Unlike the other LGPL libraries listed here, ns-pango is linked
**statically** into the Southstar executables. Per
LGPL section 6(a), you are entitled to modify ns-pango and relink
Southstar against your modified copy: write to the address in
`README.md` and we will supply the Southstar object files, together with
any data and utility programs needed, so that you can produce a modified
executable. Builds configured with `-Dns-pango=disabled` do not include
ns-pango; they link the system Pango dynamically instead.

### Unicode Character Database — Unicode License v3

> The IDNA mapping, normalization and character property tables in
> `rust/url/src/tables.rs` are generated from the Unicode Character
> Database by `scripts/gen-url-tables.py`.
> <https://www.unicode.org/ucd/>

UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

Copyright © 1991-2026 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.

## Dynamically linked

### OpenSSL (libcrypto) — Apache License 2.0

> Web Cryptography (`crypto.subtle`) primitives.
> <https://www.openssl.org>
>
> Copyright (c) 1998-2026 The OpenSSL Project Authors. All Rights Reserved.

Licensed under the Apache License, Version 2.0 (the "License"); you may
not use this software except in compliance with the License. You may
obtain a copy of the License at:

  <http://www.apache.org/licenses/LICENSE-2.0>

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
implied. See the License for the specific language governing permissions
and limitations under the License.

### libwebp — BSD 3-Clause License

> WebP image decoding.
> <https://chromium.googlesource.com/webm/libwebp>
>
> Copyright (c) 2010, Google Inc. All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are
met: redistributions of source code must retain the above copyright
notice, this list of conditions and the following disclaimer;
redistributions in binary form must reproduce the above copyright
notice, this list of conditions and the following disclaimer in the
documentation and/or other materials provided with the distribution;
neither the name of Google nor the names of its contributors may be used
to endorse or promote products derived from this software without
specific prior written permission. THIS SOFTWARE IS PROVIDED BY THE
COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED
WARRANTIES ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR
CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL,
EXEMPLARY, OR CONSEQUENTIAL DAMAGES ARISING IN ANY WAY OUT OF THE USE OF
THIS SOFTWARE.

### libpsl — MIT License

> Public Suffix List handling for cookie / origin policy.
> <https://github.com/rockdaboot/libpsl>
>
> Copyright (c) 2014-2024 Tim Rühsen

Licensed under the MIT License. See the MIT License text above for the
full text (same license).

### SQLite — public domain

> Embedded SQL database used for history, bookmarks, and caches.
> <https://www.sqlite.org>

SQLite is in the public domain. The authors disclaim copyright to the
source code; it may be used for any purpose, commercial or
non-commercial, without restriction.

### libepoxy — MIT License

> OpenGL / OpenGL ES function-pointer management (used by the WebGL
> backend and GTK's GL rendering).
> <https://github.com/anholt/libepoxy>
>
> Copyright (c) 2013-2014 Intel Corporation

Licensed under the MIT License. See the MIT License text above for the
full text (same license).

### zlib — zlib License

> DEFLATE compression used by image decoders and HTTP content decoding.
> <https://zlib.net>
>
> Copyright (c) 1995-2024 Jean-loup Gailly and Mark Adler

This software is provided 'as-is', without any express or implied
warranty. In no event will the authors be held liable for any damages
arising from the use of this software. Permission is granted to anyone to
use this software for any purpose, including commercial applications, and
to alter it and redistribute it freely, subject to the restrictions in
the zlib license: the origin of this software must not be misrepresented;
altered source versions must be plainly marked as such; and this notice
may not be removed from any source distribution.

### libseccomp — GNU LGPL 2.1 (Linux only)

> Syscall-filter sandbox for the renderer process. Linked only on Linux.
> <https://github.com/seccomp/libseccomp>
>
> Copyright (c) Paul Moore and the libseccomp contributors.

Licensed under the GNU Lesser General Public License version 2.1. See the
LGPL section below for terms and obligations.

### libuchardet — MPL-1.1 / LGPL-2.1+ / GPL-2.0+ (tri-license)

> Charset detection.
> <https://www.freedesktop.org/wiki/Software/uchardet/>
>
> Based on Mozilla's universalchardet, originally Copyright (c)
> 1998-2006 Netscape Communications Corporation and others.

Distributed under the terms of the Mozilla Public License 1.1, the
GNU Lesser General Public License 2.1, or the GNU General Public
License 2.0, at your option. The full license texts are available at:

- <https://www.mozilla.org/MPL/1.1/>
- <https://www.gnu.org/licenses/old-licenses/lgpl-2.1.html>
- <https://www.gnu.org/licenses/old-licenses/gpl-2.0.html>

### GTK 4, GLib, Pango, gdk-pixbuf — GNU LGPL 2.1 or later

> UI toolkit and core utilities.
> <https://www.gtk.org>, <https://gitlab.gnome.org/GNOME/glib>,
> <https://gitlab.gnome.org/GNOME/pango>,
> <https://gitlab.gnome.org/GNOME/gdk-pixbuf>
>
> Copyright the GNU Project and contributors.

These libraries are licensed under the GNU Lesser General Public
License version 2.1, or (at your option) any later version. The full
license text is available at:

  <https://www.gnu.org/licenses/old-licenses/lgpl-2.1.html>

Per LGPL section 6, since Southstar links to these libraries
dynamically, you are entitled to modify them and re-link Southstar
against the modified copies. On Windows / macOS bundles the libraries
are shipped alongside the executable as ordinary DLLs / dylibs that you
can replace; on Linux distributions they are loaded from the system
package manager.

### FFmpeg — libavformat / libavcodec / libavutil / libswscale / libswresample — GNU LGPL 2.1 or later (inline WebM)

> Container demuxing and audio/video decoding for the inline WebM path
> (VP9/VP8 video, Opus/Vorbis audio). <https://ffmpeg.org>
>
> Copyright the FFmpeg developers.

Required on Linux and Windows and auto-detected on macOS, so it is present
in every build with WebM support. The copy
bundled in the macOS / Windows releases is built **LGPL-only** — its
`configure` uses `--disable-gpl --disable-nonfree --disable-version3
--disable-autodetect`, so it contains no GPL components and no external
codec libraries; the enabled VP8/VP9/Opus/Vorbis decoders are FFmpeg's
own LGPL implementations (see `scripts/build-ffmpeg-lgpl.sh`). The full
license text is available at:

  <https://www.gnu.org/licenses/old-licenses/lgpl-2.1.html>

Per LGPL section 6, the libraries are linked dynamically: on the
macOS / Windows bundles they ship beside the executable as ordinary
dylibs / DLLs you can replace and re-link against; on Linux they are
loaded from the distribution's FFmpeg packages.

### Cairo — LGPL-2.1 or MPL-1.1

> 2D drawing.
> <https://www.cairographics.org>
>
> Copyright Carl Worth, Behdad Esfahbod, and the Cairo contributors.

Dual-licensed under the GNU Lesser General Public License 2.1 or the
Mozilla Public License 1.1. See:

- <https://www.gnu.org/licenses/old-licenses/lgpl-2.1.html>
- <https://www.mozilla.org/MPL/1.1/>

### librsvg — GNU LGPL 2.1 or later (Windows / macOS bundles)

> Not used by the browser: SVG is rendered in-engine by `src/svg.c`.
> librsvg reaches the Windows and macOS bundles only as a transitive
> dependency of the gdk-pixbuf loader directory and of GTK's symbolic
> icon loading, so its notice is reproduced here for redistribution.
> <https://gitlab.gnome.org/GNOME/librsvg>
>
> Copyright the GNU Project and contributors.

Licensed under the GNU Lesser General Public License version 2.1 or
later. See the LGPL section above for terms and obligations.

### Optional dynamic dependencies

These are linked only when present on the build host (meson
`required: false`). When a build bundles them, their notices apply:

- **Brotli** (`libbrotlidec`) — MIT, © the Brotli authors. `br`
  HTTP content decoding.
- **Zstandard** (`libzstd`) — BSD 3-Clause or GNU GPL 2.0, at your
  option, © Meta Platforms, Inc. and affiliates. `zstd` HTTP content
  decoding.
- **libavif** — BSD 2-Clause, © the AOMedia / libavif authors. AVIF
  image decoding.
- **Poppler** (`poppler-glib`) — GNU GPL 2.0 or later, © the Poppler
  developers. PDF rendering. Note: Poppler is GPL; a build that links it
  is subject to the GPL for that combined binary.
- **Fontconfig** — MIT-style license, © Keith Packard and contributors.
- **FreeType** — FreeType License (BSD-style with credit clause) or GNU
  GPL 2.0, at your option, © The FreeType Project.
- **wgpu-native** — MIT or Apache 2.0, © the gfx-rs authors. The
  experimental WebGPU backend. Its two C headers are vendored under
  `third_party/wgpu-native/include/webgpu/`: `webgpu.h` is BSD 3-Clause,
  © 2019-2023 the WebGPU-Native developers; `wgpu.h` carries
  wgpu-native's own MIT-or-Apache-2.0 terms. The library itself is never
  vendored — it is located at build time and, in the release bundles,
  ships beside the executable.

---


## NOTICE files

Apache 2.0 section 4(d) requires propagating any `NOTICE` files
shipped with the upstream sources. As of this release:

No bundled component currently ships a `NOTICE` file.

If a future upstream release adds one, it will be included verbatim
in this section.
