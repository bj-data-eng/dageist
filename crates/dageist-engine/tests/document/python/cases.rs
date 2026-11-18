//! The documents whose Python source is checked: the claim-timing sample's
//! own steps, and single steps that between them use every entry of the
//! catalogue, with the arguments each one takes.

use std::sync::LazyLock;

use dageist_engine::document::Document;
use serde_json::{Value, json};

use super::fixtures::{Fixture, sample, varied};
use crate::support::{call, chain, document, field, field_then, many, named, step, text, whole};

static SAMPLE: LazyLock<Fixture> = LazyLock::new(sample);
static VARIED: LazyLock<Fixture> = LazyLock::new(varied);

/// A document, the rows it is run on and the inputs it names.
pub struct Case {
    pub name: String,
    pub fixture: &'static Fixture,
    text: String,
}

impl Case {
    pub fn document(&self) -> Document {
        Document::read(&self.text)
            .unwrap_or_else(|error| panic!("{} is not a document: {error}", self.name))
    }
}

fn float(number: f64) -> Value {
    chain(
        call("polars.lit", json!({ "value": { "float": number } })),
        vec![],
    )
}

fn flag(value: bool) -> Value {
    json!({ "flag": value })
}

fn choice(word: &str) -> Value {
    json!({ "choice": word })
}

fn columns(names: &[&str]) -> Value {
    json!({ "columns": names })
}

fn input(name: &str) -> Value {
    json!({ "expression": { "start": { "input": name }, "then": [] } })
}

fn input_then(name: &str, then: Vec<Value>) -> Value {
    json!({ "expression": { "start": { "input": name }, "then": then } })
}

/// A method of an expression, with its arguments.
fn method(entry: &str, arguments: Value) -> Value {
    call(entry, arguments)
}

fn cmp(entry: &str, other: Value) -> Value {
    call(entry, json!({ "other": other }))
}

/// `with_columns` of these named expressions.
fn with_columns(id: &str, columns: Vec<(&str, Value)>) -> Value {
    step(
        id,
        "LazyFrame.with_columns",
        json!({ "named_exprs": named(columns) }),
    )
}

fn one(name: &str, fixture: &'static Fixture, step: Value) -> Case {
    Case {
        name: name.to_owned(),
        fixture,
        text: document(vec![step]),
    }
}

/// The documents of the check.
pub fn cases() -> Vec<Case> {
    let mut cases = vec![Case {
        name: "sample".to_owned(),
        fixture: &SAMPLE,
        text: dageist_engine::claim_timing::ClaimTiming::STEPS_JSON.to_owned(),
    }];
    cases.extend(frame_steps());
    cases.extend(defaults_given());
    cases.extend(expressions());
    cases
}

/// Single steps that give a parameter the default Python Polars holds for it, which the source
/// leaves out: each must run in Python Polars to what the engine gives with the argument written
/// out. One step is made for each way a default is given: a flag for every key and one for each
/// key, a choice's word, text, and a method's option.
fn defaults_given() -> Vec<Case> {
    let f = &*VARIED;
    let sort = |name: &str, arguments: Value| one(name, f, step("s", "LazyFrame.sort", arguments));
    let both = |value: bool| many(vec![flag(value), flag(value)]);
    vec![
        sort(
            "sort_with_scalar_defaults_given",
            json!({
                "by": many(vec![field("group"), field("score")]),
                "descending": flag(false),
                "nulls_last": flag(false),
                "maintain_order": flag(false),
                "multithreaded": flag(true),
            }),
        ),
        sort(
            "sort_with_a_default_given_for_each_key",
            json!({
                "by": many(vec![field("group"), field("score")]),
                "descending": both(false),
                "nulls_last": both(false),
            }),
        ),
        sort(
            "sort_with_a_default_given_for_each_key_and_more_keys",
            json!({
                "by": many(vec![field("group")]),
                "more_by": many(vec![field("score")]),
                "descending": both(false),
                "nulls_last": flag(false),
            }),
        ),
        sort(
            "sort_with_one_key_descending_and_the_other_at_its_default",
            json!({
                "by": many(vec![field("group"), field("score")]),
                "descending": many(vec![flag(true), flag(false)]),
                "nulls_last": both(false),
            }),
        ),
        one(
            "join_with_its_defaults_given_beside_the_options_that_differ",
            f,
            step(
                "s",
                "LazyFrame.join",
                json!({
                    "other": { "table": "lookup" },
                    "how": choice("inner"),
                    "left_on": many(vec![field("name")]),
                    "right_on": many(vec![field("key")]),
                    "suffix": { "string": "_right" },
                    "validate": choice("m:m"),
                    "nulls_equal": flag(false),
                    "coalesce": flag(true),
                    // Rows come in a fixed order, or the two results could not be compared.
                    "maintain_order": choice("left"),
                    "build_side": choice("auto"),
                    "allow_parallel": flag(true),
                    "force_parallel": flag(false),
                }),
            ),
        ),
        one(
            "rename_with_strict_on",
            f,
            step(
                "s",
                "LazyFrame.rename",
                json!({
                    "mapping": { "pairs": [[columns(&["id"]), { "string": "ident" }]] },
                    "strict": flag(true),
                }),
            ),
        ),
        one(
            "pivot_with_the_defaults_given",
            f,
            step(
                "s",
                "LazyFrame.pivot",
                json!({
                    "on": columns(&["group"]),
                    "on_columns": { "table": "groups" },
                    "index": columns(&["id"]),
                    "values": columns(&["score"]),
                    "aggregate_function": chain(call("polars.element", json!({})), vec![method("Expr.first", json!({}))]),
                    "maintain_order": flag(false),
                    "separator": { "string": "_" },
                    "column_naming": choice("auto"),
                }),
            ),
        ),
        one(
            "over_with_its_options_at_their_defaults",
            f,
            with_columns(
                "s",
                vec![(
                    "ranked",
                    field_then(
                        "score",
                        vec![
                            method("Expr.cum_sum", json!({})),
                            method(
                                "Expr.over",
                                json!({
                                    "partition_by": many(vec![field("group")]),
                                    "descending": flag(false),
                                    "nulls_last": flag(false),
                                    "mapping_strategy": choice("group_to_rows"),
                                }),
                            ),
                        ],
                    ),
                )],
            ),
        ),
        one(
            "shift_with_its_count_given_and_a_filler",
            f,
            with_columns(
                "s",
                vec![(
                    "moved",
                    field_then(
                        "score",
                        vec![method("Expr.shift", json!({ "n": whole(1) }))],
                    ),
                )],
            ),
        ),
    ]
}

fn frame_steps() -> Vec<Case> {
    let f = &*VARIED;
    vec![
        one(
            "sort_with_every_option",
            f,
            step(
                "s",
                "LazyFrame.sort",
                json!({
                    "by": many(vec![field("group"), field("score")]),
                    "descending": many(vec![flag(false), flag(true)]),
                    "nulls_last": flag(true),
                    "maintain_order": flag(true),
                    "multithreaded": flag(false),
                }),
            ),
        ),
        one(
            "sort_by_one_field",
            f,
            step(
                "s",
                "LazyFrame.sort",
                json!({ "by": many(vec![field("name")]) }),
            ),
        ),
        one(
            "sort_by_an_expression_and_more_keys",
            f,
            step(
                "s",
                "LazyFrame.sort",
                json!({
                    "by": many(vec![field_then("score", vec![method("Expr.sub", json!({ "other": float(1.0) }))])]),
                    "more_by": many(vec![field("id")]),
                }),
            ),
        ),
        one(
            "select_fields_and_named",
            f,
            step(
                "s",
                "LazyFrame.select",
                json!({
                    "exprs": many(vec![field("id"), field("name")]),
                    "named_exprs": named(vec![(
                        "less",
                        field_then("score", vec![cmp("Expr.sub", float(1.5))]),
                    )]),
                }),
            ),
        ),
        one(
            "with_columns_positional_and_named",
            f,
            step(
                "s",
                "LazyFrame.with_columns",
                json!({
                    "exprs": many(vec![field_then("id", vec![method("Expr.cum_sum", json!({}))])]),
                    "named_exprs": named(vec![("copy", field("name"))]),
                }),
            ),
        ),
        one(
            "filter_predicates_and_constraints",
            f,
            step(
                "s",
                "LazyFrame.filter",
                json!({
                    "predicates": many(vec![
                        field_then("id", vec![cmp("Expr.gt", whole(1))]),
                        field_then("flag", vec![method("Expr.is_not_null", json!({}))]),
                    ]),
                    "constraints": named(vec![("group", text("y"))]),
                }),
            ),
        ),
        one(
            "rename_with_strict_off",
            f,
            step(
                "s",
                "LazyFrame.rename",
                json!({
                    "mapping": { "pairs": [
                        [columns(&["id"]), { "string": "ident" }],
                        [columns(&["name"]), { "string": "label" }],
                    ] },
                    "strict": flag(false),
                }),
            ),
        ),
        one(
            "join_with_every_option",
            f,
            step(
                "s",
                "LazyFrame.join",
                json!({
                    "other": { "table": "lookup" },
                    "how": choice("left"),
                    "left_on": many(vec![field("name")]),
                    "right_on": many(vec![field("key")]),
                    "suffix": { "string": "_lk" },
                    "validate": choice("m:1"),
                    "nulls_equal": flag(true),
                    "coalesce": flag(true),
                    "maintain_order": choice("left"),
                    "build_side": choice("auto"),
                    "allow_parallel": flag(false),
                    "force_parallel": flag(false),
                }),
            ),
        ),
        one(
            "join_on_a_shared_field",
            f,
            step(
                "s",
                "LazyFrame.join",
                json!({
                    "other": { "table": "members" },
                    "on": many(vec![field("group")]),
                    "how": choice("inner"),
                    "maintain_order": choice("left"),
                }),
            ),
        ),
        one(
            "join_on_two_fields",
            f,
            step(
                "s",
                "LazyFrame.join",
                json!({
                    "other": { "table": "lookup" },
                    "how": choice("inner"),
                    "left_on": many(vec![field("name"), field("group")]),
                    "right_on": many(vec![field("key"), field("tag")]),
                    "maintain_order": choice("left"),
                }),
            ),
        ),
        one(
            "pivot_with_every_option",
            f,
            step(
                "s",
                "LazyFrame.pivot",
                json!({
                    "on": columns(&["group"]),
                    "on_columns": { "table": "groups" },
                    "index": columns(&["id"]),
                    "values": columns(&["score"]),
                    "aggregate_function": chain(call("polars.element", json!({})), vec![method("Expr.first", json!({}))]),
                    "maintain_order": flag(true),
                    "separator": { "string": "-" },
                    "column_naming": choice("combine"),
                }),
            ),
        ),
    ]
}

fn expressions() -> Vec<Case> {
    let f = &*VARIED;
    let c = |name: &str| field(name);
    let on = |name: &str, links: Vec<Value>| field_then(name, links);
    let none = || json!({});
    vec![
        one(
            "cumulative",
            f,
            with_columns(
                "s",
                vec![
                    (
                        "a",
                        on(
                            "id",
                            vec![method("Expr.cum_count", json!({ "reverse": flag(true) }))],
                        ),
                    ),
                    (
                        "b",
                        on(
                            "id",
                            vec![method("Expr.cum_max", json!({ "reverse": flag(true) }))],
                        ),
                    ),
                    ("c", on("id", vec![method("Expr.cum_sum", none())])),
                    (
                        "d",
                        on(
                            "score",
                            vec![method("Expr.cum_sum", json!({ "reverse": flag(false) }))],
                        ),
                    ),
                ],
            ),
        ),
        one(
            "rows_and_aggregates",
            f,
            with_columns(
                "s",
                vec![
                    (
                        "shifted",
                        on(
                            "id",
                            vec![method(
                                "Expr.shift",
                                json!({ "n": whole(2), "fill_value": whole(0) }),
                            )],
                        ),
                    ),
                    (
                        "shifted_default",
                        on("id", vec![method("Expr.shift", none())]),
                    ),
                    (
                        "first",
                        on(
                            "score",
                            vec![method("Expr.first", json!({ "ignore_nulls": flag(true) }))],
                        ),
                    ),
                    ("last", on("score", vec![method("Expr.last", none())])),
                    ("max", on("score", vec![method("Expr.max", none())])),
                    ("min", on("id", vec![method("Expr.min", none())])),
                    ("sum", on("id", vec![method("Expr.sum", none())])),
                    ("len", on("id", vec![method("Expr.len", none())])),
                ],
            ),
        ),
        one(
            "over_with_every_option",
            f,
            with_columns(
                "s",
                vec![
                    (
                        "per_group",
                        on(
                            "id",
                            vec![
                                method("Expr.sum", none()),
                                method(
                                    "Expr.over",
                                    json!({
                                        "partition_by": many(vec![c("group")]),
                                        "more_exprs": many(vec![c("flag")]),
                                    }),
                                ),
                            ],
                        ),
                    ),
                    (
                        "ordered",
                        on(
                            "id",
                            vec![
                                method("Expr.cum_sum", none()),
                                method(
                                    "Expr.over",
                                    json!({
                                        "partition_by": many(vec![c("group")]),
                                        "order_by": many(vec![c("id")]),
                                        "descending": flag(true),
                                        "nulls_last": flag(true),
                                        "mapping_strategy": choice("group_to_rows"),
                                    }),
                                ),
                            ],
                        ),
                    ),
                    (
                        "ordered_without_a_group",
                        on(
                            "id",
                            vec![
                                method("Expr.cum_sum", none()),
                                method("Expr.over", json!({ "order_by": many(vec![c("score")]) })),
                            ],
                        ),
                    ),
                ],
            ),
        ),
        one(
            "temporal",
            f,
            with_columns(
                "s",
                vec![
                    ("date_of_at", on("at", vec![method("Expr.dt.date", none())])),
                    (
                        "date_of_day",
                        on("day", vec![method("Expr.dt.date", none())]),
                    ),
                    (
                        "month",
                        on("day", vec![method("Expr.dt.month_start", none())]),
                    ),
                    (
                        "later",
                        on(
                            "day",
                            vec![method("Expr.dt.offset_by", json!({ "by": text("1mo") }))],
                        ),
                    ),
                    ("time", on("at", vec![method("Expr.dt.time", none())])),
                    (
                        "days",
                        on(
                            "at",
                            vec![
                                cmp("Expr.sub", on("at", vec![method("Expr.shift", none())])),
                                method("Expr.dt.total_days", none()),
                            ],
                        ),
                    ),
                    (
                        "days_with_fraction",
                        on(
                            "at",
                            vec![
                                cmp("Expr.sub", on("at", vec![method("Expr.shift", none())])),
                                method("Expr.dt.total_days", json!({ "fractional": flag(true) })),
                            ],
                        ),
                    ),
                ],
            ),
        ),
        one(
            "comparisons",
            f,
            with_columns(
                "s",
                vec![
                    ("eq", on("id", vec![cmp("Expr.eq", whole(3))])),
                    ("ne", on("id", vec![cmp("Expr.ne", whole(3))])),
                    ("gt", on("id", vec![cmp("Expr.gt", c("id"))])),
                    ("ge", on("score", vec![cmp("Expr.ge", float(2.5))])),
                    ("lt", on("name", vec![cmp("Expr.lt", text("b"))])),
                    ("le", on("id", vec![cmp("Expr.le", input("limit"))])),
                    ("null", on("score", vec![method("Expr.is_null", none())])),
                    (
                        "not_null",
                        on("score", vec![method("Expr.is_not_null", none())]),
                    ),
                ],
            ),
        ),
        one(
            "literals",
            f,
            with_columns(
                "s",
                vec![
                    ("whole", whole(7)),
                    ("negative", whole(-3)),
                    ("whole_float", float(1.0)),
                    ("tenth", float(0.1)),
                    ("large", float(1e16)),
                    ("just_below_large", float(123_456_789_012_345.0)),
                    ("small", float(1e-5)),
                    ("just_above_small", float(0.0001)),
                    ("negative_float", float(-2.5e-7)),
                    ("text", text("say \"hi\"\n\\ é\t")),
                    (
                        "flag",
                        chain(call("polars.lit", json!({ "value": flag(true) })), vec![]),
                    ),
                    (
                        "typed",
                        chain(
                            call(
                                "polars.lit",
                                json!({ "value": { "integer": 3 }, "dtype": { "dtype": "Int8" } }),
                            ),
                            vec![],
                        ),
                    ),
                    (
                        "typed_float",
                        chain(
                            call(
                                "polars.lit",
                                json!({ "value": { "float": 2.0 }, "dtype": { "dtype": "Float32" }, "allow_object": flag(false) }),
                            ),
                            vec![],
                        ),
                    ),
                ],
            ),
        ),
        one(
            "conditions",
            f,
            with_columns(
                "s",
                vec![
                    (
                        "one_case",
                        chain(
                            call(
                                "polars.when",
                                json!({ "predicates": many(vec![on("id", vec![cmp("Expr.gt", whole(2))])]) }),
                            ),
                            vec![
                                call("When.then", json!({ "statement": c("name") })),
                                call("Then.otherwise", json!({ "statement": text("none") })),
                            ],
                        ),
                    ),
                    (
                        "more_cases",
                        chain(
                            call(
                                "polars.when",
                                json!({ "predicates": many(vec![
                                    on("id", vec![cmp("Expr.gt", whole(4))]),
                                    on("flag", vec![method("Expr.is_not_null", none())]),
                                ]) }),
                            ),
                            vec![
                                call("When.then", json!({ "statement": whole(1) })),
                                call(
                                    "Then.when",
                                    json!({ "predicates": many(vec![on("id", vec![cmp("Expr.gt", whole(2))])]) }),
                                ),
                                call("ChainedWhen.then", json!({ "statement": whole(2) })),
                                call(
                                    "ChainedThen.when",
                                    json!({ "constraints": named(vec![("group", text("x"))]) }),
                                ),
                                call("ChainedWhen.then", json!({ "statement": whole(3) })),
                                call("ChainedThen.otherwise", json!({ "statement": whole(4) })),
                            ],
                        ),
                    ),
                    (
                        "by_constraint",
                        chain(
                            call(
                                "polars.when",
                                json!({ "constraints": named(vec![("name", text("a"))]) }),
                            ),
                            vec![
                                call("When.then", json!({ "statement": float(1.5) })),
                                call("Then.otherwise", json!({ "statement": float(0.0) })),
                            ],
                        ),
                    ),
                ],
            ),
        ),
        one(
            "inputs",
            f,
            with_columns(
                "s",
                vec![
                    (
                        "minus_ratio",
                        on("score", vec![cmp("Expr.sub", input("ratio"))]),
                    ),
                    ("after", on("day", vec![cmp("Expr.gt", input("since"))])),
                    (
                        "closed_days",
                        input_then("closed", vec![method("Expr.len", none())]),
                    ),
                    ("the_limit", input("limit")),
                ],
            ),
        ),
        one(
            "pivot_aggregates_an_element",
            f,
            step(
                "s",
                "LazyFrame.pivot",
                json!({
                    "on": columns(&["group"]),
                    "on_columns": { "table": "groups" },
                    "index": columns(&["id"]),
                    "values": columns(&["score"]),
                    "aggregate_function": chain(
                        call("polars.element", json!({})),
                        vec![method("Expr.sum", none())],
                    ),
                }),
            ),
        ),
        one(
            "columns_by_several_names",
            f,
            step(
                "s",
                "LazyFrame.select",
                json!({ "exprs": many(vec![chain(
                    call("polars.col", json!({ "name": columns(&["id", "name"]) })),
                    vec![],
                )]) }),
            ),
        ),
    ]
}
