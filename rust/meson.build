cargo = find_program('cargo', version: '>=1.85')
rustc = find_program('rustc', version: '>=1.85')
cargo_build_py = files('../scripts/cargo-build.py')
cargo_python = import('python').find_installation()

rust_native_libs = run_command(
  cargo_python, cargo_build_py, 'native-libs', rustc, meson.project_source_root(),
  check: true,
).stdout().strip().split('\n')

southstar_rust_lib = custom_target(
  'southstar-rust',
  output: 'libsouthstar_ffi.a',
  depfile: 'libsouthstar_ffi.a.d',
  command: [
    cargo_python, cargo_build_py, 'build', cargo, meson.project_source_root(),
    meson.current_build_dir() / 'cargo-target',
    ns_is_release ? 'release' : 'dev',
    'southstar-ffi', '@OUTPUT@', '@DEPFILE@',
  ],
  console: true,
  build_by_default: true,
)

southstar_rust_dep = declare_dependency(
  link_with: southstar_rust_lib,
  link_args: rust_native_libs,
)
