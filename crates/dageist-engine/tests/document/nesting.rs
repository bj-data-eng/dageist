//! A document nests arrays and objects at most `MAX_NESTING` deep: text nested
//! deeper is refused on reading, steps that would be written nested deeper are
//! refused on construction, and so whatever is written can be read back.

use dageist_engine::catalog::entry;
use dageist_engine::document::{
    Call, Cause, Document, Expression, MAX_NESTING, ReadError, Start, Step, StepId, SyntaxProblem,
    Value,
};

/// One level of nesting an expression is built from.
#[derive(Clone, Copy, Debug)]
enum Level {
    /// A link `Expr.dt.offset_by(by=<expression>)`.
    Link,
    /// A condition `polars.when(<expression>).then("y").otherwise("n")`.
    Condition,
}

const LEAF: &str = r#"{"expression":{"start":{"input":"x"},"then":[]}}"#;

fn literal(text: &str) -> String {
    format!(
        r#"{{"expression":{{"start":{{"entry":"polars.lit","arguments":{{"value":{{"string":"{text}"}}}}}},"then":[]}}}}"#
    )
}

/// The JSON of an expression of `levels` around a leaf, outermost first.
fn expression_text(levels: &[Level]) -> String {
    let Some((level, inner)) = levels.split_first() else {
        return LEAF.to_owned();
    };
    let inner = expression_text(inner);
    match level {
        Level::Link => format!(
            r#"{{"expression":{{"start":{{"input":"x"}},"then":[{{"entry":"Expr.dt.offset_by","arguments":{{"by":{inner}}}}}]}}}}"#
        ),
        Level::Condition => format!(
            r#"{{"expression":{{"start":{{"entry":"polars.when","arguments":{{"predicates":{{"many":[{inner}]}}}}}},"then":[{{"entry":"When.then","arguments":{{"statement":{}}}}},{{"entry":"Then.otherwise","arguments":{{"statement":{}}}}}]}}}}"#,
            literal("y"),
            literal("n")
        ),
    }
}

/// The JSON of a document of one step that selects that expression.
fn document_text(levels: &[Level]) -> String {
    format!(
        r#"{{"version":1,"steps":[{{"id":"a","call":{{"entry":"LazyFrame.select","arguments":{{"exprs":{{"many":[{}]}}}}}}}}]}}"#,
        expression_text(levels)
    )
}

fn call(id: &str, arguments: Vec<(&str, Value)>) -> Call {
    Call::new(entry(id).expect("an entry"), arguments).expect("a call")
}

fn text_expression(text: &str) -> Value {
    let start = call(
        "polars.lit",
        vec![("value", Value::String(text.to_owned()))],
    );
    Value::Expression(Expression::new(Start::Call(start), vec![]).expect("an expression"))
}

/// The same expression as [`expression_text`], made in code.
fn expression(levels: &[Level]) -> Expression {
    let Some((level, inner)) = levels.split_first() else {
        return Expression::new(Start::Input("x".to_owned()), vec![]).expect("an expression");
    };
    let inner = Value::Expression(expression(inner));
    match level {
        Level::Link => Expression::new(
            Start::Input("x".to_owned()),
            vec![call("Expr.dt.offset_by", vec![("by", inner)])],
        ),
        Level::Condition => Expression::new(
            Start::Call(call(
                "polars.when",
                vec![("predicates", Value::Many(vec![inner]))],
            )),
            vec![
                call("When.then", vec![("statement", text_expression("y"))]),
                call("Then.otherwise", vec![("statement", text_expression("n"))]),
            ],
        ),
    }
    .expect("an expression")
}

fn document(levels: &[Level]) -> Result<Document, dageist_engine::document::Refusal> {
    let select = call(
        "LazyFrame.select",
        vec![(
            "exprs",
            Value::Many(vec![Value::Expression(expression(levels))]),
        )],
    );
    Document::new(vec![
        Step::new(StepId::new("a").expect("an id"), select).expect("a step"),
    ])
}

/// How many arrays and objects are open at once in JSON text without a
/// bracket inside a string, and the column where the count first exceeds
/// `limit`.
fn measure(text: &str, limit: usize) -> (usize, Option<usize>) {
    let (mut open, mut deepest, mut over) = (0, 0, None);
    for (at, character) in text.char_indices() {
        match character {
            '{' | '[' => {
                open += 1;
                deepest = deepest.max(open);
                if open > limit && over.is_none() {
                    over = Some(at + 1);
                }
            }
            '}' | ']' => open -= 1,
            _ => {}
        }
    }
    (deepest, over)
}

#[test]
fn the_limit_is_what_the_documentation_says() {
    assert_eq!(MAX_NESTING, 128);
}

#[test]
fn text_nested_up_to_the_limit_is_read_and_deeper_is_refused_at_the_bracket() {
    let prefix = r#"{"version":1,"steps":"#;
    for open in [
        MAX_NESTING - 2,
        MAX_NESTING - 1,
        MAX_NESTING,
        MAX_NESTING + 1,
    ] {
        let text = format!("{prefix}{}{}}}", "[".repeat(open), "]".repeat(open));
        let (deepest, _) = measure(&text, MAX_NESTING);
        assert_eq!(deepest, open + 1, "the document's object is one more");
        let result = Document::read(&text);
        if deepest <= MAX_NESTING {
            // Nested arrays are no steps, but the text is read as JSON.
            assert!(matches!(result, Err(ReadError::Refused(_))), "{deepest}");
        } else {
            let Err(ReadError::Syntax(error)) = result else {
                panic!("a syntax error at depth {deepest}");
            };
            assert_eq!(error.problem(), &SyntaxProblem::TooDeep);
            // The bracket that opens the 129th level is the 128th of the arrays.
            assert_eq!(
                (error.line(), error.column()),
                (1, prefix.len() + MAX_NESTING)
            );
        }
    }
}

#[test]
fn objects_count_as_much_as_arrays() {
    let text = format!(
        "{}1{}",
        r#"{"a":"#.repeat(MAX_NESTING + 1),
        "}".repeat(MAX_NESTING + 1)
    );
    let Err(ReadError::Syntax(error)) = Document::read(&text) else {
        panic!("a syntax error");
    };
    assert_eq!(error.problem(), &SyntaxProblem::TooDeep);
    assert_eq!((error.line(), error.column()), (1, 5 * MAX_NESTING + 1));
    let allowed = format!(
        "{}1{}",
        r#"{"a":"#.repeat(MAX_NESTING),
        "}".repeat(MAX_NESTING)
    );
    assert!(matches!(
        Document::read(&allowed),
        Err(ReadError::Refused(_))
    ));
}

#[test]
fn hostile_nesting_is_refused_without_exhausting_the_stack() {
    for open in ["[", r#"{"a":"#] {
        let text = open.repeat(200_000);
        let Err(ReadError::Syntax(error)) = Document::read(&text) else {
            panic!("a syntax error");
        };
        assert_eq!(error.problem(), &SyntaxProblem::TooDeep);
    }
}

#[test]
fn a_document_reads_and_is_made_exactly_when_it_nests_within_the_limit() {
    let (mut accepted, mut refused) = (Vec::new(), Vec::new());
    for links in 0..=28 {
        for conditions in 0..=24 {
            let mut levels = vec![Level::Link; links];
            levels.extend(vec![Level::Condition; conditions]);
            let text = document_text(&levels);
            let (deepest, over) = measure(&text, MAX_NESTING);
            if deepest > 150 {
                continue;
            }
            let made = document(&levels);
            let read = Document::read(&text);
            if deepest <= MAX_NESTING {
                let made = made.expect("a document within the limit");
                let read = read.expect("text within the limit");
                assert_eq!(made, read, "{links} links, {conditions} conditions");
                // What can be written reads back: written is the text, and it is within the limit.
                assert_eq!(made.write(), text);
                accepted.push(deepest);
            } else {
                let refusal = made.expect_err("a document beyond the limit");
                assert!(matches!(refusal.cause(), Cause::TooDeep), "{refusal}");
                assert_eq!(refusal.place().step_index(), Some(0));
                let Err(ReadError::Syntax(error)) = read else {
                    panic!("a syntax error for depth {deepest}");
                };
                assert_eq!(error.problem(), &SyntaxProblem::TooDeep);
                assert_eq!(Some(error.column()), over);
                refused.push(deepest);
            }
        }
    }
    // The shapes reach the limit itself, and the first level beyond it.
    assert_eq!(accepted.iter().max(), Some(&MAX_NESTING));
    assert_eq!(refused.iter().min(), Some(&(MAX_NESTING + 1)));
}

#[test]
fn a_step_built_far_deeper_than_the_limit_is_refused_without_exhausting_the_stack() {
    // Six hundred links is several times the limit, deep enough that building
    // the step's whole JSON before measuring it overflows a test thread's
    // stack, while building and dropping the step itself does not.
    let levels = vec![Level::Link; 600];
    let refusal = document(&levels).expect_err("far too deep to be a document");
    assert!(matches!(refusal.cause(), Cause::TooDeep), "{refusal}");
}
