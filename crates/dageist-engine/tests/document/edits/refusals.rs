//! What an edit refuses, and where it says so. A refused edit gives no document,
//! and the one it was made on is as it was.

use dageist_engine::allowed::Literal;
use dageist_engine::catalog;
use dageist_engine::catalog::StepFrame;
use dageist_engine::document::{
    Call, Cause, Document, EditProblem, Expression, Segment, Start, Thing, Value, ValueKind, Wanted,
};
use serde_json::json;

use super::{inside, parameter, read, refused, select, step_at};
use crate::support::{many, step};

fn document() -> Document {
    read(vec![
        select("first", &["a", "b"]),
        step(
            "second",
            "LazyFrame.drop",
            json!({ "columns": many(vec![json!({ "columns": ["a"] })]) }),
        ),
        step(
            "third",
            "LazyFrame.join",
            json!({ "other": { "table": "t" }, "how": { "choice": "left" } }),
        ),
    ])
}

fn text_of(document: &Document) -> String {
    document.write()
}

/// Asserts that `result` is refused for `problem` at `place`, and that `document` is as it was.
fn assert_refused(
    document: &Document,
    before: &str,
    result: Result<Document, dageist_engine::document::EditError>,
    place: &dageist_engine::document::Place,
    problem: impl Fn(&EditProblem) -> bool,
) {
    let error = refused(result);
    assert!(problem(error.problem()), "{error}: {:?}", error.problem());
    assert_eq!(error.place(), place, "{error}");
    assert_eq!(text_of(document), before);
}

#[test]
fn a_step_that_the_document_does_not_have_is_refused() {
    let document = document();
    let before = text_of(&document);
    let absent = inside(7, "first", vec![parameter("columns")]);
    assert_refused(
        &document,
        &before,
        document.remove_step(&step_at(7, "first")),
        &step_at(7, "first"),
        |problem| matches!(problem, EditProblem::NoStep { steps: 3 }),
    );
    assert_refused(
        &document,
        &before,
        document.set_argument(&absent, Value::Integer(1)),
        &absent,
        |problem| matches!(problem, EditProblem::NoStep { steps: 3 }),
    );
}

#[test]
fn a_place_made_for_another_document_is_refused_and_names_the_step_that_is_there() {
    let document = document();
    let before = text_of(&document);
    // The step at that number is `second`, so the place is out of date.
    assert_refused(
        &document,
        &before,
        document.remove_step(&step_at(1, "first")),
        &step_at(1, "first"),
        |problem| matches!(problem, EditProblem::StepChanged { found } if found.as_str() == "second"),
    );
}

#[test]
fn a_place_with_nothing_at_its_end_is_refused_at_the_segment_that_has_none() {
    let document = document();
    let before = text_of(&document);
    let past = inside(1, "second", vec![parameter("columns"), Segment::Item(4)]);
    assert_refused(
        &document,
        &before,
        document.set_argument(&past, Value::Columns(vec!["x".to_owned()])),
        &past,
        |problem| matches!(problem, EditProblem::Nothing),
    );
    assert_refused(
        &document,
        &before,
        document.remove_item(&past),
        &past,
        |problem| matches!(problem, EditProblem::Nothing),
    );
    let not_given = inside(2, "third", vec![parameter("validate")]);
    assert_refused(
        &document,
        &before,
        document.clear_argument(&not_given),
        &not_given,
        |problem| matches!(problem, EditProblem::Nothing),
    );
    // The way stops at the first segment that has nothing there.
    let deeper = inside(2, "third", vec![parameter("validate"), Segment::Item(0)]);
    assert_refused(
        &document,
        &before,
        document.set_argument(&deeper, Value::Integer(1)),
        &not_given,
        |problem| matches!(problem, EditProblem::Nothing),
    );
}

#[test]
fn a_segment_that_what_is_there_does_not_have_is_refused_and_says_what_is_there() {
    let document = document();
    let before = text_of(&document);
    let item_of_a_choice = inside(2, "third", vec![parameter("how"), Segment::Item(0)]);
    assert_refused(
        &document,
        &before,
        document.set_argument(&item_of_a_choice, Value::Integer(1)),
        &item_of_a_choice,
        |problem| {
            matches!(
                problem,
                EditProblem::DoesNotFit {
                    found: Thing::Value(ValueKind::Choice)
                }
            )
        },
    );
    let link_of_a_choice = inside(
        2,
        "third",
        vec![parameter("how"), Segment::Link(0), parameter("n")],
    );
    assert_refused(
        &document,
        &before,
        document.clear_argument(&link_of_a_choice),
        &inside(2, "third", vec![parameter("how"), Segment::Link(0)]),
        |problem| {
            matches!(
                problem,
                EditProblem::DoesNotFit {
                    found: Thing::Value(ValueKind::Choice)
                }
            )
        },
    );
}

#[test]
fn a_place_that_names_another_kind_of_thing_than_the_edit_needs_is_refused() {
    let document = document();
    let before = text_of(&document);
    // An argument cannot be set at the start of an expression, which is no argument.
    let start = inside(
        0,
        "first",
        vec![parameter("exprs"), Segment::Item(0), Segment::Start],
    );
    assert_refused(
        &document,
        &before,
        document.set_argument(&start, Value::Integer(1)),
        &start,
        |problem| matches!(problem, EditProblem::WrongPlace { .. }),
    );
    // A step is no argument, and no link.
    assert_refused(
        &document,
        &before,
        document.clear_argument(&step_at(0, "first")),
        &step_at(0, "first"),
        |problem| matches!(problem, EditProblem::WrongPlace { .. }),
    );
    assert_refused(
        &document,
        &before,
        document.remove_link(&step_at(0, "first")),
        &step_at(0, "first"),
        |problem| matches!(problem, EditProblem::WrongPlace { .. }),
    );
    // A choice holds no expression to put a link in.
    let how = inside(2, "third", vec![parameter("how")]);
    assert_refused(
        &document,
        &before,
        document.insert_link(&how, 0, "Expr.abs", None),
        &how,
        |problem| {
            matches!(
                problem,
                EditProblem::WrongPlace {
                    found: Thing::Value(ValueKind::Choice),
                    ..
                }
            )
        },
    );
    // An item of an argument that is no list.
    assert_refused(
        &document,
        &before,
        document.insert_item(&how, 0, Value::Integer(1)),
        &how,
        |problem| {
            matches!(
                problem,
                EditProblem::WrongPlace {
                    needs: Wanted::List,
                    ..
                }
            )
        },
    );
}

#[test]
fn a_position_past_the_end_is_refused_for_the_list_it_was_asked_in() {
    let document = document();
    let before = text_of(&document);
    let columns = inside(1, "second", vec![parameter("columns")]);
    assert_refused(
        &document,
        &before,
        document.insert_item(&columns, 5, Value::Columns(vec!["x".to_owned()])),
        &columns,
        |problem| matches!(problem, EditProblem::OutOfRange { index: 5, len: 1 }),
    );
    assert_refused(
        &document,
        &before,
        document.move_step(&step_at(0, "first"), 3),
        &step_at(0, "first"),
        |problem| matches!(problem, EditProblem::OutOfRange { index: 3, len: 3 }),
    );
    let expression = inside(0, "first", vec![parameter("exprs"), Segment::Item(0)]);
    assert_refused(
        &document,
        &before,
        document.insert_link(&expression, 2, "Expr.abs", None),
        &expression,
        |problem| matches!(problem, EditProblem::OutOfRange { index: 2, len: 0 }),
    );
    let frame = StepFrame {
        columns: &[],
        tables: &[],
    };
    assert_refused(
        &document,
        &before,
        document.add_step(4, "LazyFrame.head", &frame),
        &dageist_engine::document::Place::default(),
        |problem| matches!(problem, EditProblem::OutOfRange { index: 4, len: 3 }),
    );
}

#[test]
fn an_entry_the_catalogue_lacks_is_refused_where_the_edit_names_it() {
    let document = document();
    let before = text_of(&document);
    let expression = inside(0, "first", vec![parameter("exprs"), Segment::Item(0)]);
    assert_refused(
        &document,
        &before,
        document.insert_link(&expression, 0, "Expr.levitate", None),
        &expression,
        |problem| matches!(problem, EditProblem::UnknownEntry(id) if id == "Expr.levitate"),
    );
    assert_refused(
        &document,
        &before,
        document.swap_link(
            &expression.clone().then(Segment::Link(0)),
            "Expr.levitate",
            None,
        ),
        &expression.then(Segment::Link(0)),
        |problem| matches!(problem, EditProblem::UnknownEntry(_)),
    );
    let frame = StepFrame {
        columns: &[],
        tables: &[],
    };
    assert_refused(
        &document,
        &before,
        document.swap_step(&step_at(0, "first"), "LazyFrame.levitate", &frame),
        &step_at(0, "first"),
        |problem| matches!(problem, EditProblem::UnknownEntry(_)),
    );
}

#[test]
fn an_argument_the_entry_does_not_take_is_refused_by_the_model_at_its_parameter() {
    let document = document();
    let before = text_of(&document);
    let colour = inside(1, "second", vec![parameter("colour")]);
    assert_refused(
        &document,
        &before,
        document.set_argument(&colour, Value::Integer(1)),
        &colour,
        |problem| {
            matches!(
                problem,
                EditProblem::Refused(Cause::Arguments(
                    catalog::BuildError::UnknownParameter { .. }
                ))
            )
        },
    );
}

#[test]
fn a_step_nested_deeper_than_a_document_may_be_written_is_refused_at_the_step() {
    let document = document();
    let before = text_of(&document);
    // Each shift holds the expression before it as its argument, which opens more arrays and objects.
    let mut deep = Expression::field("a").expect("a");
    for _ in 0..70 {
        let shift = Call::new(
            catalog::entry("Expr.shift").expect("shift"),
            [("n", Value::Expression(deep.clone()))],
        )
        .expect("a call");
        deep = Expression::new(Start::field("a").expect("a"), vec![shift]).expect("an expression");
    }
    let item = inside(0, "first", vec![parameter("exprs"), Segment::Item(0)]);
    assert_refused(
        &document,
        &before,
        document.set_argument(&item, Value::Expression(deep)),
        &step_at(0, "first"),
        |problem| matches!(problem, EditProblem::Refused(Cause::TooDeep)),
    );
}

#[test]
fn a_number_that_is_not_finite_cannot_make_a_literal() {
    let refusal = Expression::literal(&Literal::Float(f64::INFINITY)).expect_err("refused");
    assert!(matches!(refusal.cause(), Cause::NotFinite));
}

#[test]
fn what_an_expression_is_made_of_one_word_reads_back_as_itself() {
    // The one-word expressions an edit puts into a document, each in a call that holds one.
    let words: Vec<Expression> = vec![
        Expression::hole(),
        Expression::input("holidays"),
        Expression::field("a").expect("a"),
        Expression::literal(&Literal::Integer(3)).expect("3"),
        Expression::literal(&Literal::String("x".to_owned())).expect("x"),
        Expression::literal(&Literal::Float(2.5)).expect("2.5"),
    ];
    for word in words {
        let edited = read(vec![select("one", &["a"])])
            .set_argument(
                &inside(0, "one", vec![parameter("exprs"), Segment::Item(0)]),
                Value::Expression(word.clone()),
            )
            .expect("set");
        let again = Document::read(&edited.write()).expect("reads");
        assert_eq!(again, edited);
        assert_eq!(
            again.steps()[0].call().argument("exprs"),
            Some(&Value::Many(vec![Value::Expression(word)]))
        );
    }
}
