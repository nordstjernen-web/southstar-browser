cargo = find_program('cargo', version: '>=1.85')
rustc = find_program('rustc', version: '>=1.85')
cargo_build_py = files('../scripts/cargo-build.py')
cargo_python = import('python').find_installation()

rust_native_libs = run_command(
  cargo_python, cargo_build_py, 'native-libs', rustc, meson.project_source_root(),
  check: true,
).stdout().strip().split('\n')

southstar_rust_features = []
southstar_rust_link_deps = [sqlite3, libcrypto, libwebp, libwebpdemux, libcurl, libuchardet, cairo, libpsl, pango]
if host_machine.system() != 'windows'
  southstar_rust_link_deps += [meson.get_compiler('c').find_library('rt', required: false)]
endif
southstar_rust_env = {
  'NS_RUSTC_VERSION': rustc.version(),
  'NS_VERSION': meson.project_version(),
  'NS_BUILD_DATE': build_date,
  'NS_LEXBOR_VERSION': lexbor_version,
}
if quickjs_original
  southstar_rust_features += ['quickjs-original']
  southstar_rust_env += {
    'NS_QUICKJS_ORIGINAL_VERSION': quickjs_version,
  }
endif
if use_ns_pango
  southstar_rust_features += ['ns-pango']
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
if have_freetype
  southstar_rust_features += ['freetype']
  southstar_rust_link_deps += [libfreetype]
endif
if have_pangoft2
  southstar_rust_features += ['pangoft2']
  southstar_rust_link_deps += [libpangoft2]
endif
if have_avif
  southstar_rust_features += ['avif']
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
endif
if http_backend == 'curl'
  southstar_rust_features += ['http-curl']
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
    ns_is_release ? 'release' : 'dev',
    'southstar-ffi', '@OUTPUT@', '@DEPFILE@',
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

if host_machine.system() == 'linux'
  southstar_helper_rust_features = have_seccomp ? ['seccomp'] : []
  southstar_helper_rust_lib = custom_target(
    'southstar-helper-rust',
    output: 'libsouthstar_helper_ffi.a',
    depfile: 'libsouthstar_helper_ffi.a.d',
    command: [
      cargo_python, cargo_build_py, 'build', cargo, meson.project_source_root(),
      meson.current_build_dir() / 'cargo-target',
      ns_is_release ? 'release' : 'dev',
      'southstar-helper-ffi', '@OUTPUT@', '@DEPFILE@',
    ] + southstar_helper_rust_features,
    env: southstar_rust_env,
    console: true,
    build_by_default: true,
  )

  southstar_helper_rust_dep = declare_dependency(
    link_with: southstar_helper_rust_lib,
    link_args: rust_native_libs,
    dependencies: [glib] + (have_seccomp ? [libseccomp] : []),
  )
endif
