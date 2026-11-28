//! A number typed without a dtype, which Polars keeps open as `dyn int` or `dyn float`, through
//! every call that keeps it so, in every place it can stand, with Polars as the judge.
//!
//! Polars does not settle such a number where it stands alone: it gives the number the dtype it
//! falls back to where the schema is worked out, and keeps it open as it is passed on through the
//! calls that give back the dtype they follow, through `repeat`, through a condition whose values
//! are open and through a struct. A join's key that holds one is refused against the other
//! table's key where that is typed (`datatypes of join keys don't match`, the dtype `dyn float`),
//! though both give the dtype the number falls back to. A select, a filter and a sort key resolve
//! the number where they end, so none of them is refused for it.
//!
//! The grid of the keys crosses the number, at the edges of the dtypes it falls back to and past
//! them, with how it stands (alone, repeated, the values of a condition, a struct's field) and
//! the calls that follow it, and puts the chain as a join's key on either side against a key of
//! the number typed, against a key of it open too, and against a key of it typed on both sides,
//! which is the control. The walk has to tell a key that holds an open number, in the words of
//! `an-open-number-in-a-key`, where Polars refuses it, and to tell the control without them. The
//! grid of the other places puts the same chains in a select, a filter and a sort key, open and
//! typed, where Polars refuses none of them for the number.
//!
//! The calls are every method of the catalogue that is given nothing and gives back the dtype it
//! follows, for each dtype a number falls back to, by the tables; a few that are given an
//! operand or a value; and chains of two or three.

use polars::prelude::{DataFrame, NamedFrom, Series};
use serde_json::{Value, json};

use dageist_engine::allowed::{
    self, After, Chosen as Offered, Context, Outer, Place as Standing, Representative, Shape,
    Verdict,
};
use dageist_engine::catalog::{Applies, Entry, entries};

use crate::grid::literals::{self, Written};
use crate::grid::{Case, Connections, Grid, Kind, Known, Pattern, Question, Verdicts};
use crate::support::{call, chain, field, field_then, many, whole};

/// The dtype Polars gives a number typed without a dtype where it stands alone.
fn falls_back_to(number: &Written) -> &'static str {
    match number {
        Written::Integer(value) if i32::try_from(*value).is_ok() => "Int32",
        Written::Integer(_) => "Int64",
        _ => "Float64",
    }
}

/// The numbers a grid writes, at the edges of the dtypes they fall back to and past them.
fn numbers(normal: bool) -> Vec<Written> {
    if normal {
        // Each kind and each edge, a few of each.
        return vec![
            Written::Integer(0),
            Written::Integer(-1),
            Written::Integer(5),
            Written::Integer(i64::from(i32::MAX) + 1),
            Written::Integer(i64::MAX),
            Written::Float(-0.0),
            Written::Float(1.5),
            Written::Float(100.0),
            Written::Float(9.3e18),
            Written::Float(1e40),
            Written::Float(-1e40),
        ];
    }
    vec![
        Written::Integer(0),
        Written::Integer(1),
        Written::Integer(-1),
        Written::Integer(5),
        Written::Integer(i64::from(i32::MAX)),
        Written::Integer(i64::from(i32::MAX) + 1),
        Written::Integer(i64::from(i32::MIN) - 1),
        Written::Integer(i64::MAX),
        Written::Integer(i64::MIN),
        Written::Float(0.0),
        Written::Float(-0.0),
        Written::Float(1.5),
        Written::Float(-1.5),
        Written::Float(100.0),
        Written::Float(1e18),
        Written::Float(9.3e18),
        Written::Float(3.5e38),
        Written::Float(1e40),
        Written::Float(-1e40),
        Written::Float(f64::MAX),
    ]
}

/// How a number stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Form {
    /// The literal.
    Alone,
    /// `repeat` of the literal.
    Repeated,
    /// A condition whose values are the literal twice.
    BothValues,
    /// A condition whose values are the literal and one typed.
    OneValue,
    /// A struct of the literal.
    InAStruct,
    /// A struct of the absolute value of the literal.
    InAStructOfAbs,
    /// A struct of `repeat` of the literal.
    InAStructOfRepeat,
    /// A condition whose values are the struct of the literal twice.
    StructValues,
    /// A condition whose values are the counts of the literal twice.
    CountedValues,
}

impl Form {
    const fn label(self) -> &'static str {
        match self {
            Self::Alone => "the number",
            Self::Repeated => "repeat of the number",
            Self::BothValues => "a condition with both values open",
            Self::OneValue => "a condition with one value open",
            Self::InAStruct => "a struct of the number",
            Self::InAStructOfAbs => "a struct of the absolute number",
            Self::InAStructOfRepeat => "a struct of repeat of the number",
            Self::StructValues => "a condition with both values structs of the number",
            Self::CountedValues => "a condition with both values the counts of the number",
        }
    }
}

/// The chain a cell puts the number in: how it stands and the calls that follow it.
#[derive(Debug, Clone)]
struct Chain {
    form: Form,
    calls: Vec<Value>,
}

/// Whether the tables say that `entry`, given nothing, gives back the dtype it follows, for each
/// dtype a number typed without a dtype falls back to: a call that Polars may leave such a number
/// open through.
fn gives_back_the_dtype(entry: &'static Entry) -> bool {
    if std::env::var_os("SCRATCH_ALL").is_some() {
        return true;
    }
    [
        Representative::Int32,
        Representative::Int64,
        Representative::Float64,
    ]
    .into_iter()
    .all(|class| {
        let place = Standing::new(
            Context::step(Outer::Select),
            After::Value {
                dtype: class,
                shape: Shape::One,
            },
        );
        match allowed::offer(entry, &place, &Offered::default()) {
            Verdict::Allowed(allowance) => {
                allowance.result().and_then(allowed::ResultDtype::class) == Some(class)
            }
            Verdict::Refused(_) => false,
        }
    })
}

fn named(entry: &str) -> Value {
    call(entry, json!({}))
}

fn open_float() -> Value {
    chain(
        call("polars.lit", json!({ "value": { "float": 2.5 } })),
        vec![],
    )
}

fn typed_float() -> Value {
    chain(
        call(
            "polars.lit",
            json!({ "value": { "float": 2.5 }, "dtype": { "dtype": "Float64" } }),
        ),
        vec![],
    )
}

fn the_literal_field() -> Value {
    call(
        "Expr.struct.field",
        json!({ "name": { "field": "literal" } }),
    )
}

/// The calls a number is followed by, each with its label.
fn calls() -> Vec<(String, Vec<Value>)> {
    let mut calls: Vec<(String, Vec<Value>)> = vec![("no call".to_owned(), Vec::new())];
    for entry in entries() {
        if entry.applies() == Applies::Expression
            && entry
                .parameters()
                .iter()
                .all(|parameter| !parameter.is_needed())
            && gives_back_the_dtype(entry)
        {
            calls.push((
                entry.id().as_str().to_owned(),
                vec![named(entry.id().as_str())],
            ));
        }
    }
    let with = |entry: &str, name: &str, other: Value| call(entry, json!({ name: other }));
    calls.push((
        "add an open number".to_owned(),
        vec![with("Expr.add", "other", open_float())],
    ));
    calls.push((
        "add a typed number".to_owned(),
        vec![with("Expr.add", "other", typed_float())],
    ));
    calls.push((
        "add a column".to_owned(),
        vec![with("Expr.add", "other", field("x"))],
    ));
    calls.push((
        "eq_missing a column".to_owned(),
        vec![with("Expr.eq_missing", "other", field("x"))],
    ));
    calls.push((
        "fill_null with an open number".to_owned(),
        vec![with("Expr.fill_null", "value", open_float())],
    ));
    calls.push((
        "abs, abs".to_owned(),
        vec![named("Expr.abs"), named("Expr.abs")],
    ));
    calls.push((
        "abs, neg".to_owned(),
        vec![named("Expr.abs"), named("Expr.neg")],
    ));
    calls.push((
        "abs, value_counts, max".to_owned(),
        vec![
            named("Expr.abs"),
            named("Expr.value_counts"),
            named("Expr.max"),
        ],
    ));
    calls.push((
        "value_counts, field literal, max".to_owned(),
        vec![
            named("Expr.value_counts"),
            the_literal_field(),
            named("Expr.max"),
        ],
    ));
    calls
}

/// The calls that follow a struct of a number: what reads a struct, which the arithmetic of a
/// number does not.
fn struct_calls() -> Vec<(String, Vec<Value>)> {
    let one = |label: &str, calls: Vec<Value>| (label.to_owned(), calls);
    vec![
        one("no call", Vec::new()),
        one("first", vec![named("Expr.first")]),
        one("last", vec![named("Expr.last")]),
        one("max", vec![named("Expr.max")]),
        one("min", vec![named("Expr.min")]),
        one("json_encode", vec![named("Expr.struct.json_encode")]),
        one("field literal", vec![the_literal_field()]),
        one(
            "field literal, abs",
            vec![the_literal_field(), named("Expr.abs")],
        ),
        one(
            "value_counts, max",
            vec![named("Expr.value_counts"), named("Expr.max")],
        ),
        one(
            "value_counts, field literal, max",
            vec![
                named("Expr.value_counts"),
                the_literal_field(),
                named("Expr.max"),
            ],
        ),
    ]
}

/// The chains a grid puts in its places, each by its form and its calls. A repeated number stands
/// in a key only: Polars takes its length for the rows of the other table, which the walk cannot.
fn chains(repeated: bool) -> Vec<(String, Chain)> {
    let mut chains = Vec::new();
    for form in [
        Form::Alone,
        Form::Repeated,
        Form::BothValues,
        Form::OneValue,
    ] {
        if form == Form::Repeated && !repeated {
            continue;
        }
        for (label, calls) in calls() {
            chains.push((format!("{}, {label}", form.label()), Chain { form, calls }));
        }
    }
    for form in [
        Form::InAStruct,
        Form::InAStructOfAbs,
        Form::InAStructOfRepeat,
        Form::StructValues,
        Form::CountedValues,
    ] {
        for (label, calls) in struct_calls() {
            chains.push((format!("{}, {label}", form.label()), Chain { form, calls }));
        }
    }
    chains
}

#[derive(Default)]
struct Point {
    number: Option<Written>,
    chain: Option<Chain>,
    place: Option<Place>,
}

/// Where the chain stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    /// A join's left key is the chain of the number open, and its right key is the chain of it
    /// typed.
    LeftOpen,
    /// A join's right key is the chain of the number open, and its left key the chain of it typed.
    RightOpen,
    /// A join's keys are both the chain of the number open.
    BothOpen,
    /// A join's keys are both the chain of the number typed: the control.
    BothTyped,
    /// The chain is an item of a select, with the number open or typed.
    Select(bool),
    /// The chain is what a column is compared to in a filter.
    Filter(bool),
    /// The chain is a sort key.
    Sort(bool),
}

impl Place {
    const KEYS: [(&'static str, Self); 4] = [
        ("a left key open, the right typed", Self::LeftOpen),
        ("a right key open, the left typed", Self::RightOpen),
        ("both keys open", Self::BothOpen),
        ("both keys typed", Self::BothTyped),
    ];

    const ELSEWHERE: [(&'static str, Self); 6] = [
        ("a select, open", Self::Select(false)),
        ("a select, typed", Self::Select(true)),
        ("a filter, open", Self::Filter(false)),
        ("a filter, typed", Self::Filter(true)),
        ("a sort key, open", Self::Sort(false)),
        ("a sort key, typed", Self::Sort(true)),
    ];
}

/// The literal of the number, open or given the dtype it falls back to.
fn literal(number: &Written, typed: bool) -> Value {
    literals::call(number, typed.then(|| falls_back_to(number)))
}

fn true_flag() -> Value {
    chain(
        call("polars.lit", json!({ "value": { "flag": true } })),
        vec![],
    )
}

/// The chain the number makes, open or typed.
fn expression(number: &Written, chosen: &Chain, typed: bool) -> Value {
    let own = || chain(literal(number, typed), vec![]);
    let start = match chosen.form {
        Form::Alone => literal(number, typed),
        Form::Repeated => call("polars.repeat", json!({ "value": own(), "n": whole(3) })),
        Form::InAStruct => call("polars.struct", json!({ "exprs": many(vec![own()]) })),
        Form::InAStructOfRepeat => call(
            "polars.struct",
            json!({ "exprs": many(vec![chain(
                call("polars.repeat", json!({ "value": own(), "n": whole(3) })),
                vec![],
            )]) }),
        ),
        Form::InAStructOfAbs => call(
            "polars.struct",
            json!({ "exprs": many(vec![chain(literal(number, typed), vec![named("Expr.abs")])]) }),
        ),
        Form::BothValues | Form::OneValue | Form::StructValues | Form::CountedValues => {
            // A value of the condition, with the number open or typed as asked.
            let value = |typed: bool| {
                let number = chain(literal(number, typed), vec![]);
                match chosen.form {
                    Form::StructValues => chain(
                        call("polars.struct", json!({ "exprs": many(vec![number]) })),
                        vec![],
                    ),
                    Form::CountedValues => append(number, vec![named("Expr.value_counts")]),
                    _ => number,
                }
            };
            let second = if chosen.form == Form::OneValue {
                value(true)
            } else {
                value(typed)
            };
            let mut condition = chain(
                call(
                    "polars.when",
                    json!({ "predicates": many(vec![true_flag()]) }),
                ),
                vec![
                    call("When.then", json!({ "statement": value(typed) })),
                    call("Then.otherwise", json!({ "statement": second })),
                ],
            );
            condition["expression"]["then"]
                .as_array_mut()
                .expect("the links of an expression")
                .extend(chosen.calls.iter().cloned());
            return condition;
        }
    };
    chain(start, chosen.calls.clone())
}

/// The expression `expression` with the calls `more` after its own.
fn append(mut expression: Value, more: Vec<Value>) -> Value {
    expression["expression"]["then"]
        .as_array_mut()
        .expect("the links of an expression")
        .extend(more);
    expression
}

fn table() -> DataFrame {
    DataFrame::new(
        3,
        vec![Series::new("x".into(), [Some(1_i64), Some(2), None]).into()],
    )
    .expect("a table of one column")
}

fn other() -> DataFrame {
    DataFrame::new(
        3,
        vec![
            Series::new("rk".into(), [Some(1_i64), Some(2), None]).into(),
            Series::new("x".into(), [Some(1_i64), Some(2), None]).into(),
        ],
    )
    .expect("a table of two columns")
}

fn case_of(point: &Point) -> Result<Case, String> {
    let place = point.place.expect("a place");
    let number = point.number.as_ref().expect("a number");
    let chosen = point.chain.as_ref().expect("a chain");
    let connections = Connections::new().table("other", other());
    let key = |typed: bool| expression(number, chosen, typed);
    let (entry, arguments) = match place {
        Place::LeftOpen | Place::RightOpen | Place::BothOpen | Place::BothTyped => {
            let (left, right) = match place {
                Place::LeftOpen => (key(false), key(true)),
                Place::RightOpen => (key(true), key(false)),
                Place::BothOpen => (key(false), key(false)),
                _ => (key(true), key(true)),
            };
            (
                "LazyFrame.join",
                json!({
                    "other": { "table": "other" },
                    "left_on": many(vec![left]),
                    "right_on": many(vec![right]),
                }),
            )
        }
        Place::Select(typed) => (
            "LazyFrame.select",
            json!({ "exprs": many(vec![key(typed)]) }),
        ),
        Place::Filter(typed) => (
            "LazyFrame.filter",
            json!({ "predicates": many(vec![field_then(
                "x",
                vec![call("Expr.gt", json!({ "other": key(typed) }))],
            )]) }),
        ),
        Place::Sort(typed) => ("LazyFrame.sort", json!({ "by": many(vec![key(typed)]) })),
    };
    Case::of_step(entry, arguments, table(), connections)
}

fn grid(normal: bool, repeated: bool, places: &[(&'static str, Place)]) -> Grid<Point> {
    let values = numbers(normal)
        .into_iter()
        .map(|written| (written.label(), written))
        .collect();
    Grid::<Point>::new()
        .axis("number", values, |point, written| {
            point.number = Some(written.clone());
        })
        .axis("chain", chains(repeated), |point, chain| {
            point.chain = Some(chain.clone());
        })
        .axis(
            "place",
            places
                .iter()
                .map(|(label, place)| ((*label).to_owned(), *place))
                .collect(),
            |point, place| point.place = Some(*place),
        )
}

fn question() -> Question {
    Question::any_refusal().cannot_tell(|_| true)
}

fn keys(normal: bool) -> Verdicts {
    grid(normal, true, &Place::KEYS).judge(&question(), case_of)
}

fn elsewhere(normal: bool) -> Verdicts {
    grid(normal, false, &Place::ELSEWHERE).judge(&Question::any_refusal().with_dtypes(), case_of)
}

/// What the grid of the keys leaves, with the reason.
fn known_keys() -> Vec<Known> {
    vec![
        Known::new(
            Kind::WalkRefusesOnly,
            Pattern::new().ends_one_of(
                "chain",
                &[
                    ", Expr.backward_fill",
                    ", Expr.cum_max",
                    ", Expr.cum_min",
                    ", Expr.cum_sum",
                    ", Expr.diff",
                    ", Expr.forward_fill",
                    ", Expr.reverse",
                    ", Expr.shift",
                    ", Expr.sort",
                ],
            ),
            "a key made of one number is one value that Polars takes whole, whatever the calls it \
             holds, and the walk refuses the calls that are no elementwise ones, a running value, a \
             difference, a fill, a shift, a reverse and a sort (the limit of a key that is wholly \
             one value)",
        )
        .saying("must be elementwise"),
        Known::new(
            Kind::CannotTellPolarsAllows,
            Pattern::new().is("place", "both keys open"),
            "the walk judges each key apart, so where a key holds an open number it cannot tell the \
             key, or has no dtype for the number to go on from, though Polars matches the two keys \
             that hold the same open number, as `dyn float` is `dyn float` on both sides",
        ),
        Known::new(
            Kind::CannotTellPolarsAllows,
            Pattern::new()
                .starts("chain", "a condition with both values open")
                .ends_one_of(
                    "chain",
                    &[
                        ", add a column",
                        ", add a typed number",
                        ", eq_missing a column",
                        ", fill_null with an open number",
                    ],
                ),
            "the walk gives a condition whose values are all open numbers no dtype, so it tells \
             nothing of a call that follows it, and Polars adapts the open values to the typed \
             operand the call is given",
        ),
        Known::new(
            Kind::CannotTellPolarsAllows,
            Pattern::new()
                .starts("chain", "a condition with both values structs of the number")
                .ends_one_of("chain", &[", field literal", ", field literal, abs"]),
            "the walk does not carry the fields of a condition whose values are structs, so it \
             cannot tell a field read from it, though the field is as typed as the structs are",
        ),
        Known::new(
            Kind::CannotTellPolarsAllows,
            Pattern::new()
                .starts("chain", "the number, ")
                .ends_one_of("chain", &[", fill_null with an open number"]),
            "the walk gives no dtype to a number typed without a dtype that is filled with another, \
             so it tells nothing of the key, and Polars takes the key as it takes the number alone",
        ),
    ]
}

#[test]
fn a_number_typed_without_a_dtype_stays_open_through_the_calls_that_keep_it_as_a_key() {
    keys(true).assert_agree(&known_keys(), 8_000);
}

#[test]
fn a_number_typed_without_a_dtype_is_judged_as_before_in_a_select_a_filter_and_a_sort_key() {
    elsewhere(true).assert_agree(&[], 9_000);
}

#[test]
#[ignore = "the large grid, run on request"]
fn the_large_grid_of_an_open_number_in_a_key_is_taken_as_polars_does() {
    keys(false).assert_agree(&known_keys(), 16_000);
}

#[test]
#[ignore = "the large grid, run on request"]
fn the_large_grid_of_an_open_number_elsewhere_is_taken_as_polars_does() {
    elsewhere(false).assert_agree(&[], 19_000);
}
