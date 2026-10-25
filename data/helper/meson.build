helper_conf = configuration_data()
helper_conf.set('LIBEXECDIR', libexecdir)

# D-Bus, polkit and systemd only read their own folders, never ones under
# /usr/local, so these go where they look whatever the prefix: asked from
# pkg-config when it knows, else the usual places.
dbus = dependency('dbus-1', required: false)
dbus_datadir = dbus.found() ? dbus.get_variable('datadir') : '/usr/share'
polkit = dependency('polkit-gobject-1', required: false)
polkit_actiondir = polkit.found() ? polkit.get_variable('policydir') : '/usr/share/polkit-1/actions'
systemd = dependency('systemd', required: false)
systemd_unitdir = systemd.found() ? systemd.get_variable('systemd_system_unit_dir') : '/usr/lib/systemd/system'

install_data('io.github.rayytor.Sluis.Helper.conf', install_dir: dbus_datadir / 'dbus-1' / 'system.d')
configure_file(
  input: 'io.github.rayytor.Sluis.Helper.service.in',
  output: 'io.github.rayytor.Sluis.Helper.service',
  configuration: helper_conf,
  install_dir: dbus_datadir / 'dbus-1' / 'system-services',
)
configure_file(
  input: 'sluis-helper.service.in',
  output: 'sluis-helper.service',
  configuration: helper_conf,
  install_dir: systemd_unitdir,
)
install_data('io.github.rayytor.Sluis.policy', install_dir: polkit_actiondir)
if get_option('wheel_rule')
  install_data('50-io.github.rayytor.Sluis.rules', install_dir: polkit_actiondir / '..' / 'rules.d')
endif
