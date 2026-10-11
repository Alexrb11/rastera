<img src="assets/branding/rastera.svg" alt="Rastera" width="96" height="96">

# Rastera

**English** | [Español](README.md)

An image viewer and organizer with batch tools, built in Rust.

Rastera has a native desktop interface and works with local folders. Its goal is to make large image libraries responsive to browse, control memory use, and process images without blocking navigation.

## Try Rastera

The first release is `0.1.0-alpha.2`, an alpha for Windows 10/11 x64. Published alphas are available in [Releases](https://github.com/Alexrb11/rastera/releases). Download the portable ZIP and its SHA-256 checksum, extract it, and open `rastera.exe`; Rust is not required. See the [installation guide and limitations](docs/install-windows.md) and the [illustrated tutorial](docs/tutorial/index.html).

The Windows executable is unsigned. Testing on a clean Windows installation and full accessibility validation are pending. The catalog and cache are stored outside the executable's folder, in `%LOCALAPPDATA%\Rastera`.

`main` prepares `0.1.0-alpha.3`, with the Rastera logo and experimental Linux and macOS packages. Until that release is published, `alpha.2` remains the public download baseline. Read the guides before trying an Actions artifact or building from source:

| Platform | Package prepared for alpha.3 | Status and requirements |
| --- | --- | --- |
| Windows 10/11 x64 | MSVC portable ZIP | Initial alpha platform; clean Windows, other GPUs and accessibility testing pending. [Guide](docs/install-windows.md). |
| Linux x86-64 | tar.gz with optional per-user installer | Experimental: glibc ≥ 2.35, Vulkan, X11/Wayland and a file portal. X11 and Weston Wayland container checks passed; full desktop sessions and physical GPU testing remain pending. [Guide](docs/install-linux.md). |
| macOS Apple Silicon | ZIP containing arm64 `Rastera.app` | Experimental: declared minimum macOS 13; build matrix uses macOS 15. Ad-hoc signature, no Developer ID or notarization. [Guide](docs/install-macos.md). |
| macOS Intel | ZIP containing x64 `Rastera.app` | Same limits; desktop and minimum macOS 13 testing pending. [Guide](docs/install-macos.md). |

Linux uses XDG data and cache directories, falling back to locations under HOME. macOS uses `Library/Application Support/Rastera` and `Library/Caches/Rastera`; an existing catalog from an earlier source build may retain its old location. The guides describe paths and updates. The application interface and illustrated tutorial remain in Spanish.

The alpha.3 Linux artifact at revision `cf56c2e` passed 66 local checks and another 66 in GitHub Actions: 11 for each combination of Ubuntu 22.04, Ubuntu 24.04 and Debian 12 with X11 and Wayland. Each matrix verified nine launches of Rastera's native Wayland window, application identity, configuration and a buffer committed to its own surface, without `DISPLAY` or Xwayland. The tests use headless Weston and software Vulkan on WSL2 or the CI Linux environment; physical GPUs, full GNOME/KDE sessions, HiDPI, IME, accessibility and macOS still require validation.

Manual X11 review of the original artifact confirmed organization, search, viewer, automatic folder changes, JPEG export with output protection, the GTK chooser and persistence after normal close and restart. The rebuilt package passed nine manual checks in Ubuntu 24.04's nested Wayland desktop, including PNG batch export, 100% zoom, folder changes and tags. After the Weston compositor failed with pixman, the desktop now uses GL with Mesa llvmpipe; normal Rastera shutdown and persistence after an actual container restart were confirmed. These are bounded observations, with steps before and after the renderer change documented separately. See [the container testing guide](docs/testing-linux-containers.md) for each artifact's checksum, `-DisplayBackend wayland` instructions and the exact scope of each result.

## Project status

The initial viewer is implemented: it opens static JPEG, PNG, and WebP images through a dialog, a path, drag and drop, or a startup argument. It includes zoom, panning, EXIF orientation, transparency, and image details. Loading runs on a worker with cancellation and a memory budget.

The grid draws only visible rows, loads 256-pixel thumbnails, and supports multiple selection with Ctrl+click (Cmd+click on macOS), selecting the entire folder, and opening an image with a double-click. The Refresh button (`Actualizar`) rescans the folder and invalidates textures. Its LRU cache holds at most 128 textures (32 MiB of pixel data on the GPU), in addition to the preview. Thumbnails are reused across sessions through a disposable disk cache. The viewer has priority when requesting buffers; workers share the memory budget.

The SQLite catalog stores favorites, tags, and dimensions for opened images. Organize selection (`Organizar selección`) applies favorites and adds or removes tags for multiple images in one transaction, preserving each image's individual data. The grid supports searching by name or tag and filtering by favorites and format. Batch tools convert to PNG, JPEG, or lossless WebP, resize without upscaling, and rotate in 90-degree steps. They include plan review, progress, cancellation, and a SQLite journal for each batch. They export from the originals to another folder and save output files without replacing existing files. [Initial measurements](docs/performance-results.md) cover cache behavior, timings, and memory on Windows using synthetic image sets.

Folders can also be opened through a dialog, a path, drag and drop, or a startup argument. Scanning emits pages progressively, and the list draws only visible rows. Arrow keys move to the previous or next file. The initial scope is a single folder without recursion, limited to 10,000 files and 16 MiB of paths; links are excluded. The list is sorted by path when scanning finishes. A supported extension does not guarantee that the file contains a valid image.

The open folder updates automatically when files are added, modified, renamed, or removed. Changes are grouped in the background, and a check every 15 seconds recovers missed events. Filters, the selection of paths that are still present, and unchanged textures are preserved. An incomplete scan retains the previous library and displays the error while retrying. Renamed paths are treated as new and do not inherit favorites or tags.

Windows was the first development platform. Linux and macOS share the viewer, catalog, folder observation and batch engine; desktop integration and packaging are validated separately for each system. Passing CI does not replace testing a user's installation.

## Features

- An image management interface with compact toolbars, range selection, and a collapsible properties panel.
- Folder browsing with a thumbnail grid, selection, and sorting.
- Automatic updates to the open folder, viewer reloads, and recovery of missed events.
- A viewer with zoom, panning, fit to window, and keyboard navigation.
- A local catalog with favorites, tags, search, and tools to organize selected images together.
- Grid navigation with arrow keys, page keys, Shift ranges, and Enter to open an image.
- Batch image conversion, resizing, and rotation.
- Review of batch output names and dimensions, progress, cancellation, results, and recovery of pending work with verified fingerprints.
- Batch renaming in a later phase, with an operation log and recovery.

The initial scope covers static 8-bit SDR JPEG, PNG, and WebP images. The viewer, thumbnails, and batch tools convert supported RGB ICC profiles using matrices and curves to sRGB. HDR, CMYK, LUT profiles, and unsupported PNG color declarations are rejected. Compatibility is checked using test files; other formats and advanced color management will be added separately.

## Technical foundation

The architecture is a Cargo workspace with four components: desktop, domain, application engine, and adapters. The interface uses `egui/eframe`, the catalog uses SQLite, and processing runs on workers with bounded queues and memory.

The design favors progressive loading and resource reuse. Using Rust alone does not guarantee lower resource consumption than other applications.

## Documentation

Most existing guides are in Spanish; the new Linux and macOS installation guides are in English.

- [User guide](docs/usage.md): selection, organization, and export.
- [Linux installation](docs/install-linux.md) and [macOS installation](docs/install-macos.md): requirements, experimental packages, local data and integration limits.
- [Linux container testing](docs/testing-linux-containers.md): runtime-only Ubuntu/Debian checks with Docker Desktop, reports, and an optional browser desktop for manual review.
- [Illustrated tutorial](docs/tutorial/index.html): step-by-step use cases, actual screenshots, and practice images. Open it in a browser; see [preparation and provenance](docs/tutorial/README.md).
- [Architecture](docs/architecture.md): modules, dependencies, concurrency, catalog, caches, and file operations.
- [Development plan](docs/roadmap.md): milestones and acceptance criteria.
- [Measuring performance](docs/performance.md): counters, a reproducible benchmark, and process measurements.
- [Initial results](docs/performance-results.md): hardware, scenarios, observations, and limits of the comparison.
- [Cache and navigation optimization](docs/performance-cache-results.md): before-and-after comparison and native navigation tests with real photographs.
- [Preparing a release](docs/releasing.md): desktop packages, automated checks and manual testing on each platform.

The product is inspired by [Artcraft](https://getartcraft.com/apps)'s approach to native, local applications. Rastera will have its own scope and design.

## Contributing and license

Rastera's code is licensed under [MIT](LICENSE). Dependencies and fonts retain their own terms, included in each package and documented in [licensing](licensing/README.md). To contribute, see [CONTRIBUTING.md](CONTRIBUTING.md), the [code of conduct](CODE_OF_CONDUCT.md), and the [security policy](SECURITY.md). Reproducible issue reports and testing on different Windows machines, Linux desktops and Macs are especially useful during the alpha.

## Development

Rust 1.95 or later and the system's build tools are required. On Windows, the MSVC toolchain requires the C++ tools and Windows SDK; GNU requires a compatible MinGW environment. Initial local validation uses Rust 1.99 on Windows GNU.

Linux requires the development libraries and desktop services in [its guide](docs/install-linux.md); macOS requires Xcode Command Line Tools. On either platform, run `cargo run -p rastera-desktop --locked -- "$HOME/Pictures"`. Build packages on their target operating system; see [the release procedure](docs/releasing.md).

```powershell
cargo run -p rastera-desktop --locked
cargo run -p rastera-desktop --locked -- "C:\Fotos\imagen.png"
```

In PowerShell, you can also use `.\scripts\dev.ps1 run`. The script uses a local toolchain in `work/` when one exists and Cargo is not available on PATH; local tools are not part of the repository.

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked
```

The first build downloads dependencies and takes longer than subsequent builds. The viewer first displays a preview of up to 4,096 pixels and loads full resolution when zooming in, using visible tiles and bounded memory. 100% corresponds to physical screen pixels. Conversion to the monitor's color profile is still pending. Initial limits are 128 MiB per file, 64 million pixels, and a shared buffer budget of 512 MiB. An image may be rejected because of memory requirements even if it meets the pixel limit. This budget is not the process's total memory usage.

The optional `capture` feature allows visual checks of the actual rendering: with `RASTERA_CAPTURE_TO` pointing to a PNG, the application saves a screenshot when loading finishes and exits. `RASTERA_CAPTURE_SIZE=760x480` tests the minimum window size; `RASTERA_CAPTURE_SELECTION=1` selects the first three entries, and `RASTERA_CAPTURE_FILTER` allows checking filters with no matches. This is a development tool, disabled by default.
