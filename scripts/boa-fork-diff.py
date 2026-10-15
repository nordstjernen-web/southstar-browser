#!/usr/bin/env python3
"""Southstar — diff the forked Boa crates in third_party against the crates.io releases they fork, verified by the registry checksums."""

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
CRATES = ("boa_engine", "boa_parser", "boa_ast")
INDEX = "https://index.crates.io/bo/a_/{0}"
DOWNLOAD = "https://static.crates.io/crates/{0}/{0}-{1}.crate"
COMPARED = ("src", "Cargo.toml")


def fork_dir(crate):
    return ROOT / "third_party" / crate


def fork_version(crate):
    with (fork_dir(crate) / "Cargo.toml").open("rb") as manifest:
        return tomllib.load(manifest)["package"]["version"]


def fetch(url):
    request = urllib.request.Request(url, headers={"User-Agent": "southstar-boa-fork-diff"})
    with urllib.request.urlopen(request, timeout=60) as response:
        return response.read()


def registry_checksum(crate, version):
    for line in fetch(INDEX.format(crate)).decode().splitlines():
        entry = json.loads(line)
        if entry["vers"] == version:
            return entry["cksum"]
    sys.exit(f"boa-fork-diff: {crate} {version} is not in the crates.io index")


def unpack(crate, version, into):
    archive_bytes = fetch(DOWNLOAD.format(crate, version))
    expected = registry_checksum(crate, version)
    actual = hashlib.sha256(archive_bytes).hexdigest()
    if actual != expected:
        sys.exit(f"boa-fork-diff: {crate} checksum {actual} does not match the index's {expected}")
    prefix = f"{crate}-{version}/"
    with tarfile.open(fileobj=io.BytesIO(archive_bytes), mode="r:gz") as archive:
        members = [m for m in archive.getmembers() if m.name.startswith(prefix)]
        archive.extractall(Path(into) / "release", members=members, filter="data")
    return Path(into) / "release" / prefix


def copy(source_root, into):
    into.mkdir(parents=True, exist_ok=True)
    for name in COMPARED:
        source = source_root / name
        if source.is_dir():
            shutil.copytree(source, into / name)
        else:
            into.mkdir(exist_ok=True)
            shutil.copy2(source, into / name)


def patch(crates, version, scratch):
    for crate in crates:
        release = unpack(crate, version or fork_version(crate), scratch)
        copy(release, Path(scratch) / "upstream" / crate)
        copy(fork_dir(crate), Path(scratch) / "fork" / crate)
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
    parser.add_argument("--crate", choices=CRATES, help="compare one forked crate (default: all)")
    args = parser.parse_args()
    crates = (args.crate,) if args.crate else CRATES
    with tempfile.TemporaryDirectory() as scratch:
        text = patch(crates, args.version, scratch)
    if args.stat:
        stat(text)
    else:
        sys.stdout.write(text)


if __name__ == "__main__":
    main()
