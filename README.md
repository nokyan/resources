# Resources

<a href='https://flathub.org/apps/net.nokyan.Resources'><img width='240' alt='Download on Flathub' src='https://flathub.org/api/badge?svg&locale=en'/></a>

[![GNOME Circle](https://circle.gnome.org/assets/button/badge.svg
)](https://apps.gnome.org/app/net.nokyan.Resources/) [![Please do not theme this app](https://stopthemingmy.app/badge.svg)](https://stopthemingmy.app)  

Resources is a simple yet powerful monitor for your system resources and processes, written in Rust and using GTK 4 and libadwaita for its GUI. It’s capable of displaying usage and details of your CPU, memory, GPUs, NPUs, network interfaces and block devices. It’s also capable of listing and terminating running graphical applications as well as processes.

<details>
  <summary><b>Click me for screenshots!</b></summary>

  ![Apps View](data/resources/screenshots/1.png?raw=true "Apps View")

  ![Processes View](data/resources/screenshots/2.png?raw=true "Processes View")

  ![Processor View](data/resources/screenshots/3.png?raw=true "Processor View")

  ![Memory View](data/resources/screenshots/4.png?raw=true "Memory View")

  ![GPU View](data/resources/screenshots/5.png?raw=true "GPU View")

  ![Drive View](data/resources/screenshots/6.png?raw=true "Drive View")

  ![Network Interface View](data/resources/screenshots/7.png?raw=true "Network Interface View")

  ![Battery View](data/resources/screenshots/8.png?raw=true "Battery View")
  
</details>

## Installing

The **official** and **only supported** way of installing Resources is using Flatpak. Simply use your graphical software manager like GNOME Software or Discover to install Resources from Flathub or type ``flatpak install flathub net.nokyan.Resources`` in your terminal.
Please keep in mind that you need to have Flathub set up on your device. You can find out how to set up Flathub [here](https://flathub.org/setup).

### Unofficial Packages

Resources has been packaged for some Linux distributions by volunteers. Keep in mind that these are not supported.
If you’re packaging Resources for another distribution, feel free to send a pull request to add your package to this list!

#### Arch Linux

Unofficially packaged in the [extra](https://archlinux.org/packages/extra/x86_64/resources/) repository.

You can install Resources using `pacman` with no further configuration required.

```sh
pacman -S resources
```

#### Fedora

Unofficially packaged in [Copr](https://copr.fedorainfracloud.org/coprs/atim/resources/) for Fedora 39 and newer.

You first need to enable the `atim/resources` Copr repository and then use `dnf` to install Resources.

```sh
dnf copr enable atim/resources
dnf install resources
```

## Building

You can also build Resources yourself using either Meson directly or preferably using Flatpak Builder.

### Build Dependencies

- `glib-2.0` ≥ 2.66
- `gio-2.0` ≥ 2.66
- `gtk-4` ≥ 4.12
- `libadwaita-1` ≥ 1.6
- `cargo`

Other dependencies are handled by `cargo`.
Resources’ minimum supported Rust version (MSRV) is **1.85.0**.

### Runtime Dependencies

These dependencies are not needed to build Resources but Resources may lack certain functionalities when they are not present.

- `systemd` (needed for app detection using cgroups)
- `polkit` (needed for executing privileged actions like killing certain processes)

### Building Using Flatpak Builder

```sh
flatpak install org.gnome.Sdk//47 org.freedesktop.Sdk.Extension.rust-stable//24.08 org.gnome.Platform//47 org.freedesktop.Sdk.Extension.llvm18//24.08
flatpak-builder --user flatpak_app build-aux/net.nokyan.Resources.Devel.json
```

If you use [GNOME Builder](https://apps.gnome.org/app/org.gnome.Builder/) or Visual Studio Code with the [Flatpak extension](https://marketplace.visualstudio.com/items?itemName=bilelmoussaoui.flatpak-vscode), Resources can be built and run automatically.

### Building Natively Using Meson

```sh
meson . build --prefix=/usr/local
ninja -C build install
```

## Running

Running Resources is as simple as typing `flatpak run net.nokyan.Resources` into a terminal or running it from your app launcher.
If you’ve built Resources natively or installed it from a traditional package manager such as `apt` or `dnf`, or if you’ve built Resources yourself, typing `resources` in a terminal will start Resources.
If you’ve built Resources as a Flatpak, type `flatpak-builder --run flatpak_app build-aux/net.nokyan.Resources.Devel.json resources` into your terminal or use one of the aforementioned IDEs to do that automatically.

## Contributing

If you have an idea, bug report, question or something else, don’t hesitate to [open an issue](https://github.com/nokyan/resources/issues)! Translations are always welcome.

## Code of Conduct

Resources follows the [GNOME Code of Conduct](/CODE_OF_CONDUCT.md).
All communications in project spaces are expected to follow it.
