python = import('python').find_installation()

southstar_engine_sources = []

southstar_win_resources = []
if host_machine.system() == 'windows'
  windows = import('windows')
  southstar_win_resources = windows.compile_resources(
    'southstar.rc',
    depend_files: files('../data/icons/southstar.ico'),
  )
endif

southstar_c_deps = [
  glib,
  gobject,
  gio,
  cairo,
  pango,
  pangocairo,
  epoxy,
  gmodule,
  libcrypto,
  libm,
  libwebp,
  libwebpdemux,
  libuchardet,
  sqlite3,
  zlib,
]

if have_seccomp
  southstar_c_deps += [libseccomp]
endif

if have_poppler
  southstar_c_deps += [libpoppler]
endif

if have_avif
  southstar_c_deps += [libavif]
endif

if have_libav
  southstar_c_deps += [libavformat, libavcodec, libavutil, libswscale]
endif

if have_fontconfig
  southstar_c_deps += [libfontconfig]
endif

if have_pangoft2
  southstar_c_deps += [libpangoft2]
endif

if have_freetype
  southstar_c_deps += [libfreetype]
endif

southstar_c_deps += [libpsl]

southstar_c_deps += http_backend_deps

# macOS WebGL: src/glctx.c creates the offscreen GL context through CGL, whose
# symbols live in the OpenGL framework. Unused targets drop it via
# -dead_strip_dylibs.
if host_machine.system() == 'darwin'
  southstar_c_deps += [dependency('appleframeworks', modules: ['OpenGL'])]
endif

sdl2_dep = dependency('sdl2', required: get_option('audio'))
ns_lib_c_args = []
ns_sdl_deps = []
if sdl2_dep.found()
  ns_sdl_deps += [sdl2_dep]
  ns_lib_c_args += ['-DNS_HAVE_SDL']
endif

southstar_deps = [southstar_rust_dep] + southstar_c_deps

southstar_win_subsystem = (
  host_machine.system() == 'windows' ? 'windows' : 'console'
)

southstar_link_args = []
if host_machine.system() == 'windows'
  southstar_link_args += cc.get_supported_link_arguments(
    ['-Wl,--stack,8388608'],
  )
  southstar_link_args += ['-lbcrypt', '-ladvapi32', '-lshell32', '-luser32']
endif

# NetBSD pkgsrc libunistring (pulled in transitively through the libpsl /
# libidn2 stack) carries an undefined reference to libc's
# mbrtoc32, which the hardened --no-undefined link rejects even though the
# dynamic linker resolves it at runtime. Allow undefined symbols originating
# from shared libraries on NetBSD only; our own objects are still checked.
# Every binary that links that stack needs this — the executables below and
# the audio/video helpers.
if host_machine.system() == 'netbsd'
  southstar_link_args += cc.get_supported_link_arguments(
    ['-Wl,--allow-shlib-undefined'],
  )
endif

# The gmodule-2.0 dependency injects -Wl,--export-dynamic, which dumps every
# internal engine symbol (~6000 ns_* symbols) into the executables'
# .dynsym. Combined with -Wl,-z,now that bloats the dynamic-linker symbol
# resolution done on every process start — and the renderer is forked+exec'd
# afresh, so it pays that cost on each cold start. A version script localises
# the executables' own symbols (it overrides --export-dynamic regardless of
# link order); the only runtime dlsym we need (RAND_bytes for RNG warmup)
# resolves through a dependency library, whose exports this leaves untouched.
# Applies to the executables only; the shared libsouthstar keeps its API.
southstar_exe_link_args = southstar_link_args + \
  cc.get_supported_link_arguments(
    ['-Wl,--version-script=' + meson.current_source_dir() / 'exe-local.map'])

if host_machine.system() == 'windows'
  executable(
    'southstar-launcher',
    southstar_win_resources,
    link_with: southstar_launcher_rust_lib,
    win_subsystem: 'windows',
    link_args: rust_native_libs + ['-luser32', '-lshell32'],
  )
endif

southstar_embed_sources = southstar_engine_sources

# The engine is Rust, so nothing in southstar_embed_sources references it: link
# the whole static library and export exactly what libsouthstar.h declares.
libsouthstar_map = custom_target(
  'libsouthstar-map',
  input: 'libsouthstar.h',
  output: 'libsouthstar.map',
  command: [
    python,
    meson.project_source_root() / 'scripts' / 'gen-libsouthstar-map.py',
    '@INPUT@',
    '@OUTPUT@',
  ],
)
libsouthstar_api_link_args = []
if cc.has_link_argument('-Wl,--version-script=' + meson.current_source_dir() / 'exe-local.map')
  libsouthstar_api_link_args += ['-Wl,--version-script=' + libsouthstar_map.full_path()]
endif
if host_machine.system() == 'windows'
  libsouthstar_api_link_args += ['-Wl,--allow-multiple-definition']
endif

libsouthstar_lib = library(
  'southstar',
  southstar_embed_sources,
  dependencies: southstar_deps + ns_sdl_deps,
  c_args: ns_lib_c_args,
  link_whole: southstar_rust_lib,
  link_args: southstar_link_args + libsouthstar_api_link_args,
  link_depends: libsouthstar_map,
  install: true,
)

# A static variant of the engine so the out-of-process renderer links it in
# and ships as a single self-contained executable (no libsouthstar.so
# runtime dependency to bundle in distro packages).
libsouthstar_static = static_library(
  'southstar-static',
  southstar_embed_sources,
  dependencies: southstar_deps + ns_sdl_deps,
  c_args: ns_lib_c_args,
  install: false,
)

install_headers('libsouthstar.h', subdir: 'southstar')

# The out-of-process renderer: the engine served over the IPC control channel
# and shared-memory framebuffer. Its entry point is ns_renderer_main in the
# Rust library, which the shell links too and which therefore cannot export a
# symbol called main; the linker makes main an alias of it for this
# executable only.
rproc_deps = []
if host_machine.system() != 'windows'
  rproc_deps += [cc.find_library('rt', required: false)]
endif

if host_machine.system() == 'darwin'
  renderer_entry_args = ['-Wl,-alias,_ns_renderer_main,_main',
                         '-Wl,-u,_ns_renderer_main']
else
  renderer_entry_args = ['-Wl,--defsym=main=ns_renderer_main',
                         '-Wl,--undefined=ns_renderer_main']
endif

executable(
  'southstar-renderer',
  link_with: libsouthstar_static,
  dependencies: southstar_deps + ns_sdl_deps + rproc_deps,
  link_args: southstar_exe_link_args + renderer_entry_args,
  win_subsystem: 'windows',
  install: true,
)

sdl2_dep = dependency('sdl2', required: get_option('audio'))
if sdl2_dep.found()
  audio_deps = [sdl2_dep, glib] + http_backend_deps
  if have_seccomp
    audio_deps += [libseccomp]
  endif
  if have_libav
    audio_deps += [libavformat, libavcodec, libavutil, libswresample]
  endif
  if libvorbisfile.found()
    audio_deps += [libvorbisfile]
  endif
  if libopusfile.found()
    audio_deps += [libopusfile]
  endif
  # libm is linked for the Rust helper's math on Linux and the BSDs; it is part of
  # the C runtime on macOS and Windows, where find_library('m') is a harmless
  # no-op. librt carries shm_open/shm_unlink (the audio clock's POSIX shared
  # memory) on the BSDs, where they live outside libc; it is a harmless no-op
  # where absent, matching the renderer and video-helper link lines.
  audio_lib_deps = [cc.find_library('m', required: false),
                    cc.find_library('rt', required: false)]

  # The helper is rust/audio-helper, whose static library exports main; on
  # Linux it confines itself with the same Landlock + seccomp profile as the
  # renderer once its audio device and network stack are up, so
  # attacker-controlled codec bytes are decoded inside the sandbox.
  executable(
    'southstar-audio',
    link_whole: southstar_audio_rust_lib,
    dependencies: audio_deps + audio_lib_deps,
    link_args: southstar_link_args + rust_native_libs,
    win_subsystem: 'windows',
    install: true,
  )
else
  message('SDL2 not found — audio playback helper (southstar-audio) disabled')
endif

if have_libav
  videoproc_deps = [libavformat, libavcodec, libavutil, libswscale]
  if host_machine.system() == 'linux'
    videoproc_deps += [glib] + http_backend_deps + (have_seccomp ? [libseccomp] : [])
  endif
  executable(
    'southstar-video',
    link_whole: southstar_video_rust_lib,
    link_language: 'c',
    dependencies: videoproc_deps + [cc.find_library('rt', required: false),
                                    cc.find_library('m', required: false)],
    link_args: southstar_link_args + rust_native_libs,
    win_subsystem: 'windows',
    install: true,
  )
endif

if have_gtk
  subdir('gtk')
endif

if not have_gtk
  message('GTK 4 shell disabled — building the engine, out-of-process renderer' +
          ' and embeddable library without GTK. WebGL is unavailable in this' +
          ' configuration (its GL context is GTK-backed).')
endif
