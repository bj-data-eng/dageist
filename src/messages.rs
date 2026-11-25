//! The words each edit is saved with: the message of the commit a workspace makes for it, in
//! the names the canvas shows, so that a node's history reads as what was done.

use std::path::Path;

use dageist_graph::{Edit, Graph, NodeId, Reference, Slot};

/// The most characters of a value a message shows.
const SHOWN: usize = 40;

/// `text` on one line, its runs of spaces and line breaks as one space, and cut to the first
/// [`SHOWN`] characters with `…` after them when it is longer.
fn short(text: &str) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() > SHOWN {
        format!("{}…", line.chars().take(SHOWN).collect::<String>())
    } else {
        line
    }
}

/// The name `node` shows in `graph`, or its id when the graph has no such node.
fn name(graph: &Graph, node: &NodeId) -> String {
    graph
        .node(node)
        .map_or_else(|| node.to_string(), |node| node.name().to_string())
}

/// What `reference` reads, in words.
fn read(graph: &Graph, reference: &Reference) -> String {
    match reference {
        Reference::Table { node } => name(graph, node),
        Reference::Column { node, column } => {
            format!("the column {} of {}", short(column), name(graph, node))
        }
        Reference::Parameter { node, name: value } => {
            format!("{} of {}", short(value.as_str()), name(graph, node))
        }
    }
}

/// `slot` of `node`, in words.
fn slot(graph: &Graph, node: &NodeId, slot: &Slot) -> String {
    let node = name(graph, node);
    match slot {
        Slot::Start => format!("the start of {node}"),
        Slot::Input(input) => format!("the input {} of {node}", short(input.as_str())),
        Slot::Written => node,
    }
}

/// The message a graph edit is saved with. `before` is the graph the edit was made on and
/// `after` the graph it made, so that a node is named as it was for a rename or a removal and
/// as it is for an addition.
#[must_use]
pub fn graph_message(edit: &Edit, before: &Graph, after: &Graph) -> String {
    match edit {
        Edit::Add { id, .. } => format!("Add {}", name(after, id)),
        Edit::Remove { node } => format!("Remove {}", name(before, node)),
        Edit::Rename { node, name: new } => {
            format!("Rename {} to {new}", name(before, node))
        }
        Edit::SetFile { node, file } => match file {
            Some(file) => format!(
                "Set the file of {} to {}",
                name(before, node),
                short(&Path::new(&file.path).file_name().map_or_else(
                    || file.path.clone(),
                    |file| file.to_string_lossy().into_owned()
                ))
            ),
            None => format!("Clear the file of {}", name(before, node)),
        },
        Edit::Connect {
            node,
            slot: filled,
            from,
        } => format!(
            "Connect {} to {}",
            read(before, from),
            slot(before, node, filled)
        ),
        Edit::ConnectNew { node, from } => format!(
            "Connect {} to a new input of {}",
            read(before, from),
            name(before, node)
        ),
        Edit::Disconnect {
            node,
            slot: emptied,
        } => {
            format!("Disconnect {}", slot(before, node, emptied))
        }
        Edit::AddInput {
            node, name: input, ..
        } => {
            format!(
                "Add the input {} to {}",
                short(input.as_str()),
                name(before, node)
            )
        }
        Edit::RemoveInput { node, name: input } => {
            format!(
                "Remove the input {} from {}",
                short(input.as_str()),
                name(before, node)
            )
        }
        Edit::SetParameter {
            node,
            name: value,
            parameter,
        } => format!(
            "Set {} of {} to {}",
            short(value.as_str()),
            name(before, node),
            short(&parameter.text)
        ),
        Edit::RemoveParameter { node, name: value } => {
            format!(
                "Remove {} from {}",
                short(value.as_str()),
                name(before, node)
            )
        }
    }
}

/// The message moving or sizing the box of `node` is saved with.
#[must_use]
pub fn place_message(graph: &Graph, node: &NodeId) -> String {
    format!("Move {}", name(graph, node))
}

/// The message an edit of the steps of the transform `node` is saved with.
#[must_use]
pub fn steps_message(graph: &Graph, node: &NodeId) -> String {
    format!("Change the steps of {}", name(graph, node))
}

#[cfg(test)]
mod tests {
    use dageist_graph::{
        Edit, File, Format, Graph, InputKind, InputName, Kind, Name, NodeId, Parameter,
        ParameterName, Reference, Slot,
    };

    use super::{graph_message, place_message, steps_message};

    const CLAIMS: &str = "0197a3c2-5e10-7d4b-9c1e-2b6f0a8d4e31";
    const TIMING: &str = "0197a3c2-7a44-7f02-8b3d-91c5e7f2a610";
    const VALUES: &str = "0197a3c3-0b19-7c88-a4e2-5d7f3b9c1e02";

    fn id(text: &str) -> NodeId {
        NodeId::new(text).unwrap()
    }

    fn graph() -> Graph {
        let mut graph = Graph::new();
        for (text, kind, name) in [
            (CLAIMS, Kind::Source, "Claims"),
            (TIMING, Kind::Transform, "Claim timing"),
            (VALUES, Kind::Parameters, "Parameters"),
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
        graph
    }

    /// The message `edit` is saved with, made on the sample graph.
    fn message(edit: Edit) -> String {
        let before = graph();
        let after = before.edit(&edit).unwrap().graph;
        graph_message(&edit, &before, &after)
    }

    #[test]
    fn each_edit_is_saved_with_what_it_did_in_the_names_the_canvas_shows() {
        let cutoff = ParameterName::new("cutoff").unwrap();
        let holidays = InputName::new("holidays").unwrap();
        for (edit, expected) in [
            (
                Edit::Add {
                    id: NodeId::mint(),
                    kind: Kind::Output,
                    follows: None,
                    name: None,
                },
                "Add Output",
            ),
            (Edit::Remove { node: id(CLAIMS) }, "Remove Claims"),
            (
                Edit::Rename {
                    node: id(TIMING),
                    name: Name::new("Document timing").unwrap(),
                },
                "Rename Claim timing to Document timing",
            ),
            (
                Edit::SetFile {
                    node: id(CLAIMS),
                    file: Some(File {
                        path: "/data/claims.parquet".to_owned(),
                        format: Format::Parquet,
                    }),
                },
                "Set the file of Claims to claims.parquet",
            ),
            (
                Edit::Connect {
                    node: id(TIMING),
                    slot: Slot::Start,
                    from: Reference::Table { node: id(CLAIMS) },
                },
                "Connect Claims to the start of Claim timing",
            ),
            (
                Edit::ConnectNew {
                    node: id(TIMING),
                    from: Reference::Column {
                        node: id(CLAIMS),
                        column: "date".to_owned(),
                    },
                },
                "Connect the column date of Claims to a new input of Claim timing",
            ),
            (
                Edit::AddInput {
                    node: id(TIMING),
                    name: holidays.clone(),
                    kind: InputKind::List,
                },
                "Add the input holidays to Claim timing",
            ),
            (
                Edit::SetParameter {
                    node: id(VALUES),
                    name: cutoff,
                    parameter: Parameter {
                        dtype: "Int64".to_owned(),
                        text: "3".to_owned(),
                    },
                },
                "Set cutoff of Parameters to 3",
            ),
        ] {
            assert_eq!(message(edit), expected);
        }
    }

    /// The sample graph with a transform reading the claims and an input `holidays`, and a value
    /// `cutoff`.
    fn connected() -> Graph {
        let mut graph = graph();
        for edit in [
            Edit::AddInput {
                node: id(TIMING),
                name: InputName::new("holidays").unwrap(),
                kind: InputKind::Value,
            },
            Edit::SetParameter {
                node: id(VALUES),
                name: ParameterName::new("cutoff").unwrap(),
                parameter: Parameter {
                    dtype: "Int64".to_owned(),
                    text: "3".to_owned(),
                },
            },
            Edit::Connect {
                node: id(TIMING),
                slot: Slot::Start,
                from: Reference::Table { node: id(CLAIMS) },
            },
        ] {
            graph = graph.edit(&edit).unwrap().graph;
        }
        graph
    }

    #[test]
    fn the_edits_that_empty_or_remove_are_saved_with_what_they_emptied_or_removed() {
        let before = connected();
        let holidays = InputName::new("holidays").unwrap();
        let cutoff = ParameterName::new("cutoff").unwrap();
        for (edit, expected) in [
            (
                Edit::Disconnect {
                    node: id(TIMING),
                    slot: Slot::Start,
                },
                "Disconnect the start of Claim timing",
            ),
            (
                Edit::Connect {
                    node: id(TIMING),
                    slot: Slot::Input(holidays.clone()),
                    from: Reference::Parameter {
                        node: id(VALUES),
                        name: cutoff.clone(),
                    },
                },
                "Connect cutoff of Parameters to the input holidays of Claim timing",
            ),
            (
                Edit::RemoveInput {
                    node: id(TIMING),
                    name: holidays,
                },
                "Remove the input holidays from Claim timing",
            ),
            (
                Edit::RemoveParameter {
                    node: id(VALUES),
                    name: cutoff,
                },
                "Remove cutoff from Parameters",
            ),
            (
                Edit::SetFile {
                    node: id(CLAIMS),
                    file: None,
                },
                "Clear the file of Claims",
            ),
        ] {
            let after = before.edit(&edit).unwrap().graph;
            assert_eq!(graph_message(&edit, &before, &after), expected);
        }
    }

    #[test]
    fn a_value_is_shown_on_one_line_and_cut_when_it_is_long() {
        let edit = Edit::SetParameter {
            node: id(VALUES),
            name: ParameterName::new("note").unwrap(),
            parameter: Parameter {
                dtype: "String".to_owned(),
                text: format!("first line\n  second {}", "x".repeat(60)),
            },
        };
        let shown = message(edit);
        assert_eq!(
            shown,
            format!(
                "Set note of Parameters to first line second {}…",
                "x".repeat(22)
            )
        );
        assert!(!shown.contains('\n'));
    }

    #[test]
    fn a_box_moved_and_steps_changed_are_saved_with_the_node_they_belong_to() {
        assert_eq!(place_message(&graph(), &id(TIMING)), "Move Claim timing");
        assert_eq!(
            steps_message(&graph(), &id(TIMING)),
            "Change the steps of Claim timing"
        );
    }
}
