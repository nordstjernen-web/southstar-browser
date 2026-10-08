southstar_gtk_sources = files(
  'appmain.c',
  'icons.c',
  'pagelayers.c',
  'procview.c',
  'procwindow.c',
  'titlerow.c',
  '../libsouthstar.c',
  '../renderer_serve.c',
  '../renderer_tiles.c',
  '../rproc_http.c',
  '../rproc_inproc.c',
  '../ipc_http.c',
)

southstar_gtk_deps = southstar_deps + [gtk4_dep]
if host_machine.system() != 'windows'
  southstar_gtk_deps += [cc.find_library('rt', required: false)]
endif

southstar_gtk_platform_sources = []
southstar_gtk_platform_link_args = []
if host_machine.system() == 'darwin'
  add_languages('objc', native: false)
  southstar_gtk_platform_sources += files('macos_dock.m')
  southstar_gtk_platform_link_args += ['-framework', 'AppKit']
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
  southstar_engine_sources + southstar_app_sources +
    southstar_gtk_sources + southstar_gtk_platform_sources +
    southstar_icon_resources +
    southstar_win_resources + [polyfills_h, wpt_hook_h],
  dependencies: southstar_gtk_deps,
  c_args: ['-DNS_HAVE_GTK=1'],
  include_directories: include_directories('..', '.'),
  win_subsystem: southstar_win_subsystem,
  link_args: southstar_exe_link_args + southstar_gtk_platform_link_args,
  install: true,
)
