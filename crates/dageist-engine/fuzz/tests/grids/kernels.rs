//! What a group by evaluates on the values of each group, held to Polars over grids, both ways, in
//! `agg` and in `having`, beside a select of the same, which tells a refusal that is the group's
//! from one that is not.
//!
//! A group evaluates a chain with kernels of its own, which differ from a select's by what they are
//! given beside the values they follow. Every grid asks Polars the rows.
//!
//! - **Keys of a window**: what a window groups by and what it orders by: a column, a literal, a
//!   condition, and the whole numbers `n` followed by every call of the catalogue.
//! - **Methods given values beside what they follow**: every expression parameter of every method
//!   ([`axes::expression_parameters`]), on a receiver of the class the method first accepts
//!   ([`axes::accepted_class`]) that is a column, a literal, an aggregate or a null, given a
//!   column, an aggregate, a literal, a null or a product of text the tables refuse.
//! - **A condition** given a predicate the tables refuse, with values the same or different.

use dageist_engine::allowed::Representative;
use dageist_engine::catalog::{self, Applies, Entry};
use dageist_engine_fuzz::axes::{self, Param};
use dageist_engine_fuzz::doc::{call, chain, column, column_then, many, named, then, whole};
use dageist_engine_fuzz::judge::{Case, Connections, Depth, Kind, Question};
use dageist_engine_fuzz::literals::{self, Written};
use dageist_engine_fuzz::world::World;
use dageist_fuzz::grid::{Choices, Grid, Known, Pattern, Scale};
use serde_json::{Value, json};

/// Where a value is evaluated: the aggregates of a group, its `having`, or a select, the control.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Context {
    Agg,
    Having,
    Select,
}

fn contexts(select: bool) -> Choices<Context> {
    let axis = Choices::of(
        "context",
        [("agg", Context::Agg), ("having", Context::Having)],
    );
    if select {
        axis.with("select", Context::Select)
    } else {
        axis
    }
}

fn keyed() -> Choices<bool> {
    Choices::of("keys", [("a key", true), ("no key", false)])
}

/// The case of `value` evaluated in `context`, grouped by `n` where `keyed`, on the helpers of the
/// world and the columns `labels`.
fn grouped(context: Context, keyed: bool, value: Value, labels: &[&str]) -> Result<Case, String> {
    let world = World::get();
    let finish = match context {
        Context::Select => {
            return Case::of_step(
                "LazyFrame.select",
                json!({ "named_exprs": named(vec![("out", value)]) }),
                world.of(labels),
                Connections::new(),
            );
        }
        Context::Agg => vec![call(
            "LazyGroupBy.agg",
            json!({ "named_aggs": named(vec![("out", value)]) }),
        )],
        // A predicate is one value for each group: whether every value is no null.
        Context::Having => vec![
            call(
                "LazyGroupBy.having",
                json!({ "predicates": many(vec![then(value, [
                    call("Expr.is_not_null", json!({})),
                    call("Expr.all", json!({})),
                ])]) }),
            ),
            call("LazyGroupBy.len", json!({})),
        ],
    };
    Case::of_grouping(
        if keyed {
            json!({ "by": many(vec![column("n")]) })
        } else {
            json!({})
        },
        finish,
        world.of(labels),
        Connections::new(),
    )
}

// ---------------------------------------------------------------------------
// The key of a window

/// A key of a window: a named shape, or `n` followed by a call of the catalogue.
#[derive(Clone)]
enum Key {
    Columns(Vec<&'static str>),
    Literal,
    Condition,
    LiteralThen(&'static Entry),
    AggregateAndN,
    Called(&'static Entry),
}

fn window_keys(scale: Scale) -> Choices<Key> {
    let mut axis = Choices::of(
        "key",
        [
            ("column k", Key::Columns(vec!["k"])),
            ("column n", Key::Columns(vec!["n"])),
            ("column m", Key::Columns(vec!["m"])),
            ("literal 1", Key::Literal),
            ("a condition", Key::Condition),
            ("n and an aggregate", Key::AggregateAndN),
        ],
    );
    let calls =
        axes::entries("key", Applies::Expression)
            .one_of_each(scale, "family", |_, entry| entry.family());
    for (label, entry) in calls.iter() {
        axis = axis.with(format!("n {label}"), Key::Called(entry));
    }
    for (label, entry) in axes::calls_that_do("key", axes::Does::Keeps, &literals::FALLBACKS)
        .one_of_each(scale, "family", |_, entry| entry.family())
        .iter()
    {
        axis = axis.with(format!("literal {label}"), Key::LiteralThen(entry));
    }
    axis
}

fn keys_of(key: &Key) -> Result<Vec<Value>, String> {
    let n = World::get().column("Int64");
    Ok(match key {
        Key::Columns(names) => names.iter().map(|name| column(name)).collect(),
        Key::Literal => vec![whole(1)],
        Key::Condition => vec![chain(
            call(
                "polars.when",
                json!({ "predicates": many(vec![column_then("n", vec![call("Expr.gt", json!({ "other": whole(1) }))])]) }),
            ),
            vec![
                call("When.then", json!({ "statement": whole(1) })),
                call("Then.otherwise", json!({ "statement": whole(2) })),
            ],
        )],
        Key::LiteralThen(entry) => vec![then(whole(1), [axes::call_of(entry, Some(n))?])],
        Key::AggregateAndN => vec![
            column_then("n", vec![call("Expr.sum", json!({}))]),
            column("n"),
        ],
        Key::Called(entry) => vec![column_then("n", vec![axes::call_of(entry, Some(n))?])],
    })
}

#[derive(Default)]
struct Windowed {
    keyed: bool,
    context: Option<Context>,
    order: bool,
    key: Option<Key>,
}

fn windowed_case(point: &Windowed) -> Result<Case, String> {
    let keys = many(keys_of(point.key.as_ref().ok_or("no key")?)?);
    let over = if point.order {
        call(
            "Expr.over",
            json!({ "partition_by": many(vec![column("k")]), "order_by": keys }),
        )
    } else {
        call("Expr.over", json!({ "partition_by": keys }))
    };
    let windowed = column_then("m", vec![over]);
    let context = point.context.ok_or("no context")?;
    // A predicate is one value for each group, which the sum of the window is.
    let value = if context == Context::Having {
        then(windowed, [call("Expr.sum", json!({}))])
    } else {
        windowed
    };
    grouped(context, point.keyed, value, &[])
}

fn windowed_known() -> Vec<Known<Kind>> {
    vec![
        Known::new(
            Kind::WalkRefusesOnly,
            Pattern::new()
                .is("keys", "a key")
                .is("part", "order_by")
                .starts("key", "literal "),
            "Polars reads a running value, a fill or another call that keeps the order of a literal \
             as a literal where it orders a window, and refuses it where it groups one, as it does \
             the one of a column; the tables measured the call of a column, and the walk does not \
             fold the literal",
        )
        .saying("non-elementwise"),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new()
                .is("keys", "no key")
                .is("part", "order_by")
                .starts("key", "literal "),
            "a window that orders by fewer values than the rows is refused for its length in a \
             select as well, so with no key the rule for a key that is not elementwise is not what \
             refuses it",
        )
        .saying("order by expression evaluated to a length"),
        Known::new(
            Kind::WalkRefusesOnly,
            Pattern::new().is("keys", "no key"),
            "a group by with no key is one group that Polars evaluates as it evaluates a select, \
             which accepts a window whose key is not elementwise, where the tables measured the \
             groups of a key",
        )
        .saying("non-elementwise"),
    ]
}

#[test]
fn the_key_of_a_window_in_agg_and_in_having_is_what_polars_says() {
    let question = Question::any_refusal().at(Depth::Rows);
    Grid::<Windowed>::new("kernel-window-keys")
        .axis(contexts(false), |point, context| {
            point.context = Some(*context)
        })
        .axis(keyed(), |point, keyed| point.keyed = *keyed)
        .axis(
            Choices::of("part", [("partition_by", false), ("order_by", true)]),
            |point, order| point.order = *order,
        )
        .axis(window_keys(Scale::Normal), |point, key| {
            point.key = Some(key.clone())
        })
        .judge(|point| question.judge(windowed_case(point)))
        .assert_agree(&windowed_known(), 200);
}

// ---------------------------------------------------------------------------
// Methods given values beside what they follow

/// What a receiver or an argument is: the column, a literal, the first value of the column, which
/// an aggregate gives, a null, or a product of text that the tables refuse.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Operand {
    Column,
    Literal,
    Aggregate,
    Null,
    TextTimesText,
}

/// The column of the world of a class, a whole number apart: `n` for a receiver, `m` for an
/// argument, whose values all differ.
fn column_of(class: Representative, receiver: bool) -> String {
    match class {
        Representative::Int64 if receiver => "n".to_owned(),
        Representative::Int64 => "m".to_owned(),
        other => World::get().column(other.name()).name.clone(),
    }
}

fn operand(class: Representative, kind: Operand, receiver: bool) -> Result<Value, String> {
    let held = column_of(class, receiver);
    Ok(match kind {
        Operand::Column => column(&held),
        Operand::Literal => match literals::one_of(class) {
            Some(literal) => literal.expression(),
            None => return Err(format!("the editor types in no literal of {class}")),
        },
        Operand::Aggregate => column_then(&held, vec![call("Expr.first", json!({}))]),
        Operand::Null => Written::Null.expression(None),
        Operand::TextTimesText => {
            let text = column_of(Representative::String, true);
            column_then(
                &text,
                vec![call("Expr.mul", json!({ "other": column(&text) }))],
            )
        }
    })
}

fn operands(name: &'static str, receiver: bool) -> Choices<Operand> {
    let axis = Choices::of(
        name,
        [
            ("column", Operand::Column),
            ("literal", Operand::Literal),
            ("aggregate", Operand::Aggregate),
            ("null", Operand::Null),
        ],
    );
    if receiver {
        axis
    } else {
        axis.with("text times text", Operand::TextTimesText)
    }
}

/// Every expression parameter of a method, but the one that adds more of another, which `over`'s
/// `more_exprs` and `sort_by`'s `more_by` are.
fn methods() -> Choices<Param> {
    axes::expression_parameters("method").keep_unless(|_, param| {
        param
            .parameter
            .variadic()
            .map(|_| "it adds more of another parameter, which the grid gives".to_owned())
    })
}

#[derive(Default)]
struct Given {
    keyed: bool,
    goes_on: bool,
    stands_as_an_operand: bool,
    context: Option<Context>,
    method: Option<Param>,
    receiver: Option<Operand>,
    argument: Option<Operand>,
}

fn given_case(point: &Given) -> Result<Case, String> {
    let method = point.method.as_ref().ok_or("no method")?;
    let receiver = point.receiver.ok_or("no receiver")?;
    let argument = point.argument.ok_or("no argument")?;
    let class = axes::accepted_class(method.entry);
    // Polars looks one value up in another for ever where the first is a null literal.
    if method.method() == "Expr.is_in" && receiver == Operand::Null && argument == Operand::Literal
    {
        return Err("Polars never ends".to_owned());
    }
    let column = World::get().column(class.name());
    let mut given = serde_json::Map::new();
    for parameter in method.entry.parameters() {
        let value = if parameter.name() == method.name() {
            let value = operand(class, argument, false)?;
            if parameter.shape() == catalog::Shape::Many {
                many(vec![value])
            } else {
                value
            }
        } else if parameter.is_needed() {
            axes::filler(parameter, Some(column))?
        } else {
            continue;
        };
        given.insert(parameter.name().to_owned(), value);
    }
    let mut made = then(
        operand(class, receiver, true)?,
        [call(method.method(), Value::Object(given))],
    );
    // A call that the chain goes on from to an aggregate, which takes one value of what it is given.
    if point.goes_on {
        made = then(made, [call("Expr.first", json!({}))]);
    }
    // What stands beside an aggregate is met by it, as one operand is by another.
    let value = if point.stands_as_an_operand {
        let first = column_then(
            &column_of(class, false),
            vec![call("Expr.first", json!({}))],
        );
        then(first, [call("Expr.eq", json!({ "other": made }))])
    } else {
        made
    };
    let labels = [class.name(), "Int64", "String"];
    grouped(
        point.context.ok_or("no context")?,
        point.keyed,
        value,
        &labels,
    )
}

fn given_known() -> Vec<Known<Kind>> {
    vec![
        Known::new(
            Kind::WalkRefusesOnly,
            Pattern::new().is("method", "Expr.is_in(other)"),
            "the rule is-in-takes-a-list refuses by design the form that Polars deprecates, a value \
             looked up in a collection nested as deeply as it, which Polars still accepts in a \
             select as well",
        )
        .saying("is-in-takes-a-list"),
        Known::new(
            Kind::WalkRefusesOnly,
            Pattern::new()
                .one_of("method", ["Expr.replace(old)", "Expr.replace_strict(old)"])
                .is("argument", "null"),
            "the tables measured the values a replacement replaces as a column of the dtype Null, \
             whose values repeat, which a replacement refuses, and a null literal is one value, \
             which Polars takes, in a select as well",
        )
        .saying("must not contain duplicates"),
        Known::new(
            Kind::WalkRefusesOnly,
            Pattern::new(),
            "the tables were measured with the text 1d, which is no number, and the world's text \
             is of digits, which Polars reads as the count or the length of a method of text, in a \
             select as well",
        )
        .saying_one_of(&[
            "tables of Polars at Rows: conversion from `str`",
            "tables of Polars at Rows: cannot unpack series of type `str`",
        ]),
        Known::new(
            Kind::WalkRefusesOnly,
            Pattern::new().is("argument", "text times text"),
            "the tables refuse the multiplication of text by text, and Polars allows it as the \
             other side of a comparison and as the value a NaN is filled with, in a select as \
             well: a part that it does not evaluate, which a campaign settles by a probe",
        )
        .saying("mul operation not supported"),
        Known::new(
            Kind::WalkRefusesOnly,
            Pattern::new()
                .is("method", "Expr.over(partition_by)")
                .is("keys", "no key"),
            "a group by with no key is one group that Polars evaluates as it evaluates a select, \
             which takes an aggregate as the key of a window, where the tables measured the groups \
             of a key",
        )
        .saying("non-elementwise"),
        Known::new(
            Kind::WalkRefusesOnly,
            Pattern::new().is("argument", "null"),
            "the tables measured a count, a share, a fill and a length as a literal of a number, \
             and a null literal converts to none, which Polars takes, in a select as well",
        )
        .saying("to `null` failed"),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().one_of(
                "method",
                ["Expr.quantile(quantile)", "Expr.approx_quantile(quantile)"],
            ),
            "the argument of the grid is a column of the values 3, 4 and 5, or the first of them, \
             and the share of a quantile is between 0 and 1, which only the data tell (the values \
             no table holds are tracked by #224)",
        )
        .saying("should be between"),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().is("method", "Expr.gather(indices)"),
            "an index that the table's values give lies beyond the group the values of which it \
             gathers from, which the data decide",
        )
        .saying("out of bounds"),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().one_of(
                "method",
                ["Expr.replace_strict(old)", "Expr.replace_strict(new)"],
            ),
            "a strict replacement that is given no default refuses a value of the data that it does \
             not list, which the walk cannot know (the limit a-strict-replacement-given-no-default \
             holds it where a campaign meets it)",
        )
        .saying("incomplete mapping"),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().one_of(
                "method",
                [
                    "Expr.replace_strict(default)",
                    "Expr.str.replace(value)",
                    "Expr.str.replace_all(value)",
                    "Expr.sort_by(by)",
                ],
            ),
            "one value beside the values of a group, or the values of a group beside one, are of \
             another length where the group has more than one row, and Polars refuses it then and \
             takes it for a group of one row, which the data decide",
        )
        .saying_one_of(&[
            "must have the same length",
            "does not match string column length",
            "must have matching group lengths",
            "`sort_by` produced different length",
        ]),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new()
                .one_of("method", ["Expr.head(n)", "Expr.limit(n)", "Expr.tail(n)"])
                .is("argument", "column"),
            "Polars refuses a count that is given the values of a column, in a select as well, so \
             the cell is not the group's: the catalogue does not take the count of head, limit and \
             tail as one value (candidate to file)",
        )
        .saying("must be a scalar value"),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().is("receiver", "literal"),
            "a call that maps one value, a literal, given the values of a group, gives fewer values \
             than the group, which Polars refuses in a group and allows in a select, and the walk \
             does not follow the length it gives (candidate to file)",
        )
        .saying("output length of `map`"),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().is("method", "Expr.str.replace_all(value)"),
            "Polars refuses a null as what a replacement of every match puts, given as a null \
             literal or as a column that holds one, which the walk reads as a value of any dtype, \
             in a select as well (the limit a-replacement-of-every-match-given-a-null-value holds \
             it where a campaign meets it)",
        )
        .saying("value cannot be 'null' in 'replace' expression"),
        Known::new(
            Kind::Unbuildable,
            Pattern::new()
                .is("method", "Expr.is_in(other)")
                .is("receiver", "null")
                .is("argument", "literal"),
            "Polars never ends on one value looked up in another where the first is a null \
             literal, so no cell can ask it",
        )
        .saying("Polars never ends"),
    ]
}

#[test]
fn a_method_given_values_beside_what_it_follows_in_a_group_is_what_polars_says() {
    let question = Question::any_refusal().at(Depth::Rows);
    Grid::<Given>::new("kernel-methods")
        .axis(keyed(), |point, keyed| point.keyed = *keyed)
        .axis(
            Choices::of("stands", [("alone", false), ("as an operand", true)]),
            |point, operand| point.stands_as_an_operand = *operand,
        )
        .axis(
            Choices::of("then", [("ends", false), ("goes on to an aggregate", true)]),
            |point, goes_on| point.goes_on = *goes_on,
        )
        .axis(contexts(true), |point, context| {
            point.context = Some(*context)
        })
        .axis(methods(), |point, param| point.method = Some(*param))
        .axis(operands("receiver", true), |point, receiver| {
            point.receiver = Some(*receiver);
        })
        .axis(operands("argument", false), |point, argument| {
            point.argument = Some(*argument);
        })
        .judge(|point| question.judge(given_case(point)))
        .assert_agree(&given_known(), 3_000);
}

// ---------------------------------------------------------------------------
// A condition

#[derive(Default)]
struct Conditioned {
    keyed: bool,
    context: Option<Context>,
    predicate: Option<Value>,
    same: bool,
}

#[test]
fn a_condition_given_a_predicate_the_tables_refuse_is_what_polars_says_in_a_group() {
    let date = column_of(Representative::Date, true);
    let text = column_of(Representative::String, true);
    let predicates = Choices::of(
        "predicate",
        [
            (
                "a Boolean against a date",
                column_then(
                    "f",
                    vec![call("Expr.ge", json!({ "other": column(&date) }))],
                ),
            ),
            (
                "a number against text",
                column_then(
                    "m",
                    vec![call("Expr.ge", json!({ "other": column(&text) }))],
                ),
            ),
            (
                "a number against a number",
                column_then("m", vec![call("Expr.ge", json!({ "other": whole(1) }))]),
            ),
        ],
    );
    let question = Question::any_refusal().at(Depth::Rows);
    Grid::<Conditioned>::new("kernel-conditions")
        .axis(contexts(true), |point, context| point.context = Some(*context))
        .axis(keyed(), |point, keyed| point.keyed = *keyed)
        .axis(predicates, |point, predicate| point.predicate = Some(predicate.clone()))
        .axis(
            Choices::of("values", [("the same", true), ("different", false)]),
            |point, same| point.same = *same,
        )
        .judge(|point| {
            let otherwise = if point.same { column("m") } else { column("n") };
            let condition = chain(
                call(
                    "polars.when",
                    json!({ "predicates": many(vec![point.predicate.clone().unwrap_or_default()]) }),
                ),
                vec![
                    call("When.then", json!({ "statement": column("m") })),
                    call("Then.otherwise", json!({ "statement": otherwise })),
                ],
            );
            question.judge(grouped(
                point.context.unwrap_or(Context::Select),
                point.keyed,
                condition,
                &["Date", "String"],
            ))
        })
        .assert_agree(
            &[Known::new(
                Kind::WalkRefusesOnly,
                Pattern::new().is("values", "the same").one_of(
                    "predicate",
                    ["a Boolean against a date", "a number against text"],
                ),
                "Polars does not evaluate the predicate of a condition whose two values are one \
                 expression, in a select as well, and the tables refuse the comparison that is its \
                 predicate: a part Polars does not evaluate, where the other cells of the grid, \
                 whose values differ, are refused by both",
            )
            .saying("tables of Polars")],
            30,
        );
}
