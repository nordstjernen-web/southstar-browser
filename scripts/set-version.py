#!/usr/bin/env python3
"""Southstar — set the release version everywhere it is written: the workspace, meson, the Boa forks and every pinned internal dependency."""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
VERSION = re.compile(r"^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$")


def rewrite(path, pattern, replacement, count=0, flags=re.M):
    text = path.read_text(encoding="utf-8")
    updated, n = re.subn(pattern, replacement, text, count=count, flags=flags)
    if n == 0:
        sys.exit(f"set-version: no version found in {path.relative_to(ROOT)}")
    path.write_text(updated, encoding="utf-8")


def main():
    if len(sys.argv) != 2 or not VERSION.match(sys.argv[1]):
        sys.exit("usage: set-version.py VERSION")
    version = sys.argv[1]
    rewrite(ROOT / "Cargo.toml", r'(\[workspace\.package\]\nversion = )"[^"]+"', rf'\1"{version}"', 1)
    rewrite(ROOT / "meson.build", r"^(  version: )'[^']+'", rf"\1'{version}'", 1)
    for manifest in sorted(ROOT.glob("third_party/boa_*/Cargo.toml")):
        rewrite(manifest, r'(\[package\]\n(?:[^\[\n][^\n]*\n)*?version = )"[^"]+"', rf'\1"{version}"', 1)
    pinned = r'(version = )"=[^"]+"'
    for manifest in sorted(ROOT.glob("rust/*/Cargo.toml")) + sorted(ROOT.glob("third_party/boa_*/Cargo.toml")):
        text = manifest.read_text(encoding="utf-8")
        if re.search(pinned, text):
            manifest.write_text(re.sub(pinned, rf'\1"={version}"', text), encoding="utf-8")
    subprocess.run(["cargo", "update", "--workspace", "--offline"], cwd=ROOT, check=True)


if __name__ == "__main__":
    main()
