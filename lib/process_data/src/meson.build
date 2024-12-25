cargo_options = [
    '--manifest-path', meson.project_source_root() / 'lib' / 'process_data' / 'Cargo.toml',
]
cargo_options += [
    '--target-dir', meson.project_build_root() / 'lib' / 'process_data' / 'src',
]

test(
    'Cargo tests (process_data)',
    cargo,
    args: ['test', cargo_options],
    timeout: 3600,
    env: cargo_env,
)