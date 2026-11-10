# Security audit, November 2026

A source review of the whole tree for security vulnerabilities, at commit
`55be631` (1.0.9-dev). The threat model is the one in
[SECURITY.md](../SECURITY.md): the attacker controls fetched content and the
network; the user, the kernel and the filesystem outside the sandbox
allow-list are trusted.

## Method and limits

Two reviewers read the code in parallel, one on networking, web security
policy and process boundaries, one on memory safety, `unsafe` code and the
parsers of untrusted content. The main findings were then re-read against the
source.

- **Nothing was built or run.** No finding has a working proof of concept.
  "Confirmed" means the data flow was traced in the source; "Likely" means
  the code is confirmed and the impact depends on something outside it (a GL
  driver, a server); "Needs verification" means the path exists and its
  reachability was not established.
- **Coverage is partial.** About 400,000 lines of Rust were in scope and the
  review went deep on the areas an attacker reaches first. What was not read
  is listed under [Coverage](#coverage); absence of a finding there is not a
  clean bill.
- Line numbers refer to commit `55be631`.

## Summary

No memory-corruption bug reachable from web content was confirmed. The one
serious finding is in cross-origin frame isolation (S-01): frames share a
script context with their embedder and the frame bootstrap passes live
objects across the origin boundary in both directions. WebGL has two
medium findings (uninitialised memory, a bounds check that a deleted buffer
defeats), and the cookie store lets a plain-HTTP response replace a `Secure`
cookie.

| ID | Severity | Confidence | Finding |
|----|----------|------------|---------|
| S-01 | High | Confirmed (not executed) | Cross-origin frames exchange live object references with their embedder |
| S-02 | Medium | Needs verification | Frame scripts run in the embedder's realm when no frame realm can be made |
| S-03 | Medium | Confirmed | Cookie store: insecure origin overwrites `Secure` cookies; no size or count limits |
| S-04 | Medium | Confirmed / driver-dependent | WebGL exposes uninitialised GPU memory |
| S-05 | Medium | Likely | WebGL draw validation is keyed on reusable GL buffer names |
| S-06 | Medium | Confirmed unsound; no live misuse found | DOM string accessors return borrows that a safe mutation frees |
| S-07 | Low–Medium | Confirmed | WebSocket SameSite and mixed-content checks use the top-level page, not the calling document |
| S-08 | Low–Medium | Confirmed | Script-triggered downloads need no user gesture and are buffered in the shell |
| S-09 | Low | Confirmed | `fetch()` does not validate the method as an HTTP token |
| S-10 | Low | Likely | A page can dismiss the Safe Browsing interstitial for its own host |
| S-11 | Low | Likely | `X-Frame-Options` / `frame-ancestors` check only the immediate embedder |
| S-12 | Low | Confirmed | CORS preflight cache ignores credentials mode |
| S-13 | Low | Confirmed | Seccomp allow-list is wider than SECURITY.md implies |
| S-14 | Low | Confirmed | Ancestor check stops at 512 levels, so script can build a cyclic DOM |
| S-15 | Low | Likely | Shell maps helper-created shared memory the helper can shrink |
| S-16 | Low | Confirmed | WebGL object arguments ignore object kind and owning context |
| S-17 | Low | Confirmed | Shader source reaches the driver compiler unvalidated |
| S-18 | Low | Confirmed | WebGL permission is allow-by-default; SECURITY.md says otherwise |
| S-19 | Low | Confirmed | WebAssembly has no execution budget |
| S-20 | Low | Confirmed | Box tree holds raw style pointers kept valid only by call order |
| S-21 | Info | — | Smaller notes |

## Findings

### S-01 — Cross-origin frames exchange live object references with their embedder

**High** (Critical if the attacks below work when run). Confirmed by code
reading; not executed.

- `rust/js-frames/src/bootstrap.rs:149, 157, 248, 288, 298, 305–319`
- `rust/js-frames/src/realm.rs:78–83, 185–199`
- partial mitigation: `rust/js-runtime/src/ffi/mod.rs:476–496`
  (`compile_strings_allowed`)

All frames of a tab share one Boa context, each in its own realm. The frame
bootstrap runs in the frame's realm but closes over `realWin`, the embedder's
real global object, and uses it at call time. The same bootstrap serves
cross-origin frames (`sandbox & 8192`).

```js
def('fetch', { writable: true, value: function(input, init){
    ...
    return realWin.fetch.call(win, req, init); } });

function tagAs(obj, iface){
  try { var proto = realWin[iface] && realWin[iface].prototype;
    if (proto) Object.setPrototypeOf(obj, proto); } catch (e) {}
...
function evt(type, props){ ... e = new realWin.Event(type); ... }
function mk(u, b){ try { return new realWin.URL(u, b || url); } ... }
```

**Embedder reads a cross-origin frame.** `realWin.fetch` is looked up on the
embedder's global when the frame calls `fetch()`. An embedder that replaces
its own `window.fetch` receives the framed document's global as `this`, and
through it the frame's `document` and script state. Replacing `window.URL`
likewise exposes every URL the frame's `location` computes and lets the
embedder forge the frame's `location.origin`.

**Framed content reaches its embedder.** The frame's `location` and `history`
objects are given the embedder's `Location.prototype` and
`History.prototype`, and events passed to frame listeners are instances of
the embedder's `Event`. From any of them the prototype chain leads to the
embedder's `Object.prototype` and `Function.prototype`, which the frame can
then modify. A getter installed there runs with the embedder's window as
`this` on the embedder's next lookup of an undefined global.

The `HostEnsureCanCompileStrings` gate stops
`otherRealmObject.constructor.constructor("…")`, and the pristine/`realmClone`
machinery protects cloned platform properties. Neither stops ordinary
property reads and writes on a leaked object. Whether individual DOM natives
check the calling realm was not examined.

**Status: partly fixed.** Run headless with a page on `127.0.0.1` framing one
on `localhost`, the unpatched build showed both directions. The frame
bootstrap now takes `fetch`, `URL`, `Event`, `DOMException`, `Range` and the
cross-window `postMessage` from the pristine snapshot cloned into the frame's
realm, copies only pristine platform values into a cross-origin frame, and
does not give its `location` a shared prototype: the embedder's overrides no
longer see the frame, and the frame's events, promises, `location` and
`history` are its own realm's. **Still open:** about 120 interfaces defined in
script (`Blob`, `Headers`, the streams, `DOMRect`, `DOMException`, the
IndexedDB and CSSOM rule classes, …) are shared with a frame by the realm
cloner (`rust/js-frames/src/cloner.rs`, `forwarder_property`) together with
their prototype objects, so a cross-origin frame can still reach and modify
the embedder's `Object.prototype` through them. Closing that needs each realm
to have its own copy of those interfaces, or per-site renderers.

**Fix.** A cross-origin bootstrap must not touch `realWin` dynamically.
Capture the natives it needs once from the pristine snapshot, or bind
frame-realm `fetch`, `URL`, `Event` and `DOMException` from Rust as
`page.rs` does for the main realm. Use the frame realm's own
`Location.prototype`, `History.prototype` and `Range`, and do not copy
`realWin.document` properties into the frame document. Longer term, a
cross-realm access check keyed on `realms_same_origin`, or the per-site
renderers SECURITY.md already names, removes the class. Run both attacks
headless before and after the fix.

### S-02 — Frame scripts run in the embedder's realm when no frame realm can be made

**Medium** (High if an attacker can force the fallback). Needs verification:
when `make_realm_context` returns `None` was not established.

- `rust/js-frames/src/scripts.rs:358–395, 430–470`
- `rust/js-frames/src/realm.rs:309–376`

```rust
match realm_window {
    Some(made) if ... => { publish_realm_window(...); run_in_realm(js, made, iframe, &collected.classic); }
    _ if !collected.concat.is_empty() => run_in_page_realm(js, &collected.concat, &collected.exposed, ...),
```

If creating the frame realm fails, the frame's classic scripts are evaluated
in the current realm behind a `Proxy` scope, with no check that the frame is
same-origin. The proxy is not a boundary: `parent` is `realWin` itself.
Separately, `run_modules` redefines `window`, `self`, `top`, `parent`,
`document`, `location` and `history` on that realm's global for the duration
of a frame's module evaluation, including microtask drains.

**Status: fixed.** A cross-origin frame, or a sandboxed one without `allow-
same-origin`, whose realm could not be made now runs no script at all, classic
or module. A frame that has a realm runs its modules there directly; the
embedder's `window`, `document`, `location` and `history` are no longer
swapped for the frame's while they evaluate. Checked headless with module
scripts in a cross-origin and a same-origin frame.

**Fix.** Fail closed: a cross-origin frame, or a sandboxed one without
`allow-same-origin`, runs no script when it has no realm. Remove the global
swapping in `run_modules` now that modules evaluate in the frame realm.

### S-03 — Cookie store: insecure origin overwrites `Secure` cookies; no limits

**Medium.** Confirmed.

- `rust/netstore/src/cookies.rs:357–425` (`store`), `96–154` (`header`),
  `283–330` (`parse`)

```rust
let same = f[0].eq_ignore_ascii_case(file_domain) && f[2] == path && f[5] == name;
let dead = expiry != 0 && expiry < now;
if !same && !dead {
    out.extend_from_slice(line);
```

- `store` never looks at the existing line's Secure field. A network attacker
  who answers `http://example.com/` with `Set-Cookie: session=…; Path=/`
  replaces the HTTPS site's `Secure` cookie: session fixation, which RFC
  6265bis ("leave secure cookies alone") forbids. `__Host-` and `__Secure-`
  cookies are protected.
- `header()` keys the outgoing `Cookie:` header on name alone, so a later jar
  line with the same name and a different domain or path replaces the
  earlier one.
- There is no cap on cookie size, cookies per domain or jar size; the whole
  jar is rewritten on each `Set-Cookie`.
- `Domain=` is accepted for IP-literal hosts (not exploitable while jars are
  site-keyed).

**Status: fixed.** A cookie set from a plain-HTTP origin is dropped when a
live `Secure` cookie of the same name domain-matches it and its path covers
the new one. The `Cookie` header is keyed on name, domain and path and lists
longer paths first. A name and value over 4096 bytes are refused, a domain
keeps its newest 180 cookies and a jar its newest 3000, and `Domain=` on an
IP-literal host must equal the host.

**Fix.** Refuse a non-HTTPS set when a live `Secure` cookie of the same name
domain-matches. Key the header on (name, domain, path) and send every match,
longest path first. Cap name plus value at 4096 bytes and cookies per domain
at about 180. Reject `Domain` when the host is an IP literal.

### S-04 — WebGL exposes uninitialised GPU memory

**Medium** (High on software GL, where storage comes from the renderer's
heap). The missing initialisation is confirmed; what leaks depends on the
driver.

- `rust/webgl/src/methods.rs:645–656` — `bufferData(target, size)` zero-fills
  only `ELEMENT_ARRAY_BUFFER`:

  ```rust
  if is_elem {
      let zero = (size > 0).then(|| vec![0u8; size]);
      gl::buffer_data(target, size, zero.as_deref(), usage);
  } else {
      gl::buffer_data(target, size, None, usage);
  }
  ```

- `rust/webgl/src/methods2.rs:30–33, 53` — `texStorage2D`, `texStorage3D`,
  `renderbufferStorageMultisample` pass straight through.
- `rust/webgl/src/methods.rs:158–164` — `renderbufferStorage` and
  `copyTexImage2D` likewise.
- `rust/webgl/src/context.rs:170–222, 506–516` — `resize()` reallocates the
  drawing buffer and never clears it; only context creation clears, and only
  the colour buffer.

A page allocates storage or sets `canvas.width`, then reads it back with
`readPixels`, `toDataURL` or `getBufferSubData` (`methods2.rs:379`) and
receives recycled driver memory. Renderers are per tab, so that can include
a cross-origin frame's WebGL pixels or decoded media. Textures created by
`texImage2D(…, null)` are already zero-filled.

**Status: fixed.** `rust/webgl/src/fresh.rs` zero-initialises what WebGL
allocates: `bufferData(size)` for every target, the drawing buffer's colour,
depth and stencil on creation and on resize, every level, face and layer of
`texStorage2D`/`texStorage3D` textures, and renderbuffers (through a scratch
framebuffer, with the page's clear state saved and restored). `copyTexImage2D`
allocates zeroed and copies only the part inside the drawing buffer.
`texStorage*` with a format outside the WebGL 2 table raises `INVALID_ENUM`.
Run on software GL the calls work and leave GL state as it was; the leak
itself does not show there, since fresh memory is zero.

**Fix.** Zero-fill `bufferData(size)` for every target. Clear every level,
layer and attachment after `texStorage*`, `renderbufferStorage*` and
drawing-buffer reallocation. Make `copyTexImage2D` zero the region outside
the framebuffer.

### S-05 — WebGL draw validation is keyed on reusable GL buffer names

**Medium.** Likely: the logic is confirmed and relies on the driver reusing
deleted names.

- `rust/webgl/src/context.rs:55, 237–248, 303–356`
- `rust/webgl/src/methods.rs:606–615`

Buffer sizes live in `buffer_sizes: HashMap<u32, usize>` keyed by GL name,
and `attribs_cover` validates a draw by asking GL for each attribute's buffer
name and looking its size up. `deleteBuffer` drops the entry and calls
`glDeleteBuffers`, which detaches the buffer only from the current vertex
array object. A buffer still attached to another VAO stays alive under its
old name while the name becomes free; a new, larger buffer that receives the
same name makes validation pass against the wrong size, and the draw fetches
vertices beyond the small buffer. The result is an information leak or a
crash wherever the driver does not enforce robust buffer access. The
element-buffer shadow and `uniform_blocks_bound` follow the same pattern.

**Status: fixed.** Reproduced on the unpatched build with software GL: after
deleting a 16-byte buffer still attached to another vertex array and creating
a 1 MB buffer, a draw past the 16 bytes went through.
`rust/webgl/src/buffers.rs` now records which vertex array slot holds which
buffer. A deleted buffer that another vertex array still holds keeps its size
entry and its GL name is retired: `createBuffer` never hands a retired name
out, and a deleted buffer's wrapper can no longer be bound. The name is
released when the last vertex array lets go of it. The same draw is now
refused.

**Fix.** Give each `WebGLBuffer` a never-reused host id and track attribute,
element and indexed bindings per VAO in the host. As a stopgap, defer
`glDeleteBuffers` while any VAO, transform feedback object or indexed binding
still references the buffer.

### S-06 — DOM string accessors return borrows that a safe mutation frees

**Medium** (soundness; latent use-after-free). The API is confirmed unsound;
no exploitable caller was found.

- `rust/dom/src/handle.rs:134–155, 207, ~405–455`
- `rust/dom/src/data.rs:419–437, 491–530`

```rust
pub fn text(self) -> Option<&'a CStr> { self.slot.text().map(NulString::c_str) }
pub fn attr(self, name: &CStr) -> Option<&'a CStr> { crate::attrs::get(self, name) }
```

`'a` is unconstrained (`Node::alloc`, `WeakNode::get` yield `Node<'static>`),
and the safe functions `set_text`, `set_name`, `Attr::set_value`,
`remove_first_attr` and `free_tree` drop the storage these borrows point
into. Holding the result of `attr()` across `set_attr()` on the same node is
a use-after-free in safe code. The `// SAFETY:` comments and `docs/dom.md`
state a rule the compiler cannot check, and the classic browser bug —
borrow an attribute, run script, keep using the borrow — is exactly the
shape it permits. The sampled callers (`js-mutation`, `js-storage`,
`js-forms`, `js-focus`) copy before calling out, but there are over a
hundred borrow sites in the `js-*` crates.

**Status: fixed (mitigated without changing the API).** Node strings are now
always heap allocations, and on a thread that runs page script a replaced or
freed name, text or attribute value is parked (`rust/dom/src/reclaim.rs`) and
only freed at the start of a browser tick when no script is running. A borrow
held across a mutation, or across script, therefore reads stale text, not
freed memory or a reused inline buffer. The accessors keep their signatures,
so the rule in `docs/dom.md` still stands for correctness; a string appended
to in place can also still change under a borrow. A script that replaces
strings in a loop keeps the old ones until its task ends.

**Fix.** Return owned or reference-counted strings; or expose closure
accessors guarded by a per-slot borrow flag; or at minimum mark the
borrowing accessors `unsafe` so every call site falls under the unsafe
budget.

### S-07 — WebSocket SameSite and mixed-content checks use the top-level page

**Low–Medium.** Confirmed for WebSocket; EventSource was not checked.

- `rust/net/src/credentials.rs:30–45`
- `rust/js-net/src/socket.rs:425–441, 476–484`
- `rust/websocket/src/ffi.rs:158–172`

`for_subresource` computes the cookie context from the partition (top-level)
URL. A cross-site frame inside `bank.example` that opens
`wss://bank.example/ws` therefore sends `SameSite=Lax` and `Strict` cookies;
fetch and XHR use the requesting document and send only `SameSite=None`.
`Origin` is correct, so only servers that do not check it are exposed. The
WebSocket mixed-content check also uses the top page, so an HTTPS frame in an
HTTP page may open `ws://`.

**Status: fixed, not exercised.** `for_subresource` takes the origin of the
document that opened the socket and treats the handshake as cross-site unless
both that origin and the top-level page are same-site with the target. The
mixed-content check refuses `ws://` when either the calling document or the
top page is HTTPS. In local tests WebSocket handshakes carried no cookies in
either build, so the SameSite change was checked by reading only.

**Fix.** Pass the requesting document's URL to `for_subresource` as the
SameSite initiator, and base the mixed-content check on the calling
document.

### S-08 — Script-triggered downloads need no user gesture and are buffered in the shell

**Low–Medium.** Confirmed. Whether `sandbox` without `allow-downloads` is
enforced elsewhere was not checked.

- `rust/js-focus/src/activation.rs:140–158`
- `rust/browser/src/callbacks.rs:76–90`
- `rust/shell/src/ffi/downloads.rs:76–90, 147–200`
- `rust/shell/src/ffi/window.rs:871–882`

A synthetic `a.click()` on `<a download>` starts a download with no
transient-activation check, no rate limit and no prompt. Each download is
read whole, up to 1 GiB, into the memory of the GTK shell — the process
without a syscall filter — before it is written, so a page can exhaust shell
memory or fill the Downloads directory. The shell also trusts the
renderer-reported page URL when deciding that a `file:` download is allowed.
Filename sanitising is sound.

**Status: partly fixed, not exercised.** A click on `<a download>` without
transient user activation starts at most one download every 30 seconds per
renderer, a sandboxed frame without `allow-downloads` starts none, and the
shell runs at most four downloads at once; further ones fail in the downloads
list. The `file:` concern is already covered: the shell accepts a local page
only for a load it started itself (`pending_local` in
`rust/shell/src/ffi/procview_result.rs`). **Still open:** a download is still
read whole into the shell's memory, up to 1 GiB each; streaming to a file
needs a streaming sink in `rust/net`. Headless mode does not surface download
requests, so this was built and smoke-run only.

**Fix.** Require user activation, or one prompt per origin for repeated
downloads; honour `allow-downloads`; stream to a temporary file under a size
cap; have the shell allow `file:` downloads only in a tab it navigated to a
`file:` URL itself.

### S-09 — `fetch()` does not validate the method as an HTTP token

**Low.** Confirmed.

- `rust/js-net/src/fetch.rs:342–372` (XHR does check: `xhr.rs:210`)
- `rust/http/src/client.rs:121–129`, `rust/http-wire/src/h1.rs:13–30`

The method passes only `normalize_method` and the exact-match
`is_forbidden_method`. CR and LF are caught later, but spaces and other
non-token bytes reach the request line, and `"TRACE x"` is not `TRACE`.
HTTP/1.1 connections are never reused and cross-origin non-simple methods
are preflighted, which limits the impact to lenient servers and proxies.

**Status: fixed.** `fetch()` and the `Request` constructor reject a method
that is not an HTTP token with `TypeError`, alongside the forbidden methods,
and the HTTP client no longer puts a non-token method on the request line at
all (it falls back to `GET` or `POST`, as it already did for CR and LF).
Checked headless: `'GET /x HTTP/1.1 '`, `'TRACE x'` and `'pat ch'` are
refused, `QUERY` and `get` are sent.

**Fix.** Reject with `TypeError` unless `headers::is_token(method)`; make
`h1::request_head` refuse a method, path or authority containing a space or
control byte.

### S-10 — A page can dismiss the Safe Browsing interstitial for its own host

**Low.** Likely.

- `rust/browser/src/callbacks.rs:30–57`, `rust/browser/src/open.rs:206–219`

`allows_navigation_url` accepts `southstar-unsafe-continue:<url>` whenever
the target host equals the current page's host, and opening it allow-lists
the whole host. A page on a host that is not itself flagged can navigate to
the continue URL for a flagged path on the same host, and the user never
sees the warning.

**Status: fixed, not exercised.** The page the browser builds for the warning
is marked (`NsBrowser::interstitial`), and `allows_navigation_url` accepts
`southstar-unsafe-continue:` only from that page, with the host check kept. I
could not get a local blocklist entry to trigger the interstitial in headless
mode, so this is checked by reading only.

**Fix.** Accept the scheme only when the current document is the
interstitial, by a renderer-side flag set when it is shown.

### S-11 — `X-Frame-Options` / `frame-ancestors` check only the immediate embedder

**Low.** Likely; how `embedder` is computed for nested frames was not traced.

- `rust/js-frames/src/load.rs:57–83`, `rust/csp/src/lib.rs:355–362`

`framing_blocked` compares against one URL. `SAMEORIGIN` and
`frame-ancestors` must hold for every ancestor, or attacker → framable
victim page → protected victim page passes. Multiple or comma-joined header
values are not handled either.

**Status: fixed.** `framing_blocked` checks `X-Frame-Options` and `frame-
ancestors` against the immediate embedder and against every outer frame's
document and the top page (http and https ancestors). A comma-joined `X-Frame-
Options` is split, so `SAMEORIGIN, DENY` blocks; before, such a value matched
neither word and the page was framed. Checked headless against a local server:
the comma-joined case was framed cross-site by the old build and is refused
now, and same-origin double framing still renders. The old build did not
render the cross-site double-framed page in this test either, so that half of
the finding was not reproduced.

**Fix.** Walk every ancestor frame URL and require each to pass.

### S-12 — CORS preflight cache ignores credentials mode

**Low.** Confirmed.

- `rust/js-net/src/preflight.rs:87–99, 161–218`

Grants are keyed on origin and URL, so one obtained without credentials
(where `*` is legal) is reused for `credentials: 'include'`; `*` in
`Access-Control-Allow-Methods`/`-Headers` is accepted regardless of
credentials; a redirected preflight is accepted. The final response check is
correct, so nothing unreadable becomes readable — a non-simple credentialed
request may be sent when the server agreed only to the anonymous form.

**Status: fixed.** A cached grant records whether its preflight was for a
credentialed request and an anonymous grant is not reused for `credentials:
'include'`; `*` in `Access-Control-Allow-Methods` and `-Headers` counts only
without credentials; a credentialed preflight must itself allow credentials; a
redirected preflight fails. Reproduced headless against a local server: the
old build sent the credentialed `PUT` on the anonymous wildcard grant, the new
one sends a second preflight and blocks it, while an explicit grant still
works for both.

**Fix.** Add the credentials flag to the grant key, ignore `*` with
credentials, fail a redirected preflight.

### S-13 — Seccomp allow-list is wider than SECURITY.md implies

**Low** (needs a prior renderer compromise). Confirmed.

- `rust/sandbox/src/lib.rs:202–470`, `rust/sandbox/src/ffi.rs:585–650`

- `clone` and `clone3` are allowed without a flag filter, so
  `clone(CLONE_NEWUSER | …)` reaches the namespaces `unshare` is denied.
- `kill`, `tkill`, `tgkill`, `pidfd_open`, `pidfd_send_signal`,
  `rt_sigqueueinfo`, `prlimit64`, `setpriority` and `sched_setattr` are
  allowed with no PID namespace: a compromised renderer can signal any
  process of the same user, the shell included.
- SysV IPC is allowed, so other applications' shm segments are attachable.
- `ioctl` (bar `TIOCSTI`/`TIOCLINUX`) and `prctl` are unfiltered.
- The W^X rule blocks only simultaneous write and execute; map writable,
  write, then `mprotect` executable passes.

**Status: mostly fixed.** The renderer's argument filter now refuses `clone`
with any namespace flag and answers `clone3` with `ENOSYS` (glibc falls back
to `clone`); limits `kill`, `tgkill`, `rt_sigqueueinfo` and
`rt_tgsigqueueinfo` to the renderer's own process and `prlimit64`,
`sched_setattr` and `setpriority` to the caller; and refuses `tkill`,
`pidfd_open`, `pidfd_send_signal` and System V shared memory and semaphores.
Checked headless: the filters load, threads start, and DOM, fetch and WebGL
pages run. **Left as it was:** `ioctl` and `prctl` are not filtered further
(GPU drivers need a wide `ioctl` surface), and `mprotect` may still make
written memory executable, because the software GL driver compiles shaders
that way; SECURITY.md now says so instead of claiming W^X is enforced.

**Fix.** Filter `clone` to thread-creation flags and answer `clone3` with
`ENOSYS`; restrict the `kill` family and `prlimit64` to the caller's own
process; drop SysV IPC if unused; allow-list `prctl` options; consider
denying `mprotect(PROT_EXEC)` in renderers without WebGL.

### S-14 — Ancestor check stops at 512 levels, so script can build a cyclic DOM

**Low** (hang of the tab's renderer). Confirmed.

- `rust/js-node/src/lib.rs:44–60`, used by `validity.rs:75`, `child.rs`,
  `sequence.rs`

`within()` gives up after `MAX_DEPTH` parents and answers `false`. It is the
only guard against inserting a node into its own descendant, and insertions
are not depth-limited, so appending the top of a chain deeper than 512 to its
deepest node raises no `HierarchyRequestError` and creates a parent cycle.
Unbounded walkers such as `ancestors_and_self` (`rust/dom/src/lib.rs:41–43`)
then never return, and the script interrupt cannot stop a native loop.

**Fix.** Make the check exact (walk to the real root), or reject insertions
beyond a maximum tree depth.

### S-15 — Shell maps helper-created shared memory the helper can shrink

**Low** (a compromised media helper can crash the shell). Likely.

- `rust/media-shm/src/map.rs`, `rust/video-helper/src/ffi/ring.rs:30–38`,
  `rust/audio-helper/src/ffi/clock.rs:26`,
  `rust/shell/src/ffi/procview_media.rs:381, 477`

The shell opens a named shm object the helper created and maps it at its
size at open time. The helper, which runs libavcodec on attacker media, can
truncate it afterwards; the shell's next read takes `SIGBUS` and every tab
dies. Header and geometry validation in the reader is good. `Ring::create`
multiplies `stride * height` in `u32`, which wraps in release builds
(harmless today because `frame()` rejects the result), and on Windows
`CreateFileMappingA` silently opens an existing object of the same name.

**Fix.** Create the memory in the shell, or pass a `memfd` sealed against
shrinking, as the renderer framebuffer does. Use `checked_mul`. Treat
`ERROR_ALREADY_EXISTS` as failure.

### S-16 — WebGL object arguments ignore object kind and owning context

**Low.** Confirmed.

- `rust/webgl/src/args.rs:68–70`

```rust
pub(crate) fn name(args: &[Value], i: usize) -> u32 {
    args.get(i).and_then(host::object_of).map_or(0, |(_, n)| n)
}
```

Any WebGL object of any kind from any context is accepted where a name is
expected, so `bindTexture(TEXTURE_2D, someBuffer)` binds whatever texture has
that number — including the context's internal drawing-buffer objects, which
hold the lowest names. No memory-unsafe consequence was found, because the
pixel-transfer wrappers re-check sizes against live GL state, but it
undermines the host's invariants and compounds S-05.

**Status: fixed.** Every WebGL object records the context that made it, and
each argument site names the kind it expects (`name(a, i, KIND_…)`). An object
of another kind or another context is treated as null and `getError` reports
`INVALID_OPERATION`; the same check covers uniform locations and sync objects.
The context's internal objects have no wrappers, so a page cannot name them.

**Fix.** `name()` takes the expected kind and the calling context and raises
`INVALID_OPERATION` on a mismatch; keep internal names out of the page's
reach.

### S-17 — Shader source reaches the driver compiler unvalidated

**Low** (attack surface). Confirmed.

- `rust/webgl/src/methods.rs:442–453`, `rust/webgl/src/ffi/gl.rs:717`

Only a 4 MB cap and a version prefix are applied before `glShaderSource`.
None of WebGL's GLSL ES restrictions are enforced, so driver compilers —
C and C++ with a long crash history — parse arbitrary page input inside the
renderer.

**Fix.** A small in-tree ESSL checker that enforces the WebGL limits
(identifier length, nesting depth, reserved names, directives) before
compiling; prefer ES contexts with robustness.

### S-18 — WebGL permission is allow-by-default; SECURITY.md says otherwise

**Low.** Confirmed.

- `rust/webgl/src/permission.rs:46–62`

For an origin with no recorded decision, `permitted()` records `allowed =
true`, marks it pending and returns `true`: the context exists before the
user answers. SECURITY.md describes WebGL as "off by default and gated
behind a per-site trust prompt"; CLAUDE.md says enabled by default. S-04,
S-05, S-16 and S-17 all sit behind this gate.

**Fix.** Either return `false` while a decision is pending, or correct
SECURITY.md.

### S-19 — WebAssembly has no execution budget

**Low.** Confirmed.

- `rust/js-wasm/src/realm.rs:56–78`, `rust/js-wasm/src/objects.rs:461–469,
  635–656`

The wasmi engine is built with memory and table limits only. An infinite
wasm loop cannot be stopped by the long-running-script monitor or by closing
a worker, unlike JavaScript. Separately, every exported memory is answered
with the imported or first own memory; with multi-memory a second export
would alias the first, which the single-view detach in `memory_grow` relies
on not happening (whether wasmi's defaults enable multi-memory was not
checked).

**Fix.** Enable fuel or epoch interruption and drive it from the same
interrupt callback as scripts; map views by `Memory` identity.

### S-20 — Box tree holds raw style pointers kept valid only by call order

**Low** (no violation found). Confirmed.

- `rust/style/src/ffi.rs:527–542`, `rust/layout/src/boxes.rs:138, 300, 436`,
  `rust/css/src/ffi/style_alloc.rs:17–28`

Boxes store `*const Style` into allocations owned by the `StyleMap`, and a
safe function dereferences them with an unbounded lifetime. The orderings
that keep this sound (`rust/browser/src/page.rs:157–162`,
`rust/browser/src/ffi.rs:360–361`) hold today, but `Browser::drop_styles()`
and `compute_cascade()` are public and replace the map without touching the
layout.

**Fix.** Boxes hold a `RetainedStyle`, as SVG already does, or the layout
root holds the `Rc<StyleMap>`.

### S-21 — Smaller notes

- **Embedder policy** (`rust/http/src/policy.rs:97–110`): `v6_is_public`
  does not treat NAT64 `64:ff9b::/96`, 6to4 `2002::/16` or Teredo
  `2001::/32` as carrying an IPv4 address, and resolved-address checks are
  skipped behind a proxy.
- **HSTS** (`rust/netstore/src/hsts.rs`): the list is global and uncapped —
  the HSTS supercookie, and unbounded growth.
- **TLS**: no revocation checking. `tls_allow_insecure_override`, once on,
  accepts bad certificates on every non-HSTS navigation without asking per
  host (cookies are withheld and subresources never use it).
- **`X-ND-*` control lines** share the header list with author headers and
  are correctly forbidden in `rust/js-net/src/headers.rs:47`; any future
  header source that skips that check could forge `X-ND-Navigate`. A typed
  side structure would remove the class.
- **`about:` privilege** (`rust/net/src/about.rs:115–117`): a request with
  no `top_url` counts as chrome, so `about:settings-save` is protected only
  by the renderer-side `allows_navigation_url`. A compromised renderer can
  rewrite proxy and TLS settings. Consider an unforgeable chrome token.
- **`Scope::object_from_identity`** (`rust/js-engine/src/boa/misc.rs:30–41`)
  is a safe function that dereferences a raw pointer; callers looked sound.
  Make it `unsafe fn` or move to weak GC handles as `docs/boa-fork.md`
  plans.
- **Immutable ArrayBuffers**: `with_buffer_bytes_mut`
  (`rust/js-engine/src/boa.rs:989–1007`) does not consult the `immutable`
  flag, so `getRandomValues`, `readPixels` and similar can write to a buffer
  from `transferToImmutable()`. Not memory-unsafe.
- **`WeakNode` generation** is a wrapping `u32`; about 2³¹ reuses of one
  slot revalidate a stale handle. Use `u64`.
- **libav and libavif struct offsets** are probed at build time and applied
  to whatever library loads at run time. Check the major version at run time
  before the first field access.
- **Overflow checks** are off in release builds and on in CI. Shipping with
  them on would turn wraps like the one in S-15 into aborts.

## Done well

- TLS is rustls only; unverified connections are never pooled or resumed and
  carry no cookies.
- HTTP/1.1 connections are never reused, which removes response
  desynchronisation and request smuggling; header and line sizes are bounded.
- FTP ignores the PASV-supplied address and rejects CR, LF and NUL in every
  command argument.
- Redirects: http(s) targets only, no downgrade for subresources, sensitive
  headers stripped cross-origin, preflight repeated on a cross-origin
  redirect.
- Cookies: control bytes rejected (no jar-line injection), `__Host-` and
  `__Secure-`, PSL check on `Domain`, `SameSite=Lax` by default, jars
  partitioned by top-level site, script cannot overwrite an HttpOnly cookie.
- `same_origin` is true only for http(s) tuples; `pushState` is
  origin-checked; `javascript:` is not on the `location` allow-list.
- Internal `data-nd-*` attributes are filtered in the parsers,
  `setAttribute`, `NamedNodeMap`, `dataset` and cloning.
- The shell validates renderer media commands: one line, length-capped,
  `file://` only for canonicalised stream files with a strict name grammar.
- Remote control: loopback only, 256-bit random token, constant-time
  comparison, a 0600 control file outside the renderer's Landlock set.
- `unsafe` is concentrated: the parsers and decoders (`html5`, `xml`,
  `woff2`, `raster`, `plmpeg`, `minimp3`, `webvtt`, `svg`, `video`,
  `js-clone`, `js-workers`, `js-crypto`) contain none.
- XML has no DTD or entity-definition processing: no XXE, no entity
  expansion. Depth caps are pervasive; image decoders bound dimensions,
  frames and bytes and allocate with `try_reserve`.
- WebGL pixel transfers recompute the required span from live pack/unpack
  state with checked arithmetic before every pointer-taking call.
- WebAssembly memory buffers are detached before script can run after a
  grow; WebGPU mapped ranges are detached before unmap.
- WebCrypto validates key, IV, tag and counter lengths, cleanses key
  material on drop and draws from `RAND_bytes`.
- `panic = "abort"`: nothing unwinds across FFI callbacks.

## Coverage

**Read and traced**

- `rust/http` (`client.rs`, `policy.rs`, `ftp.rs`, `transfer.rs`, part of
  `tls.rs`), `rust/http-wire/src/h1.rs`
- `rust/net` (requests, hop, cookies, credentials, HSTS, `file:`, `about:`,
  listings, routes, error and view-source pages), `rust/netstore`
- `rust/js-net` (headers, preflight, fetch, WebSocket constructor),
  `rust/js-frames` (bootstrap, origin, sandbox, realm, scripts), `rust/js-window`
  (location, history, part of navigation and messages)
- `rust/browser` (navigation callbacks, open, forms, meta refresh),
  `rust/renderer-host` open and tick handlers, `rust/shell` media validation,
  downloads and remote-control transport, `rust/sandbox` seccomp rules,
  `rust/websocket` handshake
- `rust/dom`, `rust/js-engine` unsafe sites and buffer access, the Boa
  fork's ArrayBuffer patch, `rust/js-wasm`, `rust/js-node`, `rust/js-mutation`
- `rust/webgl`, `rust/canvas` image data, WebGPU buffer mapping
- `rust/image-decoders` (libavif), `rust/image`, `rust/raster`,
  `rust/texture`, `rust/av`, `rust/video-helper`, `rust/media-shm`,
  `rust/plmpeg` motion compensation
- `rust/webcrypto` AES paths, `rust/js-crypto`, `rust/xml`, `rust/woff2`
- style and box-tree pointer lifetimes across `rust/style`, `rust/css`,
  `rust/browser`

**Not read, or only skimmed**

- HTTP/2 framing, HPACK and flow-control limits; `proxy.rs`, `doh.rs`,
  `x25519_mlkem768.rs`, the content decoders
- `rust/url` parser internals, IDNA and homograph handling, URL-bar display
- `rust/csp` beyond `frame-ancestors`, `rust/safebrowsing`, `rust/mime`,
  `rust/eventsource`, `rust/http-cache`, `rust/psl`
- `rust/js-storage`, `rust/idb`, `rust/js-workers`, `rust/js-services`,
  `window.open`, `postMessage` delivery, per-realm checks in DOM natives
- `rust/ipc`, `rust/renderer-client`, the remaining renderer routes
  (`/policy`, screenshot, PDF)
- `rust/mcp` tools, `rust/extensions`, `rust/capi`, `rust/bookmarks`,
  `rust/history`, `rust/config`, `rust/sqlite`, `rust/launcher`, `java/`
- the media helpers' command parsers; Landlock rule construction; the macOS
  and Windows sandboxes
- `rust/text/src/ffi` (HarfBuzz, FreeType, fontconfig and cairo: the largest
  block of `unsafe` in the tree), `rust/font`, `rust/pdf`,
  `rust/media-foundation`, most of `rust/cairo` and `rust/glctx`
- `rust/audio-helper`, `rust/webaudio`, `rust/minimp3` internals, MSE logic,
  `rust/webvtt`, SVG filters and `<use>`
- `rust/html5`, `rust/css` and the layout crates for algorithmic-complexity
  denial of service
- the bulk of the Boa fork's patches and `third_party/regress`
- dynamic confirmation of every finding, S-01, S-04 and S-05 first
