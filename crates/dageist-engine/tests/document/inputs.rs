//! The inputs a document's steps can name.

use dageist_engine::document::{
    ColumnDescription, DuplicateInput, InputDescription, InputKind, Inputs, Origin,
};
use polars::prelude::*;

#[test]
fn a_name_belongs_to_one_input_whatever_its_kind() {
    let table = || df!("a" => [1_i64]).expect("a table").lazy();
    let value = || Scalar::new(DataType::Int64, AnyValue::Int64(1));
    let list = || Series::new("a".into(), [1_i64]);

    let origin = || Origin::Parameter;

    let mut inputs = Inputs::new();
    inputs
        .add_table("taken", origin(), table())
        .expect("a table");
    for result in [
        inputs.add_table("taken", origin(), table()),
        inputs.add_value("taken", origin(), value()),
        inputs.add_list("taken", origin(), list()),
    ] {
        assert_eq!(result, Err(DuplicateInput("taken".to_owned())));
    }
    inputs
        .add_value("value", origin(), value())
        .expect("a value");
    inputs.add_list("list", origin(), list()).expect("a list");
    assert_eq!(
        inputs.add_table("value", origin(), table()),
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
            Origin::Node("SLA by step".to_owned()),
            df!("step" => ["Intake"], "days" => [1_i64])
                .expect("a table")
                .lazy(),
        )
        .expect("a table");
    inputs
        .add_table(
            "events",
            Origin::Node("Events".to_owned()),
            df!("at" => [0_i64])
                .expect("a table")
                .lazy()
                .with_column(col("at").cast(when.clone())),
        )
        .expect("a table");
    inputs
        .add_value(
            "cutoff",
            Origin::Parameter,
            Scalar::new(DataType::Int64, AnyValue::Int64(1)),
        )
        .expect("a value");
    inputs
        .add_value(
            "as_of",
            Origin::Parameter,
            Scalar::new(
                when.clone(),
                AnyValue::Datetime(0, TimeUnit::Microseconds, None),
            ),
        )
        .expect("a value");
    inputs
        .add_list(
            "holidays",
            Origin::Column {
                node: "Calendar".to_owned(),
                column: "date".to_owned(),
            },
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
                InputKind::Value { dtype, .. } => format!("value {dtype}"),
                InputKind::List { dtype, .. } => format!("list {dtype}"),
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
            Origin::Parameter,
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

/// A date as a list of the days since the Unix epoch gives it.
fn dates(days: &[i32]) -> Series {
    Series::new("date".into(), days)
        .cast(&DataType::Date)
        .expect("dates")
}

#[test]
fn an_input_says_what_supplies_it_to_the_node() {
    let mut inputs = Inputs::new();
    inputs
        .add_table(
            "sla",
            Origin::Node("SLA by step".to_owned()),
            df!("a" => [1_i64]).expect("a table").lazy(),
        )
        .expect("a table");
    inputs
        .add_value(
            "cutoff",
            Origin::Parameter,
            Scalar::new(DataType::Int64, AnyValue::Int64(1)),
        )
        .expect("a value");
    inputs
        .add_list(
            "holidays",
            Origin::Column {
                node: "Calendar".to_owned(),
                column: "date".to_owned(),
            },
            dates(&[1]),
        )
        .expect("a list");
    let described = inputs.describe().expect("described");
    let origins: Vec<(&str, Origin)> = described
        .iter()
        .map(|input| (input.name(), input.origin().clone()))
        .collect();
    assert_eq!(
        origins,
        [
            ("cutoff", Origin::Parameter),
            (
                "holidays",
                Origin::Column {
                    node: "Calendar".to_owned(),
                    column: "date".to_owned()
                }
            ),
            ("sla", Origin::Node("SLA by step".to_owned())),
        ]
    );
}

#[test]
fn a_value_is_written_as_polars_writes_it_and_a_null_value_has_no_text() {
    let mut inputs = Inputs::new();
    inputs
        .add_value(
            "as_of",
            Origin::Parameter,
            // 2026-03-02 is day 20,514 of the epoch.
            Scalar::new(DataType::Date, AnyValue::Date(20_514)),
        )
        .expect("a date");
    inputs
        .add_value(
            "cutoff",
            Origin::Parameter,
            Scalar::new(DataType::Int64, AnyValue::Int64(1)),
        )
        .expect("a whole number");
    inputs
        .add_value(
            "label",
            Origin::Parameter,
            Scalar::new(DataType::String, AnyValue::StringOwned("late".into())),
        )
        .expect("text");
    inputs
        .add_value(
            "nothing",
            Origin::Parameter,
            Scalar::new(DataType::Int64, AnyValue::Null),
        )
        .expect("a null");
    let described = inputs.describe().expect("described");
    let values: Vec<(&str, Option<String>)> = described
        .iter()
        .map(|input| match input.kind() {
            InputKind::Value { value, .. } => (input.name(), value.clone()),
            other => panic!("not a value: {other:?}"),
        })
        .collect();
    assert_eq!(
        values,
        [
            ("as_of", Some("2026-03-02".to_owned())),
            ("cutoff", Some("1".to_owned())),
            ("label", Some("late".to_owned())),
            ("nothing", None),
        ]
    );
}

#[test]
fn a_list_gives_its_first_values_as_text_and_how_many_more_there_are() {
    let mut inputs = Inputs::new();
    inputs
        .add_list("few", Origin::Parameter, dates(&[20_515, 20_517]))
        .expect("a list");
    let many: Vec<i32> = (0..10).map(|day| 20_514 + day).collect();
    inputs
        .add_list("many", Origin::Parameter, dates(&many))
        .expect("a list");
    inputs
        .add_list(
            "gaps",
            Origin::Parameter,
            Series::new("gaps".into(), [Some(1_i64), None]),
        )
        .expect("a list");
    let lists: Vec<(String, Vec<Option<String>>, usize)> = inputs
        .describe()
        .expect("described")
        .iter()
        .map(|input| match input.kind() {
            InputKind::List { values, .. } => (
                input.name().to_owned(),
                values.shown().to_vec(),
                values.more(),
            ),
            other => panic!("not a list: {other:?}"),
        })
        .collect();
    let texts = |cells: &[Option<&str>]| -> Vec<Option<String>> {
        cells.iter().map(|cell| cell.map(str::to_owned)).collect()
    };
    assert_eq!(
        lists,
        [
            (
                "few".to_owned(),
                texts(&[Some("2026-03-03"), Some("2026-03-05")]),
                0
            ),
            ("gaps".to_owned(), texts(&[Some("1"), None]), 0),
            (
                "many".to_owned(),
                texts(&[
                    Some("2026-03-02"),
                    Some("2026-03-03"),
                    Some("2026-03-04"),
                    Some("2026-03-05")
                ]),
                6
            ),
        ]
    );
}
