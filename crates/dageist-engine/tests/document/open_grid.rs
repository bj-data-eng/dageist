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
    Unknown, Verdict,
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
        // A whole number, one past Int32, a fraction and one past Float32's range.
        return vec![
            Written::Integer(5),
            Written::Integer(i64::from(i32::MAX) + 1),
            Written::Float(1.5),
            Written::Float(1e40),
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

/// What the tables say a call that is given nothing does with a number of each dtype a number
/// typed without a dtype falls back to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Does {
    /// It gives back the dtype it follows, for each of them, and as many values: a call Polars may
    /// leave such a number open through.
    Keeps,
    /// It gives back the dtype it follows, for each of them, and one value of the many it follows.
    Aggregates,
    /// It gives back the dtype it follows for some of them and another for the rest, and as many
    /// values: a call that turns whole numbers into numbers with a fraction, as a logarithm does.
    KeepsSome,
    /// It gives another dtype for each of them, for each of them allows.
    Changes,
}

fn what_it_does(entry: &'static Entry) -> Option<Does> {
    // `over` needs one of two parameters that neither is marked as needed, so it is given none.
    if entry.id().as_str() == "Expr.over" {
        return None;
    }
    // For each dtype the call allows: whether it gives that dtype back, whether it gives one value,
    // and whether what it gives is a number that a call that keeps the dtype can follow: a signed
    // whole number or a number with a fraction, and no text, flag, list, struct or unsigned count.
    let mut does = Vec::new();
    for class in [
        Representative::Int32,
        Representative::Int64,
        Representative::Float64,
    ] {
        let place = Standing::new(
            Context::step(Outer::Select),
            After::Value {
                dtype: class,
                shape: Shape::Many,
            },
        );
        if let Verdict::Allowed(allowance) = allowed::offer(entry, &place, &Offered::default()) {
            let given = allowance.result().and_then(allowed::ResultDtype::class);
            let number = given.is_some_and(|given| {
                matches!(
                    given,
                    Representative::Int8
                        | Representative::Int16
                        | Representative::Int32
                        | Representative::Int64
                        | Representative::Int128
                        | Representative::Float16
                        | Representative::Float32
                        | Representative::Float64
                )
            });
            does.push((
                given == Some(class),
                allowance.shape() == Shape::One,
                number,
            ));
        }
    }
    // A call that one of them refuses, as the bitwise calls refuse a number with a fraction, is
    // crossed with no number: the grid would hold what Polars does of a refusal, not of an open
    // number.
    if does.len() < 3 {
        return None;
    }
    let one = does.iter().all(|(_, one, _)| *one);
    let keeps = does.iter().all(|(keeps, _, _)| *keeps);
    let some = does.iter().any(|(keeps, _, _)| *keeps);
    let numbers = does.iter().all(|(_, _, number)| *number);
    match (keeps, some, numbers) {
        (true, _, _) if one => Some(Does::Aggregates),
        (true, _, _) => Some(Does::Keeps),
        (false, true, true) if !one => Some(Does::KeepsSome),
        (_, _, true) => Some(Does::Changes),
        _ => None,
    }
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

/// The calls of the catalogue that are given nothing, by what the tables say they do with a number.
fn catalogue_calls(wanted: Does) -> Vec<&'static str> {
    entries()
        .iter()
        .filter(|entry| {
            entry.applies() == Applies::Expression
                && entry
                    .parameters()
                    .iter()
                    .all(|parameter| !parameter.is_needed())
                && what_it_does(entry) == Some(wanted)
        })
        .map(|entry| entry.id().as_str())
        .collect()
}

/// The calls a number is followed by, each chain with its label: every call that keeps the dtype
/// it follows, every aggregate that does, and every call that changes it, alone; every pair of the
/// first two kinds; one of each kind of the first two, in both orders, with every call that changes
/// the dtype; every chain of three that a few of the first two kinds make; and a few given an
/// operand or a value.
fn calls() -> Vec<(String, Vec<Value>)> {
    let keeps = catalogue_calls(Does::Keeps);
    let aggregates = catalogue_calls(Does::Aggregates);
    let changes = catalogue_calls(Does::Changes);
    let some = catalogue_calls(Does::KeepsSome);
    let kept: Vec<&str> = keeps.iter().chain(&aggregates).copied().collect();
    // A few of the calls that keep the dtype, and a few of the aggregates, which stand for them
    // where a chain is longer than two.
    let few: Vec<&str> = ["Expr.abs", "Expr.neg", "Expr.max", "Expr.sum"]
        .into_iter()
        .filter(|name| kept.contains(name))
        .collect();
    let mut chains: Vec<Vec<&str>> = vec![Vec::new()];
    chains.extend(
        kept.iter()
            .chain(&changes)
            .chain(&some)
            .map(|name| vec![*name]),
    );
    for name in &some {
        for kind in &few {
            chains.push(vec![name, kind]);
            chains.push(vec![kind, name]);
        }
    }
    for first in &kept {
        for second in &kept {
            chains.push(vec![first, second]);
        }
    }
    for kind in &few {
        for changing in &changes {
            chains.push(vec![kind, changing]);
            chains.push(vec![changing, kind]);
        }
    }
    for first in &few {
        for second in &few {
            for third in &few {
                chains.push(vec![first, second, third]);
            }
        }
    }
    // A call that keeps the dtype, an aggregate that does, and then a call that changes it.
    for keeper in ["Expr.abs", "Expr.neg"] {
        for aggregate in ["Expr.max", "Expr.sum"] {
            for changing in &changes {
                chains.push(vec![keeper, aggregate, changing]);
            }
        }
    }
    let mut calls: Vec<(String, Vec<Value>)> = chains
        .into_iter()
        .map(|names| {
            let label = if names.is_empty() {
                "no call".to_owned()
            } else {
                names.join(", ")
            };
            (label, names.into_iter().map(named).collect())
        })
        .collect();
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
    let cast = || call("Expr.cast", json!({ "dtype": { "dtype": "Float64" } }));
    calls.push((
        "Expr.abs, Expr.max, cast to Float64".to_owned(),
        vec![named("Expr.abs"), named("Expr.max"), cast()],
    ));
    calls.push((
        "Expr.abs, Expr.sum, cast to Float64".to_owned(),
        vec![named("Expr.abs"), named("Expr.sum"), cast()],
    ));
    calls.push((
        "cast to Float64, Expr.abs, Expr.max".to_owned(),
        vec![cast(), named("Expr.abs"), named("Expr.max")],
    ));
    calls.push((
        "Expr.abs, Expr.value_counts, Expr.max".to_owned(),
        vec![
            named("Expr.abs"),
            named("Expr.value_counts"),
            named("Expr.max"),
        ],
    ));
    calls.push((
        "Expr.value_counts, field literal, Expr.max".to_owned(),
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
fn chains(repeated: bool, whole: bool) -> Vec<(String, Chain)> {
    let mut chains = Vec::new();
    if whole {
        // What only whole numbers take, which a number with a fraction makes both refuse, so it
        // is crossed with whole numbers alone.
        for form in [Form::Alone, Form::BothValues, Form::OneValue] {
            for names in [
                vec!["Expr.not_"],
                vec!["Expr.abs", "Expr.not_"],
                vec!["Expr.not_", "Expr.abs"],
                vec!["Expr.not_", "Expr.max"],
                vec!["Expr.max", "Expr.not_"],
            ] {
                chains.push((
                    format!("{}, {}", form.label(), names.join(", ")),
                    Chain {
                        form,
                        calls: names.into_iter().map(named).collect(),
                    },
                ));
            }
        }
        return chains;
    }
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

fn grid(
    normal: bool,
    repeated: bool,
    whole: bool,
    places: &[(&'static str, Place)],
) -> Grid<Point> {
    let values = numbers(normal)
        .into_iter()
        .filter(|written| !whole || matches!(written, Written::Integer(_)))
        .map(|written| (written.label(), written))
        .collect();
    Grid::<Point>::new()
        .axis("number", values, |point, written| {
            point.number = Some(written.clone());
        })
        .axis("chain", chains(repeated, whole), |point, chain| {
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
    Question::any_refusal()
        .cannot_tell(|words| words.contains(&Unknown::AnOpenNumberInAKey.to_string()))
}

fn keys(normal: bool) -> Verdicts {
    grid(normal, true, false, &Place::KEYS).judge(&question(), case_of)
}

fn elsewhere(normal: bool) -> Verdicts {
    grid(normal, false, false, &Place::ELSEWHERE)
        .judge(&Question::any_refusal().with_dtypes(), case_of)
}

fn whole_numbers(normal: bool) -> Verdicts {
    let places: Vec<(&'static str, Place)> = Place::KEYS
        .iter()
        .chain(&Place::ELSEWHERE)
        .copied()
        .collect();
    grid(normal, false, true, &places).judge(&question().with_dtypes(), case_of)
}

/// The calls that are no elementwise ones, which the walk refuses in a key.
const NOT_ELEMENTWISE: &[&str] = &[
    "Expr.backward_fill",
    "Expr.cum_max",
    "Expr.cum_min",
    "Expr.cum_prod",
    "Expr.cum_sum",
    "Expr.diff",
    "Expr.forward_fill",
    "Expr.pct_change",
    "Expr.rank",
    "Expr.reverse",
    "Expr.shift",
    "Expr.sort",
];

/// What the grid of the keys leaves, with the reason. A key the walk cannot tell for another
/// reason than the open number it holds is one it neither refuses nor says is open: the grid
/// counts it as allowed, and names it where Polars refuses it.
fn known_keys() -> Vec<Known> {
    let refused_for_the_number = |pattern: Pattern, reason: &'static str| {
        Known::new(Kind::PolarsRefusesOnly, pattern, reason).saying("datatypes of join keys")
    };
    vec![
        Known::new(
            Kind::WalkRefusesOnly,
            Pattern::new().has_one_of("chain", NOT_ELEMENTWISE),
            "a key made wholly of one value, a number or a condition of numbers, is one value that \
             Polars takes whole whatever calls it holds, and the walk refuses the calls that are no \
             elementwise ones, a running value, a difference, a rank, a fill, a shift, a reverse and a \
             sort (the limit of a key that is wholly one value)",
        )
        .saying("must be elementwise"),
        Known::new(
            Kind::WalkRefusesOnly,
            Pattern::new().has_one_of("chain", &["Expr.drop_nans", "Expr.drop_nulls"]),
            "the walk refuses a method that drops values from a number, a repeat of one or a \
             condition of numbers as not elementwise even where an aggregate follows it, which \
             Polars takes as one value",
        )
        .saying("must be elementwise"),
        Known::new(
            Kind::CannotTellPolarsAllows,
            Pattern::new().is("place", "both keys open"),
            "the walk judges each key apart, so where a key holds an open number it cannot tell the \
             key, though Polars matches the two keys that hold the same open number, as `dyn float` is \
             `dyn float` on both sides",
        )
        .saying("stays open"),
        refused_for_the_number(
            Pattern::new()
                .starts("chain", "a condition with both values open")
                .has_one_of("chain", &["Expr.value_counts"]),
            "the walk does not carry the fields of the counts that a condition of open numbers \
             gives, so the key is not said to be open, though Polars keeps the number open in them",
        ),
        refused_for_the_number(
            Pattern::new()
                .starts("chain", "a condition with both values structs of the number")
                .contains_one_of("chain", &[", field literal"]),
            "the walk does not carry the fields of a condition whose values are structs, so it \
             cannot tell a field read from it, though the field is as open as the structs are",
        ),
        refused_for_the_number(
            Pattern::new().has_one_of("chain", &["Expr.log"]),
            "a logarithm takes a base that the walk reads as an operand it is not given, so it has \
             no dtype for the number to go on from after it, and the key is not said to be open, \
             though Polars keeps a number with a fraction open through it",
        ),
        refused_for_the_number(
            Pattern::new().contains_one_of("chain", &["add an open number"]),
            "the walk gives no dtype to two numbers typed without a dtype that meet, so the key is \
             not said to be open, though Polars keeps the sum of two open numbers open",
        ),
        refused_for_the_number(
            Pattern::new().has_one_of("chain", &["Expr.product"]),
            "Polars plans the product of a number with a fraction typed without a dtype as Int64 and \
             runs it as Float64, a mismatch of its own between the planned and the executed dtype \
             (for the number typed both are Float64), so the keys differ in the planned dtype for the \
             number and not for being open",
        ),
    ]
}

/// What the grid of the other places leaves, with the reason.
fn known_elsewhere() -> Vec<Known> {
    vec![
        Known::new(
            Kind::DtypeDiffers,
            Pattern::new().has_one_of("chain", &["Expr.product", "Expr.cum_prod"]),
            "Polars plans the product, and the running product, of a number with a fraction typed \
             without a dtype as Int64 and runs it as Float64, a mismatch of its own between the \
             planned and the executed dtype (for the number typed both are Float64), so the dtype \
             the tables give, Float64, is the one the rows have",
        )
        .saying("the tables give Float"),
    ]
}

#[test]
fn a_number_typed_without_a_dtype_stays_open_through_the_calls_that_keep_it_as_a_key() {
    keys(true).assert_agree(&known_keys(), 80_000);
}

#[test]
fn a_number_typed_without_a_dtype_is_judged_as_before_in_a_select_a_filter_and_a_sort_key() {
    elsewhere(true).assert_agree(&known_elsewhere(), 90_000);
}

#[test]
fn a_whole_number_stays_open_through_the_calls_only_whole_numbers_take() {
    let known: Vec<Known> = known_keys().into_iter().map(Known::shared).collect();
    whole_numbers(true).assert_agree(&known, 100);
}

#[test]
#[ignore = "the large grid, run on request"]
fn the_large_grid_of_an_open_number_in_a_key_is_taken_as_polars_does() {
    keys(false).assert_agree(&known_keys(), 400_000);
}

#[test]
#[ignore = "the large grid, run on request"]
fn the_large_grid_of_an_open_number_elsewhere_is_taken_as_polars_does() {
    elsewhere(false).assert_agree(&known_elsewhere(), 400_000);
}
