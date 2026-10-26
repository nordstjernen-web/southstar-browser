Changelog:
==Significant changes in each release:

Southstar Browser 1.0.2-dev:
======
* Security: the HTTP/2 HPACK decoder caps a decoded header list at 1 MiB.
  Indexed references to a large dynamic-table entry cost one byte each, so
  a server could expand a 2 MiB header block into gigabytes and exhaust
  memory before the 1 MiB response-header limit was checked. HTTP/2
  response fields containing CR, LF or NUL are now a protocol error
  (RFC 9113 section 8.2.1) instead of being passed on inside header lines.
* Security: the renderer's Landlock ruleset no longer grants read access
  to the whole of `~/.config`, `~/.local/share` and `~/.cache`, where other
  programs keep tokens, browser profiles and keyrings. The renderer now
  reads only the subdirectories it uses there (fontconfig, fonts, the MIME
  database, icon themes, spell-checking dictionaries and GPU shader
  caches) besides its own `southstar` directories. Other sandboxed
  processes keep the wider rules.
* Security: the audio helper's downloads of `<audio>` sources and
  standalone `.mp3` files go through the configured proxy again. The shell
  passes the proxy settings in `http_proxy`/`https_proxy`/`all_proxy`/
  `no_proxy`, which libcurl used to read; since the move to `rust/http`
  nothing read them, so these requests connected directly and revealed the
  user's address even with a proxy or Tor configured. A proxy setting that
  does not parse now fails the download instead of going direct.
* Packaging: an Arch Linux package, `southstar-browser`, in
  `packaging/arch/` (PKGBUILD and .SRCINFO, ready for the AUR), documented
  in `docs/Arch.md`. It builds release 1.0.1 in a clean Arch container and
  passes namcap.
* Web fonts' WOFF and WOFF2 containers are decoded in Rust only:
  `rust/woff2` gains a WOFF 1.0 decoder (zlib through `miniz_oxide`, already
  a dependency), and FreeType no longer opens downloaded fonts to read their
  tables. The WOFF output is byte-identical to the old FreeType path on 597
  fonts; the WOFF2 output matches fontTools' decoder glyph for glyph on 358
  Google Fonts files, which render pixel-identically. The old path wrote
  `loca` tables that pointed past the end of `glyf` for almost every WOFF2
  font. The `freetype` cargo feature, which only gated that reader, is gone.
* The rest of the browser's Unicode lookups moved from GLib to
  `rust/unicode` as well: CSS `text-transform` (`uppercase`, `lowercase`,
  `capitalize`) in layout and in `innerText`, the case-insensitive
  matching of `<select>` type-ahead and `window.find()`, `:dir()` script
  detection, MathML's single-letter test, and the character-class tests in
  key input, editing, history, layout, paint and the shell.
  `rust/unicode` gained full case folding (CaseFolding.txt) and the
  alphabetic, control and printable tests, each identical to GLib's over
  every code point; whole-string upper, lower and fold also matched GLib on
  300,000 random strings each. `text-transform` and JavaScript-visible
  case conversion no longer depend on the operating-system locale. Eleven
  crates drop 37 unsafe blocks and 31 GLib imports between them.
* Text layout no longer reads Unicode data from GLib: the new
  `rust/unicode` (`southstar-unicode`, no unsafe code, no dependencies)
  holds the general category, line break class, script, East Asian width,
  combining class, case mappings (with SpecialCasing.txt) and canonical
  decompositions, generated from the UCD by `scripts/gen-unicode-tables.py`,
  and `rust/text` drops 19 GLib imports. The tables are Unicode 17.0, the
  version of GLib 2.88: checked over all 1,114,112 code points, every
  function `rust/text` uses gives exactly GLib's answer, except line break
  classes of unassigned code points past U+33500, where the UCD default now
  applies. Layout no longer depends on the system GLib's Unicode version
  (Ubuntu 24.04 ships 15.1), and `text-transform` no longer follows the
  operating-system locale, which uppercased `i` to `İ` on Turkish and
  Azerbaijani systems whatever the page's language. Unicode 18 waits for
  the matching UAX #14 rule changes (soft hyphen and dashes change class).
* Bidirectional text is resolved in Rust: the new `rust/bidi`
  (`southstar-bidi`, no unsafe code, no dependencies) implements the Unicode
  Bidirectional Algorithm (UAX #9) from the specification and replaces
  FriBiDi, which is no longer a build dependency. Its Bidi_Class, paired
  bracket and mirroring tables are generated from Unicode 18.0 by
  `scripts/gen-bidi-tables.py`. It passes all 770,241 BidiTest.txt and
  91,707 BidiCharacterTest.txt cases, and agreed with Unicode's C reference
  implementation on 5,000,000 random paragraphs. FriBiDi 1.0.16 disagreed
  with the reference on 36,075 of those, about half of them in bracket
  pairs; it also pairs compatibility brackets such as U+FF08 with ASCII
  ones, ignores embeddings after a mid-text paragraph separator, and its
  tables stop at Unicode 16. Render tests and RTL Wikipedia articles render
  pixel-identically. Step 4 of `docs/pure-rust-text.md`, bidi half.
* The README screenshot shows the current build rendering a Wikipedia
  article.
* The MSIX package carries the Partner Center identity exactly: publisher
  display name Nordstjernen and the reserved app name Southstar Web
  Browser, so `scripts/pack-msix.sh` builds a Store-submittable package
  without overrides.

Southstar Browser 1.0.1:
======
* Text layout is now Rust: `rust/text` (`southstar-text`) is a port of
  web-text-lib, the Pango fork the browser used, covering attributes, line
  breaking, bidi, itemization, the fontconfig/FreeType/cairo font stack,
  HarfBuzz shaping with the shape and item caches, paragraph layout with
  ellipsizing and justification, and cairo rendering. Each part was checked
  against the C library on large randomized corpora, and pages render
  pixel-identically. `third_party/web-text-lib`, the `ns-pango` option, the
  system-Pango fallback and the `ns_pango_*` FFI in nine crates are gone;
  callers use the Rust API. The crate is LGPL-2.0-or-later, as a
  translation of Pango. HarfBuzz, FriBiDi, fontconfig, FreeType and cairo
  are now direct build dependencies, and libthai an optional one.
* `-Drust-shaper=true` shapes text with HarfRust, the HarfBuzz project's
  pure-Rust port (step 1 of `docs/pure-rust-text.md`); it adds the
  `harfrust`, `read-fonts` and `font-types` crates. Off by default;
  `NS_TEXT_SHAPER=harfbuzz` switches back at runtime.
* `docs/pure-rust-text.md` describes a possible later step for the text
  stack: replacing HarfBuzz, FreeType, FriBiDi and GLib's Unicode data under
  text layout with Rust crates, so web fonts are parsed by memory-safe code.
* `main` is branch-protected: every change lands through a pull request.
  CLAUDE.md and AGENTS.md say so instead of telling agents to push to
  `origin/main`.
* Text layout now builds in-tree: web-text-lib, the trimmed Pango fork from
  Nordstjernen Sol, lives in `third_party/web-text-lib` and replaces the
  `ns-pango` subproject, so `meson setup` no longer clones anything and the
  BSD workflows no longer fetch it by hand. Three functions Southstar calls
  that the trim had dropped (`attr_scale_new`, `attr_list_to_string`,
  `get_log_attrs`) are restored.
* The browser's second command name is now `internet-browser-southstar`
  instead of `internet-browser`, which Nordstjernen and Northstar also
  install. `meson install`, the `.deb`, `.rpm` and Alpine packages, the
  Windows installer (App Paths) and the MSIX package (execution alias) all
  use the new name. Upgrading a package removes the old `internet-browser`
  alternative, link or App Paths key when it pointed at Southstar.
* Security: cookies honour `SameSite`. The jar records each cookie's
  `SameSite` value and a cookie without one counts as `Lax`. A navigation a
  page on another site starts sends `Lax` and `None` cookies only when it is
  a top-level GET, and `Strict` ones never; other cross-site requests send
  only `None` cookies. A cross-site response cannot set a `Lax` or `Strict`
  cookie, and `SameSite=None` needs `Secure`. Before, a form a hostile page
  posted to another site carried that site's login cookies (cross-site
  request forgery). Cookies stored by earlier versions count as `Lax`.
* CORS preflight answers are cached per page origin and URL for the
  server's `Access-Control-Max-Age` (5 seconds when absent, at most two
  hours), so repeated non-simple `fetch()` or `XMLHttpRequest` calls to the
  same endpoint no longer send an `OPTIONS` request each time. A request
  needing a method or header the cached answer did not allow is preflighted
  again.
* Security (Linux): the renderer gets its own, narrower Landlock ruleset.
  It used to share the shell's, which gave it read and write access to the
  whole runtime directory (`/run/user/$UID`, home of the D-Bus and
  `systemd --user` sockets), the X authority file, `/dev/snd` and the
  Downloads directory. It now gets only `$XDG_RUNTIME_DIR/southstar`, where
  page exports and print sheets are exchanged with the shell. The
  renderer's seccomp filter also refuses memory mapped writable and
  executable at once (kernel-enforced W^X) and new Unix-domain sockets, so
  a compromised renderer cannot reach the session bus to start programs.
  `memfd_secret` and `remap_file_pages` are off the syscall allow-list.
* Security: an `EventSource` stream now checks every redirect against the
  page's Content-Security-Policy `connect-src`, as it already checked the
  first URL. A redirect to a URL the policy does not allow is not followed
  and closes the stream for good. Before, any redirect was followed, so a
  same-origin endpoint could forward the connection anywhere.
* Security: `ctutils` is updated to 0.4.4 for RUSTSEC-2026-0385 (its
  constant-time mask helpers could compile to branches on secret values).
  It reaches Southstar through the post-quantum key exchange's `ml-kem`.
* Security (Linux): the thread that answers SIGQUIT with a thread dump now
  starts after the sandbox is in place, in the UI shell and in the
  renderer. Landlock confines only the thread that applies it and threads
  started later, so the dump thread ran unconfined for the life of the
  process. When the download directory is the home directory itself the
  sandbox no longer opens the whole home directory for writing; downloads
  need a directory below it.
* Security: cross-origin `fetch()` and `XMLHttpRequest` requests that are
  not CORS-simple (a method other than GET, HEAD or POST, a header outside
  the safelist, or a `Content-Type` other than form, multipart or plain
  text) now send an `OPTIONS` preflight first and go out only if the
  server's `Access-Control-Allow-Origin`, `-Allow-Methods` and
  `-Allow-Headers` permit them. They used to be sent straight away, so the
  server acted on a cross-site PUT or DELETE before any CORS check; a
  `no-cors` fetch that would need a preflight is refused.
* Security: WebExtension fixes. A file an extension names (content
  script, stylesheet, rules) is resolved with its symbolic links, so a link
  inside the extension directory can no longer read a file elsewhere.
  `chrome.storage` and `runtime.getManifest()` in content scripts no longer
  go through the page's `JSON.parse` and `JSON.stringify`, which a page
  could replace to forge or corrupt an extension's storage. Cosmetic filter
  selectors containing `}` are refused, so a filter list cannot inject
  whole CSS rules. URL patterns with many `*` no longer take exponential
  time to match.
* Security: a camera permission given to a document with an opaque origin
  (a sandboxed frame, a `data:` URL) no longer applies to every other such
  document; the decision is remembered for that document's URL instead of
  the shared origin `null`.
* XML: the parser and the XHTML well-formedness check now agree on where a
  `<!DOCTYPE>` ends. The check stopped at the first `>` and the parser
  ignored quotes, so an internal subset with `>` or `]` inside a quoted
  entity value or a comment was read differently by the two; both now use
  one scanner that honours quotes, brackets and comments.
* Security: `NS_NO_SANDBOX=0`, `NS_NO_SECCOMP=0` and the other `NS_NO_…`,
  `NS_ALLOW_ROOT`, `NS_NO_WIN32_MITIGATIONS` and `NS_PRIVATE` switches no
  longer take effect when set to `0`, `false`, `no`, `off` or nothing; any
  value used to count, so a wrapper that set `NS_NO_SANDBOX=0` to keep the
  sandbox on turned it off. The undocumented `NS_EARLY_JS_FILE`, which ran
  a local file as script in every page, and `NS_CAMERA_ALLOW`, which
  granted every site the camera without asking, are removed.
* Security: a setting's text can no longer carry a line break into
  `southstar.conf`, where it would have become a separate `key = value`
  line on the next start, and the HTTP/1.1 request writer drops any header
  line containing CR, LF or NUL rather than sending it, so no caller can
  inject extra headers.
* Security: `fetch()` and `XMLHttpRequest` no longer show a response's
  `Set-Cookie` and `Set-Cookie2` headers to script, which exposed
  `HttpOnly` cookies of same-origin responses. A `BroadcastChannel`
  message reaches only channels of the sender's origin, and channels of
  opaque-origin documents reach none.
* Packaging: `scripts/pack-appimage.sh` downloads linuxdeploy, its GTK
  plugin, appimagetool and the AppImage runtime from fixed releases and a
  fixed commit and checks each against a SHA-256 checksum before running
  it, on x86_64 and aarch64. It fetched the moving `continuous` builds and
  the plugin's `master` branch and ran them unchecked.
* Security: Content-Security-Policy is enforced in more places.
  `eval`, `new Function`, string `ShadowRealm.prototype.evaluate` and the
  other string-compiling functions now throw `EvalError` unless the page's
  `script-src` (or `default-src`) allows `'unsafe-eval'`; they were never
  checked. A `<script src>` that redirects is checked again at its final
  URL, for both the policy and mixed content. Keywords such as `'SELF'`,
  `'Unsafe-Inline'` and `'NONCE-…'` match whatever their case (nonce and
  hash values stay case-sensitive), a source with an empty host such as
  `https://` no longer matches, and `https://*` matches any host.
* Security: `navigator.serviceWorker.register` now requires a secure
  context (HTTPS or a local address) for both the page and the script, and
  refuses a scope outside the script's directory, as the Service Workers
  standard does without a `Service-Worker-Allowed` header.
* Privacy: in private browsing the JavaScript bytecode cache no longer
  looks on disk at all. It already wrote nothing there, but a lookup read
  cached entries from earlier sessions and created their directories.
* Security: a page can no longer read another origin's stylesheet, or any
  other response it links as one. The engine's private `__ns_linked_css`
  function, which returned the raw bytes of any `<link rel=stylesheet>`
  fetched in the process, was callable by page script; it now answers only
  engine code, and `sheet.cssRules` of a stylesheet from another origin
  throws `SecurityError` as in other browsers. In quirks mode a
  cross-origin stylesheet must now be served as `text/css`, so an HTML or
  JSON response can no longer be loaded as CSS.
* Security: classic scripts in an `<iframe>` now obey the frame's
  Content-Security-Policy and Subresource Integrity. They ran without either
  check: the framed document's own `script-src` and an `integrity`
  attribute on its `<script src>` were ignored. A frame loaded over the
  network follows the policy of its response; `srcdoc`, `about:`, `data:`
  and `blob:` frames inherit their embedder's.
* Hardening: a web font whose table directory names the same table twice
  is now refused when its SFNT is rebuilt from FreeType. FreeType looks
  tables up by tag, so the copy read the first table's bytes for every
  duplicate; the copy already stopped at each table's own length, and now
  writes into a slice exactly that long.
* Security: a navigation a page starts to another origin now tells the
  server who started it. The renderer dropped the page's URL on
  cross-origin navigations, so a form a hostile page submitted to another
  site went out as if the user had typed it: `Sec-Fetch-Site: none` and,
  for a POST, an `Origin` header naming the target site itself, which
  defeated servers' Origin and Fetch Metadata checks against cross-site
  request forgery. Page-started navigations now carry their initiator, so
  they send `Sec-Fetch-Site: cross-site` (or `same-site`) and the
  initiator's `Origin` on POST; only navigations from the address bar,
  bookmarks and history count as browser-initiated. Navigations no longer
  send `Origin` on GET, and a typed same-origin URL no longer sends the
  previous page as `Referer`.
* Security: a script in a cross-origin or sandboxed `<iframe>` can no
  longer run code in its parent page through the parent's `Function` or
  `eval`. Frame realms still share some objects with the page realm, and
  `location.constructor.constructor` or `Blob.constructor` in a frame was
  the parent's `Function`, so `Function("return parent.document")()` ran in
  the parent with its privileges. The engine now implements
  HostEnsureCanCompileStrings and refuses to compile a string in a realm
  whose origin differs from that of the script asking for it, with an
  `EvalError`; same-origin frames and `ShadowRealm` are unaffected.
* Security: page scripts can no longer read or plant the engine's own
  `__nd…` properties on elements. An `<iframe>` element carried its frame's
  document and window as `__ndRealmDoc` and `__ndRealmWindow`, so a page
  could reach into a cross-origin frame around the `contentWindow` and
  `contentDocument` checks; the engine hid those names under QuickJS, but
  the hook did nothing under Boa. Element wrappers now keep such properties
  in a store only engine code sees, and a page's own `__nd…` expandos stay
  separate from it.
* Security: the XML parser (XHTML documents, `DOMParser` with an XML
  type) no longer copies attributes named `data-nd-*` from the markup.
  Those names hold the engine's internal element state, such as the path
  of the file picked in an `<input type="file">`, and the HTML parser and
  the DOM APIs already refused them; through XML a page could plant a file
  path and read the file back from `input.files`.
* Rust dependencies updated to their latest Rust 1.91 compatible
  releases, among them rustls-webpki 0.103.16, ml-kem 0.3.3 (now on
  sha3 0.12), libc 0.2.191, cc 1.9.0, syn 3.0.9 and serde_json 1.0.154.
* Linux: WebGL and WebGPU render on the GPU again inside the sandboxed
  renderer. Its Landlock ruleset let it read `/dev/dri` but not open the
  render nodes read-write, so Mesa printed "Permission denied" for every
  `/dev/dri` node and WebGL fell back to software rendering. The renderer
  now opens `/dev/dri` and `/dev/nvidia*` read-write while WebGL is
  enabled or WebGPU is turned on with `--enable-webgpu`; with WebGL off in
  Settings and WebGPU not turned on it still gets no access. Backported
  from Nordstjernen.
* Linux: the browser starts again on NVIDIA's proprietary driver under
  X11. The UI shell's Landlock sandbox let it read `/dev/dri` but not open
  the render nodes read-write, and gave no access to `/dev/nvidia*` at all,
  so GTK's OpenGL and Vulkan drivers were refused the GPU ("Permission
  denied" on `/dev/dri/renderD128`) and the window died with an X
  `BadDrawable` error; the watchdog restarted it until it gave up. The
  shell now opens those device nodes; the renderer's sandbox is unchanged.
  Backported from Nordstjernen.
* `internet-browser` is a second name for the browser. `meson install`
  links it to `southstar` in the binary directory; the `.deb` and `.rpm`
  packages register it with `update-alternatives`, so Southstar installs
  beside Nordstjernen and Northstar and `update-alternatives --config
  internet-browser` picks which one the name starts; the Alpine package
  creates the link when the name is free; the Windows installer adds it
  as an App Paths alias and the MSIX package as an app execution alias.
* The animated cartoon splash is gone from `about:southstar`, which now
  opens on the logo and version. `data/about-splash.gif` is no longer
  embedded in the binary, and `scripts/gen-splash.py` and
  `scripts/gen-splash.sh`, which drew it, are removed with the still
  `data/about-splash-1.0.30.png` they wrote.

Southstar Browser 1.0.0:
======
* Rust 1.91 is now the minimum again (it was 1.99), so Southstar builds
  with the distributions' own compilers: Ubuntu 24.04's `rustc-1.91` and
  `cargo-1.91` packages, and the Rust 1.96 and 1.97 that Debian forky and
  sid ship. `rust-toolchain.toml` pins 1.91.0, so CI keeps the code within
  that version. One `Box::new_zeroed` call, the only newer API in use, became
  `Box::new(MaybeUninit::zeroed())`. The code needed no other change apart
  from lint fixes for clippy 1.91.
* Distribution packages are optimized. Debian, Fedora and openSUSE build
  with meson's `plain` build type, which compiled the Rust code with cargo's
  unoptimized `dev` profile. `plain` now selects the `release` profile.
* Releases add `.deb` packages for Debian forky and Debian sid, next to
  Ubuntu 24.04, Ubuntu 26.04 and Debian 13. Southstar's FFmpeg calls, struct
  fields and constants are all present in FFmpeg 9, the version forky and
  sid ship.
* Debian packaging: the source package now builds without network access
  from a `vendor/` component tarball of the crates.io dependencies
  (`debian/README.source` shows how to make it). The description says Rust,
  not C. Build dependencies follow the current build: the GTK and FFmpeg
  version floors, python3, freetype, and poppler for the PDF viewer.
  `ca-certificates` is now a dependency, since rustls reads the system CA
  bundle. The build date comes from the changelog, so builds are
  reproducible. The unversioned embedding library is no longer shipped, and
  `debian/copyright` lists the Boa fork, the wgpu-native headers and the
  Unicode tables. The changelog drops two entries that were Nordstjernen's
  and starts at 0.9.12-1.
* Sixty-one more CSS properties parse, cascade and show up in
  `getComputedStyle`. Before, the parser dropped them, so `CSS.supports()`
  and `@supports` said no. They include `text-underline-offset`,
  `text-decoration-thickness`, `touch-action`, `color-scheme`, `zoom`,
  `resize`, `scroll-behavior`, the `font-synthesis-*` and `text-emphasis-*`
  longhands, `line-break`, `print-color-adjust`, `flood-color`,
  `marker-*`, `margin-trim` and `field-sizing`. Each one is a row in a
  table (`rust/css/src/defs.rs`) giving its kind, whether it inherits, its
  initial value and its value grammar. Keyword values are checked against
  that grammar by the new `rust/css-grammar` crate, which also orders them
  canonically for `element.style`. `white-space` is now a shorthand for
  `white-space-collapse` and `text-wrap-mode`, and the cascade works out
  the old `white-space` keyword that layout reads from those two. Before,
  `text-wrap` was mapped onto `white-space`, so
  `white-space: pre; text-wrap: wrap` lost the preserved spaces.
  `text-wrap`, `font-synthesis`, `text-emphasis` and
  `-webkit-text-stroke` are shorthands too. CSS-wide keywords and
  `currentcolor` work for all of them. Keyword-only properties no longer
  interpolate as numbers. Ported from Nordstjernen.
* Fixed-position boxes no longer extend the document's height: the
  scrollable page, `document.documentElement.scrollHeight` and headless
  PNG dumps counted them, so YouTube's hidden guide drawer
  (`top: -120px; bottom: -120px`) left a 120 px white strip below the
  viewport. Headless PNG dumps are now at least one viewport tall, like
  the window they stand in for.
* MutationObserver records queued from a timer, idle or animation-frame
  callback are delivered again. Host microtasks ran in whatever realm the
  engine was left in after the callback, so the delivery job could not
  find its page and stayed "scheduled" forever; every later mutation on
  the page was then queued and never reported. Polymer's microtask queue
  rides a MutationObserver, so YouTube's consent dialog never stamped its
  text and buttons. Host jobs now run in the realm that queued them.
* YouTube's home page renders instead of staying at grey placeholders.
  Its 10.8 MB application bundle spent the whole 60 s script limit in
  Boa's parser and bytecompiler, before any of it ran: every identifier
  was resolved by a linear search of each enclosing scope's bindings,
  and the bundle's wrapper function declares thousands of names. Scopes
  now index their bindings by name, and the parser no longer copies the
  whole subtree of every parenthesized expression and var-scoped
  declaration; loading the bundle alone went from over 90 s to about
  13 s.
* Windows: the JavaScript native-stack limit is sized from the thread's
  real stack (`GetCurrentThreadStackLimits`) instead of a fixed 1 MiB, so
  workers on their 8 MiB threads no longer stop at 768 KiB. Large worker
  scripts such as reCAPTCHA's failed with "Maximum call stack size
  exceeded" on Google's search page.
* Windows: opening a tab no longer logs a GTK critical
  (`gtk_native_get_surface: assertion 'GTK_IS_NATIVE (self)' failed`): the
  page's input-method context is attached to the page area only while the
  area is realized.
* Google Search results render again instead of the "unusual traffic"
  reCAPTCHA page. `Function.prototype.toString` printed the JavaScript
  source of engine-implemented web APIs (`document.createElement` showed
  its polyfill body) since the move to Boa; Google's challenge script
  treats non-native built-ins as tampering and its token was rejected.
  Engine functions print as `function createElement() { [native code] }`
  again, through the HostHasSourceTextAvailable hook.
* Functions that sit beside a direct `eval`, or that were created by
  indirect-eval code, keep their local variables in registers: the scope
  analysis only makes the bindings of functions that actually contain the
  `eval` reachable by name. Such code ran up to six times slower, and
  Google's search challenge script (a bytecode VM delivered through
  `(0,eval)`) took twice as long to produce its token.
* Internal: some 270 of the cross-crate `ns_*` C-ABI exports and about
  500 `extern` declarations are replaced by ordinary Rust calls with typed
  arguments across the js-* bindings, net and its clients (font, image,
  WebSocket, EventSource, workers), layout, paint, animation, SVG, video,
  forms, camera, microphone, spell checking and the headless/GTK hosts.
  net gains a Rust request API (`FetchRequest`, closures for completion),
  the blob registry holds `Arc<[u8]>` (fixing an unreferenced `GBytes`
  returned after the registry lock was dropped), and duplicate copies of
  the response struct are gone. Across the tree `unsafe` falls from 7692
  to 6289, FFI imports from 3160 to 2668 and exports from 797 to 517.
* Internal: js-runtime no longer calls up into the binding crates through
  C-ABI symbols. Each provider (frames, mutation, storage, loader,
  dispatch, URL, focus, observers, services, window, workers and others)
  registers a table of typed Rust function pointers at startup, and an
  unregistered table answers like the old empty state. js-runtime's
  fetches go through net's Rust request API and own their `Response`.
  This removes about 115 more `extern` declarations and 107 exports;
  `unsafe` across the tree is now 5979.
* The unsafe-budget check reports its failures as a GitHub annotation, so
  they are readable without the job log.
* CSS animations attached to a scroll or view timeline
  (`animation-timeline` other than `auto`) no longer play as if on the
  document timeline. With `animation-duration: auto` and
  `animation-fill-mode: both` they jumped straight to their last keyframe:
  ChatGPT's sidebar, whose grid column is a scroll-driven animation from
  260px to 52px, was collapsed to its 52px rail. Such animations now stay
  inactive, as an inactive timeline leaves them, until scroll-driven
  timelines are implemented; `animation-timeline: none` is inactive too.
* A stretched grid item's height is definite for its children: an item
  that `align-self: stretch` grows to fill its row is laid out again at
  that height when a child asked for it, so `height: 100%` inside it
  fills the row instead of collapsing to content. ChatGPT's centered
  home composer sat at the top of the page because of this.
* A microtask checkpoint no longer runs inside another one. Native calls
  that flush layout (for example `getAnimations()`) could drain the
  microtask queue while a microtask was still on the stack, so a
  scheduler that re-queues itself while it is busy (ChatGPT's) spun until
  the 60-second script monitor halted the page, leaving it half rendered.
  The nested checkpoint now returns at once, as the HTML Standard's
  "performing a microtask checkpoint" flag requires.
* Security: deeply nested script no longer crashes the tab. Boa checked
  the native-stack limit only when it called a function, so the parser
  and every recursion that does not pass through a call overflowed the
  renderer's 8 MiB stack and killed the process: about 330 nested array
  literals or parentheses, 150 nested functions, a 24,000-term `+` chain,
  `JSON.stringify` of 8,000 nested arrays, or a property read through a
  40,000-object prototype chain or 26,000 nested proxies was enough. The
  parser, `JSON.stringify` and the reviver of `JSON.parse`,
  `Array.prototype.flat`, prototype-chain property lookups, proxies and
  bound functions now stop at the same limit as calls and throw a
  catchable `RangeError` ("Maximum call stack size exceeded").
* Security: the address bar can no longer be made to show a URL other
  than the page's. The shell read the renderer's replies by searching the
  JSON text for the quoted key, so a page title or URL that spelled
  `"url"` or `"security"` itself was taken for the member: a title ending
  in `\"url` hid the real URL of a page reached by a redirect, and the
  address bar kept showing the address that redirected there. The reader
  now skips string values and takes a key only where it names a member.
  Ported from Nordstjernen.
* Security: the Landlock sandbox of the renderer and the shell no longer
  opens the whole home directory when `$XAUTHORITY` is `~/.Xauthority`,
  as LightDM sets it on X11 sessions (Linux Mint, Xubuntu). The sandbox
  granted read access to the directory holding that file, which was then
  `$HOME`, so `~/.ssh`, `~/.aws` and every other file there were readable
  from the renderer that runs page content. It now grants read access to
  the `$XAUTHORITY` file alone. Ported from Northstar.
* Security: a local HTML file can no longer read other local files. Every
  `file:` document is now an opaque origin: `fetch()` and
  `XMLHttpRequest` from a `file:` page treated every other `file:` URL as
  same-origin, so a page opened from the downloads folder could read
  another program's tokens under `~/.config` and list directories, and a
  local image from another folder drawn into a canvas did not taint it. A
  local page still loads its own scripts, style sheets and images.
  Ported to Nordstjernen and Northstar under the same name.
* Cookies set by responses that arrive at the same time are all kept.
  Every Set-Cookie header read the site's jar file, added its cookie and
  wrote the whole file back with no lock, so concurrent responses, in one
  renderer or several, overwrote each other: of eight cookies set by
  eight parallel `fetch()` calls, the next request carried five or six,
  and a rotated session cookie could come back. Jar writes now hold a
  process-wide lock and an exclusive lock on `cookies/.lock`.
  `document.cookie` also follows the jar: it was a copy taken when the
  page loaded, so a cookie set or deleted by a `fetch()` or XHR response
  never appeared, which broke sites that copy a CSRF token from a cookie
  into a request header. A read now takes the jar again when its version
  (the process's write count and the jar files' size and modification
  time) has changed.
* Security: a web page can no longer navigate to the browser's privileged
  pages. A link, a location assignment, a form submission or a Refresh
  could open about:settings-save, about:settings-clear, about:history,
  about:config or view-source:, and since a navigation carries no top
  URL the about: handler took it for the browser's own UI. Any site
  could rewrite the home page and search engine, turn off JavaScript or
  HTTPS-First, or wipe history, cookies and site storage. Those targets
  are now reached only from a browser about: page, settings-save and
  settings-clear act only on a POST (never a GET query string), and the
  safe-browsing continue link is followed only from the warning page for
  the same host, so another page cannot skip the warning. Ported from
  Nordstjernen.
* Security: XMLHttpRequest adds `X-Requested-With: XMLHttpRequest` only
  to same-origin requests. It went on every request, cross-site ones
  too, and sites read the header as proof that a request came from their
  own script, because another origin could only add it after a CORS
  preflight. Ported from Northstar.
* A page whose html element scrolls (overflow: auto) while its body has
  overflow: hidden scrolls again. Overflow propagates to the viewport
  from the root element, and from the body only when the root is
  overflow: visible; any hidden html or body locked the whole page.
  DuckDuckGo's result page could not be scrolled.
* Scripts written into an iframe with document.open(), write() and
  close() run, in the frame's realm. They were parsed into the frame's
  document and never executed.
* postMessage() called on a frame's contentWindow gives the message
  event the calling window as its source. The call ran in the frame's
  realm, so the frame saw itself as the sender and a check of
  event.source === parent failed. Together these made clicks on
  DuckDuckGo results do nothing.

Southstar Browser 0.9.11:
======
* The logo is the N of the Nordstjernen browser again instead of an S:
  the scalable icon, the animated about-page GIF and the Windows .ico
  are regenerated from it.
* A page that deletes a property from a prototype, such as
  Object.prototype, no longer crashes the tab or reads the wrong value.
  The script engine's property cache remembered where an inherited
  property sat in the prototype and kept using that position after the
  prototype lost a property, also when a getter or setter deleted it
  while it ran, reading past the end of its storage or the property that
  had moved into that place. Nine lines of script were enough to abort the
  renderer. The same cache also kept writing a property of
  Object.prototype after the page made it read-only.
* JS: Streams follow the WHATWG Streams standard. `ReadableStream`,
  `WritableStream` and `TransformStream` queue chunks by size, apply
  backpressure through `highWaterMark`, call `pull()` only when there
  is room, and `pipeTo()`/`pipeThrough()` honour `preventClose`,
  `preventAbort`, `preventCancel` and an abort `signal`. `tee()`,
  `ReadableStream.from()`, async iteration, BYOB readers over byte
  streams (buffers are transferred, not copied), the queuing strategies
  and the controller classes are there too. A stream whose source
  enqueues from `pull()` never delivered a chunk before. Not there
  yet: `byobRequest` is always null and `autoAllocateChunkSize` is
  ignored, so a byte source has to `enqueue()` its chunks.
* Fetch: `Response.body` and `Blob.stream()` are byte streams, so
  `getReader({mode: 'byob'})` works on them. The body stream of a
  buffered response is made on first use and copies nothing until it
  is read. `clone()` tees a streamed body and gives the clone its own
  copy of each chunk, and `bytes()` returns its own copy, so
  transferring one buffer no longer empties the other. A body whose
  stream is locked or was already read makes `text()`, `json()`,
  `arrayBuffer()`, `bytes()`, `blob()` and `formData()` reject with a
  TypeError, and `clone()` throws for it.
* A ::before or ::after whose content is none or normal generates no
  box, as the standard says. An absolutely positioned or inline-block
  pseudo-element with content: none was still drawn, with its size and
  background, and its counter-increment and counter-reset still counted.
* JavaScript: removed the last site-specific hack, a script that ran
  after every script evaluation on freecivweb.com and filled in missing
  match_style/sprite_type fields of the page's tileset table. Boa
  evaluates the site's own tileset configuration correctly, so the shim
  no longer did anything.
* Cairo is declared once, in a new rust/cairo crate (southstar-cairo)
  with safe RAII types (Context, Surface, Pattern, Path, Matrix and
  enums for operators, fill rules, line caps and formats) and pixel
  access lent only to a closure; the painter draws through it instead of
  its own 88 redeclared Cairo functions and raw cairo_t pointers, and
  its unsafe count drops from 426 to 332.
* The DOM, CSP and the JavaScript runtime export no C symbols any more:
  the bindings, canvas, paint, the shell and the headless driver call
  dom's attribute, index, tree and control functions directly, the page
  record owns a Rust Csp that frames, the loader and documents query
  through its methods, and headless creates and configures pages through
  js-runtime's API. The Performance API, image tracking and custom
  element reactions are reached through js-perf's, js-images's and
  js-custom-elements's Rust APIs, with the runtime calling back into
  them through tables of Rust functions the page installs at start-up.
  Bookmarks, history, the HTTP cache, debug logging, the sandbox and
  textures likewise drop their C exports for typed APIs (a counted
  TextureRef handle, an RAII log subscription). dom drops from 87 unsafe
  keywords to 14, js-runtime from 366 to 305 and the sandbox from 104 to
  79, and csp forbids unsafe code.
* DOM: <link rel="preload" as="style"> fires load (or error) once its
  fetch ends, and changing a <link>'s rel, href or as loads it again: a
  link that becomes rel="stylesheet" applies its sheet and fires load.
  The common async-CSS pattern, <link rel="preload" as="style"
  onload="this.rel='stylesheet'">, never applied its sheet, so
  chatgpt.com kept its narrow-screen styles and its cookie dialog
  covered the whole page in white.
* DOM: <link rel="stylesheet"> elements from the document's own markup
  fire load (or error) after DOMContentLoaded; only links inserted by
  script did before. The loadCSS pattern, a media=print link whose onload
  switches it to media=all, never ran its handler and its sheet stayed
  print-only.
* Layout: a table caption's ::before and ::after are built like a table
  cell's, so a floated or block pseudo-element inside a caption is a float
  or a block, and an absolutely positioned one is laid out. Captions turned
  every pseudo-element into inline text, so the floated magnifier slot in
  Wikipedia's thumbnail captions took no room and the caption text wrapped
  differently from other browsers.
* The layout crates no longer call each other through C symbols. A call
  down the crate order is an ordinary Cargo dependency; a call that has
  to come back up (block layout into flex, grid, table and inline, the
  box builders table construction needs, intrinsic-size measures, the
  positioned-box passes) goes through a small table of plain Rust fn
  pointers that the top-level layout entry fills once. Block, build,
  inline, sizing, table, tree, float, flex, grid and positioned layout
  drop from 445 unsafe keywords to 89, and layout-flex forbids unsafe
  code.
* The browser crate and the headless driver drive the engine, painter,
  selection, hit testing, layout queries, video, animation, image cache,
  printing, HTML helpers and the focus, forms, input, lifecycle and
  window bindings through typed Rust APIs instead of about 150 C
  exports: the animation engine, image cache and video cache are owned
  handles, embedder callbacks are Rust closures, text and layout dumps
  come back as Vec<u8>, print pages as a Vec, and blocking navigation
  returns net's own Response type instead of a mirrored C struct. The
  shell, engine, browser and renderer host reach net, the configuration,
  the watchdog and the image decoders the same way, so html-util,
  image-decoders, config and watchdog export nothing. browser drops from
  678 unsafe keywords to 546, headless from 236 to 164, engine from 223
  to 144 and video from 204 to 116.
* rust/css keeps a parsed style sheet in Rust-owned types instead of
  the GLib arrays, hash tables and strdup'd strings the C code used:
  rules are a Vec of Rule structs owning their declarations, waiting
  var() values, layer and container conditions and scopes; imports,
  font faces, keyframes, @property rules and the layer table are Vecs
  and HashMaps; the rule index is a HashMap of buckets hashed with
  SipHash, so selectors in page CSS cannot crowd one bucket; and parsed
  selectors are plain Rust data rather than C structs. The hand-written
  free functions and struct layout assertions are gone, and the parser,
  selector, rule index and registry modules no longer contain unsafe
  code. rust/css drops from 538 unsafe keywords to 255.
* The JavaScript bindings reach the page's runtime record through
  rust/js-runtime's Rust API instead of its C ABI: frames, the loader,
  window, lifecycle, dispatch, focus, the page, networking, services,
  documents, workers, the top layer, images, forms, storage, geometry,
  input, URLs, custom elements, mutation, the browser crate and a dozen
  smaller crates call typed functions (Node, &CStr, bool, Option, tuples,
  closures for realm and frame entry) where they redeclared about 380
  extern "C" functions taking a raw page pointer, C strings and
  out-pointers. Navigation timing lives in js-runtime with named fields
  rather than in three copies. js-runtime's C exports drop from 208 to
  19, and its unsafe count from 792 to 366.
* rust/css no longer exports any C symbol. The animation crate reads
  computed values, declaration blocks, animation lists and keyframes
  through owned Rust types (OwnedValue with a Cell reference count,
  Declarations, Keyframes, RetainedStyle) instead of seventeen C exports
  and its own copies of rust/css's structs. The engine and the renderer
  parse, cache and free style sheets through an OwnedSheet handle that
  releases itself on drop, compute styles through compute_styles() and
  track container sizes in a ContainerMap instead of thirteen more; the
  CSSOM, layout, paint and selection call typed functions for value
  serialization, dimensions, overflow, column counts, text orientation
  and srcset sizes instead of eight more. rust/css drops from 735 unsafe
  keywords to 538 and from 49 C exports to none.
* The GTK shell and the headless driver drive the sandboxed renderer
  through a Renderer type whose methods return owned Rust values,
  instead of 46 ns_rproc_* C exports that filled #[repr(C)] structs of
  g_malloc'd strings each caller had to mirror and free. The renderer
  client exports nothing now and its unsafe count drops from 291 to 98;
  the shell's drops from 976 to 914.
* rust/style, the computed-style view layout, paint, SVG and printing
  read, no longer keeps its own #[repr(C)] copies of rust/css's value
  union, style struct and variable map, which had to match rust/css's
  layout by hand: it wraps rust/css's own types, whose new accessors
  check the union tag before reading a field. Six C exports for
  keywords, the display type, lengths and custom-property lookup are
  gone, and rust/style drops from 54 unsafe keywords to 12.
* rust/css asks the painter about fonts through a FontOracle of plain
  Rust functions instead of three unsafe extern "C" callbacks that took
  raw C strings and wrote through an out-pointer, and the headless
  driver matches selectors through southstar_css::SelectorList instead
  of a hand-freed GPtrArray and two C exports.
* The length, height and edge helpers the layout crates share live in
  rust/layout-geom, a new crate that forbids unsafe code. Block, flex,
  grid, table, sizing, positioned, float, inline, hit-testing, query and
  tree layout and the painter call its Rust API through a Cargo
  dependency instead of nineteen C exports from the box-tree builder
  that took raw style, box and value pointers and wrote their results
  through out-pointers. It reads calc() math functions, the writing mode
  and alignment keywords through rust/css's Rust API rather than its C
  ABI. The unsafe budget drops by 138 unsafe keywords and 73 FFI imports.
* The GTK shell (rust/shell) no longer calls its own code through C
  symbols: the tab window, the tab view's input, drawing, media and
  chrome modules, the page layers, the icons and the application entry
  point reach each other through ordinary Rust paths instead of
  extern "C" declarations and #[no_mangle] exports, with bool, &CStr
  and Option<&PageLayers> arguments where they took C booleans, strings
  and raw pointers. Of its 120 exports only main and ns_app_self_exe
  (which rust/net imports) remain, wrappers that only forwarded to a
  private function are gone, and the shell's unsafe count drops from
  1343 to 976.
* Style sheets have a typed view in rust/css, southstar_css::Sheet, with
  its font faces, imports, @page rule and container/hover/active flags.
  The renderer and the engine read sheets through it instead of their own
  copies of the C struct's first fields (SheetHead, NsCssStylesheetHead),
  which had to be kept in step with rust/css by hand. The selector cache,
  render zoom, container-feature tracking, style-element caches, relayout
  scope and <style>/adoptedStyleSheets text are Rust functions too, the
  frame-viewport callback the engine installs is a Rust fn(Node) instead
  of an unsafe extern "C" fn writing through out-pointers, and 25 more
  ns_css_* exports are gone.
* rust/css has a typed Rust API for the state other crates set and the
  marks they leave: the focus, hover, active, focus-visible and
  fullscreen nodes, the document base, language and :target fragment,
  :visited URLs, restyle marks for attribute, child-list and inline-style
  changes, :defined custom elements, CSS.registerProperty() and property
  ids, names and inheritance. Twenty crates -- the DOM and form bindings,
  focus, frames, mutation observers, the browser and headless engines,
  render, print, selection, style and the animation engine -- call it
  directly with Node and byte-slice arguments instead of 22 ns_css_*
  exports taking raw node and C-string pointers.
* The style pass in rust/css no longer runs on GLib arrays and its own C
  ABI. Each element's matched declarations, custom properties and
  var()-dependent values are collected into Rust Vecs instead of GArrays
  read back through pointer casts, the parsed values a pass owns are freed
  by a Drop guard, layer ranks are a HashMap, the style-sharing key hashes
  the container stack field by field instead of copying its raw bytes, and
  the walk calls the gather, cascade, custom-property, sharing, fix-up and
  restyle steps as typed Rust functions. 52 #[no_mangle] exports that only
  rust/css itself called are gone (css: 1007 -> 845 unsafe, 153 -> 101
  exports); page layouts, text and renderings are unchanged.
* rust/css-prop-syntax, the @property <syntax> grammar, has no C ABI left
  and forbids unsafe code: rust/css, CSS.registerProperty() and the
  CSSPropertyRule bindings call its Rust API instead of six
  ns_css_syntax_def_* exports that handed a boxed syntax around as a
  void pointer, and it parses colours through rust/css-color directly
  instead of calling back into rust/css. Canvas and form controls parse
  colours and canvas fonts through rust/css's Rust API as well.
* An inline-block narrower than its content no longer shrinks below its
  min-content width: a long word in a narrow column now widens the
  inline-block instead of overflowing it.
* An inline-block or inline-flex with a percentage width and
  box-sizing: border-box keeps its padding when its container shrinks to
  fit it, instead of squeezing its content by the padding.
* white-space: nowrap on an inline element keeps its text on one line.
  Only a block's own nowrap was honoured, so nowrap spans wrapped at
  every space and between every CJK character (Rakuten's top menu,
  Wikipedia's page-numbered citations).
* A form whose target names an iframe now loads its response into that
  iframe, with a GET query or a POST body, instead of navigating the whole
  tab; a form inside an iframe navigates its own frame. Rakuten's tag
  manager posts to a hidden iframe on every page view, which replaced the
  page with a blank document.
* Animations and transitions follow the Web Animations interpolation
  rules more closely. An effect whose easing overshoots (iteration
  progress below 0 or above 1) extrapolates from the first or last two
  keyframes, including an implicit neutral keyframe, instead of holding
  the end value. Results are clamped to the property's range: lengths
  that cannot be negative (border, padding, sizes, border-image parts)
  stop at 0, opacity and color alpha stay within 0..1, and a fully
  transparent result has no color left. initial, unset and inherit in
  keyframes resolve to the initial or the parent's value and
  interpolate rather than flipping halfway, a shorthand in
  transition-property (border-width) transitions its longhands, and the
  initial border widths are medium (3px). Shadow lists of different
  lengths, and none, interpolate by padding the shorter list with
  transparent zero shadows, with premultiplied colors and blur kept at
  or above 0. em and rem lengths in keyframes (in lengths, calc(),
  shadows and transforms) resolve against the animated element's own
  and the root's font size instead of 16px.
* A border side whose style is set but whose width is not now draws at
  the initial medium width (3px) instead of none at all
  (border-bottom-style: solid alone gave no border), and a width set on
  a side with no border style no longer takes up layout space; computed
  border widths follow (0px for a none or hidden side).
* vertical-align is parsed as the CSS Inline 3 shorthand
  ([first | last] || alignment || shift, serialized in canonical order),
  with its alignment-baseline and baseline-shift longhands. Logical
  properties (inline-size, margin-block-start, ...) report their
  physical counterpart's resolved value in getComputedStyle, the
  margin-/padding-/inset- block and inline shorthands serialize like
  the other axis pairs, calc() with ex, ch, viewport, lh or cap units
  keeps its specified form, and style.length no longer counts
  declarations the parser rejected. A percentage inside a hue or an
  oblique angle's math function (hsl(calc(sign(50%) * 1deg) ...)) is
  rejected, and font-feature-settings takes only integer tokens
  ("liga" 1e1 is invalid).
* SHA digests and base64 come from rust/digest, a new crate that forbids
  unsafe code and hashes through ring, already in the tree for TLS. CSP
  hash-sources, Subresource Integrity, the HTTP cache keys, the bytecode,
  font, localStorage, IndexedDB and extension-storage file names, the
  safe-browsing host hashes, canvas.toDataURL, PDF page images and the
  headless screenshot no longer reach GLib's g_checksum/g_base64 or
  OpenSSL's EVP_Digest through unsafe FFI; every key and file name is
  unchanged, and GLib's checksum and base64 declarations are gone.
* URL, URLSearchParams, Response/Request bodies and getComputedStyle
  keep working in pages with iframes. The script helpers behind them were
  built once per page, in whichever window used them first, and then
  shared with every frame: URL objects made in a frame got the parent's
  URL.prototype, and on naver.com, where an ad frame got there first,
  every new URL() and new URLSearchParams() in the page threw "Illegal
  invocation" once that frame went away. Each window now has its own.
* A module may name its default import `of` or `async`
  (import of from "./react.js"). Both are ordinary identifiers there, but
  the parser rejected them with a SyntaxError, so a minified bundle that
  happened to pick one of those names - finn.no's advertising module -
  did not run at all.
* A custom property declared with an empty value (--x: ;) substitutes
  nothing into var(--x) instead of falling back. CSS minifiers such as
  lightningcss lower light-dark() to a pair of these toggles
  (var(--light, #264323) var(--dark, #141413)), so every such color came
  out invalid: crates.io lost its green header and page background.
* An absolutely positioned ::before/::after of an inline element starts
  where it sits in the line, like an absolutely positioned child element:
  it left no placeholder in the line, so with no insets it was placed at
  the top left of the paragraph. A box whose position comes from its
  line now starts at that line's top as laid out with the CSS
  line-height. MDN's external-link icons, an absolute ::after of each
  link, sat at the start of their paragraphs.
* Text decorations propagate from a block to the text of its block
  descendants, as CSS requires: a display: block or flex link around a
  <div> or <p> underlines its text, and text-decoration on a flex or grid
  container reaches its items. Only the element whose inline content held
  the text was consulted. Propagation stops at floats, absolutely
  positioned boxes and inline-blocks, and an element with display:
  contents decorates nothing (MDN's banner link).
* background-clip: text (and -webkit-background-clip: text) paints the
  background only inside the glyphs of the element's text, for blocks and
  for inline elements, and -webkit-text-fill-color sets the colour text
  is filled with. Gradient headlines (nodejs.org's "Run JavaScript
  Everywhere", Tailwind's bg-clip-text text-transparent, vuejs.org's
  tagline) showed a solid gradient rectangle or no text at all.
* The legacy -webkit-linear-gradient(), -webkit-radial-gradient() and
  their repeating forms are understood, with their old conventions
  (angles measured from east, counter-clockwise; a bare side naming where
  the gradient starts; "position, shape" for radials), as Chromium and
  Firefox still accept them. vuejs.org's tagline gradient and older
  sites' button and banner gradients were dropped.
* The ECMAScript built-in prototypes (Array, Date, Function, Number,
  String, RegExp, Boolean and the typed arrays) no longer carry an own
  Symbol.toStringTag. The interface setup gave one to every global
  constructor, so Object.prototype.toString reported "[object Date]" for
  plain objects inheriting from Date.prototype, and the common
  which-typed-array package, which reads the getter on
  %TypedArray%.prototype, threw "a function is required" and stopped
  Wired's page script.
* Playing a WebM or other libav-decoded video whose width is not a
  multiple of 16 no longer corrupts memory. The frames were converted
  into a buffer exactly four bytes per pixel wide, and FFmpeg's
  vectorized converters write whole blocks of pixels, so the last row
  ran past the end of the buffer; Discord's homepage video crashed the
  browser.
* Array.prototype.sort and TypedArray sort no longer abort the browser
  when the compare function is inconsistent (returns random or
  contradictory results). JavaScript leaves the order unspecified in
  that case; the Rust sort the engine used detected the inconsistency
  and panicked, which killed the tab on Zillow's homepage.
* Changing an element's inline style, or animating it, restyles that
  element alone; its descendants are recomputed only when a value they
  inherit changes. Every style="" write (element.style.foo = ...) and
  every animation frame used to recompute the element's whole subtree,
  so pages that move or fade large containers from script, and
  Microsoft's web-component homepage, spent most of each layout in
  selector matching.
* The text of a block ::before or ::after collapses its white space like
  any other text, so the widespread clearfix content: " "; display:
  block; clear: both no longer adds an empty line under every container
  that uses it. NPR's front page came out 700px taller than in Chromium,
  EFF's tagline sat 64px low, and apple.com and Smashing Magazine had
  gaps between sections.
* Text before a <br> keeps its formatting up to the break. Dropping the
  space before the break left the positions of the text after it one
  byte too far, so an element that ended on that space ended inside the
  line break instead: paulgraham.com's links box drew two replacement-
  character boxes where its spacer image sits.
* A block's negative horizontal margins reduce its parent's max-content
  width, as in other browsers. The shrink-to-fit measurement took the
  larger of the child's margin box and its content width, so negative
  margins never counted: Wikipedia's main-menu button (margin: 0 -12px)
  made its header slot 44px wide instead of 20px and pushed the logo
  24px to the right. Inline-blocks, floats, table cells and flex items
  holding such a block came out too wide in the same way.
* A module loaded by <script type="module" src> and imported again from
  another module, or by import(), runs once, as the module map requires.
  The script element evaluated its own copy outside the map, so the
  module's top level ran twice and its state was split in two: Nuxt
  sites (GitLab's homepage) failed with "Context conflict" and a
  hydration mismatch because the app was set up twice.
* Promise callbacks queued by an event listener run after the script
  that dispatched the event, as the HTML event loop requires: el.click()
  or dispatchEvent() from script no longer drains microtasks between
  listeners. A real user click still runs them after each listener.
* ResizeObserver and IntersectionObserver callbacks no longer run in the
  middle of a script that forces layout (getComputedStyle, offsetWidth);
  they are delivered once the script has finished. Microsoft's store
  pages ran a component's resize handler before the component had
  rendered and threw "cannot convert null to object".
* Text direction comes from the CSS direction property and the dir
  attribute only. One right-to-left element anywhere on a page (a single
  direction: rtl inside an ad) used to switch every later paragraph's
  layout to right-to-left, so on Yahoo the header search box, an
  inline-flex form, was pushed its own width to the right and covered the
  navigation links. Arabic or Hebrew text in a left-to-right paragraph now
  stays left-aligned as in other browsers, and dir="auto" still follows
  the text.
* Inline blocks, images and other atomic inlines on right-to-left lines
  sit where their glyph box is. They were drawn starting at its right
  edge, one box-width too far right; Arabic Wikipedia's infobox images
  spilled over the article text.
* Outlines follow the border-radius of their box, as in current
  browsers: the curve grows by outline-offset and shrinks for a negative
  offset. They were always drawn as rectangles, so pill-shaped search
  fields and rounded buttons (Yahoo's search box, focus rings) showed
  square outlines around round corners.
* Elements have an onerror property on HTMLElement.prototype (and on SVG
  and MathML elements and the document), like every other event handler.
  It was missing from the prototypes, so scripts that wrap the built-in
  handler accessors failed: Yahoo's script-loading guard read
  Object.getOwnPropertyDescriptor(HTMLElement.prototype, "onerror") and
  threw on every script it inserted. window.onerror keeps its own
  five-argument form.
* Arial, Helvetica, Times New Roman and the generic serif and sans-serif
  families render in the metric-compatible fonts other browsers pick
  (Liberation Sans and Liberation Serif on most Linux systems). A family
  that was not installed fell back to fontconfig's default sans-serif,
  and serif to DejaVu Serif, both much wider, so headings and buttons
  wrapped where Chromium kept them on one line. The generic families
  now default to Times New Roman and Arial as in Chromium, and a missing
  family takes the font fontconfig aliases to it by metrics.
* SVG icon sprites referenced with <use> render, in the colour of the
  text around them. A <use> pointing at a <symbol> in another <svg> of
  the page found nothing, because ids were looked up only inside the
  <svg> being painted. A sprite's fill="currentColor" was also already
  resolved by the cascade to the colour of the hidden sprite sheet, so
  every icon painted black. The referenced content now inherits its
  colour from the <use> element, as a shadow tree would.
* Images and SVGs sized with calc() contribute that size to grid and
  table tracks. The min-content contribution of a replaced element read
  only its laid-out width and fell back to 200px, so Tailwind's
  size-4 icons (calc(var(--spacing) * 4)) widened an auto grid column to
  200px and squeezed the text beside them: tailwindcss.com's "Quick
  search" button wrapped onto two lines.
* An absolutely positioned ::before or ::after is painted in tree order
  with the other positioned boxes of its stacking context. Its box has no
  DOM node of its own, so the paint sort could not place it and it came
  last, above every later position: relative element: the hero heading,
  button and illustration on svelte.dev were hidden under the hero's
  ::before gradient.
* em, i, cite and dfn follow their computed font-style. The inline
  builder made them italic by tag name even when a stylesheet set
  font-style: normal, so Stripe's hero heading (an em styled upright)
  rendered slanted.
* Assigning an array's length a second time from the same line of script
  removes the elements past the new length. The first assignment did, but
  the property cache then treated length as an ordinary property, so
  arr.length = 0 left the old elements in place and the array kept them
  once refilled; Twitch's GraphQL client failed with "unknown value" and
  rendered no stream directory.
* Running out of call stack throws a RangeError ("Maximum call stack
  size exceeded") that scripts can catch, as in other browsers. It used
  to end the script with an uncatchable error, so code that measures the
  stack depth or falls back after deep recursion stopped; x.com's login
  page logged it from every timer and message handler.
* The CSS Font Loading API works. document.fonts returned a new object
  on every access and was not an EventTarget, so
  document.fonts.addEventListener("loadingdone", ...) threw and stopped
  Vercel's homepage script; FontFace objects were not instances of
  FontFace, ignored their source, and document.fonts.add() did nothing.
  The set is now one EventTarget per document with add, delete, has,
  clear, size, iteration and the onloading handlers; an added FontFace's
  url() or binary source is installed as an @font-face rule, so the font
  renders; load() fetches it and settles loaded and status.
* Scripts and stylesheets written in the page's HTML resolve their src and
  href against <base href>. Only the engine's stylesheets, images and
  preloader used the base; the script loader resolved against the page
  URL, so a page at /sub/dir/ with <base href="/assets/"> fetched
  /sub/dir/app.js and ran nothing.
* Gradient backgrounds honour background-size, background-position and
  background-repeat. A gradient always filled its whole box, so a
  linear-gradient(red, red) 0 100% / 100% 2px no-repeat underline bar
  became a solid block and tiled dot or stripe patterns became one large
  shape. Inline elements (span, a) now paint their gradient backgrounds as
  well; only url() images were drawn.
* The width in <meta name="viewport"> is ignored, as desktop browsers
  ignore it. A page asking for width=1120 (Wikipedia) was laid out 1120px
  wide in a narrower window, so its narrow-window media queries and
  matchMedia() never matched and the page scrolled sideways.
* text-decoration: inherit, initial, unset and revert take effect. The
  shorthand dropped CSS-wide keywords, so a link styled with Tailwind's
  preflight (a { text-decoration: inherit }) kept the UA underline; the
  navigation and buttons on react.dev and other Tailwind sites were
  underlined.
* Text runs without font features no longer log a Pango critical. The
  FFI wrappers that turn a nullable pointer into an Option built the
  wrapper before checking the pointer, so a null result was dropped and
  freed: pango_attribute_destroy(NULL) ran for every such run (dozens of
  warnings per page on sites like Ars Technica), and a failed camera open
  closed fd -1.
* Iframes are inline-level boxes, 300 by 150 pixels unless sized, as in
  other browsers. They were hidden until loaded and then laid out as
  blocks, so a centred embed sat at the left, frames placed side by side
  stacked, and a loading="lazy" frame, having no box, counted as off
  screen and never loaded: dev.to's YouTube embeds stayed empty.
* Absolutely and fixed positioned content inside an iframe is placed and
  clipped within the frame. It used the top-level page as its containing
  block, so YouTube's player, which fills its frame with an absolutely
  positioned element, was drawn over the whole page.
* A framed document's html or body background fills the whole frame, and
  a framed document no longer inherits color, fonts or text-align from
  the element around its iframe.
* Promise reactions queued by a script inside an iframe run before the
  frame's next script, as they do in the top-level document. YouTube's
  embed bundle defines writeEmbed in a promise callback and the next
  inline script calls it, so every embedded player failed with
  "writeEmbed is not defined".
* Image requests advertise image/avif only in builds that can decode it.
  A build without libavif (and every Windows build, which refuses AVIF)
  still asked for it first, so CDNs that negotiate on the Accept header
  sent AVIF that could not be shown: every listing photo on Airbnb was
  missing.
* TaskController works where AbortSignal's attributes are accessors. The
  TaskSignal polyfill assigned aborted, reason and onabort, which its
  prototype defines without setters, so new TaskController() threw
  "cannot set non-writable property" in strict code; it now defines them
  on the signal itself. Airbnb's route preparation failed on it.
* A page with a live WebGL context no longer aborts the browser on exit.
  The script engine's heap is freed while thread-locals are torn down,
  and the WebGL context's destructor read two thread-locals that were
  already gone; it now skips them. x.com crashed this way about half
  the time after a headless dump.
* A connection the server or a proxy resets during a TLS exchange no
  longer kills the browser with SIGPIPE. The close_notify alert went out
  through writev(), which raises the signal on a dead socket; TLS records
  are now sent with send() and MSG_NOSIGNAL, and sockets on macOS set
  SO_NOSIGPIPE. Headless PNG, PDF and WPT runs ended with exit status 141
  on such sites.
* A zlib or Brotli stream whose initialisation fails is no longer torn
  down as if it had started: its wrapper was built, and dropped, before
  the result was checked.
* Request has a signal. new Request() left it undefined and ignored the
  signal, mode, credentials, cache, redirect, referrerPolicy, integrity
  and keepalive it was given; the request's signal now follows the one
  passed in (or the input request's), invalid option values throw, and
  fetch(request) aborts when the request's signal does. React Router's
  data loaders read request.signal.aborted, so GitHub's repository pages
  stopped short of loading their commit messages, branches and releases.
* An inline-block's min-content contribution is its own min-content
  width instead of its widest line, and a table whose width is set below
  its min-content width grows to fit it. Wikipedia's infoboxes, whose
  lists are inline-blocks, were laid out wider than their 22em width,
  and a table squeezed below its content drew its cells outside its box.
* Table cells that follow a rowspan land in the right column. Column
  counting and column widths ignored the slots a rowspan holds in later
  rows, so a cell after one was measured into the column to its left, or
  placed over the spanning cell with no width when the row ran past the
  counted columns. Cells spanning several columns now widen them by what
  they lack, in proportion to their widths, after the single-column cells
  have been measured; rowspan cells honour vertical-align; and colspan
  and rowspan take values up to 1000 and 65534 as HTML allows.
* XMLSerializer produces real XML: namespace declarations and prefixes
  per DOM Parsing, self-closing empty elements and escaped attribute
  values, instead of the element's HTML outerHTML. DOMParser returns a
  Document for XML types and a parsererror document for unclosed
  elements. HTML serialization no longer doubles a leading newline in
  pre, textarea and listing. Forms inside templates are parsed as the
  current HTML standard says.
* NUL characters in HTML no longer cut a document short. Page and frame
  loads, document.write and DOMParser passed the decoded text around as
  C strings, so everything after a NUL byte was lost; the text now stays
  a byte buffer from decoding to the parser, which handles NUL as the
  HTML standard says. Fragments parsed for a prefixed SVG or MathML
  context element ("svg:title") use its local name, so integration
  points work. WPT html/syntax: +113 subtests.
* clip-path basic shapes serialize canonically (lengths with units,
  default radii, fill rules, rounds and geometry boxes omitted,
  positions in x-then-y order, path data normalized), shape() is parsed
  by its grammar instead of only checked for "from", and shape-outside
  is a property (basic shapes, shape boxes and images). WPT css-shapes
  and clip-path parsing: +210 subtests.
* -webkit-transform, -webkit-transform-origin, -webkit-transition,
  -webkit-animation (and their longhands), -webkit-box-shadow,
  -webkit-filter, -webkit-box-sizing, -webkit-clip-path,
  -webkit-perspective (unitless lengths included), the -webkit- flexbox
  properties and the other WebKit aliases browsers accept now apply.
  They were dropped before, so pages that only set
  element.style.webkitTransform drew untransformed.
* element.style.length and item() list longhands, as CSSOM requires:
  setting "flex: 1" reports flex-grow, flex-shrink and flex-basis, and
  removing one longhand of a shorthand keeps the others.
* New properties: contain, will-change and column-height. The columns
  shorthand takes "<width> || <count> [/ <height>]" and serializes
  canonically; flex serializes as "grow shrink basis" and takes the
  content and intrinsic sizing keywords for its basis; text-transform
  combines a case with full-width and full-size-kana.
* getComputedStyle reports transform-origin and perspective-origin
  resolved to pixels against the border box.
* Fonts: font-style accepts an oblique angle ("oblique 14deg", calc()
  included, rendered slanted), font-palette is a property, font-stretch
  and font-width take calc() percentages, font-feature-settings
  serializes canonically (double-quoted tags, "on" and 1 dropped, "off"
  as 0; computed values sorted and de-duplicated), and the font
  shorthand keeps its font-stretch keyword when "normal" follows it
  ("normal expanded normal 16px serif"). WPT css-fonts: +340 subtests.
* CSS masks gain their missing longhands: mask-mode, mask-repeat,
  mask-position, mask-size, mask-origin, mask-type and the mask-border-*
  properties (with the -webkit- prefixed aliases sites use). The mask
  shorthand takes every layer component, resets mask-border, and
  serializes canonically, and getComputedStyle reports mask and
  mask-position. Masks drawn from images now follow mask-size,
  mask-position and mask-repeat instead of always fitting the image
  inside the box, so icon masks such as
  "mask: url(icon.svg) center / 16px no-repeat" draw at their size and
  place. background-size rejects negative lengths and three values.
  WPT css-masking: 7,164 to 8,676 passing subtests with css-backgrounds
  and cssom.
* Selectors serialize the way CSSOM specifies: combinators get spaces,
  attribute values are double-quoted strings with a lower-case flag,
  selector lists inside :is(), :where(), :not(), :has(), :host() and
  ::slotted() are serialized recursively (keeping the text of items a
  forgiving list drops), and An+B is parsed strictly by the CSS Syntax
  rules. The new rust/selector-text crate does this for selectorText.
* overscroll-behavior (with its -x, -y, -inline and -block longhands),
  interactivity and background-blend-mode are known properties, and
  the logical scroll-margin-block/-inline and scroll-padding-block/-inline
  shorthands and longhands map onto the physical sides. scroll-margin no
  longer accepts percentages and scroll-padding no longer accepts negative
  lengths. WPT: +254 subtests across css-overscroll-behavior,
  css-scroll-snap, compositing and css-ui parsing.
* CSS colours keep their colour space. lab(), lch(), oklab(), oklch()
  and color() with every predefined space (display-p3, rec2020,
  xyz-d50, ...) are parsed, computed and serialized in their own space
  instead of collapsing to 8-bit rgb(), and none components survive.
  Relative colours (rgb(from ...), oklch(from ... calc(l - 0.1) c h)),
  alpha(), contrast-color() and color-mix() with any number of colours,
  every interpolation space and hue method are new, including colours
  that refer to currentcolor. Missing and powerless components follow
  CSS Color 4, and transitions between non-legacy colours interpolate
  in Oklab. The new rust/css-color crate does the work; the WPT
  css-color parsing tests go from 5,321 to 11,803 passing subtests.
* An arrow function nested in another arrow function sees the this of
  the method around them: when the outer arrow did not mention this
  itself, the inner one got the global object, so code such as
  [1].map(() => [2].map(() => this.x)) read undefined. TipTap's
  editor commands threw because of it.
* innerWidth and innerHeight inside an iframe report the frame's size
  again: a relayout started from frame script overwrote them with the
  size of the top-level window.
* Absolutely positioned boxes found while laying out other absolutely
  positioned boxes get their static position from the same batched pass
  instead of a walk over their whole containing block each, so a relayout
  of a page with hundreds of positioned pseudo-elements is up to 20%
  faster.
* getComputedStyle() no longer forces a layout and copies every custom
  property name to count them before anything is read: length is
  computed when it is read, and the declaration's methods are created
  once per page instead of on every call.
* Matching CSS selectors no longer touches thread-local counters at every
  combinator step, which were slow in the Windows build; restyling pages
  with large style sheets is up to 17% faster.
* for-in loops no longer allocate a result object and a next() function
  for every loop and key, which makes React pages about 10% faster.
* The JavaScript garbage collector waits until the heap has doubled
  since the last collection instead of growing by 43%, so pages that keep
  a large heap alive spend less time collecting; Speedometer 3.1 is about
  3% faster.
* Speedometer 3.1's NewsSite-Next suite runs to completion. A property
  getter or setter that replaced itself with a plain value (as a frame's
  innerHeight does once the viewport size is known) left the script
  engine's property cache pointing at the wrong slot, so later reads
  threw "Illegal constructor" and broke React pages inside frames.
* Lit apps load again, so the Speedometer 3.1 TodoMVC-Lit suites run to
  completion: a const or let declaration list now continues at a comma on
  the line after an arrow function (minified Lit failed to parse), and
  trustedTypes works inside iframes instead of throwing "Illegal
  invocation".
* Scripts may nest calls 10,000 deep instead of 512, as in other
  browsers, while a native stack check still stops runaway recursion
  before it overflows the thread. Angular's router pipeline nests deeper
  than 512 calls at startup, so TodoMVC Angular added no todos and both
  Speedometer 3.1 Angular suites failed; they now run to completion.
* JavaScript runs about twice as fast in the Windows build. The garbage
  collector checked a thread-local flag on every value it released, and
  thread-locals are slow on the MinGW toolchain Windows builds use; the
  check now reads a shared counter unless a collection is under way.
* A headless run no longer ends early while a script is still loading:
  the settle loop could run nested inside a blocking fetch, find nothing
  else pending and quit before DOMContentLoaded, so Speedometer 3.1 runs
  stopped before their first suite about a third of the time.
* In classes, a method written "constructor"() { } with a quoted name is
  the constructor, super[key] evaluates the key before looking up the
  prototype it reads from, and delete super[key] evaluates the key
  before throwing.
* delete (true && x) no longer deletes x, (true && obj.method)() calls
  the method without obj as this, and (a) ??= function () {} leaves the
  function anonymous, as the language specifies.
* Array destructuring stops asking an iterator for values once it is
  done, and breaking out of a for-of loop over a generator that is
  itself inside a for-of loop now closes the inner iterator, so its
  return() cleanup runs.
* A variable that eval declares inside a function no longer haunts
  later calls of that function: once one call ran eval("var x = 1"),
  the next call saw an uninitialized x, so typeof x threw and a global
  x was unreachable. Each call now sees only what its own eval calls
  declared.
* eval("arguments") works in a function that does not mention
  arguments itself, and a function with default parameter values such
  as function f(a, b = 1) { var a; return a; } no longer throws a
  ReferenceError: the var starts with the parameter's value.
* Assigning to a let, const or class variable before its declaration
  runs, as in x = 1; let x; or let y = [y] = [], now throws the
  ReferenceError the language requires instead of quietly creating the
  variable, also between the cases of a switch statement, and a class
  name used in one of its own computed method names is in its temporal
  dead zone.
* A function whose try block returns a value now returns that value
  even when its finally block contains a nested try statement that
  starts its own return and then leaves it with break or continue;
  before, the abandoned inner value replaced the real one.
* The JavaScript parser accepts more valid code: let works as a label,
  a function name and an arrow function parameter in non-strict scripts,
  destructuring assignments take parenthesized targets such as
  [(a), (obj.b)] = list and nested patterns under computed property names
  such as ({ [key]: [x] } = obj), and await in a class field initializer
  is an ordinary name in scripts, as in other browsers.
* The parser also reports more of the syntax errors the language
  requires, among them var {a b} = obj, for await (;;),
  new obj?.method(), Function() parameter or body strings that close the
  function early, let, static or eval as the name or a parameter of a
  function whose body says "use strict", and 0755 or "\8" on the line
  after a "use strict" directive. 1,416 of test262's 1,483 staging tests
  pass, up from 1,401.
* eval inside a method, a class field initializer or a static block no
  longer crashes the JavaScript engine when it reads a let or const
  declared in an enclosing block, and calling a function again after an
  eval in another call added a var to it no longer crashes either.
* Regular expressions support duplicate named groups in different
  alternatives, such as /(?<year>\d{4})-\d\d|\d\d-(?<year>\d{4})/, with
  \k<year> and groups.year following whichever group matched, and accept
  supplementary characters such as 𝓓 written directly in group names.
  The JavaScript engine now passes every test of test262 (52,133 of
  52,133, test/staging aside).
* Scripts that keep their state in global variables run several times
  faster: reading and writing a global var now goes through a cache
  instead of a full property lookup each time (a loop over global
  variables runs about 3.7 times faster). test262 passes 52,129 of
  52,133 and runs in about two thirds of the time.
* Modules understand source phase imports: import source x from "m" and
  import.source("m") give a module's source object, an instance of the new
  AbstractModuleSource class, without linking or running the module, and a
  module can re-export it. Plain JavaScript modules have no source object,
  so importing theirs is a SyntaxError, as the proposal specifies; source
  stays an ordinary name elsewhere, as in import source from "m". test262
  reaches 52,126 of 52,133 (99.99%).
* In strict code, assigning to an undeclared variable throws even when
  the assigned expression itself creates that global; in non-strict code
  eval("var e") inside catch (e) works as in other browsers, and of two
  nested block functions with the same name only the outer one becomes a
  function-wide variable. test262 reaches 52,114 of 52,133 (99.96%).
* A script may start with an HTML close comment (-->) as old pages
  served inside HTML comments do, ({...rest,} = obj) is the syntax error
  the language requires, and export default class { static name() {} }
  keeps its own name method.
* (25).toExponential(0) is "3e+1" as in other browsers (ties round up),
  Date.parse rejects "-000000" years and dates beyond the representable
  range instead of misreading them, and Promise.try returns a promise the
  callback returns instead of wrapping it.
* A class field initializer that writes to its own class, such as
  id = ++Counter.count, no longer crashes the JavaScript engine.
* JavaScript decorators: classes and their methods, getters, setters and
  fields accept @decorator syntax, with decorators that replace methods and
  classes, add field initializers and extra initializers, read and write
  elements through context.access, and share context.metadata, which ends up
  as the class's Symbol.metadata. Classes also gain auto-accessors
  (accessor name = value), public or private, static or not. test262
  reaches 52,097 of 52,133 (99.9%).
* Non-strict functions have the legacy caller and arguments properties
  other browsers keep: f.caller is the function that called f (null for
  strict callers and top-level code) and f.arguments the arguments of
  its current call, so old scripts that read arguments.callee.caller no
  longer throw a TypeError. test262 reaches 52,075 of 52,133 (99.9%).
* JavaScript gains ShadowRealm: new ShadowRealm() runs code in a fresh realm
  with its own built-ins and global object, through evaluate() and
  importValue(), and only primitives and wrapped functions cross between the
  realms. All 64 of test262's ShadowRealm tests pass, which brings test262
  as a whole to 52,051 of 52,133 (99.8%). It replaces the page-level ShadowRealm,
  whose importValue() always rejected.
* Intl.DateTimeFormat called as a function on an object that inherits
  from it attaches the new formatter to that object, as Intl.NumberFormat
  does, under one fallback symbol per realm; Intl.RelativeTimeFormat
  speaks German; and Temporal rejects the "islamic" calendar, which only
  Intl.DateTimeFormat resolves, reads era and eraYear in
  PlainMonthDay.from and toPlainDate, and keeps a ZonedDateTime's offset
  by default in with(). test262's intl402 now passes in full (3,382 of
  3,382), and test262 as a whole passes 51,987 of 52,133.
* A script can call a function named async: async(x) was a syntax error
  unless an arrow followed. In non-strict scripts an assignment to a call,
  such as f() = 1, f() += 1, f()++ or for (f() in o), now parses and
  throws a ReferenceError when it runs, as in other browsers, instead of
  rejecting the whole script. test262 reaches 51,967 of 52,133 (99.7%).
* RegExp has the legacy static properties older scripts read after a match:
  RegExp.$1 to RegExp.$9, RegExp.input ($_), lastMatch ($&), lastParen
  ($+), leftContext ($`) and rightContext ($'), set by every successful
  match in the same window and cleared by a match through a RegExp
  subclass, as in other browsers.
* libsouthstar.dll links again on Windows: the whole-archive link no longer
  fails on the duplicate ProcessPrng import stubs std and getrandom carry.
* Modules can defer evaluation (the import defer proposal):
  import defer * as ns from "m" loads and links m but runs it only when a
  property of ns is first read, and import.defer("m") does the same
  dynamically; modules with top-level await in a deferred graph still run
  ahead of time. test262 reaches 51,936 of 52,133 (99.6%).
* Together, the Intl rewrites below take test262's intl402 from 2,416 to
  3,364 of 3,382 (99.5%) and test262 as a whole to 51,838 of 52,133
  (99.4%).
* Intl.DateTimeFormat follows ECMA-402 closely: locale lists are validated
  and resolved with their ca, hc and nu keywords, options are read in the
  specified order, dateStyle and timeStyle work alongside hour cycles,
  fractional seconds, eras, day periods and every timeZoneName style, IANA
  time zones (through the engine's Temporal) and offset zones such as
  "+05:30" format with the right local time, sixteen calendars and the
  simple-digit numbering systems are supported, and formatRange shares the
  parts both dates have in common. Patterns come from compact CLDR-style
  tables for twenty languages instead of a fixed day/month/year order.
  Date.prototype.toLocale*String and the toLocaleString methods of the
  Temporal types format through it, and DateTimeFormat formats Temporal
  objects directly.
* Intl.NumberFormat, Intl.PluralRules and Number/BigInt toLocaleString now
  follow ECMA-402: exact decimal rounding of numbers, BigInts and decimal
  strings with every rounding mode, rounding increments and rounding
  priorities; scientific, engineering and compact notation; signDisplay,
  accounting currency, units, useGrouping strategies, the 78 numbering
  systems with simple digits, formatRange and formatRangeToParts, and real
  locale negotiation with Unicode extensions. Locale data covers English
  (with en-IN grouping), German, French, Spanish, Japanese, Korean, Chinese
  and the other common languages as compact tables, and PluralRules knows
  CLDR's cardinal rules for about a hundred languages. test262's NumberFormat,
  PluralRules and toLocaleString tests all pass.
* Intl.ListFormat, Intl.RelativeTimeFormat, Intl.DurationFormat,
  Intl.Segmenter and Intl.DisplayNames follow ECMA-402: they require new,
  read and validate their options in the specified order, negotiate locales
  and numbering systems, brand-check their methods and gain
  supportedLocalesOf. ListFormat formats any iterable of strings in English,
  Spanish and ten more languages; RelativeTimeFormat speaks English and
  Polish, with numeric "auto" phrases; DurationFormat formats every unit
  style, display option, fractional digits and the digital clock style
  through NumberFormat and ListFormat, and accepts Temporal durations and
  ISO 8601 strings; Segmenter yields UTF-16-indexed grapheme, word and
  sentence segments with containing() and an iterator; DisplayNames names
  languages, regions, scripts, currencies, calendars and date fields.
* Intl validates and canonicalizes locales as ECMA-402 and UTS #35 require
  (alias replacement, -u- and -t- extensions, likely subtags), from compact
  CLDR tables that scripts/gen-intl-cldr.py generates; resolvedOptions()
  reports a supported, canonical locale. Intl.Locale gains its full option
  and getter set, maximize/minimize and the locale info methods
  (getCalendars, getHourCycles, getWeekInfo, getTimeZones, ...);
  Intl.Collator sorts with an in-tree multi-level collation instead of the
  C library's locale; toLocaleUpperCase/toLocaleLowerCase apply the
  Turkish, Azeri and Lithuanian rules.
* southstar-jsshell's module loader understands the import text and import
  bytes proposals: with { type: "text" } imports a file as a string and
  with { type: "bytes" } as a Uint8Array over an immutable ArrayBuffer.
  test262 reaches 50,887 of 52,133.
* Error.prototype.stack follows the error-stack accessor proposal: the
  getter throws on a non-object receiver, and the setter accepts only
  strings and, like Iterator.prototype's accessors, refuses to write to
  Error.prototype itself, updates an existing own property through [[Set]]
  and otherwise creates one. test262 reaches 50,876 of 52,133.
* filter() and with() on a BigInt typed array whose buffer the callback or
  an argument's valueOf detached or shrank no longer stop the engine with
  an internal assertion: the TypeError from converting undefined to a
  BigInt reaches the script. No test262 test crashes the engine any more.
* A function from one script now sees the top-level let, const and class
  declarations of scripts that run after it: reading such a name used to
  look only at the window object (a ReferenceError, or a stale global
  property), and assigning to it created a window property instead of
  updating the binding. Assigning to such a const throws as it should.
  test262 reaches 50,858 of 52,133.
* A module that the engine layer evaluates as an entry point is entered
  into the module map under its own path, so a module importing itself (or
  an import cycle leading back to it) links to the same instance instead
  of loading and running a second copy; temporal-dead-zone errors on its
  bindings and evaluation order across cycles follow the standard. test262
  reaches 50,854 of 52,133.
* Strict-mode JavaScript gets proper tail calls (ES2015): a function that
  ends in return f(...), including through ?:, &&, ||, ?? and the comma
  operator, reuses its stack frame for the call, so tail-recursive code
  runs to any depth instead of hitting the recursion limit. Calls inside
  try blocks, for-of loops, generators and async functions keep their
  frames, and an error's stack no longer lists the frames a tail call
  replaced. test262 reaches 50,827 of 52,133 (97.5%).
* Inside a with statement, a call such as f() looks f up once instead of
  twice, and reading a binding first checks that the object still has it,
  so proxies and getters see the operations the standard specifies and a
  binding deleted along the way reads as undefined (or throws in strict
  code). test262 reaches 50,794 of 52,133.
* WeakMap, WeakSet, WeakRef and FinalizationRegistry accept symbols that
  are not in the global symbol registry as keys, targets and unregister
  tokens (symbols as WeakMap keys, ES2023). southstar-jsshell provides
  test262's $262.IsHTMLDDA. test262 reaches 50,789 of 52,133 (97.4%).
* southstar-jsshell installs the browser's own Intl (rust/js-intl), so
  test262's intl402 measures the Intl that pages see: 71.4% (2,416 of
  3,382) instead of the 57.7% that only tested for its absence. The Intl
  hooks on String.prototype.localeCompare, Number.prototype.toLocaleString
  and the Date toLocale*String methods now reject an incompatible this
  value with a TypeError, and localeCompare compares a missing argument as
  "undefined". test262 reaches 50,738 of 52,133 (97.3%).
* southstar-jsshell implements test262's $262.agent: agents run in their own
  threads and engines, share SharedArrayBuffers with the main agent and
  report back, and the shell has a setTimeout. The engine layer gains
  RuntimeOptions.can_block ([[CanBlock]], so Atomics.wait may suspend) and
  wait_for_jobs, which makes run_jobs honour timeout deadlines and wait for
  pending asynchronous jobs such as Atomics.waitAsync; the browser keeps
  neither. Every built-ins/Atomics test passes, and test262 reaches 50,271
  of 52,133 (96.4%).
* Scripts can use explicit resource management: using and await using
  declarations dispose their values when the block, function, module or
  loop iteration ends, however it ends, and DisposableStack,
  AsyncDisposableStack and SuppressedError are available. using remains an
  ordinary identifier unless a binding name follows it on the same line.
  boa_parser and boa_ast are now forked in third_party too, since Boa 0.22
  could not parse these declarations in loop heads. test262 reaches 50,152
  of 52,133 (96.2%).
* ArrayBuffers can be made immutable (the immutable ArrayBuffer proposal):
  transferToImmutable() and sliceToImmutable() create one and the immutable
  getter reports it; writes through typed arrays, DataView and Atomics throw
  or fail, and an immutable buffer cannot be detached, transferred or
  resized. Detaching a buffer that is already detached is no longer an
  error. test262 reaches 49,813 of 52,133.
* Uint8Array converts to and from base64 and hex (ES2026):
  Uint8Array.fromBase64() and fromHex(), toBase64() with the base64url
  alphabet and omitPadding, toHex(), and setFromBase64()/setFromHex() that
  decode into an existing array. test262 reaches 49,754 of 52,133.
* Iterators gain chunks() and windows(), Iterator.zip() and
  Iterator.zipKeyed(), join() and [Symbol.dispose](); take() and drop() reject
  limits past 2^53 - 1 and includes() checks skippedElements as the proposal
  says. test262's built-ins/Iterator passes in full (654 tests), and the
  total rises to 49,694 of 52,133.
* Boa's experimental feature is on, so the script engine exposes the
  standards it already implemented behind it: Array.fromAsync, Error.isError,
  Atomics.pause, ArrayBuffer.prototype.detached/transfer/transferToFixedLength,
  Promise.allKeyed/allSettledKeyed, Iterator.prototype.includes and
  import.defer()/import.source(). test262 rises from 49,225 to 49,516 of
  52,133 tests (94.4% to 95.0%).
* The eleven crates that hold no unsafe code (html5, url, plmpeg, minimp3,
  js-temporal, js-bind, js-realm, webaudio, media-shm, about-style and
  jsshell) carry #![forbid(unsafe_code)], so they stay that way.
* scripts/unsafe-budget.py counts unsafe keywords, FFI imports, no_mangle
  exports and static mut per crate against rust/unsafe-budget.tsv. The Linux
  CI runs it on every push and pull request, publishes the changed crates in
  the job summary and fails when a count rises above its budget, or falls
  without the budget being lowered with it.
* Clippy's undocumented_unsafe_blocks, multiple_unsafe_ops_per_block and
  missing_safety_doc lints are on (8144, 1126 and 76 hits), counted per crate
  in the unsafe budget so each number can only fall; ptr_as_ptr and
  transmute_ptr_to_ptr have no hits and are denied. CI runs the lints through
  scripts/unsafe-budget.py --clippy.
* Integer overflow checks are back on in the dev profile, and the new
  -Doverflow_checks=true meson option turns them on in a release build; the
  Linux CI builds and smoke-tests with it, so an overflow aborts with its
  location instead of wrapping silently.
* docs/unsafe-policy.md sets where unsafe may appear (system-library FFI, the
  embedding API, the JS/GC binding layer, OS primitives, SIMD) and where it
  may not (parsers, engine logic, calls between Southstar's own crates). The
  agent guides describe the all-Rust tree instead of the C one it replaced.
* The PNG/GIF/JPEG/BMP decoders move from rust/image-decoders into
  rust/raster and WebVTT cue parsing from rust/video into rust/webvtt; both
  new crates forbid unsafe code, so the decoders of untrusted image and
  subtitle bytes are checked by the compiler to be memory-safe.
* The HTTP wire formats — HTTP/1.1 heads and chunked bodies, HTTP/2 frames,
  HPACK and the sans-I/O HTTP/2 client connection — move from rust/http into
  rust/http-wire, which forbids unsafe code; rust/http keeps the sockets, TLS,
  content decoding and pooling.
* Seeking in an MPEG-1 stream no longer overflows when the demuxer's byte-rate
  estimate is huge, and no longer computes a negative position in a file
  under 256 bytes; fuzzing the demuxer found both.
* rust/css-syntax has no C ABI left and forbids unsafe code: four of its five
  exports had no caller, the engine calls is_self_contained directly, and CSS
  numbers parse with Rust's float parser instead of GLib's g_ascii_strtod.
* The parsers have unit tests next to their code, run by
  scripts/rust-tests.sh and in CI: the URL parser (normalization, relative
  resolution, hosts, percent-encoding, setters, reparse idempotence), the HTML
  tree builder (implied elements, character references, the adoption agency,
  foster parenting, templates, foreign content, raw text, quirks, fragments),
  HPACK against RFC 7541's examples, chunked bodies split at every byte,
  HTTP/1.1 lines and an HTTP/2 exchange, hand-built BMP and GIF images, and
  WebVTT cues.
* deny.toml states the dependency policy and the Linux CI build checks it
  with cargo-deny on every push: RustSec advisories, yanked crates, licences
  compatible with NSL-1.0 OR GPL-3.0-or-later, crates.io as the only source,
  and a ban on Servo/Firefox engine crates and libcurl.
* The CSP checks parse URLs through rust/url directly instead of a C round
  trip into rust/net's ns_url_* exports, and have unit tests: 'self',
  default-src fallback, wildcards, ports and paths, the http-to-https
  upgrade, policy lists, nonces, hashes, 'unsafe-inline', 'strict-dynamic'
  and frame-ancestors.
* Cookie and HSTS rules move into rust/netstore, which forbids unsafe code and
  does no I/O: the jar's Netscape lines, Set-Cookie parsing with the
  __Secure-/__Host- prefixes and public-suffix check, domain/path/expiry
  matching, the Cookie header, the HSTS host list and Strict-Transport-
  Security parsing, and the HTTP date parser. rust/net keeps the files.
  GLib's g_ascii_strtoll gives way to a Rust strtoll that matched it on two
  million random inputs. Unit tests cover all of it.
* The CSSOM bindings call the style engine's Rust functions directly for
  CSS.supports(), property names and ids, initial values, the viewport,
  direction, container conditions, canonical values and !important,
  instead of round-tripping C strings through rust/css's C exports; seven
  of those exports had no other caller and are gone.
* Layout, paint, media queries, geometry and the window bindings read the
  viewport and container sizes from rust/css directly instead of through its
  ns_css_viewport_* and ns_css_container_* C exports.
* The DOM's attribute and tree edits made by the HTML parser, the shells and
  the JavaScript bindings go through rust/dom's Rust API (attrs::set_attribute,
  attrs::remove_attribute, Node::append, Node::detach) instead of its C
  exports, removing each crate's copy of those extern declarations.
* Form owners, effective disabled state, editable values, integer attribute
  parsing, element-name checks and subtree walks are also called through
  rust/dom's Rust API; six of its C exports were left without callers and
  are gone.
* new Option(text, value, defaultSelected, selected) sets the selected
  attribute for defaultSelected instead of adding a bogus defaultSelected
  attribute that showed up in the option's outerHTML.
* The headless and desktop engines, the SVG renderer and the HTML parser use
  rust/dom's Rust API for editing, form controls, fragment targets, details,
  image maps, text collection and the DOM dump; seventeen more of its C
  exports are gone.
* Paint, layout, hit testing, SVG text and the geometry and CSSOM bindings
  call rust/css's Rust functions for gradient angles and radii, transform
  matrices and their 3D test, the effective transform built from translate,
  rotate, scale and transform, border-image sources and parameters, font
  family, weight and stretch, direction and CSS escapes; eleven C exports
  and paint's GString round trip for unescaping are gone.
* The CSSOM bindings and the animation engine call rust/css's Rust functions
  for inline style text, background-position, grid placement and grid
  shorthand composition, list-style, the translate/rotate/scale properties,
  easing functions, animation-range and time lists, and rust/css-media's for
  media lists, instead of passing C strings through twelve C exports that
  are now gone.
* getComputedStyle reports animation-timing-function and
  transition-timing-function lists item by item when no animation or
  transition runs, instead of splitting at the commas inside
  cubic-bezier(), steps() and linear() and reporting "ease" for every piece.
* Numbers are formatted by rust/numfmt, a new crate without unsafe code
  that reproduces C's %g, %.Pg and %.Pf byte for byte (checked against
  glibc's snprintf on 72 million values), instead of GLib's g_ascii_formatd,
  g_ascii_dtostr and the locale-dependent g_strdup_printf. CSS serialization,
  media queries, canvas colours and fonts, form values, layout text, grid
  inspection and the headless dumps all share it; eight private %g helpers
  are gone.
* <input type=number> and <input type=range> values are the HTML
  Standard's "best representation" of the number, i.e. JavaScript's
  Number::toString (rust/numfmt's js(), checked against V8 on two million
  values): stepUp() on 0.2 with step 0.1 gives "0.3" instead of
  "0.30000000000000004", and valueAsNumber = 1e-7 gives "1e-7", not "1e-07".
  Range values are now sanitized as the standard says -- clamped, then
  rounded to the nearest allowed step within min and max, ties upward --
  so min=0 max=5 defaults to 3, step=0.001 turns 1/3 into 0.333, and
  stepUp() starts from that sanitized value.
* rust/css-media no longer depends on rust/css and has no C ABI or unsafe
  code left (#![forbid(unsafe_code)]). It owns the media state -- the
  viewport, device size and pixel ratio, print mode and the colour-scheme
  and reduced-motion preferences -- and counts its changes; rust/css depends
  on it and drops its cached style sheets when the count moves, instead of
  the two crates calling each other through C. The engines, layout, the
  window and matchMedia bindings and the animation engine call it directly;
  fifteen C exports are gone.
* Thirty crates -- the engines, layout, paint's font loader, the shells, the
  HTTP cache, camera, WebGL, video and the JavaScript bindings for elements,
  frames, loaders, fetch, workers, the window, storage and the URL API --
  resolve URLs and read origins, hosts and URL parts through rust/url's
  Rust API instead of rust/net's ns_url_* C exports, which copied every
  answer into a g_malloc'd C string. The fourteen exports and the C mirror
  of a URL's parts are gone; page dumps and render tests are unchanged.
* <base href> now applies to style sheets and images, not only to what
  script sees: <link rel=stylesheet>, @import and url() in <style> elements,
  <img> loads, img.currentSrc, layout's image lookups and the speculative
  preloader resolve against the document's base URL instead of its own URL.
  A page under /sub/ with <base href="/sub/dir/"> used to request /sub/x.css
  and /sub/i.png, get 404s and render unstyled with a broken image.
* getComputedStyle reports the common shorthands from their computed
  longhands -- flex ("0 1 auto"), flex-flow, place-items/content/self,
  border and its sides and logical pairs, outline, columns, column-rule,
  scroll-margin/padding and the -block/-inline pairs -- instead of
  undefined, an empty string or the authored text. Every shorthand the
  engine supports is now a string property of the computed declaration, and
  border, outline, column-rule, text-decoration and -webkit-text-fill
  colours left at their initial currentcolor report the element's color
  rather than "".
* Authored time lists (transition-duration/-delay, animation-duration/
  -delay) and font-feature-settings/font-variation-settings serialize with
  ", " between items in style rules and element.style, as other browsers
  do: "500ms,1.5s" now reads back as "500ms, 1.5s".
* Web Animations validate and canonicalize easing: an effect's and a
  keyframe's easing go through the CSS easing parser, so getTiming().easing
  reads "cubic-bezier(0.25, 0.1, 0.25, 1)" or "steps(1, start)" instead of
  the string passed in, and an invalid easing ("bogus", a cubic-bezier x
  outside [0, 1], a list) throws a TypeError as the standard requires
  instead of being accepted.
* rust/mat4, rust/datetime and rust/helper-ffi lose their C exports, none of
  which had a caller, and now forbid unsafe code. The shell and the browser
  call rust/i18n and rust/bytecode-cache directly instead of through
  ns_i18n*/ns_bytecode_cache_* exports; the shell's tr() no longer hands a
  caller's string back as &'static when it has no translation.
* 258 C exports that nothing referenced -- no import in any crate, no
  mention in meson or the embedding header -- are deleted across 40 crates
  (DOM indexes and attributes, the style engine's string helpers, net,
  canvas, the shell's procview helpers, IPC framing, the HTTP cache and
  the JS bindings' old entry points), with the helpers, imports and five
  css ffi modules only they used: 2,650 lines, 704 unsafe keywords.
* libsouthstar.so carries the engine again. It was linked from the engine's
  C sources, and once those were all ported to Rust nothing pulled the Rust
  static library in, so the installed embedding library exported nothing.
  It now links the whole library and exports exactly the 80 functions
  src/libsouthstar.h declares, through a version script that
  scripts/gen-libsouthstar-map.py generates from the header; a C program
  built against it opens a page and renders its text.
* The renderer's framebuffer is a FrameMemory that owns its pixels: its own
  buffer, the shared mapping the shell sent (unmapped when the session
  ends) or, in single-process mode, a reference to the shell's buffer
  (released when the session ends). Its constructor for shared memory is the
  one unsafe entry point and states its contract; new_session, which turned
  any raw pointer into a slice from safe code, now takes a FrameMemory and
  refuses one smaller than max_w x max_h x 4 bytes.
* HSTS entries expire: a host whose Strict-Transport-Security max-age has
  run out is no longer upgraded to https://, an expired line is dropped the
  next time the list is rewritten, and a max-age past the year 9999 is
  stored as the last representable date instead of an unreadable one.
* cargo vet records the supply-chain review of every crates.io dependency in
  supply-chain/: audits imported from the Bytecode Alliance, Google, ISRG,
  Mozilla and Zcash cover 31 crates and the other 163 are exempted at the
  versions in Cargo.lock, so any new crate or version bump needs an audit or
  a deliberate exemption. The in-tree Boa fork is first-party code and is not
  vetted as a crates.io crate. The Linux CI runs cargo vet --locked.
* docs/boa-fork.md records what the in-tree Boa fork changes against the
  boa_engine 0.22.0 release it copies -- 783 added lines in 17 files: each
  embedder hook, what the bindings use it for and whether it can go
  upstream -- and how to take a new Boa release or security fix.
  scripts/boa-fork-diff.py downloads the release, checks it against the
  crates.io checksum and prints the fork's patch.
* The Linux sandbox lets the browser read the CA bundle named by
  CURL_CA_BUNDLE or SSL_CERT_FILE and the directory named by SSL_CERT_DIR.
  A bundle outside /etc and /usr could not be opened once Landlock was on
  (the renderer inherits the shell's restrictions), so every HTTPS page
  failed with an untrusted-issuer error.
* TLS is rustls (0.23.46 or newer) over ring instead of OpenSSL's libssl, so every handshake,
  record and server certificate the browser receives is parsed and verified
  by memory-safe Rust. Key exchange still prefers the post-quantum hybrid
  X25519MLKEM768 -- now on every platform, not only with OpenSSL 3.5 or
  newer -- built from RustCrypto's ml-kem and ring's X25519 and offered
  alongside a plain X25519 share, so servers without it need no extra round
  trip. Certificate errors read "certificate has expired", "certificate is
  not valid for this host name" or "certificate issuer is unknown or
  self-signed"; TLS 1.3 and 1.2 sessions resume; ALPN, the insecure override,
  the CA bundle and the Windows root store work as before, and the trust
  roots are parsed once when networking starts. TLS 1.2 is limited
  to ECDHE with AES-GCM or ChaCha20, so a server that offers only CBC or RSA
  key exchange no longer connects. libssl is no longer linked; libcrypto
  remains for crypto.subtle. about:diagnostics lists TLS and Web
  Cryptography separately.
* The page object's text slots (base URL, document charset and language,
  pending navigation, download, clipboard text and window action, refresh
  URL, POST content type, find-in-page query, server address) hold
  reference-counted owned strings instead of raw C pointers. Reading one no
  longer returns a borrow that the next assignment frees: a script calling
  history.pushState while the page's scripts ran with the old base URL could
  have left that call reading freed memory, and the style, image and paint
  calls now keep the string alive for their whole duration. The POST body,
  stored as a C string beside a separate length, is a byte vector. The page
  object is no longer created by zero-filling a struct that holds them.
* A form submitted with method=post while a page is still loading -- by a
  script calling form.submit() or requestSubmit() during the first second
  -- is posted with its body. The renderer took the navigation from the
  page-open reply without keeping the POST body that went with it, so the
  shell loaded the action URL as a plain GET.
* The Linux CI runs the 19-area web-platform-tests slice tracked in
  docs/wpt-scores.md on every push, against a pinned WPT revision, and
  publishes the files-passing and subtests-passing counts per area in the
  job summary, with the per-file results as a download. scripts/wpt-run.sh
  takes --jobs=N to run tests in parallel (the slice takes about nine
  minutes with four instead of about half an hour one at a time), and
  scripts/wpt-score.sh runs the whole slice with one server and writes a
  Markdown summary with --summary=FILE.
* The renderer host drives pages through a Rust API instead of the C one:
  rust/browser's api::Page owns a page and has methods for opening,
  painting tiles and layers, input, find, printing and the pending
  navigations, downloads and permission requests, taking and returning
  Rust strings, slices and tuples. The 78 hand-copied ns_browser_*
  declarations in renderer-host, and the C structs it re-declared to talk
  to them, are gone, so the compiler checks every call; the tile and layer
  painters check their buffer sizes before drawing. The ns_browser_* exports
  in src/libsouthstar.h are unchanged for C embedders and now forward to the
  same internals. renderer-host goes from 160 unsafe blocks to 85 and from
  139 FFI declarations to 55.
* Successive document.write calls from one script appear in the order they
  were written. Each write was inserted right after the script element, so
  a loop writing paragraphs 0 to 59 showed them as 59 down to 0; a write
  now goes after what the same script wrote before it.
* CSS linear() easing works with its stops. linear(0, 0.25 75%, 1) and the
  other forms of CSS Easing 2 -- outputs outside 0..1, one or two input
  percentages per stop, calc() percentages, inputs filled in evenly and
  kept in order -- drive CSS animations, transitions and Web Animations
  (they all ran as plain linear before), extrapolate past the first and last
  stops, and serialize as written in the style rule and fully expanded in
  getComputedStyle ("linear(0 0%, 0.25 75%, 1 100%)"). Malformed linear()
  values such as linear(0) or linear(0 calc(50px - 50%)) are now rejected.
* The DOM's nodes are Rust values instead of C structs. Each thread keeps a
  store of node slots that are never handed back to the allocator, so the
  node handle is a plain reference and its tree links are Cells: walking
  and relinking the tree is safe code. A slot carries a generation, and a
  WeakNode -- what the stylesheet matcher now keeps for the hover, active,
  focus and fullscreen elements, and the DOM for the active modal dialog --
  returns nothing once its node is freed, even after the slot is reused, so
  nothing has to be told when a node goes away. Names, text, attributes,
  the id/class/tag indexes and the class tokens are owned by the node (Box,
  Vec, HashMap) instead of GLib allocations that were either owned or
  borrowed from a per-document string arena, so a node moved into another
  document can no longer point into the one it left; the arena,
  own_strings_deep, the borrowed-string setters and the GLib hash tables are
  gone. Attributes are slots in a store of their own, so an Attr handle is a
  plain reference and document.createAttribute no longer builds a fake C
  attribute struct. The HTML parser's tree sink builds nodes through the
  Rust API; the ~220 most common element and attribute names and short
  values and text are stored without an allocation, which keeps parsing
  within a few percent of the arena it replaces. Freeing a subtree unlinks
  it from its parent first. docs/dom.md describes the model; rust/dom goes
  from 212 unsafe blocks to about 100 (none of them undocumented) and from 3
  FFI imports to none, and html-parser from 16 FFI imports to none.
* A parentless DOM subtree has one owner, a Tree, which frees it when
  dropped unless it was given a parent first. The HTML parser returns its
  documents and fragments as Trees through a Rust API, so innerHTML,
  outerHTML, insertAdjacentHTML, the positional HTML methods,
  document.write, document.open, createContextualFragment, DOMParser,
  frame loading, the SVG renderer, the headless engine and the page
  document stop calling ns_html_parse and ns_node_free by hand; taking a
  template's contents and cloning a node hand back a Tree too.
* A script's element wrappers hold their node as a WeakNode in typed
  instance data instead of a raw pointer with a raw finalizer, so a wrapper
  whose node was freed unwraps to null instead of reaching whatever reused
  the memory, and creating one needs no unsafe code.
* The XML parser builds DOM nodes through the Rust API and returns an owned
  Tree (southstar_xml::parse_document) instead of void pointers through
  ns_node_* exports, so rust/xml has no unsafe code left and carries
  #![forbid(unsafe_code)]; DOMParser and XML frame documents call it
  directly. Frames, form controls, offscreen canvases and the script
  runtime create nodes through the Rust API as well, and the ns_node_new_*,
  ns_node_set_name_owned, ns_node_add_flags, ns_node_mark_doctype and
  ns_node_append_child exports are gone.
* Laying out a text run whose collapsed start lies past its end no longer
  aborts on an integer underflow (the length was computed before the check
  that guarded it).

* Element.setHTML() and the HTML Standard's positional parsing methods —
  appendHTML/prependHTML on elements and shadow roots, beforeHTML/afterHTML/
  replaceWithHTML on elements and character data, each with an Unsafe
  variant — parse in the right context, attach declarative shadow roots,
  honour runScripts and a sanitizer option, and a Sanitizer class holds its
  configuration. WPT domparsing/positional-methods.html 7 -> 480/480.
* blob: URLs take the origin of the URL they wrap, so a page can reach into
  an iframe it navigated to one of its own blobs (html5lib_url WPT tests:
  +2144 subtests).
* insertRule and other CSSOM rule mutations no longer re-serialize the whole
  sheet each time: CSS-in-JS sites that insert thousands of rules (bbc.com)
  finish hydrating instead of hitting the 60 s script limit.
* A <br> breaks the line whatever its display (rust-lang.org's tagline), and
  spaces around a forced break no longer indent the next line.
* ::before/::after text in inline elements takes its own colour,
  visibility, size, weight, style and font family (MDN's hidden heading
  anchors, icon fonts), and positioned ones leave the flow.
* An absolutely positioned box inside an inline positioned element (a
  relative link or span) is placed against that element, not the page.
* Floats start below the previous block's bottom margin (archlinux.org's
  news list), fit when percentage widths overrun the line by under 1/64 px
  (python.org's four-column rows), and paint above in-flow backgrounds.
* Auto margins centre images and other replaced flex items (go.dev's logo
  grid).
* hwb() and hsl() channels that land on an exact half round up, as in other
  engines.

* font-variant takes the full CSS Fonts 4 grammar, and font-variant-caps,
  -numeric, -east-asian, -position, -emoji, font-synthesis,
  font-size-adjust, font-optical-sizing, font-language-override, font-width,
  hanging-punctuation and text-autospace parse and serialize; computed
  font-stretch is a percentage. Small caps are synthesized when the font has
  no small-cap glyphs.
* white-space accepts the CSS Text 4 collapse/preserve/preserve-breaks and
  wrap/nowrap values.
* clip-path values are validated, and circle(), ellipse(), inset(),
  polygon(), xywh() and rect() are drawn against the border box with their
  full arguments (circle(50%) avatars on padded boxes were too small).
* The ::view-transition pseudo-elements parse by their grammar (WPT
  css-view-transitions/parsing 0 -> 1055/1055); they match nothing yet.
* docs/wpt-scores.md has a 2026-10-14 compliance slice and a refreshed
  top-10 list; CSS Color 4/5 colour spaces lead it.

Southstar Browser 0.9.10:
======
* <script type="module"> works on Boa: modules get import.meta (url, main,
  resolve()), share one instance per URL and realm, and support cycles,
  dynamic import() and top-level await.
* Worker and MessagePort messages, structuredClone, window.postMessage and
  IndexedDB values serialize on Boa (objects, arrays, Map/Set, Date, RegExp,
  typed arrays, errors, shared references and cycles); handlers such as
  IDBRequest.onsuccess and MessagePort.onmessage work again.
* WebAssembly memories expose their ArrayBuffer on Boa, Float16Array is
  available, and Date.parse accepts the common non-ISO formats (toString
  output, "Jan 2, 2024", "01/02/2024", "2024-01-02 03:04:05").
* A frame document rewritten with document.open()/write()/close() stays:
  the frame no longer re-navigates to its src attribute afterwards (the
  written document replaced the src navigation, as the HTML spec says), and
  setting src again still navigates. Acid3: 96/100; the four remaining
  failures assert pre-2025 name validation and pre-Selectors-4 :first-child
  behaviour that current specs changed.
* DOMException carries the legacy code constants (INDEX_SIZE_ERR …
  DATA_CLONE_ERR) on the interface and its prototype, and live Ranges follow
  node removal and insertion again: the polyfill now wraps every prototype
  that owns appendChild/insertBefore/removeChild/replaceChild, not only the
  nearest one, which the interface shapes later deleted. Acid3: 95/100.
* Engine-created events (submit, click, input, …) keep their type,
  cancelable flag and hidden state on Boa again: the Boa backend now answers
  is_host_access/is_host_caller from the browser's native-call stack and
  Boa's call frames (the fork exposes frame_depth and frame_source_path), as
  QuickJS did. preventDefault() in onsubmit stops the submission, and Acid3
  runs to completion (93/100) instead of navigating away at test 54.
* MP3 decoding in the southstar-audio helper uses rust/minimp3, a pure-Rust
  port of minimp3 with bit-identical output; the vendored C minimp3 and its
  libminimp3 build target are removed. The tree has no C code of its own left.
* The 75 C headers that described the interfaces between modules are deleted;
  src/libsouthstar.h, the public embedding API, is the one left. The about-page
  logo, splash and stylesheet are embedded from plain .gif/.css files, and the
  dead ns_pango_names.h and its generator are gone.
* MPEG-1 playback is pure Rust: the vendored C pl_mpeg decoder
  (subprojects/plmpeg) is replaced by rust/plmpeg, an in-tree port with no
  dependencies covering the MPEG-PS demuxer, MPEG-1 video, MP2 audio, seeking
  and looping. A differential harness found frames and samples byte-identical
  to the C decoder.
* Boa is Southstar's JavaScript engine and QuickJS-ng is gone: src/quickjs
  (about 101,000 lines of C), rust/js-engine's QuickJS backend, the
  quickjs/boa Cargo features of the binding crates, the polyfill verifier
  that ran on qjs, scripts/test262-run.sh and scripts/js-engine-compare.py are
  removed. about:southstar reports boa_engine.
* The macOS Dock icon setter (macos_dock.m) is Rust, in rust/shell over the
  Objective-C runtime; the tree has no Objective-C left and meson no longer
  enables the objc language.
* The C-ABI QuickJS shims (legacy.rs) are gone from the binding crates:
  their last callers had moved to the Rust-ABI nsr_ entry points. The window
  and worker realms bind their illegal constructors through the neutral API.

Southstar Browser 0.9.9:
======
* rust/js-interfaces no longer touches QuickJS: the DOM prototype tables
  (element, document, media, anchor and the rest, about 700 entries) are
  engine-neutral Prop tables that call the binding crates' Rust natives
  directly, and the classes, members and shapes install through the neutral
  Scope API. Scope::set_is_html_dda (document.all) is new, a no-op on Boa.
* The networking bindings (rust/js-net: fetch, Request, Response,
  XMLHttpRequest, AbortController, WebSocket, EventSource) run on the
  engine-neutral JavaScript API, with no raw QuickJS code left in the crate.
* HTML is parsed by Southstar's own HTML5 parser, rust/html5, instead of
  the vendored lexbor C library, which is removed from the tree (~68,000
  lines). The tokenizer and tree builder are written from the WHATWG HTML
  Standard in safe Rust with no dependencies: every tokenizer state, named
  character references generated from WHATWG's entities.json
  (scripts/gen-html-entities.py), every insertion mode with the adoption
  agency, foster parenting, templates, foreign content, quirks, fragment
  parsing, the relaxed <select> parsing and processing instructions. The
  engine DOM is now built directly, without a second tree and copy pass, and
  inline script source positions come from the tokenizer (they were wrong
  after a "<script" inside a comment or script). The parsed trees are
  identical on html5lib's tree-construction tests, the render tests and 30
  live sites, and the render-test images are pixel-identical.
* The engine-neutral JavaScript layer (rust/js-engine) gained what the
  bindings still took from QuickJS directly, each with a QuickJS and a Boa
  implementation: context and runtime handles that replace raw JSContext
  pointers in records and cross-crate calls and re-enter a realm from
  anywhere, runtime creation with interrupt, rejection-tracker and module
  hooks, one-at-a-time job execution, declarative property tables (Prop)
  in place of QuickJS function lists, object identities and pins, brands,
  bytecode and module records. rust/js-bind, rust/js-realm, rust/js-intl
  and rust/js-temporal are fully on the neutral API, the page/worker realm
  record in rust/js-runtime runs on it (with a typed API other crates call
  directly), DOMTokenList's prototype comes from the first Prop table, and
  docs/rust-port.md has a conversion guide for the remaining crates.
* The forms bindings (rust/js-forms), the iframe browsing contexts
  (rust/js-frames) and the messaging and worker bindings (rust/js-workers)
  are off raw QuickJS: every native has a Rust-ABI nsr_ entry point with
  Prop-table signatures, the engine and DOM calls they make go through nsr_
  twins, and the C-ABI shims the unconverted crates still bind sit in a
  quickjs-only legacy.rs. The frame-realm cloner runs on new neutral calls
  (full property descriptors, private keys, native-function clones,
  forwarders, registered host classes, plain-object tests) with QuickJS and
  Boa implementations. js-forms and js-frames build on Boa; js-workers
  waits only for rust/js-clone.
* The forms bindings (rust/js-forms) and the messaging and worker bindings
  (rust/js-workers) are off raw QuickJS: every native has a Rust-ABI nsr_
  entry point with Prop-table signatures, the engine and DOM calls they
  make go through nsr_ twins, and the C-ABI shims the unconverted crates
  still bind sit in a quickjs-only legacy.rs. Both build on Boa.
* The forms bindings (rust/js-forms) are off raw QuickJS: every native has
  a Rust-ABI nsr_ entry point with Prop-table signatures, the engine and
  DOM calls they make go through nsr_ twins, the crate builds on Boa, and
  the C-ABI shims the unconverted crates still bind sit in a quickjs-only
  legacy.rs.
* EventTarget and event dispatch (rust/js-dispatch) run on the neutral
  JavaScript API: realms are context handles, the event, console, frame and
  worker calls go through Rust-ABI entry points, the window's and worker's
  listener and dispatch methods are bound from them, and the C shims the
  unconverted crates still call are kept in a QuickJS-only legacy module.
* The window bindings (rust/js-window: History, the Navigation API,
  Location, window.open, postMessage, named access) are off raw QuickJS:
  realms are context handles, the page realm binds them through Rust-ABI
  nsr_ entry points, and the C shims raw callers still use are confined to
  a QuickJS-only legacy.rs.
* The event core (rust/js-events), the reflected HTML attributes
  (rust/js-reflect), rust/js-text, the element bindings (rust/js-elements)
  and the top layer (rust/js-top-layer) no longer use raw QuickJS: they export
  Rust-ABI entry points over the neutral API, keep their C symbols only for
  the callers still on QuickJS, are bound by the realm setup directly, and
  build on Boa.
* rust/js-attrs, rust/js-collections, rust/js-node, rust/js-dom-factory
  and rust/js-document run on the neutral JavaScript API: their natives are
  exported as Rust-ABI nsr_ entry points, they reach the realm record
  through rust/js-runtime's typed API, the window binds the node
  constructors, DOMParser, CloseWatcher and Document through them, and their
  old QuickJS C exports are kept only in a legacy.rs behind the quickjs
  feature until the prototype tables call the nsr_ functions.
* The canvas 2D API, WebGL and WebGPU (rust/canvas, rust/webgl,
  rust/webgpu) no longer touch QuickJS: they call each other through Cargo
  dependencies and Rust-ABI entry points instead of C shims, the 2D
  context's drawing state holds an engine-neutral object identity, and all
  three build against Boa (once rust/js-runtime does).
* The window services (rust/js-services: timers, navigator, console,
  screen, matchMedia, camera, ClipboardItem, the WebRTC stubs) and page
  installation (rust/js-page) no longer use raw QuickJS and build on Boa:
  they export Rust-ABI entry points over the neutral API and call the other
  binding crates through nsr_ twins, the services keep their C symbols only
  for callers still on QuickJS, and the realm setup binds them directly.
* Structured cloning (rust/js-clone) and script and module loading
  (rust/js-loader) are off raw QuickJS and build on Boa: module fetch errors
  are thrown through the neutral layer (which gained Scope::reference_error),
  the bytecode cache and import.meta use the neutral bytecode and module
  calls, and the loader's dead C-ABI module hooks are gone.
* Twenty-one more binding crates run on the neutral JavaScript API only:
  HTML elements, geometry, custom elements, traversal and ranges, media,
  CSSOM, URL and encoding, lifecycle and animation frames, focus,
  performance, observers, shadow DOM, storage, input, fonts, mutation,
  brands, WebAssembly, compression, Web Crypto, IndexedDB and WebExtension
  content scripts. Their natives are Rust-ABI functions the window and
  worker realms bind directly; the remaining C-ABI QuickJS shims for
  unconverted callers sit in quickjs-only legacy.rs files. The neutral API
  gained Scope::internal_error.
* The Boa fork gained the embedder hooks host classes need: host exotic
  internal methods (named and indexed access on Storage, DOMTokenList, form
  controls and the other hooked classes now work on Boa), an interrupt
  check that stops runaway scripts with an uncatchable error, and object
  identities for weak node-wrapper pointers. On Boa, raw-instance
  finalizers now run and class prototypes are kept per realm.
* The audio helper southstar-audio is now Rust (rust/audio-helper): the
  same decoders (pl_mpeg, minimp3, libav, vorbisfile, opusfile), mixing and
  resampling to 44.1 kHz stereo through SDL2 or the silent fallback, the
  shared-memory clock (rust/media-shm), downloads and the Linux sandbox, with
  an unchanged line protocol. With it the browser has no project C left
  except the macOS Dock menu and a four-line minimp3 wrapper.
* The MSE video helper southstar-video is now Rust (rust/video-helper),
  with FFmpeg through rust/av; its line protocol, shared-memory frame ring,
  clock and decoded frames are unchanged (byte-identical on a VP9 test
  stream), and the shared-memory layouts live in one crate, rust/media-shm,
  used by the helper and the shell alike.
* Inline video decoding (video_decode.c) moved to Rust (rust/video):
  MPEG-1 through the vendored pl_mpeg, WebM through FFmpeg over rust/av's
  meson-computed layouts. The engine library now contains no C of its own.
* AVIF decoding (image_avif.c) moved to Rust (rust/image-decoders, behind
  the avif feature): libavif is called through hand-written declarations
  whose struct offsets meson computes from the installed avif.h, so any
  libavif 1.x works; limits and output (premultiplied BGRA) are unchanged.
* rust/av declares the FFmpeg entry points the media code calls, with the
  struct offsets and enum values meson computes from the installed FFmpeg
  (and libavif) headers, so the media helpers, video decoding and AVIF can
  move to Rust without bindgen, -sys crates or one FFmpeg version's layout.
* docs/rust-port.md reviewed against the tree: the status, phase table,
  dependency map and decisions now record what was actually built (no
  bindgen or gtk-rs, rust/http instead of libcurl, js.c done) and what is
  left (the media C, macos_dock.m, the C headers, Boa, lexbor's HTML parser).
* boa_engine 0.22 is vendored in third_party/boa_engine (patched in through
  Cargo's [patch.crates-io]) as the base of a Southstar fork of Boa: the
  browser needs embedder hooks stock Boa keeps private (exotic host objects,
  interrupts for runaway scripts, raw object handles) before it can run on
  Boa and QuickJS can be removed. The copy is unmodified so far.
* js.c, the last C of the JavaScript bindings, moved to Rust: the page's
  record (realms, DOM prototypes, storage partition, cookies and CSP) now
  lives in rust/js-runtime, the image and video readiness helpers in
  rust/js-images, rust/image and rust/video, and the polyfills are embedded
  by the Rust build. No project C touches the QuickJS API any more, the
  first step of moving the browser onto Boa and removing QuickJS.
* URLs are parsed by Southstar's own WHATWG URL parser, rust/url, instead
  of lexbor's: the host parser with UTS #46 IDNA over Unicode 18 tables
  (generated by scripts/gen-url-tables.py), Punycode, NFC, the serializer
  and the URL setters, in safe Rust with no dependencies. It follows the
  current URL Standard (opaque-path spaces, ASCII hosts, lone surrogates,
  the host and port setters), raising the WPT url area from 8502 to 8646 of
  8679 subtests, and lexbor's URL, Unicode, Punycode and encoding modules
  (~220,000 lines) are gone.
* The tab view (procview.c) moved to Rust (rust/shell): the renderer process
  and worker thread, page drawing and tiles, scrolling, input forwarding,
  the audio and video helpers, loading and history, zoom, the context menu,
  the find and permission bars, printing, saving and developer tools. The
  shell no longer compiles any C of its own.
* The Windows launcher (southstar-launcher.exe: the console handoff,
  starting app\southstar-ui.exe, and the extract-the-ZIP and missing-runtime
  messages) moved to Rust (rust/launcher), and the embedding shim's
  ns_app_self_exe stub moved into rust/static.
* The Element wrapper class and the generic host-object class moved onto the
  engine-neutral host classes, and js.c's DOM function tables and class
  installers moved to rust/js-interfaces; js.c is down to about 1,200 lines.
* webgpu.c moved to Rust (rust/webgpu): navigator.gpu over the external
  wgpu-native, with hand-declared, layout-checked FFI and no wgpu crates. It
  is still built only when wgpu-native is present and still gated by
  --enable-webgpu at runtime.
* The rest of layout.c (block, pseudo-element, legend, image and media box
  builders, shadow roots and slots, form-control blocks, the abspos queue
  and ns_layout_build) moved to Rust (rust/layout-build), and layout.c is
  gone: layout is entirely Rust, with identical layout.
* The GTK browser window (tabs, toolbar, address bar, menus, the bookmarks
  popover, the downloads window, the task manager and session recovery)
  moved to Rust (rust/shell). Meson now links the non-shell targets through
  a new rust/static static library. The downloads window lists recent files
  sorted by name.
* The vendored Wuffs image decoders (~97k lines of C) are replaced with
  pure-Rust ones: the png crate with APNG, gif, zune-jpeg and an in-tree BMP
  decoder in rust/image-decoders. PNG, GIF and BMP pixels are unchanged;
  JPEG differs by at most a few levels of IDCT rounding.
* Inline-run construction (the inline collector, white-space collapsing,
  form-control and pseudo-element text, first-letter and decoration styling,
  and atomic-inline detection) moved from layout.c to Rust (rust/layout-
  build), with identical layout.
* The dataset, live collection (HTMLCollection, NodeList, NamedNodeMap),
  Attr and Event classes moved off js.c onto the engine-neutral host classes
  (rust/js-attrs, rust/js-collections, rust/js-events), which gained a host-
  access check and a forwarded ordinary [[Set]].
* The GTK shell's title row, icons, page layers and entry point moved to
  Rust (rust/shell, over a new hand-written GTK FFI crate rust/gtk-ffi),
  starting phase 4. The shell executable links its own Rust static library,
  southstar-shell, so the renderer and libsouthstar stay free of GTK.
* The page's realm scopes, eval budget, microtask and message-task
  scheduling, pumped fetches and runtime state accessors moved from js.c to
  Rust (rust/js-runtime); about 60 fields left struct ns_js for Rust-owned
  page state.
* The JavaScript engine layer gained engine-neutral host classes with exotic
  property hooks; the Storage, DOMTokenList, CSSStyleDeclaration and
  WindowProperties classes moved off js.c onto them (rust/js-storage,
  rust/js-attrs, rust/js-cssom, rust/js-window).
* The box pool and box freeing, inline-run merging and splitting, order
  reordering and table box construction moved from layout.c to Rust
  (rust/layout-tree), with single-inline multicol fragmenting in
  rust/layout-inline, with identical layout.
* The remaining small DOM and window bindings (nodeValue, data and length,
  token lists and attributes, element.style, SVG lengths, DOMRect,
  checkVisibility and the box lookup, the DOMException helpers, window.find,
  hashchange and window focus events, pointer capture, showPicker,
  caches.open, the document base URL and defaultView) moved from js.c to
  Rust.
* Realm creation and teardown (ns_js_new, the worker realm and its script
  evaluation, ns_js_free and the interrupt handler) moved from js.c to Rust
  (rust/js-runtime).
* Script and module evaluation (classic scripts, modules and their loader,
  JSON modules, frame-realm scripts, content scripts and the console eval)
  moved from js.c to Rust (rust/js-loader), over new engine-neutral compile
  and evaluate calls in rust/js-engine.
* Generated content and form-control text (CSS counters, ::before and
  ::after content and quotes, text-transform, control sizing and inline
  attributes, select and datalist rows and text input columns) moved from
  layout.c to Rust (rust/layout-content), with identical layout.
* Block layout (the per-box dispatch, block widths and auto margins, margin
  collapsing, float placement and clearance, multicol columns, heights,
  fieldset legends and legacy alignment) moved from layout.c to Rust
  (rust/layout-block), with identical layout.
* The leftover element and window bindings (the window Event constructor,
  form.elements with RadioNodeList, input.files, click activation behaviour,
  Element.animate(), canvas.toBlob(), replaceWholeText(), structuredClone(),
  the <body> window handlers, the window's compatibility members and
  IdleDeadline) moved from js.c to Rust.
* Iframe document loading and the frame script runner (the fetch with CSP
  and framing checks, document building, frame realm scripts, frame globals,
  the referrer and the cross-origin check) moved from js.c to Rust (rust/js-
  frames).
* Installing a parsed document into the page's realm and starting its
  scripts (the runtime reset, the document object, the window's interfaces
  and globals, early and content scripts, import maps and the lifecycle
  start) and the JavaScript stats dump moved from js.c to Rust (rust/js-
  page).
* Floats and block-formatting-context helpers (rust/layout-float) and
  replaced-element and intrinsic sizing (rust/layout-sizing) moved from
  layout.c to Rust, with identical layout.
* The page navigation, scroll and timing glue (fragment navigation, viewport
  scroll events, resize, pageshow and pagehide, resource origin checks,
  navigation timing milestones and render-blocking status) moved from js.c
  to Rust (rust/js-window, rust/js-perf, rust/js-loader).
* DOM change recording (child list, attribute and character data records
  with the id, class and tag index and the form past-names upkeep, the
  recorded attribute setters, the orphan set, the pending change event and
  the per-element NamedNodeMap cache) moved from js.c to Rust (rust/js-
  mutation).
* The host-object interfaces (MessageChannel, BroadcastChannel, XHR,
  AbortController, FileReader, TextEncoder and TextDecoder, FormData
  construction and accessors) and js.c's generic binding helpers moved to
  Rust (rust/js-bind).
* The window global's shape (the immutable global prototypes, the
  WindowProperties chain, the navigator and web API singleton shapes and
  namespace objects) moved from js.c to Rust (rust/js-interfaces).
* The DOM interface installers (the prototype hierarchy, instanceof and
  Symbol.hasInstance, toStringTag, unscopables, node and element member
  shapes, the PDF viewer plugins, interface object links and window.chrome)
  moved from js.c to Rust (rust/js-interfaces).
* The frame owner element bindings (contentDocument, contentWindow,
  getSVGDocument, child frame windows) and iframe source marking and load
  scheduling (lazy frames, static frame scheduling, download detection)
  moved from js.c to Rust (rust/js-frames).
* The window constructors (Text, Comment, DocumentFragment, Image, Audio,
  Option, DOMParser, CloseWatcher), the getUserMedia camera helpers and the
  blob: URL registry with FileReader completion moved from js.c to Rust
  (rust/js-dom-factory, rust/js-services, rust/js-url).
* The script bytecode cache glue, module specifier normalization, import
  attributes, import.meta and the NS_PROFILE script profiler moved from js.c
  to Rust (rust/js-loader), and native member naming to rust/js-brand.
* requestAnimationFrame, cancelAnimationFrame and requestVideoFrameCallback
  with the animation-frame tick, scrollend, the animation, transition and
  media load events and the page lifecycle tick moved from js.c to Rust
  (rust/js-lifecycle).
* Inline video playback (video.c) moved to Rust (rust/video): the video
  cache, playback off the animation tick, autoplay, loop, mute and posters,
  click-to-play, WebVTT cues, MSE streams and the audio and video helper
  commands. Media events now reach page script after the call that raised
  them returns.
* The element-specific HTML bindings (the table API, <img> complete, natural
  size and currentSrc, labels, form, list, option index, select length,
  hidden, disabled, isContentEditable and enctype) moved from js.c to Rust
  (rust/js-elements), and canvas.toDataURL() to rust/canvas.
* document.elementFromPoint and elementsFromPoint, image readiness with
  <img> load and error events, execCommand and the queryCommand methods,
  confirm and prompt moved from js.c to Rust (rust/js-geometry, the new
  rust/js-images, rust/js-services). Images that become ready together now
  fire their events in load-start order.
* CompressionStream and DecompressionStream's zlib codec moved from js.c to
  Rust (rust/js-compression); the performance object and its prototype moved
  to rust/js-perf, the StaticRange constructor to rust/js-traversal and the
  SharedArrayBuffer hiding to rust/js-realm.
* The box-tree queries (find-in-page match counting and navigation, CSS
  scroll snapping for scrollers and the viewport, paint bounds and subtree
  extents, inline element rectangles, image, video and frame viewport
  collection) moved from layout.c to Rust (rust/layout-query), with
  identical results.
* The FreeBSD, NetBSD and musl (Alpine) CI builds install the pinned Rust
  1.99.0 with rustup, since their packaged Rust (FreeBSD 1.98, Alpine 1.96)
  is older than the toolchain the build requires.
* Inline layout (line boxes, line-height struts, inline-block vertical
  alignment, text-align and text-indent, vertical text, the text measurement
  cache and inline min/max-content widths) moved from layout.c to Rust
  (rust/layout-inline), with identical layout.
* The Node tree getters (firstChild, nextSibling, parentNode and the rest),
  compareDocumentPosition, isEqualNode, contains, nodeName, tagName and the
  other names, and getAttribute, setAttribute and the other attribute
  methods moved from js.c to Rust (rust/js-node). The hot getters are as
  fast or faster than before.
* The input events the shell sends into the page (mouse and pointer events
  with iframe coordinate mapping, wheel, touch, clipboard and drag events
  with their drag sessions) and the WPT test-driver input hooks moved from
  js.c to Rust (rust/js-input), with unchanged results.
* Positioned layout (relative, absolute, fixed and sticky offsets) moved
  from layout.c to Rust (rust/layout-position), with identical layout.
* Hit testing (the point-to-element, form-control, link and scroll-container
  lookups behind clicks and elementFromPoint) moved from layout.c to Rust
  (rust/layout-hit), with identical results.
* document.open, write, writeln and close, the script-inserted parser write
  path and the documents of frames, DOMParser and DOMImplementation moved
  from js.c to Rust (rust/js-document), with unchanged results.
* Grid layout (track sizing, placement, alignment, subgrid and grid-
  positioned absolute boxes) moved from layout.c to Rust (rust/layout-grid),
  with identical layout.
* EventTarget and event dispatch (addEventListener, removeEventListener,
  dispatchEvent, the dispatch path, inline and IDL event handlers, error
  reporting and unhandled rejections) moved from js.c to Rust (rust/js-
  dispatch). unhandledrejection events now fire in the order the promises
  were rejected, and a nested dispatch no longer clears the outer event's
  composedPath().
* Text tracks (VTTCue, textTracks, addTextTrack), CSS Font Loading
  (FontFace, document.fonts) and ElementInternals (attachInternals,
  CustomStateSet) moved from js.c to Rust (rust/js-media, rust/js-fonts,
  rust/js-forms), with unchanged results.
* Flexbox layout (row, wrapping and column flex containers) moved from
  layout.c to Rust (rust/layout-flex), with identical layout.
* Table layout (column widths for auto and fixed table-layout, spans, col
  and colgroup, border-spacing and collapsed borders, row heights, vertical-
  align, captions and intrinsic widths) moved from layout.c to Rust
  (rust/layout-table), with identical layout.
* Script, module and stylesheet loading (scheduling, the parser hold, import
  maps, module fetching, the script and link load and error events) moved
  from js.c to Rust (rust/js-loader). Module source now always reaches the
  compiler NUL-terminated, so import() of a blob: URL no longer fails at
  random with a SyntaxError.
* Iframe browsing contexts and their realms (the realm cloner, the frame
  bootstraps, the frame table, sandbox flags, the frame load queues and the
  frame-script helpers) moved from js.c to Rust (rust/js-frames), with
  unchanged results.
* innerHTML, outerHTML, getHTML, setHTMLUnsafe, insertAdjacentHTML,
  insertAdjacentElement, insertAdjacentText, textContent, XMLSerializer and
  the microdata API moved from js.c to Rust (rust/js-html). Markup
  containing an embedded NUL is no longer cut short.
* The document factories (createElement, createElementNS, createTextNode,
  createComment and the rest), cloneNode, importNode, adoptNode,
  DOMImplementation, the namespace lookups and the a/area URL-part accessors
  moved from js.c to Rust (rust/js-dom-factory), with unchanged results.
* Focus, click() and keyboard activation, the Fullscreen API and Pointer
  Lock moved from js.c to Rust (rust/js-focus), with unchanged results.
* Reflected HTML attributes (string, URL, enumerated, integer and boolean
  accessors, dir, translate, autocomplete, autocapitalize, tabIndex,
  contentEditable and the rest) moved from js.c to Rust (rust/js-reflect),
  with unchanged results.
* The window's browsing-context members (window, self, frames, length, top,
  parent, name, closed, opener, print, stop, focus), its viewport metrics
  and named access on window and document moved from js.c to Rust (rust/js-
  window), with unchanged results.
* Selector queries (querySelector, querySelectorAll, matches, closest,
  getElementById, getElementsBy*), the per-page query cache and live
  HTMLCollection, NodeList and RadioNodeList moved from js.c to Rust
  (rust/js-collections). Live-collection rebuilds and getElementsByClassName
  are 15-30% faster; static NodeLists now share their item, namedItem and
  forEach functions.
* appendChild, insertBefore, removeChild, replaceChild, moveBefore, the
  ChildNode and ParentNode methods (before, after, replaceWith, remove,
  append, prepend, replaceChildren) and the pre-insertion validity checks
  moved from js.c to Rust (rust/js-node), with unchanged results.
* The form-control bindings (value and defaultValue with sanitization,
  valueAsNumber, valueAsDate, stepUp, stepDown, the text selection APIs,
  checked, indeterminate and radio groups, select options and selectedIndex,
  label.control, progress and meter) moved from js.c to Rust (rust/js-
  forms), with unchanged results.
* Shadow DOM and slots (attachShadow, the ShadowRoot wrapper, assignedNodes,
  assignedElements, assignedSlot, getRootNode) moved from js.c to Rust
  (rust/js-shadow), with unchanged results.
* CSSOM View geometry and scrolling (client rects, offset, client and scroll
  metrics, element and window scrolling, scrollIntoView) and SVG geometry
  (getBBox, getCTM, getScreenCTM, path length and point-at-length) moved
  from js.c to Rust (rust/js-geometry), with unchanged results.
* DOMTokenList (classList, relList and the other token lists),
  element.dataset, element.attributes and Attr nodes moved from js.c to Rust
  (rust/js-attrs), with unchanged results.
* The Document property bindings (cookie, title, dir, body and head, the
  legacy colours, readyState, referrer, compatMode, activeElement,
  scrollingElement, scripts and anchors) moved from js.c to Rust (rust/js-
  document), with unchanged results.
* Popovers, <dialog>, invoker commands (popovertarget, commandfor), the top
  layer, the focusing steps and exclusive <details name> groups moved from
  js.c to Rust (rust/js-top-layer), with unchanged results.
* HTMLMediaElement's playback state and methods, the MSE natives,
  canPlayType, MediaSource.isTypeSupported and navigator.mediaCapabilities
  moved from js.c to Rust (rust/js-media), with unchanged results.
* innerText, outerText and the CharacterData and Text methods
  (substringData, appendData, insertData, deleteData, replaceData,
  splitText, wholeText, normalize) moved from js.c to Rust (rust/js-text),
  with unchanged results.
* Range, Selection, TreeWalker and NodeIterator moved from js.c to Rust
  (rust/js-traversal). A NodeIterator is now traced by the garbage
  collector, so cycles through it no longer leak.
* The WebRTC stubs (RTCPeerConnection, RTCDataChannel), ClipboardItem and
  navigator.sendBeacon moved from js.c to Rust (rust/js-services, rust/js-
  net), with unchanged results.
* URL, URLSearchParams, atob, btoa, TextEncoder, TextDecoder, object URLs
  and FileReader moved from js.c to Rust (rust/js-url). Passing a value that
  cannot become a string (such as a Symbol) to btoa, atob or encode now
  throws instead of returning an empty result.
* localStorage and sessionStorage moved from js.c to Rust (rust/js-storage).
  Keys now enumerate in the same order as key(n); the on-disk format is
  unchanged.
* Custom elements (customElements.define, get, whenDefined, upgrade and
  getName, element upgrades and the lifecycle callbacks) moved from js.c to
  Rust (rust/js-custom-elements), with unchanged results.
* crypto.getRandomValues, crypto.randomUUID, crypto.subtle and CryptoKey
  moved from js.c to Rust (rust/js-crypto), which calls rust/webcrypto
  directly. CryptoKey objects now inherit from CryptoKey.prototype
  (instanceof works) and subtle methods accept DataView input.
* The Web Audio bindings (AudioContext, OfflineAudioContext, the audio
  nodes, AudioParam, AudioBuffer and offline rendering) and EME's
  requestMediaKeySystemAccess and setMediaKeys moved from js.c to Rust
  (rust/js-media), with unchanged results.
* Rust 1.99 is now required, and rust-toolchain.toml pins 1.99.0 so local
  and CI builds use that version. The Rust code is updated for it: fixed-size
  pixel and hash loops use as_chunks, and nested conditions use let chains.
  Ubuntu 24.04 (whose newest packaged compiler is rustc-1.91) and Debian 13
  build with rustup.
* getComputedStyle and its resolved values, element.style, the CSS
  namespace and the Web Animations hooks moved from js.c to Rust
  (rust/js-cssom).
* Event, CustomEvent, UIEvent and the other event constructors, their
  attributes and legacy init*Event() methods, cancelBubble, returnValue and
  composedPath() are Rust (rust/js-events), with unchanged results.
* WebAssembly runs on wasmi, a pure-Rust interpreter (rust/js-wasm), in
  place of the vendored WAMR C runtime, which is gone along with src/wasm.c.
  Funcref tables can now be read from JavaScript (WAMR crashed on
  Table.get), a memory passed in as an import keeps its size, and the -Dwasm
  option now builds on every platform, NetBSD and 32-bit x86 included.
* MessagePort, MessageChannel, BroadcastChannel, dedicated workers and
  service workers are Rust (rust/js-workers), with unchanged results.
* Painting is all Rust (`rust/paint`); paint.c is gone. The last section
  moved the box tree walk — stacking order and deferred positioned layers,
  opacity groups, blend modes, transforms, sticky and fixed offsets, overflow
  clips and scrollbars, column rules, CSS 3D contexts and picking — and the
  viewport layer planner the tiled renderer composites. Rendering is
  unchanged.
* MessagePort, MessageChannel and BroadcastChannel are Rust
  (rust/js-workers), with unchanged results.
* The -Dquickjs=quickjs build option (Fabrice Bellard's original QuickJS
  through an adapter) is gone, the first step toward replacing QuickJS with
  the Rust engine Boa.
* History (pushState, replaceState, back, forward, go, popstate), the
  Navigation API, Location, window.open() and window.postMessage() between
  windows and frames are Rust (rust/js-window), with
  unchanged results.
* navigator and its sub-objects (permissions, clipboard, geolocation,
  mediaDevices, getBattery(), userAgentData, plugins, storage), the window
  console, alert(), screen, matchMedia(), Notification, queueMicrotask(),
  setTimeout()/setInterval() and requestIdleCallback() are Rust
  (rust/js-services), with unchanged results.
* fetch(), Request, Response, AbortController, AbortSignal,
  XMLHttpRequest, WebSocket and EventSource are Rust (rust/js-net), with
  unchanged results.
* MutationObserver, IntersectionObserver and ResizeObserver are Rust
  (rust/js-observers), with unchanged results.
* FormData, constraint validation (checkValidity(), validity,
  validationMessage, setCustomValidity()) and form submission and reset are
  Rust (rust/js-forms), with unchanged results.
* WebGL 1 and 2 are Rust (rust/webgl), with unchanged results: webgl.c is
  gone. The JavaScript engine layer gained typed-array access for it on both
  QuickJS and Boa.
* structuredClone() and the serialization behind worker and MessagePort
  messages are Rust (rust/js-clone), with unchanged results.
* Painting box decorations, replaced content and inline text is Rust
  (`rust/paint`): backgrounds, gradients, shadows, borders, border images,
  outlines, images with their filters and clip paths, video and audio chrome,
  CSS masks, and text with its decorations, selection, carets and form
  controls. Rendering is unchanged; paint.c keeps only the box tree walk.
* Page text setup and list markers are Rust (`rust/paint`), the first section
  of the paint.c port: the shared text context, fonts and line heights from
  computed styles, OpenType features and variations, language and direction,
  the font metrics the style engine measures with, and list item markers.
  Rendering is unchanged.
* Canvas text — fillText(), strokeText() and measureText() — and the image
  sources drawImage() and createPattern() read from (<img>, <canvas>, <video>
  posters, OffscreenCanvas, ImageBitmap) are Rust, which finishes the canvas
  port: js_canvas.c is gone. Output is unchanged.
* DNS over HTTPS works again. With doh_url (or NS_DOH_URL) set to an https://
  resolver, host names are looked up with RFC 8484 A and AAAA queries sent
  through the Rust HTTP client and cached for their TTL; when the resolver
  gives no answer the lookup fails rather than falling back to the system
  resolver.
* fillRect(), strokeRect() and clearRect() no longer touch the canvas's
  current path. They drew their rectangle onto it, so a path built with
  beginPath()/arc()/rect() was filled, stroked or cleared along with the
  rectangle and then lost before the page's own fill() or stroke().
* ImageBitmap and createImageBitmap() are Rust, with unchanged results for
  ImageData, Blob and canvas sources, cropping, cloning and close().
* A Path2D can no longer break the canvas it is drawn on. An ellipse() with a
  zero radius, or an addPath() with a singular or NaN transform, used to put
  the path into cairo's error state, and drawing it then left that canvas
  blank for good. Zero-radius ellipses now trace their flattened outline,
  non-finite ellipse() arguments are ignored, negative radii throw
  IndexSizeError, a singular addPath() transform collapses the path and a
  non-finite one is ignored.
* Path2D is Rust, from the path it records to its methods and the
  rounded-rectangle geometry roundRect() shares with the 2D context. Arguments
  are now converted left to right, as WebIDL asks; the C converted some of
  them in whatever order the compiler chose.
* The canvas objects' JavaScript interfaces are Rust: CanvasRenderingContext2D
  and its offscreen twin, CanvasGradient, CanvasPattern, TextMetrics, ImageData,
  OffscreenCanvas and the Path2D and ImageBitmap interface objects, with their
  attribute validation, error messages and structured cloning unchanged (checked
  property by property against the C build). The drawing itself is still C.
* The canvas 2D API starts moving to Rust (rust/canvas): colour parsing and
  the colour strings fillStyle, strokeStyle and shadowColor report, and the
  filter and letter/word-spacing checks, the serialized font and the SVG
  path data parsed by new Path2D(), the shadow blur, compositing keywords and
  conic gradients, identical to the C over a fuzzed set
  of colour, filter, length, font and path strings.
* The Performance API is Rust: performance.now(), mark(), measure(), the
  entry getters and clear methods, resource timing with its Timing-Allow-Origin
  and CORS rules, each frame's own clock and timeline, and
  PerformanceObserver. Checked against the C build on local and HTTP-served
  pages, including cross-origin, Timing-Allow-Origin and frame resources, with
  identical results.
* The WebIDL brand checks on DOM interface members (calling a Node or
  Element method on the wrong kind of object throws, rejects or is ignored
  as the member requires) are applied from Rust, with identical results over
  every member of every Node interface.
* ShadowRealm is Rust, written on the engine-neutral JavaScript layer like
  Temporal and Intl, so it also runs on Boa. evaluate(), wrapped callables
  and importValue() behave as before on both QuickJS engines.
* libcurl is no longer used or linked. Every network request — page loads,
  subresources, preconnects, EventSource, WebSocket, FTP and the audio
  helper's downloads — goes through the in-tree Rust HTTP client, and the
  http_backend build option is gone. Dates in cookies and cache headers are
  parsed in-tree with the same rules curl_getdate used (checked identical
  over 300,000 generated dates). about:southstar names the HTTP client
  instead of the libcurl version. TLS Encrypted Client Hello, which came
  from libcurl, is not supported for now.
* Pages and subresources are fetched by a new HTTP client written in Rust,
  rust/http, instead of libcurl. It speaks HTTP/1.1 and HTTP/2, with its own
  HTTP/2 framing, flow control and HPACK header compression, over OpenSSL;
  HTTP/2 requests to one origin share a single pooled connection, and
  responses are decoded from gzip, deflate, br and zstd as they arrive. It
  replaces the optional nghttp2 backend (src/net_http2.c), so libnghttp2,
  ngtcp2, nghttp3 and gnutls are no longer used and HTTP/3 is gone. The
  http_backend meson option is now "rust" (the default) or "curl". libcurl is
  still used for proxied and FTP requests, WebSocket, Server-Sent Events and
  the audio helper, and goes away once those move to the new client.
* The Rust HTTP client records Strict-Transport-Security headers itself,
  into the same HSTS list curl kept, so hosts seen over valid HTTPS are
  upgraded from http:// on later visits as before.
* Proxied requests go through the Rust HTTP client too: HTTP proxies
  (plain requests forwarded, https:// tunnelled with CONNECT), SOCKS4,
  SOCKS4a, SOCKS5 and SOCKS5h, each with optional user and password, and
  the no-proxy list matching hosts, domains and IP ranges. Only FTP still
  goes through libcurl for page loads.
* EventSource (Server-Sent Events) streams over the Rust HTTP client instead
  of libcurl, following redirects, honouring the proxy settings, opening as
  soon as the response headers arrive and reconnecting with Last-Event-ID as
  before.
* WebSocket runs on the Rust HTTP client instead of libcurl's WebSocket
  API: an HTTP/1.1 upgrade over the same connection code (TLS, proxies) and
  an in-tree RFC 6455 frame codec with masking, fragmented messages,
  ping/pong and the close handshake. The server's Sec-WebSocket-Accept is
  verified, and WebSocket.protocol now reports the subprotocol the server
  chose (it was always empty). WebSocket no longer depends on the libcurl
  version.
* ftp:// downloads and directory listings go through the Rust client too,
  with anonymous or URL credentials, a CWD per path segment as curl did,
  EPSV with a PASV fallback, and proxies for both connections. Page loads
  no longer use libcurl at all.
* The audio helper downloads media through the Rust HTTP client (and
  decodes data: URLs itself) instead of libcurl, so southstar-audio no
  longer links libcurl. The helpers' Rust library is now built on every
  platform, not only Linux.
* On Windows the Rust HTTP client trusts the certificates in the Windows
  root store when no CA bundle file is found, as curl did with its native
  CA option, so HTTPS keeps working on machines without a bundle.
* HTTP/1.1 responses from a server that pauses for more than a second no
  longer fail on Windows: the Rust client waits for sockets with poll()
  instead of socket timeouts, which Windows reports as errors rather than
  as "try again".
* The logo has a large serif S behind the star, as Nordstjernen's had an N:
  the application and window icon, the Windows icon, the animated start-page
  logo and the two badges.
* The cute-tests pages are removed.
* The Windows, macOS and musl CI workflows no longer run on every push
  and pull request; they can still be started by hand.
* Nordstjernen is renamed Southstar Browser: the executables, library,
  app ID, configuration folders, user-agent token, logo and documentation
  carry the new name. The Android, iOS and Java/JVM versions are removed.
* docs/rust-port.md plans the port of Southstar from C to Rust: an
  incremental, in-place port in nine phases, starting with the helper
  processes and the renderer protocol and ending with the engine core and
  the JavaScript bindings, with the build integration, the checks every
  step must pass and the decisions still open.
* Rust joins the build. meson builds a Cargo workspace (Rust 1.85 or
  newer) into one static library that every C target links, rebuilding
  it only when Rust sources change. The first module ported is the date
  and time parsing behind <input type=date|month|week|time|datetime-local>,
  which behaves identically. Building Southstar now needs cargo and rustc;
  on Ubuntu 24.04 that means the rustc-1.85 and cargo-1.85 packages.
* Bookmarks storage, the Content-Security-Policy parser and checks, and the
  CSS Syntax tokenizer used for @property values are now Rust. Each was
  checked against the C it replaces over millions of inputs, from fuzzed
  strings to policies and style sheets taken from real sites, with no
  difference in results.
* The UI translation lookup, the local safe-browsing blocklist and its
  warning page, the debug log and the WOFF2 web-font decoder are Rust too,
  again checked against the C over millions of inputs and real fonts. The
  built-in safe-browsing test host, malware.testing.southstar, is blocked
  again; its entry still held the digest of the old name.
* Browsing history is Rust: the SQLite visits table and the about:history
  page behave as before, checked against the C over 420 randomized
  sessions of visits, clears and restarts.
* The JavaScript bytecode cache is Rust. Its files keep the same names
  and format, so caches written by earlier builds stay valid.
* The runtime configuration (southstar.conf, its defaults, the NS_*
  environment overrides, Settings saves and --print-config) is Rust and
  reads and writes the same files.
* Spell checking of editable text is Rust, still over Enchant when it is
  installed.
* about:southstar has a Rust section: the compiler a build used, the
  minimum Rust version, the build profile and the modules now in Rust.
* Temporal is the first JavaScript binding written in Rust. It is built on
  the engine-neutral layer, so the same code also runs on Boa, and it
  behaves exactly as the C did.
* Intl is Rust too: Collator, NumberFormat, DateTimeFormat, PluralRules,
  ListFormat, RelativeTimeFormat, DisplayNames, DurationFormat, Segmenter,
  Locale and the toLocaleString, toLocaleDateString, toLocaleTimeString and
  localeCompare hooks give the same strings and parts as before, again
  written against the engine-neutral layer.
* Web Cryptography (crypto.subtle) is Rust, still over OpenSSL: hashing,
  HMAC, AES, RSA, ECDSA/ECDH, Ed25519/X25519, PBKDF2 and HKDF give the same
  results and the same errors as before.
* The thread dump (Task Manager's dump button, and SIGQUIT on Unix) is Rust.
* The offscreen GL context WebGL draws into is Rust, over WGL on Windows,
  CGL on macOS and EGL elsewhere, still through libepoxy.
* The 4x4 matrices behind CSS 3D transforms are Rust and compute the same
  values bit for bit.
* ICO/CUR favicons and WebP images, still and animated, are decoded in
  Rust (WebP still through libwebp), with the same pixels as before.
* The sandbox is Rust: Landlock and seccomp on Linux (the audio and video
  helpers included), Seatbelt on macOS, the Windows mitigation policies and
  elevation check, Subresource Integrity checks and download marking. The
  Landlock rules and seccomp filters it installs are the same as before.
* @property syntax definitions are parsed, matched and computed in Rust,
  directly over the Rust CSS tokenizer, with unchanged results.
* Media queries (@media rules, matchMedia() and their serialization) are
  parsed and evaluated in Rust.
* The HTTP cache is Rust. Its database and body files are unchanged, so an
  existing cache keeps working.
* Accept-Language and navigator.languages, address-bar search detection,
  search URLs, local paths as file: URLs and proxy password masking are
  Rust. Accept-Language q-values now always use a decimal point, even when
  the system locale writes decimals with a comma.
* EventSource (Server-Sent Events) and WebSocket are Rust, still over
  libcurl.
* Decoded image textures, the inline PDF viewer and microphone capture are
  Rust.
* Webcam capture is Rust: opening a V4L2 camera (MJPEG, else YUYV), its
  buffers and stream, frame conversion, device enumeration and the per-site
  camera permission make the same system calls and give the same frames.
* The XML parser behind DOMParser, XMLHttpRequest's responseXML and XHTML
  pages is Rust and builds the same documents, reporting parse errors at the
  same line and column.
* Form submission is Rust: which buttons submit or reset a form, the
  name/value pairs a submitted form sends and the constraint validation that
  stops an invalid form give the same queries, bodies and blocked fields.
* The @font-face loader is Rust: one fetch per font URL, WOFF and WOFF2
  converted to TrueType or OpenType, the on-disk web font cache and the
  fontconfig registration under the CSS family and descriptors write the same
  files and render the same text.
* OfflineAudioContext rendering is Rust: oscillators, buffer sources,
  constant sources, gain, biquad filters, compressors, delays, wave shapers
  and panners mix to the same samples.
* MathML layout and painting are Rust: tokens, scripts, fractions, roots,
  under and over scripts, tables, fences and semantics measure and draw the
  same pixels.
* The image cache and decode chain are Rust: fetching, retries with their
  back-off, threaded and synchronous decoding, animated GIF, APNG and WebP
  frames, the 256 MB budget and which image it purges first behave as before.
* Building the DOM from lexbor's HTML parse is Rust, as are declarative
  shadow roots, inline script source positions, standard video metadata and
  the XML well-formedness check; documents and fragments get the same trees.
* The DOM's form-control helpers are Rust, the first section of dom.c to
  move: input types and their numeric values, stepUp/stepDown for number,
  range, date, month, week, time and datetime-local, range and step
  validity, required and email checks, contenteditable and spellcheck hosts,
  and the values editing reads and writes, with the same results as before.
* Serializing the DOM is Rust: innerHTML, outerHTML and getHTML() with
  declarative shadow roots, the XML serialization, textContent and the text
  of subtrees, the --dump=dom tree and client-side image map hit testing give
  the same output.
* Which option a select shows and what it submits, option text and labels,
  form owners (including the form attribute and shadow trees), form reset,
  disabled fieldsets and optgroups, inert subtrees and the active modal
  dialog are Rust, with the same results.
* The document's id, class and tag indexes are Rust: getElementById,
  getElementsByClassName and getElementsByTagName, the first element of a
  tag, fragment targets and document order return the same nodes in the same
  order, and the indexes stay the same GLib tables the rest of the engine
  reads.
* The DOM is Rust: creating, linking, cloning and freeing nodes, names and
  text with their borrowed or owned strings, attributes with their
  namespaces, the attribute bloom filter, class tokens and template
  contents. dom.c is gone; nodes and attributes keep their layout and GLib
  memory, so the rest of the engine reads and builds them as before.
* IndexedDB storage is Rust: the per-origin SQLite files, their schema,
  handle cache and quota, object stores, indexes, key generators and records
  read and write the same databases, and databases written by earlier builds
  open unchanged.
* The headless driver behind --headless is Rust: the renderer-driven run
  for --dump=text, dom and layout, the in-process run for PNG, PDF and print
  captures, --inspect, --wpt and NS_HEADLESS_LEGACY, the scripted --act input
  (clicks, typing and keys in form controls, form submission, drag and drop,
  holds, scrolls, screenshots), followed navigations, inline video frames in
  captures, the WPT harness report and --debug logging. headless.c is gone;
  output is byte for byte the same.
* The engine's captures and dumps are Rust: PNG, PDF and paged-PDF output
  with its metadata, the per-sheet print recordings, the text and layout
  dumps and keyframe loading, with the same output.
* The engine's fetching is Rust: blocking page fetches, navigations and form
  POSTs, stylesheet fetches with MIME and nosniff checks, retry markers and
  resource timings, linked-stylesheet text for CSSOM, speculative preloads
  and preconnects, and the blocking and incremental image fetches, making the
  same requests in the same order.
* The rest of the engine pipeline is Rust: collecting a page's style sheets
  (runs of inline <style>, linked sheets, @import chains, frames' own sheets
  under their viewport for media queries, and adopted sheets), the cascade,
  relayout with its NS_PROFILE timings and the second pass when a frame's
  measured viewport changes, and the relayout counters. engine.c is gone;
  styles, layouts, captures and requests are the same as before.
* The embedding API's page lifecycle is Rust: building a page from its
  document, relayout with its oscillation damper and saved scroll offsets,
  image sessions, settling and the per-frame tick, viewport and device pixel
  ratio changes, fragment targets and scroll requests, the script engine's
  and media's callbacks, form submission, declarative refresh, the dumps,
  captures and print sheets, the title, links and favicon, the caret blink
  and closing a page. Pages behave exactly as before.
* Opening and painting a page are Rust too: the safe-browsing gate,
  local file paths, HTTPS-first upgrades, error pages, image, PDF, JSON, XML
  and plain-text documents, the security state, and every way a page is
  painted for the shell (whole frames, scroll snapping, layer plans,
  document tiles, fixed and sticky layers, scroller rectangles), with
  pixel-identical output.
* Page input is Rust as well, completing the embedding API: links and the
  cursor under the pointer, media hits, find in page, selection gestures,
  hover events, wheel and scrollbar scrolling, dropped files, context menus,
  presses and clicks with select, datalist, summary, reset and submit
  activation, access keys, editing keys, typing and paste. libsouthstar.c
  is gone; pages respond to input exactly as before.
* Networking starts moving to Rust: the error page shown when a load fails
  (the failure classified from the transport error or HTTP status, with its
  icon, explanation and what to try) and data: URL decoding, percent-encoded
  or base64 under the response memory budget, produce the same pages and
  bytes as before.
* Local folders and FTP directories are listed by Rust: the "Index of" page
  with folders first, sizes and modification dates, Unix and DOS FTP listing
  formats, file: reads under the response budget with their content type,
  and view-source: pages with their highlighting, all unchanged.
* The about: pages are Rust: the new tab with its logo, tagline and search
  engine, about:southstar with its diagnostics, the license texts, history,
  the settings page and its load, save and clear endpoints, and the check
  that keeps them from web content. They read the same as before.
* URL handling is Rust, over the same lexbor WHATWG parser: resolving,
  the URL setters behind location and URL objects, origins, sites, hosts
  and components, referrers, tracking-parameter stripping, HTTPS-first
  upgrades, Refresh headers and the user agent strings, with identical
  results. Parsed URLs are now freed after use instead of accumulating in
  each thread's parser memory until the thread ends.
* The cookie jar and HSTS are Rust: reading and writing the per-site
  cookie files for requests, Set-Cookie headers and document.cookie (with
  the Secure, HttpOnly, Max-Age, Expires, Domain and Path rules and the
  __Secure- and __Host- prefixes), the HSTS host list curl keeps, and the
  network layer's data, cookie and private-mode folders. Cookies are
  stored and sent exactly as before.
* More of the network layer is Rust: the response body and header sinks
  every transport writes into (with the memory budget and the headers
  the fetch path keeps), the developer-tools network log and fetch
  counters, connection statistics, proxy settings, copying and freeing
  responses, and form encoding (urlencoded in the page's charset and
  multipart boundaries). Requests and their logs are unchanged.
* Network setup and the transport plumbing are Rust: initializing and
  tearing down curl, the thread that drives every transfer through one
  multi handle, the shared DNS, TLS-session and connection caches, the
  TLS options, cancellation, the six-connections-per-origin limit,
  remembering origins that refused a connection, the CA bundle search and
  the response memory budget. Fetches behave as before.
* Fetching a URL is Rust: the curl request for each hop (with the retry
  the insecure-TLS override allows), the Accept, Sec-Fetch, client-hint,
  Do Not Track, Global Privacy Control, Origin and revalidation headers,
  the HTTP cache lookup, conditional requests and storing, the cookie jar
  a request uses, the network log, and the redirect loop with Fetch's
  rules for methods, bodies and credentials. Requests carry the same
  headers as before, over both the curl and the nghttp2 backends.
* The request queue is Rust, and with it all of net.c: asynchronous and
  blocking fetches, the limits of 32 requests at once and 6 per host,
  sharing one response among identical requests in flight, the
  responses kept for the preloads a page announces, preconnects, blob:
  URLs and shutting the network down. Only the nghttp2 backend's own
  client is still C in the network layer.
* HEAD requests from XMLHttpRequest and fetch() no longer hang until the
  30-second timeout with the curl backend, and redirects of a HEAD request
  are followed, as the nghttp2 backend already did.
* The style-and-layout pipeline every relayout runs is Rust: computing
  styles, loading only the web fonts a page uses (by font-family and
  unicode-range, including generated content), the container-query passes
  and container units, the width a viewport meta tag asks for, and the
  @page rule and :hover and :active use it records. Pages lay out, render
  and print as before.
* CSS transitions and @keyframes animations are Rust: starting, reversing
  and cancelling transitions (with transition: all, discrete properties and
  allow-discrete), sampling keyframes with their own easing, fill modes,
  directions and iteration counts, the animation and transition events,
  the Web Animations hooks behind getAnimations(), currentTime, pause,
  play, finish, cancel and Element.animate(), and writing animated values
  into computed styles. Animations look and time as before.
* The first section of css.c is Rust: the colour parser behind every CSS
  colour value, canvas fillStyle and SVG paint (hex, named and system
  colours, rgb(), hsl(), hwb(), lab(), lch(), oklab(), oklch(), color-mix(),
  light-dark() and calc() inside them). Colours parse as before, except that
  a channel too large for an integer (rgb(1e20 0 0), rgb(calc(infinity) 0 0))
  now clamps to 255 as specified instead of coming out as 0.
* Lengths and CSS math functions are Rust: every length unit (absolute,
  font-relative, viewport, dynamic and container units), calc(), min(),
  max(), clamp(), round(), mod(), rem(), abs(), hypot(), pow(), sqrt(),
  exp(), log(), sign(), the trigonometric functions, progress(), env() and
  the constants pi, e, infinity and NaN, resolving them to pixels and
  percentages, evaluating the ones that wait for a percentage basis, and
  the canonical calc() spelling getComputedStyle and element.style report.
  Values compute and serialize as before.
* Container queries are Rust: the query containers layout records, the
  stack of ancestor containers while styles cascade, the cqw, cqh, cqi,
  cqb, cqmin and cqmax units, and @container conditions with names,
  width, height, inline-size, block-size, aspect-ratio and orientation,
  range syntax, not, and, or and sibling-index() in values, as they parse,
  serialize through CSSContainerRule and evaluate. Pages match the same
  rules as before.
* Gradients, positions and image values are Rust: linear-, radial- and
  conic-gradient() and their repeating forms (directions, angles, shapes,
  sizes, positions, colour interpolation spaces and hues, colour stops,
  hints and double positions), their specified and computed spelling and
  the angle and radii painting uses; <position> values; image-set() with
  its resolutions and types; the content property with strings,
  counter(), counters(), symbols(), attr() and quotes; unicode-range; and
  the text colours serialize to. They parse, serialize and paint as before.
* Transforms are Rust: transform lists with every 2D and 3D function,
  transform-origin and perspective-origin, the translate, rotate and scale
  properties, their computed and canonical specified spelling, whether a
  transform is 3D and the matrix it applies. Elements transform, report
  their transforms and animate as before.
* CSS grid values are Rust: grid-template-rows and -columns track lists
  with repeat() (including auto-fill and auto-fit), minmax(),
  fit-content(), math functions, line names and subgrid;
  grid-template-areas; grid-row, grid-column, grid-area and their longhand
  lines with span, integers and custom identifiers; the grid-template and
  grid shorthands with auto-flow, and grid-auto-flow; how they serialize
  and compose back from longhands in element.style. With them go the
  readers of CSS identifiers and strings and their escapes that selectors,
  @-rules and declarations share. Grids lay out and serialize as before.
* CSS font values are Rust: font-family lists (quoted names, generic
  families, random-item()) and the font shorthand in their canonical
  spelling, the font-size keywords, font-stretch, font-weight numbers and
  bolder and lighter, font-feature-settings, font-variation-settings and
  font-variant-ligatures, the ex, ch, cap and ic units from the painter's
  font metrics, and resolving a family list to the font Pango loads.
  Text picks the same fonts and serializes as before.
* box-shadow and text-shadow are Rust: shadow lists with their offsets,
  blur, spread, colours, inset and font-relative or calc() lengths, their
  canonical specified spelling and their computed text. Shadows paint and
  serialize as before.
* CSS time values, easing functions and the animation and transition
  properties are Rust: times built from s, ms and the math functions,
  their specified and computed spelling, steps(), cubic-bezier(),
  linear() and the easing keywords, every animation and transition
  longhand list with its canonical form, animation-range, the animation
  and transition shorthands, the per-element lists the animation engine
  runs, and their serialization in getComputedStyle and element.style.
  Animations and transitions run and serialize as before.
* The display property is Rust: its single and multi-keyword forms, the
  vendor-prefixed flex, grid and box values, their canonical spelling and
  blockification; so are counter-reset, counter-increment and counter-set
  lists with reversed(), list-style-type with symbols() and strings, the
  list-style shorthand's text, and overflow-clip-margin. Boxes lay out,
  count and serialize as before.
* border-image is Rust: the slice, width, outset and repeat longhands in
  canonical form, the tokens of the border-image shorthand, and the
  slices, widths, outsets and tiling painting reads from a computed
  style. Border images paint and serialize as before.
* The value parser behind every CSS declaration is Rust: for each of the
  242 properties it turns the declared text into the value css.c stores
  (keywords and their accepted sets, lengths with the per-property rules
  for negatives, bare numbers and keywords, calc(), colours, sizes,
  rects, URLs, image-set() and gradients, layered background and mask
  lists, and the structured values the earlier sections parse). Every
  property parses as before.
* The initial values getComputedStyle and the animation engine fall back
  to for properties without a computed spelling of their own come from a
  Rust table, with the same values as before.
* Expanding a CSS declaration into its longhands is Rust: all, the border,
  border side, logical border, outline and column-rule shorthands, the
  background and mask layer lists, background-position and
  object-position, grid-template, grid, gap and grid placements, the
  place-* pairs, columns, text-decoration, font with the system fonts,
  flex and flex-flow, list-style, border-radius, inset, margin, padding
  and their logical forms, text-wrap, container, animation-range, the
  animation and transition longhands, and the legacy and logical
  property aliases. Style sheets and inline styles expand as before.
* The inline style text behind element.style is Rust: reading one
  property from a style attribute (rebuilding the margin, padding and
  border quads, overflow, outline, list-style, background,
  background-position, grid, grid-template, font, animation, transition
  and animation-range shorthands from their longhands, and honouring all
  and !important), setting one (all, a shorthand written after it, and the
  animation, transition and list-style longhands), and cssText with
  complete shorthands collapsed and every value in canonical form.
  element.style reads, writes and serializes as before.
* Serializing computed values, interpolating them for transitions and
  animations (lengths, mixed lengths and percentages as calc(), numeric
  and length keywords, colours, shadows, clip rects, and transform lists
  including none against a transform) and comparing them are Rust, as is
  the canonical specified text element.style reports for display,
  transforms, border-radius, animations, colours, shadows, times and the
  other properties with a canonical spelling. Values serialize and
  animate as before.
* Reading a declaration block is Rust: each declaration's name and value
  with !important, custom properties and the values that wait for var(),
  attr() or container units set aside on the rule, var() fallbacks
  substituted in values that need no other substitution, the syntax
  checks for unbalanced brackets, stray semicolons and bad attr() types,
  and the validity checks element.style, CSS.supports() and @supports
  ask of one declaration. So is choosing an image width from an
  <img sizes> list. Style sheets parse and images pick their sources as
  before.
* The selector parser is Rust: type, universal and namespaced selectors,
  ids, classes, attribute selectors with every operator and the i and s
  flags, every pseudo-class (nth-child() and friends with An+B and "of
  S", :heading(), :lang(), :dir()), :is(), :where(), :not() and
  relative :has(), the pseudo-elements, combinators, specificity, the
  ancestor hashes the Bloom filter uses, and @supports selector(). The
  selectors css.c matches are built from it with the same structure, so
  selectors match, count specificity and report validity as before.
* CSS nesting is flattened in Rust before a style sheet is parsed: nested
  style rules joined to their parents through :is() and &, nested
  @media, @supports, @container, @layer and @scope rules lifted around
  their parents, with the same selector budget against exponential
  growth. @supports conditions and CSS.supports() are Rust too:
  declarations, selector(), not, and and or, and functions such as
  font-tech() reported unsupported. Style sheets flatten and conditions
  evaluate as before.
* The style sheet parser is Rust: style rules and their declarations,
  @media, @supports, @container and @scope blocks, @layer blocks and
  statements with anonymous layers, @import with layer() and media,
  @font-face (the best src URL chosen by format), @keyframes with their
  stops, @property with its syntax and initial value checked, and @page
  size and margins. So are resolving a sheet's URLs against its base and
  moving an imported sheet into a layer. The rules, layers and at-rules
  css.c's cascade reads are built with the same structure, so pages style
  as before (checked identical over 170,000 generated and mutated sheets);
  only an @font-face font-weight of "nan" now reads as no weight instead
  of an undefined integer conversion.
* Scoping a shadow tree's or framed document's style sheet to its host is
  Rust: :host, :host(), :host-context() and ::slotted() rewritten to the
  host's scope attribute, html and :root selectors attached to it, every
  other selector confined beneath it, through @media, @supports,
  @container, @layer and @scope blocks, with the scoped text remembered
  per host. Shadow trees and frames style as before; a host whose scope
  id is longer than 78 characters is no longer cut off in the rewritten
  selectors.
* Presentational hints are Rust: the declarations legacy HTML attributes
  stand for (bgcolor, background, text, width and height, hspace and
  vspace, align and valign, border, cellspacing and cellpadding, table
  rules and frame, font color, face and size, body margins, hr size,
  color and noshade, list type, nowrap, textarea wrap, iframe
  frameborder, image and canvas aspect ratios, picture sources' sizes)
  and SVG presentation attributes, with the HTML legacy colour parser.
  Pages with legacy markup style as before (checked identical over a
  million generated elements).
* The element state behind pseudo-classes is Rust: :checked, :default,
  :indeterminate, :valid and :invalid (with :user-valid/:user-invalid,
  required values, email and URL syntax, number parsing, range and step,
  pattern and length limits), :in-range and :out-of-range, :read-write,
  :placeholder-shown, :blank, :required, :disabled, :empty, links and
  :visited, :target and :target-within, :lang() with the
  Content-Language pragma and wildcard ranges, :dir() with dir=auto and
  bdi resolution, :heading(), and the open, popover, modal and media
  states. Selectors match as before (checked identical over 50 million
  element/pseudo-class tests).
* attr() substitution is Rust: an attribute read as a quoted string, as
  any value, against a type() syntax or with a unit, nested fallbacks,
  and the taint that keeps attribute text out of url() and the image
  functions. Values substitute as before (checked identical over 7.5
  million generated calls); a type() whose syntax has no <...> no longer
  leaks the parsed syntax.
* Selector matching is Rust: type, id, class and attribute tests (every
  operator, i and s flags, HTML case-insensitive attributes), the
  structural, nth (with "of S"), hover, focus, active, fullscreen, scope
  and defined pseudo-classes, :is()/:where()/:not(), relative :has()
  with its per-pass memo, combinators matched right to left under the
  same operation budget, nth positions cached per query batch, and
  @scope roots, limits and proximity. querySelector, matches() and the
  cascade match as before (checked identical over 20 million generated
  selector/element pairs in every mode) at the same speed.
* var() substitution is Rust: custom properties looked up through the
  element's inherited variable maps, CSS-wide keywords treated as
  guaranteed-invalid, registered properties falling back to their
  initial value, nested fallbacks, and the 1 MiB / 100,000-call budget
  that stops self-referencing variables from exploding. So are the
  variable-name listing behind getComputedStyle and the CSS-wide keyword
  test the custom-property cascade uses. Values substitute as before
  (checked identical over 150,000 generated variable chains).
* The per-sheet rule index is built in Rust: every selector filed under
  the rarest id, class, tag or attribute its subject compound requires
  (counted across the sheet), the rest under the universal list, with
  the sheet's and rules' pseudo-element masks recorded on the way.
  Indexes come out identical (checked over 83,000 generated sheets).
* Restyle invalidation is Rust: the ids, classes, tags and attributes the
  style sheets' structural, sibling and :has() selectors depend on, and
  the elements a DOM mutation marks dirty from them (the changed element,
  the siblings after it when a sibling selector can see the change, the
  whole child list when an nth or :empty selector can, and the :has()
  subjects above it), so the next style pass recomputes only those. The
  same elements are marked as before (checked over 20 million node
  comparisons after generated attribute and child-list mutations).
* The custom-property cascade is Rust: an element's matched --*
  declarations put in cascade order, revert, revert-layer and
  revert-rule rolled back to the declaration they reveal, inherit,
  initial and unset applied, var() references expanded, and registered
  @property values checked against their syntax and falling back to
  their initial or inherited value, into the variable map its style
  carries. Maps come out identical (checked over 1.9 million generated
  cascades) and building one is about a fifth faster.
* Unit resolution in computed styles is Rust: the element's font size in
  px (em, rem, %, lh, font-relative, viewport and container units and
  calc()), then em, rem, ex, ch, cap, ic, viewport units and
  line-height percentages resolved across lengths, calc() values,
  shadows, grid tracks, transforms and two-value sizes, with infinite
  or NaN calc() results clamped and shared values copied before they
  change. Values come out identical (checked over 58 million generated
  property values) and slightly faster.
* The computed text of grid-template track lists (what
  getComputedStyle reports) is Rust: lengths and math functions in em,
  rem and px resolved to px, percentages and percent-plus-length calc()
  kept, and line names, keywords and fr lengths left as written. The
  text is identical (checked over 1.6 million generated track lists).
* Registered custom properties get their computed values in Rust: each
  @property-typed variable an element sets is computed against its
  syntax with the element's font size, line height, font-relative
  units, root sizes, viewport, container and currentcolor, the context
  built only when a typed property needs it. Values are identical
  (checked over 320,000 generated styles and registries).
* The CSS property table is Rust: property names, the vendor and legacy
  aliases (-webkit-*, word-wrap, text-wrap, line-clamp, …) and logical
  properties that resolve to them, which properties inherit and which
  only repaint. Looking a property up by name is about 13 times faster
  (a hash lookup instead of a scan of all 242 names); the results are
  identical (checked over 8 million generated names).
* Cascade layer order is Rust: the layers a style pass's sheets declare,
  and every dotted prefix of them, ranked so sibling layers keep the
  order they were first declared in and a layer's sublayers come before
  the layer's own rules. The ranks are identical (checked over 400,000
  generated sets of @layer statements, blocks, nested and anonymous
  layers and layered @imports).
* Applying an element's matched declarations is Rust: cascade order
  (importance, origin, inline style, layer, specificity, scope
  proximity, sheet and source order), revert, revert-layer and
  revert-rule rolled back to the declaration they reveal, inherit,
  initial and unset, inheritance from the parent, currentcolor and
  transparent in the color properties and shadows, bolder and lighter,
  display blockification for roots, floats, positioned elements and
  flex or grid items, the legacy -webkit-box forms, and the overflow
  pair. Styles come out identical (checked over 480,000 generated
  elements, 116 million property values) and about 30% faster.
* Declarations that wait on var() or attr() are resolved in Rust: in
  cascade order each one's value is substituted from the element's
  variables and attributes, dropped when attr() taints a non-custom
  property or the result smuggles in !important, parsed into its
  longhands as matched declarations, or set to unset when it cannot
  stand. The matched declarations are identical (checked over 320,000
  generated elements, 4.5 million declarations).
* The questions layout, paint and script ask of a computed style are
  answered in Rust: writing mode and text orientation, lengths in px
  (with font-relative, root, viewport and container units and math
  functions), the used column count and gap, keyword tests, background
  layers, the effective transform with translate/rotate/scale folded
  in, alignment keywords without safe/unsafe/legacy, the overflow
  keyword an axis is used with, the border-image source and which
  colors came from currentcolor. The answers are identical (checked
  with 172 million generated queries).
* Gathering the rules that match an element is Rust: candidates are
  looked up in each sheet's rule index by id, class, tag and attribute,
  filtered by @container conditions and the ancestor Bloom filter
  (which css.c's tree walk now feeds through Rust), matched through the
  per-pass selector cache, and recorded per pseudo-element with each
  rule's most specific matching selector and scope proximity. The
  gathered declarations, custom properties and pending declarations are
  identical and in the same order (checked over 2.75 million generated
  element and sheet combinations), at the same speed.
* The user-agent style sheet, the default styles of HTML elements and
  the quirks-mode additions, now lives as plain CSS in rust/css
  (ua.css, ua-quirks.css) instead of C string literals; each is parsed
  once per process from the same text as before.
* The style sheet caches are Rust: the sheets kept per <style> element,
  per merged run of style text, per linked URL and per imported URL and
  layer, with the same trimming between style passes, and the style text
  a <style> element or adopted shadow sheet contributes, scoped to its
  shadow host or framed document. Every lookup returns the same cached or
  freshly parsed sheet as before (checked over 1.4 million generated
  calls).
* The declarations an element brings itself, its presentational hints
  and its style attribute, are turned into matched declarations in Rust,
  parsed once per distinct text as before, with the same cascade
  placement (checked over 24,000 generated documents).
* Style sharing between elements is Rust: the key an element's parent,
  root font size, container context and matched declarations make, and
  the per-pass table of styles stored under it. Elements share exactly
  when they did before (checked over 16 million generated lookups).
* CSS.registerProperty()'s registry is Rust, with the per-pass table of
  @property rules and script registrations the cascade checks custom
  properties against; registrations are accepted and rejected as before.
* Resolving var() in @keyframes stops against an element's custom
  properties is Rust, with the transform a stop's resolved translate,
  rotate, scale and transform declarations give it; the per-pass table of
  registered properties now lives on the Rust side too.
* The lifetimes of the style engine's C structs are Rust: duplicating
  and freeing values, releasing variable maps, the pool computed styles
  come from with freeing and sharing them, and freeing style sheets with
  their rule indexes.
* The fixups a computed style gets after the cascade are Rust: display:
  contents on an element that cannot be unboxed becomes none, native
  checkboxes and radios drop the box decorations their widget draws, and
  a frame's width and height give its document's viewport.
* Computing a document's styles is Rust: the walk that gathers, shares
  or cascades each element's style and its pseudo-elements', framed
  documents with their own sheets and viewport, and the incremental
  restyle that reuses the previous pass's styles for clean elements. It
  produces the same styles and reuses the same ones as before, and a
  large document's styles compute 6-15% faster.
* css.c is gone: the style engine, from the CSS value parsers to the
  cascade walk, is now the rust/css crate. The last helpers moved with it
  (reading one escaped character of CSS text, the colour-scheme and
  reduced-motion preferences, scaling a style's font size), and the
  layout, paint and script code reads the same css.h structs as before.
* Choosing the image an <img> loads is Rust, the first section of
  layout.c's port: srcset candidates with width and density descriptors
  against sizes and the device pixel ratio, a <picture>'s first <source>
  whose type and media match, and the data-src and data-srcset attributes
  lazy-loading sites use, preferring a real URL over a data: placeholder.
* Finding the image-map <area> under the pointer is Rust: the usemap's
  <map> in the image's tree scope, coordinates read as HTML's list of
  numbers, and circle, polygon, default and rectangle shapes.
* Printing's pagination is Rust: the page setup and @page rules, forced
  and avoided page breaks, keeping lines of text whole across sheets, and
  painting each sheet, with the same sheets as before.
* Text selection on the page is Rust: dragging, word and block selection,
  select-all, the highlighted runs, the selection's bounds and the copied
  text (with user-select: none, line and paragraph separators and
  zero-width characters handled as before).
* SVG rendering is Rust: shapes and path data, transforms, gradients,
  clip paths, masks, markers, text, use and symbol, nested viewports,
  switch and systemLanguage, and the geometry behind getBBox and getCTM.
* Response-body charset detection and decoding, HTML escaping and the
  image, JSON and XML viewer pages are Rust.
* The framing of the renderer protocol is Rust: the HTTP/1.1 requests and
  replies between the window and its renderer processes, their X-* headers
  and JSON bodies, and the passing of the shared framebuffer's descriptor.
  So is the renderer's side of it: the session that answers the window's
  requests (opening pages, the back/forward cache, rendering, input, find,
  dumps, printing) and the tiling that cuts a page into tiles and fixed and
  sticky layers, crops and packs them into shared memory and describes them
  to the window. The window's side is Rust as well: starting renderer
  processes, sharing their framebuffer, every request it sends them, the
  process statistics in the task manager, and single-process mode. The
  southstar-renderer executable itself now starts in Rust.
* The watchdog that restarts the browser after a crash or a hang is Rust,
  with the same restart limits, hang budget and messages.
* WebExtensions are Rust: loading unpacked extensions and their manifests,
  content scripts with the browser.* shim and its storage, declarativeNetRequest
  rules and Adblock filter lists inject the same scripts and styles and block
  the same requests as before.
* docs/rust-port.md reviews the JavaScript engines available to Rust and
  plans the JavaScript engine as a build option: the Rust bindings target an
  engine-neutral layer, QuickJS-ng stays the default, and Boa and Nova can be
  built in and compared on test262, benchmarks and real pages.
* southstar-jsshell runs JavaScript, test262 and the Octane benchmarks on
  either the in-tree QuickJS-ng or Boa through a new engine-neutral layer,
  and scripts/js-engine-compare.py writes the results to docs/js-engines.md:
  test262 83.3% on QuickJS-ng and 94.4% on Boa, while QuickJS-ng runs Octane
  2 to 8 times faster and Boa still crashes on 13 tests.
* docs/ is trimmed for the port: the pages that described the C
  implementation's internals (architecture, threading, rendering, tab
  isolation, the watchdog, the embedding API, QuickJS notes, …), the
  architecture poster and its generator, and the old per-distribution,
  nightly-server and Microsoft Store guides are gone. What remains covers
  using, building and measuring the browser, and the port plan.
* The Linux CI workflow builds with GCC only; the Clang job is gone.

Southstar Browser starts from the Nordstjernen codebase. The entries below
are Nordstjernen's release history, kept as written; they refer to the
browser by its earlier name and include the Android, iOS and Java ports that
Southstar has since removed.

1.0.30:
======
* The home page set in Settings is saved and used. A text field never
  fired `change` when it lost focus or on Enter, so Settings, which saves
  on `change`, dropped a typed home page; text inputs and textareas now
  commit a user edit with `change` before `blur`, as other browsers do.
  Settings has a Save button for the General fields again, each control
  saves only its own value so a second Settings tab no longer writes back
  stale ones, and the Home button opens the configured page instead of
  always `about:start`.
* A frame's `history.pushState` and `replaceState` act on the History
  object they are called on. YouTube binds the methods of a hidden
  `about:blank` frame to the top window's history, so every navigation
  to another video changed that frame's URL and left the page's URL,
  `history.state` and back button on the first video.
* `ResizeObserver.disconnect()` and `unobserve()` work. Both were bound
  to a no-op on the prototype, so a disconnected observer kept firing;
  YouTube's like counter threw a `TypeError` from such a callback on
  every navigation.
* The address bar, tab title and back button follow same-document
  navigations. The renderer reported a page's URL and title only when it
  opened, so after YouTube moved to another video with
  `history.pushState` the shell kept showing the first video; the
  regular tick now carries both, and a pushed entry becomes a back step.
* Inline elements paint their own box: background over the padding,
  `border-radius`, and borders, sliced at line breaks so only the first
  and last fragment get the left and right edges. Before, an inline
  element's background covered only its glyphs and its borders were not
  drawn or given space at all. YouTube's description link chips and
  Stack Overflow's inline `code` now get their rounded, padded
  background.
* A translucent background on an inline element is painted with its
  alpha. Pango's background attribute is opaque, so YouTube's
  `rgba(0,0,0,.05)` link chip in a video description became a solid
  black bar that hid its text.
* `white-space` on an inline element applies to its own text. The
  whole inline run used the mode of its first node, so a `pre-wrap` span
  inside a normal block lost its line breaks, and a `normal` span inside
  a `pre-wrap` block kept its runs of spaces. YouTube's
  `yt-formatted-string[split-lines]` relies on this for the blank line
  between paragraphs of its notices and descriptions.
* `::before` and `::after` inside inline content get their own box
  model. An inline pseudo-element now keeps its horizontal margin and
  padding, so YouTube's `89K • Streamed 1mo ago` and El País's section
  separators are spaced as in Chrome instead of running together. An
  `inline-block` pseudo-element inside an inline element is laid out as
  a box instead of flattened into text, which brings back the Guardian's
  pulsing live dot; and a pseudo-element's padding and margin are no
  longer counted twice when its width is measured, which made Amazon's
  video controls and YouTube's ad badge 8-10px too wide.
* An inline-block in a centered or right-aligned `white-space: nowrap`
  line reports its real position to scripts and hit testing. Paint
  already aligned the line, but layout placed the inline-blocks as if it
  were left-aligned, so BBC's centered section menu reported x=0 instead
  of 259 and YouTube's search tab icon sat 8px left of its button center.
* Text and inline-blocks that share wrapped lines are painted on the
  line boxes that layout computed. Paint drew each line at Pango's own
  line height and each inline-block at Pango's position, so with a CSS
  line-height the second and later lines drifted away from the layout:
  a link after a wrapped paragraph on YouTube floated above the
  baseline and text overlapped the inline-blocks around it. Text on a
  single line is drawn as before, and the horizontal position still
  comes from Pango so `text-align` keeps working.
* An image or SVG with a percentage width no longer widens the boxes
  around it when they are sized to their min-content. Such a replaced
  element is compressible: its min-content contribution is zero, as in
  Chrome. Before, a YouTube channel name next to a 100%-wide verified
  badge kept the full name width, so the badge drew on top of the text.
* Flex layout stops laying the same item out again when nothing it
  depends on changed. A row flex item was laid out twice, once to
  measure its height and once at its final place, and a stretched item
  a third time with its new definite height; nested flex containers
  multiplied that, so a 1,500-box YouTube page took 37,000 box layouts
  per relayout. The second pass now moves the already laid out subtree
  when the width is the same and nothing inside read the item's
  definite height, and an inline-block is laid out a second time only
  when the first pass changed its size. A forced relayout of a saved
  YouTube watch page drops from 74 ms to 8 ms; the layout of 6,587
  flexbox, grid, alignment, sizing and inline WPT files is unchanged.
* Fewer styles are recomputed after DOM changes. Setting an attribute to
  the value it already has no longer restyles anything (YouTube rewrites
  the same custom properties into `ytd-watch-flexy`'s `style` attribute
  dozens of times, each restyling ~2,500 elements); removing a child
  restyles only the `:has()` anchors around it unless a structural
  selector applies; and a parent with more than 64 children is no longer
  restyled whole on every insertion, because the structural-selector
  check now scans all children with hash lookups instead of giving up.
* YouTube restyles incrementally again, about five times faster per
  forced layout. One `:has()` selector whose compound had no class, id,
  type or attribute to key on (`:not(:has(...))`, or a bare `:has(> x)`
  after a descendant combinator) turned incremental restyle off for the
  whole page, so every `offsetWidth` read recomputed all ~4000 styles.
  Such an anchor now keys on the compound that holds the `:not()`, or on
  the nearest keyed compound to its left. An anchor also has to match
  every key of its compound, not just the first: `#content.x:has(...)`
  used to match YouTube's top-level `#content` and restyle the whole app
  after any change.
* `-webkit-line-clamp` clamps. `display: -webkit-box` with
  `-webkit-box-orient: vertical` and a line clamp now lays out as a
  block container whose text stops at the clamp line with an ellipsis,
  as in Chrome, instead of a flex row that showed every line. YouTube's
  "N chapters" rows spilled their whole chapter list over the results
  below. `-webkit-box-orient: vertical` without a clamp stacks the
  children, and `getComputedStyle` reports `-webkit-box` as written.
* An empty block that starts a new formatting context (`display: flex`,
  `grid`, `flow-root`, `overflow: hidden`) stops margins from collapsing
  through it, so a later child's top margin no longer escapes to the
  parent's top edge.
* Underlines, overlines and line-throughs skip inline-blocks and images
  inside a link: an avatar inside a link no longer gets a stray line
  under it.
* SVG presentation attributes (`fill`, `stroke`, `stroke-width`,
  `stop-color`, `visibility` and the other paint attributes) take part
  in the cascade as presentational hints. A value inherited from an
  ancestor no longer beats the element's own attribute: YouTube's
  icon wrapper sets `fill: currentcolor`, which painted the red play
  button of the YouTube logo black. `getComputedStyle` reports the
  attribute value as well.
* CSS masks take several layers. The `mask` / `-webkit-mask` shorthand,
  `mask-clip` and `mask-composite` (with the legacy `-webkit-` keywords)
  are parsed per layer, and gradient layers are composited the way
  Chrome does: each layer inside its own clip box, with `add`,
  `subtract`, `intersect` or `exclude`. The common "border only"
  idiom, `linear-gradient(#fff 0 0) content-box exclude,
  linear-gradient(#fff 0 0)`, now leaves a thin rim. YouTube's buttons
  use it for their rim light, which was painted as a gradient band
  across the whole button.
* `querySelector` and `querySelectorAll` find every element whose
  selector ends in an id when the id is used more than once.
  `document.querySelectorAll('#owner #avatar')` returned nothing on
  YouTube, which repeats ids such as `avatar`, `content` and `text` in
  every component, because only the first element with that id was
  tested.
* A timer or animation frame callback belongs to the window whose
  `setTimeout` or `requestAnimationFrame` was called. When an iframe's
  script called a function of its parent that scheduled one, the
  callback was tied to the iframe and silently dropped once the iframe
  was removed, which could leave the parent's scheduler waiting forever.
* Inserting a `DocumentFragment` (`appendChild`, `insertBefore`),
  `replaceChildren()` and setting `textContent` now invalidate the
  styles that depend on sibling position. Before, an item that stopped
  being `:last-child` because a fragment was appended after it kept its
  `:last-child` style.
* The toolbar, tab-strip and address-bar icons render on systems without
  librsvg's GdkPixbuf loader (Debian/Ubuntu `librsvg2-common`), which a
  KDE or minimal desktop often lacks. GTK hands SVG icons, symbolic ones
  included, to GdkPixbuf, so without that loader every icon was GTK's
  image-missing placeholder, with no warning. The shell now draws its
  bundled icons through the in-engine SVG renderer (`src/gtk/icons.c`)
  at the screen's own scale, recolouring the symbolic ones to the theme's
  foreground as GTK did, and sets the window icon only when GTK can load
  it. Ported from Northstar (northstar-browser#16).
* The geometry APIs report an inline element's own box.
  `getBoundingClientRect()`, `getClientRects()`, `offsetHeight` and
  the rest returned the rectangle of the line a `<span>`, `<a>` or
  `<code>` sat on, so with `line-height: 1.6` a 16px link measured
  22px high and started at the line's top. They now return the content
  area of the element's font (ascent plus descent around the baseline,
  rounded as other browsers round them) plus its padding and border,
  the same box the background is painted in, and a line that mixes
  text directions is measured over all of it. That painted box also
  follows a raised or lowered `<sup>` / `<sub>`, is left open on the
  side its `direction` says when it wraps, and is no longer painted
  for a `visibility: hidden` inline. A border width without a border
  style no longer takes up room on the line.
* A printable key fires one `keydown`, not two. Embedders send a
  keydown and then a keypress (`kind` 3) for each typed character, and
  the keypress path dispatched `keydown` again before `keypress`, so
  every letter typed into a page reached its `keydown` listeners twice
  in the GTK and Java shells. Kind 3 now fires only `keypress`, as
  `libnordstjernen.h` documents, and is skipped when the keydown before
  it was cancelled.
* Clicking an `<a download>` link saves the file instead of navigating
  to it. Only a scripted `element.click()` honoured the attribute; a
  real mouse click followed the link like any other.
* Java browser: the page's own shortcuts no longer swallow the
  browser's. Alt+Left, Alt+Right and Alt+Home did nothing while the page
  had the focus, because the page view consumed every arrow and Home
  key before the window's key bindings ran.
* Java browser: Ctrl+X / Ctrl+V (and Shift+Insert) cut and paste in text
  fields, and the right-click menu of a field offers Cut, Copy and
  Paste. Text a page copies (`navigator.clipboard.writeText`, a copy
  button) reaches the system clipboard.
* Java browser: downloads work. The renderer reports a download as the
  URL and the page's suggested file name separated by a tab, which the
  Java shell handed whole to `URI.create`, so every page-initiated
  download was silently dropped. The save dialog now opens in the
  Downloads folder with the suggested name, and `data:` and local
  `file:` downloads are saved too.
* Java browser: Space pages down (Shift+Space up) when no text field has
  the focus, and the arrow, Page and Home/End keys move the caret in a
  focused field instead of also scrolling the page. Pages receive
  `keyup` for every key, `code` `"Space"` for the space bar, and AltGr
  characters such as `@` on European layouts on Windows.
* Java browser: zoom reflows the page like the GTK shell — the CSS
  viewport is the window width divided by the zoom, the zoom steps
  through the same 25–500 % ladder, and resizing the window, opening the
  find bar or showing the horizontal scrollbar relays the page out.
  Pages render sharp on HiDPI screens: frames are rasterised at the
  screen's scale factor and `window.devicePixelRatio` reports it.
* Java browser: page audio plays through `nordstjernen-audio`, which the
  shell starts beside the renderer and silences on navigation, as the
  GTK shell does. A page can take the window full screen (Esc leaves),
  a middle click opens a link in a new window, touchpad scrolling and
  Shift+wheel sideways scrolling work, animations refresh at about
  30 frames a second, and a link clicked while a page is loading is
  followed once the load finishes instead of being ignored.
* Java browser: an address typed without a scheme is normalised like
  the GTK shell's — an existing local path opens as a `file:` URL,
  `localhost` and `host:port` are hosts, and the command-line start URL
  is normalised too.
* Java library: `RemotePage.render` and `renderRgba` return the frame
  again when called twice for the same region; the second call used to
  come back fully transparent because the renderer answers an identical
  request with `X-Unchanged` and no pixels. Renderer headers are decoded
  as UTF-8. `RemoteBrowser` adds `paste`, `cutSelection`,
  `clipboardText`, `openContextMenu`, `setDevicePixelRatio` and
  `audioCommands`, and frames report `downloadName`, `clipboard`,
  `audio` and `requestedScrollX`; the new `AudioHelper` plays a
  session's audio for any embedder.
* Android: pages no longer stay blank and white after a load until they
  are reloaded. The progress bar resizes the page view as a load starts
  and ends, and every resize gives the view a fresh, white bitmap. When
  a render of the previous page was still running as the new page
  arrived, the new page's render request was dropped, so the white
  bitmap stayed on screen; animated pages such as `about:nordstjernen`
  hit this most. The request is now kept, and a frame the renderer
  reports as unchanged is copied into a bitmap that has not yet received
  it, so a view resized away and back is repainted too.
* Icon fonts keep the ligatures that start with "s". ns-pango hands
  HarfBuzz made-up glyph ids for characters a font lacks, and HarfBuzz
  14.4 stores per-lookup coverage answers under the low 14 bits of the
  id. In a font without a space glyph, the made-up id for a space was
  remembered as "not covered" under glyph 32, the letter "s" in Material
  Icons, so once any text with a space had been shaped in that font,
  `search`, `settings`, `star` and `share` were drawn as plain words
  while other icons kept working (Britannica, Google). ns-pango now
  gives HarfBuzz per-call stand-in ids below that range and writes the
  made-up glyphs back afterwards. `subprojects/ns-pango.wrap` moves to
  the ns-pango commit that carries the fix; it also brings the fork's
  shape and item cache fixes and the merge of upstream Pango 1.58.2.
* A `::before` or `::after` with text content that becomes a flex or
  grid item paints its background once, on the item box, sized by its
  `width` and `height`. The generated text painted the background again
  over its own line box, so Tripletex's hidden `width: 0; height: 2px`
  focus underlines showed as blue bars under every input.
* `@font-face` rules honour their `font-weight` and `font-style`
  descriptors. A downloaded face was registered under its CSS family
  with whatever weight and slant the font file reports for itself, so
  when one family listed several files the choice between them followed
  the files' metadata rather than the stylesheet, and a face declared
  `font-weight: 700` whose file calls itself regular was emboldened
  again on top. The declared weight and style now describe the face.
  A face without the descriptor keeps what the file reports, and so
  does the weight of a face declared with a weight range or backed by a
  variable font. FINN's category labels were drawn in a bolder face
  than the regular one they ask for, Spotify's headings in the wrong
  face of their family, and the New York Times' summaries came out
  about ten pixels wider per line than in their declared face.
* Absolutely and fixed positioned elements assigned to a shadow root's
  `<slot>` find their containing block in the flattened tree, through the
  slot and its shadow ancestors, instead of among their light-DOM
  ancestors. A slotted `position: absolute; inset: 0` scroll container
  was placed against the viewport and painted under the component's
  opaque layout, so Tripletex showed its menus but no page content.
* Elements inside an inline `<svg>` report their geometry to scripts.
  `getBoundingClientRect()` returned an empty rectangle at 0,0 and
  `getClientRects()` an empty list for every `<g>`, `<path>`, `<rect>`,
  `<use>` and `<text>`, `getBBox()` ignored the transforms of child
  elements and knew nothing about `<use>`, `<text>`, `<image>` or a
  nested `<svg>`, and `getCTM()` / `getScreenCTM()` carried only the root
  `viewBox` scale, without `preserveAspectRatio`, element transforms,
  nested viewports, the root's padding and border, CSS transforms or page
  scroll. All of them are now measured by the SVG renderer itself, with
  the transforms, `viewBox` fitting, `<use>` and `<switch>` handling it
  paints with, so a script that positions a tooltip, measures a chart
  label or checks whether an icon is on screen gets the box that is
  drawn. A path arc that ends where it starts is skipped instead of
  drawing a stray curve, and CSS rotations by a multiple of 90 degrees
  use exact sines and cosines.
* SVG `systemLanguage` is honoured. An element whose `systemLanguage`
  list names none of the user's languages is no longer drawn, and
  `<switch>` picks the first child whose language matches instead of
  always the first one, so a translated diagram shows the reader's
  language rather than whichever translation comes first in the file.
* Clicking into the address bar selects the whole address, as in other
  browsers; a second click places the cursor, and dragging still selects
  part of the text.
* The title bar shows the version after the browser name
  ("Nordstjernen Browser 1.0.29"; just "Nordstjernen" when space is short).
* 32-bit builds: the selector-cache hash folded a pointer-sized value
  with `>> 32`, which is undefined on i386 and put every key in one
  bucket, so each lookup scanned the whole cache. The fold is now done
  in 64 bits. minimp3's short-block scalefactor loop states its array
  bound, so a 32-bit LTO link no longer fails `--werror` on
  `-Wstringop-overflow`. Ported from northstar-browser.
* No more 64-bit format and pointer-type warnings on macOS in the
  IndexedDB, Temporal and WebAssembly code. Ported from
  northstar-browser.
* `<picture>` is an ordinary inline element again and the `<img>` inside it
  is the image, as in other browsers. The image used to be laid out with
  the `<picture>` element's style at its natural size, so the `<img>`'s own
  `width`/`height` attributes and CSS (`width: 100%`, `max-width`,
  `position: absolute`, ...) were ignored: hero images covered headlines,
  card images grew to twice their height and product shots were squeezed
  instead of cropped. A `<picture>` without an `<img>` now renders nothing.
  `<source>` selection is unchanged.
* An inline `<svg>` that has only a `viewBox` (no `width`/`height`) no
  longer stretches the box around it. Floats, inline-blocks, buttons,
  absolutely positioned boxes, `auto` grid tracks and table cells measured
  it at the full width of the page, so icon buttons became hundreds of
  pixels wide and drew their icons as huge black shapes. Such an SVG now
  adds no width to its container's min- and max-content size, as in other
  browsers, takes its width from a definite `height` and its aspect ratio,
  and still fills the line of a flex container or an ordinary block.
* An image, video or SVG with a percentage width (`width: 100%` and the
  like) inside a box that sizes itself to its content -- a float, an
  inline-block, an absolutely positioned box, an auto table column, a
  flex or grid item -- adds its natural width to that box's max-content
  size and nothing to its min-content size, as CSS Sizing specifies (an
  SVG without a natural width counts as 300px here, a definite height
  and aspect ratio take precedence). It used to add the percentage of the
  page width, so such a box grew to the full width and the image was
  drawn at that width.
* A `::before` or `::after` with `display: block` (or `flex`, `grid`)
  and text content is a block box of its own. It used to be laid out as
  inline text at the start of the element's first line or the end of its
  last one, so it did not start a new line, and its width was added to
  the text's in shrink-to-fit sizing instead of the wider of the two being
  taken. Tabs that reserve their bold width with a hidden block `::before`
  grew by about half, and headings and counters drawn with block pseudo
  content ran into the text beside them.
* Table cells (`<td>`, `<th>`) get their `::before` and `::after`
  generated content. It was never built for cells, so sort arrows,
  `content: attr(data-label)` labels in responsive tables and icons drawn
  in a cell's pseudo-element were missing.
* CSS grid placement by name follows the spec. `grid-column: content`
  finds the lines named `content-start` and `content-end` (the implicit
  area that line names create), the end edge of `grid-row: top / nav` or
  `grid-area: a / 1 / b / -1` resolves to `nav-end`/`b-end` instead of
  the area's start line, and an item with only an end line
  (`grid-row-end: main`) is placed against it. Such items used to fall
  back to auto-placement, which squeezed the Guardian's front-page
  containers into 60px columns.
* Grid rows are sized from single-row items first and then from spanning
  items, smallest span first, and a spanning item's extra height goes to
  rows that have not reached their content height limit before it is
  shared out evenly. An empty row next to a spanning header no longer
  takes half of the header's height.
* `flex` with only a basis (`flex: 100%`, `flex: 30px`,
  `flex: calc(...)`) sets `flex-grow: 1` and `flex-shrink: 1`, as the
  shorthand defines. The grow factor stayed 0, so such items did not grow
  into the free space of their flex line.
* A single-value `overflow` (`overflow: hidden`) now sets `overflow-x`
  and `overflow-y`, so it takes part in the cascade like any shorthand.
  Any `overflow-x`/`overflow-y` from another rule used to win over it,
  even when the `overflow` rule was `!important` or more specific, so
  `overflow: hidden !important` utility classes did not clip.
  `element.style.overflowX` and a rule's `style.overflowX` read back the
  value as well.
* A custom property whose value is `var(--b, initial)` (or `inherit`,
  `unset`) sees `--b` when `--b` is a plain value declared later in the
  same rule or in the inline style. The fallback keyword used to win and
  the property was treated as unset, so Netflix's card grid lost its
  `flex-basis` and stacked the cards.
* Form controls get the default look other browsers give them:
  `<button>`, `<input>` and `<select>` use a 13.33px system font and
  `<textarea>` a 13.33px monospace font instead of the page's font, buttons have `1px 6px` padding and a
  2px border, and `<select>` and `<textarea>` a 1px border with Chrome's
  padding. Unstyled buttons and search boxes were drawn in the page's
  16px serif with wider padding, and fixed-size icon buttons left too
  little room for their icon.
* A `<video>` whose metadata has not loaded uses the 300x150 default
  object size when only its width or only its height is set, as in other
  browsers. `<video style="width: 100%">` without a loaded source used to
  be as tall as it was wide, a square blank box that pushed the page down.
* A wrapping flex container (`flex-wrap: wrap`) with a percentage height
  resolves it against its containing block's height. It used to take the
  percentage of its own width, so a one-line header with `height: 100%`
  became as tall as it was wide and `align-items: center` pushed its
  items hundreds of pixels down.
* An absolutely positioned box placed at its static position inside a
  flex or grid container, or in a right-to-left block, now moves with its
  content. Only the box itself was moved, so its text and children stayed
  where block layout had first put them: search placeholders and icons
  were drawn below their input.
* Table cells with `display: none` no longer take a column. They were
  still built into their row, so a hidden responsive header cell
  (`colspan="2"`, `width: 100%`) widened GitHub's file table past its
  container and pushed the commit message and date columns out.

1.0.29:
======
* The about:nordstjernen splash shows version 1.0.29.
* Clipboard in text fields: paste with Ctrl+V, Shift+Insert or Cmd+V on
  macOS, and from a new right-click menu on inputs, textareas and
  contenteditable elements (Cut, Copy, Paste, Select All). Ctrl+X cuts and
  Ctrl+C, Ctrl+Insert and Select All act on the focused field's text
  instead of the page. Pasting fires a cancelable `paste` ClipboardEvent
  whose `clipboardData` holds the text, honours `readonly` and `maxlength`,
  and turns line breaks into spaces in single-line inputs. Password fields
  cannot be copied or cut. `beforeinput`/`input` are now InputEvents with
  `inputType` and `data` for typing, deleting, pasting and cutting, and
  `DataTransfer` treats the `text` and `url` formats as `text/plain` and
  `text/uri-list`.
* Fixes for the HiDPI, layout and smooth-scrolling changes:
  - Keyboard scrolling (arrows, Page Up/Down, Space, Home/End) moves the
    page again instead of the scroll container under the mouse pointer.
  - On scroll-snapping pages one wheel notch or key press moves one snap
    point; the eased scroll used to skip about a dozen sections.
  - Zooming out below about 35% no longer lays the page out narrower and
    shorter than the window.
  - A page restored with Back after the window moved to a screen with a
    different pixel density picks up the new density, `resolution` media
    queries re-apply when the density changes, and out-of-range device
    pixel ratios are clamped instead of ignored.
  - Absolutely positioned children of 3D-transformed elements are drawn
    inside the transformed element again.
  - `vw`/`vh` inline styles read back from `element.style` follow
    viewport resizes; `getComputedStyle()` on a `display: none`
    `::before`/`::after` returns its real values.
  - A crafted WOFF2 file can no longer decode to a font of over 64 MB.
  - A float in the first paragraph of a block whose top margin collapses
    through (common in blog posts: `<p><img style="float:left"></p>`) no
    longer drops below the text beside it, and blocks below every float
    no longer rebuild the float bands (pages with many floats laid out up
    to seven times slower).
  - `<hr>` beside a float stops at the float's edge again (UA
    `overflow: hidden`, as in other browsers); column-flow grids measure
    their min-content with their own font; very large `line-height`
    values no longer overflow to a zero-height paragraph.
  - Each tab in single-process mode no longer commits its whole
    framebuffer up front, and the shell rejects frames larger than it
    asked for.
* Pages are drawn at the screen's real pixel density. On a Retina or other
  HiDPI display the renderer used to paint at one device pixel per CSS pixel
  and the window stretched the frame, so text and images were blurry. The
  frame is now rendered at the window's scale factor (2x on a Retina Mac,
  fractional scales on Linux) and shown pixel for pixel, re-rendering when
  the window moves to a screen with another scale. `devicePixelRatio`,
  `resolution` and `-webkit-device-pixel-ratio` media queries and `srcset`
  report the real ratio, so pages pick their sharp images. The renderer
  framebuffer limit grows from 2560x1600 to 6144x3456 device pixels, so wide
  windows are no longer cut off at 2560 CSS pixels.
* Text flows around floats like in other browsers: a paragraph next to a
  floated figure keeps its full width and only its lines beside the float
  are shortened, so text returns to the full width below the float. The
  whole paragraph used to be narrowed for its entire height, which made
  Wikipedia articles about 50% taller than in Chrome. Floats also no longer
  get their top margin twice, and a float before a block whose top margin
  collapses through the parent moves down with that margin.
* Absolutely positioned and `display: none` table rows are taken out of the
  table. Wikipedia hides collapsed table rows that way, and they used to
  stay in the grid at full height.
* Definite `min-width` and `max-width` limit a box's min- and max-content
  width, and the flex items of a single-line row contribute their min-content
  width as the flex spec describes (clamped by their flex base size when
  they cannot grow or shrink). A centered flex column holding a heading with
  `max-width` no longer stretches to the full width.
* Lit and other web components that define their reactive properties in an
  `observedAttributes` getter work: `customElements.define` now reads
  `observedAttributes` once at definition time, as the spec says, instead of
  after the first element was constructed. Lit components on MDN never
  re-rendered, so every dropdown menu in MDN's header stayed open.
* A `display: contents` shadow host lays out its shadow tree, so content
  assigned to a hidden `<slot>` is not drawn.
* `visibility: hidden` hides `::before`/`::after` text, and in flex and grid
  containers those pseudo-elements become their own items. Stack Overflow's
  menu labels ("About", "Active") were drawn twice.
* Percentage widths of inline-blocks resolve against the line's containing
  block during layout. The real layout sized them correctly and then laid
  them out again against their own content width, so a `width: 50%`
  inline-block came out a few pixels wide (apple.com's region picker showed
  "..").
* `display: none` on `::before` and `::after` removes the pseudo-element,
  as when another rule sets its `content`.
* `::placeholder` honours `opacity` and `visibility`, and keeps the default
  grey (#757575) when a rule changes other properties. GitHub's sign-up
  field drew its hidden placeholder in black on top of its label.
* Reading an element's inline style from script is much faster:
  `style[i]`, `style.length`, `getPropertyValue()` and
  `getPropertyPriority()` no longer re-serialize and re-parse the whole
  `style` attribute on every call. bbc.com/news loads in about 6 s of CPU
  instead of 33 s; in the desktop browser it used to pass the renderer's
  30-second reply timeout, restart, and never finish loading.
* Scrolling is smooth. Touchpad deltas are applied 1:1 (they were multiplied
  by 60, so a small swipe jumped hundreds of pixels), the scroll continues
  with momentum after the fingers lift, mouse-wheel notches and the arrow,
  Page Up/Down, space, Home and End keys glide to their target, and each
  scroll rides on the next frame request instead of a separate round trip.
  The browser no longer re-runs a hover hit test after every frame while
  the pointer is still (GTK repeats the last motion event), and holds hover
  updates until scrolling stops; hovering tests only the exact point under
  the pointer. Scrolling Wikipedia went from about 9 to about 55 frames per
  second on a 2x display. Wheel and touchpad scrolling also lets the
  renderer delay page timers and relayouts while the gesture lasts, as it
  already did when the scroll position came from the browser window.
* A single-line row flex container is as wide as the sum of its items'
  min-content widths, not its widest item, when it is squeezed. GitHub's
  header menu shrank below its content and the buttons ran into each other.
  Text under `white-space: nowrap` or `pre` is never measured as if it could
  wrap, including text with letter or word spacing.
* A `grid-auto-flow: column` grid is as wide as the sum of its columns, so
  button rows like apple.com's "Learn more / Buy" are no longer cut off.
* Paint order follows stacking contexts: an element with a positive
  `z-index` inside a positioned `z-index: auto` box is painted above later
  positioned siblings, and flex and grid items with a `z-index` are lifted
  even without `position`. apple.com's hero headline and buttons were
  hidden under the hero image.
* Text uses the same fonts and widths as Chrome on macOS. `sans-serif` is
  Helvetica (it was Verdana, about 14% wider, so text wrapped earlier and
  overflowed its boxes), `serif` is Times, `monospace` Menlo, `cursive`
  Apple Chancery, and `system-ui`, `-apple-system` and `BlinkMacSystemFont`
  the San Francisco system font. Glyph advances are no longer rounded to
  whole pixels, and font sizes reach HarfBuzz as CSS pixels, so San
  Francisco gets the same optical size and tracking as in Chrome; measured
  string widths now agree with Chrome to a tenth of a pixel. `line-height:
  normal` comes from the font's own ascent, descent and line gap.
* WOFF2 web fonts load on systems whose FreeType was built without Brotli,
  such as Homebrew's on macOS. Those fonts used to fail silently and pages
  fell back to a default face (GitHub's Mona Sans showed as Verdana). The
  new in-tree decoder `src/woff2.c` unpacks the Brotli stream and rebuilds
  the transformed `glyf`, `loca` and `hmtx` tables over libbrotlidec.
* Every named instance and the variable pattern of a web font are now
  registered under its CSS family, so a variable font such as Mona Sans
  renders at the requested weight instead of its first instance.
* Zoom reflows the page like other browsers: at 150% the page is laid out
  for a viewport 1.5 times narrower instead of being magnified and cut off
  at the right edge.
* Security audit of the engine, shell and helpers. Fixed memory-safety
  bugs that web content could reach:
  - Out-of-bounds reads and writes: WebGL texture uploads sized for the
    wrong pixel format or ignoring the unpack state; a grid
    `repeat(auto-fill, ...)` track count past the stored tracks; CSS
    per-rule match arrays one slot short for `::file-selector-button`;
    `container-name` lists of 16+ names; word selection over text with
    lone surrogates; Ogg Vorbis links and libav frames whose channel
    layout changes mid-stream in the audio helper; shared-memory video
    geometry re-read after it was validated in the shell.
  - Use-after-free when page script runs in the middle of an operation:
    custom element upgrades and `whenDefined`, `replaceChildren` and
    template `innerHTML`, radio groups, inline event handlers,
    `addEventListener` with an AbortSignal getter, EventSource dispatch,
    structured clone and worker messages over resized or detached
    buffers, `getAnimations()`/`getKeyframes()`, performance entries and
    observers, IndexedDB handles, canvas state and paths, WebAssembly
    memory buffers passed to `transfer()`, WebCrypto ECDH peer keys,
    scrollbar and headless hit-testing, and queued scripts or document
    index entries for removed nodes.
  - Stack exhaustion from deep nesting: `:nth-child(of ...)`, selector
    chains, `@supports`, `@container`, `image-set()`, time values,
    `random-item()`, nested grids and inline runs. CSS nesting can no
    longer expand a small stylesheet into gigabytes of selectors.
  - Integer overflow and runaway sizes: textarea rows, fitted input
    columns, multicol splits, animation delays, SVG arcs and sizes, PDF
    page rasters, print spans, Web Audio and microphone buffers.
* More fixes from the security audit:
  - `fetch()` and message delivery take the requesting document from the
    engine, not from `this.location`, which a page could forge to reach
    about: pages, local files or another site's cookies.
  - Engine-private `data-nd-*` attributes can no longer be read or written
    through `toggleAttribute`, `setAttributeNode`, `Attr.value` or the
    namespaced attribute methods; a page could otherwise make
    `<input type=file>` read any local file the renderer can open.
  - The unused `__ndDocEnter`/`__ndDocExit` globals are gone.
  - WebGL validates draws against the GL driver's own buffer and
    vertex-array state, keeps the calling context current while page
    `valueOf` code runs, and reads the canvas back without the page's pack
    settings; WebGPU keeps borrowed handles and mapped ranges alive while
    in use.
  - Use-after-free fixes for live collections, IntersectionObserver and
    ResizeObserver callbacks, Attr wrappers, form validation, pointer lock,
    `scrollIntoView()`, `document.scripts`, `document.body`, table row and
    cell insertion, `hashchange`, frame creation, and DOM insertion methods
    whose custom element callbacks move nodes; freed nodes no longer stay
    the `:active`/`:hover`/`:focus` element.
  - Media elements only send engine-issued tokens to the audio helper, and
    long chains of safe-browsing "continue" prefixes no longer recurse.
  - Titles and URLs with invalid UTF-8 can no longer inject markup into
    about:history. An iframe's internal realm document and window are no
    longer reachable as properties of the iframe element, so a
    cross-origin frame's DOM stays out of the embedding page's reach.
  - Extension content-script natives are removed from the page even when
    the content script fails; `DecompressionStream` output per chunk is
    capped at 256 MiB; assigning to read-only accessors such as
    `element.dataset` no longer crashes; error reports copy the file name
    before running the thrown value's `toString()`; orphaned nodes are not
    swept while page script is still on the stack, and freed nodes are
    dropped from video-frame callbacks and the pending fullscreen target.
* The shell and helpers trust the sandboxed renderer less: media helper
  commands with line breaks or over-long lines are refused, saved pages
  are not copied through symlinks, cursor names are limited to CSS
  keywords, renderers no longer inherit other tabs' descriptors, and
  `file:` downloads from web pages are refused.
* Sandbox: Landlock now also restricts creating symlinks, fifos, sockets
  and device nodes, and restricts truncation; it picks the rights the
  running kernel supports instead of failing on older kernels. Seccomp
  refuses the TIOCSTI/TIOCLINUX terminal-injection ioctls.
* Web security: subresource redirects keep their top-level site (cookies,
  `Sec-Fetch-Site`, `Origin`); `document.cookie` can no longer replace
  HttpOnly cookies; about:blank/about:srcdoc no longer count as browser
  pages; HSTS applies to mixed-case URLs; the nghttp2 backend refuses
  request methods containing line breaks and caps response headers at
  1 MB; frame documents read cookies for their own URL; IndexedDB is
  refused for opaque origins (file:, data:, about:) instead of sharing
  one database between them; extension content scripts keep their
  privileged natives away from the page.
* Pages load again in the desktop browser on macOS versions whose
  `shm_open()` rejects `O_CLOEXEC`. Creating a tab's shared framebuffer
  failed there, so no renderer process was started and every page stayed
  blank with "Done" in the status bar. The framebuffer is now opened
  without the flag and marked close-on-exec afterwards in that case.
* Event listeners are indexed by their target. Dispatching an event to a
  node, dispatching to the window, and checking for a duplicate in
  addEventListener each walked every listener of the page; they now look
  only at the target's own listeners.
* IntersectionObserver entries are only built for targets whose
  intersection changed. Every relayout built a full entry with three
  DOMRects for every observed target and dropped most of them.
* The animation check after each style pass skips elements that cannot
  have anything to do: those without animation state and without an
  animation or transition property, and those whose style was reused and
  have nothing running. It no longer computes their depth and sorts them.
* Style elements inside shadow roots and frames are no longer rewritten on
  every relayout. Their CSS is flattened and scoped to the host, which is a
  pure function of the text and the host, so the result is now kept by
  that text. Pages with many such style elements relayout faster
  (Speedometer's Svelte-Complex-DOM 204 ms to 115 ms).
* Relayouts measure text once per distinct run instead of on every pass.
  The size, line count and baseline of a measured text layout are kept,
  keyed by its text, font, attribute list and layout settings, so a forced
  layout after a small change no longer shapes and breaks every paragraph
  of the page again. Speedometer's NewsSite layout time halves.
* Pages that repeat the same `<style>` many times, such as inline SVG
  icons that each carry their own style element, restyle much faster.
  Consecutive style elements are merged into one sheet, and every copy of
  a rule was matched against every element it could apply to; an earlier
  copy can never win over a later identical one, so only the last copy of
  each repeated style text is kept. A Speedometer Complex-DOM page has 416
  copies of one icon style, 98% of its selector matching.
* Shadow DOM pages restyle faster. Style sheets in shadow roots are scoped
  by an attribute on the host, so their rules read `[data-nd-host="7"] .x`;
  the ancestor filter that skips rules whose ancestors are not on the
  element's path ignored attributes, and every host's copy of a rule was
  matched in full against every element. Exact attribute values in ancestor
  position now go into the filter too. Speedometer's TodoMVC-WebComponents
  style pass takes 57 ms instead of 221 ms.
* Placing absolutely positioned boxes nested in other positioned boxes is
  faster: finding each box's static position no longer walks up the tree
  for every box it visits, but compares document order ranks and a set of
  the box's ancestors made once.
* Large blurred box shadows paint up to 250 times faster. A blurred rounded
  rectangle is the same along its straight middle, so the blur now runs on a
  copy with the middle cut short and the uniform row and column are
  repeated back to full size. TodoMVC's 550 by 6000 pixel list shadow took
  300 ms per paint and now takes about 1 ms. The pixels are unchanged.
* Blurred box shadows are drawn once and reused. Every frame blurred every
  visible shadow again, three passes over a fresh surface each; the blurred
  surfaces are now kept, keyed by size, corner radii, blur and colour, up to
  32 MB.
* Loading a frame no longer measures the whole JavaScript heap. The frame's
  `performance` object needs to know whether the parent has
  `performance.memory`, and reading it to find out walked every object of
  the runtime.
* Event dispatch no longer walks every listener of the page after each
  target it visits to clear out removed listeners when none was removed.
* Dispatching an event no longer searches the document. Looking up the
  window's `on<type>` handler fell through to the window's named
  properties whenever the page had not set one, which walked the whole
  document for a frame and then an element with that name, for every
  event.
* Relayouts skip the transition and animation check for elements whose
  style was reused unchanged and that have nothing running. An element with
  `transition: all` compared every transitionable property on every
  relayout.
* Restyles after a DOM change are much faster on pages whose elements
  declare `transition`. Every element that had ever been checked for a
  transition was restyled on every relayout, with its whole subtree, even
  when nothing was animating; now only elements whose style an animation
  or transition actually changed are. Speedometer's Complex-DOM suites run
  two to five times faster (Svelte-Complex-DOM 748 ms to 244 ms).
* The title bar always shows the browser name. "Nordstjernen Browser" sits
  centred in the window, moves to the right edge once the tabs reach the
  middle, and shortens to "Nordstjernen" when the full name no longer fits.
* The Nordstjernen "N" logo is back at the right end of the toolbar; clicking
  it opens nordstjernen.org.

* `flex: unset`, `flex: inherit` and the other CSS-wide keywords reset all
  three flex longhands. `flex-basis` kept its old value, so GitHub's
  security section drew its screenshot as an 18px wide sliver.
* Auto margins work in wrapping flex containers (`flex-wrap: wrap`): free
  space on each line goes to the items' `auto` margins, and `margin-top` or
  `margin-bottom: auto` aligns an item in its line. MDN's Baseline box
  showed the browser icons next to the label instead of at the right edge.
* Links, labels and summaries with `display: inline-block` are laid out as
  inline blocks, with their vertical padding, instead of as plain text.
  MDN's table of contents links were 19px tall instead of 32px. Inline
  blocks and images with a horizontal margin are no longer moved right by
  that margin a second time.
* Margins collapse as in other browsers. When a first child's top margin
  collapses through its parent, the parent's box now starts below that
  margin, so its background and border no longer cover it. A top margin
  also collapses with the margin above the parent, and flex and grid items
  keep their children's margins inside. The root element keeps its
  children's margins too, while `<body>` lets its last child's bottom
  margin collapse through it. MDN's page header was 16px shorter than in
  Chrome.
* Grid sizing: `minmax()` tracks keep their minimum when a grid with
  flexible columns is too narrow (MDN's three columns overflowed the
  window), a stretched grid item stops at its `max-height`, and `0fr` or
  `0.5fr` rows in a grid without a fixed height get that fraction of their
  content. The `grid-template-rows: 0fr` accordions on github.com showed
  every closed panel open. Flex and grid items with `overflow: hidden` now
  clip their content even when they are zero pixels tall.
* Each line box takes the `line-height` of every inline element on it, as
  CSS describes. A `<span>` with a larger font or line height inside a
  paragraph made its lines taller when drawn but not in the layout, so the
  text overlapped the next block; GitHub's feature descriptions were 9px
  short. A line made only of smaller text keeps the paragraph's line height.

* A box shadow without blur is drawn only outside its box. A shadow
  shifted sideways also showed a strip inside the box on the opposite
  edge, so MDN's table of contents links had a grey line on both sides.

* Solid borders whose sides have different colours meet in a diagonal
  mitre, as in other browsers, so CSS triangles drawn with transparent side
  borders show as triangles. Stack Overflow's "More" dropdown arrow was a
  black bar.

* An absolutely positioned `::before` or `::after` with `content: ""`
  no longer draws its borders and background a second time around an empty
  line of text next to the element.

* A box with `aspect-ratio` and an explicit `min-height` (or one that
  scrolls vertically) takes the height from its ratio even when its content
  is taller, as the spec's automatic minimum size describes. Content still
  grows such a box when `min-height` is `auto`.

* The experimental V8 JavaScript backend is removed. The `js_engine` and
  `v8_root` build options, `src/js_v8.cc`, docs/V8.md and the V8 CI job are
  gone; every build binds the DOM to QuickJS, with `-Dquickjs` still picking
  the in-tree quickjs-ng fork or Bellard's original QuickJS.

* `-Dquickjs=quickjs` builds again. The binding had started calling four
  quickjs-ng functions that the adapter for Bellard's original QuickJS did
  not provide (`JS_ToNumber`, `JS_GetClassCount`, `JS_NewForwarder`,
  `JS_CloneCFunction`), so `src/js.c` no longer compiled there. The adapter
  now supplies them; a frame's copies of native functions forward to the
  page's functions on that engine, as docs/quickjs.md describes.

* The window hands each page frame to GTK as a texture instead of painting
  it through cairo on every redraw. GTK had to rasterize the whole frame
  into a new image and upload it each time the window redrew, also when
  only a wheel event or a video frame changed; it now uploads a frame once.
  Scrolling a results page in a 1200x760 window on a 2x screen draws 51-54
  frames a second instead of 44-46, and the window's memory stays around
  100-130 MB instead of 120-250 MB.
* Scrolling no longer waits for the renderer. The renderer paints the page
  as tiles and keeps each `position: fixed` and `position: sticky` part in
  its own layer; the window keeps the tiles as textures and moves them
  itself, so a long task in the page's scripts no longer stops the page from
  moving, and only tiles that come into view are uploaded. After a change on
  the page only the visible tiles are painted again, and tiles whose pixels
  did not change stay on the GPU. A playing YouTube video is drawn under the
  tiles. Scrolling at 1500 px/s in a 1280x800 window on a 2x, 120 Hz screen:
  Wikipedia 56 -> 102 frames a second, GitHub 51 -> 102, BBC News 42 -> 100,
  a YouTube video page 38 -> 95, and the longest pause on the YouTube video
  page went from 509 ms to 25 ms. Pages that cannot be split this way (for
  example `background-attachment: fixed`) still get full frames;
  `NS_TILES=0` turns tiles off.
* A grid item can span more than 24 rows. `grid-row: span N` and the other
  span forms were cut to 24, the limit on explicit tracks in a template,
  although implicit rows go up to 4096. Google's image results lay their
  tiles out on 5px rows with spans of 25 to 100, so every tile was 24 rows
  tall, the images were cropped and the captions were drawn over them.
  Column spans keep the 24 track limit.
* Web fonts used only by `::before` and `::after` content are downloaded.
  The window decided which `@font-face` files to fetch from the text nodes
  in the document, so an icon font that a page only uses through generated
  content (Britannica writes every icon as `content: attr(data-icon)`) was
  never loaded, and the icon names were drawn as plain words.

* Signing in to Google works: Gmail opens after the email and password
  steps, where the sign-in page used to turn the browser away with "This
  browser or app may not be secure" after the email. Methods and
  attributes of Node, Element, Document and the other node interfaces now
  check that `this` is a real node, as WebIDL requires: called on any other
  object they throw a TypeError ("Illegal invocation"), the promise-returning
  ones reject, and the `[LegacyLenientThis]` handlers `onmouseenter`,
  `onmouseleave` and `onreadystatechange` return undefined. Before, they
  quietly returned null or 0, or ran on the wrong object, and Google's
  sign-in checks for this. `HTMLImageElement.decode()` called on something
  that is not a node now rejects at once instead of never settling.
* `meson setup` checks every hard system-library version floor -- libcurl
  8.5; with ns-pango, GLib 2.80, Cairo 1.18, HarfBuzz 8.3, FriBidi 1.0.6
  and fontconfig 2.15; with `-Dgtk=enabled`, GTK 4.14 -- before it
  configures ns-pango, and names each library that is missing or too old
  in one error. On a release older than Ubuntu 24.04 the build used to
  stop inside the ns-pango subproject on whichever floor it met first, as
  a bare GLib version mismatch. The error also points at
  `-Dns-pango=disabled`, which shapes through the system Pango instead.
* The release ships one .deb per supported distro release -- Ubuntu
  24.04 LTS, Ubuntu 26.04 LTS and Debian 13 -- named for the release it is
  for (`nordstjernen_<version>_ubuntu24.04_amd64.deb`,
  `..._ubuntu26.04_...`, `..._debian13_...`). The single .deb was built on
  Debian 13, so it depended on that release's FFmpeg 7.1 packages
  (libavcodec61, libavformat61, libavutil59, libswscale8, libswresample5)
  and on libwebp 1.5, and dpkg refused to install it on Ubuntu 24.04
  (FFmpeg 6.1, libwebp 1.3.2) and Ubuntu 26.04 (FFmpeg 8.0). FFmpeg puts
  its ABI in its package names, so no single package can depend on every
  release's FFmpeg. Each .deb is now built in a container of its own release
  against that release's libraries, then installed there and started
  headless before it is attached to the GitHub release.
  scripts/pack-deb.sh takes the release tag for the file name from
  /etc/os-release (`DEB_DISTRO_TAG` overrides it). (Reported by Danik-a-a.)

1.0.28:
======
* Scrolling no longer waits behind the page's own work. While a page is
  animating, the window asked the renderer for a page tick every frame and
  put those requests ahead of everything else, so the frames that carry the
  wheel movement waited behind them; while the user scrolls it now asks for
  frames, which run the page's work at a lower rate themselves.
* `Map` and `Set` with number keys are fast again. Small integers hashed to a
  handful of buckets, so a `Set` of a few thousand numbers searched long
  chains on every `add`/`has`/`set` (100,000 operations took about 180 ms,
  now about 5 ms).
* Pages with container queries restyle incrementally again: after a small
  DOM change only the affected elements get new styles, instead of all of
  them. Incremental restyle was turned off whenever container sizes were in
  use, which on Google's results page was every relayout; a style write
  followed by a layout read on a 4,800 element container query page takes
  35 ms instead of 45 ms.
* Scrolling stays smooth on pages that react to it. While the user scrolls,
  frames are painted from the current layout and the page's timers, scroll
  events and relayouts run at most every 250 ms (longer when they are slow);
  before, every frame waited for them, and a page that restyles its header
  on scroll, as Google's results page does, dropped to a few frames per
  second. The page sees the final scroll position as soon as the user stops.
* A grid that gets its height from `top` and `bottom` (absolutely positioned,
  no `height`) sizes its `fr` rows from that height; they were 0 px tall, so
  the items collapsed or took their content height (the picture mosaic at
  the top of a Google results page showed one picture and an empty tile).
* Percentages inside `round()`, `mod()`, `rem()` and `abs()` resolve against
  the same basis as a plain percentage. They were resolved against the
  viewport width when the style sheet was parsed, so
  `width: round(nearest, 100%, 1px)` made a box as wide as the window
  whatever its container (Google's results page laid out its header, its
  top cards and its news list too wide, over the right-hand column).
* A column flex container with `min-height` and no `height` is as tall as
  its content again; the minimum was used as the height, so the items were
  shrunk to fit it and the rest of the page was laid out over them
  (DuckDuckGo's comparison page drew its table over the introduction).
  `overflow: clip` on a flex item no longer lets it shrink below its
  content, since it does not make a scroll container.
* Relayouts of pages with container queries are about 17% faster when the
  containers keep their size (measured on a 6,000 element page relaid out
  after each style write): the selector match cache that serves the second
  container pass is no longer filled and thrown away on relayouts that need
  no second pass. It is used again as soon as a relayout needs one.
* The about:start splash shows version 1.0.28.
* Nordstjernen is now dual-licensed: it may be used, modified and
  redistributed under either the Nordstjernen Source License v1.0 or the GNU
  General Public License version 3 or (at the recipient's option) any later
  version (`LicenseRef-NSL-1.0 OR GPL-3.0-or-later`). The GPL text ships as
  `COPYING` in every package and shows at `about:gpl`; the About page and the
  Debian,
  RPM, openSUSE, Alpine, Maven and AppStream metadata declare both licenses.
* Style resolution is faster on pages with attribute selectors: whether an
  attribute's value compares case-insensitively is decided once per selector
  instead of for every element (Speedometer's Complex-DOM suites run about 18%
  faster).
* Scripts with many functions parse faster (about 9% on a 1.2 MB script of
  40,000 small functions): a nested function now shares its script's file
  name instead of looking the URL up again in the atom table.
* Reading any property of a `<form>` no longer rebuilds the list of its
  controls: forms resolve control names before their own members, so every
  `form.action` or `form.q` walked the document for the form's controls. The
  list is now cached until the DOM changes, about three times faster on a
  form with a hundred controls. Changing a control's `name` or `form`
  attribute now also updates `form.elements` and `getElementsByName()`
  collections a page already holds, which kept the old members before.
* Pages with container queries relayout in one pass instead of two when no
  container changed size: the previous relayout's container sizes are used
  for the first cascade and checked against the new layout, and only a
  change in a container's size or position among its siblings runs the second
  pass. Google's results page ran the second pass on 44 of its 45 relayouts.
* Constructed style sheets adopted by a shadow root style it again. The
  engine scopes them like a `<style>` at the end of that shadow root, so
  `:host` works, `innerHTML` set later no longer drops them,
  `adoptedStyleSheets.push()` takes effect, and a frame's adopted sheets no
  longer pile up in the top document (they made every later relayout slower;
  Speedometer's later suites ran several times slower than alone).
* `instanceof` on an interface that extends `Node`, `Element`, `HTMLElement`
  or `Document` is the ordinary prototype chain test: every HTML element was an
  instance of `SVGElement`, `HTMLUnknownElement` and `HTMLMediaElement`, so Vue
  rendered whole apps as SVG elements without `click()` (Speedometer's
  TodoMVC-Vue, TodoMVC-Vue-Complex-DOM and NewsSite-Nuxt failed). MathML
  elements are `MathMLElement`s, an HTML element named with upper-case letters
  through `createElementNS()` is an `HTMLUnknownElement`, and the events the
  engine fires (submit, wheel, keyboard, touch, drag, animation, transition,
  WebSocket close, EventSource, media query change, offline audio completion)
  have their interfaces. Frame realms get their own `customElements`,
  `WindowProperties` and stream objects, and named access on a kept frame
  window follows its navigations.
* A box that sizes itself to its content (flex, inline-block, float) counts a
  child's `calc()` width. Wikipedia's "Checked" review indicator came out
  20 px too narrow, its icon shrank to half width and the text ran into the
  lock icon.
* A grid item that spans a `1fr` row in a grid without a set height grows
  only that row, not the `min-content` rows it also spans. Wikipedia's
  Appearance sidebar spans the title, tab bar and article rows, and pushed
  large blank gaps above and below the Article/Talk tabs.
* A page with many shadow roots no longer crashes the renderer. The cache of
  parsed `<style>` sheets dropped its oldest entries when it grew past 64,
  even sheets the relayout in progress was still using (Speedometer's
  TodoMVC-WebComponents crashed as soon as it added its items).
* A style sheet styles only its own document. The page's rules no longer
  apply to the elements of its frames, and a frame's `<link>` style sheets no
  longer apply to the page.
* `loadedmetadata`, `loadeddata` and `canplay` fire once per video resource
  instead of after every layout, and `progress` fires only when the buffered
  range grows. YouTube got them several times a second and ran its player
  handlers for each one.
* A `<video>` that switches to a new source stops getting events from the old
  one. After a YouTube ad the old stream kept sending its buffered range,
  position and `ended` to the element, so the player saw the video stall,
  stopped at 0:00 and reloaded it at 144p.
* A page that registers one custom property with `@property` no longer
  makes every element that sets its own custom properties copy all the
  inherited ones. YouTube registers one and carries 1361 variables, so each
  forced layout on the watch page took 444 ms; it now takes 56 ms.
* On macOS the sandboxed renderer can create its profile directories when
  their parents do not exist yet, so history and IndexedDB work on a fresh
  profile (before, every `indexedDB.open()` failed with `UnknownError`).
* The fetcher looks for a bundled CA certificate file next to the browser
  through the executable path the app resolved at startup, instead of
  resolving that path a second time with its own copy of the code.
* The built-in pages share one modern style with light and dark colours.
  about:start is a new-tab page with the Nordstjernen mark, a large search
  field that also opens typed addresses, and shortcuts to History,
  Settings, About and nordstjernen.org; the release splash moves to the
  top of about:nordstjernen. about:settings has a section list, a short
  description under each option and switches that save as soon as they
  change. about:history groups visits by day and has a filter box, error
  pages use drawn icons instead of emoji, and the license pages match.
* The browser window has a new, flatter look that follows the light or
  dark desktop theme. The old toolbar stayed light with black text on a
  dark desktop, and the active tab drew white text on a near-white tab.
  Tabs now sit in the title bar at full width, and the selected tab joins
  the toolbar below it. The toolbar has round icon buttons drawn for
  Nordstjernen (Back, Forward, Reload, Home, Bookmarks, Downloads, Menu),
  Reload turns into Stop while a page loads, and each tab shows a spinner
  while its page loads, so the address bar no longer jumps when loading
  starts. The address bar is a rounded field with a lock or warning icon,
  the Nordstjernen mark on built-in pages and a star that bookmarks the
  page. Private tabs and the toolbar above them are tinted purple, the zoom
  level is a pill next to the address, and the find bar, status bubble and
  window buttons are rounded to match. Print stays in the menu and on
  Ctrl+P, and tabs shrink when many are open.
* Switching tabs updates the tab strip, the address bar, the window title
  and the Back and Forward buttons for the tab you switch to. The shell
  read the current tab before GTK had changed it, so after a click on a
  tab, Ctrl+Tab or Ctrl+Page Down these kept showing the tab you left.
* Content too wide for a centred line starts at the line's start and
  overflows the end edge, as CSS Text says, instead of being centred with its
  start cut off. reCAPTCHA's image challenge showed every tile one column off
  and the last column empty; it can now be solved.
* The IndexedDB interfaces (`IDBFactory`, `IDBDatabase`, `IDBObjectStore`,
  `IDBIndex`, `IDBCursor`, `IDBKeyRange`, `IDBRequest`, `IDBTransaction`,
  `IDBVersionChangeEvent` and the rest) have the shape of other browsers: their
  attributes are getters on the prototypes, their state is no longer kept in
  `_records`, `_store` and `_meta` members, `IDBRequest`, `IDBDatabase` and
  `IDBTransaction` inherit `EventTarget`, `IDBVersionChangeEvent` is an
  `Event`, and `objectStoreNames` is a `DOMStringList`. A transaction no
  longer stays open forever when a request is issued from a promise callback
  after the last one finished (idb-style wrappers never saw `complete`), a
  request's `result` throws until it is done, errors bubble from the request to
  the transaction and the database and abort the transaction unless cancelled,
  `cursor.continue(key)` lands on the key, `cursor.update()` works on stores
  with a key path, and `getAll()` takes an options dictionary.
* `MediaSource`, `SourceBuffer` and `SourceBufferList` keep their state out of
  sight, inherit `EventTarget`'s listener methods instead of carrying copies,
  and lose `appendBufferAsync()`, `removeAsync()`, `item()`, `audioTracks`,
  `videoTracks` and `textTracks`, which other browsers do not have; `handle`
  exists only in dedicated workers. Missing or empty arguments throw `TypeError`
  and an invalid `SourceBuffer.mode` is ignored, as in other browsers.
* `Cache` and `CacheStorage` are real interfaces (also in workers) whose
  operations reject for a wrong receiver or missing arguments. `ignoreSearch`
  now applies to `delete()` and `matchAll()`, `keys()` takes a request,
  `caches.match()` takes `cacheName`, and a cached response keeps its own `url`.
* `Observable` follows the current specification, and `Subscriber` exists (also
  in workers): `subscribe()` returns nothing, the subscribe callback gets a
  `Subscriber` with `active`, `signal` and `addTeardown()`, concurrent
  subscriptions to one Observable share one producer, and the operators are
  enumerable methods of the standard length.
* A document, a text node, a comment or a document fragment no longer
  has the members of an element. The element member table was installed
  on the `Node`, `Document`, `HTMLDocument` and `DocumentFragment`
  prototypes as well, so `document.click`, `document.style`,
  `document.tagName` and hundreds more existed and `Node.prototype` had
  519 members (47 in other browsers). Each of these prototypes now holds
  the members its interface defines; `CharacterData`, `Text`,
  `DocumentType` and `ShadowRoot` get the ones (`appendData()`,
  `splitText()`, `remove()`, `innerHTML` and so on) they only reached
  through `Node.prototype`, and `EventTarget.prototype`'s listener
  methods work on any node. Elements get the same treatment: what an
  HTML element interface defines (`value`, `href`, `checked`, `rows`,
  `play()`...) is on that interface's prototype and what every HTML
  element has (`style`, `click()`, `focus()`, `title`...) on
  `HTMLElement.prototype`, so a `<div>` no longer has `href` or
  `checked` and an SVG element no longer has `click()`;
  `Element.prototype` holds Element's own members.
* A redirect turns a request into a `GET` only where the Fetch standard
  says so (a `POST` on 301 and 302, anything but `GET` and `HEAD` on 303),
  and the redirected `GET` drops the request's `Content-Type` and the
  other headers that described its body. A `HEAD` request stayed a
  `HEAD` only on 307 and 308, and a `PUT` became a `GET` on 301 and 302;
  a `POST` turned `GET` still sent its `Content-Type`.
* A `POST` without a body (`XMLHttpRequest.send()` or `send(null)`, an
  empty `fetch()`) goes out with `Content-Length: 0`. libcurl read such a
  body from the renderer's standard input and sent it chunked; a server
  that does not take chunked requests then read the end of that body as
  the start of the next request on the connection and answered it with
  an error, so the next request on a kept-alive connection failed.
* `XMLHttpRequest` sends the `Content-Type` its body calls for: a string
  as `text/plain;charset=UTF-8`, a `Blob` as its own type and an
  `ArrayBuffer` or untyped `Blob` without one, instead of
  `application/x-www-form-urlencoded` for all of them. An author
  `Content-Type` on a string body has its charset changed to UTF-8, and
  a header set to an empty value is sent with an empty value instead of
  being dropped. Requests from scripts no longer get libcurl's form type
  when they have none.
* `SharedArrayBuffer` is no longer on the global object of pages,
  frames and workers. The HTML standard exposes it only to documents and
  workers that are cross-origin isolated, which no document here is
  (`crossOriginIsolated` is always false), and other browsers hide it the
  same way.
* `prepend()` with the parent's first child no longer hangs the page. The
  child was inserted before itself and became its own next sibling, so
  the next walk over the document never ended; example.com's new script
  does this, and loading it hung the renderer.
* Events the browser fires (messages from frames, workers, ports and
  broadcast channels, `XMLHttpRequest` progress, `load` and the others)
  carry a `timeStamp`, as in other browsers; it was `undefined`. Events
  at a port, worker, request or channel are at their target while their
  listeners run (`eventPhase` is 2 and `composedPath()` returns the
  target) and have no phase or current target afterwards. Messages and
  errors from a worker are trusted (`isTrusted` was false), and
  `XMLHttpRequest` progress events are `ProgressEvent`s.
* A frame without a source or with `srcdoc` shows `about:blank` or
  `about:srcdoc` as its `location.href`, `document.URL` and
  `document.location`, as in other browsers, and keeps the base URL and
  origin of the document that holds it. These frames showed the holding
  page's URL. A frame's initial `about:blank` document now reads as
  complete and same-origin with its creator from the start.
* A same-origin frame no longer sees the page's own global variables and
  functions as its own. A frame's window started with a copy of every
  property of the page's window, so a script in the frame found the
  page's `var` declarations, functions and `window.x` values (reCAPTCHA's
  frame found the page's `grecaptcha` configuration); it now gets only
  the names the platform defines.
* An asynchronous `XMLHttpRequest` a frame sends to its own origin works
  when that origin differs from the page's. The response was checked
  against whichever document the event loop was in when it arrived,
  usually the page, so it failed with status 0; synchronous requests and
  `fetch()` were not affected.
* The canvas, WebGL and geometry objects have the shape their interfaces
  define. A 2D context carried its attributes, its methods and the
  engine's bookkeeping as own properties, a gradient its stops and
  `addColorStop`, an `ImageData` or a `TextMetrics` its values, an
  `OffscreenCanvas` was an `HTMLCanvasElement`, a WebGL context carried
  about 400 members and writable constants, a `DOMRect` and a `DOMMatrix`
  their numbers, and `OffscreenCanvasRenderingContext2D`, `CanvasGradient`,
  `CanvasPattern`, `ImageBitmap`, `DOMQuad` and the WebGL object
  interfaces did not exist. `CanvasRenderingContext2D`,
  `OffscreenCanvasRenderingContext2D`, `CanvasGradient`, `CanvasPattern`,
  `ImageData`, `ImageBitmap`, `TextMetrics`, `Path2D`, `OffscreenCanvas`,
  `WebGLRenderingContext`, `WebGL2RenderingContext`, `WebGLBuffer` and the
  other WebGL objects, `DOMRect`, `DOMPoint`, `DOMQuad` and `DOMMatrix`
  (and their read-only bases) are classes with their attributes and
  methods on the prototypes, the constants on the interface and its
  prototype as read-only, WebIDL's argument counts and lengths, and a
  wrong receiver or too few arguments throwing `TypeError`. The objects
  are made in the realm of their canvas, so a canvas in an iframe gives
  the iframe's objects.
* `canvas.getContext('2d')` returns the same context each time, `null`
  once the canvas has another kind of context, and a resized canvas has a
  fresh context state. The context's attributes validate as the HTML
  standard says: an invalid value is ignored, `fillStyle`, `strokeStyle`
  and `shadowColor` read back as `#rrggbb` or `rgba(r, g, b, a)`, `font`
  reads back in pixels without a line height, and `color(srgb ...)` and
  `color(display-p3 ...)` colors parse. `new ImageData()` works, `new
  OffscreenCanvas()` is an `EventTarget` that resizes and converts to a
  `Blob`, `CanvasPattern.setTransform()` exists, and `createImageData()`
  and `getImageData()` throw `IndexSizeError` for an empty region.
* `OffscreenCanvas` and its context, `ImageData`, `ImageBitmap`, `Path2D`,
  `createImageBitmap()`, `DOMRect`, `DOMPoint`, `DOMQuad` and `DOMMatrix`
  exist in workers, where an `OffscreenCanvas` draws. `structuredClone()`,
  `postMessage()` and worker messages carry `ImageData`, `ImageBitmap` and
  the geometry objects; the other canvas objects throw `DataCloneError`.
* WebGL: `getAttachedShaders()`, `getIndexedParameter()`,
  `getSamplerParameter()`, `getTransformFeedbackVarying()`,
  `getUniformIndices()`, `invalidateSubFramebuffer()` and
  `drawingBufferStorage()` exist, `stencilFuncSeparate()`,
  `stencilOpSeparate()`, `stencilMaskSeparate()` and `sampleCoverage()`
  call GL instead of doing nothing, `copyTexImage2D()` and
  `copyTexSubImage2D()` work in WebGL 1, `drawingBufferWidth` follows a
  resize, the stencil masks read back unsigned and `getContextAttributes()`
  lists its members in dictionary order.
* CI no longer runs CodeQL: the `codeql` workflow, its configuration and
  the README badge are gone. The Semgrep badge is gone too.

1.0.27:
======
* The about:start splash shows version 1.0.27.
* The default search engine is DuckDuckGo (`https://duckduckgo.com/`)
  instead of DuckDuckGo Lite, for the address bar, the about:start search
  box and the Android app. A saved configuration still holding the old
  Lite default moves to the new one; other choices are kept.
* A CSS value with thousands of nested parentheses or functions, in an
  `@property` `initial-value` or in `CSS.registerProperty()`, no longer
  crashes the renderer. Values nested deeper than 128 levels are
  invalid.
* An SVG with thousands of nested `<g>` elements, as an `<img>` or
  inline, no longer crashes the renderer. Rendering stops descending
  after 256 nested elements.
* A page nested tens of thousands of elements deep no longer crashes the
  renderer. Font-usage collection, video discovery, container-unit
  detection, animated-value propagation and iframe cleanup walked the
  tree recursively and ran out of stack around 50,000 levels (a plain
  run of `<div>` or `<li>` was enough); they now walk it in a loop.
* The desktop browser now records visited pages in History. The shell
  recorded each page title into a history database it never opened, so
  in the default multi-process mode `about:history` stayed empty. A
  `--private` launch still records nothing.
* A link opened in a new tab from a private tab (middle-click,
  Ctrl+click or "Open in new tab") opens in a private tab. It opened in
  a normal tab, so its cookies, cache and storage were written to the
  profile on disk and its address was saved in the session file.
* The shell checks every `open` and `reload` command a renderer sends to
  the video helper, as it already did for the audio helper: only a
  stream file under `~/.cache/nordstjernen/msvideo/` is passed on. A
  renderer could make the unsandboxed video helper open any local file
  or URL. Both checks now split a command on spaces and tabs the way the
  helpers do, so a tab after `open` no longer skips the audio check, and
  both helpers open media with FFmpeg's `file` protocol only.
* Each published release gets a Debian/Ubuntu `.deb`, a portable Linux
  x86_64 zip and a `SHA256SUMS` file attached by a new `release`
  workflow. The packages are built and checked with the same script as
  the nightly builds, which installs the `.deb` and runs it headless
  before anything is uploaded.
* A connection that fails to one port of a host no longer makes
  requests to the host's other ports fail for two minutes: recent
  connection failures are remembered per origin (scheme, host and port)
  instead of per host.
* `performance.getEntriesByType('resource')` lists the document's
  scripts, stylesheets, frames and images along with `fetch()` and
  `XMLHttpRequest` requests, as `PerformanceResourceTiming` entries with
  the timings the network layer measured (DNS lookup, connection, TLS,
  request and response), the connection's protocol in `nextHopProtocol`,
  the real `responseStatus`, transfer and body sizes, and
  `renderBlockingStatus`. A cross-origin resource without a
  `Timing-Allow-Origin` header naming the page keeps only its start and
  end time, and a no-CORS cross-origin response shows status 0, as in
  other browsers. Only `fetch()`, `XMLHttpRequest` and script-made images
  were listed before, with every phase at the start or the end of the
  request, `nextHopProtocol` always `"h2"` and `responseStatus` always
  200. A second image with the same URL comes from memory and gets no
  entry, nor does a frame whose response is a download, and
  `PerformanceObserver.supportedEntryTypes` is in alphabetical order.
* Each document has its own performance timeline. A frame's
  `performance` listed the page's entries and marks and its
  `performance.mark()` added to the page's; now marks, measures and
  resource entries stay with the document that made them, a
  PerformanceObserver sees the entries of the document its callback
  comes from, and `clearResourceTimings()` clears resource entries
  instead of doing nothing.
* Scripts inserted with `append()`, `prepend()`, `before()`, `after()`,
  `replaceWith()` or `replaceChildren()` run, as they do with
  `appendChild()`. They never ran, so `document.head.append(script)`
  loaded nothing and Svelte 5 components lost the scripts in their
  `<svelte:head>` (Stack Overflow's Google sign-in among them).
  Stylesheet links and `srcdoc` frames these methods insert load now
  too. Custom
  elements these methods insert or move get `connectedCallback` and
  `disconnectedCallback` before the method returns, moving a node out of
  another parent produces its MutationObserver removal record and
  updates NodeIterators, and for every insertion method scripts run
  before custom element callbacks, as in other browsers.
* Images in the page's own markup fire `load` and `error` events, and
  the window's `load` event waits for them, as in other browsers. A
  broken or missing image could finish without an `error` event and
  still read as incomplete when the page's `load` event fired.
* Dedicated workers follow the HTML standard more closely. Messages to
  and from workers use structured cloning, so `Error` objects (with
  `cause`), `DOMException`, `Blob` and `File`, non-index array
  properties, shared references and cycles arrive intact, functions and
  symbols throw `DataCloneError`, and a transferred `MessagePort` can be
  sent back to where its other end lives. `new Worker()` accepts `data:`
  URLs (which run with an opaque origin), resolves URLs against the
  document's base URL, throws `SyntaxError` only for a URL that does not
  parse and otherwise reports failures through an `error` event.
  Uncaught worker errors reach `self.onerror`, then the `Worker`'s
  `onerror`, then the page's `window.onerror`, unless one of them
  cancels it; a worker keeps running after one, and `terminate()` drops
  messages the worker had already sent.
* `document.write` runs the scripts it writes; they used to be skipped.
  A write whose markup closes everything it opens is inserted right away,
  so the written elements exist when `write()` returns and a written
  inline script runs inside the call, as in other browsers. A written
  external script runs before the rest of the page is parsed.
* Scripts run against the part of the page parsed so far, as in other
  browsers: a script in `<head>` sees `document.body` as `null`, a
  script is the last `<script>` in the document while it runs, elements
  further down cannot be found yet, and the rest of the page is inserted
  after the script, with the MutationObserver records parser insertions
  produce. The whole page used to be visible to every script.
* Shadow roots behave as document fragments: nodes directly inside one
  have a `null` `parentElement`, the root's `nodeName` is
  `#document-fragment`, it is not `instanceof Element`, and `closest()`,
  `contains()` and `compareDocumentPosition()` stop at the shadow
  boundary. Document-wide queries such as `getElementById` and
  `querySelector` no longer return elements inside shadow trees, which
  they did once a script had looked those elements up inside the shadow
  root.
* A frame starts with its own `onload`, `onclick` and other window event
  handlers set to `null`. It used to start with copies of its parent's,
  so the page's `window.onload` could run again for the frame.
* Nested frames know their place: a frame inside a frame has the middle
  frame as its `parent`, so `parent.frames`, `parent.document` and
  `parent.name` refer to it, while `top` stays the page. `parent.location`
  and `top.location` read in a frame give that window's URL instead of
  the frame's own, setting `parent.location.hash` fires `hashchange` at
  the page, and a frame's `document.referrer` is the URL of the document
  holding it, cut down by the referrer policy as in other browsers.
* Focus follows the HTML focus update steps. `blur` and `focusout` run
  while no element has focus, `relatedTarget` names the other element,
  and the windows get `blur` and `focus` when focus moves between a page
  and its frames, by script or by a click. `document.activeElement` is
  the iframe, not the element inside it, when a frame has focus, so a
  page can no longer see which element of a cross-origin frame is
  focused. `document.hasFocus()` is true only for the focused document
  and the documents containing it, and `focus()` does nothing on an
  element that cannot take focus or is not in the document. Clicking
  plain content inside a frame focuses the frame's document, as in other
  browsers, instead of the iframe element around it.
* Events the browser fires carry the flags other browsers give them:
  `focus`, `blur`, `readystatechange`, element `scroll` and media events
  no longer bubble, and `DOMContentLoaded`, `input`, `change`, `scroll`
  and `focusin`/`focusout` can no longer be cancelled. `invalid` can be.
* Window events look as they do in other browsers: `load` reaches only
  window listeners, with the document as its target; `hashchange` and
  `popstate` are fired at the window and do not bubble; `pageshow` and
  `pagehide` have the document as their target; `window.onload`,
  `window.onmessage` and the other window handlers see `eventPhase` and
  `currentTarget`; and `<body onload>`-style handlers run with the window
  as `this`. Scrolling the page fires one `scroll` event instead of two,
  and `resize` is fired at the window only.
* Transferring a `MessagePort` moves it: the receiver gets a new port
  object, references to the port inside the message point to that new
  port, and the sender's port stops sending and receiving. Messages
  already queued for the port go with it. Transferring a port twice,
  transferring the port that sends the message, or listing a port twice
  throws `DataCloneError`, and a message that fails to clone leaves its
  ports in place. `window.postMessage` transfers its ports even when the
  target origin does not match, as in other browsers.
* A frame's document no longer shows through its iframe element:
  `textContent`, `innerHTML`, `outerHTML`, `getHTML()`, `XMLSerializer`,
  `TreeWalker`, `hasChildNodes()` and `cloneNode(true)` treat the iframe as
  having only its fallback content, as `childNodes` already did. They used
  to include the frame's text and markup, even for a cross-origin frame.
* Deep recursion in a worker throws `RangeError` instead of crashing the
  browser: the worker's JavaScript stack limit now fits its thread's stack.
* Message ports work across frames: a frame that receives a transferred
  `MessagePort` gets its message events, `data` and `ports` in its own
  realm, so arrays, dates and objects pass `instanceof` checks there,
  `addEventListener` on a port accepts `{handleEvent}` objects, and the
  handlers run against the frame's document. Exceptions thrown by port
  handlers and by window `on<event>` handlers now reach `onerror` and
  `error` listeners instead of being dropped.
* Events inside a frame reach that frame's window: window listeners and
  `window.onclick`-style handlers see clicks, pointer and key events,
  `DOMContentLoaded` and bubbling custom events, and a frame's `load`
  event fires once with the document as its target.
* When the browser dispatches an event, such as a user click or a port
  message, microtasks queued by one listener run before the next
  listener, as in other browsers.
* The window keeps being ticked while a `requestIdleCallback`, a posted
  message or a script-started image load is waiting, so they no longer
  stall until an unrelated timer fires.
* `window.postMessage` throws `SyntaxError` for an unparseable target
  origin and defaults to `"/"` when none is given; messages to and from a
  frame sandboxed without `allow-same-origin` use the opaque origin
  `"null"`; message events target the receiving window and carry a
  frozen `ports` array from the receiving realm.
* Aliased built-ins keep their spec names (`String.prototype.trimLeft`
  is the function named `trimStart`), and `Function.prototype.name` is
  empty again.
* `postMessage` and `MessagePort` messages are delivered as tasks, after
  the sender's microtasks, as in other browsers. They ran as microtasks,
  so a message arrived before promise callbacks queued ahead of it, and a
  handler that posted back to itself kept `setTimeout` callbacks from
  ever running.
* A frame's `WindowProxy` stays the same object across its first
  navigation. A `contentWindow` read while an iframe still showed its
  initial `about:blank` now reaches the document that loads into it:
  same-origin frames reuse the initial window, as the HTML spec
  requires, and messages posted through the early reference to a
  cross-origin frame are delivered. The link between a frame's outer
  window and its realm global moved out of JavaScript-visible properties,
  which had let a cross-origin frame reach its embedder's window.
* Structured data sent between windows, frames and `MessageChannel`
  ports arrives as objects of the receiving realm: `e.data instanceof
  Uint8Array`, `Map`, `Date`, `Array` and `Object` hold in the receiver,
  as in other browsers. Messages crossing a frame boundary carried the
  sender's objects, and on ports `Map`, `Set`, `Date`, `RegExp`,
  `DataView` and boxed primitives degraded to plain objects.
* Frames have their own document lifecycle and geometry. Each frame's
  `document.readyState` runs `loading` -> `interactive` -> `complete`
  with `readystatechange` at each step, instead of reporting the top
  page's state (usually already `complete`). `innerWidth`/`innerHeight`
  in a frame are the frame's size, not the top window's, element rects
  are measured from the frame's content box rather than its border box,
  and `document.elementFromPoint` in a frame hit-tests that frame.
* A message from a cross-origin frame has the frame's WindowProxy as
  `event.source`, the same object as `iframe.contentWindow` and
  `frames[i]`, so pages can tell which frame spoke. It was a different
  wrapper, so `e.source === iframe.contentWindow` was false.
* A `Response` or `Request` built from a string keeps that string as its
  body. `new Response('{"a":1}').json()` rejected with a `SyntaxError`
  because `text()` saw an empty body, and `fetch(new Request(url,
  {method: 'POST', body: 'x'}))` sent nothing.
* Rounded solid borders whose sides differ in color, or have some sides
  transparent, follow the corner radius. Each side was stroked as a
  straight line, so a `border-radius` ring with two transparent sides,
  such as the reCAPTCHA checkbox spinner, drew as a right angle.
* Events carry the interface their type implies: messages from windows,
  `MessageChannel` ports and workers are `MessageEvent`s, and `error`,
  `hashchange`, `popstate`, `storage`, `pageshow`/`pagehide` and promise
  rejection events get their own interfaces. Port messages were plain
  objects. `isTrusted` lives on each event, not on `Event.prototype`, so
  objects deriving from `Event.prototype` can define their own, as in
  other browsers. Assigning to a getter-only property now names it in
  the `TypeError`.
* Page scripts enumerating the global object (`Object.keys(window)`,
  `Object.getOwnPropertyNames`, `for...in`) no longer see the engine's
  own `__nd`/`__ns`/`__js` helper properties, which no other browser
  exposes. The engine's own code still reaches them by name.
* Objects the engine hands to pages inherit from their WebIDL interface
  and report its name: `new FileReader() instanceof FileReader`,
  `Object.prototype.toString.call(localStorage)` is `[object Storage]`,
  and canvas contexts, `TextMetrics`, `ImageData`, `MediaQueryList`,
  `FontFaceSet`, `location`, `screen` and the `navigator` sub-objects
  follow suit. `Location`, `Screen`, `BarProp`, `CustomElementRegistry`
  and the other interfaces these belong to now exist as globals.
* Functions the engine implements in JavaScript print as native code,
  `function animate() { [native code] }`, like every other built-in.
  434 of them printed their JavaScript source and many carried internal
  names (`elementAnimate`, `value`), which no browser does. Page scripts,
  inline handlers and `new Function` keep their source.
* Inside a frame, `window`, `location` and `history` report themselves
  as `[object Window]`, `[object Location]` and `[object History]`, and
  `history instanceof History` holds, as on the top-level page. The
  frame's global still carried QuickJS's `global` tag.
* A worker's `navigator` matches the page's: `appVersion` follows the
  user agent instead of a fixed Linux string, and `platform`, `product`
  and `deviceMemory` are present. `performance.getEntries*` return empty
  lists in workers instead of throwing. A frame's document has `domain`,
  `timeline`, `pictureInPictureEnabled`, `adoptedStyleSheets`,
  `xmlEncoding` and `xmlStandalone` like the top-level document.
* Dedicated workers have `Intl` and `crossOriginIsolated`. A worker that
  read its time zone or formatted a number threw `ReferenceError`, and
  since worker errors do not reach the page, the page waited forever.
* Numbers the engine formats keep a `.` decimal point under locales that
  use a comma, such as Turkish or Norwegian. `--single-process` runs the
  engine inside the GTK shell, whose startup applies the OS locale, so
  `Accept-Language` went out as `tr-TR,tr;q=0,9`, an invalid header.
* `window.postMessage(message, [port])` with an array as the second
  argument follows the `(message, options)` overload, as WebIDL overload
  resolution requires: the array is read as an options dictionary, so
  nothing is transferred. It was taken as a transfer list, so a library
  that posts `postMessage(token, [channel.port2])` from code shared with
  workers handed its own port to every `message` listener on the page,
  and a listener waiting for a port from another frame accepted that one
  instead. Any second argument that is neither an object nor
  `undefined`/`null` is the target origin, and `postMessage()` with no
  arguments throws `TypeError`.
* The Android app builds with the current toolchain: Android Gradle
  plugin 9.4 with its built-in Kotlin, Gradle 9.8, compileSdk 37
  (Android 17) with build-tools 37, CMake 4.1.2 and the current AndroidX
  libraries. targetSdk stays 36. Native libraries are still stored
  compressed and extracted at install, now requested from the build script
  (packaging.jniLibs.useLegacyPackaging) because AGP 9 rejects
  android:extractNativeLibs in the manifest.
* Nordstjernen can run on Fabrice Bellard's original QuickJS as well as the
  in-tree quickjs-ng fork, chosen in meson with `-Dquickjs=quickjs` or
  `-Dquickjs=quickjs-ng` (the default). The original engine is fetched at
  configure time through `subprojects/quickjs.wrap`, pinned to its
  2026-06-04 release, and is never vendored. The same binding runs on both:
  `src/ns_quickjs.h` maps the quickjs-ng API onto the original's, and the
  original keeps its own bytecode cache. On the original engine, named
  access into another frame's window, a view taken on an imported
  WebAssembly memory before instantiation, and the fork's language
  compatibility changes (`RegExp.$1`, `Function.prototype.caller`) behave
  as stock QuickJS does; `docs/quickjs.md` lists the differences.
* A module script that imports other modules runs again after the page
  has created an event, posted a message, opened IndexedDB or fetched
  something. Native code that built those objects assigned `isTrusted`
  over the read-only accessor on `Event.prototype`, and a fetched
  response's `body` over the read-only `Response.prototype.body`, ignoring
  the failure; quickjs-ng kept the error pending and reported it from the
  next module evaluation as `TypeError: no setter for property`, so the
  whole module was lost. Each page load also left two such errors behind
  from start-up: `Event.prototype` was being made its own prototype, and
  `new URL()` built its `searchParams` before `URLSearchParams` existed.
  Trusted events keep `isTrusted` true through the accessor.
* `new Response(body)` and `new Request(url, { body })` keep a string body
  again. The constructors assigned it over the read-only
  `Response.prototype.body` accessor, so the text never reached the body,
  and `text()`, `json()` and `clone()` returned an empty body,
  `new Response(null).body` was a stream instead of `null`, and reading
  the stream failed. The raw body is now defined on the object and
  replaced by the buffered body once it is read, as the GPL edition does.
* On the original QuickJS, `Array.prototype.sort` calls the comparator for
  identical values, as the fork and every other engine do, through one
  patch applied to the fetched source
  (`subprojects/packagefiles/quickjs-sort-calls-comparator.patch`).
  Without it jQuery 4's `uniqueSort` kept duplicates, so `$(a).add(a)` and
  `.closest()` returned the same element twice. The adapter also pads
  short argument lists to `JS_NewTypedArray`, whose original constructor
  reads three arguments whatever `argc` says, gives `JS_EvalThis2` the
  quickjs-ng default file name when none is passed, and refuses a
  resizable external `ArrayBuffer` instead of quietly making it fixed.
* `window[i]` and `frames[i]` return the WindowProxy of the i-th child
  frame, and `window[name]` returns the frame whose `name` attribute
  matches, as the HTML named-access rules specify. Indexed access always
  gave `undefined`, and from inside a frame `parent.frames` counted the
  frames of the calling document instead of the parent's, so sibling
  frames could not reach each other to `postMessage`. Named and indexed
  access resolves in the document of the window being read, so a frame's
  `window.foo` no longer finds an element in its parent. A cross-origin
  parent now exposes its child frames by index and name too, each behind
  the same cross-origin WindowProxy, while every other property still
  throws `SecurityError`.
* Inline event-handler content attributes inside a frame, such as
  `<body onload="...">`, are compiled and run in that frame's realm. They
  ran in the top-level page's realm, so they could not see functions
  declared by the frame's own scripts and failed with `ReferenceError`,
  and a frame without scripts ran its handlers with the parent's document.
  An `<iframe onload="...">` attribute runs in the page that holds the
  iframe, not inside the frame.
* A frame's `window.name` starts as its `<iframe name>`, and a same-origin
  frame's `window.frameElement` is its `<iframe>`. Both were empty, so a
  frame could not find itself in `parent.frames`.
* `postMessage` into a same-origin frame reports the caller as
  `event.source`. It reported the receiving frame itself, so replying to
  `event.source` sent the answer back into the frame instead of to its
  parent. A message from another origin hands the frame a cross-origin
  WindowProxy as `event.source` rather than the sender's own window.
* The in-tree HTML parser, lexbor, is refreshed to upstream master
  (327a8b6). It fixes memory corruption when a pooled allocation shrinks,
  an out-of-bounds write in `CharacterData.replaceData()`, and
  uninitialized memory in IDNA host processing. URLs parse more exactly:
  `.` and `..` path segments no longer swallow the `?` or `#` after them
  or miscount the path (`/a/b/../../../c/é/../../x` is `/x`), `//./c`
  keeps its empty segment, a username may contain `@`, and spaces at the
  end of an opaque path such as `sc:a ?q` are encoded. A `<form>` inside
  a template's `innerHTML` is parsed as in the rest of the template.
* `align-content` works on block containers, as CSS Box Alignment
  specifies: `center`, `end` and their `safe`/`unsafe` forms move the
  content of a block with a fixed height. It only worked in flex and grid
  containers, so a vertically centered block kept its text at the top.
* A block-level `<textarea>` with a CSS height is sized by that height.
  Its text run was sized as the whole control, padding included, so a
  single-row field overflowed its own content box: it drew a scrollbar
  and reported a `scrollHeight` 20px too tall, which auto-growing chat
  inputs copied into their height.
* A focused, empty `<textarea>` keeps showing its placeholder, with the
  caret in front of it, as `<input>` already did. Placeholder text is no
  longer spell-checked.
* Web fonts whose files carry a family name other than the `@font-face`
  family are registered under the CSS family with each file's own
  weight. A family split over Regular, Medium and Bold files used to be
  aliased to whichever file loaded first, so normal text could render
  bold.
* Nordstjernen builds against GLib 2.90. Its `g_new()`/`g_renew()`
  macros now declare a local named `_n`, which the CSS selector matcher
  shadowed, so the Windows (MSYS2) build failed under `--werror`.
* The macOS CI smoke test of the bundled app accepts the redesigned
  example.com page, which no longer carries the "Example Domain"
  heading the test looked for, so every macOS run failed after a
  successful build.
* YouTube and other MSE players play video again.
  `SourceBuffer.appendBuffer()` still called a helper that the Blob
  rewrite removed, so every append threw `ReferenceError` and no media
  byte reached the decoder: YouTube showed "An error occurred". It now
  copies the `ArrayBuffer` or view it is given and throws `TypeError`
  for anything else, before the state checks, as WebIDL requires.
* `animation` and `transition` set to a CSS-wide keyword (`inherit`,
  `initial`, `unset`, `revert`, `revert-layer`) give that keyword to
  every longhand. The shorthand expansion read the keyword as an
  animation list, which crashed the renderer on pages such as YouTube
  and made `animation: inherit` compute to `none`.

1.0.26:
======
* Grids with more than 24 rows lay out fully. Auto-placement stopped at
  row 24 and piled every later item onto it.
* Grids that use `grid-template-areas` are laid out by the full grid
  algorithm. They went through a reduced code path that gave `fr` rows no
  share of a fixed container height (a header / `1fr` / footer page left
  the footer under the header), sized the columns the areas added as `1fr`
  instead of by `grid-auto-columns`, and ignored `align-content`,
  `align-items` and `grid-auto-rows`. Area names also work as line names,
  so `grid-row: main` places an item on a named area.
* `fr` rows grow to fill a container's `min-height`, so the `min-height:
  100vh; grid-template-rows: auto 1fr auto` page layout keeps its footer
  at the bottom.
* A grid row with a fixed size keeps it: content taller than a `20px` row
  overflows instead of pushing later rows down. Items given both a row and
  a column claim their cell before auto-placed items flow in, instead of
  landing on top of one.
* Grid track sizing follows the specification more closely: an item
  spanning several `auto` or content-sized columns widens them instead of
  overflowing; `fit-content()` tracks shrink to their content;
  `minmax(<length>, 1fr)` columns no longer push the grid wider than its
  container; `repeat(auto-fill / auto-fit)` counts repetitions in the
  space the other tracks leave and now works for rows too, empty
  `auto-fit` rows collapse, and line names inside the repetition repeat
  with it; columns created past the explicit grid are sized from
  `grid-auto-columns`.
* `grid`, `grid-template`, `grid-row`, `grid-column`, `grid-area` and
  their longhands follow their grammar: invalid values such as
  `grid-template: 10px` or `grid-row-start: 0` are dropped instead of
  partly applied, omitted parts are reset, rows written without a size in
  the template form are `auto`, line and area names keep their case, and
  `element.style` and `getComputedStyle()` read the shorthands back in
  shortest form. Computed track lists keep `repeat()`, `minmax()`,
  `fit-content()` and subgrid line names.
* `self-start` and `self-end` on a grid item use the item's own writing
  mode.
* `sibling-index()` and `sibling-count()` in a container size query
  resolve against the container element.
* Popovers follow the HTML standard. `showPopover()` left the element
  hidden until something else restyled the page, opening one auto popover
  never closed another, nothing threw for elements without a `popover`
  attribute or for disconnected ones, `toggle` fired synchronously as a
  plain event, and a `popovertarget` submit button inside a form toggled
  the popover instead of submitting. Popovers now use the standard auto
  and hint stacks, fire a cancelable `beforetoggle` and a queued
  `ToggleEvent` carrying `source`, close when the user clicks outside or
  presses Escape, hide when removed or retyped, and get the standard
  centred fixed box. The `popover`, `popoverTargetAction`,
  `popoverTargetElement`, `command` and `commandForElement` properties
  reflect their attributes, and `commandfor`/`command` buttons work: show,
  hide and toggle-popover, show-modal, close, request-close, and custom
  `--` commands through `CommandEvent`.
* `<dialog>` follows the HTML show and close steps: `show()`,
  `showModal()` and closing fire `beforetoggle` and a queued `toggle`, and
  the `close` event is queued; `close()` on a closed dialog no longer
  changes `returnValue`; `show()` on an open modal dialog throws, as does
  `showModal()` on a dialog outside the active document; modal dialogs
  stack in the order they were opened, so the last one opened is drawn on
  top and stays interactive, and `:modal` matches every open modal dialog;
  focus returns to the element focused before the dialog opened;
  `requestClose()`, Escape and `closedby="any"` light dismiss go through a
  close watcher that honours `closedby`, and cancel can no longer re-enter
  itself.
* Light dismiss of popovers and dialogs runs before the pointer event
  reaches the page, so `beforetoggle` arrives before the page's
  `pointerup` listeners, as the standard orders it.
* A click on a popover opened inside a modal dialog lands on the popover,
  not the dialog behind it.
* Hiding a popover no longer moves focus back onto an element the page has
  since moved into that popover.
* `:focus-visible` is no longer the same as `:focus`: a button or link
  focused by a mouse click no longer shows the focus styling pages reserve
  for keyboard users, and `:focus:not(:focus-visible)` now matches.
  Keyboard focus, text fields and script focus remain visible, and a
  focused control that turns into a text field starts matching.
* Pages' `autofocus` attribute works: the first visible, focusable element
  with `autofocus` gets focus before the page finishes loading, unless the
  page already focused something or the URL fragment points at an element.
* The `--wpt` runner sends test_driver clicks, action sequences and keys
  as trusted input, so light dismiss, Escape, Tab, keyboard activation and
  click-to-focus are exercised the way real input exercises them, and
  WebDriver key codes are no longer typed into fields.
* The audio helper only opens what a page may play. It opened any
  `file://` path the renderer named, and treated any other string that was
  not an http, https or data URL as a local path, so the page's JavaScript
  check was the only thing keeping a web page from playing (and so
  probing) local files. The shell now forwards `open` and `reload`
  commands only for http, https and data URLs, the browser's own MSE audio
  stream files, and local files when the tab itself shows a `file:` page;
  the helper no longer reads bare paths at all.
* The Windows process mitigations are the ones intended. The policies were
  passed as bare numbers and two were wrong: 0 is DEP, not ASLR, and 7 is
  Control Flow Guard, not the dynamic-code policy -- DEP is always on for a
  64-bit process and CFG cannot be turned on after start, so both calls did
  nothing. ASLR now forces image relocation; dynamic-code prohibition stays
  off, since the renderer's GPU driver and a V8 build need executable
  memory; SECURITY.md is corrected to match.
* A frame whose URL redirects to another site takes the origin of the page
  it actually loads. The frame kept the URL it was requested with, so a
  same-origin address that redirected elsewhere gave the other site's
  document the embedding page's origin: it could read the parent's DOM and
  cookies, and the parent could read it. The frame now uses the response's
  final URL, and a redirect into a source the page's CSP `frame-src` or
  `object-src` does not allow is blocked.
* A page can no longer read the pixels of another site's images. Drawing
  a cross-origin image into a canvas -- directly, through a pattern,
  another canvas, an ImageBitmap, an OffscreenCanvas or a video poster --
  left its pixels readable through `getImageData`, `toDataURL`, `toBlob`
  and `convertToBlob`, and WebGL's `texImage2D` plus `readPixels` read them
  back too. The canvas is now marked tainted and those calls throw
  `SecurityError`; WebGL and WebGPU refuse cross-origin sources outright;
  a redirect to another origin counts as cross-origin. Same-origin, `data:`
  and `blob:` images are unaffected, and so are images loaded with
  `crossorigin` from servers that allow it through CORS, so WebGL texture
  and map-tile pages keep working.
* `OfflineAudioContext` and `createBuffer` reject impossible sizes with
  `NotSupportedError`. A page could ask for an 8 GB render buffer, or raise
  `length` after construction, and crash the browser, and bad channel
  counts, lengths or sample rates were quietly replaced with 1. Very deep
  audio graphs no longer abort when scratch memory runs out.
* AES-CTR honours the counter `length`: the whole 128-bit block was used as
  the counter, so once the counter wrapped the ciphertext no longer
  decrypted in other browsers. Invalid lengths and messages that would
  reuse a counter block are rejected.
* `crypto.getRandomValues` throws the errors the specification names:
  `TypeMismatchError` for a DataView and `QuotaExceededError` for more than
  65536 bytes.
* Changing an element's `class` or `id` to a name no style sheet mentions
  no longer restyles everything inside it: toggling an unused theme class
  on `<body>` re-ran the whole cascade, about 200 ms on a page of 12,000
  elements and 3,000 rules. Names used anywhere in a selector, including
  inside `:is()`, `:not()`, `:has()`, `:nth-child(... of S)` and `@scope`,
  and any `[class]`/`[id]` attribute selector, still restyle as before.
* Class changes and element insertion no longer walk sibling lists to keep
  the document's class and tag indexes in order; large index entries are
  re-sorted only when read, so 3,000 class changes on a 12,000-element page
  take 5 ms instead of 480.
* Removing or inserting children no longer counts the node's position
  among its siblings when no live `Range` exists: emptying a 20,000-item
  list from the end took 19 seconds and now takes 58 ms.
* A CSS `font-family` list is resolved to a font once rather than on every
  text run of every layout and paint, until the system font set changes or
  a web font loads.
* An image with a CSS `filter` is filtered once and the result kept with
  the decoded image, not on every paint.
* Ordered lists are numbered once per layout or paint pass instead of once
  per item.
* Page zoom scales each element's font size once; computed values shared
  between sibling styles were multiplied once per sharer.
* An absolutely positioned grid child placed with `grid-column`/`grid-row`
  stays on its tracks when the grid moves after layout, under a
  relatively positioned ancestor or in a reversed wrapping flex container.
* A cross-origin frame no longer reaches the embedding page through what
  the two share. A frame's global object received a copy of every global
  on the parent's window, including the page's own variables, so
  `window.someState` or any stored reference to `document` handed the
  frame the parent's DOM and cookies; `new Text()`, `new Comment()` and
  `new Range()` produced nodes belonging to the parent's document;
  `customElements.define()` in the frame upgraded the parent's elements
  with the frame's class; and the `cookieStore` shim returned the parent's
  cookies. A cross-origin frame now gets only the browser's own globals,
  captured before the page's scripts run, and none of the parent-bound ones
  (`cookieStore`, `caches`, `getSelection`, `opener`, `frameElement`,
  `name`, `origin`, `navigation`); nodes and ranges a frame creates belong
  to the frame's document; each document has its own custom element
  registry, as HTML specifies; and `self.origin` reports the frame's
  origin.
* URL setters follow the URL Standard where the parser library does not:
  `url.host = "example.com:99999"` changes the host and keeps the port,
  clearing the host of a non-special URL with credentials is refused, and
  `new URL("??a=b").searchParams` keeps the second `?`. Links whose `href`
  does not parse report `":"` as their protocol, and an `href` containing
  a NUL is no longer cut short there.
* `innerText` follows the rendered-text rules: a shadow host's text no
  longer includes its shadow tree, an inline `<svg>`'s `<text>` counts, a
  `visibility: hidden` paragraph or `<br>` adds no line breaks, and setting
  `innerText` or `outerText` to a string with a NUL keeps what follows it.
* Reflected attributes that ignored writes or read the wrong type are
  fixed: `meta.content`, `textarea.rows` and `frameset.rows` can be set
  from script; ARIA properties read `null` when absent and the missing
  ones exist; `font.size` is a string; `progress.max` ignores non-positive
  values; progress and meter use the HTML number rules; and `label` and
  `defaultValue` writes reach mutation observers.
* `innerHTML`, `outerHTML`, `insertAdjacentHTML` and
  `Range.createContextualFragment` parse markup in the context element's
  namespace, so gradients, filters and shapes that D3, icon libraries and
  chart code insert into an `<svg>` that way are drawn.
* A `<script>` inserted empty runs once it gets text or a `src`, and
  `src=""` fires `error` instead of running the inline text.
* Page images start downloading when the document is laid out rather than
  at first paint; each `<img>` fires `load` when its image arrives,
  `complete` and `naturalWidth` report it, and the window `load` event
  waits for the page's non-lazy images.
* Setting `document.title` on a page without a `<title>` creates one that
  later reads find and mutation observers see.
* `reportError(value)` reports the value like an uncaught exception, as a
  cancelable `ErrorEvent` at the window, and uncaught-exception events are
  `ErrorEvent`s.
* Elements whose interface is plain `HTMLElement` (`article`, `section`,
  `b`, `nav`, `summary`, ...) are no longer `HTMLUnknownElement`, and
  `listing`/`xmp` are `HTMLPreElement`s.
* Enter activates a focused link, button or `<summary>`, and Space on
  release a focused button, checkbox, radio button or summary, with a
  trusted `click` as a mouse press would; and Tab continues from where you
  last clicked instead of from the top of the page.
* Form validation: every radio button in a required group reports
  `valueMissing` while none is checked; submit buttons are validation
  candidates, so a custom validity message on one blocks submission; and a
  `readonly` input of any type is left out of validation.
* `relList.supports()` answers per element and `<form>` has a `relList`;
  `String(link)` gives an `<a>`'s or `<area>`'s URL; and natively
  implemented interfaces have their `Symbol.toStringTag`.
* Client-side image maps work: clicking, hovering or `elementFromPoint()`
  over an `<img usemap>` lands on the `<area>` under the pointer, and
  clicking an area follows its link.
* `Blob` and `File` follow the File API: any iterable of parts, `endings:
  "native"`, printable-only `type`, prototype getters, clamping `slice()`
  and `blob.bytes()`.
* `structuredClone()` and `postMessage()` follow HTML's serialization
  rules more closely: transferring an `ArrayBuffer` detaches it, a detached
  or duplicate transfer throws `DataCloneError`, resizable buffers and
  shared views survive, and a page that replaces `window.structuredClone`
  no longer changes what `postMessage()` sends.
* Styling a large page is much faster. Every rule used to be tried on each
  element whose tag, class or id matched its last compound, and each try
  walked all the way up the ancestors. The cascade now keeps a small
  filter of the tags, ids and classes above the element being styled and
  drops a rule at once when an ancestor it needs cannot be there: a page of
  16,000 elements and 3,000 rules loads in under a second instead of three.
* A selector whose left end cannot match no longer makes matching
  exponential in the depth of the page. `.nomatch div div div span` on a
  deep tree took seconds, in a style sheet or in `querySelectorAll`;
  matching now stops as soon as the rest of the selector cannot match.
* `:nth-child`, `:nth-last-child`, `:nth-of-type` and `:nth-last-of-type`
  number each list once per style pass instead of counting siblings for
  every element, so a long list is no longer quadratic to style: 40,000
  rows with `li:nth-child(even)` load in about two seconds instead of 25.
* A child no longer inherits from the wrong parent through style sharing.
  Sharing identified parents by a counter restarted on every style pass,
  so after a partial restyle, or under parents styled with `attr()`, two
  different parents could look the same.
* Relayouts no longer re-resolve, re-scan and re-key every style sheet,
  and `@import`ed sheets are parsed once rather than on every relayout,
  which also lets a page with an import restyle incrementally. Identical
  inline CSS on two pages in different directories resolves its `url()`
  values against each page's own address.
* `@container` conditions are parsed once per rule instead of for every
  element they are tested against.
* Changing the desktop colour scheme or the reduced-motion preference takes
  effect on reload; cached style sheets parsed for the old preference were
  being reused.
* `<img style="width:100%">` no longer crashes the renderer. Measuring a
  percentage-width image asked for its own natural width, which asked for
  the percentage basis again, until the stack ran out; replaced elements
  resolve the percentage against their container again.
* `srcset` is parsed as the HTML standard describes: a `data:` URL or any
  URL containing a comma is no longer cut short, malformed descriptors are
  rejected, and a `1x` candidate beats `src`. An image picked from a `2x`
  or `w` candidate is laid out at its density-corrected size and reports it
  through `naturalWidth`/`naturalHeight`, and a `<picture>` only considers
  the `<source>` elements before its `<img>`.
* Legacy presentational attributes follow the rendering section. Dimension
  attributes use HTML's parsing rules (`width="20.25e2"` is 20.25px, not
  2025px); `<font size="+1">` is relative to size 3 and nested sizes no
  longer compound; hspace/vspace take percentages and apply to embed,
  object and image buttons. Newly mapped: body topmargin/leftmargin/
  marginwidth/marginheight (including the containing frame's),
  `background`, `nobr`, `br clear`, `caption align` and legend `align`;
  `<marquee>` shows its text. Table `border`, `frame`, `rules` and
  `bordercolor` give the spec's outset/inset borders, and a width/height
  pair on img, video and image buttons sets `aspect-ratio`, taken from the
  selected `<source>` inside a `<picture>`.
* The `dir` attribute sets CSS `direction`, so right-to-left pages lay out
  flex rows and list markers from the right, not only their text.
* The default stylesheet follows the HTML rendering section instead of
  house style: lists get disc/circle/square by nesting depth with no gaps
  around nested lists; text and links use the system colours;
  `code`/`kbd`/`samp` are plain monospace; `pre` has 1em margins;
  `<dialog open>` is centred instead of pushing the page down; form
  controls no longer inherit uppercase, indent or letter-spacing. Also the
  hidden-element and form-in-table rules, `appearance: auto` on controls,
  the default iframe border with `frameborder`, and fieldset's groove
  border.
* A `<legend>` sits in its fieldset's top border, with the border broken
  around it, positioned by its margins and `justify-self`.
* The `<details>` disclosure triangle is a list marker, so `list-style:
  none` or `display: block` on the summary removes it, and content inside
  `<details>` is no longer indented 16px per level.
* Frames and objects are no longer forced with user-agent `!important`, so
  a page can hide a loaded helper iframe, and an `<object>` without a
  document shows its fallback content.
* No-doctype pages get HTML's quirks-mode rules: tables stop inheriting
  body fonts and alignment, forms keep a bottom margin, and a nowrap cell
  with a pixel width wraps.
* An image loaded into an iframe shows as an image rather than its bytes
  parsed as HTML.
* Each `<style>` element is its own style sheet. Adjacent inline sheets
  were joined before parsing, so one that ended inside an unclosed block
  or comment swallowed every sheet after it. `<style type="text/foo">`, a
  `<link>` whose `type` is not CSS and `<link disabled>` no longer apply,
  and `styleEl.disabled` / `sheet.disabled` switch a sheet off.
* A declaration whose `var()` cannot be substituted computes as `unset`
  instead of letting an earlier declaration win, as CSS Variables
  requires; a substituted value carrying `!important` counts as failed.
* Declarations written after a nested rule keep their place in the
  cascade instead of losing to the nested rule.
* A stray `;` or `}` between rules invalidates the rule after it, and a
  selector list with a trailing comma, an empty item or junk is dropped
  whole, as CSS Syntax says and other browsers do. A type selector after
  an id, class or attribute is rejected too.
* Comments inside a media query are whitespace, so
  `@media (min-width: 100px) /* desktop */` applies again.
* An inline `!important` beats a layered `!important` rule.
* `initial` gives inherited properties their real initial value instead
  of behaving like `inherit`; `color: currentColor` takes the parent's
  colour; and `bolder`/`lighter` resolve against the parent's weight with
  the CSS Fonts 4 table.
* `text-shadow`, `orphans`, `widows` and `dominant-baseline` are
  inherited, so a shadow on a container reaches its paragraphs.
* A percentage `line-height` is inherited as a length rather than
  re-applied to each child's font, and `rem` in the root's `font-size`
  refers to the initial size rather than to itself.
* Every layer of a multi-image background resolves its `url()` against
  the stylesheet, not the document.
* A `min()`, `max()` or `clamp()` inside `calc()` resolves against the real
  percentage basis instead of the window width, and NaN or infinite
  `calc()` results are clamped instead of reaching layout.
* `display: contents` computes to `none` on replaced elements, form
  controls and an outermost `<svg>`, so their fallback no longer leaks.
* `translate()` and the `translate` property keep percentages and font
  units, so the `translateX(calc(-50% + 10px))` centring idiom works, and
  `getComputedStyle().transform` no longer folds in `translate`, `rotate`
  and `scale`.
* `selectorText` and a style rule's `cssText` drop comments and write the
  attribute case flag as ` i]`.
* A flex item with a height of its own keeps it. Stretching ignored
  whether an item's height was `auto` and ignored `min-height`/`max-height`,
  so a 20px item in a 100px row came out 100px tall; only auto-height items
  stretch now, within their min and max, and the relayout that stretching
  triggers keeps the flexed width.
* A `position: fixed` box inside a transformed element is positioned and
  painted relative to that element and scrolls with it, as css-transforms
  requires, instead of being pinned to the window.
* Border-box sizing is honoured in more places: floats are placed by their
  border box, so a Bootstrap-3-style row of padded `width: 50%` columns no
  longer wraps; shrink-to-fit widths stop counting padding twice; a
  percentage height inside a border-box parent resolves against its content
  box; and tables honour `box-sizing` and are border-box by default, so a
  bordered `width: 100%` table no longer overhangs its container.
* A float is never narrower than its longest word, and a `width: 0` child no
  longer widens a shrink-to-fit parent.
* Absolutely positioned boxes shrink-wrap including their margins, padding
  and border, honour `min-height`/`max-height`, find their static position
  inside their parent rather than after it, and treat `margin: auto` as 0
  unless both `left` and `right` are set.
* A last child's bottom margin stays inside `overflow: hidden` blocks,
  floats, inline-blocks and parents with a fixed or minimum height instead
  of escaping through them, and the document contains the body's bottom
  margin.
* Flexbox: images and other replaced items stretch across column
  containers, items move with their line when `align-content` grows it, and
  row and column follow a vertical `writing-mode`.
* `min-content`, `max-content`, `fit-content` and `stretch` work in
  `min-width`, `max-width`, `min-height` and `max-height`, and as flex item
  sizes.
* Text around an HTML comment or a hidden element keeps its spaces ("Text
  after", not "Textafter").
* `<center>` and `align="center|right"` move block-level children too,
  including a table nested in an aligned cell.
* `document.fonts.load()` and `FontFace.load()` wait until the web font has
  loaded, and `document.fonts.ready` resolves with the FontFaceSet, so
  scripts that measure text after them see the web font's metrics.
* A text input's line height is never smaller than `normal`, so a reset
  such as `input { line-height: 1px }` no longer clips the field.
* Viewport units in an iframe resolve against the frame, even when a
  stylesheet sizes the frame, and `calc()` expressions with vw/vh follow the
  current viewport rather than the one in effect when the stylesheet was
  parsed.
* An embedding page can no longer read a cross-origin frame's document.
  `iframe.contentDocument` returned the framed page's DOM and
  `contentWindow` its real window whatever their origin, so any page
  could frame another site and read or script it. For a cross-origin
  frame, or one sandboxed without `allow-same-origin`, `contentDocument`
  is now `null` and `contentWindow` (and `window.frames[n]`) a restricted
  window that allows only `postMessage`, the `location` setter, `closed`,
  `length`, `window`/`self`/`frames`/`parent`/`top`/`opener` and
  `close`/`focus`/`blur`. Same-origin, `about:blank` and `srcdoc` frames
  are unaffected.
* `window.scrollTo()`, `scroll()` and `scrollBy()`, and setting
  `scrollTop`/`scrollLeft` on the root element, move the page. They only
  changed the position script read back; the view stayed where it was. They
  now lay out if needed, clamp to the scrollable range and scroll the
  viewport in both axes. `scrollBy()` called bare, as it almost always is,
  read its position from `undefined` and scrolled to NaN, and `scrollY`
  briefly snapped back to the old position before the view caught up.
* `postMessage`'s `targetOrigin` is checked against the origin of the
  window the message goes to. The check read that window's `location`,
  which answers with the URL of whichever frame's script is running, so
  when a frame posted to its parent, the parent seemed to have the
  frame's own origin: a message addressed to the parent's real origin was
  dropped, and one addressed to any other origin -- the case
  `targetOrigin` exists to stop -- was delivered. The top-level window's
  origin now comes from its document, `"/"` means the sender's own origin
  instead of matching everything, and an origin is compared as an origin,
  not as a string prefix.
* A cross-origin iframe can no longer script the page that embeds it.
  A frame's `parent` and `top` were the embedding page's real window, and
  the frame's global object inherited from it, so a framed site could read
  and rewrite the embedder's DOM, `document.cookie` and `localStorage`
  (only its own storage threw SecurityError). A cross-origin frame now
  gets a restricted window proxy for `parent` and `top`, exposing only
  `postMessage`, the `location` setter, `closed`, `length`,
  `window`/`self`/`frames`/`parent`/`top` and `close`/`focus`/`blur`, and
  throwing SecurityError for anything else. Its global inherits
  `Window.prototype` rather than the parent's global. Messages it
  exchanges with the parent carry the right `source` in both directions.
  A frame sandboxed without `allow-same-origin` is treated the same way,
  whatever its URL.
* A framed document's `document.cookie` reads and writes the cookies of
  the frame's own URL. It used to return the embedding page's cookie
  string, and assigning to it replaced that string. Documents that have no
  browsing context -- from `DOMParser`, `cloneNode` or
  `createHTMLDocument` -- are cookie-averse and return the empty string,
  as HTML specifies. The `Document.prototype` accessor also no longer
  hands the top-level page's cookies to another document it is called on.
* An iframe whose load handler navigates it again no longer hangs the
  page. Loading a queued frame fires its load event synchronously, so a
  handler that set `src` once more re-queued the same frame and the loader
  never returned: timers, painting and the rest of the page starved while
  the renderer spun at full CPU. Each pass now loads a frame at most once;
  a frame queued again waits for the next tick.
* A multi-column block splits a list, not just a run of siblings. The
  column code distributed a container's own children and gave up below
  two of them, so `column-width` on a wrapper whose sole child is an
  `<ol>` -- which is how a Wikipedia reference list is built -- laid the
  whole list out in one column. A lone in-flow block child is looked
  through now and its children distributed instead.
* A multi-column block establishes a block formatting context, as the
  spec says, so it sits beside a float rather than under it.
* A block that establishes a formatting context is placed clear of every
  float it spans, not just those beside its top edge. It was narrowed
  against the float band at its first line and kept that width all the
  way down, so a wider float lower down overlapped it.
* A table column is never narrower than its cells' contents need. The
  auto layout measured a cell's minimum through measure_min_width, which
  returns a specified width when it has one, so a cell's own width
  doubled as its minimum: `width: 1%` on a heading cell -- the idiom
  Wikipedia's navboxes use to shrink a column to its label -- left the
  column one per cent of the table wide and its text ran across the cell
  beside it. The floor is the cell's min-content width now, and when the
  minimums together exceed the width the table asked for, the table grows
  to their sum instead of scaling every column down below what it can
  hold, as CSS 2.1 requires.
* `min-width` and `max-width` on a table cell take part in the column
  measures, clamped max-then-min the way the rest of the box model is.
* A table cell inherits `text-align` from the table or the row. The
  default stylesheet pinned `td, th` to `text-align: left`, which no
  browser's does, so `text-align: center` on a `<table>` or a `<tr>`
  reached the caption and nothing else. `vertical-align` likewise moves
  from a pinned `middle` on the cell to the spec's arrangement -- the row
  groups carry it and the cells inherit -- so a row can set it, and
  `align`/`valign` now map on the row, row-group and column elements too.
* An inline-block whose width is a percentage no longer drags the
  intrinsic width of whatever contains it up to the width of the page:
  a percentage is indefinite while intrinsic sizes are measured, so the
  atomic is measured against its own content.
* `content: '[' / ''` renders just the bracket. The alternative text a
  `content` value carries after a slash, for a screen reader to read in
  place of the glyphs, was drawn as part of the text, so MediaWiki's
  section-edit links came out as `[/ edit ]/`.
* A list item styled `display: inline-block` or `display: block` draws no
  bullet; only a `list-item` display generates a marker.
* `<th>` paints no background of its own and `<caption>` is not bold,
  `<figcaption>` is not italic, and `<figure>` and `<dl>` carry the
  margins the HTML rendering rules specify. None of these are in a
  browser's default sheet, and each showed through wherever a page paints
  its own tables or figures.
* The Android app is about 8 MB smaller. The build staged every shared
  library in the dependency sysroot into the APK, including ones this
  build does not link at all -- llama/ggml, gobject-introspection and the
  unused harfbuzz and pcre2 variants -- so 19 of the 51 libraries per ABI
  were dead weight. Only the engine's `DT_NEEDED` closure is packaged now.

1.0.25:
======
* Restyling a large document is roughly 40% faster. The style-sharing
  cache builds a lookup key for every element on every cascade pass by
  serialising that element's matched declarations; on Speedometer 3.1's
  6,650-node complex-DOM pages that key cost more than the cascade it
  was meant to avoid — 76ms of a 190ms pass. The key is now written
  once into a correctly sized buffer instead of through ~2 million
  incremental byte-array appends, the container-query part of the key
  no longer scans every matched custom property when no container is on
  the stack, and the key hash reads eight bytes at a time. Selector
  gathering also jumps straight to the pseudo-element bucket a matched
  selector belongs to rather than testing all ten. The same pass now
  takes 120ms. Across the 22 loadable Speedometer 3.1 TodoMVC workloads
  the aggregate score improves 31.7% (1.328 to 1.749); Vue-Complex-DOM
  drops from 4810ms to 881ms and jQuery-Complex-DOM from 4948ms to
  3140ms. Layout output is byte-identical.
* The toolbar takes the classic look of the Northstar web browser: a
  raised, softly shaded bar with labelled colour buttons for Back,
  Forward, Reload, Stop, Home, Print and Downloads, bevelled hover and
  pressed states, etched separators, an inset address field that shows
  a page icon when there is no certificate state to report, labelled
  Bookmarks and Menu buttons, and the Nordstjernen logo on a dark tile
  at the far right. Stop stays in place and greys out when nothing is
  loading instead of disappearing, Print and Downloads have their own
  toolbar buttons, the Go button is gone (Enter and the address bar do
  the same thing), and the bookmark button uses the bookmark icon, which
  fills in with a gold star when the current page is saved. The README
  screenshot shows the new toolbar.
* Fullscreen mode is announced: when a page calls requestFullscreen the
  shell overlays a notice at the top of the page naming the site's host
  and saying it is now full screen and that Esc exits, in the style of
  Chrome and Firefox, so a page can't hide the address bar and paint a
  spoofed one without the user being told. The notice stays for five
  seconds and can't be covered by page content. Element fullscreen now
  also ends by itself when the fullscreen tab navigates to a new
  document, when another tab is switched to, or when the tab is closed —
  the header and toolbar used to stay hidden across all three. And per
  the Fullscreen API, requestFullscreen() now needs transient user
  activation: called from a timer or on load without a recent click,
  tap or key press it rejects with a TypeError, fires fullscreenerror
  and logs to the console instead of taking over the screen.
  (Reported by Muhammad Wishal.)
* The nightly .deb installs again on a Debian that is a patch release
  behind the build container. dpkg-shlibdeps copied Debian's FFmpeg
  shlibs floor, which is the exact upstream version of the build host's
  FFmpeg, so the package demanded e.g. libavcodec61 (>= 7:7.1.5) and
  dpkg refused it on a system with 7:7.1.1 although the SONAME, and so
  the ABI, is the same. scripts/pack-deb.sh now relaxes the libav* and
  libsw* floors to the FFmpeg major.minor release (>= 7:7.1) while the
  SONAME-numbered package names keep guarding the ABI, logs the final
  Depends line, and no longer Recommends an external media player the
  shell doesn't launch. The container build then installs the .deb, .rpm
  or .apk it produced and runs the installed browser headlessly, so a
  package whose metadata, maintainer scripts, dependencies or installed
  paths are broken fails the nightly stage instead of reaching the
  download links. (Reported by guest271314.)
* The nightly download links no longer 404 when one platform's build
  fails: scripts/nightly.sh publishes a stage's directory only once it
  holds artifacts and otherwise keeps the previous night's files, a
  container that fails after pack-linux.sh still ships its portable zip,
  the Linux zip link falls back from the Ubuntu build to the Debian or
  openSUSE one, dangling stable links are removed instead of left to
  404, and MANIFEST.txt records why a stage failed and that its files
  are stale.
* The README's download table lists the Windows Store and Google Play
  listings and the source release tags; the nightly build links are
  gone from it.
* The .deb no longer bundles the dynamic loader: pack-deb.sh's core
  runtime deny list matched ld-linux only when a dot followed the name.
* The lexbor encoding module is trimmed to UTF-8. Its 43 legacy codecs
  (Big5, GB18030, EUC-JP, Shift_JIS, the ISO-8859 and Windows code
  pages, UTF-16 and the rest) had no caller: page bodies are decoded
  through uchardet and g_convert, and lexbor's own URL parser only ever
  needs UTF-8, yet its codec lookup table kept every conversion table
  alive through the linker. Dropping them removes 11 MB of generated
  source, about 1 MB from each binary that links lexbor, and the
  per-encoding branches from the URL parser's query serializer.
* The C DOMMatrix in js_canvas.c is gone. The startup polyfill defines
  the 4x4 DOMMatrix, DOMMatrixReadOnly, DOMPoint and WebKitCSSMatrix
  and assigned them over the native constructors on every page, so the
  C class was dead at runtime while canvas getTransform() and the SVG
  getCTM()/getScreenCTM() helpers still minted the C flavour, which
  failed instanceof DOMMatrix. They now construct through the page's
  DOMMatrix constructor, and about 250 lines of duplicate matrix code
  and their declarations are removed.
* The startup polyfill no longer stubs the Credential Management API:
  navigator.credentials resolved every call to null and Credential,
  PasswordCredential, FederatedCredential, PublicKeyCredential and
  IdentityCredential were empty data holders, so sites that feature
  detect WebAuthn or password autofill took a code path that could only
  fail. With the properties absent they fall back to plain forms.
* Repository weight: the 1.0.22, 1.0.23 and 1.0.24 splash frames
  (1.4 MB of PNGs nothing referenced) are deleted, and the QuickJS
  tree drops the harness sources the build never compiled (api-test.c,
  lre-test.c, fuzz.c, ctest.c, cxxtest.cc and the WASI reactor) with
  their CMake and Makefile targets.
* CI moves to current toolchains: CodeQL Action v4 (v3 is retired in
  December 2026), setup-java v6, actions/cache restore and save v6 on
  Windows, FreeBSD 15.1 and NetBSD 11.0 VMs (14.2 and 10.0 are past
  end-of-life), NDK r30 (30.0.16248370) for the Android and CodeQL
  builds, and the V8 15.2 monolith for the js_engine=v8 job. The V8
  backend follows the V8 15 embedder API, which requires a type tag on
  every v8::External and aligned internal-field pointer, and the build
  now passes the pointer-compression defines the just-js monolith is
  built with; docs/V8.md notes that the CREL relocations in those
  releases need lld 19 or newer. NDK r30's bionic refuses
  malloc_usable_size under _FORTIFY_SOURCE=3, so the QuickJS allocator
  reports the usable size as unknown on Android instead of calling it.
* about:start wears the Northstar web browser's splash, carried over
  from that project's scripts/gen-splash.py and retitled "Nordstjernen
  web browser" with this release's version read from meson.build: a
  flat, sunny xkcd-style comic of Noah's ark in Comic Neue lettering,
  with the animal pairs walking two by two to the gangplank behind Noah
  and his clipboard ("Two of each. Yes, even browsers."), a wooden ark
  with a flag, portholes, a chimney and a penguin on deck, blue wavy
  water with a spouting whale, a red-and-white lighthouse with its
  keeper waving from the gallery, gulls, a dove, drifting clouds and a
  yellow sun. Every frame is drawn at three times supersampling and the
  32 frames (walking gaits, a bobbing ark, a fluttering flag, chimney
  smoke, spinning sun rays, flapping gulls) are quantized to one shared
  256-colour palette and squeezed by gifsicle into the embedded GIF.
  scripts/gen-splash.sh regenerates it and writes the first frame to
  data/about-splash-<version>.png; the earlier build-splash-art.py and
  build-splash-gif.py generators are gone.
* Container queries evaluate the full condition grammar: not/and/or
  with nesting, size features in plain, boolean and range form
  (double-sided ranges and math functions included), aspect-ratio and
  orientation, unknown features that make the enclosing condition
  false, comma-separated condition lists, name-only rules and vertical
  writing-mode containers. Invalid preludes are dropped from the engine
  and the CSSOM, conditionText serializes canonically, container-name
  and the container shorthand validate their values, and
  CSSContainerRule exposes containerName, containerQuery and
  conditions.
* image-set() is validated against the CSS Images 4 grammar (url and
  string images, gradients, resolutions in x, dppx, dpi and dpcm, a
  type() hint, no duplicate or missing options) and serializes
  canonically; unicode-range validates and canonicalizes its ranges
  (U+26, u+0-7F, U+4?? read back U+26, U+0-7F, U+400-4FF).
* An absolutely positioned box whose static position comes from an rtl
  ancestor sits at that ancestor's content-right edge, and the used
  values of auto insets are reported physically. font-family keeps
  random-item() and -webkit-generic() items, validates random-item()'s
  arguments and rejects a generic family inside a multi-word name; the
  background-position and object-position shorthands accept CSS-wide
  keywords; specified grid track lists serialize their line names.
* Transitions and animations run for every property. The animation
  engine keeps one channel per (element, property): a transition starts
  whenever a property named by transition-property (or "all") computes
  to a different interpolable value, lengths, percentages, calc()
  mixes, numbers, colours, shadow lists and op-compatible transform
  lists interpolate, visibility flips discretely, and keyframe
  animations sample every declared property between the surrounding
  keyframes, so margins, sizes and colours animate, not just opacity
  and transform. Interpolated values are written into the computed
  style after each cascade, so layout, paint and getComputedStyle see
  the in-flight value; a frame whose animated properties affect layout
  marks the page for relayout.
* The Web Animations surface: document.getAnimations() and
  element.getAnimations() return CSSTransition and CSSAnimation
  objects (stable identity per element, property and run) with
  currentTime and startTime that seek the engine, playState, pending,
  the ready and finished promises, play/pause/finish/cancel, the finish
  and cancel events, transitionProperty/animationName and an
  AnimationEffect whose target, getTiming() and getComputedTiming()
  describe the run. Element.animate() builds a keyframe animation from
  a keyframe list or a property-indexed object with duration, delay,
  iterations, direction, fill and easing.
* Several CSS animations run on one element, one per animation-name
  entry, each with its own duration, delay, timing function, iteration
  count, direction, fill mode and play state from the animation
  longhands; animation-timing-function, animation-iteration-count,
  animation-direction, animation-fill-mode, animation-name,
  transition-property and transition-timing-function are real
  longhands that parse, cascade and serialize on their own, and
  AnimationEvent and TransitionEvent are constructible.
* The animation and transition shorthands parse per comma-separated
  item against their full grammar (a time is a duration before it is
  a delay, an easing keyword, steps() or cubic-bezier(), a
  transition-behavior keyword, "auto" and "none" where allowed),
  expand into their longhands with the omitted ones reset, and
  style.animation, style.transition and getComputedStyle rebuild the
  shorthand from the longhands in canonical order. animation-timeline,
  animation-range-start, animation-range-end, animation-composition and
  transition-behavior are properties.
* Animation and transition events fire by phase: animationstart at
  the end of the delay, animationiteration on every iteration boundary,
  animationend once, transitionrun when the transition is created,
  transitionstart after its delay, transitionend when it completes,
  and the cancel events when a run is interrupted; elapsedTime and
  pseudoElement are filled in. A transition can start from an unset
  value (the property's initial value) and a keyframe that leaves a
  property out fills it from the base style.
* attr() is substituted at cascade time, so content: attr(data-x)
  and attr() in other properties follow attribute changes; an attr()
  URL is tainted. Boxes honour width: stretch and height: stretch,
  grid item margins are resolved against the grid area, a canvas
  takes its width and height attributes as dimension hints, and the
  specified style serializes shorthands from their longhands.
* aspect-ratio keeps its numerator and denominator (16 / 9 reads back
  16 / 9), an absolutely positioned box with both insets set is
  aligned inside them by justify-self and align-self rather than
  stretched unconditionally, and a shrink-to-fit abspos box measures
  against the inset width.
* A declaration whose value is exactly one {}-block is a declaration
  rather than a nested style rule when the CSSOM splits a style block,
  and a custom property keeps a {}-block anywhere in its value.
* A table's max-content and min-content widths are measured column by
  column, as css-tables-3 requires: each column takes the widest cell
  it holds, the columns are summed once with the border spacing, and
  captions widen the result. They used to be the sum of every row, so
  a table in a flex or grid item, a floated infobox and a table nested
  in a cell reported several times their real width.
* position: fixed elements stay anchored to the viewport while the
  page scrolls: paint offsets a fixed box by the viewport origin,
  hit-testing applies the same offset so clicks land on the fixed
  element, getBoundingClientRect reports its viewport position, and
  mouse events carry viewport-relative clientX/clientY with document
  coordinates in pageX/pageY. position: sticky boxes are hit-tested
  where they paint through one shared ns_box_sticky_offset that
  resolves percentage and calc() insets against the scrollport and
  measures a sticky box inside an overflow container against that
  container's padding box.
* box-shadow and text-shadow serialize their specified value in
  canonical order (colour, offsets, blur, spread, inset) with 0
  written as 0px, and reject the forms the grammar excludes: a lone
  length, a fifth length, two colours, inset twice, a negative blur,
  a percentage, or a colour splitting the lengths. A shadow without a
  colour takes currentcolor from the computed color, and rgb(0, 255,
  0) with spaces inside the parentheses no longer splits into tokens.
  A colour keyword keeps its lowercase spelling in the specified style
  and the deprecated CSS2 system colours map to their CSS Color 4
  replacements.
* The background shorthand is parsed layer by layer against the
  css-backgrounds grammar: every comma-separated layer sets all eight
  longhands (image, position, size after the slash, repeat,
  attachment, origin and clip, with the colour on the final layer) and
  a longhand the layer leaves out resets to its initial value, so
  background: red no longer keeps an earlier background-image.
  background-attachment is a property, background-clip,
  background-origin and background-attachment take comma-separated
  lists, paint resolves origin and clip per layer, clips the colour by
  the last layer's clip and positions a background-attachment: fixed
  layer against the viewport, background-position keeps the keywords
  it was written with, background-position-x/-y accept x-start,
  y-end and an edge with an offset, and getComputedStyle composes
  border-radius and background-position from their longhands.
* -webkit-border-radius and the -webkit-border-*-radius corners are
  aliases of the unprefixed properties, and the border-radius
  shorthand is validated before any corner is written: a fifth value,
  a negative radius or a second slash rejects the declaration, and the
  specified value collapses each half as a quad.
* transform is validated function by function against css-transforms:
  each function checks its argument count and types, so translate(1px,
  2px, 3px), scale(6, 7, 8) and skewX(0, 0) are rejected, and the
  specified value serializes canonically (percentages in scale()
  become numbers, rotate(0) reads rotate(0deg), 0 lengths read 0px,
  function names are lowercased). The scale, rotate and translate
  properties get the same treatment, transform-origin and
  perspective-origin follow the position grammar, perspective: 1000
  without a unit is rejected, and transform-box is a property.
* The border, border-top/right/bottom/left, border-block-*,
  border-inline-*, outline and column-rule shorthands are validated
  against their grammar before any longhand is written: a second
  width, style or colour, a negative or percentage width, a unitless
  number other than zero or an unknown keyword rejects the whole
  declaration instead of leaving a partial expansion behind, and the
  outline and column-rule shorthands reset the longhands they leave
  out. outline-style accepts auto.
* Percentage border radii resolve against the box (border-radius: 50%
  on a 200x100 box is an ellipse, not a 50px circle), a corner takes a
  horizontal and vertical radius pair (border-top-left-radius: 10px 5px)
  and the border-radius shorthand honours the vertical radii after the
  slash; paint draws elliptical corners and scales overlapping radii
  down together as css-backgrounds requires, and em/rem pairs resolve
  against the font size in the computed style.
* A transition from transform: none no longer resets the target
  transform to identity: the interpolation built its identity endpoint
  on an alias of the target value and zeroed it in place. An animated
  value propagates to descendants only for inherited properties, so a
  child sharing the parent's opacity, width or transform value is no
  longer animated along with it. A transition on a property paint does
  not read live (box-shadow, filter, border colours, visibility) now
  restyles the page each frame instead of showing the stale value.
* A frame's parent.postMessage() reaches the listeners the top document
  added with addEventListener: the message used to be dispatched
  against the frame's own document, so only window.onmessage saw it.
* Setting style.borderTopLeftRadius (or another corner) on an element
  whose inline style carries a later border-radius appends the corner
  after the shorthand, so the new value wins as the CSSOM requires.
* When several options of a single-choice <select> carry the selected
  attribute, the last one wins, as the HTML selectedness setting
  algorithm requires. HTMLOptionsCollection exposes selectedIndex, and
  every event the engine dispatches carries a composed flag.
* quickjs-ng is at v0.16.2: bytecode constant pools are 8-byte
  aligned, proxy traps consult IsExtensible() on the target so a
  nested proxy's trap is observable, ownKeys must return an object,
  the array iteration builtins poll for interrupts so a long loop
  stays interruptible, TypedArray.prototype.with converts through
  ToBigInt on the 64-bit arrays, and the regexp parser rejects the
  identity escapes that are invalid in unicode mode outside a class.
  WAMR is at 2.4.5: the constant-expression loader rejects an invalid
  reference type in ref.null and the fast-interpreter constant table
  can no longer desynchronise its two passes.

1.0.24:
======
* about:start wears a new splash: the same night scene, painted in an
  impressionist hand with a gilded frame, rendered by
  scripts/build-splash-art.py from the version in meson.build so a
  release regenerates the artwork with two scripts and no image editor.
* Flex layout resolves flexible lengths the way css-flexbox-1 §9.7
  describes: one implementation shared by row, wrapping-row and column
  containers distributes free space with the item freezing loop, so flex
  factors below one scale the free space, flex-shrink is weighted by the
  flex base size, and min/max violations are frozen and re-distributed.
  The automatic minimum size of a flex item is min(content size,
  specified size) rather than the specified size, so width: 200px in a
  100px container shrinks as browsers do; min-content and max-content
  minimums are honoured, and a percentage size on a replaced element or
  text control counts as zero for the specified size suggestion.
* Column flex containers wrap: flex-wrap: wrap and wrap-reverse break
  items into lines against the definite main size, align-content places
  the lines (start/end resolve in the inline axis; space-around and
  space-evenly fall back to start when the lines overflow), a wrapping
  container with a single line is still multi-line, rtl mirrors the
  cross axis, column-reverse packs from the main end, auto margins in
  the main axis absorb free space, and an indefinite-height column sizes
  itself from its items' content contributions so flex: 1 items no
  longer collapse.
* Negative free space overflows in the right direction: space-between,
  space-around and space-evenly fall back to start, flex-end and center
  overflow the start edge, and a scroll container packs overflowing
  content toward its start so it stays reachable; row wrap-reverse
  mirrors lines against the container's definite height.
* scrollWidth and scrollHeight include the scroll container's end
  padding and, in rtl, overflow to the left. offsetTop and offsetLeft
  round negative values to nearest and flush pending layout before
  locating the offset parent.
* The static position of an absolutely positioned flex child honours
  start, end, left and right on justify-content and align-self as
  writing-mode-relative keywords, self-start/self-end use the child's
  own direction, and last baseline aligns to the cross end.
* align-items, align-self, align-content, justify-content, justify-items
  and justify-self parse first baseline, last baseline and the safe and
  unsafe prefixes.
* WPT css/css-flexbox: 1465 -> 1997 of 3670 subtests on a 2026-09
  checkout of the horizontal-writing-mode suites.
* An absolutely positioned box whose containing block is a grid
  container takes that block from its grid-column and grid-row lines,
  as css-grid-1 §9 requires: a line inside the explicit grid resolves to
  the edge of the adjacent track, a line outside it, an unknown name or
  a span resolves to the padding edge after the start/end swap, and
  offsets, percentages and shrink-to-fit sizes resolve against that
  area. A shrink-to-fit abspos box no longer squeezes below its
  min-content width when the area is narrower than its content.
* align-content: stretch on a grid container distributes free block
  space only to rows whose max track size is auto; fixed-length rows
  kept their length.
* An absolutely positioned element with an inline-level display that
  follows inline content takes its static position from the line it
  would have occupied, after the preceding text, instead of the top of
  the block.
* document.fonts.ready waits for the web fonts the page needs: it
  flushes style so pending @font-face loads are requested, resolves
  once the loader is idle and marks the document for relayout;
  fonts.status reports loading meanwhile.
* Alignment properties keep their full specified keyword: safe and
  unsafe prefixes, legacy left/center/right, first baseline (computed
  as baseline) and last baseline parse and serialize, and the
  place-self, place-items and place-content shorthands split two-word
  values and serialize a repeated value once.
* offsetTop/offsetLeft flush pending layout before locating the offset
  parent, so a first read during parsing no longer returns viewport
  coordinates.
* Grid containers with direction: rtl lay their columns out from the
  right, and grid-placed absolutely positioned boxes mirror with them.
* repeat(auto-fit, ...) collapses the repeated tracks that no in-flow
  item occupies, so a card grid with fewer cards than columns stretches
  the remaining fr tracks as browsers do.
* A grid item's percentage height resolves against its grid area when
  the rows it spans have definite track sizes, and an absolutely
  positioned child of a grid container with auto offsets takes its
  static position from its grid area, aligned by justify-self and
  align-self.
* getComputedStyle on a grid container returns the used track sizes for
  grid-template-columns and grid-template-rows, with explicit line names
  in place, as CSSOM requires.
* Track lists resolve em against the element's own font size and calc()
  percentages against the track axis; an auto track grows to its
  max-content contribution before fr tracks share the remainder, fr rows
  fill a definite container height, over-constrained minmax() rows
  shrink toward their minimum, percentage rows in an indefinite-height
  grid re-resolve against the final height, and an auto column measures
  its content with real text metrics.
* The grid track-list parser rejects negative sizes, stray commas,
  consecutive or trailing-only line-name lists, reserved words as line
  names, a second auto-repeat and non-fixed tracks beside one, and
  supports line names inside repeat().
* JavaScript can be turned off: an "Enable JavaScript" toggle in
  Settings parses pages with scripting disabled (so noscript content
  renders) and skips script execution entirely — the user decides what
  runs, in the old Mozilla tradition.
* about:mozilla shows the maroon page every browser of this lineage
  owes its readers, and about:config opens the settings page.
* The status bar reads "Done" for a moment when a page finishes
  loading, as it always did.
* Fixed a use-after-free of the session URL: timer, event-dispatch and
  requestAnimationFrame callbacks saved the current URL pointer and restored
  it unconditionally after the callback, so a handler that navigated (a
  fragment click, location.hash =, history.pushState) left the engine
  reading and double-freeing a freed URL. The URL is now restored only when
  it was actually swapped for an iframe realm.
* Added view-source: — Ctrl+U / "Page Source" in the menu shows the current
  page's HTML with classic syntax highlighting. Only chrome-initiated
  navigations can use the scheme; web content is refused.
* input.showPicker() and select.showPicker() are implemented per spec:
  InvalidStateError on disabled or readonly controls, NotAllowedError
  without a user gesture, and a successful call consumes the activation.
  WPT show-picker suites pass 129/129.
* stepUp()/stepDown() follow the spec: they throw InvalidStateError on
  non-numeric input types and step="any", honor the per-type default
  step and scale, round to the step grid, clamp to min/max, and
  serialize date, month, week, time and datetime-local values back to
  their canonical strings. WPT input-stepup 53/53 and time 32/32.
* The color input sanitizes through the CSS color parser: keywords,
  rgb() and #rgb shorthand normalize to lowercase six-digit hex, and
  surrounding whitespace is stripped.
* Label association follows the spec: label.control resolves the for
  attribute against the label's own tree to the first element with that
  id (null when it is not labelable or the attribute is empty),
  label.form returns the associated control's form owner, and .labels
  is a live NodeList.
* The WPT harness's testdriver bridge grants real user activation for
  simulated gestures, and the __nsWpt* hooks are inert outside the
  harness.
* Form constraint validation follows the HTML spec much more closely:
  ValidityState flags are computed for disabled and readonly controls
  (bars from validation affect willValidate, not the flags), valueMissing
  is suppressed on disabled/readonly controls, the pattern attribute
  compiles as a JavaScript regular expression with the v flag (invalid
  patterns are ignored) and applies to each address of a multiple email
  input, tooLong/tooShort fire only after a user edit as the spec's dirty
  value flag requires, and willValidate is false inside a datalist.
  WPT form-validation: patternMismatch, tooLong, tooShort, typeMismatch
  and badInput suites now fully pass.
* Live HTMLCollection/NodeList property semantics follow WebIDL: silent
  sloppy-mode failures and strict TypeErrors for read-only indexed and
  named properties, spec-compliant descriptors, expando support, and
  Object.keys listing only indices; moveBefore() is ParentNode-only;
  replaceChildren() queues a single mutation record; Node.isConnected is
  true for any node whose root is a document.
* JavaScript can be turned off: an "Enable JavaScript" toggle in
  Settings (and the NS_NO_JAVASCRIPT=1 environment variable) parses
  pages with scripting disabled, so noscript content renders, and skips
  script execution entirely.
* about:mozilla shows the maroon page every browser of this lineage
  owes its readers, and about:config opens the settings page.
* The status bar reads "Done" for a moment when a page finishes
  loading, as it always did.
* Popup blocking, in the Firefox 1 tradition: window.open only navigates
  when called within five seconds of a real user gesture (click, key
  press, touch), consumes that activation, and logs blocked attempts to
  the console. navigator.userActivation now reports the live activation
  state instead of constants.
* Ctrl+Enter in the address bar completes a bare name to www.name.com,
  Shift+Enter to .net and Ctrl+Shift+Enter to .org, as in classic
  Firefox.
* Ctrl+Shift+R and Ctrl+F5 reload the page bypassing the HTTP cache.
* Ctrl+D bookmarks the current page, with a matching "Bookmark This Page"
  menu entry, in the classic browser tradition.
* about:book — every browser of the lineage carries its Book.
* Updated the ns-pango subproject pin to the latest upstream commit.
* Optimized event dispatch throughput by lazily evaluating composedPath()
  arrays on demand using active dispatch paths.
* Added node-level listener filtering with NS_NODE_HAS_LISTENERS flag, skipping
  listener array iterations on intermediate DOM tree nodes without listeners.
* Bound standard Event prototype methods (preventDefault, stopPropagation,
  stopImmediatePropagation, cancelBubble, composedPath) directly on Event.prototype.
* Accelerated document.createElement with lowercase ASCII fast paths and
  ASCII-first element name validation.
* Optimized inline style property conversions (camel_to_kebab) and empty initial
  inline style updates avoiding intermediate GString allocations.
* Optimized dataset property lookups by matching target attribute names directly
  without repeated per-attribute string allocations.
* Bypassed redundant storage allocations and event dispatches when setting
  identical localStorage/sessionStorage values.
* Added HTMLDialogElement.showModal() standards compliance validating open and
  connected document state.
* Aligned structuredClone with specification requirements (argument validation
  and transfer options support).
* Optimized CSS.escape for simple identifiers and bounded selector and form
  ancestor traversals.
* Optimized DOM textContent and innerText get/set paths with zero-allocation
  string returns for empty and single text child nodes.
* Optimized DOMTokenList (classList) add, remove, and toggle with single-token
  fast paths avoiding token array and parser allocations.
* Accelerated DOM hierarchy validation in pre-insert checks and ancestor-or-self
  queries with O(1) leaf child checks and bounded traversals.
* Optimized DOM attribute operations (getAttribute, setAttribute, hasAttribute,
  toggleAttribute, removeAttribute) with zero-allocation lowercase ASCII fast paths
  and single-pass string conversion.
* Fast-path selector matching in Element.matches() and Element.closest() for simple
  class (.cls), tag (tag), and id (#id) selectors, avoiding full CSS selector AST
  allocations on delegated event lookups.
* Subtree getElementById queries leverage document root ID indexes and bloom filters
  prior to falling back to full recursive DOM tree traversal.
* CSS line-height unit calculations now cover all root font relative units
  (rem, rlh, rex, rch, rcap, ric), element font relative units (lh, ex, ch,
  cap, ic), viewport percentage units (vh, vw, vmin, vmax, vi, vb, svh, svw,
  lvh, lvw, dvh, dvw), and container query units (cqw, cqh, cqi, cqb, cqmin,
  cqmax) across text layout and painting.
* DOMTokenList (classList) optimizes add and remove operations to skip
  redundant attribute re-serializations and DOM mutation dispatches when the
  underlying token set is unchanged.
* AbortSignal spec compliance: AbortSignal.abort() and AbortSignal.timeout()
  generate standard DOMException instances (AbortError, TimeoutError),
  AbortSignal.any() accepts any iterable signal collection, and prototype
  chains correctly inherit from EventTarget.
* Norwegian regional Accept-Language configuration adds complete fallbacks
  across Bokmål (nb), Nynorsk (nn), and generic Norwegian (no) locales.
* Parser and layout loop safety improvements: bounded counter formatting
  buffers, guaranteed forward pointer progress in pseudo content resolution,
  unified depth-bounded ns_node_root tree traversals, and cycle-protected
  document order comparisons.
* Selection highlights the text it is actually on. The highlight was
  painted as one flat pass over the finished page, in document
  coordinates, after everything else had been drawn — so inside a
  scrolling box it landed wherever the text would have been unscrolled,
  spilled past the box it belonged to, and ignored the transforms and
  clips the glyphs themselves were drawn under. It is now drawn where the
  text is drawn, from a range table computed once per frame and looked up
  per box, so it inherits that box's scroll offset, transform and clip by
  construction. It also goes down before the glyphs rather than over them,
  which is what lets `::selection` carry an opaque background without
  washing the letters out. Selection hit-testing gained the two things the
  click path already had: it adds a scrolling ancestor's scroll offset as
  it descends, so a click inside a scrolled `<div>` selects the line under
  the pointer instead of the line that would be there at scroll zero, and
  it stops at a box that clips its children, so a point below an
  `overflow: hidden` container no longer reaches the content clipped out
  of it.
* `user-select` and `::selection` are read from where the style is. Both
  were looked up on the inline box's own `style`, which inline boxes do
  not carry — the style lives on an ancestor — so the pointer was always
  null and both properties were silently ignored: `user-select: none` text
  copied anyway, and a page's `::selection` colours never appeared. Both
  now walk to the nearest ancestor that has a style, the same way the text
  painter finds its font.
* Copied text reads like the page. Every inline box ended with a newline,
  so a paragraph broken into runs by a `<b>` or an inline-block came out
  one word per line; a `<br>` — which layout carries as U+2028 — came out
  as nothing at all. A line break is now emitted where a block boundary
  is, U+2028 and U+2029 become newlines, and the zero-width characters
  `<wbr>` leaves behind are dropped. Text under `user-select: none` is no
  longer collected at the ends of a range, only in the middle of one.
* Double-click selects a word, triple-click selects the block. Both
  gestures previously did what a single click did. Word edges come from
  the shaper's own break attributes rather than an ASCII rule, so they
  hold for scripts that do not put spaces between words. Shift-click
  extends the existing selection from its anchor instead of dropping it,
  and a drag that selected text no longer activates the link it ended on —
  releasing the mouse after selecting a sentence containing a link used to
  navigate away.
* A page can see and set the selection it is showing. `getSelection()`
  reported an empty, collapsed selection no matter what was selected on
  screen, because nothing ever told the JavaScript engine what the
  selection was; `document.execCommand` answered false to everything and
  `navigator.clipboard.write` was absent. The page selection now flows
  into the engine on every change, so `toString()`, `type`,
  `isCollapsed`, `rangeCount` and the range's `getBoundingClientRect()`
  describe the real one; `execCommand` performs `copy`, `cut`,
  `selectAll` and `unselect`, with `queryCommandSupported` and
  `queryCommandEnabled` agreeing about them; and `clipboard.writeText`
  and `clipboard.write` reach the system clipboard by way of an
  `X-Clipboard` flag on the render response, which the shell answers with
  a `/clipboard` fetch from the renderer. Reading the clipboard stays
  refused — there is no permission prompt behind which to put it.
* The heading of `about:nordstjernen` names the browser's version and
  nothing else. It carried the JavaScript engine's version beside it, which
  told a reader looking for the browser's own version that there were two
  numbers to choose between; QuickJS is still listed under *Version &
  libraries* further down the same page, next to lexbor, Pango, SQLite and
  the rest, which is where a reader goes looking for it.
* The heading drops its tagline too. *The unique, legendary web browser*
  sat between the version and the sentence that says what the browser is,
  making a reader read past an advertisement to reach the description.
* A custom property can say what it holds. `@property` was parsed for its
  `inherits` and `initial-value` descriptors and nothing else: the `syntax`
  descriptor was read past, so the one thing the rule exists to declare —
  the grammar its value has to match — was never checked, and
  `CSS.registerProperty` was absent entirely, which is how most of the
  libraries that use registered properties reach for them. Both now go
  through one grammar: `src/css_prop_syntax.c` parses a `<syntax>` string
  into its alternatives and multipliers and matches a value against it,
  including the arithmetic, so `calc(7in - 12px)` is a `<length>` and
  `calc(5px + 10%)` is not, and so is the computational-independence rule
  that makes `10em` a legal length in a stylesheet but not as an initial
  value. A rule missing `syntax` or `inherits`, or carrying an
  `initial-value` its own syntax rejects, is now dropped rather than half
  honoured, and a declaration whose value does not match the registered
  syntax falls back to the initial or inherited value instead of being
  taken at face value. `CSS.registerProperty` throws the errors the API is
  specified to throw — `SyntaxError` for a name, syntax or initial value it
  cannot accept, `InvalidModificationError` for a second registration — and
  the CSSOM grew `CSSPropertyRule`, so `name`, `syntax`, `inherits` and
  `initialValue` read back off the rule.
* Viewport units inside a frame measure that frame. `vw`, `vh`, `vmin`,
  `vmax` and their small/large/dynamic spellings resolved against the
  top-level window wherever they appeared, so a 200x100 iframe laid its
  `100vw` box out at the width of the whole browser. The cascade now
  swaps in the frame's own viewport while it walks a nested document,
  the way the media-query evaluation already did, and only when the
  frame's size is actually known — from its `width`/`height` attributes,
  an inline size, or the last layout — so a frame whose size has not been
  measured yet keeps the behaviour it had rather than guessing at
  300x150. `data/fixtures` aside, a 200x100 `<iframe>` whose content asks
  for `100vw` by `50vh` now lays that box out at 200x50.
* A registered custom property computes its value instead of carrying the
  text it was written with. `<length>` arrives in pixels whatever unit it
  was authored in and whatever the element's font size is, a
  `<length-percentage>` that mixes the two serializes as the `calc()` the
  CSSOM specifies, `<angle>` lands in degrees, `<time>` in seconds,
  `<resolution>` in `dppx`, `<integer>` rounded, `<color>` as `rgb()` with
  `currentcolor` resolved against the element's own colour, `<string>`
  requoted, and a `<transform-function>` with its arguments computed the
  same way. The computation runs once the element's font metrics are
  known, so `--x: 14em` on a ten-pixel element is `140px` and an inherited
  value keeps the number its parent computed.
* A property registered through `CSS.registerProperty` now restyles the
  page. Incremental restyle skips a pass when no stylesheet has changed,
  and a registration changes no stylesheet, so a property registered from
  script had no effect until something else happened to dirty the tree.
  The registration is part of the signature that decides whether the pass
  can be skipped.
* `@counter-style` rules with a name no counter style may take — `none`, a
  CSS-wide keyword, or one of the six predefined styles the spec forbids
  overriding — are dropped instead of entering the stylesheet, and
  `CSSCounterStyleRule` reports its `name`.
* The CSS tokenizer decides what starts an identifier the way the Syntax
  specification does. A `-` was treated as the beginning of a name whatever
  followed it, so the subtraction in `calc(7in - 12px)` tokenized as an
  identifier rather than an operator; a hyphen now only starts a name when
  a name character, a second hyphen or an escape follows it, and the same
  rule governs the unit after a number and the name after `@`.
* The page itself snaps. `scroll-snap-type` worked on scroll containers
  only, which left out the arrangement almost every page that asks for
  snapping actually uses: full-height sections down the document, with the
  property on `html` or `body` and nothing overflowing in between. The
  document scroller is the one scroller that is not a box — the shell owns
  its offsets in a `GtkAdjustment`, and the renderer only learns them as the
  coordinates it is asked to paint from — so the snap positions its
  descendants offer were never consulted. The solver no longer derives the
  snapport from the scrolling box: it takes one, so the viewport can supply
  its own, and the renderer resolves the proposed offset against the root
  element's `scroll-snap-type` on the way into a frame and returns the
  snapped one to the shell on the render response, the channel
  `scrollIntoView` and fragment navigation already use to move a tab's
  scroll position. Every source of document scrolling therefore snaps — the
  wheel, the scrollbar, the keyboard — because each ends in a frame rendered
  at a new offset, and it works the same in process-per-tab and
  `--single-process` because both drive the same renderer service.
  `scroll-padding` on the root insets the viewport snapport as it insets a
  box's, `mandatory` and `proximity` keep their meanings, and the horizontal
  axis now rides back beside the vertical one. `html` is the root element
  and `body` is honoured as a source too, matching how the root's `overflow`
  is already read here. Verified on
  `data/render-tests/scroll-snap-viewport.html`, four sections of `100vh` in
  a 953-pixel viewport: proposed offsets of 100, 600 and 1400 resolve to
  953, 2000 to 1906 and anything past the end to 2859, while
  `scroll-snap.html`, whose snapping is all inside boxes, and the ordinary
  layout pages resolve to no snap and scroll exactly as before.
* `Ctrl+P` prints in the default process-per-tab mode. Printing paginates
  in the renderer and drew onto cairo recording surfaces, which cross no
  process boundary, so every window that was not started with
  `--single-process` — which is every window, by default — answered
  *Nothing to print*. The renderer now rasterises each finished sheet and
  writes it into the runtime directory the way the page export already
  hands a file across, and the shell loads the sheets back and feeds them
  to the same `GtkPrintOperation` as before, dividing out the scale they
  were rendered at. Pagination, `@media print`, `@page` and the `break-*`
  properties are the single code path they always were; single-process
  printing still hands the recording surfaces over untouched, so it keeps
  printing vectors.

1.0.23:
======
* The browser prints. `Ctrl+P`, or *Print…* in the menu, lays the page out
  for paper and hands the sheets to the operating system's own print
  dialog through `GtkPrintOperation` — CUPS on Linux, the Win32 printer
  dialog on Windows, the Cocoa panel on macOS — so no printing code is
  written per platform and no dependency is added. The sheets are cairo
  recording surfaces, which cross no process boundary, so the print
  action needs the in-process renderer: it works under
  `--single-process` (or `NS_SINGLE_PROCESS=1`) and reports *Nothing to
  print* otherwise. `--dump=print:FILE` renders the same pagination to a
  multi-page PDF from any mode and needs no printer.
* The engine gained the parts of CSS a printer needs. `@media print` now
  matches — the media type was hardcoded to `screen`, so a page's print
  stylesheet was simply ignored. `@page` sets the sheet size from a name
  (`A4`, `letter`, `legal`, `ledger`, the A/B series), from one or two
  lengths, or from `portrait`/`landscape`, along with its margins.
  `break-before`, `break-after` and `break-inside` — with the legacy
  `page-break-*` spellings mapping onto them and `always` becoming `page`
  — decide where a sheet may end. A sheet is cut at a forced break when
  one falls before the page is full; otherwise the cut is pulled up above
  any box it would split, which is every leaf box, every line of a
  paragraph, and anything asking for `break-inside: avoid`. Printing
  restores the on-screen layout afterwards.
  `data/render-tests/print-pagination.html` comes out as three A4 sheets
  with every card whole, the `@media print` paragraph swapped in for the
  screen one, and the forced break starting sheet three.
* `offsetLeft` and `offsetTop` are measured from the offsetParent again.
  Both returned a document coordinate, built from the margin box rather
  than the border box, so an element inside any positioned ancestor
  reported where it sat on the page instead of where it sat in its
  parent. CSSOM View asks for the distance from the offsetParent's
  padding edge, with a statically positioned `body` or root the exception
  every engine makes. A great many pages measure this way, and so does
  the `checkLayout` harness most of WPT's layout tests are written
  against — in the sibling GPL edition, where this was measured, fixing
  it plus the flex change below took `css/css-flexbox` from 653 to 1437
  of the same 3535 subtests.
* An absolutely positioned child of a flex container is placed where the
  flexbox specification says. It landed at the container's content-box
  origin whatever the container asked for; CSS Flexbox 4.1 gives it a
  static position from `justify-content` and its own `align-self`, as
  though it were the only flex item, and `flex-direction: *-reverse`,
  `flex-wrap: wrap-reverse` and `direction: rtl` each turn around the
  axis they govern. Vertical writing modes are not covered — flex layout
  itself is horizontal-only here.
* `flex-wrap: wrap-reverse` puts the first line last. Lines wrapped, but
  the cross axis was never turned around, so the first line stayed at the
  top and `align-content: flex-start` stayed at the top with it. The
  lines are now mirrored within the container after `align-content` has
  placed them, and each item within its line, which reverses
  `align-items: flex-start`/`flex-end` along with them.
* CSS Scroll Snap. `scroll-snap-type` on a scroll container, with
  `scroll-snap-align` on the things inside it, moves the container onto
  the nearest snap position once a scroll lands — from the wheel, and
  from `scrollTop`/`scrollLeft`. `scroll-padding` on the container and
  `scroll-margin` on an item inset the snapport and outset the snap area,
  both as shorthands and per side; `mandatory` always snaps, `proximity`
  only from within half a page. A wheel tick shorter than the gap between
  two snap positions still moves the reader forward rather than falling
  back to the one behind. `data/render-tests/scroll-snap.html` walks both
  axes. This is scroll containers only: the document scroller belongs to
  the window, not to a box, so `scroll-snap-type` on `html` or `body`
  does nothing yet.
* A regular expression whose `v`-flag class contains the empty string,
  `/[\q{}]/v`, no longer writes outside the string set it is building.
  The JavaScript engine is refreshed onto quickjs-ng 0.16.1, which
  carries that fix along with the iterator proposals — `Iterator.concat`,
  `Iterator.prototype.join`, `includes`, and the chunking proposal's
  `chunks` and `windows`, with `take` and `drop` now rejecting
  out-of-range limits — resizable externally managed ArrayBuffers, and a
  parser that no longer rescans the line to report an identifier's
  column.
* A processing instruction is parsed as one. `<?target data?>` produced a
  comment that the engine then took apart again by hand, guessing where
  the target ended; the HTML parser now implements the specification's
  own rules, so the node carries a real target and data, a target the
  specification disallows stays a comment, and serializing one writes
  `<?target data>` instead of dropping the target on the floor — which
  the XML parser's processing instructions had been doing all along.
  Two URL fixes come with the same refresh: a caret in a path is
  percent-encoded, and a URL that has userinfo but no host is rejected.
* Two paragraphs measuring the same words come out the same width. The
  text shaper cached a piece ending in a space together with the kerning
  the following word had induced on it, so "Type of" was laid out narrow
  after "Type A" had been drawn, and which paragraph came out wrong
  depended on what the process had already done. The shaper now asks
  HarfBuzz which pieces are safe to store.
* The bundled MP3 decoder and the WebGPU headers match their upstreams
  again — the headers move to wgpu-native v29.0.1.1, the release the
  build actually downloads.
* The third-party notices name every library in the binary. pl_mpeg and
  minimp3 are compiled into it and were listed nowhere; the wgpu-native
  headers were missing too; lexbor's `NOTICE` file, which Apache 2.0
  requires be propagated, was recorded as not existing.
* The build instructions install what the build requires. The
  Debian/Ubuntu, Fedora and openSUSE package lines omitted FFmpeg, which
  `meson setup` has refused to proceed without on Linux since inline WebM
  landed, so following them exactly produced a failing configure.
* The documentation stops pointing at files that are not there:
  `src/mobile.c`, `src/tab_worker.c`, `src/env.c`, `src/media.c` and
  `docs/ipc-http-experiment.md` were all cited by name. The mobile-site
  note described a per-host list the browser does not have — the choice
  is made once for the whole build — and the threading model still
  documented a per-tab worker thread that the move to process-per-tab
  removed.
* Every link to the project's own repository points at
  nordstjernen-web/nordstjernen-browser, its new home: the README badges
  and release-tag link, the AppStream metainfo, the Debian, RPM and Alpine
  packaging, the Java POM and Gradle metadata, and the build documentation.
  The Alpine recipe also follows the archive root GitHub names after the
  repository rather than the package, so the tarball it fetches for 1.0.22
  unpacks where the build looks for it, with a checksum to match.
* A distribution package built from a release tarball declares the FFmpeg
  libav\* libraries it needs. meson has required them on Linux and Windows
  since inline WebM landed, but the Debian control file, the OBS and Fedora
  spec files, the Alpine APKBUILD and the source-RPM recipe all still
  described them as optional, so each of those builds failed at configure
  time on a clean machine. The nightly container build no longer treats a
  missing FFmpeg as a reason to carry on either, and the Linux and Windows
  build guides list the packages.
* A package built where the build host has no network reaches the system
  Pango. ns-pango is cloned by meson at setup time, which an OBS worker, an
  sbuild chroot, a mock root and Alpine's build phase all forbid, so those
  recipes now pass -Dns-pango=disabled and build-depend on Pango itself.
* A source RPM carries a version rpm accepts and installs what it built.
  The generated spec spelled a development version with the hyphen rpm
  rejects, and hand-installed two of the four binaries with none of the
  runtime data, so the browser it packaged could not start a renderer or
  find its translations.
* The Debian tree has the changelog dpkg-buildpackage needs, and its rules
  file configures the build the way the packaging documentation says.
* The documentation index lists the architecture, iOS, extensions,
  vendored-engine and wpt-fast documents that were missing from it, and no
  longer points at two documents that are not there. The OBS packaging notes
  no longer describe a _service file the repository does not carry, and the
  HTML compatibility table describes what actually happens when an
  undecodable media element is clicked.
* The Android, iOS and Java hosts compile again. Each of them builds its
  bridge against the engine's headers with nothing on the include path but
  `src/`, and printing put `<glib.h>` and `print.h` into three of those
  headers — `libnordstjernen.h`, which is the one the engine installs,
  `renderer_serve.h` and `rproc_http.h`. `print.h` reaches on to cairo, the
  CSS engine and the layout tree, none of which those builds can see, so
  the Java bridge, the Android bridge and the Swift bridging header all
  stopped finding what they included while the engine itself, which has the
  full include path, kept building. Every print entry point moves to
  `print.h` beside the pagination it drives, and the three headers are
  self-contained again.
* The Android bridge attaches its in-process renderer again: the callback
  that hands one over began returning the connection the print path needs
  rather than a status code, and Android's still returned the status code,
  which its compiler rejects outright.
* The NetBSD build no longer rests on one package mirror. It took whatever
  `pkg_add` defaults to, which is `ftp.netbsd.org` over plain HTTP, and when
  that host refused connections every dependency failed to install and the
  job died before a compiler ran. It now names the CDN the NetBSD sets
  already come from first and that host second, and retries, so one mirror
  being down is no longer the end of the build.

1.0.22:
======
* A mouse or pointer event carries the window it was dispatched in.
  `UIEvent.view` is that window and every event the engine synthesised
  for a click, a drag or a hover reported null, which is a value no
  browser produces.
* A grid item placed by area name is aligned to its row. Items placed
  through `grid-template-areas` went down a layout path that never read
  `align-items` or `align-self`: each was put at the top of its row at its
  own height, where the default is to stretch. On lichess.org the lobby's
  start-button column stayed 179 pixels tall beside a 600-pixel
  neighbour, and the player counts pinned to its bottom edge came to rest
  on top of the buttons.
* A single flex line is as tall as the container says. A row flex
  container with a definite height has a line exactly that tall, and a
  stretched item gets that height whether its content fits or not. The
  line was sized to the taller of the container and its content instead,
  so one over-tall item dragged the whole line past the height the author
  asked for.
* A percentage inside `min()`, `max()` and `clamp()` is measured against
  the box rather than the viewport. These functions were folded to a
  single pixel value during parsing, when the only basis available was
  the viewport width, so the comparison ran against the wrong number: in
  a 400-pixel column `min(300px, 50%)` came out 300 instead of 200.
* A dialog opened from script renders, and a modal one is centred.
  `showModal()` and `show()` set the open attribute without telling the
  style engine, so the element kept the `display: none` it was matched
  with at parse time and had no box at all. The user-agent sheet now also
  carries the modal rule the HTML specification defines, and an
  out-of-flow box asking for an intrinsic height is no longer stretched
  between its top and bottom offsets.
* A grid track can be measured in any length unit. `grid-template-columns`
  understood px, %, fr, em and rem, and quietly dropped every track it
  could not read, which moved each remaining track one place to the left.
  lichess.org asks for five columns with a `1vw` margin at either end;
  three arrived, its `<main>` landed in a column with no room in it, and
  the whole site laid out zero pixels wide down a 26000-pixel page. Track
  lengths now go through the same reader as every other length, and a
  track list that still cannot be read is discarded whole rather than
  closed up, because a missing list leaves the columns to
  `grid-template-areas` while a shifted one leaves nothing standing.
* A positioned box answers the pointer in the layer it paints in. Hit
  testing ranked a box against its own siblings and nothing else, so a
  fixed, high `z-index` overlay never rose above content in another branch
  of the page even though the painter drew it on top: an overlay's button
  was visible and the content behind it took the click. Positioned boxes
  are now collected and tried in the order the painter flushes them, and a
  modal dialog in the top layer is tried before the document.
* An `<svg>` that paints nothing hands the pointer to what is under it.
  SVG hit-tests as `visiblePainted` -- a shape answers where it draws and
  nowhere else -- but the engine lays an `<svg>` out as one replaced box
  and let that box answer for its whole rectangle. chess.com stretches an
  `<svg>` of rank and file labels over the board, so a piece could be
  picked up and never put down: the labels swallowed the pointerup that
  ends the drag.
* An SVG `font-size` attribute survives the cascade. A presentation
  attribute is author style at the very bottom of the cascade, so a real
  declaration beats it but inheritance must not; the renderer read the
  attribute and then overwrote it from the computed style, which always
  has a font-size because font-size is inherited. Every `<text>` drew at
  the page's font size scaled by the viewBox -- chess.com's board
  coordinates came out four times their size and spilled across the board.
* A worker shares the storage of the page that started it. The worker
  runtime was built without a storage partition, and IndexedDB reads that
  partition to find the origin's databases, so every `indexedDB` call
  inside a worker threw "Storage is unavailable" and chess.com reported
  its opening database as unusable.
* `prefers-color-scheme` answers with the scheme the desktop is actually
  using. Nothing set it at all, so every window reported "light" however
  dark the desktop was: a site's dark stylesheet never applied. The shell
  judges the scheme by the luminance of the foreground colour the theme
  resolves for its window -- which holds for any theme, rather than only
  the ones that set GTK 3's `gtk-application-prefer-dark-theme` -- honours
  an explicit `color_scheme` setting over it, hands the answer to each
  renderer it starts, and re-evaluates when the theme changes underneath a
  running window. The internal pages -- start, about, settings, history and
  the error page -- gain the dark half they never had, and stop declaring
  themselves light-only.
* Text is laid out through three new ns-pango caches: the unicode break
  attributes, the items a paragraph was cut into, and shaping keyed on a
  word rather than on a run. Intrinsic sizing means the same paragraph is
  laid out for min-content, for max-content, for the real width and again
  to paint it, and the line breaker cuts a run wherever a line ends and
  shapes the piece again -- so a paragraph shared no cache entry even with
  itself. On this repository's own test page the shape cache now serves
  1512 lookups against 67 misses, the break cache 312 against 24 and the
  item cache 277 against 63. The layout dump is byte-identical to the one
  the previous pin produced.
* The status line behaves like a status line. It held a permanent row under
  the page saying "Done" for the life of every visit, and hovering a link
  wrote the URL there while moving off it wrote nothing -- so the last link
  the pointer touched stayed on screen indefinitely, naming a destination
  the cursor had left. It now floats over the bottom-left corner of the
  page, appears only while it has something to say, and clears when the
  pointer leaves a link or a load finishes. Notices the window raises
  itself -- a bookmark added, a session recovered -- fade after five
  seconds.
* The toolbar menu is grouped into tab, view, page and tool sections and
  gains Zoom In/Out/Reset, Full Screen, History and the two save entries;
  zoom, full screen and history had keyboard shortcuts but no visible
  affordance, and saving a page was reachable only by right-clicking it.
  Items whose action carries more than one accelerator name one explicitly,
  because GTK shows nothing when a shortcut is ambiguous. A popover menu
  also takes the height its items need: GtkPopoverMenu builds a section's
  separator after the popover has negotiated its size, so the last item was
  always clipped.
* The page zoom is shown beside the address bar while the page is scaled,
  and resets when clicked. Zooming said "Zoom 121%" in the status line for a
  moment and then left no trace, so a window could sit at any magnification
  with nothing on screen admitting it -- and 121% is where successive tenths
  land. The steps now follow the usual ladder: 90, 100, 110, 125, 150.
* The bookmark button says whether the page is bookmarked, carrying a hollow
  star that fills once the page is on the list, and the popover's action
  becomes "Remove this bookmark" where it would otherwise do nothing --
  adding a URL already on the list is a no-op, so pressing it a second time
  silently did nothing.
* Escape in the address bar reverts it to the URL the window is showing.
  It restored focus to the page but left whatever had been typed sitting in
  the bar, naming a page that was not on screen. Escape also closes the find
  bar, which its own tooltip already promised, and stops a load in progress.
  The connection indicator moves inside the entry, where the padlock
  belongs, and the title bar no longer reads "Nordstjernen 1.0.22 —
  Nordstjernen 1.0.22" on a page with no title of its own.
* A text field shows as much of its value as it has room for. An `<input>`
  was given a visible window of exactly as many characters as its `size`
  attribute names, and CSS that widened the control -- `flex-grow`,
  `width: 100%` -- only stretched the painted frame, so a field with room
  for sixty characters still scrolled its text away after twenty. The
  window now comes from the used content width: it grows when the box is
  wider than `size` asks for, and shrinks when a definite CSS width is
  narrower, where the value used to be painted straight through the
  control's own border.
* An inline-block, inline-flex or inline-grid sits on the line's baseline.
  Two things put it elsewhere: the shape rect handed to the shaper aligned
  the box's bottom margin edge to the baseline -- only the fallback CSS 2.1
  gives a box with no in-flow line boxes of its own -- and the placement
  pass then ignored the shaper's answer and pinned the box to the top of
  the line. A badge or button written inline with a sentence was drawn with
  its own text floating above the words either side of it, and the line box
  grew to cover the overshoot.
* A `#fragment` stays anchored while the rest of the page loads. The scroll
  position was computed once, from whatever layout existed at navigation
  time, so images decoding above the target pushed it down afterwards and
  the view landed short of the heading it was asked for. The target is held
  and its position re-applied until the document goes quiet or the reader
  scrolls away.
* A navigation that ends with nothing to render gets an error page whatever
  its scheme. Only `https://` failures had one, so a missing `file://` path
  came back 404 with an empty body and rendered as a blank white page --
  no heading, no URL, nothing to act on. The classifier has also stopped
  blaming the network for everything it does not recognise: an unmatched
  transport message falls through to the status code, and a file URL is
  described as a file rather than as an unreachable server.
* Resolving an `ex`, `ch`, `cap` or `ic` length no longer shapes four probe
  glyphs every time. The metrics oracle built a layout and measured `x`,
  `H`, `0` and the water ideograph on each call, and the cascade asks once
  per element a rule matches, so a stylesheet that sizes fields in `ch`
  paid for four layouts on every one of them. The answer depends on nothing
  but the family, size, weight and slant, so it is measured once per font
  and kept until the font map changes under it.
* An IndexedDB write no longer walks the origin's whole storage directory.
  Every `put` recomputed the origin's quota by opening each `.sqlite` file
  beside the current one, asking it for `page_count` and closing it again --
  a directory scan and a fresh SQLite connection per record written, inside
  the write transaction. A page that stores a burst of records stalled the
  browser in the filesystem for as long as the burst lasted: starting a game
  on chess.com left the main thread inside `CreateFile` and it never came
  back. Cache the siblings' total for five seconds; the current database is
  still measured live, so the limit is enforced as before.
* `document.styleSheets` includes the sheets a page links to, with their
  `href` and their rules. Only inline `<style>` blocks had a populated
  `CSSStyleSheet`; a `<link rel=stylesheet>` produced one with a null href
  and an empty `cssRules`, because the sheet was built from the element's
  own text content and a link has none. nrk.no went from 13 reachable rules
  to 1895. The engine keeps a reference to the CSS it already fetched for
  the cascade, so nothing is downloaded or stored twice.
* `getComputedStyle(el).cssFloat` reports the used float. The accessor read
  the declaration block directly rather than going through whichever
  `getPropertyValue` the object carries, so on a computed style -- which has
  its own -- it always came back as the empty string, while the equivalent
  `getPropertyValue('float')` answered correctly.
* Instantiating a module through the `WebAssembly` JS API runs the module's
  start section and nothing else. WAMR, built for a standalone runtime, also
  called `_initialize`, `__wasm_call_ctors` and `__post_instantiate` from
  inside `wasm_runtime_instantiate` -- but on the web those are ordinary
  exports the JS glue calls itself, after it has pointed its heap views at
  the instance's memory. Running them first meant an Emscripten module tore
  down on its own first WASI call: chess.com's analysis engine died in
  `environ_sizes_get` before `new WebAssembly.Instance` had returned.
* A finished keyframe animation keeps the value `animation-fill-mode`
  says it should. The engine sampled the last keyframe correctly, then threw
  the sample away: every getter the painter calls required the animation to
  still be running, so at the moment it ended the box snapped back to its
  specified value. The common `opacity: 0` plus a `fade ... forwards`
  animation therefore faded in and vanished again within one frame, and the
  content stayed invisible for the life of the page -- chess.com's bot
  gallery, which is exactly that pattern, was a set of empty boxes.
* `AbortSignal` is an interface object, not a bare namespace. It was a plain
  object carrying `abort()`, `timeout()` and `any()`, and the signals an
  `AbortController` hands out did not inherit from it, so `signal instanceof
  AbortSignal` -- the guard every fetch wrapper writes -- threw "invalid
  'instanceof' right operand" instead of answering. chess.com's RPC client
  turned that TypeError into a 500 and never issued its first request.
  `MessagePort` gains the same treatment in the window: it existed only in
  workers, so ports came back with no prototype at all.
* A grid container's max-content width is the sum of its columns, not the
  width of its widest item. Anything that shrink-wraps a grid -- a float, a
  table cell, an inline-grid, `width: max-content` -- was sized as if the
  columns were stacked, so the tracks overflowed the box they were given.
  bbc.com's "LIVE" flag is a floated two-column grid, and the headline
  beside it started inside the flag rather than after it.
* Flex and grid items measure their intrinsic sizes in the font they will
  actually be drawn in. The base size of a flex item and the min-content
  floor of a flex or grid item were measured against the item's own style
  rather than the style its text inherits, so text in a web font was sized
  by the fallback face. An item then got a base size a pixel or two under
  what the real font needs and wrapped mid-phrase however much room the
  container had -- dn.no's nav pills broke "DN Helg" and "DN i VM" across
  two lines inside a box wide enough for either.
* An absolutely positioned box with `width: auto` gets the shrink-to-fit
  width CSS 2.1 asks for -- its max-content size clamped to the available
  space and floored at min-content -- measured by shaping the text. It used
  to be guessed from a character count at 0.65em each, so every tooltip,
  dropdown, badge and popover came out at a width unrelated to its
  contents: a seven-character label was sized 81px where the text needs 63.
* Event-listener objects follow the Web IDL callback-interface algorithm.
  `handleEvent` is looked up for every dispatch, non-callable values and
  throwing getters are reported as uncaught listener exceptions, and generic
  `EventTarget` objects no longer discard object listeners. The focused DOM
  event test moves from three passing subtests out of six to all six.
* Checkbox and radio activation keeps the state required by HTML's legacy
  pre-activation and canceled-activation steps. `indeterminate` is a real
  cloned input state, a canceled radio click restores the previously checked
  group member, synthetic `click()` events are untrusted and cannot recurse on
  the same element, and the resulting `input` and `change` events are not
  cancelable. Three focused input tests move from 52/80 to 80/80 subtests.
* `CSSStyleSheet.insertRule()` and `deleteRule()` enforce their required
  arguments, while the deprecated but web-visible `addRule()` and
  `removeRule()` methods mutate both constructed and document sheets. The
  CSSStyleSheet interface test moves from 8/17 to 17/17 subtests.
* A worker scope gets the same JavaScript platform the page does.
  Workers were built from a hand-kept list of C-side globals and never
  ran the polyfill bundle, so `indexedDB`, `Headers`, `Blob`,
  `AbortController`, `AbortSignal`, `ReadableStream`, `WritableStream`,
  `TransformStream` and `caches` were all absent inside one. The bundle
  now runs in the worker too, up to the end of the IndexedDB section and
  no further -- everything past that point is DOM and window surface a
  worker must not have. Of the 27 globals a worker is expected to carry,
  14 were missing and 3 are: `FileReader`, `WebSocket` and nested
  `Worker`. chess.com's play page opened its database inside a worker
  and threw on the first line.
* `min-content` and `max-content` grid tracks size to their content
  instead of stretching like `auto`. A box with a definite width now
  contributes that width to min-content, an intrinsic track is measured
  against the space left once `minmax()` tracks are at their minimums
  rather than their maximums, a `min-content` track is never scaled below
  its content, and a grid container's own min-content is the sum of its
  columns rather than its widest child. Together these stop a nested grid
  -- chess.com's board -- from overflowing the panel beside it.
* An SVG with a `viewBox` but no width or height is sized the way CSS
  says: its ratio fitted inside the 300x150 default object size. It was
  rasterised into a square, so the artwork was letterboxed and then
  stretched into the page's box; Wikipedia's wordmark came out a smear.
* A declaration that uses `var()` no longer overrides the declarations
  that follow it. Such a declaration is held back until custom properties
  are known, and it was then re-inserted after every plain declaration in
  its block rather than at the place it was written, so it won every
  conflict with a later one. `color: var(--c); color: green` computed
  blue, `background: var(--c); background-color: red` computed blue, and
  `margin: var(--w); margin-left: 40px` kept the shorthand's margin. The
  New York Times sets `* { outline: var(--size) solid var(--accent);
  outline-color: #0000 }` -- an outline on every element that is
  transparent until something takes focus -- so the front page was drawn
  as a grid of blue boxes, one around every element on it. A held-back
  declaration now keeps its position in its block.
* A regexp search skips the positions that cannot start a match. A
  pattern without the sticky flag is compiled with a `.*?` prologue, so
  the matcher was re-entered at every index of the subject: `/^zebra/`
  walked all 880KB of a string to fail at the first assertion 880,000
  times. The search now reads what a match must begin with -- a start
  anchor, a single character, or a character class turned into a
  256-bit table -- and skips ahead with `memchr` or a table probe
  instead, the way V8's Irregexp does. A failing literal search over
  880KB drops from 5.6ms to 0.25ms, a leading character class from
  10.8ms to 0.35ms, a case-insensitive literal from 8.5ms to 0.85ms,
  and an anchored pattern from 6.1ms to nothing. Ten thousand
  `test()` calls that miss on a short string fall from 200ms to 2.2ms.
  A pattern that starts with an alternation is not covered and still
  runs as before. Verified by differential fuzzing: 40,000 random
  pattern/subject/flag combinations produce byte-identical `exec`,
  `replace`, `split` and `search` results before and after.
* The local `\p{RGI_Emoji}` tables are gone. Upstream quickjs-ng has
  since grown the general properties-of-strings machinery, which covers
  `RGI_Emoji` along with `Basic_Emoji` and the flag, tag, ZWJ, modifier
  and keycap sequences, and composes with the `v` flag's set operations
  -- so the vendored Emoji 17.0 sequence data, its generator and the
  local emit path were dropped for it.
* Grid items can be placed on named lines. A line named in the track
  list was parsed as a track, rejected, and dropped, so `grid-column:
  main` resolved to nothing and the item was auto placed, which collapses
  the layout of any page that names its lines. Names are now recorded and
  resolved, an area called `foo` also defines `foo-start` and `foo-end`,
  an end line repeating the start's name means the next line with that
  name, and an item with a column but no row keeps its column. chess.com's
  play page draws its board again.
* Headless `--viewport` takes `WIDTHxHEIGHT`. It read an integer and
  insisted the argument ended there, so `--viewport=1280x900` was dropped
  without a word and the page laid out at the default width. Anything it
  cannot read is now an error rather than silence.
* A page on an origin that does not speak QUIC no longer stalls for the
  whole connect timeout. Whenever libcurl was built with HTTP/3, every
  request asked for it, so the first hop to an origin that silently drops
  UDP on 443 waited out the 15-second navigation connect timeout (6 for a
  subresource) and returned a timeout rather than falling back. The
  timeout also counted as a connection failure, which parked the host in
  the unreachable cache for two minutes and failed every subsequent
  request to it -- so acid3.acidtests.org took 15 seconds to answer and
  then lost all of its subresources. Requests now ask for HTTP/2 and are
  upgraded to HTTP/3 by the alt-svc cache, the way an origin advertises
  it; `NS_FORCE_HTTP3=1` still asks for HTTP/3 outright. Acid3 loads in
  2.2 seconds instead of 15.5, and three seconds in it has run 65 of its
  tests rather than 12.
* Table cells centre their content vertically again. A cell with no
  `vertical-align` of its own fell back to the initial `baseline`, so in
  a row taller than the cell's own line the text sat at the top. Every
  browser's user-agent sheet gives cells `middle`, which is what pages
  written as tables expect. Cells now default to `middle`, and an author
  rule or a `valign` attribute still overrides it.
* An image is drawn inside its own borders, padding and margin. The
  painter placed the bitmap at the box's margin-box origin and gave it
  the content size, so a bordered image covered its own top and left
  borders and a margin shifted the picture instead of the box. Replaced
  content now starts where the content box starts, the way inline SVG
  and MathML already did, and the placeholder, alt text and drop shadow
  follow it.
* `OfflineAudioContext` renders audio instead of silence. Every `create*`
  method returned the same generic node, `connect()` recorded no edge,
  `start()` and `stop()` did nothing, `AudioBuffer` could not hold
  samples -- `getChannelData` minted a fresh zeroed array on every call --
  and `startRendering()` resolved a buffer of zeros. Nodes now carry their
  kind, the graph is recorded, buffers keep one array per channel, and
  `src/webaudio.c` renders oscillators, gain, a dynamics compressor, the
  RBJ biquad types, delay, wave shaping, constant sources and buffer
  playback. Rendering is mono, summed into every channel, and `AudioParam`
  automation is not applied. The context, nodes and buffers also brand
  themselves for `Object.prototype.toString`.
* The toolbar reads by colour again: back and forward green, reload blue,
  and a red stop button between reload and home that appears only while a
  page is loading. Stop marks the in-flight frame stale, ends the loading
  state and drops the busy cursor; it does not abort the network request.
  The title bar is shorter, the home button is set off from the address
  bar, and the security shield is drawn smaller than the buttons.
* `about:nordstjernen` lists the user agent, resolved the way a request
  resolves it.
* The start page is titled "Home" rather than "Nordstjernen", so its tab
  and window title say what the page is.
* The embedded ns-pango build no longer asks for link-time optimization.
  The rest of the tree links without LTO, so a clang build on Windows
  archived the fork as LLVM bitcode that the mingw linker could not read
  ("archive has no index" / "file format not recognized"). The subproject
  keeps `-O3` and the release `NDEBUG`.
* The CSSOM rule interfaces carry their real names and classes. Every
  interface the polyfill synthesises reported `name: "ctor"`, because the
  constructor was an anonymous function expression, and `@import` and
  `@keyframes` were plain `CSSRule` objects even though their `type` said
  3 and 7, so `instanceof` and `Object.prototype.toString` disagreed with
  `type`. They now get `CSSImportRule` and `CSSKeyframesRule`, `@namespace`
  and `@counter-style` get theirs, and the grouping at-rules that were all
  lumped under `CSSGroupingRule` get `CSSContainerRule`,
  `CSSLayerBlockRule` and `CSSScopeRule`.

Java
----
* The URL bar warned "Not encrypted" on every HTTPS page. Both the Java and
  the Android shell numbered the engine's transport-security states
  themselves and got them backwards -- 1 is a validated chain, not plain
  HTTP -- so a valid certificate showed the warning and an untrusted one
  showed the lock. They now follow `ns_security` and tell an untrusted
  certificate apart from an unencrypted connection.
* `java/pom.xml` builds the library with Maven: the same sources, the same
  manifest, and the jar, sources and javadoc artifacts, plus the metadata a
  public repository needs and a `release` profile that signs them and
  publishes to the Maven Central portal. `build.gradle` grows the matching
  `maven-publish` block, so `gradle publishToMavenLocal` and `mvn install`
  produce interchangeable artifacts, and it reads its version back out of
  `pom.xml` so the number lives in one place.
* `RemoteBrowser` spoke about a third of the renderer's control protocol. It
  now covers transport security and the server address, the live page size
  the render headers carry, the scroll position a page asks for (anchors,
  `scrollTo`, focus), camera prompts, window actions, overflow scrolling,
  in-page scrollbar dragging, `contextmenu` delivery, file drops, page dumps,
  the focused editable, idle ticks, caret blinking, and back/forward-cache
  traversal. `RemotePage` grows the `text`, `links`, `linkAt`, `dump`, `eval`
  and `renderToFile` it always claimed to mirror from `Page`, and `links`
  becomes a `/dump` kind so it has an endpoint to reach.
* Both clients had their own copy of a JSON reader that found a key anywhere
  in the document, including inside a string value. They now share one that
  only matches a quoted token immediately followed by a colon. Frames convert
  the renderer's BGRA to the raster's ARGB in one bulk copy rather than a
  per-pixel loop.
* The Swing browser uses all of it: a lock or warning beside the URL, wheel
  notches offered to the scroller under the pointer before the page,
  draggable in-page scrollbars, pages that draw their own context menu,
  camera prompts, page-source and layout/network/performance inspectors,
  History and Settings entries, files dropped onto the page, a blinking
  caret, and several windows (`Ctrl+N`, or `Ctrl+Shift+N` for a private one
  whose renderer keeps no cookies, cache or history) each with its own
  renderer process.

Android
-------
* The app had a back stack but no forward, no find-in-page, and no way to
  select page text -- and it silently dropped the WebGL and camera
  permission requests the engine raised, so a page asking for either got
  neither a prompt nor an answer. The JNI bridge now carries find, selection,
  favicons, transport security, both permission prompts and their
  resolutions, media resolution, PDF export, `contextmenu` delivery, `eval`,
  the scroll position a page asks for, and viewport changes; it also stops
  leaking the camera, audio and window-action strings on every rendered
  frame.
* The shell grows a forward button and a real history list that survives
  process death, a find bar, long-press-to-select with a copy/share action
  bar, a lock or warning at the head of the URL bar, and back/forward that
  reuses the renderer's back/forward cache instead of refetching.
* Rotation no longer refetches the page: the open document is re-laid out at
  the new viewport, so scripts, form state and the reading position survive
  turning the device.
* Platform integration: sharing a page or a selection, pull-to-refresh,
  printing through the system print service (Android's Save as PDF), pinning
  a page to the launcher, static app shortcuts, handling shared text,
  `WEB_SEARCH` and `PROCESS_TEXT` intents, and handing media the engine
  cannot play inline to another app.

CSS
---
* `prefers-color-scheme` and `prefers-reduced-motion` were hardcoded to
  `light` and `no-preference` with no way to change them.
  `ns_browser_set_color_scheme` and `ns_browser_set_reduced_motion` let an
  embedder mirror the platform's theme and animation settings into the
  cascade; the Android shell follows the system dark theme and the "remove
  animations" accessibility switch through them.

Text layout
-----------
* Desktop builds shape text through ns-pango, a fork of Pango carried as a
  meson subproject, instead of the system Pango. Pango keeps no cache that
  outlives a `PangoLayout`, so the same bytes were shaped by HarfBuzz once to
  measure an inline run and again to paint it, and a table cell was shaped
  for `min-content`, for `max-content` and once more to lay out. The fork
  caches finished glyph strings process-wide -- keyed on the font, bidi
  level, gravity, script, language, analysis and show flags, text transform,
  OpenType features and the item bytes -- and caches
  `pango_context_get_metrics` per font description, which resolving
  `line-height: normal` asks for on every inline run. On a table-heavy page
  the cache serves 92% of shaping requests and cuts layout time 24%; a
  text-heavy page falls 13%. Every symbol in the fork is renamed, because
  GTK loads the system Pango into the same process and GObject aborts when
  two libraries register the same type name. Android and iOS keep the system
  Pango, which has the backends they need, and `src/ns_pango_names.h` maps
  the renamed API back for them. Rendering is unchanged: the fixture smoke
  set matches its baselines and a corpus covering RTL and bidi, CJK, the
  white-space modes, intrinsic sizing, spacing, tabs, ellipsis, columns,
  inline atomics, decorations and font features renders byte-identically on
  both paths.

1.0.21:
======

Media and images
* The vendored pl_mpeg no longer reads past a frame plane. Half-pel motion
  compensation samples `s[si + 1]`, `s[si + dw]` and `s[si + dw + 1]`, but
  `plm_video_process_macroblock` bounds only `s[si]`, so a macroblock on
  the bottom row reads up to one row plus one byte beyond the plane it
  samples -- absorbed by the next plane for interior planes, and off the
  end of the allocation for the last one. This is reachable from page
  content: `ns_video_player_new` hands an MPEG-1 `<video>` body to
  `plm_create_with_memory`, and `ns_video_backend_next` decodes it. Fuzzing
  the decoder under AddressSanitizer with mutated streams reported it as a
  heap-buffer overflow read. The three frames are allocated as one chunk,
  which is now padded by that overshoot and zeroed, so the read stays
  inside the allocation and a corrupt stream decodes deterministically.
  Valid video is unaffected: no bound is tightened, so no macroblock that
  decoded before is rejected now.
* Animated images decode as animations on the engine's own fetch path.
  `ns_image_decode_body` routed GIF, APNG and animated WebP to the
  animation decoder, but the two fetch handlers in `engine.c` -- the ones
  headless rendering and the browser's own image pass use -- called
  `ns_image_decode_bytes` instead, which only ever returns a still frame.
  An animated image fetched through those paths therefore froze on frame
  one. Both now go through `ns_image_cache_insert_encoded`, and the
  still-versus-animated decision lives in one function rather than three
  copies that had already drifted.

Media capture and WebRTC
* `MediaStream` and `MediaStreamTrack` are constructors, and the streams
  and tracks `getUserMedia` hands back are instances of them, so
  `stream instanceof MediaStream` holds and `new MediaStream([track])`
  works. They were plain object literals with the right methods, which
  passes a duck-typing check and fails everything else.
* `RTCSessionDescription` and `RTCIceCandidate` exist. Signalling code
  wraps the objects it receives before handing them to the peer
  connection, so their absence stopped a session at the first offer.
  `RTCIceCandidate` rejects an initialiser carrying neither `sdpMid` nor
  `sdpMLineIndex`, as the specification requires.
* `RTCRtpSender`, `RTCRtpReceiver` and `RTCRtpTransceiver` exist, with
  `getCapabilities` on the two that define it.
* `RTCPeerConnection` gains `addTrack`, `removeTrack`, `addTransceiver`,
  `getConfiguration`, `setConfiguration`, `restartIce` and the
  `generateCertificate` static, and `getSenders`, `getReceivers` and
  `getTransceivers` return what was added to the connection instead of
  always returning an empty array.
  This is API surface: there is still no ICE agent, no DTLS and no SRTP,
  so a connection never leaves the `new` state. What changes is that
  feature detection and object construction no longer throw partway
  through a page's setup code.

Adaptive streaming
* HLS and DASH play. `data/js/streaming.js` adopts any `<video>` or
  `<audio>` whose source is an `.m3u8` or `.mpd` -- by extension or by
  `type` -- parses the manifest, picks a rendition, and feeds segments
  through Media Source Extensions, which is how the sites that use these
  formats already expect to be served. HLS covers master and media
  playlists, `EXT-X-MAP` initialisation segments, `EXT-X-BYTERANGE`,
  separate `EXT-X-MEDIA` audio renditions, and live playlists, which are
  re-fetched on the target-duration cadence and merged by media sequence.
  DASH covers `SegmentTemplate` with either `SegmentTimeline` or a fixed
  segment duration, `$Number$`/`$Time$`/`$RepresentationID$`/`$Bandwidth$`
  substitution with `%0Nd` padding, `BaseURL`, and separate audio and
  video adaptation sets. Rendition choice prefers the highest bandwidth at
  1080p or below among the codecs the build can actually decode. The
  player keeps roughly thirty seconds buffered ahead of the playhead and,
  on `QuotaExceededError`, evicts everything more than ten seconds behind
  it and retries rather than giving up.
* `video/mp2t` is an accepted Media Source type. Browsers reject it
  because their Media Source pipelines take fragmented MP4 and WebM only,
  which is why HLS players written in JavaScript transmux MPEG-TS before
  appending. Appended bytes here go to libavformat, which demuxes
  transport streams natively, so the transmuxing step is wasted work and
  segments can be handed over as they arrive.

Media Source Extensions
* `navigator.mediaCapabilities.decodingInfo()` answered from a hardcoded
  substring list -- WebM, VP8, VP9, Opus and WAV -- so it reported
  `supported: false` for every MP4 and AAC configuration even though
  `canPlayType` reported `probably` for the same string. Adaptive players
  ask `decodingInfo` which rendition to fetch, so a browser that denies
  H.264 there selects nothing and never starts. Container and codec
  support now resolve through one shared table that `canPlayType`,
  `decodingInfo` and `MediaSource.isTypeSupported` all consult, and a
  configuration is supported only when every stream in it is -- audio and
  video both, not whichever one was inspected first.
* `MediaSource.isTypeSupported` is a native call rather than a
  round-trip through `document.createElement('video').canPlayType`, and
  answers are memoised. Adaptive players probe it hundreds of times while
  building the format ladder; each probe used to allocate an element.
  It also answers for the segmented containers only, as the specification
  requires, instead of inheriting `canPlayType`'s whole-file container
  list.
* A single failed `appendBuffer` no longer wedges a `SourceBuffer` for
  the rest of the page's life. Any native rejection set a `_quotaFull`
  latch that made every later append throw `QuotaExceededError`, and the
  latch cleared only on a successful `remove()`. Quota is now checked
  against the buffer's real byte count before the append is queued, so it
  throws `QuotaExceededError` synchronously the way the specification
  says and the way players expect when they run their eviction path; a
  genuine decode failure runs the append-error steps instead, ending the
  media source. A zero-length append is a no-op rather than an error.
* `SourceBuffer` reports `audioTracks`, `videoTracks` and `textTracks`,
  and `AudioTrackList`, `VideoTrackList` and `TextTrackList` exist as
  constructors.
* `MediaSource.readyState`, `sourceBuffers` and `activeSourceBuffers`,
  and `SourceBuffer.updating`, moved from per-instance properties to
  prototype accessors, where feature detection looks for them.
* `MediaSourceHandle`, `MediaSource.prototype.handle`,
  `MediaSource.canConstructInDedicatedWorker`, `ManagedMediaSource` and
  `ManagedSourceBuffer` are present.
* AC-3, E-AC-3, FLAC, ALAC, MP3-in-MP4 (`mp4a.69`, `mp4a.6b`) and the
  AAC object types beyond `mp4a.40` resolve to their decoders, and
  `video/quicktime`, `audio/aac`, `audio/flac` and `audio/wav` are
  recognised containers.
* The headless renderer builds a video cache, wires the Media Source
  callbacks, ticks the cache from the settle loop and forwards media
  events back to the document, so appended segments reach the demuxer
  there instead of every `appendBuffer` failing for want of a callback,
  and `video.buffered` reports the real demuxed range. Media Source
  behaviour is now reproducible from `--headless`.

Images and graphics
* Animated PNG plays. Wuffs already decoded APNG frames and the
  animation loop is format-agnostic, but the callers only routed GIF
  magic to it and the animation decoder itself hardcoded the GIF
  signature check and the GIF decoder, so a PNG was rejected inside the
  function meant to decode it. Both now use the same format detection
  the still path uses. An APNG is recognised as the spec defines it, by
  an `acTL` chunk before the first `IDAT`, so a still PNG never pays for
  the animation decoder. The decode-pipeline documentation had listed
  APNG as supported already; it is now accurate.
* The vendored Wuffs moves to v0.4.0-alpha.10, nine months newer than
  the alpha.9 the tree carried. The release adds the VP8 decoder, so
  lossy WebP -- the common case on the web -- now decodes through the
  memory-safe path instead of libwebp. `MODULE__VP8` was already set in
  the subproject's build flags, where it had been a no-op because the
  module did not exist in alpha.9. libwebp and libwebpdemux are left
  serving animated WebP and nothing else.
* gdk-pixbuf no longer decodes page images. Every format the web
  actually uses is already handled in-tree -- ICO, then Wuffs for PNG,
  GIF, BMP and JPEG, then libwebp, then libavif, then the in-engine SVG
  renderer -- so the pixbuf fallback had been reduced to TIFF, TGA, PPM
  and ICNS, none of which Chrome or Firefox render either. What it cost
  was the ability to know what parses untrusted bytes:
  `gdk_pixbuf_get_formats` enumerates loader plugins installed on the
  user's machine, so the set of decoders reachable from a web page was
  decided at runtime, varied per system, and could not be audited from
  the build. The decode chain now ends after SVG: an unsupported format
  fails to decode instead of falling through to a plugin. GTK 4 still
  depends on gdk-pixbuf for its icon theme, so a desktop build links it
  either way -- what goes away is the browser feeding it. The mobile
  builds, which never had it, are unaffected.
  `ns_image_pixbuf_supports_mime` is renamed `ns_image_supports_mime`.
* libavif is optional on the desktop builds too. It was a hard
  `dependency()` off the mobile path, so a desktop tree without it would
  not configure at all, even though every AVIF call site already sat
  behind `NS_HAVE_AVIF` and `image_avif.c` was already compiled
  conditionally. The new `avif` meson feature defaults to `auto`, so a
  host that has libavif is unchanged; `-Davif=disabled` drops it and
  AVIF images fail to decode like any other unsupported format. libavif
  pulls in a complete AV1 decoder for a format that is rare on the web.
  The README listed libavif as both required and optional; it is now
  listed once, as optional.
* `var()` resolves inside SVG presentation attributes. A custom property
  set by a stylesheet rule now reaches `r="var(--radii)"` or
  `fill="var(--tint)"`, so a class can retheme an inline icon's colour
  and geometry the way it does for ordinary CSS properties.
* `mask` is honoured on SVG elements. The referenced `<mask>` renders to
  an offscreen surface whose sRGB luminance becomes the alpha the
  element is composited through, so a white mask shows the element,
  black hides it, and a gradient fades it. Group opacity and masking
  combine.
* `marker-start`, `marker-mid` and `marker-end` draw their `<marker>` on
  path, line, polyline and polygon vertices. Vertices and their tangents
  come from the built Cairo path, so arcs and curves orient the same way
  straight segments do, and a mid vertex uses the bisector of its two
  tangents. `markerUnits="strokeWidth"` scales the marker with the
  stroke, `orient="auto"` and `auto-start-reverse` rotate it, and
  `refX`/`refY` are mapped through the marker's own `viewBox` before
  positioning.
* `vector-effect: non-scaling-stroke` keeps a stroke's width in device
  space instead of scaling it with the current transform.
* SVG is rendered by the engine instead of librsvg. `src/svg.c` walks
  the SVG DOM and paints it through the same Cairo surface, cascade and
  font stack that HTML uses, and `librsvg` is gone from the dependency
  list, the packaging manifests and the CI images. Inline `<svg>` was
  previously re-serialised to XML and handed to librsvg as an opaque
  raster, so the document's own stylesheet could never reach inside it:
  `fill: currentColor`, `svg .icon { fill: … }` and script-driven
  geometry changes were invisible. SVG elements now take part in the
  normal cascade, so `fill`, `stroke`, `stroke-width`,
  `stroke-dasharray`, `fill-rule`, `stop-color`, `text-anchor`,
  `paint-order` and the SVG geometry properties `x`, `y`, `cx`, `cy`,
  `r`, `rx`, `ry` are real CSS properties that inherit like the rest.
  Covered: paths including elliptical arcs and smooth-curve
  continuation, rect/circle/ellipse/line/polyline/polygon, `viewBox`
  and `preserveAspectRatio`, nested `<svg>`, `<g>`, `<use>`,
  `<symbol>`, `<switch>`, `<defs>`, linear and radial gradients with
  `href` inheritance, `spreadMethod`, `gradientUnits` and
  `gradientTransform`, `clipPath`, group opacity, dashing, and `<text>`
  shaped through Pango. A standalone `.svg` document sizes to the
  viewport rather than to a 300x150 default. Android and iOS, which
  dropped librsvg with the rest of the desktop stack, gain SVG for the
  first time.

CSS
* `text-decoration-color` reaches the painted line. The colour was only
  read from the *block's* style, never from the inline run that carries
  the decoration, so an underline set on an `<a>` was always drawn in
  the text colour -- and a fully transparent one, the idiom behind every
  "underline grows in on hover" teaser, was drawn as a solid line. On
  Tidens Krav and the other Amedia fronts that put an underline under
  every headline and nav link. The decoration attributes now carry the
  style of the element that turned them on, so
  `text-decoration-color: #e00` paints red, and a decoration whose
  resolved colour is fully transparent is not emitted at all. A
  decoration propagated from an ancestor still paints in the ancestor's
  colour, as the spec requires.
* A flex item that is itself a flex or grid container re-aligns its own
  children after the cross-axis stretch resizes it. The item laid its
  children out at its content height, and the stretch then overwrote
  that height in place without a second pass, so anything the item
  centred or bottom-aligned stayed where the pre-stretch height had put
  it. VG's masthead is the shape that shows it: a 56px-tall `header`
  flex row, a logo link inside it that is a flex container with
  `align-items: center`, and a 24px logo that rendered flush against the
  top of the bar instead of centred on it. The column-flex path already
  re-ran layout for a resized item; the row and wrapped-row paths now do
  the same, and only when the stretch actually changed the height.
* A `container-type: inline-size` (or `size`) element no longer sizes
  itself from its own contents. CSS Contain 3 gives such an element
  inline-size containment, so its intrinsic inline sizes are computed as
  if it had no children; the engine measured the children anyway. On a
  page whose container queries feed back into the container -- headlines
  sized in `cqw`, the pattern VG, Aftenposten and the other Schibsted
  fronts use -- that closed a loop: wide contents made the container
  measure wide, `cqw` then resolved against the inflated width and made
  the contents wider still. VG's lead teaser laid out 1245px wide inside
  a 734px column and its headline computed to 276px where the site asks
  for 157px. The three intrinsic-width paths (`measure_natural_width`,
  `measure_min_width`, `estimate_natural_width`) now return zero content
  contribution for such a box, so an explicit `width` still wins and a
  flex or grid item shrinks to the space its parent gives it.
* A square border is painted inside its border box rather than centred
  on the edge. Each side was stroked along the border-box boundary with
  the line width set to the border width, and Cairo centres a stroke on
  its path, so every bordered element rendered half a border wider than
  it laid out on each side -- a 4px border occupied 6..9 and 60..63
  where the box model puts it at 8..11 and 58..61. Layout was always
  right; only the paint was wrong, so borders overlapped whatever sat
  next to them. Rounded borders and border-image already inset
  correctly and are unchanged.
* An `<iframe>` becomes visible as soon as its document loads, on a
  quiet page as well as a busy one. The UA sheet hides frames until the
  engine stamps `data-nd-frame-loaded` on them, but that stamp is
  written by the loader rather than through the scripted attribute
  path, so it never invalidated style. The frame kept the cached
  `display: none` and produced no box at all — its document parsed and
  its scripts ran, entirely unpainted — until some unrelated mutation
  happened to force a restyle. Pages with continuous script activity
  masked it; a page whose only content was a frame never showed it. The
  three places that add or remove the attribute now mark it dirty.
* Inline atomic boxes contribute their full height to the individual
  wrapped line that contains them. Multi-line form controls and table
  cells now reserve the correct vertical space instead of allowing later
  lines to overlap following content, fixing the Google footer position.
* Inherited properties set on the root element reach the rest of the
  page. The UA stylesheet declared `color`, `font-family`, `font-size`
  and `line-height` on `html, body` together, and a UA declaration on
  `body` outranks inheritance from `html` — so a page styling only
  `html` (`html{font-family:"Helvetica Neue","Segoe UI",Arial,
  sans-serif}` on lite.duckduckgo.com) had its font, colour and size
  dropped at `body` and rendered in the UA serif default. The
  declarations now sit on `html` alone and `body` inherits them.
* A concrete font family is used when the system actually has it.
  `Arial`, `Helvetica`, `Segoe UI`, `Roboto` and the SF Pro names were
  rewritten to generic `sans-serif` unconditionally, which resolved
  through fontconfig to whatever the default sans happened to be —
  Noto Sans rather than the requested Segoe UI or Arial. Each name is
  now resolved against the installed families first, and substituted
  by `sans-serif` only when it is missing.
* `calc()` serializes per CSS Values 4 instead of being echoed back as
  authored. A typed math sum — one coefficient per unit, sitting beside
  the px/pct/em/rem value layout uses — sums terms of the same type,
  folds absolute lengths, angles, times, frequencies and resolutions to
  their canonical unit, and distributes products and quotients by a
  number. A sum that cannot reduce to a single term serializes sorted:
  number, then percentage, then dimensions in ASCII-alphabetical unit
  order. `calc(1px + 1%)` is `calc(1% + 1px)`,
  `calc(1px + 2em + 3rem + 4%)` is `calc(4% + 2em + 1px + 3rem)`,
  `calc(2 * (1px + 1em))` is `calc(2em + 2px)`, and a single-argument
  `min()`/`max()` reduces to `calc()`. The quad shorthands serialize the
  same sum rather than dropping every term but px, so
  `margin: calc(1px + 1em) 2px` no longer reads back as `1px 2px`.
  Comparisons that need layout, such as `min(20px, 10%)`, still stay as
  authored.
* `border-image` is implemented (CSS Backgrounds 3): the five longhands
  (`border-image-source`/`-slice`/`-width`/`-outset`/`-repeat`), the
  `border-image` shorthand and its `-webkit-` alias parse, cascade,
  serialize canonically and reach `getComputedStyle`; the `border`
  shorthand resets them. The painter nine-slices the source — raster
  `url()` images and gradients alike — honouring `fill`, percentage and
  number slices, `auto`/length/percentage/number widths, outsets and all
  four `stretch`/`repeat`/`round`/`space` tiling modes, and replaces the
  element's border style while it renders.
* Media Queries Level 4: the heuristic matcher is replaced by a real
  evaluation engine (`src/css_media.c`) with grammar-complete parsing
  (range syntax, boolean context, nested conditions, `and`/`or`/`not`,
  general-enclosed), Kleene three-valued logic and CSSOM media-list
  serialization (`not all` for unparseable queries). Iframe documents
  evaluate against their own viewport, `matchMedia` and
  `CSSMediaRule.media` expose the serialized form, `CSSMediaRule.media`
  is a real `MediaList`, and changing a `media` attribute restyles.
* The native `sheet` / `document.styleSheets` stubs that shadowed the
  real CSSOM are gone. `style.sheet.cssRules` now returns the parsed
  rule tree and `style.sheet === document.styleSheets[i]` holds.
* A script read of a resolved value flushes every pending mutation, not
  just the first one in the task. `getComputedStyle`,
  `getBoundingClientRect`, `offsetWidth`, `scrollIntoView` and the rest
  force a synchronous reflow whenever the document is dirty; the
  wall-clock interval and the oscillation dampener now apply only to the
  rendering tick, where they belong. Mutating a style and reading it
  back in the same task returns the new value, and `:has()`
  invalidation, inset resolution and stylesheet insertion are observable
  immediately.
  Two gaps this makes visible, which the frozen styles had been hiding:
  `scrollWidth`/`scrollHeight` are wrong on inline-level boxes
  (`inline-block`, `inline-flex`, `inline-grid`), and `attr()` is
  substituted when generated content is rendered but not when
  `getComputedStyle().content` is serialized.
* Cascade layers are ordered as a tree rather than by first-declaration
  order across the whole document: sublayers sort inside their parent,
  a layer's own declarations act as its implicit final sublayer, and
  nested anonymous layers stay nested instead of escaping to the top
  level.
* The incremental restyle pass identifies stylesheets by a parse-time
  serial instead of by address. A reparsed `<style>` reusing the freed
  block of the sheet it replaced used to look unchanged, which froze the
  page at stale styles.
* `@scope` preludes are parsed against the grammar and invalid ones drop
  the rule; the prelude is serialized canonically.
* `StyleSheet.media` is a live `MediaList` that writes back to the
  owner node's `media` attribute, and `ShadowRoot.styleSheets` is empty
  for a disconnected tree.
* CSS Display Level 3: `display` is a structured computed value — outer
  type, inner type, list-item flag and layout-internal kind — resolved
  once in the cascade instead of a keyword string that layout, paint and
  the CSSOM each re-read with `strcmp`. Multi-word canonical forms now
  reach layout, so `display: flow-root list-item` keeps its box instead
  of losing it; blockification of floated and absolutely positioned
  boxes has one implementation rather than three; `-webkit-box` and
  `-webkit-inline-box` map to flex and inline-flex.
* Anonymous table boxes are generated around any run of table-internal
  siblings, per CSS 2.1 17.2.1, so `display: table-row` and
  `display: table-row-group` outside a table lay out as tables instead
  of collapsing into the surrounding inline content.
* CSSOM: declaration blocks are canonicalized, rule mutations apply
  synchronously, constructed stylesheets are backed by live rules,
  at-rules are exposed on declarations, and shorthand serialization
  covers the quad shorthands, `all` and the complete shorthand
  families.
* `getComputedStyle` resolved values: insets absolutize against the
  correct containing block, static insets follow the writing mode,
  automatic minimum sizes resolve, and pseudo-element styles compute.
* Declaration grammar is enforced: values with unbalanced brackets or
  quotes, stray `!` or `;`, out-of-place `auto`/`normal`, negative
  values on non-negative properties, or transform functions with bad
  units are dropped instead of being half-parsed.
* `content-visibility: hidden` applies size containment, and
  `writing-mode: vertical-rl/lr` with `text-orientation`
  `upright`/`mixed`/`sideways` measures and paints vertical inline runs.
* Flexbox honours the automatic (content-based) minimum size, and
  column stretch no longer double-counts item margins.
* Media queries inside a frame evaluate against the frame's own size,
  not a 300x150 guess. The viewport pushed while collecting a frame's
  stylesheets came from the frame's inline `style` attribute or its
  `width`/`height` content attributes, so a frame sized by a stylesheet
  rule -- `iframe { width: 100% }`, the common responsive-embed pattern --
  was measured as 300x150 and its `@media (min-width: ...)` blocks
  resolved against a size the frame never had. Layout now records each
  frame's content box, collection prefers it over the default, and when
  the recorded size disagrees with the one a viewport-dependent frame
  sheet was collected under, style and layout run once more so the frame
  settles on its real size. Frames whose CSS carries no width, height,
  aspect-ratio or orientation query never trigger the extra pass. Acid3
  goes from 98/100 to 99/100.
* A frame document's own stylesheet can style its root element. Sheets
  inside an iframe are rewritten to be scoped to the frame's root, and
  every selector whose subject was not literally `html` or `:root` got a
  descendant combinator — so `* { … }` or `.cls { … }` in a framed
  document matched everything inside the frame except the frame's own
  `<html>`, and `getComputedStyle` on that element reported no value for
  any property. The scope marker now also attaches directly to the
  subject compound, and lands before a pseudo-element rather than after
  it. Shadow scopes are unchanged: a shadow host is still not styled by
  its own shadow tree. Acid3 goes from 97/100 to 98/100.
* `getComputedStyle(el).someUnknownName` is `undefined` rather than the
  empty string. The proxy in front of a computed declaration answered
  every string key through `getPropertyValue`; its `has` trap already
  distinguished supported properties from unknown ones, and `get` now
  draws the same line. jQuery's `css()` returns
  `computed.getPropertyValue(name) || computed[name]` and expects
  `undefined` for a property the engine does not know.

Scripting
* Removed the IE-only `attachEvent` and `detachEvent`. They were exposed
  on Element, Document and Window as no-op stubs that returned true and
  registered nothing. Libraries still feature-detect them to select a
  legacy path: RequireJS, finding a native-looking `attachEvent`, bound
  its script-load callback to `onreadystatechange` instead of
  `addEventListener`, the stub swallowed it, and every module load ended
  in "Load timeout for modules". jQuery's test suite could not get past
  its RequireJS bootstrap before this.
* `DOMParser` reports the line and column of an XML parse error. The
  synthesized `parsererror` document carried the bare text "XML parsing
  error"; it now names the position the parser stopped at.

Scripting
* `Intl.DateTimeFormat` names months and weekdays in the requested
  language, and picks the clock the locale actually uses. The month and
  weekday tables held English names only and the hour cycle defaulted to
  12-hour whatever the locale, so `new Date().toLocaleTimeString()` on a
  Norwegian desktop read "2:47:00 PM" instead of "14:47:00", and
  `toLocaleDateString('nb-NO', {weekday: 'long', month: 'long'})` read
  "Tuesday, July 28" instead of "tirsdag 28. juli". Every Nordic news
  front page shows a formatted date, so this was visible on all of them
  -- Aftonbladet's masthead read "TUESDAY, JULY 28, 2026". Month and
  weekday names are now carried for the fourteen languages whose date
  *patterns* the formatter already knew (the Nordic five plus German,
  Dutch, French, Spanish, Italian, Portuguese, Polish and Russian);
  `short` and `narrow` are derived from the long name by UTF-8-safe
  truncation rather than byte truncation, which previously cut a
  multi-byte name mid-character. The 12-hour default is now restricted
  to the locales that use one, `en` (outside GB/IE/ZA) and a dozen
  others; everything else formats h23. Swedish and Lithuanian numeric
  dates serialize in ISO order (`2026-07-28`), and the day-month-year
  languages get their own literals -- the ordinal period in Norwegian,
  Danish, German, Finnish and Icelandic, ` de ` in Spanish and
  Portuguese -- instead of the English comma layout. An explicit
  `hour12`/`hourCycle` option still wins, and `en-US` output is
  unchanged.

Networking
* A top-level navigation follows a redirect that leaves HTTPS. Any
  redirect off `https://` was refused outright, which is the right rule
  for a subresource -- that is mixed content -- but not for a document
  the user asked for. Sunnmørsposten is the common shape: `smp.no`
  answers 302 to `http://www.smp.no/`, whose server immediately sends
  302 back to `https://www.smp.no/`, so the whole site was unreachable
  and rendered as "That address looks malformed". Navigations now follow
  the hop and report the resulting scheme in the security indicator;
  subresource fetches are still blocked exactly as before.
* HTTP/3 can receive a response larger than a megabyte. nghttp3's
  `nghttp3_conn_read_stream()` returns the bytes it consumed *excluding* the
  DATA frame payload — the application is required to extend QUIC's stream
  and connection flow-control credit for the body itself, from the
  `recv_data` callback. The backend extended only for what nghttp3 reported,
  so credit for body bytes was never returned: every HTTP/3 transfer
  deadlocked the moment it reached the 1 MB
  `initial_max_stream_data_bidi_local` advertised at connection setup, and
  sat there until the request timeout expired 30 seconds later. A 10.7 MB
  script that HTTP/2 fetched in 140 ms failed outright over HTTP/3; it now
  completes.
* HTTP/3 resolves the origin over IPv6 as well as IPv4. The QUIC socket
  asked `getaddrinfo` for `AF_INET` only and used the first result, so on an
  IPv6-only network every HTTP/3 hop failed to connect and fell back to
  HTTP/2. It now asks for `AF_UNSPEC` and tries each address in turn, the
  way the TCP path does.
* A UDP socket that reports `EAGAIN` no longer fails the HTTP/3 request. The
  QUIC socket is non-blocking, so a full send buffer is an ordinary
  condition under load; it was treated as a fatal write error and abandoned
  the connection. The datagram is now dropped and left to QUIC loss
  recovery, which is what it is for.
* HTTP/3 loss-recovery and idle timers fire while packets are arriving.
  `ngtcp2_conn_handle_expiry()` was called only when the poll timed out, so
  a connection with steady inbound traffic never processed an expired PTO or
  ACK timer. It is now called every iteration, which is a no-op when nothing
  is due.
* The nghttp2 backend reads the final response of a request that begins
  with an informational one. A `103 Early Hints` — what Cloudflare, Fastly
  and Shopify send ahead of the real response — arrives as a first HEADERS
  block, and libnghttp2 categorises the *final* HEADERS that follows as
  `NGHTTP2_HCAT_HEADERS` rather than `NGHTTP2_HCAT_RESPONSE`, the same
  category it gives trailers. The header callback accepted only
  `HCAT_RESPONSE`, so the page kept the interim status and every real
  header was discarded: no `Content-Type` (an HTML document rendered as
  plain text, its source visible), no `Content-Encoding` (a gzip body
  handed to the sink still compressed), no `Set-Cookie`. The callback now
  admits the block that follows an informational response and drops the
  interim headers instead of the final ones; trailers are still ignored.
  The HTTP/1.1 fallback had the same defect and worse — it treated the
  blank line ending the interim response as the end of all headers, so the
  entire second response, status line included, became the body. It now
  skips interim blocks and parses the response after them.
* A timed-out or cancelled HTTP/2 stream is detached from its session.
  `ns_h2_io_scan_timeouts` sent RST_STREAM and released the request, which
  lives on the requesting thread's stack, but left the session's
  `stream_user_data` pointing at it. DATA or HEADERS already in flight for
  that stream — the ordinary case, since RST_STREAM races a response — then
  drove the header and body callbacks through a dangling pointer and wrote
  into a returned stack frame. Streams are now detached with
  `nghttp2_session_set_stream_user_data()` before the request is released.
* HTTP/2 downloads are no longer capped by the default connection-level
  flow-control window. The session advertised an 8 MB
  `SETTINGS_INITIAL_WINDOW_SIZE`, but per RFC 9113 §6.9.2 that setting
  governs streams only: the connection window stayed at the protocol
  default of 65535 bytes, which throttles *aggregate* throughput on a
  connection to one window per round trip — about 640 KB/s at 100 ms RTT no
  matter how many streams are multiplexed over it. The connection window is
  now raised to match with `nghttp2_session_set_local_window_size()`.
* A request the server refused is retried on a fresh connection. When a
  pooled connection goes away, libnghttp2 closes the streams the server
  never processed with `NGHTTP2_REFUSED_STREAM` — the code exists precisely
  so the request can be sent again — but the retry only covered streams
  that had not been submitted, so a subresource lost this race and failed
  outright instead of being refetched.
* Idle HTTP/2 connections are closed. The pool defined an idle timeout, a
  reuse ceiling and a per-origin cap and enforced none of them: every
  origin visited kept a connection, its TLS state, three file descriptors
  and a live I/O thread polling four times a second until the browser
  exited. A connection with no streams for a minute now stops its I/O
  thread, and the pool drops connections that are dead, idle-expired, over
  the reuse ceiling or beyond the per-origin cap.
* The nghttp2 backend uses the `nghttp2_ssize` API on the versions that
  have it. Upstream deprecated the `ssize_t`-based entry points in favour
  of `…2` variants in 1.60.0 and lets an application compile the old ones
  out entirely with `NGHTTP2_NO_SSIZE_T`; the backend now defines that
  macro and calls `nghttp2_submit_request2()`,
  `nghttp2_session_mem_recv2()` and
  `nghttp2_session_callbacks_set_send_callback2()` when the headers are new
  enough, keeping the deprecated names only as the fallback for older
  libnghttp2. This is what a toolchain without `ssize_t` needs, and it
  makes a future upstream removal a non-event.
* An informational response no longer contributes headers to the HTTP/3
  response that follows it, matching the HTTP/2 and HTTP/1.1 paths.
* The request identity a fetch coalesces and preloads on no longer depends
  on the order its `Accept`-style headers happen to be listed in; the header
  lines are sorted into the key.
* `Vary: Origin` no longer defeats the HTTP cache. `Origin` is now one of
  the headers the cache can resolve at lookup time: `net.c` computes the
  value it will send once and uses that same string both as the request
  header and as the cache selector, so the two can never disagree, and
  the absence of an `Origin` selects distinctly from any present one.
  Google serves its stylesheets `public, immutable, max-age=31536000`
  with `Vary: Origin`; those were being refetched on every load and are
  now cached.
* ES module fetches join the same request identity as every other
  subresource. The module loader passed no top-level URL, so a module was
  partitioned in the HTTP cache under its own site rather than the
  document's — two unrelated sites importing the same module shared one
  cache entry — and it neither coalesced with nor consumed the preload
  issued for the same `<script type=module src>`, since that preload
  carries the JavaScript `Accept` and the module fetch did not. It now
  passes the document URL and the script `Accept`.
* Shutting down no longer hangs a caller waiting on a coalesced fetch.
  `ns_net_drain` discarded queued fetch tasks without telling the
  coalescer, so a task that led a group left the group behind: blocking
  joiners waited on a condition nobody would signal again, and
  asynchronous joiners never had their callback run. A blocking joiner
  now also gives up when the network layer starts aborting, and in any
  case five seconds past the longest transfer timeout a leader can have,
  instead of waiting without a bound. These waits happen on worker
  threads that teardown joins, so one that never returned took the
  joining thread down with it.
* The HTTP cache selects the right variant of a negotiated response.
  `cache.c` keyed entries on URL and partition and stored nothing about
  `Vary`, so a resource served `Vary: Accept` and referenced both as a
  stylesheet and as a script was fetched once and that single variant
  handed to both — a `<script>` element could receive CSS. Entries now
  carry the response's `Vary` and are keyed on a selector built from the
  request headers it names, with an indexed base key so a lookup can walk
  the variants stored for a URL and match the right one. `Accept`,
  `Accept-Language` and `User-Agent` are resolved; `Accept-Encoding` is
  ignored because bodies are stored decoded, which keeps the web's most
  common `Vary` from fragmenting the cache; anything else, including
  `Vary: *`, is not stored rather than stored wrongly. The preload scan
  deduplicates candidates on (URL, destination) instead of URL alone, so
  both variants are preloaded. The cache schema is versioned through
  `PRAGMA user_version` and an upgrade discards the old cache.
* The speculative preloader hands its bytes to the loader that needs
  them through a single deduplication point keyed on the request's
  identity. Preload responses used to be parked in a private store
  keyed on the bare URL and consulted ahead of the HTTP cache. That
  store ignored the cache partition, so within its 20-second window one
  site could be served bytes another site had fetched with that site's
  cookies; it ignored `no-store`; it recorded a placeholder for every
  fetch it started but only removed entries when a loader consumed one,
  so failed preloads and preloaded images — which nothing consumed —
  permanently occupied its 32 slots until the preloader silently
  stopped preloading anything. Deduplication now happens in one place.
  The in-flight coalescer keys on method, URL, cache partition and
  request headers rather than URL plus referrer, and every entry point
  joins it — `ns_net_request_async` and the blocking fetchers as well
  as `ns_net_fetch_async` — so a loader that arrives while a preload is
  still in flight waits for it instead of issuing a second request. A
  preload that finishes first is held in a preload map under that same
  key, handed over by the fetch layer itself so there is no window in
  which a resource is in neither place, and dropped when the next
  navigation begins. The preloader now sends the `Accept` header its
  consumer will send, so content-negotiated resources match. The
  separate external-script prefetcher, a third path over the same URLs,
  is gone. A page with six scripts and five stylesheets issues exactly
  one request per resource, counted at the origin.

Layout and rendering
* Box `x`/`y` uniformly means the margin-box origin, which fixes flex
  and grid items with margins rendering and measuring double-shifted;
  `getBoundingClientRect` derives the border box the same way the
  painter does. Grid row placement was still adding the item's top
  margin on top of that origin, so a negative margin moved the item the
  wrong way by twice the amount.
* Flex items are sized by the flex algorithm rather than by their own
  `width`. `layout_block` used to read `width` back out of the style and
  ignore the main size the container had assigned, so nothing ever
  shrank — `flex-shrink: 1` is the initial value, so every
  over-constrained flex row overflowed instead of fitting.
* The flex main axis is reversed when exactly one of
  `flex-direction: row-reverse` and `direction: rtl` applies, and items
  are then packed from the opposite edge. `row-reverse` used to reverse
  the item order but still pack against the left edge, and `rtl` was
  ignored for the main axis entirely.
* `scrollWidth`/`scrollHeight` measure the real scrollable overflow
  region from descendant border boxes, including overflow from
  negative margins on non-scrolling boxes.
* Escaping floats and compact line boxes lay out correctly, the
  non-rendering elements (`area`, `base`, `link`, `meta`, `param`,
  `source`, `track`, …) are excluded from inline layout, anonymous
  table cells lay out as blocks, and malformed legacy declarations are
  skipped rather than derailing the rest of the block.

HTML, DOM and JavaScript
* Core content-attribute reflection is complete, and the text-control
  selection APIs (`selectionStart`/`End`/`Direction`,
  `setSelectionRange`, `textLength`) match the spec including per-type
  applicability and the `IndexSizeError`/`InvalidStateError` cases.
* Mutation observers and shadow trees align with the spec: old-value
  records for attributes and character data, `attachShadow` options,
  `assignedSlot`, and `ShadowRoot.styleSheets`/`activeElement`/
  `elementFromPoint`.
* Frames are isolated per document: events, event handlers and the
  `Performance` objects belong to the frame's own realm, so scripts
  inside an iframe see their own `window` and `document`.
* Speedometer 3.1 fixes: nested custom-element upgrades during
  construction, unhandled rejections queued to the microtask
  checkpoint, module scripts in iframes seeing the frame realm,
  `ownerDocument` returning the realm wrapper, frame windows in the
  event propagation path, images in frames resolving relative URLs
  against the frame document, and the always-rejecting
  `navigator.wakeLock` stub removed so feature detection falls back
  cleanly.
* Promise rejection events: cancelable `unhandledrejection` carrying
  `promise`/`reason`, with `rejectionhandled` for rejections handled
  later.
* `XMLHttpRequest` gains the upload object, the full progress event
  set, `responseXML` and method/state validation; `MessageEvent` and
  `ExtendableMessageEvent` follow the messaging spec.
* `Navigator` and friends (`MimeTypeArray`, `PluginArray`,
  `NetworkInformation`, `StorageManager`, `UserActivation`,
  `NavigatorUAData`, `MediaCapabilities`, `MediaDevices`) are real
  interfaces with non-constructible prototypes, so brand and
  `instanceof` checks agree.
* Cookie Store API: promise-based `cookieStore.get`/`getAll`/`set`/
  `delete` over the document cookie jar.
* Legacy media and timing surfaces: the `HTMLMediaElement`/
  `MediaError` constants, `PerformanceTiming` and
  `PerformanceNavigation`; `blob:` URLs work as module script sources.
* `<input type=email>` validation follows the HTML email grammar, and
  Trusted Types plus void-element serialization are tightened.
* Reading an `<iframe>`'s `contentDocument` or `contentWindow` more than
  once no longer aborts the process. The realm document's API was
  installed onto the per-node wrapper every time it was requested, and
  the second install hit QuickJS's "property already exists" abort in
  `JS_DefineAutoInitProperty`. The install now runs once per wrapper, so
  repeat reads return the same document, as the DOM requires.

Identity and privacy
* `Sec-CH-UA` and `navigator.userAgentData` report `"Nordstjernen"`
  instead of impersonating Chromium and Google Chrome;
  `getHighEntropyValues` fills in `formFactors`, `platformVersion`,
  `architecture`, `bitness`, `wow64`, `model`, `uaFullVersion` and
  `fullVersionList`.
* New `--private` command-line flag starts the browser in private
  browsing mode.
* The Chrome compatibility token in the user agent moves to 150.

Media
* MSE segments are tracked with their byte offset and probed time
  range. `SourceBuffer.remove()` rebuilds the helper input from the
  initialization segment plus the retained segments, which keeps
  long-running adaptive playback (YouTube, Vimeo) inside its byte
  quota, and `SourceBuffer.buffered` reports the retained demux range
  instead of a synthetic zero-based one. `remove()` no longer refuses
  ranges that are not a clean prefix.
* `@font-face` sources are only fetched when their `unicode-range`
  covers a codepoint the page actually uses, so font-heavy pages stop
  competing with streaming playback for bandwidth.
* `<video>`/`<audio>` expose `seeking` and `played`, and fire
  `ratechange` and the seek events.

Layout
* A flex item with padding or a border is no longer sized smaller than
  its content. The automatic flex base size came from
  measure_natural_width(), which already reports a content width, and
  then subtracted the item's own padding and border a second time — the
  surrounding code tracks those separately. Items came out exactly one
  padding-and-border narrower than they should be, so buttons and labels
  in a flex row wrapped mid-phrase for no reason. A row of consent
  buttons that Chrome lays out as two single-line pills was wrapping to
  two lines each; it now matches. Items without padding were unaffected,
  which is why this survived so long.

Headless
* `--dump=png` no longer crashes on pages that keep scripting busy while
  media is fetched. The video prefetch held box pointers across a
  blocking fetch, and the nested main loop that fetch runs can relayout
  the page and free them underneath it. It now resolves the URLs, then
  re-finds the box before attaching, so no box outlives a fetch.

Performance
* A page using container units settles in two container passes instead
  of three. Container queries are resolved by iterating cascade and
  layout until the styles stop changing, up to three times. The loop
  compared the two style tables to decide, which meant the third pass
  always ran a full cascade before discovering it had nothing to do.
  It now compares the container geometry the cascade actually reads --
  and only the axis a query can observe, so the block size of a
  `container-type: inline-size` element, which no query and no `cqh`
  unit can ever see, no longer counts as a change. VG's front page is
  the shape this was costing: seventeen inline-size containers whose
  heights kept moving while their widths had already converged, so
  every relayout paid for three cascades and three layouts. A relayout
  there drops from ~1.6 s to ~0.6 s, and the page, which previously
  never finished rendering at all, now settles in 23 s.
* Nested flex rows no longer lay out in exponential time. A flex item
  that stretches to the line's cross size was laid out once against its
  natural height and then, because its own children had been aligned
  against the wrong height, laid out a second time. `align-items:
  stretch` is the default, so this doubled the work at every level of
  flex nesting: layout cost 2^depth. A synthetic page of nested
  stretched rows took 6.4 s at depth 14, 71 s at depth 16, and killed
  the renderer at depth 18. The stretched cross size is known before the
  item is laid out, so it is now handed down as the item's definite
  height on the first pass and the second pass is skipped. The same page
  takes 39 ms at depth 14, 89 ms at depth 16, and 1.3 s at depth 20.
  Real pages are shallower but wide: this is layout work removed from
  every flex row on every page, not only deep ones.
* Container queries no longer defeat incremental restyle. The second
  cascade pass — the one that runs with container sizes known — took the
  branch that throws the previous pass's computed styles away, so every
  page using `@container` re-cascaded every element from scratch on
  every relayout, forever. The cache now survives that pass, and rules
  carrying a container condition no longer force conservative
  invalidation keys either: those rules are inert in the first pass,
  which is the only one incremental restyle runs in. On a 1610-element
  container-query page churning through 13 relayouts, cascade time drops
  from 47ms to 12ms and style reuse goes from 0 to 1609 of 1610
  elements per pass.
* A `:has()` selector whose subject is matched by an attribute — say
  `[data-state]:has(...)` — keys invalidation on that attribute instead
  of switching incremental restyle off for the whole page. `class`, `id`
  and `style` are excluded, since keying on those matches nearly every
  element and floods the document anyway.
* `:nth-child`/`:nth-last-child` sibling indices are computed once per
  selector-matching batch instead of per element.
* Live DOM collections use the QuickJS array-index atom fast path, and
  childlist invalidation is narrowed to the affected parent.

User interface
* The `about:start` splash is redesigned as a 1997 Netscape release
  screen — beveled chrome, dithered sky, receding cyberspace grid —
  then reworked into a daytime scene whose ground is a procedurally
  generated planet surface sampled through the real pinhole
  projection, with an ocean-to-snow terrain ramp, clouds, a specular
  sun column and atmospheric haze into the limb. The loading bar is
  gone and the animation is 322 KB, down from 1.8 MB.
* Android: the kebab icon is readable on the toolbar, nested consent
  dialogs scroll, and the renderer thread no longer frees the
  shell-owned framebuffer.

Documentation, build and CI
* A whole-system architecture poster (`docs/Software-Architecture.png`)
  maps the process tree, IPC boundaries, engine pipeline, module
  dependencies and information flows; the generators live in
  `scripts/arch-diagram/`.
* The CSS and HTML compatibility documents are refreshed, and
  `docs/media.md` describes the MSE eviction model.
* CI hardens the V8 artifact download with retries and integrity
  checks, and the V8 smoke test now covers microtasks and timers.
* The readme links the openSUSE RPM directly.
