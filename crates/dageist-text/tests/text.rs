//! A document written as text and read back, a step read from text run beside the same step made
//! by the step editor's edits, and each kind of refusal with its span and reason.
//!
//! The expected texts are written by hand from the form the crate documents; the expected
//! documents are made by the engine's edits, as the step editor makes them, not by the reader.

use dageist_engine::allowed::Literal;
use dageist_engine::catalog::{Reason, StepFrame};
use dageist_engine::document::{
    Cause, Document, Expression, Inputs, Outcome, Place, Segment, Start, StepId, Value,
};
use dageist_text::{Problem, read, write};
use polars::prelude::{DataFrame, NamedFrom, Series};

const COLUMNS: [&str; 3] = ["ordered_at", "customer_id", "amount"];

fn frame_columns() -> Vec<String> {
    COLUMNS.map(str::to_owned).to_vec()
}

fn added(document: &Document, at: usize, entry: &str) -> Document {
    let columns = frame_columns();
    document
        .add_step(
            at,
            entry,
            &StepFrame {
                columns: &columns,
                tables: &[],
            },
        )
        .expect("the step is added")
}

/// `lf.with_columns(previous_order=pl.col("ordered_at").shift(1).over(pl.col("customer_id")))`,
/// made as the step editor makes it: the step added, its new column named, its expression
/// started from a column, and each method inserted and given its argument.
fn previous_order() -> Document {
    let document = added(
        &Document::new(Vec::new()).expect("no steps"),
        0,
        "LazyFrame.with_columns",
    );
    let step = Place::of_step(0, Some(document.steps()[0].id().clone()))
        .then(Segment::Parameter("named_exprs".to_owned()));
    let expression = step.clone().then(Segment::Value(0));
    let document = document
        .set_argument(
            &step.clone().then(Segment::Key(0)),
            Value::Columns(vec!["previous_order".to_owned()]),
        )
        .expect("the column is named")
        .set_start(&expression, Start::field("ordered_at").expect("a column"))
        .expect("the expression starts from a column")
        .insert_link(&expression, 0, "Expr.shift", None)
        .expect("shift is inserted")
        .insert_link(&expression, 1, "Expr.over", None)
        .expect("over is inserted");
    document
        .set_argument(
            &expression
                .clone()
                .then(Segment::Link(0))
                .then(Segment::Parameter("n".to_owned())),
            Value::Expression(Expression::literal(&Literal::Integer(1)).expect("a literal")),
        )
        .expect("shift is given its count")
        .set_argument(
            &expression
                .then(Segment::Link(1))
                .then(Segment::Parameter("partition_by".to_owned())),
            Value::Many(vec![Value::Expression(
                Expression::field("customer_id").expect("a column"),
            )]),
        )
        .expect("over is given its column")
}

const PREVIOUS_ORDER: &str = "lf.with_columns(
    previous_order=pl.col(\"ordered_at\").shift(1).over(pl.col(\"customer_id\")),
)
";

#[test]
fn a_document_the_editor_made_writes_as_the_polars_chain_it_is() {
    assert_eq!(write(&previous_order()).expect("written"), PREVIOUS_ORDER);
}

#[test]
fn the_polars_chain_reads_as_the_document_the_editor_made() {
    assert_eq!(read(PREVIOUS_ORDER).expect("read"), previous_order());
    let written_otherwise = "(lf.with_columns(\n  previous_order = pl.col('ordered_at')\n    .shift(n=1)\n    .over([pl.col(\"customer_id\")]),\n))";
    assert_eq!(read(written_otherwise).expect("read"), previous_order());
}

fn table() -> DataFrame {
    DataFrame::new(
        4,
        vec![
            Series::new("ordered_at".into(), [1_i64, 2, 3, 4]).into(),
            Series::new("customer_id".into(), ["a", "b", "a", "b"]).into(),
            Series::new("amount".into(), [10.0_f64, 20.0, 30.0, 40.0]).into(),
        ],
    )
    .expect("a table")
}

#[test]
fn a_step_read_from_text_runs_and_reports_as_the_same_step_made_in_the_editor() {
    let made = previous_order();
    let text = "lf.with_columns(previous_order=pl.col(\"ordered_at\").shift(1).over(pl.col(\"customer_id\")))";
    let read = read(text).expect("read");
    let inputs = Inputs::new();
    let from_text = read.run_on(&inputs, &table()).expect("runs");
    let from_editor = made.run_on(&inputs, &table()).expect("runs");
    assert!(
        matches!(from_text.steps()[0].outcome(), Outcome::Valid { .. }),
        "{from_text:?}"
    );
    assert_eq!(format!("{from_text:?}"), format!("{from_editor:?}"));
    let id = StepId::new("with_columns").expect("a name");
    let rows_from_text = read.rows(&inputs, &table(), &id, 10).expect("rows");
    let rows_from_editor = made.rows(&inputs, &table(), &id, 10).expect("rows");
    assert_eq!(rows_from_text, rows_from_editor);
    let previous_order = rows_from_text
        .column("previous_order")
        .expect("the column")
        .i64()
        .expect("whole numbers");
    let previous: Vec<Option<i64>> = (0..4).map(|row| previous_order.get(row)).collect();
    assert_eq!(previous, [None, None, Some(1), Some(2)]);
}

#[test]
fn a_step_named_as_the_editor_names_it_is_written_without_its_name() {
    let document = added(
        &added(&previous_order(), 1, "LazyFrame.sort"),
        2,
        "LazyFrame.sort",
    );
    let names: Vec<&str> = document
        .steps()
        .iter()
        .map(|step| step.id().as_str())
        .collect();
    assert_eq!(names, ["with_columns", "sort", "sort_2"]);
    let text = write(&document).expect("written");
    assert!(!text.contains("# step:"), "{text}");
    assert_eq!(read(&text).expect("read"), document);
}

#[test]
fn a_step_named_otherwise_keeps_its_name_through_a_comment() {
    // The second sort is added first, so it holds the name `sort` and stands after `sort_2`.
    let document = added(
        &added(&previous_order(), 1, "LazyFrame.sort"),
        1,
        "LazyFrame.sort",
    );
    let names: Vec<&str> = document
        .steps()
        .iter()
        .map(|step| step.id().as_str())
        .collect();
    assert_eq!(names, ["with_columns", "sort_2", "sort"]);
    let text = write(&document).expect("written");
    assert_eq!(
        text,
        "(\n    lf\n    .with_columns(\n        previous_order=pl.col(\"ordered_at\").shift(1).over(pl.col(\"customer_id\")),\n    )\n    # step: sort_2\n    .sort(...)\n    # step: sort\n    .sort(...)\n)\n"
    );
    assert_eq!(read(&text).expect("read"), document);
}

#[test]
fn a_name_that_is_no_bare_word_is_written_as_a_string() {
    let named = "(\n    lf\n    # step: \"latest order\"\n    .sort(pl.col(\"ordered_at\"))\n)\n";
    let document = read(named).expect("read");
    assert_eq!(document.steps()[0].id().as_str(), "latest order");
    assert_eq!(write(&document).expect("written"), named);
}

#[test]
fn an_input_named_as_the_text_names_something_else_is_not_written() {
    let document = read("lf.filter(limit)").expect("read");
    assert!(matches!(
        document.steps()[0].call().argument("predicates"),
        Some(Value::Many(items)) if matches!(&items[..], [Value::Expression(expression)] if expression.start() == &Start::Input("limit".to_owned()))
    ));
    for name in ["pl", "lf", "dg", "None", "class", "two words"] {
        let document = Document::new(vec![
            read("lf.filter(...)").expect("read").steps()[0].clone(),
        ])
        .expect("a document")
        .set_start(
            &Place::of_step(0, Some(StepId::new("filter").expect("a name")))
                .then(Segment::Parameter("predicates".to_owned()))
                .then(Segment::Item(0)),
            Start::Input(name.to_owned()),
        )
        .expect("the input is set");
        let error = write(&document).expect_err("not written");
        assert_eq!(error.name(), name);
    }
}

/// Reads `text`, which is refused, and checks the refusal's span, as the line and the columns it
/// starts and ends at, its problem, and words of its reason.
fn refused(text: &str, at: (usize, usize, usize), problem: fn(&Problem) -> bool, words: &str) {
    let error = read(text).expect_err("the text is refused");
    let span = error.span();
    assert_eq!(
        (
            span.start().line(),
            span.start().column(),
            span.end().column()
        ),
        at,
        "{text}: {error}"
    );
    assert_eq!(span.start().line(), span.end().line(), "{error}");
    assert!(problem(error.problem()), "{text}: {:?}", error.problem());
    let reason = error.to_string();
    assert!(reason.contains(words), "{text}: {reason}");
}

#[test]
fn a_name_the_catalogue_lacks_is_refused_at_the_name() {
    refused(
        "lf.sorted(pl.col(\"a\"))",
        (1, 4, 10),
        |problem| matches!(problem, Problem::UnknownName(name) if name == "lf.sorted"),
        "the catalogue has no `lf.sorted`",
    );
    refused(
        "lf.filter(pl.col(\"a\").str.lenght())",
        (1, 23, 33),
        |problem| matches!(problem, Problem::UnknownName(name) if name == ".str.lenght"),
        "no `.str.lenght`",
    );
}

#[test]
fn a_form_the_catalogue_leaves_out_is_refused_with_its_recorded_reason() {
    refused(
        "lf.with_columns(pl.col(\"a\").map_elements(1))",
        (1, 29, 41),
        |problem| {
            matches!(
                problem,
                Problem::LeftOut {
                    id: "Expr.map_elements",
                    reason: Reason::TakesPythonFunction
                }
            )
        },
        "it takes a Python function",
    );
    refused(
        "lf.with_columns(pl.col(\"a\").cut([1]))",
        (1, 29, 32),
        |problem| {
            matches!(
                problem,
                Problem::LeftOut {
                    id: "Expr.cut",
                    reason: Reason::Deprecated
                }
            )
        },
        "Polars deprecates it",
    );
    refused(
        "lf.with_columns(pl.col(\"a\").__add__(1))",
        (1, 29, 36),
        |problem| {
            matches!(
                problem,
                Problem::LeftOut {
                    id: "Expr.__add__",
                    reason: Reason::NeverOffered
                }
            )
        },
        "Python's syntax for one",
    );
}

#[test]
fn a_call_that_cannot_stand_where_it_is_is_refused_with_the_catalogues_reason() {
    refused(
        "lf.col(\"a\")",
        (1, 4, 7),
        |problem| matches!(problem, Problem::Refused(refusal) if matches!(refusal.cause(), Cause::WrongPlace { entry: "polars.col", .. })),
        "polars.col",
    );
    refused(
        "lf.filter(pl.col(\"a\").col(\"b\"))",
        (1, 23, 26),
        |problem| matches!(problem, Problem::Refused(refusal) if matches!(refusal.cause(), Cause::WrongPlace { entry: "polars.col", .. })),
        "polars.col",
    );
    refused(
        "lf.filter(pl.when(pl.col(\"a\")).otherwise(1))",
        (1, 32, 41),
        |problem| matches!(problem, Problem::Refused(refusal) if matches!(refusal.cause(), Cause::OutOfOrder { .. })),
        "then",
    );
}

#[test]
fn an_argument_the_catalogue_refuses_is_refused_at_the_argument() {
    refused(
        "lf.head(rows=5)",
        (1, 9, 13),
        |problem| matches!(problem, Problem::Refused(_)),
        "there is no parameter rows",
    );
    refused(
        "lf.head(\"five\")",
        (1, 9, 15),
        |problem| matches!(problem, Problem::Refused(_)),
        "n takes a whole number, not text",
    );
    refused(
        "lf.with_columns(pl.col(\"a\").rank(method=\"middle\"))",
        (1, 41, 49),
        |problem| matches!(problem, Problem::Refused(_)),
        "method has no choice \"middle\"",
    );
    refused(
        "lf.sort(pl.col(\"a\"), descending=[True, 2])",
        (1, 40, 41),
        |problem| matches!(problem, Problem::Refused(_)),
        "descending takes a flag",
    );
    refused(
        "lf.head(1, 2)",
        (1, 12, 13),
        |problem| {
            matches!(
                problem,
                Problem::TooManyArguments {
                    entry: "LazyFrame.head",
                    takes: 1
                }
            )
        },
        "takes 1 argument by position",
    );
    refused(
        "lf.head(99999999999999999999)",
        (1, 9, 29),
        |problem| matches!(problem, Problem::NumberOutOfRange(_)),
        "64 bits",
    );
    refused(
        "lf.with_columns(pl.col(\"a\").cast(pl.Decimal))",
        (1, 34, 44),
        |problem| matches!(problem, Problem::Dtype(_)),
        "\"Decimal\" is not a dtype",
    );
    refused(
        "lf.head(*[1])",
        (1, 9, 13),
        |problem| matches!(problem, Problem::Spread),
        "starred parameter",
    );
}

#[test]
fn what_is_no_expression_where_one_goes_is_refused_there() {
    refused(
        "lf.filter(\"a\")",
        (1, 11, 14),
        |problem| matches!(problem, Problem::TextForAnExpression),
        "pl.col(...)",
    );
    refused(
        "lf.filter(pl)",
        (1, 11, 13),
        |problem| matches!(problem, Problem::ReservedName(name) if name == "pl"),
        "cannot name an input",
    );
    refused(
        "lf.filter((1).abs())",
        (1, 11, 14),
        |problem| matches!(problem, Problem::MethodOfALiteral),
        "pl.lit(...)",
    );
    refused(
        "lf.filter([pl.col(\"a\")])",
        (1, 11, 24),
        |problem| {
            matches!(
                problem,
                Problem::Form {
                    expects: "an expression",
                    found: "a list"
                }
            )
        },
        "an expression goes here, not a list",
    );
}

#[test]
fn a_text_that_is_no_chain_of_steps_on_lf_is_refused() {
    refused(
        "df.head()",
        (1, 1, 3),
        |problem| matches!(problem, Problem::NotOnTheFrame),
        "start from `lf`",
    );
    refused(
        "lf.head",
        (1, 4, 8),
        |problem| matches!(problem, Problem::NotCalled),
        "called",
    );
    refused(
        "lf.filter(lambda row: row)",
        (1, 11, 17),
        |problem| matches!(problem, Problem::Syntax(dageist_syntax::SyntaxProblem::Keyword(word)) if word == "lambda"),
        "a lambda is not read",
    );
    refused(
        "lf.filter(pl.col(\"a\") > 1)",
        (1, 23, 24),
        |problem| {
            matches!(
                problem,
                Problem::Syntax(dageist_syntax::SyntaxProblem::Operator(_))
            )
        },
        "operator `>`",
    );
}

#[test]
fn a_step_name_where_it_names_no_step_is_refused() {
    refused(
        "lf.filter(\n    # step: here\n    pl.col(\"a\"),\n)",
        (2, 5, 17),
        |problem| matches!(problem, Problem::MisplacedStepName),
        "the step whose dot",
    );
    refused(
        "lf.filter(\n    pl.col(\"a\")\n    # step: here\n    .is_null(),\n)",
        (3, 5, 17),
        |problem| matches!(problem, Problem::MisplacedStepName),
        "the step whose dot",
    );
    refused(
        "(\n    lf\n    # step: one\n    # step: two\n    .head()\n)",
        (4, 5, 16),
        |problem| matches!(problem, Problem::TwoStepNames),
        "named twice",
    );
    refused(
        "(\n    lf\n    # step:\n    .head()\n)",
        (3, 5, 12),
        |problem| matches!(problem, Problem::EmptyStepName),
        "names no step",
    );
    refused(
        "(\n    lf\n    # step: first\n    .head()\n    # step: first\n    .tail()\n)",
        (5, 5, 18),
        |problem| matches!(problem, Problem::Refused(refusal) if matches!(refusal.cause(), Cause::DuplicateStep(_))),
        "first",
    );
}

#[test]
fn a_call_with_no_name_before_it_is_refused_at_its_brackets() {
    refused(
        "lf.sort(pl.col(\"a\"))()",
        (1, 21, 23),
        |problem| matches!(problem, Problem::CallWithoutName),
        "a call names what it calls",
    );
}

#[test]
fn none_where_an_expression_goes_is_refused_and_pl_lit_of_none_is_written_out() {
    refused(
        "lf.with_columns(pl.col(\"a\").fill_null(None))",
        (1, 39, 43),
        |problem| matches!(problem, Problem::NoneForAnExpression),
        "pl.lit(None)",
    );
    let text = "lf.with_columns(pl.col(\"a\").fill_null(pl.lit(None)))\n";
    assert_eq!(write(&read(text).expect("read")).expect("written"), text);
}

/// `with_columns` given its pairs, `{key: pl.col("a")}`, before its expressions, `[pl.col("b")]`,
/// as a document's JSON may hold them.
fn pairs_before_expressions(key: &str) -> Document {
    let column = |name: &str| {
        format!(
            r#"{{"expression":{{"start":{{"entry":"polars.col","arguments":{{"name":{{"columns":["{name}"]}}}}}},"then":[]}}}}"#
        )
    };
    let json = format!(
        r#"{{"version":1,"steps":[{{"id":"with_columns","call":{{"entry":"LazyFrame.with_columns","arguments":{{"named_exprs":{{"pairs":[[{{"columns":["{key}"]}},{a}]]}},"exprs":{{"many":[{b}]}}}}}}}}]}}"#,
        a = column("a"),
        b = column("b"),
    );
    Document::read(&json).expect("the document reads")
}

#[test]
fn expressions_after_pairs_by_name_keep_their_place() {
    let document = pairs_before_expressions("total");
    let text = write(&document).expect("written");
    assert_eq!(
        text,
        "lf.with_columns(total=pl.col(\"a\"), *[pl.col(\"b\")])\n"
    );
    assert_eq!(read(&text).expect("read"), document);
}

#[test]
fn expressions_after_pairs_written_as_a_dict_are_written_before_them() {
    let document = pairs_before_expressions("grand total");
    let text = write(&document).expect("written");
    assert_eq!(
        text,
        "lf.with_columns(*[pl.col(\"b\")], **{\"grand total\": pl.col(\"a\")})\n"
    );
    let read = read(&text).expect("read");
    let order: Vec<&str> = read.steps()[0]
        .call()
        .arguments()
        .iter()
        .map(|given| given.parameter())
        .collect();
    assert_eq!(order, ["exprs", "named_exprs"]);
    assert_eq!(
        read.steps()[0].call().argument("named_exprs"),
        document.steps()[0].call().argument("named_exprs")
    );
    assert_eq!(
        read.steps()[0].call().argument("exprs"),
        document.steps()[0].call().argument("exprs")
    );
}

#[test]
fn pairs_whose_names_repeat_are_written_as_a_dict() {
    let json = r#"{"version":1,"steps":[{"id":"with_columns","call":{"entry":"LazyFrame.with_columns","arguments":{"named_exprs":{"pairs":[[{"columns":["x"]},{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["a"]}}},"then":[]}}],[{"columns":["x"]},{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["b"]}}},"then":[]}}]]}}}}]}"#;
    let document = Document::read(json).expect("the document reads");
    let text = write(&document).expect("written");
    assert_eq!(
        text,
        "lf.with_columns(**{\"x\": pl.col(\"a\"), \"x\": pl.col(\"b\")})\n"
    );
    assert_eq!(read(&text).expect("read"), document);
}
