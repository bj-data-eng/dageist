//! A number typed without a dtype, which Polars keeps open as `dyn int` or `dyn float`, through
//! every call that keeps it so, in every place it can stand, with Polars as the judge.
//!
//! Polars does not settle such a number where it stands alone: it gives the number the class it
//! falls back to ([`literals::falls_back_to`]) where the schema is worked out, and keeps it open as
//! it is passed on through the calls that give back the dtype they follow, through `repeat`,
//! through a condition whose values are open and through a struct. A join's key that holds one is
//! refused against the other table's key where that is typed (`datatypes of join keys don't
//! match`), though both give the class the number falls back to. A select, a filter and a sort key
//! resolve the number where they end, so none of them is refused for it.
//!
//! The grid of the keys crosses the number, at the edges of the classes it falls back to and past
//! them, with how it stands and the calls that follow it, and puts the chain as a join's key on
//! either side against a key of the number typed, against a key of it open too, and against a key
//! of it typed on both sides, which is the control. The walk has to tell a key that holds an open
//! number, in the words of `an-open-number-in-a-key`, where Polars refuses it, and to tell the
//! control without them. The grid of the other places puts the same chains in a select, a filter
//! and a sort key, open and typed, where Polars refuses none of them for the number.
//!
//! The calls are the catalogue's, by what the tables say each does after every class a number falls
//! back to ([`axes::does`]): every one that keeps it, aggregates it, changes it or keeps it for some,
//! alone; every pair of those that keep or aggregate it; chains of three of one of each family of
//! them. A struct of the number is followed by every call that reads a struct, and a whole number by
//! every call that only whole numbers take. How the number stands, and the few calls given an
//! operand or a value, are named cases.
//!
//! A second grid holds the class of a number that starts a comparison for missing values, beside a
//! column of every whole number class, at every place.

use dageist_engine::allowed::{Representative, Unknown};
use dageist_engine::catalog::Entry;
use dageist_engine_fuzz::axes::{self, Does, Param};
use dageist_engine_fuzz::doc::{call, chain, column, column_then, many, then, whole};
use dageist_engine_fuzz::judge::{Case, Connections, Kind, Outcome, Question};
use dageist_engine_fuzz::literals::{self, Literal, Written};
use dageist_engine_fuzz::world::{Column, World};
use dageist_fuzz::grid::{Choices, Grid, Known, Pattern, Scale, Verdicts};
use serde_json::{Value, json};

/// The numbers a grid writes: whole numbers and numbers with a fraction at the edges of every class.
/// At normal scale one of each kind: a whole number of `Int32`, one past it, a fraction a `Float32`
/// holds and one past what it holds.
fn numbers(scale: Scale) -> Choices<Written> {
    literals::edge_integers()
        .and(literals::edge_floats())
        .named("number")
        .one_of_each(scale, "kind of number", |_, written| match written {
            Written::Integer(value) if i32::try_from(*value).is_ok() => "a whole number of Int32",
            Written::Integer(_) => "a whole number past Int32",
            Written::Float(value) if value.abs() <= f64::from(f32::MAX) => "a fraction of Float32",
            _ => "a fraction past Float32",
        })
}

/// How a number stands: a named case of the shapes Polars keeps an open number open through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Form {
    Alone,
    Repeated,
    BothValues,
    OneValue,
    InAStruct,
    InAStructOfAbs,
    InAStructOfRepeat,
    StructValues,
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

    const fn is_a_struct(self) -> bool {
        matches!(
            self,
            Self::InAStruct
                | Self::InAStructOfAbs
                | Self::InAStructOfRepeat
                | Self::StructValues
                | Self::CountedValues
        )
    }
}

/// The chain a cell puts the number in: how it stands and the calls that follow it.
#[derive(Debug, Clone)]
struct Chain {
    form: Form,
    calls: Vec<Value>,
}

fn named(entry: &Entry) -> Value {
    call(entry.id().as_str(), json!({}))
}

fn label_of(entries: &[&Entry]) -> String {
    if entries.is_empty() {
        "no call".to_owned()
    } else {
        entries
            .iter()
            .map(|entry| entry.id().as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn the_literal_field() -> Value {
    call(
        "Expr.struct.field",
        json!({ "name": { "field": "literal" } }),
    )
}

fn open_float() -> Value {
    Written::Float(2.5).expression(None)
}

fn typed_float() -> Value {
    Written::Float(2.5).expression(Some("Float64"))
}

/// The calls that keep or aggregate what they follow, and the first of each family of them.
fn kept() -> (Vec<&'static Entry>, Vec<&'static Entry>) {
    let kept: Vec<&Entry> = axes::calls_that_do("call", Does::Keeps, &literals::FALLBACKS)
        .and(axes::calls_that_do("call", Does::Aggregates, &literals::FALLBACKS).named("call"))
        .iter()
        .map(|(_, entry)| *entry)
        .collect();
    let few = axes::calls_that_do("call", Does::Keeps, &literals::FALLBACKS)
        .and(axes::calls_that_do("call", Does::Aggregates, &literals::FALLBACKS).named("call"))
        .one_of_each(Scale::Normal, "family", |_, entry| entry.family())
        .iter()
        .map(|(_, entry)| *entry)
        .collect();
    (kept, few)
}

/// The calls a number is followed by, each chain with its label: every call that keeps, aggregates,
/// changes or keeps for some what it follows, alone; every pair of the first two; the first of each
/// family of them three deep; and a few given an operand or a value, which are named cases.
fn calls() -> Vec<(String, Vec<Value>)> {
    let (kept, few) = kept();
    let of = |does| -> Vec<&Entry> {
        axes::calls_that_do("call", does, &literals::FALLBACKS)
            .iter()
            .map(|(_, entry)| *entry)
            .collect()
    };
    let changes = of(Does::Changes);
    let some = of(Does::KeepsSome);
    let mut chains: Vec<Vec<&Entry>> = vec![Vec::new()];
    chains.extend(
        kept.iter()
            .chain(&changes)
            .chain(&some)
            .map(|entry| vec![*entry]),
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
    let mut seen = std::collections::BTreeSet::new();
    let mut calls: Vec<(String, Vec<Value>)> = chains
        .into_iter()
        .filter_map(|entries| {
            let label = label_of(&entries);
            seen.insert(label.clone())
                .then(|| (label, entries.into_iter().map(named).collect()))
        })
        .collect();
    let with = |entry: &str, name: &str, other: Value| call(entry, json!({ name: other }));
    let int64 = World::get().column("Int64").name.clone();
    let cast = || call("Expr.cast", json!({ "dtype": { "dtype": "Float64" } }));
    let abs = || call("Expr.abs", json!({}));
    let max = || call("Expr.max", json!({}));
    // The calls given an operand or a value, and through a cast, a named case each.
    calls.extend([
        (
            "add an open number".to_owned(),
            vec![with("Expr.add", "other", open_float())],
        ),
        (
            "add a typed number".to_owned(),
            vec![with("Expr.add", "other", typed_float())],
        ),
        (
            "add a column".to_owned(),
            vec![with("Expr.add", "other", column(&int64))],
        ),
        (
            "eq_missing a column".to_owned(),
            vec![with("Expr.eq_missing", "other", column(&int64))],
        ),
        (
            "fill_null with an open number".to_owned(),
            vec![with("Expr.fill_null", "value", open_float())],
        ),
        (
            "Expr.abs, Expr.max, cast to Float64".to_owned(),
            vec![abs(), max(), cast()],
        ),
        (
            "cast to Float64, Expr.abs, Expr.max".to_owned(),
            vec![cast(), abs(), max()],
        ),
        (
            "Expr.abs, Expr.value_counts, Expr.max".to_owned(),
            vec![abs(), call("Expr.value_counts", json!({})), max()],
        ),
        (
            "Expr.value_counts, field literal, Expr.max".to_owned(),
            vec![
                call("Expr.value_counts", json!({})),
                the_literal_field(),
                max(),
            ],
        ),
    ]);
    calls
}

/// The calls that follow a struct of a number: every call given nothing that the tables allow
/// after a struct, the first of each family of them at normal scale, and the field of the number
/// read and followed by its absolute value or by the counts of it.
fn struct_calls(scale: Scale) -> Vec<(String, Vec<Value>)> {
    let reads: Vec<(String, Vec<Value>)> = axes::calls_on("call", Representative::Struct)
        .keep_unless(|_, (entry, effect)| {
            (entry
                .parameters()
                .iter()
                .any(|parameter| parameter.is_needed())
                || matches!(effect, axes::Effect::Refused | axes::Effect::NotKnown))
            .then(|| "the tables allow it after no struct, or it needs a parameter".to_owned())
        })
        .one_of_each(scale, "family", |_, (entry, _)| entry.family())
        .iter()
        .map(|(_, (entry, _))| (entry.id().as_str().to_owned(), vec![named(entry)]))
        .collect();
    let mut calls = vec![("no call".to_owned(), Vec::new())];
    calls.extend(reads);
    calls.extend([
        ("field literal".to_owned(), vec![the_literal_field()]),
        (
            "field literal, abs".to_owned(),
            vec![the_literal_field(), call("Expr.abs", json!({}))],
        ),
        (
            "value_counts, field literal, max".to_owned(),
            vec![
                call("Expr.value_counts", json!({})),
                the_literal_field(),
                call("Expr.max", json!({})),
            ],
        ),
    ]);
    calls
}

/// The calls only whole numbers take: those the tables allow after every whole number a number
/// falls back to and refuse after a float, alone and with the first of each family of those that
/// keep or aggregate what they follow, on either side.
fn whole_calls() -> Vec<(String, Vec<Value>)> {
    let whole = [Representative::Int32, Representative::Int64];
    let only: Vec<&Entry> = axes::entries("call", dageist_engine::catalog::Applies::Expression)
        .iter()
        .map(|(_, entry)| *entry)
        .filter(|entry| {
            entry
                .parameters()
                .iter()
                .all(|parameter| !parameter.is_needed())
                && axes::does(entry, &whole).is_ok()
                && axes::does(entry, &[Representative::Float64]).is_err()
        })
        .collect();
    let (_, few) = kept();
    let mut calls = Vec::new();
    for entry in &only {
        calls.push((label_of(&[entry]), vec![named(entry)]));
        for kind in &few {
            calls.push((label_of(&[entry, kind]), vec![named(entry), named(kind)]));
            calls.push((label_of(&[kind, entry]), vec![named(kind), named(entry)]));
        }
    }
    calls
}

/// The chains a grid puts in its places, each by its form and its calls. A repeated number stands
/// in a key only: Polars takes its length for the rows of the other table, which the walk cannot.
fn chains(scale: Scale, repeated: bool, whole: bool) -> Choices<Chain> {
    let mut axis = Choices::new("chain");
    let mut add = |form: Form, label: &str, calls: Vec<Value>| {
        axis = std::mem::replace(&mut axis, Choices::new("chain"))
            .with(format!("{}, {label}", form.label()), Chain { form, calls });
    };
    if whole {
        // What only whole numbers take, which a number with a fraction makes both refuse, so it
        // is crossed with whole numbers alone.
        for form in [Form::Alone, Form::BothValues, Form::OneValue] {
            for (label, calls) in whole_calls() {
                add(form, &label, calls);
            }
        }
        return axis;
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
            add(form, &label, calls);
        }
    }
    for form in [
        Form::InAStruct,
        Form::InAStructOfAbs,
        Form::InAStructOfRepeat,
        Form::StructValues,
        Form::CountedValues,
    ] {
        debug_assert!(form.is_a_struct());
        for (label, calls) in struct_calls(scale) {
            add(form, &label, calls);
        }
    }
    axis
}

/// Where the chain stands, and which of it is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    LeftOpen,
    RightOpen,
    BothOpen,
    BothTyped,
    Select(bool),
    Filter(bool),
    Sort(bool),
}

/// The keys of a join: the chain of the number open on one side or both, against it typed, and
/// typed on both sides, the control.
fn keys() -> Choices<Place> {
    Choices::of(
        "place",
        [
            ("a left key open, the right typed", Place::LeftOpen),
            ("a right key open, the left typed", Place::RightOpen),
            ("both keys open", Place::BothOpen),
            ("both keys typed", Place::BothTyped),
        ],
    )
}

/// The places a number is resolved where it ends: a select, a filter and a sort key, open and
/// typed.
fn elsewhere() -> Choices<Place> {
    Choices::of(
        "place",
        [
            ("a select, open", Place::Select(false)),
            ("a select, typed", Place::Select(true)),
            ("a filter, open", Place::Filter(false)),
            ("a filter, typed", Place::Filter(true)),
            ("a sort key, open", Place::Sort(false)),
            ("a sort key, typed", Place::Sort(true)),
        ],
    )
}

#[derive(Default)]
struct Point {
    number: Option<Written>,
    chain: Option<Chain>,
    place: Option<Place>,
}

/// The literal of the number, open or given the class it falls back to.
fn literal(number: &Written, typed: bool) -> Value {
    let word = typed.then(|| {
        literals::falls_back_to(number)
            .expect("a number")
            .name()
            .to_owned()
    });
    number.call(word.as_deref())
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
            json!({ "exprs": many(vec![chain(literal(number, typed), vec![call("Expr.abs", json!({}))])]) }),
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
                    Form::CountedValues => then(number, [call("Expr.value_counts", json!({}))]),
                    _ => number,
                }
            };
            let second = if chosen.form == Form::OneValue {
                value(true)
            } else {
                value(typed)
            };
            let condition = chain(
                call(
                    "polars.when",
                    json!({ "predicates": many(vec![Written::Flag(true).expression(None)]) }),
                ),
                vec![
                    call("When.then", json!({ "statement": value(typed) })),
                    call("Then.otherwise", json!({ "statement": second })),
                ],
            );
            return then(condition, chosen.calls.iter().cloned());
        }
    };
    chain(start, chosen.calls.clone())
}

fn case_of(point: &Point) -> Result<Case, String> {
    let place = point.place.expect("a place");
    let number = point.number.as_ref().expect("a number");
    let chosen = point.chain.as_ref().expect("a chain");
    let world = World::get();
    let int64 = world.column("Int64");
    let key = |typed: bool| expression(number, chosen, typed);
    let (entry, arguments) = match place {
        Place::LeftOpen | Place::RightOpen | Place::BothOpen | Place::BothTyped => {
            let (left, right) = match place {
                Place::LeftOpen => (key(false), key(true)),
                Place::RightOpen => (key(true), key(false)),
                Place::BothOpen => (key(false), key(false)),
                _ => (key(true), key(true)),
            };
            // The right key reads the other table, whose columns are the world's under other names.
            let right = dageist_engine_fuzz::places::on_the_other_table(&right);
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
            json!({ "predicates": many(vec![column_then(
                &int64.name,
                vec![call("Expr.gt", json!({ "other": key(typed) }))],
            )]) }),
        ),
        Place::Sort(typed) => ("LazyFrame.sort", json!({ "by": many(vec![key(typed)]) })),
    };
    Case::of_step(
        entry,
        arguments,
        world.of(&["Int64"]),
        Connections::new().table("other", world.other().clone()),
    )
}

fn grid(
    name: &'static str,
    scale: Scale,
    repeated: bool,
    whole: bool,
    places: Choices<Place>,
) -> Grid<Point> {
    let numbers = numbers(scale).keep_unless(|_, written| {
        (whole && !matches!(written, Written::Integer(_)))
            .then(|| "a call only whole numbers take is crossed with whole numbers".to_owned())
    });
    Grid::<Point>::new(name)
        .axis(numbers, |point, written| {
            point.number = Some(written.clone());
        })
        .axis(chains(scale, repeated, whole), |point, chain| {
            point.chain = Some(chain.clone());
        })
        .axis(places, |point, place| point.place = Some(*place))
}

fn question() -> Question {
    Question::any_refusal()
        .cannot_tell(|words| words.contains(&Unknown::AnOpenNumberInAKey.to_string()))
}

fn keys_verdicts(scale: Scale) -> Verdicts<Outcome> {
    let question = question();
    let name = if scale == Scale::Normal {
        "open-keys"
    } else {
        "open-keys-large"
    };
    grid(name, scale, true, false, keys()).judge(|point| question.judge(case_of(point)))
}

fn elsewhere_verdicts(scale: Scale) -> Verdicts<Outcome> {
    let question = Question::any_refusal().with_dtypes();
    let name = if scale == Scale::Normal {
        "open-elsewhere"
    } else {
        "open-elsewhere-large"
    };
    grid(name, scale, false, false, elsewhere()).judge(|point| question.judge(case_of(point)))
}

fn whole_number_verdicts() -> Verdicts<Outcome> {
    let question = question().with_dtypes();
    grid(
        "open-whole-numbers",
        Scale::Normal,
        false,
        true,
        keys().and(elsewhere()),
    )
    .judge(|point| question.judge(case_of(point)))
}

/// The calls that are no elementwise ones by the tables of contexts: a key of a join refuses them
/// after a whole number, which the walk refuses in a key.
fn not_elementwise() -> Vec<String> {
    use dageist_engine::allowed::{self, After, Chosen, Context, Outer, Place as Standing, Shape};
    let place = Standing::new(
        Context::step(Outer::JoinLeft),
        After::Value {
            dtype: Representative::Int64,
            shape: Shape::Many,
        },
    );
    axes::calls_on("call", Representative::Int64)
        .iter()
        .filter(|(_, (entry, _))| {
            matches!(
                allowed::offer(entry, &place, &Chosen::default()),
                allowed::Verdict::Refused(_)
            )
        })
        .map(|(label, _)| label.to_owned())
        .collect()
}

/// What the grid of the keys leaves, with the reason. A key the walk cannot tell for another
/// reason than the open number it holds is one it neither refuses nor says is open: the grid counts
/// it as allowed, and names it where Polars refuses it.
fn known_keys() -> Vec<Known<Kind>> {
    let refused_for_the_number = |pattern: Pattern, reason: &'static str| {
        Known::new(Kind::PolarsRefusesOnly, pattern, reason).saying("datatypes of join keys")
    };
    vec![
        Known::new(
            Kind::WalkRefusesOnly,
            Pattern::new().has_one_of("chain", not_elementwise()),
            "a key made wholly of one value, a number or a condition of numbers, is one value that \
             Polars takes whole whatever calls it holds, and the walk refuses the calls that are no \
             elementwise ones, a running value, a difference, a rank, a fill, a shift, a reverse and a \
             sort (the limit of a key that is wholly one value)",
        )
        .saying("must be elementwise"),
        Known::new(
            Kind::CannotTellPolarsAllows,
            Pattern::new().is("place", "both keys open"),
            "the walk judges each key apart, so where a key holds an open number it cannot tell the \
             key, though Polars matches the two keys that hold the same open number, as `dyn float` \
             is `dyn float` on both sides",
        )
        .saying("stays open"),
        refused_for_the_number(
            Pattern::new()
                .starts("chain", "a condition with both values open")
                .has_one_of("chain", ["Expr.value_counts"]),
            "the walk does not carry the fields of the counts that a condition of open numbers \
             gives, so the key is not said to be open, though Polars keeps the number open in them",
        ),
        refused_for_the_number(
            Pattern::new()
                .starts("chain", "a condition with both values structs of the number")
                .contains_one_of("chain", [", field literal"]),
            "the walk does not carry the fields of a condition whose values are structs, so it \
             cannot tell a field read from it, though the field is as open as the structs are",
        ),
        refused_for_the_number(
            Pattern::new().has_one_of("chain", ["Expr.log"]),
            "a logarithm takes a base that the walk reads as an operand it is not given, so it has \
             no dtype for the number to go on from after it, and the key is not said to be open, \
             though Polars keeps a number with a fraction open through it",
        ),
        refused_for_the_number(
            Pattern::new().contains_one_of("chain", ["add an open number"]),
            "the walk gives no dtype to two numbers typed without a dtype that meet, so the key is \
             not said to be open, though Polars keeps the sum of two open numbers open",
        ),
        refused_for_the_number(
            Pattern::new().has_one_of("chain", ["Expr.product"]),
            "Polars plans the product of a number with a fraction typed without a dtype as Int64 and \
             runs it as Float64, a mismatch of its own between the planned and the executed dtype \
             (for the number typed both are Float64), so the keys differ in the planned dtype for \
             the number and not for being open",
        ),
    ]
}

/// What the grid of the other places leaves, with the reason.
fn known_elsewhere() -> Vec<Known<Kind>> {
    vec![
        Known::new(
            Kind::WalkRefusesOnly,
            Pattern::new().is("chain", "a struct of repeat of the number, Expr.arg_unique"),
            "the walk cannot know how many values arg_unique gives of a repeat, which the data \
             decide, and refuses it where the rows of the table are needed; here the repeat is as \
             long as the table by chance, and Polars allows it (the limit of a count the data \
             decide)",
        )
        .saying_one_of(&[
            "rule values-meet-in-count",
            "may not end an expression in sort",
        ]),
        Known::new(
            Kind::DtypeDiffers,
            Pattern::new().has_one_of("chain", ["Expr.product", "Expr.cum_prod"]),
            "Polars plans the product, and the running product, of a number with a fraction typed \
             without a dtype as Int64 and runs it as Float64, a mismatch of its own between the \
             planned and the executed dtype (for the number typed both are Float64), so the dtype \
             the walk gives, Float64, is the one the rows have",
        )
        .saying("the walk gives Float"),
    ]
}

#[test]
fn a_number_typed_without_a_dtype_stays_open_through_the_calls_that_keep_it_as_a_key() {
    keys_verdicts(Scale::Normal).assert_agree(&known_keys(), 80_000);
}

#[test]
fn a_number_typed_without_a_dtype_is_judged_as_before_in_a_select_a_filter_and_a_sort_key() {
    elsewhere_verdicts(Scale::Normal).assert_agree(&known_elsewhere(), 90_000);
}

#[test]
fn a_whole_number_stays_open_through_the_calls_only_whole_numbers_take() {
    let mut known: Vec<Known<Kind>> = known_keys().into_iter().map(Known::shared).collect();
    known.push(
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new()
                .has_one_of(
                    "chain",
                    ["Expr.bitwise_and", "Expr.bitwise_or", "Expr.bitwise_xor"],
                )
                .starts("number", "-"),
            "Polars keeps a whole number below nothing typed without a dtype open through a \
             bitwise aggregate of it, which the walk does not say is open (candidate to file)",
        )
        .saying("datatypes of join keys"),
    );
    whole_number_verdicts().assert_agree(&known, 1_000);
}

#[test]
#[ignore = "the large grid, run on request"]
fn the_large_grid_of_an_open_number_in_a_key_is_taken_as_polars_does() {
    keys_verdicts(Scale::Large).assert_agree(&known_keys(), 400_000);
}

#[test]
#[ignore = "the large grid, run on request"]
fn the_large_grid_of_an_open_number_elsewhere_is_taken_as_polars_does() {
    elsewhere_verdicts(Scale::Large).assert_agree(&known_elsewhere(), 400_000);
}

// ---------------------------------------------------------------------------
// A number that starts a comparison for missing values

#[derive(Default)]
struct Missing {
    column: Option<Column>,
    param: Option<Param>,
    first: bool,
    literal: Literal,
    key: bool,
}

/// The comparisons of the catalogue that compare missing values as values.
fn missing_comparisons() -> Choices<Param> {
    axes::literal_parameters("method").keep_unless(|_, param| {
        (!param.method().ends_with("_missing"))
            .then(|| "it compares no missing value as a value".to_owned())
    })
}

fn missing_verdicts(scale: Scale) -> Verdicts<Outcome> {
    let whole = World::get().classes("column").keep_unless(|_, column| {
        let kind = axes::kind_of_class(column.class.expect("a class"));
        (!kind.ends_with("whole number")).then(|| "no whole number".to_owned())
    });
    let numbers = literals::edge_integers()
        .and(literals::edge_floats())
        .named("number")
        .thin(scale, 2);
    let grid = Grid::<Missing>::new(if scale == Scale::Normal {
        "missing-start"
    } else {
        "missing-start-large"
    })
    .axis(whole, |point, column| point.column = Some(column.clone()))
    .axis(numbers, |point, number| {
        point.literal.value = Some(number.clone());
    })
    .axis(missing_comparisons(), |point, param| {
        point.param = Some(*param)
    })
    .axis(
        Choices::of(
            "order",
            [("the column first", false), ("the number first", true)],
        ),
        |point, first| point.first = *first,
    )
    .axis(
        Choices::of("place", [("a select", false), ("a join's key", true)]),
        |point, key| point.key = *key,
    );
    let question = Question::any_refusal().with_dtypes();
    grid.judge(|point| {
        question.judge((|| {
            let held = point.column.as_ref().ok_or("no column")?;
            let compared = axes::method_given(
                point.param.as_ref().ok_or("no method")?,
                &point.literal,
                held,
                point.first,
            )?;
            let world = World::get();
            let (entry, arguments) = if point.key {
                (
                    "LazyFrame.join",
                    json!({
                        "other": { "table": "other" },
                        "left_on": many(vec![compared]),
                        "right_on": many(vec![column("rf")]),
                    }),
                )
            } else {
                ("LazyFrame.select", json!({ "exprs": many(vec![compared]) }))
            };
            Case::of_step(
                entry,
                arguments,
                world.of(&[&held.label]),
                Connections::new().table("other", world.other().clone()),
            )
        })())
    })
}

#[test]
fn the_dtype_of_a_number_that_starts_a_missing_comparison_is_the_dtype_polars_gives() {
    missing_verdicts(Scale::Normal).assert_agree(&[], 2_000);
}

#[test]
#[ignore = "the large grid, run on request"]
fn the_large_grid_of_a_number_that_starts_a_missing_comparison_is_taken_as_polars_does() {
    missing_verdicts(Scale::Large).assert_agree(&[], 4_000);
}
