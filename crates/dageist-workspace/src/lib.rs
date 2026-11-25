//! A dageist graph's workspace: a folder of text files in a git repository, where every edit is
//! saved as a commit.
//!
//! A workspace holds `workspace.json` with the version of its format, `layout.json` with the boxes
//! of every node, and a folder for each node under `nodes/`, named by the node's id, which holds
//! `node.json` and, for a transform, `steps.json`, its step document. Its `.dageist/` folder holds
//! the application's own state and is never committed.
//!
//! [`Workspace::save`] writes a graph, its layout and its documents as those files and commits
//! them as one commit on the branch, then makes the files on disk match it. Saving and committing
//! are one act: there is nothing saved that is not committed. [`Workspace::open`] reads the
//! branch's latest commit, not the files on disk, and reads the graph back by making the edits
//! that build it, so a graph read from a workspace passes the checks every edit passes.
//!
//! git keeps each new object as a file of its own until it is packed. A workspace packs the
//! objects of every so many commits as it goes, each file's version a delta against the version
//! before it, and [`Workspace::close`] packs what the refs, git's index, the pseudo refs and the reflogs reach into one
//! pack.
//!
//! A node's [history](Workspace::history) is the commits that changed its folder, read from the
//! branch's one line of commits.

#![forbid(unsafe_code)]

mod files;
mod store;

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

use dageist_graph::{Graph, Layout, NodeId};

pub use store::{Author, Change, Options, Workspace};

/// What a workspace saves and opens: a graph, the layout of its boxes, and the step document of
/// each transform, as the text the engine reads and writes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Saved {
    /// The graph.
    pub graph: Graph,
    /// Where its boxes stand.
    pub layout: Layout,
    /// Each transform's step document, by the transform's id.
    pub documents: BTreeMap<NodeId, String>,
}

/// Why a workspace could not be made, opened, saved or read.
#[derive(Debug)]
pub enum WorkspaceError {
    /// The folder a workspace is made in already holds something.
    NotEmpty(PathBuf),
    /// A file on disk could not be read or written.
    Io {
        /// The file or folder.
        path: PathBuf,
        /// What went wrong.
        message: String,
    },
    /// The repository could not be read or written.
    Git {
        /// What was being done.
        context: &'static str,
        /// What went wrong.
        message: String,
    },
    /// The branch has no commit, so there is no workspace to open.
    NoCommit,
    /// The workspace is written in another version of the format.
    Version(u32),
    /// A file of the workspace is not of its format, or describes a graph no edit could make.
    Format {
        /// The file, by its path in the workspace.
        path: String,
        /// Why.
        message: String,
    },
    /// A document is held for a node that is not a transform of the graph.
    NotATransform(NodeId),
    /// A data file's path is relative, which a workspace would read against itself.
    RelativePath {
        /// The source or output.
        node: NodeId,
        /// The path.
        path: String,
    },
}

impl fmt::Display for WorkspaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotEmpty(path) => write!(
                formatter,
                "{} already holds files, so no workspace is made there",
                path.display()
            ),
            Self::Io { path, message } => write!(formatter, "{}: {message}", path.display()),
            Self::Git { context, message } => write!(formatter, "{context}: {message}"),
            Self::NoCommit => formatter.write_str("the workspace's branch has no commit"),
            Self::Version(version) => write!(
                formatter,
                "the workspace is written in version {version} of its format, and this reads \
                 version {}",
                files::VERSION
            ),
            Self::Format { path, message } => write!(formatter, "{path}: {message}"),
            Self::NotATransform(node) => write!(
                formatter,
                "a document is held for {node}, which is not a transform of the graph"
            ),
            Self::RelativePath { node, path } => write!(
                formatter,
                "the file of {node} is at {path:?}, which is not a whole path"
            ),
        }
    }
}

impl std::error::Error for WorkspaceError {}
