//! Southstar build — build-script helpers for a plain cargo build: system libraries through pkg-config, and C layouts probed from installed headers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![forbid(unsafe_code)]

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

pub struct Library {
    pub cflags: Vec<String>,
    pub libs: Vec<String>,
}

pub fn meson_build() -> bool {
    println!("cargo:rerun-if-env-changed=NS_VERSION");
    env::var_os("NS_VERSION").is_some()
}

pub fn pkg_config(modules: &[&str]) -> Option<Library> {
    let pkg_config = env::var("PKG_CONFIG").unwrap_or_else(|_| "pkg-config".to_owned());
    println!("cargo:rerun-if-env-changed=PKG_CONFIG");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");
    let query = |flag: &str| -> Option<Vec<String>> {
        let output = Command::new(&pkg_config)
            .arg(flag)
            .args(modules)
            .output()
            .ok()?;
        output.status.success().then(|| {
            String::from_utf8_lossy(&output.stdout)
                .split_whitespace()
                .map(str::to_owned)
                .collect()
        })
    };
    Some(Library {
        cflags: query("--cflags")?,
        libs: query("--libs")?,
    })
}

pub fn link_args(library: &Library) -> Vec<String> {
    let mut args = Vec::new();
    let mut flags = library.libs.iter();
    while let Some(flag) = flags.next() {
        if flag == "-framework" {
            if let Some(name) = flags.next() {
                args.extend(["-framework".to_owned(), name.clone()]);
            }
        } else if let Some(dir) = flag.strip_prefix("-L") {
            println!("cargo:rustc-link-search=native={dir}");
        } else if flag.starts_with("-l") && !args.contains(flag) {
            args.push(flag.clone());
        }
    }
    args
}

pub fn require(modules: &[&str]) -> Library {
    pkg_config(modules).unwrap_or_else(|| {
        panic!(
            "southstar: pkg-config cannot find {}; install its development package",
            modules.join(" ")
        )
    })
}

fn out_dir() -> PathBuf {
    PathBuf::from(env::var_os("OUT_DIR").expect("cargo sets OUT_DIR for build scripts"))
}

pub fn layout(list: &str, library: &Library) -> Option<String> {
    let mut source = String::from("#include <stdio.h>\n");
    let mut prints = String::new();
    for line in list.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if line.starts_with('#') {
            source.push_str(line);
            source.push('\n');
        } else if let Some((key, expr)) = line.split_once('=') {
            let separator = if prints.is_empty() { "" } else { "," };
            prints.push_str(&format!(
                "    printf(\"{separator}{key}=%lld\", (long long)({expr}));\n"
            ));
        }
    }
    source.push_str("int main(void)\n{\n");
    source.push_str(&prints);
    source.push_str("    return 0;\n}\n");
    let dir = out_dir();
    let stem = format!("layout-{:016x}", fnv(list.as_bytes()));
    let c_file = dir.join(format!("{stem}.c"));
    let exe = dir.join(format!("{stem}{}", env::consts::EXE_SUFFIX));
    fs::write(&c_file, source).ok()?;
    let compiler = cc::Build::new().cargo_metadata(false).get_compiler();
    let compiled = compiler
        .to_command()
        .args(&library.cflags)
        .arg(&c_file)
        .arg("-o")
        .arg(&exe)
        .status()
        .ok()?;
    if !compiled.success() {
        return None;
    }
    let output = Command::new(&exe).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}
