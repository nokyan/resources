# Resources

Resources is a simple yet powerful monitor for your system resources and processes, written in Rust and using GTK 4 and libadwaita for its GUI. It's currently WIP, but is already capable of displaying usage and details of your CPU, memory, GPUs, network interfaces and block devices. It's also capable of listing and terminating running graphical applications as well as processes.

<details>
  <summary><b>Click me for screenshots!</b></summary>

  ![Applications View of Resources](data/resources/screenshots/1.png?raw=true "Applications View of Resources")

  ![Applications View of Resources](data/resources/screenshots/2.png?raw=true "Processor View of Resources")

  ![Applications View of Resources](data/resources/screenshots/3.png?raw=true "GPU View of Resources")

  ![Applications View of Resources](data/resources/screenshots/4.png?raw=true "Network Interface View of Resources")
  
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
You can either build and install Resources natively on your system like this:

```sh
meson . build --prefix=/usr/local
ninja -C build install
```

Or, even better, use the Flatpak CLI to build:

```sh
flatpak install --user org.gnome.Sdk//44 org.freedesktop.Sdk.Extension.rust-nightly//22.08 org.gnome.Platform//44
flatpak-builder --user flatpak_app build-aux/me.nalux.Resources.Devel.json
```

Flatpak support is still experimental, bugs might occur.
If you use [GNOME Builder](https://apps.gnome.org/app/org.gnome.Builder/) or Visual Studio Code with the [Flatpak extension](https://marketplace.visualstudio.com/items?itemName=bilelmoussaoui.flatpak-vscode), Resources can be built and run automatically.

## Running

Running Resources is as simple as typing `resources` into a terminal or running it from your application launcher. If you've built Resources using Flatpak, type `
flatpak-builder --run flatpak_app build-aux/me.nalux.Resources.Devel.json resources` into your terminal or use one of the afforementioned IDEs to do that automatically.

## To-do

The following list is *roughly* in order of their importance with the most important item being first in the list.

- Battery usage and details
- Preferences such as a unit selection
- Translations

## Contributing

If you have an idea, bug report, question or something else, don't hesitate to [open an issue](https://github.com/nokyan/resources/issues)! Translations are always welcome.
