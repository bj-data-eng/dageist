//! Host-independent core of the dageist visual DAG studio.
//!
//! This crate owns application behavior that every host adapter shares. Host
//! adapters, such as `dageist-tauri`, depend on it and translate its contracts
//! for their window system; it never depends on a host.
//!
//! It holds the [`Session`]: the graph being edited (from `dageist-graph`),
//! its layout, and each transform's step document (from `dageist-engine`),
//! each at a revision of its own. [`Session::work_out`] works the graph out
//! node by node through the engine, without reading rows, and reports each
//! node's columns and [problems](Problem).

#![forbid(unsafe_code)]

mod session;
mod walk;

pub use session::{DocumentError, Session, Stale};
pub use walk::{Bindings, Problem, Worked, Workings};

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
