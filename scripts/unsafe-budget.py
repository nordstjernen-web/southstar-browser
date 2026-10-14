#!/usr/bin/env python3
"""Southstar — count unsafe code, FFI declarations and static mut per Rust crate and hold each crate to a budget that may only go down."""

import argparse
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BUDGET = ROOT / "rust" / "unsafe-budget.tsv"
METRICS = ("unsafe", "ffi_imports", "ffi_exports", "static_mut")
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
        "static_mut": len(re.findall(r"\bstatic\s+mut\b", code)),
    }


def measure():
    counts = {}
    for manifest in sorted((ROOT / "rust").glob("*/Cargo.toml")):
        crate = manifest.parent.name
        totals = dict.fromkeys(METRICS, 0)
        for source in sorted(manifest.parent.rglob("*.rs")):
            if "target" in source.relative_to(manifest.parent).parts:
                continue
            for metric, value in count_source(source.read_text(encoding="utf-8")).items():
                totals[metric] += value
        counts[crate] = totals
    return counts


def parse_budget(text):
    budget = {}
    for line in text.splitlines():
        if not line or line.startswith("#") or line.startswith("crate\t"):
            continue
        fields = line.split("\t")
        budget[fields[0]] = dict(zip(METRICS, (int(f) for f in fields[1:])))
    return budget


def format_budget(counts):
    lines = ["# Southstar — per-crate unsafe budget, maintained by scripts/unsafe-budget.py; a number here may only go down.",
             "crate\t" + "\t".join(METRICS)]
    for crate, totals in sorted(counts.items()):
        lines.append(crate + "\t" + "\t".join(str(totals[m]) for m in METRICS))
    return "\n".join(lines) + "\n"


def baseline_budget(ref):
    result = subprocess.run(["git", "show", f"{ref}:rust/unsafe-budget.tsv"],
                            cwd=ROOT, capture_output=True, text=True)
    return parse_budget(result.stdout) if result.returncode == 0 else None


def total(counts, metric):
    return sum(t[metric] for t in counts.values())


def summary_table(counts, reference):
    rows = ["| crate | " + " | ".join(METRICS) + " |", "|---" * (len(METRICS) + 1) + "|"]

    def cell(value, before):
        delta = value - before
        return f"{value} ({delta:+d})" if delta else str(value)

    zero = dict.fromkeys(METRICS, 0)
    changed = [c for c in sorted(counts) if counts[c] != reference.get(c, zero)]
    for crate in changed + ["**total**"]:
        if crate == "**total**":
            values = {m: total(counts, m) for m in METRICS}
            before = {m: total(reference, m) for m in METRICS}
        else:
            values = counts[crate]
            before = reference.get(crate, zero)
        rows.append(f"| {crate} | " + " | ".join(cell(values[m], before[m]) for m in METRICS) + " |")
    return "\n".join(rows) + "\n"


def check(counts, budget, baseline, increase_allowed):
    failures = []
    zero = dict.fromkeys(METRICS, 0)
    for crate in sorted(set(counts) | set(budget)):
        have = counts.get(crate, zero)
        allowed = budget.get(crate)
        if allowed is None:
            failures.append(f"{crate}: not in rust/unsafe-budget.tsv; run scripts/unsafe-budget.py --update")
            continue
        for metric in METRICS:
            if have[metric] > allowed[metric]:
                failures.append(f"{crate}: {metric} is {have[metric]}, over its budget of {allowed[metric]}")
            elif have[metric] < allowed[metric]:
                failures.append(f"{crate}: {metric} fell to {have[metric]} from {allowed[metric]}; "
                                "lower the budget with scripts/unsafe-budget.py --update")
    if baseline is not None and not increase_allowed:
        for crate, allowed in sorted(budget.items()):
            before = baseline.get(crate, zero)
            for metric in METRICS:
                if allowed[metric] > before[metric]:
                    failures.append(f"{crate}: the {metric} budget rose from {before[metric]} to {allowed[metric]}; "
                                    "an increase needs an Unsafe-Budget-Increase: trailer in the commit message")
    return failures


def increase_trailer(ref):
    result = subprocess.run(["git", "log", "--format=%B", f"{ref}..HEAD"],
                            cwd=ROOT, capture_output=True, text=True)
    return result.returncode == 0 and re.search(r"^Unsafe-Budget-Increase:\s*\S", result.stdout, re.M) is not None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--update", action="store_true", help="rewrite rust/unsafe-budget.tsv with the current counts")
    parser.add_argument("--baseline", metavar="REF", help="also fail when the budget rose since this git revision")
    parser.add_argument("--summary", metavar="FILE", help="append a markdown table of the changed crates to FILE")
    args = parser.parse_args()

    counts = measure()
    if args.update:
        BUDGET.write_text(format_budget(counts))
        print(f"wrote {BUDGET.relative_to(ROOT)}: " + ", ".join(f"{m} {total(counts, m)}" for m in METRICS))
        return 0

    budget = parse_budget(BUDGET.read_text()) if BUDGET.exists() else {}
    baseline = baseline_budget(args.baseline) if args.baseline else None
    failures = check(counts, budget, baseline, args.baseline is not None and increase_trailer(args.baseline))
    table = summary_table(counts, baseline if baseline is not None else budget)
    if args.summary:
        with open(args.summary, "a", encoding="utf-8") as summary:
            summary.write("## Unsafe budget\n\n" + table + "\n")
    print(", ".join(f"{m} {total(counts, m)}" for m in METRICS))
    for failure in failures:
        print("unsafe-budget: " + failure, file=sys.stderr)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
