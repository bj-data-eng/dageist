//! The axes the catalogue and the tables of allowed combinations give, so that no grid lists a
//! method, a parameter or a call by hand: what a grid leaves out of one is named on the axis, with
//! why.

use dageist_engine::allowed::{self, After, Chosen, Context, Outer, Place, Representative, Shape};
use dageist_engine::catalog::{self, Applies, Entry, Kind, Parameter, Role, ValueType};
use dageist_fuzz::grid::Choices;
use serde_json::Value;

use crate::doc;
use crate::literals::{Literal, Written};
use crate::world::Column;

/// Every entry of the catalogue that applies as `applies`, labelled by its identifier.
#[must_use]
pub fn entries(name: &'static str, applies: Applies) -> Choices<&'static Entry> {
    Choices::of(
        name,
        catalog::entries()
            .iter()
            .filter(|entry| entry.applies() == applies)
            .map(|entry| (entry.id().as_str(), entry)),
    )
}

/// A parameter of an entry.
#[derive(Debug, Clone, Copy)]
pub struct Param {
    pub entry: &'static Entry,
    pub parameter: &'static Parameter,
}

impl Param {
    /// The label of the parameter: `Expr.add(other)`.
    #[must_use]
    pub fn label(&self) -> String {
        format!("{}({})", self.entry.id().as_str(), self.parameter.name())
    }

    #[must_use]
    pub fn method(&self) -> &'static str {
        self.entry.id().as_str()
    }

    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.parameter.name()
    }

    #[must_use]
    pub const fn role(&self) -> Role {
        self.parameter.role()
    }
}

/// Every parameter of a method of an expression that holds an expression, as an axis named
/// `name`.
#[must_use]
pub fn expression_parameters(name: &'static str) -> Choices<Param> {
    let mut axis = Choices::new(name);
    for entry in catalog::entries()
        .iter()
        .filter(|entry| entry.applies() == Applies::Expression)
    {
        for parameter in entry.parameters() {
            if matches!(parameter.kind(), Kind::Expression(_)) {
                let param = Param { entry, parameter };
                axis = axis.with(param.label(), param);
            }
        }
    }
    axis
}

/// The parameters of a method of an expression that the walk reads a literal of by the role it
/// stands in, which is every expression parameter whose role is not plain: what a number is added
/// to, divided by, raised to, a logarithm is taken to, a bound it is cut to, a share, a count or a
/// length, a value that fills, the values a replacement lists; and the other expression
/// parameters of the same methods, which meet the literal beside them, as the new values and the
/// default of a strict replacement do. The rest are left out, each with why.
#[must_use]
pub fn literal_parameters(name: &'static str) -> Choices<Param> {
    let reads = |entry: &Entry| {
        entry.parameters().iter().any(|parameter| {
            matches!(parameter.kind(), Kind::Expression(_)) && parameter.role() != Role::Plain
        })
    };
    expression_parameters(name).keep_unless(|_, param| {
        (!reads(param.entry))
            .then(|| "its method reads no literal by the role of a parameter".to_owned())
    })
}

/// The kind of a class a literal's dtype is grouped by where a grid of normal scale keeps one of
/// each: signed and unsigned whole numbers, floats, flags, text, moments and the rest.
#[must_use]
pub fn kind_of_class(class: Representative) -> &'static str {
    use Representative as R;
    match class {
        R::Int8 | R::Int16 | R::Int32 | R::Int64 | R::Int128 => "signed whole number",
        R::UInt8 | R::UInt16 | R::UInt32 | R::UInt64 | R::UInt128 => "unsigned whole number",
        R::Float16 | R::Float32 | R::Float64 => "float",
        R::Boolean => "flag",
        R::String => "text",
        R::Date | R::Datetime | R::DatetimeZoned | R::Time | R::Duration => "moment",
        R::Decimal => "decimal",
        R::Null => "null",
        R::Struct | R::List(_) | R::Array(_) => "nested",
    }
}

/// What a method's needed parameter is given where a grid does not vary it, so that the one a grid
/// varies is the only one that differs from cell to cell: for an expression, a literal of the kind
/// of the column it follows; for a typed value, one of its type; the first word of a choice; a
/// flag set; the column's own dtype. A list is a list of one.
///
/// # Errors
///
/// The words of why, for a parameter of a kind no grid gives a value of: a path, a table, pairs.
pub fn filler(param: &Parameter, column: Option<&Column>) -> Result<Value, String> {
    let one = match param.kind() {
        Kind::Expression(_) => {
            // The place a text is cut from is a whole number whatever the column is.
            if param.role().is_a_count() || param.name() == "offset" {
                Written::Integer(0).expression(None)
            } else if param.role() == Role::Share {
                Written::Float(0.5).expression(None)
            } else {
                companion(column).expression(None)
            }
        }
        Kind::Value(ValueType::Integer) => doc::integer(1),
        Kind::Value(ValueType::Float) => doc::float(0.5),
        Kind::Value(ValueType::String | ValueType::Any) => doc::string("a"),
        Kind::Value(ValueType::Date) => doc::string("2020-01-01"),
        Kind::Choice(words) => doc::choice(words.first().ok_or("a choice of no word")?),
        Kind::Flag => doc::flag(true),
        Kind::Pattern => doc::pattern("a"),
        Kind::Duration => serde_json::json!({ "duration": "1d" }),
        Kind::TimeZone => serde_json::json!({ "time_zone": "UTC" }),
        Kind::Dtype => match column {
            Some(column) => doc::dtype(
                &dageist_engine::document::Dtype::new(column.dtype.clone())
                    .map_err(|error| error.to_string())?
                    .to_string(),
            ),
            None => doc::dtype("Int64"),
        },
        Kind::Column => doc::columns(&[column.map_or("n", |column| column.name.as_str())]),
        Kind::Field => serde_json::json!({ "field": "x" }),
        Kind::Table | Kind::Path | Kind::Pairs(..) => {
            return Err(format!(
                "{} is of a kind no grid gives a value of",
                param.name()
            ));
        }
    };
    Ok(match param.shape() {
        catalog::Shape::Many => doc::many(vec![one]),
        catalog::Shape::One | catalog::Shape::PerItem(_) => one,
    })
}

/// The arguments of `param`'s method where the parameter is given `varied`: each other expression
/// parameter of the method is given a literal of the kind of the column, and each other needed
/// parameter what [`filler`] gives it.
///
/// # Errors
///
/// When a needed parameter is of a kind no grid gives a value of.
pub fn arguments(param: &Param, varied: &Value, column: &Column) -> Result<Value, String> {
    let mut arguments = serde_json::Map::new();
    for parameter in param.entry.parameters() {
        let value = if parameter.name() == param.name() {
            match parameter.shape() {
                catalog::Shape::Many => doc::many(vec![varied.clone()]),
                catalog::Shape::One | catalog::Shape::PerItem(_) => varied.clone(),
            }
        } else if matches!(parameter.kind(), Kind::Expression(_)) || parameter.is_needed() {
            filler(parameter, Some(column))?
        } else {
            continue;
        };
        arguments.insert(parameter.name().to_owned(), value);
    }
    Ok(Value::Object(arguments))
}

/// The expression of `param`'s method given `literal`: with the column first, the method following
/// the column and given the literal as the parameter; or with the literal first, the method
/// following the literal and given the column.
///
/// # Errors
///
/// When a needed parameter is of a kind no grid gives a value of, and for a text padded to a length
/// of 2^24 characters or more, which Polars does not end making within a cell's deadline.
pub fn method_given(
    param: &Param,
    literal: &Literal,
    column: &Column,
    literal_first: bool,
) -> Result<Value, String> {
    // A text padded to millions of characters for each row takes Polars longer than a cell may.
    if param.role() == Role::Length
        && param.entry.family() == catalog::Family::Text
        && literal.written().is_at_least(f64::from(1_u32 << 24))
    {
        return Err("Polars never ends: it pads a text to the length it is given".to_owned());
    }
    Ok(if literal_first {
        doc::chain(
            literal.call(),
            vec![doc::call(
                param.method(),
                arguments(param, &doc::column(&column.name), column)?,
            )],
        )
    } else {
        doc::column_then(
            &column.name,
            vec![doc::call(
                param.method(),
                arguments(param, &literal.expression(), column)?,
            )],
        )
    })
}

/// A literal of the kind of the values of `column`: a flag for flags, a text of a number for
/// text, a number with a fraction for floats, and a whole number for the rest.
#[must_use]
pub fn companion(column: Option<&Column>) -> Written {
    use polars::prelude::DataType;
    match column.map(|column| &column.dtype) {
        Some(DataType::Boolean) => Written::Flag(true),
        Some(DataType::String) => Written::Text("1".to_owned()),
        Some(DataType::Float16 | DataType::Float32 | DataType::Float64) => Written::Float(1.0),
        _ => Written::Integer(1),
    }
}

/// The two orders of a literal and the column it meets: the column first, the method following
/// it and given the literal; or the literal first, the method following it and given the column.
#[must_use]
pub fn orders(name: &'static str) -> Choices<bool> {
    Choices::of(
        name,
        [("the column first", false), ("the literal first", true)],
    )
}

/// What the tables say a call does with the values of a class it follows, in a select.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Effect {
    /// It gives back the dtype it follows, and as many values.
    Keeps,
    /// It gives back the dtype it follows, and one value of the many it follows.
    Aggregates,
    /// It gives another dtype, and as many values.
    Changes,
    /// It gives another dtype, and one value.
    ChangesToOne,
    /// It gives another number of values than one or as many.
    Counts,
    /// The tables refuse it.
    Refused,
    /// The tables cannot tell what it gives.
    NotKnown,
}

/// The class the tables say `entry` gives after a column of `class` in a select, where they allow
/// it and know it.
#[must_use]
pub fn gives_after(entry: &'static Entry, class: Representative) -> Option<Representative> {
    let place = Place::new(
        Context::step(Outer::Select),
        After::Value {
            dtype: class,
            shape: Shape::Many,
        },
    );
    match allowed::offer(entry, &place, &Chosen::default()) {
        allowed::Verdict::Allowed(allowance) => {
            allowance.result().and_then(allowed::ResultDtype::class)
        }
        allowed::Verdict::Refused(_) => None,
    }
}

/// What the tables say `entry`, given what [`filler`] gives its needed parameters, does after a
/// column of `class` in a select.
#[must_use]
pub fn effect(entry: &'static Entry, class: Representative) -> Effect {
    let place = Place::new(
        Context::step(Outer::Select),
        After::Value {
            dtype: class,
            shape: Shape::Many,
        },
    );
    match allowed::offer(entry, &place, &Chosen::default()) {
        allowed::Verdict::Allowed(allowance) => {
            let given = allowance.result().and_then(allowed::ResultDtype::class);
            match (given, allowance.shape()) {
                (None, _) => Effect::NotKnown,
                (Some(given), Shape::Many) if given == class => Effect::Keeps,
                (Some(given), Shape::One) if given == class => Effect::Aggregates,
                (Some(_), Shape::Many) => Effect::Changes,
                (Some(_), Shape::One) => Effect::ChangesToOne,
                (Some(_), Shape::Counted) => Effect::Counts,
            }
        }
        allowed::Verdict::Refused(_) => Effect::Refused,
    }
}

/// The methods of an expression, which a chain may follow a value with, each with what the tables
/// say it does after `class`, as an axis named `name`. A method that needs a parameter of a kind
/// [`filler`] gives no value of is left out.
#[must_use]
pub fn calls_on(name: &'static str, class: Representative) -> Choices<(&'static Entry, Effect)> {
    entries(name, Applies::Expression)
        .keep_unless(|_, entry| call_of(entry, None).err())
        .map(|entry| (entry, effect(entry, class)))
}

/// The call of `entry`, each needed parameter given what [`filler`] gives it beside `column`.
///
/// # Errors
///
/// When a needed parameter is of a kind no grid gives a value of.
pub fn call_of(entry: &'static Entry, column: Option<&Column>) -> Result<Value, String> {
    let mut arguments = serde_json::Map::new();
    for parameter in entry
        .parameters()
        .iter()
        .filter(|parameter| parameter.is_needed())
    {
        arguments.insert(parameter.name().to_owned(), filler(parameter, column)?);
    }
    Ok(doc::call(entry.id().as_str(), Value::Object(arguments)))
}

/// The calls of [`calls_on`] whose effect after each of `classes` is `wanted`, as an axis named
/// `name`, with each that does otherwise after one of them left out with what it does.
#[must_use]
pub fn calls_that(
    name: &'static str,
    wanted: Effect,
    classes: &[Representative],
) -> Choices<&'static Entry> {
    let first = classes.first().copied().unwrap_or(Representative::Int64);
    calls_on(name, first)
        .keep_unless(|_, (entry, _)| {
            classes.iter().find_map(|class| {
                let does = effect(entry, *class);
                (does != wanted).then(|| format!("after {class} the tables say it {does:?}"))
            })
        })
        .map(|(entry, _)| entry)
}

/// What the tables say a call given nothing does with a number of each of a few classes, as a
/// grid of an open number crosses the calls by it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Does {
    /// It gives back the class it follows, for each of them, and as many values: a call Polars may
    /// leave an open number open through.
    Keeps,
    /// It gives back the class it follows, for each of them, and one value of the many.
    Aggregates,
    /// It gives back the class it follows for some of them and another number for the rest, and as
    /// many values, as a logarithm turns whole numbers into numbers with a fraction.
    KeepsSome,
    /// It gives another number for each of them.
    Changes,
}

/// Whether `class` is a signed whole number or a float, which a call that keeps the class it
/// follows can follow in turn.
const fn is_a_signed_number(class: Representative) -> bool {
    matches!(
        class,
        Representative::Int8
            | Representative::Int16
            | Representative::Int32
            | Representative::Int64
            | Representative::Int128
            | Representative::Float16
            | Representative::Float32
            | Representative::Float64
    )
}

/// What the tables say `entry`, given nothing, does after each of `classes` in a select, or why it
/// is none of the four: a class refuses it, or it gives what is no signed number.
///
/// # Errors
///
/// The words of why the call does none of the four.
pub fn does(entry: &'static Entry, classes: &[Representative]) -> Result<Does, String> {
    let mut seen = Vec::with_capacity(classes.len());
    for class in classes {
        let place = Place::new(
            Context::step(Outer::Select),
            After::Value {
                dtype: *class,
                shape: Shape::Many,
            },
        );
        match allowed::offer(entry, &place, &Chosen::default()) {
            allowed::Verdict::Allowed(allowance) => {
                let given = allowance.result().and_then(allowed::ResultDtype::class);
                seen.push((
                    given == Some(*class),
                    allowance.shape() == Shape::One,
                    given.is_some_and(is_a_signed_number),
                ));
            }
            allowed::Verdict::Refused(_) => {
                return Err(format!("the tables refuse it after {class}"));
            }
        }
    }
    let one = seen.iter().all(|(_, one, _)| *one);
    let keeps = seen.iter().all(|(keeps, _, _)| *keeps);
    let some = seen.iter().any(|(keeps, _, _)| *keeps);
    let numbers = seen.iter().all(|(_, _, number)| *number);
    match (keeps, some, numbers) {
        (true, _, _) if one => Ok(Does::Aggregates),
        (true, _, _) => Ok(Does::Keeps),
        (false, true, true) if !one => Ok(Does::KeepsSome),
        (_, _, true) => Ok(Does::Changes),
        _ => Err("it gives what is no signed number".to_owned()),
    }
}

/// The methods of an expression that need nothing and that [`does`] says do `wanted` after each of
/// `classes`, as an axis named `name`, with every other method left out and why.
#[must_use]
pub fn calls_that_do(
    name: &'static str,
    wanted: Does,
    classes: &[Representative],
) -> Choices<&'static Entry> {
    entries(name, Applies::Expression).keep_unless(|_, entry| {
        if let Some(parameter) = entry
            .parameters()
            .iter()
            .find(|parameter| parameter.is_needed())
        {
            return Some(format!(
                "it needs {}, which a chain gives it none of",
                parameter.name()
            ));
        }
        match does(entry, classes) {
            Ok(done) if done == wanted => None,
            Ok(done) => Some(format!("it {done:?}")),
            Err(why) => Some(why),
        }
    })
}
