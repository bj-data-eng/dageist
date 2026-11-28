//! What a group by evaluates on the values of each group, held to Polars over grids on the shared
//! helper in `grid.rs`, both ways, in `agg` and in `having`.
//!
//! A group evaluates a chain with kernels of its own, which differ from a select's by what they
//! are given beside the values they follow. The table has a column of text `t`, of whole numbers
//! `n` (the key, with a group of two rows and one of one), `m` (every value its own) and of dates
//! `d`, and every grid asks Polars the rows.
//!
//! - **Keys of a window**: what a window groups by and what it orders by, in every form that is
//!   elementwise or is not, in `agg` and in `having`.
//! - **Methods given values beside what they follow**: every method of the catalogue that is given
//!   an expression, for each parameter, on a receiver that is a column, an aggregate or a literal,
//!   given a column, an aggregate or a literal. Where a method refuses in a select too, the line
//!   says so: it is not the group's.

use dageist_engine::catalog::{self, Applies, Kind as ParameterKind};
use polars::prelude::*;
use serde_json::{Value, json};

use crate::grid::{Case, Connections, Depth, Grid, Kind, Known, Pattern, Question};
use crate::support::{call, chain, field, field_then, many, named, whole};

fn table() -> DataFrame {
    DataFrame::new(
        3,
        vec![
            Series::new("k".into(), ["a", "b", "a"]).into(),
            Series::new("n".into(), [1_i64, 1, 2]).into(),
            Series::new("m".into(), [3_i64, 4, 5]).into(),
            Series::new("t".into(), ["x", "y", "x"]).into(),
            Series::new("f".into(), [true, false, true]).into(),
            Series::new("d".into(), [1_i32, 2, 3])
                .cast(&DataType::Date)
                .expect("a date")
                .into(),
        ],
    )
    .expect("columns of three rows")
}

fn lit(value: Value) -> Value {
    chain(call("polars.lit", json!({ "value": value })), vec![])
}

fn text_literal(text: &str) -> Value {
    lit(json!({ "string": text }))
}

fn one(entry: &str) -> Value {
    call(entry, json!({}))
}

fn count(entry: &str, rows: i64) -> Value {
    call(entry, json!({ "n": whole(rows) }))
}

fn labelled<T: Clone>(items: &[(&str, T)]) -> Vec<(String, T)> {
    items
        .iter()
        .map(|(label, item)| ((*label).to_owned(), item.clone()))
        .collect()
}

/// A step that groups by `n`, and finishes with what `value` makes in the context: an aggregate
/// of the name `out`, or a predicate that is one value for each group.
fn grouped(context: &str, keyed: bool, value: Value) -> Result<Case, String> {
    // The same expression as an item of a select of the table, which is no group's: what Polars
    // does there is what a cell of a group is told apart from.
    if context == "select" {
        return Case::of_step(
            "LazyFrame.select",
            json!({ "named_exprs": named(vec![("out", value)]) }),
            table(),
            Connections::new(),
        );
    }
    let finish = match context {
        "agg" => vec![call(
            "LazyGroupBy.agg",
            json!({ "named_aggs": named(vec![("out", value)]) }),
        )],
        "having" => {
            let mut value = value;
            value["expression"]["then"]
                .as_array_mut()
                .ok_or("an expression")?
                .extend([one("Expr.is_not_null"), one("Expr.all")]);
            vec![
                call(
                    "LazyGroupBy.having",
                    json!({ "predicates": many(vec![value]) }),
                ),
                call("LazyGroupBy.len", json!({})),
            ]
        }
        other => panic!("no context {other}"),
    };
    Case::of_grouping(
        if keyed {
            json!({ "by": many(vec![field("n")]) })
        } else {
            json!({})
        },
        finish,
        table(),
        Connections::new(),
    )
}

/// `value` with `link` after it.
fn then(value: Value, link: Value) -> Value {
    let Value::Object(mut object) = value else {
        panic!("an expression")
    };
    object["expression"]["then"]
        .as_array_mut()
        .expect("links")
        .push(link);
    Value::Object(object)
}

const CONTEXTS: [(&str, &str); 2] = [("agg", "agg"), ("having", "having")];

/// The contexts of the grid of the methods: a select too, which a cell that is the group's is not
/// refused or allowed in as it is in a group.
const CONTEXTS_AND_SELECT: [(&str, &str); 3] =
    [("agg", "agg"), ("having", "having"), ("select", "select")];

// ---------------------------------------------------------------------------
// The key of a window

#[derive(Default)]
struct Windowed {
    keyed: bool,
    context: &'static str,
    part: &'static str,
    key: &'static str,
}

const KEYS: [&str; 30] = [
    "column k",
    "column n",
    "column m",
    "literal 1",
    "n is_null",
    "n plus 1",
    "n cum_sum",
    "n rle_id",
    "n sort",
    "n shift 1",
    "n head 1",
    "n reverse",
    "n unique",
    "n drop_nulls",
    "n first",
    "n sum",
    "n replace n by n",
    "n replace 1 by 2",
    "n cum_count",
    "n diff",
    "n filter",
    "n over n",
    "n implode",
    "n eq null",
    "n cast",
    "a condition",
    "literal cum_sum",
    "literal head 1",
    "n and an aggregate",
    "k str.to_uppercase",
];

fn keys_of(words: &str) -> Vec<Value> {
    let n = |links: Vec<Value>| field_then("n", links);
    let null = || lit(json!({ "flag": null }));
    vec![match words {
        "column k" => field("k"),
        "column n" => field("n"),
        "column m" => field("m"),
        "literal 1" => lit(json!({ "integer": 1 })),
        "n is_null" => n(vec![one("Expr.is_null")]),
        "n plus 1" => n(vec![call("Expr.add", json!({ "other": whole(1) }))]),
        "n cum_sum" => n(vec![one("Expr.cum_sum")]),
        "n rle_id" => n(vec![one("Expr.rle_id")]),
        "n sort" => n(vec![one("Expr.sort")]),
        "n shift 1" => n(vec![call("Expr.shift", json!({ "n": whole(1) }))]),
        "n head 1" => n(vec![count("Expr.head", 1)]),
        "n reverse" => n(vec![one("Expr.reverse")]),
        "n unique" => n(vec![one("Expr.unique")]),
        "n drop_nulls" => n(vec![one("Expr.drop_nulls")]),
        "n first" => n(vec![one("Expr.first")]),
        "n sum" => n(vec![one("Expr.sum")]),
        "n replace n by n" => n(vec![call(
            "Expr.replace",
            json!({ "old": field("n"), "new": field("n") }),
        )]),
        "n replace 1 by 2" => n(vec![call(
            "Expr.replace",
            json!({ "old": whole(1), "new": whole(2) }),
        )]),
        "n cum_count" => n(vec![one("Expr.cum_count")]),
        "n diff" => n(vec![one("Expr.diff")]),
        "n filter" => n(vec![call(
            "Expr.filter",
            json!({ "predicates": many(vec![n(vec![call(
                "Expr.gt",
                json!({ "other": whole(1) }),
            )])]) }),
        )]),
        "n over n" => n(vec![call(
            "Expr.over",
            json!({ "partition_by": many(vec![field("n")]) }),
        )]),
        "n implode" => n(vec![one("Expr.implode")]),
        "n eq null" => {
            let other = null();
            n(vec![call("Expr.eq", json!({ "other": other }))])
        }
        "n cast" => n(vec![call(
            "Expr.cast",
            json!({ "dtype": { "dtype": "Float64" } }),
        )]),
        "a condition" => chain(
            call(
                "polars.when",
                json!({ "predicates": many(vec![n(vec![call(
                    "Expr.gt",
                    json!({ "other": whole(1) }),
                )])]) }),
            ),
            vec![
                call("When.then", json!({ "statement": whole(1) })),
                call("Then.otherwise", json!({ "statement": whole(2) })),
            ],
        ),
        "literal cum_sum" => then(lit(json!({ "integer": 1 })), one("Expr.cum_sum")),
        "literal head 1" => then(lit(json!({ "integer": 1 })), count("Expr.head", 1)),
        "n and an aggregate" => return vec![n(vec![one("Expr.sum")]), field("n")],
        "k str.to_uppercase" => field_then("k", vec![one("Expr.str.to_uppercase")]),
        other => panic!("no key {other}"),
    }]
}

fn windowed_case(point: &Windowed) -> Result<Case, String> {
    let keys = many(keys_of(point.key));
    let over = match point.part {
        "partition_by" => call("Expr.over", json!({ "partition_by": keys })),
        "order_by" => call(
            "Expr.over",
            json!({ "partition_by": many(vec![field("k")]), "order_by": keys }),
        ),
        other => panic!("no part {other}"),
    };
    let windowed = field_then("m", vec![over]);
    // A predicate is one value for each group, which the sum of the window is.
    grouped(
        point.context,
        point.keyed,
        if point.context == "having" {
            then(
                then(windowed, one("Expr.sum")),
                call("Expr.gt", json!({ "other": whole(0) })),
            )
        } else {
            windowed
        },
    )
}

fn windowed_known() -> Vec<Known> {
    vec![
        Known::new(
            Kind::WalkRefusesOnly,
            Pattern::new()
                .is("keys", "a key")
                .is("part", "order_by")
                .is("key", "literal cum_sum"),
            "Polars reads the cumulative sum of a literal as a literal where it orders a window, and refuses it where it groups one, as it does the one of a column; the tables measured the cumulative sum of a column, and the walk does not fold the literal",
        )
        .saying("non-elementwise"),
        Known::new(
            Kind::WalkRefusesOnly,
            Pattern::new().is("keys", "no key"),
            "a group by with no key is one group that Polars evaluates as it evaluates a select, which accepts a window whose key is not elementwise, where the tables measured the groups of a key",
        )
        .saying("non-elementwise"),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new()
                .is("keys", "no key")
                .is("key", "n replace n by n"),
            "the values of the column n repeat, which a replacement of them by themselves refuses in a select as well, so with no key the rule for a key that is not elementwise is not what refuses it",
        )
        .saying("must not contain duplicates"),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new()
                .is("keys", "no key")
                .is("part", "order_by")
                .is("key", "literal head 1"),
            "a window that orders by one value beside the rows is refused for its length in a select as well, so with no key the rule for a key that is not elementwise is not what refuses it",
        )
        .saying("order by expression evaluated to a length"),
    ]
}

#[test]
fn the_key_of_a_window_in_agg_and_in_having_is_what_polars_says() {
    let keys: Vec<(String, &'static str)> =
        KEYS.iter().map(|key| ((*key).to_owned(), *key)).collect();
    let verdicts = Grid::<Windowed>::new()
        .axis("context", labelled(&CONTEXTS), |point, context| {
            point.context = context;
        })
        .axis(
            "keys",
            labelled(&[("a key", true), ("no key", false)]),
            |point, keyed| point.keyed = *keyed,
        )
        .axis(
            "part",
            labelled(&[("partition_by", "partition_by"), ("order_by", "order_by")]),
            |point, part| point.part = part,
        )
        .axis("key", keys, |point, key| point.key = key)
        .judge(&Question::any_refusal().at(Depth::Rows), windowed_case);
    verdicts.assert_agree(&windowed_known(), 100);
}

// ---------------------------------------------------------------------------
// Methods given values beside what they follow

#[derive(Default)]
struct Given {
    keyed: bool,
    goes_on: bool,
    stands: &'static str,
    context: &'static str,
    method: Option<(&'static catalog::Entry, &'static str)>,
    receiver: &'static str,
    argument: &'static str,
}

/// The kind of value a method is given and follows, which its family reads: text, a date, or a
/// number.
fn family_of(id: &str) -> &'static str {
    if id.starts_with("Expr.str.") {
        "text"
    } else if id.starts_with("Expr.dt.") {
        "date"
    } else {
        "number"
    }
}

/// An operand of the family: the column, a literal, or the first value of the column, which an
/// aggregate gives. A receiver is of the family and an argument of the one a parameter takes.
fn operand(family: &str, kind: &str, receiver: bool) -> Value {
    let (column, literal) = match (family, receiver) {
        ("text", _) => ("t", text_literal("x")),
        ("date", true) => (
            "d",
            chain(
                call(
                    "polars.date",
                    json!({ "year": whole(2000), "month": whole(1), "day": whole(1) }),
                ),
                vec![],
            ),
        ),
        (_, true) => ("n", whole(5)),
        (_, false) => ("m", whole(1)),
    };
    match kind {
        "column" => field(column),
        "literal" => literal,
        "aggregate" => field_then(column, vec![one("Expr.first")]),
        "null" => lit(json!({ "flag": null })),
        // What Polars refuses in a select: text multiplied by text.
        "text times text" => {
            field_then("t", vec![call("Expr.mul", json!({ "other": field("t") }))])
        }
        other => panic!("no operand {other}"),
    }
}

/// The methods the grid leaves out: those whose argument is a duration written as text, and those
/// whose argument is a predicate, which no column or aggregate of the table is of text or of
/// `Boolean`.
const LEFT_OUT: [&str; 4] = [
    "Expr.dt.offset_by",
    "Expr.dt.round",
    "Expr.dt.truncate",
    "Expr.filter",
];

/// The parameters that are one more of another: `over` groups by `partition_by` and by what
/// `more_exprs` adds, as `sort_by` sorts by `by` and by `more_by`.
const MORE_OF_ANOTHER: [&str; 2] = ["more_exprs", "more_by"];

/// Every parameter of the catalogue's methods that is given an expression.
fn methods() -> Vec<(String, (&'static catalog::Entry, &'static str))> {
    let mut found = Vec::new();
    for entry in catalog::entries() {
        let id = entry.id().as_str();
        if entry.applies() != Applies::Expression
            || !id.starts_with("Expr.")
            || LEFT_OUT.contains(&id)
        {
            continue;
        }
        for parameter in entry.parameters() {
            if matches!(parameter.kind(), ParameterKind::Expression(_))
                && !MORE_OF_ANOTHER.contains(&parameter.name())
            {
                found.push((
                    format!("{id}.{}", parameter.name()),
                    (entry, parameter.name()),
                ));
            }
        }
    }
    found
}

fn given_case(point: &Given) -> Result<Case, String> {
    let (entry, under_test) = point.method.ok_or("no method")?;
    let id = entry.id().as_str();
    let family = family_of(id);
    // Polars looks one value up in another for ever where the first is a null literal.
    if id == "Expr.is_in" && point.receiver == "null" && point.argument == "literal" {
        return Err("Polars never ends".to_owned());
    }
    let mut given = serde_json::Map::new();
    for parameter in entry.parameters() {
        let value = if parameter.name() == under_test {
            let argument = operand(family, point.argument, false);
            if parameter.shape() == catalog::Shape::Many {
                many(vec![argument])
            } else {
                argument
            }
        } else if parameter.is_needed() {
            match parameter.kind() {
                ParameterKind::Expression(_) if parameter.shape() == catalog::Shape::Many => {
                    many(vec![whole(1)])
                }
                ParameterKind::Expression(_) => whole(1),
                ParameterKind::Value(catalog::ValueType::Integer) => json!({ "integer": 1 }),
                ParameterKind::Value(catalog::ValueType::Float) => json!({ "float": 0.5 }),
                ParameterKind::Value(catalog::ValueType::String) => json!({ "string": "a" }),
                ParameterKind::Choice(words) => json!({ "choice": words[0] }),
                ParameterKind::Flag => json!({ "flag": true }),
                ParameterKind::Pattern => json!({ "pattern": "x" }),
                _ => return Err(format!("{id} needs {}", parameter.name())),
            }
        } else {
            continue;
        };
        given.insert(parameter.name().to_owned(), value);
    }
    let mut made = then(
        operand(family, point.receiver, true),
        call(id, Value::Object(given)),
    );
    // A call that the chain goes on from to an aggregate, which takes one value of what it is given.
    if point.goes_on {
        made = then(made, one("Expr.first"));
    }
    // What stands beside an aggregate is met by it where the chain goes on, as one operand is by
    // another.
    let value = match point.stands {
        "alone" => made,
        "as an operand" => {
            let first = |column: &str| field_then(column, vec![one("Expr.first")]);
            match family {
                "text" => then(first("t"), call("Expr.eq", json!({ "other": made }))),
                "date" => then(first("d"), call("Expr.eq", json!({ "other": made }))),
                _ => then(first("m"), call("Expr.add", json!({ "other": made }))),
            }
        }
        other => panic!("no standing {other}"),
    };
    grouped(point.context, point.keyed, value)
}

fn given_known() -> Vec<Known> {
    vec![
        Known::new(
            Kind::WalkRefusesOnly,
            Pattern::new()
                .one_of("method", &["Expr.replace.old", "Expr.replace_strict.old"])
                .is("argument", "null"),
            "the tables measured the values a replacement replaces as a column of the dtype Null, whose values repeat, which a replacement refuses, and a null literal is one value, which Polars takes, in a select as well",
        )
        .saying("must not contain duplicates"),
        Known::new(
            Kind::WalkRefusesOnly,
            Pattern::new()
                .one_of(
                    "method",
                    &[
                        "Expr.eq.other",
                        "Expr.ne.other",
                        "Expr.gt.other",
                        "Expr.ge.other",
                        "Expr.lt.other",
                        "Expr.le.other",
                        "Expr.fill_nan.value",
                        "Expr.sort_by.by",
                    ],
                )
                .is("argument", "text times text"),
            "the tables refuse the multiplication of text by text, and Polars allows it as the other side of a comparison, as the value a NaN is filled with and as a sort key, in a select as well: a part that it does not evaluate, which a campaign settles by a probe",
        )
        .saying("mul operation not supported"),
        Known::new(
            Kind::WalkRefusesOnly,
            Pattern::new()
                .is("method", "Expr.over.partition_by")
                .is("keys", "no key"),
            "a group by with no key is one group that Polars evaluates as it evaluates a select, which takes an aggregate as the key of a window, where the tables measured the groups of a key",
        )
        .saying("non-elementwise"),
        Known::new(
            Kind::WalkRefusesOnly,
            Pattern::new()
                .one_of(
                    "method",
                    &[
                        "Expr.approx_quantile.quantile",
                        "Expr.head.n",
                        "Expr.limit.n",
                        "Expr.pct_change.n",
                        "Expr.quantile.quantile",
                        "Expr.shift.fill_value",
                        "Expr.slice.length",
                    ],
                )
                .is("argument", "null"),
            "the tables measured a count, a share, a fill and a length as a literal of a number, and a null literal converts to none, which Polars takes, in a select as well",
        )
        .saying("to `null` failed"),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new()
                .one_of(
                    "method",
                    &[
                        "Expr.floordiv.other",
                        "Expr.gather.indices",
                        "Expr.shift.n",
                        "Expr.str.replace_all.value",
                    ],
                )
                .is("argument", "null"),
            "Polars refuses a null literal as what a literal is divided by (and panics), as an index, as the count of a shift and as the value of a replacement of every match, which the walk reads as a value of any dtype, in a select as well (the limit a-replacement-of-every-match-given-a-null-value holds the last where a campaign meets it)",
        ),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new()
                .one_of("method", &["Expr.replace.new", "Expr.replace_strict.new"])
                .is("receiver", "literal")
                .is("argument", "column"),
            "a replacement of a literal by the values of a column has as many new values as old ones, or one, which the rule replacement-values-match-in-number does not reach beside a literal, in a select as well",
        )
        .saying("must have the same length as `old`"),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new()
                .is("keys", "no key")
                .is("context", "agg")
                .one_of(
                    "method",
                    &[
                        "Expr.replace_strict.default",
                        "Expr.sort_by.by",
                        "Expr.str.replace.value",
                        "Expr.str.replace_all.value",
                    ],
                )
                .is("argument", "column"),
            "a group by with no key is one group that Polars evaluates for the most part as it evaluates a select, as the maps of a literal beside the values of the group show, and these kernels still refuse the values of the group beside a literal or an aggregate where the other calls do not",
        )
        .saying_one_of(&[
            "`default` input for `replace_strict` must have the same length",
            "lengths don't match: `sort_by`",
            "replacement value length",
        ]),
        Known::new(
            Kind::Unbuildable,
            Pattern::new()
                .is("method", "Expr.is_in.other")
                .is("receiver", "null")
                .is("argument", "literal"),
            "Polars never ends on one value looked up in another where the first is a null literal, so no cell can ask it",
        )
        .saying("Polars never ends"),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new()
                .one_of(
                    "method",
                    &[
                        "Expr.and_.others",
                        "Expr.or_.others",
                        "Expr.xor.other",
                        "Expr.floordiv.other",
                    ],
                )
                .is("receiver", "null"),
            "Polars has no kernel of a bitwise call or of a floor division for the dtype Null where a group evaluates it, which the tables measured with a column of a dtype: what the kernels of a group refuse of a null literal is tracked by #245",
        )
        .saying("not supported for dtype `null`"),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new()
                .one_of("receiver", &["literal", "null"])
                .is("then", "goes on to an aggregate"),
            "where an aggregate takes one value of what a call makes of a literal, Polars refuses the map of one value beside many, and a sort of a literal by many, that it allows where the call ends, and allows the quantile of one in a campaign: what the aggregate meets of the call is not the call's rule, which the walk does not follow",
        )
        .saying_one_of(&[
            "output length of `map`",
            "lengths don't match: `sort_by`",
            "replacement value length",
            "`default` input for `replace_strict` must have the same length",
        ]),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().is("method", "Expr.gather.indices"),
            "an index that the table's values give lies beyond the group the values of which it gathers from, which the data decide",
        )
        .saying("out of bounds"),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().one_of("method", &["Expr.replace_strict.old", "Expr.replace_strict.new"]),
            "a strict replacement that is given no default refuses a value of the data that it does not list, which the walk cannot know (the limit a-strict-replacement-given-no-default holds it where a campaign meets it)",
        )
        .saying("incomplete mapping"),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().one_of(
                "method",
                &["Expr.quantile.quantile", "Expr.approx_quantile.quantile"],
            ),
            "the argument of the grid is a column of the values 3, 4 and 5, or the first of them, and the share of a quantile is between 0 and 1, which only the data tell (the values no table holds are tracked by #224)",
        )
        .saying("should be between"),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().one_of(
                "method",
                &[
                    "Expr.head.n",
                    "Expr.limit.n",
                    "Expr.tail.n",
                    "Expr.quantile.quantile",
                    "Expr.approx_quantile.quantile",
                    "Expr.shift.n",
                    "Expr.shift.fill_value",
                ],
            ),
            "Polars refuses a count or a share that is given the values of a column in a select as well, so the cell is not the group's: the catalogue does not take the count of head, limit and tail as one value, and the rule parameter-takes-one-value does not reach a receiver that is a literal",
        )
        .saying("must be a scalar value"),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new()
                .one_of(
                    "method",
                    &[
                        "Expr.replace_strict.default",
                        "Expr.str.replace.value",
                        "Expr.str.replace_all.value",
                        "Expr.sort_by.by",
                    ],
                )
                .is("receiver", "aggregate")
                .is("argument", "column"),
            "one value beside the values of a group is of another length where the group has more than one row, and Polars refuses it then and takes it for a group of one row, which the data decide",
        )
        .saying_one_of(&[
            "must have the same length",
            "does not match string column length",
            "must have matching group lengths",
        ]),
        Known::new(
            Kind::WalkRefusesOnly,
            Pattern::new().is("method", "Expr.is_in.other"),
            "the rule is-in-takes-a-list refuses by design the form that Polars deprecates, a value looked up in a collection nested as deeply as it, which Polars still accepts in a select as well",
        )
        .saying("is-in-takes-a-list"),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new()
                .is("method", "Expr.is_in.other")
                .is("receiver", "literal")
                .is("argument", "literal"),
            "Polars panics on one value looked up in another, in a select as well, which the rule is-in-takes-a-list names and the walk does not reach where both are literals typed without a dtype",
        )
        .saying("panicked"),
    ]
}

#[test]
fn a_method_given_values_beside_what_it_follows_in_a_group_is_what_polars_says() {
    let verdicts = Grid::<Given>::new()
        .axis(
            "keys",
            labelled(&[("a key", true), ("no key", false)]),
            |point, keyed| point.keyed = *keyed,
        )
        .axis(
            "stands",
            labelled(&[("alone", "alone"), ("as an operand", "as an operand")]),
            |point, stands| point.stands = stands,
        )
        .axis(
            "then",
            labelled(&[("ends", false), ("goes on to an aggregate", true)]),
            |point, goes_on| point.goes_on = *goes_on,
        )
        .axis(
            "context",
            labelled(&CONTEXTS_AND_SELECT),
            |point, context| {
                point.context = context;
            },
        )
        .axis("method", methods(), |point, method| {
            point.method = Some(*method);
        })
        .axis(
            "receiver",
            labelled(&[
                ("column", "column"),
                ("literal", "literal"),
                ("aggregate", "aggregate"),
                ("null", "null"),
            ]),
            |point, receiver| point.receiver = receiver,
        )
        .axis(
            "argument",
            labelled(&[
                ("column", "column"),
                ("aggregate", "aggregate"),
                ("literal", "literal"),
                ("null", "null"),
                ("text times text", "text times text"),
            ]),
            |point, argument| point.argument = argument,
        )
        .judge(&Question::any_refusal().at(Depth::Rows), given_case);
    verdicts.assert_agree(&given_known(), 3_000);
}

// ---------------------------------------------------------------------------
// A condition

#[derive(Default)]
struct Conditioned {
    keyed: bool,
    context: &'static str,
    predicate: &'static str,
    values: &'static str,
}

fn conditioned_case(point: &Conditioned) -> Result<Case, String> {
    let predicate = match point.predicate {
        "a Boolean against a date" => {
            field_then("f", vec![call("Expr.ge", json!({ "other": field("d") }))])
        }
        "a number against text" => {
            field_then("m", vec![call("Expr.ge", json!({ "other": field("t") }))])
        }
        "a number against a number" => {
            field_then("m", vec![call("Expr.ge", json!({ "other": whole(1) }))])
        }
        other => panic!("no predicate {other}"),
    };
    let otherwise = match point.values {
        "the same" => field("m"),
        "different" => field("n"),
        other => panic!("no values {other}"),
    };
    let condition = chain(
        call(
            "polars.when",
            json!({ "predicates": many(vec![predicate]) }),
        ),
        vec![
            call("When.then", json!({ "statement": field("m") })),
            call("Then.otherwise", json!({ "statement": otherwise })),
        ],
    );
    grouped(point.context, point.keyed, condition)
}

#[test]
fn a_condition_given_a_predicate_the_tables_refuse_is_what_polars_says_in_a_group() {
    let verdicts = Grid::<Conditioned>::new()
        .axis(
            "context",
            labelled(&CONTEXTS_AND_SELECT),
            |point, context| {
                point.context = context;
            },
        )
        .axis(
            "keys",
            labelled(&[("a key", true), ("no key", false)]),
            |point, keyed| point.keyed = *keyed,
        )
        .axis(
            "predicate",
            labelled(&[
                ("a Boolean against a date", "a Boolean against a date"),
                ("a number against text", "a number against text"),
                ("a number against a number", "a number against a number"),
            ]),
            |point, predicate| point.predicate = predicate,
        )
        .axis(
            "values",
            labelled(&[("the same", "the same"), ("different", "different")]),
            |point, values| point.values = values,
        )
        .judge(&Question::any_refusal().at(Depth::Rows), conditioned_case);
    verdicts.assert_agree(
        &[Known::new(
            Kind::WalkRefusesOnly,
            Pattern::new().is("values", "the same").one_of(
                "predicate",
                &["a Boolean against a date", "a number against text"],
            ),
            "Polars does not evaluate the predicate of a condition whose two values are one expression, in a select as well, and the tables refuse the comparison that is its predicate: a part Polars does not evaluate, where the other cells of the grid, whose values differ, are refused by both",
        )
        .saying("tables of Polars")],
        30,
    );
}
