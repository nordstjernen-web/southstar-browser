#!/usr/bin/env python3
"""Southstar — build the Rust workspace for meson: one static library plus a ninja depfile, or the native libraries Rust's std links against."""

import filecmp
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


def native_libs(rustc, source_root):
    with tempfile.TemporaryDirectory() as work:
        src = Path(work) / "empty.rs"
        src.write_text("")
        result = subprocess.run(
            [rustc, "--crate-type", "staticlib", "--print", "native-static-libs",
             "-o", str(Path(work) / "libempty.a"), str(src)],
            cwd=source_root, capture_output=True, text=True, check=True)
    for line in result.stderr.splitlines():
        marker = "native-static-libs:"
        if marker in line:
            print("\n".join(line.split(marker, 1)[1].split()))
            return
    sys.exit("rustc did not report its native static libraries")


def depfile_entries(path):
    text = path.read_text().replace("\\\n", " ")
    for line in text.splitlines():
        head, sep, deps = line.partition(": ")
        if sep and head.endswith(".a"):
            return deps.split()
    return []


def build(cargo, source_root, target_dir, profile, crate, output, depfile, features):
    source_root = Path(source_root)
    command = [cargo, "build", "--locked", "--package", crate,
               "--manifest-path", str(source_root / "Cargo.toml"),
               "--target-dir", target_dir]
    command += ["--release"] if profile == "release" else []
    command += ["--features", ",".join(features)] if features else []
    subprocess.run(command, cwd=source_root, check=True)
    artifact_dir = Path(target_dir) / ("release" if profile == "release" else "debug")
    stem = "lib" + crate.replace("-", "_")
    artifact = artifact_dir / (stem + ".a")
    if not Path(output).exists() or not filecmp.cmp(artifact, output, shallow=False):
        shutil.copyfile(artifact, output)
    manifests = [source_root / "Cargo.toml", source_root / "Cargo.lock",
                 source_root / "rust-toolchain.toml"]
    manifests += sorted(source_root.glob("rust/*/Cargo.toml"))
    deps = depfile_entries(artifact_dir / (stem + ".d")) + [str(m) for m in manifests if m.exists()]
    Path(depfile).write_text(output.replace(" ", "\\ ") + ": " + " \\\n  ".join(deps) + "\n")


def main():
    if len(sys.argv) >= 4 and sys.argv[1] == "native-libs":
        native_libs(sys.argv[2], sys.argv[3])
    elif len(sys.argv) >= 9 and sys.argv[1] == "build":
        build(*sys.argv[2:9], sys.argv[9:])
    else:
        sys.exit("usage: cargo-build.py native-libs RUSTC SOURCE_ROOT | "
                 "build CARGO SOURCE_ROOT TARGET_DIR PROFILE CRATE OUTPUT DEPFILE [FEATURE...]")


if __name__ == "__main__":
    main()
