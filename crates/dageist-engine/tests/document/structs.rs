//! Structs, with their fields, and the calls that read, name and make them.
//!
//! Every expectation is what Python Polars 2.2.0 does, which each case asks by running the same
//! document on the rows of a table and on a table with no rows. The walk must say the same of the
//! same document, and where it refuses, a rule that names what was wrong must say it before Polars
//! is asked. The dtype the walk writes after the last call is the dtype Polars gives, field by
//! field, so a struct is written with the fields it holds.

use dageist_engine::allowed::{RefusalSource, Representative, dtype_name};
use dageist_engine::document::{Document, Inputs, Outcome, Report, Resolution, Segment, StepRows};
use polars::prelude::*;
use serde_json::{Value, json};

use crate::support::{
    call, chain, col_call, document, field, field_then, many, step, text, text_call, whole,
};

// ---------------------------------------------------------------------------
// The table

fn struct_column(name: &str, fields: Vec<Series>) -> Column {
    StructChunked::from_series(name.into(), 3, fields.iter())
        .expect("fields of three rows make a struct")
        .into_series()
        .into()
}

/// A column of arrays of whole numbers, as wide as the rows given.
fn arrays<const WIDTH: usize>(name: &str, rows: &[[i64; WIDTH]]) -> Column {
    ListChunked::from_iter(
        rows.iter()
            .map(|row| Some(Series::new("".into(), row.as_slice()))),
    )
    .into_series()
    .with_name(name.into())
    .cast(&DataType::Array(Box::new(DataType::Int64), WIDTH))
    .expect("lists cast to arrays of their width")
    .into()
}

/// The struct columns, with the names of their fields.
const FIELD_SETS: &[(&str, &[&str])] = &[
    ("s_xy", &["x", "y"]),
    ("s_abc", &["a", "b", "c"]),
    ("s_one", &["v"]),
    ("s_xz", &["x", "z"]),
    ("s_nest", &["inner", "n"]),
];

fn frame() -> DataFrame {
    let whole_numbers = |name: &str| Series::new(name.into(), [1_i64, 2, 3]);
    let text = |name: &str| Series::new(name.into(), ["a", "b", "c"]);
    let inner = StructChunked::from_series("inner".into(), 3, [whole_numbers("p")].iter())
        .expect("a struct of one field")
        .into_series();
    let columns: Vec<Column> = vec![
        struct_column("s_xy", vec![whole_numbers("x"), text("y")]),
        struct_column(
            "s_abc",
            vec![
                Series::new("a".into(), [1.5_f64, 2.5, 3.5]),
                Series::new("b".into(), [true, false, true]),
                whole_numbers("c")
                    .cast(&DataType::Date)
                    .expect("whole numbers cast to dates"),
            ],
        ),
        struct_column(
            "s_one",
            vec![
                whole_numbers("v")
                    .cast(&DataType::UInt8)
                    .expect("whole numbers cast to a small one"),
            ],
        ),
        struct_column("s_xz", vec![whole_numbers("x"), whole_numbers("z")]),
        struct_column("s_nest", vec![inner, whole_numbers("n")]),
        struct_column(
            "s_arr",
            vec![arrays("w", &[[1, 2, 3], [3, 4, 5], [5, 6, 7]]).take_materialized_series()],
        ),
        Series::new("t".into(), ["a-b-c", "d-e", "f"]).into(),
        ListChunked::from_iter([
            Some(Series::new("".into(), [1_i64, 2, 3])),
            Some(Series::new("".into(), [4_i64])),
            None,
        ])
        .into_series()
        .with_name("l".into())
        .into(),
        arrays("raw", &[[1, 2], [3, 4], [5, 6]]),
        arrays("raw3", &[[1, 2, 3], [3, 4, 5], [5, 6, 7]]),
        whole_numbers("n").into(),
        whole_numbers("m").into(),
        whole_numbers("count").into(),
        Series::new("f".into(), [true, false, true]).into(),
        Series::new("g".into(), ["x", "y", "x"]).into(),
    ];
    DataFrame::new(3, columns).expect("columns of three rows")
}

fn inputs() -> Inputs {
    let mut inputs = Inputs::new();
    inputs
        .add_table(
            "other",
            "Other",
            df!("rk" => [1_i64, 2, 3]).expect("the other table").lazy(),
        )
        .expect("a first input");
    inputs
}

// ---------------------------------------------------------------------------
// What Polars and the walk say

#[derive(Debug, Clone, PartialEq, Eq)]
enum Said {
    Accepts,
    Refuses(String),
}

fn first_line(text: &impl ToString) -> String {
    text.to_string()
        .lines()
        .next()
        .unwrap_or_default()
        .to_owned()
}

fn polars_says(report: &Report) -> Said {
    let step = &report.steps()[0];
    match (step.outcome(), step.rows()) {
        (Outcome::Valid { .. }, StepRows::Counted(_)) => Said::Accepts,
        (Outcome::Valid { .. }, StepRows::Failed(error)) => Said::Refuses(first_line(error)),
        (Outcome::Refused(refusal), _) => Said::Refuses(first_line(refusal)),
        (other, _) => Said::Refuses(format!("{other:?}")),
    }
}

/// What the walk and Polars say of one step, the name of the rule that refused it where a rule did,
/// and the dtype each gives after the last call of the expression the step holds first.
#[derive(Debug)]
struct Found {
    polars: Said,
    walk_refusal: Option<(Option<&'static str>, String)>,
    walk_knows_nothing_of_it: Option<String>,
    polars_dtype: Option<DataType>,
    walked_name: Option<String>,
}

impl Found {
    fn rule(&self) -> Option<&'static str> {
        self.walk_refusal.as_ref().and_then(|(rule, _)| *rule)
    }

    fn reason(&self) -> &str {
        self.walk_refusal
            .as_ref()
            .map_or("", |(_, reason)| reason.as_str())
    }
}

fn ask_step(step: Value) -> Found {
    let document = Document::read(&document(vec![step])).expect("a document that reads");
    let report = document
        .run_on(&inputs(), &frame())
        .expect("a document that runs on a schema");
    let mut polars = polars_says(&report);
    if polars == Said::Accepts {
        let empty = frame().clear();
        let on_none = document
            .run_on(&inputs(), &empty)
            .expect("a document that runs on no rows");
        if polars_says(&on_none) != Said::Accepts {
            polars = Said::Refuses("the table with no rows".to_owned());
        }
    }
    let walk = report.steps()[0].walk();
    let walk_refusal = walk.refusal().map(|refusal| {
        let rule = match refusal.refusal().source() {
            RefusalSource::Rule(rule) => Some(rule.name()),
            _ => None,
        };
        (rule, refusal.refusal().reason().to_owned())
    });
    // The first item of the step: the expressions inside its arguments are listed before it.
    let item = [Segment::Parameter("exprs".to_owned()), Segment::Item(0)];
    let expression = report.steps()[0]
        .expressions()
        .iter()
        .find(|expression| expression.place().path() == item);
    let polars_dtype = expression.and_then(|expression| {
        match expression
            .links()
            .last()
            .unwrap_or_else(|| expression.start())
        {
            Resolution::Dtype(dtype) => Some(dtype.clone()),
            _ => None,
        }
    });
    let walked_name = walk
        .expressions()
        .iter()
        .find(|expression| expression.place().path() == item)
        .and_then(|expression| expression.calls().last()?.verdict()?.allowance())
        .and_then(|allowance| allowance.result().map(|result| result.name().to_owned()));
    Found {
        polars,
        walk_refusal,
        walk_knows_nothing_of_it: walk.not_known().map(str::to_owned),
        polars_dtype,
        walked_name,
    }
}

/// A select of the one expression that starts with `start` and goes on with `links`.
fn select_of(start: Value, links: Vec<Value>) -> Value {
    step(
        "s",
        "LazyFrame.select",
        json!({ "exprs": many(vec![chain(start, links)]) }),
    )
}

fn ask(column: &str, links: Vec<Value>) -> Found {
    ask_step(select_of(col_call(column), links))
}

fn the_field(name: &str) -> Value {
    call("Expr.struct.field", json!({ "name": { "field": name } }))
}

fn names(names: &[&str]) -> Value {
    many(names.iter().map(|name| json!({ "string": name })).collect())
}

fn rename(new: &[&str]) -> Value {
    call("Expr.struct.rename_fields", json!({ "names": names(new) }))
}

fn drop_fields(dropped: &[&str], strict: Option<bool>) -> Value {
    let named = many(
        dropped
            .iter()
            .map(|name| json!({ "field": name }))
            .collect(),
    );
    let mut arguments = json!({ "names": named });
    if let Some(strict) = strict {
        arguments["strict"] = json!({ "flag": strict });
    }
    call("Expr.struct.drop", arguments)
}

/// Asserts that Polars and the walk both accept, and that the walk writes the dtype Polars gives.
fn assert_accepted_as_polars_has_it(found: &Found, what: &str) {
    assert_eq!(found.polars, Said::Accepts, "Polars refuses {what}");
    assert!(
        found.walk_refusal.is_none(),
        "the walk refuses {what}: {:?}",
        found.walk_refusal
    );
    assert!(
        found.walk_knows_nothing_of_it.is_none(),
        "the walk cannot tell {what}: {:?}",
        found.walk_knows_nothing_of_it
    );
    let polars = found
        .polars_dtype
        .as_ref()
        .map(dtype_name)
        .unwrap_or_else(|| panic!("Polars gave no dtype for {what}"));
    assert_eq!(
        found.walked_name.as_deref(),
        Some(polars.as_str()),
        "the dtype the walk writes for {what}"
    );
}

/// Asserts that Polars refuses and the walk refuses by `rule`, saying `words` of it.
fn assert_refused_by(found: &Found, rule: &str, words: &[&str], what: &str) {
    assert!(
        matches!(found.polars, Said::Refuses(_)),
        "Polars accepts {what}"
    );
    assert_eq!(found.rule(), Some(rule), "the rule that refuses {what}");
    for word in words {
        assert!(
            found.reason().contains(word),
            "the words of the refusal of {what} name {word:?}: {:?}",
            found.reason()
        );
    }
}

// ---------------------------------------------------------------------------
// Fields from the schema

#[test]
fn a_struct_column_is_written_with_the_fields_its_schema_gives() {
    for (column, _) in FIELD_SETS {
        let found = ask(column, vec![call("Expr.first", json!({}))]);
        assert_accepted_as_polars_has_it(&found, &format!("the first of {column}"));
    }
}

#[test]
fn a_struct_column_keeps_its_fields_through_the_methods_that_keep_the_struct() {
    for (column, fields) in FIELD_SETS {
        for keep in [
            call("Expr.first", json!({})),
            call("Expr.last", json!({})),
            call("Expr.reverse", json!({})),
            call("Expr.shift", json!({})),
        ] {
            let name = fields[0];
            let found = ask(column, vec![keep.clone(), the_field(name)]);
            assert_accepted_as_polars_has_it(&found, &format!("{name} of {column} kept"));
            let missing = ask(column, vec![keep, the_field("q")]);
            assert_refused_by(&missing, "field-is-of-the-struct", fields, "q after a keep");
        }
    }
}

// ---------------------------------------------------------------------------
// struct.field

#[test]
fn a_field_gives_the_class_polars_gives_for_each_field_set() {
    for (column, fields) in FIELD_SETS {
        for name in *fields {
            let found = ask(column, vec![the_field(name)]);
            assert_accepted_as_polars_has_it(&found, &format!("{name} of {column}"));
        }
    }
}

#[test]
fn a_name_that_is_no_field_is_refused_by_the_rule_that_names_the_fields() {
    for (column, fields) in FIELD_SETS {
        let found = ask(column, vec![the_field("nope")]);
        assert_refused_by(
            &found,
            "field-is-of-the-struct",
            fields,
            &format!("nope of {column}"),
        );
    }
}

#[test]
fn a_field_of_a_field_is_read_where_the_inner_struct_is_known() {
    let found = ask("s_nest", vec![the_field("inner"), the_field("p")]);
    assert_accepted_as_polars_has_it(&found, "the p of the inner struct");
    let missing = ask("s_nest", vec![the_field("inner"), the_field("q")]);
    assert_refused_by(
        &missing,
        "field-is-of-the-struct",
        &["p"],
        "q of the inner struct",
    );
}

#[test]
fn a_field_of_a_column_that_is_no_struct_is_refused_as_polars_does() {
    let found = ask("n", vec![the_field("x")]);
    assert!(matches!(found.polars, Said::Refuses(_)));
    assert!(
        found.walk_refusal.is_some(),
        "the walk accepts a field of a whole number"
    );
}

// ---------------------------------------------------------------------------
// rename_fields, drop, json_encode

#[test]
fn renamed_fields_are_the_same_classes_under_new_names() {
    for (column, fields) in FIELD_SETS {
        let new: Vec<String> = (0..fields.len()).map(|at| format!("new_{at}")).collect();
        let new: Vec<&str> = new.iter().map(String::as_str).collect();
        let found = ask(column, vec![rename(&new)]);
        assert_accepted_as_polars_has_it(&found, &format!("{column} renamed"));
        let read = ask(column, vec![rename(&new), the_field(new[0])]);
        assert_accepted_as_polars_has_it(&read, &format!("{} of {column} renamed", new[0]));
        let old = ask(column, vec![rename(&new), the_field(fields[0])]);
        assert_refused_by(
            &old,
            "field-is-of-the-struct",
            &new,
            &format!("{} of {column} renamed", fields[0]),
        );
    }
}

#[test]
fn a_rename_with_another_number_of_names_than_fields_is_refused_by_a_rule() {
    for (column, fields) in FIELD_SETS {
        let fewer: Vec<&str> = ["a", "b", "c"][..fields.len() - 1].to_vec();
        let found = ask(column, vec![rename(&fewer)]);
        assert_refused_by(
            &found,
            "field-names-fit-the-struct",
            &[],
            &format!("fewer names for {column}"),
        );
        let more = ask(column, vec![rename(&["a", "b", "c", "d"])]);
        assert_refused_by(
            &more,
            "field-names-fit-the-struct",
            &[],
            &format!("more names for {column}"),
        );
    }
}

#[test]
fn a_rename_that_gives_two_fields_one_name_is_refused_by_a_rule() {
    let found = ask("s_xy", vec![rename(&["a", "a"])]);
    assert_refused_by(
        &found,
        "field-names-fit-the-struct",
        &["a"],
        "two fields named a",
    );
}

#[test]
fn dropped_fields_are_gone_from_what_the_walk_carries() {
    let found = ask("s_abc", vec![drop_fields(&["b"], None)]);
    assert_accepted_as_polars_has_it(&found, "s_abc without b");
    let kept = ask("s_abc", vec![drop_fields(&["b"], None), the_field("c")]);
    assert_accepted_as_polars_has_it(&kept, "c after b is dropped");
    let gone = ask("s_abc", vec![drop_fields(&["b"], None), the_field("b")]);
    assert_refused_by(
        &gone,
        "field-is-of-the-struct",
        &["a", "c"],
        "b after b is dropped",
    );
    let all = ask("s_xy", vec![drop_fields(&["x", "y"], None)]);
    assert_accepted_as_polars_has_it(&all, "s_xy without any field");
}

#[test]
fn a_drop_of_a_name_that_is_no_field_is_refused_where_it_is_strict() {
    let found = ask("s_abc", vec![drop_fields(&["q"], None)]);
    assert_refused_by(
        &found,
        "field-is-of-the-struct",
        &["a", "b", "c"],
        "q dropped",
    );
    let strict = ask("s_abc", vec![drop_fields(&["q"], Some(true))]);
    assert_refused_by(
        &strict,
        "field-is-of-the-struct",
        &["a", "b", "c"],
        "q dropped strictly",
    );
    let lenient = ask("s_abc", vec![drop_fields(&["q"], Some(false))]);
    assert_accepted_as_polars_has_it(
        &lenient,
        "q dropped where it is no field and drop is not strict",
    );
}

#[test]
fn a_struct_encoded_as_json_is_text() {
    for (column, _) in FIELD_SETS {
        let found = ask(column, vec![call("Expr.struct.json_encode", json!({}))]);
        assert_accepted_as_polars_has_it(&found, &format!("{column} as json"));
        assert_eq!(found.walked_name.as_deref(), Some("String"));
    }
}

// ---------------------------------------------------------------------------
// unnest

fn unnest() -> Value {
    call("Expr.struct.unnest", json!({}))
}

fn select_items(items: Vec<Value>) -> Value {
    step("s", "LazyFrame.select", json!({ "exprs": many(items) }))
}

fn with_columns_items(items: Vec<Value>) -> Value {
    step(
        "s",
        "LazyFrame.with_columns",
        json!({ "exprs": many(items) }),
    )
}

#[test]
fn an_unnest_stands_as_an_item_of_a_select_and_of_with_columns() {
    for (column, _) in FIELD_SETS {
        for held in [
            select_items(vec![chain(col_call(column), vec![unnest()])]),
            with_columns_items(vec![chain(col_call(column), vec![unnest()])]),
        ] {
            let found = ask_step(held);
            assert_eq!(
                found.polars,
                Said::Accepts,
                "Polars refuses an unnest of {column}"
            );
            assert!(
                found.walk_refusal.is_none(),
                "the walk refuses an unnest of {column}: {:?}",
                found.walk_refusal
            );
        }
    }
}

#[test]
fn an_unnest_beside_a_column_of_another_name_is_accepted() {
    let found = ask_step(select_items(vec![
        chain(col_call("s_xy"), vec![unnest()]),
        field("n"),
    ]));
    assert_eq!(found.polars, Said::Accepts);
    assert!(found.walk_refusal.is_none(), "{:?}", found.walk_refusal);
}

#[test]
fn an_unnest_whose_field_is_named_as_another_item_writes_is_refused_by_a_rule() {
    // The first writes the columns x and y, the second the column x.
    let held = select_items(vec![
        chain(col_call("s_xy"), vec![unnest()]),
        chain(col_call("s_xz"), vec![the_field("x")]),
    ]);
    let found = ask_step(held);
    assert_refused_by(
        &found,
        "written-names-are-distinct",
        &["x"],
        "x written twice in a select",
    );
    let held = with_columns_items(vec![
        chain(col_call("s_xy"), vec![unnest()]),
        chain(col_call("s_xz"), vec![unnest()]),
    ]);
    let found = ask_step(held);
    assert_refused_by(
        &found,
        "written-names-are-distinct",
        &["x"],
        "x written twice in with_columns",
    );
}

#[test]
fn an_unnest_of_a_struct_renamed_to_a_name_that_is_not_taken_is_accepted() {
    let held = select_items(vec![
        chain(col_call("s_xy"), vec![unnest()]),
        chain(col_call("s_xz"), vec![rename(&["p", "q"]), unnest()]),
    ]);
    let found = ask_step(held);
    assert_eq!(found.polars, Said::Accepts);
    assert!(found.walk_refusal.is_none(), "{:?}", found.walk_refusal);
}

#[test]
fn an_unnest_of_several_fields_is_refused_where_one_value_is_needed() {
    let held = step(
        "s",
        "LazyFrame.filter",
        json!({ "predicates": many(vec![chain(col_call("s_abc"), vec![unnest()])]) }),
    );
    let found = ask_step(held);
    assert_refused_by(
        &found,
        "one-column-is-needed",
        &[],
        "an unnest of three fields as a predicate",
    );
}

#[test]
fn an_unnest_of_one_field_is_one_column_where_one_value_is_needed() {
    let held = step(
        "s",
        "LazyFrame.filter",
        json!({ "predicates": many(vec![chain(
            col_call("s_one"),
            vec![unnest(), call("Expr.is_not_null", json!({}))],
        )]) }),
    );
    let found = ask_step(held);
    assert_eq!(found.polars, Said::Accepts);
    assert!(found.walk_refusal.is_none(), "{:?}", found.walk_refusal);
}

#[test]
fn an_unnest_stands_as_a_sort_key() {
    let held = step(
        "s",
        "LazyFrame.sort",
        json!({ "by": many(vec![chain(col_call("s_xy"), vec![unnest()])]) }),
    );
    let found = ask_step(held);
    assert_eq!(found.polars, Said::Accepts);
    assert!(found.walk_refusal.is_none(), "{:?}", found.walk_refusal);
}

// ---------------------------------------------------------------------------
// polars.struct

fn a_struct_of(items: Vec<Value>, links: Vec<Value>) -> Found {
    ask_step(select_of(
        call("polars.struct", json!({ "exprs": many(items) })),
        links,
    ))
}

#[test]
fn a_struct_made_of_columns_has_a_field_for_each_named_as_its_column() {
    let found = a_struct_of(vec![field("n"), field("g"), field("f")], vec![]);
    assert_accepted_as_polars_has_it(&found, "a struct of three columns");
    let read = a_struct_of(vec![field("n"), field("g")], vec![the_field("g")]);
    assert_accepted_as_polars_has_it(&read, "g of a struct of n and g");
    let missing = a_struct_of(vec![field("n"), field("g")], vec![the_field("m")]);
    assert_refused_by(
        &missing,
        "field-is-of-the-struct",
        &["n", "g"],
        "m of a struct of n and g",
    );
}

#[test]
fn a_struct_made_of_a_field_is_named_by_the_field() {
    let found = a_struct_of(
        vec![chain(col_call("s_xy"), vec![the_field("y")]), field("n")],
        vec![the_field("y")],
    );
    assert_accepted_as_polars_has_it(&found, "y of a struct of the y of s_xy and n");
}

#[test]
fn a_struct_made_of_two_inputs_named_alike_is_refused_by_a_rule() {
    let found = a_struct_of(vec![field("n"), field("n")], vec![]);
    assert_refused_by(
        &found,
        "field-names-fit-the-struct",
        &["n"],
        "a struct of n twice",
    );
    let literals = a_struct_of(vec![whole(1), whole(2)], vec![]);
    assert_refused_by(
        &literals,
        "field-names-fit-the-struct",
        &["literal"],
        "a struct of two literals",
    );
}

#[test]
fn a_struct_made_of_a_literal_has_a_field_named_literal() {
    let found = a_struct_of(vec![field("n"), text("x")], vec![the_field("literal")]);
    assert_accepted_as_polars_has_it(&found, "the literal of a struct of n and text");
}

// ---------------------------------------------------------------------------
// split_exact and splitn

fn split_exact(parts: i64) -> Value {
    call(
        "Expr.str.split_exact",
        json!({ "by": chain(text_call("-"), vec![]), "n": { "integer": parts } }),
    )
}

fn splitn(parts: i64) -> Value {
    call(
        "Expr.str.splitn",
        json!({ "by": chain(text_call("-"), vec![]), "n": { "integer": parts } }),
    )
}

#[test]
fn a_split_exact_gives_one_field_more_than_its_number_named_field_0_and_on() {
    for parts in [0, 1, 2, 4] {
        let found = ask("t", vec![split_exact(parts)]);
        assert_accepted_as_polars_has_it(&found, &format!("t split exactly in {parts}"));
        let last = ask(
            "t",
            vec![split_exact(parts), the_field(&format!("field_{parts}"))],
        );
        assert_accepted_as_polars_has_it(&last, &format!("the last field of {parts}"));
        let past = ask(
            "t",
            vec![
                split_exact(parts),
                the_field(&format!("field_{}", parts + 1)),
            ],
        );
        assert_refused_by(
            &past,
            "field-is-of-the-struct",
            &["field_0"],
            &format!("a field past {parts}"),
        );
    }
}

#[test]
fn a_splitn_gives_as_many_fields_as_its_number() {
    for parts in [0, 1, 2, 4] {
        let found = ask("t", vec![splitn(parts)]);
        assert_accepted_as_polars_has_it(&found, &format!("t split in at most {parts}"));
        let past = ask(
            "t",
            vec![splitn(parts), the_field(&format!("field_{parts}"))],
        );
        assert_refused_by(
            &past,
            "field-is-of-the-struct",
            &[],
            &format!("a field past {parts}"),
        );
    }
}

// ---------------------------------------------------------------------------
// to_struct

fn list_to_struct(fields: &[&str]) -> Value {
    call("Expr.list.to_struct", json!({ "fields": names(fields) }))
}

fn array_to_struct(fields: Option<&[&str]>) -> Value {
    match fields {
        Some(fields) => call("Expr.arr.to_struct", json!({ "fields": names(fields) })),
        None => call("Expr.arr.to_struct", json!({})),
    }
}

#[test]
fn a_list_made_a_struct_has_the_fields_it_is_given_of_its_element() {
    let found = ask("l", vec![list_to_struct(&["a", "b"])]);
    assert_accepted_as_polars_has_it(&found, "a list as a struct of a and b");
    let read = ask("l", vec![list_to_struct(&["a", "b", "c"]), the_field("c")]);
    assert_accepted_as_polars_has_it(&read, "c of a list as a struct");
    let missing = ask("l", vec![list_to_struct(&["a", "b"]), the_field("c")]);
    assert_refused_by(
        &missing,
        "field-is-of-the-struct",
        &["a", "b"],
        "c of a struct of a and b",
    );
}

#[test]
fn a_list_or_an_array_made_a_struct_of_a_name_given_twice_is_refused_by_a_rule_of_its_own() {
    let list = ask("l", vec![list_to_struct(&["a", "a"])]);
    assert_refused_by(
        &list,
        "to-struct-names-are-distinct",
        &["a"],
        "a list as a struct of a twice",
    );
    let array = ask("raw", vec![array_to_struct(Some(&["a", "a"]))]);
    assert_refused_by(
        &array,
        "to-struct-names-are-distinct",
        &["a"],
        "an array as a struct of a twice",
    );
    // A struct made of columns, or renamed, is refused by Polars, and by the rule about names.
    let renamed = ask("s_xy", vec![rename(&["a", "a"])]);
    assert_refused_by(
        &renamed,
        "field-names-fit-the-struct",
        &["a"],
        "a struct renamed to a twice",
    );
}

#[test]
fn polars_accepts_a_list_made_a_struct_of_a_name_given_twice_and_builds_one_that_disagrees_with_itself()
 {
    // This is why the rule above refuses it, and the rule can go once a Polars release refuses
    // the names itself (pola-rs/polars#30725). `polars.struct` and `struct.rename_fields` refuse
    // the same names, which the test above holds.
    let names = ["y", "y"];
    let made = col("l").list().to_struct(
        names
            .iter()
            .map(|name| PlSmallStr::from(*name))
            .collect::<Vec<_>>()
            .into(),
    );
    let lazy = frame().lazy().select([made.alias("s")]);
    // The plan accepts it, and gives a struct of two fields named alike.
    let schema = lazy
        .clone()
        .collect_schema()
        .expect("Polars accepts the names");
    let DataType::Struct(in_the_dtype) = schema.get("s").expect("a column s") else {
        panic!("a struct");
    };
    assert_eq!(
        in_the_dtype
            .iter()
            .map(|field| field.name().as_str())
            .collect::<Vec<_>>(),
        ["y", "y"]
    );
    let collected = lazy.clone().collect().expect("Polars runs it");
    let series = collected
        .column("s")
        .expect("a column s")
        .as_materialized_series();
    let held = series.struct_().expect("a struct");
    // A field read of the name gives the first of the two, where the value of a row holds both.
    let read = held.field_by_name("y").expect("a field y");
    assert_eq!(read.get(0).expect("a value"), AnyValue::Int64(1));
    let second = &held.fields_as_series()[1];
    assert_eq!(second.get(0).expect("a value"), AnyValue::Int64(2));
    // The unnest of the column is refused, as the names are, after the plan accepted them.
    let unnested = lazy.clone().unnest(cols(["s"]), None).collect();
    assert!(
        unnested
            .expect_err("a column named twice")
            .to_string()
            .contains("more than one occurrence")
    );
    // And `clear` of a frame that holds it panics, so the panic is caught, not let to abort.
    let cleared =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| collected.clear().height()));
    assert!(cleared.is_err(), "clearing a frame that holds it panics");
}

#[test]
fn an_array_made_a_struct_has_a_field_for_each_place_named_by_its_width_or_its_names() {
    let found = ask("raw", vec![array_to_struct(None)]);
    assert_accepted_as_polars_has_it(&found, "an array of two as a struct");
    let wide = ask("raw3", vec![array_to_struct(None), the_field("field_2")]);
    assert_accepted_as_polars_has_it(&wide, "field_2 of an array of three as a struct");
    let past = ask("raw", vec![array_to_struct(None), the_field("field_2")]);
    assert_refused_by(
        &past,
        "field-is-of-the-struct",
        &["field_0", "field_1"],
        "field_2 of an array of two",
    );
    let named = ask(
        "raw3",
        vec![array_to_struct(Some(&["a", "b", "c"])), the_field("b")],
    );
    assert_accepted_as_polars_has_it(&named, "b of an array of three named");
}

#[test]
fn an_array_given_other_names_than_its_width_is_refused_by_a_rule() {
    let fewer = ask("raw3", vec![array_to_struct(Some(&["a", "b"]))]);
    assert_refused_by(
        &fewer,
        "field-names-fit-the-struct",
        &["3"],
        "two names for an array of three",
    );
    let more = ask("raw", vec![array_to_struct(Some(&["a", "b", "c"]))]);
    assert_refused_by(
        &more,
        "field-names-fit-the-struct",
        &["2"],
        "three names for an array of two",
    );
}

// ---------------------------------------------------------------------------
// value_counts

fn value_counts(options: Value) -> Value {
    call("Expr.value_counts", options)
}

#[test]
fn the_value_counts_of_a_column_is_a_struct_of_the_column_and_its_count() {
    let found = ask("n", vec![value_counts(json!({}))]);
    assert_accepted_as_polars_has_it(&found, "the value counts of n");
    for (column, _) in [("g", ()), ("f", ()), ("l", ())] {
        let found = ask(column, vec![value_counts(json!({}))]);
        assert_accepted_as_polars_has_it(&found, &format!("the value counts of {column}"));
    }
    let counted = ask("n", vec![value_counts(json!({})), the_field("count")]);
    assert_accepted_as_polars_has_it(&counted, "the count of the value counts of n");
    let own = ask("n", vec![value_counts(json!({})), the_field("n")]);
    assert_accepted_as_polars_has_it(&own, "the values of the value counts of n");
    let missing = ask("n", vec![value_counts(json!({})), the_field("proportion")]);
    assert_refused_by(
        &missing,
        "field-is-of-the-struct",
        &["n", "count"],
        "proportion of the counts",
    );
}

#[test]
fn the_value_counts_names_its_count_as_it_is_told() {
    let named = ask(
        "n",
        vec![
            value_counts(json!({ "name": { "string": "times" } })),
            the_field("times"),
        ],
    );
    assert_accepted_as_polars_has_it(&named, "the times of the value counts of n");
    let proportion = ask(
        "n",
        vec![
            value_counts(json!({ "normalize": { "flag": true } })),
            the_field("proportion"),
        ],
    );
    assert_accepted_as_polars_has_it(&proportion, "the proportion of the value counts of n");
    let both = ask(
        "n",
        vec![
            value_counts(json!({ "normalize": { "flag": true }, "name": { "string": "share" } })),
            the_field("share"),
        ],
    );
    assert_accepted_as_polars_has_it(&both, "the share of the value counts of n");
}

#[test]
fn the_value_counts_of_a_column_named_as_its_count_is_refused_by_a_rule() {
    let clash = ask(
        "n",
        vec![value_counts(json!({ "name": { "string": "n" } }))],
    );
    assert_refused_by(
        &clash,
        "field-names-fit-the-struct",
        &["n"],
        "a count named as its column",
    );
    let column = ask("count", vec![value_counts(json!({}))]);
    assert_refused_by(
        &column,
        "field-names-fit-the-struct",
        &["count"],
        "the counts of a column named count",
    );
}

#[test]
fn the_value_counts_of_a_column_is_a_count_of_values_of_its_own() {
    // The values that differ are not the table's rows, so a filter refuses a struct of them as it
    // refuses the unique values: here, a rule says that a struct is no predicate either way.
    let found = ask("n", vec![value_counts(json!({}))]);
    assert_eq!(found.polars, Said::Accepts);
    assert!(found.walk_refusal.is_none(), "{:?}", found.walk_refusal);
}

// ---------------------------------------------------------------------------
// The tables hold the one class for the ordinary methods

#[test]
fn the_class_a_struct_has_is_one_whatever_its_fields() {
    for (column, _) in FIELD_SETS {
        let found = ask(column, vec![call("Expr.is_null", json!({}))]);
        assert_eq!(found.polars, Said::Accepts);
        assert!(
            found.walk_refusal.is_none(),
            "{column}: {:?}",
            found.walk_refusal
        );
        assert_eq!(
            Representative::of(&frame().column(column).unwrap().dtype().clone()),
            Some(Representative::Struct)
        );
    }
}

#[test]
fn an_unnest_as_a_join_key_is_one_the_walk_cannot_tell_to_stand() {
    // Polars takes the keys of one side as many columns as the other side's, and writes them by the
    // names of the fields, so what the tables measured of one column cannot judge it.
    let held = step(
        "s",
        "LazyFrame.join",
        json!({
            "other": { "table": "other" },
            "left_on": many(vec![chain(col_call("s_xy"), vec![unnest()])]),
            "right_on": many(vec![field("rk"), field("rk")]),
        }),
    );
    let found = ask_step(held);
    assert!(
        found.walk_refusal.is_none(),
        "the walk refuses an unnest as a join key: {:?}",
        found.walk_refusal
    );
    assert!(found.walk_knows_nothing_of_it.is_some());
}

#[test]
fn an_unnest_of_one_field_is_named_by_the_field_as_a_struct_input() {
    // The struct of an unnest of one field and a literal holds two fields named `literal`, and
    // Polars refuses it, as the walk must for the name the unnest gives its value.
    let found = a_struct_of(
        vec![
            chain(
                call("polars.struct", json!({ "exprs": many(vec![whole(1)]) })),
                vec![unnest()],
            ),
            chain(
                call("polars.lit", json!({ "value": { "flag": true } })),
                vec![],
            ),
        ],
        vec![],
    );
    assert_refused_by(
        &found,
        "field-names-fit-the-struct",
        &["literal"],
        "a struct of the unnested literal and a flag",
    );
}

#[test]
fn keys_that_an_unnest_of_one_field_names_alike_are_refused_as_a_repeated_pair() {
    let key = |links: Vec<Value>| chain(col_call("s_one"), links);
    let held = step(
        "s",
        "LazyFrame.join",
        json!({
            "other": { "table": "other" },
            "left_on": many(vec![
                key(vec![unnest(), call("Expr.struct.json_encode", json!({}))]),
                key(vec![unnest(), call("Expr.abs", json!({})), call("Expr.struct.json_encode", json!({}))]),
            ]),
            "right_on": many(vec![field("rk"), field("rk")]),
        }),
    );
    let found = ask_step(held);
    assert_refused_by(
        &found,
        "join-key-names-are-not-repeated",
        &[],
        "two keys of the field v paired with rk twice",
    );
}

#[test]
fn a_field_of_an_untyped_number_adapts_to_what_it_meets_so_the_walk_says_no_dtype_for_it() {
    // Polars keeps a number typed without a dtype open inside a struct, so reading its field and
    // meeting an unsigned number gives that unsigned dtype, where a field of a column, which has
    // its dtype, meets it at the dtype both hold. The walk does not say which of the two it is.
    let null = call(
        "polars.lit",
        json!({ "value": { "flag": null }, "dtype": { "dtype": "UInt32" } }),
    );
    let read = chain(
        call("polars.struct", json!({ "exprs": many(vec![whole(1)]) })),
        vec![the_field("literal")],
    );
    let found = ask_step(select_of(
        null,
        vec![call("Expr.and_", json!({ "others": many(vec![read]) }))],
    ));
    assert_eq!(found.polars, Said::Accepts, "Polars refuses the meeting");
    assert!(
        found.walk_refusal.is_none(),
        "the walk refuses it: {:?}",
        found.walk_refusal
    );
    assert!(
        found.walk_knows_nothing_of_it.is_some(),
        "the walk writes the dtype {:?}",
        found.walked_name
    );
}

/// The column `s_one` with its one field dropped and the struct unnested: no expression at all.
fn unnest_of_none() -> Vec<Value> {
    vec![drop_fields(&["v"], None), unnest()]
}

#[test]
fn an_unnest_of_a_struct_with_no_field_is_refused_in_a_filter_as_polars_does() {
    let held = step(
        "s",
        "LazyFrame.filter",
        json!({ "predicates": many(vec![chain(col_call("s_one"), unnest_of_none())]) }),
    );
    let found = ask_step(held);
    assert_refused_by(
        &found,
        "one-column-is-needed",
        &["none"],
        "a filter of an unnest of a struct with no field",
    );
}

#[test]
fn an_unnest_of_a_struct_with_no_field_is_left_out_by_what_holds_it_and_the_walk_cannot_tell() {
    // Polars makes a struct of what is left, a column of the one literal.
    let found = a_struct_of(
        vec![whole(1), chain(col_call("s_one"), unnest_of_none())],
        vec![],
    );
    assert_eq!(found.polars, Said::Accepts, "Polars refuses the struct");
    assert!(
        found.walk_refusal.is_none(),
        "the walk refuses it: {:?}",
        found.walk_refusal
    );
    assert!(
        found.walk_knows_nothing_of_it.is_some(),
        "the walk says what it gives: {:?}",
        found.walked_name
    );
    // A column meets it as it meets nothing: the schema is empty.
    let beside = ask_step(select_of(
        col_call("k"),
        vec![call(
            "Expr.add",
            json!({ "other": chain(col_call("s_one"), unnest_of_none()) }),
        )],
    ));
    assert_eq!(beside.polars, Said::Accepts, "Polars refuses the sum");
    assert!(beside.walk_refusal.is_none(), "{:?}", beside.walk_refusal);
    assert!(beside.walk_knows_nothing_of_it.is_some());
}

#[test]
fn the_width_of_an_array_a_struct_holds_is_kept_where_the_field_is_read() {
    // The struct is made of the array, whose width of two the field read gives back, so names of
    // another number are refused as they are for the column itself.
    let fewer = a_struct_of(
        vec![field("raw")],
        vec![the_field("raw"), array_to_struct(Some(&["a"]))],
    );
    assert_refused_by(
        &fewer,
        "field-names-fit-the-struct",
        &["2"],
        "one name for an array of two read from a struct",
    );
    let right = a_struct_of(
        vec![field("raw")],
        vec![the_field("raw"), array_to_struct(Some(&["a", "b"]))],
    );
    assert_accepted_as_polars_has_it(&right, "two names for an array of two read from a struct");
}

#[test]
fn the_width_of_an_array_is_kept_where_a_struct_of_it_is_unnested() {
    let fewer = a_struct_of(
        vec![field("raw3")],
        vec![unnest(), array_to_struct(Some(&["a", "b"]))],
    );
    assert_refused_by(
        &fewer,
        "field-names-fit-the-struct",
        &["3"],
        "two names for an array of three unnested from a struct",
    );
}

#[test]
fn a_struct_column_that_holds_an_array_is_written_with_its_width() {
    let found = ask("s_arr", vec![the_field("w"), array_to_struct(Some(&["a"]))]);
    assert_refused_by(
        &found,
        "field-names-fit-the-struct",
        &["3"],
        "one name for an array of three in a struct column",
    );
}

// ---------------------------------------------------------------------------
// Arithmetic on a struct

/// What Polars does of arithmetic on a struct depends on its fields, one by one, by the dtype of
/// each and of what meets it: a struct of numbers is added to a number, a field of text refuses it,
/// a list field is added as a list, and the others differ for each operation. The walk does not say.
fn assert_the_walk_cannot_tell(found: &Found, what: &str) {
    assert!(
        found.walk_refusal.is_none(),
        "the walk refuses {what}: {:?}",
        found.walk_refusal
    );
    assert!(
        found.walk_knows_nothing_of_it.is_some(),
        "the walk says what {what} gives: {:?}",
        found.walked_name
    );
}

#[test]
fn arithmetic_on_a_struct_of_numbers_is_not_said_by_the_walk_where_polars_does_it() {
    for operation in ["Expr.add", "Expr.sub", "Expr.mul", "Expr.truediv"] {
        let found = ask("s_xz", vec![call(operation, json!({ "other": whole(2) }))]);
        assert_eq!(found.polars, Said::Accepts, "Polars refuses {operation}");
        assert_the_walk_cannot_tell(&found, operation);
    }
}

#[test]
fn arithmetic_that_polars_always_refuses_a_struct_is_refused_by_a_rule() {
    // `abs`, `neg`, `sign`, `clip` and `pow` of a struct, whatever its fields are.
    for operation in ["Expr.abs", "Expr.neg", "Expr.sign"] {
        let found = ask("s_xz", vec![call(operation, json!({}))]);
        assert_refused_by(
            &found,
            "struct-arithmetic-needs-numbers",
            &[],
            &format!("{operation} of a struct"),
        );
    }
    let clip = ask(
        "s_xz",
        vec![call("Expr.clip", json!({ "lower_bound": whole(0) }))],
    );
    assert_refused_by(
        &clip,
        "struct-arithmetic-needs-numbers",
        &[],
        "clip of a struct",
    );
    let base = ask(
        "s_xz",
        vec![call("Expr.pow", json!({ "exponent": whole(2) }))],
    );
    assert_refused_by(
        &base,
        "struct-arithmetic-needs-numbers",
        &[],
        "a struct to a power",
    );
    let exponent = ask(
        "n",
        vec![call("Expr.pow", json!({ "exponent": field("s_xz") }))],
    );
    assert_refused_by(
        &exponent,
        "struct-arithmetic-needs-numbers",
        &[],
        "a number to the power of a struct",
    );
}

#[test]
fn arithmetic_on_a_struct_with_a_field_that_is_no_number_is_refused_in_polars_words() {
    // `s_xy` holds a field of text, `s_abc` one of a date.
    for column in ["s_xy", "s_abc"] {
        for operation in [
            "Expr.add",
            "Expr.sub",
            "Expr.mul",
            "Expr.truediv",
            "Expr.floordiv",
            "Expr.mod",
        ] {
            let found = ask(column, vec![call(operation, json!({ "other": whole(2) }))]);
            assert_refused_by(
                &found,
                "struct-arithmetic-needs-numbers",
                &["non-numeric field"],
                &format!("{operation} of {column}"),
            );
        }
    }
    // A number that meets such a struct, on either side.
    let met = ask(
        "n",
        vec![call("Expr.sub", json!({ "other": field("s_xy") }))],
    );
    assert_refused_by(
        &met,
        "struct-arithmetic-needs-numbers",
        &["non-numeric field"],
        "a number less a struct of text",
    );
    // A sum with a text is one Polars does, and the walk cannot tell what it gives.
    let text = ask(
        "s_xy",
        vec![call("Expr.add", json!({ "other": text("a") }))],
    );
    assert_eq!(text.polars, Said::Accepts, "{:?}", text.polars);
    assert_the_walk_cannot_tell(&text, "a sum of a struct and a text");
}

#[test]
fn arithmetic_given_a_struct_to_meet_is_not_said_by_the_walk() {
    let found = ask(
        "n",
        vec![call("Expr.add", json!({ "other": field("s_xz") }))],
    );
    assert_the_walk_cannot_tell(&found, "a sum of a number and a struct");
}

// ---------------------------------------------------------------------------
// A number typed without a dtype in a struct that is a join's key

fn struct_of_a_number(typed: bool) -> Value {
    let literal = if typed {
        call(
            "polars.lit",
            json!({ "value": { "integer": 5 }, "dtype": { "dtype": "Int32" } }),
        )
    } else {
        call("polars.lit", json!({ "value": { "integer": 5 } }))
    };
    chain(
        call(
            "polars.struct",
            json!({ "exprs": many(vec![chain(literal, vec![])]) }),
        ),
        vec![],
    )
}

#[test]
fn a_struct_of_an_untyped_number_as_a_key_is_not_said_by_the_walk_where_polars_matches_its_dtype() {
    // Polars keeps the number open in a key, so the key does not match the same struct with the
    // number given its dtype, though both resolve to one. See Polars issue draft 23.
    let key = |left: bool, right: bool| {
        step(
            "s",
            "LazyFrame.join",
            json!({
                "other": { "table": "other" },
                "left_on": many(vec![struct_of_a_number(left)]),
                "right_on": many(vec![struct_of_a_number(right)]),
            }),
        )
    };
    let open_and_typed = ask_step(key(false, true));
    assert!(
        matches!(open_and_typed.polars, Said::Refuses(_)),
        "{:?}",
        open_and_typed.polars
    );
    assert_the_walk_cannot_tell(&open_and_typed, "a key of an open number");
    // Both typed is the keys of one dtype, which the walk says.
    let typed = ask_step(key(true, true));
    assert_eq!(typed.polars, Said::Accepts, "{:?}", typed.polars);
    assert!(typed.walk_refusal.is_none(), "{:?}", typed.walk_refusal);
}

// ---------------------------------------------------------------------------
// A struct filled by a strategy

fn filled_by(strategy: &str) -> Value {
    call(
        "Expr.fill_null",
        json!({ "strategy": { "choice": strategy } }),
    )
}

#[test]
fn a_struct_is_filled_forward_or_backward_and_by_no_other_strategy() {
    // Polars fills a struct forward and backward, and refuses the others where it holds a null and
    // does nothing where it holds none, so the walk refuses them whatever the rows hold.
    for strategy in ["forward", "backward"] {
        let found = ask("s_xz", vec![filled_by(strategy)]);
        assert_accepted_as_polars_has_it(&found, strategy);
    }
    for strategy in ["min", "max", "mean", "zero", "one"] {
        let found = ask("s_xz", vec![filled_by(strategy)]);
        assert_refused_by(
            &found,
            "struct-is-filled-forward-or-backward",
            &[strategy],
            &format!("a struct filled by {strategy}"),
        );
    }
}

#[test]
fn a_struct_of_an_untyped_number_that_a_call_keeps_is_as_open_as_the_struct_as_a_key() {
    // The struct stays open through the calls that keep it, as the largest of one does.
    for link in ["Expr.max", "Expr.last"] {
        let kept = chain(
            call(
                "polars.struct",
                json!({ "exprs": many(vec![chain(call("polars.lit", json!({ "value": { "float": 1.5 } })), vec![])]) }),
            ),
            vec![call(link, json!({}))],
        );
        let held = step(
            "s",
            "LazyFrame.join",
            json!({
                "other": { "table": "other" },
                "left_on": many(vec![field("rk")]),
                "right_on": many(vec![kept]),
            }),
        );
        let found = ask_step(held);
        assert!(
            found.walk_refusal.is_none(),
            "{link}: {:?}",
            found.walk_refusal
        );
        assert!(
            found.walk_knows_nothing_of_it.is_some(),
            "{link}: the walk says what the key gives: {:?}",
            found.walked_name
        );
    }
}

#[test]
fn an_unnest_of_a_struct_with_no_field_is_a_predicate_of_nothing_that_the_walk_cannot_tell() {
    // A condition whose predicate expands to no expression is made as Polars makes it.
    let predicate = chain(col_call("s_one"), unnest_of_none());
    let condition = chain(
        call(
            "polars.when",
            json!({ "predicates": many(vec![predicate]) }),
        ),
        vec![
            call("When.then", json!({ "statement": whole(1) })),
            call("Then.otherwise", json!({ "statement": whole(2) })),
        ],
    );
    let found = ask_step(step(
        "s",
        "LazyFrame.select",
        json!({ "exprs": many(vec![condition]) }),
    ));
    assert!(
        found.walk_refusal.is_none(),
        "the walk refuses it: {:?}",
        found.walk_refusal
    );
}

#[test]
fn two_inputs_that_write_one_name_are_refused_though_another_writes_none_known() {
    // A condition is named by what the walk cannot say here, but the two literals write `literal`.
    let condition = chain(
        call(
            "polars.when",
            json!({ "predicates": many(vec![field("f")]) }),
        ),
        vec![call("When.then", json!({ "statement": field("n") }))],
    );
    let found = a_struct_of(vec![whole(1), condition, whole(2)], vec![]);
    assert_refused_by(
        &found,
        "field-names-fit-the-struct",
        &["literal"],
        "a struct of two literals and a condition",
    );
}

#[test]
fn an_open_number_in_a_struct_in_a_struct_is_as_open_as_one_in_a_struct_as_a_key() {
    let inner = chain(
        call(
            "polars.struct",
            json!({ "exprs": many(vec![chain(call("polars.lit", json!({ "value": { "float": 1.5 } })), vec![])]) }),
        ),
        vec![],
    );
    let outer = chain(
        call("polars.struct", json!({ "exprs": many(vec![inner]) })),
        vec![],
    );
    let held = step(
        "s",
        "LazyFrame.join",
        json!({
            "other": { "table": "other" },
            "left_on": many(vec![outer]),
            "right_on": many(vec![field("rk")]),
        }),
    );
    let found = ask_step(held);
    assert!(found.walk_refusal.is_none(), "{:?}", found.walk_refusal);
    assert!(
        found.walk_knows_nothing_of_it.is_some(),
        "the walk says what the key gives: {:?}",
        found.walked_name
    );
}

#[test]
fn a_call_given_an_unnest_of_no_field_is_made_as_if_it_were_not_there_and_the_walk_cannot_tell() {
    // Python Polars 2.2.0 makes `select("g", col("g").str.tail(z))` of three rows and one column,
    // for `z` an unnest of a struct of no field: an expression given one expands to none. The
    // walk does not judge what holds it, as it does not judge the call on a struct it is meant for.
    let none = chain(col_call("s_one"), unnest_of_none());
    let found = ask("s_xz", vec![call("Expr.str.tail", json!({ "n": none }))]);
    assert!(
        found.walk_refusal.is_none(),
        "the walk refuses it: {:?}",
        found.walk_refusal
    );
    assert!(
        found.walk_knows_nothing_of_it.is_some(),
        "the walk says what it gives: {:?}",
        found.walked_name
    );
}

#[test]
fn a_filter_of_several_predicates_takes_an_unnest_beside_the_others_as_polars_does() {
    // Polars refuses an expression that expands to none or to several only as the sole predicate
    // of a filter: beside another it is taken, each expression it expands to a predicate.
    for (what, predicate) in [
        (
            "an unnest of no field",
            chain(col_call("s_one"), unnest_of_none()),
        ),
        (
            "an unnest of two flags",
            chain(
                call(
                    "polars.struct",
                    json!({ "exprs": many(vec![
                        field("f"),
                        field_then("n", vec![call("Expr.gt", json!({ "other": whole(1) }))]),
                    ]) }),
                ),
                vec![unnest()],
            ),
        ),
    ] {
        let found = ask_step(step(
            "s",
            "LazyFrame.filter",
            json!({ "predicates": many(vec![field("f"), predicate]) }),
        ));
        assert_eq!(
            found.polars,
            Said::Accepts,
            "Polars refuses {what} beside a flag"
        );
        assert!(
            found.walk_refusal.is_none(),
            "the walk refuses {what} beside a flag: {:?}",
            found.walk_refusal
        );
    }
    // Beside a flag a field that is no flag is refused, as a predicate of any other dtype is.
    let found = ask_step(step(
        "s",
        "LazyFrame.filter",
        json!({ "predicates": many(vec![field("f"), chain(col_call("s_abc"), vec![unnest()])]) }),
    ));
    assert_refused_by(
        &found,
        "filter-predicate-is-boolean",
        &["Float64"],
        "an unnest of a struct of a number beside a flag",
    );
    // Alone they are refused, which the rule says and Polars does.
    for predicate in [
        chain(col_call("s_one"), unnest_of_none()),
        chain(col_call("s_abc"), vec![unnest()]),
    ] {
        let found = ask_step(step(
            "s",
            "LazyFrame.filter",
            json!({ "predicates": many(vec![predicate]) }),
        ));
        assert_refused_by(
            &found,
            "one-column-is-needed",
            &[],
            "an unnest alone in a filter",
        );
    }
}
#[test]
fn a_condition_whose_value_is_an_unnest_is_named_by_the_fields_not_by_the_column() {
    // Polars names what the condition gives by the fields it unnests, `x` and `z`, so the value
    // named after the column `s_xz` beside it is no name twice.
    let condition = chain(
        call(
            "polars.when",
            json!({ "predicates": many(vec![field("f")]) }),
        ),
        vec![call(
            "When.then",
            json!({ "statement": chain(col_call("s_xz"), vec![unnest()]) }),
        )],
    );
    let made = chain(
        call(
            "polars.struct",
            json!({ "exprs": many(vec![condition, field("s_xz")]) }),
        ),
        vec![],
    );
    let found = ask_step(step(
        "s",
        "LazyFrame.select",
        json!({ "exprs": many(vec![made]) }),
    ));
    assert_eq!(
        found.polars,
        Said::Accepts,
        "Polars refuses: {:?}",
        found.polars
    );
    assert!(
        found.walk_refusal.is_none(),
        "the walk refuses it: {:?}",
        found.walk_refusal
    );
}

#[test]
fn structs_of_other_fields_that_meet_are_not_said_by_the_walk() {
    // Polars casts the old and the new values of a replacement to what they replace, field by
    // field, and refuses or not by what the fields are, which the tables measured for one struct.
    for (old, new) in [("s_xy", "s_xz"), ("s_xz", "s_abc"), ("s_one", "s_xz")] {
        let found = ask(
            "s_xy",
            vec![call(
                "Expr.replace",
                json!({ "old": field(old), "new": field(new) }),
            )],
        );
        assert!(
            found.walk_refusal.is_none(),
            "the walk refuses {old} then {new}: {:?}",
            found.walk_refusal
        );
        assert!(
            found.walk_knows_nothing_of_it.is_some(),
            "the walk says what {old} then {new} gives: {:?}",
            found.walked_name
        );
    }
}
