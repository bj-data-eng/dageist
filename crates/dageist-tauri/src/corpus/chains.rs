//! The documents of chains: each entry that stands in an expression, at each kind of place it
//! stands at, and each kind of start.

use std::collections::{BTreeMap, HashMap, HashSet};

use dageist_engine::allowed::{
    self, After, Context, Outer, Place as OfferPlace, Representative, Shape,
};
use dageist_engine::catalog::{self, Applies};
use dageist_engine::document::{Document, Expression, Place, Segment, Start, Value};

use super::Case;
use super::documents::{StartPlan, call, field, filled, host_in, source_of, with_link, with_start};
use super::grid::{Cell, Grid};
use super::parameters::{given_parameters, with_all_parameters};
use super::reach::{Plan, reach};
use super::source::{LIST, VALUE, column_of};

/// The class a place is of, where it is of a value.
fn class_of(after: After) -> Representative {
    match after {
        After::Value { dtype, .. } => dtype,
        After::Start | After::Condition(_) => Representative::Int64,
    }
}

/// What the corpus plans from: the engine's offers, and the places each context reaches.
pub(super) struct Planner<'a> {
    pub grid: &'a Grid,
    reached: BTreeMap<usize, HashMap<(Representative, Shape), Plan>>,
}

impl<'a> Planner<'a> {
    pub(super) fn new(grid: &'a Grid) -> Self {
        Self {
            grid,
            reached: BTreeMap::new(),
        }
    }

    /// The places of the `context`th context a chain can be brought to.
    pub(super) fn reach(&mut self, context: usize) -> &HashMap<(Representative, Shape), Plan> {
        let grid = self.grid;
        self.reached
            .entry(context)
            .or_insert_with(|| reach(grid, grid.contexts[context]))
    }

    /// The document that holds the chain of `plan` in `context`, and where the chain is.
    pub(super) fn chain(&self, context: Context, plan: &Plan) -> (Document, Place) {
        let hosted = host_in(context, plan.root);
        let start = source_of(context.outer(), plan.root);
        let mut document = with_start(&hosted.document, &hosted.at, &start);
        for (at, (id, follows)) in plan.links.iter().enumerate() {
            document = with_link(
                &document,
                &hosted.at,
                at,
                id,
                &OfferPlace::new(context, *follows),
                class_of(*follows),
            );
        }
        (document, hosted.at)
    }

    /// The places, in the order of the grid, where the engine allows the entry numbered `entry`
    /// after a value of `shape` that a chain can be brought to: first those where an expression
    /// may end at it, then the others, at most `most` of each: the context, the class.
    pub(super) fn places(
        &mut self,
        entry: usize,
        shape: Shape,
        most: usize,
    ) -> Vec<(usize, Representative)> {
        let mut ending = Vec::new();
        let mut other = Vec::new();
        for context in 0..self.grid.contexts.len() {
            for class in Representative::ALL {
                let after = self.grid.after_at(After::Value {
                    dtype: class,
                    shape,
                });
                let Cell::Allowed { ends, .. } = self.grid.cell(context, after, entry) else {
                    continue;
                };
                if !self.reach(context).contains_key(&(class, shape)) {
                    continue;
                }
                let list = if ends { &mut ending } else { &mut other };
                if list.len() < most {
                    list.push((context, class));
                }
            }
        }
        ending.extend(other);
        ending
    }
}

/// How a context is written in a name.
fn context_words(context: Context) -> String {
    context.name()
}

/// The document where the function `id` starts the chain in the `context`th context.
fn function_starts(planner: &Planner<'_>, id: &'static str, context: usize) -> Case {
    let found = planner.grid.contexts[context];
    let hosted = host_in(found, Representative::Int64);
    let document = with_start(&hosted.document, &hosted.at, &StartPlan::Function(id));
    Case {
        name: format!("{id} starts a chain in {}", context_words(found)),
        document: filled(&document),
    }
}

/// The first context in which the engine allows the entry numbered `entry` to start a chain.
fn first_start(grid: &Grid, entry: usize) -> Option<usize> {
    let start = grid.after_at(After::Start);
    (0..grid.contexts.len())
        .find(|context| matches!(grid.cell(*context, start, entry), Cell::Allowed { .. }))
}

/// How many places a method is tried at, of each of the two kinds, before the walk is taken to
/// disagree with the tables everywhere.
const PLACES_TRIED: usize = 8;

/// How many methods a chain of the corpus has at most besides the one it is made for.
const MOST_LINKS: usize = 7;

/// What a chain of methods was made of: its context, where it starts, and each method with the
/// place it stands at.
struct Made {
    context: usize,
    root: Representative,
    links: Vec<(&'static str, After)>,
}

/// Which methods the walk says may stand where a chain puts them, by the entry and the shape of
/// the value it follows.
type Covered = HashSet<(&'static str, Shape)>;

/// What the documents hold so far: the methods that the walk allows at a shape of value, and the
/// parameters given to each entry.
#[derive(Default)]
pub(super) struct Coverage {
    pub methods: Covered,
    pub parameters: HashSet<(&'static str, &'static str)>,
}

/// Whether `entry` must be given something, as an expression or any other argument.
fn needs_arguments(entry: &catalog::Entry) -> bool {
    entry.parameters().iter().any(|parameter| {
        parameter.default() == catalog::ParameterDefault::Required || parameter.is_needed()
    })
}

/// The shape a place is of, where it is a value.
fn shape_of(after: After) -> Option<Shape> {
    match after {
        After::Value { shape, .. } => Some(shape),
        After::Start | After::Condition(_) => None,
    }
}

/// The chain that holds the method `target` at a place after a value of `class` and `shape` in
/// the `context`th context, and as many methods after it as the engine allows and no chain has
/// held yet, up to [`MOST_LINKS`].
fn long_chain(
    planner: &mut Planner<'_>,
    covered: &Covered,
    target: usize,
    context: usize,
    class: Representative,
    shape: Shape,
) -> Made {
    let plan = planner.reach(context)[&(class, shape)].clone();
    let mut links = plan.links.clone();
    let mut after = After::Value {
        dtype: class,
        shape,
    };
    let mut next = Some(target);
    let mut added = 0;
    let mut tried: HashSet<usize> = HashSet::new();
    while let Some(entry) = next.take() {
        let id = planner.grid.entries[entry].id().as_str();
        links.push((id, after));
        added += 1;
        tried.insert(entry);
        let here = planner.grid.after_at(after);
        let Cell::Allowed {
            class: Some(gives_class),
            shape: gives_shape,
            ..
        } = planner.grid.cell(context, here, entry)
        else {
            break;
        };
        after = After::Value {
            dtype: gives_class,
            shape: gives_shape,
        };
        if added > MOST_LINKS {
            break;
        }
        // The next method: the first the engine allows here that no chain holds yet.
        let at = planner.grid.after_at(after);
        next = (0..planner.grid.entries.len()).find(|candidate| {
            let held = planner.grid.entries[*candidate];
            held.applies() == Applies::Expression
                && !tried.contains(candidate)
                && !needs_arguments(held)
                && !covered.contains(&(held.id().as_str(), gives_shape))
                && matches!(
                    planner.grid.cell(context, at, *candidate),
                    Cell::Allowed { class: Some(_), .. }
                )
        });
    }
    Made {
        context,
        root: plan.root,
        links,
    }
}

/// The document of `made`.
fn made_document(planner: &Planner<'_>, made: &Made) -> (Document, Place) {
    let context = planner.grid.contexts[made.context];
    planner.chain(
        context,
        &Plan {
            root: made.root,
            links: made.links.clone(),
        },
    )
}

/// The documents of every entry that stands in an expression: each method after each shape of
/// value that the engine allows it after and a chain can be brought to, in chains that hold as
/// many methods as they can; each function where it starts a chain.
pub(super) fn entry_cases(
    planner: &mut Planner<'_>,
    only: Option<&str>,
    coverage: &mut Coverage,
) -> (Vec<Case>, Vec<Case>) {
    let mut cases = Vec::new();
    let mut functions = Vec::new();
    let mut attempted: Covered = HashSet::new();
    let entries = planner.grid.entries.clone();
    for (number, entry) in entries.iter().enumerate() {
        let id = entry.id().as_str();
        if only.is_some_and(|only| only != id) {
            continue;
        }
        match entry.applies() {
            Applies::Expression => {
                for shape in Shape::ALL {
                    if coverage.methods.contains(&(id, shape)) || attempted.contains(&(id, shape)) {
                        continue;
                    }
                    attempted.insert((id, shape));
                    let mut chosen = None;
                    for (context, class) in planner.places(number, shape, PLACES_TRIED) {
                        let made =
                            long_chain(planner, &coverage.methods, number, context, class, shape);
                        let (document, at) = made_document(planner, &made);
                        let document = filled(&document);
                        let held = standings(&document, &made);
                        // The method the chain is for stands where the walk says it does.
                        let allows =
                            made.links
                                .iter()
                                .zip(&held)
                                .any(|((link, _), (follows, allowed))| {
                                    *link == id && *follows == Some(shape) && *allowed
                                });
                        let first = chosen.is_none();
                        if allows || first {
                            chosen = Some((made, document, at, held, context, class));
                        }
                        if allows {
                            break;
                        }
                    }
                    let Some((made, document, at, held, context, class)) = chosen else {
                        continue;
                    };
                    let outer = planner.grid.contexts[context].outer();
                    let allowed_before: Vec<bool> =
                        held.iter().map(|(_, allowed)| *allowed).collect();
                    let document = fold_parameters(
                        document,
                        &at,
                        &made,
                        &allowed_before,
                        outer,
                        &mut coverage.parameters,
                    );
                    // What a parameter given to a method does to what follows it is the walk's to
                    // say, so what the chain holds is read from the finished document.
                    for ((link, _), (follows, allowed)) in
                        made.links.iter().zip(standings(&document, &made))
                    {
                        if let (true, Some(follows)) = (allowed, follows) {
                            coverage.methods.insert((link, follows));
                        }
                    }
                    cases.push(Case {
                        name: format!(
                            "{id} after {} {} in {}",
                            class.name(),
                            shape.name(),
                            context_words(planner.grid.contexts[context])
                        ),
                        document,
                    });
                }
            }
            Applies::Function => {
                if let Some(context) = first_start(planner.grid, number)
                    && !["polars.col", "polars.lit"].contains(&id)
                {
                    functions.push(function_starts(planner, id, context));
                }
            }
            Applies::Condition => {
                if id == "polars.when"
                    && let Some(context) = first_start(planner.grid, number)
                {
                    functions.push(function_starts(planner, id, context));
                }
            }
            Applies::Frame => {}
        }
    }
    (cases, functions)
}

/// The document with every parameter the engine offers given to each method of `made`, where
/// that leaves the walk allowing the methods it allowed before, and the parameters given noted.
fn fold_parameters(
    mut document: Document,
    at: &Place,
    made: &Made,
    allowed_before: &[bool],
    outer: Outer,
    given: &mut HashSet<(&'static str, &'static str)>,
) -> Document {
    let outcome = outcome_word(&document);
    for (index, (id, _)) in made.links.iter().enumerate() {
        let link = at
            .clone()
            .then(dageist_engine::document::Segment::Link(index));
        let candidate = with_all_parameters(&document, &link, outer);
        if allowed_links(&candidate, made) == allowed_before && outcome_word(&candidate) == outcome
        {
            for parameter in given_parameters(&candidate, &link) {
                given.insert((id, parameter));
            }
            document = candidate;
        }
    }
    document
}

/// What running the first step of `document` came to, in a word: valid, refused for a reason of
/// the document, or incomplete.
pub(super) fn outcome_word(document: &Document) -> String {
    let reported = super::source::report(document);
    match reported
        .steps()
        .first()
        .map(dageist_engine::document::StepReport::outcome)
    {
        Some(dageist_engine::document::Outcome::Valid { .. }) => "valid".to_owned(),
        Some(dageist_engine::document::Outcome::Refused(refusal)) => {
            format!("refused {:?}", std::mem::discriminant(refusal.cause()))
        }
        Some(dageist_engine::document::Outcome::Incomplete { .. }) => "incomplete".to_owned(),
        Some(dageist_engine::document::Outcome::NotRun { .. }) | None => "not run".to_owned(),
    }
}

/// What the walk says of each method of `made` where `document` puts it: the shape of the value
/// it follows, and whether it allows the method there.
fn standings(document: &Document, made: &Made) -> Vec<(Option<Shape>, bool)> {
    let reported = super::source::report(document);
    let Some(step) = reported.steps().first() else {
        return vec![(None, false); made.links.len()];
    };
    // The chain is the expression whose walk has the most calls: its arguments are walked too.
    let walked = step
        .walk()
        .expressions()
        .iter()
        .max_by_key(|walked| walked.calls().len());
    (0..made.links.len())
        .map(|at| {
            let standing = walked.and_then(|walked| walked.calls().get(at + 1));
            (
                standing
                    .and_then(dageist_engine::document::Standing::follows)
                    .and_then(shape_of),
                standing
                    .and_then(dageist_engine::document::Standing::verdict)
                    .is_some_and(allowed::Verdict::is_allowed),
            )
        })
        .collect()
}

/// Whether the walk allows each method of `made` where `document` puts it.
fn allowed_links(document: &Document, made: &Made) -> Vec<bool> {
    standings(document, made)
        .into_iter()
        .map(|(_, allowed)| allowed)
        .collect()
}

/// The documents of the kinds of start besides a function: a field, a connected value or list,
/// a number, text or flag typed in, and nothing picked yet; each alone, and with a method after
/// it where the engine allows one.
pub(super) fn start_cases(planner: &mut Planner<'_>) -> Vec<Case> {
    let select = Context::step(Outer::Select);
    let context = planner.grid.context_at(select);
    let mut cases = Vec::new();
    let hosted = host_in(select, Representative::Int64);

    // The first method the engine allows after a value of a class and a shape.
    let first_method = |after: After| -> Option<&'static str> {
        let at = planner.grid.after_at(after);
        planner
            .grid
            .entries
            .iter()
            .enumerate()
            .find(|(number, entry)| {
                entry.applies() == Applies::Expression
                    && matches!(
                        planner.grid.cell(context, at, *number),
                        Cell::Allowed { .. }
                    )
            })
            .map(|(_, entry)| entry.id().as_str())
    };

    let starts: Vec<(&str, Start, After)> = vec![
        (
            "a field",
            Start::field(super::source::column_of(Representative::Int64)).expect("a field"),
            After::Value {
                dtype: Representative::Int64,
                shape: Shape::Many,
            },
        ),
        (
            "a connected value",
            Start::Input(VALUE.to_owned()),
            After::Value {
                dtype: Representative::Int64,
                shape: Shape::One,
            },
        ),
        (
            "a connected list",
            Start::Input(LIST.to_owned()),
            After::Value {
                dtype: Representative::String,
                shape: Shape::Many,
            },
        ),
        (
            "a whole number typed in",
            typed(Value::Integer(12)),
            After::Value {
                dtype: Representative::Int32,
                shape: Shape::One,
            },
        ),
        (
            "a number typed in",
            typed(Value::Float(1.5)),
            After::Value {
                dtype: Representative::Float64,
                shape: Shape::One,
            },
        ),
        (
            "text typed in",
            typed(Value::String("text".to_owned())),
            After::Value {
                dtype: Representative::String,
                shape: Shape::One,
            },
        ),
        (
            "a flag typed in",
            typed(Value::Flag(Some(true))),
            After::Value {
                dtype: Representative::Boolean,
                shape: Shape::One,
            },
        ),
    ];
    for (name, start, after) in starts {
        let alone = hosted
            .document
            .set_start(&hosted.at, start)
            .unwrap_or_else(|error| panic!("{name} does not start the chain: {error}"));
        cases.push(Case {
            name: format!("{name} alone"),
            document: filled(&alone),
        });
        if let Some(method) = first_method(after) {
            let followed = with_link(
                &alone,
                &hosted.at,
                0,
                method,
                &OfferPlace::new(select, after),
                class_of(after),
            );
            cases.push(Case {
                name: format!("{name} followed by {method}"),
                document: filled(&followed),
            });
        }
    }
    // Nothing picked yet: the step is incomplete, and nothing follows.
    cases.push(Case {
        name: "nothing picked to start the chain".to_owned(),
        document: hosted.document.clone(),
    });
    cases
}

/// The start that is a call of `polars.lit` that holds `value`.
fn typed(value: Value) -> Start {
    Start::Call(call("polars.lit", vec![("value", value)]))
}

/// The documents of conditions: a `when` with every link of a condition the catalogue has, in
/// the order they come to, and a condition that ends and is followed by a method.
pub(super) fn condition_cases(planner: &Planner<'_>) -> Vec<Case> {
    let select = Context::step(Outer::Select);
    let hosted = host_in(select, Representative::Int64);
    let boolean = column_of(Representative::Boolean);
    let whole = column_of(Representative::Int64);
    let tests = || Value::Many(vec![field(&boolean)]);
    let when = |id: &str| call(id, vec![("predicates", tests())]);
    let value = |id: &str| call(id, vec![("statement", field(&whole))]);
    let long = Expression::new(
        Start::Call(when("polars.when")),
        vec![
            value("When.then"),
            when("Then.when"),
            value("ChainedWhen.then"),
            when("ChainedThen.when"),
            value("ChainedWhen.then"),
            value("ChainedThen.otherwise"),
        ],
    )
    .expect("a condition with every link");
    let short = Expression::new(
        Start::Call(when("polars.when")),
        vec![value("When.then"), value("Then.otherwise")],
    )
    .expect("a condition");
    let put = |expression: Expression| {
        hosted
            .document
            .set_argument(&hosted.at, Value::Expression(expression))
            .expect("the condition is put in the step")
    };
    // Every parameter the engine offers of the `when` and of the links that start a further case.
    let every_parameter = |document: Document| {
        let mut document = with_all_parameters(
            &document,
            &hosted.at.clone().then(Segment::Start),
            Outer::Select,
        );
        for (at, link) in long.links().iter().enumerate() {
            if link.entry().id().as_str().ends_with(".when") {
                document = with_all_parameters(
                    &document,
                    &hosted.at.clone().then(Segment::Link(at)),
                    Outer::Select,
                );
            }
        }
        document
    };
    let after = After::Value {
        dtype: Representative::Int64,
        shape: Shape::Many,
    };
    let method = first_method(planner.grid, select, after);
    let mut ended = put(short.clone());
    if let Some(method) = method {
        ended = with_link(
            &ended,
            &hosted.at,
            short.links().len(),
            method,
            &OfferPlace::new(select, after),
            Representative::Int64,
        );
    }
    vec![
        Case {
            name: "a condition with every link of one".to_owned(),
            document: every_parameter(put(long.clone())),
        },
        Case {
            name: "a condition followed by a method".to_owned(),
            document: ended,
        },
    ]
}

/// The first method the engine allows in `context` after `after`.
fn first_method(grid: &Grid, context: Context, after: After) -> Option<&'static str> {
    let at = grid.after_at(after);
    let context = grid.context_at(context);
    grid.entries
        .iter()
        .enumerate()
        .find(|(number, entry)| {
            entry.applies() == Applies::Expression
                && !needs_arguments(entry)
                && matches!(grid.cell(context, at, *number), Cell::Allowed { .. })
        })
        .map(|(_, entry)| entry.id().as_str())
}

/// One document for each context: the chain starts there and goes no further, so that every
/// position that holds an expression, and every way inside one, has a document of its own.
pub(super) fn context_cases(planner: &Planner<'_>) -> Vec<Case> {
    planner
        .grid
        .contexts
        .iter()
        .map(|context| {
            let hosted = host_in(*context, Representative::Int64);
            let start = source_of(context.outer(), Representative::Int64);
            let document = with_start(&hosted.document, &hosted.at, &start);
            Case {
                name: format!("a chain starts in {}", context.name()),
                document: filled(&document),
            }
        })
        .collect()
}

/// The documents that give an entry the parameters no chain has given it: the method after a
/// value it is allowed after, or the function that starts a chain, with every parameter the
/// engine offers.
pub(super) fn parameter_cases(planner: &mut Planner<'_>, coverage: &mut Coverage) -> Vec<Case> {
    let mut cases = Vec::new();
    let entries = planner.grid.entries.clone();
    for (number, entry) in entries.iter().enumerate() {
        let id = entry.id().as_str();
        let missing: Vec<&'static str> = entry
            .parameters()
            .iter()
            .filter(|held| held.offered() && !coverage.parameters.contains(&(id, held.name())))
            .map(catalog::Parameter::name)
            .collect();
        if missing.is_empty() || entry.applies() == Applies::Condition && id != "polars.when" {
            continue;
        }
        let document = match entry.applies() {
            Applies::Expression => {
                let Some((context, class, shape)) = Shape::ALL.into_iter().find_map(|shape| {
                    planner
                        .places(number, shape, 1)
                        .first()
                        .map(|(context, class)| (*context, *class, shape))
                }) else {
                    continue;
                };
                let made = long_chain(
                    planner,
                    &Coverage::default().methods,
                    number,
                    context,
                    class,
                    shape,
                );
                // Only the method itself: what follows it in a long chain is not what is asked.
                let made = Made {
                    links: made
                        .links
                        .into_iter()
                        .take_while(|(link, _)| *link != id)
                        .chain([(
                            id,
                            After::Value {
                                dtype: class,
                                shape,
                            },
                        )])
                        .collect(),
                    ..made
                };
                let (document, at) = made_document(planner, &made);
                let call = at.then(Segment::Link(made.links.len() - 1));
                let outer = planner.grid.contexts[context].outer();
                with_all_parameters(&filled(&document), &call, outer)
            }
            Applies::Function | Applies::Condition => {
                let hosted = host_in(Context::step(Outer::Select), Representative::Int64);
                let start = match id {
                    "polars.col" => {
                        Start::field(column_of(Representative::Int64)).expect("a field")
                    }
                    "polars.lit" => typed(Value::Integer(12)),
                    _ => {
                        let Some(context) = first_start(planner.grid, number) else {
                            continue;
                        };
                        let hosted = host_in(planner.grid.contexts[context], Representative::Int64);
                        let document =
                            with_start(&hosted.document, &hosted.at, &StartPlan::Function(id));
                        let call = hosted.at.then(Segment::Start);
                        let outer = planner.grid.contexts[context].outer();
                        let document = with_all_parameters(&filled(&document), &call, outer);
                        for parameter in given_parameters(&document, &call) {
                            coverage.parameters.insert((id, parameter));
                        }
                        cases.push(Case {
                            name: format!("{id} given every parameter"),
                            document,
                        });
                        continue;
                    }
                };
                let document = hosted
                    .document
                    .set_start(&hosted.at, start)
                    .expect("the start is put in the step");
                let call = hosted.at.then(Segment::Start);
                with_all_parameters(&filled(&document), &call, Outer::Select)
            }
            Applies::Frame => continue,
        };
        for parameter in missing {
            coverage.parameters.insert((id, parameter));
        }
        cases.push(Case {
            name: format!("{id} given every parameter"),
            document,
        });
    }
    cases
}
