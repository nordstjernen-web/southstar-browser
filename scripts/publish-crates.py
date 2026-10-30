#!/usr/bin/env python3
"""Southstar — publish every crate to crates.io in dependency order, skipping versions already there and waiting out the rate limit on new crates."""

import argparse
import datetime
import email.utils
import json
import re
import shutil
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BUNDLE = ROOT / "rust" / "southstar" / "bundle"
RETRY_AFTER = re.compile(r"try again after (\w{3}, \d{1,2} \w{3} \d{4} \d{2}:\d{2}:\d{2} GMT)")
BUNDLED = ["License.md", "COPYING", "THIRD-PARTY-LICENSES.md", "data/safebrowsing.list", "data/i18n", "data/icons"]


def metadata():
    output = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked"],
        cwd=ROOT, check=True, capture_output=True, text=True).stdout
    return json.loads(output)["packages"]


def publish_order(packages):
    local = {p["name"]: p for p in packages if p["source"] is None and p["publish"] != []}
    order, seen = [], set()

    def visit(name):
        if name in seen:
            return
        seen.add(name)
        for dep in local[name]["dependencies"]:
            if dep["kind"] != "dev" and dep.get("path") and dep["name"] in local:
                visit(dep["name"])
        order.append(local[name])

    for name in sorted(local):
        visit(name)
    return order


def published(name, version):
    request = urllib.request.Request(
        f"https://crates.io/api/v1/crates/{name}/{version}",
        headers={"User-Agent": "southstar-publish-crates (github.com/nordstjernen-web/southstar-browser)"})
    try:
        with urllib.request.urlopen(request, timeout=60):
            return True
    except urllib.error.HTTPError as error:
        if error.code == 404:
            return False
        raise


def stage_bundle():
    shutil.rmtree(BUNDLE, ignore_errors=True)
    for item in BUNDLED:
        source, target = ROOT / item, BUNDLE / item
        target.parent.mkdir(parents=True, exist_ok=True)
        if source.is_dir():
            shutil.copytree(source, target)
        else:
            shutil.copy2(source, target)


def retry_delay(message):
    match = RETRY_AFTER.search(message)
    if not match:
        return 60
    try:
        when = email.utils.parsedate_to_datetime(match.group(1))
    except (TypeError, ValueError):
        return 60
    remaining = (when - datetime.datetime.now(datetime.timezone.utc)).total_seconds()
    return max(10, int(remaining) + 5)


def publish(package):
    command = ["cargo", "publish", "--locked", "--no-verify", "--manifest-path", package["manifest_path"]]
    if package["name"] == "southstar":
        command.append("--allow-dirty")
    while True:
        result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True)
        sys.stdout.write(result.stdout)
        sys.stderr.write(result.stderr)
        if result.returncode == 0:
            return
        if "429" in result.stderr or "too many" in result.stderr.lower():
            wait = retry_delay(result.stderr)
            print(f"publish-crates: rate limited on {package['name']}; waiting {wait} s", flush=True)
            time.sleep(wait)
            continue
        sys.exit(f"publish-crates: {package['name']} failed")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--list", action="store_true", help="print the publish order and stop")
    args = parser.parse_args()
    order = publish_order(metadata())
    if args.list:
        for package in order:
            print(package["name"], package["version"])
        return
    try:
        for index, package in enumerate(order, 1):
            name, version = package["name"], package["version"]
            if published(name, version):
                print(f"[{index}/{len(order)}] {name} {version} is already on crates.io", flush=True)
                continue
            print(f"[{index}/{len(order)}] publishing {name} {version}", flush=True)
            if name == "southstar":
                stage_bundle()
            publish(package)
    finally:
        shutil.rmtree(BUNDLE, ignore_errors=True)


if __name__ == "__main__":
    main()
