//! The documents the engine refuses: each cause a step can be refused for, and each refusal the
//! tables give a call at a place that a chain can be brought to.

use dageist_engine::allowed::{After, Place as OfferPlace, RefusalSource, Representative, Shape};
use dageist_engine::catalog::Applies;
use dageist_engine::document::{Document, Start, Value};

use super::Case;
use super::chains::Planner;
use super::documents::{StartPlan, field, filled, hole, host_in, one_step, with_link, with_start};
use super::grid::Cell;
use super::source::{OTHER, column_of};

/// The documents of the causes that no call at a place gives: a table that is not connected, an
/// input that is no value or list, a step with something still to be picked, one that Polars accepts
/// and cannot run on the rows, and the step that follows each step that is refused or fails.
pub(super) fn cause_cases() -> Vec<Case> {
    let whole = column_of(Representative::Int64);
    let unknown_table = one_step(
        "LazyFrame.join",
        vec![
            ("other", Value::Table("missing".to_owned())),
            ("left_on", Value::Many(vec![field(&whole)])),
            ("right_on", Value::Many(vec![field(&whole)])),
        ],
    );
    // The connected table is no value or list, so an expression cannot start from it.
    let unknown_input = one_step(
        "LazyFrame.select",
        vec![(
            "exprs",
            Value::Many(vec![Value::Expression(
                dageist_engine::document::Expression::new(Start::Input(OTHER.to_owned()), vec![])
                    .expect("a chain"),
            )]),
        )],
    );
    // Polars accepts a step that takes every row of a zero, and cannot run it on the rows: no table
    // says so, and the steps after it are not counted.
    let failed_on_the_rows = one_step("LazyFrame.gather_every", vec![("n", Value::Integer(0))]);
    let mut after_failure = failed_on_the_rows.steps().to_vec();
    after_failure.extend(one_step("LazyFrame.head", vec![]).steps().to_vec());
    let after_failure = Document::new(after_failure).expect("two steps");
    let still_to_pick = one_step(
        "LazyFrame.select",
        vec![("exprs", Value::Many(vec![hole()]))],
    );
    // A step is not run after one that was refused.
    let mut steps = unknown_table.steps().to_vec();
    steps.extend(one_step("LazyFrame.head", vec![]).steps().to_vec());
    let not_run = Document::new(steps).expect("two steps");
    vec![
        Case {
            name: "a table that is not connected".to_owned(),
            document: unknown_table,
        },
        Case {
            name: "a step after one that was refused".to_owned(),
            document: not_run,
        },
        Case {
            name: "an input that is a table".to_owned(),
            document: unknown_input,
        },
        Case {
            name: "a step Polars accepts and cannot run on the rows".to_owned(),
            document: failed_on_the_rows,
        },
        Case {
            name: "a step after one that failed on the rows".to_owned(),
            document: after_failure,
        },
        Case {
            name: "a step with something still to be picked".to_owned(),
            document: still_to_pick,
        },
    ]
}

/// How many places a refusal is tried at before it is given up.
const TRIES: usize = 12;

/// The documents of the refusals the tables give a call: the first place, in the order of the
/// grid, where a chain can be brought to and the walk gives the call the same refusal, for each
/// rule and each stage of Polars that refuses one.
pub(super) fn call_cases(planner: &mut Planner<'_>) -> Vec<Case> {
    let grid = planner.grid;
    let mut sources: Vec<RefusalSource> = Vec::new();
    let mut found: Vec<(RefusalSource, usize, Representative, Shape, usize)> = Vec::new();
    let mut tried: Vec<usize> = Vec::new();
    for context in 0..grid.contexts.len() {
        for class in Representative::ALL {
            for shape in Shape::ALL {
                let after = grid.after_at(After::Value {
                    dtype: class,
                    shape,
                });
                for (number, entry) in grid.entries.iter().enumerate() {
                    if entry.applies() != Applies::Expression {
                        continue;
                    }
                    let Cell::Refused(source) = grid.cell(context, after, number) else {
                        continue;
                    };
                    if matches!(source, RefusalSource::Structure) {
                        continue;
                    }
                    if !sources.contains(&source) {
                        sources.push(source);
                        tried.push(0);
                    }
                    let at = sources
                        .iter()
                        .position(|held| *held == source)
                        .expect("noted");
                    if tried[at] < TRIES {
                        tried[at] += 1;
                        found.push((source, context, class, shape, number));
                    }
                }
            }
        }
    }
    let mut cases = Vec::new();
    let mut done: Vec<RefusalSource> = Vec::new();
    for (source, context, class, shape, number) in found {
        if done.contains(&source) {
            continue;
        }
        if !planner.reach(context).contains_key(&(class, shape)) {
            continue;
        }
        let found_context = grid.contexts[context];
        let id = grid.entries[number].id().as_str();
        let plan = planner.reach(context)[&(class, shape)].clone();
        let (document, at) = planner.chain(found_context, &plan);
        let document = with_link(
            &document,
            &at,
            plan.links.len(),
            id,
            &OfferPlace::new(
                found_context,
                After::Value {
                    dtype: class,
                    shape,
                },
            ),
            class,
        );
        let document = super::documents::filled(&document);
        if !refused_the_same(&document, plan.links.len() + 1, source) {
            continue;
        }
        done.push(source);
        cases.push(Case {
            name: format!(
                "{id} refused by {} after {} {} in {}",
                words(source),
                class.name(),
                shape.name(),
                found_context.name()
            ),
            document,
        });
    }
    cases.extend(start_cases(planner, &mut done));
    cases
}

/// The documents of the refusals the tables give a function at the start of a chain: the first
/// context, in the order of the grid, where the walk gives the function the same refusal, for each
/// source not held yet.
fn start_cases(planner: &Planner<'_>, done: &mut Vec<RefusalSource>) -> Vec<Case> {
    let grid = planner.grid;
    let start = grid.after_at(After::Start);
    let mut cases = Vec::new();
    for context in 0..grid.contexts.len() {
        for (number, entry) in grid.entries.iter().enumerate() {
            if !matches!(entry.applies(), Applies::Function | Applies::Condition) {
                continue;
            }
            let Cell::Refused(source) = grid.cell(context, start, number) else {
                continue;
            };
            if done.contains(&source) || matches!(source, RefusalSource::Structure) {
                continue;
            }
            let id = entry.id().as_str();
            let found = grid.contexts[context];
            let hosted = host_in(found, Representative::Int64);
            let document = filled(&with_start(
                &hosted.document,
                &hosted.at,
                &StartPlan::Function(id),
            ));
            if !refused_the_same(&document, 0, source) {
                continue;
            }
            done.push(source);
            cases.push(Case {
                name: format!(
                    "{id} refused by {} at the start in {}",
                    words(source),
                    found.name()
                ),
                document,
            });
        }
    }
    cases
}

/// What a source of a refusal is called in a name.
fn words(source: RefusalSource) -> String {
    match source {
        RefusalSource::Polars(stage) => format!("polars at the {} stage", stage.name()),
        RefusalSource::Rule(rule) => format!("the rule {}", rule.name()),
        RefusalSource::Structure => "the shape of the expression".to_owned(),
    }
}

/// Whether the walk gives the call numbered `index` of the longest expression of `document` the
/// refusal `source`.
fn refused_the_same(document: &Document, index: usize, source: RefusalSource) -> bool {
    let reported = super::source::report(document);
    let Some(step) = reported.steps().first() else {
        return false;
    };
    step.walk()
        .expressions()
        .iter()
        .max_by_key(|walked| walked.calls().len())
        .and_then(|walked| walked.calls().get(index))
        .and_then(dageist_engine::document::Standing::verdict)
        .and_then(dageist_engine::allowed::Verdict::refusal)
        .is_some_and(|refusal| refusal.source() == source)
}
