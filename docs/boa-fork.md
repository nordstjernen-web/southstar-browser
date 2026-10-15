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
2026-10-15 the difference is 10,340 added and 1,105 removed lines in 123 files;
148 of the removed lines are the `Cargo.toml` trims below.

## What the fork adds

| Change | Files | Used by `rust/js-engine` for | Upstream? |
| --- | --- | --- | --- |
| `Context::set_interrupt_handler`, polled every 1024 loop back-edges and calls, stops a script with the uncatchable `RuntimeLimitError::Interrupted` | `vm/mod.rs`, `context/mod.rs`, `error/mod.rs`, `vm/opcode/iteration/loop_ops.rs` | halting a realm, a closing worker, the eval budget and the long-running script monitor (`rust/js-runtime/src/interrupt.rs`) | yes: a general embedder need |
| `Context::frame_depth`, `Context::frame_source_path` | `context/mod.rs` | telling host access from page script (`boa/access.rs`): engine state on events and other host objects is visible only to host code | yes |
| `Context::native_constructor_receiver`: the `this` value a native constructor received when it was called without `new`, which Boa replaces with `undefined` | `vm/mod.rs`, `native_function/mod.rs`, `context/mod.rs` | `Scope::constructor_receiver`: `Intl.NumberFormat.call(object)` attaches the new format to `object` (ECMA-402 ChainNumberFormat) | yes |
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
| Symbols as weak keys: `WeakMap`, `WeakSet`, `WeakRef` and `FinalizationRegistry` accept any value that `CanBeHeldWeakly`, so a symbol outside the global registry too; object keys stay ephemerons, and symbol keys are held strongly, since Boa never collects symbols | `builtins/weak/{mod,table,weak_ref}.rs`, `builtins/weak_map/mod.rs`, `builtins/weak_set/mod.rs`, `builtins/finalization_registry/mod.rs`, `builtins/symbol/mod.rs` | `built-ins/WeakMap`, `built-ins/WeakSet`, `built-ins/WeakRef`, `built-ins/FinalizationRegistry` |
| Calling an `[[IsHTMLDDA]]` object returns `null`, as test262's `$262.IsHTMLDDA` does | `builtins/is_html_dda.rs` | `annexB/language` |
| Object environment records follow the spec's lookups: `GetBindingValue` checks `HasProperty` before `Get` (a binding deleted meanwhile reads as `undefined`, or throws a `ReferenceError` in strict code), and a call through a `with` resolves its callee once, taking both the function and its `this` from that one resolution | `environments/runtime/mod.rs`, `vm/opcode/environment/mod.rs`, `vm/opcode/mod.rs`, `vm/code_block.rs`, `bytecompiler/mod.rs` | `language/statements/with` |
| Proper tail calls: a `return` in strict, non-async, non-generator code outside `try` blocks, `for-in`/`for-of` loops and dispose scopes compiles the calls in its tail positions (through `?:`, `&&`, `\|\|`, `??`, `,` and parentheses, tagged templates included) to `TailCall`/`TailCallEval`, which discard the caller's frame before calling a bytecode function; a frame entered from native code or constructing an object keeps a normal call | `bytecompiler/tail_call.rs` (new), `bytecompiler/{mod,statement/mod,expression/mod}.rs`, `vm/opcode/call/mod.rs`, `vm/opcode/mod.rs`, `vm/mod.rs`, `vm/code_block.rs`, `vm/flowgraph/mod.rs` | `language/expressions/*/tco-*`, `language/statements/*/tco-*` |
| A global `let`, `const` or `class` declared by a later script is visible to code compiled earlier: each code block records how many global lexical bindings existed when it was compiled, and a name that compiled to the global object checks only the bindings added since (a single comparison when there are none); assigning to such a `const` throws | `boa_ast`: `scope.rs`; `boa_engine`: `vm/code_block.rs`, `bytecompiler/mod.rs`, `environments/runtime/mod.rs` | `language/global-code`, `annexB/language/eval-code` |
| `%TypedArray%.prototype.filter` and `with` propagate the `TypeError` that writing `undefined` into a BigInt array throws once the callback or an argument's `valueOf` detached or shrank the buffer, instead of failing an internal assertion | `builtins/typed_array/builtin.rs` | `built-ins/TypedArray` |
| `Error.prototype.stack` as the error-stack accessor proposal now specifies: the getter throws a `TypeError` for a non-object `this`; the setter (length 1) requires a string and stores it through `SetterThatIgnoresPrototypeProperties` | `builtins/error/mod.rs`, `builtins/iterable/iterator_prototype.rs` | `built-ins/Error/prototype/stack` |
| `SimpleModuleLoader` loads `with { type: "text" }` (the file decoded as UTF-8) and `with { type: "bytes" }` (a `Uint8Array` over an immutable `ArrayBuffer`) as synthetic default-export modules, from any file, `.json` included | `module/loader/mod.rs`, `builtins/array_buffer/mod.rs` | `language/import/import-attributes`, `language/import/import-bytes`, `language/expressions/dynamic-import` |
| Deferred module evaluation (`import defer * as ns from "m"`, `import.defer("m")`): requests keep their phase and evaluation order, a deferred import evaluates only the module's asynchronous transitive dependencies, its namespace is a `[[Deferred]]` namespace (`@@toStringTag` "Deferred Module") whose string-keyed operations other than `then` run `EvaluateSync` after `ReadyForSyncExecution` (with `IsModuleSCCEvaluated`), and `import.defer()` waits for the async dependencies with internal reactions; `Module::load_link_evaluate` no longer resolves through an observable `then` | `boa_parser`: `statement/declaration/import.rs`; `boa_ast`: `declaration/import.rs`, `module_item_list/mod.rs`; `boa_engine`: `module/{mod,source,namespace}.rs`, `vm/opcode/call/mod.rs`, `builtins/promise/mod.rs`, `object/builtins/jspromise.rs` | `language/import/import-defer`, `language/expressions/dynamic-import/import-defer` |
| Annex B legacy `RegExp` static properties (`RegExp.$1`–`$9`, `input`/`$_`, `lastMatch`/`$&`, `lastParen`/`$+`, `leftContext`/`` $` ``, `rightContext`/`$'`): each realm keeps the statics of its last successful match, accessors throw a `TypeError` for a `this` other than that realm's `%RegExp%` and after a match by a subclass instance, and `RegExp.prototype.compile` refuses subclass instances and regular expressions from other realms; `Function.prototype.toString` writes a built-in whose name is not an identifier as a computed name | `boa_engine`: `realm.rs`, `builtins/regexp/mod.rs`, `builtins/function/mod.rs` | `annexB/built-ins/RegExp/legacy-accessors`, `annexB/built-ins/RegExp/prototype/compile` |
| Annex B runtime errors for function call assignment targets: in non-strict code `f() = 1`, `f() op= 1`, `f()++`, `++f()` and `for (f() in/of …)` parse (`AssignTarget::Call`, `UpdateTarget::Call`, `IterableLoopInitializer::Call`) and throw a `ReferenceError` after evaluating the call, while `f() &&= 1` and friends stay early errors; `async(…)` is a `CoverCallExpressionAndAsyncArrowHead`: parsed as a call and refined into async arrow parameters only when `=>` follows (the cursor records argument lists with a trailing comma; `ContainsSymbol::AwaitIdentifier` rejects `await` bindings and references, including nested arrow parameters) | `boa_ast`: `expression/operator/{assign,update}/mod.rs`, `statement/iteration/mod.rs`, `expression/literal/{array,object}.rs`, `operations/mod.rs`; `boa_parser`: `parser/cursor/mod.rs`, `expression/assignment/{mod,async_arrow_function}.rs`, `expression/left_hand_side/arguments.rs`, `expression/{primary/mod,update}.rs`, `statement/iteration/for_statement.rs`; `boa_engine`: `bytecompiler/{mod,expression/assign,expression/update,statement/loop}.rs` | `annexB/language/expressions/assignmenttargettype`, `language/expressions/async-arrow-function`, `language/expressions/logical-assignment` |
| Temporal over `temporal_rs`: `CanonicalizeCalendar` rejects the calendar ID `islamic` with a `RangeError` (only the `Intl.DateTimeFormat` constructor falls back for it), both in a calendar string or property and in the constructors' calendar argument; `PlainMonthDay.from` reads every month-day field through `PrepareCalendarFields`, `era` and `eraYear` included, and `PlainMonthDay.prototype.toPlainDate` reads `era` and `eraYear` beside `year`; `ZonedDateTime.prototype.with` defaults its `offset` option to `prefer`; `PlainYearMonth.prototype.add`/`subtract` read the duration before the options object | `builtins/temporal/{calendar,plain_date,plain_date_time,plain_month_day,plain_year_month,zoneddatetime}/mod.rs` | `intl402/Temporal`, `built-ins/Temporal` |
| `ShadowRealm` (Stage 2.7, behind the `experimental` feature the engine is built with): the constructor creates a realm with an ordinary global object and the default global bindings; `evaluate` runs the source as an indirect eval in that realm (`Eval::evaluate_parsed`, the half of `perform_eval` after parsing), throwing the caller realm's `SyntaxError` for invalid source and its `TypeError` for anything the code throws; `importValue` loads the module into the shadow realm through the module loader, with relative specifiers resolved against the calling script or module; wrapped function exotic objects copy `length` and `name`, wrap arguments, `this` and results across the boundary, turn exceptions into `TypeError`s and report their `[[Realm]]` to `GetFunctionRealm`. `Function.prototype.toString` gives every callable without source text the native-code form | `builtins/shadow_realm/mod.rs` (new), `builtins/mod.rs`, `builtins/eval/mod.rs`, `builtins/function/mod.rs`, `context/intrinsics.rs`, `object/operations.rs` | `built-ins/ShadowRealm` |
| The legacy `caller` and `arguments` properties of non-strict functions (an extension the forbidden-extensions clause allows): ordinary non-strict functions are created from templates with own non-configurable `arguments` and `caller` accessors whose shared getters find the function's most recent call frame; `caller` answers the calling non-strict function or `null` (never a strict function), `arguments` a fresh arguments object of that call; `Function.prototype.caller` and `.arguments` stay `%ThrowTypeError%` | `boa_engine`: `context/intrinsics.rs`, `vm/code_block.rs`, `builtins/function/mod.rs` | `built-ins/Function` (`15.3.5.4_2-*gs`), `language/arguments-object/10.6-13-a-*` |
| Decorators (the 2023-11 proposal, with decorator metadata) and auto-accessors: `@` decorator lists on class declarations and expressions (before or after `export`) and on methods, getters, setters, fields and `accessor` fields, static or not, private or not; decorators are called with the `this` of their member expression and a context object (`kind`, `name`, `static`, `private`, `access`, `addInitializer`, `metadata`), replace methods, accessors and classes, add field initializers and extra initializers, and the shared metadata object becomes the class's `Symbol.metadata`. A class with decorators or auto-accessors defines its elements through four opcodes (`CreateClassDecorations`, `AddClassElement`, `DecorateClass`, `InitializeStaticElements`) and its fields as `ClassFieldDefinition::Decorated`; other classes compile as before | `boa_ast`: `function/class.rs`, `operations/mod.rs`, `scope_analyzer.rs`, `punctuator/mod.rs`; `boa_parser`: `lexer/mod.rs`, `parser/expression/{decorator,mod}.rs`, `parser/expression/{assignment/yield,left_hand_side/mod,primary/mod,primary/class_expression/mod}.rs`, `parser/statement/{mod,expression/mod,declaration/mod,declaration/export}.rs`, `parser/statement/declaration/hoistable/{mod,class_decl/mod}.rs`; `boa_engine`: `vm/opcode/decorator/mod.rs` (new), `bytecompiler/class.rs`, `builtins/function/mod.rs`, `object/{mod,operations}.rs`, `symbol.rs`, `builtins/symbol/mod.rs`, `vm/opcode/{mod,args}.rs`, `vm/code_block.rs`, `vm/flowgraph/mod.rs` | `language/statements/class/decorator`, `language/expressions/class/decorator`, `language/statements/class/elements`, `language/expressions/class/elements`, `staging/decorators` |
| `InitializeInstanceElements` no longer keeps the class constructor borrowed while the field initializers run, so an initializer that writes to its own class (`id = ++Counter.count`) no longer aborts with a `BorrowMutError` | `object/operations.rs` | `language/statements/class/elements`, `language/expressions/class/elements` |
| `Number.prototype.toExponential` rounds exact ties up (the larger `n`) from the value's exact decimal expansion instead of Rust's ties-to-even; `Date.parse` returns `NaN` for a string with a signed six-digit year that the Date Time String Format parser rejected (`-000000`, out-of-range time values) instead of handing it to the legacy parser; `Promise.try` calls the callback first and returns `PromiseResolve(C, result)`, so a promise of the same constructor comes back unwrapped (the current specification) | `boa_engine`: `builtins/number/mod.rs`, `builtins/date/utils.rs`, `builtins/promise/mod.rs` | `built-ins/Number/prototype/toExponential`, `built-ins/Date`, `built-ins/Promise/try` |
| Parser: an object literal ending in a spread and a comma (`{...rest,}`) records it (`ObjectLiteral::has_trailing_comma_spread`) and is not refined into an assignment pattern; the start of a Script counts as the start of a line for Annex B `HTMLCloseComment` (`-->` on the first line, after whitespace and single-line block comments), while whitespace before `#!` still rules out a hashbang; `export default` of an anonymous function or class names it `default` at parse time (NamedEvaluation), so a class's static `name` member wins | `boa_ast`: `expression/literal/object.rs`; `boa_parser`: `expression/primary/object_initializer/mod.rs`, `lexer/mod.rs`, `parser/cursor/buffered_lexer/mod.rs`, `statement/declaration/export.rs` | `language/expressions/assignment/dstr`, `language/statements/for-{in,of}/dstr`, `annexB/language/comments`, `language/comments/hashbang`, `language/module-code`, `language/expressions/dynamic-import` |

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
