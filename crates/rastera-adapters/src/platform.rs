//! User-owned storage locations. Resolving a path never creates or moves files.
use rastera_core::{ErrorKind, RasteraError, Result};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Platform {
    Windows,
    Linux,
    MacOs,
}

impl Platform {
    fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Linux
        }
    }
}

#[derive(Clone, Copy)]
enum Storage {
    Data,
    Cache,
}

fn unix_absolute(value: OsString) -> Option<PathBuf> {
    // Test the target's syntax rather than the host's Path implementation, so the
    // pure resolver can exercise Unix locations even in Windows unit tests.
    value
        .as_encoded_bytes()
        .starts_with(b"/")
        .then(|| PathBuf::from(value))
}

fn resolve_root(
    platform: Platform,
    storage: Storage,
    mut environment: impl FnMut(&str) -> Option<OsString>,
) -> Result<PathBuf> {
    let root = match platform {
        // Keep the original Windows location for both durable data and cache.
        Platform::Windows => environment("LOCALAPPDATA")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from),
        Platform::Linux => {
            let (variable, fallback) = match storage {
                Storage::Data => ("XDG_DATA_HOME", ".local/share"),
                Storage::Cache => ("XDG_CACHE_HOME", ".cache"),
            };
            // XDG specifies ignoring empty and relative values, then using HOME.
            environment(variable).and_then(unix_absolute).or_else(|| {
                environment("HOME")
                    .and_then(unix_absolute)
                    .map(|home| home.join(fallback))
            })
        }
        Platform::MacOs => environment("HOME").and_then(unix_absolute).map(|home| {
            home.join("Library").join(match storage {
                Storage::Data => "Application Support",
                Storage::Cache => "Caches",
            })
        }),
    }
    .ok_or_else(|| {
        RasteraError::new(
            ErrorKind::Io,
            match storage {
                Storage::Data => "No se encontró el directorio de datos de usuario",
                Storage::Cache => "No se encontró el directorio de caché de usuario",
            },
        )
    })?;
    // Preserve the existing Linux catalog directory's case as well as Windows.
    Ok(root.join("Rastera"))
}

fn resolve_catalog_path(
    platform: Platform,
    mut environment: impl FnMut(&str) -> Option<OsString>,
    mut is_present: impl FnMut(&Path) -> Result<bool>,
) -> Result<PathBuf> {
    let current = resolve_root(platform, Storage::Data, &mut environment)?.join("catalog.sqlite");
    if platform == Platform::MacOs && !is_present(&current)? {
        // Earlier source builds used XDG paths on macOS as well. Continue opening
        // an existing catalog there rather than silently dropping annotations.
        // This does not migrate files; the disposable cache always uses Library.
        if let Ok(legacy) = resolve_root(Platform::Linux, Storage::Data, &mut environment) {
            let legacy = legacy.join("catalog.sqlite");
            if is_present(&legacy)? {
                return Ok(legacy);
            }
        }
    }
    Ok(current)
}

pub(crate) fn catalog_path() -> Result<PathBuf> {
    resolve_catalog_path(
        Platform::current(),
        |name| std::env::var_os(name),
        |path| {
            path.try_exists().map_err(|error| {
                RasteraError::new(
                    ErrorKind::Io,
                    format!("No se pudo comprobar el catálogo: {error}"),
                )
            })
        },
    )
}

pub(crate) fn thumbnail_cache_path() -> Result<PathBuf> {
    resolve_root(Platform::current(), Storage::Cache, |name| {
        std::env::var_os(name)
    })
    .map(|root| root.join("thumbnails-v1"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolve(platform: Platform, storage: Storage, values: &[(&str, &str)]) -> Result<PathBuf> {
        resolve_root(platform, storage, |name| {
            values
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| OsString::from(value))
        })
    }

    #[test]
    fn windows_preserves_catalog_and_cache_locations() {
        let local = "C:/Users/ana/AppData/Local";
        let environment = [
            ("LOCALAPPDATA", local),
            ("HOME", "/home/other"),
            ("XDG_DATA_HOME", "/data/other"),
            ("XDG_CACHE_HOME", "/cache/other"),
        ];
        let expected = PathBuf::from(local).join("Rastera");
        for storage in [Storage::Data, Storage::Cache] {
            assert_eq!(
                resolve(Platform::Windows, storage, &environment).unwrap(),
                expected
            );
            assert!(resolve(Platform::Windows, storage, &[]).is_err());
            assert!(resolve(Platform::Windows, storage, &[("LOCALAPPDATA", "")]).is_err());
        }
    }

    #[test]
    fn linux_uses_separate_absolute_xdg_roots_and_preserves_catalog_case() {
        let environment = [
            ("XDG_DATA_HOME", "/mnt/data/ana"),
            ("XDG_CACHE_HOME", "/mnt/cache/ana"),
        ];
        assert_eq!(
            resolve(Platform::Linux, Storage::Data, &environment).unwrap(),
            PathBuf::from("/mnt/data/ana").join("Rastera")
        );
        assert_eq!(
            resolve(Platform::Linux, Storage::Cache, &environment).unwrap(),
            PathBuf::from("/mnt/cache/ana").join("Rastera")
        );
    }

    #[test]
    fn linux_ignores_empty_or_relative_xdg_values_and_falls_back_independently() {
        let home = PathBuf::from("/home/ana");
        for invalid in ["", "relative/folder", "~/folder", "C:/folder"] {
            let environment = [
                ("HOME", "/home/ana"),
                ("XDG_DATA_HOME", invalid),
                ("XDG_CACHE_HOME", invalid),
            ];
            assert_eq!(
                resolve(Platform::Linux, Storage::Data, &environment).unwrap(),
                home.join(".local/share").join("Rastera")
            );
            assert_eq!(
                resolve(Platform::Linux, Storage::Cache, &environment).unwrap(),
                home.join(".cache").join("Rastera")
            );
        }
        let environment = [("HOME", "/home/ana"), ("XDG_DATA_HOME", "/custom/data")];
        assert_eq!(
            resolve(Platform::Linux, Storage::Data, &environment).unwrap(),
            PathBuf::from("/custom/data").join("Rastera")
        );
        assert_eq!(
            resolve(Platform::Linux, Storage::Cache, &environment).unwrap(),
            home.join(".cache").join("Rastera")
        );
    }

    #[test]
    fn unix_missing_or_relative_home_never_writes_to_the_working_directory() {
        for platform in [Platform::Linux, Platform::MacOs] {
            for storage in [Storage::Data, Storage::Cache] {
                for environment in [&[][..], &[("HOME", "")][..], &[("HOME", "relative")][..]] {
                    assert!(resolve(platform, storage, environment).is_err());
                }
            }
        }
    }

    #[test]
    fn macos_uses_user_library_and_ignores_xdg_overrides() {
        let environment = [
            ("HOME", "/Users/ana"),
            ("XDG_DATA_HOME", "/wrong/data"),
            ("XDG_CACHE_HOME", "/wrong/cache"),
        ];
        let library = PathBuf::from("/Users/ana").join("Library");
        assert_eq!(
            resolve(Platform::MacOs, Storage::Data, &environment).unwrap(),
            library.join("Application Support").join("Rastera")
        );
        assert_eq!(
            resolve(Platform::MacOs, Storage::Cache, &environment).unwrap(),
            library.join("Caches").join("Rastera")
        );
    }

    #[test]
    fn macos_keeps_an_existing_legacy_catalog_without_moving_files() {
        let home = PathBuf::from("/Users/ana");
        let current = home.join("Library/Application Support/Rastera/catalog.sqlite");
        let legacy = home.join(".local/share/Rastera/catalog.sqlite");
        let resolve_catalog = |present: &[PathBuf]| {
            resolve_catalog_path(
                Platform::MacOs,
                |name| (name == "HOME").then(|| OsString::from("/Users/ana")),
                |path| Ok(present.iter().any(|entry| entry == path)),
            )
            .unwrap()
        };
        assert_eq!(resolve_catalog(std::slice::from_ref(&legacy)), legacy);
        assert_eq!(resolve_catalog(&[current.clone(), legacy]), current);
        assert_eq!(resolve_catalog(&[]), current);
    }

    #[test]
    fn macos_legacy_catalog_uses_only_valid_xdg_paths() {
        let home = PathBuf::from("/Users/ana");
        let legacy_default = home.join(".local/share/Rastera/catalog.sqlite");
        for xdg in ["", "relative/data", "/custom/data"] {
            let expected = if xdg.starts_with('/') {
                PathBuf::from(xdg).join("Rastera/catalog.sqlite")
            } else {
                legacy_default.clone()
            };
            let resolved = resolve_catalog_path(
                Platform::MacOs,
                |name| match name {
                    "HOME" => Some(OsString::from("/Users/ana")),
                    "XDG_DATA_HOME" => Some(OsString::from(xdg)),
                    _ => None,
                },
                |path| Ok(path == expected),
            )
            .unwrap();
            assert_eq!(resolved, expected);
        }
    }

    #[cfg(unix)]
    #[test]
    fn unix_user_roots_preserve_non_unicode_path_bytes() {
        use std::os::unix::ffi::OsStringExt;
        let home = OsString::from_vec(b"/home/user-\xff".to_vec());
        let resolved = resolve_root(Platform::Linux, Storage::Data, |name| {
            (name == "HOME").then(|| home.clone())
        })
        .unwrap();
        assert_eq!(resolved, PathBuf::from(home).join(".local/share/Rastera"));
    }
}
