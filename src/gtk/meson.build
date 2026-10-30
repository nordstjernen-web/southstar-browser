southstar_gtk_sources = []

southstar_gtk_deps = [southstar_shell_rust_dep] + southstar_c_deps + [gtk4_dep]
if host_machine.system() != 'windows'
  southstar_gtk_deps += [cc.find_library('rt', required: false)]
endif

southstar_gtk_platform_link_args = []
if host_machine.system() == 'darwin'
  southstar_gtk_platform_link_args += ['-framework', 'AppKit', '-lobjc']
endif

gnome = import('gnome')
southstar_icon_resources = gnome.compile_resources(
  'southstar-icon-resources',
  meson.project_source_root() / 'data' / 'icons' / 'southstar-icons.gresource.xml',
  source_dir: meson.project_source_root() / 'data' / 'icons',
  c_name: 'southstar_icons',
)

executable(
  'southstar',
  southstar_engine_sources +
    southstar_gtk_sources +
    southstar_icon_resources +
    southstar_win_resources,
  dependencies: southstar_gtk_deps,
  win_subsystem: southstar_win_subsystem,
  link_args: southstar_exe_link_args + southstar_gtk_platform_link_args,
  install: true,
)

if host_machine.system() != 'windows'
  install_symlink(
    'internet-browser-southstar',
    pointing_to: 'southstar',
    install_dir: get_option('bindir'),
  )
endif
