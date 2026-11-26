//! Where the application keeps its cache and its data, without Tauri.
//!
//! Tauri puts an application's cache and its data each in a folder named for its identifier inside
//! the platform's folder for them. The bridge keeps the sample table in the same cache folder, so
//! that the browser and the application share the one table, and both keep the sample workspace in
//! the same data folder, found before Tauri starts. Tests hold both to what Tauri answers.

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

/// The folder of the platform that holds the data of applications.
#[cfg(target_os = "macos")]
fn platform_data() -> Option<PathBuf> {
    std::env::home_dir().map(|home| home.join("Library").join("Application Support"))
}

/// The folder of the platform that holds the data of applications.
#[cfg(windows)]
fn platform_data() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(PathBuf::from)
}

/// The folder of the platform that holds the data of applications.
#[cfg(not(any(target_os = "macos", windows)))]
fn platform_data() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|folder| folder.is_absolute())
        .or_else(|| std::env::home_dir().map(|home| home.join(".local").join("share")))
}

/// The cache folder of the application whose Tauri identifier is `identifier`, or `None` when
/// the platform's cache folder is not known.
pub fn application_cache(identifier: &str) -> Option<PathBuf> {
    platform_cache().map(|cache| cache.join(identifier))
}

/// The data folder of the application whose Tauri identifier is `identifier`, or `None` when the
/// platform's data folder is not known.
pub fn application_data(identifier: &str) -> Option<PathBuf> {
    platform_data().map(|data| data.join(identifier))
}

#[cfg(test)]
mod tests {
    use tauri::Manager;
    use tauri::test::{mock_builder, mock_context, noop_assets};

    #[test]
    fn the_cache_and_data_folders_are_the_ones_tauri_keeps_for_the_identifier() {
        let app = mock_builder()
            .build(mock_context(noop_assets()))
            .expect("the mock application builds");
        let identifier = app.config().identifier.clone();
        assert_eq!(
            super::application_cache(&identifier),
            Some(app.path().app_cache_dir().expect("Tauri knows its cache"))
        );
        assert_eq!(
            super::application_data(&identifier),
            Some(app.path().app_data_dir().expect("Tauri knows its data"))
        );
    }
}
