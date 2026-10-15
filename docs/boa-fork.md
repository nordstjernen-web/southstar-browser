# The Boa fork

Southstar's JavaScript engine is [Boa](https://github.com/boa-dev/boa).
Three of its crates are forked into `third_party/`, each a copy of the
**0.22.0** release from crates.io patched into the build through
`[patch.crates-io]` in `Cargo.toml`:

| Crate | crates.io sha256 |
| --- | --- |
| `boa_engine` | `5d6aaf7fdc4299df68adb67e1a5440a51d34630a1f5271b9698fd74bc97b60d9` |
| `boa_parser` | `641e81b3c8cbb7a2140547fb3ea534e9edf210b4e8c920b5fa864a919e11fbcf` |
| `boa_ast` | `9221b0a090572b4a118b46c5874c0781fe4bc9546b85f0a615b8bfb609ca4ed7` |

`boa_engine` carries a small set of embedder hooks; the parser and the AST
were forked for syntax that 0.22 does not parse. `boa_gc`, `boa_interner`,
`boa_string` and the other Boa crates come unchanged from crates.io at the
same version, and `rust/js-engine` is the only crate that depends on Boa.

The aim is a fork whose difference from upstream stays small and readable,
so a Boa release, and above all a security fix, can be taken by re-applying
the patch rather than by merging. Besides the embedder hooks, the fork
carries the ECMAScript features test262 covers that Boa 0.22 lacks or gets
wrong (see "Conformance work" below); each one is a candidate for upstream.

## Seeing the difference

```sh
python3 scripts/boa-fork-diff.py --stat   # per-file summary
python3 scripts/boa-fork-diff.py          # the whole patch
```

The script downloads the releases the forks' `Cargo.toml` files name, checks
them against the crates.io index checksums and diffs `src/` and `Cargo.toml`
of each (`--crate boa_parser` limits it to one). On
2026-10-15 the difference is 4,821 added and 385 removed lines in 64 files;
148 of the removed lines are the `Cargo.toml` trims below.

## What the fork adds

| Change | Files | Used by `rust/js-engine` for | Upstream? |
| --- | --- | --- | --- |
| `Context::set_interrupt_handler`, polled every 1024 loop back-edges and calls, stops a script with the uncatchable `RuntimeLimitError::Interrupted` | `vm/mod.rs`, `context/mod.rs`, `error/mod.rs`, `vm/opcode/iteration/loop_ops.rs` | halting a realm, a closing worker, the eval budget and the long-running script monitor (`rust/js-runtime/src/interrupt.rs`) | yes: a general embedder need |
| `Context::frame_depth`, `Context::frame_source_path` | `context/mod.rs` | telling host access from page script (`boa/access.rs`): engine state on events and other host objects is visible only to host code | yes |
| `FunctionObjectBuilder::host`, `NativeFunctionObject::{is_host, function, name, is_constructor}` | `object/mod.rs`, `native_function/mod.rs`, `builtins/builder.rs`, `object/builtins/jsfunction.rs` | telling the embedder's functions from Boa's built-ins (structured clone, `on<type>` handlers) | yes |
| `HostExoticHooks`, `host_internal_methods`, `ordinary_define_own_property_of`, `set_with_receiver`, the `InternalObjectMethods` re-export | `object/internal_methods/host.rs` (new), `object/internal_methods/mod.rs`, `object/mod.rs` | WebIDL indexed and named properties (collections, `Storage`, `dataset`, `DOMTokenList`, the CSSOM, the window) and setters forwarded to a prototype with the right `this` | yes, as a public custom-internal-methods API; upstream may want a different shape |
| `JsArrayBuffer::from_external_bytes`: an `ArrayBuffer` viewing embedder memory, detachable, never transferable or resizable | `builtins/array_buffer/mod.rs`, `object/builtins/jsarraybuffer.rs` | `WebAssembly.Memory.prototype.buffer` over wasmi's linear memory | yes |
| `Module::import_meta`, creating `import.meta` once through the loader's `init_import_meta` hook | `module/mod.rs` | filling `import.meta.url`/`resolve()` on the object the module sees | yes |
| `JsObject::get_function_realm` made `pub` | `object/operations.rs` | `Scope::function_handle`: the realm a callback is re-entered in | yes, trivially |
| The legacy `Date.parse` fallback: month and weekday names, `/`, `-`, `,` and space separated dates, 12/24-hour times, zone names and offsets; ISO fractions past milliseconds ignored | `builtins/date/utils.rs` | `Date.parse(String(new Date()))` and the other strings every browser accepts | yes: the format is implementation-defined, and every other engine accepts these |
| `JsObject::{identity, from_identity, retain_identity, release_identity}`: a `JsObject` as a raw pointer and back | `object/jsobject.rs` | the internal C ABI's opaque object handles (canvas, js-runtime wrappers) | no: scaffolding of the C port, removed as that ABI goes; the arena DOM and GC-traced bindings replace it |
| `Cargo.toml`: no `cdylib` crate type; the tests, benches, dev-dependencies and lint tables of files the crate does not ship are dropped (in `boa_parser` and `boa_ast` too) | `Cargo.toml` | building as an rlib inside the workspace | no |

## Conformance work

test262 runs through `southstar-jsshell test262 --root <test262 checkout>`
(`rust/jsshell`). The fork adds what it needs to pass:

| Change | Files | test262 directories |
| --- | --- | --- |
| `Iterator.prototype.chunks`/`windows` (iterator chunking), `Iterator.zip`/`zipKeyed` (joint iteration), `Iterator.prototype.join`, `Iterator.prototype[Symbol.dispose]`; `take`/`drop` reject limits above 2^53 - 1, `includes` validates `skippedElements` without coercion, and `return()` on a helper that never started marks it completed before closing what it wraps | `builtins/iterable/iterator_prototype.rs`, `builtins/iterable/iterator_constructor.rs`, `builtins/iterable/iterator_helper/{chunks,zip,mod}.rs` | `built-ins/Iterator` |
| `Uint8Array.fromBase64`/`fromHex` and `Uint8Array.prototype.toBase64`/`toHex`/`setFromBase64`/`setFromHex` (ES2026) | `builtins/typed_array/base64.rs`, `builtins/typed_array/mod.rs` | `built-ins/Uint8Array` |
| Immutable ArrayBuffers: `ArrayBuffer.prototype.immutable`, `transferToImmutable`, `sliceToImmutable`; typed arrays, `DataView`, `Atomics` and the `Uint8Array` setters refuse to write to them, their elements read as non-writable and non-configurable, and they cannot be detached, transferred or resized | `builtins/array_buffer/mod.rs`, `builtins/typed_array/{object,builtin,base64}.rs`, `builtins/dataview/mod.rs`, `builtins/atomics/mod.rs` | `built-ins/ArrayBuffer`, `built-ins/TypedArray`, `built-ins/DataView` |
| Explicit resource management: `using` and `await using` declarations in blocks, function and module bodies and `for`/`for-of` heads (`using` stays an ordinary identifier unless a binding name follows on the same line), disposed at scope exit by a `try`/`finally`-shaped scope and five opcodes (`CreateDisposeCapability`, `AddDisposableResource`, `DisposeResources`, `AsyncDisposeStep`, `AddDisposeError`); `DisposableStack`, `AsyncDisposableStack`, `SuppressedError` and `%AsyncIteratorPrototype%[Symbol.asyncDispose]` | `boa_parser`: `statement/{mod,declaration/*,iteration/for_statement,switch/mod}.rs`, `expression/**`, `parser/mod.rs`; `boa_ast`: `statement/iteration/mod.rs`, `scope_analyzer.rs`, `operations/mod.rs`, `declaration/variable.rs`; `boa_engine`: `builtins/disposable_stack/mod.rs`, `builtins/error/suppressed.rs`, `bytecompiler/statement/{using,loop}.rs`, `bytecompiler/{mod,module}.rs`, `vm/opcode/dispose/mod.rs` | `language/statements/using`, `language/statements/await-using`, `built-ins/DisposableStack`, `built-ins/AsyncDisposableStack`, `built-ins/SuppressedError` |

## Rules for changing the fork

- A hook goes into the fork only when `rust/js-engine` cannot be written
  against Boa's public API. Prefer the narrowest addition: a getter, a
  `pub` on an existing function, an extra builder flag.
- Add each new hook to the table above with the crate feature it serves.
  `scripts/boa-fork-diff.py --stat` must account for every file it lists.
- The fork keeps upstream's style and comments (third-party code is exempt
  from Southstar's comment rules) so its hunks re-apply cleanly.
- Nothing outside `rust/js-engine` names `boa_engine`; the rest of the tree
  uses the `rust/js-engine` API (`Scope`, `Value`, `HostClass`).

## Taking an upstream release

1. `python3 scripts/boa-fork-diff.py > boa.patch` on the current fork.
2. Unpack each new release (`boa_engine`, `boa_parser`, `boa_ast`) over its
   directory in `third_party/` (`src/`, `Cargo.toml`, `ABOUT.md`) and apply
   `boa.patch` with `patch -p1 -d third_party`, fixing hunks that no longer
   apply. Drop the rows of the tables that upstream has since taken.
3. Bump the other `boa_*` crates in `Cargo.lock` to the same version
   (`cargo update -p boa_gc -p boa_interner …`), update the versions and
   checksums at the top of this page, and run `cargo vet` for the new
   crate versions.
4. Run `scripts/rust-tests.sh`, test262 through `southstar-jsshell`
   (`rust/jsshell`) and a browser smoke test before committing.

A security fix released only on Boa's `main` branch is applied the same
way, as a patch onto the fork, and named in the commit message.

## Upstreaming

Every row marked "yes" is a candidate for a pull request to
[boa-dev/boa](https://github.com/boa-dev/boa). Each one that lands upstream
shrinks this table at the next release; the goal is a fork that differs from
upstream only in `Cargo.toml`, and then no fork at all. Only the
`JsObject` identity functions stay local, and they leave with the C ABI
they serve.
