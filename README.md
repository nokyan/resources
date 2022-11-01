# Resources

Resources is a simple yet powerful monitor for your system resources and processes, written in Rust and using GTK 4 and libadwaita for its GUI. It's currently very much WIP, but is already capable of displaying usage and details of your CPU and memory.

## Dependencies

- `glib-2.0`
- `gio-2.0`
- `gtk-4`
- `libadwaita-1`
- `systemd`
- `cargo`

Other dependencies are handled by `cargo`.
Note: Right now, Resources requires the nightly version of Rust.

## Installing

Resources uses Meson as its build system. It alse uses a daemon in order to gather some root-only information (such as your memory specs), which needs to be started before the actual GUI can be started.
Since Resources requires access to the system's running processes (soon), building it as a Flatpak is not recommended.

```sh
meson . build --prefix=/usr
ninja -C build install
systemctl enable --now me.nalux.Resources
```

You can also run the daemon manually by launching `resources-daemon` with root privileges.

## Running

Running Resources is as simple as typing `resources` into a terminal or running it from your application launcher.

## To-do

- Display graphs instead of or alongside the current progress bars
- Battery usage and details
- List processes and make them sortable by CPU, memory and maybe GPU usage
