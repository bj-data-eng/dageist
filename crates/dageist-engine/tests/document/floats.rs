//! Numbers in a document: a float comes back as the same `f64`, bit for bit,
//! because its text is read as IEEE 754 rounds a decimal to the nearest value.

use dageist_engine::catalog::entry;
use dageist_engine::document::{
    Call, Cause, Document, ReadError, Start, Step, StepId, SyntaxProblem, Value,
};

/// A document with one step, `LazyFrame.select(polars.lit(value))`.
fn with_literal(value: Value) -> Document {
    let literal =
        Call::new(entry("polars.lit").expect("an entry"), [("value", value)]).expect("a call");
    let expression = dageist_engine::document::Expression::new(Start::Call(literal), vec![])
        .expect("an expression");
    let select = Call::new(
        entry("LazyFrame.select").expect("an entry"),
        [("exprs", Value::Many(vec![Value::Expression(expression)]))],
    )
    .expect("a call");
    let step = Step::new(StepId::new("only").expect("an id"), select).expect("a step");
    Document::new(vec![step]).expect("a document")
}

/// The literal of the only step of a document made by `with_literal`.
fn literal(document: &Document) -> &Value {
    let Some(Value::Many(items)) = document.steps()[0].call().argument("exprs") else {
        panic!("the step's exprs are a list");
    };
    let [Value::Expression(expression)] = items.as_slice() else {
        panic!("one expression");
    };
    let Start::Call(start) = expression.start() else {
        panic!("a call starts it");
    };
    start.argument("value").expect("a value")
}

fn float_of(document: &Document) -> f64 {
    match literal(document) {
        Value::Float(number) => *number,
        other => panic!("not a float: {other:?}"),
    }
}

/// A document of one step, `LazyFrame.select(polars.lit(<argument>))`, where the
/// argument is `{"<kind>": <number>}` as written.
fn text_with(kind: &str, number: &str) -> String {
    r#"{"version":1,"steps":[{"id":"a","call":{"entry":"LazyFrame.select","arguments":{"exprs":{"many":[{"expression":{"start":{"entry":"polars.lit","arguments":{"value":{"KIND":NUMBER}}},"then":[]}}]}}}}]}"#
        .replace("KIND", kind)
        .replace("NUMBER", number)
}

/// Floats a decimal text can get wrong: needing seventeen digits, the
/// smallest and largest of each kind, and the first that an `f64` cannot
/// tell from its neighbour as an integer.
fn hard_floats() -> Vec<f64> {
    vec![
        0.07_f64 * 3.0,
        0.1,
        0.1 + 0.2,
        1e-308,
        2.2250738585072014e-308,
        2.225073858507201e-308,
        5e-324,
        f64::MIN_POSITIVE,
        f64::EPSILON,
        f64::MAX,
        f64::MIN,
        -0.0,
        0.0,
        9_007_199_254_740_993_i64 as f64, // 2^53 + 1 rounds to 2^53
        9_007_199_254_740_992.0,
        1e16,
        1e21,
        1e-7,
        123_456_789.123_456_79,
        std::f64::consts::PI,
        -std::f64::consts::E,
    ]
}

#[test]
fn a_float_is_written_and_read_back_with_the_same_bits() {
    for value in hard_floats() {
        let document = with_literal(Value::Float(value));
        let text = document.write();
        let again = Document::read(&text).expect("the text just written");
        assert_eq!(
            float_of(&again).to_bits(),
            value.to_bits(),
            "{value:?} was written {text}"
        );
        assert_eq!(again.write(), text);
    }
}

#[test]
fn a_float_text_is_read_as_the_nearest_f64() {
    // The oracle is the compiler's own reading of the same decimal.
    let cases: [(&str, f64); 8] = [
        ("0.21000000000000002", 0.210_000_000_000_000_02),
        ("0.21", 0.21),
        ("1e-308", 1e-308),
        ("5e-324", 5e-324),
        ("1.7976931348623157e308", f64::MAX),
        ("-0.0", -0.0),
        ("1E2", 100.0),
        ("2.5e+1", 25.0),
    ];
    for (written, expected) in cases {
        let document = Document::read(&text_with("float", written)).expect("a document");
        assert_eq!(
            float_of(&document).to_bits(),
            expected.to_bits(),
            "{written}"
        );
    }
}

#[test]
fn a_decimal_between_two_floats_goes_to_the_nearer() {
    // 2^53 + 1 is halfway between two f64; IEEE 754 rounds to even.
    let text = text_with("float", "9007199254740993.0");
    let document = Document::read(&text).expect("a document");
    assert_eq!(float_of(&document), 9_007_199_254_740_992.0);
}

#[test]
fn a_number_too_large_for_an_f64_is_a_syntax_error_at_its_start() {
    let text = r#"{"version":1e999}"#;
    let Err(ReadError::Syntax(error)) = Document::read(text) else {
        panic!("a syntax error");
    };
    assert_eq!(error.problem(), &SyntaxProblem::NumberOutOfRange);
    assert_eq!((error.line(), error.column()), (1, 12));
}

#[test]
fn a_number_too_small_for_an_f64_reads_as_zero() {
    let text = text_with("float", "1e-999");
    let document = Document::read(&text).expect("a document");
    assert_eq!(float_of(&document).to_bits(), 0.0_f64.to_bits());
}

#[test]
fn integers_read_and_write_across_the_whole_of_i64() {
    for value in [i64::MIN, -1, 0, 1, i64::MAX, 9_007_199_254_740_993] {
        let document = with_literal(Value::Integer(value));
        let text = document.write();
        assert!(
            text.contains(&format!(r#"{{"integer":{value}}}"#)),
            "{text}"
        );
        let again = Document::read(&text).expect("the text just written");
        assert_eq!(literal(&again), &Value::Integer(value));
    }
}

#[test]
fn a_whole_number_beyond_i64_is_no_integer_and_no_float() {
    let beyond = "9223372036854775808";
    for kind in ["integer", "float"] {
        let Err(ReadError::Refused(refusal)) = Document::read(&text_with(kind, beyond)) else {
            panic!("a refusal for {kind}");
        };
        assert!(matches!(refusal.cause(), Cause::Shape { .. }), "{kind}");
    }
    // As a version it is a version this engine does not read, as written.
    let Err(ReadError::Refused(refusal)) =
        Document::read(&format!(r#"{{"version":{beyond},"steps":[]}}"#))
    else {
        panic!("a refusal");
    };
    assert!(matches!(refusal.cause(), Cause::UnsupportedVersion(text) if text == beyond));
}

#[test]
fn a_float_that_is_not_finite_is_refused_when_a_call_is_made() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let call = Call::new(
            entry("polars.lit").expect("an entry"),
            [("value", Value::Float(value))],
        );
        assert!(
            matches!(call, Err(ref refusal) if matches!(refusal.cause(), Cause::NotFinite)),
            "{value}"
        );
    }
}
