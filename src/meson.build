# Cargo builds the app and the helper (resources are compiled by build.rs);
# meson only drives it and installs the results.
cargo = find_program('cargo')

cargo_env = environment()
cargo_env.set('SLUIS_LOCALEDIR', localedir)

target_dir = meson.project_build_root() / 'target'

custom_target(
  'cargo-build',
  build_by_default: true,
  build_always_stale: true,
  console: true,
  output: ['sluis', 'sluis-helper'],
  install: true,
  install_dir: [bindir, libexecdir],
  env: cargo_env,
  command: [
    cargo, 'build', '--release', '--workspace',
    '--manifest-path', meson.project_source_root() / 'Cargo.toml',
    '--target-dir', target_dir,
    '&&', 'cp', target_dir / 'release' / 'sluis', '@OUTPUT0@',
    '&&', 'cp', target_dir / 'release' / 'sluis-helper', '@OUTPUT1@',
  ],
)
