//! Host-independent core of the dageist visual DAG studio.
//!
//! This crate owns application behavior that every host adapter shares. Host
//! adapters, such as `dageist-tauri`, depend on it and translate its contracts
//! for their window system; it never depends on a host.

#![forbid(unsafe_code)]

/// Returns the package name of the dageist core.
#[must_use]
pub const fn crate_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

/// Returns the package version of the dageist core.
#[must_use]
pub const fn crate_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::{crate_name, crate_version};

    #[test]
    fn exposes_crate_identity() {
        assert_eq!(crate_name(), "dageist");
        assert_eq!(crate_version(), "0.1.0");
    }
}
