//! The axes the catalogue and the tables of allowed combinations give, so that no grid lists a
//! method, a parameter or a call by hand: what a grid leaves out of one is named on the axis, with
//! why.

use dageist_engine::allowed::{self, After, Chosen, Context, Outer, Place, Representative, Shape};
use dageist_engine::catalog::{self, Applies, Entry, Kind, Parameter, Role, ValueType};
use dageist_fuzz::grid::{Choices, Scale};
use serde_json::Value;

use crate::doc;
use crate::literals::Written;
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
/// length, a value that fills, the values a replacement lists. A grid of normal scale keeps the
/// first parameter of each role and family, the large one every one.
#[must_use]
pub fn parameters_read_by_role(name: &'static str, scale: Scale) -> Choices<Param> {
    expression_parameters(name)
        .keep_unless(|_, param| {
            (param.role() == Role::Plain)
                .then(|| "the walk reads no literal by the role of a plain parameter".to_owned())
        })
        .one_of_each(scale, "role and family", |_, param| {
            (format!("{:?}", param.role()), param.entry.family())
        })
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

/// The methods of an expression that are given nothing they need, which a chain may follow a
/// value with, each with what the tables say it does after `class`, as an axis named `name`. A
/// method that needs a parameter is left out: a chain gives it nothing.
#[must_use]
pub fn calls_on(name: &'static str, class: Representative) -> Choices<(&'static Entry, Effect)> {
    entries(name, Applies::Expression)
        .keep_unless(|_, entry| {
            entry
                .parameters()
                .iter()
                .find(|parameter| parameter.is_needed())
                .map(|parameter| {
                    format!(
                        "it needs {}, which a chain gives it none of",
                        parameter.name()
                    )
                })
        })
        .map(|entry| (entry, effect(entry, class)))
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
