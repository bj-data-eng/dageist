//! The files a workspace holds, and how a graph, its layout and its documents are written to them
//! and read back.
//!
//! A graph is read back by making the edits that build it, so a graph read from files passes the
//! checks an edit on the canvas passes, and no file can hold a graph no edit could make.

use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::path::{Component, Path};

use dageist_graph::{
    Body, Edit, File, Format, Frame, Graph, Input, InputKind, InputName, Kind, Layout, Name,
    NodeId, Parameter, ParameterName, Place, Reference, Size, Slot,
};
use serde::de::{Error as _, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

use crate::{Saved, WorkspaceError};

/// The version of the files' format this build reads and writes.
pub(crate) const VERSION: u32 = 1;

/// The file that says which version of the format the workspace is written in.
pub(crate) const WORKSPACE: &str = "workspace.json";
/// The file that holds the boxes of every node.
pub(crate) const LAYOUT: &str = "layout.json";
/// The file that keeps git from the workspace's local folder.
pub(crate) const IGNORE: &str = ".gitignore";
/// The folder of the application's own state for this workspace, which is never committed.
pub(crate) const LOCAL: &str = ".dageist";
/// The folder that holds a folder for each node.
const NODES: &str = "nodes";
/// A node's own file, in its folder.
pub(crate) const NODE: &str = "node.json";
/// A transform's step document, in its folder.
pub(crate) const STEPS: &str = "steps.json";

/// The text of the `.gitignore` a workspace holds.
pub(crate) fn ignore_text() -> String {
    format!("{LOCAL}/\n")
}

/// Every file of a workspace, by its path in the workspace with `/` between folders.
pub(crate) type Files = BTreeMap<String, Vec<u8>>;

// -- The shapes of the files -----------------------------------------------------------------

/// Reads an optional value that must be written, as `null` when it is absent, so that a file
/// missing it is refused rather than read as having nothing.
fn written<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

/// Reads an object as a map, refusing a key written twice, which JSON readers would otherwise
/// read as the last of its values.
fn unique<'de, D, V>(deserializer: D) -> Result<BTreeMap<String, V>, D::Error>
where
    D: Deserializer<'de>,
    V: Deserialize<'de>,
{
    struct Once<V>(PhantomData<V>);

    impl<'de, V: Deserialize<'de>> Visitor<'de> for Once<V> {
        type Value = BTreeMap<String, V>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("an object with no key written twice")
        }

        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            let mut read = BTreeMap::new();
            while let Some((key, value)) = map.next_entry::<String, V>()? {
                if read.contains_key(&key) {
                    return Err(A::Error::custom(format!("{key:?} is written twice")));
                }
                read.insert(key, value);
            }
            Ok(read)
        }
    }

    deserializer.deserialize_map(Once(PhantomData))
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceFile {
    version: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum FormatFile {
    Parquet,
    Csv,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileFile {
    /// Relative to the workspace, with `/` between folders, when the file lies inside it, and
    /// as the system's file dialog gave it otherwise.
    path: String,
    format: FormatFile,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
enum ReferenceFile {
    Table { node: String },
    Column { node: String, column: String },
    Parameter { node: String, name: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum TakesFile {
    Table,
    Value,
    List,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InputFile {
    takes: TakesFile,
    #[serde(deserialize_with = "written")]
    from: Option<ReferenceFile>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ParameterFile {
    dtype: String,
    text: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
enum NodeFile {
    Source {
        name: String,
        #[serde(deserialize_with = "written")]
        file: Option<FileFile>,
    },
    Transform {
        name: String,
        #[serde(deserialize_with = "written")]
        start: Option<ReferenceFile>,
        #[serde(deserialize_with = "unique")]
        inputs: BTreeMap<String, InputFile>,
    },
    Output {
        name: String,
        #[serde(deserialize_with = "written")]
        written: Option<ReferenceFile>,
        #[serde(deserialize_with = "written")]
        file: Option<FileFile>,
    },
    Parameters {
        name: String,
        #[serde(deserialize_with = "unique")]
        values: BTreeMap<String, ParameterFile>,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FrameFile {
    x: i32,
    y: i32,
    #[serde(deserialize_with = "written")]
    width: Option<u32>,
    #[serde(deserialize_with = "written")]
    height: Option<u32>,
}

/// `value` as the file holds it: JSON laid out two spaces deep, ending with a new line.
fn text_of(value: &impl Serialize) -> Result<Vec<u8>, WorkspaceError> {
    let mut text = serde_json::to_vec_pretty(value).map_err(|error| WorkspaceError::Format {
        path: String::new(),
        message: error.to_string(),
    })?;
    text.push(b'\n');
    Ok(text)
}

fn read<'a, T: Deserialize<'a>>(path: &str, bytes: &'a [u8]) -> Result<T, WorkspaceError> {
    serde_json::from_slice(bytes).map_err(|error| WorkspaceError::Format {
        path: path.to_owned(),
        message: error.to_string(),
    })
}

fn refused(path: &str, message: impl std::fmt::Display) -> WorkspaceError {
    WorkspaceError::Format {
        path: path.to_owned(),
        message: message.to_string(),
    }
}

// -- Writing -----------------------------------------------------------------------------------

/// The folder of `node`.
pub(crate) fn node_folder(node: &NodeId) -> String {
    format!("{NODES}/{node}")
}

fn reference_file(reference: &Reference) -> ReferenceFile {
    match reference {
        Reference::Table { node } => ReferenceFile::Table {
            node: node.to_string(),
        },
        Reference::Column { node, column } => ReferenceFile::Column {
            node: node.to_string(),
            column: column.clone(),
        },
        Reference::Parameter { node, name } => ReferenceFile::Parameter {
            node: node.to_string(),
            name: name.to_string(),
        },
    }
}

/// `path` relative to `root`, with `/` between folders, when it lies inside it as plain folder
/// names, and as it is otherwise. A relative path is refused: it would be read back against the
/// workspace, which is not what it means.
fn path_in(root: &Path, node: &NodeId, path: &str) -> Result<String, WorkspaceError> {
    let given = Path::new(path);
    if !given.is_absolute() {
        return Err(WorkspaceError::RelativePath {
            node: node.clone(),
            path: path.to_owned(),
        });
    }
    Ok(match given.strip_prefix(root) {
        Ok(inside)
            if !inside.as_os_str().is_empty()
                && inside
                    .components()
                    .all(|part| matches!(part, Component::Normal(_))) =>
        {
            inside
                .components()
                .map(|part| part.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/")
        }
        _ => path.to_owned(),
    })
}

fn file_file(root: &Path, node: &NodeId, file: &File) -> Result<FileFile, WorkspaceError> {
    Ok(FileFile {
        path: path_in(root, node, &file.path)?,
        format: match file.format {
            Format::Parquet => FormatFile::Parquet,
            Format::Csv => FormatFile::Csv,
        },
    })
}

const fn takes_file(kind: InputKind) -> TakesFile {
    match kind {
        InputKind::Table => TakesFile::Table,
        InputKind::Value => TakesFile::Value,
        InputKind::List => TakesFile::List,
    }
}

fn node_file(
    root: &Path,
    id: &NodeId,
    name: &Name,
    body: &Body,
) -> Result<NodeFile, WorkspaceError> {
    let name = name.to_string();
    let file_of = |file: &Option<File>| {
        file.as_ref()
            .map(|file| file_file(root, id, file))
            .transpose()
    };
    Ok(match body {
        Body::Source { file } => NodeFile::Source {
            name,
            file: file_of(file)?,
        },
        Body::Transform { start, inputs } => NodeFile::Transform {
            name,
            start: start.as_ref().map(reference_file),
            inputs: inputs
                .iter()
                .map(|(input, Input { kind, from })| {
                    (
                        input.to_string(),
                        InputFile {
                            takes: takes_file(*kind),
                            from: from.as_ref().map(reference_file),
                        },
                    )
                })
                .collect(),
        },
        Body::Output { written, file } => NodeFile::Output {
            name,
            written: written.as_ref().map(reference_file),
            file: file_of(file)?,
        },
        Body::Parameters { values } => NodeFile::Parameters {
            name,
            values: values
                .iter()
                .map(|(value, Parameter { dtype, text })| {
                    (
                        value.to_string(),
                        ParameterFile {
                            dtype: dtype.clone(),
                            text: text.clone(),
                        },
                    )
                })
                .collect(),
        },
    })
}

/// Every file `saved` is written as, with the paths of data files inside `root` written relative
/// to it.
///
/// # Errors
///
/// When a document is held for a node that is not a transform of the graph, or a data file's
/// path is relative.
pub(crate) fn files_of(saved: &Saved, root: &Path) -> Result<Files, WorkspaceError> {
    let mut files = Files::new();
    files.insert(IGNORE.to_owned(), ignore_text().into_bytes());
    files.insert(
        WORKSPACE.to_owned(),
        text_of(&WorkspaceFile { version: VERSION })?,
    );
    let frames: BTreeMap<String, FrameFile> = saved
        .layout
        .frames()
        .map(|(node, frame)| {
            (
                node.to_string(),
                FrameFile {
                    x: frame.place.x,
                    y: frame.place.y,
                    width: frame.size.map(|size| size.width),
                    height: frame.size.map(|size| size.height),
                },
            )
        })
        .collect();
    files.insert(LAYOUT.to_owned(), text_of(&frames)?);
    for (id, node) in saved.graph.nodes() {
        let folder = node_folder(id);
        files.insert(
            format!("{folder}/{NODE}"),
            text_of(&node_file(root, id, node.name(), node.body())?)?,
        );
    }
    for (id, document) in &saved.documents {
        if saved.graph.node(id).map(dageist_graph::Node::kind) != Some(Kind::Transform) {
            return Err(WorkspaceError::NotATransform(id.clone()));
        }
        files.insert(
            format!("{}/{STEPS}", node_folder(id)),
            document.clone().into_bytes(),
        );
    }
    Ok(files)
}

// -- Reading -----------------------------------------------------------------------------------

fn node_id(path: &str, text: &str) -> Result<NodeId, WorkspaceError> {
    NodeId::new(text).map_err(|error| refused(path, error))
}

fn reference(path: &str, file: ReferenceFile) -> Result<Reference, WorkspaceError> {
    Ok(match file {
        ReferenceFile::Table { node } => Reference::Table {
            node: node_id(path, &node)?,
        },
        ReferenceFile::Column { node, column } => Reference::Column {
            node: node_id(path, &node)?,
            column,
        },
        ReferenceFile::Parameter { node, name } => Reference::Parameter {
            node: node_id(path, &node)?,
            name: ParameterName::new(name).map_err(|error| refused(path, error))?,
        },
    })
}

/// The file `file` names: a relative path is read against `root`.
fn chosen_file(root: &Path, path: &str, file: FileFile) -> Result<File, WorkspaceError> {
    let FileFile {
        path: written,
        format,
    } = file;
    let at = if Path::new(&written).is_absolute() {
        written
    } else {
        let mut at = root.to_path_buf();
        for part in written.split('/') {
            if part.is_empty() || part == "." || part == ".." {
                return Err(refused(
                    path,
                    format!("{written:?} is not a path inside the workspace"),
                ));
            }
            at.push(part);
        }
        at.to_string_lossy().into_owned()
    };
    Ok(File {
        path: at,
        format: match format {
            FormatFile::Parquet => Format::Parquet,
            FormatFile::Csv => Format::Csv,
        },
    })
}

const fn input_kind(takes: TakesFile) -> InputKind {
    match takes {
        TakesFile::Table => InputKind::Table,
        TakesFile::Value => InputKind::Value,
        TakesFile::List => InputKind::List,
    }
}

/// The edits that build a graph, in the order they are made: every node is added before any
/// node is set or connected, so that every reference names a node the graph has.
#[derive(Default)]
struct Plan {
    adds: Vec<(String, Edit)>,
    sets: Vec<(String, Edit)>,
    connects: Vec<(String, Edit)>,
}

impl Plan {
    fn node(
        &mut self,
        root: &Path,
        path: &str,
        id: &NodeId,
        file: NodeFile,
    ) -> Result<(), WorkspaceError> {
        let at = || path.to_owned();
        let (kind, name) = match &file {
            NodeFile::Source { name, .. } => (Kind::Source, name),
            NodeFile::Transform { name, .. } => (Kind::Transform, name),
            NodeFile::Output { name, .. } => (Kind::Output, name),
            NodeFile::Parameters { name, .. } => (Kind::Parameters, name),
        };
        let name = Name::new(name).map_err(|error| refused(path, error))?;
        if name.as_str() != file_name(&file) {
            return Err(refused(path, "a name is written without spaces around it"));
        }
        self.adds.push((
            at(),
            Edit::Add {
                id: id.clone(),
                kind,
                follows: None,
                name: Some(name),
            },
        ));
        let connect = |slot: Slot, from: Reference| Edit::Connect {
            node: id.clone(),
            slot,
            from,
        };
        match file {
            NodeFile::Source { file: chosen, .. } => {
                if let Some(chosen) = chosen {
                    self.sets.push((
                        at(),
                        Edit::SetFile {
                            node: id.clone(),
                            file: Some(chosen_file(root, path, chosen)?),
                        },
                    ));
                }
            }
            NodeFile::Transform { start, inputs, .. } => {
                if let Some(start) = start {
                    self.connects
                        .push((at(), connect(Slot::Start, reference(path, start)?)));
                }
                for (input, InputFile { takes, from }) in inputs {
                    let input = InputName::new(input).map_err(|error| refused(path, error))?;
                    self.sets.push((
                        at(),
                        Edit::AddInput {
                            node: id.clone(),
                            name: input.clone(),
                            kind: input_kind(takes),
                        },
                    ));
                    if let Some(from) = from {
                        self.connects
                            .push((at(), connect(Slot::Input(input), reference(path, from)?)));
                    }
                }
            }
            NodeFile::Output {
                written,
                file: chosen,
                ..
            } => {
                if let Some(chosen) = chosen {
                    self.sets.push((
                        at(),
                        Edit::SetFile {
                            node: id.clone(),
                            file: Some(chosen_file(root, path, chosen)?),
                        },
                    ));
                }
                if let Some(written) = written {
                    self.connects
                        .push((at(), connect(Slot::Written, reference(path, written)?)));
                }
            }
            NodeFile::Parameters { values, .. } => {
                for (value, ParameterFile { dtype, text }) in values {
                    self.sets.push((
                        at(),
                        Edit::SetParameter {
                            node: id.clone(),
                            name: ParameterName::new(value)
                                .map_err(|error| refused(path, error))?,
                            parameter: Parameter { dtype, text },
                        },
                    ));
                }
            }
        }
        Ok(())
    }

    /// The graph the edits build, or the first edit refused, with the file it came from.
    fn graph(self) -> Result<Graph, WorkspaceError> {
        let mut graph = Graph::new();
        for (path, edit) in self.adds.into_iter().chain(self.sets).chain(self.connects) {
            graph = graph
                .edit(&edit)
                .map_err(|error| refused(&path, error))?
                .graph;
        }
        Ok(graph)
    }
}

/// The name a node file holds, as written.
fn file_name(file: &NodeFile) -> &str {
    match file {
        NodeFile::Source { name, .. }
        | NodeFile::Transform { name, .. }
        | NodeFile::Output { name, .. }
        | NodeFile::Parameters { name, .. } => name,
    }
}

fn layout(graph: &Graph, bytes: &[u8]) -> Result<Layout, WorkspaceError> {
    let mut reader = serde_json::Deserializer::from_slice(bytes);
    let frames: BTreeMap<String, FrameFile> = unique(&mut reader)
        .and_then(|frames| reader.end().map(|()| frames))
        .map_err(|error| refused(LAYOUT, error))?;
    let mut layout = Layout::new();
    for (node, frame) in frames {
        let id = node_id(LAYOUT, &node)?;
        let size = match (frame.width, frame.height) {
            (Some(width), Some(height)) => Some(Size { width, height }),
            (None, None) => None,
            _ => {
                return Err(refused(
                    LAYOUT,
                    format!("the box of {node} has one of its width and height"),
                ));
            }
        };
        let place = Place {
            x: frame.x,
            y: frame.y,
        };
        layout
            .set(graph, &id, Frame { place, size })
            .map_err(|error| refused(LAYOUT, error))?;
    }
    Ok(layout)
}

/// The graph, layout and documents `files` hold, with the relative paths of data files read
/// against `root`.
///
/// # Errors
///
/// When a file the format needs is missing, a file is not of the format or is one it does not
/// have, or the graph the files describe is one no edit could make.
pub(crate) fn saved_of(files: &Files, root: &Path) -> Result<Saved, WorkspaceError> {
    let version: WorkspaceFile = read(
        WORKSPACE,
        files
            .get(WORKSPACE)
            .ok_or_else(|| refused(WORKSPACE, "the file is missing"))?,
    )?;
    if version.version != VERSION {
        return Err(WorkspaceError::Version(version.version));
    }
    let mut plan = Plan::default();
    let mut steps = BTreeMap::new();
    let mut folders = BTreeMap::<NodeId, bool>::new();
    for (path, bytes) in files {
        if [WORKSPACE, LAYOUT, IGNORE].contains(&path.as_str()) {
            continue;
        }
        let parts: Vec<&str> = path.split('/').collect();
        let [NODES, folder, leaf] = parts.as_slice() else {
            return Err(refused(path, "the format has no such file"));
        };
        let id = node_id(path, folder)?;
        match *leaf {
            NODE => {
                let node: NodeFile = read(path, bytes)?;
                plan.node(root, path, &id, node)?;
                folders.insert(id, true);
            }
            STEPS => {
                let text = String::from_utf8(bytes.clone())
                    .map_err(|_| refused(path, "the steps are not UTF-8 text"))?;
                steps.insert(id.clone(), (path.clone(), text));
                folders.entry(id).or_insert(false);
            }
            _ => return Err(refused(path, "the format has no such file")),
        }
    }
    if let Some((id, _)) = folders.iter().find(|(_, has_node)| !**has_node) {
        return Err(refused(
            &format!("{}/{NODE}", node_folder(id)),
            "the file is missing",
        ));
    }
    let graph = plan.graph()?;
    let mut documents = BTreeMap::new();
    for (id, (path, text)) in steps {
        if graph.node(&id).map(dageist_graph::Node::kind) != Some(Kind::Transform) {
            return Err(refused(&path, "only a transform has steps"));
        }
        documents.insert(id, text);
    }
    let layout = layout(
        &graph,
        files
            .get(LAYOUT)
            .ok_or_else(|| refused(LAYOUT, "the file is missing"))?,
    )?;
    Ok(Saved {
        graph,
        layout,
        documents,
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use dageist_graph::{Edit, Graph, Kind, Name, NodeId};

    use super::{Files, files_of, saved_of};
    use crate::{Saved, WorkspaceError};

    const ROOT: &str = "/workspace";
    const SOURCE: &str = "0197a3c2-5e10-7d4b-9c1e-2b6f0a8d4e31";
    const TRANSFORM: &str = "0197a3c2-7a44-7f02-8b3d-91c5e7f2a610";

    fn id(text: &str) -> NodeId {
        NodeId::new(text).unwrap()
    }

    /// The files of a source and a transform that starts from it, with the transform's steps.
    fn files() -> Files {
        let mut graph = Graph::new();
        for (text, kind, name) in [
            (SOURCE, Kind::Source, "Claims"),
            (TRANSFORM, Kind::Transform, "Timing"),
        ] {
            graph = graph
                .edit(&Edit::Add {
                    id: id(text),
                    kind,
                    follows: None,
                    name: Some(Name::new(name).unwrap()),
                })
                .unwrap()
                .graph;
        }
        let saved = Saved {
            graph,
            layout: dageist_graph::Layout::new(),
            documents: [(id(TRANSFORM), "{}\n".to_owned())].into(),
        };
        files_of(&saved, Path::new(ROOT)).unwrap()
    }

    fn node(of: &str) -> String {
        format!("nodes/{of}/node.json")
    }

    fn with(path: &str, text: &str) -> Files {
        let mut files = files();
        files.insert(path.to_owned(), text.as_bytes().to_vec());
        files
    }

    /// The file `files` is refused for, and why.
    fn refusal(files: &Files) -> (String, String) {
        match saved_of(files, Path::new(ROOT)) {
            Err(WorkspaceError::Format { path, message }) => (path, message),
            other => panic!("not refused as a file of another format: {other:?}"),
        }
    }

    #[test]
    fn the_files_written_read_back() {
        let saved = saved_of(&files(), Path::new(ROOT)).unwrap();
        assert_eq!(saved.graph.len(), 2);
        assert_eq!(saved.documents[&id(TRANSFORM)], "{}\n");
    }

    #[test]
    fn a_value_written_as_null_is_read_and_one_left_out_is_refused() {
        let missing = with(&node(SOURCE), r#"{ "kind": "source", "name": "Claims" }"#);
        let (path, message) = refusal(&missing);
        assert_eq!(path, node(SOURCE));
        assert!(message.contains("missing field `file`"), "{message}");
    }

    #[test]
    fn a_field_or_a_file_the_format_lacks_is_refused() {
        let field = with(
            &node(SOURCE),
            r#"{ "kind": "source", "name": "Claims", "file": null, "colour": "red" }"#,
        );
        assert!(refusal(&field).1.contains("unknown field `colour`"));
        let file = with("notes.txt", "mine");
        assert_eq!(refusal(&file).0, "notes.txt");
        let inside = with(&format!("nodes/{SOURCE}/extra.json"), "{}");
        assert_eq!(refusal(&inside).0, format!("nodes/{SOURCE}/extra.json"));
        let folder = with("nodes/not-an-id/node.json", "{}");
        assert!(refusal(&folder).1.contains("UUID"));
    }

    #[test]
    fn another_version_of_the_format_is_refused() {
        let files = with("workspace.json", r#"{ "version": 2 }"#);
        assert!(matches!(
            saved_of(&files, Path::new(ROOT)),
            Err(WorkspaceError::Version(2))
        ));
    }

    #[test]
    fn a_path_that_leaves_the_workspace_is_refused_and_one_inside_is_read_against_it() {
        let leaves = with(
            &node(SOURCE),
            r#"{ "kind": "source", "name": "Claims", "file": { "path": "../x.csv", "format": "csv" } }"#,
        );
        assert!(
            refusal(&leaves)
                .1
                .contains("not a path inside the workspace")
        );
        let inside = with(
            &node(SOURCE),
            r#"{ "kind": "source", "name": "Claims", "file": { "path": "data/x.csv", "format": "csv" } }"#,
        );
        let saved = saved_of(&inside, Path::new(ROOT)).unwrap();
        let dageist_graph::Body::Source { file: Some(file) } =
            saved.graph.node(&id(SOURCE)).unwrap().body()
        else {
            panic!("no file");
        };
        assert_eq!(
            Path::new(&file.path),
            Path::new(ROOT).join("data").join("x.csv")
        );
    }

    #[test]
    fn a_graph_no_edit_could_make_is_refused_with_the_file_it_comes_from() {
        let missing = with(
            &node(TRANSFORM),
            &format!(
                r#"{{ "kind": "transform", "name": "Timing", "start": {{ "kind": "table", "node": "{}" }}, "inputs": {{}} }}"#,
                "0197a3c3-0b19-7c88-a4e2-5d7f3b9c1e02"
            ),
        );
        let (path, message) = refusal(&missing);
        assert_eq!(path, node(TRANSFORM));
        assert!(message.contains("there is no node"), "{message}");
        let same_name = with(
            &node(TRANSFORM),
            r#"{ "kind": "transform", "name": "claims", "start": null, "inputs": {} }"#,
        );
        assert!(refusal(&same_name).1.contains("another node is named"));
        let spaced = with(
            &node(SOURCE),
            r#"{ "kind": "source", "name": " Claims", "file": null }"#,
        );
        assert_eq!(refusal(&spaced).0, node(SOURCE));
    }

    #[test]
    fn steps_belong_to_a_transform_beside_its_node_file() {
        let source_steps = with(&format!("nodes/{SOURCE}/steps.json"), "{}");
        assert_eq!(
            refusal(&source_steps).0,
            format!("nodes/{SOURCE}/steps.json")
        );
        let mut alone = files();
        alone.remove(&node(TRANSFORM));
        assert_eq!(
            refusal(&alone),
            (node(TRANSFORM), "the file is missing".to_owned())
        );
    }

    /// The graph of `files` with the source's file set to `path`.
    fn with_source_file(path: &str) -> Saved {
        let saved = saved_of(&files(), Path::new(ROOT)).unwrap();
        let graph = saved
            .graph
            .edit(&Edit::SetFile {
                node: id(SOURCE),
                file: Some(dageist_graph::File {
                    path: path.to_owned(),
                    format: dageist_graph::Format::Csv,
                }),
            })
            .unwrap()
            .graph;
        Saved { graph, ..saved }
    }

    #[test]
    fn a_relative_path_is_not_saved_and_one_that_leaves_the_workspace_is_saved_whole() {
        assert!(matches!(
            files_of(&with_source_file("data/x.csv"), Path::new(ROOT)),
            Err(WorkspaceError::RelativePath { node, path })
                if node == id(SOURCE) && path == "data/x.csv"
        ));
        for whole in ["/workspace/../x.csv", "/elsewhere/x.csv", "/workspace"] {
            let saved = with_source_file(whole);
            let written = files_of(&saved, Path::new(ROOT)).unwrap();
            let text = String::from_utf8(written[&node(SOURCE)].clone()).unwrap();
            assert!(text.contains(&format!("\"path\": \"{whole}\"")), "{text}");
            assert_eq!(saved_of(&written, Path::new(ROOT)).unwrap(), saved);
        }
    }

    #[test]
    fn a_key_written_twice_is_refused() {
        let inputs = with(
            &node(TRANSFORM),
            r#"{ "kind": "transform", "name": "Timing", "start": null, "inputs": {
                "a": { "takes": "table", "from": null },
                "a": { "takes": "list", "from": null } } }"#,
        );
        assert!(refusal(&inputs).1.contains("\"a\" is written twice"));
        let values = with(
            &node(SOURCE),
            r#"{ "kind": "parameters", "name": "Claims", "values": {
                "x": { "dtype": "Int64", "text": "1" },
                "x": { "dtype": "Int64", "text": "2" } } }"#,
        );
        assert!(refusal(&values).1.contains("\"x\" is written twice"));
        let layout = with(
            "layout.json",
            &format!(
                r#"{{ "{SOURCE}": {{ "x": 0, "y": 0, "width": null, "height": null }},
                     "{SOURCE}": {{ "x": 1, "y": 1, "width": null, "height": null }} }}"#
            ),
        );
        let (path, message) = refusal(&layout);
        assert_eq!(path, "layout.json");
        assert!(message.contains("is written twice"), "{message}");
    }

    #[test]
    fn a_box_with_one_of_its_width_and_height_is_refused() {
        let layout = with(
            "layout.json",
            &format!(r#"{{ "{SOURCE}": {{ "x": 0, "y": 0, "width": 10, "height": null }} }}"#),
        );
        let (path, message) = refusal(&layout);
        assert_eq!(path, "layout.json");
        assert!(message.contains("one of its width and height"), "{message}");
        let unknown = with(
            "layout.json",
            r#"{ "0197a3c3-0b19-7c88-a4e2-5d7f3b9c1e02": { "x": 0, "y": 0, "width": null, "height": null } }"#,
        );
        assert_eq!(refusal(&unknown).0, "layout.json");
    }
}
