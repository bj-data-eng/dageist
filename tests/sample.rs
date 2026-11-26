//! The sample the application opens on, in `samples/orders`: made into a workspace as the host
//! makes it, every node works out with no problem, and the chain of transforms gives the summary
//! worked out here by hand from the sample's rows.

use std::path::{Path, PathBuf};

use dageist::Session;
use dageist::workspace::{Options, Workspace};
use dageist_engine::document::StepId;
use dageist_graph::{Kind, NodeId};

/// The sample folder in the repository.
fn sample() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("samples")
        .join("orders")
}

/// The node of `session` named `name`.
fn named(session: &Session, name: &str) -> NodeId {
    session
        .graph()
        .nodes()
        .find(|(_, node)| node.name().as_str() == name)
        .map(|(id, _)| id.clone())
        .unwrap_or_else(|| panic!("the sample has no node {name}"))
}

/// A workspace made from the sample in a folder of its own, and the session it holds.
fn opened(name: &str) -> (PathBuf, Session) {
    let root = std::env::temp_dir().join(format!("dageist-sample-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (_, saved) = Workspace::create_from_folder(
        &sample(),
        &root,
        dageist::unsigned_author(),
        Options::default(),
    )
    .expect("the sample is a workspace");
    (
        root,
        Session::from_saved(saved, 1).expect("the sample's steps read"),
    )
}

#[test]
fn the_sample_is_written_as_the_application_writes_a_workspace() {
    let (root, session) = opened("written");
    // Saving the session the sample opens as changes nothing: its files are the ones a save writes.
    let (mut workspace, saved) =
        Workspace::open(&root, dageist::unsigned_author(), Options::default()).expect("opens");
    assert_eq!(session.saved(), saved);
    assert_eq!(
        workspace.save(&session.saved(), "Nothing").expect("saved"),
        None
    );
    std::fs::remove_dir_all(&root).expect("removed");
}

#[test]
fn every_node_of_the_sample_works_out_with_two_sources_of_each_format() {
    let (root, session) = opened("works");
    let workings = session.work_out();
    let mut formats = Vec::new();
    for (id, node) in session.graph().nodes() {
        let worked = workings.node(id).expect("worked out");
        // The output names a file it has not written, which is all it is short of.
        if node.kind() != Kind::Output {
            assert_eq!(worked.problems, [], "{}", node.name());
        }
        if let dageist_graph::Body::Source { file: Some(file) } = node.body() {
            formats.push(file.format);
        }
    }
    formats.sort_by_key(|format| format!("{format:?}"));
    assert_eq!(
        formats,
        [
            dageist_graph::Format::Csv,
            dageist_graph::Format::Csv,
            dageist_graph::Format::Parquet,
            dageist_graph::Format::Parquet,
        ]
    );
    let columns = |name: &str| -> Vec<String> {
        workings
            .node(&named(&session, name))
            .expect("worked out")
            .columns
            .as_ref()
            .expect("columns")
            .iter()
            .map(|column| format!("{}: {}", column.name(), column.dtype()))
            .collect()
    };
    assert_eq!(
        columns("Order lines"),
        [
            "order_id: Int64",
            "customer_id: Int64",
            "product_id: Int64",
            "quantity: Int64",
            "ordered_at: Datetime(us)",
            "product: String",
            "category: String",
            "unit_price: Float64",
            "line_total: Float64",
        ]
    );
    assert_eq!(columns("Category names"), ["category: String"]);
    assert_eq!(
        columns("Region summary"),
        [
            "region: String",
            "Furniture: Float64",
            "Lighting: Float64",
            "Stationery: Float64",
        ]
    );
    std::fs::remove_dir_all(&root).expect("removed");
}

#[test]
fn the_chain_of_transforms_gives_each_regions_totals_by_category() {
    let (root, session) = opened("chain");
    let workings = session.work_out();
    let summary = named(&session, "Region summary");
    let bindings = workings
        .node(&summary)
        .expect("worked out")
        .bindings
        .as_ref()
        .expect("bound");
    // The summary starts from the customer lines, which start from the order lines: the rows
    // reaching it are the orders of two or more, each with its customer's region, by region.
    let lines = bindings.start.working(100).expect("the lines");
    assert_eq!(lines.height(), 9);
    let (document, _) = session.document(&summary).expect("its steps");
    let rows = document
        .rows(
            &bindings.inputs,
            &lines,
            &StepId::new("by_category").expect("an id"),
            10,
        )
        .expect("the summary");
    let column = |name: &str| rows.column(name).expect("a column of the summary");
    let total = |category: &str| -> Vec<Option<f64>> {
        (0..rows.height())
            .map(|row| {
                column(category)
                    .get(row)
                    .expect("a row")
                    .extract::<f64>()
                    .map(|value| (value * 100.0).round() / 100.0)
            })
            .collect()
    };
    let regions: Vec<Option<String>> = (0..rows.height())
        .map(|row| {
            column("region")
                .get(row)
                .expect("a row")
                .get_str()
                .map(str::to_owned)
        })
        .collect();
    assert_eq!(
        regions,
        [
            Some("North".to_owned()),
            Some("South".to_owned()),
            Some("West".to_owned())
        ]
    );
    // North: desks 2 × 250, chairs 4 × 120 and 6 × 120, staplers 2 × 15.
    // South: notebooks 10 × 4.5 and pens 25 × 1.2. West: lamps 3 × 45, notebooks 12 × 4.5 and
    // staplers 3 × 15. Orders of one are left out.
    assert_eq!(total("Furniture"), [Some(1700.0), Some(0.0), Some(0.0)]);
    assert_eq!(total("Lighting"), [Some(0.0), Some(0.0), Some(135.0)]);
    assert_eq!(total("Stationery"), [Some(30.0), Some(75.0), Some(99.0)]);
    std::fs::remove_dir_all(&root).expect("removed");
}
