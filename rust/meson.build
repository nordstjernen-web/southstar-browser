cargo = find_program('cargo', version: '>=1.91')
rustc = find_program('rustc', version: '>=1.91')
cargo_build_py = files('../scripts/cargo-build.py')
cargo_python = import('python').find_installation()

rust_native_libs = run_command(
  cargo_python, cargo_build_py, 'native-libs', rustc, meson.project_source_root(),
  check: true,
).stdout().strip().split('\n')

southstar_rust_features = []
southstar_rust_link_deps = [sqlite3, libcrypto, libwebp, libwebpdemux, libuchardet, glib, cairo, cairo_ft, harfbuzz, libfontconfig, libfreetype, libpsl, epoxy]
if host_machine.system() != 'windows'
  southstar_rust_link_deps += [meson.get_compiler('c').find_library('rt', required: false)]
endif
southstar_rust_env = {
  'NS_RUSTC_VERSION': rustc.version(),
  'NS_VERSION': meson.project_version(),
  'NS_BUILD_DATE': build_date,
}
if get_option('overflow_checks')
  southstar_rust_env += {'CARGO_PROFILE_RELEASE_OVERFLOW_CHECKS': 'true'}
endif
rust_cc = meson.get_compiler('c')
fs = import('fs')
layout_probes = []
if have_libav
  layout_probes += [['NS_AV_LAYOUT', 'av/layout.txt',
                     [libavformat, libavcodec, libavutil, libswscale, libswresample]]]
endif
if libvorbisfile.found()
  layout_probes += [['NS_VORBIS_LAYOUT', 'audio-helper/vorbis-layout.txt', [libvorbisfile]]]
endif
if have_avif
  layout_probes += [['NS_AVIF_LAYOUT', 'image-decoders/avif-layout.txt', [libavif]]]
endif
foreach probe : layout_probes
  layout_prefix = []
  layout_values = []
  foreach line : fs.read(probe[1]).strip().split('\n')
    if line.startswith('#')
      layout_prefix += line
    elif line != ''
      entry = line.split('=')
      layout_values += entry[0] + '=' + rust_cc.compute_int(entry[1],
        prefix: '\n'.join(layout_prefix), dependencies: probe[2],
        low: -2147483648, high: 2147483647).to_string()
    endif
  endforeach
  southstar_rust_env += {probe[0]: ','.join(layout_values)}
endforeach
if libthai.found()
  southstar_rust_features += ['thai']
  southstar_rust_link_deps += [libthai]
endif
if get_option('rust-shaper')
  southstar_rust_features += ['rust-shaper']
endif
if have_egl and host_machine.system() not in ['darwin', 'windows']
  southstar_rust_features += ['egl']
endif
rust_sdl2 = dependency('sdl2', required: get_option('audio'))
if rust_sdl2.found()
  southstar_rust_features += ['sdl']
  southstar_rust_link_deps += [rust_sdl2.partial_dependency(link_args: true)]
endif
if have_fontconfig and host_machine.system() == 'windows'
  southstar_rust_features += ['fontconfig']
  southstar_rust_link_deps += [libfontconfig]
endif
if have_fontconfig
  southstar_rust_features += ['font-fontconfig']
  southstar_rust_link_deps += [libfontconfig]
endif
if have_avif
  southstar_rust_features += ['avif']
  southstar_rust_link_deps += [libavif]
endif
if have_poppler
  southstar_rust_features += ['poppler']
  southstar_rust_link_deps += [libpoppler]
endif
if have_seccomp
  southstar_rust_features += ['seccomp']
  southstar_rust_link_deps += [libseccomp]
endif
if have_enchant
  southstar_rust_features += ['enchant']
  southstar_rust_link_deps += [libenchant]
endif
if have_libav
  southstar_rust_features += ['libav']
  southstar_rust_link_deps += [libavformat, libavcodec, libavutil, libswscale]
endif
if have_audio_native_vorbis
  southstar_rust_features += ['audio-native-vorbis']
endif
if have_audio_native_opus
  southstar_rust_features += ['audio-native-opus']
endif
if have_http_brotli
  southstar_rust_features += ['http-brotli']
endif
if have_http_zstd
  southstar_rust_features += ['http-zstd']
endif
southstar_rust_link_deps += http_backend_deps
if have_webgpu
  southstar_rust_features += ['webgpu']
  southstar_rust_link_deps += [webgpu_dep]
endif
if have_wasm
  southstar_rust_features += ['wasm']
endif
if have_woff2
  southstar_rust_features += ['woff2']
  southstar_rust_link_deps += [libbrotlidec_fonts]
endif

southstar_rust_lib = custom_target(
  'southstar-rust',
  output: 'libsouthstar_ffi.a',
  depfile: 'libsouthstar_ffi.a.d',
  command: [
    cargo_python, cargo_build_py, 'build', cargo, meson.project_source_root(),
    meson.current_build_dir() / 'cargo-target',
    ns_rust_release ? 'release' : 'dev',
    'southstar-static', '@OUTPUT@', '@DEPFILE@',
  ] + southstar_rust_features,
  env: southstar_rust_env,
  console: true,
  build_by_default: true,
)

southstar_rust_dep = declare_dependency(
  link_with: southstar_rust_lib,
  link_args: rust_native_libs,
  dependencies: southstar_rust_link_deps,
)

if have_gtk
  southstar_shell_rust_lib = custom_target(
    'southstar-shell-rust',
    output: 'libsouthstar_shell.a',
    depfile: 'libsouthstar_shell.a.d',
    command: [
      cargo_python, cargo_build_py, 'build', cargo, meson.project_source_root(),
      meson.current_build_dir() / 'cargo-target',
      ns_rust_release ? 'release' : 'dev',
      'southstar-shell', '@OUTPUT@', '@DEPFILE@',
    ] + southstar_rust_features,
    env: southstar_rust_env,
    console: true,
    build_by_default: true,
    depends: southstar_rust_lib,
  )

  southstar_shell_rust_dep = declare_dependency(
    link_with: southstar_shell_rust_lib,
    link_args: rust_native_libs,
    dependencies: southstar_rust_link_deps + [gtk4_dep],
  )
endif

if host_machine.system() == 'windows'
  southstar_launcher_rust_lib = custom_target(
    'southstar-launcher-rust',
    output: 'libsouthstar_launcher.a',
    depfile: 'libsouthstar_launcher.a.d',
    command: [
      cargo_python, cargo_build_py, 'build', cargo, meson.project_source_root(),
      meson.current_build_dir() / 'cargo-target',
      ns_rust_release ? 'release' : 'dev',
      'southstar-launcher', '@OUTPUT@', '@DEPFILE@',
    ],
    env: southstar_rust_env,
    console: true,
    build_by_default: true,
    depends: southstar_rust_lib,
  )
endif

southstar_helper_rust_features = have_seccomp ? ['seccomp'] : []
southstar_helper_rust_lib = custom_target(
  'southstar-helper-rust',
  output: 'libsouthstar_helper_ffi.a',
  depfile: 'libsouthstar_helper_ffi.a.d',
  command: [
    cargo_python, cargo_build_py, 'build', cargo, meson.project_source_root(),
    meson.current_build_dir() / 'cargo-target',
    ns_rust_release ? 'release' : 'dev',
    'southstar-helper-ffi', '@OUTPUT@', '@DEPFILE@',
  ] + southstar_helper_rust_features,
  env: southstar_rust_env,
  console: true,
  build_by_default: true,
)

southstar_helper_rust_dep = declare_dependency(
  link_with: southstar_helper_rust_lib,
  link_args: rust_native_libs,
  dependencies: [glib] + http_backend_deps + (have_seccomp ? [libseccomp] : []),
)


if have_libav
  southstar_video_rust_lib = custom_target(
    'southstar-video-rust',
    output: 'libsouthstar_video_helper.a',
    depfile: 'libsouthstar_video_helper.a.d',
    command: [
      cargo_python, cargo_build_py, 'build', cargo, meson.project_source_root(),
      meson.current_build_dir() / 'cargo-target',
      ns_rust_release ? 'release' : 'dev',
      'southstar-video-helper', '@OUTPUT@', '@DEPFILE@',
    ] + southstar_helper_rust_features,
    env: southstar_rust_env,
    console: true,
    build_by_default: true,
    depends: southstar_helper_rust_lib,
  )
endif

if rust_sdl2.found()
  southstar_audio_rust_features = []
  if have_libav
    southstar_audio_rust_features += ['libav']
  endif
  if libvorbisfile.found()
    southstar_audio_rust_features += ['vorbisfile']
  endif
  if libopusfile.found()
    southstar_audio_rust_features += ['opusfile']
  endif
  if have_seccomp
    southstar_audio_rust_features += ['seccomp']
  endif
  southstar_audio_rust_lib = custom_target(
    'southstar-audio-rust',
    output: 'libsouthstar_audio_helper.a',
    depfile: 'libsouthstar_audio_helper.a.d',
    command: [
      cargo_python, cargo_build_py, 'build', cargo, meson.project_source_root(),
      meson.current_build_dir() / 'cargo-target',
      ns_rust_release ? 'release' : 'dev',
      'southstar-audio-helper', '@OUTPUT@', '@DEPFILE@',
    ] + southstar_audio_rust_features,
    env: southstar_rust_env,
    console: true,
    build_by_default: true,
    depends: southstar_helper_rust_lib,
  )
endif
