//! The repository a workspace is: its commits, the files on disk that copy the latest one, and
//! the packing of its objects.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Display;
use std::path::{Path, PathBuf};

use dageist_graph::NodeId;
use gix::ObjectId;
use gix::object::Kind;

use crate::files::{self, Files};
use crate::{Saved, WorkspaceError};

/// Who the commits of a workspace are made by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Author {
    /// The name the commits carry.
    pub name: String,
    /// The email address the commits carry.
    pub email: String,
}

/// How a workspace keeps its repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// After how many commits the objects written since the last pack are packed.
    pub pack_every: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self { pack_every: 100 }
    }
}

/// One commit of a node's history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// The commit's id, in hexadecimal.
    pub commit: String,
    /// What the commit says it did: the first line of its message.
    pub message: String,
    /// When it was made, in seconds since 1970 began, in UTC.
    pub seconds: i64,
}

/// The key that keeps the commits of a pack apart from its files and folders.
const COMMITS: &str = ":commits";
/// The key of the tags of a pack.
const TAGS: &str = ":tags";

fn git(context: &'static str) -> impl Fn(&dyn Display) -> WorkspaceError {
    move |error| WorkspaceError::Git {
        context,
        message: error.to_string(),
    }
}

fn io(path: &Path) -> impl Fn(std::io::Error) -> WorkspaceError + '_ {
    move |error| WorkspaceError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    }
}

/// A workspace open in the application.
pub struct Workspace {
    root: PathBuf,
    repo: gix::Repository,
    author: Author,
    options: Options,
    /// The branch's latest commit, once there is one.
    head: Option<ObjectId>,
    /// Every file and folder of the latest commit, by its path; the top folder is `""`.
    tree: BTreeMap<String, (ObjectId, bool)>,
    /// The objects written since the last pack, each with the path it was written at, or
    /// [`COMMITS`] for a commit, in the order they were written.
    written: Vec<(String, ObjectId)>,
    /// How many commits were made since the last pack.
    since_pack: usize,
    /// Whether the files on disk and git's index match the latest commit; not while a save that
    /// failed after its commit is still to be finished.
    in_step: bool,
    /// The file in the workspace's `.dageist/` folder whose lock says the workspace is open here.
    /// The system lets the lock go when the file is closed, as the workspace is dropped or the
    /// process ends however it ends.
    _held: std::fs::File,
}

impl std::fmt::Debug for Workspace {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Workspace")
            .field("root", &self.root)
            .field("head", &self.head)
            .finish_non_exhaustive()
    }
}

impl Workspace {
    /// Makes a workspace in `root`, which is made when it is missing and must be empty, and
    /// saves `saved` as its first commit.
    ///
    /// # Errors
    ///
    /// When `root` holds anything, or the repository or a file cannot be written.
    pub fn create(
        root: &Path,
        saved: &Saved,
        author: Author,
        options: Options,
    ) -> Result<Self, WorkspaceError> {
        ensure_empty(root)?;
        std::fs::create_dir_all(root).map_err(io(root))?;
        let root = root.canonicalize().map_err(io(root))?;
        // What is saved is checked before the repository is made, so a refusal leaves the
        // folder empty.
        files::files_of(saved, &root)?;
        let held = hold(&root)?;
        let repo = gix::init(&root).map_err(|error| git("making the repository")(&error))?;
        let mut workspace = Self {
            root,
            repo,
            author,
            options,
            head: None,
            tree: BTreeMap::new(),
            written: Vec::new(),
            since_pack: 0,
            in_step: true,
            _held: held,
        };
        workspace.save(saved, "Make the workspace")?;
        Ok(workspace)
    }

    /// Makes a workspace in `root` from the files of a workspace in `source`, a folder that is
    /// not a repository, such as a sample shipped with the application, and copies the data files
    /// under its `data/` folder across. Paths of data files written relative to `source` are read
    /// against `root`, so the workspace made reads its own copies. `root` is made when it is
    /// missing and must be empty.
    ///
    /// `source`'s own `.gitignore` is not read: the workspace made has the one every workspace
    /// has. Its `.git` and `.dageist` folders at the top are passed by, and so are the files an
    /// operating system writes into a folder a person browses, wherever they are and in the data
    /// too: `.DS_Store`, `._` files, and `Thumbs.db` and `desktop.ini` in any case. A folder of
    /// one of those names is read as any other. A path to a data file
    /// outside its `data/` folder is read against `root` all the same, and finds no file there
    /// until one is put there. Symbolic links are refused, not followed.
    ///
    /// # Errors
    ///
    /// When `source`'s files are not of the format or describe a graph no edit could make,
    /// `root` holds anything, or a file cannot be read or written. Whatever the failure, `root` is
    /// left as it was found: missing, or empty.
    pub fn create_from_folder(
        source: &Path,
        root: &Path,
        author: Author,
        options: Options,
    ) -> Result<(Self, Saved), WorkspaceError> {
        let files = folder_files(source)?;
        ensure_empty(root)?;
        let existed = root.exists();
        let made = (|| {
            std::fs::create_dir_all(root).map_err(io(root))?;
            let canonical = root.canonicalize().map_err(io(root))?;
            let saved = files::saved_of(&files, &canonical)?;
            let workspace = Self::create(&canonical, &saved, author, options)?;
            copy_tree(&source.join(files::DATA), &canonical.join(files::DATA))?;
            Ok((workspace, saved))
        })();
        if made.is_err() {
            // Nothing of a workspace half made is left: the folder goes back to how it was.
            let _ = std::fs::remove_dir_all(root);
            if existed {
                let _ = std::fs::create_dir(root);
            }
        }
        made
    }

    /// Opens the workspace in `root` and reads what its branch's latest commit holds. The files on
    /// disk are a copy of the latest commit, which is what the workspace holds: a file of the
    /// commit that differs on disk is written again from it, a node folder the commit lacks is
    /// removed, and git's index is written again. Other files on disk are left as they are.
    ///
    /// # Errors
    ///
    /// When `root` is not a repository, its branch has no commit, or the commit's files are not
    /// of the format or describe a graph no edit could make; or when the workspace is open
    /// already, here or in another process ([`Held`]).
    ///
    /// [`Held`]: WorkspaceError::Held
    pub fn open(
        root: &Path,
        author: Author,
        options: Options,
    ) -> Result<(Self, Saved), WorkspaceError> {
        let root = root.canonicalize().map_err(io(root))?;
        // The lock is taken before the branch is read, so that what is read is not changed by a
        // process that held it. A refusal removes the `.dageist` folder when the lock made it,
        // so a folder that is no workspace is left as it was.
        let local = root.join(files::LOCAL);
        let made_local = !local.exists();
        let held = hold(&root)?;
        let opened = (|| {
            let repo = gix::open(&root).map_err(|error| git("opening the repository")(&error))?;
            let head = repo
                .head_id()
                .map_err(|_| WorkspaceError::NoCommit)?
                .detach();
            let mut workspace = Self {
                root: root.clone(),
                repo,
                author,
                options,
                head: Some(head),
                tree: BTreeMap::new(),
                written: Vec::new(),
                since_pack: 0,
                in_step: true,
                _held: held,
            };
            let tree_id = workspace.commit_tree(head)?;
            workspace.tree = workspace.read_tree(tree_id)?;
            let files = workspace.files()?;
            let saved = files::saved_of(&files, &workspace.root)?;
            workspace.restore(&files)?;
            workspace.write_index(tree_id)?;
            Ok((workspace, saved))
        })();
        // The workspace, and the lock with it, is let go on a refusal before its folder goes.
        if opened.is_err() && made_local {
            let _ = std::fs::remove_dir_all(&local);
        }
        opened
    }

    /// The folder the workspace is in.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Saves `saved` as a commit with `message`, and makes the files on disk match it. Gives the
    /// commit's id in hexadecimal, or nothing when `saved` is what the latest commit holds, in
    /// which case no commit is made.
    ///
    /// A save that fails after its commit is made leaves the commit, and the next save, of the
    /// same state or another, brings the files on disk and git's index to its own commit.
    ///
    /// # Errors
    ///
    /// When a document is held for a node that is not a transform, or the repository or a file
    /// cannot be written. Packing, which follows the commit every so many saves, can fail after
    /// the commit is made; the next save packs again.
    pub fn save(&mut self, saved: &Saved, message: &str) -> Result<Option<String>, WorkspaceError> {
        let files = files::files_of(saved, &self.root)?;
        let ids: BTreeMap<&String, ObjectId> = files
            .iter()
            .map(|(path, bytes)| Ok((path, blob_id(bytes)?)))
            .collect::<Result<_, WorkspaceError>>()?;
        let unchanged = self.head.is_some()
            && ids.len() == self.blobs().count()
            && ids
                .iter()
                .all(|(path, id)| self.tree.get(*path) == Some(&(*id, false)));
        if unchanged {
            if !self.in_step {
                // A save failed after its commit: the commit holds `saved`, and the files on
                // disk and the index are brought to it now.
                self.restore(&files)?;
                self.write_index(self.tree[""].0)?;
                self.in_step = true;
            }
            if self.since_pack >= self.options.pack_every {
                self.pack_written()?;
            }
            return Ok(None);
        }
        let mut editor = self
            .repo
            .edit_tree(ObjectId::empty_tree(gix::hash::Kind::Sha1))
            .map_err(|error| git("starting a tree")(&error))?;
        for (path, bytes) in &files {
            let id = ids[path];
            if self.tree.get(path) != Some(&(id, false)) {
                self.repo
                    .write_blob(bytes)
                    .map_err(|error| git("writing a file")(&error))?;
                self.written.push((path.clone(), id));
            }
            editor
                .upsert(path.as_str(), gix::object::tree::EntryKind::Blob, id)
                .map_err(|error| git("adding a file to the tree")(&error))?;
        }
        let tree_id = editor
            .write()
            .map_err(|error| git("writing the tree")(&error))?
            .detach();
        let tree = self.read_tree(tree_id)?;
        for (path, (id, is_tree)) in &tree {
            if *is_tree && self.tree.get(path) != Some(&(*id, true)) {
                self.written.push((path.clone(), *id));
            }
        }
        let commit = self.commit(tree_id, message)?;
        self.written.push((COMMITS.to_owned(), commit));
        let before = std::mem::replace(&mut self.tree, tree);
        self.head = Some(commit);
        self.since_pack += 1;
        if self.in_step {
            self.in_step = false;
            self.check_out(&files, &before)?;
        } else {
            // The save before this failed after its commit, so what is on disk is not known to
            // be `before`: every file is brought to this commit.
            self.restore(&files)?;
        }
        self.write_index(tree_id)?;
        self.in_step = true;
        if self.since_pack >= self.options.pack_every {
            self.pack_written()?;
        }
        Ok(Some(commit.to_string()))
    }

    /// Packs into one pack what the refs, git's index, the `*_HEAD` pseudo refs and the reflogs
    /// reach, and removes every other pack and every object held as a file of its own. The
    /// branch's line of commits comes first, oldest to latest, so that each file's versions are
    /// deltas against the version before; then whatever another branch, a tag, a stash or a
    /// merge's other parent reaches; then, as far as the repository can read them, what the
    /// index, the pseudo refs and the reflogs name. An object none of those reach is removed.
    ///
    /// # Errors
    ///
    /// When the repository cannot be read or the pack cannot be written, in which case nothing
    /// is removed.
    pub fn repack(&mut self) -> Result<(), WorkspaceError> {
        let Some(head) = self.head else {
            return Ok(());
        };
        let mut line = Vec::new();
        let mut next = Some(head);
        while let Some(commit) = next {
            line.push(commit);
            next = self.parent(commit)?;
        }
        line.reverse();
        let mut order = Vec::new();
        let mut seen = BTreeSet::new();
        let mut others = Vec::new();
        for commit in &line {
            self.gather_commit(*commit, &mut order, &mut seen)?;
            others.extend(self.parents(*commit)?);
        }
        let references = self
            .repo
            .references()
            .map_err(|error| git("listing the refs")(&error))?;
        for reference in references
            .all()
            .map_err(|error| git("listing the refs")(&error))?
        {
            let reference = reference.map_err(|error| git("reading a ref")(&error))?;
            if let Some(id) = reference.target().try_id() {
                others.push(id.to_owned());
            }
        }
        // What git's index holds, an operation's pseudo refs and the reflogs name are kept when
        // the repository still has them; what it has lost already is not looked for.
        self.reach(others, &mut order, &mut seen)?;
        // What git's index holds, an operation's pseudo refs and the reflogs name are kept as far
        // as the repository can read them: one it has lost, or cannot read all of, is passed by
        // rather than stopping the pack.
        let mut kept_if_held: BTreeSet<ObjectId> = self.pseudo_and_logged()?;
        if let Ok(index) = self.repo.open_index() {
            kept_if_held.extend(index.entries().iter().map(|entry| entry.id));
        }
        for id in kept_if_held {
            if seen.contains(&id) || self.repo.find_object(id).is_err() {
                continue;
            }
            let (length, before) = (order.len(), seen.clone());
            if self.reach(vec![id], &mut order, &mut seen).is_err() {
                order.truncate(length);
                seen = before;
            }
        }
        let packed = self.write_pack(&order)?;
        let packs = self.objects().join("pack");
        let kept = packed.file_stem().map(std::ffi::OsStr::to_os_string);
        for entry in std::fs::read_dir(&packs).map_err(io(&packs))? {
            let path = entry.map_err(io(&packs))?.path();
            if path.file_stem().map(std::ffi::OsStr::to_os_string) != kept {
                std::fs::remove_file(&path).map_err(io(&path))?;
            }
        }
        self.remove_loose(None)?;
        self.written.clear();
        self.since_pack = 0;
        self.reopen()
    }

    /// Adds to `order` every object `starts` reach that is not yet `seen`: a commit with its
    /// parents and its tree, a tag with what it names, a tree with what it holds, and a blob.
    fn reach(
        &self,
        mut starts: Vec<ObjectId>,
        order: &mut Vec<(String, ObjectId)>,
        seen: &mut BTreeSet<ObjectId>,
    ) -> Result<(), WorkspaceError> {
        while let Some(id) = starts.pop() {
            if seen.contains(&id) {
                continue;
            }
            let object = self
                .repo
                .find_object(id)
                .map_err(|error| git("reading an object to pack")(&error))?;
            match object.kind {
                Kind::Commit => {
                    starts.extend(self.parents(id)?);
                    self.gather_commit(id, order, seen)?;
                }
                Kind::Tag => {
                    seen.insert(id);
                    order.push((TAGS.to_owned(), id));
                    let tag = gix::objs::TagRef::from_bytes(&object.data, gix::hash::Kind::Sha1)
                        .map_err(|error| git("reading a tag")(&error))?;
                    starts.push(tag.target());
                }
                Kind::Tree => {
                    for (path, (entry, _)) in self.read_tree(id)? {
                        if seen.insert(entry) {
                            order.push((format!(":{id}/{path}"), entry));
                        }
                    }
                }
                Kind::Blob => {
                    seen.insert(id);
                    order.push((format!(":{id}"), id));
                }
            }
        }
        Ok(())
    }

    /// The ids the pseudo refs at the top of the git folder, such as `ORIG_HEAD` and
    /// `MERGE_HEAD`, and the lines of every reflog name.
    fn pseudo_and_logged(&self) -> Result<BTreeSet<ObjectId>, WorkspaceError> {
        let mut ids = BTreeSet::new();
        let mut take = |text: &str| {
            for word in text.split_whitespace() {
                if word.len() == 40
                    && let Ok(id) = ObjectId::from_hex(word.as_bytes())
                    && !id.is_null()
                {
                    ids.insert(id);
                }
            }
        };
        let folder = self.repo.path();
        for entry in std::fs::read_dir(folder).map_err(io(folder))? {
            let path = entry.map_err(io(folder))?.path();
            let is_pseudo = path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with("_HEAD"));
            if is_pseudo && let Ok(text) = std::fs::read_to_string(&path) {
                take(&text);
            }
        }
        let mut logs = vec![folder.join("logs")];
        while let Some(at) = logs.pop() {
            if at.is_dir() {
                for entry in std::fs::read_dir(&at).map_err(io(&at))? {
                    logs.push(entry.map_err(io(&at))?.path());
                }
            } else if let Ok(text) = std::fs::read_to_string(&at) {
                // Each line starts with the old id and the new one.
                for line in text.lines() {
                    take(
                        &line
                            .split_whitespace()
                            .take(2)
                            .collect::<Vec<_>>()
                            .join(" "),
                    );
                }
            }
        }
        Ok(ids)
    }

    /// Adds `commit` and every file and folder of its tree not yet `seen` to `order`.
    fn gather_commit(
        &self,
        commit: ObjectId,
        order: &mut Vec<(String, ObjectId)>,
        seen: &mut BTreeSet<ObjectId>,
    ) -> Result<(), WorkspaceError> {
        if !seen.insert(commit) {
            return Ok(());
        }
        order.push((COMMITS.to_owned(), commit));
        for (path, (id, _)) in self.read_tree(self.commit_tree(commit)?)? {
            if seen.insert(id) {
                order.push((path, id));
            }
        }
        Ok(())
    }

    /// Packs the whole history, as [`repack`](Self::repack) does, and closes the workspace.
    ///
    /// # Errors
    ///
    /// As [`repack`](Self::repack).
    pub fn close(mut self) -> Result<(), WorkspaceError> {
        self.repack()
    }

    /// The commits that changed `node`'s folder, latest first, read from the branch's line of
    /// commits.
    ///
    /// # Errors
    ///
    /// When the repository cannot be read.
    pub fn history(&self, node: &NodeId) -> Result<Vec<Change>, WorkspaceError> {
        let folder = files::node_folder(node);
        let mut changes = Vec::new();
        let mut next = self.head;
        while let Some(commit) = next {
            let parent = self.parent(commit)?;
            let now = self.entry(self.commit_tree(commit)?, &folder)?;
            let before = match parent {
                Some(parent) => self.entry(self.commit_tree(parent)?, &folder)?,
                None => None,
            };
            if now != before {
                let found = self
                    .repo
                    .find_commit(commit)
                    .map_err(|error| git("reading a commit")(&error))?;
                let message = found
                    .message()
                    .map_err(|error| git("reading a commit's message")(&error))?
                    .summary()
                    .to_string();
                let seconds = found
                    .time()
                    .map_err(|error| git("reading a commit's time")(&error))?
                    .seconds;
                changes.push(Change {
                    commit: commit.to_string(),
                    message,
                    seconds,
                });
            }
            next = parent;
        }
        Ok(changes)
    }

    // -- Reading the repository ------------------------------------------------------------

    fn objects(&self) -> PathBuf {
        self.repo.path().join("objects")
    }

    /// The files of the latest commit, by path.
    fn blobs(&self) -> impl Iterator<Item = (&String, &ObjectId)> {
        self.tree
            .iter()
            .filter(|(_, (_, is_tree))| !is_tree)
            .map(|(path, (id, _))| (path, id))
    }

    fn commit_tree(&self, commit: ObjectId) -> Result<ObjectId, WorkspaceError> {
        self.repo
            .find_commit(commit)
            .map_err(|error| git("reading a commit")(&error))?
            .tree_id()
            .map(gix::Id::detach)
            .map_err(|error| git("reading a commit's tree")(&error))
    }

    fn parent(&self, commit: ObjectId) -> Result<Option<ObjectId>, WorkspaceError> {
        Ok(self.parents(commit)?.into_iter().next())
    }

    fn parents(&self, commit: ObjectId) -> Result<Vec<ObjectId>, WorkspaceError> {
        Ok(self
            .repo
            .find_commit(commit)
            .map_err(|error| git("reading a commit")(&error))?
            .parent_ids()
            .map(gix::Id::detach)
            .collect())
    }

    /// Every file and folder under the tree `id`, by path, the tree itself at `""`.
    fn read_tree(
        &self,
        id: ObjectId,
    ) -> Result<BTreeMap<String, (ObjectId, bool)>, WorkspaceError> {
        let mut found = BTreeMap::new();
        let mut folders = vec![(String::new(), id)];
        while let Some((path, id)) = folders.pop() {
            found.insert(path.clone(), (id, true));
            let tree = self
                .repo
                .find_tree(id)
                .map_err(|error| git("reading a tree")(&error))?;
            let decoded = tree
                .decode()
                .map_err(|error| git("reading a tree")(&error))?;
            for entry in &decoded.entries {
                let name = entry.filename.to_string();
                let at = if path.is_empty() {
                    name
                } else {
                    format!("{path}/{name}")
                };
                if entry.mode.is_commit() {
                    // A submodule's commit, which this repository does not hold.
                    continue;
                }
                if entry.mode.is_tree() {
                    folders.push((at, entry.oid.to_owned()));
                } else {
                    found.insert(at, (entry.oid.to_owned(), false));
                }
            }
        }
        Ok(found)
    }

    /// The id of the file or folder at `path` in the tree `tree`, if it has one.
    fn entry(&self, tree: ObjectId, path: &str) -> Result<Option<ObjectId>, WorkspaceError> {
        Ok(self
            .repo
            .find_tree(tree)
            .map_err(|error| git("reading a tree")(&error))?
            .lookup_entry_by_path(path)
            .map_err(|error| git("reading a tree")(&error))?
            .map(|entry| entry.object_id()))
    }

    /// The bytes of every file of the latest commit.
    fn files(&self) -> Result<Files, WorkspaceError> {
        self.blobs()
            .map(|(path, id)| {
                let blob = self
                    .repo
                    .find_blob(*id)
                    .map_err(|error| git("reading a file")(&error))?;
                Ok((path.clone(), blob.data.clone()))
            })
            .collect()
    }

    // -- Writing the repository ------------------------------------------------------------

    fn commit(&self, tree: ObjectId, message: &str) -> Result<ObjectId, WorkspaceError> {
        let signature = gix::actor::Signature {
            name: self.author.name.as_str().into(),
            email: self.author.email.as_str().into(),
            time: gix::date::Time::now_local_or_utc(),
        };
        let mut time = gix::date::parse::TimeBuf::default();
        let signed = signature.to_ref(&mut time);
        self.repo
            .commit_as(signed, signed, "HEAD", message, tree, self.head)
            .map(gix::Id::detach)
            .map_err(|error| git("committing")(&error))
    }

    /// Writes the files of `files` that differ from `before`, and removes the files of `before`
    /// that `files` lacks, with any folder left empty.
    fn check_out(
        &self,
        files: &Files,
        before: &BTreeMap<String, (ObjectId, bool)>,
    ) -> Result<(), WorkspaceError> {
        for (path, bytes) in files {
            if before.get(path).map(|(id, _)| *id) != Some(blob_id(bytes)?) {
                self.write_file(path, bytes)?;
            }
        }
        for (path, (_, is_tree)) in before {
            if !is_tree && !files.contains_key(path) {
                self.remove_file(path)?;
            }
        }
        Ok(())
    }

    /// Writes again every file on disk that differs from `files`, removes the node folders on
    /// disk that `files` lacks, and removes a node's file or steps that `files` lacks from a
    /// folder it holds. Other files are left as they are.
    fn restore(&self, files: &Files) -> Result<(), WorkspaceError> {
        for (path, bytes) in files {
            let at = self.on_disk(path);
            if std::fs::read(&at).ok().as_deref() != Some(bytes.as_slice()) {
                self.write_file(path, bytes)?;
            }
        }
        let nodes = self.root.join("nodes");
        if nodes.is_dir() {
            for entry in std::fs::read_dir(&nodes).map_err(io(&nodes))? {
                let folder = entry.map_err(io(&nodes))?.path();
                // A file beside the node folders, such as one a file browser leaves, stays.
                if !folder.is_dir() {
                    continue;
                }
                let name = folder
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned());
                let held = name.is_some_and(|name| {
                    let prefix = format!("nodes/{name}/");
                    files.keys().any(|path| path.starts_with(&prefix))
                });
                if !held {
                    std::fs::remove_dir_all(&folder).map_err(io(&folder))?;
                    continue;
                }
                for leaf in [files::NODE, files::STEPS] {
                    let path = format!(
                        "nodes/{}/{leaf}",
                        folder.file_name().unwrap_or_default().to_string_lossy()
                    );
                    if !files.contains_key(&path) && self.on_disk(&path).is_file() {
                        self.remove_file(&path)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn on_disk(&self, path: &str) -> PathBuf {
        path.split('/')
            .fold(self.root.clone(), |at, part| at.join(part))
    }

    fn write_file(&self, path: &str, bytes: &[u8]) -> Result<(), WorkspaceError> {
        let at = self.on_disk(path);
        if let Some(folder) = at.parent() {
            std::fs::create_dir_all(folder).map_err(io(folder))?;
        }
        std::fs::write(&at, bytes).map_err(io(&at))
    }

    fn remove_file(&self, path: &str) -> Result<(), WorkspaceError> {
        let at = self.on_disk(path);
        match std::fs::remove_file(&at) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(io(&at)(error)),
        }
        let mut folder = at.parent();
        while let Some(at) = folder {
            if at == self.root || std::fs::remove_dir(at).is_err() {
                break;
            }
            folder = at.parent();
        }
        Ok(())
    }

    /// Writes the index git reads the files on disk against, from the tree `tree`.
    fn write_index(&self, tree: ObjectId) -> Result<(), WorkspaceError> {
        let mut index = self
            .repo
            .index_from_tree(&tree)
            .map_err(|error| git("making the index")(&error))?;
        index
            .write(gix::index::write::Options::default())
            .map_err(|error| git("writing the index")(&error))
    }

    // -- Packing -----------------------------------------------------------------------------

    /// Packs the objects written since the last pack, and removes them as loose files.
    fn pack_written(&mut self) -> Result<(), WorkspaceError> {
        let loose = self.loose()?;
        let mut order: Vec<(String, ObjectId)> = self
            .written
            .iter()
            .filter(|(_, id)| loose.contains(id))
            .cloned()
            .collect();
        let known: BTreeSet<ObjectId> = order.iter().map(|(_, id)| *id).collect();
        // A loose object the workspace did not write in this run, left by one before it, is
        // packed whole.
        order.extend(
            loose
                .iter()
                .filter(|id| !known.contains(*id))
                .map(|id| (format!(":{id}"), *id)),
        );
        if !order.is_empty() {
            let packed = self.write_pack(&order)?;
            let packed_ids: BTreeSet<ObjectId> = order.iter().map(|(_, id)| *id).collect();
            self.remove_loose(Some(&packed_ids))?;
            drop(packed);
        }
        self.written.clear();
        self.since_pack = 0;
        self.reopen()
    }

    /// Writes the objects of `order` into one pack, each against the object before it of the
    /// same path, and gives the pack's path. An object listed twice is packed once.
    fn write_pack(&self, order: &[(String, ObjectId)]) -> Result<PathBuf, WorkspaceError> {
        let mut seen = BTreeSet::new();
        let mut loaded: Vec<(Kind, Vec<u8>, Option<usize>)> = Vec::new();
        let mut last: BTreeMap<&str, usize> = BTreeMap::new();
        for (path, id) in order {
            if !seen.insert(*id) {
                continue;
            }
            let object = self
                .repo
                .find_object(*id)
                .map_err(|error| git("reading an object to pack")(&error))?;
            let base = last
                .get(path.as_str())
                .copied()
                .filter(|base| loaded[*base].0 == object.kind);
            last.insert(path, loaded.len());
            loaded.push((object.kind, object.data.clone(), base));
        }
        let objects: Vec<dageist_pack::Object<'_>> = loaded
            .iter()
            .map(|(kind, data, base)| dageist_pack::Object {
                kind: *kind,
                data,
                base: *base,
            })
            .collect();
        let written = dageist_pack::write_pack(
            &objects,
            &self.objects().join("pack"),
            dageist_pack::Options::default(),
        )
        .map_err(|error| git("writing a pack")(&error))?;
        // The branch points at what the pack holds, so git may keep it without a keep file.
        if let Some(keep) = &written.keep {
            std::fs::remove_file(keep).map_err(io(keep))?;
        }
        Ok(written.pack)
    }

    /// The ids of the objects held as files of their own.
    fn loose(&self) -> Result<BTreeSet<ObjectId>, WorkspaceError> {
        let objects = self.objects();
        let mut loose = BTreeSet::new();
        for folder in std::fs::read_dir(&objects).map_err(io(&objects))? {
            let folder = folder.map_err(io(&objects))?.path();
            let Some(prefix) = folder
                .file_name()
                .and_then(|name| name.to_str())
                .filter(|name| name.len() == 2 && name.bytes().all(|byte| byte.is_ascii_hexdigit()))
                .map(str::to_owned)
            else {
                continue;
            };
            for file in std::fs::read_dir(&folder).map_err(io(&folder))? {
                let file = file.map_err(io(&folder))?.path();
                if let Some(rest) = file.file_name().and_then(|name| name.to_str())
                    && let Ok(id) = ObjectId::from_hex(format!("{prefix}{rest}").as_bytes())
                {
                    loose.insert(id);
                }
            }
        }
        Ok(loose)
    }

    /// Removes the loose objects of `ids`, or every loose object when `ids` is nothing.
    fn remove_loose(&self, ids: Option<&BTreeSet<ObjectId>>) -> Result<(), WorkspaceError> {
        for id in self.loose()? {
            if ids.is_some_and(|ids| !ids.contains(&id)) {
                continue;
            }
            let hex = id.to_string();
            let folder = self.objects().join(&hex[..2]);
            let file = folder.join(&hex[2..]);
            std::fs::remove_file(&file).map_err(io(&file))?;
            // A folder left empty goes; one another object still uses stays.
            let _ = std::fs::remove_dir(&folder);
        }
        Ok(())
    }

    /// Opens the repository again, so that it reads the packs as they are now.
    fn reopen(&mut self) -> Result<(), WorkspaceError> {
        self.repo = gix::open(&self.root).map_err(|error| git("opening the repository")(&error))?;
        Ok(())
    }
}

/// Takes the lock that says the workspace in `root` is open here, in the file `.dageist/lock`.
///
/// # Errors
///
/// When another workspace holds it, in this process or another, or the file cannot be made.
fn hold(root: &Path) -> Result<std::fs::File, WorkspaceError> {
    let folder = root.join(files::LOCAL);
    std::fs::create_dir_all(&folder).map_err(io(&folder))?;
    let path = folder.join("lock");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(io(&path))?;
    // A lock let go can stay held for a moment: a process being started holds a copy of every
    // open file until it runs its program. So a lock held is asked for again for a while before
    // the workspace is taken to be open elsewhere.
    let deadline = std::time::Instant::now() + HOLD_WAIT;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(std::fs::TryLockError::WouldBlock) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(std::fs::TryLockError::WouldBlock) => {
                return Err(WorkspaceError::Held(root.to_path_buf()));
            }
            Err(std::fs::TryLockError::Error(error)) => return Err(io(&path)(error)),
        }
    }
}

/// How long a lock held by something else is asked for again before the workspace is taken to be
/// open elsewhere.
const HOLD_WAIT: std::time::Duration = std::time::Duration::from_secs(2);

/// Refuses `root` when it holds anything; a folder that is missing or empty is one a workspace
/// can be made in.
fn ensure_empty(root: &Path) -> Result<(), WorkspaceError> {
    if root.exists() {
        let mut entries = std::fs::read_dir(root).map_err(io(root))?;
        if entries.next().is_some() {
            return Err(WorkspaceError::NotEmpty(root.to_path_buf()));
        }
    }
    Ok(())
}

/// Refuses the entry at `path` when it is a symbolic link, which a workspace's folder does not
/// hold: a link would make a file of the workspace one that lies elsewhere.
fn refuse_link(kind: std::fs::FileType, path: &Path) -> Result<(), WorkspaceError> {
    if kind.is_symlink() {
        return Err(WorkspaceError::Io {
            path: path.to_path_buf(),
            message: "it is a symbolic link, which a workspace's folder does not hold".to_owned(),
        });
    }
    Ok(())
}

/// The files of the workspace written in `folder`, a folder that is not a repository, by their
/// paths in it. Its `.git`, `.dageist` and `data` folders at the top hold no file of the format
/// and are passed by; anywhere else, they are files the format has no place for. A file an
/// operating system writes there ([`files::written_by_the_system`]) is passed by wherever it is;
/// a folder of that name is read as any other.
fn folder_files(folder: &Path) -> Result<Files, WorkspaceError> {
    let mut found = Files::new();
    let mut folders = vec![(folder.to_path_buf(), String::new())];
    while let Some((at, path)) = folders.pop() {
        for entry in std::fs::read_dir(&at).map_err(io(&at))? {
            let entry = entry.map_err(io(&at))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let inside = if path.is_empty() {
                name.clone()
            } else {
                format!("{path}/{name}")
            };
            let kind = entry.file_type().map_err(io(&entry.path()))?;
            if (path.is_empty() && [".git", files::LOCAL, files::DATA].contains(&name.as_str()))
                || (!kind.is_dir() && files::written_by_the_system(&name))
            {
                continue;
            }
            refuse_link(kind, &entry.path())?;
            if kind.is_dir() {
                folders.push((entry.path(), inside));
            } else {
                found.insert(
                    inside,
                    std::fs::read(entry.path()).map_err(io(&entry.path()))?,
                );
            }
        }
    }
    Ok(found)
}

/// Copies every file under `from` to the same place under `to`, but for the files an operating
/// system writes ([`files::written_by_the_system`]); nothing when `from` is missing.
fn copy_tree(from: &Path, to: &Path) -> Result<(), WorkspaceError> {
    if !from.is_dir() {
        return Ok(());
    }
    std::fs::create_dir_all(to).map_err(io(to))?;
    for entry in std::fs::read_dir(from).map_err(io(from))? {
        let entry = entry.map_err(io(from))?;
        let kind = entry.file_type().map_err(io(&entry.path()))?;
        if !kind.is_dir() && files::written_by_the_system(&entry.file_name().to_string_lossy()) {
            continue;
        }
        let target = to.join(entry.file_name());
        refuse_link(kind, &entry.path())?;
        if kind.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target).map_err(io(&target))?;
        }
    }
    Ok(())
}

/// The id git gives `bytes` as a file.
fn blob_id(bytes: &[u8]) -> Result<ObjectId, WorkspaceError> {
    gix::objs::compute_hash(gix::hash::Kind::Sha1, Kind::Blob, bytes)
        .map_err(|error| git("naming a file")(&error))
}
