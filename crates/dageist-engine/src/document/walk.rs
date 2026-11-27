//! What the tables of allowed combinations say of a step, without running
//! Polars: where each expression in it stands, what each call of it follows,
//! whether each call may stand there and whether the expression may end.
//!
//! The walk follows the document as running it does: the arguments of a call
//! first, each expression start by link, the way it is placed in the named
//! positions that hold it. Every call of an expression is asked of
//! [`allowed::offer`], at the place the chain has come to, with the dtypes of
//! the arguments chosen for its expression parameters, the options it is given,
//! and, for a start, the dtype of the column or literal it reads. Every
//! expression that a position holds is asked whether it may end there, and a
//! join's key whether it holds an aggregate beside a column, or a column compared
//! with a null. A join's keys are also asked whether they pair the same two output
//! names twice, which Polars refuses, and `on` is walked against both tables, as
//! Polars reads it. The options a step is given are asked of [`allowed::options`].
//!
//! The tables are measured, so what they cannot say is not guessed: a chain
//! whose dtype is not known where a method follows it is not asked from there
//! on, and the walk says it cannot tell.

use polars::prelude::{DataType, Schema, SchemaRef, TimeUnit};

use crate::allowed::{
    self, After, Chosen, ConditionState, Context, Count, Ending, Fields, Input, Literal, Nested,
    Outer, Place as OfferPlace, Position as Slot, Refusal, Representative, Shape, Unknown, Verdict,
};
use crate::allowed::{
    Loose, Receiver, adapted_dtype, adapts_numbers_only, broadcasts, could_be_taken_by_a_number,
    is_count, is_left_unresolved, is_refused_as_a_count, is_required, keeps_its_dtype,
    operand_adapts, receiver_beside,
};
use crate::catalog::{self, Applies, AsManyAs, Comparison, Counting, Entry, Kind, Role};

use super::error::{Place, Segment};
use super::inputs::Inputs;
use super::model::{Call, Expression, Start, Step, Value};
use super::ranges::{Made, made_by};

/// A call of an expression: what it follows, and whether it may stand there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Standing {
    follows: Option<After>,
    verdict: Option<Verdict>,
}

impl Standing {
    const fn not_asked() -> Self {
        Self {
            follows: None,
            verdict: None,
        }
    }

    /// What the call follows: nothing for a start, otherwise the value or the
    /// condition before it. `None` where that is not known, because the dtype
    /// of what comes before it is not.
    #[must_use]
    pub const fn follows(&self) -> Option<After> {
        self.follows
    }

    /// Whether the call may stand there and what it gives, as [`allowed::offer`]
    /// answered. `None` where it was not asked. Where the dtype of an argument
    /// was not known it was asked with the arguments that were, so what it says
    /// the call gives is not what the chain has; the places say that.
    #[must_use]
    pub const fn verdict(&self) -> Option<&Verdict> {
        self.verdict.as_ref()
    }
}

/// An expression of a step, as the walk found it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkedExpression {
    place: Place,
    context: Option<Context>,
    calls: Vec<Standing>,
    next: Option<After>,
    ending: Option<Ending>,
    count: Option<Count>,
    fields: Vec<Option<Fields>>,
    output_name: Option<String>,
}

impl WalkedExpression {
    /// Where the expression is: the step, then the way to the argument that
    /// holds it.
    #[must_use]
    pub const fn place(&self) -> &Place {
        &self.place
    }

    /// The context the expression stands in. `None` when no named position
    /// holds it.
    #[must_use]
    pub const fn context(&self) -> Option<Context> {
        self.context
    }

    /// The start of the expression, then each of its links.
    #[must_use]
    pub fn calls(&self) -> &[Standing] {
        &self.calls
    }

    /// The places the links stand in, in order, and after them the place a link
    /// added at the end would stand in. `None` where the context or what comes
    /// before is not known. They are what [`allowed::offers`] is asked at.
    #[must_use]
    pub fn places(&self) -> Vec<Option<OfferPlace>> {
        let follows = self
            .calls
            .iter()
            .skip(1)
            .map(Standing::follows)
            .chain(std::iter::once(self.next));
        follows
            .map(|after| Some(OfferPlace::new(self.context?, after?)))
            .collect()
    }

    /// The fields of the struct that each of [`places`](Self::places) follows, in the same order,
    /// and none where the value there is no struct or the walk does not know what it holds.
    #[must_use]
    pub fn fields(&self) -> &[Option<Fields>] {
        &self.fields
    }

    /// Whether the expression may end where it does, when a position holds it
    /// and the walk could ask.
    #[must_use]
    pub const fn ending(&self) -> Option<&Ending> {
        self.ending.as_ref()
    }

    /// How many values the expression gives, where that is neither one value nor as many as the
    /// table has rows: what the arguments fix of it, such as `head(2)` of a column giving at most
    /// two values, and where they fix nothing, that the count is not known. `None` where the
    /// expression gives one value or as many as the rows, or where the walk could not say what it
    /// gives.
    #[must_use]
    pub const fn count(&self) -> Option<Count> {
        self.count
    }
}

/// A refusal the walk found, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkRefusal {
    place: Place,
    refusal: Refusal,
    at_end: bool,
    argument_unknown: bool,
}

impl WalkRefusal {
    /// Where, in the document.
    #[must_use]
    pub const fn place(&self) -> &Place {
        &self.place
    }

    /// Who refused it, and why.
    #[must_use]
    pub const fn refusal(&self) -> &Refusal {
        &self.refusal
    }

    /// Whether the expression there may not end where it does, and not that a
    /// call may not stand.
    #[must_use]
    pub const fn at_end(&self) -> bool {
        self.at_end
    }

    /// Whether the call was refused without the dtype of an argument being known:
    /// no dtype that argument might have lets the call stand, and the reason is
    /// the one Polars gave for one of them, not for the dtype the argument has.
    #[must_use]
    pub const fn argument_unknown(&self) -> bool {
        self.argument_unknown
    }
}

/// What the tables say of a step: its expressions, and what is refused.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Walk {
    expressions: Vec<WalkedExpression>,
    refusal: Option<WalkRefusal>,
    not_known: Option<String>,
}

impl Walk {
    /// The expressions of the step, those inside another expression's
    /// arguments first.
    #[must_use]
    pub fn expressions(&self) -> &[WalkedExpression] {
        &self.expressions
    }

    /// The first thing refused: a call that may not stand, an expression that
    /// may not end, or an option Polars refuses. A step that holds one is not
    /// run: it is refused at its place and with its words.
    #[must_use]
    pub const fn refusal(&self) -> Option<&WalkRefusal> {
        self.refusal.as_ref()
    }

    /// Why the tables could not be asked past a point, where they could not.
    #[must_use]
    pub fn not_known(&self) -> Option<&str> {
        self.not_known.as_deref()
    }
}

impl Step {
    /// What the tables of allowed combinations say of the step as the
    /// `index`th of a document, when it starts from a table with the schema
    /// `source`. `inputs` are what its `table` arguments name.
    #[must_use]
    pub fn walk(&self, index: usize, source: &Schema, inputs: &Inputs) -> Walk {
        crate::startup::ensure();
        let other = self
            .call()
            .argument("other")
            .and_then(|value| match value {
                Value::Table(name) => inputs.table(name),
                _ => None,
            })
            .and_then(|mut table| table.collect_schema().ok());
        let mut walker = Walker {
            zones: Vec::new(),
            fieldsets: Vec::new(),
            gave_no_expression: 0,
            undecided_folds: 0,
            repeated_comparisons: 0,
            keyed_unnests: 0,
            names: Vec::new(),
            written: Vec::new(),
            pair_key: None,
            selected: Vec::new(),
            of_lists: Vec::new(),
            step: self.call(),
            source,
            inputs,
            other,
            base: Place::of_step(index, Some(self.id().clone())),
            path: Vec::new(),
            slots: Vec::new(),
            walked: Vec::new(),
            refusal: None,
            not_known: None,
        };
        walker.arguments(self.call(), false);
        walker.selected_items_meet();
        walker.written_names_are_distinct();
        walker.step_options(self.call());
        walker.nan_test_columns();
        walker.cut_by_a_number();
        walker.key_names();
        Walk {
            expressions: walker.walked,
            refusal: walker.refusal,
            not_known: walker.not_known,
        }
    }
}

/// What a value that a method took of one value is to Polars. It is one value that no row of a group
/// takes as the value of the group, which a window refuses, and one that Polars cannot tell is one
/// before the rows run, which the parameters that must be a scalar refuse. The two are carried by
/// what a call reads, and not by the same parameters of it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Taken {
    /// Whether a window refuses it.
    window: bool,
    /// Whether a parameter that must be a scalar refuses it.
    scalar: bool,
}

impl Taken {
    /// What a method that takes some of the values gives of one value.
    const ALL: Self = Self {
        window: true,
        scalar: true,
    };

    fn or(self, other: Self) -> Self {
        Self {
            window: self.window || other.window,
            scalar: self.scalar || other.scalar,
        }
    }

    /// Only where the value is one: a count and the rows are refused by their shape.
    fn if_one(self, shape: Shape) -> Self {
        if shape == Shape::One {
            self
        } else {
            Self::default()
        }
    }
}

/// What an expression held by an argument has: the class it gives, if known,
/// and whether it gives one value.
#[derive(Clone, Copy)]
struct Operand {
    dtype: Option<Representative>,
    /// Whether it is a number typed without a dtype and alone, which adapts to
    /// the operand it is used with.
    loose: Option<Loose>,
    shape: Shape,
    /// How many values it gives where its shape is [`Shape::Counted`], as far as the arguments
    /// fix it.
    count: Count,
    /// Where it is one value that a method took of one value, or one that meets such a value (the
    /// first none or one of it, which no row of a group takes as the value of the group): which a
    /// window refuses and which a parameter read as one value refuses.
    taken: Taken,
    /// Whether the expression was walked to its end: a refused or unwalked one
    /// has no shape either.
    known: bool,
    /// Whether how many values it gives can be relied on: not where a condition has one
    /// expression for all its values, whose predicate Polars drops where it optimizes the plan
    /// and not where a call that is no elementwise one follows it.
    reliable: bool,
    /// The time zone it is a moment in, where the chain knows it.
    zone: Option<Zone>,
    /// The width of the array it is, where the chain knows it.
    width: Option<usize>,
    /// The fields of the struct it is, where the chain knows them.
    fields: Option<FieldsAt>,
    /// The name it is written under, where the chain knows it.
    name: Option<NameAt>,
}

/// The name an expression is written under that the walk holds, by its place in the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NameAt(usize);

/// The fields of a struct that the walk holds once, by their place in the list, as it holds a time
/// zone by its name: what an operand and a link carry is this handle, which can be copied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FieldsAt(usize);

/// The time zone of a moment: none, or a name that the walk holds once, by its place in the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Zone {
    /// A moment that has none.
    Naive,
    /// A moment in the zone the walker named at that place.
    Named(usize),
}

/// The class of one value of a condition, and whether it is a number typed
/// without a dtype, which adapts to the numbers beside it.
type Valued = (Option<Representative>, Option<Loose>);

/// What surrounds a link of an expression besides the chain before it.
#[derive(Clone, Copy)]
struct Around<'a> {
    /// Whether the values of the condition the link is part of are one expression.
    branches_are_one: bool,
    /// What the chain starts from, when that is a number typed without a dtype and the
    /// link follows it.
    receiver_loose: Option<Loose>,
    /// The unit the moment or the length of time the link follows is counted in, where it is known.
    unit: Option<TimeUnit>,
    /// The width of the array the link follows, where it is known.
    width: Option<usize>,
    /// Whether the value the link follows is one that a part widened: a value of one row that a method
    /// met with each of the many values of a part, which a window refuses.
    widened: bool,
    /// The zone of the moment the link follows, where it is known.
    zone: Option<Zone>,
    /// The fields of the struct the link follows, where they are known.
    fields: Option<FieldsAt>,
    /// Whether the chain the link is part of starts from a connected list.
    from_a_list: bool,
    /// The name the value the link follows is written under, where the chain knows it.
    output_name: Option<&'a str>,
}

/// What a chain has so far.
#[derive(Clone)]
enum Cur {
    Value {
        dtype: Option<Representative>,
        shape: Shape,
        count: Count,
        /// Where it is one value that a method took of one value, or one that meets such a value:
        /// which a window refuses and which a parameter read as one value refuses.
        taken: Taken,
    },
    Condition {
        state: ConditionState,
        values: Vec<Valued>,
        /// What the predicates and values met so far give together: one value meets each of them,
        /// and the others are as many as one another.
        meeting: (Shape, Count),
        /// Where a predicate or a value so far is one that a method took of one value: which a
        /// window refuses and which a parameter read as one value refuses.
        taken: Taken,
        /// The zone of each value, where it is a moment whose zone is known, in the order of `values`.
        zones: Vec<Option<Zone>>,
        /// The width of each value, where it is an array whose width is known, in the order of `values`.
        widths: Vec<Option<usize>>,
        /// The fields of each value, where it is a struct whose fields are known, in the order of
        /// `values`.
        structs: Vec<Option<Fields>>,
        /// Whether every predicate and value so far gives one value, which a
        /// condition of them gives too.
        single: bool,
        /// Whether every value so far gives one value: what the condition is
        /// where its values are one expression, and the predicates are dropped.
        values_single: bool,
        /// Whether every predicate and value so far was walked to its end: a
        /// refused one has no shape, so the condition has none.
        known: bool,
    },
    /// Refused: nothing more is asked of it.
    Dead,
}

struct Walker<'a> {
    step: &'a Call,
    source: &'a Schema,
    inputs: &'a Inputs,
    other: Option<SchemaRef>,
    base: Place,
    path: Vec<Segment>,
    slots: Vec<Slot>,
    walked: Vec<WalkedExpression>,
    refusal: Option<WalkRefusal>,
    not_known: Option<String>,
    /// The names of the zones the walk has met, which a [`Zone::Named`] points into.
    zones: Vec<String>,
    /// The fields of the structs the walk has met, which a [`FieldsAt`] points into.
    fieldsets: Vec<Fields>,
    /// How many expressions gave no expression: an unnest of a struct that has no field.
    gave_no_expression: usize,
    /// How many comparisons of missing values with a number that some dtype cannot hold the walk met
    /// whose dtype of what is compared it could not use, as that is one value, whose dtype decides
    /// what Polars does and which a limit of the fuzz tests covers, or is not known.
    undecided_folds: usize,
    /// How many comparisons of missing values of a column the walk met that Polars decides for no
    /// value of the column, and that end a chain of values for each row: what each leaves is a
    /// constant repeated to the length of the column, wherever it stands in a key.
    repeated_comparisons: usize,
    /// How many unnests stood in a join's key, whose columns are written by how many keys the other
    /// table expands to.
    keyed_unnests: usize,
    /// The names the walk has met expressions written under, which a [`NameAt`] points into.
    names: Vec<String>,
    /// What each item of a select or of the columns a step adds writes: where the item is, the
    /// names it writes where they are known, and whether it is named by a field of a struct.
    written: Vec<(Place, Option<Vec<String>>, bool)>,
    /// The name a pair gives the expression it holds, while that is walked.
    pair_key: Option<String>,
    /// The items of a select, where each is, and what it gives: the values of the items are met
    /// by one another in the table the select gives.
    selected: Vec<(Place, Shape, Count)>,
    /// Where an expression is a chain from a connected list that still gives a count of its values.
    of_lists: Vec<Place>,
}

/// Whether the parameter `parameter` of `step` holds a list of keys, whose items
/// are each a place.
fn left_is_many(step: &Call, parameter: &str) -> bool {
    matches!(step.argument(parameter), Some(Value::Many(_)))
}

/// The output name Polars gives a join's key: the column the expression reads
/// first, whatever methods follow it, `literal` for a literal, and for a
/// condition the name of the first value it gives. `None` where it is not known:
/// a start that expands to several columns or names them by a pattern, an input,
/// a hole. A key is never given a name by an alias, which Polars refuses in a
/// join's key and the catalogue does not hold.
fn key_name(key: &Expression) -> Option<String> {
    // What a field read or an unnest of fields not known here names it by, as the walk does.
    key.links()
        .iter()
        .fold(start_name(key.start(), key.links()), |name, link| {
            name_after(link, name, None)
        })
}

/// The name an expression is written under after `link`, which `before` is the name it was written
/// under before it: reading a field names the value by the field, an unnest of a struct of one
/// field by that field and of several by none, for it writes a column for each, and any other call
/// keeps the name. `followed` is the fields of the struct the call follows, where they are known.
fn name_after(link: &Call, before: Option<String>, followed: Option<&Fields>) -> Option<String> {
    match link.entry().id().as_str() {
        "Expr.struct.field" => {
            let several = matches!(
                link.argument("more_names"),
                Some(Value::Many(more)) if !more.is_empty()
            );
            match link.argument("name") {
                Some(Value::Field(field)) if !several => Some(field.clone()),
                _ => None,
            }
        }
        "Expr.struct.unnest" => {
            let mut names = followed?.names();
            match (names.next(), names.next()) {
                (Some(only), None) => Some(only.to_owned()),
                _ => None,
            }
        }
        _ => before,
    }
}

/// The name that the start of an expression gives it, which `links` tell where a condition's
/// first value is one of them.
fn start_name(start: &Start, links: &[Call]) -> Option<String> {
    let Start::Call(start) = start else {
        return None;
    };
    match start.entry().id().as_str() {
        "polars.col" => {
            if Walker::names_several(start) {
                return None;
            }
            let Some(Value::Columns(names)) = start.argument("name") else {
                return None;
            };
            let name = names.first()?;
            (!(name.starts_with('^') && name.ends_with('$'))).then(|| name.clone())
        }
        "polars.lit" => Some("literal".to_owned()),
        // Python names what these make by an alias.
        "polars.date" => Some("date".to_owned()),
        "polars.time" => Some("time".to_owned()),
        // A count since 1970 is named as the column it counts is.
        "polars.from_epoch" => match start.argument("column") {
            Some(Value::Expression(column)) => key_name(column),
            _ => None,
        },
        // A moment and a length of time are named as the first of the parts they are made of,
        // which is `literal` for one that is a number typed in, unless every part is one: Polars
        // then folds the call into a literal where it can, and names it `datetime` or `duration`,
        // or leaves it as a call where it cannot, which depends on the values.
        "polars.datetime" => {
            let zoned = matches!(start.argument("time_zone"), Some(Value::TimeZone(Some(_))));
            first_part_name(start, "year", &PARTS_OF_A_MOMENT, !zoned)
        }
        "polars.duration" => first_part_name(start, "weeks", &PARTS_OF_A_LENGTH, true),
        // A range is named as the first of the expressions it is given, which Python makes of
        // 0 where `int_range` is given one bound and where a bound is left to its default: `literal`.
        "polars.int_range" if start.argument("end").is_none() => Some("literal".to_owned()),
        "polars.int_range"
        | "polars.date_range"
        | "polars.datetime_range"
        | "polars.time_range"
        | "polars.linear_space" => match start.argument("start") {
            Some(Value::Expression(first)) => key_name(first),
            _ => Some("literal".to_owned()),
        },
        "polars.repeat" => match start.argument("value") {
            Some(Value::Expression(value)) => key_name(value),
            _ => None,
        },
        // A struct is named as the first of the values it is made of, which a name given to a value
        // is where the value is one of the named ones.
        "polars.struct" => struct_inputs(start)
            .into_iter()
            .next()
            .and_then(|(name, value)| name.or_else(|| key_name(value))),
        "polars.when" => links
            .iter()
            .find(|link| link.entry().id().as_str() == "When.then")
            .and_then(|link| match link.argument("statement") {
                Some(Value::Expression(value)) => key_name(value),
                _ => None,
            }),
        _ => None,
    }
}

/// One value a call of `polars.struct` makes a struct of: the name it writes, its class, the fields
/// it holds where it is a struct, whether it is a number typed without a dtype, and the width it has
/// where it is an array whose width is known.
struct StructInput {
    name: Option<String>,
    class: Option<Representative>,
    fields: Option<Fields>,
    loose: bool,
    width: Option<usize>,
}

/// The values a call of `polars.struct` makes a struct of, in order, each with the name it is given
/// where it is one of the named values: those that are listed first, then the named ones.
fn struct_inputs(start: &Call) -> Vec<(Option<String>, &Expression)> {
    fn expressions_of<'a>(value: &'a Value, found: &mut Vec<&'a Expression>) {
        match value {
            Value::Expression(expression) => found.push(expression),
            Value::Many(items) => items.iter().for_each(|item| expressions_of(item, found)),
            _ => {}
        }
    }
    let mut listed = Vec::new();
    if let Some(value) = start.argument("exprs") {
        expressions_of(value, &mut listed);
    }
    let mut inputs: Vec<(Option<String>, &Expression)> = listed
        .into_iter()
        .map(|expression| (None, expression))
        .collect();
    if let Some(Value::Pairs(pairs)) = start.argument("named_exprs") {
        for (key, value) in pairs {
            if let (Value::Columns(names), Value::Expression(expression)) = (key, value)
                && let [name] = names.as_slice()
            {
                inputs.push((Some(name.clone()), expression));
            }
        }
    }
    inputs
}

/// The unit a call is given for the parameter `parameter`, one of the three a time unit has, if it
/// is given one.
fn time_unit_word(call: &Call, parameter: &str) -> Option<String> {
    match call.argument(parameter) {
        Some(Value::Choice(word)) => Some(word.clone()),
        _ => None,
    }
}

/// The dtype a function that makes a date, a time, a moment or a length of time gives, where its
/// arguments decide it: the unit and the zone it is given, which Python reads as it builds it,
/// and the unit a count since 1970 is read in.
fn made_dtype(start: &Call) -> Option<DataType> {
    let unit = |word: &str| match word {
        "ns" => TimeUnit::Nanoseconds,
        "ms" => TimeUnit::Milliseconds,
        _ => TimeUnit::Microseconds,
    };
    match start.entry().id().as_str() {
        "polars.date" => Some(DataType::Date),
        "polars.time" => Some(DataType::Time),
        "polars.datetime" => {
            let unit = unit(&time_unit_word(start, "time_unit").unwrap_or_else(|| "us".to_owned()));
            let zone = match start.argument("time_zone") {
                Some(Value::TimeZone(Some(text))) => crate::catalog::check_time_zone(text).ok()?,
                _ => None,
            };
            Some(DataType::Datetime(unit, zone))
        }
        "polars.duration" => Some(DataType::Duration(
            match time_unit_word(start, "time_unit") {
                Some(word) => unit(&word),
                None if start.argument("nanoseconds").is_some() => TimeUnit::Nanoseconds,
                None => TimeUnit::Microseconds,
            },
        )),
        "polars.int_range" => Some(match start.argument("dtype") {
            Some(Value::Dtype(dtype)) => dtype.as_polars().clone(),
            _ => DataType::Int64,
        }),
        "polars.date_range" => Some(DataType::Date),
        "polars.time_range" => Some(DataType::Time),
        // The unit and the zone of moments that a range makes are those of its bounds unless it
        // is told: they are known here where the call tells them, and by the bounds otherwise.
        "polars.datetime_range" => {
            let interval = match start.argument("interval") {
                Some(Value::Duration(text)) => text.as_str(),
                _ => "1d",
            };
            let unit = match time_unit_word(start, "time_unit") {
                Some(word) => unit(&word),
                None if interval.contains("ns") => TimeUnit::Nanoseconds,
                None => return None,
            };
            let zone = match start.argument("time_zone") {
                Some(Value::TimeZone(Some(text))) => crate::catalog::check_time_zone(text).ok()?,
                _ => return None,
            };
            Some(DataType::Datetime(unit, zone))
        }
        "polars.from_epoch" => Some(match time_unit_word(start, "time_unit").as_deref() {
            Some("d") => DataType::Date,
            Some("ns") => DataType::Datetime(TimeUnit::Nanoseconds, None),
            _ => DataType::Datetime(TimeUnit::Microseconds, None),
        }),
        _ => None,
    }
}

/// The parts a moment is made of, which `polars.datetime` folds into a literal where all are.
const PARTS_OF_A_MOMENT: [&str; 7] = [
    "year",
    "month",
    "day",
    "hour",
    "minute",
    "second",
    "microsecond",
];

/// The parts a length of time is made of, which `polars.duration` folds into a literal where all
/// are.
const PARTS_OF_A_LENGTH: [&str; 8] = [
    "weeks",
    "days",
    "hours",
    "minutes",
    "seconds",
    "milliseconds",
    "microseconds",
    "nanoseconds",
];

/// The output name of a function that makes a moment or a length of time of `parts`, which is the
/// name of the first part it is given, `literal` for one that is a number typed in or is not given.
/// Where `may_fold` and every part is a number typed in and nothing more, Polars makes a literal
/// of the call where the values fit, which it names otherwise, so the name is not known.
fn first_part_name(start: &Call, first: &str, parts: &[&str], may_fold: bool) -> Option<String> {
    // A part that Polars may fold: a number, or a flag that it reads as one, typed in. A null and
    // text are no number, so a call that holds one stays a call. A flag is taken for foldable here
    // and not by `is_folded_into_a_literal`, on purpose: the name is then not known, which only
    // leaves the rule for repeated names silent, where taking a flag for folded there would refuse
    // a key that Polars may allow. The two disagree only on the side of saying less.
    let bare_literal = |value: Option<&Value>| match value {
        None => true,
        Some(Value::Expression(part)) => {
            part.links().is_empty()
                && matches!(part.start(), Start::Call(call)
                if call.entry().id().as_str() == "polars.lit"
                    && (matches!(
                        call.argument("value"),
                        Some(Value::Integer(_) | Value::Float(_) | Value::Flag(Some(_)))
                    ) || is_a_number_of_a_dtype(call)))
        }
        Some(_) => false,
    };
    if may_fold && parts.iter().all(|part| bare_literal(start.argument(part))) {
        return None;
    }
    match start.argument(first) {
        Some(Value::Expression(part)) => key_name(part),
        _ => Some("literal".to_owned()),
    }
}

/// Whether the literal is a text of a number given no dtype, which Polars reads as the number it
/// is where a part of a moment is given one.
fn is_a_text_of_a_number(call: &Call) -> bool {
    call.argument("dtype").is_none()
        && matches!(call.argument("value"), Some(Value::String(text))
            if text.trim().parse::<f64>().is_ok_and(f64::is_finite))
}

/// Whether the literal is a number given a dtype of numbers, as the step engine makes it: the value
/// cast strictly to the dtype, so a float or a text of a number given a dtype of whole numbers is
/// the whole number it casts to, which Polars folds as it does a number typed in.
fn is_a_number_of_a_dtype(call: &Call) -> bool {
    use polars::prelude::DataType as D;
    let (Some(value), Some(Value::Dtype(dtype))) = (call.argument("value"), call.argument("dtype"))
    else {
        return false;
    };
    matches!(
        dtype.as_polars(),
        D::Int8
            | D::Int16
            | D::Int32
            | D::Int64
            | D::Int128
            | D::UInt8
            | D::UInt16
            | D::UInt32
            | D::UInt64
            | D::UInt128
            | D::Float16
            | D::Float32
            | D::Float64
    ) && catalog::typed_literal(&value.placeholder(), dtype.as_polars())
        .is_ok_and(|scalar| !scalar.is_null())
}

/// Whether Polars folds the call into a literal, which it names by an alias: a moment, with no
/// zone, or a length of time whose every part is a number typed in, given or left to the number 0.
/// The values may stop it, which the document does not decide: a month of 13 is no moment, and
/// Polars then keeps the call and refuses it when the rows run.
fn is_folded_into_a_literal(start: &Call) -> bool {
    let (parts, zoned): (&[&str], bool) = match start.entry().id().as_str() {
        "polars.datetime" => (
            &PARTS_OF_A_MOMENT,
            matches!(start.argument("time_zone"), Some(Value::TimeZone(Some(_)))),
        ),
        "polars.duration" => (&PARTS_OF_A_LENGTH, false),
        _ => return false,
    };
    let a_number = |value: Option<&Value>| match value {
        None => true,
        Some(Value::Expression(part)) => {
            part.links().is_empty()
                && matches!(part.start(), Start::Call(call)
                    if call.entry().id().as_str() == "polars.lit"
                        && (matches!(call.argument("value"), Some(Value::Integer(_) | Value::Float(_)))
                            || is_a_text_of_a_number(call)
                            || is_a_number_of_a_dtype(call)))
        }
        Some(_) => false,
    };
    !zoned && parts.iter().all(|part| a_number(start.argument(part)))
}

/// Why the two bounds of a linear space are of dtypes that have none in common, where both are known:
/// numbers meet in a float, and moments, times of day and lengths of time only in their own dtype.
fn incompatible_bounds(start: &Call, arguments: &[(String, Operand)]) -> Option<String> {
    use Representative as R;
    if start.entry().id().as_str() != "polars.linear_space" {
        return None;
    }
    let class_of = |name: &str| {
        arguments
            .iter()
            .find(|(given, _)| given == name)
            .and_then(|(_, operand)| operand.dtype)
    };
    // A number typed without a dtype takes the dtype of the number beside it, and is no other.
    let loose = |name: &str| {
        arguments
            .iter()
            .any(|(given, operand)| given == name && operand.loose.is_some())
    };
    let (first, last) = match (class_of("start"), class_of("end")) {
        (Some(first), Some(last)) => (first, last),
        (Some(known), None) if loose("end") => (known, R::Int64),
        (None, Some(known)) if loose("start") => (R::Int64, known),
        _ => return None,
    };
    let number = |class: R| {
        matches!(
            class,
            R::Int8
                | R::Int16
                | R::Int32
                | R::Int64
                | R::Int128
                | R::UInt8
                | R::UInt16
                | R::UInt32
                | R::UInt64
                | R::UInt128
                | R::Float16
                | R::Float32
                | R::Float64
        )
    };
    let compatible = (number(first) && number(last))
        || (first == last
            && matches!(
                first,
                R::Date | R::Datetime | R::DatetimeZoned | R::Time | R::Duration
            ));
    (!compatible)
        .then(|| format!("'start' and 'end' have incompatible dtypes, got {first} and {last}"))
}

/// The class of what a range or a repeat gives where it is the arguments that decide it: the dtype
/// a repeat is given or the dtype of its value, the number or the moment a linear space is made of,
/// and the moments of a range of moments, in the zone they are given or that their bounds have.
fn range_class(start: &Call, arguments: &[(String, Operand)]) -> Option<Representative> {
    use Representative as R;
    let class_of = |name: &str| {
        arguments
            .iter()
            .find(|(given, _)| given == name)
            .and_then(|(_, operand)| operand.dtype)
    };
    // A number typed without a dtype takes the one it is met with.
    let is_a_number = |name: &str| {
        arguments.iter().any(|(given, operand)| {
            given == name
                && (operand.loose.is_some()
                    || operand.dtype.is_some_and(|class| {
                        matches!(
                            class,
                            R::Int8
                                | R::Int16
                                | R::Int32
                                | R::Int64
                                | R::Int128
                                | R::UInt8
                                | R::UInt16
                                | R::UInt32
                                | R::UInt64
                                | R::UInt128
                                | R::Float16
                                | R::Float32
                                | R::Float64
                        )
                    }))
        })
    };
    match start.entry().id().as_str() {
        "polars.repeat" => match start.argument("dtype") {
            Some(Value::Dtype(dtype)) => Representative::of(dtype.as_polars()),
            _ => class_of("value"),
        },
        "polars.linear_space" => match (class_of("start"), class_of("end")) {
            (Some(R::Float32), Some(R::Float32)) => Some(R::Float32),
            (Some(R::Float16), Some(R::Float16)) => Some(R::Float16),
            (Some(R::Date), Some(R::Date)) => Some(R::Datetime),
            (Some(first), Some(last))
                if first == last
                    && matches!(
                        first,
                        R::Datetime | R::DatetimeZoned | R::Time | R::Duration
                    ) =>
            {
                Some(first)
            }
            _ if is_a_number("start") && is_a_number("end") => Some(R::Float64),
            _ => None,
        },
        "polars.datetime_range" => {
            let zone_given = matches!(
                start.argument("time_zone"),
                Some(Value::TimeZone(Some(text)))
                    if matches!(crate::catalog::check_time_zone(text), Ok(Some(_)))
            );
            match (class_of("start"), class_of("end")) {
                _ if zone_given => Some(R::DatetimeZoned),
                (Some(R::DatetimeZoned), _) | (_, Some(R::DatetimeZoned)) => Some(R::DatetimeZoned),
                (Some(R::Date | R::Datetime), Some(R::Date | R::Datetime)) => Some(R::Datetime),
                _ => None,
            }
        }
        _ => None,
    }
}

/// The word of a flag or a choice, as the option table writes it.
fn option_word(value: &Value) -> Option<String> {
    match value {
        Value::Flag(Some(true)) => Some("true".to_owned()),
        Value::Flag(Some(false)) => Some("false".to_owned()),
        Value::Flag(None) => Some("unset".to_owned()),
        Value::Choice(word) => Some(word.clone()),
        _ => None,
    }
}

/// The options `call` was given, each by the parameter and its word as the option table writes
/// it: a flag, a choice, and a dtype, which the table writes as the name of its class, such as
/// `Int64`. A dtype that is none of the classes has no word, and is reported apart: what the
/// call gives depends on it, and the tables say nothing of it.
fn option_words(call: &Call) -> (Vec<(String, String)>, bool) {
    let entry = call.entry();
    let mut unknown = false;
    let mut words = Vec::new();
    for given in call.arguments() {
        let Some(parameter) = entry
            .parameters()
            .iter()
            .find(|parameter| parameter.name() == given.parameter())
        else {
            continue;
        };
        let word = match (parameter.kind(), given.value()) {
            (Kind::Flag | Kind::Choice(_), value) => option_word(value),
            (Kind::Dtype, Value::Dtype(dtype)) => {
                let class = Representative::of(dtype.as_polars());
                unknown |= class.is_none();
                class.map(|class| class.name().to_owned())
            }
            _ => None,
        };
        words.extend(word.map(|word| (given.parameter().to_owned(), word)));
    }
    (words, unknown)
}

/// Whether the chain has a value that a part widened after `link`: the link met a receiver of one
/// value with each of the many values a part gives, where the part is not met so in a window, or the
/// value was widened before and is still many.
fn widens(
    link: &Call,
    arguments: &[(String, Operand)],
    receiver: Option<Shape>,
    next: &Cur,
    before: bool,
) -> bool {
    let widens_here = receiver == Some(Shape::One)
        && arguments.iter().any(|(name, operand)| {
            operand.shape == Shape::Many
                && link.entry().parameters().iter().any(|parameter| {
                    parameter.name() == name && !parameter.broadcasts_in_a_window()
                })
        });
    let many = matches!(
        next,
        Cur::Value {
            shape: Shape::Many,
            ..
        }
    );
    widens_here || (before && many)
}

/// The unit a moment or a length of time of the dtype is counted in.
const fn unit_of(dtype: &DataType) -> Option<TimeUnit> {
    match dtype {
        DataType::Datetime(unit, _) | DataType::Duration(unit) => Some(*unit),
        _ => None,
    }
}

/// The unit the word the tables write for a dtype, such as `Datetime(ns, UTC)`, counts in.
fn unit_in_name(name: &str) -> Option<TimeUnit> {
    let inside = name
        .strip_prefix("Datetime(")
        .or_else(|| name.strip_prefix("Duration("))?;
    match inside.split([',', ')']).next()? {
        "ns" => Some(TimeUnit::Nanoseconds),
        "us" => Some(TimeUnit::Microseconds),
        "ms" => Some(TimeUnit::Milliseconds),
        _ => None,
    }
}

/// The unit the chain is counted in after `link`, which follows a value counted in `before`: what a
/// method that is given a unit says it gives, as the table writes it, and the unit it follows for
/// the others, where they give a moment or a length of time.
fn unit_after(link: &Call, before: Option<TimeUnit>, standing: &Standing) -> Option<TimeUnit> {
    let result = standing.verdict()?.allowance()?.result()?;
    if !matches!(
        result.class(),
        Some(Representative::Datetime | Representative::DatetimeZoned | Representative::Duration)
    ) {
        return None;
    }
    if link
        .entry()
        .parameters()
        .iter()
        .any(|parameter| parameter.name() == "time_unit")
    {
        return unit_in_name(result.name());
    }
    before
}

/// The width of the array the chain gives after a call that follows one of the width `before`: the
/// same where the call gives an array, as sorting and reversing one does, and none where it gives
/// anything else.
fn width_after(before: Option<usize>, standing: &Standing) -> Option<usize> {
    let result = standing.verdict()?.allowance()?.result()?;
    matches!(result.class(), Some(Representative::Array(_)))
        .then_some(before)
        .flatten()
}

/// The class a literal has, when its dtype is known without another operand:
/// one it was given, or text, a flag, or a null, which have one.
fn literal_class(call: &Call) -> Option<Representative> {
    if let Some(Value::Dtype(dtype)) = call.argument("dtype") {
        return Representative::of(dtype.as_polars());
    }
    match call.argument("value")? {
        Value::String(_) => Some(Representative::String),
        Value::Flag(Some(_)) => Some(Representative::Boolean),
        Value::Flag(None) => Some(Representative::Null),
        // A number typed without a dtype has the one Polars gives it by itself.
        Value::Integer(number) => Literal::Integer(*number).class(),
        Value::Float(number) => Literal::Float(*number).class(),
        _ => None,
    }
}

/// What `call` is when it is a number typed without a dtype.
fn loose_number(call: &Call) -> Option<Loose> {
    if call.entry().id().as_str() != "polars.lit" || call.argument("dtype").is_some() {
        return None;
    }
    match call.argument("value") {
        Some(Value::Integer(number)) => Loose::of(&Literal::Integer(*number)),
        Some(Value::Float(number)) => Loose::of(&Literal::Float(*number)),
        _ => None,
    }
}

/// Whether a method takes an operand besides what it follows, which a number
/// typed without a dtype adapts to or keeps its dtype beside: arithmetic,
/// comparisons, a fill value, an exponent.
fn takes_an_operand(entry: &Entry) -> bool {
    entry.applies() == Applies::Expression
        && entry.parameters().iter().any(|parameter| {
            matches!(parameter.kind(), Kind::Expression(_)) && !parameter.role().is_a_count()
        })
}

/// Whether the argument `name` of `entry` meets the value the method follows, as an operand, a value
/// that fills it or the collection it is looked up in does, and not as a key that groups or orders
/// it.
fn meets_the_receiver(entry: &Entry, name: &str) -> bool {
    entry.id().as_str() == "Expr.is_in"
        || entry
            .parameters()
            .iter()
            .find(|parameter| parameter.name() == name)
            .is_some_and(|parameter| parameter.role() != Role::Plain)
}

/// Whether the values of a condition that are arrays are not all of one width that is known: Polars
/// gives a list for them, where the tables, which write one width, give an array.
fn arrays_of_other_widths(values: &[Valued], widths: &[Option<usize>]) -> bool {
    let arrays: Vec<Option<usize>> = values
        .iter()
        .zip(widths)
        .filter(|((class, _), _)| matches!(class, Some(Representative::Array(_))))
        .map(|(_, width)| *width)
        .collect();
    arrays.len() > 1
        && !arrays
            .iter()
            .all(|width| width.is_some() && *width == arrays[0])
}

/// Whether the values of a condition that are structs are not all of one set of fields that is known.
fn structs_of_other_fields(values: &[Valued], structs: &[Option<Fields>]) -> bool {
    let held: Vec<&Option<Fields>> = values
        .iter()
        .zip(structs)
        .filter(|((class, _), _)| *class == Some(Representative::Struct))
        .map(|(_, fields)| fields)
        .collect();
    held.len() > 1
        && !held
            .iter()
            .all(|fields| fields.is_some() && **fields == *held[0])
}

/// The class every value of a condition has, when they have the same one. A
/// number typed without a dtype adapts to the values beside it that are typed,
/// as Polars gives it: a whole number to whole numbers and numbers with a
/// fraction it fits, and a number with a fraction only to numbers with a
/// fraction. Among such numbers alone it has no class. A condition of values
/// of different dtypes has the supertype, which the tables do not give.
fn unify(values: &[Valued]) -> Option<Representative> {
    let classes: Option<Vec<(Representative, Option<Loose>)>> = values
        .iter()
        .map(|(class, loose)| class.map(|class| (class, *loose)))
        .collect();
    let classes = classes?;
    let first_strict = classes
        .iter()
        .find(|(_, loose)| loose.is_none())
        .map(|(class, _)| *class);
    // Numbers typed without a dtype alone have none to give: Polars cannot resolve them where
    // it runs, in a sort key, for one.
    let first = first_strict?;
    let same = |(class, loose): &(Representative, Option<Loose>)| match loose {
        None => *class == first,
        Some(number) => number.fits(first),
    };
    classes.iter().all(same).then_some(first)
}

impl Walker<'_> {
    fn here(&self) -> Place {
        self.path
            .iter()
            .cloned()
            .fold(self.base.clone(), Place::then)
    }

    fn refuse(&mut self, refusal: &Refusal, at_end: bool) {
        self.refuse_given(refusal, at_end, false);
    }

    /// Refuses a call that was asked without the dtype of one of its arguments.
    fn refuse_blind(&mut self, refusal: &Refusal) {
        self.refuse_given(refusal, false, true);
    }

    fn refuse_given(&mut self, refusal: &Refusal, at_end: bool, argument_unknown: bool) {
        let found = WalkRefusal {
            place: self.here(),
            refusal: refusal.clone(),
            at_end,
            argument_unknown,
        };
        if self.refusal.is_none() {
            self.refusal = Some(found);
        }
    }

    fn blind(&mut self, reason: impl Into<String>) {
        if self.not_known.is_none() {
            self.not_known = Some(format!("{}: {}", self.here(), reason.into()));
        }
    }

    /// The options a step was given, asked of the option table. The options of
    /// a method are asked of [`allowed::offer`], with the call.
    fn step_options(&mut self, step: &Call) {
        let (chosen, _) = option_words(step);
        let chosen: Vec<(&str, &str)> = chosen
            .iter()
            .map(|(name, word)| (name.as_str(), word.as_str()))
            .collect();
        if let Err(refusal) = allowed::options(step.entry(), &chosen) {
            self.refuse(&refusal, false);
        }
    }

    /// Asks whether the columns that `drop_nans` is told to look at may each be tested for a NaN,
    /// which is what Polars makes of them: it refuses a column that `is_nan` refuses, such as text,
    /// when the rows run, with that method's words. The tables say which dtypes those are. The
    /// columns it looks at when it is told none are the floating-point ones, which it accepts.
    fn nan_test_columns(&mut self) {
        if self.step.entry().id().as_str() != "LazyFrame.drop_nans" {
            return;
        }
        let Some(Value::Columns(names)) = self.step.argument("subset") else {
            return;
        };
        let Some(test) = catalog::entry("Expr.is_nan") else {
            return;
        };
        for name in names {
            let Some(class) = self.source.get(name).and_then(Representative::of) else {
                continue;
            };
            let place = OfferPlace::new(
                Context::step(Outer::Select),
                After::Value {
                    dtype: class,
                    shape: Shape::Many,
                },
            );
            if let Verdict::Refused(refusal) = allowed::offer(test, &place, &Chosen::default()) {
                self.path = vec![Segment::Parameter("subset".to_owned())];
                self.refuse(&refusal, false);
                self.path.clear();
                return;
            }
        }
    }

    /// Asks whether the table is cut by numbers Polars cannot cut it by: every 0th row, and a slice
    /// that is given a length whose place and length together run past the last row number that
    /// Polars counts, which is a whole number of 63 bits. A slice given no length runs to the end.
    fn cut_by_a_number(&mut self) {
        let (parameter, said) = match self.step.entry().id().as_str() {
            "LazyFrame.gather_every" => match self.step.argument("n") {
                Some(Value::Integer(0)) => (
                    "n",
                    "gather_every needs n above nothing: Polars refuses to take every 0th row",
                ),
                _ => return,
            },
            "LazyFrame.slice" => {
                let Some(Value::Integer(offset)) = self.step.argument("offset") else {
                    return;
                };
                // A slice given no length runs to the end of the table, whatever its place is.
                let Some(Value::Integer(length)) = self.step.argument("length") else {
                    return;
                };
                if i128::from(*offset) + i128::from(*length) <= i128::from(i64::MAX) {
                    return;
                }
                (
                    "offset",
                    "a slice cannot start where it runs past the last row Polars counts, which is row 9223372036854775807",
                )
            }
            _ => return,
        };
        if let Err(refusal) = allowed::rule_forbids_a_range_that_cannot_be_made(said) {
            self.path = vec![Segment::Parameter(parameter.to_owned())];
            self.refuse(&refusal, false);
            self.path.clear();
        }
    }

    /// The output names of the keys the parameter `parameter` of the join holds, before the first
    /// whose name is not known, in order, as the walk found them: Polars expands each side's keys
    /// before it pairs them, so a key that may expand, or is not picked yet, shifts every pair after it.
    fn known_key_names(&self, parameter: &str, value: &Value, side: Outer) -> Vec<String> {
        let items = match value {
            Value::Many(items) => items.len(),
            _ => 1,
        };
        (0..items)
            .map_while(|at| {
                let mut path = vec![Segment::Parameter(parameter.to_owned())];
                if matches!(value, Value::Many(_)) {
                    path.push(Segment::Item(at));
                }
                let place = path.into_iter().fold(self.base.clone(), Place::then);
                self.walked
                    .iter()
                    .find(|walked| {
                        walked.place == place && walked.context == Some(Context::step(side))
                    })
                    .and_then(|walked| walked.output_name.clone())
            })
            .collect()
    }

    /// Asks whether a join's keys pair the same two output names twice, which
    /// Polars refuses, at the later key that makes the pair again. Only the pairs
    /// before the first key on either side whose name is not known are judged:
    /// Polars pairs the keys after expanding each side, and a key that may expand
    /// or is not picked yet shifts every pair after it.
    fn key_names(&mut self) {
        if self.step.entry().id().as_str() != "LazyFrame.join" {
            return;
        }
        let on = self.step.argument("on");
        let left = self.step.argument("left_on").or(on);
        let right = self.step.argument("right_on").or(on);
        let (Some(left), Some(right)) = (left, right) else {
            return;
        };
        let left_parameter = if self.step.argument("left_on").is_some() {
            "left_on"
        } else {
            "on"
        };
        let right_parameter = if self.step.argument("right_on").is_some() {
            "right_on"
        } else {
            "on"
        };
        let (left, right) = (
            self.known_key_names(left_parameter, left, Outer::JoinLeft),
            self.known_key_names(right_parameter, right, Outer::JoinRight),
        );
        let mut paired: Vec<(&String, &String)> = Vec::new();
        for (at, pair) in left.iter().zip(&right).enumerate() {
            if !paired.contains(&pair) {
                paired.push(pair);
                continue;
            }
            let context = Context::step(Outer::JoinLeft);
            let Err(refusal) = allowed::rule_forbids_a_repeated_pair_of_key_names(context) else {
                return;
            };
            let mut path = vec![Segment::Parameter(left_parameter.to_owned())];
            if left_is_many(self.step, left_parameter) {
                path.push(Segment::Item(at));
            }
            let place = path.into_iter().fold(self.base.clone(), Place::then);
            // A key already refused keeps its refusal: this one is the later reason.
            if let Some(walked) = self
                .walked
                .iter_mut()
                .find(|walked| walked.place == place && walked.context == Some(context))
                && !matches!(walked.ending, Some(Ending::Refused(_)))
            {
                walked.ending = Some(Ending::Refused(refusal.clone()));
            }
            if self.refusal.is_none() {
                self.refusal = Some(WalkRefusal {
                    place,
                    refusal,
                    at_end: true,
                    argument_unknown: false,
                });
            }
            return;
        }
    }

    /// The names the item `expression` of a select writes, where they are known: the name it is
    /// given by a pair, or the one its start and the fields it reads give it, or the names of the
    /// fields an unnest ends it with.
    fn names_written(
        &self,
        expression: &Expression,
        calls: &[Standing],
        name: Option<&String>,
    ) -> Option<Vec<String>> {
        let several = |link: &Call| match link.entry().id().as_str() {
            "Expr.struct.unnest" => true,
            "Expr.struct.field" => matches!(
                link.argument("more_names"),
                Some(Value::Many(more)) if !more.is_empty()
            ),
            _ => false,
        };
        if expression.links().iter().any(several) {
            // Only what ends in the several columns says their names.
            if !expression.links().last().is_some_and(several) {
                return None;
            }
            let columns = calls.last()?.verdict()?.allowance()?.columns()?;
            return Some(columns.names().map(str::to_owned).collect());
        }
        let name = self.pair_key.clone().or_else(|| name.cloned())?;
        Some(vec![name])
    }

    /// Asks whether the items of a select, or of the columns a step adds, write the columns they do
    /// under names that differ, as Polars needs: it refuses two items that write one name. Names
    /// that no field of a struct writes are not asked, which waits for the outputs of one name that
    /// every step gives ([#161](https://github.com/bj-data-eng/dageist/issues/161)).
    fn written_names_are_distinct(&mut self) {
        let written = std::mem::take(&mut self.written);
        for (at, (place, names, by_a_field)) in written.iter().enumerate() {
            let Some(names) = names else { continue };
            for (_, before, before_by_a_field) in &written[..at] {
                let Some(before) = before else { continue };
                if !(*by_a_field || *before_by_a_field) {
                    continue;
                }
                let Some(clash) = names.iter().find(|name| before.contains(name)) else {
                    continue;
                };
                if let Err(refusal) = allowed::rule_forbids_a_name_written_twice(clash) {
                    self.refusal.get_or_insert(WalkRefusal {
                        place: place.clone(),
                        refusal,
                        at_end: true,
                        argument_unknown: false,
                    });
                }
                return;
            }
        }
    }

    /// Whether `value` is, or holds, an expression that starts from a connected list.
    fn starts_from_a_list(&self, value: &Value) -> bool {
        match value {
            Value::Expression(expression) => {
                matches!(expression.start(), Start::Input(name) if self.inputs.is_list(name))
            }
            Value::Many(items) => items.iter().any(|item| self.starts_from_a_list(item)),
            _ => false,
        }
    }

    /// Whether `call` is given, as an argument that gives a count of values, a chain from a
    /// connected list.
    fn is_given_a_list(&self, call: &Call, arguments: &[(String, Operand)]) -> bool {
        arguments.iter().any(|(name, operand)| {
            operand.shape == Shape::Counted
                && call
                    .argument(name)
                    .is_some_and(|value| self.starts_from_a_list(value))
        })
    }

    /// Meets what parts give together, refusing where they are not as many as one another, and
    /// giving what they are before the first that is not. `of_a_list` says a part is a connected
    /// list's values, which the words of the refusal then say.
    fn meet_all(
        &mut self,
        parts: impl Iterator<Item = (Shape, Count)>,
        of_a_list: bool,
    ) -> (Shape, Count) {
        let mut met = (Shape::One, Count::NotKnown);
        for part in parts {
            match meet(met, part) {
                Some(next) => met = next,
                None => {
                    if let Err(refusal) = allowed::rule_forbids_values_of_other_counts() {
                        let refusal = if of_a_list {
                            allowed::refusal_of_a_connected_list(&refusal)
                        } else {
                            refusal
                        };
                        self.refuse(&refusal, false);
                    }
                    return met;
                }
            }
        }
        met
    }

    /// Asks whether the items of a select, or of the columns a step adds, are as many as one another:
    /// one value is met by each of the others, and the others are as many as one another. A step that
    /// adds columns to a table gives them as many values as the table has rows, so none is a count
    /// of values that the data or the arguments decide, as the first values of a column are.
    fn selected_items_meet(&mut self) {
        let adds = self.step.entry().id().as_str() == "LazyFrame.with_columns";
        let mut met = (Shape::One, Count::NotKnown);
        for at in 0..self.selected.len() {
            let (place, shape, count) = self.selected[at].clone();
            if shape == Shape::One {
                continue;
            }
            // The table's rows are what the columns of a step that adds them meet.
            let next = if adds {
                meet((Shape::Many, Count::NotKnown), (shape, count))
            } else {
                meet(met, (shape, count))
            };
            match next {
                Some(next) => met = next,
                None => {
                    if let Err(refusal) = allowed::rule_forbids_values_of_other_counts() {
                        // A list among the items so far, or this one, is what the others are met with.
                        let listed = self.selected[..=at]
                            .iter()
                            .any(|(held, _, _)| self.of_lists.contains(held));
                        let refusal = if listed {
                            allowed::refusal_of_a_connected_list(&refusal)
                        } else {
                            refusal
                        };
                        self.refusal.get_or_insert(WalkRefusal {
                            place,
                            refusal,
                            at_end: true,
                            argument_unknown: false,
                        });
                    }
                    return;
                }
            }
        }
    }

    /// Walks the arguments of `call`, and gives what each expression it holds
    /// has, by the parameter that holds it. `values_end` says whether the values
    /// of a condition that `call` is a link of end the predicate: the condition
    /// is all the expression is.
    fn arguments(&mut self, call: &Call, values_end: bool) -> Vec<(String, Operand)> {
        let entry = call.entry().id().as_str();
        let mut found = Vec::new();
        for given in call.arguments() {
            self.path
                .push(Segment::Parameter(given.parameter().to_owned()));
            let slot = Slot::of_parameter(entry, given.parameter());
            self.slots.extend(slot);
            // What an expression held here ends: a filter's predicate does, and
            // a value of a condition does when the condition does.
            let ends = match slot {
                Some(Slot::Outer(Outer::Filter)) => true,
                Some(Slot::Nested(Nested::ConditionValue)) => values_end,
                _ => false,
            };
            self.value(
                given.value(),
                given.parameter(),
                slot.is_some().then_some(ends),
                &mut found,
            );
            // A connected list given to `is_in` is looked up as one list of its values, which is
            // how the run builds it, and not as a collection of their dtype.
            if entry == "Expr.is_in"
                && given.parameter() == "other"
                && self.inputs.is_a_list_alone(given.value())
                && let Some((_, operand)) = found.last_mut()
            {
                *operand = imploded(*operand);
            }
            // `on` is the keys of both tables: Polars evaluates it against the other
            // one as well, so what the tables say of it there is asked too. Its
            // expressions are the left's first, at the same places.
            if slot == Some(Slot::Outer(Outer::JoinLeft)) && given.parameter() == "on" {
                self.slots.pop();
                self.slots.push(Slot::Outer(Outer::JoinRight));
                self.value(
                    given.value(),
                    given.parameter(),
                    Some(false),
                    &mut Vec::new(),
                );
            }
            if slot.is_some() {
                self.slots.pop();
            }
            self.path.pop();
        }
        found
    }

    fn value(
        &mut self,
        value: &Value,
        parameter: &str,
        held: Option<bool>,
        found: &mut Vec<(String, Operand)>,
    ) {
        match value {
            Value::Expression(expression) => {
                let operand = self.expression(expression, held);
                found.push((parameter.to_owned(), operand));
            }
            Value::Many(items) => {
                for (at, item) in items.iter().enumerate() {
                    self.path.push(Segment::Item(at));
                    self.value(item, parameter, held, found);
                    self.path.pop();
                }
            }
            Value::Pairs(pairs) => {
                for (at, (key, entry)) in pairs.iter().enumerate() {
                    self.path.push(Segment::Key(at));
                    self.value(key, parameter, held, found);
                    self.path.pop();
                    // The name the pair gives what it holds, which an item of a select is written
                    // under.
                    self.pair_key = match key {
                        Value::Columns(names) => match names.as_slice() {
                            [name] => Some(name.clone()),
                            _ => None,
                        },
                        _ => None,
                    };
                    self.path.push(Segment::Value(at));
                    self.value(entry, parameter, held, found);
                    self.path.pop();
                    self.pair_key = None;
                }
            }
            _ => {}
        }
    }

    /// Whether a `col` names more than one column, which expands it to an
    /// expression for each, whatever the first one is.
    fn names_several(call: &Call) -> bool {
        let several = |value: Option<&Value>| match value {
            Some(Value::Columns(names)) => names.len() > 1,
            _ => false,
        };
        let more = match call.argument("more_names") {
            Some(Value::Many(items)) => !items.is_empty(),
            Some(_) => true,
            None => false,
        };
        call.entry().id().as_str() == "polars.col" && (several(call.argument("name")) || more)
    }

    /// The class of the column an argument names, in the table the expression
    /// is evaluated against, where it names one.
    fn column_of(&self, call: &Call) -> Option<Representative> {
        let Some(Value::Columns(names)) = call.argument("name") else {
            return None;
        };
        if Self::names_several(call) {
            return None;
        }
        Representative::of(self.read_against()?.get(names.first()?)?)
    }

    /// The schema of the table the expression being walked reads its columns
    /// from: the other table's for a join's `right_on`, and for `on`, which is
    /// read against it as well, and the table the step starts from otherwise.
    /// `None` where it is the other table and that is not known.
    fn read_against(&self) -> Option<&Schema> {
        if self.slots.first() == Some(&Slot::Outer(Outer::JoinRight)) {
            self.other.as_deref()
        } else {
            Some(self.source)
        }
    }

    /// Says the tables cannot tell what follows a column of a join's `on` that
    /// the table it is read against does not have, which Polars refuses as not
    /// found. `on` is read against both tables, so a column in only one of them is
    /// a mistake the walk can see. The editor picks the columns of other positions
    /// from the table they are read against, which the walk does not ask.
    fn blind_to_a_missing_column(&mut self, call: &Call) {
        let in_on = matches!(
            self.path.first(),
            Some(Segment::Parameter(parameter)) if parameter == "on"
        ) && self.step.entry().id().as_str() == "LazyFrame.join";
        if !in_on || call.entry().id().as_str() != "polars.col" || Self::names_several(call) {
            return;
        }
        let Some(Value::Columns(names)) = call.argument("name") else {
            return;
        };
        let Some(name) = names.first() else {
            return;
        };
        match self.read_against() {
            Some(schema) if schema.get(name).is_some() => {}
            Some(_) => self.blind(format!("the table has no column called {name:?}")),
            None => self.blind("the other table's columns are not known"),
        }
    }

    /// The class of what a pivot's group holds, which `element` stands for: the
    /// column the pivot takes values from, where it takes them from one.
    fn group_of(&self) -> Option<Representative> {
        if self.step.entry().id().as_str() != "LazyFrame.pivot" {
            return None;
        }
        let Some(Value::Columns(names)) = self.step.argument("values") else {
            return None;
        };
        // A group of several columns has the dtype of none of them alone.
        let [name] = names.as_slice() else {
            return None;
        };
        Representative::of(self.source.get(name)?)
    }

    /// The dtype a start gives, where its arguments decide it: that of the column it reads, or of
    /// the dtype a literal is given.
    fn start_dtype(&self, start: &Call) -> Option<DataType> {
        match start.entry().id().as_str() {
            "polars.col" => {
                let Some(Value::Columns(names)) = start.argument("name") else {
                    return None;
                };
                if Self::names_several(start) {
                    return None;
                }
                self.read_against()?.get(names.first()?).cloned()
            }
            "polars.lit" => match start.argument("dtype") {
                Some(Value::Dtype(dtype)) => Some(dtype.as_polars().clone()),
                _ => None,
            },
            "polars.element" => {
                let Some(Value::Columns(names)) = self.step.argument("values") else {
                    return None;
                };
                let [name] = names.as_slice() else {
                    return None;
                };
                self.source.get(name).cloned()
            }
            _ => made_dtype(start),
        }
    }

    /// The names of the two zones of the bounds `start` and `end` of a range, where both are moments,
    /// their zones are known and they differ.
    fn bounds_clash(&self, arguments: &[(String, Operand)]) -> Option<(String, String)> {
        const NONE: &str = "no time zone";
        let zone_of = |name: &str| {
            arguments
                .iter()
                .find(|(given, _)| given == name)
                .and_then(|(_, operand)| operand.zone)
        };
        let named = |at: usize| self.zones[at].clone();
        match (zone_of("start")?, zone_of("end")?) {
            (Zone::Named(first), Zone::Named(second)) if first != second => {
                Some((named(first), named(second)))
            }
            (Zone::Named(first), Zone::Naive) => Some((named(first), NONE.to_owned())),
            (Zone::Naive, Zone::Named(second)) => Some((NONE.to_owned(), named(second))),
            _ => None,
        }
    }

    /// The zone of the moment a start gives, where its arguments decide it.
    fn start_zone(&mut self, start: &Call) -> Option<Zone> {
        match self.start_dtype(start)? {
            DataType::Datetime(_, None) => Some(Zone::Naive),
            DataType::Datetime(_, Some(zone)) => Some(self.zone_named(zone.as_str())),
            _ => None,
        }
    }

    /// The names of the two zones of a moment a method follows and an operand it is given as one the
    /// moment is compared with, subtracted by, filled with or bound by, where both are known and
    /// they differ.
    fn zone_clash(
        &self,
        entry: &Entry,
        arguments: &[(String, Operand)],
        receiver: Option<Zone>,
    ) -> Option<(String, String)> {
        let zone_of = |name: &str| {
            arguments
                .iter()
                .find(|(given, _)| given == name)
                .and_then(|(_, operand)| operand.zone)
        };
        let differ = |first: Option<Zone>, second: Option<Zone>| match (first, second) {
            (Some(Zone::Named(first)), Some(Zone::Named(second))) if first != second => {
                Some((self.zones[first].clone(), self.zones[second].clone()))
            }
            _ => None,
        };
        entry.parameters().iter().find_map(|parameter| {
            let own = zone_of(parameter.name())?;
            // An operand is met with the value it follows, as a comparison, a difference, a fill and
            // the bounds of an interval are; a parameter that shares a dtype with another is met with
            // that one, as the default of a replacement is with its new values, and the value it
            // follows is no matter.
            if matches!(parameter.role(), Role::Operand | Role::Filler)
                && let Some(clash) = differ(receiver, Some(own))
            {
                return Some(clash);
            }
            let partner = parameter.shares_a_dtype_with()?;
            differ(zone_of(partner), Some(own))
        })
    }

    /// The names of two zones of the values a condition has when this link gives it a value after
    /// the first, where both are known and they differ: Polars finds no dtype that the moments of
    /// two zones have in common, and meets the values where it works out the schema, so a `then` of
    /// a chain is refused as an `otherwise` is, and a condition that has none is refused too.
    fn condition_zone_clash(
        &self,
        entry: &Entry,
        cur: &Cur,
        arguments: &[(String, Operand)],
    ) -> Option<(String, String)> {
        if !matches!(
            entry.id().as_str(),
            "ChainedWhen.then" | "Then.otherwise" | "ChainedThen.otherwise"
        ) {
            return None;
        }
        let Cur::Condition { zones, .. } = cur else {
            return None;
        };
        let last = arguments
            .iter()
            .find(|(name, _)| name == "statement")
            .and_then(|(_, operand)| operand.zone);
        let known: Vec<usize> = zones
            .iter()
            .copied()
            .chain(std::iter::once(last))
            .filter_map(|zone| match zone {
                Some(Zone::Named(at)) => Some(at),
                _ => None,
            })
            .collect();
        let first = *known.first()?;
        let other = known.iter().find(|at| **at != first)?;
        Some((self.zones[first].clone(), self.zones[*other].clone()))
    }

    /// The fields the walk holds, which a handle points to: held once however often they are met.
    fn hold(&mut self, fields: Fields) -> FieldsAt {
        let at = self
            .fieldsets
            .iter()
            .position(|held| *held == fields)
            .unwrap_or_else(|| {
                self.fieldsets.push(fields);
                self.fieldsets.len() - 1
            });
        FieldsAt(at)
    }

    /// The fields of the struct the chain has after a call whose standing is `standing`, where the
    /// call gave a struct and the tables and the rules know its fields.
    fn fields_after(&mut self, standing: &Standing) -> Option<FieldsAt> {
        let fields = standing.verdict()?.allowance()?.fields()?.clone();
        Some(self.hold(fields))
    }

    /// What a call of `polars.struct` is made of, one for each value in order: the name it is
    /// written under, its class, and the fields it has where it is a struct. The operands are in the
    /// order the arguments were walked, which is the document's, and the values are in the order
    /// Python makes them in, the listed ones first.
    fn struct_inputs_made(&self, start: &Call, operands: &[(String, Operand)]) -> Vec<StructInput> {
        if start.entry().id().as_str() != "polars.struct" {
            return Vec::new();
        }
        let listed = operands.iter().filter(|(name, _)| name == "exprs");
        let named = operands.iter().filter(|(name, _)| name == "named_exprs");
        struct_inputs(start)
            .into_iter()
            .zip(listed.chain(named))
            .map(|((name, _), (_, operand))| StructInput {
                name: name.or_else(|| operand.name.map(|at| self.names[at.0].clone())),
                class: operand.dtype,
                fields: operand.fields.map(|at| self.fieldsets[at.0].clone()),
                loose: operand.loose.is_some(),
                width: operand.width,
            })
            .collect()
    }

    /// How many other expressions stand beside the one being walked in the list of items it is one
    /// of: the other predicates and constraints of a filter, where it is an item of one.
    fn siblings_of_the_item(&self) -> usize {
        if self.step.entry().id().as_str() != "LazyFrame.filter" {
            return 0;
        }
        if !matches!(
            self.path.first(),
            Some(Segment::Parameter(name)) if name == "predicates" || name == "constraints"
        ) {
            return 0;
        }
        let items = |name: &str| match self.step.argument(name) {
            Some(Value::Many(items)) => items.len(),
            Some(Value::Pairs(pairs)) => pairs.len(),
            _ => 0,
        };
        (items("predicates") + items("constraints")).saturating_sub(1)
    }

    /// The fields of the struct an operand is, where it is one whose fields the walk knows.
    fn fields_of(&self, operand: Option<&Operand>) -> Option<Fields> {
        operand
            .and_then(|operand| operand.fields)
            .map(|at| self.fieldsets[at.0].clone())
    }

    /// The zone the walk holds under `name`.
    fn zone_named(&mut self, name: &str) -> Zone {
        let at = self
            .zones
            .iter()
            .position(|known| known == name)
            .unwrap_or_else(|| {
                self.zones.push(name.to_owned());
                self.zones.len() - 1
            });
        Zone::Named(at)
    }

    /// The zone of the chain after `link`, which follows a moment in `before`: the zone a method that
    /// is given one gives, as the call says it, and the zone it follows for the others, where they
    /// give a moment.
    fn zone_after(
        &mut self,
        link: &Call,
        before: Option<Zone>,
        standing: &Standing,
    ) -> Option<Zone> {
        let result = standing.verdict()?.allowance()?.result()?;
        match result.class() {
            Some(Representative::Datetime) => Some(Zone::Naive),
            Some(Representative::DatetimeZoned) => {
                if link
                    .entry()
                    .parameters()
                    .iter()
                    .any(|parameter| parameter.kind() == Kind::TimeZone)
                {
                    let (_, zone) = result.name().split_once(", ")?;
                    Some(self.zone_named(zone.strip_suffix(')')?))
                } else {
                    before.filter(|zone| matches!(zone, Zone::Named(_)))
                }
            }
            _ => None,
        }
    }

    /// The unit the moment or the length of time a start gives is counted in, where its arguments
    /// decide it.
    fn start_unit(&self, start: &Call) -> Option<TimeUnit> {
        unit_of(&self.start_dtype(start)?)
    }

    /// The width of the array a start gives, where its arguments decide it.
    fn start_width(&self, start: &Call) -> Option<usize> {
        match self.start_dtype(start)? {
            DataType::Array(_, width) => Some(width),
            _ => None,
        }
    }

    /// The class a start gives, where its arguments decide it.
    fn start_class(&self, start: &Call) -> Option<Representative> {
        match start.entry().id().as_str() {
            "polars.col" => self.column_of(start),
            "polars.lit" => literal_class(start),
            "polars.element" => self.group_of(),
            // A struct is the class of a struct, whatever fields it is made of.
            "polars.struct" => Some(Representative::Struct),
            _ => Representative::of(&made_dtype(start)?),
        }
    }

    /// Walks `expression`, which a position holds when `held` says whether it ends
    /// the predicate there, and which is an argument of a call otherwise.
    fn expression(&mut self, expression: &Expression, held: Option<bool>) -> Operand {
        let none_before = self.gave_no_expression;
        let undecided_before = self.undecided_folds;
        let repeated_before = self.repeated_comparisons;
        let keyed_before = self.keyed_unnests;
        // The values of a condition end the predicate where the condition is
        // all the expression is: a method that goes on from it is what ends it.
        let values_end = held == Some(true)
            && matches!(expression.start(), Start::Call(start) if start.entry().applies() == Applies::Condition)
            && expression
                .links()
                .iter()
                .all(|link| link.entry().applies() == Applies::Condition);
        let place = self.here();
        let context = Context::from_positions(&self.slots);
        let unsaid = Operand {
            dtype: None,
            loose: None,
            shape: Shape::Many,
            count: Count::NotKnown,
            taken: Taken::default(),
            known: false,
            reliable: false,
            zone: None,
            width: None,
            fields: None,
            name: None,
        };
        // Whether the chain starts from a connected list, which the words of a refusal then say.
        let from_a_list =
            matches!(expression.start(), Start::Input(name) if self.inputs.is_list(name));
        let mut calls: Vec<Standing> = Vec::new();
        let mut last_end: Option<Ending> = None;
        let mut cur = Cur::Dead;
        let mut start_given: Option<String> = None;
        let mut start_arguments: Vec<(String, Operand)> = Vec::new();
        match expression.start() {
            Start::Call(start) => {
                self.path.push(Segment::Start);
                start_arguments = self.arguments(start, values_end);
                self.path.pop();
            }
            Start::Input(_) | Start::Hole => {}
        }
        let Some(context) = context else {
            self.blind("no named position holds the expression");
            self.walked.push(WalkedExpression {
                place,
                context: None,
                calls,
                next: None,
                ending: None,
                count: None,
                fields: Vec::new(),
                output_name: None,
            });
            return unsaid;
        };
        match expression.start() {
            Start::Call(start) => {
                self.path.push(Segment::Start);
                let takes_expressions = takes_expressions(start);
                let counted_by_arguments =
                    Counting::of(start.entry(), &[]) == Some(Counting::TheArguments);
                // The two ends of a range of moments are in one time zone, as the two ends of a
                // linear space are: Polars finds no dtype that two zones have in common.
                let clash = if matches!(
                    start.entry().id().as_str(),
                    "polars.datetime_range" | "polars.linear_space"
                ) {
                    self.bounds_clash(&start_arguments)
                } else {
                    None
                };
                // What a function that takes expressions gives is said only where each argument's
                // dtype is known, as for a method: the tables are not asked of one that is not, and
                // Polars may refuse it for its dtype.
                let class = self
                    .start_class(start)
                    .or_else(|| range_class(start, &start_arguments))
                    .filter(|_| {
                        !takes_expressions
                            || counted_by_arguments
                            || start_arguments
                                .iter()
                                .all(|(_, operand)| operand.dtype.is_some())
                    });
                self.blind_to_a_missing_column(start);
                // A function that takes expressions is judged for what it is given, and gives as
                // many values as its widest argument.
                let given: Vec<(&str, Representative)> = start_arguments
                    .iter()
                    .filter_map(|(name, operand)| operand.dtype.map(|dtype| (name.as_str(), dtype)))
                    .collect();
                let unclassed: Vec<&str> = start_arguments
                    .iter()
                    .filter(|(_, operand)| operand.dtype.is_none())
                    .map(|(name, _)| name.as_str())
                    .collect();
                let (words, _) = option_words(start);
                let words: Vec<(&str, &str)> = words
                    .iter()
                    .map(|(name, word)| (name.as_str(), word.as_str()))
                    .collect();
                let time_zones = start.time_zones();
                // The fields of a column that is a struct, which its schema gives, and what a function
                // that makes a struct is made of.
                let start_fields = self
                    .start_dtype(start)
                    .and_then(|dtype| Fields::of_dtype(&dtype));
                let inputs_made: Vec<StructInput> =
                    self.struct_inputs_made(start, &start_arguments);
                let input_names: Vec<Input<'_>> = inputs_made
                    .iter()
                    .map(|input| {
                        Input::new(input.name.as_deref(), input.class, input.fields.as_ref())
                            .loose_if(input.loose)
                            .with_width(input.width)
                    })
                    .collect();
                start_given = inputs_made.first().and_then(|input| input.name.clone());
                let names = start.names();
                let numbers = start.numbers();
                let chosen = Chosen::default()
                    .following_fields(start_fields.as_ref())
                    .with_names(&names)
                    .with_numbers(&numbers)
                    .with_inputs(&input_names);
                let chosen = match class {
                    Some(dtype) => chosen.giving(dtype),
                    None => chosen,
                };
                let chosen = if takes_expressions {
                    let chosen = chosen
                        .with_arguments(&given)
                        .with_unclassed(&unclassed)
                        .with_options(&words)
                        .with_time_zones(&time_zones)
                        .with_zone_clash(
                            clash
                                .as_ref()
                                .map(|(first, second)| (first.as_str(), second.as_str())),
                        )
                        .following(self.start_unit(start))
                        .following_width(self.start_width(start));
                    if counted_by_arguments {
                        // A range or a repeat gives as many values as its arguments fix, which is
                        // neither one value nor as many as the table has rows.
                        chosen.giving_shape(Shape::Counted)
                    } else if start_arguments
                        .iter()
                        .all(|(_, operand)| operand.shape == Shape::One)
                    {
                        chosen.giving_one()
                    } else {
                        chosen.giving_many()
                    }
                } else {
                    chosen
                };
                let mut verdict =
                    match allowed::rule_forbids_a_folded_function_in_a_join_key(context) {
                        Err(refusal) if is_folded_into_a_literal(start) => {
                            Verdict::Refused(refusal)
                        }
                        _ => allowed::offer(
                            start.entry(),
                            &OfferPlace::new(context, After::Start),
                            &chosen,
                        ),
                    };
                // A range or a repeat reads each argument it takes as one value, and refuses what
                // no arguments typed in make a range of.
                let made = counted_by_arguments.then(|| made_by(start)).flatten();
                if verdict.is_allowed() && counted_by_arguments {
                    let refusal = given_other_than_one_value(start.entry(), &start_arguments)
                        .and_then(|(parameter, given)| {
                            allowed::rule_forbids_a_parameter_given_more_than_one_value(
                                parameter, given,
                            )
                            .err()
                        })
                        .or_else(|| {
                            let said = match &made {
                                Some(Made::Refused(said)) => Some(said.clone()),
                                _ => incompatible_bounds(start, &start_arguments),
                            }?;
                            allowed::rule_forbids_a_range_that_cannot_be_made(&said).err()
                        });
                    if let Some(refusal) = refusal {
                        verdict = Verdict::Refused(refusal);
                    }
                }
                match &verdict {
                    Verdict::Refused(refusal) => self.refuse(refusal, false),
                    Verdict::Allowed(allowance) => {
                        last_end = Some(allowance.can_end().clone());
                        // What the tables are not told is not guessed: an argument whose dtype is
                        // not known leaves whether it may end, and fits, not known.
                        if takes_expressions && !unclassed.is_empty() {
                            self.blind(format!(
                                "the dtype of an argument of {} is not known",
                                start.entry().id().as_str()
                            ));
                            last_end = None;
                        }
                        // An expansion has a dtype for each column, so whether it
                        // may end is not known, whatever the table measured.
                        if Self::names_several(start) && last_end == Some(Ending::Allowed) {
                            last_end = Some(Ending::NotKnown(Unknown::TheStartsDtype));
                        }
                        cur = if start.entry().id().as_str() == "polars.when" {
                            Cur::Condition {
                                state: ConditionState::When,
                                values: Vec::new(),
                                meeting: self.meet_all(
                                    start_arguments
                                        .iter()
                                        .filter(|(_, operand)| operand.known && operand.reliable)
                                        .map(|(_, operand)| (operand.shape, operand.count)),
                                    self.is_given_a_list(start, &start_arguments),
                                ),
                                taken: start_arguments
                                    .iter()
                                    .fold(Taken::default(), |all, (_, operand)| {
                                        all.or(operand.taken)
                                    }),
                                zones: Vec::new(),
                                widths: Vec::new(),
                                structs: Vec::new(),
                                single: start_arguments
                                    .iter()
                                    .all(|(_, operand)| operand.shape == Shape::One),
                                values_single: true,
                                known: start_arguments.iter().all(|(_, operand)| operand.known),
                            }
                        } else {
                            Cur::Value {
                                dtype: class,
                                shape: allowance.shape(),
                                count: match made {
                                    Some(Made::Counts(count)) => count,
                                    _ => Count::NotKnown,
                                },
                                // A function that makes a value of what it is given gives one that
                                // was taken where one of them is.
                                taken: start_arguments
                                    .iter()
                                    .fold(Taken::default(), |all, (_, operand)| {
                                        all.or(operand.taken)
                                    })
                                    .if_one(allowance.shape()),
                            }
                        };
                    }
                }
                calls.push(Standing {
                    follows: Some(After::Start),
                    verdict: Some(verdict),
                });
                self.path.pop();
            }
            Start::Hole => {
                // Nothing is picked, so nothing is asked of what follows.
                calls.push(Standing::not_asked());
                cur = Cur::Value {
                    dtype: None,
                    shape: Shape::Many,
                    count: Count::NotKnown,
                    taken: Taken::default(),
                };
            }
            Start::Input(name) => {
                cur = Cur::Value {
                    dtype: None,
                    shape: Shape::Many,
                    count: Count::NotKnown,
                    taken: Taken::default(),
                };
                let standing = self.input_start(name, context, &mut cur, &mut last_end);
                calls.push(standing);
            }
        }
        // Values that are one expression make the condition that expression, and
        // its predicates are dropped.
        let statements: Vec<&Expression> = expression
            .links()
            .iter()
            .filter_map(|link| match link.argument("statement") {
                Some(Value::Expression(statement)) => Some(statement),
                _ => None,
            })
            .collect();
        // Where both values are the same one value Polars may drop the predicate, which is then
        // met by it. Where they are the same rows or the same count it does not, and a predicate
        // that is not as many refuses the condition as it would two values that differ.
        let mut branches_are_one = statements.len() > 1
            && statements.windows(2).all(|pair| pair[0] == pair[1])
            && statements
                .iter()
                .all(|statement| wholly_single(statement, statement.links().len(), true));
        let mut widened = false;
        let mut unit = match expression.start() {
            Start::Call(start) => self.start_unit(start),
            _ => None,
        };
        let mut width = match expression.start() {
            Start::Call(start) => self.start_width(start),
            _ => None,
        };
        let mut zone = match expression.start() {
            Start::Call(start) => self.start_zone(start),
            _ => None,
        };
        let mut held_fields: Vec<Option<FieldsAt>> = Vec::new();
        let mut fields: Option<FieldsAt> = match expression.start() {
            Start::Call(start) => {
                let held = self
                    .start_dtype(start)
                    .and_then(|dtype| Fields::of_dtype(&dtype))
                    .or_else(|| {
                        calls
                            .first()
                            .and_then(|standing| standing.verdict()?.allowance()?.fields().cloned())
                    });
                held.map(|found| self.hold(found))
            }
            _ => None,
        };
        held_fields.push(fields);
        // The name the chain is written under: its start's, then what a field read or an unnest
        // says. A struct is named as the first value it is made of, as the walk found it.
        let mut name: Option<String> = match expression.start() {
            Start::Call(start) if start.entry().id().as_str() == "polars.struct" => start_given,
            start => start_name(start, expression.links()),
        };
        let mut folds_to_a_repeat = false;
        // Whether a link unnests a struct that has no field: it gives no expression, and what
        // follows it is none either.
        let mut gives_no_expression = false;
        for (at, link) in expression.links().iter().enumerate() {
            self.path.push(Segment::Link(at));
            let none_before_the_arguments = self.gave_no_expression;
            let arguments = self.arguments(link, values_end);
            // A call given what expands to no expression is made as if it were not there, so what
            // it gives is not told, nor is what follows it.
            if self.gave_no_expression > none_before_the_arguments {
                gives_no_expression = true;
                calls.push(Standing::not_asked());
                held_fields.push(None);
                cur = Cur::Dead;
                self.path.pop();
                continue;
            }
            // The first link follows the start, which is a number typed without a dtype when
            // it is one.
            let receiver_loose = match expression.start() {
                Start::Call(start) if at == 0 => loose_number(start),
                _ => None,
            };
            // A condition whose predicates and values are all one value gives one value whether
            // or not Polars drops its predicates, so what follows it need not wait on that.
            // Values that are many are many whether or not Polars drops the predicates.
            let all_single = matches!(
                &cur,
                Cur::Condition { single: true, .. }
                    | Cur::Condition {
                        values_single: false,
                        ..
                    }
            );
            let receiver_shape = match &cur {
                Cur::Value { shape, .. } => Some(*shape),
                _ => None,
            };
            // Polars decides some comparisons of missing values of a column of whole numbers while it
            // coerces the types, for no value of the column is the number: one with a number that has a
            // fraction, or with a whole number that the dtype of the column cannot hold. What it
            // leaves is a constant repeated to the length of the column, which a join refuses as not
            // elementwise, for `repeat` is no elementwise call and the length is an aggregate. Whether
            // the key is one is for what ends the chain to say: a key that is one value, as the count
            // or the sum of the comparison is, is taken whole. Where the dtype of the column is not
            // known the chain says so, and the walk does not judge the key.
            match fold_of_a_missing_comparison(expression, at, &cur, &arguments) {
                Fold::Repeats => folds_to_a_repeat = true,
                Fold::Undecided => self.undecided_folds += 1,
                Fold::Kept => {}
            }
            let fields_followed: Option<Fields> = fields.map(|at| self.fieldsets[at.0].clone());
            gives_no_expression |= link.entry().id().as_str() == "Expr.struct.unnest"
                && fields_followed.as_ref().is_some_and(Fields::is_empty);
            let name_before = name.clone();
            let (next, standing) = self.link(
                link,
                &arguments,
                context,
                cur,
                &mut last_end,
                Around {
                    branches_are_one,
                    receiver_loose,
                    unit,
                    width,
                    widened,
                    zone,
                    fields,
                    from_a_list,
                    output_name: name_before.as_deref(),
                },
            );
            unit = unit_after(link, unit, &standing);
            // A field read of an array has the width the field holds.
            width = standing
                .verdict()
                .and_then(Verdict::allowance)
                .and_then(allowed::Allowance::width)
                .or_else(|| width_after(width, &standing));
            zone = self.zone_after(link, zone, &standing);
            name = name_after(link, name, fields_followed.as_ref());
            fields = self.fields_after(&standing);
            held_fields.push(fields);
            widened = widens(link, &arguments, receiver_shape, &next, widened);
            if branches_are_one
                && all_single
                && matches!(
                    link.entry().id().as_str(),
                    "Then.otherwise" | "ChainedThen.otherwise"
                )
            {
                branches_are_one = false;
            }
            // A method that gives one value of the many that a condition of identical values may
            // give is one value however the optimizer treats the condition. It is one by its own
            // nature, as an aggregate is, where the one value after a method that follows the
            // condition and gives a value for each of what it follows is only the one that the
            // condition may be, which is not counted.
            if branches_are_one && is_one_value(link) {
                branches_are_one = false;
            }
            cur = next;
            calls.push(standing);
            self.path.pop();
        }
        // A condition that has no `otherwise` and holds a number typed without a dtype has no
        // dtype to give it: Polars cannot resolve the number, and refuses where it runs.
        if let Cur::Condition { values, .. } = &cur
            && values.iter().any(|(_, loose)| loose.is_some())
        {
            last_end = Some(Ending::NotKnown(Unknown::TheResultsDtype));
        }
        // A chain that goes on from a connected list and still gives a count of its values is the
        // values of another table, which the words of a refusal of it say.
        let of_a_list = from_a_list
            && matches!(
                &cur,
                Cur::Value {
                    shape: Shape::Counted,
                    ..
                }
            );
        if of_a_list {
            self.of_lists.push(place.clone());
        }
        // An unnest anywhere in a join's key, inside a condition too, leaves the key not told.
        if self.keyed_unnests > keyed_before && !matches!(last_end, Some(Ending::Refused(_))) {
            last_end = Some(Ending::NotKnown(Unknown::AnUnnestAsAKey));
        }
        // What a comparison that Polars decides leaves, where the chain goes on to end with a value
        // for each row, is as many constants as the column has values, which a key that holds one,
        // at any depth, is not elementwise for.
        if folds_to_a_repeat
            && matches!(
                &cur,
                Cur::Value {
                    shape: Shape::Many,
                    ..
                }
            )
        {
            self.repeated_comparisons += 1;
        }
        let mut ending = None;
        if held.is_some() && !matches!(cur, Cur::Dead) {
            // A value of a condition in a filter need be a Boolean only where
            // the condition ends the predicate.
            let asked = held == Some(true)
                || context != Context::inside(Outer::Filter, Nested::ConditionValue);
            if asked {
                ending = last_end;
            }
            if let Some(Ending::Refused(refusal)) = &ending {
                let refusal = if of_a_list {
                    allowed::refusal_of_a_connected_list(refusal)
                } else {
                    refusal.clone()
                };
                self.refuse(&refusal, true);
                ending = Some(Ending::Refused(refusal));
            } else if let Some(Ending::NotKnown(unknown)) = &ending {
                self.blind(format!("whether it may end is not known: {unknown}"));
            }
            // A join's key that holds an aggregate beside a column, or a column
            // compared with a null, is not elementwise, which only the whole key
            // shows.
            // A column of the dtype Null holds a null in every row, so what compares it with a
            // null or a column leaves no value to replace, though it is a column to an aggregate
            // beside it.
            let as_read = self
                .read_against()
                .and_then(|schema| with_null_columns_as_literals(expression, schema));
            // A key that is wholly one value is taken whole, whatever the aggregate that makes it is
            // given, as it is for the aggregate beside a column.
            let repeats = self.repeated_comparisons > repeated_before
                && !wholly_single(expression, expression.links().len(), true);
            let comparisons = if self.undecided_folds > undecided_before {
                Comparisons::SomeUndecided
            } else {
                Comparisons::AllJudged
            };
            // An expression that holds what gives no expression is judged without it, which the
            // syntax of the whole does not show.
            if self.gave_no_expression == none_before
                && (is_not_elementwise(
                    expression,
                    as_read.as_ref().unwrap_or(expression),
                    comparisons,
                ) || repeats)
                && let Err(refusal) = allowed::rule_forbids_a_key_known_not_elementwise(context)
            {
                self.refuse(&refusal, true);
                ending = Some(Ending::Refused(refusal));
            }
        }
        let next = match &cur {
            Cur::Value {
                dtype: Some(dtype),
                shape,
                ..
            } => Some(After::Value {
                dtype: *dtype,
                shape: *shape,
            }),
            Cur::Condition { state, .. } => Some(After::Condition(*state)),
            _ => None,
        };
        let count = match &cur {
            Cur::Value {
                dtype: Some(_),
                shape: Shape::Counted,
                count,
                ..
            } => Some(*count),
            _ => None,
        };
        let held_here = held.is_some() && context == Context::step(Outer::Select);
        let held_place = held_here.then(|| place.clone());
        if held_here {
            let names = self.names_written(expression, &calls, name.as_ref());
            let by_a_field = expression.links().iter().any(|link| {
                matches!(
                    link.entry().id().as_str(),
                    "Expr.struct.field" | "Expr.struct.unnest"
                )
            });
            self.written.push((place.clone(), names, by_a_field));
        }
        self.walked.push(WalkedExpression {
            place,
            context: Some(context),
            calls,
            next,
            ending,
            count,
            fields: held_fields
                .iter()
                .map(|at| at.map(|at| self.fieldsets[at.0].clone()))
                .collect(),
            output_name: name.clone(),
        });
        let name_at = name.clone().map(|held| {
            self.names.push(held);
            NameAt(self.names.len() - 1)
        });
        // What gives no expression is left out by what holds it, as Polars does: it is no value to
        // judge, and what holds it cannot be told without it.
        if gives_no_expression {
            self.gave_no_expression += 1;
            self.blind("an unnest of a struct that has no field gives no expression");
            return unsaid;
        }
        let operand = match cur {
            Cur::Value {
                dtype,
                shape,
                count,
                taken,
            } => Operand {
                dtype,
                loose: match expression.start() {
                    Start::Call(start) if expression.links().is_empty() => loose_number(start),
                    _ => None,
                },
                shape,
                count,
                taken,
                // What was not asked has no shape that can be counted on.
                known: dtype.is_some(),
                reliable: dtype.is_some() && !branches_are_one,
                zone,
                width,
                fields,
                name: name_at,
            },
            Cur::Condition {
                values,
                meeting,
                taken,
                widths,
                ..
            } => Operand {
                dtype: unify(&values),
                loose: None,
                shape: if meeting.0 == Shape::Counted {
                    Shape::Counted
                } else {
                    Shape::Many
                },
                count: meeting.1,
                taken,
                known: true,
                reliable: !branches_are_one,
                zone: None,
                fields: None,
                name: None,
                // The width the arrays among the values share, where they are of one.
                width: widths
                    .iter()
                    .zip(&values)
                    .filter(|(_, (class, _))| matches!(class, Some(Representative::Array(_))))
                    .map(|(width, _)| *width)
                    .reduce(|first, other| if first == other { first } else { None })
                    .flatten(),
            },
            Cur::Dead => unsaid,
        };
        // What an item of a select gives is met by what the others give.
        if let Some(place) = held_place
            && operand.known
            && operand.reliable
        {
            self.selected.push((place, operand.shape, operand.count));
        }
        operand
    }

    /// Asks the tables about a chain that starts from the connected input
    /// `name`, which the run makes a literal of its dtype. `cur` and `last_end`
    /// are what the chain has afterwards, as for any other start.
    fn input_start(
        &mut self,
        name: &str,
        context: Context,
        cur: &mut Cur,
        last_end: &mut Option<Ending>,
    ) -> Standing {
        let Some(literal) = self.inputs.start_literal(name) else {
            self.blind(format!(
                "there is no connected value or list called {name:?}"
            ));
            return Standing::not_asked();
        };
        let Some(class) = literal.class else {
            self.blind(format!(
                "the dtype of the connected input {name:?} is not one the tables cover"
            ));
            return Standing::not_asked();
        };
        let verdict = allowed::offer_input(
            &OfferPlace::new(context, After::Start),
            class,
            literal.shape,
        );
        match &verdict {
            Verdict::Refused(refusal) => {
                self.refuse(refusal, false);
                *cur = Cur::Dead;
            }
            Verdict::Allowed(allowance) => {
                *last_end = Some(allowance.can_end().clone());
                *cur = Cur::Value {
                    dtype: Some(class),
                    shape: allowance.shape(),
                    count: Count::NotKnown,
                    taken: Taken::default(),
                };
            }
        }
        Standing {
            follows: Some(After::Start),
            verdict: Some(verdict),
        }
    }

    /// Asks whether `link` may stand after what `cur` has, and gives what the
    /// chain has after it.
    fn link(
        &mut self,
        link: &Call,
        arguments: &[(String, Operand)],
        context: Context,
        cur: Cur,
        last_end: &mut Option<Ending>,
        around: Around<'_>,
    ) -> (Cur, Standing) {
        let Around {
            branches_are_one,
            receiver_loose,
            unit,
            width,
            widened,
            zone,
            fields,
            from_a_list,
            output_name,
        } = around;
        // What the call says of a struct: the fields it follows, the names and numbers it is given.
        let fields_followed: Option<Fields> = fields.map(|at| self.fieldsets[at.0].clone());
        let names = link.names();
        let numbers = link.numbers();
        let entry = link.entry();
        // What the link follows is the values of another table where the chain started from a
        // connected list and still gives a count of its values.
        let receiver_listed = from_a_list
            && matches!(
                &cur,
                Cur::Value {
                    shape: Shape::Counted,
                    ..
                }
            );
        let involves_a_list = receiver_listed || self.is_given_a_list(link, arguments);
        // A window is refused a value of one row that a part widened, whose output is longer than
        // its input.
        if widened
            && entry.id().as_str() == "Expr.over"
            && let Err(refusal) = allowed::rule_forbids_a_window_over_a_widened_value()
        {
            self.refuse(&refusal, false);
            return (
                Cur::Dead,
                Standing {
                    follows: None,
                    verdict: Some(Verdict::Refused(refusal)),
                },
            );
        }
        // Polars cannot extract the length that cuts a value from a number below nothing, whatever
        // the value and its dtype are, so it is said first.
        if let Some(said) = cut_below_nothing(link)
            && let Err(refusal) = allowed::rule_forbids_a_range_that_cannot_be_made(&said)
        {
            self.refuse(&refusal, false);
            return (
                Cur::Dead,
                Standing {
                    follows: None,
                    verdict: Some(Verdict::Refused(refusal)),
                },
            );
        }
        // A window takes a value as long as its group. One value that a method took of one value is
        // that of no row of it, where the aggregate of one value is the value of the group, unless
        // the window joins the values of a group or explodes them.
        if entry.id().as_str() == "Expr.over"
            && matches!(
                &cur,
                Cur::Value {
                    shape: Shape::One,
                    taken: Taken { window: true, .. },
                    ..
                }
            )
            && !matches!(
                link.argument("mapping_strategy"),
                Some(Value::Choice(word)) if word == "join" || word == "explode"
            )
            && let Err(refusal) = allowed::rule_forbids_a_window_over_a_count()
        {
            self.refuse(&refusal, false);
            return (
                Cur::Dead,
                Standing {
                    follows: None,
                    verdict: Some(Verdict::Refused(refusal)),
                },
            );
        }
        // Moments in two zones that a method is given to compare, subtract, fill or bind by one
        // another have no dtype in common.
        let clash = self
            .zone_clash(entry, arguments, zone)
            .or_else(|| self.condition_zone_clash(entry, &cur, arguments));
        let clash_of_zones = clash
            .as_ref()
            .map(|(first, second)| (first.as_str(), second.as_str()));
        // The zone of the moment the call follows, which a length of time given to it is read in.
        let zone_name = match zone {
            Some(Zone::Named(at)) => Some(self.zones[at].clone()),
            _ => None,
        };
        // A number typed without a dtype that a method takes another operand beside adapts to
        // it, as the dtype of that operand where it fits it, or keeps the dtype it has alone, as
        // the base of a power does. Where there is no such operand the tables, measured with typed
        // columns, cannot say what it follows.
        let cur = match (cur, receiver_loose) {
            (
                Cur::Value {
                    shape,
                    count,
                    taken,
                    dtype: alone,
                },
                Some(number),
            ) if takes_an_operand(entry) => {
                let meetings: Vec<(Receiver, &Operand)> = arguments
                    .iter()
                    .filter_map(|(name, operand)| {
                        receiver_beside(entry, name, number).map(|meeting| (meeting, operand))
                    })
                    .collect();
                let keeps = !meetings.is_empty()
                    && meetings
                        .iter()
                        .all(|(meeting, _)| *meeting == Receiver::Keeps);
                let dtype = if keeps {
                    alone
                } else {
                    meetings
                        .iter()
                        .filter(|(meeting, operand)| {
                            *meeting == Receiver::Adapts && operand.loose.is_none()
                        })
                        .map(|(_, operand)| operand.dtype)
                        .collect::<Option<Vec<_>>>()
                        .and_then(|dtypes| match dtypes.as_slice() {
                            [dtype] => number.beside(*dtype),
                            _ => None,
                        })
                };
                // A number that stays no dtype beside values for each row is a value for each row
                // with them, as it is where its dtype is known.
                let shape = if dtype.is_none()
                    && shape == Shape::One
                    && meetings
                        .iter()
                        .any(|(_, operand)| operand.known && operand.shape == Shape::Many)
                {
                    Shape::Many
                } else {
                    shape
                };
                Cur::Value {
                    dtype,
                    shape,
                    count,
                    taken,
                }
            }
            (other, _) => other,
        };
        let after = match &cur {
            Cur::Dead => return (Cur::Dead, Standing::not_asked()),
            Cur::Value {
                dtype: None,
                shape,
                count,
                taken,
            } => {
                // A number typed without a dtype that the call follows has none beside a date, a
                // moment, a length of time or a time of day it is given, where no dtype it could
                // take lets the call stand: Polars refuses it as it refuses it given to them.
                let refused = receiver_loose.and_then(|number| {
                    arguments
                        .iter()
                        .filter(|(name, operand)| {
                            operand.loose.is_none()
                                && receiver_beside(entry, name, number) == Some(Receiver::Adapts)
                        })
                        .find_map(|(name, operand)| {
                            allowed::refused_for_every_receiver_number(
                                entry,
                                context,
                                *shape,
                                (name.as_str(), operand.dtype?),
                                number,
                            )
                        })
                });
                if let Some(refusal) = refused {
                    self.refuse(&refusal, false);
                    return (
                        Cur::Dead,
                        Standing {
                            follows: None,
                            verdict: Some(Verdict::Refused(refusal)),
                        },
                    );
                }
                self.blind(format!(
                    "the dtype before {} is not known",
                    entry.id().as_str()
                ));
                *last_end = None;
                return (
                    Cur::Value {
                        dtype: None,
                        shape: *shape,
                        count: *count,
                        taken: *taken,
                    },
                    Standing::not_asked(),
                );
            }
            Cur::Value {
                dtype: Some(dtype),
                shape,
                ..
            } => After::Value {
                dtype: *dtype,
                shape: *shape,
            },
            Cur::Condition { state, .. } => After::Condition(*state),
        };
        // A number typed without a dtype that a method takes as an operand adapts to what it
        // follows. Its dtype is that of what the method follows where it fits it, and otherwise
        // not known to the tables, unless it counts rows.
        let receiver = match &cur {
            Cur::Value { dtype, .. } if receiver_loose.is_none() => *dtype,
            _ => None,
        };
        let resolved: Vec<(&str, Option<Representative>, Option<Loose>)> = arguments
            .iter()
            .map(|(name, operand)| {
                let counts = operand.loose.is_some_and(|number| {
                    number.is_whole() && !is_refused_as_a_count(entry, name, number)
                });
                let hidden = entry.applies() == Applies::Expression
                    && operand.loose.is_some()
                    && !(counts && is_count(entry, name))
                    && !keeps_its_dtype(entry, name);
                let dtype = if hidden {
                    operand
                        .loose
                        .zip(receiver)
                        .filter(|_| operand_adapts(entry, name))
                        .filter(|(number, receiver)| {
                            !is_left_unresolved(entry, name, *number, *receiver)
                        })
                        .and_then(|(number, receiver)| adapted_dtype(entry, name, number, receiver))
                } else {
                    operand.dtype
                };
                (name.as_str(), dtype, operand.loose.filter(|_| hidden))
            })
            .collect();
        let given: Vec<(&str, Representative)> = resolved
            .iter()
            .filter_map(|(name, dtype, _)| dtype.map(|dtype| (*name, dtype)))
            .collect();
        // Where every argument the tables are not told is a number given to a parameter that must
        // be given, what the call gives may be the same whatever dtype the number takes.
        let missing: Vec<&str> = resolved
            .iter()
            .filter(|(_, dtype, _)| dtype.is_none())
            .map(|(name, _, _)| *name)
            .collect();
        let only_a_number = match missing.as_slice() {
            [name] => {
                resolved
                    .iter()
                    .any(|(given, _, number)| given == name && number.is_some())
                    && operand_adapts(entry, name)
                    && is_required(entry, name)
                    // A number that adapts to numbers only is no number of any dtype beside
                    // another, which Polars refuses for some methods.
                    && (!adapts_numbers_only(entry, name)
                        || receiver.is_some_and(could_be_taken_by_a_number))
            }
            _ => false,
        };
        let (words, unrepresented) = option_words(link);
        let words: Vec<(&str, &str)> = words
            .iter()
            .map(|(name, word)| (name.as_str(), word.as_str()))
            .collect();
        let patterns = link.patterns();
        let durations = link.durations();
        let time_zones = link.time_zones();
        let place = OfferPlace::new(context, after);
        // Two arguments that meet in the supertype of their dtypes are judged each apart, so
        // where their dtypes differ, whether Polars finds a supertype is not known, and what
        // the tables say of either beside a companion of the dtype of the other is not
        // what Polars says of the two.
        let class_of_given = |name: &str| {
            given
                .iter()
                .find(|(given, _)| *given == name)
                .map(|(_, class)| *class)
        };
        let meeting: Vec<&str> = entry
            .parameters()
            .iter()
            .filter_map(|parameter| {
                let other = parameter.shares_a_dtype_with()?;
                match (class_of_given(parameter.name()), class_of_given(other)) {
                    (Some(first), Some(second)) if first != second => {
                        Some([parameter.name(), other])
                    }
                    _ => None,
                }
            })
            .flatten()
            .collect();
        // A null literal of another dtype than what it fills is folded away by Polars where it
        // runs the rows, which then give the dtype of what it fills, where its plan gives the
        // supertype of the two, so what the call gives is not known.
        let fills_with_a_null_of_another_dtype = entry.id().as_str() == "Expr.fill_null"
            && matches!(link.argument("value"), Some(Value::Expression(filler)) if is_a_null(filler))
            && matches!(
                (class_of_given("value"), receiver),
                (Some(filler), Some(follows)) if filler != follows
            );
        let mut meeting = meeting;
        if fills_with_a_null_of_another_dtype {
            meeting.push("value");
        }
        let supertyped = !meeting.is_empty();
        // What the tables are not told is not guessed: an argument whose dtype
        // is not known leaves the call to be offered for any dtype that fits,
        // which says it stands, not that this argument fits. The dtype a call is given is
        // an argument too, which a cast to a dtype the tables do not cover leaves unknown.
        if given.len() < arguments.len() || unrepresented || supertyped {
            self.blind(format!(
                "the dtype of an argument of {} is not known",
                entry.id().as_str()
            ));
            *last_end = None;
            // The call is still offered, for a refusal is a refusal whatever
            // the argument, but nothing is said of what it gives.
            let judged: Vec<(&str, Representative)> = given
                .iter()
                .filter(|(name, _)| !meeting.contains(name))
                .copied()
                .collect();
            let unjudged: Vec<&str> = arguments
                .iter()
                .map(|(name, _)| name.as_str())
                .filter(|name| !judged.iter().any(|(judged, _)| judged == name))
                .collect();
            let held_by_arguments: Vec<(&str, Fields)> = arguments
                .iter()
                .filter_map(|(name, operand)| {
                    Some((name.as_str(), self.fieldsets[operand.fields?.0].clone()))
                })
                .collect();
            let argument_fields: Vec<(&str, &Fields)> = held_by_arguments
                .iter()
                .map(|(name, fields)| (*name, fields))
                .collect();
            let open_numbers: Vec<&str> = resolved
                .iter()
                .filter(|(_, _, number)| number.is_some())
                .map(|(name, _, _)| *name)
                .collect();
            let verdict = allowed::offer(
                entry,
                &place,
                &Chosen::default()
                    .with_open_numbers(&open_numbers)
                    .following_an_open_number(receiver_loose.is_some())
                    .among_others(self.siblings_of_the_item())
                    .with_argument_fields(&argument_fields)
                    .with_arguments(&judged)
                    .with_unclassed(&unjudged)
                    .with_options(&words)
                    .with_patterns(&patterns)
                    .with_durations(&durations)
                    .with_time_zones(&time_zones)
                    .following(unit)
                    .following_width(width)
                    .following_fields(fields_followed.as_ref())
                    .with_names(&names)
                    .with_numbers(&numbers)
                    .named(output_name)
                    .in_zone(zone_name.as_deref())
                    .with_zone_clash(clash_of_zones),
            );
            // A number typed without a dtype that adapts to what the call follows has none beside
            // what is no number, so where no dtype it could take lets the call stand, Polars
            // refuses it whatever the number is.
            let refused_for_a_number = match (&verdict, missing.as_slice()) {
                (Verdict::Allowed(_), [name]) if operand_adapts(entry, name) => resolved
                    .iter()
                    .find(|(given, _, _)| given == name)
                    .and_then(|(_, _, number)| *number)
                    .and_then(|number| {
                        allowed::refused_for_every_number(
                            entry,
                            &place,
                            &Chosen::default()
                                .with_arguments(&given)
                                .with_options(&words),
                            name,
                            number,
                        )
                        .or_else(|| allowed::refused_as_ambiguous(entry, &place, name))
                    }),
                _ => None,
            };
            let verdict = refused_for_a_number.map_or(verdict, Verdict::Refused);
            let standing = Standing {
                follows: Some(after),
                verdict: Some(verdict.clone()),
            };
            if let Verdict::Refused(refusal) = &verdict {
                self.refuse_blind(refusal);
                return (Cur::Dead, standing);
            }
            let same = match (&verdict, missing.as_slice()) {
                (Verdict::Allowed(_), [name]) if only_a_number => allowed::same_for_any_number(
                    entry,
                    &place,
                    &Chosen::default()
                        .with_arguments(&given)
                        .with_options(&words),
                    name,
                ),
                _ => None,
            };
            let (dtype, shape) = match same {
                Some((result, shape)) => (result.class(), shape),
                None => (None, Shape::Many),
            };
            // What a method that takes some of the values gives of how many is the arguments' to
            // say, whether or not the dtype of one of them is known.
            let (option_words, _) = option_words(link);
            let option_words: Vec<(&str, &str)> = option_words
                .iter()
                .map(|(name, word)| (name.as_str(), word.as_str()))
                .collect();
            let (shape, count) = match &verdict {
                Verdict::Allowed(allowance) if Counting::of(entry, &option_words).is_some() => {
                    let receiver = match &cur {
                        Cur::Value { shape, count, .. } => (*shape, *count),
                        _ => (Shape::Many, Count::NotKnown),
                    };
                    self.counted_after(link, receiver, arguments, allowance)
                        .unwrap_or((shape, Count::NotKnown))
                }
                _ => (shape, Count::NotKnown),
            };
            let receiver_taken = match &cur {
                Cur::Value { taken, .. } => *taken,
                _ => Taken::default(),
            };
            let taken = taken_after(link, receiver_taken, arguments, shape);
            return (
                Cur::Value {
                    dtype,
                    shape,
                    count,
                    taken,
                },
                standing,
            );
        }
        let statement = arguments
            .iter()
            .find(|(name, _)| name == "statement")
            .map(|(_, operand)| *operand);
        let statement_class: Valued = (
            statement.and_then(|operand| operand.dtype),
            statement.and_then(|operand| operand.loose),
        );
        // Whether the predicates and the value this link is given give one
        // value.
        let given_known = arguments
            .iter()
            .filter(|(name, _)| matches!(name.as_str(), "predicates" | "statement"))
            .all(|(_, operand)| operand.known);
        let given_values_single = arguments
            .iter()
            .filter(|(name, _)| name == "statement")
            .all(|(_, operand)| operand.shape == Shape::One);
        let given_single = arguments
            .iter()
            .filter(|(name, _)| matches!(name.as_str(), "predicates" | "statement"))
            .all(|(_, operand)| operand.shape == Shape::One);
        // A condition that ends gives the dtype its values have, when they
        // have one.
        let gives = match (&cur, entry.id().as_str()) {
            (Cur::Condition { values, .. }, "Then.otherwise" | "ChainedThen.otherwise") => {
                let mut all = values.clone();
                all.push(statement_class);
                unify(&all)
            }
            _ => None,
        };
        let unclassed: Vec<&str> = arguments
            .iter()
            .map(|(name, _)| name.as_str())
            .filter(|name| !given.iter().any(|(given, _)| given == name))
            .collect();
        let held_by_arguments: Vec<(&str, Fields)> = arguments
            .iter()
            .filter_map(|(name, operand)| {
                Some((name.as_str(), self.fieldsets[operand.fields?.0].clone()))
            })
            .collect();
        let argument_fields: Vec<(&str, &Fields)> = held_by_arguments
            .iter()
            .map(|(name, fields)| (*name, fields))
            .collect();
        let open_numbers: Vec<&str> = resolved
            .iter()
            .filter(|(_, _, number)| number.is_some())
            .map(|(name, _, _)| *name)
            .collect();
        let chosen = Chosen::default()
            .with_open_numbers(&open_numbers)
            .following_an_open_number(receiver_loose.is_some())
            .among_others(self.siblings_of_the_item())
            .with_argument_fields(&argument_fields)
            .with_arguments(&given)
            .with_unclassed(&unclassed)
            .with_options(&words)
            .with_patterns(&patterns)
            .with_durations(&durations)
            .with_time_zones(&time_zones)
            .following(unit)
            .following_width(width)
            .following_fields(fields_followed.as_ref())
            .with_names(&names)
            .with_numbers(&numbers)
            .named(output_name)
            .in_zone(zone_name.as_deref())
            .with_zone_clash(clash_of_zones);
        let chosen = match gives {
            Some(dtype) => chosen.giving(dtype),
            None => chosen,
        };
        let verdict = allowed::offer(entry, &place, &chosen);
        if matches!(
            &verdict,
            Verdict::Allowed(found) if matches!(found.can_end(), Ending::NotKnown(Unknown::AnUnnestAsAKey))
        ) {
            self.keyed_unnests += 1;
        }
        let mut standing = Standing {
            follows: Some(after),
            verdict: Some(verdict.clone()),
        };
        let allowance = match &verdict {
            Verdict::Refused(refusal) => {
                self.refuse(refusal, false);
                return (Cur::Dead, standing);
            }
            Verdict::Allowed(allowance) => allowance,
        };
        // Arrays that meet and give an array, or that one is looked up in, are of one width, which
        // is known: Polars refuses a sum of arrays of two widths or to look one up in the other,
        // and gives a list where it fills one with the other. What an array is partitioned or
        // sorted by does not meet it.
        if (matches!(
            allowance.result().and_then(allowed::ResultDtype::class),
            Some(Representative::Array(_))
        ) || entry.id().as_str() == "Expr.is_in")
            && matches!(
                &cur,
                Cur::Value {
                    dtype: Some(Representative::Array(_)),
                    ..
                }
            )
        {
            let met: Vec<Option<usize>> = arguments
                .iter()
                .filter(|(name, operand)| {
                    matches!(operand.dtype, Some(Representative::Array(_)))
                        && meets_the_receiver(entry, name)
                })
                .map(|(_, operand)| operand.width)
                .collect();
            if !met.is_empty()
                && !(width.is_some() && met.iter().all(|other| *other == width))
                && let Err(refusal) = allowed::rule_forbids_arrays_of_other_widths()
            {
                self.refuse(&refusal, false);
                return (
                    Cur::Dead,
                    Standing {
                        follows: standing.follows,
                        verdict: Some(Verdict::Refused(refusal)),
                    },
                );
            }
        }
        // Polars refuses the rows of a replacement given values of other numbers than it takes,
        // which the shapes of the arguments say.
        if let Cur::Value { shape, .. } = &cur
            && !branches_are_one
            && gives_other_numbers_of_values(entry, arguments, *shape)
            && let Err(refusal) = allowed::rule_forbids_replacement_values_of_other_numbers()
        {
            self.refuse(&refusal, false);
            return (
                Cur::Dead,
                Standing {
                    follows: standing.follows,
                    verdict: Some(Verdict::Refused(refusal)),
                },
            );
        }
        *last_end = Some(allowance.can_end().clone());
        // How many values the call gives is what the shape of the call and the counts of what it
        // follows and is given make of it, which the tables, measured with columns, do not say. Where
        // it is not the shape that was offered, the chain may end where it does by the one it is.
        let receiver = match &cur {
            Cur::Value { shape, count, .. } => (*shape, *count),
            _ => (Shape::Many, Count::NotKnown),
        };
        // The values a replacement lists and the default it gives are as many as what they meet, or
        // one, where one of them is a count too.
        if matches!(cur, Cur::Value { .. })
            && !branches_are_one
            && replaces_by_another_count(entry, arguments, receiver)
            && let Err(refusal) = allowed::rule_forbids_values_of_other_counts()
        {
            let refusal = if involves_a_list {
                allowed::refusal_of_a_connected_list(&refusal)
            } else {
                refusal
            };
            self.refuse(&refusal, false);
            return (
                Cur::Dead,
                Standing {
                    follows: standing.follows,
                    verdict: Some(Verdict::Refused(refusal)),
                },
            );
        }
        // A parameter that Polars reads as one value is given one: not the rows, not a count, and, where it
        // must be one that Polars can tell is one, not one that a method took of one value.
        if matches!(cur, Cur::Value { .. })
            && let Some((parameter, given)) = given_other_than_one_value(entry, arguments)
            && let Err(refusal) =
                allowed::rule_forbids_a_parameter_given_more_than_one_value(parameter, given)
        {
            self.refuse(&refusal, false);
            return (
                Cur::Dead,
                Standing {
                    follows: standing.follows,
                    verdict: Some(Verdict::Refused(refusal)),
                },
            );
        }
        let receiver_taken = match &cur {
            Cur::Value { taken, .. } => *taken,
            _ => Taken::default(),
        };
        let next = match (entry.applies(), cur) {
            (Applies::Expression, _) => {
                let (shape, count) = match self.counted_after(link, receiver, arguments, allowance)
                {
                    Ok(found) => found,
                    Err(refusal) => {
                        let refusal = if involves_a_list {
                            allowed::refusal_of_a_connected_list(&refusal)
                        } else {
                            refusal
                        };
                        self.refuse(&refusal, false);
                        return (
                            Cur::Dead,
                            Standing {
                                follows: standing.follows,
                                verdict: Some(Verdict::Refused(refusal)),
                            },
                        );
                    }
                };
                if shape != allowance.shape() {
                    let again = allowed::offer(entry, &place, &chosen.giving_shape(shape));
                    if let Verdict::Allowed(refreshed) = &again {
                        *last_end = Some(refreshed.can_end().clone());
                    }
                    standing.verdict = Some(again);
                }
                Cur::Value {
                    dtype: allowance.result().and_then(allowed::ResultDtype::class),
                    shape,
                    count,
                    taken: taken_after(link, receiver_taken, arguments, shape),
                }
            }
            (
                Applies::Condition,
                Cur::Condition {
                    mut values,
                    mut zones,
                    mut widths,
                    mut structs,
                    single,
                    values_single,
                    known,
                    meeting,
                    taken,
                    ..
                },
            ) => {
                let taken = arguments
                    .iter()
                    .filter(|(name, _)| matches!(name.as_str(), "predicates" | "statement"))
                    .fold(taken, |all, (_, operand)| all.or(operand.taken));
                // The predicates and the values a condition is given are as many as one another,
                // unless the values are one expression, whose predicates Polars drops.
                let mut meeting = meeting;
                if !branches_are_one {
                    let parts = arguments
                        .iter()
                        .filter(|(name, operand)| {
                            matches!(name.as_str(), "predicates" | "statement")
                                && operand.known
                                && operand.reliable
                        })
                        .map(|(_, operand)| (operand.shape, operand.count));
                    for part in parts {
                        match meet(meeting, part) {
                            Some(met) => meeting = met,
                            None => {
                                let Err(refusal) = allowed::rule_forbids_values_of_other_counts()
                                else {
                                    unreachable!("the rule always refuses")
                                };
                                let refusal = if involves_a_list {
                                    allowed::refusal_of_a_connected_list(&refusal)
                                } else {
                                    refusal
                                };
                                self.refuse(&refusal, false);
                                return (
                                    Cur::Dead,
                                    Standing {
                                        follows: standing.follows,
                                        verdict: Some(Verdict::Refused(refusal)),
                                    },
                                );
                            }
                        }
                    }
                }
                let single = single && given_single;
                let values_single = values_single && given_values_single;
                let mut known = known && given_known;
                let state = match entry.id().as_str() {
                    "When.then" => {
                        values.push(statement_class);
                        zones.push(statement.and_then(|operand| operand.zone));
                        widths.push(statement.and_then(|operand| operand.width));
                        structs.push(self.fields_of(statement.as_ref()));
                        ConditionState::Then
                    }
                    "ChainedWhen.then" => {
                        values.push(statement_class);
                        zones.push(statement.and_then(|operand| operand.zone));
                        widths.push(statement.and_then(|operand| operand.width));
                        structs.push(self.fields_of(statement.as_ref()));
                        ConditionState::ChainedThen
                    }
                    "Then.when" | "ChainedThen.when" => ConditionState::ChainedWhen,
                    _ => {
                        values.push(statement_class);
                        widths.push(statement.and_then(|operand| operand.width));
                        structs.push(self.fields_of(statement.as_ref()));
                        // Structs of other fields that are the values of a condition meet as
                        // Polars casts them, which it refuses or not by what the fields are.
                        if structs_of_other_fields(&values, &structs) {
                            known = false;
                            *last_end = Some(Ending::NotKnown(Unknown::StructsOfOtherFieldsMeet));
                        }
                        // Arrays that are the values of a condition are an array where they are
                        // of one width, which is known, and a list where they are not.
                        if arrays_of_other_widths(&values, &widths)
                            && let Err(refusal) = allowed::rule_forbids_arrays_of_other_widths()
                        {
                            self.refuse(&refusal, false);
                            return (
                                Cur::Dead,
                                Standing {
                                    follows: standing.follows,
                                    verdict: Some(Verdict::Refused(refusal)),
                                },
                            );
                        }
                        let dtype = if known { unify(&values) } else { None };
                        let (shape, count) = if meeting.0 == Shape::Counted {
                            meeting
                        } else if (branches_are_one && values_single)
                            || (!branches_are_one && single)
                        {
                            (Shape::One, Count::NotKnown)
                        } else {
                            // Where the values are one expression the predicates are dropped,
                            // and the values give the shape.
                            (Shape::Many, Count::NotKnown)
                        };
                        // What the condition gives is not the shape that its last link was offered
                        // with: a count may not end where the rows do.
                        if shape != allowance.shape() {
                            let again = allowed::offer(entry, &place, &chosen.giving_shape(shape));
                            if let Verdict::Allowed(refreshed) = &again {
                                *last_end = Some(refreshed.can_end().clone());
                            }
                            standing.verdict = Some(again);
                        }
                        // Numbers typed without a dtype that have no typed value beside them
                        // have none to give, and Polars cannot always resolve them where it
                        // runs, so whether the expression may end here is not known.
                        if dtype.is_none() && values.iter().any(|(_, loose)| loose.is_some()) {
                            *last_end = Some(Ending::NotKnown(Unknown::TheResultsDtype));
                        }
                        return (
                            Cur::Value {
                                dtype,
                                shape,
                                count,
                                taken: taken.if_one(shape),
                            },
                            standing,
                        );
                    }
                };
                Cur::Condition {
                    state,
                    values,
                    meeting,
                    taken,
                    zones,
                    widths,
                    structs,
                    single,
                    values_single,
                    known,
                }
            }
            _ => Cur::Dead,
        };
        (next, standing)
    }

    /// How many values the call of a method gives, from the count of the value it follows, what it
    /// is given and what the tables offered of it. `receiver` is the shape and the count of what it
    /// follows. The refusal is where the values it meets are not as many as one another, or the
    /// window it is cannot be given what it follows.
    fn counted_after(
        &self,
        link: &Call,
        receiver: (Shape, Count),
        arguments: &[(String, Operand)],
        offered: &allowed::Allowance,
    ) -> Result<(Shape, Count), Refusal> {
        let entry = link.entry();
        let (words, _) = option_words(link);
        let words: Vec<(&str, &str)> = words
            .iter()
            .map(|(name, word)| (name.as_str(), word.as_str()))
            .collect();
        let operand = |name: &str| {
            arguments
                .iter()
                .find(|(given, _)| given == name)
                .map(|(_, operand)| operand)
                .filter(|operand| operand.known && operand.reliable)
        };
        // The values a method that gives a count meets as it follows them: a test of which hold,
        // as many as the values it follows, or one.
        let meets = |receiver: (Shape, Count)| -> Result<(Shape, Count), Refusal> {
            let mut met = receiver;
            for (name, given) in arguments {
                if !(given.known && given.reliable && meets_receiver(entry, name, given)) {
                    continue;
                }
                met = meet(met, (given.shape, given.count)).ok_or_else(|| {
                    allowed::rule_forbids_values_of_other_counts()
                        .expect_err("the rule always refuses")
                })?;
            }
            Ok(met)
        };
        if let Some(counting) = Counting::of(entry, &words) {
            if let Counting::TheArgument(parameter) = counting {
                return Ok(
                    operand(parameter).map_or((Shape::Many, Count::NotKnown), |given| {
                        (given.shape, given.count)
                    }),
                );
            }
            if counting == Counting::TheCondition {
                // The values it follows and the tests of them are as many as one another, and
                // what holds is a count, however few values the receiver has: one value is met
                // by each test.
                let (shape, _) = meets(receiver)?;
                let most = match (receiver.0, shape) {
                    (Shape::One, Shape::One) => Some(1),
                    (Shape::Counted, _) => receiver.1.most(),
                    _ => None,
                };
                return Ok((Shape::Counted, most.map_or(Count::NotKnown, Count::AtMost)));
            }
            let held = match receiver.0 {
                Shape::One => Some(Count::Exactly(1)),
                Shape::Counted => Some(receiver.1),
                Shape::Many => None,
            };
            let count = counted_by(link, counting, held);
            // What is taken of one value is one value where it leaves it, and a count where it
            // leaves none or does not say.
            return Ok(if receiver.0 == Shape::One && count == Count::Exactly(1) {
                (Shape::One, Count::NotKnown)
            } else {
                (Shape::Counted, count)
            });
        }
        // A window gives each row of its group the value it gives the group: a count is not as
        // long as the group, unless the window joins the values of a group or explodes them.
        if entry.id().as_str() == "Expr.over" && receiver.0 == Shape::Counted {
            return match link.argument("mapping_strategy") {
                Some(Value::Choice(word)) if word == "join" => Ok((Shape::Many, Count::NotKnown)),
                Some(Value::Choice(word)) if word == "explode" => {
                    Ok((Shape::Counted, Count::NotKnown))
                }
                _ => Err(allowed::rule_forbids_a_window_over_a_count()
                    .expect_err("the rule always refuses")),
            };
        }
        // The offer of a method that follows a count says the count where the tables say a value for
        // each of the values it follows; a count that the tables themselves say is not that.
        let table = match (offered.shape(), receiver.0) {
            (Shape::Counted, Shape::Counted) => Shape::Many,
            (Shape::Counted, _) => return Ok((Shape::Counted, Count::NotKnown)),
            (shape, _) => shape,
        };
        let widened = broadcast(link, arguments, table);
        // An aggregate reduces what it follows, wherever the tables measured it, and a method that
        // gives a value for each of the values it follows keeps them, which one value met by many
        // widens, and one value met by a count is.
        if is_one_value(link) || (offered.shape() == Shape::One && receiver.0 != Shape::One) {
            return Ok((widened, Count::NotKnown));
        }
        let start = if receiver.0 == Shape::One && widened == Shape::Many {
            (Shape::Many, Count::NotKnown)
        } else {
            receiver
        };
        meets(start)
    }
}

/// Whether the one value a call gives is one that a method took of one value, or that meets such a
/// value: a method that takes some of the values gives one, an aggregate gives one that is not, and
/// a method that gives a value for each of the values it follows gives one where what it follows, or
/// an operand it meets, is. No row of a group takes it as the value of the group, so a window
/// refuses it.
fn taken_after(
    link: &Call,
    receiver_taken: Taken,
    arguments: &[(String, Operand)],
    shape: Shape,
) -> Taken {
    if shape != Shape::One {
        return Taken::default();
    }
    let read = arguments
        .iter()
        .fold(Taken::default(), |all, (name, operand)| {
            all.or(Taken {
                window: operand.taken.window && carries_to_a_window(link.entry(), name, operand),
                scalar: operand.taken.scalar && carries_to_a_scalar(link.entry(), name),
            })
        });
    if is_counting(link) {
        return Taken::ALL;
    }
    // An aggregate reduces what it follows, and keeps what it is given to read, as a statistic its share.
    if is_one_value(link) {
        return read;
    }
    receiver_taken.or(read)
}

/// Whether a value that a method took of one value, given to the parameter `name` of `entry`, makes
/// what the call gives one that a window refuses. It does wherever the call reads the value, as a part
/// of a moment, a bound, a fill, a statistic's share or a strict replacement's default, and whether or
/// not Polars meets it with each of the values the call follows. It does not where the call only
/// looks up in it or by it: the keys of a sort order the values and are no part of them, and what a
/// replacement lists is a table of values it looks up. The lists `is_in` looks up in are met row by
/// row with the values it follows, so they are carried where they are known to be lists.
fn carries_to_a_window(entry: &Entry, name: &str, operand: &Operand) -> bool {
    match (entry.id().as_str(), name) {
        ("Expr.sort_by", "by" | "more_by")
        | ("Expr.replace" | "Expr.replace_strict", "old" | "new") => false,
        ("Expr.is_in", "other") => is_met_by_its_dtype(entry, name, operand),
        _ => true,
    }
}

/// Whether a value that a method took of one value, given to the parameter `name` of `entry`, makes
/// what the call gives one that Polars cannot tell is one before the rows run. It does for every
/// parameter, the keys of a sort included, but not for what a replacement lists and the lists `is_in`
/// looks up in, which Polars looks up in and does not ask whether they are a scalar.
fn carries_to_a_scalar(entry: &Entry, name: &str) -> bool {
    !matches!(
        (entry.id().as_str(), name),
        ("Expr.replace" | "Expr.replace_strict", "old" | "new") | ("Expr.is_in", "other")
    )
}

/// What two values that a call meets give together: one value is met by each of the other, and
/// the others are as many as one another. `None` where they are not: the table's rows are not a
/// count, and two counts that are fixed and differ are not the same.
fn meet(first: (Shape, Count), second: (Shape, Count)) -> Option<(Shape, Count)> {
    match (first.0, second.0) {
        (Shape::One, _) => Some(second),
        (_, Shape::One) | (Shape::Many, Shape::Many) => Some(first),
        (Shape::Counted, Shape::Counted) => first
            .1
            .meeting(second.1)
            .map(|count| (Shape::Counted, count)),
        _ => None,
    }
}

/// Whether the parameter `name` of the method `entry` is met with the value the method follows, as
/// the other side of an operation is, and so must give as many values as it, or one. What the
/// method takes the values of, and what a window keys by, are not.
fn meets_receiver(entry: &Entry, name: &str, operand: &Operand) -> bool {
    let id = entry.id().as_str();
    if id == "Expr.over" {
        return false;
    }
    if let Some(Counting::TheArgument(parameter)) = Counting::of(entry, &[]) {
        return parameter != name;
    }
    is_met_by_its_dtype(entry, name, operand)
        && entry
            .parameters()
            .iter()
            .any(|parameter| parameter.name() == name)
}

/// Whether the operand `name` of `entry` is met with each value of the value the method follows, as
/// the catalogue says of the parameter, except where the dtype of the operand decides it: `is_in` meets
/// a list or an array row by row with the values it follows, and is given nothing else but what its
/// rule refuses, so an operand that is not known to be one is not met.
fn is_met_by_its_dtype(entry: &Entry, name: &str, operand: &Operand) -> bool {
    if entry.id().as_str() == "Expr.is_in" && name == "other" {
        return operand.dtype.is_some_and(Representative::is_nested);
    }
    broadcasts(entry, name)
}

/// Why a method that cuts the values it follows may not cut them by what it is given, where it is a
/// number typed in that Polars cannot cut by: below nothing for how many values the first, the last
/// and `limit` keep and for the length of a slice, and for every place and count one with a
/// fraction, which Polars refuses as no whole number, whatever its value is. The number is read as
/// the step engine makes it, so a float or a text given the dtype of whole numbers is the whole
/// number it casts to.
fn cut_below_nothing(link: &Call) -> Option<String> {
    let parameters: &[&str] = match link.entry().id().as_str() {
        "Expr.head" | "Expr.limit" | "Expr.tail" => &["n"],
        "Expr.slice" => &["offset", "length"],
        "Expr.gather_every" => {
            return matches!(link.argument("n"), Some(Value::Integer(0))).then(|| {
                "gather_every needs n above nothing: Polars refuses to take every 0th value"
                    .to_owned()
            });
        }
        _ => return None,
    };
    parameters.iter().find_map(|parameter| {
        match cut_number(link, parameter)? {
            CutNumber::Whole(number) if number < 0 && *parameter != "offset" => Some(format!(
                "values cannot be cut by {parameter} {number}: Polars cannot extract a length below nothing from them"
            )),
            CutNumber::Fraction => Some(format!(
                "values cannot be cut by {parameter} given as a number with a fraction: Polars needs it to be integral"
            )),
            CutNumber::Whole(_) => None,
        }
    })
}

/// A number typed in that a cut reads.
enum CutNumber {
    /// A whole number, as written or as a dtype of whole numbers makes it.
    Whole(i128),
    /// A number with a fraction typed without a dtype, which Polars does not read as a whole number.
    Fraction,
}

/// The number the call is given for `parameter` where it is a literal alone, and what it is.
fn cut_number(link: &Call, parameter: &str) -> Option<CutNumber> {
    let literal = match link.argument(parameter)? {
        Value::Integer(number) => return Some(CutNumber::Whole(i128::from(*number))),
        Value::Expression(expression) if expression.links().is_empty() => {
            match expression.start() {
                Start::Call(start) if start.entry().id().as_str() == "polars.lit" => start,
                _ => return None,
            }
        }
        _ => return None,
    };
    let value = literal.argument("value")?;
    match (literal.argument("dtype"), value) {
        (None, Value::Integer(number)) => Some(CutNumber::Whole(i128::from(*number))),
        (None, Value::Float(_)) => Some(CutNumber::Fraction),
        (Some(Value::Dtype(dtype)), value) => {
            let scalar = catalog::typed_literal(&value.placeholder(), dtype.as_polars()).ok()?;
            whole_value(scalar.value()).map(CutNumber::Whole)
        }
        _ => None,
    }
}

/// A whole number the call is given for `parameter`, typed in, where it is one.
fn whole_number(link: &Call, parameter: &str) -> Option<i64> {
    match link.argument(parameter)? {
        Value::Integer(number) => Some(*number),
        Value::Expression(expression)
            if expression.links().is_empty()
                && matches!(expression.start(), Start::Call(start)
                    if start.entry().id().as_str() == "polars.lit") =>
        {
            let Start::Call(start) = expression.start() else {
                return None;
            };
            match start.argument("value")? {
                Value::Integer(number) => Some(*number),
                _ => None,
            }
        }
        _ => None,
    }
}

/// How many values a method that takes some of the values it follows gives, from the whole numbers
/// it is given and, where it follows a count, what that is.
fn counted_by(link: &Call, counting: Counting, held: Option<Count>) -> Count {
    let held = held.unwrap_or(Count::NotKnown);
    // The most that what it follows holds, where that is known.
    let most = held.most();
    let at_most = |bound: Option<u64>| match (bound, most) {
        (Some(bound), Some(most)) => Count::AtMost(bound.min(most)),
        (Some(bound), None) => Count::AtMost(bound),
        (None, Some(most)) => Count::AtMost(most),
        (None, None) => Count::NotKnown,
    };
    // A count that was fixed stays fixed where a method takes the first of them.
    let fixed = |taken: u64| match held {
        Count::Exactly(count) => Count::Exactly(count.min(taken)),
        _ => at_most(Some(taken)),
    };
    let natural = |number: Option<i64>| number.and_then(|number| u64::try_from(number).ok());
    // What Python takes where the count is not given: ten values of `head`, `tail` and `limit`, and
    // five of `top_k` and `bottom_k`.
    let default_of = |parameter: &str| (parameter == "n").then_some(10).or(Some(5));
    match counting {
        Counting::AtMost(parameter) => {
            let given = if link.argument(parameter).is_none() {
                default_of(parameter)
            } else {
                natural(whole_number(link, parameter))
            };
            given.map_or_else(|| at_most(None), fixed)
        }
        Counting::FromAPlace => {
            let offset = natural(whole_number(link, "offset"));
            let length = natural(whole_number(link, "length"));
            match (held, offset, length) {
                (Count::Exactly(count), Some(offset), Some(length)) => {
                    Count::Exactly(count.saturating_sub(offset).min(length))
                }
                (Count::Exactly(count), Some(offset), None)
                    if link.argument("length").is_none() =>
                {
                    Count::Exactly(count.saturating_sub(offset))
                }
                (_, _, Some(length)) => at_most(Some(length)),
                _ => at_most(None),
            }
        }
        Counting::EveryNth => {
            let every = natural(whole_number(link, "n")).filter(|every| *every > 0);
            let offset = natural(whole_number(link, "offset")).unwrap_or(0);
            match (held, every) {
                (Count::Exactly(count), Some(every)) => {
                    Count::Exactly(count.saturating_sub(offset).div_ceil(every))
                }
                (Count::AtMost(count), Some(every)) => Count::AtMost(count.div_ceil(every)),
                _ => at_most(None),
            }
        }
        // The different values, the most common and their places and counts of one value are that
        // value.
        Counting::TheData if held == Count::Exactly(1) => held,
        Counting::TheData | Counting::TheCondition => at_most(None),
        Counting::TheRowsLess(parameter) => {
            // Python drops one value where it is not told how many.
            let dropped = if link.argument(parameter).is_none() {
                Some(1)
            } else {
                natural(whole_number(link, parameter))
            };
            match (held, dropped) {
                (Count::Exactly(count), Some(dropped)) => {
                    Count::Exactly(count.saturating_sub(dropped))
                }
                _ => at_most(None),
            }
        }
        Counting::TheArgument(_) | Counting::TheArguments => Count::NotKnown,
    }
}

/// The number of values a method gives, which is many where it gives one value of the value it
/// follows and is given an operand of many: the one value is met by each of them, as the count of
/// nulls is by the base of a logarithm that is a column.
fn broadcast(link: &Call, arguments: &[(String, Operand)], shape: Shape) -> Shape {
    let entry = link.entry();
    // A window gives each row of its group the value it gives the group, so one value over a
    // window is as many values as the table has rows, unless the window explodes, which gives
    // one row for each group, and a partitioned one as many as the groups of the rows.
    if entry.id().as_str() == "Expr.over" && shape == Shape::One {
        let explodes = matches!(
            link.argument("mapping_strategy"),
            Some(Value::Choice(word)) if word == "explode"
        );
        // With nothing to partition by the table is one group, so the window gives one value;
        // with a partition it gives one for each group, as many as the rows decide.
        let partitions = ["partition_by", "more_exprs"].iter().any(|name| {
            matches!(link.argument(name), Some(Value::Many(items)) if !items.is_empty())
                || matches!(link.argument(name), Some(Value::Expression(_)))
        });
        return if explodes && !partitions {
            shape
        } else {
            Shape::Many
        };
    }
    // Polars meets a receiver of one value with each value of an operand of many, whatever the
    // parameter is: a fact about the method, which the catalogue holds apart from how a number
    // typed without a dtype adapts. What a replacement lists is a table of values, which gives no
    // more values than what it follows, and a shift takes one value only: neither broadcasts.
    let given_many = arguments.iter().any(|(name, operand)| {
        operand.shape == Shape::Many && is_met_by_its_dtype(entry, name, operand)
    });
    if shape == Shape::One && given_many {
        Shape::Many
    } else {
        shape
    }
}

/// Whether the call `entry` is given an expression that gives many values for a parameter that
/// takes as many as another, or one, where that is one value: `new` beside an `old` of one, and a
/// strict replacement's `default` beside a value of one. An expression that was not walked to its
/// end has no shape that can be counted on, and is not judged.
fn gives_other_numbers_of_values(
    entry: &Entry,
    arguments: &[(String, Operand)],
    receiver: Shape,
) -> bool {
    let operand = |name: &str| {
        arguments
            .iter()
            .find(|(given, _)| given == name)
            .map(|(_, operand)| operand)
            .filter(|operand| operand.known && operand.reliable)
    };
    entry.parameters().iter().any(|parameter| {
        let gives_many = parameter.as_many_as().is_some()
            && operand(parameter.name()).is_some_and(|mine| mine.shape == Shape::Many);
        if !gives_many {
            return false;
        }
        match parameter.as_many_as() {
            Some(AsManyAs::TheReceiver) => receiver == Shape::One,
            Some(AsManyAs::Parameter(other)) => {
                operand(other).is_some_and(|other| other.shape == Shape::One)
            }
            None => false,
        }
    })
}

/// The first parameter of `entry` that Polars reads as one value and is given another, with what it
/// was given: the rows, a count of values, or, for a parameter that must be a scalar, one value that a
/// method took of one value. An expression that was not walked to its end is not judged.
fn given_other_than_one_value<'a>(
    entry: &'a Entry,
    arguments: &[(String, Operand)],
) -> Option<(&'a str, &'static str)> {
    entry.parameters().iter().find_map(|parameter| {
        let how = parameter.one_value()?;
        let operand = arguments
            .iter()
            .find(|(name, _)| name == parameter.name())
            .map(|(_, operand)| operand)
            .filter(|operand| operand.known && operand.reliable)?;
        let given = match operand.shape {
            Shape::Many => "the rows",
            Shape::Counted => "a count of values",
            Shape::One if operand.taken.scalar && how == catalog::OneValue::Scalar => {
                "one value that a method took of one value"
            }
            Shape::One => return None,
        };
        Some((parameter.name(), given))
    })
}

/// Whether the call `entry` is given values for a parameter that gives as many as another, or one,
/// of a count that is not as many as that: a count is not the rows, and two counts that are fixed and
/// differ are not the same. Values of one and of many are judged by [`gives_other_numbers_of_values`].
fn replaces_by_another_count(
    entry: &Entry,
    arguments: &[(String, Operand)],
    receiver: (Shape, Count),
) -> bool {
    let operand = |name: &str| {
        arguments
            .iter()
            .find(|(given, _)| given == name)
            .map(|(_, operand)| operand)
            .filter(|operand| operand.known && operand.reliable)
            .map(|operand| (operand.shape, operand.count))
    };
    entry.parameters().iter().any(|parameter| {
        let (Some(mine), Some(as_many)) = (operand(parameter.name()), parameter.as_many_as())
        else {
            return false;
        };
        let other = match as_many {
            AsManyAs::TheReceiver => Some(receiver),
            AsManyAs::Parameter(other) => operand(other),
        };
        other.is_some_and(|other| {
            (mine.0 == Shape::Counted || other.0 == Shape::Counted) && meet(mine, other).is_none()
        })
    })
}

// ---------------------------------------------------------------------------
// Keys

/// The expressions a call is given.
fn operands(call: &Call) -> Vec<&Expression> {
    fn collect<'a>(value: &'a Value, found: &mut Vec<&'a Expression>) {
        match value {
            Value::Expression(expression) => found.push(expression),
            Value::Many(items) => items.iter().for_each(|item| collect(item, found)),
            Value::Pairs(pairs) => pairs.iter().for_each(|(key, entry)| {
                collect(key, found);
                collect(entry, found);
            }),
            _ => {}
        }
    }
    let mut found = Vec::new();
    for given in call.arguments() {
        collect(given.value(), &mut found);
    }
    found
}

pub(super) const ONE_VALUE: [&str; 32] = [
    "Expr.sum",
    "Expr.min",
    "Expr.max",
    "Expr.len",
    "Expr.first",
    "Expr.last",
    "Expr.has_nulls",
    "Expr.null_count",
    "Expr.n_unique",
    "Expr.count",
    "Expr.str.join",
    "Expr.mean",
    "Expr.median",
    "Expr.std",
    "Expr.var",
    "Expr.quantile",
    "Expr.product",
    "Expr.skew",
    "Expr.kurtosis",
    "Expr.entropy",
    "Expr.arg_max",
    "Expr.arg_min",
    "Expr.nan_max",
    "Expr.nan_min",
    "Expr.all",
    "Expr.any",
    "Expr.approx_n_unique",
    "Expr.approx_quantile",
    "Expr.bitwise_and",
    "Expr.bitwise_or",
    "Expr.bitwise_xor",
    "Expr.implode",
];

/// What the values of a connected list are once they are gathered into one list: a list of their
/// dtype, which is one value. Where the dtype is not one a list of the tables covers, it is not
/// known.
fn imploded(values: Operand) -> Operand {
    let listed = values
        .dtype
        .and_then(|class| allowed::Inner::of_class(class).map(Representative::List));
    Operand {
        dtype: listed,
        loose: None,
        shape: Shape::One,
        count: Count::NotKnown,
        known: values.known && listed.is_some(),
        reliable: values.reliable && listed.is_some(),
        zone: None,
        width: None,
        fields: None,
        ..values
    }
}

fn is_one_value(call: &Call) -> bool {
    ONE_VALUE.contains(&call.entry().id().as_str())
}

/// Whether the call takes some of the values it follows, so that how many it gives is for the data
/// and the arguments to say: `head`, `filter`, `unique` and the rest that the catalogue says count.
/// Such a call is not elementwise, whatever it follows.
fn is_counting(call: &Call) -> bool {
    let (words, _) = option_words(call);
    let words: Vec<(&str, &str)> = words
        .iter()
        .map(|(name, word)| (name.as_str(), word.as_str()))
        .collect();
    Counting::of(call.entry(), &words).is_some()
}

/// Whether the first `upto` links of `expression` give one value and nothing
/// beside them does not: an aggregate among them with only single values after
/// it; with `literals`, a start made of literals too; or a condition whose parts
/// are single.
fn wholly_single(expression: &Expression, upto: usize, literals: bool) -> bool {
    let links = &expression.links()[..upto.min(expression.links().len())];
    let single = |call: &Call| {
        // What a replacement lists is a table of values, which gives no more values than what it
        // follows, and its default is as many as that or one. A call that takes some of the
        // values of what it follows is not one value even of a literal: it is not elementwise.
        !is_counting(call)
            && (is_a_replacement(call)
            // An interval with a null bound that has no dtype is folded whole into a null, and
            // what its other bound reads is not met.
            || (call.entry().id().as_str() == "Expr.is_between"
                && operands(call).into_iter().any(is_an_untyped_null))
            || operands(call)
                .into_iter()
                .all(|inside| wholly_single(inside, inside.links().len(), true)))
    };
    if let Some(last) = links.iter().rposition(is_one_value) {
        return links[last + 1..].iter().all(single);
    }
    match expression.start() {
        Start::Call(start) if start.entry().id().as_str() == "polars.lit" => {
            literals && links.iter().all(single)
        }
        Start::Call(start) if start.entry().id().as_str() == "polars.when" => {
            single(start) && links.iter().all(single)
        }
        // A range or a repeat is a count of values, and is none of one value whatever it is given.
        Start::Call(start) if Counting::of(start.entry(), &[]) == Some(Counting::TheArguments) => {
            false
        }
        // A function that makes a moment of what it is given is one value where each of them is.
        Start::Call(start) if takes_expressions(start) => single(start) && links.iter().all(single),
        _ => false,
    }
}

/// Whether `start` is a function that starts an expression and takes expressions, as the ones
/// that make a date, a moment or a length of time do: it gives as many values as its widest
/// argument.
fn takes_expressions(start: &Call) -> bool {
    start.entry().applies() == Applies::Function
        && start
            .entry()
            .parameters()
            .iter()
            .any(|parameter| matches!(parameter.kind(), Kind::Expression(_)))
}

/// Whether `link` is a comparison, which with a null literal gives a null. `is_between` is two: the
/// value compared with each of its bounds, so a bound that is a null literal with no dtype is a
/// comparison with one. A null bound of a dtype is one Polars folds for some dtypes of what it
/// bounds and not for others, which the document does not decide, so it is left to a limit.
/// `eq_missing` and `ne_missing` are not comparisons: they take a null as a value.
fn is_a_comparison(link: &Call) -> bool {
    is_a_comparison_of_its_receiver(link)
        || (link.entry().id().as_str() == "Expr.is_between"
            && operands(link).into_iter().any(is_an_untyped_null))
}

/// Whether an expression is a null literal and no more, which is given no dtype.
fn is_an_untyped_null(expression: &Expression) -> bool {
    is_a_null(expression)
        && matches!(expression.start(), Start::Call(start) if start.argument("dtype").is_none())
}

/// Whether `link` compares what it follows with what it is given, so that a null literal it
/// follows, and not only one it is given, gives a null: the six comparisons do, and `is_between`,
/// which compares each of its bounds with what it follows, does not.
fn is_a_comparison_of_its_receiver(link: &Call) -> bool {
    Comparison::of_entry(link.entry().id().as_str()).is_some()
}

/// Whether `link` is the expression it follows, as `and_` and `or_` are where they are given no
/// operand: Polars does not make a node of the call, so what comes after it meets what comes
/// before it.
fn is_the_expression_it_follows(link: &Call) -> bool {
    matches!(link.entry().id().as_str(), "Expr.and_" | "Expr.or_")
        && match link.argument("others") {
            None => true,
            Some(Value::Many(items)) => items.is_empty(),
            Some(_) => false,
        }
}

/// Whether the calls of `expression` before the link `at` leave its start as it was, for Polars
/// does not make a node of a call that is the expression it follows, and simplifies a not of a
/// not away: the link `at` is then the first it meets after the start.
fn follows_the_start(expression: &Expression, at: usize) -> bool {
    let mut nots = 0_usize;
    for link in &expression.links()[..at] {
        if link.entry().id().as_str() == "Expr.not_" {
            nots += 1;
        } else if !is_the_expression_it_follows(link) {
            return false;
        }
    }
    nots.is_multiple_of(2)
}

/// The calls of an expression: its start, then its links.
fn calls_of(expression: &Expression) -> impl Iterator<Item = &Call> {
    std::iter::once(expression.start())
        .filter_map(|start| match start {
            Start::Call(call) => Some(call),
            Start::Input(_) | Start::Hole => None,
        })
        .chain(expression.links())
}

/// Whether an expression holds, at any depth, a comparison with a null literal:
/// one given it, or one that follows it at the start. Polars folds it into a
/// null.
fn folds_a_null(expression: &Expression) -> bool {
    let from_a_null = starts_from_a_null(expression);
    expression.links().iter().enumerate().any(|(at, link)| {
        (is_a_comparison_of_its_receiver(link) && from_a_null && follows_the_start(expression, at))
            || (is_a_comparison(link) && operands(link).into_iter().any(is_a_null))
    }) || calls_of(expression).any(|call| operands(call).into_iter().any(folds_a_null))
}

/// Whether a null fold is in the first `upto` calls of an expression, or in what
/// they are given.
fn folds_a_null_before(expression: &Expression, upto: usize) -> bool {
    let from_a_null = starts_from_a_null(expression);
    let links = &expression.links()[..upto];
    links.iter().enumerate().any(|(at, link)| {
        (is_a_comparison_of_its_receiver(link) && from_a_null && follows_the_start(expression, at))
            || (is_a_comparison(link) && operands(link).into_iter().any(is_a_null))
    }) || calls_of(expression)
        .take(upto + 1)
        .any(|call| operands(call).into_iter().any(folds_a_null))
}

/// Whether the comparison `at` of an expression is one that a null reaches:
/// Polars folds it into a null. It is reached where it is given a null or what
/// folds into one, follows a fold, or starts from a null. What Polars folds
/// depends on dtypes, so any fold that can reach a comparison counts.
fn is_reached_by_a_null(expression: &Expression, at: usize) -> bool {
    is_a_comparison(&expression.links()[at])
        && (operands(&expression.links()[at])
            .into_iter()
            .any(|operand| is_a_null(operand) || folds_a_null(operand))
            || (is_a_comparison_of_its_receiver(&expression.links()[at])
                && starts_from_a_null(expression)
                && follows_the_start(expression, at))
            || folds_a_null_before(expression, at))
}

/// Whether an expression holds an aggregate that Polars evaluates. See
/// [`holds_what_polars_evaluates`]. Where a join's key is judged elementwise a subtraction with a
/// null drops nothing: the key `k != f.len() - null` is not elementwise.
fn holds_an_aggregate(expression: &Expression) -> bool {
    holds_what_polars_evaluates(expression, &is_one_value)
}

/// Whether an expression holds a call that `is_held` says yes to and that Polars evaluates, at any
/// depth. A comparison that a null reaches is a null. The first of them drops what it follows and
/// is given where that is one value or literals alone, and the comparisons after it compare a
/// null, so they drop only what they are given where it is one value or literals alone, except
/// one that is given a null, which drops what it follows too where that is one value or literals
/// alone: what comes before the last that does is not evaluated. A side that is more is
/// evaluated, and what it holds counts.
fn holds_what_polars_evaluates(expression: &Expression, is_held: &dyn Fn(&Call) -> bool) -> bool {
    let evaluated = |call: &Call| {
        is_held(call)
            || operands(call)
                .into_iter()
                .any(|operand| holds_what_polars_evaluates(operand, is_held))
    };
    let start = match expression.start() {
        Start::Call(call) => Some(call),
        Start::Input(_) | Start::Hole => None,
    };
    let links = expression.links();
    let Some(first) = (0..links.len()).find(|at| is_reached_by_a_null(expression, *at)) else {
        return start.into_iter().chain(links).any(evaluated);
    };
    // A comparison that is given a null drops what it follows where that is one
    // value or literals alone, as the first fold does, so what a later one drops
    // is not evaluated either: the last of them that does is where the key is
    // judged from.
    let first = (first..links.len())
        .rev()
        .find(|at| {
            is_a_comparison(&links[*at])
                && operands(&links[*at]).into_iter().any(is_a_null)
                && wholly_single(expression, *at, true)
        })
        .unwrap_or(first);
    // What the first comparison follows is the start and the links before it,
    // dropped as a whole where they are one value or literals alone.
    let followed = !wholly_single(expression, first, true)
        && start.into_iter().chain(&links[..first]).any(evaluated);
    // What a comparison that a null reaches is given is dropped where it is one
    // value or literals alone, and what is not reached is evaluated.
    let after = links[first..].iter().enumerate().any(|(at, link)| {
        if is_reached_by_a_null(expression, first + at) {
            // An interval with a null bound is folded whole, so what its other bound holds is
            // not met, where a comparison evaluates what it is given that is more than one value.
            link.entry().id().as_str() != "Expr.is_between"
                && operands(link).into_iter().any(|operand| {
                    !wholly_single(operand, operand.links().len(), true)
                        && holds_what_polars_evaluates(operand, is_held)
                })
        } else {
            evaluated(link)
        }
    });
    followed || after
}

/// Whether an expression reads a column: it starts from one, or a call in it is
/// given an expression that does.
fn reads_a_column(expression: &Expression) -> bool {
    let calls = std::iter::once(expression.start())
        .filter_map(|start| match start {
            Start::Call(call) => Some(call),
            Start::Input(_) | Start::Hole => None,
        })
        .chain(expression.links());
    matches!(
        expression.start(),
        Start::Call(start) if start.entry().id().as_str() == "polars.col"
    ) || calls
        .into_iter()
        .any(|call| operands(call).into_iter().any(reads_a_column))
}

/// Whether an expression is a null literal and no more, as it is where only calls that leave it
/// as it was come after it: calls that are the expression they follow, and nots that undo each
/// other.
fn is_a_null(expression: &Expression) -> bool {
    follows_the_start(expression, expression.links().len()) && starts_from_a_null(expression)
}

/// Whether an expression starts from a null literal.
fn starts_from_a_null(expression: &Expression) -> bool {
    matches!(
        expression.start(),
        Start::Call(start)
            if start.entry().id().as_str() == "polars.lit"
                && matches!(start.argument("value"), Some(Value::Flag(None)))
    )
}

/// Whether an expression holds, at any depth, a comparison of a null literal
/// with what reads a column and is not one value. Polars decides the comparison
/// while it coerces the types, and leaves a null repeated to the length of what is compared, which
/// is not elementwise, where that is many values, and the null alone where it is one value. A
/// comparison with what an earlier one decided is left to a limit: what Polars does with it
/// depends on dtypes that the document does not decide.
fn holds_a_null_comparison(expression: &Expression) -> bool {
    let start = match expression.start() {
        Start::Call(call) => Some(call),
        Start::Input(_) | Start::Hole => None,
    };
    let mut reads = matches!(
        start,
        Some(start) if start.entry().id().as_str() == "polars.col"
    ) || start.is_some_and(|start| operands(start).into_iter().any(reads_a_column));
    // A comparison with a null that follows one value or literals alone folds into a null and
    // drops what it follows, the calls before it and what they are given, unevaluated: a
    // comparison of a column with a null among them is not one Polars meets.
    let dropped = expression
        .links()
        .iter()
        .enumerate()
        .filter(|(at, link)| {
            is_a_comparison(link)
                && (operands(link).into_iter().any(is_a_null)
                    || (is_a_comparison_of_its_receiver(link)
                        && starts_from_a_null(expression)
                        && follows_the_start(expression, *at)))
                && wholly_single(expression, *at, true)
        })
        .map(|(at, _)| at + usize::from(start.is_some()))
        .max()
        .unwrap_or(0);
    for (at, link) in expression.links().iter().enumerate() {
        // What a later fold drops is not met, so the comparisons before the last of them, which
        // are among the calls it drops, are not asked about.
        if at + usize::from(start.is_some()) < dropped {
            reads |= operands(link).into_iter().any(reads_a_column);
            continue;
        }
        if is_a_comparison(link) {
            let given = operands(link);
            let null_given = given.iter().any(|operand| is_a_null(operand));
            let column_given = given.iter().any(|operand| {
                reads_a_column(operand) && !wholly_single(operand, operand.links().len(), true)
            });
            let column_before = reads && !wholly_single(expression, at, true);
            let null_start = is_a_comparison_of_its_receiver(link)
                && follows_the_start(expression, at)
                && starts_from_a_null(expression);
            if (null_given && column_before) || (null_start && column_given) {
                return true;
            }
        }
        reads |= operands(link).into_iter().any(reads_a_column);
    }
    // A comparison with a null is a null whatever it is given, so what it is given that is one
    // value or literals alone is not evaluated either, and a comparison of a column with a null
    // in it is not one Polars meets.
    let folds = |at: usize, link: &Call| {
        is_a_comparison(link)
            && (operands(link).into_iter().any(is_a_null)
                || (is_a_comparison_of_its_receiver(link)
                    && starts_from_a_null(expression)
                    && follows_the_start(expression, at)))
    };
    start
        .into_iter()
        .map(|call| (call, false))
        .chain(
            expression
                .links()
                .iter()
                .enumerate()
                .map(|(at, link)| (link, folds(at, link))),
        )
        .skip(dropped)
        .any(|(call, folded)| {
            operands(call)
                .into_iter()
                .filter(|operand| !(folded && wholly_single(operand, operand.links().len(), true)))
                .any(holds_a_null_comparison)
        })
}

/// What the walk found of the comparisons of missing values with a number that a key holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Comparisons {
    /// Each is one the walk could judge, by the dtype of what is compared, or there is none.
    AllJudged,
    /// Some is one Polars may decide by a dtype that the walk does not follow: Polars may have
    /// replaced it by a constant that hides what it is given, so the rest of the key is not judged
    /// as it would be.
    SomeUndecided,
}

/// Whether a key is not elementwise, as Polars takes it: it holds an aggregate
/// beside what is not one value, or a column compared with a null. A key that is
/// wholly one value is the whole key, which the tables measured, and Polars
/// allows it. It takes the null literal beside a column as elementwise, and an
/// aggregate as not. The second expression is the key with each column of the dtype Null made the
/// null literal, which only the comparison with a null reads.
fn is_not_elementwise(
    key: &Expression,
    with_nulls_for_null_columns: &Expression,
    comparisons: Comparisons,
) -> bool {
    !wholly_single(key, key.links().len(), true)
        && !holds_a_comparison_that_polars_may_fold(key)
        && comparisons == Comparisons::AllJudged
        && (holds_an_aggregate(key)
            || holds_what_polars_evaluates(key, &is_counting)
            || holds_a_null_comparison(with_nulls_for_null_columns)
            || holds_a_replacement_of_columns(key))
}

/// `expression` with each column of the dtype Null made a literal, which reads no column, in the
/// calls it holds and what they are given: every value of such a column is a null, so what Polars
/// compares it with a null or with a column leaves no value of it to be replaced. `None` when
/// there is no such column in it, or an expression cannot be rebuilt.
fn with_null_columns_as_literals(expression: &Expression, schema: &Schema) -> Option<Expression> {
    fn value_of(value: &Value, schema: &Schema, changed: &mut bool) -> Option<Value> {
        Some(match value {
            Value::Expression(inside) => Value::Expression(expression_of(inside, schema, changed)?),
            Value::Many(items) => Value::Many(
                items
                    .iter()
                    .map(|item| value_of(item, schema, changed))
                    .collect::<Option<Vec<_>>>()?,
            ),
            Value::Pairs(pairs) => Value::Pairs(
                pairs
                    .iter()
                    .map(|(key, entry)| {
                        Some((
                            value_of(key, schema, changed)?,
                            value_of(entry, schema, changed)?,
                        ))
                    })
                    .collect::<Option<Vec<_>>>()?,
            ),
            other => other.clone(),
        })
    }
    fn call_of(call: &Call, schema: &Schema, changed: &mut bool) -> Option<Call> {
        if call.entry().id().as_str() == "polars.col"
            && let Some(Value::Columns(names)) = call.argument("name")
            && let [name] = names.as_slice()
            && schema
                .get(name)
                .is_some_and(|dtype| matches!(dtype, polars::prelude::DataType::Null))
        {
            *changed = true;
            return Call::new(
                crate::catalog::entry("polars.lit")?,
                [("value", Value::Integer(1))],
            )
            .ok();
        }
        let given = call
            .arguments()
            .iter()
            .map(|given| Some((given.parameter(), value_of(given.value(), schema, changed)?)))
            .collect::<Option<Vec<_>>>()?;
        Call::new(call.entry(), given).ok()
    }
    fn expression_of(
        expression: &Expression,
        schema: &Schema,
        changed: &mut bool,
    ) -> Option<Expression> {
        let start = match expression.start() {
            Start::Call(call) => Start::Call(call_of(call, schema, changed)?),
            other => other.clone(),
        };
        let links = expression
            .links()
            .iter()
            .map(|link| call_of(link, schema, changed))
            .collect::<Option<Vec<_>>>()?;
        Expression::new(start, links).ok()
    }
    let mut changed = false;
    let rebuilt = expression_of(expression, schema, &mut changed)?;
    changed.then_some(rebuilt)
}

/// Whether an expression holds, at any depth, an interval that Polars may decide while it coerces
/// the types: one with a null bound of a dtype, which it decides for some dtypes of what it bounds
/// and not for others, or with a negative whole number as a bound, which it decides at once where
/// what it bounds is unsigned, as a count is, and keeps where it is not. What it leaves of a
/// decided interval is a constant, which is one value where what is bounded is, and a constant
/// repeated to the length of what is bounded where that is many values. The document does not
/// decide which dtype it is, so the key that holds one is judged to be one value that is taken
/// whole, and a limit of the fuzz tests holds the rest. A comparison of missing values with a
/// number that some dtype cannot hold is found as the walk follows the chain, which knows what is
/// compared.
fn holds_a_comparison_that_polars_may_fold(expression: &Expression) -> bool {
    calls_of(expression).any(|call| {
        (call.entry().id().as_str() == "Expr.is_between"
            && operands(call).into_iter().any(|operand| {
                (is_a_null(operand)
                    && matches!(operand.start(), Start::Call(start) if start.argument("dtype").is_some()))
                    || is_a_negative_whole_number(operand)
            }))
            || operands(call)
                .into_iter()
                .any(holds_a_comparison_that_polars_may_fold)
    })
}

/// Whether `link` is an `eq_missing` or an `ne_missing`, which compare a missing value as a
/// value.
fn is_a_comparison_of_missing_values(link: &Call) -> bool {
    matches!(
        link.entry().id().as_str(),
        "Expr.eq_missing" | "Expr.ne_missing"
    )
}

/// The whole-number dtypes a column may have, which a comparison of missing values with a literal
/// is decided for or kept by their own bounds.
const WHOLE_NUMBERS: [Representative; 10] = [
    Representative::Int8,
    Representative::Int16,
    Representative::Int32,
    Representative::Int64,
    Representative::Int128,
    Representative::UInt8,
    Representative::UInt16,
    Representative::UInt32,
    Representative::UInt64,
    Representative::UInt128,
];

/// Whether a dtype of whole numbers holds `number`: it casts to the dtype and keeps its value.
/// `Int128` holds every whole number that fits 128 signed bits, and `UInt128` every one from zero
/// up to that, the ones above it being [`Typed::AboveSigned`], which a text or a float given the
/// dtype of `UInt128` can be.
const fn holds_strictly(dtype: Representative, number: i128) -> bool {
    match dtype {
        Representative::Int8 => number >= i8::MIN as i128 && number <= i8::MAX as i128,
        Representative::Int16 => number >= i16::MIN as i128 && number <= i16::MAX as i128,
        Representative::Int32 => number >= i32::MIN as i128 && number <= i32::MAX as i128,
        Representative::Int64 => number >= i64::MIN as i128 && number <= i64::MAX as i128,
        Representative::UInt8 => number >= 0 && number <= u8::MAX as i128,
        Representative::UInt16 => number >= 0 && number <= u16::MAX as i128,
        Representative::UInt32 => number >= 0 && number <= u32::MAX as i128,
        Representative::UInt64 => number >= 0 && number <= u64::MAX as i128,
        Representative::UInt128 => number >= 0,
        _ => true,
    }
}

/// A literal of a comparison of missing values, as the step engine makes it.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Typed {
    /// A whole number given no dtype, which adapts to what it meets.
    UntypedWhole(i64),
    /// A float given no dtype: a 64 bit float.
    UntypedFloat(f64),
    /// A 32 or 64 bit float, which is 32 bit where `narrow`.
    Float { value: f64, narrow: bool },
    /// A whole number of a dtype of its own.
    Whole { dtype: Representative, value: i128 },
    /// A whole number of `UInt128` above what `Int128` holds, which no signed dtype holds.
    AboveSigned,
    /// Any other: 16 bit floats, `Boolean`, text, or a value that does not cast to its dtype.
    Other,
}

/// The literal that `call` makes, if it is `lit`. Given a dtype it is what the step engine makes,
/// which is `typed_literal`: the value cast strictly to the dtype, so a float given a dtype of
/// whole numbers is a whole number, and a text given a number's dtype is that number.
fn literal_of(call: &Call) -> Option<Typed> {
    use polars::prelude::{AnyValue, DataType as D};
    if call.entry().id().as_str() != "polars.lit" {
        return None;
    }
    let value = call.argument("value")?;
    match call.argument("dtype") {
        None => Some(match value {
            Value::Integer(number) => Typed::UntypedWhole(*number),
            Value::Float(number) => Typed::UntypedFloat(*number),
            _ => Typed::Other,
        }),
        Some(Value::Dtype(dtype)) => {
            let Ok(scalar) = catalog::typed_literal(&value.placeholder(), dtype.as_polars()) else {
                return Some(Typed::Other);
            };
            Some(match (scalar.dtype(), scalar.value()) {
                (D::Float64, AnyValue::Float64(value)) => Typed::Float {
                    value: *value,
                    narrow: false,
                },
                (D::Float32, AnyValue::Float32(value)) => Typed::Float {
                    value: f64::from(*value),
                    narrow: true,
                },
                (D::UInt128, AnyValue::UInt128(value)) if i128::try_from(*value).is_err() => {
                    Typed::AboveSigned
                }
                (dtype, value) => match (Representative::of(dtype), whole_value(value)) {
                    (Some(dtype), Some(value)) if is_a_whole_number(dtype) => {
                        Typed::Whole { dtype, value }
                    }
                    _ => Typed::Other,
                },
            })
        }
        Some(_) => Some(Typed::Other),
    }
}

/// The whole number that a value of Polars is, where it is one.
fn whole_value(value: &polars::prelude::AnyValue) -> Option<i128> {
    use polars::prelude::AnyValue as V;
    Some(match *value {
        V::Int8(value) => i128::from(value),
        V::Int16(value) => i128::from(value),
        V::Int32(value) => i128::from(value),
        V::Int64(value) => i128::from(value),
        V::Int128(value) => value,
        V::UInt8(value) => i128::from(value),
        V::UInt16(value) => i128::from(value),
        V::UInt32(value) => i128::from(value),
        V::UInt64(value) => i128::from(value),
        V::UInt128(value) => i128::try_from(value).ok()?,
        _ => return None,
    })
}

/// Whether Polars decides a comparison of missing values of a column of `column`, a dtype of whole
/// numbers, with `literal`, while it coerces the types, for no value of the column is the
/// literal. It follows `coerce_comparison_literal` of Polars 2.2.0:
///
/// - A literal of 32 or 64 bit floats is read as the number it is, where that number casts to the
///   float exactly: below 2^24 for 32 bit floats where the column meets them as 32 bit floats, which
///   the 8 and 16 bit whole numbers do, and below 2^53 otherwise. A number with a fraction, or a
///   whole number the column cannot hold, is decided. A number that casts inexactly is kept.
/// - A literal of 16 bit floats or of `Boolean`, or of any dtype that is not a number, is kept.
/// - A literal of whole numbers is decided where the column cannot hold it. It is never for a
///   column of `UInt128`, which no signed literal meets, and not where it has the dtype of the
///   column.
/// - A literal that is no dtype of its own is the number it is: a whole number of a dtype that
///   adapts to the column, or a float that is a 64 bit float.
///
/// A comparison with `UInt128` of a whole number typed as a float is decided where the number is
/// below zero, as it is not where the number is typed as a whole number.
fn decides(column: Representative, literal: Typed) -> bool {
    if !is_a_whole_number(column) {
        return false;
    }
    // The floats that Polars rounds to whole numbers: the value as the float holds it, and the
    // number below which a whole number casts to the float exactly.
    let rounded = |value: f64, exact_below: f64| {
        if value.is_nan() || value.abs() >= exact_below {
            return false;
        }
        let floor = value.floor();
        floor != value || !holds_strictly(column, floor as i128)
    };
    match literal {
        // `UInt128` holds every number a document can write, even one below zero.
        Typed::UntypedWhole(number) => {
            column != Representative::UInt128 && !holds_strictly(column, i128::from(number))
        }
        Typed::UntypedFloat(value) => rounded(value, 2f64.powi(53)),
        Typed::Float { value, narrow } => {
            let meets_as_float32 = matches!(
                column,
                Representative::Int8
                    | Representative::Int16
                    | Representative::UInt8
                    | Representative::UInt16
            );
            rounded(
                value,
                if narrow && meets_as_float32 {
                    2f64.powi(24)
                } else {
                    2f64.powi(53)
                },
            )
        }
        Typed::Whole { dtype, value } => {
            dtype != column && column != Representative::UInt128 && !holds_strictly(column, value)
        }
        // Polars decides it for the unsigned columns of 8 to 64 bits, which the rest of the whole
        // numbers meet in a dtype that holds it or in none.
        Typed::AboveSigned => matches!(
            column,
            Representative::UInt8
                | Representative::UInt16
                | Representative::UInt32
                | Representative::UInt64
        ),
        Typed::Other => false,
    }
}

/// What the walk makes of a comparison of missing values with a number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fold {
    /// Polars keeps the comparison, or it holds no number that Polars decides it for.
    Kept,
    /// Polars decides the comparison for a value in each row of a column of whole numbers, none of
    /// which is the number: the number has a fraction, or the dtype of the column cannot hold it.
    Repeats,
    /// A whole number that some dtype cannot hold, or a number with a fraction, where what is
    /// compared is not a value for each row of a dtype that is known: if it is one value, the dtype
    /// of the value decides whether Polars keeps the comparison or leaves a plain constant, which the
    /// walk does not follow, and a limit of the fuzz tests covers it.
    Undecided,
}

/// What Polars makes of the comparison of missing values that is the link `at` of `expression`,
/// where `receiver` is what the chain gives before it and `arguments` are what the link is given.
/// The number is the operand of the link or, where the chain starts from the number, the start, and
/// what is compared with it is the other of the two.
fn fold_of_a_missing_comparison(
    expression: &Expression,
    at: usize,
    receiver: &Cur,
    arguments: &[(String, Operand)],
) -> Fold {
    let link = &expression.links()[at];
    if !is_a_comparison_of_missing_values(link) {
        return Fold::Kept;
    }
    // What is compared, and the literals it is compared with: the value the chain has come to with
    // the operands of the link, and, where the chain starts from a literal, the operand it is given
    // with the start.
    let mut compared: Vec<(Option<Representative>, Shape, Vec<Typed>)> = Vec::new();
    if let Cur::Value { dtype, shape, .. } = receiver {
        let literals = operands(link)
            .into_iter()
            .filter(|operand| operand.links().is_empty())
            .filter_map(|operand| match operand.start() {
                Start::Call(call) => literal_of(call),
                Start::Input(_) | Start::Hole => None,
            })
            .collect();
        compared.push((*dtype, *shape, literals));
    }
    if at == 0
        && let Start::Call(start) = expression.start()
        && let Some(literal) = literal_of(start)
        && let Some((_, other)) = arguments.iter().find(|(name, _)| name == "other")
    {
        compared.push((other.dtype, other.shape, vec![literal]));
    }
    // A literal that some dtype of whole numbers is decided for.
    let is_decided_for_some_dtype =
        |literal: Typed| WHOLE_NUMBERS.iter().any(|dtype| decides(*dtype, literal));
    let mut fold = Fold::Kept;
    for (dtype, shape, literals) in compared {
        for literal in literals {
            match (dtype, shape) {
                (Some(dtype), Shape::Many) => {
                    if decides(dtype, literal) {
                        return Fold::Repeats;
                    }
                }
                _ => {
                    if is_decided_for_some_dtype(literal) {
                        fold = Fold::Undecided;
                    }
                }
            }
        }
    }
    // A literal that leads a chain which calls that leave it as it was follow: what is compared
    // with it is not the first operand, and the dtype of it is not asked.
    if fold == Fold::Kept
        && at > 0
        && matches!(expression.start(), Start::Call(start)
            if literal_of(start).is_some_and(is_decided_for_some_dtype))
        && follows_the_start(expression, at)
    {
        fold = Fold::Undecided;
    }
    fold
}

/// Whether a dtype holds whole numbers only.
const fn is_a_whole_number(dtype: Representative) -> bool {
    matches!(
        dtype,
        Representative::Int8
            | Representative::Int16
            | Representative::Int32
            | Representative::Int64
            | Representative::Int128
            | Representative::UInt8
            | Representative::UInt16
            | Representative::UInt32
            | Representative::UInt64
            | Representative::UInt128
    )
}

/// Whether an expression is a whole number below zero and no more.
fn is_a_negative_whole_number(expression: &Expression) -> bool {
    expression.links().is_empty()
        && matches!(expression.start(), Start::Call(start)
            if start.entry().id().as_str() == "polars.lit"
                && matches!(start.argument("value"), Some(Value::Integer(number)) if *number < 0))
}

/// Whether `call` is a replacement of values.
fn is_a_replacement(call: &Call) -> bool {
    matches!(
        call.entry().id().as_str(),
        "Expr.replace" | "Expr.replace_strict"
    )
}

/// Whether an expression holds, at any depth, a replacement given what it replaces, or what
/// replaces it, that reads a column, and that Polars evaluates. Polars calls a replacement
/// elementwise only where they are literals, which is how one is written, and refuses a join's
/// key that is given a column. Its default may be a column, which it takes a value of for each
/// row. A comparison with a null that Polars folds drops what it follows and is given where
/// that is one value, as it does an aggregate, so a replacement there is never met.
fn holds_a_replacement_of_columns(expression: &Expression) -> bool {
    holds_what_polars_evaluates(expression, &|call| {
        is_a_replacement(call)
            && ["old", "new"].iter().any(|name| {
                matches!(call.argument(name), Some(Value::Expression(value)) if reads_a_column(value))
            })
    })
}
