//! What a parameter holds when it is not given, what it is first set to, and what a
//! new step is given, against Python Polars' own signatures.
//!
//! The expectations are written from the signatures in
//! `vendor/polars/py-polars/src/polars/lazyframe/frame.py` and the first word of each
//! choice's `Literal`: `join(how="inner", suffix="_right", validate="m:m",
//! nulls_equal=False, coalesce=None, maintain_order=None, allow_parallel=True)`,
//! `sort(descending=False)`, and the other `how` words in the order Python lists them.

use crate::catalog::{self, Entry, Setting, StepFrame, start_step};

fn entry(id: &str) -> &'static Entry {
    catalog::entry(id).unwrap_or_else(|| panic!("the catalogue has {id}"))
}

fn parameter(entry_id: &str, name: &str) -> &'static catalog::Parameter {
    entry(entry_id)
        .parameters()
        .iter()
        .find(|parameter| parameter.name() == name)
        .unwrap_or_else(|| panic!("{entry_id} has {name}"))
}

fn word(text: &str) -> Setting {
    Setting::Word(text.to_owned())
}

#[test]
fn a_parameter_holds_when_it_is_not_given_what_polars_gives_it() {
    let cases = [
        // A flag, a choice's word, text and a whole number, as Python's signatures write them.
        ("LazyFrame.sort", "descending", Some(Setting::Flag(false))),
        (
            "LazyFrame.join",
            "allow_parallel",
            Some(Setting::Flag(true)),
        ),
        ("LazyFrame.join", "how", Some(word("inner"))),
        ("LazyFrame.join", "validate", Some(word("m:m"))),
        (
            "LazyFrame.join",
            "suffix",
            Some(Setting::Text("_right".to_owned())),
        ),
        ("Expr.shift", "n", Some(Setting::Integer(1))),
        // Polars decides where the default is None; the others are not given a value or are empty.
        ("LazyFrame.join", "coalesce", None),
        ("LazyFrame.join", "maintain_order", None),
        ("LazyFrame.join", "left_on", None),
        ("LazyFrame.join", "other", None),
        ("LazyFrame.select", "exprs", None),
        ("LazyFrame.rename", "mapping", None),
    ];
    for (entry_id, name, expected) in cases {
        assert_eq!(
            parameter(entry_id, name).default_setting(),
            expected,
            "{entry_id} {name}"
        );
    }
}

#[test]
fn a_parameter_that_is_added_starts_with_something_that_differs_from_its_default() {
    let cases = [
        // A flag starts as the opposite of its default, and true where Polars decides.
        ("LazyFrame.sort", "descending", Setting::Flag(true)),
        ("LazyFrame.join", "allow_parallel", Setting::Flag(false)),
        ("LazyFrame.join", "coalesce", Setting::Flag(true)),
        // A choice starts with the first word that is not the default: `inner` is, and
        // `m:m`, `auto`; where the default is None, every word differs.
        ("LazyFrame.join", "how", word("left")),
        ("LazyFrame.join", "validate", word("m:1")),
        ("LazyFrame.join", "build_side", word("prefer_left")),
        ("LazyFrame.join", "maintain_order", word("none")),
        // A list starts with one item to be given, and pairs with none.
        (
            "LazyFrame.join",
            "left_on",
            Setting::List(vec![Setting::ToGive]),
        ),
        (
            "LazyFrame.select",
            "exprs",
            Setting::List(vec![Setting::ToGive]),
        ),
        ("LazyFrame.rename", "mapping", Setting::Pairs(Vec::new())),
        // Anything else is still to be given.
        ("LazyFrame.join", "suffix", Setting::ToGive),
        ("Expr.shift", "n", Setting::ToGive),
        ("LazyFrame.join", "other", Setting::ToGive),
    ];
    for (entry_id, name, expected) in cases {
        assert_eq!(
            parameter(entry_id, name).start_setting(),
            expected,
            "{entry_id} {name}"
        );
    }
}

fn names(texts: &[&str]) -> Vec<String> {
    texts.iter().map(|text| (*text).to_owned()).collect()
}

/// What the step `id` starts with on `columns`, with `tables` connected, by parameter.
fn started(id: &str, columns: &[&str], tables: &[&str]) -> Vec<(&'static str, Setting)> {
    let (columns, tables) = (names(columns), names(tables));
    start_step(
        entry(id),
        &StepFrame {
            columns: &columns,
            tables: &tables,
        },
    )
    .unwrap_or_else(|| panic!("{id} is a step"))
    .arguments()
    .to_vec()
}

fn unset_list() -> Setting {
    Setting::List(vec![Setting::ToGive])
}

#[test]
fn a_new_step_starts_with_what_it_cannot_be_built_without_still_to_be_given() {
    assert_eq!(
        started("LazyFrame.sort", &["a", "b"], &[]),
        [("by", unset_list())]
    );
    // A filter starts with a comparison for equality, `Expr.eq`, of a field and a value that are
    // both still to be given.
    assert_eq!(
        started("LazyFrame.filter", &["a"], &[]),
        [(
            "predicates",
            Setting::List(vec![Setting::Comparison {
                entry: "Expr.eq",
                field: Box::new(Setting::ToGive),
                other: Box::new(Setting::ToGive),
            }])
        )]
    );
    assert_eq!(
        started("LazyFrame.rename", &["a"], &[]),
        [("mapping", Setting::Pairs(Vec::new()))]
    );
    assert_eq!(
        started("LazyFrame.with_columns", &["a"], &[]),
        [(
            "named_exprs",
            Setting::Pairs(vec![(
                Setting::Text("new_column".to_owned()),
                Setting::ToGive
            )])
        )]
    );
}

#[test]
fn a_new_select_keeps_every_column_in_the_order_of_the_frame() {
    assert_eq!(
        started("LazyFrame.select", &["dcn", "owner", "timestamp"], &[]),
        [(
            "exprs",
            Setting::List(vec![
                Setting::Field("dcn".to_owned()),
                Setting::Field("owner".to_owned()),
                Setting::Field("timestamp".to_owned()),
            ])
        )]
    );
    // A frame with no column keeps none.
    assert_eq!(
        started("LazyFrame.select", &[], &[]),
        [("exprs", Setting::List(Vec::new()))]
    );
}

#[test]
fn a_new_join_takes_the_first_table_and_keeps_the_rows_that_have_no_match() {
    assert_eq!(
        started("LazyFrame.join", &["a"], &["step_names", "step_sla"]),
        [
            ("other", word("step_names")),
            ("left_on", unset_list()),
            ("right_on", unset_list()),
            // Polars' default is `inner`; the first other word of its `how` is `left`.
            ("how", word("left")),
        ]
    );
    // With no table connected, which table is still to be given.
    assert_eq!(
        started("LazyFrame.join", &["a"], &[])[0],
        ("other", Setting::ToGive)
    );
}

#[test]
fn a_new_pivot_takes_the_first_table_for_its_columns_and_the_first_value_of_each_cell() {
    assert_eq!(
        started("LazyFrame.pivot", &["a"], &["step_names"]),
        [
            ("on", Setting::ToGive),
            ("on_columns", word("step_names")),
            ("index", Setting::ToGive),
            ("values", Setting::ToGive),
            ("aggregate_function", Setting::Element("first")),
        ]
    );
    assert_eq!(
        started("LazyFrame.pivot", &["a"], &[])[1],
        ("on_columns", Setting::ToGive)
    );
}

#[test]
fn the_comparison_a_filter_starts_with_is_an_entry_that_takes_what_it_is_compared_with() {
    let eq = entry("Expr.eq");
    assert!(
        eq.parameters()
            .iter()
            .any(|parameter| parameter.name() == "other")
    );
}

#[test]
fn the_word_a_join_starts_with_is_a_word_of_how_that_differs_from_its_default() {
    let how = parameter("LazyFrame.join", "how");
    let default = how.default_setting();
    let catalog::Kind::Choice(words) = how.kind() else {
        panic!("how is a choice");
    };
    let started = started("LazyFrame.join", &["a"], &["t"]);
    let Some((_, Setting::Word(word))) = started.iter().find(|(name, _)| *name == "how") else {
        panic!("a join starts with how");
    };
    assert!(words.contains(&word.as_str()));
    assert_ne!(default, Some(Setting::Word(word.clone())));
}

#[test]
fn every_step_of_the_catalogue_has_a_start() {
    // A start that cannot be built, such as one for a parameter the entry lacks, is not given:
    // the steps the catalogue has must each be given one.
    let columns = names(&["a"]);
    let tables = names(&["t"]);
    let frame = StepFrame {
        columns: &columns,
        tables: &tables,
    };
    for id in [
        "LazyFrame.filter",
        "LazyFrame.join",
        "LazyFrame.pivot",
        "LazyFrame.rename",
        "LazyFrame.select",
        "LazyFrame.sort",
        "LazyFrame.with_columns",
    ] {
        assert!(start_step(entry(id), &frame).is_some(), "{id} has no start");
    }
}

#[test]
fn what_is_not_a_step_starts_with_nothing() {
    let frame = StepFrame {
        columns: &[],
        tables: &[],
    };
    for id in ["Expr.shift", "polars.col", "When.then"] {
        assert_eq!(start_step(entry(id), &frame), None, "{id}");
    }
}

#[test]
fn every_argument_a_new_step_starts_with_is_a_parameter_of_its_entry() {
    let columns = names(&["a", "b"]);
    let tables = names(&["t"]);
    let frame = StepFrame {
        columns: &columns,
        tables: &tables,
    };
    let steps: Vec<_> = catalog::entries()
        .iter()
        .filter_map(|entry| start_step(entry, &frame).map(|start| (entry, start)))
        .collect();
    // The seven steps of the catalogue.
    assert_eq!(steps.len(), 7);
    for (entry, start) in steps {
        for (name, _) in start.arguments() {
            assert!(
                entry
                    .parameters()
                    .iter()
                    .any(|parameter| parameter.name() == *name),
                "{} has no parameter {name}",
                entry.id().as_str()
            );
        }
        // The method the pivot's cells hold is an entry that stands in an expression.
        if let Some(Setting::Element(method)) = start.argument("aggregate_function") {
            assert!(catalog::entry(&format!("Expr.{method}")).is_some());
        }
    }
}
