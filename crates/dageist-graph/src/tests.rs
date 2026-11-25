use crate::{
    Body, Edit, EditError, File, Fit, Format, Frame, Gap, Graph, InputKind, InputName, Kind,
    Layout, Name, NodeId, Parameter, ParameterName, Place, Reference, Slot,
};

fn id(text: &str) -> NodeId {
    NodeId::new(text).unwrap()
}

fn table(node: &str) -> Reference {
    Reference::Table { node: id(node) }
}

/// Makes `edit` and returns the edited graph with the node it added.
fn edited(graph: &Graph, edit: Edit) -> (Graph, Option<NodeId>) {
    let edited = graph.edit(&edit).unwrap();
    (edited.graph, edited.added)
}

fn add(graph: &Graph, kind: Kind, follows: Option<&str>) -> (Graph, NodeId) {
    let (graph, added) = edited(
        graph,
        Edit::Add {
            kind,
            follows: follows.map(id),
        },
    );
    (graph, added.unwrap())
}

/// A source, a transform following it, and an output following the
/// transform.
fn chain() -> Graph {
    let (graph, _) = add(&Graph::new(), Kind::Source, None);
    let (graph, _) = add(&graph, Kind::Transform, Some("source"));
    let (graph, _) = add(&graph, Kind::Output, Some("transform"));
    graph
}

#[test]
fn added_nodes_are_named_after_their_kind_and_numbered_apart() {
    let (graph, first) = add(&Graph::new(), Kind::Source, None);
    let (graph, second) = add(&graph, Kind::Source, None);
    assert_eq!((first.as_str(), second.as_str()), ("source", "source-2"));
    assert_eq!(graph.node(&first).unwrap().name().as_str(), "Source");
    assert_eq!(graph.node(&second).unwrap().name().as_str(), "Source 2");
}

#[test]
fn a_following_node_reads_the_table_of_the_node_it_follows() {
    let graph = chain();
    let transform = graph.node(&id("transform")).unwrap();
    assert_eq!(
        transform.body(),
        &Body::Transform {
            start: Some(table("source")),
            inputs: Default::default(),
        }
    );
    let output = graph.node(&id("output")).unwrap();
    assert_eq!(
        output.slots(),
        vec![(Slot::Written, InputKind::Table, Some(&table("transform")))]
    );
}

#[test]
fn only_a_transform_or_an_output_can_follow_and_only_a_node_that_gives_a_table() {
    let (graph, _) = add(&Graph::new(), Kind::Parameters, None);
    let (graph, _) = add(&graph, Kind::Source, None);
    assert_eq!(
        graph.edit(&Edit::Add {
            kind: Kind::Source,
            follows: Some(id("source")),
        }),
        Err(EditError::ReadsNoTable(Kind::Source))
    );
    assert_eq!(
        graph.edit(&Edit::Add {
            kind: Kind::Transform,
            follows: Some(id("parameters")),
        }),
        Err(EditError::NoSuchOutput(table("parameters")))
    );
    assert_eq!(
        graph.edit(&Edit::Add {
            kind: Kind::Transform,
            follows: Some(id("missing")),
        }),
        Err(EditError::NoSuchNode(id("missing")))
    );
}

#[test]
fn a_refused_edit_leaves_the_graph_as_it_was() {
    let graph = chain();
    let before = graph.clone();
    assert!(
        graph
            .edit(&Edit::Connect {
                node: id("source"),
                slot: Slot::Start,
                from: table("output"),
            })
            .is_err()
    );
    assert_eq!(graph, before);
}

#[test]
fn renaming_keeps_the_id_and_refuses_a_name_another_node_has_in_any_case() {
    let graph = chain();
    let (renamed, _) = edited(
        &graph,
        Edit::Rename {
            node: id("source"),
            name: Name::new("Claims").unwrap(),
        },
    );
    assert_eq!(
        renamed.node(&id("source")).unwrap().name().as_str(),
        "Claims"
    );
    assert_eq!(
        renamed.edit(&Edit::Rename {
            node: id("transform"),
            name: Name::new("CLAIMS").unwrap(),
        }),
        Err(EditError::NameTaken(Name::new("CLAIMS").unwrap()))
    );
    // A node may take its own name in another case.
    assert!(
        renamed
            .edit(&Edit::Rename {
                node: id("source"),
                name: Name::new("claims").unwrap(),
            })
            .is_ok()
    );
}

#[test]
fn connecting_refuses_a_slot_of_another_kind_than_the_reference_gives() {
    let (graph, _) = add(&chain(), Kind::Parameters, None);
    let cutoff = ParameterName::new("cutoff").unwrap();
    let (graph, _) = edited(
        &graph,
        Edit::SetParameter {
            node: id("parameters"),
            name: cutoff.clone(),
            parameter: Parameter {
                dtype: "Int64".to_owned(),
                text: "1".to_owned(),
            },
        },
    );
    assert_eq!(
        graph.edit(&Edit::Connect {
            node: id("transform"),
            slot: Slot::Start,
            from: Reference::Parameter {
                node: id("parameters"),
                name: cutoff,
            },
        }),
        Err(EditError::KindMismatch {
            takes: InputKind::Table,
            gives: InputKind::Value,
        })
    );
}

#[test]
fn connecting_refuses_a_cycle_and_reading_itself() {
    let graph = chain();
    let (graph, _) = add(&graph, Kind::Transform, Some("transform"));
    assert_eq!(
        graph.edit(&Edit::Connect {
            node: id("transform"),
            slot: Slot::Start,
            from: table("transform-2"),
        }),
        Err(EditError::Cycle {
            node: id("transform"),
            from: id("transform-2"),
        })
    );
    assert_eq!(
        graph.edit(&Edit::Connect {
            node: id("transform"),
            slot: Slot::Start,
            from: table("transform"),
        }),
        Err(EditError::Cycle {
            node: id("transform"),
            from: id("transform"),
        })
    );
}

#[test]
fn connecting_refuses_a_slot_the_node_lacks_and_an_output_its_node_lacks() {
    let graph = chain();
    assert_eq!(
        graph.edit(&Edit::Connect {
            node: id("source"),
            slot: Slot::Start,
            from: table("transform"),
        }),
        Err(EditError::NoSuchSlot {
            node: id("source"),
            slot: Slot::Start,
        })
    );
    assert_eq!(
        graph.edit(&Edit::Connect {
            node: id("transform"),
            slot: Slot::Start,
            from: table("output"),
        }),
        Err(EditError::NoSuchOutput(table("output")))
    );
    assert_eq!(
        graph.edit(&Edit::Connect {
            node: id("transform"),
            slot: Slot::Input(InputName::new("missing").unwrap()),
            from: table("source"),
        }),
        Err(EditError::NoSuchSlot {
            node: id("transform"),
            slot: Slot::Input(InputName::new("missing").unwrap()),
        })
    );
}

#[test]
fn a_new_input_is_named_after_what_feeds_it_and_numbered_apart() {
    let (graph, _) = add(&chain(), Kind::Source, None);
    let (graph, _) = edited(
        &graph,
        Edit::Rename {
            node: id("source-2"),
            name: Name::new("SLA by step").unwrap(),
        },
    );
    let (graph, _) = edited(
        &graph,
        Edit::ConnectNew {
            node: id("transform"),
            from: table("source-2"),
        },
    );
    let (graph, _) = edited(
        &graph,
        Edit::ConnectNew {
            node: id("transform"),
            from: Reference::Column {
                node: id("source-2"),
                column: "SLA by step".to_owned(),
            },
        },
    );
    let names: Vec<_> = graph
        .node(&id("transform"))
        .unwrap()
        .slots()
        .into_iter()
        .map(|(slot, kind, _)| (slot, kind))
        .collect();
    assert_eq!(
        names,
        vec![
            (Slot::Start, InputKind::Table),
            (
                Slot::Input(InputName::new("sla_by_step").unwrap()),
                InputKind::Table
            ),
            (
                Slot::Input(InputName::new("sla_by_step_2").unwrap()),
                InputKind::List
            ),
        ]
    );
}

#[test]
fn a_column_reference_needs_a_column_name() {
    assert_eq!(
        chain().edit(&Edit::ConnectNew {
            node: id("transform"),
            from: Reference::Column {
                node: id("source"),
                column: String::new(),
            },
        }),
        Err(EditError::EmptyColumn)
    );
}

#[test]
fn removing_a_node_clears_the_references_to_it_and_retires_its_id() {
    let (graph, _) = edited(
        &chain(),
        Edit::Remove {
            node: id("transform"),
        },
    );
    assert!(graph.node(&id("transform")).is_none());
    assert_eq!(
        graph.gaps(&id("output")),
        vec![Gap::Unconnected(Slot::Written), Gap::NoFile]
    );
    let (graph, added) = add(&graph, Kind::Transform, None);
    assert_eq!(added.as_str(), "transform-2");
    assert_eq!(graph.node(&added).unwrap().name().as_str(), "Transform");
}

#[test]
fn removing_a_parameter_clears_the_inputs_that_read_it() {
    let (graph, _) = add(&chain(), Kind::Parameters, None);
    let cutoff = ParameterName::new("cutoff").unwrap();
    let (graph, _) = edited(
        &graph,
        Edit::SetParameter {
            node: id("parameters"),
            name: cutoff.clone(),
            parameter: Parameter {
                dtype: "Int64".to_owned(),
                text: "1".to_owned(),
            },
        },
    );
    let (graph, _) = edited(
        &graph,
        Edit::ConnectNew {
            node: id("transform"),
            from: Reference::Parameter {
                node: id("parameters"),
                name: cutoff.clone(),
            },
        },
    );
    let (graph, _) = edited(
        &graph,
        Edit::RemoveParameter {
            node: id("parameters"),
            name: cutoff,
        },
    );
    assert_eq!(
        graph.gaps(&id("transform")),
        vec![Gap::Unconnected(Slot::Input(
            InputName::new("cutoff").unwrap()
        ))]
    );
}

#[test]
fn files_belong_to_sources_and_outputs_only() {
    let file = File {
        path: "/data/claims.parquet".to_owned(),
        format: Format::Parquet,
    };
    let (graph, _) = edited(
        &chain(),
        Edit::SetFile {
            node: id("source"),
            file: Some(file.clone()),
        },
    );
    assert_eq!(graph.gaps(&id("source")), Vec::new());
    assert_eq!(
        graph.edit(&Edit::SetFile {
            node: id("transform"),
            file: Some(file),
        }),
        Err(EditError::WrongKind {
            node: id("transform"),
            kind: Kind::Transform,
        })
    );
}

#[test]
fn the_order_puts_every_node_after_the_nodes_it_reads() {
    // Added in an order unlike the order the reads ask for.
    let (graph, _) = add(&Graph::new(), Kind::Output, None);
    let (graph, _) = add(&graph, Kind::Transform, None);
    let (graph, _) = add(&graph, Kind::Source, None);
    let (graph, _) = edited(
        &graph,
        Edit::Connect {
            node: id("output"),
            slot: Slot::Written,
            from: table("transform"),
        },
    );
    let (graph, _) = edited(
        &graph,
        Edit::Connect {
            node: id("transform"),
            slot: Slot::Start,
            from: table("source"),
        },
    );
    let order: Vec<_> = graph.order().into_iter().map(NodeId::as_str).collect();
    assert_eq!(order, vec!["source", "transform", "output"]);
}

#[test]
fn what_fits_a_slot_leaves_out_the_nodes_that_read_it() {
    let (graph, _) = add(&chain(), Kind::Transform, Some("transform"));
    let (graph, _) = add(&graph, Kind::Parameters, None);
    let (graph, _) = edited(
        &graph,
        Edit::SetParameter {
            node: id("parameters"),
            name: ParameterName::new("as_of").unwrap(),
            parameter: Parameter {
                dtype: "Date".to_owned(),
                text: "2026-03-02".to_owned(),
            },
        },
    );
    assert_eq!(
        graph.fits(&id("transform"), Some(InputKind::Table)),
        vec![Fit::Table(id("source"))]
    );
    let value = Fit::Parameter(Reference::Parameter {
        node: id("parameters"),
        name: ParameterName::new("as_of").unwrap(),
    });
    assert_eq!(
        graph.fits(&id("transform"), Some(InputKind::Value)),
        vec![value.clone()]
    );
    assert_eq!(
        graph.fits(&id("transform"), None),
        vec![value, Fit::Table(id("source"))]
    );
}

#[test]
fn readers_name_the_slot_they_read_through() {
    let graph = chain();
    assert_eq!(
        graph.readers(&id("source")),
        vec![(&id("transform"), Slot::Start)]
    );
}

#[test]
fn a_layout_places_only_nodes_of_its_graph_and_drops_removed_ones() {
    let graph = chain();
    let mut layout = Layout::new();
    let frame = Frame {
        place: Place { x: 40, y: -20 },
        size: None,
    };
    layout.set(&graph, &id("transform"), frame).unwrap();
    assert!(layout.set(&graph, &id("missing"), frame).is_err());
    let (graph, _) = edited(
        &graph,
        Edit::Remove {
            node: id("transform"),
        },
    );
    layout.keep_to(&graph);
    assert_eq!(layout.frame(&id("transform")), None);
}
