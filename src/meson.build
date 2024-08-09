global_conf = configuration_data()
global_conf.set_quoted('APP_ID', application_id)
global_conf.set_quoted('PKGDATADIR', pkgdatadir)
global_conf.set_quoted('PROFILE', profile)
global_conf.set_quoted('VERSION', version + version_suffix)
global_conf.set_quoted('GETTEXT_PACKAGE', gettext_package)
global_conf.set_quoted('LOCALEDIR', localedir)
global_conf.set_quoted('LIBEXECDIR', libexecdir)
config = configure_file(input: 'config.rs.in', output: 'config.rs', configuration: global_conf)
# Copy the config.rs output to the source directory.
run_command(
  'cp',
  meson.project_build_root() / 'src' / 'config.rs',
  meson.project_source_root() / 'src' / 'config.rs',
  check: true,
)

cargo_options = ['--manifest-path', meson.project_source_root() / 'Cargo.toml']
cargo_options += ['--target-dir', meson.project_build_root() / 'src']

if get_option('profile') == 'default'
  cargo_options += ['--release']
  rust_target = 'release'
  message('Building in release mode')
else
  rust_target = 'debug'
  message('Building in debug mode')
endif

cargo_env = ['CARGO_HOME=' + meson.project_build_root() / 'cargo']

cargo_build = custom_target(
  'cargo-build',
  depends: resources,
  build_by_default: true,
  build_always_stale: true,
  output: rust_target,
  console: true,
  command: [
    'env',
    cargo_env,
    cargo,
    'build',
    cargo_options,
  ],
)

copy_binary = custom_target(
  'cp-binary',
  depends: cargo_build,
  build_by_default: true,
  build_always_stale: true,
  install: true,
  install_dir: bindir,
  output: meson.project_name(),
  command: [
    'cp',
    'src' / rust_target / meson.project_name(),
    '@OUTPUT@',
  ],
)

copy_kill_binary = custom_target(
  'cp-kill-binary',
  depends: cargo_build,
  build_by_default: true,
  build_always_stale: true,
  install: true,
  install_dir: libexecdir,
  output: meson.project_name() + '-kill',
  command: [
    'cp',
    'src' / rust_target / meson.project_name() + '-kill',
    '@OUTPUT@',
  ],
)

copy_processes_binary = custom_target(
  'cp-processes-binary',
  depends: cargo_build,
  build_by_default: true,
  build_always_stale: true,
  install: true,
  install_dir: libexecdir,
  output: meson.project_name() + '-processes',
  command: [
    'cp',
    'src' / rust_target / meson.project_name() + '-processes',
    '@OUTPUT@',
  ],
)

copy_adjust_binary = custom_target(
  'cp-adjust-binary',
  depends: cargo_build,
  build_by_default: true,
  build_always_stale: true,
  install: true,
  install_dir: libexecdir,
  output: meson.project_name() + '-adjust',
  command: [
    'cp',
    'src' / rust_target / meson.project_name() + '-adjust',
    '@OUTPUT@',
  ],
)