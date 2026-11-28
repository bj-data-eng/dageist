//! The chain `group_by` starts, built through the catalogue's own functions and run on a table
//! small enough to write the answer by hand, as Python's documentation of each call gives it.

use polars::prelude::{
    DataFrame, DataType, IntoLazy, LazyFrame, SortMultipleOptions, col, df, lit,
};

use crate::catalog::{Applies, Argument, Arguments, Build, BuildError, Family, entries, entry};

use super::build::{choice, expression_arg, flag, many, none, pairs, run, schema};

/// Three rows, a key with a repeat, and a whole number, a float and a text to aggregate. The
/// whole numbers hold a null, which the first and the last of a group may skip.
fn table() -> LazyFrame {
    df!(
        "k" => ["a", "b", "a", "b"],
        "x" => [Some(1_i64), None, Some(3), Some(4)],
        "y" => [0.5_f64, 1.5, 2.5, 3.5],
    )
    .expect("a table")
    .lazy()
}

/// The arguments the entry `id` holds for `given`, as a saved step gives them.
fn arguments(id: &str, given: Vec<(&str, Argument)>) -> Arguments {
    entry(id)
        .expect("an entry")
        .arguments(given)
        .expect("arguments that fit")
}

/// The table `grouping` makes of `table()`, grouped by `k` in the order the keys first appear,
/// continued by the calls of `links` and finished by `finish`.
fn grouped(
    links: &[(&str, Arguments)],
    finish: &str,
    finishing: &Arguments,
) -> Result<LazyFrame, BuildError> {
    crate::startup::ensure();
    let Build::Grouping(start) = entry("LazyFrame.group_by").expect("an entry").build() else {
        panic!("group_by starts a chain");
    };
    let keys = arguments(
        "LazyFrame.group_by",
        vec![
            ("by", many(vec![expression_arg(col("k"))])),
            ("maintain_order", flag(true)),
        ],
    );
    let mut group = start(table(), &keys)?;
    for (id, given) in links {
        let Build::GroupedStep(step) = entry(id).expect("an entry").build() else {
            panic!("{id} continues a chain");
        };
        group = step(group, given)?;
    }
    let Build::GroupedEnd(end) = entry(finish).expect("an entry").build() else {
        panic!("{finish} finishes a chain");
    };
    end(group, finishing)
}

fn shortcut(id: &str, given: Vec<(&str, Argument)>) -> DataFrame {
    run(grouped(&[], id, &arguments(id, given)).expect("a chain"))
}

fn floats(frame: &DataFrame, name: &str) -> Vec<Option<f64>> {
    frame
        .column(name)
        .expect("a column")
        .f64()
        .expect("floats")
        .iter()
        .collect()
}

fn whole(frame: &DataFrame, name: &str) -> Vec<Option<i64>> {
    frame
        .column(name)
        .expect("a column")
        .i64()
        .expect("whole numbers")
        .iter()
        .collect()
}

#[test]
fn the_calls_of_a_grouping_are_entries_of_their_own_kind_and_family() {
    for entry in entries() {
        let id = entry.id().as_str();
        assert_eq!(
            entry.applies() == Applies::Grouped,
            id.starts_with("LazyGroupBy."),
            "{id}: the methods of a lazy group by are what applies to a group by"
        );
        assert_eq!(
            entry.family() == Family::Grouped,
            entry.applies() == Applies::Grouped,
            "{id}: the Grouped family is the methods of a lazy group by"
        );
    }
    // `group_by` is a step of the list that starts the chain, and `having` is the one call
    // that continues it.
    assert!(matches!(
        entry("LazyFrame.group_by").expect("an entry").build(),
        Build::Grouping(_)
    ));
    assert!(matches!(
        entry("LazyGroupBy.having").expect("an entry").build(),
        Build::GroupedStep(_)
    ));
    let finishing: Vec<&str> = entries()
        .iter()
        .filter(|entry| matches!(entry.build(), Build::GroupedEnd(_)))
        .map(|entry| entry.id().as_str())
        .collect();
    assert_eq!(
        finishing,
        [
            "LazyGroupBy.agg",
            "LazyGroupBy.all",
            "LazyGroupBy.first",
            "LazyGroupBy.head",
            "LazyGroupBy.last",
            "LazyGroupBy.len",
            "LazyGroupBy.max",
            "LazyGroupBy.mean",
            "LazyGroupBy.median",
            "LazyGroupBy.min",
            "LazyGroupBy.n_unique",
            "LazyGroupBy.quantile",
            "LazyGroupBy.sum",
            "LazyGroupBy.tail",
        ]
    );
}

#[test]
fn agg_gives_the_keys_then_each_aggregate_named_as_its_chain_or_its_key() {
    let given = arguments(
        "LazyGroupBy.agg",
        vec![
            ("aggs", many(vec![expression_arg(col("x").sum())])),
            (
                "named_aggs",
                pairs(vec![(
                    Argument::Columns(vec!["mean_y".to_owned()]),
                    expression_arg(col("y").mean()),
                )]),
            ),
        ],
    );
    let frame = run(grouped(&[], "LazyGroupBy.agg", &given).expect("a chain"));
    let names: Vec<String> = frame
        .get_column_names()
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(names, ["k", "x", "mean_y"]);
    assert_eq!(whole(&frame, "x"), [Some(4), Some(4)]);
    assert_eq!(floats(&frame, "mean_y"), [Some(1.5), Some(2.5)]);
}

#[test]
fn an_aggregate_that_does_not_aggregate_gives_a_list_for_each_group() {
    let given = arguments(
        "LazyGroupBy.agg",
        vec![("aggs", many(vec![expression_arg(col("y") + lit(1))]))],
    );
    let mut frame = grouped(&[], "LazyGroupBy.agg", &given).expect("a chain");
    let schema = schema(frame.clone());
    assert_eq!(
        schema.get("y"),
        Some(&DataType::List(Box::new(DataType::Float64)))
    );
    let rows = run(frame.clone());
    let first = rows.column("y").expect("a column").list().expect("lists");
    let first = first.get_as_series(0).expect("a list");
    assert_eq!(
        first.f64().expect("floats").iter().collect::<Vec<_>>(),
        [Some(1.5), Some(3.5)]
    );
    // The schema is what a run gives.
    assert_eq!(
        frame.collect_schema().expect("a schema").get("y"),
        Some(&DataType::List(Box::new(DataType::Float64)))
    );
}

#[test]
fn having_keeps_the_groups_every_predicate_holds_for() {
    let having = arguments(
        "LazyGroupBy.having",
        vec![(
            "predicates",
            many(vec![
                expression_arg(polars::prelude::len().gt(lit(1))),
                expression_arg(col("y").sum().gt(lit(3.5))),
            ]),
        )],
    );
    let frame = run(grouped(
        &[("LazyGroupBy.having", having)],
        "LazyGroupBy.len",
        &none(),
    )
    .expect("a chain"));
    assert_eq!(frame.height(), 1);
    let key = frame.column("k").expect("a column").str().expect("text");
    // Only the second group holds more than one row and has a sum of `y` above 3.5.
    assert_eq!(key.get(0), Some("b"));
}

#[test]
fn len_counts_the_rows_of_each_group_under_its_name_or_len() {
    let frame = shortcut("LazyGroupBy.len", Vec::new());
    assert_eq!(frame.get_column_names(), ["k", "len"]);
    assert_eq!(
        frame.column("len").expect("a column").dtype(),
        &DataType::UInt32
    );
    let named = shortcut(
        "LazyGroupBy.len",
        vec![("name", Argument::String("rows".to_owned()))],
    );
    assert_eq!(named.get_column_names(), ["k", "rows"]);
    let rows: Vec<Option<u32>> = named
        .column("rows")
        .expect("a column")
        .u32()
        .expect("counts")
        .iter()
        .collect();
    assert_eq!(rows, [Some(2), Some(2)]);
}

#[test]
fn a_shortcut_aggregates_every_column_that_is_not_a_key_as_the_documentation_shows() {
    let sum = shortcut("LazyGroupBy.sum", Vec::new());
    assert_eq!(sum.get_column_names(), ["k", "x", "y"]);
    assert_eq!(whole(&sum, "x"), [Some(4), Some(4)]);
    assert_eq!(floats(&sum, "y"), [Some(3.0), Some(5.0)]);

    let mean = shortcut("LazyGroupBy.mean", Vec::new());
    assert_eq!(floats(&mean, "x"), [Some(2.0), Some(4.0)]);
    assert_eq!(floats(&mean, "y"), [Some(1.5), Some(2.5)]);

    let median = shortcut("LazyGroupBy.median", Vec::new());
    assert_eq!(floats(&median, "x"), [Some(2.0), Some(4.0)]);

    let min = shortcut("LazyGroupBy.min", Vec::new());
    assert_eq!(whole(&min, "x"), [Some(1), Some(4)]);
    let max = shortcut("LazyGroupBy.max", Vec::new());
    assert_eq!(whole(&max, "x"), [Some(3), Some(4)]);

    // `x` holds 1 and 3 in the first group, and a null and 4 in the second: a null is a value.
    let unique = shortcut("LazyGroupBy.n_unique", Vec::new());
    let counted: Vec<Option<u32>> = unique
        .column("x")
        .expect("a column")
        .u32()
        .expect("counts")
        .iter()
        .collect();
    assert_eq!(counted, [Some(2), Some(2)]);

    let quantile = shortcut(
        "LazyGroupBy.quantile",
        vec![
            ("quantile", Argument::Float(0.5)),
            ("interpolation", choice("linear")),
        ],
    );
    assert_eq!(floats(&quantile, "y"), [Some(1.5), Some(2.5)]);
}

#[test]
fn first_and_last_take_the_null_of_a_group_unless_they_are_told_to_skip_it() {
    let first = shortcut("LazyGroupBy.first", Vec::new());
    assert_eq!(whole(&first, "x"), [Some(1), None]);
    let first = shortcut("LazyGroupBy.first", vec![("ignore_nulls", flag(true))]);
    assert_eq!(whole(&first, "x"), [Some(1), Some(4)]);
    let last = shortcut("LazyGroupBy.last", Vec::new());
    assert_eq!(whole(&last, "x"), [Some(3), Some(4)]);
}

#[test]
fn all_gives_the_values_of_each_group_in_a_list() {
    let frame = shortcut("LazyGroupBy.all", Vec::new());
    let values = frame.column("y").expect("a column").list().expect("lists");
    let group = values.get_as_series(1).expect("a list");
    assert_eq!(
        group.f64().expect("floats").iter().collect::<Vec<_>>(),
        [Some(1.5), Some(3.5)]
    );
}

#[test]
fn head_and_tail_give_rows_of_each_group_and_not_one_value() {
    let head = shortcut("LazyGroupBy.head", vec![("n", Argument::Integer(1))]);
    assert_eq!(head.height(), 2);
    assert_eq!(whole(&head, "x"), [Some(1), None]);
    let tail = shortcut("LazyGroupBy.tail", vec![("n", Argument::Integer(1))]);
    assert_eq!(whole(&tail, "x"), [Some(3), Some(4)]);
    // Without a number, five rows of each group, which are all of these.
    assert_eq!(shortcut("LazyGroupBy.head", Vec::new()).height(), 4);
}

#[test]
fn a_negative_number_of_rows_is_refused_as_python_refuses_it() {
    let error = grouped(
        &[],
        "LazyGroupBy.head",
        &arguments("LazyGroupBy.head", vec![("n", Argument::Integer(-1))]),
    )
    .err()
    .expect("a negative number of rows");
    assert!(
        matches!(error, BuildError::OutOfRange { parameter: "n", .. }),
        "{error}"
    );
}

#[test]
fn a_quantile_needs_its_share() {
    let error = grouped(
        &[],
        "LazyGroupBy.quantile",
        &arguments("LazyGroupBy.quantile", vec![]),
    )
    .err()
    .expect("no share");
    assert!(
        matches!(
            error,
            BuildError::Missing {
                parameter: "quantile"
            }
        ),
        "{error}"
    );
}

#[test]
fn a_key_given_by_name_is_a_column_of_that_name() {
    crate::startup::ensure();
    let Build::Grouping(start) = entry("LazyFrame.group_by").expect("an entry").build() else {
        panic!("group_by starts a chain");
    };
    let keys = arguments(
        "LazyFrame.group_by",
        vec![(
            "named_by",
            pairs(vec![(
                Argument::Columns(vec!["key".to_owned()]),
                expression_arg(col("k").str().to_uppercase()),
            )]),
        )],
    );
    let Build::GroupedEnd(end) = entry("LazyGroupBy.len").expect("an entry").build() else {
        panic!("len finishes a chain");
    };
    let frame = run(end(start(table(), &keys).expect("a chain"), &none()).expect("a chain"))
        .sort(["key"], SortMultipleOptions::default())
        .expect("a sort");
    assert_eq!(frame.get_column_names(), ["key", "len"]);
    let key = frame.column("key").expect("a column").str().expect("text");
    assert_eq!(key.iter().collect::<Vec<_>>(), [Some("A"), Some("B")]);
}
