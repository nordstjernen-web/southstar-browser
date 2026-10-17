#!/usr/bin/env python3
"""Southstar — count unsafe code, FFI declarations and static mut per Rust crate and hold each crate to a budget that may only go down."""

import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BUDGET = ROOT / "rust" / "unsafe-budget.tsv"
SOURCE_METRICS = ("unsafe", "ffi_imports", "ffi_exports", "static_mut")
LINTS = {
    "clippy::undocumented_unsafe_blocks": "undocumented_unsafe",
    "clippy::multiple_unsafe_ops_per_block": "multi_op_unsafe",
    "clippy::missing_safety_doc": "missing_safety_doc",
}
METRICS = SOURCE_METRICS + tuple(LINTS.values())
CLIPPY = ["cargo", "clippy", "--workspace", "--all-targets", "--locked", "--message-format=json"]
ABI_NAMES = {"C", "C-unwind", "system", "system-unwind", "Rust"}
RAW_STRING = re.compile(r'(?:b|c)?r(#*)"')
EXTERN_BLOCK = re.compile(r'\bextern\s*(?:"[^"]*"\s*)?\{')


def skip_block_comment(text, i):
    depth = 1
    i += 2
    while i < len(text) and depth:
        if text.startswith("/*", i):
            depth += 1
            i += 2
        elif text.startswith("*/", i):
            depth -= 1
            i += 2
        else:
            i += 1
    return i


def skip_quoted(text, i, quote):
    i += 1
    while i < len(text) and text[i] != quote:
        i += 2 if text[i] == "\\" else 1
    return i + 1


def char_literal_end(text, i):
    if text.startswith("\\", i + 1):
        return skip_quoted(text, i, "'")
    if i + 2 < len(text) and text[i + 2] == "'":
        return i + 3
    return None


def strip(text):
    out = []
    i = 0
    while i < len(text):
        c = text[i]
        word_start = i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")
        raw = RAW_STRING.match(text, i) if word_start and c in "bcr" else None
        if text.startswith("//", i):
            end = text.find("\n", i)
            i = len(text) if end < 0 else end
        elif text.startswith("/*", i):
            i = skip_block_comment(text, i)
            out.append(" ")
        elif raw:
            terminator = '"' + raw.group(1)
            end = text.find(terminator, raw.end())
            i = len(text) if end < 0 else end + len(terminator)
            out.append('""')
        elif c == '"' or (word_start and c in "bc" and text.startswith('"', i + 1)):
            start = i + 1 if c == '"' else i + 2
            end = skip_quoted(text, start - 1, '"')
            literal = text[start:end - 1]
            out.append('"' + literal + '"' if literal in ABI_NAMES else '""')
            i = end
        elif c == "'" and (end := char_literal_end(text, i)) is not None:
            out.append("' '")
            i = end
        else:
            out.append(c)
            i += 1
    return "".join(out)


def matching_brace(text, open_index):
    depth = 0
    for j in range(open_index, len(text)):
        if text[j] == "{":
            depth += 1
        elif text[j] == "}":
            depth -= 1
            if depth == 0:
                return j
    return len(text)


def count_source(text):
    code = strip(text)
    imports = 0
    for block in EXTERN_BLOCK.finditer(code):
        body = code[block.end() - 1:matching_brace(code, block.end() - 1)]
        imports += len(re.findall(r"\bfn\b", body)) + len(re.findall(r"\bstatic\b", body))
    return {
        "unsafe": len(re.findall(r"\bunsafe\b", code)),
        "ffi_imports": imports,
        "ffi_exports": len(re.findall(r"\b(?:no_mangle|export_name)\b", code)),
        "static_mut": len(re.findall(r"(?<!')\bstatic\s+mut\b", code)),
    }


def measure():
    counts = {}
    for manifest in sorted((ROOT / "rust").glob("*/Cargo.toml")):
        crate = manifest.parent.name
        totals = dict.fromkeys(SOURCE_METRICS, 0)
        for source in sorted(manifest.parent.rglob("*.rs")):
            if "target" in source.relative_to(manifest.parent).parts:
                continue
            for metric, value in count_source(source.read_text(encoding="utf-8")).items():
                totals[metric] += value
        counts[crate] = totals
    return counts


def package_crate(package_id):
    match = re.search(r"/rust/([^/#]+)#", package_id)
    return match.group(1) if match else None


def run_clippy(counts):
    result = subprocess.run(CLIPPY, cwd=ROOT, stdout=subprocess.PIPE, text=True)
    for totals in counts.values():
        totals.update(dict.fromkeys(LINTS.values(), 0))
    failures = []
    seen = set()
    for line in result.stdout.splitlines():
        message = json.loads(line) if line.startswith("{") else {}
        if message.get("reason") != "compiler-message":
            continue
        diagnostic = message["message"]
        code = (diagnostic.get("code") or {}).get("code")
        level = diagnostic["level"]
        spans = [s for s in diagnostic["spans"] if s["is_primary"]]
        if level not in ("warning", "error") or (level == "warning" and not spans):
            continue
        span = spans[0] if spans else {}
        key = (message["package_id"], span.get("file_name"), span.get("line_start"), span.get("column_start"), code,
               diagnostic["rendered"] if code not in LINTS else None)
        if key in seen:
            continue
        seen.add(key)
        crate = package_crate(message["package_id"])
        if code in LINTS and level == "warning" and crate in counts:
            counts[crate][LINTS[code]] += 1
        else:
            failures.append(diagnostic["rendered"].rstrip())
    if result.returncode and not failures:
        failures.append(f"cargo clippy exited with status {result.returncode}")
    return failures


def parse_budget(text):
    budget = {}
    columns = METRICS
    for line in text.splitlines():
        fields = line.split("\t")
        if not line or line.startswith("#"):
            continue
        if fields[0] == "crate":
            columns = fields[1:]
            continue
        budget[fields[0]] = dict(zip(columns, (int(f) for f in fields[1:])))
    return budget


def format_budget(counts):
    lines = ["# Southstar — per-crate unsafe budget, maintained by scripts/unsafe-budget.py; a number here may only go down.",
             "crate\t" + "\t".join(METRICS)]
    for crate, totals in sorted(counts.items()):
        lines.append(crate + "\t" + "\t".join(str(totals.get(m, 0)) for m in METRICS))
    return "\n".join(lines) + "\n"


def baseline_budget(ref):
    result = subprocess.run(["git", "show", f"{ref}:rust/unsafe-budget.tsv"],
                            cwd=ROOT, capture_output=True, encoding="utf-8", errors="replace")
    return parse_budget(result.stdout) if result.returncode == 0 else None


def total(counts, metric):
    return sum(t.get(metric, 0) for t in counts.values())


def summary_table(counts, reference, metrics):
    rows = ["| crate | " + " | ".join(metrics) + " |", "|---" * (len(metrics) + 1) + "|"]

    def cell(value, before):
        delta = value - before
        return f"{value} ({delta:+d})" if delta else str(value)

    def differs(crate):
        before = reference.get(crate, {})
        return any(counts[crate][m] != before.get(m, 0) for m in metrics)

    for crate in [c for c in sorted(counts) if differs(c)] + ["**total**"]:
        if crate == "**total**":
            values = {m: total(counts, m) for m in metrics}
            before = {m: total(reference, m) for m in metrics}
        else:
            values = counts[crate]
            before = reference.get(crate, {})
        rows.append(f"| {crate} | " + " | ".join(cell(values[m], before.get(m, 0)) for m in metrics) + " |")
    return "\n".join(rows) + "\n"


def check(counts, budget, baseline, increase_allowed, metrics):
    failures = []
    for crate in sorted(set(counts) | set(budget)):
        have = counts.get(crate, {})
        allowed = budget.get(crate)
        if allowed is None:
            failures.append(f"{crate}: not in rust/unsafe-budget.tsv; run scripts/unsafe-budget.py --update")
            continue
        for metric in metrics:
            value, limit = have.get(metric, 0), allowed.get(metric, 0)
            if value > limit:
                failures.append(f"{crate}: {metric} is {value}, over its budget of {limit}")
            elif value < limit:
                failures.append(f"{crate}: {metric} fell to {value} from {limit}; "
                                "lower the budget with scripts/unsafe-budget.py --update")
    if baseline is not None and not increase_allowed:
        for crate, allowed in sorted(budget.items()):
            before = baseline.get(crate, {})
            for metric in METRICS:
                if metric in before and allowed.get(metric, 0) > before[metric]:
                    failures.append(f"{crate}: the {metric} budget rose from {before[metric]} to {allowed[metric]}; "
                                    "an increase needs an Unsafe-Budget-Increase: trailer in the commit message")
    return failures


def increase_trailer(ref):
    result = subprocess.run(["git", "log", "--format=%B", f"{ref}..HEAD"],
                            cwd=ROOT, capture_output=True, text=True)
    return result.returncode == 0 and re.search(r"^Unsafe-Budget-Increase:\s*\S", result.stdout, re.M) is not None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--clippy", action="store_true",
                        help="also run cargo clippy: count the unsafe lints and fail on any other warning")
    parser.add_argument("--update", action="store_true", help="rewrite rust/unsafe-budget.tsv with the current counts")
    parser.add_argument("--baseline", metavar="REF", help="also fail when the budget rose since this git revision")
    parser.add_argument("--summary", metavar="FILE", help="append a markdown table of the changed crates to FILE")
    args = parser.parse_args()

    budget = parse_budget(BUDGET.read_text(encoding="utf-8")) if BUDGET.exists() else {}
    counts = measure()
    metrics = METRICS if args.clippy else SOURCE_METRICS
    failures = run_clippy(counts) if args.clippy else []
    for crate, totals in counts.items():
        for metric in METRICS:
            totals.setdefault(metric, budget.get(crate, {}).get(metric, 0))

    if args.update:
        if failures:
            print("\n\n".join(failures), file=sys.stderr)
            return 1
        BUDGET.write_text(format_budget(counts), encoding="utf-8", newline="\n")
        print(f"wrote {BUDGET.relative_to(ROOT)}: " + ", ".join(f"{m} {total(counts, m)}" for m in metrics))
        return 0

    baseline = baseline_budget(args.baseline) if args.baseline else None
    increase_allowed = baseline is not None and increase_trailer(args.baseline)
    failures += check(counts, budget, baseline, increase_allowed, metrics)
    if args.summary:
        with open(args.summary, "a", encoding="utf-8") as summary:
            reference = baseline if baseline is not None else budget
            summary.write("## Unsafe budget\n\n" + summary_table(counts, reference, metrics) + "\n")
    print(", ".join(f"{m} {total(counts, m)}" for m in metrics))
    for failure in failures:
        print(failure if "\n" in failure else "unsafe-budget: " + failure, file=sys.stderr)
    if failures and os.environ.get("GITHUB_ACTIONS") == "true":
        annotate(failures)
    return 1 if failures else 0


def annotate(failures):
    budget_lines = [f for f in failures if "\n" not in f]
    chunks = ["\n".join(budget_lines[i:i + 25]) for i in range(0, len(budget_lines), 25)]
    chunks += [f for f in failures if "\n" in f]
    for chunk in chunks[:10]:
        escaped = chunk[:3000].replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")
        print(f"::error title=unsafe-budget::{escaped}", flush=True)


if __name__ == "__main__":
    sys.exit(main())
