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

use polars::frame::PivotColumnNaming;
use polars::prelude::{
    Expr, JoinBuildSide, JoinCoalesce, JoinType, JoinValidation, LazyFrame, MaintainOrderJoin,
    NULL, NamedFrom, Operator, PolarsError, Scalar, Series, SortMultipleOptions, SortOptions,
    WindowMapping, all, all_horizontal, binary_expr, col, cols, element, lit, when,
};

use super::{
    Argument, Arguments, Build, BuildError, Conditional, Dtypes, Entry, EntryId, Family, Kind,
    Laziness, Parameter, ParameterDefault, Role, Shape, ValueType, Variadic, Writes,
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
const COLUMN_NAMING: &[&str] = &["auto", "combine"];
const MAPPING_STRATEGY: &[&str] = &["group_to_rows", "join", "explode"];

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

/// The parameter `truediv` takes, which is divided by.
const DIVISOR: Parameter = required("other", EXPRESSION, Shape::One).with_role(Role::Divisor);

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

fn dt_date(expr: Expr, _: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.dt().date())
}

fn dt_time(expr: Expr, _: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.dt().time())
}

fn dt_offset_by(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.dt().offset_by(arguments.required_expression("by")?))
}

fn dt_month_start(expr: Expr, _: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.dt().month_start())
}

fn dt_total_days(expr: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    Ok(expr.dt().total_days(arguments.flag("fractional", false)?))
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
        // A number typed without a dtype stays loose, as in Python: Polars gives it the
        // dtype its other operand needs, or the narrowest it fits.
        return match value {
            Argument::Integer(value) => Ok(lit(*value)),
            Argument::Float(value) => Ok(lit(*value)),
            Argument::String(value) => Ok(lit(value.as_str())),
            Argument::Flag(Some(value)) => Ok(lit(*value)),
            Argument::Flag(None) => Ok(lit(NULL)),
            other => Err(Arguments::wrong("value", "a number, text or a flag", other)),
        };
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
        id: EntryId("Expr.cum_sum"),
        family: Family::Cumulative,
        accepts: CUM_SUM,
        parameters: &[flag("reverse", "False").keyword_only()],
        laziness: Laziness::Lazy,
        build: Build::Expression(cum_sum),
    },
    Entry {
        id: EntryId("Expr.dt.date"),
        family: Family::Temporal,
        accepts: DATE_PART,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(dt_date),
    },
    Entry {
        id: EntryId("Expr.dt.month_start"),
        family: Family::Temporal,
        accepts: DATE_PART,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(dt_month_start),
    },
    Entry {
        id: EntryId("Expr.dt.offset_by"),
        family: Family::Temporal,
        accepts: DATE_PART,
        // `str | Expr`: a duration string is the expression that holds it.
        parameters: &[required("by", Kind::Expression(STRING), Shape::One)],
        laziness: Laziness::Lazy,
        build: Build::Expression(dt_offset_by),
    },
    Entry {
        id: EntryId("Expr.dt.time"),
        family: Family::Temporal,
        accepts: TIME_PART,
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Expression(dt_time),
    },
    Entry {
        id: EntryId("Expr.dt.total_days"),
        family: Family::Temporal,
        accepts: DURATION,
        parameters: &[flag("fractional", "False").keyword_only()],
        laziness: Laziness::Lazy,
        build: Build::Expression(dt_total_days),
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
        id: EntryId("Expr.first"),
        family: Family::Rows,
        accepts: ANY,
        parameters: &[flag("ignore_nulls", "False").keyword_only()],
        laziness: Laziness::Lazy,
        build: Build::Expression(first),
    },
    Entry {
        id: EntryId("Expr.floordiv"),
        family: Family::Arithmetic,
        accepts: DIVISIBLE,
        parameters: &[OTHER],
        laziness: Laziness::Lazy,
        build: Build::Expression(floordiv),
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
        id: EntryId("Expr.mul"),
        family: Family::Arithmetic,
        accepts: SCALABLE,
        parameters: &[OTHER],
        laziness: Laziness::Lazy,
        build: Build::Expression(mul),
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
        id: EntryId("Expr.pow"),
        family: Family::Arithmetic,
        accepts: NUMERIC,
        parameters: &[required("exponent", EXPRESSION, Shape::One).with_role(Role::Exponent)],
        laziness: Laziness::Lazy,
        build: Build::Expression(pow),
    },
    Entry {
        id: EntryId("Expr.shift"),
        family: Family::Rows,
        accepts: ANY,
        parameters: &[
            optional("n", Kind::Expression(INTEGER), Shape::One, "1").with_role(Role::Count),
            optional("fill_value", EXPRESSION, Shape::One, "None")
                .keyword_only()
                .with_role(Role::Filler),
        ],
        laziness: Laziness::Lazy,
        build: Build::Expression(shift),
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
        id: EntryId("Expr.truediv"),
        family: Family::Arithmetic,
        accepts: SCALABLE,
        parameters: &[DIVISOR],
        laziness: Laziness::Lazy,
        build: Build::Expression(truediv),
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
        id: EntryId("LazyFrame.filter"),
        family: Family::Steps,
        accepts: &[],
        parameters: &[PREDICATES, CONSTRAINTS],
        laziness: Laziness::Lazy,
        build: Build::Frame(filter),
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
        id: EntryId("polars.element"),
        family: Family::Start,
        accepts: &[],
        parameters: &[],
        laziness: Laziness::Lazy,
        build: Build::Function(polars_element),
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
        id: EntryId("polars.when"),
        family: Family::Condition,
        accepts: &[],
        parameters: &[PREDICATES, CONSTRAINTS],
        laziness: Laziness::Lazy,
        build: Build::Condition(polars_when),
    },
];
