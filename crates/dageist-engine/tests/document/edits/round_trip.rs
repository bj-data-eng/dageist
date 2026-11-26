//! Every edit gives a document that the reader reads back as the same
//! document. The first tests make each start of the catalogue; the last makes
//! many edits one after another, chosen by a generator with a fixed seed, over
//! every place the documents it makes have, and checks each document that an
//! edit gives: that it reads back, and that the edit did what it is for.

use dageist_engine::catalog::{self, Applies, Entry, Kind, Shape, StepFrame, ValueType, Variadic};
use dageist_engine::document::{
    Call, Document, EditError, Expression, Place, Segment, Start, Value,
};

use super::{json_of, parameter, step_at};

fn columns() -> Vec<String> {
    vec!["a".to_owned(), "b".to_owned()]
}

fn tables() -> Vec<String> {
    vec!["documents".to_owned()]
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
    /// Each pairs, with how many entries it has.
    pairs: Vec<(Place, usize)>,
    /// Each argument, and each item, key and value inside one.
    values: Vec<Place>,
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
    places.values.push(place.clone());
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
            places.pairs.push((place.clone(), pairs.len()));
            for (at, (key, entry)) in pairs.iter().enumerate() {
                value_places(key, &place.clone().then(Segment::Key(at)), places);
                value_places(entry, &place.clone().then(Segment::Value(at)), places);
            }
        }
        _ => {}
    }
}

/// What is at a place of a document.
enum Node<'a> {
    Call(&'a Call),
    Value(&'a Value),
}

/// Follows `place` through `document` by the meaning of its segments, as nothing of the engine's
/// edits does.
fn node_at<'a>(document: &'a Document, place: &Place) -> Option<Node<'a>> {
    let mut node = Node::Call(document.steps().get(place.step_index()?)?.call());
    for segment in place.path() {
        node = match (node, segment) {
            (Node::Call(call), Segment::Parameter(name)) => Node::Value(call.argument(name)?),
            (Node::Value(Value::Many(items)), Segment::Item(at)) => Node::Value(items.get(*at)?),
            (Node::Value(Value::Pairs(pairs)), Segment::Key(at)) => Node::Value(&pairs.get(*at)?.0),
            (Node::Value(Value::Pairs(pairs)), Segment::Value(at)) => {
                Node::Value(&pairs.get(*at)?.1)
            }
            (Node::Value(Value::Expression(expression)), Segment::Start) => {
                let Start::Call(start) = expression.start() else {
                    return None;
                };
                Node::Call(start)
            }
            (Node::Value(Value::Expression(expression)), Segment::Link(at)) => {
                Node::Call(expression.links().get(*at)?)
            }
            _ => return None,
        };
    }
    Some(node)
}

fn value_at<'a>(document: &'a Document, place: &Place) -> Option<&'a Value> {
    match node_at(document, place)? {
        Node::Value(value) => Some(value),
        Node::Call(_) => None,
    }
}

fn expression_at<'a>(document: &'a Document, place: &Place) -> Option<&'a Expression> {
    match value_at(document, place)? {
        Value::Expression(expression) => Some(expression),
        _ => None,
    }
}

/// The entries of the links of the expression at `place`, in order.
fn links_of(document: &Document, place: &Place) -> Vec<&'static str> {
    expression_at(document, place)
        .map(|expression| {
            expression
                .links()
                .iter()
                .map(|link| link.entry().id().as_str())
                .collect()
        })
        .unwrap_or_default()
}

/// The items of the list at `place`.
fn items_of(document: &Document, place: &Place) -> Vec<Value> {
    match value_at(document, place) {
        Some(Value::Many(items)) => items.clone(),
        _ => Vec::new(),
    }
}

type Json = serde_json::Value;

/// Follows `place` through the JSON of a document by what the document format says each segment
/// is, as nothing of the engine's edits does.
fn json_node<'a>(json: &'a mut Json, place: &Place) -> Option<&'a mut Json> {
    let mut node = json
        .get_mut("steps")?
        .get_mut(place.step_index()?)?
        .get_mut("call")?;
    let mut in_call = true;
    for segment in place.path() {
        node = match (in_call, segment) {
            (true, Segment::Parameter(name)) => {
                in_call = false;
                node.get_mut("arguments")?.get_mut(name.as_str())?
            }
            (false, Segment::Item(at)) => node.get_mut("many")?.get_mut(*at)?,
            (false, Segment::Key(at)) => node.get_mut("pairs")?.get_mut(*at)?.get_mut(0)?,
            (false, Segment::Value(at)) => node.get_mut("pairs")?.get_mut(*at)?.get_mut(1)?,
            (false, Segment::Start) => {
                in_call = true;
                node.get_mut("expression")?.get_mut("start")?
            }
            (false, Segment::Link(at)) => {
                in_call = true;
                node.get_mut("expression")?.get_mut("then")?.get_mut(*at)?
            }
            _ => return None,
        };
    }
    Some(node)
}

fn array_in<'a>(node: &'a mut Json, keys: &[&str]) -> Option<&'a mut Vec<Json>> {
    let mut node = node;
    for key in keys {
        node = node.get_mut(*key)?;
    }
    node.as_array_mut()
}

/// The place of the call or value that holds the one at `place`.
fn parent_of(place: &Place) -> Place {
    let path = place.path();
    let kept = path.len().saturating_sub(1);
    path[..kept].iter().fold(
        Place::of_step(place.step_index().unwrap_or_default(), None),
        |parent, segment| parent.then(segment.clone()),
    )
}

/// How many keys `call` has for the parameters that hold a value for each of them: the items of
/// the parameter `of` and of the starred parameter after it, counted from the arguments given.
fn key_count(call: &Call, of: &str) -> usize {
    let items = |name: &str| match call.argument(name) {
        Some(Value::Many(items)) => items.len(),
        Some(_) => 1,
        None => 0,
    };
    let parameters = call.entry().parameters();
    let starred = parameters
        .iter()
        .position(|parameter| parameter.name() == of)
        .and_then(|at| parameters.get(at + 1))
        .filter(|next| next.variadic() == Some(Variadic::Args));
    items(of) + starred.map_or(0, |next| items(next.name()))
}

/// Takes the values that a call keeps for each key out of both documents' JSON when the edit changed
/// how many keys the call nearest to `place` has, since the edit then changes those values too,
/// which have tests of their own. When it changed no key, everything is compared.
fn mask_per_key_values(
    before: &Document,
    after: &Document,
    place: &Place,
    expected: &mut Json,
    actual: &mut Json,
) {
    let path = place.path();
    let nearest = (0..=path.len()).rev().find_map(|kept| {
        let prefix = path[..kept].iter().fold(
            Place::of_step(place.step_index()?, None),
            |parent, segment| parent.then(segment.clone()),
        );
        match node_at(before, &prefix)? {
            Node::Call(call) => Some((prefix, call)),
            Node::Value(_) => None,
        }
    });
    let Some((call_place, was)) = nearest else {
        return;
    };
    let Some(Node::Call(now)) = node_at(after, &call_place) else {
        return;
    };
    for follower in was.entry().parameters() {
        let Shape::PerItem(of) = follower.shape() else {
            continue;
        };
        if key_count(was, of) == key_count(now, of) {
            continue;
        }
        for json in [&mut *expected, &mut *actual] {
            if let Some(arguments) = json_node(json, &call_place)
                .and_then(|call| call.get_mut("arguments"))
                .and_then(Json::as_object_mut)
            {
                arguments.remove(follower.name());
            }
        }
    }
}

/// Asserts that `after` is `before` but for what is at `place`: everything else is as it was.
fn assert_only_replaced(before: &Document, after: &Document, place: &Place) {
    let mut expected = json_of(before);
    let mut actual = json_of(after);
    let new = json_node(&mut actual, place)
        .expect("the part that was set")
        .clone();
    if let Some(old) = json_node(&mut expected, place) {
        *old = new;
    } else if let Some(Segment::Parameter(name)) = place.path().last() {
        // A parameter that was not given before: what it was given is all there is to take out.
        json_node(&mut actual, &parent_of(place))
            .and_then(|call| call.get_mut("arguments"))
            .and_then(Json::as_object_mut)
            .expect("the arguments")
            .remove(name);
    }
    mask_per_key_values(before, after, place, &mut expected, &mut actual);
    assert_eq!(expected, actual, "more than {place} changed");
}

/// Asserts that `after` is `before` but for a parameter that is no longer given.
fn assert_only_cleared(before: &Document, after: &Document, place: &Place) {
    let mut expected = json_of(before);
    let mut actual = json_of(after);
    if let (Some(Segment::Parameter(name)), Some(call)) = (
        place.path().last(),
        json_node(&mut expected, &parent_of(place)),
    ) {
        call.get_mut("arguments")
            .and_then(Json::as_object_mut)
            .expect("the arguments")
            .remove(name);
    }
    mask_per_key_values(before, after, place, &mut expected, &mut actual);
    assert_eq!(expected, actual, "more than {place} changed");
}

/// Asserts that `after` is `before` but for the array at `keys` inside what is at `container`,
/// which `change` makes of the array there was, given the array there is.
fn assert_only_array_changed(
    before: &Document,
    after: &Document,
    container: &Place,
    keys: &[&str],
    change: &dyn Fn(&mut Vec<Json>, &[Json]),
) {
    let mut expected = json_of(before);
    let mut actual = json_of(after);
    let theirs = array_in(
        json_node(&mut actual, container).expect("the container"),
        keys,
    )
    .expect("the array")
    .clone();
    let ours = array_in(
        json_node(&mut expected, container).expect("the container"),
        keys,
    )
    .expect("the array");
    change(ours, &theirs);
    mask_per_key_values(before, after, container, &mut expected, &mut actual);
    assert_eq!(
        expected, actual,
        "more than the array at {container} changed"
    );
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

/// What an edit gave, and a check of what it did, for the document it gave.
struct Attempt {
    result: Result<Document, EditError>,
    effect: Box<dyn Fn(&Document)>,
}

fn attempt(result: Result<Document, EditError>, effect: impl Fn(&Document) + 'static) -> Attempt {
    Attempt {
        result,
        effect: Box::new(effect),
    }
}

/// An argument for `parameter` that its kind takes, or `None` where there is none to make.
fn value_for(parameter: &catalog::Parameter) -> Option<Value> {
    let one = match parameter.kind() {
        Kind::Expression(_) => Value::Expression(Expression::hole()),
        Kind::Column => Value::Columns(vec!["a".to_owned()]),
        Kind::Field => Value::Field("a".to_owned()),
        Kind::Flag => Value::Flag(Some(true)),
        Kind::Choice(words) => Value::Choice((*words.first()?).to_owned()),
        Kind::Value(ValueType::Integer) => Value::Integer(3),
        Kind::Value(ValueType::Float) => Value::Float(1.5),
        Kind::Value(_) => Value::String("x".to_owned()),
        Kind::Dtype => Value::Dtype("Int64".parse().ok()?),
        Kind::Duration => Value::Duration("1d".to_owned()),
        Kind::Pattern => Value::Pattern("[a-z]".to_owned()),
        Kind::TimeZone => Value::TimeZone(Some("UTC".to_owned())),
        Kind::Table => Value::Table("documents".to_owned()),
        Kind::Path => Value::Path("documents.parquet".to_owned()),
        Kind::Pairs(..) => return Some(Value::Pairs(Vec::new())),
    };
    Some(match parameter.shape() {
        Shape::Many => Value::Many(vec![one]),
        _ => one,
    })
}

/// How many kinds of edit `edit_of_kind` makes.
const KINDS: usize = 23;

/// One edit of the kind `kind`, with what it is made on chosen by `dice`, made on `document`, or
/// `None` where there was nothing to choose it from.
fn edit_of_kind(dice: &mut Dice, document: &Document, kind: usize) -> Option<Attempt> {
    let (columns, tables) = (columns(), tables());
    let frame = StepFrame {
        columns: &columns,
        tables: &tables,
    };
    let places = places_of(document);
    let frames: Vec<&'static str> = catalog::entries()
        .iter()
        .filter(|entry| catalog::start_step(entry, &frame).is_some())
        .map(|entry| entry.id().as_str())
        .collect();
    let methods: Vec<&'static str> = catalog::entries()
        .iter()
        .filter(|entry| entry.applies() != Applies::Frame && entry.applies() != Applies::Function)
        .map(|entry| entry.id().as_str())
        .collect();
    let steps = document.steps().len();
    let before = document.clone();
    let step_place = |dice: &mut Dice| {
        let index = dice.below(steps);
        document
            .steps()
            .get(index)
            .map(|step| (index, Place::of_step(index, Some(step.id().clone()))))
    };
    match kind {
        0 | 1 => {
            let at = dice.below(steps + 1);
            let id = *dice.pick(&frames)?;
            Some(attempt(document.add_step(at, id, &frame), move |after| {
                assert_eq!(after.steps().len(), before.steps().len() + 1);
                assert_eq!(after.steps()[at].call().entry().id().as_str(), id);
                assert_eq!(after.steps()[..at], before.steps()[..at]);
                assert_eq!(after.steps()[at + 1..], before.steps()[at..]);
            }))
        }
        2 => {
            let (index, place) = step_place(dice)?;
            Some(attempt(document.remove_step(&place), move |after| {
                let mut expected = before.steps().to_vec();
                expected.remove(index);
                assert_eq!(after.steps(), expected);
            }))
        }
        3 => {
            let (index, place) = step_place(dice)?;
            let to = dice.below(steps);
            Some(attempt(document.move_step(&place, to), move |after| {
                let mut expected = before.steps().to_vec();
                let moved = expected.remove(index);
                expected.insert(to, moved);
                assert_eq!(after.steps(), expected);
            }))
        }
        4 => {
            let (index, place) = step_place(dice)?;
            let id = *dice.pick(&frames)?;
            Some(attempt(
                document.swap_step(&place, id, &frame),
                move |after| {
                    assert_eq!(after.steps().len(), before.steps().len());
                    assert_eq!(after.steps()[index].id(), before.steps()[index].id());
                    assert_eq!(after.steps()[index].call().entry().id().as_str(), id);
                },
            ))
        }
        5 | 6 => {
            let (expression, len) = dice.pick(&places.expressions)?.clone();
            let at = dice.below(len + 1);
            let id = *dice.pick(&methods)?;
            Some(attempt(
                document.insert_link(&expression, at, id, None),
                move |after| {
                    let mut expected = links_of(&before, &expression);
                    expected.insert(at, id);
                    assert_eq!(links_of(after, &expression), expected);
                    assert_only_array_changed(
                        &before,
                        after,
                        &expression,
                        &["expression", "then"],
                        &|array, theirs| array.insert(at, theirs[at].clone()),
                    );
                },
            ))
        }
        7 => {
            let (expression, at, _) = dice.pick(&places.links)?.clone();
            let id = *dice.pick(&methods)?;
            Some(attempt(
                document.swap_link(&expression.clone().then(Segment::Link(at)), id, None),
                move |after| {
                    let mut expected = links_of(&before, &expression);
                    expected[at] = id;
                    assert_eq!(links_of(after, &expression), expected);
                    assert_only_replaced(
                        &before,
                        after,
                        &expression.clone().then(Segment::Link(at)),
                    );
                },
            ))
        }
        8 => {
            let (expression, at, _) = dice.pick(&places.links)?.clone();
            Some(attempt(
                document.remove_link(&expression.clone().then(Segment::Link(at))),
                move |after| {
                    let mut expected = links_of(&before, &expression);
                    expected.remove(at);
                    assert_eq!(links_of(after, &expression), expected);
                    assert_only_array_changed(
                        &before,
                        after,
                        &expression,
                        &["expression", "then"],
                        &|array, _| {
                            array.remove(at);
                        },
                    );
                },
            ))
        }
        9 => {
            let (expression, at, len) = dice.pick(&places.links)?.clone();
            let to = dice.below(len);
            Some(attempt(
                document.move_link(&expression.clone().then(Segment::Link(at)), to),
                move |after| {
                    let mut expected = links_of(&before, &expression);
                    let moved = expected.remove(at);
                    expected.insert(to, moved);
                    assert_eq!(links_of(after, &expression), expected);
                    assert_only_array_changed(
                        &before,
                        after,
                        &expression,
                        &["expression", "then"],
                        &|array, _| {
                            let moved = array.remove(at);
                            array.insert(to, moved);
                        },
                    );
                },
            ))
        }
        10 => {
            let (expression, _) = dice.pick(&places.expressions)?.clone();
            let start = match dice.below(3) {
                0 => Start::Hole,
                1 => Start::Input("holidays".to_owned()),
                _ => Start::field("b").ok()?,
            };
            let given = start.clone();
            Some(attempt(
                document.set_start(&expression, start),
                move |after| {
                    let held = expression_at(after, &expression).expect("an expression");
                    assert_eq!(held.start(), &given);
                    let kept = expression_at(&before, &expression).expect("an expression");
                    assert_eq!(held.links(), kept.links());
                    assert_only_replaced(&before, after, &expression.clone().then(Segment::Start));
                },
            ))
        }
        11 => {
            let (expression, _) = dice.pick(&places.expressions)?.clone();
            let id = ["polars.when", "polars.element"][dice.below(2)];
            Some(attempt(
                document.start_with(&expression, id),
                move |after| {
                    let held = expression_at(after, &expression).expect("an expression");
                    let Start::Call(start) = held.start() else {
                        panic!("a call starts it");
                    };
                    assert_eq!(start.entry().id().as_str(), id);
                },
            ))
        }
        12 => {
            let list = dice.pick(&places.lists)?.clone();
            let at = dice.below(items_of(document, &list).len() + 1);
            let test = dice.below(2) == 0;
            let item = Value::Expression(Expression::hole());
            let result = if test {
                document.insert_test(&list, at)
            } else {
                document.insert_item(&list, at, item.clone())
            };
            Some(attempt(result, move |after| {
                let mut expected = items_of(&before, &list);
                let made = items_of(after, &list);
                assert_eq!(made.len(), expected.len() + 1);
                if test {
                    expected.insert(at, made[at].clone());
                } else {
                    expected.insert(at, item.clone());
                }
                assert_eq!(made, expected);
                assert_only_array_changed(&before, after, &list, &["many"], &|array, theirs| {
                    array.insert(at, theirs[at].clone());
                });
            }))
        }
        13 => {
            let list = dice.pick(&places.lists)?.clone();
            let at = dice.below(items_of(document, &list).len().max(1));
            Some(attempt(
                document.remove_item(&list.clone().then(Segment::Item(at))),
                move |after| {
                    let mut expected = items_of(&before, &list);
                    expected.remove(at);
                    // A per-item parameter that moves with the keys is not among the items.
                    assert_eq!(items_of(after, &list), expected);
                    assert_only_array_changed(&before, after, &list, &["many"], &|array, _| {
                        array.remove(at);
                    });
                },
            ))
        }
        14 => {
            let (call, entry) = dice.pick(&places.calls)?.clone();
            let parameter = dice.pick(entry.parameters())?.name();
            Some(attempt(
                document.add_option(&call, parameter),
                move |after| {
                    let Some(Node::Call(held)) = node_at(after, &call) else {
                        panic!("a call");
                    };
                    assert!(held.argument(parameter).is_some());
                    assert_only_replaced(
                        &before,
                        after,
                        &call.clone().then(parameter_segment(parameter)),
                    );
                },
            ))
        }
        15 => {
            let (call, entry) = dice.pick(&places.calls)?.clone();
            let parameter = dice.pick(entry.parameters())?.name();
            let place = call.then(parameter_segment(parameter));
            Some(attempt(document.clear_argument(&place), move |after| {
                assert!(value_at(after, &place).is_none());
                assert_only_cleared(&before, after, &place);
            }))
        }
        16 | 17 => {
            let place = dice.pick(&places.values)?.clone();
            let value = [
                Value::Expression(Expression::hole()),
                Value::Expression(Expression::input("holidays")),
                Value::Columns(vec!["b".to_owned()]),
                Value::Flag(Some(false)),
                Value::Integer(7),
            ][dice.below(5)]
            .clone();
            let given = value.clone();
            Some(attempt(
                document.set_argument(&place, value),
                move |after| {
                    assert_eq!(value_at(after, &place), Some(&given));
                    assert_only_replaced(&before, after, &place);
                },
            ))
        }
        18 | 19 => {
            let (call, entry) = dice.pick(&places.calls)?.clone();
            let parameter = dice.pick(entry.parameters())?;
            let value = value_for(parameter)?;
            let place = call.then(parameter_segment(parameter.name()));
            let given = value.clone();
            Some(attempt(
                document.set_argument(&place, value),
                move |after| {
                    assert_eq!(value_at(after, &place), Some(&given));
                    assert_only_replaced(&before, after, &place);
                },
            ))
        }
        20 => {
            let (pairs, len) = dice.pick(&places.pairs)?.clone();
            let at = dice.below(len + 1);
            let key = Value::Columns(vec!["b".to_owned()]);
            let value = Value::Expression(Expression::hole());
            let entry = (key.clone(), value.clone());
            Some(attempt(
                document.insert_entry(&pairs, at, key, value),
                move |after| {
                    let Some(Value::Pairs(held)) = value_at(after, &pairs) else {
                        panic!("pairs");
                    };
                    assert_eq!(held.len(), len + 1);
                    assert_eq!(held[at], entry);
                    assert_only_array_changed(
                        &before,
                        after,
                        &pairs,
                        &["pairs"],
                        &|array, theirs| {
                            array.insert(at, theirs[at].clone());
                        },
                    );
                },
            ))
        }
        21 => {
            let (pairs, len) = dice.pick(&places.pairs)?.clone();
            let at = dice.below(len.max(1));
            let part = if dice.below(2) == 0 {
                Segment::Key(at)
            } else {
                Segment::Value(at)
            };
            Some(attempt(
                document.remove_entry(&pairs.clone().then(part)),
                move |after| {
                    let (Some(Value::Pairs(old)), Some(Value::Pairs(held))) =
                        (value_at(&before, &pairs), value_at(after, &pairs))
                    else {
                        panic!("pairs");
                    };
                    let mut expected = old.clone();
                    expected.remove(at);
                    assert_eq!(held, &expected);
                    assert_only_array_changed(&before, after, &pairs, &["pairs"], &|array, _| {
                        array.remove(at);
                    });
                },
            ))
        }
        _ => {
            let keyed: Vec<&(Place, &'static Entry)> = places
                .calls
                .iter()
                .filter(|(_, entry)| {
                    entry
                        .parameters()
                        .iter()
                        .any(|parameter| matches!(parameter.shape(), Shape::PerItem(_)))
                })
                .collect();
            let (call, entry) = (*dice.pick(&keyed)?).clone();
            let parameter = entry
                .parameters()
                .iter()
                .find(|parameter| matches!(parameter.shape(), Shape::PerItem(_)))?;
            let item = dice.below(2);
            let flag = dice.below(2) == 0;
            let name = parameter.name();
            Some(attempt(
                document.set_for_item(&call, name, item, Value::Flag(Some(flag))),
                move |after| {
                    let Some(Value::Many(values)) =
                        value_at(after, &call.clone().then(parameter_segment(name)))
                    else {
                        panic!("a value for each item");
                    };
                    assert_eq!(values[item], Value::Flag(Some(flag)));
                    assert_only_replaced(
                        &before,
                        after,
                        &call.clone().then(parameter_segment(name)),
                    );
                },
            ))
        }
    }
}

/// One edit of a kind chosen by `dice`, and which kind it is.
fn some_edit(dice: &mut Dice, document: &Document) -> (usize, Option<Attempt>) {
    let kind = dice.below(KINDS);
    (kind, edit_of_kind(dice, document, kind))
}

fn parameter_segment(name: &str) -> Segment {
    Segment::Parameter(name.to_owned())
}

#[test]
fn every_document_a_run_of_chosen_edits_gives_reads_back_as_itself_and_is_what_the_edit_made() {
    let mut accepted = 0;
    let mut refused = 0;
    let mut accepted_of = [0_usize; KINDS];
    for seed in 1..=12_u64 {
        let mut dice = Dice(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut document = Document::new(vec![]).expect("a document");
        for round in 0..200 {
            let (kind, made) = some_edit(&mut dice, &document);
            let Some(attempt) = made else {
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
            match attempt.result {
                Ok(edited) => {
                    assert_reads_back(&edited, &format!("seed {seed}, edit {round}"));
                    (attempt.effect)(&edited);
                    document = edited;
                    accepted += 1;
                    accepted_of[kind] += 1;
                }
                Err(_) => refused += 1,
            }
        }
    }
    assert!(
        accepted > 1_000,
        "{accepted} edits were made, {refused} refused"
    );
    // Every kind of edit was made, and checked, at least a few times.
    assert!(
        accepted_of.iter().all(|made| *made >= 5),
        "edits made of each kind: {accepted_of:?}"
    );
}
