//! The rows and inputs the Python source is checked on: the claim-timing
//! sample's own, and a small table that holds each dtype the catalogue's
//! entries work on, nulls and whole floats included.

use dageist_engine::claim_timing::ClaimTiming;
use dageist_engine::document::{Inputs, ListOrigin};
use polars::prelude::*;

pub struct TableInput {
    pub name: &'static str,
    pub frame: DataFrame,
}

/// A connected value or list: its name, and the series that holds it. A value
/// is a series of one item.
pub struct SeriesInput {
    pub name: &'static str,
    pub series: Series,
    pub list: bool,
}

/// The rows a document is run on, with the inputs its steps name.
pub struct Fixture {
    pub frame_name: &'static str,
    pub frame: DataFrame,
    pub tables: Vec<TableInput>,
    pub series: Vec<SeriesInput>,
}

impl Fixture {
    /// The inputs as the engine is given them.
    pub fn inputs(&self) -> Inputs {
        let mut inputs = Inputs::new();
        for table in &self.tables {
            inputs
                .add_table(table.name, "Node", table.frame.clone().lazy())
                .expect("a table");
        }
        for input in &self.series {
            if input.list {
                inputs
                    .add_list(input.name, ListOrigin::Parameter, input.series.clone())
                    .expect("a list");
            } else {
                let value = input.series.get(0).expect("a value").into_static();
                inputs
                    .add_value(input.name, Scalar::new(input.series.dtype().clone(), value))
                    .expect("a value");
            }
        }
        inputs
    }
}

fn dates(texts: &[&str]) -> Series {
    df!("value" => texts)
        .expect("the texts")
        .lazy()
        .select([col("value").str().to_date(StrptimeOptions::default())])
        .collect()
        .expect("dates")
        .column("value")
        .expect("the column")
        .as_materialized_series()
        .clone()
}

/// The claim-timing sample's rows and inputs, written out again so that the
/// check does not depend on what the sample hides. The first test below holds
/// the copy to the sample.
pub fn sample() -> Fixture {
    let source = ClaimTiming::new().expect("the sample node");
    Fixture {
        frame_name: ClaimTiming::SOURCE_NAME,
        frame: source.source().clone(),
        tables: vec![
            TableInput {
                name: "step_sla",
                frame: df!(
                    "step" => ["Intake", "Review", "Pending", "Complete"],
                    "sla_days" => [1_i64, 1, 2, 3],
                )
                .expect("a table"),
            },
            TableInput {
                name: "step_names",
                frame: df!("step_to" => ["Intake", "Review", "Pending", "Complete"])
                    .expect("a table"),
            },
        ],
        series: vec![
            SeriesInput {
                name: "sla_cutoff",
                series: Series::new("value".into(), [1_i64]),
                list: false,
            },
            SeriesInput {
                name: "as_of",
                series: dates(&["2026-03-02"]),
                list: false,
            },
            SeriesInput {
                name: "holidays",
                series: dates(&["2026-03-03", "2026-03-05"]),
                list: true,
            },
        ],
    }
}

/// Six rows with one column of each dtype the entries work on.
pub fn varied() -> Fixture {
    let frame = df!(
        "id" => [1_i64, 2, 3, 4, 5, 6],
        "name" => [Some("a"), Some("b"), Some("a"), None, Some("c"), Some("b")],
        "score" => [Some(1.0_f64), Some(2.5), None, Some(4.0), Some(-0.5), Some(2.5)],
        "flag" => [Some(true), Some(false), None, Some(true), Some(false), Some(true)],
        "group" => ["x", "x", "y", "y", "z", "z"],
        "day" => ["2026-03-02", "2026-03-05", "2026-03-06", "2026-03-02", "2026-03-04", "2026-03-09"],
        "at" => [
            "2026-03-02 13:30:00",
            "2026-03-05 07:50:00",
            "2026-03-06 10:00:00",
            "2026-03-02 09:15:00",
            "2026-03-04 17:20:00",
            "2026-03-09 11:05:00",
        ],
    )
    .expect("the rows")
    .lazy()
    .with_columns([
        col("day").str().to_date(StrptimeOptions::default()),
        col("at").str().to_datetime(
            Some(TimeUnit::Microseconds),
            None,
            StrptimeOptions::default(),
            lit("raise"),
        ),
    ])
    .collect()
    .expect("the rows have their dtypes");
    Fixture {
        frame_name: "data",
        frame,
        tables: vec![
            TableInput {
                name: "lookup",
                frame: df!(
                    "key" => ["a", "b", "c", "d"],
                    "tag" => ["x", "x", "y", "y"],
                    "weight" => [10_i64, 20, 30, 40],
                )
                .expect("a table"),
            },
            TableInput {
                name: "members",
                frame: df!("group" => ["x", "y"], "rank" => [1_i64, 2]).expect("a table"),
            },
            TableInput {
                name: "groups",
                frame: df!("group" => ["x", "y", "z"]).expect("a table"),
            },
        ],
        series: vec![
            SeriesInput {
                name: "limit",
                series: Series::new("value".into(), [2_i64]),
                list: false,
            },
            SeriesInput {
                name: "ratio",
                series: Series::new("value".into(), [1.5_f64]),
                list: false,
            },
            SeriesInput {
                name: "since",
                series: dates(&["2026-03-04"]),
                list: false,
            },
            SeriesInput {
                name: "closed",
                series: dates(&["2026-03-03", "2026-03-05"]),
                list: true,
            },
        ],
    }
}

#[test]
fn the_sample_fixture_holds_what_the_sample_node_holds() {
    let node = ClaimTiming::new().expect("the sample node");
    let mut from_node = node.inputs().describe().expect("a description");
    let fixture = sample();
    let mut ours = fixture.inputs().describe().expect("a description");
    // The origin is the node's to say; the names, kinds and values must be the same.
    for list in [&mut from_node, &mut ours] {
        list.sort_by(|left, right| left.name().cmp(right.name()));
    }
    let shown = |list: &[dageist_engine::document::InputDescription]| {
        list.iter()
            .map(|input| format!("{} {:?}", input.name(), input.kind()))
            .collect::<Vec<_>>()
    };
    assert_eq!(shown(&ours), shown(&from_node));
}
