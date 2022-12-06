# Resources

Resources is a simple yet powerful monitor for your system resources and processes, written in Rust and using GTK 4 and libadwaita for its GUI. It's currently WIP, but is already capable of displaying usage and details of your CPU, memory, GPUs, network interfaces and block devices. It's also capable of listing and terminating running graphical applications.

## Dependencies

- `glib-2.0`
- `gio-2.0`
- `gtk-4`
- `libadwaita-1`
- `systemd`
- `polkit`
- `cargo`

Other dependencies are handled by `cargo`.
Note: Right now, Resources requires the nightly version of Rust.

## Installing

Resources uses Meson as its build system. It alse uses a daemon in order to gather some root-only information (such as your memory specs).
Since Resources requires access to the system's running processes, building it as a Flatpak is possible but not recommended as it lacks functionality.

```sh
meson . build --prefix=/usr
ninja -C build install
systemctl enable --now me.nalux.Resources
```

You can also run the daemon manually by launching `resources-daemon` with root privileges.

## Running

Running Resources is as simple as typing `resources` into a terminal or running it from your application launcher.

## To-do

- Process viewer
- Display graphs instead of or alongside the current progress bars
- Battery usage and details
- Preferences such as a unit selection
- Translations
