//! Southstar xtask — the command line: the subcommand, the build options and the arguments handed to the browser.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::path::PathBuf;

pub const USAGE: &str =
    "usage: cargo xtask <build|run|install|features> [options] [-- browser arguments]

options:
  --debug                     build the dev profile (no LTO, overflow checks)
  --lto fat|thin|off          link-time optimization of a release build (default fat)
  --overflow-checks           integer overflow checks in a release build
  --audio auto|enabled|disabled
  --avif auto|enabled|disabled
  --webgpu auto|enabled|disabled
  --wgpu-native-root DIR      an extracted wgpu-native release
  --no-wasm                   leave out the WebAssembly API
  --no-rust-shaper            leave out the pure-Rust font path
  --build-date YYYY-MM-DD     the date the About page shows (default today, UTC)
  --target-dir DIR            cargo's target directory
  --prefix DIR                install prefix (default /usr/local)
  --libdir DIR                library directory under the prefix (default lib)
  --destdir DIR               staging root prepended to the prefix";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Toggle {
    Auto,
    Enabled,
    Disabled,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Lto {
    Fat,
    Thin,
    Off,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Build,
    Run,
    Install,
    Features,
}

pub struct Options {
    pub command: Command,
    pub debug: bool,
    pub lto: Lto,
    pub overflow_checks: bool,
    pub audio: Toggle,
    pub avif: Toggle,
    pub webgpu: Toggle,
    pub wgpu_native_root: Option<PathBuf>,
    pub wasm: bool,
    pub rust_shaper: bool,
    pub build_date: Option<String>,
    pub target_dir: Option<PathBuf>,
    pub prefix: PathBuf,
    pub libdir: PathBuf,
    pub destdir: Option<PathBuf>,
    pub passthrough: Vec<String>,
}

fn toggle(value: &str) -> Result<Toggle, String> {
    match value {
        "auto" => Ok(Toggle::Auto),
        "enabled" => Ok(Toggle::Enabled),
        "disabled" => Ok(Toggle::Disabled),
        other => Err(format!("expected auto, enabled or disabled, not {other}")),
    }
}

fn lto(value: &str) -> Result<Lto, String> {
    match value {
        "fat" => Ok(Lto::Fat),
        "thin" => Ok(Lto::Thin),
        "off" => Ok(Lto::Off),
        other => Err(format!("expected fat, thin or off, not {other}")),
    }
}

pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let mut args = args.into_iter();
    let command = match args.next().as_deref() {
        Some("build") => Command::Build,
        Some("run") => Command::Run,
        Some("install") => Command::Install,
        Some("features") => Command::Features,
        _ => return Err(USAGE.to_owned()),
    };
    let mut options = Options {
        command,
        debug: false,
        lto: Lto::Fat,
        overflow_checks: false,
        audio: Toggle::Auto,
        avif: Toggle::Auto,
        webgpu: Toggle::Auto,
        wgpu_native_root: None,
        wasm: true,
        rust_shaper: true,
        build_date: None,
        target_dir: None,
        prefix: PathBuf::from("/usr/local"),
        libdir: PathBuf::from("lib"),
        destdir: None,
        passthrough: Vec::new(),
    };
    while let Some(arg) = args.next() {
        let (name, inline) = match arg.split_once('=') {
            Some((name, value)) if name.starts_with("--") => {
                (name.to_owned(), Some(value.to_owned()))
            }
            _ => (arg.clone(), None),
        };
        let mut value = || {
            inline
                .clone()
                .or_else(|| args.next())
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match name.as_str() {
            "--" => {
                options.passthrough.extend(args.by_ref());
            }
            "--debug" => options.debug = true,
            "--release" => options.debug = false,
            "--lto" => options.lto = lto(&value()?)?,
            "--overflow-checks" => options.overflow_checks = true,
            "--audio" => options.audio = toggle(&value()?)?,
            "--avif" => options.avif = toggle(&value()?)?,
            "--webgpu" => options.webgpu = toggle(&value()?)?,
            "--wgpu-native-root" => options.wgpu_native_root = Some(PathBuf::from(value()?)),
            "--no-wasm" => options.wasm = false,
            "--no-rust-shaper" => options.rust_shaper = false,
            "--build-date" => options.build_date = Some(value()?),
            "--target-dir" => options.target_dir = Some(PathBuf::from(value()?)),
            "--prefix" => options.prefix = PathBuf::from(value()?),
            "--libdir" => options.libdir = PathBuf::from(value()?),
            "--destdir" => options.destdir = Some(PathBuf::from(value()?)),
            _ => return Err(format!("unknown option {arg}\n\n{USAGE}")),
        }
    }
    Ok(options)
}
