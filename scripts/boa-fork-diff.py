#!/usr/bin/env python3
"""Southstar — diff third_party/boa_engine against the crates.io release it forks, verified by the registry checksum."""

import argparse
import hashlib
import io
import json
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FORK = ROOT / "third_party" / "boa_engine"
INDEX = "https://index.crates.io/bo/a_/boa_engine"
DOWNLOAD = "https://static.crates.io/crates/boa_engine/boa_engine-{0}.crate"
COMPARED = ("src", "Cargo.toml")


def fork_version():
    with (FORK / "Cargo.toml").open("rb") as manifest:
        return tomllib.load(manifest)["package"]["version"]


def fetch(url):
    request = urllib.request.Request(url, headers={"User-Agent": "southstar-boa-fork-diff"})
    with urllib.request.urlopen(request, timeout=60) as response:
        return response.read()


def registry_checksum(version):
    for line in fetch(INDEX).decode().splitlines():
        entry = json.loads(line)
        if entry["vers"] == version:
            return entry["cksum"]
    sys.exit(f"boa-fork-diff: boa_engine {version} is not in the crates.io index")


def unpack(version, into):
    crate = fetch(DOWNLOAD.format(version))
    expected = registry_checksum(version)
    actual = hashlib.sha256(crate).hexdigest()
    if actual != expected:
        sys.exit(f"boa-fork-diff: checksum {actual} does not match the index's {expected}")
    prefix = f"boa_engine-{version}/"
    with tarfile.open(fileobj=io.BytesIO(crate), mode="r:gz") as archive:
        members = [m for m in archive.getmembers() if m.name.startswith(prefix)]
        archive.extractall(Path(into) / "release", members=members, filter="data")
    return Path(into) / "release" / prefix


def copy(source_root, into):
    for name in COMPARED:
        source = source_root / name
        if source.is_dir():
            shutil.copytree(source, into / name)
        else:
            into.mkdir(exist_ok=True)
            shutil.copy2(source, into / name)


def patch(release, scratch):
    copy(release, Path(scratch) / "upstream")
    copy(FORK, Path(scratch) / "fork")
    result = subprocess.run(
        ["git", "diff", "--no-index", "--no-prefix", "upstream", "fork"],
        cwd=scratch,
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode > 1:
        sys.exit(result.stderr)
    return result.stdout


def stat(text):
    counts = {}
    name = None
    for line in text.splitlines():
        if line.startswith("diff --git "):
            name = line.split()[-1].split("/", 1)[1]
            counts[name] = [0, 0]
        elif name and line.startswith("+") and not line.startswith("+++ "):
            counts[name][0] += 1
        elif name and line.startswith("-") and not line.startswith("--- "):
            counts[name][1] += 1
    width = max(map(len, counts), default=0)
    for name, (added, removed) in counts.items():
        print(f"{name:<{width}}  +{added:<4} -{removed}")
    added = sum(a for a, _ in counts.values())
    removed = sum(r for _, r in counts.values())
    print(f"{len(counts)} files, +{added} -{removed}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--stat", action="store_true", help="print a per-file summary")
    parser.add_argument("--version", help="upstream release to compare (default: the fork's)")
    args = parser.parse_args()
    version = args.version or fork_version()
    with tempfile.TemporaryDirectory() as scratch:
        text = patch(unpack(version, scratch), scratch)
    if args.stat:
        stat(text)
    else:
        sys.stdout.write(text)


if __name__ == "__main__":
    main()
