#!/usr/bin/env python3
"""Southstar — write the linker version script that exports exactly the functions src/libsouthstar.h declares."""

import re
import sys


def api_names(header):
    text = re.sub(r"/\*.*?\*/", " ", header, flags=re.S)
    text = re.sub(r"//[^\n]*", " ", text)
    text = re.sub(r"^\s*#[^\n]*", " ", text, flags=re.M)
    names = re.findall(r"\b(ns_[A-Za-z0-9_]+)\s*\(", text)
    return sorted(set(names))


def main():
    header_path, out_path = sys.argv[1], sys.argv[2]
    with open(header_path, encoding="utf-8") as f:
        names = api_names(f.read())
    if not names:
        sys.exit(f"{header_path}: no ns_* functions found")
    lines = ["{", "  global:"]
    lines += [f"    {name};" for name in names]
    lines += ["  local:", "    *;", "};", ""]
    with open(out_path, "w", encoding="utf-8") as f:
        f.write("\n".join(lines))


if __name__ == "__main__":
    main()
