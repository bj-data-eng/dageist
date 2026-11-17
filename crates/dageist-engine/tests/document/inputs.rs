//! The inputs a document's steps can name.

use dageist_engine::document::{
    ColumnDescription, DuplicateInput, InputDescription, InputKind, Inputs,
};
use polars::prelude::*;

#[test]
fn a_name_belongs_to_one_input_whatever_its_kind() {
    let table = || df!("a" => [1_i64]).expect("a table").lazy();
    let value = || Scalar::new(DataType::Int64, AnyValue::Int64(1));
    let list = || Series::new("a".into(), [1_i64]);

    let mut inputs = Inputs::new();
    inputs.add_table("taken", table()).expect("a table");
    for result in [
        inputs.add_table("taken", table()),
        inputs.add_value("taken", value()),
        inputs.add_list("taken", list()),
    ] {
        assert_eq!(result, Err(DuplicateInput("taken".to_owned())));
    }
    inputs.add_value("value", value()).expect("a value");
    inputs.add_list("list", list()).expect("a list");
    assert_eq!(
        inputs.add_table("value", table()),
        Err(DuplicateInput("value".to_owned()))
    );
    assert!(
        DuplicateInput("taken".to_owned())
            .to_string()
            .contains("taken")
    );
}

/// A column as its accessors give it.
fn shown(columns: &[ColumnDescription]) -> Vec<(&str, &str)> {
    columns
        .iter()
        .map(|column| (column.name(), column.dtype()))
        .collect()
}

#[test]
fn inputs_describe_themselves_by_name_kind_and_short_dtype() {
    let when = DataType::Datetime(TimeUnit::Microseconds, None);
    let mut inputs = Inputs::new();
    inputs
        .add_table(
            "sla",
            df!("step" => ["Intake"], "days" => [1_i64])
                .expect("a table")
                .lazy(),
        )
        .expect("a table");
    inputs
        .add_table(
            "events",
            df!("at" => [0_i64])
                .expect("a table")
                .lazy()
                .with_column(col("at").cast(when.clone())),
        )
        .expect("a table");
    inputs
        .add_value("cutoff", Scalar::new(DataType::Int64, AnyValue::Int64(1)))
        .expect("a value");
    inputs
        .add_value(
            "as_of",
            Scalar::new(
                when.clone(),
                AnyValue::Datetime(0, TimeUnit::Microseconds, None),
            ),
        )
        .expect("a value");
    inputs
        .add_list(
            "holidays",
            Series::new("holidays".into(), [1_i32])
                .cast(&DataType::Date)
                .expect("dates"),
        )
        .expect("a list");

    let described = inputs.describe().expect("every table has a schema");
    let summary: Vec<(&str, String)> = described
        .iter()
        .map(|input: &InputDescription| {
            let what = match input.kind() {
                InputKind::Table(columns) => format!("table {:?}", shown(columns)),
                InputKind::Value(dtype) => format!("value {dtype}"),
                InputKind::List(dtype) => format!("list {dtype}"),
            };
            (input.name(), what)
        })
        .collect();
    // In the order of the names, with a dtype as its short name: a time unit
    // is spelled out and not Polars' own `datetime[μs]`.
    assert_eq!(
        summary,
        [
            ("as_of", "value Datetime(us)".to_owned()),
            ("cutoff", "value Int64".to_owned()),
            ("events", r#"table [("at", "Datetime(us)")]"#.to_owned()),
            ("holidays", "list Date".to_owned()),
            (
                "sla",
                r#"table [("step", "String"), ("days", "Int64")]"#.to_owned()
            ),
        ]
    );
}

#[test]
fn a_table_whose_schema_polars_cannot_work_out_is_an_error_not_a_guess() {
    let mut inputs = Inputs::new();
    inputs
        .add_table(
            "broken",
            df!("a" => [1_i64])
                .expect("a table")
                .lazy()
                .select([col("missing")]),
        )
        .expect("a table");
    assert!(inputs.describe().is_err());
}

#[test]
fn no_inputs_describe_as_none() {
    assert!(
        Inputs::new()
            .describe()
            .expect("nothing to resolve")
            .is_empty()
    );
}
