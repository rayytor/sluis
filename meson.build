project(
  'sluis',
  version: '0.9.0',
  meson_version: '>= 1.1',
  license: 'GPL-3.0-or-later',
)

app_id = 'io.github.rayytor.Sluis'
gettext_package = meson.project_name()

i18n = import('i18n')
gnome = import('gnome')

prefix = get_option('prefix')
bindir = prefix / get_option('bindir')
libexecdir = prefix / get_option('libexecdir')
datadir = prefix / get_option('datadir')
localedir = prefix / get_option('localedir')

subdir('data')
subdir('po')
subdir('src')

gnome.post_install(
  glib_compile_schemas: true,
  gtk_update_icon_cache: true,
  update_desktop_database: true,
)
