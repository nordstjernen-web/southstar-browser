# Southstar documentation

Index of the docs in this directory. The project overview is in the
top-level [README.md](../README.md); the development plan is
[SOUTHSTAR.md](../SOUTHSTAR.md); the AI/Claude working rules are in
[CLAUDE.md](../CLAUDE.md).

Southstar is being ported from C to Rust. The docs here describe what the
browser does, how to build it and how it is measured. They avoid describing
the C implementation, which is being replaced module by module; for that, read
the code and the plan.

## The Rust port

- [rust-port.md](rust-port.md) — the plan: the code it starts from, the phases and their order, build integration, verification, open decisions, and the modules ported so far.

## Using the browser

- [Controls.md](Controls.md) — keyboard, mouse, and touch controls.
- [printing.md](printing.md) — printing to paper or PDF, and the CSS that paginates it (`@page`, `@media print`, `break-*`).
- [media.md](media.md) — how `<video>`/`<audio>` play (MPEG-1, optional WebM, the audio helper, WebVTT `<track>` captions, external-player fallback).
- [Proxy.md](Proxy.md) — proxies and VPNs.
- [extensions.md](extensions.md) — the scoped WebExtensions support (content scripts and a slice of `browser.*`).
- [privacy-policy.md](privacy-policy.md) — what the browser does and does not collect.

## Building

- [Linux.md](Linux.md) — build, run, and package on Linux.
- [Windows.md](Windows.md) — build and package on Windows (MSYS2).
- [macOS.md](macOS.md) — macOS install (first-launch quarantine step, troubleshooting), build, `.app`/`.dmg`, and signing.

## Standards and measurement

- [HTML-compatibility.md](HTML-compatibility.md) — section-by-section WHATWG HTML coverage.
- [CSS-compatibility.md](CSS-compatibility.md) — CSS feature coverage.
- [wpt.md](wpt.md) — running web-platform-tests against the browser.
- [wpt-scores.md](wpt-scores.md) — tracked WPT scores over time (written by `scripts/wpt-score.sh`, with per-run detail in `wpt-runs/` and `wpt-subtests.tsv`).
- [wpt-fast-scores.md](wpt-fast-scores.md) — scores from the wpt-fast tree (`scripts/wpt-fast.sh`).
- [Benchmarking.md](Benchmarking.md) — Speedometer benchmarking.
