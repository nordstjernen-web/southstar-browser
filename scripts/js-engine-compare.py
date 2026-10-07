#!/usr/bin/env python3
"""Southstar — builds southstar-jsshell for each JavaScript engine, runs test262 and the Octane benchmarks on each, and writes docs/js-engines.md."""

import argparse
import json
import os
import platform
import re
import statistics
import subprocess
import sys
import tarfile
import time
import urllib.request
from collections import defaultdict
from datetime import date
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WORK = ROOT / "builddir" / "js-compare"
TEST262 = ROOT / "src" / "quickjs" / "test262"
OCTANE_URL = "https://raw.githubusercontent.com/chromium/octane/master/"
ENGINES = {
    "quickjs": ["--features", "quickjs"],
    "boa": ["--no-default-features", "--features", "boa"],
}
OCTANE = [
    ("Richards", ["richards.js"]),
    ("DeltaBlue", ["deltablue.js"]),
    ("Crypto", ["crypto.js"]),
    ("RayTrace", ["raytrace.js"]),
    ("EarleyBoyer", ["earley-boyer.js"]),
    ("RegExp", ["regexp.js"]),
    ("Splay", ["splay.js"]),
    ("NavierStokes", ["navier-stokes.js"]),
    ("PdfJS", ["pdfjs.js"]),
    ("Gameboy", ["gbemu-part1.js", "gbemu-part2.js"]),
    ("CodeLoad", ["code-load.js"]),
    ("Box2D", ["box2d.js"]),
    ("zlib", ["zlib.js", "zlib-data.js"]),
    ("Typescript", ["typescript.js", "typescript-input.js", "typescript-compiler.js"]),
]
DRIVER = """
var success = true;
function PrintResult(name, result) { print(name + ': ' + result); }
function PrintError(name, error) { PrintResult(name, error); success = false; }
function PrintScore(score) { if (success) print('Score: ' + score); }
BenchmarkSuite.config.doWarmup = undefined;
BenchmarkSuite.config.doDeterministic = undefined;
BenchmarkSuite.RunSuites({ NotifyResult: PrintResult, NotifyError: PrintError, NotifyScore: PrintScore });
"""


def shell(engine):
    exe = WORK.parent / f"jsshell-{engine}" / "release" / "southstar-jsshell"
    return exe.with_suffix(".exe") if os.name == "nt" or sys.platform == "msys" else exe


def build(engine):
    lib_dir = ROOT / "builddir" / "src" / "quickjs"
    if engine == "quickjs" and not list(lib_dir.glob("libqjs.*")):
        sys.exit("build the browser first (meson compile -C builddir): libqjs is missing")
    command = ["cargo", "build", "--release", "-p", "southstar-jsshell",
               "--target-dir", str(WORK.parent / f"jsshell-{engine}")] + ENGINES[engine]
    env = dict(os.environ, NS_QUICKJS_LIB_DIR=str(lib_dir))
    subprocess.run(command, cwd=ROOT, env=env, check=True)


def fetch_test262():
    if (TEST262 / "harness").is_dir():
        return
    print("fetching tc39/test262 ...", file=sys.stderr)
    archive = WORK / "test262.tar.gz"
    urllib.request.urlretrieve("https://codeload.github.com/tc39/test262/tar.gz/refs/heads/main", archive)
    with tarfile.open(archive) as tar:
        for member in tar.getmembers():
            parts = member.name.split("/", 1)
            if len(parts) == 2 and parts[1]:
                member.name = parts[1]
                tar.extract(member, TEST262)
    archive.unlink()


def fetch_octane():
    directory = WORK / "octane"
    directory.mkdir(parents=True, exist_ok=True)
    for _, files in [("base", ["base.js"])] + OCTANE:
        for name in files:
            target = directory / name
            if not target.exists():
                urllib.request.urlretrieve(OCTANE_URL + name, target)
    (directory / "driver.js").write_text(DRIVER)
    return directory


def run_test262(engine, reuse):
    results = WORK / f"test262-{engine}.jsonl"
    if not (reuse and results.exists()):
        subprocess.run([str(shell(engine)), "test262", "--root", str(TEST262), "--results", str(results)],
                       cwd=ROOT, check=True, stdout=subprocess.DEVNULL)
    counts = defaultdict(lambda: [0, 0])
    for line in results.read_text(encoding="utf-8").splitlines():
        record = json.loads(line)
        parts = record["test"].split("/")
        group = parts[1] if len(parts) > 2 else "other"
        for key in (group, "total"):
            counts[key][1] += 1
            counts[key][0] += record["status"] == "PASS"
        if record["status"] in ("CRASH", "TIMEOUT"):
            counts[record["status"].lower()][0] += 1
    return dict(counts)


def run_octane(engine, octane, timeout):
    scores = {}
    for name, files in OCTANE:
        paths = [str(octane / "base.js")] + [str(octane / f) for f in files] + [str(octane / "driver.js")]
        try:
            done = subprocess.run([str(shell(engine)), "run"] + paths, capture_output=True,
                                  text=True, timeout=timeout, cwd=octane)
            found = re.search(rf"^{re.escape(name)}: (.*)$", done.stdout, re.M)
            result = found.group(1).strip() if found else "no result"
            scores[name] = result if re.fullmatch(r"[0-9.]+", result) else f"fails ({result.split(':')[0]})"
        except subprocess.TimeoutExpired:
            scores[name] = "timeout"
        print(f"  {engine} {name}: {scores[name]}", file=sys.stderr)
    numbers = [float(s) for s in scores.values() if re.fullmatch(r"[0-9.]+", s)]
    if len(numbers) == len(OCTANE):
        scores["Score (geometric mean)"] = f"{statistics.geometric_mean(numbers):.0f}"
    return scores


def startup(engine):
    empty = WORK / "empty.js"
    empty.write_text("")
    samples = []
    for _ in range(15):
        started = time.perf_counter()
        subprocess.run([str(shell(engine)), "run", str(empty)], capture_output=True, check=True)
        samples.append((time.perf_counter() - started) * 1000)
    return statistics.median(samples)


def version(engine):
    return subprocess.run([str(shell(engine)), "--version"], capture_output=True, text=True).stdout.strip()


def percent(passed, total):
    return f"{100.0 * passed / total:.1f}% ({passed:,} / {total:,})" if total else "—"


def write_doc(engines, data):
    lines = [
        "# JavaScript engines compared",
        "",
        "Generated by `scripts/js-engine-compare.py` on "
        f"{date.today().isoformat()} ({platform.system()} {platform.machine()}, "
        f"{os.cpu_count()} threads). Each engine runs behind the same `js-engine` "
        "layer and the same `southstar-jsshell` host (see docs/rust-port.md, "
        "\"JavaScript engines\"). Optional engines are built with "
        "`cargo build -p southstar-jsshell --no-default-features --features <engine>`.",
        "",
        "| | " + " | ".join(engines) + " |",
        "|---|" + "---:|" * len(engines),
        "| Version | " + " | ".join(data[e]["version"] for e in engines) + " |",
    ]
    groups = sorted({g for e in engines for g in data[e].get("test262", {})} - {"total", "crash", "timeout"})
    if groups:
        for group in ["total"] + groups:
            label = "test262 total" if group == "total" else f"test262 {group}"
            row = []
            for e in engines:
                passed, total = data[e]["test262"].get(group, [0, 0])
                row.append(percent(passed, total))
            lines.append(f"| {label} | " + " | ".join(row) + " |")
        for kind, label in (("crash", "test262 tests that crashed the engine"), ("timeout", "test262 tests over 10 s")):
            row = [str(data[e]["test262"].get(kind, [0, 0])[0]) for e in engines]
            lines.append(f"| {label} | " + " | ".join(row) + " |")
    names = [n for n, _ in OCTANE] + ["Score (geometric mean)"]
    if any("octane" in data[e] for e in engines):
        for name in names:
            row = [data[e].get("octane", {}).get(name, "—") for e in engines]
            lines.append(f"| Octane {name} | " + " | ".join(row) + " |")
    lines.append("| Start-up, empty script (median ms) | "
                 + " | ".join(f"{data[e]['startup']:.1f}" for e in engines) + " |")
    lines.append("| southstar-jsshell size (MB, release) | "
                 + " | ".join(f"{data[e]['size'] / 1e6:.1f}" for e in engines) + " |")
    lines += [
        "",
        "test262 counts each test file once: it passes only if every mode it runs in "
        "(sloppy and strict, or module) passes. `test/staging` is left out. Octane "
        "scores are higher-is-better and come from one run each, so treat small "
        "differences as noise. Neither engine has a JIT.",
        "",
    ]
    (ROOT / "docs" / "js-engines.md").write_text("\n".join(lines), encoding="utf-8", newline="\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--engines", default=",".join(ENGINES))
    parser.add_argument("--skip-build", action="store_true")
    parser.add_argument("--skip-test262", action="store_true")
    parser.add_argument("--reuse-test262", action="store_true")
    parser.add_argument("--skip-octane", action="store_true")
    parser.add_argument("--octane-timeout", type=int, default=600)
    parser.add_argument("--rewrite", action="store_true",
                        help="rewrite docs/js-engines.md from the saved results, recounting test262")
    args = parser.parse_args()
    engines = args.engines.split(",")
    WORK.mkdir(parents=True, exist_ok=True)
    if args.rewrite:
        data = json.loads((WORK / "results.json").read_text())
        for engine in engines:
            data[engine]["test262"] = run_test262(engine, True)
        (WORK / "results.json").write_text(json.dumps(data, indent=2))
        write_doc(engines, data)
        print((ROOT / "docs" / "js-engines.md").read_text(encoding="utf-8"))
        return
    data = {}
    for engine in engines:
        if not args.skip_build:
            build(engine)
        data[engine] = {"version": version(engine), "size": shell(engine).stat().st_size}
    if not args.skip_test262:
        fetch_test262()
        for engine in engines:
            print(f"test262 on {engine} ...", file=sys.stderr)
            data[engine]["test262"] = run_test262(engine, args.reuse_test262)
    if not args.skip_octane:
        octane = fetch_octane()
        for engine in engines:
            data[engine]["octane"] = run_octane(engine, octane, args.octane_timeout)
    for engine in engines:
        data[engine]["startup"] = startup(engine)
    (WORK / "results.json").write_text(json.dumps(data, indent=2))
    write_doc(engines, data)
    print((ROOT / "docs" / "js-engines.md").read_text(encoding="utf-8"))


if __name__ == "__main__":
    main()
