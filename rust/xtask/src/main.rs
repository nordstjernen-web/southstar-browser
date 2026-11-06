//! Southstar xtask — `cargo xtask build|run|install|features`: configures the build from the installed libraries and drives cargo.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![forbid(unsafe_code)]

mod detect;
mod install;
mod options;

use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command as Process, ExitCode};
use std::time::{SystemTime, UNIX_EPOCH};

use detect::Configuration;
use options::{Command, Lto, Options};

const CAPI_FEATURES: [&str; 13] = [
    "libav",
    "sdl",
    "http-brotli",
    "http-zstd",
    "woff2",
    "thai",
    "avif",
    "poppler",
    "enchant",
    "wasm",
    "rust-shaper",
    "webgpu",
    "font-fontconfig",
];

fn source_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the xtask crate sits two levels below the source root")
        .to_path_buf()
}

fn workspace_version(root: &Path) -> String {
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap_or_default();
    manifest
        .lines()
        .skip_while(|line| line.trim() != "[workspace.package]")
        .find_map(|line| line.trim().strip_prefix("version = "))
        .map(|version| version.trim_matches('"').to_owned())
        .unwrap_or_default()
}

fn today_utc() -> String {
    let days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() / 86_400);
    let days = i64::try_from(days).unwrap_or(0) + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

fn rustc_version() -> String {
    let rustc = env::var("RUSTC").unwrap_or_else(|_| "rustc".to_owned());
    Process::new(rustc)
        .arg("--version")
        .output()
        .ok()
        .and_then(|output| {
            String::from_utf8_lossy(&output.stdout)
                .split_whitespace()
                .nth(1)
                .map(str::to_owned)
        })
        .unwrap_or_default()
}

fn target_dir(options: &Options, root: &Path) -> PathBuf {
    options
        .target_dir
        .clone()
        .or_else(|| env::var_os("CARGO_TARGET_DIR").map(PathBuf::from))
        .unwrap_or_else(|| root.join("target"))
}

fn built_dir(options: &Options, root: &Path) -> PathBuf {
    target_dir(options, root).join(if options.debug { "debug" } else { "release" })
}

fn capi_features(configuration: &Configuration) -> Vec<String> {
    let mut features: Vec<String> = configuration
        .features
        .iter()
        .filter(|feature| CAPI_FEATURES.contains(feature))
        .map(|feature| format!("southstar-capi/{feature}"))
        .collect();
    let native = [
        ("vorbisfile", "audio-native-vorbis"),
        ("opusfile", "audio-native-opus"),
    ];
    for (feature, capi) in native {
        if configuration.features.contains(&feature) {
            features.push(format!("southstar-capi/{capi}"));
        }
    }
    match env::consts::OS {
        "linux" => features.extend([
            "southstar-capi/seccomp".to_owned(),
            "southstar-capi/egl".to_owned(),
        ]),
        "freebsd" | "netbsd" | "openbsd" => features.push("southstar-capi/egl".to_owned()),
        "windows" => features.push("southstar-capi/fontconfig".to_owned()),
        _ => {}
    }
    features
}

fn cargo_build(
    options: &Options,
    configuration: &Configuration,
    root: &Path,
) -> Result<(), String> {
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let mut features: Vec<String> = configuration
        .features
        .iter()
        .map(|feature| format!("southstar/{feature}"))
        .collect();
    features.extend(capi_features(configuration));
    let mut command = Process::new(cargo);
    command
        .current_dir(root)
        .args([
            "build",
            "--locked",
            "--package",
            "southstar",
            "--package",
            "southstar-capi",
        ])
        .arg("--no-default-features")
        .arg("--features")
        .arg(features.join(","))
        .arg("--target-dir")
        .arg(target_dir(options, root));
    if !options.debug {
        command.arg("--release");
    }
    let date = options.build_date.clone().unwrap_or_else(today_utc);
    command
        .env("NS_VERSION", workspace_version(root))
        .env("NS_BUILD_DATE", date)
        .env("NS_RUSTC_VERSION", rustc_version());
    if options.overflow_checks {
        command.env("CARGO_PROFILE_RELEASE_OVERFLOW_CHECKS", "true");
    }
    match options.lto {
        Lto::Fat => {}
        Lto::Thin => {
            command
                .env("CARGO_PROFILE_RELEASE_LTO", "thin")
                .env("CARGO_PROFILE_RELEASE_CODEGEN_UNITS", "16");
        }
        Lto::Off => {
            command
                .env("CARGO_PROFILE_RELEASE_LTO", "off")
                .env("CARGO_PROFILE_RELEASE_CODEGEN_UNITS", "256")
                .env("CARGO_PROFILE_RELEASE_INCREMENTAL", "true");
        }
    }
    if let Some(wgpu) = &configuration.wgpu_native_root {
        command.env("WGPU_NATIVE_ROOT", wgpu);
    }
    let status = command
        .status()
        .map_err(|err| format!("cannot run cargo: {err}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("cargo build failed".to_owned())
    }
}

fn run(options: &Options, root: &Path) -> Result<(), String> {
    let configuration = detect::configure(options)?;
    if options.command == Command::Features {
        println!("{}", configuration.features.join(","));
        return Ok(());
    }
    cargo_build(options, &configuration, root)?;
    let built = built_dir(options, root);
    match options.command {
        Command::Run => {
            let browser = built.join(format!("southstar{}", env::consts::EXE_SUFFIX));
            let status = Process::new(&browser)
                .args(&options.passthrough)
                .status()
                .map_err(|err| format!("cannot start {}: {err}", browser.display()))?;
            if !status.success() {
                return Err(format!("southstar exited with {status}"));
            }
        }
        Command::Install => {
            let prefix = options.prefix.strip_prefix("/").unwrap_or(&options.prefix);
            let target = options
                .destdir
                .as_ref()
                .map_or_else(|| options.prefix.clone(), |destdir| destdir.join(prefix));
            install::install(root, &built, &target)
                .map_err(|err| format!("install into {} failed: {err}", target.display()))?;
            eprintln!("southstar: installed into {}", target.display());
        }
        Command::Build | Command::Features => {
            eprintln!("southstar: built into {}", built.display());
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    let options = match options::parse(env::args().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    match run(&options, &source_root()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("southstar: {message}");
            ExitCode::FAILURE
        }
    }
}
