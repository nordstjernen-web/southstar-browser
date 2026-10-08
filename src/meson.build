python = import('python').find_installation()

qjsc_prog = find_program('qjsc', required: false)
qjs_prog  = find_program('qjs',  required: false)

if not meson.is_cross_build() and not quickjs_original and qjsc_prog.found() and qjs_prog.found()
  # files() so the script path is absolute — a raw '../scripts/...' string would
  # be resolved relative to the build dir and break out-of-tree builds.
  verify_polyfills_py = files('../scripts/verify-polyfills.py')
  polyfills_verify = custom_target(
    'polyfills-verify',
    input: ['../data/js/polyfills.js', '../scripts/verify-polyfills.js'],
    output: 'polyfills.verified',
    command: [
      python, verify_polyfills_py,
      qjsc_prog, qjs_prog, '@INPUT0@', '@INPUT1@', '@OUTPUT@',
    ],
    build_by_default: true,
  )
endif

polyfills_h = custom_target(
  'polyfills.h',
  input: ['../scripts/embed-text.py', '../data/js/polyfills.js'],
  output: 'polyfills.h',
  command: [python, '@INPUT0@', '@INPUT1@', '@OUTPUT@',
            'ns_js_polyfills_src'],
)

streaming_h = custom_target(
  'streaming.h',
  input: ['../scripts/embed-text.py', '../data/js/streaming.js'],
  output: 'streaming.h',
  command: [python, '@INPUT0@', '@INPUT1@', '@OUTPUT@',
            'ns_js_streaming_src'],
)

wpt_hook_h = custom_target(
  'wpt_hook.h',
  input: ['../scripts/embed-text.py', '../data/js/wpt-hook.js'],
  output: 'wpt_hook.h',
  command: [python, '@INPUT0@', '@INPUT1@', '@OUTPUT@',
            'ns_wpt_hook_src'],
)

southstar_engine_sources = files(
  'anim.c',
  'cache.c',
  'css.c',
  'css_media.c',
  'dom.c',
  'engine.c',
  'eventsource.c',
  'forms.c',
  'camera.c',
  'mic.c',
  'font.c',
  'html.c',
  'image.c',
  'svg.c',
  'image_wuffs.c',
  'layout.c',
  'mathml.c',
  'net.c',
  'netutil.c',
  'paint.c',
  'print.c',
  'pdf.c',
  'render.c',
  'selection.c',
  'texture.c',
  'video.c',
  'video_decode.c',
  'ws.c',
  'html_lexbor.c',
  'xml.c',
)

southstar_engine_sources += files(
  'ext.c',
  'idb.c',
  'js.c',
  'js_brand.c',
  'js_canvas.c',
  'js_canvas_api.c',
  'js_intl.c',
  'js_perf.c',
  'js_realm.c',
  'wasm.c',
  'webaudio.c',
  'webgl.c',
)
if quickjs_original
  southstar_engine_sources += files('ns_quickjs.c')
endif

# GTK-free helpers used by the desktop executable but not the engine UI:
# the headless render driver and the crash watchdog. Kept in src/ as common
# code; the GTK frontend itself lives in src/gtk/.
southstar_app_sources = files(
  'headless.c',
  'watchdog.c',
)

if have_avif
  southstar_engine_sources += files('image_avif.c')
endif

if http_backend == 'nghttp2'
  southstar_engine_sources += files('net_http2.c')
endif

if have_webgpu
  southstar_engine_sources += files('webgpu.c')
endif

southstar_win_resources = []
if host_machine.system() == 'windows'
  windows = import('windows')
  southstar_win_resources = windows.compile_resources(
    'southstar.rc',
    depend_files: files('../data/icons/southstar.ico'),
  )
endif

southstar_deps = [
  southstar_rust_dep,
  glib,
  gobject,
  gio,
  cairo,
  pango,
  pangocairo,
  epoxy,
  gmodule,
  libcurl,
  libcrypto,
  libm,
  libquickjs,
  libwebp,
  libwebpdemux,
  libwuffs,
  libplmpeg,
  libuchardet,
  liblexbor,
  libwamr,
  sqlite3,
  zlib,
]

if have_seccomp
  southstar_deps += [libseccomp]
endif

if have_poppler
  southstar_deps += [libpoppler]
endif

if have_avif
  southstar_deps += [libavif]
endif

if have_libav
  southstar_deps += [libavformat, libavcodec, libavutil, libswscale]
endif

if have_fontconfig
  southstar_deps += [libfontconfig]
endif

if have_pangoft2
  southstar_deps += [libpangoft2]
endif

if have_freetype
  southstar_deps += [libfreetype]
endif

southstar_deps += [libpsl]

if http_backend == 'nghttp2'
  southstar_deps += http_backend_deps
endif

# macOS WebGL: src/glctx.c creates the offscreen GL context through CGL, whose
# symbols live in the OpenGL framework. Unused targets drop it via
# -dead_strip_dylibs.
if host_machine.system() == 'darwin'
  southstar_deps += [dependency('appleframeworks', modules: ['OpenGL'])]
endif

sdl2_dep = dependency('sdl2', required: get_option('audio'))
ns_lib_c_args = []
ns_sdl_deps = []
if sdl2_dep.found()
  ns_sdl_deps += [sdl2_dep]
  ns_lib_c_args += ['-DNS_HAVE_SDL']
endif

if have_webgpu
  southstar_deps += [webgpu_dep]
endif

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

# NetBSD pkgsrc libunistring (pulled in transitively through the libcurl /
# libpsl / libidn2 / gnutls stack) carries an undefined reference to libc's
# mbrtoc32, which the hardened --no-undefined link rejects even though the
# dynamic linker resolves it at runtime. Allow undefined symbols originating
# from shared libraries on NetBSD only; our own objects are still checked.
# Every binary that links that stack needs this — the executables below and
# the audio/video helpers, which pull in libunistring via libcurl.
if host_machine.system() == 'netbsd'
  southstar_link_args += cc.get_supported_link_arguments(
    ['-Wl,--allow-shlib-undefined'],
  )
endif

# The gmodule-2.0 dependency injects -Wl,--export-dynamic, which dumps every
# internal engine symbol (~6000: ns_*, lexbor, quickjs) into the executables'
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
    files('win_launcher.c') + southstar_win_resources,
    win_subsystem: 'windows',
    link_args: ['-luser32', '-lshell32'],
  )
endif

southstar_embed_sources = southstar_engine_sources + files(
  'embed_shim.c',
  'libsouthstar.c',
) + [polyfills_h, streaming_h]

libsouthstar_lib = library(
  'southstar',
  southstar_embed_sources,
  dependencies: southstar_deps + ns_sdl_deps,
  c_args: ns_lib_c_args,
  link_args: southstar_link_args,
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

# Out-of-process renderer (step 1 of the process-per-tab direction): a small
# executable that drives libsouthstar over an IPC control channel + shared
# memory framebuffer (src/rproc.*), plus a proof-of-concept client.
rproc_deps = []
if host_machine.system() != 'windows'
  rproc_deps += [cc.find_library('rt', required: false)]
endif

executable(
  'southstar-renderer',
  files('renderer_http.c', 'renderer_serve.c', 'renderer_tiles.c',
        'ipc_http.c'),
  link_with: libsouthstar_static,
  include_directories: include_directories('.'),
  dependencies: southstar_deps + ns_sdl_deps + rproc_deps,
  c_args: ns_lib_c_args,
  link_args: southstar_exe_link_args,
  win_subsystem: 'windows',
  install: true,
)

sdl2_dep = dependency('sdl2', required: get_option('audio'))
if sdl2_dep.found()
  libminimp3 = static_library(
    'minimp3',
    files('audio/minimp3_impl.c'),
    include_directories: include_directories('audio'),
    c_args: cc.get_supported_arguments(['-w']),
    install: false,
  )

  audio_deps = [dependency('libcurl'), sdl2_dep, libplmpeg]
  if have_libav
    audio_deps += [libavformat, libavcodec, libavutil, libswresample]
  endif
  if libvorbisfile.found()
    audio_deps += [libvorbisfile]
  endif
  if libopusfile.found()
    audio_deps += [libopusfile]
  endif
  # libm is needed by pl_mpeg / minimp3 on Linux and the BSDs; it is part of
  # the C runtime on macOS and Windows, where find_library('m') is a harmless
  # no-op. librt carries shm_open/shm_unlink (the audio clock's POSIX shared
  # memory) on the BSDs, where they live outside libc; it is a harmless no-op
  # where absent, matching the renderer and video-helper link lines.
  audio_lib_deps = [cc.find_library('m', required: false),
                    cc.find_library('rt', required: false)]

  # On Linux the helper confines itself with the same Landlock + seccomp
  # profile as the renderer once its audio device and network stack are up,
  # so attacker-controlled codec bytes are decoded inside the sandbox.
  audio_sandbox_sources = []
  if host_machine.system() == 'linux'
    audio_deps += [southstar_helper_rust_dep]
  endif

  executable(
    'southstar-audio',
    files('audio/main.c') + audio_sandbox_sources,
    include_directories: include_directories('audio', '.'),
    link_with: libminimp3,
    dependencies: audio_deps + audio_lib_deps,
    link_args: southstar_link_args,
    win_subsystem: 'windows',
    install: true,
  )
else
  message('SDL2 not found — audio playback helper (southstar-audio) disabled')
endif

if have_libav
  videoproc_deps = [libavformat, libavcodec, libavutil, libswscale,
                    dependency('threads')]
  videoproc_sources = files('videoproc/main.c')
  if host_machine.system() == 'linux'
    videoproc_deps += [southstar_helper_rust_dep]
    videoproc_deps += [cc.find_library('rt', required: false)]
  endif
  executable(
    'southstar-video',
    videoproc_sources,
    include_directories: include_directories('.'),
    dependencies: videoproc_deps + [cc.find_library('m', required: false)],
    link_args: southstar_link_args,
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
