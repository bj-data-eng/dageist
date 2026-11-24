//! What an edit refuses, and where it says so.

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

/// Asserts that `result` is refused for `problem` at `place`.
fn assert_refused(
    result: Result<Document, dageist_engine::document::EditError>,
    place: &dageist_engine::document::Place,
    problem: impl Fn(&EditProblem) -> bool,
) {
    let error = refused(result);
    assert!(problem(error.problem()), "{error}: {:?}", error.problem());
    assert_eq!(error.place(), place, "{error}");
}

#[test]
fn a_step_that_the_document_does_not_have_is_refused() {
    let document = document();
    let absent = inside(7, "first", vec![parameter("columns")]);
    assert_refused(
        document.remove_step(&step_at(7, "first")),
        &step_at(7, "first"),
        |problem| matches!(problem, EditProblem::NoStep { steps: 3 }),
    );
    assert_refused(
        document.set_argument(&absent, Value::Integer(1)),
        &absent,
        |problem| matches!(problem, EditProblem::NoStep { steps: 3 }),
    );
}

#[test]
fn a_place_made_for_another_document_is_refused_and_names_the_step_that_is_there() {
    let document = document();
    // The step at that number is `second`, so the place is out of date.
    assert_refused(
        document.remove_step(&step_at(1, "first")),
        &step_at(1, "first"),
        |problem| matches!(problem, EditProblem::StepChanged { found } if found.as_str() == "second"),
    );
}

#[test]
fn a_place_with_nothing_at_its_end_is_refused_at_the_segment_that_has_none() {
    let document = document();
    let past = inside(1, "second", vec![parameter("columns"), Segment::Item(4)]);
    assert_refused(
        document.set_argument(&past, Value::Columns(vec!["x".to_owned()])),
        &past,
        |problem| matches!(problem, EditProblem::Nothing),
    );
    assert_refused(document.remove_item(&past), &past, |problem| {
        matches!(problem, EditProblem::Nothing)
    });
    let not_given = inside(2, "third", vec![parameter("validate")]);
    assert_refused(document.clear_argument(&not_given), &not_given, |problem| {
        matches!(problem, EditProblem::Nothing)
    });
    // The way stops at the first segment that has nothing there.
    let deeper = inside(2, "third", vec![parameter("validate"), Segment::Item(0)]);
    assert_refused(
        document.set_argument(&deeper, Value::Integer(1)),
        &not_given,
        |problem| matches!(problem, EditProblem::Nothing),
    );
}

#[test]
fn a_segment_that_what_is_there_does_not_have_is_refused_and_says_what_is_there() {
    let document = document();
    let item_of_a_choice = inside(2, "third", vec![parameter("how"), Segment::Item(0)]);
    assert_refused(
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
    // An argument cannot be set at the start of an expression, which is no argument.
    let start = inside(
        0,
        "first",
        vec![parameter("exprs"), Segment::Item(0), Segment::Start],
    );
    assert_refused(
        document.set_argument(&start, Value::Integer(1)),
        &start,
        |problem| matches!(problem, EditProblem::WrongPlace { .. }),
    );
    // A step is no argument, and no link.
    assert_refused(
        document.clear_argument(&step_at(0, "first")),
        &step_at(0, "first"),
        |problem| matches!(problem, EditProblem::WrongPlace { .. }),
    );
    assert_refused(
        document.remove_link(&step_at(0, "first")),
        &step_at(0, "first"),
        |problem| matches!(problem, EditProblem::WrongPlace { .. }),
    );
    // A choice holds no expression to put a link in.
    let how = inside(2, "third", vec![parameter("how")]);
    assert_refused(
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
    let columns = inside(1, "second", vec![parameter("columns")]);
    assert_refused(
        document.insert_item(&columns, 5, Value::Columns(vec!["x".to_owned()])),
        &columns,
        |problem| matches!(problem, EditProblem::OutOfRange { index: 5, len: 1 }),
    );
    assert_refused(
        document.move_step(&step_at(0, "first"), 3),
        &step_at(0, "first"),
        |problem| matches!(problem, EditProblem::OutOfRange { index: 3, len: 3 }),
    );
    let expression = inside(0, "first", vec![parameter("exprs"), Segment::Item(0)]);
    assert_refused(
        document.insert_link(&expression, 2, "Expr.abs", None),
        &expression,
        |problem| matches!(problem, EditProblem::OutOfRange { index: 2, len: 0 }),
    );
    let frame = StepFrame {
        columns: &[],
        tables: &[],
    };
    assert_refused(
        document.add_step(4, "LazyFrame.head", &frame),
        &dageist_engine::document::Place::default(),
        |problem| matches!(problem, EditProblem::OutOfRange { index: 4, len: 3 }),
    );
}

#[test]
fn an_entry_the_catalogue_lacks_is_refused_where_the_edit_names_it() {
    let document = document();
    let expression = inside(0, "first", vec![parameter("exprs"), Segment::Item(0)]);
    assert_refused(
        document.insert_link(&expression, 0, "Expr.levitate", None),
        &expression,
        |problem| matches!(problem, EditProblem::UnknownEntry(id) if id == "Expr.levitate"),
    );
    assert_refused(
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
        document.swap_step(&step_at(0, "first"), "LazyFrame.levitate", &frame),
        &step_at(0, "first"),
        |problem| matches!(problem, EditProblem::UnknownEntry(_)),
    );
}

#[test]
fn an_argument_the_entry_does_not_take_is_refused_by_the_model_at_its_parameter() {
    let document = document();
    let colour = inside(1, "second", vec![parameter("colour")]);
    assert_refused(
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

fn sort_by_two_keys() -> Document {
    read(vec![step(
        "order",
        "LazyFrame.sort",
        json!({ "by": many(vec![crate::support::field("a"), crate::support::field("b")]) }),
    )])
}

#[test]
fn a_per_key_value_is_refused_for_a_parameter_that_holds_none_for_each_key() {
    let document = sort_by_two_keys();
    // `maintain_order` is one flag for the whole sort.
    assert_refused(
        document.set_for_item(
            &step_at(0, "order"),
            "maintain_order",
            0,
            Value::Flag(Some(true)),
        ),
        &inside(0, "order", vec![parameter("maintain_order")]),
        |problem| matches!(problem, EditProblem::NotPerItem),
    );
}

#[test]
fn a_per_key_value_is_refused_for_a_key_the_sort_does_not_have() {
    let document = sort_by_two_keys();
    assert_refused(
        document.set_for_item(
            &step_at(0, "order"),
            "descending",
            5,
            Value::Flag(Some(true)),
        ),
        &inside(0, "order", vec![parameter("descending")]),
        |problem| matches!(problem, EditProblem::OutOfRange { index: 5, len: 2 }),
    );
}

#[test]
fn a_per_key_value_is_refused_for_a_parameter_the_entry_does_not_have() {
    let document = sort_by_two_keys();
    assert_refused(
        document.set_for_item(&step_at(0, "order"), "colour", 0, Value::Flag(Some(true))),
        &inside(0, "order", vec![parameter("colour")]),
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
fn a_link_is_not_moved_past_the_last_and_an_entry_is_not_put_past_the_end() {
    let document = read(vec![
        step(
            "calc",
            "LazyFrame.select",
            json!({ "exprs": many(vec![crate::support::field_then(
                "a",
                vec![
                    crate::support::call("Expr.abs", json!({})),
                    crate::support::call("Expr.is_null", json!({})),
                ],
            )]) }),
        ),
        step(
            "renamed",
            "LazyFrame.rename",
            json!({ "mapping": { "pairs": [[{ "columns": ["a"] }, { "string": "x" }]] } }),
        ),
    ]);
    let expression = inside(0, "calc", vec![parameter("exprs"), Segment::Item(0)]);
    assert_refused(
        document.move_link(&expression.clone().then(Segment::Link(0)), 2),
        &expression,
        |problem| matches!(problem, EditProblem::OutOfRange { index: 2, len: 2 }),
    );
    let mapping = inside(1, "renamed", vec![parameter("mapping")]);
    assert_refused(
        document.insert_entry(
            &mapping,
            2,
            Value::Columns(vec!["b".to_owned()]),
            Value::String("y".to_owned()),
        ),
        &mapping,
        |problem| matches!(problem, EditProblem::OutOfRange { index: 2, len: 1 }),
    );
}

#[test]
fn a_refusal_of_a_place_with_the_largest_index_can_be_shown() {
    let document = document();
    // Each segment that counts from one when it is shown, at the largest number there is.
    let list = Thing::Value(ValueKind::List);
    let ways = [
        (Segment::Item(usize::MAX), EditProblem::Nothing),
        (
            Segment::Key(usize::MAX),
            EditProblem::DoesNotFit { found: list },
        ),
        (
            Segment::Value(usize::MAX),
            EditProblem::DoesNotFit { found: list },
        ),
        (
            Segment::Link(usize::MAX),
            EditProblem::WrongPlace {
                needs: Wanted::Argument,
                found: Thing::Link,
            },
        ),
    ];
    for (way, expected) in ways {
        let place = inside(1, "second", vec![parameter("columns"), way]);
        let error = refused(document.set_argument(&place, Value::Integer(1)));
        assert_eq!(
            std::mem::discriminant(error.problem()),
            std::mem::discriminant(&expected),
            "{error}"
        );
        assert!(!error.to_string().is_empty(), "{error:?}");
    }
    let item = inside(
        1,
        "second",
        vec![parameter("columns"), Segment::Item(usize::MAX)],
    );
    let error = refused(document.remove_item(&item));
    assert!(matches!(error.problem(), EditProblem::Nothing));
    assert!(error.to_string().contains("item"), "{error}");
    // A step number that no document has, with no identifier to show it by.
    let error =
        refused(document.remove_step(&dageist_engine::document::Place::of_step(usize::MAX, None)));
    assert!(matches!(error.problem(), EditProblem::NoStep { steps: 3 }));
    assert!(error.to_string().contains("step number"), "{error}");
}

#[test]
fn a_place_with_no_identifier_is_checked_against_the_step_number_alone() {
    // The documentation says so: such a place names whichever step is at its number.
    let document = document();
    let edited = document
        .remove_step(&dageist_engine::document::Place::of_step(1, None))
        .expect("removed");
    let ids: Vec<&str> = edited
        .steps()
        .iter()
        .map(|step| step.id().as_str())
        .collect();
    assert_eq!(ids, ["first", "third"]);
}

#[test]
fn a_place_made_for_a_step_that_was_taken_away_names_a_new_step_that_has_its_identifier() {
    // The documentation says so: a name is free again once its step is gone, so the identifier
    // check passes for a place made for the step before, and a host keeps a revision of the
    // document to tell the two documents apart.
    let columns = vec!["a".to_owned()];
    let frame = StepFrame {
        columns: &columns,
        tables: &[],
    };
    let sorted = read(vec![step(
        "sort",
        "LazyFrame.sort",
        json!({ "by": many(vec![crate::support::field("a")]), "descending": { "flag": true } }),
    )]);
    let old_place = inside(0, "sort", vec![parameter("descending")]);
    let again = sorted
        .remove_step(&step_at(0, "sort"))
        .and_then(|empty| empty.add_step(0, "LazyFrame.sort", &frame))
        .expect("a sort again");
    // The new step has the identifier the old one had.
    assert_eq!(again.steps()[0].id().as_str(), "sort");
    assert_ne!(again.steps()[0], sorted.steps()[0]);
    // The place for the old step is not refused: it clears an argument the new step does not have.
    let error = refused(again.clear_argument(&old_place));
    assert!(matches!(error.problem(), EditProblem::Nothing));
    // ... and it edits the new step where the way leads somewhere in it.
    let edited = again
        .set_argument(&old_place, Value::Flag(Some(false)))
        .expect("set");
    assert_eq!(edited.steps()[0].id().as_str(), "sort");
    assert_eq!(
        edited.steps()[0].call().argument("descending"),
        Some(&Value::Flag(Some(false)))
    );
}
