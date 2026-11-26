//! Workspaces made, saved, opened and packed, read back by the workspace and checked by git
//! itself: a graph reads back as it was saved, each save is one commit, the files on disk are what
//! git sees as the latest commit, the repository is packed as it goes, and a node's history is the
//! commits that changed its folder.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use dageist_graph::{
    Edit, File, Format, Frame, Graph, InputKind, InputName, Kind, Layout, Name, NodeId, Parameter,
    ParameterName, Place, Reference, Size, Slot,
};
use dageist_workspace::{Author, Options, Saved, Workspace, WorkspaceError};

/// A folder of its own for each test, removed when the test ends.
struct Folder(PathBuf);

impl std::ops::Deref for Folder {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn folder(name: &str) -> Folder {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let folder = std::env::temp_dir().join(format!(
        "dageist-workspace-{name}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).expect("a test folder");
    // As the system names it, as a file dialog gives paths inside it.
    Folder(folder.canonicalize().expect("the test folder"))
}

fn author() -> Author {
    Author {
        name: "Test".to_owned(),
        email: "test@example.com".to_owned(),
    }
}

fn edit(graph: &Graph, edit: Edit) -> Graph {
    graph.edit(&edit).expect("the edit is made").graph
}

fn add(graph: &Graph, kind: Kind, name: &str) -> (Graph, NodeId) {
    let id = NodeId::mint();
    let graph = edit(
        graph,
        Edit::Add {
            id: id.clone(),
            kind,
            follows: None,
            name: Some(Name::new(name).expect("a name")),
        },
    );
    (graph, id)
}

/// The ids of the sample's nodes.
struct Nodes {
    claims: NodeId,
    calendar: NodeId,
    parameters: NodeId,
    timing: NodeId,
    output: NodeId,
}

/// A graph with a node of every kind and a reference of every kind: two sources, one whose file
/// lies inside `root` and one outside it, a Parameters node, a transform reading all three and
/// an output of the transform's table, with a box placed with and without a size, and the
/// transform's document.
fn sample(root: &Path) -> (Saved, Nodes) {
    let (graph, claims) = add(&Graph::new(), Kind::Source, "Claims");
    let graph = edit(
        &graph,
        Edit::SetFile {
            node: claims.clone(),
            file: Some(File {
                path: root
                    .join("data")
                    .join("claims.parquet")
                    .to_string_lossy()
                    .into_owned(),
                format: Format::Parquet,
            }),
        },
    );
    let (graph, calendar) = add(&graph, Kind::Source, "Holiday calendar");
    let graph = edit(
        &graph,
        Edit::SetFile {
            node: calendar.clone(),
            file: Some(File {
                path: "/elsewhere/holidays.csv".to_owned(),
                format: Format::Csv,
            }),
        },
    );
    let (graph, parameters) = add(&graph, Kind::Parameters, "Parameters");
    let graph = edit(
        &graph,
        Edit::SetParameter {
            node: parameters.clone(),
            name: ParameterName::new("as_of").expect("a name"),
            parameter: Parameter {
                dtype: "Date".to_owned(),
                text: "2026-03-02".to_owned(),
            },
        },
    );
    let (graph, timing) = add(&graph, Kind::Transform, "Claim timing");
    let graph = edit(
        &graph,
        Edit::Connect {
            node: timing.clone(),
            slot: Slot::Start,
            from: Reference::Table {
                node: claims.clone(),
            },
        },
    );
    let mut graph = graph;
    for (name, kind, from) in [
        (
            "holidays",
            InputKind::List,
            Some(Reference::Column {
                node: calendar.clone(),
                column: "date".to_owned(),
            }),
        ),
        (
            "as_of",
            InputKind::Value,
            Some(Reference::Parameter {
                node: parameters.clone(),
                name: ParameterName::new("as_of").expect("a name"),
            }),
        ),
        ("waiting", InputKind::Table, None),
    ] {
        let input = InputName::new(name).expect("an input name");
        graph = edit(
            &graph,
            Edit::AddInput {
                node: timing.clone(),
                name: input.clone(),
                kind,
            },
        );
        if let Some(from) = from {
            graph = edit(
                &graph,
                Edit::Connect {
                    node: timing.clone(),
                    slot: Slot::Input(input),
                    from,
                },
            );
        }
    }
    let (graph, output) = add(&graph, Kind::Output, "Claim timing output");
    let graph = edit(
        &graph,
        Edit::Connect {
            node: output.clone(),
            slot: Slot::Written,
            from: Reference::Table {
                node: timing.clone(),
            },
        },
    );
    let mut layout = Layout::new();
    layout
        .set(
            &graph,
            &claims,
            Frame {
                place: Place { x: -40, y: 12 },
                size: None,
            },
        )
        .expect("a box");
    layout
        .set(
            &graph,
            &timing,
            Frame {
                place: Place { x: 400, y: 0 },
                size: Some(Size {
                    width: 320,
                    height: 400,
                }),
            },
        )
        .expect("a box");
    let documents = BTreeMap::from([(
        timing.clone(),
        "{\"version\": 1, \"steps\": []}\n".to_owned(),
    )]);
    (
        Saved {
            graph,
            layout,
            documents,
        },
        Nodes {
            claims,
            calendar,
            parameters,
            timing,
            output,
        },
    )
}

/// Runs git in `repository` and gives what it printed.
fn git(repository: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(arguments)
        .output()
        .expect("git runs; these tests check the workspace with git and need it on the path");
    assert!(
        output.status.success(),
        "git {arguments:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("text")
}

fn commits(root: &Path) -> usize {
    git(root, &["rev-list", "--count", "HEAD"])
        .trim()
        .parse()
        .expect("a count")
}

fn rename(saved: &Saved, node: &NodeId, name: &str) -> Saved {
    Saved {
        graph: edit(
            &saved.graph,
            Edit::Rename {
                node: node.clone(),
                name: Name::new(name).expect("a name"),
            },
        ),
        ..saved.clone()
    }
}

#[test]
fn a_saved_graph_reads_back_as_it_was_with_its_layout_and_documents() {
    let root = folder("reads");
    let (saved, _) = sample(&root);
    Workspace::create(&root, &saved, author(), Options::default()).expect("the workspace is made");
    let (_, opened) =
        Workspace::open(&root, author(), Options::default()).expect("the workspace opens");
    assert_eq!(opened, saved);
}

#[test]
fn the_files_are_text_in_a_folder_for_each_node_with_paths_inside_the_workspace_relative() {
    let root = folder("files");
    let (saved, nodes) = sample(&root);
    Workspace::create(&root, &saved, author(), Options::default()).expect("the workspace is made");
    let read = |path: String| std::fs::read_to_string(root.join(path)).expect("the file");
    assert_eq!(
        read("workspace.json".to_owned()),
        "{\n  \"version\": 1\n}\n"
    );
    assert_eq!(read(".gitignore".to_owned()), ".dageist/\ndata/\n");
    let claims = read(format!("nodes/{}/node.json", nodes.claims));
    assert!(
        claims.contains("\"path\": \"data/claims.parquet\""),
        "{claims}"
    );
    let calendar = read(format!("nodes/{}/node.json", nodes.calendar));
    assert!(
        calendar.contains("\"path\": \"/elsewhere/holidays.csv\""),
        "{calendar}"
    );
    assert_eq!(
        read(format!("nodes/{}/steps.json", nodes.timing)),
        "{\"version\": 1, \"steps\": []}\n"
    );
    for node in [&nodes.parameters, &nodes.output] {
        assert!(root.join(format!("nodes/{node}/node.json")).is_file());
        assert!(!root.join(format!("nodes/{node}/steps.json")).exists());
    }
    // Git sees the files on disk as the latest commit holds them.
    assert_eq!(git(&root, &["status", "--porcelain"]), "");
}

#[test]
fn each_save_is_one_commit_and_a_save_that_changes_nothing_makes_none() {
    let root = folder("commits");
    let (saved, nodes) = sample(&root);
    let mut workspace = Workspace::create(&root, &saved, author(), Options::default())
        .expect("the workspace is made");
    assert_eq!(commits(&root), 1);
    let renamed = rename(&saved, &nodes.timing, "Document timing");
    let commit = workspace
        .save(&renamed, "Rename Claim timing to Document timing")
        .expect("saved");
    assert!(commit.is_some());
    assert_eq!(commits(&root), 2);
    assert_eq!(
        workspace.save(&renamed, "Nothing").expect("saved"),
        None,
        "nothing changed, so nothing is committed"
    );
    assert_eq!(commits(&root), 2);
    assert_eq!(
        git(&root, &["log", "-1", "--format=%s|%an|%ae"]).trim(),
        "Rename Claim timing to Document timing|Test|test@example.com"
    );
    assert_eq!(
        git(&root, &["rev-parse", "HEAD"]).trim(),
        commit.expect("a commit")
    );
    assert_eq!(git(&root, &["status", "--porcelain"]), "");
    let (_, opened) = Workspace::open(&root, author(), Options::default()).expect("opens");
    assert_eq!(
        opened
            .graph
            .node(&nodes.timing)
            .expect("the node")
            .name()
            .as_str(),
        "Document timing"
    );
}

#[test]
fn removing_a_node_removes_its_folder_from_the_commit_and_the_disk() {
    let root = folder("remove");
    let (saved, nodes) = sample(&root);
    let mut workspace = Workspace::create(&root, &saved, author(), Options::default())
        .expect("the workspace is made");
    let mut documents = saved.documents.clone();
    documents.remove(&nodes.timing);
    let graph = edit(
        &saved.graph,
        Edit::Remove {
            node: nodes.timing.clone(),
        },
    );
    let mut layout = saved.layout.clone();
    layout.keep_to(&graph);
    let removed = Saved {
        graph,
        layout,
        documents,
    };
    workspace
        .save(&removed, "Remove Claim timing")
        .expect("saved");
    assert!(!root.join(format!("nodes/{}", nodes.timing)).exists());
    let tracked = git(&root, &["ls-tree", "-r", "--name-only", "HEAD"]);
    assert!(!tracked.contains(nodes.timing.as_str()), "{tracked}");
    assert_eq!(git(&root, &["status", "--porcelain"]), "");
    let (_, opened) = Workspace::open(&root, author(), Options::default()).expect("opens");
    assert_eq!(opened, removed);
}

#[test]
fn a_nodes_history_is_the_commits_that_changed_its_folder_latest_first() {
    let root = folder("history");
    let (saved, nodes) = sample(&root);
    let mut workspace = Workspace::create(&root, &saved, author(), Options::default())
        .expect("the workspace is made");
    let renamed = rename(&saved, &nodes.timing, "Document timing");
    workspace
        .save(&renamed, "Rename Claim timing to Document timing")
        .expect("saved");
    // Moving a box changes the layout, not the node.
    let mut moved = renamed.clone();
    moved
        .layout
        .set(
            &moved.graph,
            &nodes.timing,
            Frame {
                place: Place { x: 0, y: 0 },
                size: None,
            },
        )
        .expect("a box");
    workspace
        .save(&moved, "Move Document timing")
        .expect("saved");
    let other = rename(&moved, &nodes.claims, "Documents");
    workspace
        .save(&other, "Rename Claims to Documents")
        .expect("saved");
    let mut stepped = other.clone();
    stepped.documents.insert(
        nodes.timing.clone(),
        "{\"version\": 1, \"steps\": [\"sort\"]}\n".to_owned(),
    );
    workspace
        .save(&stepped, "Add a sort to Document timing")
        .expect("saved");
    let history: Vec<String> = workspace
        .history(&nodes.timing)
        .expect("a history")
        .into_iter()
        .map(|change| change.message)
        .collect();
    assert_eq!(
        history,
        [
            "Add a sort to Document timing",
            "Rename Claim timing to Document timing",
            "Make the workspace",
        ]
    );
    let claims: Vec<String> = workspace
        .history(&nodes.claims)
        .expect("a history")
        .into_iter()
        .map(|change| change.message)
        .collect();
    assert_eq!(claims, ["Rename Claims to Documents", "Make the workspace"]);
}

/// The number of loose objects and of packs, as git counts them.
fn counts(root: &Path) -> (usize, usize) {
    let counted = git(root, &["count-objects", "-v"]);
    let value = |key: &str| -> usize {
        counted
            .lines()
            .find_map(|line| line.strip_prefix(key))
            .expect("the count")
            .trim()
            .parse()
            .expect("a number")
    };
    (value("count:"), value("packs:"))
}

#[test]
fn the_repository_is_packed_as_it_goes_and_whole_when_closed() {
    let root = folder("packs");
    let (saved, nodes) = sample(&root);
    let options = Options { pack_every: 5 };
    let mut workspace = Workspace::create(&root, &saved, author(), options).expect("made");
    let mut current = saved;
    for edit in 1..=12 {
        current = rename(&current, &nodes.timing, &format!("Claim timing {edit}"));
        workspace
            .save(&current, &format!("Rename to {edit}"))
            .expect("saved");
    }
    // Thirteen commits, packed after the fifth and the tenth: what is loose is the last three
    // edits' objects, a commit, a tree, the nodes folder, the node's folder and its file each.
    let (loose, packs) = counts(&root);
    assert_eq!(packs, 2);
    assert_eq!(loose, 3 * 5);
    git(&root, &["fsck", "--full", "--no-dangling"]);
    workspace.close().expect("closed");
    assert_eq!(counts(&root), (0, 1));
    git(&root, &["fsck", "--full", "--no-dangling"]);
    let (_, opened) = Workspace::open(&root, author(), options).expect("opens");
    assert_eq!(
        opened
            .graph
            .node(&nodes.timing)
            .expect("the node")
            .name()
            .as_str(),
        "Claim timing 12"
    );
    assert_eq!(commits(&root), 13);
}

#[test]
fn opening_writes_again_the_files_on_disk_that_differ_from_the_latest_commit() {
    let root = folder("restore");
    let (saved, nodes) = sample(&root);
    Workspace::create(&root, &saved, author(), Options::default()).expect("made");
    let node = root.join(format!("nodes/{}/node.json", nodes.claims));
    let committed = std::fs::read(&node).expect("the file");
    std::fs::write(&node, b"changed on disk").expect("written");
    std::fs::create_dir_all(root.join("nodes/stray")).expect("a folder");
    Workspace::open(&root, author(), Options::default()).expect("opens");
    assert_eq!(std::fs::read(&node).expect("the file"), committed);
    assert!(!root.join("nodes/stray").exists());
    assert_eq!(git(&root, &["status", "--porcelain"]), "");
}

#[test]
fn a_workspace_is_made_only_in_an_empty_folder_and_opened_only_where_one_is() {
    let root = folder("empty");
    std::fs::create_dir_all(&*root).expect("a folder");
    std::fs::write(root.join("notes.txt"), b"mine").expect("written");
    let (saved, _) = sample(&root);
    assert!(matches!(
        Workspace::create(&root, &saved, author(), Options::default()),
        Err(WorkspaceError::NotEmpty(_))
    ));
    assert!(Workspace::open(&root, author(), Options::default()).is_err());
}

#[test]
fn a_document_for_a_node_that_is_not_a_transform_is_not_saved() {
    let root = folder("document");
    let (saved, nodes) = sample(&root);
    let mut wrong = saved.clone();
    wrong
        .documents
        .insert(nodes.claims.clone(), "{}\n".to_owned());
    assert!(matches!(
        Workspace::create(&root, &wrong, author(), Options::default()),
        Err(WorkspaceError::NotATransform(node)) if node == nodes.claims
    ));
}

/// Runs git in `repository` as a person using it directly would, with an identity of its own.
fn git_as_person(repository: &Path, arguments: &[&str]) -> String {
    let mut all = vec![
        "-c",
        "user.name=Person",
        "-c",
        "user.email=person@example.com",
    ];
    all.extend_from_slice(arguments);
    git(repository, &all)
}

#[test]
fn packing_whole_keeps_what_every_ref_reaches_and_stores_versions_as_deltas() {
    let root = folder("refs");
    let (saved, nodes) = sample(&root);
    let mut workspace =
        Workspace::create(&root, &saved, author(), Options::default()).expect("made");
    let mut current = saved;
    for edit in 1..=6 {
        current = rename(&current, &nodes.timing, &format!("Claim timing {edit}"));
        workspace
            .save(&current, &format!("Rename to {edit}"))
            .expect("saved");
    }
    // Someone uses git on the workspace: a branch with a commit of its own and a tag on it, then
    // back to the workspace's branch.
    let branch = git(&root, &["branch", "--show-current"]);
    git_as_person(&root, &["checkout", "-q", "-b", "experiment"]);
    std::fs::write(root.join("notes.txt"), b"an idea").expect("written");
    git_as_person(&root, &["add", "notes.txt"]);
    git_as_person(&root, &["commit", "-q", "-m", "An idea"]);
    git_as_person(&root, &["tag", "-a", "keep-me", "-m", "Keep me"]);
    let experiment = git(&root, &["rev-parse", "experiment"]);
    git_as_person(&root, &["checkout", "-q", branch.trim()]);
    workspace.close().expect("closed");
    assert_eq!(counts(&root), (0, 1));
    git(&root, &["fsck", "--full", "--no-dangling"]);
    assert_eq!(git(&root, &["rev-parse", "experiment"]), experiment);
    assert_eq!(git(&root, &["cat-file", "-t", "keep-me"]).trim(), "tag");
    assert_eq!(git(&root, &["show", "experiment:notes.txt"]), "an idea");
    // The node file's seven versions are a chain of six deltas, as git reads the pack.
    let pack = std::fs::read_dir(root.join(".git/objects/pack"))
        .expect("the packs")
        .map(|entry| entry.expect("an entry").path())
        .find(|path| path.extension().is_some_and(|extension| extension == "idx"))
        .expect("an index");
    let listing = git(
        &root,
        &["verify-pack", "-v", pack.to_str().expect("a path")],
    );
    assert!(
        listing
            .lines()
            .any(|line| line.starts_with("chain length = 6:")),
        "{listing}"
    );
}

#[test]
fn opening_leaves_other_files_alone_and_writes_the_index_again() {
    let root = folder("index");
    let (saved, _) = sample(&root);
    Workspace::create(&root, &saved, author(), Options::default()).expect("made");
    std::fs::write(root.join("nodes/.DS_Store"), b"a browser's file").expect("written");
    std::fs::remove_file(root.join(".git/index")).expect("the index is removed");
    Workspace::open(&root, author(), Options::default()).expect("opens");
    assert_eq!(
        std::fs::read(root.join("nodes/.DS_Store")).expect("still there"),
        b"a browser's file"
    );
    assert_eq!(
        git(&root, &["status", "--porcelain", "--untracked-files=no"]),
        ""
    );
}

#[test]
fn a_save_that_failed_after_its_commit_is_finished_by_the_next() {
    let root = folder("finish");
    let (saved, nodes) = sample(&root);
    let mut workspace =
        Workspace::create(&root, &saved, author(), Options::default()).expect("made");
    // A folder where the node's file is to be written makes the writing fail after the commit.
    let file = root.join(format!("nodes/{}/node.json", nodes.timing));
    std::fs::remove_file(&file).expect("removed");
    std::fs::create_dir(&file).expect("a folder in its place");
    let renamed = rename(&saved, &nodes.timing, "Document timing");
    assert!(workspace.save(&renamed, "Rename").is_err());
    assert_eq!(commits(&root), 2, "the commit was made");
    std::fs::remove_dir(&file).expect("the folder is removed");
    assert_eq!(workspace.save(&renamed, "Rename").expect("saved"), None);
    assert_eq!(commits(&root), 2);
    assert!(
        std::fs::read_to_string(&file)
            .expect("the file is written")
            .contains("Document timing")
    );
    assert_eq!(git(&root, &["status", "--porcelain"]), "");
}

#[test]
fn a_save_of_another_state_after_one_that_failed_brings_every_file_to_its_commit() {
    let root = folder("finish-other");
    let (saved, nodes) = sample(&root);
    let mut workspace =
        Workspace::create(&root, &saved, author(), Options::default()).expect("made");
    let file = root.join(format!("nodes/{}/node.json", nodes.timing));
    std::fs::remove_file(&file).expect("removed");
    std::fs::create_dir(&file).expect("a folder in its place");
    let renamed = rename(&saved, &nodes.timing, "Document timing");
    assert!(workspace.save(&renamed, "Rename").is_err());
    std::fs::remove_dir(&file).expect("the folder is removed");
    // The next edit is of another node: the transform's file is written all the same.
    let other = rename(&renamed, &nodes.claims, "Documents");
    assert!(
        workspace
            .save(&other, "Rename Claims")
            .expect("saved")
            .is_some()
    );
    assert!(
        std::fs::read_to_string(&file)
            .expect("the file is written")
            .contains("Document timing")
    );
    assert_eq!(git(&root, &["status", "--porcelain"]), "");
}

#[test]
fn packing_whole_keeps_what_git_still_uses_a_staged_file_a_stash_and_orig_head() {
    let root = folder("staged");
    let (saved, nodes) = sample(&root);
    let mut workspace =
        Workspace::create(&root, &saved, author(), Options::default()).expect("made");
    let mut current = saved;
    for edit in 1..=3 {
        current = rename(&current, &nodes.timing, &format!("Claim timing {edit}"));
        workspace
            .save(&current, &format!("Rename to {edit}"))
            .expect("saved");
    }
    // A stash of one file, then another file staged and not committed.
    std::fs::write(root.join("stashed.txt"), b"put aside").expect("written");
    git_as_person(&root, &["add", "stashed.txt"]);
    git_as_person(&root, &["stash", "-q"]);
    std::fs::write(root.join("staged.txt"), b"staged").expect("written");
    git_as_person(&root, &["add", "staged.txt"]);
    // A commit made with git and reset away, which only ORIG_HEAD and the reflog still name; the
    // file it held stays staged.
    git_as_person(&root, &["commit", "-q", "-m", "Staged and reset"]);
    git_as_person(&root, &["reset", "-q", "--soft", "HEAD~1"]);
    let orig = git(&root, &["rev-parse", "ORIG_HEAD"]);
    assert_eq!(
        git(&root, &["branch", "--contains", orig.trim()]).trim(),
        "",
        "no branch holds the reset commit"
    );
    workspace.close().expect("closed");
    assert_eq!(counts(&root), (0, 1));
    git(&root, &["fsck", "--full", "--no-dangling"]);
    assert_eq!(git(&root, &["show", "stash@{0}:stashed.txt"]), "put aside");
    assert_eq!(git(&root, &["show", ":staged.txt"]), "staged");
    assert_eq!(
        git(&root, &["cat-file", "-t", orig.trim()]).trim(),
        "commit"
    );
    git_as_person(&root, &["commit", "-q", "-m", "Staged"]);
}

/// Copies every file under `from` to `to`, leaving out `.git`.
fn copy_without_git(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("a folder");
    for entry in std::fs::read_dir(from).expect("the folder") {
        let entry = entry.expect("an entry");
        if entry.file_name() == ".git" {
            continue;
        }
        let target = to.join(entry.file_name());
        if entry.file_type().expect("a kind").is_dir() {
            copy_without_git(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copied");
        }
    }
}

#[test]
fn a_workspace_is_made_from_a_plain_folder_with_its_data_read_where_it_is_copied() {
    let base = folder("from-folder");
    // A plain folder of a workspace's files, as a sample is shipped: no repository, and its data
    // under data/, which the graph names by a path relative to the folder.
    let first = base.join("first");
    let (saved, nodes) = sample(&first);
    Workspace::create(&first, &saved, author(), Options::default()).expect("made");
    let plain = base.join("plain");
    copy_without_git(&first, &plain);
    std::fs::create_dir_all(plain.join("data").join("more")).expect("a folder");
    std::fs::write(plain.join("data").join("claims.parquet"), b"rows").expect("written");
    std::fs::write(plain.join("data").join("more").join("notes.csv"), b"a\n").expect("written");
    // What the format has no place for at the top is passed by, and a .gitignore of its own is
    // not read.
    std::fs::create_dir_all(plain.join(".git")).expect("a folder");
    std::fs::write(plain.join(".git").join("HEAD"), b"junk").expect("written");
    std::fs::create_dir_all(plain.join(".dageist")).expect("a folder");
    std::fs::write(plain.join(".dageist").join("state"), b"junk").expect("written");
    std::fs::write(plain.join(".gitignore"), b"everything\n").expect("written");

    let copy = base.join("copy");
    let (_, opened) =
        Workspace::create_from_folder(&plain, &copy, author(), Options::default()).expect("made");
    let copy = copy.canonicalize().expect("the folder");
    // The source's file is now read from the copy's data folder, which holds the copied file.
    let dageist_graph::Body::Source { file: Some(file) } =
        opened.graph.node(&nodes.claims).expect("the node").body()
    else {
        panic!("the source has no file");
    };
    assert_eq!(
        Path::new(&file.path),
        copy.join("data").join("claims.parquet")
    );
    assert_eq!(
        std::fs::read(copy.join("data").join("claims.parquet")).expect("copied"),
        b"rows"
    );
    assert_eq!(
        std::fs::read(copy.join("data").join("more").join("notes.csv")).expect("copied"),
        b"a\n"
    );
    assert!(!copy.join(".dageist").exists());
    assert_eq!(
        git(&copy, &["show", "HEAD:.gitignore"]),
        ".dageist/\ndata/\n"
    );
    let tracked = git(&copy, &["ls-tree", "-r", "--name-only", "HEAD"]);
    assert!(
        !tracked.contains("data/") && !tracked.contains(".dageist"),
        "{tracked}"
    );
    assert_eq!(opened.graph.len(), saved.graph.len());
    assert_eq!(opened.documents, saved.documents);
    // One commit, and git does not see the data.
    assert_eq!(commits(&copy), 1);
    assert_eq!(git(&copy, &["status", "--porcelain", "--ignored=no"]), "");
    assert_eq!(
        git(&copy, &["status", "--porcelain", "--ignored"]).trim(),
        "!! data/"
    );
    // A folder holding something else is refused, and so is making a copy where files are.
    std::fs::write(plain.join("notes.txt"), b"mine").expect("written");
    assert!(matches!(
        Workspace::create_from_folder(&plain, &base.join("again"), author(), Options::default()),
        Err(WorkspaceError::Format { path, .. }) if path == "notes.txt"
    ));
    assert!(!base.join("again").exists());
    std::fs::remove_file(plain.join("notes.txt")).expect("removed");
    // A failure after the workspace is made leaves the folder as it was found, here empty: a
    // symbolic link in the data is refused once the workspace is committed.
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("/elsewhere", plain.join("data").join("link")).expect("a link");
        let empty = base.join("empty");
        std::fs::create_dir_all(&empty).expect("a folder");
        assert!(matches!(
            Workspace::create_from_folder(&plain, &empty, author(), Options::default()),
            Err(WorkspaceError::Io { .. })
        ));
        assert_eq!(std::fs::read_dir(&empty).expect("the folder").count(), 0);
        std::fs::remove_file(plain.join("data").join("link")).expect("removed");
    }
    assert!(matches!(
        Workspace::create_from_folder(&plain, &copy, author(), Options::default()),
        Err(WorkspaceError::NotEmpty(_))
    ));
}
