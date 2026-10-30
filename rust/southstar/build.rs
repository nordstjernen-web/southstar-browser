//! Southstar — links the system libraries, compiles the icon bundle and embeds the data files for a plain cargo build of the browser.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use southstar_build::{Library, link_args, require};

const ENGINE_LIBRARIES: [&str; 17] = [
    "glib-2.0",
    "gobject-2.0",
    "gio-2.0",
    "gmodule-2.0",
    "cairo",
    "cairo-ft",
    "harfbuzz",
    "fontconfig",
    "freetype2",
    "epoxy",
    "libcrypto",
    "zlib",
    "uchardet",
    "libpsl",
    "sqlite3",
    "libwebp",
    "libwebpdemux",
];

const FEATURE_LIBRARIES: [(&str, &[&str]); 11] = [
    (
        "LIBAV",
        &[
            "libavformat",
            "libavcodec",
            "libavutil",
            "libswscale",
            "libswresample",
        ],
    ),
    ("SDL", &["sdl2"]),
    ("HTTP_BROTLI", &["libbrotlidec"]),
    ("HTTP_ZSTD", &["libzstd"]),
    ("WOFF2", &["libbrotlidec"]),
    ("THAI", &["libthai"]),
    ("AVIF", &["libavif"]),
    ("POPPLER", &["poppler-glib"]),
    ("ENCHANT", &["enchant-2"]),
    ("VORBISFILE", &["vorbisfile"]),
    ("OPUSFILE", &["opusfile"]),
];

fn feature(name: &str) -> bool {
    env::var_os(format!("CARGO_FEATURE_{name}")).is_some()
}

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

fn link_libraries(target_os: &str) {
    let mut engine = ENGINE_LIBRARIES.to_vec();
    if target_os == "linux" {
        engine.push("libseccomp");
    }
    for (name, modules) in FEATURE_LIBRARIES {
        if feature(name) {
            engine.extend(modules);
        }
    }
    let engine_library = require(&engine);
    let engine_args: Vec<String> = link_args(&engine_library)
        .into_iter()
        .filter(|arg| arg != "-lSDL2main" && arg != "-lmingw32")
        .collect();
    let shell_args: Vec<String> = link_args(&require(&["gtk4"]))
        .into_iter()
        .filter(|arg| !engine_args.contains(arg))
        .collect();
    emit_link_args("rustc-link-arg-bin=southstar", &shell_args);
    emit_link_args("rustc-link-arg-bins", &engine_args);
    let system: &[&str] = match target_os {
        "windows" => &[
            "-Wl,--stack,8388608",
            "-lcrypt32",
            "-lbcrypt",
            "-ladvapi32",
            "-lshell32",
            "-luser32",
        ],
        "macos" => &["-framework", "OpenGL", "-framework", "AppKit", "-lobjc"],
        _ => &["-lrt", "-lm"],
    };
    let system: Vec<String> = system.iter().map(|arg| (*arg).to_owned()).collect();
    emit_link_args("rustc-link-arg-bins", &system);
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
    build
        .link_lib_modifier("+whole-archive")
        .compile("southstar_icons");
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
    link_libraries(&target_os);
    compile_icons(&root, &require(&["gio-2.0"]));
    if target_os == "windows" {
        compile_windows_icon(&root);
    }
}
