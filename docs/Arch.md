# Southstar on Arch Linux — packaging and the AUR

The Arch package recipe is `packaging/arch/PKGBUILD`, with its generated
`packaging/arch/.SRCINFO`. The package is named `southstar-browser`; the
command it installs is `southstar`. It builds a tagged release tarball with
`arch-meson`, the same way the Debian and openSUSE packages build it. For
plain build-from-source instructions see [Linux.md](Linux.md).

## Build the package

```sh
cd packaging/arch
makepkg -si                 # build and install
extra-x86_64-build          # or: clean-chroot build (devtools), as Arch builds it
namcap PKGBUILD *.pkg.tar.zst
```

The package:

- fetches the crates `Cargo.lock` pins with `cargo fetch --locked` in
  `prepare()` and builds with `CARGO_NET_OFFLINE=true`, as the Arch Rust
  package guidelines describe. It uses the Arch `rust` package; the
  `rust-toolchain.toml` pin applies only to rustup.
- builds the Rust engine with line-table debug info instead of makepkg's
  `-C debuginfo=2`. The engine is a single fat-LTO staticlib, and its link
  with full debug info is killed for lack of memory on an 8 GB machine.
- passes `-Dwebgpu=disabled`. wgpu-native is not in the Arch repositories
  (only in the AUR), and meson would otherwise look for a prebuilt release.
- builds the C objects that cargo compiles (ring's) with
  `-ffat-lto-objects`. makepkg enables GCC LTO, and the archive rustc writes
  for the staticlib has no symbol index for LTO bytecode, so without machine
  code in those objects the final link cannot resolve them.
- requires AVIF and the audio helper explicitly, so a missing dependency
  fails the build instead of dropping a feature.
- drops `libsouthstar.so` and its header (the embedding API, unversioned and
  unused by the browser).
- declares `license=('LicenseRef-NSL-1.0 OR GPL-3.0-or-later')` and links
  `License.md` and `COPYING` into `/usr/share/licenses/southstar-browser/`,
  as Arch requires for a `LicenseRef-` license.

## Submit it to the AUR

To publish it, use an AUR account with an SSH key:

```sh
git clone ssh://aur@aur.archlinux.org/southstar-browser.git aur-southstar-browser
cp packaging/arch/PKGBUILD packaging/arch/.SRCINFO aur-southstar-browser/
cd aur-southstar-browser
git add PKGBUILD .SRCINFO
git commit -m "Initial upload: southstar-browser 1.0.1"
git push
```

For each release, set `pkgver`, reset `pkgrel=1`, run `updpkgsums`, then
`makepkg --printsrcinfo > .SRCINFO`, and commit both files here and in the
AUR repository.

## Tested on

Release 1.0.1 was built with `makepkg -s` in a fresh `archlinux:base-devel`
podman container (GCC 16, Rust 1.99, FFmpeg 9, GTK 4.24) without compiler
warnings. `namcap` reports nothing for the package; for
`southstar-browser-debug` it flags the build-id symlinks into `/usr/bin`, which
is expected for every debug package. The package installs alongside
`nordstjernen-browser` and `northstar-browser` without file conflicts, and the
installed browser loads an HTTPS page under Xvfb.
