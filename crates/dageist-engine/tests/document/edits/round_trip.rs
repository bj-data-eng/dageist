//! Every edit gives a document that the reader reads back as the same
//! document. The first tests make each start of the catalogue; the last makes
//! many edits one after another, chosen by a generator with a fixed seed, over
//! every place the documents it makes have, and checks each document that an
//! edit gives.

use dageist_engine::catalog::{self, Applies, Entry, StepFrame};
use dageist_engine::document::{
    Call, Document, EditError, Expression, Place, Segment, Start, Value,
};

use super::{parameter, step_at};

fn columns() -> Vec<String> {
    vec!["a".to_owned(), "b".to_owned()]
}

fn tables() -> Vec<String> {
    vec!["claims".to_owned()]
}

/// Asserts that writing `document` and reading it again gives `document`.
fn assert_reads_back(document: &Document, after: &str) {
    let text = document.write();
    let read = Document::read(&text).unwrap_or_else(|error| panic!("{after}: {error}\n{text}"));
    assert_eq!(&read, document, "{after}");
}

#[test]
fn a_new_step_of_every_entry_that_starts_one_reads_back_as_itself() {
    let (columns, tables) = (columns(), tables());
    let frame = StepFrame {
        columns: &columns,
        tables: &tables,
    };
    let mut made = 0;
    for entry in catalog::entries()
        .iter()
        .filter(|entry| entry.applies() == Applies::Frame)
    {
        if catalog::start_step(entry, &frame).is_none() {
            continue;
        }
        let id = entry.id().as_str();
        let document = Document::new(vec![])
            .expect("a document")
            .add_step(0, id, &frame)
            .unwrap_or_else(|error| panic!("{id}: {error}"));
        assert_reads_back(&document, id);
        // Swapping a step for each of them gives the same step as adding one, in the same place.
        let swapped = document
            .swap_step(&step_at(0, document.steps()[0].id().as_str()), id, &frame)
            .unwrap_or_else(|error| panic!("{id}: {error}"));
        assert_eq!(swapped, document, "{id}");
        made += 1;
    }
    assert!(made >= 15, "{made} steps start");
}

#[test]
fn a_link_of_every_method_of_the_catalogue_can_be_added_and_reads_back_as_itself() {
    let base = Document::read(
        r#"{"version":1,"steps":[{"id":"s","call":{"entry":"LazyFrame.select","arguments":{"exprs":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["a"]}}},"then":[]}}]}}}}]}"#,
    )
    .expect("a document");
    let expression = Place::of_step(0, None)
        .then(parameter("exprs"))
        .then(Segment::Item(0));
    let methods: Vec<&Entry> = catalog::entries()
        .iter()
        .filter(|entry| entry.applies() == Applies::Expression)
        .collect();
    assert!(methods.len() > 100, "{} methods", methods.len());
    for entry in methods {
        let id = entry.id().as_str();
        let added = base
            .insert_link(&expression, 0, id, None)
            .unwrap_or_else(|error| panic!("{id}: {error}"));
        assert_reads_back(&added, id);
        // And swapped for the next, which takes over what it can.
        let link = expression.clone().then(Segment::Link(0));
        let swapped = added
            .swap_link(&link, "Expr.abs", None)
            .unwrap_or_else(|error| panic!("{id}: {error}"));
        assert_reads_back(&swapped, id);
    }
}

/// What a document holds, by place.
#[derive(Default)]
struct Places {
    /// Each call, with its entry.
    calls: Vec<(Place, &'static Entry)>,
    /// Each expression, with how many links it has.
    expressions: Vec<(Place, usize)>,
    lists: Vec<Place>,
    /// Each link, as the expression's place, where it is and how many links there are.
    links: Vec<(Place, usize, usize)>,
}

fn places_of(document: &Document) -> Places {
    let mut places = Places::default();
    for (index, step) in document.steps().iter().enumerate() {
        call_places(
            step.call(),
            &Place::of_step(index, Some(step.id().clone())),
            &mut places,
        );
    }
    places
}

fn call_places(call: &Call, place: &Place, places: &mut Places) {
    places.calls.push((place.clone(), call.entry()));
    for given in call.arguments() {
        value_places(
            given.value(),
            &place.clone().then(parameter(given.parameter())),
            places,
        );
    }
}

fn value_places(value: &Value, place: &Place, places: &mut Places) {
    match value {
        Value::Expression(expression) => {
            places
                .expressions
                .push((place.clone(), expression.links().len()));
            if let Start::Call(start) = expression.start() {
                call_places(start, &place.clone().then(Segment::Start), places);
            }
            for (at, link) in expression.links().iter().enumerate() {
                places
                    .links
                    .push((place.clone(), at, expression.links().len()));
                call_places(link, &place.clone().then(Segment::Link(at)), places);
            }
        }
        Value::Many(items) => {
            places.lists.push(place.clone());
            for (at, item) in items.iter().enumerate() {
                value_places(item, &place.clone().then(Segment::Item(at)), places);
            }
        }
        Value::Pairs(pairs) => {
            for (at, (key, entry)) in pairs.iter().enumerate() {
                value_places(key, &place.clone().then(Segment::Key(at)), places);
                value_places(entry, &place.clone().then(Segment::Value(at)), places);
            }
        }
        _ => {}
    }
}

/// A generator of numbers that is the same on every run.
struct Dice(u64);

impl Dice {
    fn below(&mut self, limit: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        usize::try_from(self.0 % u64::try_from(limit.max(1)).expect("a limit")).expect("a number")
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() {
            None
        } else {
            items.get(self.below(items.len()))
        }
    }
}

/// One edit chosen by `dice`, made on `document`, or `None` where there was nothing to choose it
/// from.
fn some_edit(dice: &mut Dice, document: &Document) -> Option<Result<Document, EditError>> {
    let (columns, tables) = (columns(), tables());
    let frame = StepFrame {
        columns: &columns,
        tables: &tables,
    };
    let places = places_of(document);
    let frames: Vec<&str> = catalog::entries()
        .iter()
        .filter(|entry| catalog::start_step(entry, &frame).is_some())
        .map(|entry| entry.id().as_str())
        .collect();
    let methods: Vec<&str> = catalog::entries()
        .iter()
        .filter(|entry| entry.applies() != Applies::Frame && entry.applies() != Applies::Function)
        .map(|entry| entry.id().as_str())
        .collect();
    let steps = document.steps().len();
    let step_place = |dice: &mut Dice| {
        let index = dice.below(steps);
        document
            .steps()
            .get(index)
            .map(|step| Place::of_step(index, Some(step.id().clone())))
    };
    Some(match dice.below(16) {
        0 | 1 => document.add_step(dice.below(steps + 1), dice.pick(&frames)?, &frame),
        2 => document.remove_step(&step_place(dice)?),
        3 => document.move_step(&step_place(dice)?, dice.below(steps)),
        4 => document.swap_step(&step_place(dice)?, dice.pick(&frames)?, &frame),
        5 | 6 => {
            let (expression, len) = dice.pick(&places.expressions)?;
            document.insert_link(expression, dice.below(len + 1), dice.pick(&methods)?, None)
        }
        7 => {
            let (expression, at, _) = dice.pick(&places.links)?.clone();
            document.swap_link(
                &expression.then(Segment::Link(at)),
                dice.pick(&methods)?,
                None,
            )
        }
        8 => {
            let (expression, at, _) = dice.pick(&places.links)?.clone();
            document.remove_link(&expression.then(Segment::Link(at)))
        }
        9 => {
            let (expression, at, len) = dice.pick(&places.links)?.clone();
            document.move_link(&expression.then(Segment::Link(at)), dice.below(len))
        }
        10 => {
            let (expression, _) = dice.pick(&places.expressions)?;
            let start = match dice.below(3) {
                0 => Start::Hole,
                1 => Start::Input("holidays".to_owned()),
                _ => Expression::field("b").ok()?.start().clone(),
            };
            document.set_start(expression, start)
        }
        11 => document.start_with(
            &dice.pick(&places.expressions)?.0,
            ["polars.when", "polars.element"][dice.below(2)],
        ),
        12 => {
            let list = dice.pick(&places.lists)?;
            if dice.below(2) == 0 {
                document.insert_test(list, 0)
            } else {
                document.insert_item(list, 0, Value::Expression(Expression::hole()))
            }
        }
        13 => {
            let list = dice.pick(&places.lists)?.clone();
            document.remove_item(&list.then(Segment::Item(0)))
        }
        14 => {
            let (call, entry) = dice.pick(&places.calls)?;
            let parameter = dice.pick(entry.parameters())?;
            document.add_option(call, parameter.name())
        }
        _ => {
            let (call, entry) = dice.pick(&places.calls)?;
            let parameter = dice.pick(entry.parameters())?;
            document.clear_argument(
                &call
                    .clone()
                    .then(Segment::Parameter(parameter.name().to_owned())),
            )
        }
    })
}

#[test]
fn every_document_a_run_of_chosen_edits_gives_reads_back_as_itself() {
    let mut accepted = 0;
    let mut refused = 0;
    for seed in 1..=8_u64 {
        let mut dice = Dice(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut document = Document::new(vec![]).expect("a document");
        for round in 0..150 {
            let Some(result) = some_edit(&mut dice, &document) else {
                // Nothing to edit yet: begin with a step.
                let (columns, tables) = (columns(), tables());
                let frame = StepFrame {
                    columns: &columns,
                    tables: &tables,
                };
                document = document
                    .add_step(0, "LazyFrame.select", &frame)
                    .expect("a select");
                continue;
            };
            match result {
                Ok(edited) => {
                    assert_reads_back(&edited, &format!("seed {seed}, edit {round}"));
                    document = edited;
                    accepted += 1;
                }
                Err(_) => refused += 1,
            }
        }
    }
    assert!(
        accepted > 300,
        "{accepted} edits were made, {refused} refused"
    );
}
