cargo = find_program('cargo', version: '>=1.85')
rustc = find_program('rustc', version: '>=1.85')
cargo_build_py = files('../scripts/cargo-build.py')
cargo_python = import('python').find_installation()

rust_native_libs = run_command(
  cargo_python, cargo_build_py, 'native-libs', rustc, meson.project_source_root(),
  check: true,
).stdout().strip().split('\n')

southstar_rust_features = []
southstar_rust_link_deps = [sqlite3, libcrypto, libwebp, libwebpdemux, libcurl, libuchardet, cairo]
if host_machine.system() != 'windows'
  southstar_rust_link_deps += [meson.get_compiler('c').find_library('rt', required: false)]
endif
southstar_rust_env = {'NS_RUSTC_VERSION': rustc.version()}
if quickjs_original
  southstar_rust_features += ['quickjs-original']
  southstar_rust_env = {
    'NS_RUSTC_VERSION': rustc.version(),
    'NS_QUICKJS_ORIGINAL_VERSION': quickjs_version,
  }
endif
if have_egl and host_machine.system() not in ['darwin', 'windows']
  southstar_rust_features += ['egl']
endif
rust_sdl2 = dependency('sdl2', required: get_option('audio'))
if rust_sdl2.found()
  southstar_rust_features += ['sdl']
  southstar_rust_link_deps += [rust_sdl2]
endif
if have_fontconfig and host_machine.system() == 'windows'
  southstar_rust_features += ['fontconfig']
  southstar_rust_link_deps += [libfontconfig]
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
