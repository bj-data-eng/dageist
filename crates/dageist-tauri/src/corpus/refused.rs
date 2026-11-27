//! The documents the engine refuses: each cause a step can be refused for, and each refusal the
//! tables give a call at a place that a chain can be brought to.

use dageist_engine::allowed::{
    After, Context, Literal, Outer, Place as OfferPlace, RefusalSource, Representative, Shape,
};
use dageist_engine::catalog::Applies;
use dageist_engine::document::{Document, Expression, Place, Segment, Start, Value};

use super::Case;
use super::chains::Planner;
use super::documents::{
    StartPlan, call, field, filled, hole, host_in, one_step, with_link, with_start,
};
use super::grid::Cell;
use super::source::{OTHER, column_of, right_column_of};

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
    // Polars accepts whole numbers raised to a power below nothing when it works out the schema, and
    // cannot run it on the rows, for it takes the exponent as an unsigned number: no table says so,
    // and the steps after it are not counted. A step that takes every 0th row is refused by a rule
    // and is no such step.
    let below_nothing = Expression::new(
        Start::Call(call("polars.lit", vec![("value", Value::Integer(-1))])),
        vec![],
    )
    .expect("a literal");
    let raised = Expression::new(
        Start::Call(call(
            "polars.col",
            vec![("name", Value::Columns(vec![whole.clone()]))],
        )),
        vec![call(
            "Expr.pow",
            vec![("exponent", Value::Expression(below_nothing))],
        )],
    )
    .expect("a chain");
    let failed_on_the_rows = one_step(
        "LazyFrame.select",
        vec![("exprs", Value::Many(vec![Value::Expression(raised)]))],
    );
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

/// A method of a select that the tables allow, given arguments that a rule of the walk, not of the
/// offers at a place, refuses: the chain brought to a value of `class` and `shape`, then `id`, with
/// each of `arguments` set on it. It gives the document, where the chain is and how many methods the
/// chain has.
fn method_chain(
    planner: &mut Planner<'_>,
    class: Representative,
    shape: Shape,
    id: &str,
    arguments: Vec<(&str, Value)>,
) -> (Document, Place, usize) {
    let grid = planner.grid;
    let context = grid.context_at(Context::step(Outer::Select));
    let plan = planner.reach(context)[&(class, shape)].clone();
    let (document, at) = planner.chain(grid.contexts[context], &plan);
    let mut document = with_link(
        &document,
        &at,
        plan.links.len(),
        id,
        &OfferPlace::new(
            grid.contexts[context],
            After::Value {
                dtype: class,
                shape,
            },
        ),
        class,
    );
    for (parameter, value) in arguments {
        document = document
            .set_argument(
                &at.clone()
                    .then(Segment::Link(plan.links.len()))
                    .then(Segment::Parameter(parameter.to_owned())),
                value,
            )
            .unwrap_or_else(|error| panic!("{id} is not given {parameter}: {error}"));
    }
    (filled(&document), at, plan.links.len() + 1)
}

/// The document of [`method_chain`].
fn method_given(
    planner: &mut Planner<'_>,
    class: Representative,
    shape: Shape,
    id: &str,
    arguments: Vec<(&str, Value)>,
) -> Document {
    method_chain(planner, class, shape, id, arguments).0
}

/// The expression of a number or text typed in.
fn typed(literal: &Literal) -> Value {
    Value::Expression(Expression::literal(literal).expect("a literal"))
}

/// The documents refused by a rule that the walk applies to what a call is given or to the keys of a
/// join, which the offers at a place do not give a call: a pattern, a duration and a time zone that
/// are not valid, a range that cannot be made, a key that is no elementwise, two pairs of keys
/// named alike, a window given a count of values, and a value looked up in a column of its own dtype.
pub(super) fn rule_cases(planner: &mut Planner<'_>) -> Vec<Case> {
    let whole = column_of(Representative::Int64);
    let text = |words: &str| typed(&Literal::String(words.to_owned()));
    let mut cases = vec![
        Case {
            name: "a pattern that is no regular expression".to_owned(),
            document: method_given(
                planner,
                Representative::String,
                Shape::Many,
                "Expr.str.contains",
                vec![("pattern", Value::Pattern("(".to_owned()))],
            ),
        },
        Case {
            name: "a duration that is no duration".to_owned(),
            document: method_given(
                planner,
                Representative::Datetime,
                Shape::Many,
                "Expr.dt.truncate",
                vec![("every", text("1x"))],
            ),
        },
        Case {
            name: "a time zone that is no time zone".to_owned(),
            document: method_given(
                planner,
                Representative::Datetime,
                Shape::Many,
                "Expr.dt.convert_time_zone",
                vec![("time_zone", Value::TimeZone(Some("Not/AZone".to_owned())))],
            ),
        },
        Case {
            name: "a window given a count of values".to_owned(),
            document: method_given(
                planner,
                Representative::Int64,
                Shape::Counted,
                "Expr.over",
                vec![(
                    "partition_by",
                    Value::Many(vec![field(&column_of(Representative::Int64))]),
                )],
            ),
        },
    ];

    // A column looked up in a column of its own dtype, which Polars deprecates: the values that
    // `is_in` looks up in are a list, or a collection imploded.
    cases.push(Case {
        name: "a value looked up in a column of its own dtype".to_owned(),
        document: method_given(
            planner,
            Representative::Int64,
            Shape::Many,
            "Expr.is_in",
            vec![("other", field(&whole))],
        ),
    });

    // A replacement of one old value by many new ones, which the rule asks to be as many as the old.
    cases.push(Case {
        name: "a replacement of one value by many".to_owned(),
        document: method_given(
            planner,
            Representative::Int64,
            Shape::Many,
            "Expr.replace",
            vec![("old", typed(&Literal::Integer(1))), ("new", field(&whole))],
        ),
    });

    // A moment of one zone beside one of another: a column of a zone, moved to another, compared
    // with the column of the first.
    let (moved, at, methods) = method_chain(
        planner,
        Representative::DatetimeZoned,
        Shape::Many,
        "Expr.dt.convert_time_zone",
        vec![("time_zone", Value::TimeZone(Some("Asia/Tokyo".to_owned())))],
    );
    let grid = planner.grid;
    let select = grid.contexts[grid.context_at(Context::step(Outer::Select))];
    let zoned = with_link(
        &moved,
        &at,
        methods,
        "Expr.eq",
        &OfferPlace::new(
            select,
            After::Value {
                dtype: Representative::DatetimeZoned,
                shape: Shape::Many,
            },
        ),
        Representative::DatetimeZoned,
    );
    let zoned = zoned
        .set_argument(
            &at.then(Segment::Link(methods))
                .then(Segment::Parameter("other".to_owned())),
            field(&column_of(Representative::DatetimeZoned)),
        )
        .expect("a column is compared with");
    cases.push(Case {
        name: "two moments of different zones".to_owned(),
        document: filled(&zoned),
    });

    // A range with a step of nothing cannot be made.
    let hosted = host_in(Context::step(Outer::Select), Representative::Int64);
    let range = with_start(
        &hosted.document,
        &hosted.at,
        &StartPlan::Function("polars.int_range"),
    );
    let range = range
        .set_argument(
            &hosted
                .at
                .clone()
                .then(Segment::Start)
                .then(Segment::Parameter("step".to_owned())),
            Value::Integer(0),
        )
        .expect("a step is given");
    cases.push(Case {
        name: "a range with a step of nothing".to_owned(),
        document: filled(&range),
    });

    // A key that holds an aggregate beside a column is not elementwise.
    let hosted = host_in(Context::step(Outer::JoinLeft), Representative::Int64);
    let sum = Expression::new(
        Start::field(whole.clone()).expect("a field"),
        vec![call("Expr.sum", vec![])],
    )
    .expect("a chain");
    let key = Expression::new(
        Start::field(whole.clone()).expect("a field"),
        vec![call("Expr.add", vec![("other", Value::Expression(sum))])],
    )
    .expect("a chain");
    cases.push(Case {
        name: "a join key that holds an aggregate beside a column".to_owned(),
        document: hosted
            .document
            .set_argument(&hosted.at, Value::Expression(key))
            .map(|document| filled(&document))
            .expect("a key is given"),
    });

    // A step that groups by a column and gives the count of its rows the name of that column gives
    // two columns of one name.
    let named = |text: &str| {
        Document::read(text).unwrap_or_else(|error| panic!("a document of the corpus: {error}"))
    };
    let key = |name: &str| {
        format!(
            r#"{{"entry":"LazyFrame.group_by","arguments":{{"by":{{"many":[{{"expression":{{"start":{{"entry":"polars.col","arguments":{{"name":{{"columns":["{name}"]}}}}}},"then":[]}}}}]}}}}}}"#
        )
    };
    let step = |call: &str, then: &str| {
        format!(r#"{{"version":1,"steps":[{{"id":"group_by","call":{call},"then":[{then}]}}]}}"#)
    };
    let having = |predicate: &str| {
        format!(
            r#"{{"entry":"LazyGroupBy.having","arguments":{{"predicates":{{"many":[{predicate}]}}}}}}"#
        )
    };
    let column = |name: &str, then: &str| {
        format!(
            r#"{{"expression":{{"start":{{"entry":"polars.col","arguments":{{"name":{{"columns":["{name}"]}}}}}},"then":[{then}]}}}}"#
        )
    };
    let len =
        format!(r#"{{"entry":"LazyGroupBy.len","arguments":{{"name":{{"string":"{whole}"}}}}}}"#);
    cases.push(Case {
        name: "a group by that gives two columns of one name".to_owned(),
        document: named(&step(&key(&whole), &len)),
    });
    // The values of a group are no one value to keep the group by.
    let boolean = column_of(Representative::Boolean);
    let finishing = r#"{"entry":"LazyGroupBy.len","arguments":{}}"#;
    cases.push(Case {
        name: "a having that gives the values of each group".to_owned(),
        document: named(&step(
            &key(&whole),
            &format!("{},{finishing}", having(&column(&boolean, ""))),
        )),
    });
    // Text is no value Polars casts to a Boolean to keep a group.
    let first = r#"{"entry":"Expr.first","arguments":{}}"#;
    cases.push(Case {
        name: "a having that gives text".to_owned(),
        document: named(&step(
            &key(&whole),
            &format!(
                "{},{finishing}",
                having(&column(&column_of(Representative::String), first))
            ),
        )),
    });

    // Two keys of each side that read the same columns pair the same two names twice.
    let right = right_column_of(Representative::Int64);
    cases.push(Case {
        name: "a join with two pairs of keys named alike".to_owned(),
        document: one_step(
            "LazyFrame.join",
            vec![
                ("other", Value::Table(OTHER.to_owned())),
                ("left_on", Value::Many(vec![field(&whole), field(&whole)])),
                ("right_on", Value::Many(vec![field(&right), field(&right)])),
            ],
        ),
    });
    cases
}
