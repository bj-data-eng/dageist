//! Writing a document gives the JSON it was read from.

use dageist_engine::catalog::entry;
use dageist_engine::document::{Call, Document, Expression, Start, Step, StepId, Value};
use serde_json::Value as Json;

use crate::claims::storyboard_text;

/// A document in the text `write` gives: compact, each object's fields in the
/// order of the format, each call's arguments in the order given. It has an
/// argument of each kind a parameter of the catalogue takes.
const CANONICAL: &str = concat!(
    r#"{"version":1,"steps":["#,
    r#"{"id":"columns","call":{"entry":"LazyFrame.with_columns","arguments":{"named_exprs":{"pairs":["#,
    r#"[{"columns":["x"]},{"expression":{"start":{"entry":"polars.lit","arguments":{"value":{"float":2.5},"dtype":{"dtype":"Float32"}}},"then":[{"entry":"Expr.cum_sum","arguments":{"reverse":{"flag":true}}}]}}],"#,
    r#"[{"columns":["y"]},{"expression":{"start":{"input":"cutoff"},"then":[{"entry":"Expr.dt.offset_by","arguments":{"by":{"expression":{"start":{"entry":"polars.lit","arguments":{"value":{"string":"1mo"}}},"then":[]}}}}]}}],"#,
    r#"[{"columns":["z"]},{"expression":{"start":{"entry":"polars.lit","arguments":{"value":{"integer":-7},"dtype":{"dtype":"Datetime(time_unit='ns', time_zone='UTC')"}}},"then":[]}}]"#,
    r#"]}}}},"#,
    r#"{"id":"join","call":{"entry":"LazyFrame.join","arguments":{"other":{"table":"other"},"left_on":{"many":["#,
    r#"{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["k"]}}},"then":[]}}]},"#,
    r#""how":{"choice":"left"},"coalesce":{"flag":null},"suffix":{"string":"_r"},"force_parallel":{"flag":false}}}},"#,
    r#"{"id":"condition","call":{"entry":"LazyFrame.select","arguments":{"exprs":{"many":[{"expression":{"start":{"entry":"polars.when","arguments":{"predicates":{"many":["#,
    r#"{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["b"]}}},"then":[]}}]}}},"then":["#,
    r#"{"entry":"When.then","arguments":{"statement":{"expression":{"start":{"entry":"polars.lit","arguments":{"value":{"string":"y"}}},"then":[]}}}},"#,
    r#"{"entry":"Then.otherwise","arguments":{"statement":{"expression":{"start":{"entry":"polars.lit","arguments":{"value":{"string":"n"}}},"then":[]}}}}]}}]}}}}"#,
    r#"]}"#,
);

#[test]
fn a_document_in_the_written_form_is_written_back_exactly() {
    let document = Document::read(CANONICAL).expect("a document");
    assert_eq!(document.steps().len(), 3);
    assert_eq!(document.write(), CANONICAL);
}

#[test]
fn what_is_written_reads_back_as_the_same_document() {
    let document = Document::read(CANONICAL).expect("a document");
    let again = Document::read(&document.write()).expect("the text just written");
    assert_eq!(again, document);
    assert_eq!(again.write(), document.write());
}

#[test]
fn text_in_another_field_order_is_written_as_the_same_json() {
    let shuffled = r#"{"steps":[{"call":{"arguments":{"exprs":{"many":[]}},"entry":"LazyFrame.select"},"id":"a"}],"version":1}"#;
    let document = Document::read(shuffled).expect("a document");
    let (written, original): (Json, Json) = (
        serde_json::from_str(&document.write()).expect("JSON"),
        serde_json::from_str(shuffled).expect("JSON"),
    );
    assert_eq!(written, original);
}

#[test]
fn the_storyboards_steps_are_written_as_the_same_json() {
    let text = storyboard_text();
    let written = Document::read(&text).expect("a document").write();
    let (written, original): (Json, Json) = (
        serde_json::from_str(&written).expect("JSON"),
        serde_json::from_str(&text).expect("JSON"),
    );
    assert_eq!(written, original);
}

#[test]
fn a_document_made_in_code_is_written_in_the_format() {
    let call = |id: &str, arguments: Vec<(&str, Value)>| {
        Call::new(entry(id).expect("an entry"), arguments).expect("a call")
    };
    let field = Value::Expression(
        Expression::new(
            Start::Call(call(
                "polars.col",
                vec![("name", Value::Columns(vec!["dcn".into()]))],
            )),
            vec![],
        )
        .expect("an expression"),
    );
    let sort = Step::new(
        StepId::new("by_key").expect("an id"),
        call("LazyFrame.sort", vec![("by", Value::Many(vec![field]))]),
    )
    .expect("a step");
    let document = Document::new(vec![sort]).expect("a document");
    let expected = concat!(
        r#"{"version":1,"steps":[{"id":"by_key","call":{"entry":"LazyFrame.sort","arguments":{"#,
        r#""by":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"#,
        r#""name":{"columns":["dcn"]}}},"then":[]}}]}}}}]}"#,
    );
    assert_eq!(document.write(), expected);
    assert_eq!(Document::read(expected).expect("a document"), document);
}
