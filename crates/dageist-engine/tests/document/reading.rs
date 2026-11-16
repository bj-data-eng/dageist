//! Reading is strict: each thing the reader does not accept is refused, with
//! the place of it in the document and the reason.

use dageist_engine::catalog::{Applies, BuildError, entry};
use dageist_engine::document::{
    Call, Cause, Document, DtypeError, ReadError, Refusal, Segment, SyntaxProblem, Value,
};
use serde_json::{Value as Json, json};

use crate::support::{call, chain, col_call, document, field, field_then, many, named, step, text};

fn refused(text: &str) -> Refusal {
    match Document::read(text) {
        Err(ReadError::Refused(refusal)) => refusal,
        other => panic!("not refused: {other:?} for {text}"),
    }
}

fn refused_steps(steps: Vec<Json>) -> Refusal {
    refused(&document(steps))
}

fn parameter(name: &str) -> Segment {
    Segment::Parameter(name.to_owned())
}

/// Asserts the refusal is in the step at `index` called `id`, at the way `path`
/// into it.
#[track_caller]
fn assert_place(refusal: &Refusal, index: usize, id: Option<&str>, path: &[Segment]) {
    let place = refusal.place();
    assert_eq!(place.step_index(), Some(index), "{refusal}");
    assert_eq!(place.step_id().map(|id| id.as_str()), id, "{refusal}");
    assert_eq!(place.path(), path, "{refusal}");
}

/// A step `s` that sorts by `by`.
fn sort_by(by: Json) -> Json {
    step("s", "LazyFrame.sort", json!({ "by": by }))
}

/// A step `s` that adds `expression` as a column `x`.
fn add_x(expression: Json) -> Json {
    step(
        "s",
        "LazyFrame.with_columns",
        json!({ "named_exprs": named(vec![("x", expression)]) }),
    )
}

#[test]
fn text_that_is_not_json_is_a_syntax_error() {
    assert!(matches!(Document::read("{"), Err(ReadError::Syntax(_))));
    assert!(matches!(Document::read(""), Err(ReadError::Syntax(_))));
}

#[test]
fn a_repeated_field_is_a_syntax_error_not_a_last_one_wins() {
    let error = Document::read(r#"{"version":1,"version":1,"steps":[]}"#).expect_err("repeated");
    let ReadError::Syntax(error) = error else {
        panic!("not a syntax error: {error:?}");
    };
    assert_eq!(
        error.problem(),
        &SyntaxProblem::DuplicateKey("version".to_owned())
    );
    // The place is the second `"version"`, which opens at column 14.
    assert_eq!((error.line(), error.column()), (1, 14));

    let twice = r#"{"id":"s","call":{"entry":"LazyFrame.select","arguments":{"exprs":{"many":[]},"exprs":{"many":[]}}}}"#;
    assert!(matches!(
        Document::read(&format!(r#"{{"version":1,"steps":[{twice}]}}"#)),
        Err(ReadError::Syntax(_))
    ));
}

#[test]
fn a_document_of_no_steps_is_valid() {
    let document = Document::read(r#"{"version":1,"steps":[]}"#).expect("a document");
    assert!(document.steps().is_empty());
}

#[test]
fn a_version_but_one_is_refused() {
    for (written, as_text) in [
        (r#"{"version":2,"steps":[]}"#, "2"),
        (r#"{"version":0,"steps":[]}"#, "0"),
        (r#"{"version":1.0,"steps":[]}"#, "1.0"),
        (r#"{"version":"1","steps":[]}"#, "\"1\""),
        (r#"{"version":null,"steps":[]}"#, "null"),
    ] {
        let refusal = refused(written);
        assert!(
            matches!(refusal.cause(), Cause::UnsupportedVersion(version) if version == as_text),
            "{written}: {refusal}"
        );
        assert_eq!(refusal.place().step_index(), None);
    }
}

#[test]
fn a_future_version_is_refused_before_its_other_fields_are_looked_at() {
    let refusal = refused(r#"{"version":2,"steps":{"not":"a list"},"more":true}"#);
    assert!(
        matches!(refusal.cause(), Cause::UnsupportedVersion(_)),
        "{refusal}"
    );
}

#[test]
fn the_version_and_the_steps_must_be_there() {
    let refusal = refused(r#"{"steps":[]}"#);
    assert!(
        matches!(refusal.cause(), Cause::MissingField("version")),
        "{refusal}"
    );
    let refusal = refused(r#"{"version":1}"#);
    assert!(
        matches!(refusal.cause(), Cause::MissingField("steps")),
        "{refusal}"
    );
}

#[test]
fn a_document_that_is_not_an_object_with_a_list_of_steps_is_refused() {
    for text in ["[]", "1", "\"x\""] {
        assert!(
            matches!(refused(text).cause(), Cause::Shape { .. }),
            "{text}"
        );
    }
    let refusal = refused(r#"{"version":1,"steps":{}}"#);
    assert!(
        matches!(
            refusal.cause(),
            Cause::Shape {
                what: "steps",
                expected: "an array",
                found: "an object"
            }
        ),
        "{refusal}"
    );
}

#[test]
fn an_unknown_field_is_refused_where_it_is() {
    let refusal = refused(r#"{"version":1,"steps":[],"extra":1}"#);
    assert!(matches!(refusal.cause(), Cause::UnknownField(field) if field == "extra"));
    assert_eq!(refusal.place().step_index(), None);

    let in_step = r#"{"version":1,"steps":[
        {"id":"a","call":{"entry":"LazyFrame.select","arguments":{}}},
        {"id":"b","call":{"entry":"LazyFrame.select","arguments":{}},"note":"x"}]}"#;
    let refusal = refused(in_step);
    assert!(matches!(refusal.cause(), Cause::UnknownField(field) if field == "note"));
    assert_place(&refusal, 1, Some("b"), &[]);
}

#[test]
fn an_unknown_field_of_a_call_an_expression_or_a_start_is_refused() {
    let in_call =
        json!({ "id": "s", "call": { "entry": "LazyFrame.select", "arguments": {}, "x": 1 } });
    let refusal = refused_steps(vec![in_call]);
    assert!(matches!(refusal.cause(), Cause::UnknownField(field) if field == "x"));
    assert_place(&refusal, 0, Some("s"), &[]);

    let in_expression = json!({ "expression": { "start": col_call("a"), "then": [], "x": 1 } });
    let refusal = refused_steps(vec![step(
        "s",
        "LazyFrame.select",
        json!({ "exprs": many(vec![in_expression]) }),
    )]);
    assert!(matches!(refusal.cause(), Cause::UnknownField(field) if field == "x"));
    assert_place(
        &refusal,
        0,
        Some("s"),
        &[parameter("exprs"), Segment::Item(0)],
    );

    let in_input = json!({ "expression": { "start": { "input": "a", "x": 1 }, "then": [] } });
    let refusal = refused_steps(vec![step(
        "s",
        "LazyFrame.select",
        json!({ "exprs": many(vec![in_input]) }),
    )]);
    assert!(matches!(refusal.cause(), Cause::UnknownField(field) if field == "x"));
    assert_place(
        &refusal,
        0,
        Some("s"),
        &[parameter("exprs"), Segment::Item(0), Segment::Start],
    );
}

#[test]
fn a_step_needs_an_identifier_and_a_call() {
    let refusal =
        refused(r#"{"version":1,"steps":[{"call":{"entry":"LazyFrame.select","arguments":{}}}]}"#);
    assert!(matches!(refusal.cause(), Cause::MissingField("id")));
    assert_place(&refusal, 0, None, &[]);

    let refusal = refused(r#"{"version":1,"steps":[{"id":"a"}]}"#);
    assert!(matches!(refusal.cause(), Cause::MissingField("call")));
    assert_place(&refusal, 0, Some("a"), &[]);

    let refusal = refused(
        r#"{"version":1,"steps":[{"id":"","call":{"entry":"LazyFrame.select","arguments":{}}}]}"#,
    );
    assert!(matches!(refusal.cause(), Cause::EmptyStepId));
    assert_place(&refusal, 0, None, &[]);

    let refusal = refused(r#"{"version":1,"steps":[{"id":3,"call":{}}]}"#);
    assert!(
        matches!(refusal.cause(), Cause::Shape { what: "id", .. }),
        "{refusal}"
    );

    let refusal = refused(r#"{"version":1,"steps":["select"]}"#);
    assert!(
        matches!(refusal.cause(), Cause::Shape { what: "a step", .. }),
        "{refusal}"
    );
}

#[test]
fn a_call_needs_an_entry_and_arguments() {
    let refusal = refused(r#"{"version":1,"steps":[{"id":"a","call":{"arguments":{}}}]}"#);
    assert!(matches!(refusal.cause(), Cause::MissingField("entry")));
    assert_place(&refusal, 0, Some("a"), &[]);

    let refusal =
        refused(r#"{"version":1,"steps":[{"id":"a","call":{"entry":"LazyFrame.select"}}]}"#);
    assert!(matches!(refusal.cause(), Cause::MissingField("arguments")));
}

#[test]
fn an_entry_the_catalogue_lacks_is_refused() {
    for name in ["LazyFrame.nope", "select", "", "lazyframe.select"] {
        let refusal = refused_steps(vec![step("s", name, json!({}))]);
        assert!(
            matches!(refusal.cause(), Cause::UnknownEntry(entry) if entry == name),
            "{name}: {refusal}"
        );
        assert_place(&refusal, 0, Some("s"), &[]);
    }
    // Inside an expression too, at the link.
    let refusal = refused_steps(vec![add_x(field_then(
        "a",
        vec![call("Expr.nope", json!({}))],
    ))]);
    assert!(matches!(refusal.cause(), Cause::UnknownEntry(_)));
    assert_place(
        &refusal,
        0,
        Some("s"),
        &[
            parameter("named_exprs"),
            Segment::Value(0),
            Segment::Link(0),
        ],
    );
}

#[test]
fn a_step_must_call_a_method_of_a_lazy_frame() {
    for name in ["Expr.shift", "polars.col", "When.then", "polars.when"] {
        let refusal = refused_steps(vec![step("s", name, json!({}))]);
        assert!(
            matches!(refusal.cause(), Cause::WrongPlace { entry, .. } if *entry == name),
            "{name}: {refusal}"
        );
        assert_place(&refusal, 0, Some("s"), &[]);
    }
}

#[test]
fn an_expression_must_start_with_a_function_or_a_when() {
    // A frame step and an expression method cannot start a chain.
    for name in ["LazyFrame.sort", "Expr.shift", "Then.otherwise"] {
        let start = call(name, json!({}));
        let refusal = refused_steps(vec![sort_by(many(vec![chain(start, vec![])]))]);
        assert!(
            matches!(refusal.cause(), Cause::WrongPlace { entry, .. } if *entry == name),
            "{name}: {refusal}"
        );
        assert_place(
            &refusal,
            0,
            Some("s"),
            &[parameter("by"), Segment::Item(0), Segment::Start],
        );
    }
}

#[test]
fn a_function_or_a_step_cannot_follow_the_start_of_an_expression() {
    for name in ["polars.col", "polars.when", "LazyFrame.filter"] {
        let refusal = refused_steps(vec![sort_by(many(vec![field_then(
            "a",
            vec![call("Expr.is_null", json!({})), call(name, json!({}))],
        )]))]);
        let Cause::WrongPlace { entry, applies, .. } = refusal.cause() else {
            panic!("{name}: {refusal}");
        };
        assert_eq!(*entry, name);
        assert_eq!(*applies, self::entry(name).expect("an entry").applies(),);
        assert_place(
            &refusal,
            0,
            Some("s"),
            &[parameter("by"), Segment::Item(0), Segment::Link(1)],
        );
    }
}

#[test]
fn a_function_that_is_a_wrong_place_says_what_it_applies_to() {
    let refusal = refused_steps(vec![step("s", "Expr.shift", json!({}))]);
    assert!(matches!(
        refusal.cause(),
        Cause::WrongPlace {
            applies: Applies::Expression,
            ..
        }
    ));
}

/// A condition of `links` after `when(a)`, as a `by` argument.
fn when_then(links: Vec<Json>) -> Json {
    let start = call(
        "polars.when",
        json!({ "predicates": many(vec![field("a")]) }),
    );
    sort_by(many(vec![chain(start, links)]))
}

fn statement(value: Json) -> Json {
    json!({ "statement": value })
}

#[test]
fn condition_links_out_of_order_are_refused_at_the_link() {
    let by = [parameter("by"), Segment::Item(0)];
    let at = |link: usize| [by.to_vec(), vec![Segment::Link(link)]].concat();
    let then = || call("When.then", statement(text("x")));
    let otherwise = || call("Then.otherwise", statement(text("y")));
    let again = || call("Then.when", json!({ "predicates": many(vec![field("b")]) }));

    // otherwise before its then
    let refusal = refused_steps(vec![when_then(vec![otherwise()])]);
    assert!(
        matches!(
            refusal.cause(),
            Cause::OutOfOrder {
                entry: "Then.otherwise",
                needs: "then",
                found: "when"
            }
        ),
        "{refusal}"
    );
    assert_place(&refusal, 0, Some("s"), &at(0));

    // a second then
    let refusal = refused_steps(vec![when_then(vec![then(), then()])]);
    assert!(
        matches!(
            refusal.cause(),
            Cause::OutOfOrder {
                entry: "When.then",
                needs: "when",
                found: "then"
            }
        ),
        "{refusal}"
    );
    assert_place(&refusal, 0, Some("s"), &at(1));

    // a link after the condition has ended
    let refusal = refused_steps(vec![when_then(vec![then(), otherwise(), again()])]);
    assert!(
        matches!(
            refusal.cause(),
            Cause::OutOfOrder {
                entry: "Then.when",
                needs: "then",
                found: "an expression"
            }
        ),
        "{refusal}"
    );
    assert_place(&refusal, 0, Some("s"), &at(2));

    // a chained when takes the chained then, not the first one
    let refusal = refused_steps(vec![when_then(vec![then(), again(), then()])]);
    assert!(
        matches!(
            refusal.cause(),
            Cause::OutOfOrder {
                entry: "When.then",
                needs: "when",
                found: "chained when"
            }
        ),
        "{refusal}"
    );
    assert_place(&refusal, 0, Some("s"), &at(2));

    // the chained then closes with the chained otherwise
    let refusal = refused_steps(vec![when_then(vec![
        then(),
        again(),
        call("ChainedWhen.then", statement(text("z"))),
        otherwise(),
    ])]);
    assert!(
        matches!(
            refusal.cause(),
            Cause::OutOfOrder {
                entry: "Then.otherwise",
                needs: "then",
                found: "chained then"
            }
        ),
        "{refusal}"
    );
    assert_place(&refusal, 0, Some("s"), &at(3));
}

#[test]
fn an_expression_method_on_a_condition_that_has_no_then_is_refused() {
    let refusal = refused_steps(vec![when_then(vec![call("Expr.is_null", json!({}))])]);
    assert!(
        matches!(
            refusal.cause(),
            Cause::OutOfOrder {
                entry: "Expr.is_null",
                needs: "then",
                found: "when"
            }
        ),
        "{refusal}"
    );
}

#[test]
fn a_condition_that_ends_without_its_then_is_refused() {
    let refusal = refused_steps(vec![when_then(vec![])]);
    assert!(
        matches!(
            refusal.cause(),
            Cause::UnfinishedCondition { needs: "then" }
        ),
        "{refusal}"
    );
    assert_place(
        &refusal,
        0,
        Some("s"),
        &[parameter("by"), Segment::Item(0), Segment::Start],
    );

    let links = vec![
        call("When.then", statement(text("x"))),
        call("Then.when", json!({ "predicates": many(vec![field("b")]) })),
    ];
    let refusal = refused_steps(vec![when_then(links)]);
    assert!(
        matches!(refusal.cause(), Cause::UnfinishedCondition { .. }),
        "{refusal}"
    );
    assert_place(
        &refusal,
        0,
        Some("s"),
        &[parameter("by"), Segment::Item(0), Segment::Link(1)],
    );
}

#[test]
fn a_condition_that_has_its_then_is_an_expression_and_methods_may_follow_it() {
    let links = vec![
        call("When.then", statement(text("x"))),
        call("Expr.is_null", json!({})),
    ];
    let document = Document::read(&document(vec![when_then(links)])).expect("a document");
    assert_eq!(document.steps().len(), 1);
}

#[test]
fn arguments_the_catalogue_refuses_are_refused_at_the_parameter() {
    // No such parameter.
    let refusal = refused_steps(vec![step(
        "s",
        "LazyFrame.select",
        json!({ "bogus": many(vec![]) }),
    )]);
    assert!(
        matches!(refusal.cause(), Cause::Arguments(BuildError::UnknownParameter { parameter }) if parameter == "bogus"),
        "{refusal}"
    );
    assert_place(&refusal, 0, Some("s"), &[parameter("bogus")]);

    // A list where a parameter of shape many wants a list, but one is not given.
    let refusal = refused_steps(vec![step(
        "s",
        "LazyFrame.sort",
        json!({ "by": field("a") }),
    )]);
    assert!(matches!(
        refusal.cause(),
        Cause::Arguments(BuildError::WrongKind {
            parameter: "by",
            ..
        })
    ));
    assert_place(&refusal, 0, Some("s"), &[parameter("by")]);

    // A word that is not one of the choices.
    let refusal = refused_steps(vec![step(
        "s",
        "LazyFrame.join",
        json!({ "other": { "table": "t" }, "how": { "choice": "sideways" } }),
    )]);
    assert!(
        matches!(refusal.cause(), Cause::Arguments(BuildError::UnknownChoice { parameter: "how", word }) if word == "sideways"),
        "{refusal}"
    );
    assert_place(&refusal, 0, Some("s"), &[parameter("how")]);
}

#[test]
fn a_wrong_item_of_a_list_is_found_by_its_index() {
    let by = many(vec![field("a"), field("b"), json!({ "string": "c" })]);
    let refusal = refused_steps(vec![sort_by(by)]);
    assert!(matches!(
        refusal.cause(),
        Cause::Arguments(BuildError::WrongKind {
            parameter: "by",
            ..
        })
    ));
    assert_place(&refusal, 0, Some("s"), &[parameter("by"), Segment::Item(2)]);
}

#[test]
fn a_wrong_key_or_value_of_pairs_is_found_by_its_entry() {
    // The value of the second entry is text where an expression is wanted.
    let pairs = json!({ "pairs": [
        [{ "columns": ["a"] }, field("a")],
        [{ "columns": ["b"] }, { "string": "x" }],
    ] });
    let refusal = refused_steps(vec![step(
        "s",
        "LazyFrame.with_columns",
        json!({ "named_exprs": pairs }),
    )]);
    assert!(matches!(
        refusal.cause(),
        Cause::Arguments(BuildError::WrongKind {
            parameter: "named_exprs",
            ..
        })
    ));
    assert_place(
        &refusal,
        0,
        Some("s"),
        &[parameter("named_exprs"), Segment::Value(1)],
    );

    // The key of the first is an expression where a column name is wanted.
    let pairs = json!({ "pairs": [[field("a"), field("a")]] });
    let refusal = refused_steps(vec![step(
        "s",
        "LazyFrame.with_columns",
        json!({ "named_exprs": pairs }),
    )]);
    assert_place(
        &refusal,
        0,
        Some("s"),
        &[parameter("named_exprs"), Segment::Key(0)],
    );

    // The new names of a rename are text.
    let pairs = json!({ "pairs": [[{ "columns": ["a"] }, { "string": "b" }], [{ "columns": ["c"] }, { "integer": 1 }]] });
    let refusal = refused_steps(vec![step(
        "s",
        "LazyFrame.rename",
        json!({ "mapping": pairs }),
    )]);
    assert_place(
        &refusal,
        0,
        Some("s"),
        &[parameter("mapping"), Segment::Value(1)],
    );
}

#[test]
fn an_argument_wrong_inside_an_expression_is_found_through_the_links() {
    // shift's `n` is an expression, not a bare number: the second link of the
    // third item's chain.
    let wrong = field_then(
        "a",
        vec![
            call("Expr.is_null", json!({})),
            call("Expr.shift", json!({ "n": { "integer": 1 } })),
        ],
    );
    let by = many(vec![field("a"), field("b"), wrong]);
    let refusal = refused_steps(vec![sort_by(by)]);
    assert!(
        matches!(
            refusal.cause(),
            Cause::Arguments(BuildError::WrongKind { parameter: "n", .. })
        ),
        "{refusal}"
    );
    assert_place(
        &refusal,
        0,
        Some("s"),
        &[
            parameter("by"),
            Segment::Item(2),
            Segment::Link(1),
            parameter("n"),
        ],
    );

    // And through the start of a chain inside a pair's value.
    let start = call("polars.col", json!({ "name": { "string": "a" } }));
    let refusal = refused_steps(vec![add_x(chain(start, vec![]))]);
    assert_place(
        &refusal,
        0,
        Some("s"),
        &[
            parameter("named_exprs"),
            Segment::Value(0),
            Segment::Start,
            parameter("name"),
        ],
    );
}

#[test]
fn a_parameter_given_twice_to_a_call_is_refused_at_the_parameter() {
    let select = entry("LazyFrame.select").expect("an entry");
    let refusal = Call::new(
        select,
        [
            ("exprs", Value::Many(vec![])),
            ("exprs", Value::Many(vec![])),
        ],
    )
    .expect_err("repeated");
    assert!(matches!(
        refusal.cause(),
        Cause::Arguments(BuildError::Repeated { parameter: "exprs" })
    ));
    assert_eq!(refusal.place().path(), [parameter("exprs")]);
}

#[test]
fn an_argument_is_an_object_of_one_key_of_a_kind_the_document_has() {
    let by = |argument: Json| many(vec![argument]);
    let refusal = refused_steps(vec![sort_by(by(json!({ "bogus": 1 })))]);
    assert!(matches!(refusal.cause(), Cause::UnknownArgument(kind) if kind == "bogus"));
    assert_place(&refusal, 0, Some("s"), &[parameter("by"), Segment::Item(0)]);

    let refusal = refused_steps(vec![sort_by(by(json!({ "string": "a", "integer": 1 })))]);
    assert!(matches!(refusal.cause(), Cause::ArgumentKeys(2)));
    let refusal = refused_steps(vec![sort_by(by(json!({})))]);
    assert!(matches!(refusal.cause(), Cause::ArgumentKeys(0)));
    let refusal = refused_steps(vec![sort_by(by(json!("a")))]);
    assert!(matches!(
        refusal.cause(),
        Cause::Shape {
            what: "an argument",
            ..
        }
    ));
}

#[test]
fn the_value_of_an_argument_must_be_of_its_json_type() {
    for (argument, what) in [
        (json!({ "integer": "1" }), "integer"),
        (json!({ "integer": 1.5 }), "integer"),
        (
            json!({ "integer": 9_223_372_036_854_775_808_u64 }),
            "integer",
        ),
        (json!({ "float": 1 }), "float"),
        (json!({ "float": "1.5" }), "float"),
        (json!({ "flag": "true" }), "flag"),
        (json!({ "string": 1 }), "string"),
        (json!({ "choice": ["a"] }), "choice"),
        (json!({ "columns": "a" }), "columns"),
        (json!({ "pairs": {} }), "pairs"),
        (json!({ "many": "a" }), "many"),
        (json!({ "table": null }), "table"),
        (json!({ "dtype": 1 }), "dtype"),
        (json!({ "expression": [] }), "an expression"),
    ] {
        let refusal = refused_steps(vec![sort_by(many(vec![argument.clone()]))]);
        assert!(
            matches!(refusal.cause(), Cause::Shape { what: found, .. } if *found == what),
            "{argument}: {refusal}"
        );
        assert_place(&refusal, 0, Some("s"), &[parameter("by"), Segment::Item(0)]);
    }
    // A column name that is not text is found by its index.
    let refusal = refused_steps(vec![step(
        "s",
        "LazyFrame.pivot",
        json!({ "on": { "columns": ["a", 1] }, "on_columns": { "table": "t" } }),
    )]);
    assert_place(&refusal, 0, Some("s"), &[parameter("on"), Segment::Item(1)]);
}

#[test]
fn a_pair_is_a_key_and_a_value() {
    let pairs = json!({ "pairs": [[{ "columns": ["a"] }]] });
    let refusal = refused_steps(vec![step(
        "s",
        "LazyFrame.with_columns",
        json!({ "named_exprs": pairs }),
    )]);
    assert!(matches!(refusal.cause(), Cause::PairSize(1)));
    assert_place(
        &refusal,
        0,
        Some("s"),
        &[parameter("named_exprs"), Segment::Item(0)],
    );

    let three = json!({ "pairs": [[{ "columns": ["a"] }, field("a"), field("a")]] });
    let refusal = refused_steps(vec![step(
        "s",
        "LazyFrame.with_columns",
        json!({ "named_exprs": three }),
    )]);
    assert!(matches!(refusal.cause(), Cause::PairSize(3)));

    let object = json!({ "pairs": [{ "key": 1 }] });
    let refusal = refused_steps(vec![step(
        "s",
        "LazyFrame.with_columns",
        json!({ "named_exprs": object }),
    )]);
    assert!(matches!(
        refusal.cause(),
        Cause::Shape { what: "a pair", .. }
    ));
}

#[test]
fn a_dtype_is_read_as_python_polars_spells_it() {
    let lit = |dtype: &str| {
        let value = call(
            "polars.lit",
            json!({ "value": { "integer": 1 }, "dtype": { "dtype": dtype } }),
        );
        sort_by(many(vec![chain(value, vec![])]))
    };
    assert!(Document::read(&document(vec![lit("Int8")])).is_ok());
    assert!(
        Document::read(&document(vec![lit(
            "Datetime(time_unit='ms', time_zone=None)"
        )]))
        .is_ok()
    );

    let refusal = refused_steps(vec![lit("int8")]);
    assert!(matches!(refusal.cause(), Cause::Dtype(DtypeError::Unknown(text)) if text == "int8"));
    assert_place(
        &refusal,
        0,
        Some("s"),
        &[
            parameter("by"),
            Segment::Item(0),
            Segment::Start,
            parameter("dtype"),
        ],
    );
}

#[test]
fn a_number_that_is_not_finite_is_refused_where_it_is() {
    let lit = entry("polars.lit").expect("an entry");
    let refusal = Call::new(lit, [("value", Value::Float(f64::NAN))]).expect_err("not finite");
    assert!(matches!(refusal.cause(), Cause::NotFinite));
    assert_eq!(refusal.place().path(), [parameter("value")]);
    assert!(Call::new(lit, [("value", Value::Float(f64::INFINITY))]).is_err());
    assert!(Call::new(lit, [("value", Value::Float(1.5))]).is_ok());
}

#[test]
fn a_step_identifier_used_twice_is_refused_at_the_second() {
    let steps = vec![
        step("same", "LazyFrame.select", json!({})),
        step("other", "LazyFrame.select", json!({})),
        step("same", "LazyFrame.select", json!({})),
    ];
    let refusal = refused_steps(steps);
    assert!(matches!(refusal.cause(), Cause::DuplicateStep(id) if id.as_str() == "same"));
    assert_place(&refusal, 2, Some("same"), &[]);
}

#[test]
fn a_refusal_says_where_and_why_in_words() {
    let refusal = refused_steps(vec![
        step("first", "LazyFrame.select", json!({})),
        sort_by(many(vec![field("a"), field("b"), json!({ "string": "c" })])),
    ]);
    let words = refusal.to_string();
    assert!(words.contains("step \"s\""), "{words}");
    assert!(words.contains("by item 3"), "{words}");
    assert!(words.contains("takes an expression"), "{words}");
}
