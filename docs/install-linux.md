# Install Rastera on Linux

Linux support is experimental in `0.1.0-alpha.3`. The prepared package targets x86-64 Linux with glibc 2.35 or later, using Ubuntu 22.04 as the build baseline. Packaging and container checks do not replace testing on a normal desktop. The published Windows alpha remains available while this release is prepared.

## Requirements

- An x86-64 desktop session using X11 or Wayland; this is a graphical application.
- glibc 2.35 or later. The package is not a musl/Alpine build.
- A working Vulkan graphics driver and loader, plus libxkbcommon and the libraries for your display server. X11 needs libxkbcommon-x11, Xlib, Xlib/XCB, Xcursor, XInput and XCB. Wayland needs its client library and a running compositor.
- A session D-Bus service, `xdg-desktop-portal`, and a portal backend that implements FileChooser. Install the backend appropriate to your desktop, such as the GTK, GNOME or KDE backend. Zenity supplies the file-dialog fallback.
- Python 3 for the optional per-user installer; launching the extracted binary does not require Python or Rust.

On Ubuntu, runtime package names include `libvulkan1`, `libxkbcommon0`, `xdg-desktop-portal` and `zenity`. X11 additionally requires `libxkbcommon-x11-0`, `libx11-6`, `libx11-xcb1`, `libxcursor1`, `libxi6` and `libxcb1`. The `libxkbcommon-x11-0` package also installs its XCB keyboard-extension dependency. Wayland uses `libwayland-client0`. These display and keyboard libraries are loaded dynamically, so a successful `ldd` check or `--build-info` does not establish that the graphical session can start. In particular, installing `libxkbcommon0` alone does not supply `libxkbcommon-x11.so.0`; [Ubuntu supplies it in a separate package](https://packages.ubuntu.com/jammy/libxkbcommon-x11-0).

Mesa's Vulkan drivers are supplied by `mesa-vulkan-drivers`; use a driver appropriate to your GPU. Installing a portal service without a working FileChooser backend is insufficient. An empty dialog or a graphics initialization error should be reported with desktop, session type and GPU/driver details.

## Download and run

Once this version is published, download `rastera-0.1.0-alpha.3-linux-x64.tar.gz` and its `.sha256` from [the official releases](https://github.com/Alexrb11/rastera/releases). Check the checksum in the directory containing both files:

```sh
sha256sum -c rastera-0.1.0-alpha.3-linux-x64.tar.gz.sha256
tar -xzf rastera-0.1.0-alpha.3-linux-x64.tar.gz
cd rastera-0.1.0-alpha.3-linux-x64
./bin/rastera
./bin/rastera "$HOME/Pictures"
```

The checksum detects a changed download; it does not authenticate a publisher. The archive includes the logo, license notices, a user guide, and the illustrated tutorial with practice images. Open `docs/tutorial/index.html` in a browser. The application interface and tutorial are currently in Spanish.

To add a launcher and icon for your user account:

```sh
./install-user.sh
```

The installer uses `~/.local/share/rastera/releases/<version>` and creates `~/.local/bin/rastera`, an application-menu entry and icons. It needs no administrator privileges. If `~/.local/bin` is absent from PATH, use that full executable path. When installing a newer version, use `--replace` to update the launchers and icon managed by a previous installation; older version directories are retained. It refuses to overwrite an already installed version or unrelated files and does not replace the user's image library or catalog. Optional `--register-mime` adds Rastera to the available applications for JPEG, PNG and WebP without changing their default application.

## Images, keyboard and local data

Use **Abrir carpeta…** to open a folder, **Abrir imagen…** to open an image, or drag a file or folder into the window. Ctrl+O opens an image; Ctrl+Shift+O opens a folder. Ctrl+click changes a multiple selection, Ctrl+F searches, and Ctrl+E opens export. See [the user guide](usage.md) for batch review and recovery.

The catalog is `$XDG_DATA_HOME/Rastera/catalog.sqlite`, or `$HOME/.local/share/Rastera/catalog.sqlite` when XDG_DATA_HOME is unset, empty or relative. Thumbnails use `$XDG_CACHE_HOME/Rastera/thumbnails-v1`, falling back to `$HOME/.cache/Rastera/thumbnails-v1` under the same rules. HOME must be an absolute path for these fallbacks. The installer location uses lowercase `rastera`; the existing data-directory name is `Rastera`.

Replacing the application preserves these directories. Cache files are disposable; the catalog contains favorites and tags, so back it up together with its SQLite companion files while Rastera is closed. Export journals remain in the output folder. Image originals are not moved into the application directory. Permissions, network filesystems and unusual volumes still require desktop testing.

## Current limits and testing

The scope remains one folder without recursion, up to 10,000 entries, with static 8-bit SDR JPEG, PNG and WebP. RAW, HEIC, animated images and monitor-profile conversion are unsupported. External renames do not transfer tags or favorites.

The original alpha.3 artifact passed all 11 runtime checks on Ubuntu 22.04, Ubuntu 24.04 and Debian 12 containers under Docker Desktop. Separate manual interaction on Ubuntu 24.04 confirmed the grid, favorite/tag editing and persistence after restart, search, viewer, automatic folder updates, JPEG export with existing-output refusal, the GTK portal file chooser and normal shutdown. See [the Linux container testing guide](testing-linux-containers.md) for the exact archive revision, results and fixture scope.

The rebuilt alpha.3 artifact at source revision `cf56c2e` passed 66 local checks and 66 checks in the expanded GitHub Actions matrix across six X11/Wayland services on those same distributions. Each matrix included nine launches verifying Rastera's native Wayland application identity, matching configure acknowledgements and a buffer committed to its own surface, with `DISPLAY` absent and Xwayland disabled. A separate Ubuntu 24.04 review passed nine manual checks, including retained annotations, maximization/restoration, the GTK chooser, the viewer at 100%, a three-image PNG batch, folder changes, tag editing/search and normal close. Persistence was also confirmed after an actual container restart using the final GL/llvmpipe nested compositor. The guide records each archive's checksum and distinguishes checks performed before and after that compositor change.

The automated Wayland tests use headless Weston with pixman. Interactive Wayland nests Weston 13 in Xvfb for browser viewing and uses GL with Mesa llvmpipe after a recorded pixman compositor SIGSEGV; its internal cause remains unknown. Rastera uses native Wayland and Mesa software Vulkan in both modes. Local containers share the WSL2 kernel, while CI supplies its own Linux runner environment. Physical GPU drivers, full GNOME/KDE sessions, other desktop portal backends, HiDPI, input methods (IME), accessibility and installations on independent distribution kernels still require testing. The containers do not validate macOS or long-term compositor stability.

For a useful report, include version, archive checksum, distribution, glibc version, desktop, X11/Wayland, GPU/driver and reproducible steps. Use practice images you may share. Report security problems through [SECURITY.md](../SECURITY.md).

## Build from source

Use Rust 1.95 or later, Git, a C/C++ toolchain and pkg-config. Ubuntu development packages include `build-essential`, `pkg-config`, `libxkbcommon-dev`, `libwayland-dev`, `libx11-dev`, `libxrandr-dev`, `libxi-dev`, `libxinerama-dev` and `libxcursor-dev`; runtime graphics and portal requirements still apply.

```sh
cargo run -p rastera-desktop --locked
cargo run -p rastera-desktop --locked -- "$HOME/Pictures"
```

For package creation and verification, see [the release procedure](releasing.md).
