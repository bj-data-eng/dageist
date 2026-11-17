//! Where the application keeps its cache, without Tauri.
//!
//! Tauri puts an application's cache in a folder named for its identifier inside the platform's
//! cache folder. The bridge keeps the sample table in the same place, so that the browser and the
//! application share the one sample. A test holds this to what Tauri answers.

use std::path::PathBuf;

/// The folder of the platform that holds the caches of applications.
#[cfg(target_os = "macos")]
fn platform_cache() -> Option<PathBuf> {
    std::env::home_dir().map(|home| home.join("Library").join("Caches"))
}

/// The folder of the platform that holds the caches of applications.
#[cfg(windows)]
fn platform_cache() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
}

/// The folder of the platform that holds the caches of applications.
#[cfg(not(any(target_os = "macos", windows)))]
fn platform_cache() -> Option<PathBuf> {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|folder| folder.is_absolute())
        .or_else(|| std::env::home_dir().map(|home| home.join(".cache")))
}

/// The cache folder of the application whose Tauri identifier is `identifier`, or `None` when
/// the platform's cache folder is not known.
pub fn application_cache(identifier: &str) -> Option<PathBuf> {
    platform_cache().map(|cache| cache.join(identifier))
}
