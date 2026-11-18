//! The Python Polars source of a document. Every expected text is worked out by
//! hand from how Python writes its values and calls its methods, and from the
//! signatures in `api/polars/python.txt`, not read from what is written.

use dageist_engine::claim_timing::ClaimTiming;
use dageist_engine::document::{Document, Inputs, ListOrigin, NotRunnableReason};
use polars::prelude::*;
use serde_json::{Value, json};

use crate::support::{call, chain, document, field, field_then, many, named, step, text, whole};

fn read(steps: Vec<Value>) -> Document {
    Document::read(&document(steps)).expect("a document")
}

/// The text of the single step of a document of `steps`, with no inputs connected.
fn step_source(step_json: Value) -> String {
    read(vec![step_json]).steps()[0].source(&Inputs::new())
}

fn float(number: f64) -> Value {
    chain(
        call("polars.lit", json!({ "value": { "float": number } })),
        vec![],
    )
}

fn named_columns(columns: Vec<(&str, Value)>) -> String {
    step_source(step(
        "s",
        "LazyFrame.with_columns",
        json!({ "named_exprs": named(columns) }),
    ))
}

#[test]
fn floats_are_written_as_python_writes_them() {
    // Python's repr: a whole float keeps its `.0`, the shortest digits that read back are used,
    // and an exponent only appears below 1e-4 and from 1e16, with a sign and two digits.
    let written = named_columns(vec![
        ("a", float(1.0)),
        ("b", float(0.1)),
        ("c", float(1e16)),
        ("d", float(123_456_789_012_345.0)),
        ("e", float(1e-5)),
        ("f", float(0.0001)),
        ("g", float(-2.5e-7)),
        ("h", float(1.5e300)),
        ("i", float(-0.0)),
        ("j", float(100.0)),
    ]);
    assert_eq!(
        written,
        concat!(
            ".with_columns(pl.lit(1.0).alias(\"a\"), pl.lit(0.1).alias(\"b\"), ",
            "pl.lit(1e+16).alias(\"c\"), pl.lit(123456789012345.0).alias(\"d\"), ",
            "pl.lit(1e-05).alias(\"e\"), pl.lit(0.0001).alias(\"f\"), ",
            "pl.lit(-2.5e-07).alias(\"g\"), pl.lit(1.5e+300).alias(\"h\"), ",
            "pl.lit(-0.0).alias(\"i\"), pl.lit(100.0).alias(\"j\"))"
        )
    );
}

#[test]
fn text_is_quoted_and_whole_numbers_and_flags_are_written_as_they_are() {
    let flag = |value: Value| {
        chain(
            call("polars.lit", json!({ "value": { "flag": value } })),
            vec![],
        )
    };
    let written = named_columns(vec![
        ("a", text("say \"hi\"\n\\ é\t\u{1}")),
        ("b", whole(-3)),
        ("c", flag(json!(true))),
        ("d", flag(json!(false))),
        ("e", flag(Value::Null)),
    ]);
    assert_eq!(
        written,
        concat!(
            ".with_columns(pl.lit(\"say \\\"hi\\\"\\n\\\\ é\\t\\x01\").alias(\"a\"), ",
            "pl.lit(-3).alias(\"b\"), pl.lit(True).alias(\"c\"), pl.lit(False).alias(\"d\"), ",
            "pl.lit(None).alias(\"e\"))"
        )
    );
}

#[test]
fn a_dtype_is_written_as_the_attribute_of_pl_with_its_parameters_by_name() {
    let typed = |dtype: &str| {
        chain(
            call(
                "polars.lit",
                json!({ "value": { "integer": 3 }, "dtype": { "dtype": dtype } }),
            ),
            vec![],
        )
    };
    let written = named_columns(vec![
        ("a", typed("Int8")),
        ("b", typed("List(Int64)")),
        ("c", typed("Datetime(time_unit='us', time_zone=None)")),
        (
            "d",
            typed("Datetime(time_unit='ms', time_zone='Europe/Berlin')"),
        ),
        ("e", typed("List(List(Duration(time_unit='ns')))")),
    ]);
    assert_eq!(
        written,
        concat!(
            ".with_columns(pl.lit(3, dtype=pl.Int8).alias(\"a\"), ",
            "pl.lit(3, dtype=pl.List(pl.Int64)).alias(\"b\"), ",
            "pl.lit(3, dtype=pl.Datetime(time_unit='us', time_zone=None)).alias(\"c\"), ",
            "pl.lit(3, dtype=pl.Datetime(time_unit='ms', time_zone='Europe/Berlin')).alias(\"d\"), ",
            "pl.lit(3, dtype=pl.List(pl.List(pl.Duration(time_unit='ns')))).alias(\"e\"))"
        )
    );
}

#[test]
fn a_sort_names_its_fields_and_takes_its_options_by_name_in_pythons_order() {
    // sort(by, *more_by, descending=False, nulls_last=False, maintain_order=False, ...): the
    // keys are by position, and the options are by name, in the order of the signature though
    // the document gave them in another.
    let sort = step(
        "s",
        "LazyFrame.sort",
        json!({
            "maintain_order": { "flag": true },
            "by": many(vec![field("a"), field_then("b", vec![call("Expr.is_null", json!({}))])]),
            "nulls_last": { "flag": true },
            "descending": many(vec![json!({ "flag": true }), json!({ "flag": false })]),
        }),
    );
    assert_eq!(
        step_source(sort),
        ".sort(\"a\", pl.col(\"b\").is_null(), descending=[True, False], nulls_last=True, maintain_order=True)"
    );
}

#[test]
fn what_a_list_of_keys_holds_is_written_as_its_items_by_position() {
    // `by` stands just before `*more_by`, so Python reads the list as its items: more_by keeps
    // going where by stopped.
    let sort = step(
        "s",
        "LazyFrame.sort",
        json!({
            "by": many(vec![field("a")]),
            "more_by": many(vec![field("b"), field("c")]),
        }),
    );
    assert_eq!(step_source(sort), ".sort(\"a\", \"b\", \"c\")");
}

#[test]
fn a_parameter_before_a_starred_one_that_is_given_items_is_written_with_its_default() {
    // over(partition_by=None, *more_exprs, ...): more_exprs are by position only.
    let over = step(
        "s",
        "LazyFrame.with_columns",
        json!({ "named_exprs": named(vec![(
            "x",
            field_then("a", vec![call("Expr.over", json!({ "more_exprs": many(vec![field("g")]) }))]),
        )]) }),
    );
    assert_eq!(
        step_source(over),
        ".with_columns(pl.col(\"a\").over(None, pl.col(\"g\")).alias(\"x\"))"
    );
}

#[test]
fn select_and_with_columns_name_what_they_write_with_alias() {
    let select = step(
        "s",
        "LazyFrame.select",
        json!({
            "exprs": many(vec![field("a"), field_then("b", vec![call("Expr.sum", json!({}))])]),
            "named_exprs": named(vec![("total", field_then("c", vec![call("Expr.sum", json!({}))]))]),
        }),
    );
    assert_eq!(
        step_source(select),
        ".select(\"a\", pl.col(\"b\").sum(), pl.col(\"c\").sum().alias(\"total\"))"
    );
}

#[test]
fn a_filter_holds_its_predicates_and_its_constraints_as_a_dictionary() {
    let filter = step(
        "s",
        "LazyFrame.filter",
        json!({
            "predicates": many(vec![
                field_then("a", vec![call("Expr.gt", json!({ "other": whole(1) }))]),
                field_then("b", vec![call("Expr.is_not_null", json!({}))]),
            ]),
            "constraints": named(vec![("c", text("x")), ("d", whole(2))]),
        }),
    );
    assert_eq!(
        step_source(filter),
        ".filter(pl.col(\"a\").gt(pl.lit(1)), pl.col(\"b\").is_not_null(), **{\"c\": pl.lit(\"x\"), \"d\": pl.lit(2)})"
    );
}

#[test]
fn a_rename_is_a_dictionary_and_a_join_names_its_other_table() {
    let rename = step(
        "s",
        "LazyFrame.rename",
        json!({
            "mapping": { "pairs": [[{ "columns": ["a"] }, { "string": "b" }]] },
            "strict": { "flag": false },
        }),
    );
    assert_eq!(step_source(rename), ".rename({\"a\": \"b\"}, strict=False)");
    let join = step(
        "s",
        "LazyFrame.join",
        json!({
            "other": { "table": "lookup" },
            "how": { "choice": "left" },
            "left_on": many(vec![field("a"), field("b")]),
            "right_on": many(vec![field("c")]),
            "suffix": { "string": "_x" },
        }),
    );
    // A list of one key is the key; a list of two is a list.
    assert_eq!(
        step_source(join),
        ".join(lookup, how=\"left\", left_on=[\"a\", \"b\"], right_on=\"c\", suffix=\"_x\")"
    );
}

#[test]
fn a_condition_is_written_as_the_calls_that_build_it() {
    let when = chain(
        call(
            "polars.when",
            json!({ "predicates": many(vec![field_then("a", vec![call("Expr.lt", json!({ "other": whole(0) }))])]) }),
        ),
        vec![
            call("When.then", json!({ "statement": text("neg") })),
            call(
                "Then.when",
                json!({ "constraints": named(vec![("b", whole(1))]) }),
            ),
            call("ChainedWhen.then", json!({ "statement": text("one") })),
            call(
                "ChainedThen.otherwise",
                json!({ "statement": text("other") }),
            ),
        ],
    );
    assert_eq!(
        named_columns(vec![("c", when)]),
        concat!(
            ".with_columns(pl.when(pl.col(\"a\").lt(pl.lit(0))).then(pl.lit(\"neg\"))",
            ".when(**{\"b\": pl.lit(1)}).then(pl.lit(\"one\")).otherwise(pl.lit(\"other\")).alias(\"c\"))"
        )
    );
}

#[test]
fn a_hole_is_written_as_none_and_the_module_says_it_does_not_run() {
    let hole = json!({ "expression": { "start": { "hole": null }, "then": [] } });
    let document = read(vec![step(
        "s",
        "LazyFrame.with_columns",
        json!({ "named_exprs": named(vec![("x", hole)]) }),
    )]);
    let source = document.source("data", &Inputs::new());
    let lines: Vec<&str> = source.lines().iter().map(|line| line.text()).collect();
    assert_eq!(
        lines.first().copied(),
        Some("# Not runnable as written: step \"s\", something in it is still to be picked.")
    );
    assert!(lines.contains(&"        .with_columns(None.alias(\"x\"))"));
    assert_eq!(source.not_runnable().len(), 1);
    assert_eq!(
        source.not_runnable()[0].reason(),
        &NotRunnableReason::Unfinished
    );
    assert_eq!(
        source.not_runnable()[0].step().map(|id| id.as_str()),
        Some("s")
    );
}

fn lookup_inputs() -> Inputs {
    let mut inputs = Inputs::new();
    inputs
        .add_table(
            "look up",
            "Node",
            df!("a" => [1_i64]).expect("a table").lazy(),
        )
        .expect("a table");
    inputs
        .add_value("class", Scalar::new(DataType::Int64, AnyValue::Int64(1)))
        .expect("a value");
    inputs
        .add_list(
            "lambda",
            ListOrigin::Parameter,
            Series::new("v".into(), [1.5_f64]),
        )
        .expect("a list");
    inputs
}

#[test]
fn an_input_is_a_parameter_of_run_named_as_a_python_name_and_typed_by_its_dtype() {
    let document = read(vec![
        step(
            "join",
            "LazyFrame.join",
            json!({ "other": { "table": "look up" }, "on": many(vec![field("a")]) }),
        ),
        step(
            "values",
            "LazyFrame.with_columns",
            json!({ "named_exprs": named(vec![
                ("one", chain(json!({ "input": "class" }), vec![])),
                ("many", chain(json!({ "input": "lambda" }), vec![call("Expr.len", json!({}))])),
            ]) }),
        ),
    ]);
    let source = document.source("My frame", &lookup_inputs());
    assert!(source.not_runnable().is_empty());
    assert_eq!(
        source.text(),
        concat!(
            "import polars as pl\n",
            "\n",
            "\n",
            "def run(\n",
            "    My_frame: pl.LazyFrame,\n",
            "    look_up: pl.LazyFrame,\n",
            "    class_: int,\n",
            "    lambda_: pl.Series,\n",
            ") -> pl.LazyFrame:\n",
            "    return (\n",
            "        My_frame\n",
            "        .join(look_up, on=\"a\")\n",
            "        .with_columns(pl.lit(class_, dtype=pl.Int64).alias(\"one\"), pl.lit(lambda_).len().alias(\"many\"))\n",
            "    )\n",
        )
    );
    // The line of a step says which step it is.
    let joined = source
        .lines()
        .iter()
        .find(|line| line.text().contains(".join("))
        .expect("the join's line");
    assert_eq!(joined.step().map(|id| id.as_str()), Some("join"));
}

#[test]
fn an_input_that_is_not_connected_is_marked_and_two_names_that_are_one_are_marked() {
    let document = read(vec![step(
        "s",
        "LazyFrame.with_columns",
        json!({ "named_exprs": named(vec![
            ("a", chain(json!({ "input": "missing" }), vec![])),
            ("b", chain(json!({ "input": "look up" }), vec![])),
        ]) }),
    )]);
    let mut inputs = Inputs::new();
    for name in ["look up", "look_up"] {
        inputs
            .add_table(name, "Node", df!("a" => [1_i64]).expect("a table").lazy())
            .expect("a table");
    }
    let source = document.source("data", &inputs);
    let reasons: Vec<&NotRunnableReason> = source
        .not_runnable()
        .iter()
        .map(|problem| problem.reason())
        .collect();
    // `look up` is a table, which a value's place does not take.
    assert_eq!(
        reasons,
        [
            &NotRunnableReason::NotConnected("missing".to_owned()),
            &NotRunnableReason::NotConnected("look up".to_owned()),
        ]
    );
    let clash = read(vec![step(
        "s",
        "LazyFrame.with_columns",
        json!({ "named_exprs": named(vec![
            ("a", chain(json!({ "input": "look up" }), vec![])),
            ("b", chain(json!({ "input": "look_up" }), vec![])),
        ]) }),
    )]);
    let source = clash.source("data", &inputs);
    assert!(source.not_runnable().iter().any(|problem| {
        problem.reason() == &NotRunnableReason::SameName("look up".to_owned(), "look_up".to_owned())
    }));
}

#[test]
fn a_document_with_no_steps_returns_its_frame() {
    let source = read(vec![]).source("claims", &Inputs::new());
    assert_eq!(
        source.text(),
        "import polars as pl\n\n\ndef run(\n    claims: pl.LazyFrame,\n) -> pl.LazyFrame:\n    return claims\n"
    );
}

#[test]
fn a_connected_date_is_imported_and_a_text_input_is_a_string() {
    let mut inputs = Inputs::new();
    inputs
        .add_value("day", Scalar::new(DataType::Date, AnyValue::Date(20_514)))
        .expect("a value");
    inputs
        .add_value(
            "label",
            Scalar::new(DataType::String, AnyValue::StringOwned("x".into())),
        )
        .expect("a value");
    let document = read(vec![step(
        "s",
        "LazyFrame.with_columns",
        json!({ "named_exprs": named(vec![
            ("a", chain(json!({ "input": "day" }), vec![])),
            ("b", chain(json!({ "input": "label" }), vec![])),
        ]) }),
    )]);
    let text = document.source("data", &inputs).text();
    assert!(
        text.starts_with("from datetime import date\n\nimport polars as pl\n"),
        "{text}"
    );
    assert!(text.contains("    day: date,\n    label: str,\n"), "{text}");
    assert!(
        text.contains("pl.lit(day, dtype=pl.Date).alias(\"a\")"),
        "{text}"
    );
    assert!(
        text.contains("pl.lit(label, dtype=pl.String).alias(\"b\")"),
        "{text}"
    );
}

#[test]
fn the_claim_timing_sample_is_written_as_this_module() {
    let sample = ClaimTiming::new().expect("the sample node");
    let source = sample
        .steps()
        .source(ClaimTiming::SOURCE_NAME, sample.inputs());
    assert!(source.not_runnable().is_empty());
    // By hand from the sample's steps: `sort` by two fields, columns added with `with_columns`
    // and `alias`, a left join of `step_sla`, conditions as `when`/`then`/`otherwise`, the
    // connected date, cutoff and list as literals of their dtypes, and a pivot that takes the
    // values of its new columns from a collected table. Numbers typed without a dtype are
    // `pl.lit` of the number, and comparisons are the methods `eq`, `le` and `gt`.
    assert_eq!(
        source.text(),
        concat!(
            "from datetime import date\n",
            "\n",
            "import polars as pl\n",
            "\n",
            "\n",
            "def run(\n",
            "    claims: pl.LazyFrame,\n",
            "    step_sla: pl.LazyFrame,\n",
            "    as_of: date,\n",
            "    sla_cutoff: int,\n",
            "    holidays: pl.Series,\n",
            "    step_names: pl.LazyFrame,\n",
            ") -> pl.LazyFrame:\n",
            "    return (\n",
            "        claims\n",
            "        .sort(\"dcn\", \"timestamp\")\n",
            "        .with_columns(pl.col(\"dcn\").cum_count().over(pl.col(\"dcn\")).alias(\"step_index\"))\n",
            "        .with_columns(pl.col(\"timestamp\").shift(pl.lit(1)).over(pl.col(\"dcn\")).alias(\"resolved_at\"))\n",
            "        .with_columns(pl.col(\"timestamp\").alias(\"received_at\"))\n",
            "        .with_columns(pl.col(\"resolved_at\").dt.date().alias(\"resolved_day\"))\n",
            "        .with_columns(pl.col(\"received_at\").sub(pl.col(\"resolved_at\")).dt.total_days().alias(\"days_open\"))\n",
            "        .with_columns(pl.when(pl.col(\"owner\").eq(pl.lit(\"Company A\"))).then(pl.col(\"row_days\")).otherwise(pl.lit(0)).cum_sum().over(pl.col(\"dcn\")).alias(\"a_days\"))\n",
            "        .join(step_sla, how=\"left\", left_on=\"step_to\", right_on=\"step\", maintain_order=\"left\")\n",
            "        .with_columns(pl.when(pl.col(\"row_days\").le(pl.col(\"sla_days\"))).then(pl.lit(\"within\")).otherwise(pl.lit(\"late\")).alias(\"sla_status\"))\n",
            "        .with_columns(pl.when(pl.col(\"received_at\").dt.date().eq(pl.lit(as_of, dtype=pl.Date))).then(pl.lit(\"holiday\")).otherwise(pl.when(pl.col(\"row_days\").gt(pl.lit(sla_cutoff, dtype=pl.Int64))).then(pl.lit(\"over\")).otherwise(pl.lit(\"ok\"))).alias(\"flag\"))\n",
            "        .with_columns(pl.lit(holidays).len().alias(\"holiday_count\"))\n",
            "        .pivot(\"step_to\", on_columns=step_names.collect(), index=\"dcn\", values=\"row_days\", aggregate_function=pl.element().first())\n",
            "    )\n",
        )
    );
}

fn one_table(name: &str) -> Inputs {
    let mut inputs = Inputs::new();
    inputs
        .add_table(name, "Node", df!("a" => [1_i64]).expect("a table").lazy())
        .expect("a table");
    inputs
}

#[test]
fn an_input_named_like_the_table_the_document_starts_from_is_marked() {
    // `run(claims, claims)` is not Python that runs.
    let document = read(vec![step(
        "join",
        "LazyFrame.join",
        json!({ "other": { "table": "claims" }, "on": many(vec![field("a")]) }),
    )]);
    let source = document.source("claims", &one_table("claims"));
    let reasons: Vec<&NotRunnableReason> = source
        .not_runnable()
        .iter()
        .map(|problem| problem.reason())
        .collect();
    assert_eq!(
        reasons,
        [&NotRunnableReason::NamedLikeFrame("claims".to_owned())]
    );
    // The name is the function's parameter once.
    assert_eq!(
        source.text().matches("    claims: ").count(),
        1,
        "{}",
        source.text()
    );
}

#[test]
fn an_input_used_as_one_kind_by_a_step_and_as_another_by_a_later_step_is_marked() {
    // The first step takes the name as a table, the second as a value.
    let document = read(vec![
        step(
            "join",
            "LazyFrame.join",
            json!({ "other": { "table": "lookup" }, "on": many(vec![field("a")]) }),
        ),
        step(
            "value",
            "LazyFrame.with_columns",
            json!({ "named_exprs": named(vec![("x", chain(json!({ "input": "lookup" }), vec![]))]) }),
        ),
    ]);
    let source = document.source("data", &one_table("lookup"));
    let marked: Vec<(Option<&str>, &NotRunnableReason)> = source
        .not_runnable()
        .iter()
        .map(|problem| (problem.step().map(|id| id.as_str()), problem.reason()))
        .collect();
    assert_eq!(
        marked,
        [(
            Some("value"),
            &NotRunnableReason::NotConnected("lookup".to_owned())
        )]
    );
    // It is one parameter, the lazy frame the first step takes.
    assert_eq!(
        source.text().matches("    lookup: pl.LazyFrame,").count(),
        1
    );
}

#[test]
fn a_connected_value_whose_dtype_cannot_be_written_is_marked_not_written_without_it() {
    // Binary is not a dtype a document or the source can name, so the value would be written
    // with no dtype, which Python types as it likes.
    let mut inputs = Inputs::new();
    inputs
        .add_value(
            "blob",
            Scalar::new(DataType::Binary, AnyValue::BinaryOwned(vec![1, 2])),
        )
        .expect("a value");
    let document = read(vec![step(
        "s",
        "LazyFrame.with_columns",
        json!({ "named_exprs": named(vec![("x", chain(json!({ "input": "blob" }), vec![]))]) }),
    )]);
    let source = document.source("data", &inputs);
    assert_eq!(
        source
            .not_runnable()
            .iter()
            .map(|problem| problem.reason())
            .collect::<Vec<_>>(),
        [&NotRunnableReason::UnwritableDtype {
            input: "blob".to_owned(),
            dtype: "binary".to_owned(),
        }]
    );
    // It is written as Python would type it, and says so in a comment.
    assert!(source.text().contains("pl.lit(blob)"), "{}", source.text());
    assert!(
        source
            .text()
            .starts_with("# Not runnable as written: step \"s\", the dtype binary")
    );
}
