//! The entries of the catalogue, ordered by identifier so that
//! [`super::entry`] can search them.
//!
//! Each entry names one Python-level Polars item, lists its parameters as
//! Python's signature in `api/polars/python.txt` does, and builds it with the
//! public Rust API the Python binding calls for the same item. A starred
//! parameter (`*exprs`) is a parameter of shape many whose default is `()`,
//! and a double-starred one (`**named_exprs`) is a parameter of kind pairs
//! whose default is `{}`.

use std::sync::Arc;

use polars::chunked_array::cast::CastOptions;
use polars::frame::PivotColumnNaming;
use polars::prelude::{
    AnyValue, ApproxQuantileMethod, ClosedInterval, DataType, DatetimeArgs, DurationArgs, Expr,
    FillNullStrategy, JoinBuildSide, JoinCoalesce, JoinType, JoinValidation, LazyFrame,
    MaintainOrderJoin, NULL, NamedFrom, NonExistent, Operator, PolarsError, QuantileMethod,
    RankMethod, RankOptions, RoundMode, Scalar, SearchSortedSide, Selector, Series,
    SortMultipleOptions, SortOptions, TimeUnit, TimeZone, WindowMapping, all, all_horizontal,
    binary_expr, by_name, col, cols, datetime, duration, element, lit, when,
};
use polars::series::ops::NullBehavior;

use super::{
    Argument, Arguments, AsManyAs, Build, BuildError, Conditional, Dtypes, DurationUse, Entry,
    EntryId, Family, Kind, Laziness, OneValue, Parameter, ParameterDefault, PatternMeans, Role,
    Shape, ValueType, Variadic, Writes,
};

const ANY: &[Dtypes] = &[Dtypes::Any];
const INTEGER: &[Dtypes] = &[Dtypes::Integer];
const BOOLEAN: &[Dtypes] = &[Dtypes::Boolean];
const STRING: &[Dtypes] = &[Dtypes::String];

// What Polars accepts on the receiver, from the functions that run it: `cum_sum` adds
// numbers, durations and (as counts of true) booleans; `cum_max` orders anything with a
// physical number, text and booleans; `Date` and `Time` pass through `dt.date` and
// `dt.time` unchanged.
const CUM_SUM: &[Dtypes] = &[Dtypes::Numeric, Dtypes::Boolean, Dtypes::Duration];
const CUM_MAX: &[Dtypes] = &[
    Dtypes::Numeric,
    Dtypes::Temporal,
    Dtypes::String,
    Dtypes::Boolean,
];
const SUBTRACTABLE: &[Dtypes] = &[Dtypes::Numeric, Dtypes::Temporal];
const DATE_PART: &[Dtypes] = &[Dtypes::Datetime, Dtypes::Date];
const TIME_PART: &[Dtypes] = &[Dtypes::Datetime, Dtypes::Time];
const DURATION: &[Dtypes] = &[Dtypes::Duration];
// What Polars takes a time unit of: a `Datetime` and a `Duration` are held in one, and changed to
// another by `cast_time_unit`.
// What Polars takes a time zone of: a `Datetime`, which it gives one it has none, converts to another
// and, where it has one, reads the offsets of. A `Datetime` with none is refused the offsets.
const ZONED: &[Dtypes] = &[Dtypes::Datetime];
// The time of day `combine` puts on a date or a moment.
const TIME_OF_DAY: &[Dtypes] = &[Dtypes::Time];
// What Polars writes as text with a format: a moment, a date, a time of day and a duration.
const WRITABLE: &[Dtypes] = &[Dtypes::Temporal];
const HAS_A_UNIT: &[Dtypes] = &[Dtypes::Datetime, Dtypes::Duration];
// What Polars counts since the start of 1970: a moment, and a number or a `Boolean`, which it
// reads as the count of microseconds a moment is. A time of day, a duration and text it refuses.
const COUNTED_SINCE_1970: &[Dtypes] = &[
    Dtypes::Numeric,
    Dtypes::Boolean,
    Dtypes::Datetime,
    Dtypes::Date,
];

// What Polars accepts on the receiver of arithmetic and logic, from the functions that run
// them: a coarse group is claimed when Polars accepts it with some dtype of operand, and the
// tables of allowed combinations say which operands, and which members of the group, such as
// the unsigned integers for `neg`, Polars refuses.
//
// `+` adds numbers, joins text, counts booleans, adds durations to each other and to dates and
// datetimes, and has nothing for a `Time`; `*` and `/` take numbers, booleans and durations;
// `//` and `%` take numbers and booleans, and `**` numbers; `neg` takes the signed numbers and
// durations, `abs` numbers and durations.
const ADDABLE: &[Dtypes] = &[
    Dtypes::Numeric,
    Dtypes::String,
    Dtypes::Boolean,
    Dtypes::Datetime,
    Dtypes::Date,
    Dtypes::Duration,
];
const SCALABLE: &[Dtypes] = &[Dtypes::Numeric, Dtypes::Boolean, Dtypes::Duration];
const DIVISIBLE: &[Dtypes] = &[Dtypes::Numeric, Dtypes::Boolean];
const NUMERIC: &[Dtypes] = &[Dtypes::Numeric];
const NUMERIC_OR_DURATION: &[Dtypes] = &[Dtypes::Numeric, Dtypes::Duration];
// `and`, `or`, `xor` and `not` combine and flip the truth of booleans, and the bits of whole
// numbers.
const BITWISE: &[Dtypes] = &[Dtypes::Integer, Dtypes::Boolean];

// What Polars accepts on the receiver of the statistics, from the functions that run them: `mean`
// and `median` take numbers, booleans (the share of the true), and the moments, times of day and
// lengths of time, whose mean is another of the same; `skew` and `kurtosis` convert what they
// follow to a float, so they take the same; `std`, `var`, `product`, `entropy` and `cum_prod` take
// numbers and booleans, and a quantile takes numbers and the temporal dtypes but no boolean. The
// tables of allowed combinations say which members of a group, such as a `Decimal`, Polars refuses.
const MEANABLE: &[Dtypes] = &[Dtypes::Numeric, Dtypes::Boolean, Dtypes::Temporal];
const MOMENTS: &[Dtypes] = &[Dtypes::Numeric, Dtypes::Boolean, Dtypes::Temporal];
const QUANTILE: &[Dtypes] = &[Dtypes::Numeric, Dtypes::Temporal];

// What Polars accepts on the receiver of the nulls, checks and conversions, from the functions
// that run them: `is_nan`, `is_finite` and the like take numbers, as does `fill_nan`, which is
// a condition on `is_not_nan`; `is_close` takes numbers, and a date, a time or a duration, a
// boolean and a decimal beside a number; `is_between` orders what `cum_max` orders. The rest take
// every dtype, and the tables of allowed combinations say which a given argument or option, such
// as the dtype a cast gives or the strategy a fill takes, Polars refuses.
const CLOSE: &[Dtypes] = &[Dtypes::Numeric, Dtypes::Temporal, Dtypes::Boolean];

// What Polars accepts on the receiver of the maths of one value, from the functions that run
// them: roots, logarithms, the exponential and `erf` take numbers and booleans and give a float,
// a `Float32` staying one and every other dtype being a `Float64`; the trigonometric functions
// take numbers and give a float; `sign` and the roundings take numbers and give the dtype they
// are given, and `clip` also takes dates, times and durations. The tables of allowed combinations
// say which members of a group, such as a `Decimal` or the integers for the roundings, Polars
// refuses. Polars resolves a root of any dtype, to a `Float64` where its schema says the dtype it
// was given, which is no resolution: the tables refuse it, and so does the accepts check.
const NUMERIC_OR_BOOLEAN: &[Dtypes] = &[Dtypes::Numeric, Dtypes::Boolean];
const CLIPPABLE: &[Dtypes] = &[Dtypes::Numeric, Dtypes::Temporal];

// What Polars accepts on the receiver of the methods of the `str` namespace, from the functions
// that run them: text, and no other dtype, a `Null` included, except for `join`, which casts what
// it joins to text and refuses a dtype it cannot cast, as a list, an array or a duration. The
// tables of allowed combinations show which, and what each argument may be, for Polars casts
// some of them: a count of characters may be a float or a `Boolean`, and a prefix to test for may
// be a number.
const JOINABLE: &[Dtypes] = &[
    Dtypes::Numeric,
    Dtypes::String,
    Dtypes::Boolean,
    Dtypes::Datetime,
    Dtypes::Date,
    Dtypes::Time,
];

const EXPRESSION: Kind = Kind::Expression(ANY);
/// A new column's name and the expression that gives it: `**named_exprs`.
const NAMED_EXPRESSIONS: Kind = Kind::Pairs(&Kind::Column, &EXPRESSION);

const HOW: &[&str] = &["inner", "left", "right", "full", "semi", "anti", "cross"];
const VALIDATE: &[&str] = &["m:m", "m:1", "1:m", "1:1"];
const MAINTAIN_ORDER: &[&str] = &["none", "left", "right", "left_right", "right_left"];
const BUILD_SIDE: &[&str] = &[
    "auto",
    "prefer_left",
    "prefer_right",
    "force_left",
    "force_right",
];
// The words Python's `TimeUnit` takes: the unit a `Datetime` or a `Duration` counts in.
const TIME_UNIT: &[&str] = &["ns", "us", "ms"];
// The words Python's `epoch` takes for `time_unit`: the three units of a `TimeUnit`, and seconds
// and days, which `epoch` works out of them.
const EPOCH_UNIT: &[&str] = &["ns", "us", "ms", "s", "d"];
// The words Python's `Ambiguous` takes: which of the two times a clock that is read twice, as it is
// where the clocks go back, stands for, and what to do where no word is given.
const AMBIGUOUS: &[&str] = &["earliest", "latest", "raise", "null"];
// The words Python's `replace_time_zone` takes for `non_existent`: what a clock that is never read,
// as where the clocks go forward, gives.
const NON_EXISTENT: &[&str] = &["raise", "null"];
const COLUMN_NAMING: &[&str] = &["auto", "combine"];
const MAPPING_STRATEGY: &[&str] = &["group_to_rows", "join", "explode"];
// The words Python's binding takes for `mode`. Polars' enum also has `to_zero`, which the
// documentation lists and the binding refuses, as `truncate` does it.
const ROUND_MODE: &[&str] = &["half_to_even", "half_away_from_zero"];
// The words Python's `is_between` takes for `closed`, which sides of the interval hold their bound.
const CLOSED: &[&str] = &["both", "left", "right", "none"];
// The words Python's `fill_null` takes for `strategy`, which `parse_fill_null_strategy` in the
// binding reads. `forward` and `backward` are the strategies `limit` bounds.
const FILL_STRATEGY: &[&str] = &["forward", "backward", "min", "max", "mean", "zero", "one"];
// The words Python's `rank` takes for `method`, which the binding reads.
const RANK_METHOD: &[&str] = &["average", "min", "max", "dense", "ordinal", "random"];
// The words Python's `quantile` takes for `interpolation`, which the binding reads, in the order
// Python's `QuantileMethod` lists them.
const QUANTILE_METHOD: &[&str] = &[
    "nearest",
    "higher",
    "lower",
    "midpoint",
    "linear",
    "equiprobable",
];
// The words Python's `approx_quantile` takes for `method`, which Python checks and Polars reads.
const APPROX_QUANTILE_METHOD: &[&str] = &["auto", "kll", "req_lo", "req_hi", "req_both"];
// The words Python's `approx_quantile` takes for `error_tightness`: `formal` is Polars'
// `use_formal_bound`.
const ERROR_TIGHTNESS: &[&str] = &["empirical", "formal"];
// The words Python's `search_sorted` takes for `side`: which place among values that are equal to the one
// looked for, the first, the last or any.
const SEARCH_SIDE: &[&str] = &["any", "left", "right"];
// Python's `diff` takes `ignore` and `drop` for `null_behavior`. `drop` leaves out the values that
// have none before them, as many as `n` says, so it gives as many values as the rows less `n`, a
// count the data decide.
const NULL_BEHAVIOR: &[&str] = &["ignore", "drop"];

/// A method of the `dt` namespace: it takes the dates, times and lengths of time it `accepts` and
/// is swapped for the others of its family.
const fn temporal(
    id: &'static str,
    accepts: &'static [Dtypes],
    parameters: &'static [Parameter],
    build: fn(Expr, &Arguments) -> Result<Expr, BuildError>,
) -> Entry {
    Entry {
        id: EntryId(id),
        family: Family::Temporal,
        accepts,
        parameters,
        laziness: Laziness::Lazy,
        build: Build::Expression(build),
    }
}

const fn required(name: &'static str, kind: Kind, shape: Shape) -> Parameter {
    Parameter::new(name, kind, shape, ParameterDefault::Required)
}

const fn optional(name: &'static str, kind: Kind, shape: Shape, python: &'static str) -> Parameter {
    Parameter::new(name, kind, shape, ParameterDefault::Python(python))
}

/// `*name`: any number of arguments of `kind`.
const fn starred(name: &'static str, kind: Kind) -> Parameter {
    optional(name, kind, Shape::Many, "()").with_variadic(Variadic::Args)
}

/// `**name`: entries of a new column's name and its expression.
const fn double_starred(name: &'static str) -> Parameter {
    optional(name, NAMED_EXPRESSIONS, Shape::One, "{}").with_variadic(Variadic::Kwargs)
}

/// A flag with Python's default text, which may be `None` where Polars decides.
const fn flag(name: &'static str, default: &'static str) -> Parameter {
    optional(name, Kind::Flag, Shape::One, default)
}

/// The parameter every comparison and every method of arithmetic or logic on two values takes.
const OTHER: Parameter = required("other", EXPRESSION, Shape::One).with_role(Role::Operand);

/// The parameter `is_close` takes, which only a number adapts to the one it follows.
const CLOSE_OTHER: Parameter = OTHER.adapting_numbers_only();

/// The parameter `truediv` takes, which is divided by.
const DIVISOR: Parameter = required("other", EXPRESSION, Shape::One).with_role(Role::Divisor);

/// The parameter `floordiv` takes, which is floor divided by.
const FLOOR_DIVISOR: Parameter =
    required("other", EXPRESSION, Shape::One).with_role(Role::FloorDivisor);

/// A bound of `clip`: one of two, of which a step gives at least one, as a clip with neither is
/// the expression as it was.
const fn bound(name: &'static str) -> Parameter {
    optional(name, EXPRESSION, Shape::One, "None")
        .with_role(Role::Bound)
        .with_at_least_one()
}

/// `decimals`: how many places after the point a value is rounded or truncated to, which
/// Python passes on as an unsigned 32-bit number.
const DECIMALS: Parameter = optional("decimals", Kind::Value(ValueType::Integer), Shape::One, "0");

/// `ddof`: the delta degrees of freedom of a variance and a standard deviation, which Python
/// passes on as an unsigned 8-bit number. The values are divided by their count less it.
const DDOF: Parameter = optional("ddof", Kind::Value(ValueType::Integer), Shape::One, "1");

/// `limit`: how many consecutive nulls a forward or backward fill fills, which Python passes on
/// as an unsigned 32-bit number. Unset, it fills them all.
const LIMIT: Parameter = optional("limit", Kind::Value(ValueType::Integer), Shape::One, "None");

/// A count of characters that a method of the `str` namespace takes: the number to keep, or the
/// place to start at. A whole number typed without a dtype is that dtype and adapts to nothing, as
/// the rows `shift` counts are.
const fn characters_counted(name: &'static str) -> Parameter {
    required(name, EXPRESSION, Shape::One).with_role(Role::Count)
}

/// The length a method of the `str` namespace makes text, which it converts to an unsigned whole
/// number.
const fn a_length(name: &'static str) -> Parameter {
    required(name, EXPRESSION, Shape::One).with_role(Role::Length)
}

/// `characters`: what `strip_chars` and its two sides take off the text, which is whitespace where
/// it is not given.
const CHARACTERS: Parameter = optional("characters", EXPRESSION, Shape::One, "None");

/// `fill_char`: the one character that `pad_start` and `pad_end` add, which Python's binding takes
/// as a single character.
const FILL_CHAR: Parameter = optional(
    "fill_char",
    Kind::Value(ValueType::String),
    Shape::One,
    "' '",
);

/// `pattern`: the regular expression a method of text looks for. Polars takes a pattern as an
/// expression, so that a column of them is one for each row; the catalogue holds one typed in,
/// which is what the editor and the designer of patterns work on.
const PATTERN: Parameter = required("pattern", Kind::Pattern, Shape::One);

/// A `pattern` that is plain text where `literal` is true, and so no regular expression.
const PATTERN_OR_TEXT: Parameter = PATTERN.with_plain_text_flag("literal");

/// `literal`: whether the pattern is plain text. Python's default for the methods that search and
/// replace is a regular expression.
const LITERAL: Parameter = flag("literal", "False").keyword_only();

/// `strict`: whether a pattern that is no regular expression is refused. Polars asks it of each
/// row's pattern where they are a column, which a split's separator may be, so it is offered there.
const STRICT: Parameter = flag("strict", "True").keyword_only();

/// `strict` of a method whose pattern is typed in, which is refused when it is none whatever
/// `strict` says, as the editor holds every part to what Polars accepts: it is in Python's
/// signature, and the editor does not offer it.
const TYPED_STRICT: Parameter = STRICT.not_offered();

/// What a replacement puts in place of what the pattern matches. Where the pattern is a regular
/// expression, `$1` and `${name}` stand for the text of a group of the match, and `$$` for a
/// dollar sign. Polars meets a text of one value with each of the many values of it.
const REPLACEMENT: Parameter = required("value", EXPRESSION, Shape::One);

/// A tolerance `is_close` takes: a number from zero, which Python passes on as a float.
const fn tolerance(name: &'static str, default: &'static str) -> Parameter {
    optional(name, Kind::Value(ValueType::Float), Shape::One, default).keyword_only()
}

/// `*predicates`: conditions that must all hold, which `when` and `filter` take.
const PREDICATES: Parameter = starred("predicates", Kind::Expression(BOOLEAN));
/// `**constraints`: columns that must equal a value, which `when` and `filter` take.
const CONSTRAINTS: Parameter = double_starred("constraints");
/// The value a `then` or an `otherwise` gives.
const STATEMENT: Parameter = required("statement", EXPRESSION, Shape::One);

// ---------------------------------------------------------------------------
// Reading arguments

/// Polars refused the arguments the way Python refuses them, with a message.
fn refused(message: &'static str) -> BuildError {
    BuildError::Polars(PolarsError::InvalidOperation(message.into()))
}

/// The flag given for `parameter`, or `None` when it is unset.
fn optional_flag(
    arguments: &Arguments,
    parameter: &'static str,
) -> Result<Option<bool>, BuildError> {
    match arguments.get(parameter) {
        None | Some(Argument::Flag(None)) => Ok(None),
        Some(Argument::Flag(Some(value))) => Ok(Some(*value)),
        Some(other) => Err(Arguments::wrong(parameter, "a flag", other)),
    }
}

/// The flags of a parameter of shape per item: one for each of `count` items.
/// One flag applies to every item, as Python's `extend_bool` does.
fn per_item_flags(
    arguments: &Arguments,
    parameter: &'static str,
    count: usize,
    default: bool,
) -> Result<Vec<bool>, BuildError> {
    let one = |argument: &Argument| match argument {
        Argument::Flag(value) => Ok(value.unwrap_or(default)),
        other => Err(Arguments::wrong(parameter, "a flag", other)),
    };
    match arguments.get(parameter) {
        None => Ok(vec![default; count]),
        Some(Argument::Many(items)) if items.len() == count => items.iter().map(one).collect(),
        Some(Argument::Many(_)) => Err(BuildError::WrongKind {
            parameter,
            expected: "one flag, or one for each item",
            found: "a list of another length",
        }),
        Some(single) => Ok(vec![one(single)?; count]),
    }
}

/// The whole number given for `parameter` as a `u32`, as Python's binding takes it, or
/// `default` when it was not given.
fn unsigned(
    arguments: &Arguments,
    parameter: &'static str,
    default: u32,
) -> Result<u32, BuildError> {
    match arguments.get(parameter) {
        None => Ok(default),
        Some(Argument::Integer(value)) => {
            u32::try_from(*value).map_err(|_| BuildError::OutOfRange {
                parameter,
                expected: "a whole number from 0 to 4294967295",
            })
        }
        Some(other) => Err(Arguments::wrong(parameter, "a whole number", other)),
    }
}

/// The whole number given for `parameter` as a `u32`, as Python's binding takes it, or `None`
/// when it was not given.
fn optional_unsigned(
    arguments: &Arguments,
    parameter: &'static str,
) -> Result<Option<u32>, BuildError> {
    match arguments.get(parameter) {
        None => Ok(None),
        Some(_) => unsigned(arguments, parameter, 0).map(Some),
    }
}

/// The number with a fraction given for `parameter`, or `default` when it was not given. A whole
/// number is the number with a fraction it equals, as Python's `float` takes it.
fn float(arguments: &Arguments, parameter: &'static str, default: f64) -> Result<f64, BuildError> {
    match arguments.get(parameter) {
        None => Ok(default),
        Some(Argument::Float(value)) => Ok(*value),
        #[allow(clippy::cast_precision_loss)]
        Some(Argument::Integer(value)) => Ok(*value as f64),
        Some(other) => Err(Arguments::wrong(parameter, "a number", other)),
    }
}

/// The dtype given for `parameter`, if one was.
fn optional_dtype(
    arguments: &Arguments,
    parameter: &'static str,
) -> Result<Option<DataType>, BuildError> {
    match arguments.get(parameter) {
        None => Ok(None),
        Some(Argument::Dtype(dtype)) => Ok(Some(dtype.clone())),
        Some(other) => Err(Arguments::wrong(parameter, "a dtype", other)),
    }
}

/// The whole number given for the required `parameter` as an `i32`, as Python's binding takes it.
fn signed(arguments: &Arguments, parameter: &'static str) -> Result<i32, BuildError> {
    match arguments.get(parameter) {
        None => Err(BuildError::Missing { parameter }),
        Some(Argument::Integer(value)) => {
            i32::try_from(*value).map_err(|_| BuildError::OutOfRange {
                parameter,
                expected: "a whole number from -2147483648 to 2147483647",
            })
        }
        Some(other) => Err(Arguments::wrong(parameter, "a whole number", other)),
    }
}

/// The whole number given for `parameter`, which Python passes on as it is, or `default` when it
/// was not given.
fn whole(arguments: &Arguments, parameter: &'static str, default: i64) -> Result<i64, BuildError> {
    match arguments.get(parameter) {
        None => Ok(default),
        Some(Argument::Integer(value)) => Ok(*value),
        Some(other) => Err(Arguments::wrong(parameter, "a whole number", other)),
    }
}

/// The index of a group that Python passes on as an unsigned whole number, or `default` when it
/// was not given: a negative number is refused, as Python's binding refuses it.
fn index(
    arguments: &Arguments,
    parameter: &'static str,
    default: usize,
) -> Result<usize, BuildError> {
    match arguments.get(parameter) {
        None => Ok(default),
        Some(Argument::Integer(value)) => {
            usize::try_from(*value).map_err(|_| BuildError::OutOfRange {
                parameter,
                expected: "a whole number from 0",
            })
        }
        Some(other) => Err(Arguments::wrong(parameter, "a whole number", other)),
    }
}

/// The pattern `pattern` of a call that takes `literal`, as a literal: plain text where `literal`
/// is true, and otherwise a regular expression, which Polars is asked is one.
fn pattern_with_literal(arguments: &Arguments) -> Result<(Expr, bool), BuildError> {
    let literal = arguments.flag("literal", false)?;
    let means = if literal {
        PatternMeans::PlainText
    } else {
        PatternMeans::RegularExpression
    };
    Ok((lit(arguments.pattern("pattern", means)?), literal))
}

/// The one character given for `parameter`, as Python's binding takes it, or `default` when it
/// was not given: a text of another length is refused.
fn one_character(
    arguments: &Arguments,
    parameter: &'static str,
    default: char,
) -> Result<char, BuildError> {
    match arguments.get(parameter) {
        None => Ok(default),
        Some(Argument::String(text)) => {
            let mut characters = text.chars();
            match (characters.next(), characters.next()) {
                (Some(character), None) => Ok(character),
                _ => Err(BuildError::OutOfRange {
                    parameter,
                    expected: "one character",
                }),
            }
        }
        Some(other) => Err(Arguments::wrong(parameter, "text", other)),
    }
}

/// The name in one key or value of a pairs argument of kind column.
fn name_of(parameter: &'static str, argument: &Argument) -> Result<String, BuildError> {
    match argument {
        Argument::Columns(names) if names.len() == 1 => Ok(names[0].clone()),
        Argument::Columns(_) => Err(BuildError::WrongKind {
            parameter,
            expected: "one column name",
            found: "column names",
        }),
        other => Err(Arguments::wrong(parameter, "a column name", other)),
    }
}

/// The entries of a pairs argument whose keys are column names and whose
/// values are expressions.
fn named_expressions(
    arguments: &Arguments,
    parameter: &'static str,
) -> Result<Vec<(String, Expr)>, BuildError> {
    arguments
        .pairs(parameter)?
        .iter()
        .map(|(key, value)| match value {
            Argument::Expression(expr) => Ok((name_of(parameter, key)?, expr.clone())),
            other => Err(Arguments::wrong(parameter, "an expression", other)),
        })
        .collect()
}

/// The expressions of `*exprs` followed by those of `**named_exprs`, each of
/// the latter named for its key.
fn positional_and_named(arguments: &Arguments) -> Result<Vec<Expr>, BuildError> {
    let mut exprs = arguments.expressions("exprs")?;
    for (name, expr) in named_expressions(arguments, "named_exprs")? {
        exprs.push(expr.alias(name));
    }
    Ok(exprs)
}

/// The expressions the step `entry` writes as columns, named as they are
/// written. See [`Entry::writes`].
pub(super) fn writes(entry: &Entry, arguments: &Arguments) -> Result<Vec<Expr>, BuildError> {
    let mut written = Vec::new();
    for parameter in entry
        .parameters()
        .iter()
        .filter(|parameter| parameter.writes() == Writes::Columns)
    {
        match parameter.kind() {
            Kind::Expression(_) => written.extend(arguments.expressions(parameter.name())?),
            Kind::Pairs(..) => written.extend(
                named_expressions(arguments, parameter.name())?
                    .into_iter()
                    .map(|(name, expr)| expr.alias(name)),
            ),
            // No other kind holds an expression that is written.
            _ => {}
        }
    }
    // A bare column reference, or a selection of columns by name, gives them back as they are.
    written.retain(|expr| !matches!(expr, Expr::Column(_) | Expr::Selector(_)));
    Ok(written)
}

/// The conditions of `*predicates` followed by the equalities of
/// `**constraints`, each a column that must equal its value.
fn predicates_and_constraints(arguments: &Arguments) -> Result<Vec<Expr>, BuildError> {
    let mut predicates = arguments.expressions("predicates")?;
    for (name, value) in named_expressions(arguments, "constraints")? {
        predicates.push(col(name).eq(value));
    }
    Ok(predicates)
}

/// The column names given for a required parameter.
fn required_columns(
    arguments: &Arguments,
    parameter: &'static str,
) -> Result<Vec<String>, BuildError> {
    match arguments.get(parameter) {
        None => Err(BuildError::Missing { parameter }),
        Some(_) => arguments.columns(parameter),
    }
}

/// The keys of a join, as given for `parameter`, if any were.
fn join_keys(
    arguments: &Arguments,
    parameter: &'static str,
) -> Result<Option<Vec<Expr>>, BuildError> {
    arguments
        .get(parameter)
        .map(|_| arguments.expressions(parameter))
        .transpose()
}

// ---------------------------------------------------------------------------
// Expression methods

fn shift(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    let n = arguments.expression("n")?.unwrap_or_else(|| lit(1));
    Ok(match arguments.expression("fill_value")? {
        Some(fill_value) => expr.shift_and_fill(n, fill_value),
        None => expr.shift(n),
    })
}

fn sort_values(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.sort(SortOptions {
        descending: arguments.flag("descending", false)?,
        nulls_last: arguments.flag("nulls_last", false)?,
        multithreaded: true,
        maintain_order: false,
        limit: None,
    }))
}

fn arg_sort(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.arg_sort(
        arguments.flag("descending", false)?,
        arguments.flag("nulls_last", false)?,
    ))
}

fn sort_by(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    if arguments.get("by").is_none() {
        return Err(BuildError::Missing { parameter: "by" });
    }
    let mut by = arguments.expressions("by")?;
    by.extend(arguments.expressions("more_by")?);
    let options = SortMultipleOptions {
        descending: per_item_flags(arguments, "descending", by.len(), false)?,
        nulls_last: per_item_flags(arguments, "nulls_last", by.len(), false)?,
        maintain_order: arguments.flag("maintain_order", false)?,
        multithreaded: arguments.flag("multithreaded", true)?,
        limit: None,
    };
    Ok(expr.sort_by(by, options))
}

fn diff(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    let n = arguments.expression("n")?.unwrap_or_else(|| lit(1));
    let null_behavior = match arguments.choice("null_behavior", NULL_BEHAVIOR, "ignore")? {
        "drop" => NullBehavior::Drop,
        _ => NullBehavior::Ignore,
    };
    Ok(expr.diff(n, null_behavior))
}

fn pct_change(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    let n = arguments.expression("n")?.unwrap_or_else(|| lit(1));
    Ok(expr.pct_change(n))
}

fn rank(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    let method = match arguments.choice("method", RANK_METHOD, "average")? {
        "min" => RankMethod::Min,
        "max" => RankMethod::Max,
        "dense" => RankMethod::Dense,
        "ordinal" => RankMethod::Ordinal,
        "random" => RankMethod::Random,
        _ => RankMethod::Average,
    };
    let seed = match arguments.get("seed") {
        None | Some(Argument::Flag(None)) => None,
        Some(Argument::Integer(value)) => {
            Some(u64::try_from(*value).map_err(|_| BuildError::OutOfRange {
                parameter: "seed",
                expected: "a whole number from 0",
            })?)
        }
        Some(other) => return Err(Arguments::wrong("seed", "a whole number", other)),
    };
    let options = RankOptions {
        method,
        descending: arguments.flag("descending", false)?,
    };
    Ok(expr.rank(options, seed))
}

/// The count of values a method that cuts a column takes, as Python passes it on: an expression
/// that is a number typed in or another one, and 10 where it is not given.
fn cut_by(
    arguments: &Arguments,
    parameter: &'static str,
    default: i32,
) -> Result<Expr, BuildError> {
    Ok(arguments
        .expression(parameter)?
        .unwrap_or_else(|| lit(default)))
}

fn head(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.slice(lit(0), cut_by(arguments, "n", 10)?))
}

/// Python takes `tail` of an expression as a slice that starts as many values before the end as
/// the count is, which it makes an unsigned count signed to negate.
fn tail(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    let n = cut_by(arguments, "n", 10)?;
    let offset = -n
        .clone()
        .cast_with_options(DataType::Int64, CastOptions::Overflowing);
    Ok(expr.slice(offset, n))
}

fn slice(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    let Some(offset) = arguments.expression("offset")? else {
        return Err(BuildError::Missing {
            parameter: "offset",
        });
    };
    let length = arguments.expression("length")?.unwrap_or_else(|| lit(NULL));
    Ok(expr.slice(offset, length))
}

fn top_k(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.top_k(cut_by(arguments, "k", 5)?))
}

fn bottom_k(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.bottom_k(cut_by(arguments, "k", 5)?))
}

fn gather_every(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    if arguments.get("n").is_none() {
        return Err(BuildError::Missing { parameter: "n" });
    }
    Ok(expr.gather_every(index(arguments, "n", 0)?, index(arguments, "offset", 0)?))
}

fn gather(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    let Some(indices) = arguments.expression("indices")? else {
        return Err(BuildError::Missing {
            parameter: "indices",
        });
    };
    Ok(expr.gather(indices, arguments.flag("null_on_oob", false)?))
}

fn arg_unique(expr: Expr, _arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.arg_unique())
}

fn unique(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(if arguments.flag("maintain_order", false)? {
        expr.unique_stable()
    } else {
        expr.unique()
    })
}

fn mode(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.mode(arguments.flag("maintain_order", false)?))
}

fn unique_counts(expr: Expr, _arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.unique_counts())
}

/// Python makes one predicate of the several it is given, which hold all together, and of each
/// constraint a comparison of the column with the value.
fn expr_filter(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    let mut predicates = predicates_and_constraints(arguments)?;
    match predicates.len() {
        0 => Err(BuildError::Missing {
            parameter: "predicates",
        }),
        1 => Ok(expr.filter(predicates.remove(0))),
        _ => Ok(expr.filter(all_horizontal(&predicates)?)),
    }
}

fn is_in(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    let Some(other) = arguments.expression("other")? else {
        return Err(BuildError::Missing { parameter: "other" });
    };
    Ok(expr.is_in(other, arguments.flag("nulls_equal", false)?))
}

fn search_sorted(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    let Some(element) = arguments.expression("element")? else {
        return Err(BuildError::Missing {
            parameter: "element",
        });
    };
    let side = match arguments.choice("side", SEARCH_SIDE, "any")? {
        "left" => SearchSortedSide::Left,
        "right" => SearchSortedSide::Right,
        _ => SearchSortedSide::Any,
    };
    Ok(expr.search_sorted(element, side, arguments.flag("descending", false)?))
}

fn sum(expr: Expr, _arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.sum())
}

fn min(expr: Expr, _arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.min())
}

fn max(expr: Expr, _arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.max())
}

fn len(expr: Expr, _arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.len())
}

fn first(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(if arguments.flag("ignore_nulls", false)? {
        expr.first_non_null()
    } else {
        expr.first()
    })
}

fn last(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(if arguments.flag("ignore_nulls", false)? {
        expr.last_non_null()
    } else {
        expr.last()
    })
}

fn cum_sum(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.cum_sum(arguments.flag("reverse", false)?))
}

fn cum_count(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.cum_count(arguments.flag("reverse", false)?))
}

fn cum_max(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.cum_max(arguments.flag("reverse", false)?))
}

fn cum_min(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.cum_min(arguments.flag("reverse", false)?))
}

fn cum_prod(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.cum_prod(arguments.flag("reverse", false)?))
}

/// The delta degrees of freedom, which Python's binding takes as an unsigned 8-bit number.
fn ddof(arguments: &Arguments) -> Result<u8, BuildError> {
    match arguments.get("ddof") {
        None => Ok(1),
        Some(Argument::Integer(value)) => {
            u8::try_from(*value).map_err(|_| BuildError::OutOfRange {
                parameter: "ddof",
                expected: "a whole number from 0 to 255",
            })
        }
        Some(other) => Err(Arguments::wrong("ddof", "a whole number", other)),
    }
}

fn std_deviation(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.std(ddof(arguments)?))
}

fn variance(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.var(ddof(arguments)?))
}

fn quantile(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    let quantile = arguments.required_expression("quantile")?;
    let method = match arguments.choice("interpolation", QUANTILE_METHOD, "nearest")? {
        "higher" => QuantileMethod::Higher,
        "lower" => QuantileMethod::Lower,
        "midpoint" => QuantileMethod::Midpoint,
        "linear" => QuantileMethod::Linear,
        "equiprobable" => QuantileMethod::Equiprobable,
        _ => QuantileMethod::Nearest,
    };
    Ok(expr.quantile(quantile, method))
}

fn approx_quantile(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    let quantile = arguments.required_expression("quantile")?;
    let method = match arguments.choice("method", APPROX_QUANTILE_METHOD, "auto")? {
        "kll" => ApproxQuantileMethod::KLL,
        "req_lo" => ApproxQuantileMethod::ReqSketch { hra: false },
        "req_hi" => ApproxQuantileMethod::ReqSketch { hra: true },
        "req_both" => ApproxQuantileMethod::DoubleReqSketch,
        _ => ApproxQuantileMethod::Auto,
    };
    let formal = arguments.choice("error_tightness", ERROR_TIGHTNESS, "empirical")? == "formal";
    let error = float(arguments, "error", 0.001)?;
    Ok(expr.approx_quantile(quantile, error, formal, method))
}

fn skew(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.skew(arguments.flag("bias", true)?))
}

fn kurtosis(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.kurtosis(
        arguments.flag("fisher", true)?,
        arguments.flag("bias", true)?,
    ))
}

fn entropy(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.entropy(
        float(arguments, "base", std::f64::consts::E)?,
        arguments.flag("normalize", true)?,
    ))
}

fn all_values(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.all(arguments.flag("ignore_nulls", true)?))
}

fn any_value(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.any(arguments.flag("ignore_nulls", true)?))
}

fn add(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    let other = arguments.required_expression("other")?;
    Ok(binary_expr(expr, Operator::Plus, other))
}

fn sub(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    let other = arguments.required_expression("other")?;
    Ok(binary_expr(expr, Operator::Minus, other))
}

fn mul(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    let other = arguments.required_expression("other")?;
    Ok(binary_expr(expr, Operator::Multiply, other))
}

fn truediv(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    let other = arguments.required_expression("other")?;
    Ok(binary_expr(expr, Operator::TrueDivide, other))
}

fn floordiv(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    let other = arguments.required_expression("other")?;
    Ok(binary_expr(expr, Operator::FloorDivide, other))
}

fn modulo(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    let other = arguments.required_expression("other")?;
    Ok(binary_expr(expr, Operator::Modulus, other))
}

fn pow(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.pow(arguments.required_expression("exponent")?))
}

fn neg(expr: Expr, _arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(-expr)
}

fn abs(expr: Expr, _arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.abs())
}

/// As Python folds `self & *others`: each operand meets what the ones before it gave, and
/// with none the expression is as it was.
fn and_(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(arguments
        .expressions("others")?
        .into_iter()
        .fold(expr, Expr::and))
}

/// As Python folds `self | *others`.
fn or_(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(arguments
        .expressions("others")?
        .into_iter()
        .fold(expr, Expr::or))
}

fn xor(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.xor(arguments.required_expression("other")?))
}

fn not_(expr: Expr, _arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.not())
}

/// Each function applies the method of the same name that Polars' `Expr` has, as the binding
/// does, to the expression it follows: they take nothing else.
macro_rules! unary {
    ($($name:ident),+ $(,)?) => {
        $(
            fn $name(expr: Expr, _arguments: &Arguments) -> Result<Expr, BuildError> {
                Ok(expr.$name())
            }
        )+
    };
}

unary!(
    sqrt,
    cbrt,
    exp,
    log1p,
    sign,
    ceil,
    floor,
    sin,
    cos,
    tan,
    cot,
    arcsin,
    arccos,
    arctan,
    sinh,
    cosh,
    tanh,
    arcsinh,
    arccosh,
    arctanh,
    degrees,
    radians,
    erf,
    erfc,
    drop_nulls,
    drop_nans,
    is_nan,
    is_not_nan,
    is_finite,
    is_infinite,
    is_duplicated,
    is_unique,
    is_first_distinct,
    is_last_distinct,
    has_nulls,
    null_count,
    reverse,
    rle_id,
    n_unique,
    count,
    mean,
    median,
    product,
    arg_max,
    arg_min,
    nan_max,
    nan_min,
    approx_n_unique,
    bitwise_and,
    bitwise_or,
    bitwise_xor,
    bitwise_count_ones,
    bitwise_count_zeros,
    bitwise_leading_ones,
    bitwise_leading_zeros,
    bitwise_trailing_ones,
    bitwise_trailing_zeros,
);

/// The methods of the `str` namespace that take nothing, each the method of the same name that
/// Polars' `StringNameSpace` has.
macro_rules! unary_str {
    ($($name:ident => $method:ident),+ $(,)?) => {
        $(
            fn $name(expr: Expr, _arguments: &Arguments) -> Result<Expr, BuildError> {
                Ok(expr.str().$method())
            }
        )+
    };
}

unary_str!(
    str_to_lowercase => to_lowercase,
    str_to_uppercase => to_uppercase,
    str_to_titlecase => to_titlecase,
    str_len_chars => len_chars,
    str_len_bytes => len_bytes,
    str_reverse => reverse,
);

/// The base defaults to `math.e`, which Python makes a number of no dtype, as it does a base
/// given. Polars leaves the logarithm of a whole number to such a base an unresolved float, which
/// a join's key or a sort key does not resolve, so the default is a `Float64`, which gives the
/// logarithm of a whole number the `Float64` every other place does.
fn log(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    let base = arguments.expression("base")?.unwrap_or_else(|| {
        lit(Scalar::new(
            DataType::Float64,
            AnyValue::Float64(std::f64::consts::E),
        ))
    });
    Ok(expr.log(base))
}

fn round(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    let mode = match arguments.choice("mode", ROUND_MODE, "half_to_even")? {
        "half_away_from_zero" => RoundMode::HalfAwayFromZero,
        _ => RoundMode::HalfToEven,
    };
    Ok(expr.round(unsigned(arguments, "decimals", 0)?, mode))
}

fn round_sig_figs(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.round_sig_figs(signed(arguments, "digits")?))
}

fn truncate(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.truncate(unsigned(arguments, "decimals", 0)?))
}

/// As the binding does: one bound clips on that side, both clip on both, and neither leaves the
/// expression as it was.
fn clip(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(
        match (
            arguments.expression("lower_bound")?,
            arguments.expression("upper_bound")?,
        ) {
            (Some(lower), Some(upper)) => expr.clip(lower, upper),
            (Some(lower), None) => expr.clip_min(lower),
            (None, Some(upper)) => expr.clip_max(upper),
            (None, None) => expr,
        },
    )
}

/// As Python's `fill_null` takes its arguments: a value or a strategy, and not both, and a limit
/// only with the strategy `forward` or `backward`; each refusal is Python's own.
fn fill_null(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    let value = arguments.expression("value")?;
    let strategy = match arguments.get("strategy") {
        None => None,
        Some(_) => Some(arguments.choice("strategy", FILL_STRATEGY, "forward")?),
    };
    let limit = optional_unsigned(arguments, "limit")?;
    if value.is_some() && strategy.is_some() {
        return Err(refused("cannot specify both `value` and `strategy`"));
    }
    if value.is_none() && strategy.is_none() {
        return Err(refused("must specify either a fill `value` or `strategy`"));
    }
    if !matches!(strategy, Some("forward" | "backward")) && limit.is_some() {
        return Err(refused(
            "can only specify `limit` when strategy is set to 'backward' or 'forward'",
        ));
    }
    Ok(match (value, strategy) {
        (Some(value), _) => expr.fill_null(value),
        (None, Some(strategy)) => expr.fill_null_with_strategy(match strategy {
            "forward" => FillNullStrategy::Forward(limit),
            "backward" => FillNullStrategy::Backward(limit),
            "min" => FillNullStrategy::Min,
            "max" => FillNullStrategy::Max,
            "mean" => FillNullStrategy::Mean,
            "zero" => FillNullStrategy::Zero,
            _ => FillNullStrategy::One,
        }),
        (None, None) => expr,
    })
}

fn forward_fill(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(
        expr.fill_null_with_strategy(FillNullStrategy::Forward(optional_unsigned(
            arguments, "limit",
        )?)),
    )
}

fn backward_fill(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(
        expr.fill_null_with_strategy(FillNullStrategy::Backward(optional_unsigned(
            arguments, "limit",
        )?)),
    )
}

fn fill_nan(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.fill_nan(arguments.required_expression("value")?))
}

/// As the binding does: `wrap_numerical` wraps what overflows, and otherwise `strict` says
/// whether a value that cannot be converted is refused or becomes a null.
fn cast(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    let dtype =
        optional_dtype(arguments, "dtype")?.ok_or(BuildError::Missing { parameter: "dtype" })?;
    let options = if arguments.flag("wrap_numerical", false)? {
        CastOptions::Overflowing
    } else if arguments.flag("strict", true)? {
        CastOptions::Strict
    } else {
        CastOptions::NonStrict
    };
    Ok(expr.cast_with_options(dtype, options))
}

/// As the binding does, from the expressions the step gives: Python's mapping and its sequences
/// are sugar for two expressions, which the catalogue holds as they are.
fn replace(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.replace(
        arguments.required_expression("old")?,
        arguments.required_expression("new")?,
    ))
}

fn replace_strict(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.replace_strict(
        arguments.required_expression("old")?,
        arguments.required_expression("new")?,
        arguments.expression("default")?,
        optional_dtype(arguments, "return_dtype")?,
    ))
}

fn is_between(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    let closed = match arguments.choice("closed", CLOSED, "both")? {
        "left" => ClosedInterval::Left,
        "right" => ClosedInterval::Right,
        "none" => ClosedInterval::None,
        _ => ClosedInterval::Both,
    };
    Ok(expr.is_between(
        arguments.required_expression("lower_bound")?,
        arguments.required_expression("upper_bound")?,
        closed,
    ))
}

fn is_close(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.is_close(
        arguments.required_expression("other")?,
        float(arguments, "abs_tol", 0.0)?,
        float(arguments, "rel_tol", 1e-9)?,
        arguments.flag("nans_equal", false)?,
    ))
}

fn eq_missing(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.eq_missing(arguments.required_expression("other")?))
}

fn ne_missing(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.neq_missing(arguments.required_expression("other")?))
}

/// The characters a trim takes off, which Python makes the null literal where none are given, and
/// then Polars trims whitespace.
fn characters(arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(arguments
        .expression("characters")?
        .unwrap_or_else(|| lit(NULL)))
}

fn str_strip_chars(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.str().strip_chars(characters(arguments)?))
}

fn str_strip_chars_start(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.str().strip_chars_start(characters(arguments)?))
}

fn str_strip_chars_end(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.str().strip_chars_end(characters(arguments)?))
}

fn str_strip_prefix(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr
        .str()
        .strip_prefix(arguments.required_expression("prefix")?))
}

fn str_strip_suffix(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr
        .str()
        .strip_suffix(arguments.required_expression("suffix")?))
}

fn str_starts_with(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr
        .str()
        .starts_with(arguments.required_expression("prefix")?))
}

fn str_ends_with(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr
        .str()
        .ends_with(arguments.required_expression("suffix")?))
}

/// As Python's `slice` takes its arguments: a length that is not given is the null literal, which
/// is the rest of the text.
fn str_slice(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.str().slice(
        arguments.required_expression("offset")?,
        arguments.expression("length")?.unwrap_or_else(|| lit(NULL)),
    ))
}

fn str_head(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.str().head(arguments.required_expression("n")?))
}

fn str_tail(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.str().tail(arguments.required_expression("n")?))
}

fn str_pad_start(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.str().pad_start(
        arguments.required_expression("length")?,
        one_character(arguments, "fill_char", ' ')?,
    ))
}

fn str_pad_end(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.str().pad_end(
        arguments.required_expression("length")?,
        one_character(arguments, "fill_char", ' ')?,
    ))
}

fn str_zfill(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.str().zfill(arguments.required_expression("length")?))
}

/// As Python's `split` takes its arguments: by plain text where `literal` is true, and otherwise by
/// a pattern, which Polars splits at each match. `strict` is for a pattern: it decides whether a
/// pattern that is no regex is refused when rows run or gives a null, so beside plain text it
/// changes nothing.
fn str_split(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    let by = arguments.required_expression("by")?;
    let inclusive = arguments.flag("inclusive", false)?;
    Ok(if arguments.flag("literal", true)? {
        if inclusive {
            expr.str().split_inclusive(by)
        } else {
            expr.str().split(by)
        }
    } else {
        let strict = arguments.flag("strict", true)?;
        if inclusive {
            expr.str().split_regex_inclusive(by, strict)
        } else {
            expr.str().split_regex(by, strict)
        }
    })
}

/// One text, as Python's `join` gives it: every value of the column joined by the delimiter, a
/// null being skipped unless it is asked to make the whole a null.
fn str_join(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    let delimiter = arguments.string("delimiter")?.unwrap_or_default();
    Ok(expr
        .str()
        .join(&delimiter, arguments.flag("ignore_nulls", true)?))
}

fn str_contains(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    let (pattern, literal) = pattern_with_literal(arguments)?;
    // `strict` is for a pattern, as `literal` true takes plain text that is never refused.
    Ok(if literal {
        expr.str().contains_literal(pattern)
    } else {
        expr.str()
            .contains(pattern, arguments.flag("strict", true)?)
    })
}

fn str_count_matches(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    let (pattern, literal) = pattern_with_literal(arguments)?;
    Ok(expr.str().count_matches(pattern, literal))
}

fn str_find(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    let (pattern, literal) = pattern_with_literal(arguments)?;
    Ok(if literal {
        expr.str().find_literal(pattern)
    } else {
        expr.str().find(pattern, arguments.flag("strict", true)?)
    })
}

/// As Python's `replace` takes its arguments: the first `n` matches are replaced, and all of them
/// where `n` is below zero.
fn str_replace(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    let (pattern, literal) = pattern_with_literal(arguments)?;
    Ok(expr.str().replace_n(
        pattern,
        arguments.required_expression("value")?,
        literal,
        whole(arguments, "n", 1)?,
    ))
}

fn str_replace_all(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    let (pattern, literal) = pattern_with_literal(arguments)?;
    Ok(expr
        .str()
        .replace_all(pattern, arguments.required_expression("value")?, literal))
}

fn str_extract(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.str().extract(
        lit(arguments.pattern("pattern", PatternMeans::RegularExpression)?),
        index(arguments, "group_index", 1)?,
    ))
}

fn str_extract_all(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.str().extract_all(lit(
        arguments.pattern("pattern", PatternMeans::RegularExpression)?
    )))
}

fn str_extract_groups(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    let pattern = arguments.pattern("pattern", PatternMeans::RegularExpression)?;
    Ok(expr.str().extract_groups(&pattern)?)
}

fn str_escape_regex(expr: Expr, _arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.str().escape_regex())
}

fn dt_date(expr: Expr, _: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.dt().date())
}

fn dt_time(expr: Expr, _: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.dt().time())
}

fn dt_offset_by(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.dt().offset_by(arguments.required_expression("by")?))
}

fn dt_truncate(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.dt().truncate(arguments.required_expression("every")?))
}

fn dt_round(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.dt().round(arguments.required_expression("every")?))
}

fn dt_month_start(expr: Expr, _: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.dt().month_start())
}

fn dt_total_days(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.dt().total_days(arguments.flag("fractional", false)?))
}

fn dt_century(expr: Expr, _: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.dt().century())
}

fn dt_day(expr: Expr, _: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.dt().day())
}

fn dt_days_in_month(expr: Expr, _: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.dt().days_in_month())
}

fn dt_iso_year(expr: Expr, _: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.dt().iso_year())
}

fn dt_millennium(expr: Expr, _: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.dt().millennium())
}

fn dt_month(expr: Expr, _: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.dt().month())
}

fn dt_month_end(expr: Expr, _: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.dt().month_end())
}

fn dt_ordinal_day(expr: Expr, _: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.dt().ordinal_day())
}

fn dt_quarter(expr: Expr, _: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.dt().quarter())
}

fn dt_week(expr: Expr, _: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.dt().week())
}

fn dt_weekday(expr: Expr, _: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.dt().weekday())
}

fn dt_year(expr: Expr, _: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.dt().year())
}

fn dt_is_leap_year(expr: Expr, _: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.dt().is_leap_year())
}

fn dt_hour(expr: Expr, _: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.dt().hour())
}

fn dt_minute(expr: Expr, _: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.dt().minute())
}

fn dt_millisecond(expr: Expr, _: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.dt().millisecond())
}

fn dt_microsecond(expr: Expr, _: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.dt().microsecond())
}

fn dt_nanosecond(expr: Expr, _: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.dt().nanosecond())
}

/// The second of the minute, with the fraction of it as Python writes it: the whole second and
/// the nanoseconds of it over a billion.
fn dt_second(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(if arguments.flag("fractional", false)? {
        expr.clone().dt().second() + expr.dt().nanosecond() / lit(1_000_000_000.0)
    } else {
        expr.dt().second()
    })
}

fn dt_total_hours(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.dt().total_hours(arguments.flag("fractional", false)?))
}

fn dt_total_minutes(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr
        .dt()
        .total_minutes(arguments.flag("fractional", false)?))
}

fn dt_total_seconds(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr
        .dt()
        .total_seconds(arguments.flag("fractional", false)?))
}

fn dt_total_milliseconds(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr
        .dt()
        .total_milliseconds(arguments.flag("fractional", false)?))
}

fn dt_total_microseconds(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr
        .dt()
        .total_microseconds(arguments.flag("fractional", false)?))
}

fn dt_total_nanoseconds(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr
        .dt()
        .total_nanoseconds(arguments.flag("fractional", false)?))
}

/// The time unit given for `parameter`, which is one of `words`.
fn time_unit(
    arguments: &Arguments,
    parameter: &'static str,
    words: &[&'static str],
    default: Option<&'static str>,
) -> Result<TimeUnit, BuildError> {
    let word = match (arguments.get(parameter), default) {
        (None, None) => return Err(BuildError::Missing { parameter }),
        (_, default) => arguments.choice(parameter, words, default.unwrap_or(words[0]))?,
    };
    Ok(match word {
        "ns" => TimeUnit::Nanoseconds,
        "ms" => TimeUnit::Milliseconds,
        _ => TimeUnit::Microseconds,
    })
}

fn dt_base_utc_offset(expr: Expr, _: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.dt().base_utc_offset())
}

fn dt_dst_offset(expr: Expr, _: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.dt().dst_offset())
}

/// A moment in the zone, which keeps its instant and changes the clock it is read by. Python takes
/// a zone only, so none is refused.
fn dt_convert_time_zone(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    let zone = arguments
        .time_zone("time_zone", false)?
        .unwrap_or(TimeZone::UTC);
    Ok(expr.dt().convert_time_zone(zone))
}

/// A moment in the zone that keeps its clock and changes its instant, or in none where the zone is
/// none. A clock that is read twice or never is resolved by the words given.
fn dt_replace_time_zone(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    let zone = arguments.time_zone("time_zone", true)?;
    let ambiguous = arguments.choice("ambiguous", AMBIGUOUS, "raise")?;
    let non_existent = match arguments.choice("non_existent", NON_EXISTENT, "raise")? {
        "null" => NonExistent::Null,
        _ => NonExistent::Raise,
    };
    Ok(expr
        .dt()
        .replace_time_zone(zone, lit(ambiguous), non_existent))
}

fn dt_combine(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.dt().combine(
        arguments.required_expression("time")?,
        time_unit(arguments, "time_unit", TIME_UNIT, Some("us"))?,
    ))
}

/// A part of a moment that is left as it is where it is not given: Python gives each a null.
fn dt_replace(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    let part = |name: &'static str| -> Result<Expr, BuildError> {
        Ok(arguments.expression(name)?.unwrap_or_else(|| lit(NULL)))
    };
    let ambiguous = arguments.choice("ambiguous", AMBIGUOUS, "raise")?;
    Ok(expr.dt().replace(
        part("year")?,
        part("month")?,
        part("day")?,
        part("hour")?,
        part("minute")?,
        part("second")?,
        part("microsecond")?,
        lit(ambiguous),
    ))
}

fn dt_strftime(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    let format = arguments.string("format")?.ok_or(BuildError::Missing {
        parameter: "format",
    })?;
    Ok(expr.dt().strftime(&format))
}

/// A moment as text, in ISO form where Python's `to_string` is given no format.
fn dt_to_string(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    let format = arguments
        .string("format")?
        .unwrap_or_else(|| "iso".to_owned());
    Ok(expr.dt().to_string(&format))
}

fn dt_cast_time_unit(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr
        .dt()
        .cast_time_unit(time_unit(arguments, "time_unit", TIME_UNIT, None)?))
}

fn dt_timestamp(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr
        .dt()
        .timestamp(time_unit(arguments, "time_unit", TIME_UNIT, Some("us"))?))
}

/// The time since the start of 1970 in the unit asked for, as Python works it out: a time stamp
/// in the three units of a time unit, a whole number of seconds from one in milliseconds, and a
/// whole number of days from the date.
fn dt_epoch(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(match arguments.choice("time_unit", EPOCH_UNIT, "us")? {
        "s" => expr
            .dt()
            .timestamp(TimeUnit::Milliseconds)
            .floor_div(lit(1000_i64)),
        "d" => expr.cast(DataType::Date).cast(DataType::Int32),
        _ => expr
            .dt()
            .timestamp(time_unit(arguments, "time_unit", TIME_UNIT, Some("us"))?),
    })
}

fn over(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    // As in Python, `*more_exprs` extends `partition_by` and means nothing without it.
    let partition_by = match arguments.get("partition_by") {
        None => None,
        Some(_) => {
            let mut partition_by = arguments.expressions("partition_by")?;
            partition_by.extend(arguments.expressions("more_exprs")?);
            Some(partition_by)
        }
    };
    let order_by = match arguments.get("order_by") {
        None => None,
        Some(_) => Some((
            arguments.expressions("order_by")?,
            SortOptions {
                descending: arguments.flag("descending", false)?,
                nulls_last: arguments.flag("nulls_last", false)?,
                maintain_order: true,
                ..SortOptions::default()
            },
        )),
    };
    let mapping = match arguments.choice("mapping_strategy", MAPPING_STRATEGY, "group_to_rows")? {
        "join" => WindowMapping::Join,
        "explode" => WindowMapping::Explode,
        _ => WindowMapping::GroupsToRows,
    };
    Ok(expr.over_with_options(partition_by, order_by, mapping)?)
}

fn eq(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.eq(arguments.required_expression("other")?))
}

fn ne(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.neq(arguments.required_expression("other")?))
}

fn gt(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.gt(arguments.required_expression("other")?))
}

fn ge(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.gt_eq(arguments.required_expression("other")?))
}

fn lt(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.lt(arguments.required_expression("other")?))
}

fn le(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.lt_eq(arguments.required_expression("other")?))
}

fn is_null(expr: Expr, _: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.is_null())
}

fn is_not_null(expr: Expr, _: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.is_not_null())
}

// ---------------------------------------------------------------------------
// Conditions

/// The one condition `*predicates` and `**constraints` make, which holds where
/// every one of them does, as Python's `when` takes them.
fn condition(arguments: &Arguments) -> Result<Expr, BuildError> {
    let mut predicates = predicates_and_constraints(arguments)?;
    match predicates.len() {
        0 => Err(refused(
            "at least one predicate or constraint must be provided",
        )),
        1 => Ok(predicates.remove(0)),
        _ => Ok(all_horizontal(&predicates)?),
    }
}

fn polars_when(arguments: &Arguments) -> Result<Conditional, BuildError> {
    Ok(Conditional::When(when(condition(arguments)?)))
}

fn when_then(after: Conditional, arguments: &Arguments) -> Result<Conditional, BuildError> {
    match after {
        Conditional::When(when) => Ok(Conditional::Then(
            when.then(arguments.required_expression("statement")?),
        )),
        other => Err(other.out_of_order("when")),
    }
}

fn then_when(after: Conditional, arguments: &Arguments) -> Result<Conditional, BuildError> {
    match after {
        Conditional::Then(then) => Ok(Conditional::ChainedWhen(then.when(condition(arguments)?))),
        other => Err(other.out_of_order("then")),
    }
}

fn then_otherwise(after: Conditional, arguments: &Arguments) -> Result<Expr, BuildError> {
    match after {
        Conditional::Then(then) => Ok(then.otherwise(arguments.required_expression("statement")?)),
        other => Err(other.out_of_order("then")),
    }
}

fn chained_when_then(after: Conditional, arguments: &Arguments) -> Result<Conditional, BuildError> {
    match after {
        Conditional::ChainedWhen(when) => Ok(Conditional::ChainedThen(
            when.then(arguments.required_expression("statement")?),
        )),
        other => Err(other.out_of_order("chained when")),
    }
}

fn chained_then_when(after: Conditional, arguments: &Arguments) -> Result<Conditional, BuildError> {
    match after {
        Conditional::ChainedThen(then) => {
            Ok(Conditional::ChainedWhen(then.when(condition(arguments)?)))
        }
        other => Err(other.out_of_order("chained then")),
    }
}

fn chained_then_otherwise(after: Conditional, arguments: &Arguments) -> Result<Expr, BuildError> {
    match after {
        Conditional::ChainedThen(then) => {
            Ok(then.otherwise(arguments.required_expression("statement")?))
        }
        other => Err(other.out_of_order("chained then")),
    }
}

// ---------------------------------------------------------------------------
// Frame steps

fn filter(frame: LazyFrame, arguments: &Arguments) -> Result<LazyFrame, BuildError> {
    let mut predicates = predicates_and_constraints(arguments)?;
    // Python keeps the table as it is when it is given nothing to filter by.
    Ok(match predicates.len() {
        0 => frame,
        1 => frame.filter(predicates.remove(0)),
        _ => frame.filter(all_horizontal(&predicates)?),
    })
}

fn sort(frame: LazyFrame, arguments: &Arguments) -> Result<LazyFrame, BuildError> {
    if arguments.get("by").is_none() {
        return Err(BuildError::Missing { parameter: "by" });
    }
    let mut by = arguments.expressions("by")?;
    by.extend(arguments.expressions("more_by")?);
    let options = SortMultipleOptions {
        descending: per_item_flags(arguments, "descending", by.len(), false)?,
        nulls_last: per_item_flags(arguments, "nulls_last", by.len(), false)?,
        maintain_order: arguments.flag("maintain_order", false)?,
        multithreaded: arguments.flag("multithreaded", true)?,
        limit: None,
    };
    Ok(frame.sort_by_exprs(by, options))
}

/// The number of rows `head`, `limit` and `tail` take: Python's binding passes it on as an
/// unsigned 32-bit number, so a negative one and one that is too large are refused.
fn row_count(arguments: &Arguments) -> Result<u32, BuildError> {
    unsigned(arguments, "n", 5)
}

fn frame_head(frame: LazyFrame, arguments: &Arguments) -> Result<LazyFrame, BuildError> {
    Ok(frame.slice(0, row_count(arguments)?))
}

fn frame_tail(frame: LazyFrame, arguments: &Arguments) -> Result<LazyFrame, BuildError> {
    Ok(frame.tail(row_count(arguments)?))
}

fn frame_slice(frame: LazyFrame, arguments: &Arguments) -> Result<LazyFrame, BuildError> {
    let offset = match arguments.get("offset") {
        None => {
            return Err(BuildError::Missing {
                parameter: "offset",
            });
        }
        Some(Argument::Integer(offset)) => *offset,
        Some(other) => return Err(Arguments::wrong("offset", "a whole number", other)),
    };
    // Python leaves the length out as the most rows an index holds, and refuses a negative one.
    let length = optional_unsigned(arguments, "length")?.unwrap_or(u32::MAX);
    Ok(frame.slice(offset, length))
}

fn frame_first(frame: LazyFrame, _: &Arguments) -> Result<LazyFrame, BuildError> {
    Ok(frame.slice(0, 1))
}

fn frame_last(frame: LazyFrame, _: &Arguments) -> Result<LazyFrame, BuildError> {
    Ok(frame.tail(1))
}

/// The name of the column Python's `LazyFrame` gives a table for a step that selects over every
/// column, so that a table of no columns keeps its height.
const HEIGHT_COLUMN: &str = "__POLARS_INTERNAL_HEIGHT_COLUMN";

/// As Python's `_height_preserving` runs `operation`: on the table with a column of its own that
/// holds one empty struct for each row, which it drops from what the operation gives. A table of
/// no columns has the height of its rows that way, and an operation that gathers or keeps rows
/// gives it the height it gathers.
fn height_preserving(
    frame: LazyFrame,
    operation: impl FnOnce(LazyFrame) -> LazyFrame,
) -> LazyFrame {
    let height = lit(Scalar::new(
        DataType::Struct(Vec::new()),
        AnyValue::StructOwned(Box::new((Vec::new(), Vec::new()))),
    ))
    .alias(HEIGHT_COLUMN);
    operation(frame.with_columns([height])).drop(by_name([HEIGHT_COLUMN], true, true))
}

/// Python gathers every `n`th row of every column, from the `offset`th, and takes both as unsigned
/// numbers. Polars refuses an `n` of zero when it runs the rows.
fn frame_gather_every(frame: LazyFrame, arguments: &Arguments) -> Result<LazyFrame, BuildError> {
    if arguments.get("n").is_none() {
        return Err(BuildError::Missing { parameter: "n" });
    }
    let n = index(arguments, "n", 0)?;
    let offset = index(arguments, "offset", 0)?;
    Ok(height_preserving(frame, |frame| {
        frame.select([col("*").gather_every(n, offset)])
    }))
}

fn frame_reverse(frame: LazyFrame, _: &Arguments) -> Result<LazyFrame, BuildError> {
    Ok(height_preserving(frame, LazyFrame::reverse))
}

/// As Python's `drop` takes its columns: by name, each name read as a pattern where it is one, and
/// a name the table lacks refused where `strict` is true.
fn frame_drop(frame: LazyFrame, arguments: &Arguments) -> Result<LazyFrame, BuildError> {
    let names = arguments.columns("columns")?;
    Ok(frame.drop(by_name(names, arguments.flag("strict", true)?, true)))
}

/// The columns `drop_nulls` and `drop_nans` look at, if the step names them.
fn subset(arguments: &Arguments) -> Result<Option<Selector>, BuildError> {
    arguments
        .get("subset")
        .map(|_| arguments.columns("subset").map(cols))
        .transpose()
}

fn frame_drop_nulls(frame: LazyFrame, arguments: &Arguments) -> Result<LazyFrame, BuildError> {
    Ok(frame.drop_nulls(subset(arguments)?))
}

fn frame_drop_nans(frame: LazyFrame, arguments: &Arguments) -> Result<LazyFrame, BuildError> {
    Ok(frame.drop_nans(subset(arguments)?))
}

/// As Python's `fill_nan` makes a value that is not an expression: a literal, as `lit` makes one
/// without a dtype.
fn frame_fill_nan(frame: LazyFrame, arguments: &Arguments) -> Result<LazyFrame, BuildError> {
    let value = arguments
        .get("value")
        .ok_or(BuildError::Missing { parameter: "value" })?;
    Ok(frame.fill_nan(untyped_literal("value", value)?))
}

fn frame_with_row_index(frame: LazyFrame, arguments: &Arguments) -> Result<LazyFrame, BuildError> {
    let name = arguments
        .string("name")?
        .unwrap_or_else(|| "index".to_owned());
    Ok(frame.with_row_index(name, Some(unsigned(arguments, "offset", 0)?)))
}

fn select(frame: LazyFrame, arguments: &Arguments) -> Result<LazyFrame, BuildError> {
    Ok(frame.select(positional_and_named(arguments)?))
}

fn with_columns(frame: LazyFrame, arguments: &Arguments) -> Result<LazyFrame, BuildError> {
    Ok(frame.with_columns(positional_and_named(arguments)?))
}

fn rename(frame: LazyFrame, arguments: &Arguments) -> Result<LazyFrame, BuildError> {
    if arguments.get("mapping").is_none() {
        return Err(BuildError::Missing {
            parameter: "mapping",
        });
    }
    let (mut existing, mut new) = (Vec::new(), Vec::new());
    for (from, to) in arguments.pairs("mapping")? {
        existing.push(name_of("mapping", &from)?);
        new.push(match to {
            Argument::String(name) => name,
            other => return Err(Arguments::wrong("mapping", "a new name", &other)),
        });
    }
    Ok(frame.rename(existing, new, arguments.flag("strict", true)?))
}

fn join(frame: LazyFrame, arguments: &Arguments) -> Result<LazyFrame, BuildError> {
    let other = arguments.table("other")?;
    let how = match arguments.choice("how", HOW, "inner")? {
        "left" => JoinType::Left,
        "right" => JoinType::Right,
        "full" => JoinType::Full,
        "semi" => JoinType::Semi,
        "anti" => JoinType::Anti,
        "cross" => JoinType::Cross,
        _ => JoinType::Inner,
    };
    let (on, left_on, right_on) = (
        join_keys(arguments, "on")?,
        join_keys(arguments, "left_on")?,
        join_keys(arguments, "right_on")?,
    );
    let uses_left_right = left_on.is_some() || right_on.is_some();
    if on.is_some() && uses_left_right {
        return Err(refused("cannot use `on` with `left_on` or `right_on`"));
    }
    if left_on.is_some() != right_on.is_some() {
        return Err(refused("`left_on` needs a matching `right_on`"));
    }
    let coalesce = match optional_flag(arguments, "coalesce")? {
        None => JoinCoalesce::JoinSpecific,
        Some(true) => JoinCoalesce::CoalesceColumns,
        Some(false) => JoinCoalesce::KeepColumns,
    };
    let (left_on, right_on, coalesce) = if how == JoinType::Cross {
        if on.is_some() || uses_left_right {
            return Err(refused("a cross join takes no join keys"));
        }
        (Vec::new(), Vec::new(), JoinCoalesce::JoinSpecific)
    } else if let Some(on) = on {
        (on.clone(), on, coalesce)
    } else if let (Some(left_on), Some(right_on)) = (left_on, right_on) {
        (left_on, right_on, coalesce)
    } else {
        return Err(refused("give `on`, or `left_on` and `right_on`"));
    };
    let validate = match arguments.choice("validate", VALIDATE, "m:m")? {
        "m:1" => JoinValidation::ManyToOne,
        "1:m" => JoinValidation::OneToMany,
        "1:1" => JoinValidation::OneToOne,
        _ => JoinValidation::ManyToMany,
    };
    let maintain_order = match arguments.choice("maintain_order", MAINTAIN_ORDER, "none")? {
        "left" => MaintainOrderJoin::Left,
        "right" => MaintainOrderJoin::Right,
        "left_right" => MaintainOrderJoin::LeftRight,
        "right_left" => MaintainOrderJoin::RightLeft,
        _ => MaintainOrderJoin::None,
    };
    let build_side = match arguments.choice("build_side", BUILD_SIDE, "auto")? {
        "prefer_left" => Some(JoinBuildSide::PreferLeft),
        "prefer_right" => Some(JoinBuildSide::PreferRight),
        "force_left" => Some(JoinBuildSide::ForceLeft),
        "force_right" => Some(JoinBuildSide::ForceRight),
        _ => None,
    };
    let suffix = arguments
        .string("suffix")?
        .unwrap_or_else(|| "_right".to_owned());
    Ok(frame
        .join_builder()
        .with(other)
        .left_on(left_on)
        .right_on(right_on)
        .allow_parallel(arguments.flag("allow_parallel", true)?)
        .force_parallel(arguments.flag("force_parallel", false)?)
        .join_nulls(arguments.flag("nulls_equal", false)?)
        .how(how)
        .suffix(suffix)
        .validate(validate)
        .coalesce(coalesce)
        .maintain_order(maintain_order)
        .build_side(build_side)
        .finish()?)
}

fn pivot(frame: LazyFrame, arguments: &Arguments) -> Result<LazyFrame, BuildError> {
    crate::startup::ensure();
    let on = cols(required_columns(arguments, "on")?);
    // The values the new columns are named for, collected from the table given.
    let on_columns = Arc::new(arguments.table("on_columns")?.collect()?);
    let index = arguments.get("index").map(|_| arguments.columns("index"));
    let values = arguments.get("values").map(|_| arguments.columns("values"));
    let (index, values) = match (index.transpose()?, values.transpose()?) {
        (Some(index), Some(values)) => (cols(index), cols(values)),
        (Some(index), None) => {
            let index = cols(index);
            (index.clone(), all() - on.clone() - index)
        }
        (None, Some(values)) => {
            let values = cols(values);
            (all() - on.clone() - values.clone(), values)
        }
        (None, None) => return Err(refused("pivot needs `index` or `values`")),
    };
    let aggregate = match arguments.expression("aggregate_function")? {
        Some(aggregate) => aggregate,
        None => element().item(true),
    };
    let column_naming = match arguments.choice("column_naming", COLUMN_NAMING, "auto")? {
        "combine" => PivotColumnNaming::Combine,
        _ => PivotColumnNaming::Auto,
    };
    let separator = arguments
        .string("separator")?
        .unwrap_or_else(|| "_".to_owned());
    Ok(frame.pivot(
        on,
        on_columns,
        index,
        values,
        aggregate,
        arguments.flag("maintain_order", false)?,
        separator.into(),
        column_naming,
    ))
}

// ---------------------------------------------------------------------------
// Functions

fn polars_col(arguments: &Arguments) -> Result<Expr, BuildError> {
    let mut names = required_columns(arguments, "name")?;
    let one_name = names.len() == 1;
    let more = arguments.columns("more_names")?;
    if one_name && more.is_empty() {
        return Ok(col(names.remove(0)));
    }
    names.extend(more);
    Ok(cols(names).as_expr())
}

fn polars_element(_: &Arguments) -> Result<Expr, BuildError> {
    Ok(element())
}

/// A literal as Python's `lit` makes one of a value it is given without a dtype: a number typed
/// without a dtype stays loose, as in Python, and Polars gives it the dtype its other operand
/// needs, or the narrowest it fits.
fn untyped_literal(parameter: &'static str, value: &Argument) -> Result<Expr, BuildError> {
    match value {
        Argument::Integer(value) => Ok(lit(*value)),
        Argument::Float(value) => Ok(lit(*value)),
        Argument::String(value) => Ok(lit(value.as_str())),
        Argument::Flag(Some(value)) => Ok(lit(*value)),
        Argument::Flag(None) => Ok(lit(NULL)),
        other => Err(Arguments::wrong(
            parameter,
            "a number, text or a flag",
            other,
        )),
    }
}

fn polars_lit(arguments: &Arguments) -> Result<Expr, BuildError> {
    if arguments.flag("allow_object", false)? {
        return Err(refused(
            "allow_object is for a Python object, which a step cannot hold",
        ));
    }
    let value = arguments
        .get("value")
        .ok_or(BuildError::Missing { parameter: "value" })?;
    let dtype = match arguments.get("dtype") {
        None => None,
        Some(Argument::Dtype(dtype)) => Some(dtype),
        Some(other) => return Err(Arguments::wrong("dtype", "a dtype", other)),
    };
    let Some(dtype) = dtype else {
        return untyped_literal("value", value);
    };
    // With a dtype, Python casts a one-value series and makes a scalar of it.
    let name = "literal".into();
    let series = match value {
        Argument::Integer(value) => Series::new(name, [*value]),
        Argument::Float(value) => Series::new(name, [*value]),
        Argument::String(value) => Series::new(name, [value.as_str()]),
        Argument::Flag(Some(value)) => Series::new(name, [*value]),
        Argument::Flag(None) => Series::new_null(name, 1),
        other => return Err(Arguments::wrong("value", "a number, text or a flag", other)),
    }
    .strict_cast(dtype)?;
    Ok(lit(Scalar::new(
        series.dtype().clone(),
        series.get(0)?.into_static(),
    )))
}

/// A part of a date or a time that is not given, which Python makes the number 0.
fn part_or_zero(arguments: &Arguments, parameter: &'static str) -> Result<Expr, BuildError> {
    Ok(arguments.expression(parameter)?.unwrap_or_else(|| lit(0)))
}

/// `polars.date`: Python makes a moment of the three parts and keeps its date, named `date`.
fn polars_date(arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(datetime(DatetimeArgs::new(
        arguments.required_expression("year")?,
        arguments.required_expression("month")?,
        arguments.required_expression("day")?,
    ))
    .cast(DataType::Date)
    .alias("date"))
}

/// `polars.time`: Python makes a moment on the first of January 1970 of the parts and keeps its
/// time, named `time`.
fn polars_time(arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(datetime(DatetimeArgs {
        hour: part_or_zero(arguments, "hour")?,
        minute: part_or_zero(arguments, "minute")?,
        second: part_or_zero(arguments, "second")?,
        microsecond: part_or_zero(arguments, "microsecond")?,
        ..DatetimeArgs::default()
    })
    .cast(DataType::Time)
    .alias("time"))
}

/// `polars.datetime`: a moment of the parts, in the unit and the zone given, where a clock read
/// twice is resolved by the word `ambiguous`.
fn polars_datetime(arguments: &Arguments) -> Result<Expr, BuildError> {
    let time_zone = match arguments.get("time_zone") {
        None => None,
        Some(_) => arguments.time_zone("time_zone", true)?,
    };
    Ok(datetime(DatetimeArgs {
        year: arguments.required_expression("year")?,
        month: arguments.required_expression("month")?,
        day: arguments.required_expression("day")?,
        hour: part_or_zero(arguments, "hour")?,
        minute: part_or_zero(arguments, "minute")?,
        second: part_or_zero(arguments, "second")?,
        microsecond: part_or_zero(arguments, "microsecond")?,
        time_unit: time_unit(arguments, "time_unit", TIME_UNIT, Some("us"))?,
        time_zone,
        ambiguous: lit(arguments.choice("ambiguous", AMBIGUOUS, "raise")?),
    }))
}

/// `polars.duration`: a length of time of the parts, in the unit given, or in nanoseconds where
/// nanoseconds are given and no unit is, as Python infers it, and in microseconds otherwise.
fn polars_duration(arguments: &Arguments) -> Result<Expr, BuildError> {
    let time_unit = if arguments.get("time_unit").is_some() {
        time_unit(arguments, "time_unit", TIME_UNIT, None)?
    } else if arguments.get("nanoseconds").is_some() {
        TimeUnit::Nanoseconds
    } else {
        TimeUnit::Microseconds
    };
    Ok(duration(DurationArgs {
        weeks: part_or_zero(arguments, "weeks")?,
        days: part_or_zero(arguments, "days")?,
        hours: part_or_zero(arguments, "hours")?,
        minutes: part_or_zero(arguments, "minutes")?,
        seconds: part_or_zero(arguments, "seconds")?,
        milliseconds: part_or_zero(arguments, "milliseconds")?,
        microseconds: part_or_zero(arguments, "microseconds")?,
        nanoseconds: part_or_zero(arguments, "nanoseconds")?,
        time_unit,
    }))
}

/// `polars.from_epoch`: a count since the start of 1970 as Python reads it: a date for days, a
/// moment in microseconds for seconds and milliseconds, which it scales by an `Int64`, and a
/// moment in the unit itself for microseconds and nanoseconds.
fn polars_from_epoch(arguments: &Arguments) -> Result<Expr, BuildError> {
    let count = arguments.required_expression("column")?;
    let scaled = |scale: i64| {
        binary_expr(
            count.clone(),
            Operator::Multiply,
            lit(Scalar::new(DataType::Int64, AnyValue::Int64(scale))),
        )
        .cast(DataType::Datetime(TimeUnit::Microseconds, None))
    };
    Ok(match arguments.choice("time_unit", EPOCH_UNIT, "s")? {
        "d" => count.cast(DataType::Date),
        "s" => scaled(1_000_000),
        "ms" => scaled(1_000),
        "ns" => count.cast(DataType::Datetime(TimeUnit::Nanoseconds, None)),
        _ => count.cast(DataType::Datetime(TimeUnit::Microseconds, None)),
    })
}

/// Every entry, ordered by identifier.
pub(super) const ENTRIES: &[Entry] = &[
    Entry {
        id: EntryId("ChainedThen.otherwise"),
        family: Family::Condition,
        accepts: &[],
        parameters: &[STATEMENT],
        laziness: Laziness::Lazy,
        build: Build::ConditionEnd(chained_then_otherwise),
    },
    Entry {
        id: EntryId("ChainedThen.when"),
        family: Family::Condition,
        accepts: &[],
        parameters: &[PREDICATES, CONSTRAINTS],
        laziness: Laziness::Lazy,
        build: Build::ConditionStep(chained_then_when),
    },
    Entry {
        id: EntryId("ChainedWhen.then"),
        family: Family::Condition,
        accepts: &[],
        parameters: &[STATEMENT],
        laziness: Laziness::Lazy,
        build: Build::ConditionStep(chained_when_then),
    },
    Entry {
        id: EntryId("Expr.abs"),
        family: Family::Arithmetic,
        accepts: NUMERIC_OR_DURATION,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(abs),
    },
    Entry {
        id: EntryId("Expr.add"),
        family: Family::Arithmetic,
        accepts: ADDABLE,
        parameters: &[OTHER],
        laziness: Laziness::Lazy,
        build: Build::Expression(add),
    },
    Entry {
        id: EntryId("Expr.all"),
        family: Family::Aggregate,
        accepts: NUMERIC_OR_BOOLEAN,
        parameters: &[flag("ignore_nulls", "True").keyword_only()],
        laziness: Laziness::Lazy,
        build: Build::Expression(all_values),
    },
    Entry {
        id: EntryId("Expr.and_"),
        family: Family::Logic,
        accepts: BITWISE,
        parameters: &[starred("others", EXPRESSION)
            .with_role(Role::Operand)
            .with_at_least_one()],
        laziness: Laziness::Lazy,
        build: Build::Expression(and_),
    },
    Entry {
        id: EntryId("Expr.any"),
        family: Family::Aggregate,
        accepts: NUMERIC_OR_BOOLEAN,
        parameters: &[flag("ignore_nulls", "True").keyword_only()],
        laziness: Laziness::Lazy,
        build: Build::Expression(any_value),
    },
    Entry {
        id: EntryId("Expr.approx_n_unique"),
        family: Family::Aggregate,
        accepts: CUM_MAX,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(approx_n_unique),
    },
    Entry {
        id: EntryId("Expr.approx_quantile"),
        family: Family::Aggregate,
        accepts: CUM_MAX,
        parameters: &[
            required("quantile", Kind::Expression(NUMERIC), Shape::One).with_role(Role::Share),
            optional(
                "method",
                Kind::Choice(APPROX_QUANTILE_METHOD),
                Shape::One,
                "'auto'",
            )
            .keyword_only(),
            optional("error", Kind::Value(ValueType::Float), Shape::One, "0.001").keyword_only(),
            optional(
                "error_tightness",
                Kind::Choice(ERROR_TIGHTNESS),
                Shape::One,
                "'empirical'",
            )
            .keyword_only(),
        ],
        laziness: Laziness::Lazy,
        build: Build::Expression(approx_quantile),
    },
    Entry {
        id: EntryId("Expr.arccos"),
        family: Family::Trigonometry,
        accepts: NUMERIC,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(arccos),
    },
    Entry {
        id: EntryId("Expr.arccosh"),
        family: Family::Trigonometry,
        accepts: NUMERIC,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(arccosh),
    },
    Entry {
        id: EntryId("Expr.arcsin"),
        family: Family::Trigonometry,
        accepts: NUMERIC,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(arcsin),
    },
    Entry {
        id: EntryId("Expr.arcsinh"),
        family: Family::Trigonometry,
        accepts: NUMERIC,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(arcsinh),
    },
    Entry {
        id: EntryId("Expr.arctan"),
        family: Family::Trigonometry,
        accepts: NUMERIC,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(arctan),
    },
    Entry {
        id: EntryId("Expr.arctanh"),
        family: Family::Trigonometry,
        accepts: NUMERIC,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(arctanh),
    },
    Entry {
        id: EntryId("Expr.arg_max"),
        family: Family::Aggregate,
        accepts: ANY,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(arg_max),
    },
    Entry {
        id: EntryId("Expr.arg_min"),
        family: Family::Aggregate,
        accepts: ANY,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(arg_min),
    },
    Entry {
        id: EntryId("Expr.arg_sort"),
        family: Family::Order,
        accepts: ANY,
        parameters: &[
            flag("descending", "False").keyword_only(),
            flag("nulls_last", "False").keyword_only(),
        ],
        laziness: Laziness::Lazy,
        build: Build::Expression(arg_sort),
    },
    Entry {
        id: EntryId("Expr.arg_unique"),
        family: Family::Rows,
        accepts: ANY,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(arg_unique),
    },
    Entry {
        id: EntryId("Expr.backward_fill"),
        family: Family::Nulls,
        accepts: ANY,
        parameters: &[LIMIT],
        laziness: Laziness::Lazy,
        build: Build::Expression(backward_fill),
    },
    Entry {
        id: EntryId("Expr.bitwise_and"),
        family: Family::Aggregate,
        accepts: BITWISE,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(bitwise_and),
    },
    Entry {
        id: EntryId("Expr.bitwise_count_ones"),
        family: Family::Logic,
        accepts: BITWISE,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(bitwise_count_ones),
    },
    Entry {
        id: EntryId("Expr.bitwise_count_zeros"),
        family: Family::Logic,
        accepts: BITWISE,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(bitwise_count_zeros),
    },
    Entry {
        id: EntryId("Expr.bitwise_leading_ones"),
        family: Family::Logic,
        accepts: BITWISE,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(bitwise_leading_ones),
    },
    Entry {
        id: EntryId("Expr.bitwise_leading_zeros"),
        family: Family::Logic,
        accepts: BITWISE,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(bitwise_leading_zeros),
    },
    Entry {
        id: EntryId("Expr.bitwise_or"),
        family: Family::Aggregate,
        accepts: BITWISE,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(bitwise_or),
    },
    Entry {
        id: EntryId("Expr.bitwise_trailing_ones"),
        family: Family::Logic,
        accepts: BITWISE,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(bitwise_trailing_ones),
    },
    Entry {
        id: EntryId("Expr.bitwise_trailing_zeros"),
        family: Family::Logic,
        accepts: BITWISE,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(bitwise_trailing_zeros),
    },
    Entry {
        id: EntryId("Expr.bitwise_xor"),
        family: Family::Aggregate,
        accepts: BITWISE,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(bitwise_xor),
    },
    Entry {
        id: EntryId("Expr.bottom_k"),
        family: Family::Rows,
        accepts: ANY,
        parameters: &[optional("k", Kind::Expression(INTEGER), Shape::One, "5")
            .with_role(Role::Count)
            .not_broadcasting()
            .reading_one_value(OneValue::SingleRow)],
        laziness: Laziness::Lazy,
        build: Build::Expression(bottom_k),
    },
    Entry {
        id: EntryId("Expr.cast"),
        family: Family::Conversion,
        accepts: ANY,
        parameters: &[
            required("dtype", Kind::Dtype, Shape::One),
            flag("strict", "True").keyword_only(),
            flag("wrap_numerical", "False").keyword_only(),
        ],
        laziness: Laziness::Lazy,
        build: Build::Expression(cast),
    },
    Entry {
        id: EntryId("Expr.cbrt"),
        family: Family::Maths,
        accepts: NUMERIC_OR_BOOLEAN,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(cbrt),
    },
    Entry {
        id: EntryId("Expr.ceil"),
        family: Family::Rounding,
        accepts: NUMERIC,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(ceil),
    },
    Entry {
        id: EntryId("Expr.clip"),
        family: Family::Arithmetic,
        accepts: CLIPPABLE,
        parameters: &[bound("lower_bound"), bound("upper_bound")],
        laziness: Laziness::Lazy,
        build: Build::Expression(clip),
    },
    Entry {
        id: EntryId("Expr.cos"),
        family: Family::Trigonometry,
        accepts: NUMERIC,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(cos),
    },
    Entry {
        id: EntryId("Expr.cosh"),
        family: Family::Trigonometry,
        accepts: NUMERIC,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(cosh),
    },
    Entry {
        id: EntryId("Expr.cot"),
        family: Family::Trigonometry,
        accepts: NUMERIC,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(cot),
    },
    Entry {
        id: EntryId("Expr.count"),
        family: Family::Aggregate,
        accepts: ANY,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(count),
    },
    Entry {
        id: EntryId("Expr.cum_count"),
        family: Family::Cumulative,
        accepts: ANY,
        parameters: &[flag("reverse", "False").keyword_only()],
        laziness: Laziness::Lazy,
        build: Build::Expression(cum_count),
    },
    Entry {
        id: EntryId("Expr.cum_max"),
        family: Family::Cumulative,
        accepts: CUM_MAX,
        parameters: &[flag("reverse", "False").keyword_only()],
        laziness: Laziness::Lazy,
        build: Build::Expression(cum_max),
    },
    Entry {
        id: EntryId("Expr.cum_min"),
        family: Family::Cumulative,
        accepts: CUM_MAX,
        parameters: &[flag("reverse", "False").keyword_only()],
        laziness: Laziness::Lazy,
        build: Build::Expression(cum_min),
    },
    Entry {
        id: EntryId("Expr.cum_prod"),
        family: Family::Cumulative,
        accepts: NUMERIC_OR_BOOLEAN,
        parameters: &[flag("reverse", "False").keyword_only()],
        laziness: Laziness::Lazy,
        build: Build::Expression(cum_prod),
    },
    Entry {
        id: EntryId("Expr.cum_sum"),
        family: Family::Cumulative,
        accepts: CUM_SUM,
        parameters: &[flag("reverse", "False").keyword_only()],
        laziness: Laziness::Lazy,
        build: Build::Expression(cum_sum),
    },
    Entry {
        id: EntryId("Expr.degrees"),
        family: Family::Trigonometry,
        accepts: NUMERIC,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(degrees),
    },
    Entry {
        id: EntryId("Expr.diff"),
        family: Family::Rows,
        accepts: SUBTRACTABLE,
        parameters: &[
            optional("n", Kind::Expression(INTEGER), Shape::One, "1")
                .with_role(Role::Count)
                .not_broadcasting()
                .reading_one_value(OneValue::Scalar),
            optional(
                "null_behavior",
                Kind::Choice(NULL_BEHAVIOR),
                Shape::One,
                "'ignore'",
            ),
        ],
        laziness: Laziness::Lazy,
        build: Build::Expression(diff),
    },
    Entry {
        id: EntryId("Expr.drop_nans"),
        family: Family::Nulls,
        accepts: ANY,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(drop_nans),
    },
    Entry {
        id: EntryId("Expr.drop_nulls"),
        family: Family::Nulls,
        accepts: ANY,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(drop_nulls),
    },
    temporal("Expr.dt.base_utc_offset", ZONED, &[], dt_base_utc_offset),
    temporal(
        "Expr.dt.cast_time_unit",
        HAS_A_UNIT,
        &[required("time_unit", Kind::Choice(TIME_UNIT), Shape::One)],
        dt_cast_time_unit,
    ),
    temporal("Expr.dt.century", DATE_PART, &[], dt_century),
    temporal(
        "Expr.dt.combine",
        DATE_PART,
        &[
            required("time", Kind::Expression(TIME_OF_DAY), Shape::One),
            optional("time_unit", Kind::Choice(TIME_UNIT), Shape::One, "'us'"),
        ],
        dt_combine,
    ),
    temporal(
        "Expr.dt.convert_time_zone",
        ZONED,
        &[required("time_zone", Kind::TimeZone, Shape::One)],
        dt_convert_time_zone,
    ),
    Entry {
        id: EntryId("Expr.dt.date"),
        family: Family::Temporal,
        accepts: DATE_PART,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(dt_date),
    },
    temporal("Expr.dt.day", DATE_PART, &[], dt_day),
    temporal("Expr.dt.days_in_month", DATE_PART, &[], dt_days_in_month),
    temporal("Expr.dt.dst_offset", ZONED, &[], dt_dst_offset),
    temporal(
        "Expr.dt.epoch",
        COUNTED_SINCE_1970,
        &[optional(
            "time_unit",
            Kind::Choice(EPOCH_UNIT),
            Shape::One,
            "'us'",
        )],
        dt_epoch,
    ),
    temporal("Expr.dt.hour", TIME_PART, &[], dt_hour),
    temporal("Expr.dt.is_leap_year", DATE_PART, &[], dt_is_leap_year),
    temporal("Expr.dt.iso_year", DATE_PART, &[], dt_iso_year),
    temporal("Expr.dt.microsecond", TIME_PART, &[], dt_microsecond),
    temporal("Expr.dt.millennium", DATE_PART, &[], dt_millennium),
    temporal("Expr.dt.millisecond", TIME_PART, &[], dt_millisecond),
    temporal("Expr.dt.minute", TIME_PART, &[], dt_minute),
    temporal("Expr.dt.month", DATE_PART, &[], dt_month),
    temporal("Expr.dt.month_end", DATE_PART, &[], dt_month_end),
    Entry {
        id: EntryId("Expr.dt.month_start"),
        family: Family::Temporal,
        accepts: DATE_PART,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(dt_month_start),
    },
    temporal("Expr.dt.nanosecond", TIME_PART, &[], dt_nanosecond),
    Entry {
        id: EntryId("Expr.dt.offset_by"),
        family: Family::Temporal,
        accepts: DATE_PART,
        // `str | Expr`: a duration string is the expression that holds it.
        parameters: &[required("by", Kind::Expression(STRING), Shape::One)
            .holding_a_duration_string(DurationUse::Offset)],
        laziness: Laziness::Lazy,
        build: Build::Expression(dt_offset_by),
    },
    temporal("Expr.dt.ordinal_day", DATE_PART, &[], dt_ordinal_day),
    temporal("Expr.dt.quarter", DATE_PART, &[], dt_quarter),
    temporal(
        "Expr.dt.replace",
        DATE_PART,
        &[
            optional("year", EXPRESSION, Shape::One, "None")
                .keyword_only()
                .not_broadcasting_in_a_window(),
            optional("month", EXPRESSION, Shape::One, "None")
                .keyword_only()
                .not_broadcasting_in_a_window(),
            optional("day", EXPRESSION, Shape::One, "None")
                .keyword_only()
                .not_broadcasting_in_a_window(),
            optional("hour", EXPRESSION, Shape::One, "None")
                .keyword_only()
                .not_broadcasting_in_a_window(),
            optional("minute", EXPRESSION, Shape::One, "None")
                .keyword_only()
                .not_broadcasting_in_a_window(),
            optional("second", EXPRESSION, Shape::One, "None")
                .keyword_only()
                .not_broadcasting_in_a_window(),
            optional("microsecond", EXPRESSION, Shape::One, "None")
                .keyword_only()
                .not_broadcasting_in_a_window(),
            optional("ambiguous", Kind::Choice(AMBIGUOUS), Shape::One, "'raise'").keyword_only(),
        ],
        dt_replace,
    ),
    temporal(
        "Expr.dt.replace_time_zone",
        ZONED,
        &[
            required("time_zone", Kind::TimeZone, Shape::One).or_no_time_zone(),
            optional("ambiguous", Kind::Choice(AMBIGUOUS), Shape::One, "'raise'").keyword_only(),
            optional(
                "non_existent",
                Kind::Choice(NON_EXISTENT),
                Shape::One,
                "'raise'",
            )
            .keyword_only(),
        ],
        dt_replace_time_zone,
    ),
    temporal(
        "Expr.dt.round",
        DATE_PART,
        // `str | timedelta | Expr`: a duration string is the expression that holds it.
        &[required("every", Kind::Expression(STRING), Shape::One)
            .holding_a_duration_string(DurationUse::Round)],
        dt_round,
    ),
    temporal(
        "Expr.dt.second",
        TIME_PART,
        &[flag("fractional", "False").keyword_only()],
        dt_second,
    ),
    temporal(
        "Expr.dt.strftime",
        WRITABLE,
        &[required(
            "format",
            Kind::Value(ValueType::String),
            Shape::One,
        )],
        dt_strftime,
    ),
    Entry {
        id: EntryId("Expr.dt.time"),
        family: Family::Temporal,
        accepts: TIME_PART,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(dt_time),
    },
    temporal(
        "Expr.dt.timestamp",
        COUNTED_SINCE_1970,
        &[optional(
            "time_unit",
            Kind::Choice(TIME_UNIT),
            Shape::One,
            "'us'",
        )],
        dt_timestamp,
    ),
    temporal(
        "Expr.dt.to_string",
        WRITABLE,
        &[optional(
            "format",
            Kind::Value(ValueType::String),
            Shape::One,
            "None",
        )],
        dt_to_string,
    ),
    Entry {
        id: EntryId("Expr.dt.total_days"),
        family: Family::Temporal,
        accepts: DURATION,
        parameters: &[flag("fractional", "False").keyword_only()],
        laziness: Laziness::Lazy,
        build: Build::Expression(dt_total_days),
    },
    temporal(
        "Expr.dt.total_hours",
        DURATION,
        &[flag("fractional", "False").keyword_only()],
        dt_total_hours,
    ),
    temporal(
        "Expr.dt.total_microseconds",
        DURATION,
        &[flag("fractional", "False").keyword_only()],
        dt_total_microseconds,
    ),
    temporal(
        "Expr.dt.total_milliseconds",
        DURATION,
        &[flag("fractional", "False").keyword_only()],
        dt_total_milliseconds,
    ),
    temporal(
        "Expr.dt.total_minutes",
        DURATION,
        &[flag("fractional", "False").keyword_only()],
        dt_total_minutes,
    ),
    temporal(
        "Expr.dt.total_nanoseconds",
        DURATION,
        &[flag("fractional", "False").keyword_only()],
        dt_total_nanoseconds,
    ),
    temporal(
        "Expr.dt.total_seconds",
        DURATION,
        &[flag("fractional", "False").keyword_only()],
        dt_total_seconds,
    ),
    temporal(
        "Expr.dt.truncate",
        DATE_PART,
        // `str | timedelta | Expr`: a duration string is the expression that holds it.
        &[required("every", Kind::Expression(STRING), Shape::One)
            .holding_a_duration_string(DurationUse::Cut)],
        dt_truncate,
    ),
    temporal("Expr.dt.week", DATE_PART, &[], dt_week),
    temporal("Expr.dt.weekday", DATE_PART, &[], dt_weekday),
    temporal("Expr.dt.year", DATE_PART, &[], dt_year),
    Entry {
        id: EntryId("Expr.entropy"),
        family: Family::Aggregate,
        accepts: NUMERIC_OR_BOOLEAN,
        parameters: &[
            optional("base", Kind::Value(ValueType::Float), Shape::One, "math.e"),
            flag("normalize", "True").keyword_only(),
        ],
        laziness: Laziness::Lazy,
        build: Build::Expression(entropy),
    },
    Entry {
        id: EntryId("Expr.eq"),
        family: Family::Compare,
        accepts: ANY,
        parameters: &[OTHER],
        laziness: Laziness::Lazy,
        build: Build::Expression(eq),
    },
    Entry {
        id: EntryId("Expr.eq_missing"),
        family: Family::Compare,
        accepts: ANY,
        parameters: &[OTHER],
        laziness: Laziness::Lazy,
        build: Build::Expression(eq_missing),
    },
    Entry {
        id: EntryId("Expr.erf"),
        family: Family::Maths,
        accepts: NUMERIC_OR_BOOLEAN,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(erf),
    },
    Entry {
        id: EntryId("Expr.erfc"),
        family: Family::Maths,
        accepts: NUMERIC_OR_BOOLEAN,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(erfc),
    },
    Entry {
        id: EntryId("Expr.exp"),
        family: Family::Maths,
        accepts: NUMERIC_OR_BOOLEAN,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(exp),
    },
    Entry {
        id: EntryId("Expr.fill_nan"),
        family: Family::Nulls,
        accepts: NUMERIC,
        parameters: &[required("value", EXPRESSION, Shape::One).with_role(Role::Filler)],
        laziness: Laziness::Lazy,
        build: Build::Expression(fill_nan),
    },
    Entry {
        id: EntryId("Expr.fill_null"),
        family: Family::Nulls,
        accepts: ANY,
        parameters: &[
            optional("value", EXPRESSION, Shape::One, "None")
                .with_role(Role::Filler)
                .with_at_least_one(),
            optional("strategy", Kind::Choice(FILL_STRATEGY), Shape::One, "None")
                .given_in_place_of("value"),
            LIMIT,
        ],
        laziness: Laziness::Lazy,
        build: Build::Expression(fill_null),
    },
    Entry {
        id: EntryId("Expr.filter"),
        family: Family::Rows,
        accepts: ANY,
        parameters: &[PREDICATES.with_at_least_one(), CONSTRAINTS],
        laziness: Laziness::Lazy,
        build: Build::Expression(expr_filter),
    },
    Entry {
        id: EntryId("Expr.first"),
        family: Family::Rows,
        accepts: ANY,
        parameters: &[flag("ignore_nulls", "False").keyword_only()],
        laziness: Laziness::Lazy,
        build: Build::Expression(first),
    },
    Entry {
        id: EntryId("Expr.floor"),
        family: Family::Rounding,
        accepts: NUMERIC,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(floor),
    },
    Entry {
        id: EntryId("Expr.floordiv"),
        family: Family::Arithmetic,
        accepts: DIVISIBLE,
        parameters: &[FLOOR_DIVISOR],
        laziness: Laziness::Lazy,
        build: Build::Expression(floordiv),
    },
    Entry {
        id: EntryId("Expr.forward_fill"),
        family: Family::Nulls,
        accepts: ANY,
        parameters: &[LIMIT],
        laziness: Laziness::Lazy,
        build: Build::Expression(forward_fill),
    },
    Entry {
        id: EntryId("Expr.gather"),
        family: Family::Rows,
        accepts: ANY,
        parameters: &[
            required("indices", Kind::Expression(INTEGER), Shape::One),
            flag("null_on_oob", "False").keyword_only(),
        ],
        laziness: Laziness::Lazy,
        build: Build::Expression(gather),
    },
    Entry {
        id: EntryId("Expr.gather_every"),
        family: Family::Rows,
        accepts: ANY,
        parameters: &[
            required("n", Kind::Value(ValueType::Integer), Shape::One),
            optional("offset", Kind::Value(ValueType::Integer), Shape::One, "0"),
        ],
        laziness: Laziness::Lazy,
        build: Build::Expression(gather_every),
    },
    Entry {
        id: EntryId("Expr.ge"),
        family: Family::Compare,
        accepts: ANY,
        parameters: &[OTHER],
        laziness: Laziness::Lazy,
        build: Build::Expression(ge),
    },
    Entry {
        id: EntryId("Expr.gt"),
        family: Family::Compare,
        accepts: ANY,
        parameters: &[OTHER],
        laziness: Laziness::Lazy,
        build: Build::Expression(gt),
    },
    Entry {
        id: EntryId("Expr.has_nulls"),
        family: Family::Nulls,
        accepts: ANY,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(has_nulls),
    },
    Entry {
        id: EntryId("Expr.head"),
        family: Family::Rows,
        accepts: ANY,
        parameters: &[optional("n", Kind::Expression(INTEGER), Shape::One, "10")
            .with_role(Role::Count)
            .not_broadcasting()],
        laziness: Laziness::Lazy,
        build: Build::Expression(head),
    },
    Entry {
        id: EntryId("Expr.is_between"),
        family: Family::Compare,
        accepts: CUM_MAX,
        parameters: &[
            required("lower_bound", EXPRESSION, Shape::One).with_role(Role::Operand),
            required("upper_bound", EXPRESSION, Shape::One)
                .with_role(Role::Operand)
                .sharing_a_dtype_with("lower_bound"),
            optional("closed", Kind::Choice(CLOSED), Shape::One, "'both'"),
        ],
        laziness: Laziness::Lazy,
        build: Build::Expression(is_between),
    },
    Entry {
        id: EntryId("Expr.is_close"),
        family: Family::Compare,
        accepts: CLOSE,
        parameters: &[
            CLOSE_OTHER,
            tolerance("abs_tol", "0.0"),
            tolerance("rel_tol", "1e-09"),
            flag("nans_equal", "False").keyword_only(),
        ],
        laziness: Laziness::Lazy,
        build: Build::Expression(is_close),
    },
    Entry {
        id: EntryId("Expr.is_duplicated"),
        family: Family::Checks,
        accepts: ANY,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(is_duplicated),
    },
    Entry {
        id: EntryId("Expr.is_finite"),
        family: Family::Checks,
        accepts: NUMERIC,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(is_finite),
    },
    Entry {
        id: EntryId("Expr.is_first_distinct"),
        family: Family::Checks,
        accepts: ANY,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(is_first_distinct),
    },
    Entry {
        id: EntryId("Expr.is_in"),
        family: Family::Compare,
        accepts: ANY,
        parameters: &[
            required("other", EXPRESSION, Shape::One),
            flag("nulls_equal", "False").keyword_only(),
        ],
        laziness: Laziness::Lazy,
        build: Build::Expression(is_in),
    },
    Entry {
        id: EntryId("Expr.is_infinite"),
        family: Family::Checks,
        accepts: NUMERIC,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(is_infinite),
    },
    Entry {
        id: EntryId("Expr.is_last_distinct"),
        family: Family::Checks,
        accepts: ANY,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(is_last_distinct),
    },
    Entry {
        id: EntryId("Expr.is_nan"),
        family: Family::Checks,
        accepts: NUMERIC,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(is_nan),
    },
    Entry {
        id: EntryId("Expr.is_not_nan"),
        family: Family::Checks,
        accepts: NUMERIC,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(is_not_nan),
    },
    Entry {
        id: EntryId("Expr.is_not_null"),
        family: Family::Compare,
        accepts: ANY,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(is_not_null),
    },
    Entry {
        id: EntryId("Expr.is_null"),
        family: Family::Compare,
        accepts: ANY,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(is_null),
    },
    Entry {
        id: EntryId("Expr.is_unique"),
        family: Family::Checks,
        accepts: ANY,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(is_unique),
    },
    Entry {
        id: EntryId("Expr.kurtosis"),
        family: Family::Aggregate,
        accepts: MOMENTS,
        parameters: &[
            flag("fisher", "True").keyword_only(),
            flag("bias", "True").keyword_only(),
        ],
        laziness: Laziness::Lazy,
        build: Build::Expression(kurtosis),
    },
    Entry {
        id: EntryId("Expr.last"),
        family: Family::Rows,
        accepts: ANY,
        parameters: &[flag("ignore_nulls", "False").keyword_only()],
        laziness: Laziness::Lazy,
        build: Build::Expression(last),
    },
    Entry {
        id: EntryId("Expr.le"),
        family: Family::Compare,
        accepts: ANY,
        parameters: &[OTHER],
        laziness: Laziness::Lazy,
        build: Build::Expression(le),
    },
    Entry {
        id: EntryId("Expr.len"),
        family: Family::Aggregate,
        accepts: ANY,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(len),
    },
    Entry {
        id: EntryId("Expr.limit"),
        family: Family::Rows,
        accepts: ANY,
        parameters: &[optional("n", Kind::Expression(INTEGER), Shape::One, "10")
            .with_role(Role::Count)
            .not_broadcasting()],
        laziness: Laziness::Lazy,
        build: Build::Expression(head),
    },
    Entry {
        id: EntryId("Expr.log"),
        family: Family::Maths,
        accepts: NUMERIC_OR_BOOLEAN,
        parameters: &[optional("base", EXPRESSION, Shape::One, "math.e").with_role(Role::Base)],
        laziness: Laziness::Lazy,
        build: Build::Expression(log),
    },
    Entry {
        id: EntryId("Expr.log1p"),
        family: Family::Maths,
        accepts: NUMERIC_OR_BOOLEAN,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(log1p),
    },
    Entry {
        id: EntryId("Expr.lt"),
        family: Family::Compare,
        accepts: ANY,
        parameters: &[OTHER],
        laziness: Laziness::Lazy,
        build: Build::Expression(lt),
    },
    Entry {
        id: EntryId("Expr.max"),
        family: Family::Aggregate,
        accepts: CUM_MAX,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(max),
    },
    Entry {
        id: EntryId("Expr.mean"),
        family: Family::Aggregate,
        accepts: MEANABLE,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(mean),
    },
    Entry {
        id: EntryId("Expr.median"),
        family: Family::Aggregate,
        accepts: MEANABLE,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(median),
    },
    Entry {
        id: EntryId("Expr.min"),
        family: Family::Aggregate,
        accepts: CUM_MAX,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(min),
    },
    Entry {
        id: EntryId("Expr.mod"),
        family: Family::Arithmetic,
        accepts: DIVISIBLE,
        parameters: &[OTHER],
        laziness: Laziness::Lazy,
        build: Build::Expression(modulo),
    },
    Entry {
        id: EntryId("Expr.mode"),
        family: Family::Rows,
        accepts: ANY,
        parameters: &[flag("maintain_order", "False").keyword_only()],
        laziness: Laziness::Lazy,
        build: Build::Expression(mode),
    },
    Entry {
        id: EntryId("Expr.mul"),
        family: Family::Arithmetic,
        accepts: SCALABLE,
        parameters: &[OTHER],
        laziness: Laziness::Lazy,
        build: Build::Expression(mul),
    },
    Entry {
        id: EntryId("Expr.n_unique"),
        family: Family::Aggregate,
        accepts: ANY,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(n_unique),
    },
    Entry {
        id: EntryId("Expr.nan_max"),
        family: Family::Aggregate,
        accepts: CUM_MAX,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(nan_max),
    },
    Entry {
        id: EntryId("Expr.nan_min"),
        family: Family::Aggregate,
        accepts: CUM_MAX,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(nan_min),
    },
    Entry {
        id: EntryId("Expr.ne"),
        family: Family::Compare,
        accepts: ANY,
        parameters: &[OTHER],
        laziness: Laziness::Lazy,
        build: Build::Expression(ne),
    },
    Entry {
        id: EntryId("Expr.ne_missing"),
        family: Family::Compare,
        accepts: ANY,
        parameters: &[OTHER],
        laziness: Laziness::Lazy,
        build: Build::Expression(ne_missing),
    },
    Entry {
        id: EntryId("Expr.neg"),
        family: Family::Arithmetic,
        accepts: NUMERIC_OR_DURATION,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(neg),
    },
    Entry {
        id: EntryId("Expr.not_"),
        family: Family::Logic,
        accepts: BITWISE,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(not_),
    },
    Entry {
        id: EntryId("Expr.null_count"),
        family: Family::Nulls,
        accepts: ANY,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(null_count),
    },
    Entry {
        id: EntryId("Expr.or_"),
        family: Family::Logic,
        accepts: BITWISE,
        parameters: &[starred("others", EXPRESSION)
            .with_role(Role::Operand)
            .with_at_least_one()],
        laziness: Laziness::Lazy,
        build: Build::Expression(or_),
    },
    Entry {
        id: EntryId("Expr.over"),
        family: Family::Window,
        accepts: ANY,
        parameters: &[
            optional("partition_by", EXPRESSION, Shape::Many, "None"),
            starred("more_exprs", EXPRESSION),
            optional("order_by", EXPRESSION, Shape::Many, "None").keyword_only(),
            flag("descending", "False").keyword_only(),
            flag("nulls_last", "False").keyword_only(),
            optional(
                "mapping_strategy",
                Kind::Choice(MAPPING_STRATEGY),
                Shape::One,
                "'group_to_rows'",
            )
            .keyword_only(),
        ],
        laziness: Laziness::Lazy,
        build: Build::Expression(over),
    },
    Entry {
        id: EntryId("Expr.pct_change"),
        family: Family::Rows,
        accepts: ANY,
        parameters: &[optional("n", Kind::Expression(INTEGER), Shape::One, "1")
            .with_role(Role::Count)
            .not_broadcasting()
            .reading_one_value(OneValue::SingleRow)],
        laziness: Laziness::Lazy,
        build: Build::Expression(pct_change),
    },
    Entry {
        id: EntryId("Expr.pow"),
        family: Family::Arithmetic,
        accepts: NUMERIC,
        parameters: &[required("exponent", EXPRESSION, Shape::One).with_role(Role::Exponent)],
        laziness: Laziness::Lazy,
        build: Build::Expression(pow),
    },
    Entry {
        id: EntryId("Expr.product"),
        family: Family::Aggregate,
        accepts: NUMERIC_OR_BOOLEAN,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(product),
    },
    Entry {
        id: EntryId("Expr.quantile"),
        family: Family::Aggregate,
        accepts: QUANTILE,
        parameters: &[
            required("quantile", Kind::Expression(NUMERIC), Shape::One)
                .with_role(Role::Share)
                .not_broadcasting()
                .reading_one_value(OneValue::SingleRow),
            optional(
                "interpolation",
                Kind::Choice(QUANTILE_METHOD),
                Shape::One,
                "'nearest'",
            ),
        ],
        laziness: Laziness::Lazy,
        build: Build::Expression(quantile),
    },
    Entry {
        id: EntryId("Expr.radians"),
        family: Family::Trigonometry,
        accepts: NUMERIC,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(radians),
    },
    Entry {
        id: EntryId("Expr.rank"),
        family: Family::Order,
        accepts: ANY,
        parameters: &[
            optional("method", Kind::Choice(RANK_METHOD), Shape::One, "'average'"),
            flag("descending", "False").keyword_only(),
            optional("seed", Kind::Value(ValueType::Integer), Shape::One, "None").keyword_only(),
        ],
        laziness: Laziness::Lazy,
        build: Build::Expression(rank),
    },
    Entry {
        id: EntryId("Expr.replace"),
        family: Family::Conversion,
        accepts: ANY,
        parameters: &[
            required("old", EXPRESSION, Shape::One)
                .with_role(Role::Bound)
                .not_broadcasting(),
            optional("new", EXPRESSION, Shape::One, "NO_DEFAULT")
                .with_role(Role::Bound)
                .with_at_least_one()
                .given_as_many_as(AsManyAs::Parameter("old"))
                .not_broadcasting(),
        ],
        laziness: Laziness::Lazy,
        build: Build::Expression(replace),
    },
    Entry {
        id: EntryId("Expr.replace_strict"),
        family: Family::Conversion,
        accepts: ANY,
        parameters: &[
            required("old", EXPRESSION, Shape::One)
                .with_role(Role::Bound)
                .not_broadcasting(),
            optional("new", EXPRESSION, Shape::One, "NO_DEFAULT")
                .with_at_least_one()
                .given_as_many_as(AsManyAs::Parameter("old"))
                .not_broadcasting(),
            optional("default", EXPRESSION, Shape::One, "NO_DEFAULT")
                .keyword_only()
                .sharing_a_dtype_with("new")
                .given_as_many_as(AsManyAs::TheReceiver)
                .not_broadcasting(),
            optional("return_dtype", Kind::Dtype, Shape::One, "None")
                .keyword_only()
                .converting(&["new", "default"])
                .converting_leniently_beside("default"),
        ],
        laziness: Laziness::Lazy,
        build: Build::Expression(replace_strict),
    },
    Entry {
        id: EntryId("Expr.reverse"),
        family: Family::Order,
        accepts: ANY,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(reverse),
    },
    Entry {
        id: EntryId("Expr.rle_id"),
        family: Family::Rows,
        accepts: ANY,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(rle_id),
    },
    Entry {
        id: EntryId("Expr.round"),
        family: Family::Rounding,
        accepts: NUMERIC,
        parameters: &[
            DECIMALS,
            optional(
                "mode",
                Kind::Choice(ROUND_MODE),
                Shape::One,
                "'half_to_even'",
            ),
        ],
        laziness: Laziness::Lazy,
        build: Build::Expression(round),
    },
    Entry {
        id: EntryId("Expr.round_sig_figs"),
        family: Family::Rounding,
        accepts: NUMERIC,
        parameters: &[required(
            "digits",
            Kind::Value(ValueType::Integer),
            Shape::One,
        )],
        laziness: Laziness::Lazy,
        build: Build::Expression(round_sig_figs),
    },
    Entry {
        id: EntryId("Expr.search_sorted"),
        family: Family::Order,
        accepts: ANY,
        parameters: &[
            required("element", EXPRESSION, Shape::One),
            optional("side", Kind::Choice(SEARCH_SIDE), Shape::One, "'any'"),
            flag("descending", "False").keyword_only(),
        ],
        laziness: Laziness::Lazy,
        build: Build::Expression(search_sorted),
    },
    Entry {
        id: EntryId("Expr.shift"),
        family: Family::Rows,
        accepts: ANY,
        parameters: &[
            optional("n", Kind::Expression(INTEGER), Shape::One, "1")
                .with_role(Role::Count)
                .not_broadcasting()
                .reading_one_value(OneValue::Scalar),
            optional("fill_value", EXPRESSION, Shape::One, "None")
                .keyword_only()
                .with_role(Role::Filler)
                .not_broadcasting()
                .reading_one_value(OneValue::Scalar),
        ],
        laziness: Laziness::Lazy,
        build: Build::Expression(shift),
    },
    Entry {
        id: EntryId("Expr.sign"),
        family: Family::Arithmetic,
        accepts: NUMERIC,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(sign),
    },
    Entry {
        id: EntryId("Expr.sin"),
        family: Family::Trigonometry,
        accepts: NUMERIC,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(sin),
    },
    Entry {
        id: EntryId("Expr.sinh"),
        family: Family::Trigonometry,
        accepts: NUMERIC,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(sinh),
    },
    Entry {
        id: EntryId("Expr.skew"),
        family: Family::Aggregate,
        accepts: MOMENTS,
        parameters: &[flag("bias", "True").keyword_only()],
        laziness: Laziness::Lazy,
        build: Build::Expression(skew),
    },
    Entry {
        id: EntryId("Expr.slice"),
        family: Family::Rows,
        accepts: ANY,
        parameters: &[
            required("offset", Kind::Expression(INTEGER), Shape::One)
                .with_role(Role::Count)
                .not_broadcasting(),
            optional("length", Kind::Expression(INTEGER), Shape::One, "None")
                .with_role(Role::Count)
                .not_broadcasting(),
        ],
        laziness: Laziness::Lazy,
        build: Build::Expression(slice),
    },
    Entry {
        id: EntryId("Expr.sort"),
        family: Family::Order,
        accepts: ANY,
        parameters: &[
            flag("descending", "False").keyword_only(),
            flag("nulls_last", "False").keyword_only(),
        ],
        laziness: Laziness::Lazy,
        build: Build::Expression(sort_values),
    },
    Entry {
        id: EntryId("Expr.sort_by"),
        family: Family::Order,
        accepts: ANY,
        parameters: &[
            required("by", EXPRESSION, Shape::Many),
            starred("more_by", EXPRESSION),
            optional("descending", Kind::Flag, Shape::PerItem("by"), "False").keyword_only(),
            optional("nulls_last", Kind::Flag, Shape::PerItem("by"), "False").keyword_only(),
            flag("multithreaded", "True").keyword_only(),
            flag("maintain_order", "False").keyword_only(),
        ],
        laziness: Laziness::Lazy,
        build: Build::Expression(sort_by),
    },
    Entry {
        id: EntryId("Expr.sqrt"),
        family: Family::Maths,
        accepts: NUMERIC_OR_BOOLEAN,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(sqrt),
    },
    Entry {
        id: EntryId("Expr.std"),
        family: Family::Aggregate,
        accepts: NUMERIC_OR_BOOLEAN,
        parameters: &[DDOF],
        laziness: Laziness::Lazy,
        build: Build::Expression(std_deviation),
    },
    Entry {
        id: EntryId("Expr.str.contains"),
        family: Family::Compare,
        accepts: STRING,
        parameters: &[PATTERN_OR_TEXT, LITERAL, TYPED_STRICT],
        laziness: Laziness::Lazy,
        build: Build::Expression(str_contains),
    },
    Entry {
        id: EntryId("Expr.str.count_matches"),
        family: Family::Search,
        accepts: STRING,
        parameters: &[PATTERN_OR_TEXT, LITERAL],
        laziness: Laziness::Lazy,
        build: Build::Expression(str_count_matches),
    },
    Entry {
        id: EntryId("Expr.str.ends_with"),
        family: Family::Compare,
        accepts: STRING,
        parameters: &[required("suffix", EXPRESSION, Shape::One)],
        laziness: Laziness::Lazy,
        build: Build::Expression(str_ends_with),
    },
    Entry {
        id: EntryId("Expr.str.escape_regex"),
        family: Family::Text,
        accepts: STRING,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(str_escape_regex),
    },
    Entry {
        id: EntryId("Expr.str.extract"),
        family: Family::Search,
        accepts: STRING,
        parameters: &[
            PATTERN,
            optional(
                "group_index",
                Kind::Value(ValueType::Integer),
                Shape::One,
                "1",
            ),
        ],
        laziness: Laziness::Lazy,
        build: Build::Expression(str_extract),
    },
    Entry {
        id: EntryId("Expr.str.extract_all"),
        family: Family::Search,
        accepts: STRING,
        parameters: &[PATTERN],
        laziness: Laziness::Lazy,
        build: Build::Expression(str_extract_all),
    },
    Entry {
        id: EntryId("Expr.str.extract_groups"),
        family: Family::Search,
        accepts: STRING,
        parameters: &[PATTERN.naming_the_fields_of_the_result()],
        laziness: Laziness::Lazy,
        build: Build::Expression(str_extract_groups),
    },
    Entry {
        id: EntryId("Expr.str.find"),
        family: Family::Search,
        accepts: STRING,
        parameters: &[PATTERN_OR_TEXT, LITERAL, TYPED_STRICT],
        laziness: Laziness::Lazy,
        build: Build::Expression(str_find),
    },
    Entry {
        id: EntryId("Expr.str.head"),
        family: Family::Pieces,
        accepts: STRING,
        parameters: &[characters_counted("n")],
        laziness: Laziness::Lazy,
        build: Build::Expression(str_head),
    },
    Entry {
        id: EntryId("Expr.str.join"),
        family: Family::Pieces,
        accepts: JOINABLE,
        parameters: &[
            optional(
                "delimiter",
                Kind::Value(ValueType::String),
                Shape::One,
                "''",
            ),
            flag("ignore_nulls", "True").keyword_only(),
        ],
        laziness: Laziness::Lazy,
        build: Build::Expression(str_join),
    },
    Entry {
        id: EntryId("Expr.str.len_bytes"),
        family: Family::Pieces,
        accepts: STRING,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(str_len_bytes),
    },
    Entry {
        id: EntryId("Expr.str.len_chars"),
        family: Family::Pieces,
        accepts: STRING,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(str_len_chars),
    },
    Entry {
        id: EntryId("Expr.str.pad_end"),
        family: Family::Text,
        accepts: STRING,
        parameters: &[a_length("length"), FILL_CHAR],
        laziness: Laziness::Lazy,
        build: Build::Expression(str_pad_end),
    },
    Entry {
        id: EntryId("Expr.str.pad_start"),
        family: Family::Text,
        accepts: STRING,
        parameters: &[a_length("length"), FILL_CHAR],
        laziness: Laziness::Lazy,
        build: Build::Expression(str_pad_start),
    },
    Entry {
        id: EntryId("Expr.str.replace"),
        family: Family::Text,
        accepts: STRING,
        parameters: &[
            PATTERN_OR_TEXT,
            REPLACEMENT,
            LITERAL,
            optional("n", Kind::Value(ValueType::Integer), Shape::One, "1").keyword_only(),
        ],
        laziness: Laziness::Lazy,
        build: Build::Expression(str_replace),
    },
    Entry {
        id: EntryId("Expr.str.replace_all"),
        family: Family::Text,
        accepts: STRING,
        parameters: &[PATTERN_OR_TEXT, REPLACEMENT, LITERAL],
        laziness: Laziness::Lazy,
        build: Build::Expression(str_replace_all),
    },
    Entry {
        id: EntryId("Expr.str.reverse"),
        family: Family::Text,
        accepts: STRING,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(str_reverse),
    },
    Entry {
        id: EntryId("Expr.str.slice"),
        family: Family::Pieces,
        accepts: STRING,
        parameters: &[
            characters_counted("offset"),
            optional("length", EXPRESSION, Shape::One, "None").with_role(Role::Length),
        ],
        laziness: Laziness::Lazy,
        build: Build::Expression(str_slice),
    },
    Entry {
        id: EntryId("Expr.str.split"),
        family: Family::Pieces,
        accepts: STRING,
        parameters: &[
            // A column of separators is Polars' to check, row by row; text typed in is a pattern
            // where `literal` is false, and is refused as one.
            required("by", EXPRESSION, Shape::One).with_plain_text_flag("literal"),
            flag("inclusive", "False").keyword_only(),
            flag("literal", "True").keyword_only(),
            flag("strict", "True").keyword_only(),
        ],
        laziness: Laziness::Lazy,
        build: Build::Expression(str_split),
    },
    Entry {
        id: EntryId("Expr.str.starts_with"),
        family: Family::Compare,
        accepts: STRING,
        parameters: &[required("prefix", EXPRESSION, Shape::One)],
        laziness: Laziness::Lazy,
        build: Build::Expression(str_starts_with),
    },
    Entry {
        id: EntryId("Expr.str.strip_chars"),
        family: Family::Text,
        accepts: STRING,
        parameters: &[CHARACTERS],
        laziness: Laziness::Lazy,
        build: Build::Expression(str_strip_chars),
    },
    Entry {
        id: EntryId("Expr.str.strip_chars_end"),
        family: Family::Text,
        accepts: STRING,
        parameters: &[CHARACTERS],
        laziness: Laziness::Lazy,
        build: Build::Expression(str_strip_chars_end),
    },
    Entry {
        id: EntryId("Expr.str.strip_chars_start"),
        family: Family::Text,
        accepts: STRING,
        parameters: &[CHARACTERS],
        laziness: Laziness::Lazy,
        build: Build::Expression(str_strip_chars_start),
    },
    Entry {
        id: EntryId("Expr.str.strip_prefix"),
        family: Family::Text,
        accepts: STRING,
        parameters: &[required("prefix", EXPRESSION, Shape::One)],
        laziness: Laziness::Lazy,
        build: Build::Expression(str_strip_prefix),
    },
    Entry {
        id: EntryId("Expr.str.strip_suffix"),
        family: Family::Text,
        accepts: STRING,
        parameters: &[required("suffix", EXPRESSION, Shape::One)],
        laziness: Laziness::Lazy,
        build: Build::Expression(str_strip_suffix),
    },
    Entry {
        id: EntryId("Expr.str.tail"),
        family: Family::Pieces,
        accepts: STRING,
        parameters: &[characters_counted("n")],
        laziness: Laziness::Lazy,
        build: Build::Expression(str_tail),
    },
    Entry {
        id: EntryId("Expr.str.to_lowercase"),
        family: Family::Text,
        accepts: STRING,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(str_to_lowercase),
    },
    Entry {
        id: EntryId("Expr.str.to_titlecase"),
        family: Family::Text,
        accepts: STRING,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(str_to_titlecase),
    },
    Entry {
        id: EntryId("Expr.str.to_uppercase"),
        family: Family::Text,
        accepts: STRING,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(str_to_uppercase),
    },
    Entry {
        id: EntryId("Expr.str.zfill"),
        family: Family::Text,
        accepts: STRING,
        parameters: &[a_length("length")],
        laziness: Laziness::Lazy,
        build: Build::Expression(str_zfill),
    },
    Entry {
        id: EntryId("Expr.sub"),
        family: Family::Arithmetic,
        accepts: SUBTRACTABLE,
        parameters: &[OTHER],
        laziness: Laziness::Lazy,
        build: Build::Expression(sub),
    },
    Entry {
        id: EntryId("Expr.sum"),
        family: Family::Aggregate,
        accepts: CUM_SUM,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(sum),
    },
    Entry {
        id: EntryId("Expr.tail"),
        family: Family::Rows,
        accepts: ANY,
        parameters: &[optional("n", Kind::Expression(INTEGER), Shape::One, "10")
            .with_role(Role::Count)
            .not_broadcasting()],
        laziness: Laziness::Lazy,
        build: Build::Expression(tail),
    },
    Entry {
        id: EntryId("Expr.tan"),
        family: Family::Trigonometry,
        accepts: NUMERIC,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(tan),
    },
    Entry {
        id: EntryId("Expr.tanh"),
        family: Family::Trigonometry,
        accepts: NUMERIC,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(tanh),
    },
    Entry {
        id: EntryId("Expr.top_k"),
        family: Family::Rows,
        accepts: ANY,
        parameters: &[optional("k", Kind::Expression(INTEGER), Shape::One, "5")
            .with_role(Role::Count)
            .not_broadcasting()
            .reading_one_value(OneValue::SingleRow)],
        laziness: Laziness::Lazy,
        build: Build::Expression(top_k),
    },
    Entry {
        id: EntryId("Expr.truediv"),
        family: Family::Arithmetic,
        accepts: SCALABLE,
        parameters: &[DIVISOR],
        laziness: Laziness::Lazy,
        build: Build::Expression(truediv),
    },
    Entry {
        id: EntryId("Expr.truncate"),
        family: Family::Rounding,
        accepts: NUMERIC,
        parameters: &[DECIMALS],
        laziness: Laziness::Lazy,
        build: Build::Expression(truncate),
    },
    Entry {
        id: EntryId("Expr.unique"),
        family: Family::Rows,
        accepts: ANY,
        parameters: &[flag("maintain_order", "False").keyword_only()],
        laziness: Laziness::Lazy,
        build: Build::Expression(unique),
    },
    Entry {
        id: EntryId("Expr.unique_counts"),
        family: Family::Rows,
        accepts: ANY,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(unique_counts),
    },
    Entry {
        id: EntryId("Expr.var"),
        family: Family::Aggregate,
        accepts: NUMERIC_OR_BOOLEAN,
        parameters: &[DDOF],
        laziness: Laziness::Lazy,
        build: Build::Expression(variance),
    },
    Entry {
        id: EntryId("Expr.xor"),
        family: Family::Logic,
        accepts: BITWISE,
        parameters: &[OTHER],
        laziness: Laziness::Lazy,
        build: Build::Expression(xor),
    },
    Entry {
        id: EntryId("LazyFrame.drop"),
        family: Family::Steps,
        accepts: &[],
        parameters: &[
            starred("columns", Kind::Column),
            flag("strict", "True").keyword_only(),
        ],
        laziness: Laziness::Lazy,
        build: Build::Frame(frame_drop),
    },
    Entry {
        id: EntryId("LazyFrame.drop_nans"),
        family: Family::Steps,
        accepts: &[],
        parameters: &[optional("subset", Kind::Column, Shape::One, "None")],
        laziness: Laziness::Lazy,
        build: Build::Frame(frame_drop_nans),
    },
    Entry {
        id: EntryId("LazyFrame.drop_nulls"),
        family: Family::Steps,
        accepts: &[],
        parameters: &[optional("subset", Kind::Column, Shape::One, "None")],
        laziness: Laziness::Lazy,
        build: Build::Frame(frame_drop_nulls),
    },
    Entry {
        id: EntryId("LazyFrame.fill_nan"),
        family: Family::Steps,
        accepts: &[],
        parameters: &[required("value", Kind::Value(ValueType::Any), Shape::One)],
        laziness: Laziness::Lazy,
        build: Build::Frame(frame_fill_nan),
    },
    Entry {
        id: EntryId("LazyFrame.filter"),
        family: Family::Steps,
        accepts: &[],
        parameters: &[PREDICATES, CONSTRAINTS],
        laziness: Laziness::Lazy,
        build: Build::Frame(filter),
    },
    Entry {
        id: EntryId("LazyFrame.first"),
        family: Family::Steps,
        accepts: &[],
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Frame(frame_first),
    },
    Entry {
        id: EntryId("LazyFrame.gather_every"),
        family: Family::Steps,
        accepts: &[],
        parameters: &[
            required("n", Kind::Value(ValueType::Integer), Shape::One),
            optional("offset", Kind::Value(ValueType::Integer), Shape::One, "0"),
        ],
        laziness: Laziness::Lazy,
        build: Build::Frame(frame_gather_every),
    },
    Entry {
        id: EntryId("LazyFrame.head"),
        family: Family::Steps,
        accepts: &[],
        parameters: &[optional(
            "n",
            Kind::Value(ValueType::Integer),
            Shape::One,
            "5",
        )],
        laziness: Laziness::Lazy,
        build: Build::Frame(frame_head),
    },
    Entry {
        id: EntryId("LazyFrame.join"),
        family: Family::Steps,
        accepts: &[],
        parameters: &[
            required("other", Kind::Table, Shape::One),
            optional("on", EXPRESSION, Shape::Many, "None"),
            optional("how", Kind::Choice(HOW), Shape::One, "'inner'"),
            optional("left_on", EXPRESSION, Shape::Many, "None").keyword_only(),
            optional("right_on", EXPRESSION, Shape::Many, "None").keyword_only(),
            optional(
                "suffix",
                Kind::Value(ValueType::String),
                Shape::One,
                "'_right'",
            )
            .keyword_only(),
            optional("validate", Kind::Choice(VALIDATE), Shape::One, "'m:m'").keyword_only(),
            flag("nulls_equal", "False").keyword_only(),
            flag("coalesce", "None").keyword_only(),
            optional(
                "maintain_order",
                Kind::Choice(MAINTAIN_ORDER),
                Shape::One,
                "None",
            )
            .keyword_only(),
            optional("build_side", Kind::Choice(BUILD_SIDE), Shape::One, "'auto'").keyword_only(),
            flag("allow_parallel", "True").keyword_only(),
            flag("force_parallel", "False").keyword_only(),
        ],
        laziness: Laziness::Lazy,
        build: Build::Frame(join),
    },
    Entry {
        id: EntryId("LazyFrame.last"),
        family: Family::Steps,
        accepts: &[],
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Frame(frame_last),
    },
    Entry {
        id: EntryId("LazyFrame.limit"),
        family: Family::Steps,
        accepts: &[],
        parameters: &[optional(
            "n",
            Kind::Value(ValueType::Integer),
            Shape::One,
            "5",
        )],
        laziness: Laziness::Lazy,
        build: Build::Frame(frame_head),
    },
    Entry {
        id: EntryId("LazyFrame.pivot"),
        family: Family::Steps,
        accepts: &[],
        parameters: &[
            required("on", Kind::Column, Shape::One),
            // A table of the values to make columns for, not a Python series or data frame.
            required("on_columns", Kind::Table, Shape::One),
            optional("index", Kind::Column, Shape::One, "None").keyword_only(),
            optional("values", Kind::Column, Shape::One, "None").keyword_only(),
            optional("aggregate_function", EXPRESSION, Shape::One, "None").keyword_only(),
            flag("maintain_order", "False").keyword_only(),
            optional(
                "separator",
                Kind::Value(ValueType::String),
                Shape::One,
                "'_'",
            )
            .keyword_only(),
            optional(
                "column_naming",
                Kind::Choice(COLUMN_NAMING),
                Shape::One,
                "'auto'",
            )
            .keyword_only(),
        ],
        // Building the step collects the table given for `on_columns`, which runs its query.
        laziness: Laziness::NeedsCollect,
        build: Build::Frame(pivot),
    },
    Entry {
        id: EntryId("LazyFrame.rename"),
        family: Family::Steps,
        accepts: &[],
        parameters: &[
            // Python also takes a function from the old name to the new; a step holds none.
            required(
                "mapping",
                Kind::Pairs(&Kind::Column, &Kind::Value(ValueType::String)),
                Shape::One,
            ),
            flag("strict", "True").keyword_only(),
        ],
        laziness: Laziness::Lazy,
        build: Build::Frame(rename),
    },
    Entry {
        id: EntryId("LazyFrame.reverse"),
        family: Family::Steps,
        accepts: &[],
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Frame(frame_reverse),
    },
    Entry {
        id: EntryId("LazyFrame.select"),
        family: Family::Steps,
        accepts: &[],
        parameters: &[
            starred("exprs", EXPRESSION).writing(),
            double_starred("named_exprs").writing(),
        ],
        laziness: Laziness::Lazy,
        build: Build::Frame(select),
    },
    Entry {
        id: EntryId("LazyFrame.slice"),
        family: Family::Steps,
        accepts: &[],
        parameters: &[
            required("offset", Kind::Value(ValueType::Integer), Shape::One),
            optional(
                "length",
                Kind::Value(ValueType::Integer),
                Shape::One,
                "None",
            ),
        ],
        laziness: Laziness::Lazy,
        build: Build::Frame(frame_slice),
    },
    Entry {
        id: EntryId("LazyFrame.sort"),
        family: Family::Steps,
        accepts: &[],
        parameters: &[
            required("by", EXPRESSION, Shape::Many),
            starred("more_by", EXPRESSION),
            optional("descending", Kind::Flag, Shape::PerItem("by"), "False").keyword_only(),
            optional("nulls_last", Kind::Flag, Shape::PerItem("by"), "False").keyword_only(),
            flag("maintain_order", "False").keyword_only(),
            flag("multithreaded", "True").keyword_only(),
        ],
        laziness: Laziness::Lazy,
        build: Build::Frame(sort),
    },
    Entry {
        id: EntryId("LazyFrame.tail"),
        family: Family::Steps,
        accepts: &[],
        parameters: &[optional(
            "n",
            Kind::Value(ValueType::Integer),
            Shape::One,
            "5",
        )],
        laziness: Laziness::Lazy,
        build: Build::Frame(frame_tail),
    },
    Entry {
        id: EntryId("LazyFrame.with_columns"),
        family: Family::Steps,
        accepts: &[],
        parameters: &[
            starred("exprs", EXPRESSION).writing(),
            double_starred("named_exprs").writing(),
        ],
        laziness: Laziness::Lazy,
        build: Build::Frame(with_columns),
    },
    Entry {
        id: EntryId("LazyFrame.with_row_index"),
        family: Family::Steps,
        accepts: &[],
        parameters: &[
            optional(
                "name",
                Kind::Value(ValueType::String),
                Shape::One,
                "'index'",
            ),
            optional("offset", Kind::Value(ValueType::Integer), Shape::One, "0"),
        ],
        laziness: Laziness::Lazy,
        build: Build::Frame(frame_with_row_index),
    },
    Entry {
        id: EntryId("Then.otherwise"),
        family: Family::Condition,
        accepts: &[],
        parameters: &[STATEMENT],
        laziness: Laziness::Lazy,
        build: Build::ConditionEnd(then_otherwise),
    },
    Entry {
        id: EntryId("Then.when"),
        family: Family::Condition,
        accepts: &[],
        parameters: &[PREDICATES, CONSTRAINTS],
        laziness: Laziness::Lazy,
        build: Build::ConditionStep(then_when),
    },
    Entry {
        id: EntryId("When.then"),
        family: Family::Condition,
        accepts: &[],
        parameters: &[STATEMENT],
        laziness: Laziness::Lazy,
        build: Build::ConditionStep(when_then),
    },
    Entry {
        id: EntryId("polars.col"),
        family: Family::Start,
        accepts: &[],
        parameters: &[
            required("name", Kind::Column, Shape::One),
            starred("more_names", Kind::Column),
        ],
        laziness: Laziness::Lazy,
        build: Build::Function(polars_col),
    },
    Entry {
        id: EntryId("polars.date"),
        family: Family::Start,
        accepts: &[],
        parameters: &[
            required("year", EXPRESSION, Shape::One),
            required("month", EXPRESSION, Shape::One),
            required("day", EXPRESSION, Shape::One),
        ],
        laziness: Laziness::Lazy,
        build: Build::Function(polars_date),
    },
    Entry {
        id: EntryId("polars.datetime"),
        family: Family::Start,
        accepts: &[],
        parameters: &[
            required("year", EXPRESSION, Shape::One),
            required("month", EXPRESSION, Shape::One),
            required("day", EXPRESSION, Shape::One),
            optional("hour", EXPRESSION, Shape::One, "None"),
            optional("minute", EXPRESSION, Shape::One, "None"),
            optional("second", EXPRESSION, Shape::One, "None"),
            optional("microsecond", EXPRESSION, Shape::One, "None"),
            optional("time_unit", Kind::Choice(TIME_UNIT), Shape::One, "'us'").keyword_only(),
            optional("time_zone", Kind::TimeZone, Shape::One, "None")
                .keyword_only()
                .or_no_time_zone(),
            optional("ambiguous", Kind::Choice(AMBIGUOUS), Shape::One, "'raise'").keyword_only(),
        ],
        laziness: Laziness::Lazy,
        build: Build::Function(polars_datetime),
    },
    Entry {
        id: EntryId("polars.duration"),
        family: Family::Start,
        accepts: &[],
        parameters: &[
            optional("weeks", EXPRESSION, Shape::One, "None").keyword_only(),
            optional("days", EXPRESSION, Shape::One, "None").keyword_only(),
            optional("hours", EXPRESSION, Shape::One, "None").keyword_only(),
            optional("minutes", EXPRESSION, Shape::One, "None").keyword_only(),
            optional("seconds", EXPRESSION, Shape::One, "None").keyword_only(),
            optional("milliseconds", EXPRESSION, Shape::One, "None").keyword_only(),
            optional("microseconds", EXPRESSION, Shape::One, "None").keyword_only(),
            optional("nanoseconds", EXPRESSION, Shape::One, "None").keyword_only(),
            optional("time_unit", Kind::Choice(TIME_UNIT), Shape::One, "None").keyword_only(),
        ],
        laziness: Laziness::Lazy,
        build: Build::Function(polars_duration),
    },
    Entry {
        id: EntryId("polars.element"),
        family: Family::Start,
        accepts: &[],
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Function(polars_element),
    },
    Entry {
        id: EntryId("polars.from_epoch"),
        family: Family::Start,
        accepts: &[],
        parameters: &[
            required("column", EXPRESSION, Shape::One),
            optional("time_unit", Kind::Choice(EPOCH_UNIT), Shape::One, "'s'"),
        ],
        laziness: Laziness::Lazy,
        build: Build::Function(polars_from_epoch),
    },
    Entry {
        id: EntryId("polars.lit"),
        family: Family::Start,
        accepts: &[],
        parameters: &[
            required("value", Kind::Value(ValueType::Any), Shape::One),
            optional("dtype", Kind::Dtype, Shape::One, "None"),
            flag("allow_object", "False").keyword_only(),
        ],
        laziness: Laziness::Lazy,
        build: Build::Function(polars_lit),
    },
    Entry {
        id: EntryId("polars.time"),
        family: Family::Start,
        accepts: &[],
        parameters: &[
            optional("hour", EXPRESSION, Shape::One, "None"),
            optional("minute", EXPRESSION, Shape::One, "None"),
            optional("second", EXPRESSION, Shape::One, "None"),
            optional("microsecond", EXPRESSION, Shape::One, "None"),
        ],
        laziness: Laziness::Lazy,
        build: Build::Function(polars_time),
    },
    Entry {
        id: EntryId("polars.when"),
        family: Family::Condition,
        accepts: &[],
        parameters: &[PREDICATES, CONSTRAINTS],
        laziness: Laziness::Lazy,
        build: Build::Condition(polars_when),
    },
];
