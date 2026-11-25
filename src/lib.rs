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
//!
//! A session is saved to a [workspace](workspace::Workspace), a folder of files
//! in a git repository, through [`Session::saved`], and opened from one through
//! [`Session::from_saved`]. Every edit is saved as a commit with the words
//! [`graph_message`], [`place_message`] and [`steps_message`] give, by the
//! identity [`unsigned_author`] gives until a developer signs in.

#![forbid(unsafe_code)]

mod messages;
mod session;
mod walk;

/// The workspace on disk a session is saved to, from `dageist-workspace`.
pub mod workspace {
    pub use dageist_workspace::{Author, Change, Options, Saved, Workspace, WorkspaceError};
}

pub use messages::{graph_message, place_message, steps_message};
pub use session::{DocumentError, SavedError, Session, Stale};
pub use walk::{Bindings, Problem, Worked, Workings};

/// The identity a workspace's commits carry until a developer signs into GitHub in the
/// application. Its address is in the `.invalid` domain, which can never be registered, so no
/// account is credited with its commits.
#[must_use]
pub fn unsigned_author() -> workspace::Author {
    workspace::Author {
        name: "dageist".to_owned(),
        email: "workspace@dageist.invalid".to_owned(),
    }
}

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
