# Resources

Resources is a simple yet powerful monitor for your system resources and processes, written in Rust and using GTK 4 and libadwaita for its GUI. It's currently WIP, but is already capable of displaying usage and details of your CPU, memory, GPUs, network interfaces and block devices. It's also capable of listing and terminating running graphical applications as well as processes.

<details>
  <summary><b>Click me for screenshots!</b></summary>

  ![Applications View of Resources](data/resources/screenshots/1.png?raw=true "Applications View of Resources")

  ![Applications View of Resources](data/resources/screenshots/2.png?raw=true "Processor View of Resources")

  ![Applications View of Resources](data/resources/screenshots/3.png?raw=true "Network Interface View of Resources")
  
</details>

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

Resources uses Meson as its build system.
Since Resources requires access to the system's running processes, building it as a Flatpak is possible but not recommended as it lacks functionality.

```sh
meson . build --prefix=/usr/local
ninja -C build install
```

## Running

Running Resources is as simple as typing `resources` into a terminal or running it from your application launcher.

## To-do

- Make process reading work when packaged as a Flatpak
- Battery usage and details
- Preferences such as a unit selection
- Translations
