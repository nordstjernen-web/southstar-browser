#!/usr/bin/env python3
"""Southstar — merge the workspace's crates into the groups of crates-io-groups.toml, check the result, and publish it to crates.io."""

import argparse
import datetime
import email.utils
import json
import os
import re
import shutil
import subprocess
import sys
import time
import tomllib
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
GROUPS = Path(__file__).resolve().parent / "crates-io-groups.toml"
READMES = Path(__file__).resolve().parent / "crates-io-readmes"
STAGING = ROOT / "target" / "crates-io"
BUNDLED = ["License.md", "COPYING", "THIRD-PARTY-LICENSES.md", "data/safebrowsing.list", "data/i18n", "data/icons"]
RETRY_AFTER = re.compile(r"try again after (\w{3}, \d{1,2} \w{3} \d{4} \d{2}:\d{2}:\d{2} GMT)")
CRATE_IDENT = re.compile(r"(::)?\bsouthstar_[a-z0-9_]+\b")
SELF_PATH = re.compile(r"(\$?)\bcrate::")
IDENT_CHAR = re.compile(r"[A-Za-z0-9_>]")
CARGO = os.environ.get("CARGO") or shutil.which("cargo") or "cargo"
REPOSITORY = "https://github.com/nordstjernen-web/southstar-browser"
USER_AGENT = "southstar-crates-io (github.com/nordstjernen-web/southstar-browser)"


def short(name):
    return name if name == "southstar" else name.removeprefix("southstar-")


def ident(name):
    return name.replace("-", "_")


class Plan:
    def __init__(self):
        output = subprocess.run([CARGO, "metadata", "--format-version", "1", "--locked"],
                                cwd=ROOT, check=True, capture_output=True, text=True).stdout
        metadata = json.loads(output)
        local = [p for p in metadata["packages"] if p["source"] is None]
        self.version = next(p["version"] for p in local if p["name"] == "southstar")
        self.forks = {p["name"]: p for p in local if "third_party" in Path(p["manifest_path"]).parts}
        config = tomllib.loads(GROUPS.read_text(encoding="utf-8"))
        unpublished = set(config.pop("unpublished"))
        self.members = {short(p["name"]): p for p in local
                        if p["name"] not in self.forks and short(p["name"]) not in unpublished}
        self.groups = config
        self.group_of = {}
        for group, entry in config.items():
            for member in entry["members"]:
                if member not in self.members:
                    sys.exit(f"crates-io: {group} names {member}, which is not a published workspace crate")
                if member in self.group_of:
                    sys.exit(f"crates-io: {member} is in both {self.group_of[member]} and {group}")
                self.group_of[member] = group
        rest = sorted(set(self.members) - set(self.group_of))
        config["southstar-engine"]["members"] = rest
        for member in rest:
            self.group_of[member] = "southstar-engine"
        workspace = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
        self.lints = workspace["workspace"]["lints"]
        self.gates = self.optional_members()
        self.order = self.publish_order()

    def optional_members(self):
        uses = {}
        for member, package in self.members.items():
            for dep in package["dependencies"]:
                used = short(dep["name"])
                if dep["kind"] != "dev" and used in self.members and dep["name"] not in self.forks:
                    uses.setdefault(used, []).append((member, dep["optional"]))
        gates = {}
        for member, users in uses.items():
            group = self.group_of[member]
            inside = [(user, optional) for user, optional in users if self.group_of[user] == group]
            if not inside or not all(optional for _, optional in users):
                continue
            needle = f"dep:{self.members[member]['name']}"
            features = sorted({feature for user, _ in inside
                               for feature, values in self.members[user]["features"].items() if needle in values})
            if features:
                gates[member] = features
        return gates

    def standalone(self, group):
        members = self.groups[group]["members"]
        return len(members) == 1 and self.members[members[0]]["name"] == group

    def group_deps(self, group):
        deps = set()
        for member in self.groups[group]["members"]:
            for dep in self.members[member]["dependencies"]:
                target = self.group_of.get(short(dep["name"])) if dep["kind"] != "dev" else None
                if target and target != group and dep["name"] not in self.forks:
                    deps.add(target)
        return deps

    def publish_order(self):
        order, state = [], {}

        def visit(group, path):
            if state.get(group) == "open":
                sys.exit("crates-io: the groups form a cycle: " + " -> ".join(path + [group]))
            if state.get(group) == "done":
                return
            state[group] = "open"
            for dep in sorted(self.group_deps(group)):
                visit(dep, path + [group])
            state[group] = "done"
            order.append(group)

        for group in sorted(self.groups):
            visit(group, [])
        return order

    def path_to(self, crate, group):
        member = short(crate)
        target = self.group_of[member]
        if self.standalone(target):
            return f"::{ident(target)}" if target != group else "crate"
        module = ident(member)
        return f"crate::{module}" if target == group else f"::{ident(target)}::{module}"


def rewrite_source(text, plan, group, module):
    if module:
        text = SELF_PATH.sub(lambda m: f"{m.group(1)}crate::{module}::", text)

    def crate_ident(match):
        token = match.group(0).lstrip(":")
        name = token.replace("_", "-")
        start = match.start()
        if short(name) not in plan.members or (match.group(1) and start and IDENT_CHAR.match(text[start - 1])):
            return match.group(0)
        target = plan.path_to(name, group)
        rest = text[match.end():match.end() + 8]
        if re.match(r"\s+as\s+_\b", rest):
            owner = plan.group_of[short(name)]
            return "crate" if owner == group else f"::{ident(owner)}"
        if rest.startswith("::") or re.match(r"\s+as\b", rest):
            return target
        return f"{target} as {token}"

    return CRATE_IDENT.sub(crate_ident, text)


def copy_member(plan, group, member, into, module):
    source = Path(plan.members[member]["manifest_path"]).parent
    for path in source.rglob("*"):
        relative = path.relative_to(source)
        if path.is_dir() or relative.parts[0] in ("target", "bundle") or relative == Path("Cargo.toml"):
            continue
        target = into / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        if path.suffix == ".rs":
            text = rewrite_source(path.read_text(encoding="utf-8"), plan, group, module)
            if module and relative == Path("build.rs"):
                text = re.sub(r"^fn main\(\)", "pub fn main()", text, flags=re.M)
            target.write_text(text, encoding="utf-8")
        else:
            shutil.copy2(path, target)


def toml_value(value):
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, (int, float)):
        return str(value)
    if isinstance(value, str):
        return json.dumps(value, ensure_ascii=False)
    if isinstance(value, list):
        return "[" + ", ".join(toml_value(v) for v in value) + "]"
    return "{ " + ", ".join(f"{json.dumps(k)} = {toml_value(v)}" for k, v in value.items()) + " }"


def toml_table(name, table):
    lines = [f"[{name}]"]
    lines += [f"{json.dumps(k)} = {toml_value(v)}" for k, v in table.items()]
    return "\n".join(lines) + "\n"


def merge_dependencies(plan, group):
    merged = {}
    for member in plan.groups[group]["members"]:
        gate = plan.gates.get(member, [])
        for dep in plan.members[member]["dependencies"]:
            if dep["kind"] == "dev":
                continue
            name = dep["name"]
            internal = short(name) in plan.members and name not in plan.forks
            if internal:
                target = plan.group_of[short(name)]
                if target == group:
                    continue
                key, spec = target, {"version": f"={plan.version}", "path": f"../{target}"}
            elif name in plan.forks:
                key = dep["rename"] or name
                spec = {"version": f"={plan.forks[name]['version']}",
                        "path": str(Path(plan.forks[name]["manifest_path"]).parent.as_posix())}
                if dep["rename"]:
                    spec["package"] = name
            else:
                key = dep["rename"] or name
                spec = {"version": dep["req"]}
                if dep["rename"]:
                    spec["package"] = name
            slot = (dep["kind"] or "normal", dep["target"] or "", key)
            entry = merged.setdefault(slot, {"spec": spec, "features": set(), "default": False,
                                             "optional": True, "gates": set()})
            entry["features"].update(dep["features"])
            entry["default"] |= dep["uses_default_features"]
            entry["optional"] &= dep["optional"] or bool(gate)
            if gate and not dep["optional"]:
                entry["gates"].update(gate)
    return merged


def dependency_tables(merged):
    tables = {}
    for (kind, target, key), entry in sorted(merged.items()):
        spec = dict(entry["spec"])
        if entry["features"]:
            spec["features"] = sorted(entry["features"])
        if not entry["default"]:
            spec["default-features"] = False
        if entry["optional"]:
            spec["optional"] = True
        section = {"normal": "dependencies", "build": "build-dependencies"}[kind]
        if target:
            section = f"target.'{target}'.{section}"
        tables.setdefault(section, {})[key] = spec
    return tables


def merge_features(plan, group, merged):
    optional = {key: entry["optional"] for (kind, _, key), entry in merged.items() if kind == "normal"}
    features = {}
    for member in plan.groups[group]["members"]:
        for feature, values in plan.members[member]["features"].items():
            out = features.setdefault(feature, [])
            for value in values:
                mapped = map_feature_value(plan, group, value, optional)
                if mapped and mapped != feature and mapped not in out:
                    out.append(mapped)
    for (kind, _, key), entry in sorted(merged.items()):
        if entry["optional"]:
            for feature in sorted(entry["gates"]):
                if f"dep:{key}" not in features.setdefault(feature, []):
                    features[feature].append(f"dep:{key}")
    return features


def map_feature_value(plan, group, value, optional):
    if value.startswith("dep:"):
        key = value[4:]
        if short(key) in plan.members and key not in plan.forks:
            member, key = short(key), plan.group_of[short(key)]
            if key == group:
                return None
            if member in plan.gates:
                return f"{key}/{plan.gates[member][0]}"
        return f"dep:{key}" if optional.get(key) else None
    if "/" in value:
        key, feature = value.split("/", 1)
        weak = key.endswith("?")
        key = key.rstrip("?")
        if short(key) in plan.members and key not in plan.forks:
            key = plan.group_of[short(key)]
            if key == group:
                return feature
        marker = "?" if weak and optional.get(key) else ""
        return f"{key}{marker}/{feature}"
    return value


def licence(plan, group):
    licences = sorted({plan.members[m]["license"] for m in plan.groups[group]["members"]})
    return licences[0] if len(licences) == 1 else " AND ".join(f"({x})" for x in licences)


def manifest(plan, group, build_script, lib_path, bins, extra):
    first = plan.members[plan.groups[group]["members"][0]]
    package = {
        "name": group,
        "version": plan.version,
        "edition": first["edition"],
        "rust-version": first["rust_version"],
        "license": licence(plan, group),
        "description": plan.groups[group]["description"],
        "repository": first["repository"],
        "homepage": first["homepage"],
        "readme": "README.md",
    }
    package.update(extra)
    if build_script:
        package["build"] = build_script
    text = toml_table("package", package)
    if lib_path:
        text += "\n" + toml_table("lib", {"path": lib_path})
    for target in bins:
        text += "\n[[bin]]\n" + "\n".join(f"{json.dumps(k)} = {toml_value(v)}" for k, v in target.items()) + "\n"
    merged = merge_dependencies(plan, group)
    for section, deps in dependency_tables(merged).items():
        text += "\n" + toml_table(section, deps)
    features = merge_features(plan, group, merged)
    if features:
        text += "\n" + toml_table("features", features)
    for tool, lints in plan.lints.items():
        text += "\n" + toml_table(f"lints.{tool}", lints)
    return text


def write_readme(plan, group, into):
    if (into / "README.md").exists():
        return
    handwritten = READMES / f"{group}.md"
    if handwritten.exists():
        shutil.copy2(handwritten, into / "README.md")
        return
    members = plan.groups[group]["members"]
    lines = [f"# {group}", "", plan.groups[group]["description"] + ".", "",
             f"This crate is part of [Southstar]({REPOSITORY}), a web browser written from scratch in Rust "
             "with a GTK 4 interface. It is published so that `cargo install southstar` can build the "
             "browser; its API follows the browser's needs and is not stable.", ""]
    if not plan.standalone(group):
        lines += ["## Modules", "", "Each module is a crate of the Southstar workspace, merged into this one for publishing:", ""]
        lines += [f"- `{ident(m)}`: {plan.members[m]['description']}" for m in members]
        lines.append("")
    lines += ["## Install the browser", "", "```sh", "cargo install southstar", "```", "",
              f"The [southstar](https://crates.io/crates/southstar) crate lists the system libraries it needs.", "",
              "## License", "", f"`{licence(plan, group)}`. The license texts are in the "
              f"[repository]({REPOSITORY}).", ""]
    (into / "README.md").write_text("\n".join(lines), encoding="utf-8")


def write_wrapped(plan, group, into):
    members = plan.groups[group]["members"]
    lib = ["//! Southstar — " + plan.groups[group]["description"].removeprefix("Southstar's ") + ".",
           "//! Generated by scripts/crates-io.py from the workspace crates it names as modules.",
           f"//! SPDX-License-Identifier: {licence(plan, group)}", "",
           "#![allow(clashing_extern_declarations)]", ""]
    build_members = []
    for member in members:
        module = ident(member)
        package = plan.members[member]
        source = Path(package["manifest_path"]).parent
        copy_member(plan, group, member, into / "crates" / member, module)
        lib_target = next(t for t in package["targets"] if "lib" in t["kind"] or "staticlib" in t["kind"]
                          or "rlib" in t["kind"])
        relative = Path(lib_target["src_path"]).relative_to(source).as_posix()
        if member in plan.gates:
            lib.append("#[cfg(any(" + ", ".join(f'feature = "{f}"' for f in plan.gates[member]) + "))]")
        lib += [f'#[path = "../crates/{member}/{relative}"]', f"pub mod {module};"]
        if any("custom-build" in t["kind"] for t in package["targets"]):
            build_members.append(member)
    (into / "src").mkdir(parents=True, exist_ok=True)
    (into / "src" / "lib.rs").write_text("\n".join(lib) + "\n", encoding="utf-8")
    if build_members:
        script = ["//! Southstar — runs the build scripts of the crates merged into this one, each in its own directory.", ""]
        for member in build_members:
            script += [f'#[path = "crates/{member}/build.rs"]', f"mod {ident(member)};"]
        script += ["", "fn main() {", '    let root = std::env::current_dir().expect("the package directory");']
        for member in build_members:
            script += [f'    std::env::set_current_dir(root.join("crates/{member}")).expect("crates/{member}");',
                       f"    {ident(member)}::main();"]
        script += ['    std::env::set_current_dir(&root).expect("the package directory");', "}"]
        (into / "build.rs").write_text("\n".join(script) + "\n", encoding="utf-8")
    text = manifest(plan, group, "build.rs" if build_members else None, "src/lib.rs", [], {})
    (into / "Cargo.toml").write_text(text, encoding="utf-8")
    write_readme(plan, group, into)


def write_standalone(plan, group, into):
    member = plan.groups[group]["members"][0]
    package = plan.members[member]
    source = Path(package["manifest_path"]).parent
    copy_member(plan, group, member, into, None)
    original = tomllib.loads((source / "Cargo.toml").read_text(encoding="utf-8"))["package"]
    extra = {k: original[k] for k in ("keywords", "categories", "include") if k in original}
    lib_path, bins, build_script = None, [], None
    for target in package["targets"]:
        path = Path(target["src_path"]).relative_to(source).as_posix()
        if "bin" in target["kind"]:
            entry = {"name": target["name"], "path": path}
            if target.get("required-features"):
                entry["required-features"] = target["required-features"]
            bins.append(entry)
        elif "custom-build" in target["kind"]:
            build_script = path
        elif target["kind"][0] in ("lib", "rlib", "staticlib"):
            lib_path = path
    text = manifest(plan, group, build_script, lib_path, bins, extra)
    (into / "Cargo.toml").write_text(text, encoding="utf-8")
    write_readme(plan, group, into)
    if group == "southstar":
        for item in BUNDLED:
            source_item, target_item = ROOT / item, into / "bundle" / item
            target_item.parent.mkdir(parents=True, exist_ok=True)
            if source_item.is_dir():
                shutil.copytree(source_item, target_item)
            else:
                shutil.copy2(source_item, target_item)


def generate(plan, staging):
    shutil.rmtree(staging, ignore_errors=True)
    staging.mkdir(parents=True)
    for group in plan.order:
        if plan.standalone(group):
            write_standalone(plan, group, staging / group)
        else:
            write_wrapped(plan, group, staging / group)
    members = ", ".join(json.dumps(g) for g in plan.order)
    (staging / "Cargo.toml").write_text(f'[workspace]\nresolver = "3"\nmembers = [{members}]\n', encoding="utf-8")
    shutil.copy2(ROOT / "Cargo.lock", staging / "Cargo.lock")


def published(name, version):
    request = urllib.request.Request(f"https://crates.io/api/v1/crates/{name}/{version}",
                                     headers={"User-Agent": USER_AGENT})
    try:
        with urllib.request.urlopen(request, timeout=60):
            return True
    except urllib.error.HTTPError as error:
        if error.code == 404:
            return False
        raise


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


def publish_one(name, manifest_path):
    command = [CARGO, "publish", "--no-verify", "--allow-dirty", "--manifest-path", str(manifest_path)]
    while True:
        result = subprocess.run(command, cwd=manifest_path.parent, capture_output=True, text=True)
        sys.stdout.write(result.stdout)
        sys.stderr.write(result.stderr)
        if result.returncode == 0:
            return
        if "429" in result.stderr or "too many" in result.stderr.lower():
            wait = retry_delay(result.stderr)
            print(f"crates-io: rate limited on {name}; waiting {wait} s", flush=True)
            time.sleep(wait)
            continue
        sys.exit(f"crates-io: publishing {name} failed")


def fork_order(plan):
    order, done = [], set()

    def visit(name):
        if name in done:
            return
        done.add(name)
        for dep in plan.forks[name]["dependencies"]:
            if dep["kind"] != "dev" and dep["name"] in plan.forks:
                visit(dep["name"])
        order.append((name, plan.forks[name]))

    for name in sorted(plan.forks):
        visit(name)
    return order


def publish(plan, staging):
    queue = [(name, Path(p["manifest_path"]), p["version"]) for name, p in fork_order(plan)]
    queue += [(group, staging / group / "Cargo.toml", plan.version) for group in plan.order]
    for index, (name, manifest_path, version) in enumerate(queue, 1):
        if published(name, version):
            print(f"[{index}/{len(queue)}] {name} {version} is already on crates.io", flush=True)
            continue
        print(f"[{index}/{len(queue)}] publishing {name} {version}", flush=True)
        publish_one(name, manifest_path)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["list", "generate", "check", "publish"],
                        help="list the published crates in order; generate them; also cargo check them; or publish them")
    parser.add_argument("--staging", type=Path, default=STAGING, help="where to generate the crates")
    args = parser.parse_args()
    plan = Plan()
    if args.command == "list":
        for name, package in sorted(plan.forks.items()):
            print(f"{name} {package['version']}")
        for group in plan.order:
            print(f"{group} {plan.version}  ({', '.join(plan.groups[group]['members'])})")
        return
    generate(plan, args.staging)
    if args.command == "check":
        subprocess.run([CARGO, "check", "--manifest-path", str(args.staging / "Cargo.toml"), "-p", "southstar"],
                       check=True)
    elif args.command == "publish":
        publish(plan, args.staging)


if __name__ == "__main__":
    main()
