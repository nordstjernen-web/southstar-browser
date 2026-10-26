// product.js: what this package installs: the Southstar Browser release, its build tools and the libraries its build requires.
'use strict';

const fs = require('fs');
const path = require('path');
const { version } = require('../package.json');

const RUST_MINIMUM = '1.91';
const rustupAccepted = process.platform === 'win32' ? [] : ['rustup'];
const rustNote = 'Debian 13 and Alpine 3.22 package a Rust older than 1.91: use rustup (https://rustup.rs) there, or rustc-1.91 and cargo-1.91 on Ubuntu 24.04';
const rustUpgrade = [
  `Rust ${RUST_MINIMUM} or newer is needed. Install it for your user with rustup, which also picks up the`,
  'toolchain pinned in rust-toolchain.toml:',
  '',
  "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh",
  '',
  'On Ubuntu 24.04 the packaged one works too: sudo apt install rustc-1.91 cargo-1.91',
];

function versionedRustDirectories() {
  try {
    return fs.readdirSync('/usr/lib')
      .map((name) => name.match(/^rust-1\.(\d+)$/))
      .filter((match) => match && Number(match[1]) >= 91)
      .sort((a, b) => Number(b[1]) - Number(a[1]))
      .map((match) => path.join('/usr/lib', match[0], 'bin'));
  } catch {
    return [];
  }
}

module.exports = {
  name: 'Southstar Browser',
  packageName: 'southstar-browser',
  envPrefix: 'SOUTHSTAR',
  licenseFiles: ['License.md', 'COPYING'],
  version,
  repo: 'nordstjernen-web/southstar-browser',
  sourceDirectory: `southstar-browser-${version}`,
  executable: process.platform === 'win32' ? 'bin/southstar.exe' : 'bin/southstar',
  mesonOptions: ['--buildtype=release'],
  windowsEnv: { CC: 'clang', CXX: 'clang++' },
  extraPath: () => (process.platform === 'linux' ? versionedRustDirectories().slice(0, 1) : []),
  supportedDistributions: 'Ubuntu 24.04, Debian 13, Fedora 40, openSUSE Tumbleweed, Arch Linux, Alpine 3.21 or newer',

  tools: [
    { label: 'meson', probe: ['meson'], minimum: '1.0',
      upgrade: ['A newer meson can be installed for your user with:', '', '  pipx install meson', '', 'or: python3 -m pip install --user meson'],
      apt: 'meson', dnf: 'meson', zypper: 'meson', pacman: 'meson', apk: 'meson', brew: 'meson', msys: 'meson' },
    { label: 'ninja', probe: ['ninja', 'samu'],
      apt: 'ninja-build', dnf: 'ninja-build', zypper: 'ninja', pacman: 'ninja', apk: 'samurai', brew: 'ninja', msys: 'ninja' },
    { label: 'pkg-config', probe: ['pkg-config', 'pkgconf'],
      apt: 'pkgconf', dnf: 'pkgconf', zypper: 'pkgconf', pacman: 'pkgconf', apk: 'pkgconf', brew: 'pkgconf', msys: 'pkgconf' },
    { label: 'python3', probe: ['python3', 'python'],
      apt: 'python3', dnf: 'python3', zypper: 'python3', pacman: 'python', apk: 'python3', brew: 'python', msys: 'python' },
    { label: 'tar', probe: ['tar'],
      apt: 'tar', dnf: 'tar', zypper: 'tar', pacman: 'tar', apk: 'tar', msysBase: 'tar' },
    { label: 'cargo', probe: ['cargo'], minimum: RUST_MINIMUM, satisfiedBy: rustupAccepted, note: rustNote, upgrade: rustUpgrade,
      apt: 'cargo', dnf: 'cargo', zypper: 'cargo', pacman: 'rust', apk: 'cargo', brew: 'rust', msys: 'rust' },
    { label: 'rustc', probe: ['rustc'], minimum: RUST_MINIMUM, satisfiedBy: rustupAccepted, note: rustNote, upgrade: rustUpgrade,
      apt: 'rustc', dnf: 'rust', zypper: 'rust', pacman: 'rust', apk: 'rust', brew: 'rust', msys: 'rust' },
    { label: 'a C compiler (gcc or clang)', probe: ['cc', 'gcc', 'clang'], compiler: true,
      apt: 'build-essential', dnf: 'gcc', zypper: 'gcc', pacman: 'base-devel', apk: 'build-base', msys: 'clang' },
    { label: 'a C++ compiler (g++ or clang++)', probe: ['c++', 'g++', 'clang++'], compiler: true,
      apt: 'build-essential', dnf: 'gcc-c++', zypper: 'gcc-c++', pacman: 'base-devel', apk: 'build-base', msys: 'clang' },
  ],

  required: [
    { module: 'glib-2.0', minimum: '2.80', apt: 'libglib2.0-dev', pacman: 'glib2', brew: 'glib', msys: 'glib2' },
    { module: 'gtk4', minimum: process.platform === 'win32' ? '4.22.1' : '4.14',
      apt: 'libgtk-4-dev', pacman: 'gtk4', brew: 'gtk4', msys: 'gtk4' },
    { module: 'epoxy', apt: 'libepoxy-dev', pacman: 'libepoxy', brew: 'libepoxy', msys: 'libepoxy' },
    { module: 'libcrypto', apt: 'libssl-dev', pacman: 'openssl', brew: 'openssl@3', msys: 'openssl' },
    { module: 'uchardet', apt: 'libuchardet-dev', pacman: 'uchardet', brew: 'uchardet', msys: 'uchardet' },
    { module: 'libpsl', apt: 'libpsl-dev', pacman: 'libpsl', brew: 'libpsl', msys: 'libpsl' },
    { module: 'sqlite3', apt: 'libsqlite3-dev', pacman: 'sqlite', brew: 'sqlite', msys: 'sqlite3' },
    { module: 'zlib', apt: 'zlib1g-dev', pacman: 'zlib', brew: 'zlib', msys: 'zlib' },
    { module: 'harfbuzz', minimum: '8.3', apt: 'libharfbuzz-dev', pacman: 'harfbuzz', brew: 'harfbuzz', msys: 'harfbuzz' },
    { module: 'fontconfig', minimum: '2.15', apt: 'libfontconfig-dev', pacman: 'fontconfig', brew: 'fontconfig', msys: 'fontconfig' },
    { module: 'freetype2', apt: 'libfreetype-dev', pacman: 'freetype2', brew: 'freetype', msys: 'freetype' },
    { module: 'cairo', minimum: '1.18', apt: 'libcairo2-dev', pacman: 'cairo', brew: 'cairo', msys: 'cairo' },
    { module: 'cairo-ft', minimum: '1.18', apt: 'libcairo2-dev', pacman: 'cairo', brew: 'cairo', msys: 'cairo' },
    { module: 'libwebp', apt: 'libwebp-dev', pacman: 'libwebp', brew: 'webp', msys: 'libwebp' },
    { module: 'libwebpdemux', apt: 'libwebp-dev', pacman: 'libwebp', brew: 'webp', msys: 'libwebp' },
    { module: 'libseccomp', linux: true, apt: 'libseccomp-dev', pacman: 'libseccomp' },
    ...[
      ['libavformat', '60'],
      ['libavcodec', '60'],
      ['libavutil', '58'],
      ['libswscale', '7'],
      ['libswresample', '4'],
    ].map(([module, minimum]) => ({
      module, minimum, platforms: ['linux', 'win32'], apt: `${module}-dev`, pacman: 'ffmpeg', msys: 'ffmpeg',
      note: 'FFmpeg 6.0 or newer; on Fedora/RHEL from RPM Fusion, on openSUSE from Packman',
    })),
  ],

  optional: [
    { module: 'libbrotlidec', feature: 'Brotli compression and WOFF2 fonts', apt: 'libbrotli-dev', pacman: 'brotli', brew: 'brotli', msys: 'brotli' },
    { module: 'libzstd', feature: 'zstd compression', apt: 'libzstd-dev', pacman: 'zstd', brew: 'zstd', msys: 'zstd' },
    { module: 'sdl2', feature: 'audio playback', apt: 'libsdl2-dev', pacman: 'sdl2', brew: 'sdl2', msys: 'SDL2' },
    { module: 'poppler-glib', feature: 'viewing PDF files', apt: 'libpoppler-glib-dev', pacman: 'poppler-glib', brew: 'poppler', msys: 'poppler' },
    { module: 'opusfile', feature: 'Ogg Opus audio', apt: 'libopusfile-dev', pacman: 'opusfile', brew: 'opusfile', msys: 'opusfile' },
    { module: 'vorbisfile', feature: 'Ogg Vorbis audio', apt: 'libvorbis-dev', pacman: 'libvorbis', brew: 'libvorbis', msys: 'libvorbis' },
    { module: 'libavif', feature: 'AVIF images', apt: 'libavif-dev', pacman: 'libavif', brew: 'libavif', msys: 'libavif' },
    { module: 'enchant-2', feature: 'spell-checking', apt: 'libenchant-2-dev', pacman: 'enchant', brew: 'enchant', msys: 'enchant' },
    { module: 'libthai', minimum: '0.1.9', unix: true, feature: 'Thai word breaking', apt: 'libthai-dev', pacman: 'libthai', brew: 'libthai' },
    { module: 'libavcodec', minimum: '60', mac: true, feature: 'WebM video', brew: 'ffmpeg' },
  ],
};
