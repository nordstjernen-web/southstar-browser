#!/usr/bin/env python3
"""Southstar — diff the forked Boa crates (and regress) in third_party against what they fork: a crates.io release, verified by the registry checksum, or a Boa git commit."""

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
CRATES = ("boa_engine", "boa_parser", "boa_ast", "boa_gc", "boa_interner", "boa_string")
FORKS = CRATES + ("regress",)
INDEX = "https://index.crates.io/{0}/{1}"
DOWNLOAD = "https://static.crates.io/crates/{0}/{0}-{1}.crate"
COMPARED = ("src", "Cargo.toml")
COMMIT_ARCHIVE = "https://codeload.github.com/boa-dev/boa/tar.gz/{0}"


def fork_dir(crate):
    return ROOT / "third_party" / crate


def fork_metadata(crate):
    with (fork_dir(crate) / "Cargo.toml").open("rb") as manifest:
        return tomllib.load(manifest)["package"]["metadata"]["southstar"]


def fetch(url):
    request = urllib.request.Request(url, headers={"User-Agent": "southstar-boa-fork-diff"})
    with urllib.request.urlopen(request, timeout=60) as response:
        return response.read()


def index_prefix(crate):
    if len(crate) <= 2:
        return str(len(crate))
    if len(crate) == 3:
        return f"3/{crate[0]}"
    return f"{crate[:2]}/{crate[2:4]}"


def registry_checksum(crate, version):
    for line in fetch(INDEX.format(index_prefix(crate), crate)).decode().splitlines():
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


def unpack_commit(crate, commit, into):
    archive_path = Path(into) / f"boa-{commit}.tar.gz"
    if not archive_path.exists():
        archive_path.write_bytes(fetch(COMMIT_ARCHIVE.format(commit)))
    root = f"boa-{commit}/core/{crate.removeprefix('boa_')}/"
    with tarfile.open(archive_path, mode="r:gz") as archive:
        members = [m for m in archive.getmembers() if m.name.startswith(root)]
        if not members:
            sys.exit(f"boa-fork-diff: {crate} is not in Boa commit {commit}")
        archive.extractall(Path(into) / "commit", members=members, filter="data")
    return Path(into) / "commit" / root


def copy(source_root, into, names=COMPARED):
    into.mkdir(parents=True, exist_ok=True)
    for name in names:
        source = source_root / name
        if source.is_dir():
            shutil.copytree(source, into / name)
        else:
            into.mkdir(exist_ok=True)
            shutil.copy2(source, into / name)


def patch(crates, version, scratch):
    for crate in crates:
        metadata = fork_metadata(crate)
        commit = None if version else metadata.get("upstream-commit")
        if commit:
            upstream = unpack_commit(crate, commit, scratch)
            names = ("src",)
        else:
            upstream = unpack(crate, version or metadata["upstream-version"], scratch)
            names = COMPARED
        copy(upstream, Path(scratch) / "upstream" / crate, names)
        copy(fork_dir(crate), Path(scratch) / "fork" / crate, names)
    result = subprocess.run(
        ["git", "diff", "--no-index", "--no-prefix", "upstream", "fork"],
        cwd=scratch,
        check=False,
        capture_output=True,
        encoding="utf-8",
        errors="replace",
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
    parser.add_argument("--crate", choices=FORKS, help="compare one forked crate (default: the Boa crates)")
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
