//! Southstar — links the system libraries, compiles the icon bundle and embeds the data files for a plain cargo build of the browser.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use southstar_build::{Library, engine_link, hardening_link_args, require};

fn out_dir() -> PathBuf {
    PathBuf::from(env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"))
}

fn source_root() -> PathBuf {
    let manifest =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR"));
    let bundle = manifest.join("bundle");
    if bundle.is_dir() {
        bundle
    } else {
        manifest.join("..").join("..")
    }
}

fn emit_link_args(target: &str, args: &[String]) {
    for arg in args {
        println!("cargo:{target}={arg}");
    }
}

const ENGINE_BINS: [&str; 4] = [
    "southstar",
    "southstar-renderer",
    "southstar-audio",
    "southstar-video",
];

fn link_libraries(target_os: &str) {
    let link = engine_link(target_os, true);
    let mut engine = link.engine;
    engine.extend(link.system);
    engine.extend(hardening_link_args(target_os));
    if !matches!(target_os, "windows" | "macos") {
        let script = out_dir().join("exe-local.map");
        fs::write(
            &script,
            "{
  local: *;
};
",
        )
        .expect("write exe-local.map");
        engine.push(format!("-Wl,--version-script={}", script.display()));
    }
    emit_link_args("rustc-link-arg-bin=southstar", &link.shell);
    for bin in ENGINE_BINS {
        emit_link_args(&format!("rustc-link-arg-bin={bin}"), &engine);
    }
}

fn compile_icons(root: &Path, gio: &Library) {
    let icons = root.join("data").join("icons");
    let manifest = icons.join("southstar-icons.gresource.xml");
    println!("cargo:rerun-if-changed={}", icons.display());
    let source = out_dir().join("southstar-icons.c");
    let compiler =
        env::var("GLIB_COMPILE_RESOURCES").unwrap_or_else(|_| "glib-compile-resources".to_owned());
    let status = Command::new(&compiler)
        .arg("--generate-source")
        .arg("--c-name=southstar_icons")
        .arg(format!("--sourcedir={}", icons.display()))
        .arg(format!("--target={}", source.display()))
        .arg(&manifest)
        .status()
        .unwrap_or_else(|err| panic!("southstar: cannot run {compiler}: {err}"));
    assert!(
        status.success(),
        "southstar: {compiler} failed on {}",
        manifest.display()
    );
    let mut build = cc::Build::new();
    build.file(&source).warnings(false);
    for flag in &gio.cflags {
        build.flag(flag);
    }
    build.cargo_metadata(false);
    for object in build.compile_intermediates() {
        println!("cargo:rustc-link-arg-bin=southstar={}", object.display());
    }
}

fn link_launcher() {
    for library in ["-lshell32", "-luser32", "-lkernel32"] {
        println!("cargo:rustc-link-arg-bin=southstar-launcher={library}");
    }
}

fn compile_windows_icon(root: &Path) {
    let icon = root.join("data").join("icons").join("southstar.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    let script = out_dir().join("southstar.rc");
    let object = out_dir().join("southstar-rc.o");
    fs::write(
        &script,
        format!(
            "1 ICON \"{}\"\n",
            icon.display().to_string().replace('\\', "/")
        ),
    )
    .expect("write southstar.rc");
    let windres = env::var("WINDRES").unwrap_or_else(|_| "windres".to_owned());
    let status = Command::new(&windres)
        .arg(&script)
        .arg("-O")
        .arg("coff")
        .arg("-o")
        .arg(&object)
        .status()
        .unwrap_or_else(|err| panic!("southstar: cannot run {windres}: {err}"));
    assert!(status.success(), "southstar: {windres} failed");
    println!("cargo:rustc-link-arg-bin=southstar={}", object.display());
    if southstar_build::feature_enabled("LAUNCHER") {
        println!(
            "cargo:rustc-link-arg-bin=southstar-launcher={}",
            object.display()
        );
    }
}

fn bundle_files(root: &Path) -> Vec<(String, PathBuf)> {
    let mut files: Vec<(String, PathBuf)> = ["License.md", "COPYING", "THIRD-PARTY-LICENSES.md"]
        .iter()
        .map(|name| ((*name).to_owned(), root.join(name)))
        .collect();
    files.push((
        "safebrowsing.list".to_owned(),
        root.join("data").join("safebrowsing.list"),
    ));
    let i18n = root.join("data").join("i18n");
    let mut catalogues: Vec<PathBuf> = fs::read_dir(&i18n)
        .unwrap_or_else(|err| panic!("southstar: cannot read {}: {err}", i18n.display()))
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "lang"))
        .collect();
    catalogues.sort();
    for path in catalogues {
        let name = path
            .file_name()
            .expect("a catalogue has a file name")
            .to_string_lossy();
        files.push((format!("i18n/{name}"), path.clone()));
    }
    files
}

fn write_bundle(root: &Path) {
    let mut code = String::from("&[\n");
    for (relative, path) in bundle_files(root) {
        println!("cargo:rerun-if-changed={}", path.display());
        let _ = writeln!(
            code,
            "    ({relative:?}, include_bytes!({:?})),",
            path.display().to_string()
        );
    }
    code.push(']');
    fs::write(out_dir().join("bundle.rs"), code).expect("write bundle.rs");
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let root = source_root();
    write_bundle(&root);
    compile_icons(&root, &require(&["gio-2.0"]));
    link_libraries(&target_os);
    if target_os == "windows" {
        compile_windows_icon(&root);
        link_launcher();
    }
}
