//! The session works a graph out through the engine: tests on the small files
//! in `tests/corpus`, each shaped for the case it tests.

use dageist::{Problem, Session, Stale};
use dageist_engine::document::Document;
use dageist_graph::{
    Edit, File, Format, Frame, Gap, InputKind, Kind, Name, NodeId, Parameter, ParameterName, Place,
    Reference, Slot,
};

fn corpus(name: &str) -> String {
    format!("{}/tests/corpus/{name}", env!("CARGO_MANIFEST_DIR"))
}

/// Makes `edit` on the graph at its current revision and gives the node it
/// added, if any.
fn edit(session: &mut Session, edit: Edit) -> Option<NodeId> {
    let revision = session.graph_revision();
    session.edit_graph(revision, &edit).unwrap().unwrap()
}

/// Adds a source reading the CSV file `file` of the corpus, named `name`.
fn source(session: &mut Session, name: &str, file: &str) -> NodeId {
    let node = edit(
        session,
        Edit::Add {
            id: NodeId::mint(),
            kind: Kind::Source,
            follows: None,
            name: None,
        },
    )
    .unwrap();
    edit(
        session,
        Edit::Rename {
            node: node.clone(),
            name: Name::new(name).unwrap(),
        },
    );
    edit(
        session,
        Edit::SetFile {
            node: node.clone(),
            file: Some(File {
                path: corpus(file),
                format: Format::Csv,
            }),
        },
    );
    node
}

/// The columns `node` gives, as `name: dtype`.
fn columns(session: &Session, node: &NodeId) -> Vec<String> {
    session
        .work_out()
        .node(node)
        .unwrap()
        .columns
        .as_ref()
        .unwrap()
        .iter()
        .map(|column| format!("{}: {}", column.name(), column.dtype()))
        .collect()
}

fn problems(session: &Session, node: &NodeId) -> Vec<Problem> {
    session.work_out().node(node).unwrap().problems.clone()
}

/// Sets the steps of the transform `node` to the document `json`.
fn set_steps(session: &mut Session, node: &NodeId, json: &str) {
    let document = Document::read(json).unwrap();
    let revision = session.document(node).unwrap().1;
    session
        .edit_document(node, revision, |_| Ok::<_, ()>(document))
        .unwrap()
        .unwrap();
}

#[test]
fn a_source_gives_the_columns_of_its_file() {
    let mut session = Session::new(1);
    let orders = source(&mut session, "Orders", "orders.csv");
    assert_eq!(
        columns(&session, &orders),
        ["order_id: Int64", "customer: String", "amount: Int64"]
    );
    assert_eq!(problems(&session, &orders), []);
}

#[test]
fn a_source_without_a_file_or_with_one_that_cannot_be_read_says_so() {
    let mut session = Session::new(1);
    let empty = edit(
        &mut session,
        Edit::Add {
            id: NodeId::mint(),
            kind: Kind::Source,
            follows: None,
            name: None,
        },
    )
    .unwrap();
    assert_eq!(problems(&session, &empty), [Problem::Gap(Gap::NoFile)]);
    let missing = source(&mut session, "Missing", "no-such-file.csv");
    let found = problems(&session, &missing);
    assert!(
        matches!(found.as_slice(), [Problem::FileUnreadable(_)]),
        "{found:?}"
    );
    assert!(session.work_out().node(&missing).unwrap().columns.is_none());
}

#[test]
fn a_transform_with_no_steps_gives_the_table_it_starts_from_and_an_output_writes_it() {
    let mut session = Session::new(1);
    let orders = source(&mut session, "Orders", "orders.csv");
    let transform = edit(
        &mut session,
        Edit::Add {
            id: NodeId::mint(),
            kind: Kind::Transform,
            follows: Some(orders.clone()),
            name: None,
        },
    )
    .unwrap();
    let output = edit(
        &mut session,
        Edit::Add {
            id: NodeId::mint(),
            kind: Kind::Output,
            follows: Some(transform.clone()),
            name: None,
        },
    )
    .unwrap();
    assert_eq!(columns(&session, &transform), columns(&session, &orders));
    assert_eq!(columns(&session, &output), columns(&session, &orders));
    assert_eq!(problems(&session, &output), [Problem::Gap(Gap::NoFile)]);
}

/// A transform on orders that joins customers, compares each amount with the
/// parameter `cutoff`, and counts the holidays of a list.
const STEPS: &str = r#"{
  "version": 1,
  "steps": [
    {"id": "join", "call": {"entry": "LazyFrame.join", "arguments": {
      "other": {"table": "customers"},
      "left_on": {"many": [{"expression": {"start": {"entry": "polars.col", "arguments": {"name": {"columns": ["customer"]}}}, "then": []}}]},
      "right_on": {"many": [{"expression": {"start": {"entry": "polars.col", "arguments": {"name": {"columns": ["customer"]}}}, "then": []}}]},
      "how": {"choice": "left"}, "maintain_order": {"choice": "left"}}}},
    {"id": "large", "call": {"entry": "LazyFrame.with_columns", "arguments": {"named_exprs": {"pairs": [[{"columns": ["large"]},
      {"expression": {"start": {"entry": "polars.col", "arguments": {"name": {"columns": ["amount"]}}},
        "then": [{"entry": "Expr.gt", "arguments": {"other": {"expression": {"start": {"input": "cutoff"}, "then": []}}}}]}}]]}}}},
    {"id": "holiday_count", "call": {"entry": "LazyFrame.with_columns", "arguments": {"named_exprs": {"pairs": [[{"columns": ["holiday_count"]},
      {"expression": {"start": {"input": "holidays"}, "then": [{"entry": "Expr.len", "arguments": {}}]}}]]}}}}
  ]
}"#;

/// Orders, customers and holidays, a Parameters node with `cutoff`, and a
/// transform on orders whose inputs read them.
fn connected() -> (Session, NodeId, NodeId, NodeId) {
    let mut session = Session::new(1);
    let orders = source(&mut session, "Orders", "orders.csv");
    let customers = source(&mut session, "Customers", "customers.csv");
    let holidays = source(&mut session, "Holidays", "holidays.csv");
    let parameters = edit(
        &mut session,
        Edit::Add {
            id: NodeId::mint(),
            kind: Kind::Parameters,
            follows: None,
            name: None,
        },
    )
    .unwrap();
    edit(
        &mut session,
        Edit::SetParameter {
            node: parameters.clone(),
            name: ParameterName::new("cutoff").unwrap(),
            parameter: Parameter {
                dtype: "Int64".to_owned(),
                text: "100".to_owned(),
            },
        },
    );
    let transform = edit(
        &mut session,
        Edit::Add {
            id: NodeId::mint(),
            kind: Kind::Transform,
            follows: Some(orders),
            name: None,
        },
    )
    .unwrap();
    for from in [
        Reference::Table { node: customers },
        Reference::Column {
            node: holidays.clone(),
            column: "date".to_owned(),
        },
        Reference::Parameter {
            node: parameters.clone(),
            name: ParameterName::new("cutoff").unwrap(),
        },
    ] {
        edit(
            &mut session,
            Edit::ConnectNew {
                node: transform.clone(),
                from,
            },
        );
    }
    set_steps(&mut session, &transform, STEPS);
    (session, transform, holidays, parameters)
}

#[test]
fn a_transform_binds_tables_lists_and_values_to_the_names_its_steps_use() {
    let (session, transform, ..) = connected();
    assert_eq!(problems(&session, &transform), []);
    assert_eq!(
        columns(&session, &transform),
        [
            "order_id: Int64",
            "customer: String",
            "amount: Int64",
            "region: String",
            "large: Boolean",
            "holiday_count: UInt32",
        ]
    );
    let workings = session.work_out();
    let bindings = workings
        .node(&transform)
        .unwrap()
        .bindings
        .as_ref()
        .unwrap();
    assert_eq!(bindings.start_name, "Orders");
    let working = bindings.start.working(10).unwrap();
    assert_eq!(working.height(), 4);
}

#[test]
fn a_column_the_table_lacks_and_a_parameter_that_is_not_its_dtype_are_problems() {
    let (mut session, transform, holidays, parameters) = connected();
    edit(
        &mut session,
        Edit::Connect {
            node: transform.clone(),
            slot: Slot::Input(dageist_graph::InputName::new("holidays").unwrap()),
            from: Reference::Column {
                node: holidays,
                column: "day".to_owned(),
            },
        },
    );
    edit(
        &mut session,
        Edit::SetParameter {
            node: parameters.clone(),
            name: ParameterName::new("cutoff").unwrap(),
            parameter: Parameter {
                dtype: "Int64".to_owned(),
                text: "a hundred".to_owned(),
            },
        },
    );
    let found = problems(&session, &transform);
    assert!(found.contains(&Problem::NoSuchColumn {
        slot: Slot::Input(dageist_graph::InputName::new("holidays").unwrap()),
        column: "day".to_owned(),
    }));
    assert!(
        found
            .iter()
            .any(|problem| matches!(problem, Problem::InputUnreadable { .. })),
        "{found:?}"
    );
    assert!(
        problems(&session, &parameters)
            .iter()
            .any(|problem| matches!(problem, Problem::ParameterUnreadable { .. }))
    );
}

#[test]
fn a_refused_step_stops_the_transform_and_what_reads_it_gets_nothing() {
    let (mut session, transform, ..) = connected();
    edit(
        &mut session,
        Edit::RemoveInput {
            node: transform.clone(),
            name: dageist_graph::InputName::new("customers").unwrap(),
        },
    );
    let output = edit(
        &mut session,
        Edit::Add {
            id: NodeId::mint(),
            kind: Kind::Output,
            follows: Some(transform.clone()),
            name: None,
        },
    )
    .unwrap();
    assert_eq!(problems(&session, &transform), [Problem::StepsStopped]);
    assert!(
        problems(&session, &output).contains(&Problem::NothingGiven {
            slot: Slot::Written,
            from: transform,
        })
    );
}

#[test]
fn what_fits_a_list_input_is_any_table_before_the_transform() {
    let (session, transform, ..) = connected();
    let fits = session.graph().fits(&transform, Some(InputKind::List));
    assert_eq!(fits.len(), 3, "{fits:?}");
}

#[test]
fn an_edit_against_an_old_revision_is_stale_and_changes_nothing() {
    let mut session = Session::new(7);
    let before = session.graph().clone();
    assert_eq!(
        session.edit_graph(
            6,
            &Edit::Add {
                id: NodeId::mint(),
                kind: Kind::Source,
                follows: None,
                name: None,
            },
        ),
        Err(Stale {
            asked: 6,
            current: 7
        })
    );
    assert_eq!(session.graph(), &before);
}

#[test]
fn a_transform_has_a_document_of_its_own_and_removing_it_drops_its_document_and_place() {
    let mut session = Session::new(1);
    let transform = edit(
        &mut session,
        Edit::Add {
            id: NodeId::mint(),
            kind: Kind::Transform,
            follows: None,
            name: None,
        },
    )
    .unwrap();
    let (document, revision) = session.document(&transform).unwrap();
    assert!(document.steps().is_empty());
    assert_eq!(revision, 1);
    let frame = Frame {
        place: Place { x: 10, y: 20 },
        size: None,
    };
    let layout = session.layout_revision();
    session.place(layout, &transform, frame).unwrap().unwrap();
    assert_eq!(session.layout_revision(), layout + 1);
    // Moving the box leaves the graph's revision where it was.
    assert_eq!(session.graph_revision(), 2);
    edit(
        &mut session,
        Edit::Remove {
            node: transform.clone(),
        },
    );
    assert!(session.document(&transform).is_none());
    assert_eq!(session.layout().frame(&transform), None);
    assert_eq!(session.layout_revision(), layout + 2);
}

#[test]
fn a_document_edit_against_an_old_revision_is_stale() {
    let mut session = Session::new(1);
    let transform = edit(
        &mut session,
        Edit::Add {
            id: NodeId::mint(),
            kind: Kind::Transform,
            follows: None,
            name: None,
        },
    )
    .unwrap();
    let edited = session.edit_document(&transform, 0, |document| Ok::<_, ()>(document.clone()));
    assert!(matches!(edited, Err(dageist::DocumentError::Stale(_))));
    let made = session.edit_document(&transform, 1, |document| Ok::<_, ()>(document.clone()));
    assert_eq!(made, Ok(Ok(2)));
}
