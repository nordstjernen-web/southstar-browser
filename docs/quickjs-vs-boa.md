# QuickJS vs Boa

Measured 2026-11-15: Bellard's QuickJS (release 2026-06-04) against
Southstar's Boa fork, on performance, security and standards compliance.

**Recommendation: keep Boa.** QuickJS is about 5.4× faster and roughly tied on
ECMAScript conformance, but it is unsafe C with a record of exploitable memory
bugs. Adopting it would also mean rebuilding the backend Southstar deleted in
October 2026.

| | QuickJS (Bellard 2026-06-04) | Boa (Southstar fork) | Edge |
| --- | --- | --- | --- |
| Speed, Octane geometric mean | 2,914 | 543 | QuickJS, 5.4× |
| Peak memory, heavy benchmarks | 57–113 MB | 207–368 MB | QuickJS, 2–6× |
| test262 pass, all 53.8k files | 98.6% | 99.88% | Boa, narrowly |
| test262 pass, on tests run | 99.96% | 99.88% | Tie |
| Memory safety | C; 17 CVEs incl. 3 use-after-free | Rust; 1 advisory (DoS) | Boa |
| HTML host hooks in Southstar | Would need rewriting | Done, 77,820 WPT subtests passing | Boa |
| Cost to adopt | New FFI backend and C fork | None | Boa |

Speed is QuickJS's one real advantage, and on pages it matters less than Octane
suggests: layout, style, networking and DOM calls run in Rust either way. The
renderer's syscall filter is a no-op on macOS and Windows, so the engine's own
memory safety carries more weight there.

What would change this: Speedometer showing script time dominates page load,
or a hardened QuickJS (fuzzed, CVE-free for a year) with Southstar's hooks
upstreamed. Until then the better investment is Boa's speed and memory (GC
thresholds, shape and inline caches) and a fork resync for the 48 new
class-field tests below.

## Where Southstar stands

- Boa is forked in `third_party/boa_*`, tracking Boa `main` at `bea404d1`
  (2026-11-10); the fork adds 15,243 and removes 3,939 lines in 184 files
  (`docs/boa-fork.md`).
- All bindings go through `rust/js-engine`; only that crate depends on Boa.
- Bellard's QuickJS was dropped as a backend in early October 2026, and
  QuickJS-ng was deleted on 13 October 2026 (`docs/rust-port.md`). A QuickJS
  backend today needs new FFI, a `js-engine` backend module and the embedder
  hooks the old QuickJS-ng fork carried (receiver-aware property hooks for
  `WindowProxy` and legacy platform objects, interrupts, memory caps).

## Performance

Octane, one run per benchmark, each in its own process. Higher is better.

| Benchmark | QuickJS | Boa | QuickJS / Boa |
| --- | ---: | ---: | ---: |
| Splay | 5,055 | 694 | 7.3× |
| Mandreel | 2,999 | 475 | 6.3× |
| PdfJS | 5,991 | 979 | 6.1× |
| RayTrace | 1,960 | 333 | 5.9× |
| Gameboy | 8,120 | 1,380 | 5.9× |
| RegExp | 379 | 65.5 | 5.8× |
| EarleyBoyer | 2,396 | 433 | 5.5× |
| DeltaBlue | 1,038 | 195 | 5.3× |
| zlib | 2,254 | 440 | 5.1× |
| NavierStokes | 2,711 | 535 | 5.1× |
| Crypto | 1,236 | 249 | 5.0× |
| Richards | 1,102 | 231 | 4.8× |
| TypeScript | 12,348 | 2,695 | 4.6× |
| CodeLoad | 16,835 | 3,817 | 4.4× |
| Box2D | 3,944 | 931 | 4.2× |
| **Geometric mean** | **2,914** | **543** | **5.4×** |

| Measure | QuickJS | Boa | Ratio |
| --- | ---: | ---: | ---: |
| Start-up, empty script (median of 30, ms) | 2.1 | 4.1 | 2.0× |
| Peak RSS, Splay (MB) | 113 | 368 | 3.3× |
| Peak RSS, Mandreel (MB) | 57 | 355 | 6.2× |
| Peak RSS, TypeScript (MB) | 94 | 207 | 2.2× |
| Peak RSS, zlib (MB) | under 13 | 163 | >12× |
| Shell binary, text segment (MB) | 2.6 | 11.6 | 4.5× |

The gap has widened since October, when `docs/js-engines.md` measured 3–4×
against QuickJS-ng: Bellard's 2026-06-04 release claims 30% and 11% speed-ups
on bench-v8. Neither engine has a JIT. Boa's zlib run took 347 s of wall time.

## Security

| | QuickJS (Bellard) | Boa (Southstar fork) |
| --- | --- | --- |
| Language | C, about 74,000 lines in the core (`quickjs.c`, regexp, unicode, dtoa); 173,000 with the new Intl and Temporal code | Rust, about 232,000 lines in six crates plus 45,000 in `regress` |
| Published advisories | 17 CVEs, 2020–2025 | 1 RustSec advisory, RUSTSEC-2024-0444 |
| Bug classes | 3 use-after-free, 5 out-of-bounds writes or heap overflows, 1 type confusion, 2 out-of-bounds reads, plus assertion and stack-overflow DoS | A panic in `AsyncGenerator`; DoS only, fixed in 0.19 |
| Related code | quickjs-ng had more in 2026: typed-array heap overflows and a use-after-free when a resizable `ArrayBuffer` shrinks during `valueOf()` (CVE-2026-1144) | — |
| Where `unsafe` lives | The whole engine | 183 `unsafe` uses in `boa_gc` (4,400 lines), 114 in `boa_string`, 301 in the 166,000-line `boa_engine`; 2 in the parser, 0 in the AST |
| Residual risk | Any refcount or bounds slip is exploitable | A wrong `Trace` impl or GC root can still cause use-after-free; panics abort the renderer |

- The renderer runs under seccomp on Linux; on macOS and Windows the syscall
  filter is a no-op, so the engine's memory safety is most of the defence.
- Neither engine has a JIT, so the JIT-bug class behind most V8 and
  SpiderMonkey advisories applies to neither.
- The 2026-06-04 QuickJS release adds about 100,000 lines of new C for Intl
  and Temporal with no public fuzzing history yet.
- `docs/unsafe-policy.md` bans `unsafe` in parsers and engine logic. Boa meets
  that apart from its GC and string internals; QuickJS would be a
  74,000-line exception.

## Standards compliance

Both engines ran the same test262 checkout (tc39/test262 `2e58f87d`,
2026-11-11), staging included, each test file counted once.

| test262 | QuickJS 2026-06-04 | Boa (Southstar fork) |
| --- | ---: | ---: |
| Test files | 53,792 | 53,795 |
| Passed | 53,045 (98.6%) | 53,729 (99.88%) |
| Failed | 20 | 64 |
| Timed out | 0 | 2 |
| Not run (feature skipped or excluded) | 727 | 0 |
| Pass rate on tests run | 99.96% | 99.88% |
| intl402 | all run passed | 3,394 / 3,394 |
| Wall time, 4 workers | 31 s | 215 s |

- QuickJS's `test262.conf` skips decorators, `ShadowRealm`, `import defer`,
  source-phase imports, canonical time zones, legacy RegExp statics and tail
  calls (680 tests), and excludes 47 more, some deliberately (strict-mode
  assignment order, `Function.prototype.caller`).
- QuickJS fails 16 SpiderMonkey staging tests, 3 explicit-resource-management
  module tests and 1 Temporal `Duration.from` test.
- Boa fails 48 class-field tests added to test262 after the fork's last sync
  (an escaped `async` before a field, with automatic semicolon insertion), 6
  `DataView` tests on detached or resized buffers, 7 staging RegExp tests from
  `regress` and a few other staging tests.
- QuickJS-ng passed 0.7% of intl402 in October; Bellard's 2026-06-04 release
  ships its own Intl and Temporal and passes all of it.

HTML-facing host hooks:

| HTML need | QuickJS | Boa fork |
| --- | --- | --- |
| `document.all` ([[IsHTMLDDA]]) | `JS_SetIsHTMLDDA` | Supported; all 42 `IsHTMLDDA` test262 tests pass |
| `WindowProxy`, legacy platform objects | `JSClassExoticMethods`; QuickJS-ng needed a receiver-aware hook added | `HostExoticHooks`, `set_with_receiver` |
| A realm per frame, incumbent and entry realm | Contexts share a runtime; no caller-realm query | `native_caller_realm`, `script_realm`, weak realms for gone frames |
| Promise jobs, rejection tracking, module loading | `JS_EnqueueJob`, `JS_SetHostPromiseRejectionTracker`, `JS_SetModuleLoaderFunc2` | Pluggable job queue and module loader, `import.meta` hook |
| Long-running script watchdog | `JS_SetInterruptHandler` | `set_interrupt_handler`, including inside regex backtracking |
| Source decoding | Bytes as given | WHATWG UTF-8 decoding |

`docs/wpt-scores.md` shows 77,820 of 81,855 tracked WPT subtests passing on
Boa; no QuickJS number on the current bindings exists.

## Integration, maintenance and licensing

| | QuickJS (Bellard) | Boa (fork) |
| --- | --- | --- |
| State in Southstar | Removed in October 2026 | Shipping |
| Work to adopt | C build, hand-written FFI, a `js-engine` backend, the hooks in a C fork, an `Unsafe-Budget-Increase` | None |
| Embedder hooks upstream | Strong: interrupt handler, memory limit, exotic-object methods | Weak; the fork adds them |
| Fork upkeep | A C fork outside clippy and the unsafe budget | 15,243 / 3,939 lines against Boa `main`, re-applied per update |
| Release cadence | Single maintainer; releases 2025-04, 2025-09, 2026-06 | A team; the fork tracks `main` |
| Licence | MIT | MIT / Unlicense |
| Toolchain | Any C compiler | Rust 1.95, Boa `main`'s floor |

Both licences are compatible with NSL-1.0 OR GPL-3.0-or-later, and Bellard's
QuickJS is not upstream browser engine code, so `deny.toml` would not block it.

## Method

All runs were on one Linux x86-64 container with 4 vCPUs, one engine at a
time.

- QuickJS: github.com/bellard/quickjs master `e585640` (2026-11-10, version
  2026-06-04), built with `make` (gcc `-O2`); test262 through
  `run-test262 -a -T 4 -c test262.conf` with `tests/test262.patch` applied.
- Boa: `cargo build --release -p southstar-jsshell` from this tree;
  test262 through `southstar-jsshell test262 --jobs 4 --staging`.
- Octane: github.com/chromium/octane master; each benchmark concatenated with
  `base.js` and a `BenchmarkSuite.RunSuites` driver. QuickJS's zlib needed a
  `read` shim over `std.loadFile` (`qjs --std`). Peak RSS is from `wait4`;
  values under about 13 MB are the parent process's floor.
- One run per benchmark on a shared cloud VM: treat differences under about
  10% as noise. Shell numbers are not browser numbers.

Sources: the QuickJS CVE list at feedly.com/cve/vendors/quickjs_project,
rustsec.org/advisories/RUSTSEC-2024-0444, CVE-2026-1144, and in this tree
`docs/js-engines.md`, `docs/rust-port.md`, `docs/boa-fork.md`,
`docs/wpt-scores.md` and `docs/unsafe-policy.md`.
