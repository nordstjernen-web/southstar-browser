# Southstar nightly builds — Ubuntu 24.04 build server

This document records how to run Southstar's nightly builds on an
Ubuntu 24.04 LTS server from a cron job. The orchestrator is
`scripts/nightly.sh`; the per-distro container build is
`scripts/nightly-distro-build.sh`. This guide is the operational
recipe around them.

## What the nightly produces

One run of `scripts/nightly.sh` writes straight into `$NIGHTLY_ROOT`
(default `/var/www/html/nightly`, so the artifacts are served from the
web server's document root). There is no per-date directory — each stage
builds into a scratch directory and replaces its published directory
(`source/`, `linux/<distro>/`, `windows/`, …) only once it holds
artifacts, so a stage that fails leaves the previous night's files in
place, with the failed `build.log` beside them, and the stable links
below keep resolving. `MANIFEST.txt` records the failure and that those
files are stale. The root contains:

| Artifact | Built by | Where |
| --- | --- | --- |
| Source tarballs (`.tar.gz`, `.tar.xz`) | `git archive` on the host | `source/` |
| Debian package (`.deb`) + binary zip | `debian:trixie` container | `linux/debian/` |
| Ubuntu package (`.deb`) + binary zip | `ubuntu:26.04` container | `linux/ubuntu/` |
| openSUSE package (`.rpm`) + binary zip | `opensuse/tumbleweed` container | `linux/opensuse/` |
| Alpine (musl) binary zip | `alpine:edge` container | `linux/alpine/` |
| Windows bundle (`.zip`) + `southstar.exe` | GitHub Actions `windows.yml` | `windows/` |
| macOS `.dmg` + binary | GitHub Actions `macos.yml` | `macos/` |
| FreeBSD portable zip | GitHub Actions `freebsd.yml` (vmactions VM) | `freebsd/` |
| NetBSD portable zip | GitHub Actions `netbsd.yml` (vmactions VM) | `netbsd/` |

Each glibc Linux artifact (Debian/Ubuntu/openSUSE zip, `.deb`, `.rpm`) also
ships **experimental WebGPU**: `scripts/pack-linux.sh` fetches the pinned
wgpu-native release with `scripts/fetch-wgpu-native.sh`, builds the `webgpu`
feature in, and bundles `libwgpu_native.so` beside the binaries (zip) or under
`/usr/lib/southstar` (deb/rpm) with an `$ORIGIN` rpath. WebGPU stays dormant
until the browser is started with `--enable-webgpu`. The Alpine (musl) build
skips it — wgpu-native ships no musl library — and the Windows/macOS/BSD builds
are WebGPU-free for now (the `webgpu` feature is `auto`, so it is simply not
built where wgpu-native is absent). Set `NS_WEBGPU=0` to force a WebGPU-free
Linux nightly, or `NS_WEBGPU=1` to make a missing wgpu-native fail the build
instead of degrading.

Plus `SHA256SUMS`, `MANIFEST.txt` (version, commit, per-stage status,
file sizes), and `nightly.log`.

### Stable download URLs

The artifact filenames embed the build version, but each run also drops
version-less symlinks beside them, so these URLs always resolve to the
newest successful build of that platform. A link is removed only when
nothing is left to point it at, which is a platform that has never
built. The portable Linux zip is taken from the Ubuntu build and falls
back to the Debian and then the openSUSE build:

- `https://<your-server>/nightly/southstar-windows-x86_64.zip`
- `https://<your-server>/nightly/southstar-windows-x86_64.exe`
- `https://<your-server>/nightly/southstar-macos.dmg` (Apple Silicon)
- `https://<your-server>/nightly/southstar-macos-arm64` (Apple Silicon, unbundled binary)
- `https://<your-server>/nightly/southstar-debian-amd64.deb`
- `https://<your-server>/nightly/southstar-ubuntu-amd64.deb`
- `https://<your-server>/nightly/southstar-opensuse-x86_64.rpm`
- `https://<your-server>/nightly/southstar-linux-x86_64.zip`
- `https://<your-server>/nightly/southstar-alpine-x86_64.zip` (musl)
- `https://<your-server>/nightly/southstar-freebsd-x86_64.zip`
- `https://<your-server>/nightly/southstar-netbsd-x86_64.zip`
- `https://<your-server>/nightly/southstar-src.tar.xz`
- `https://<your-server>/nightly/southstar-src.tar.gz`

(These are symlinks, so the web server must follow symlinks — nginx
does so by default.)

### Why this split

The Linux server cannot natively produce macOS binaries and cannot
practically cross-compile a GTK 4 Windows build. So the nightly uses
the project's existing GitHub Actions runners — which are real native
macOS and Windows machines — and downloads their artifacts. For the
commit it is building it first looks for the `windows.yml` / `macos.yml`
run that the *push to `main` already triggered* (waiting if it is still
in progress); only if no usable run exists does it dispatch a fresh one
(`NIGHTLY_GHA_DISPATCH=0` disables that fallback). This avoids
re-running CI and the `concurrency`-group cancellation that a redundant
dispatch would cause. The Linux
packages are built locally in distro containers so each binary links
against that distro's own glibc / GTK, and the source tarball is a
plain `git archive` (the engine — lexbor, quickjs, wuffs — is vendored
in-tree, so the tarball is buildable fully offline).

The FreeBSD and NetBSD builds run the same way as Windows/macOS —
driven through GitHub Actions — but GitHub has no native BSD runners, so
`freebsd.yml` / `netbsd.yml` boot a BSD guest inside the
Linux runner with the `vmactions/*-vm` actions, install the runtime
dependencies from that OS's package collection (`pkg` / `pkg_add` /
`pkgin`), build with meson, and `scripts/pack-bsd.sh` zips the binary plus
the sandboxed renderer and runtime data. These zips are portable but not
self-contained — GTK 4 and the other shared libraries come from the user's
system (the bundled `INSTALL.md` lists the packages). The BSD workflows are
`workflow_dispatch` + nightly `schedule` only (the VM boot makes them too
slow for every push/PR), and the syscall sandbox (seccomp) is Linux-only,
so it is compiled out on the BSDs.

**The build server itself needs no GTK / curl / meson toolchain.** All
compilation happens inside the distro containers or on the GitHub
runners. The host only needs git, a container engine, the GitHub CLI,
and standard coreutils.

## One-time setup

### 1. Packages

```sh
sudo apt update
sudo apt install -y git docker.io gh xz-utils gzip zip ca-certificates
sudo apt install -y nginx      # or apache2 — to serve /var/www/html
```

### 2. Container engine

Enable Docker and let the build user run it without sudo:

```sh
sudo systemctl enable --now docker
sudo usermod -aG docker "$USER"      # log out / back in for this to apply
docker run --rm hello-world          # verify
```

Podman works too — set `NS_DOCKER=podman`. The orchestrator mounts the
build tree with the `:z` SELinux relabel, which Docker on Ubuntu
(AppArmor, no SELinux) ignores harmlessly.

### 3. GitHub CLI authentication

The nightly calls `gh workflow run` and `gh run download`, so `gh` must
be authenticated as the cron user. Authenticate once interactively:

```sh
gh auth login          # choose github.com, HTTPS, paste a token or use the browser
gh auth status         # confirm
```

The token needs `repo` and `workflow` scopes. The credential is stored
in `~/.config/gh` and reused by cron (cron sets `$HOME`). Alternatively,
export `GH_TOKEN` in the cron environment — see the cron section.

If you only want the Linux + source artifacts and not the
GitHub-driven Windows/macOS builds, skip this step and pass
`--no-gha`.

### 4. Clone the repository and create the output root

```sh
git clone https://github.com/nordstjernen-web/southstar-browser.git \
    ~/southstar
sudo install -d -o "$USER" -g "$USER" /var/www/html/nightly
```

### 5. Smoke-test the orchestrator

Run the local stages once by hand before wiring cron:

```sh
cd ~/southstar
./scripts/nightly.sh --no-gha          # Linux + source only
```

Then test the full run including the GitHub-driven builds:

```sh
./scripts/nightly.sh
```

Inspect `/var/www/html/nightly/MANIFEST.txt` — every stage should
read `ok`.

## Cron

`nightly.sh` keeps itself current: before building it fast-forwards its
own checkout to `origin/main` and re-runs the updated script if that
moved `nightly.sh`, so improvements to the orchestrator are picked up
without a wrapper step (a dirty or diverged working tree is left
untouched — pass `--no-pull` or set `NIGHTLY_PULL=0` to disable). The
built *sources* still come from `origin/main` via `git archive`
independently. So the only thing the cron line must do is run from the
repo with a sane `PATH`.

Install a system cron fragment at `/etc/cron.d/southstar-nightly`
(replace `andreas` with the build user):

```cron
SHELL=/bin/bash
PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
MAILTO=andreas@example.com

# 02:30 every day: build the nightly (the script self-updates from main).
30 2 * * *  andreas  cd /home/andreas/southstar && ./scripts/nightly.sh >> /var/www/html/nightly/cron.log 2>&1
```

`nightly.sh` exits non-zero if any enabled stage failed, so with
`MAILTO` set cron emails you the run when something breaks. Each run's
full output is also in `/var/www/html/nightly/nightly.log`.

If you authenticate via a token instead of `gh auth login`, prepend it
to the command (keep the cron file `chmod 600`):

```cron
30 2 * * *  andreas  GH_TOKEN=ghp_xxx bash -lc 'cd /home/andreas/southstar && ./scripts/nightly.sh' >> /var/www/html/nightly/cron.log 2>&1
```

## Options and environment

`scripts/nightly.sh --help` lists the flags. The common knobs:

| Flag / env | Default | Meaning |
| --- | --- | --- |
| `--root DIR` / `NIGHTLY_ROOT` | `/var/www/html/nightly` | Output root. |
| `--ref REF` / `NIGHTLY_REF` | `origin/main` | Git ref to archive and build. |
| `--no-docker` | — | Skip the Linux container builds. |
| `--no-gha` | — | Skip the Windows/macOS GitHub Actions builds. |
| `--no-tarball` | — | Skip the source tarball. |
| `--no-pull` / `NIGHTLY_PULL` | `1` | Fast-forward the checkout to `origin/main` and re-exec before building; `0`/`--no-pull` to disable. |
| `NIGHTLY_PULL_BRANCH` | `main` | Branch the working tree is fast-forwarded to. |
| `NS_DOCKER` | `docker` | Container engine (`docker` or `podman`). |
| `NIGHTLY_GHA_TIMEOUT` | `4200` | Seconds to wait for each GitHub run. |
| `NIGHTLY_GHA_DISPATCH` | `1` | Dispatch a fresh run if none exists for the commit; set `0` to only reuse. |
| `NIGHTLY_DEBIAN_IMAGE` | `debian:trixie` | Override the Debian base image. |
| `NIGHTLY_UBUNTU_IMAGE` | `ubuntu:26.04` | Override the Ubuntu base image. |
| `NIGHTLY_OPENSUSE_IMAGE` | `opensuse/tumbleweed` | Override the openSUSE base image. |
| `NIGHTLY_ALPINE_IMAGE` | `alpine:edge` | Override the Alpine (musl) base image. |
| `NS_WEBGPU` | `auto` | WebGPU in the Linux builds: `auto` builds it when wgpu-native is fetched, `0` forces it off, `1` makes a missing wgpu-native fatal. Forwarded into the distro containers. |
| `WGPU_NATIVE_VERSION` | `v29.0.1.1` | wgpu-native release tag fetched for the WebGPU build. Forwarded into the distro containers. |

## Serving the artifacts

`$NIGHTLY_ROOT` defaults to `/var/www/html/nightly`, which is under the
default document root of both nginx and Apache on Ubuntu — so once a web
server is installed the build is reachable at `http://<server>/nightly/`
(see the stable per-platform URLs above). Enable a directory listing if
you want a browsable index:

```nginx
# /etc/nginx/sites-available/default — inside the server { } block
location /nightly/ {
    autoindex on;
}
```

For Apache, `Options +Indexes` on the directory does the same. If you
prefer a dedicated vhost, point its `root` at `/var/www/html/nightly`
(or wherever you set `NIGHTLY_ROOT`).

## Troubleshooting

- **`gh not authenticated` in the manifest** — cron couldn't find the
  `gh` credential. Confirm `gh auth status` works as the cron user, or
  set `GH_TOKEN`. Use `--no-gha` to build only the Linux + source
  artifacts.
- **`pull <image> failed`** — the Docker daemon isn't running or the
  user isn't in the `docker` group. `docker run --rm hello-world`
  should succeed as the cron user.
- **A container build dies mid-compile/link with no error** — the OOM
  killer. The in-container build of the large translation units
  (quickjs, wuffs) at `-O3`, and especially the whole-program LTO link
  under newer GCC, is memory-hungry. The helper mitigates this two
  ways: it caps `ninja` parallelism to roughly `MemGB / 2` jobs
  (`NS_BUILD_JOBS`) and disables LTO for the nightly packages
  (`NS_BUILD_LTO=false`) — nightly binaries are functionally identical,
  just slightly larger. Override either env if your box has plenty of
  RAM (`NS_BUILD_LTO=true NS_BUILD_JOBS=16`). Builds also run
  sequentially, one distro at a time, so don't launch a second heavy
  build alongside a nightly.
- **Debian build fails with `implicit declaration of
  gtk_file_dialog_*` / `gtk_css_provider_load_from_string`** — the
  Debian image's GTK 4 is too old (Southstar needs the GtkFileDialog
  / `load_from_string` APIs from GTK 4.10–4.12). The default image is
  `debian:trixie` (Debian 13), which ships a new enough GTK 4;
  `debian:12` (bookworm, GTK 4.8) cannot build the browser. If you
  override `NIGHTLY_DEBIAN_IMAGE`, pick trixie or newer. The container
  helper also installs a modern meson via pip, since older Debian meson
  predates the `c_std` fallback-list syntax the project uses.
- **Stale orchestrator** — `nightly.sh` fast-forwards its own checkout
  to `origin/main` and re-execs itself on each run, so script changes are
  picked up automatically. If the working tree is dirty or diverged the
  pull is skipped (you'll see a `warn:` line) and the on-disk script runs
  as-is — commit/stash or reset the tree, or run with `--no-pull` if that
  is intentional. The *built* sources always come from `origin/main` via
  `git archive`, independent of the working tree.
