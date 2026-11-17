//! The dtypes the tables of allowed combinations are made for, and how a
//! result's dtype is written.

use std::fmt;

use polars::prelude::{DataType, Field, PlSmallStr, TimeUnit, TimeZone};

/// One of the representative dtypes the tables cover: every integer and float
/// width, `String`, `Boolean`, `Date`, `Time`, `Datetime` with and without a
/// time zone, `Duration`, `Null`, `List` of an integer and of `String`,
/// `Struct`, `Array` and `Decimal`.
///
/// A dtype that has parameters stands for its whole class: a `Datetime` of any
/// time unit and no zone is [`Representative::Datetime`], and a `Decimal` of
/// any precision is [`Representative::Decimal`]. Polars treats the members of
/// a class alike, and the tables were measured on one of them, named by
/// [`Representative::name`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Representative {
    /// `Int8`.
    Int8,
    /// `Int16`.
    Int16,
    /// `Int32`.
    Int32,
    /// `Int64`.
    Int64,
    /// `Int128`.
    Int128,
    /// `UInt8`.
    UInt8,
    /// `UInt16`.
    UInt16,
    /// `UInt32`.
    UInt32,
    /// `UInt64`.
    UInt64,
    /// `UInt128`.
    UInt128,
    /// `Float16`.
    Float16,
    /// `Float32`.
    Float32,
    /// `Float64`.
    Float64,
    /// `String`.
    String,
    /// `Boolean`.
    Boolean,
    /// `Date`.
    Date,
    /// `Time`.
    Time,
    /// `Datetime` without a time zone.
    Datetime,
    /// `Datetime` with a time zone.
    DatetimeZoned,
    /// `Duration`.
    Duration,
    /// `Null`.
    Null,
    /// `List` of an integer.
    ListOfInteger,
    /// `List` of `String`.
    ListOfString,
    /// `Struct`.
    Struct,
    /// `Array`.
    Array,
    /// `Decimal`.
    Decimal,
}

impl Representative {
    /// Every representative dtype, in the order the tables list them.
    pub const ALL: [Self; 26] = [
        Self::Int8,
        Self::Int16,
        Self::Int32,
        Self::Int64,
        Self::Int128,
        Self::UInt8,
        Self::UInt16,
        Self::UInt32,
        Self::UInt64,
        Self::UInt128,
        Self::Float16,
        Self::Float32,
        Self::Float64,
        Self::String,
        Self::Boolean,
        Self::Date,
        Self::Time,
        Self::Datetime,
        Self::DatetimeZoned,
        Self::Duration,
        Self::Null,
        Self::ListOfInteger,
        Self::ListOfString,
        Self::Struct,
        Self::Array,
        Self::Decimal,
    ];

    /// The dtype as the tables write it. A parameterised dtype is written as
    /// the one the tables were measured on.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Int8 => "Int8",
            Self::Int16 => "Int16",
            Self::Int32 => "Int32",
            Self::Int64 => "Int64",
            Self::Int128 => "Int128",
            Self::UInt8 => "UInt8",
            Self::UInt16 => "UInt16",
            Self::UInt32 => "UInt32",
            Self::UInt64 => "UInt64",
            Self::UInt128 => "UInt128",
            Self::Float16 => "Float16",
            Self::Float32 => "Float32",
            Self::Float64 => "Float64",
            Self::String => "String",
            Self::Boolean => "Boolean",
            Self::Date => "Date",
            Self::Time => "Time",
            Self::Datetime => "Datetime(us)",
            Self::DatetimeZoned => "Datetime(us, UTC)",
            Self::Duration => "Duration(us)",
            Self::Null => "Null",
            Self::ListOfInteger => "List(Int64)",
            Self::ListOfString => "List(String)",
            Self::Struct => "Struct(x: Int64, y: String)",
            Self::Array => "Array(Int64, 2)",
            Self::Decimal => "Decimal(10, 2)",
        }
    }

    /// The representative dtype whose name is `name`.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|dtype| dtype.name() == name)
    }

    /// The class of the dtype written `name` the way [`dtype_name`] writes it,
    /// such as `Datetime(ns)`, if it is one of the representative classes.
    #[must_use]
    pub fn of_name(name: &str) -> Option<Self> {
        class_of_name(name)
    }

    /// The class of `dtype`, if it is one of the representative classes.
    #[must_use]
    pub fn of(dtype: &DataType) -> Option<Self> {
        Some(match dtype {
            DataType::Int8 => Self::Int8,
            DataType::Int16 => Self::Int16,
            DataType::Int32 => Self::Int32,
            DataType::Int64 => Self::Int64,
            DataType::Int128 => Self::Int128,
            DataType::UInt8 => Self::UInt8,
            DataType::UInt16 => Self::UInt16,
            DataType::UInt32 => Self::UInt32,
            DataType::UInt64 => Self::UInt64,
            DataType::UInt128 => Self::UInt128,
            DataType::Float16 => Self::Float16,
            DataType::Float32 => Self::Float32,
            DataType::Float64 => Self::Float64,
            DataType::String => Self::String,
            DataType::Boolean => Self::Boolean,
            DataType::Date => Self::Date,
            DataType::Time => Self::Time,
            DataType::Datetime(_, None) => Self::Datetime,
            DataType::Datetime(_, Some(_)) => Self::DatetimeZoned,
            DataType::Duration(_) => Self::Duration,
            DataType::Null => Self::Null,
            DataType::List(inner) if inner.is_integer() => Self::ListOfInteger,
            DataType::List(inner) if **inner == DataType::String => Self::ListOfString,
            DataType::Struct(_) => Self::Struct,
            DataType::Array(..) => Self::Array,
            DataType::Decimal(..) => Self::Decimal,
            _ => return None,
        })
    }

    /// The dtype the tables were measured on for this class.
    #[must_use]
    pub fn dtype(self) -> DataType {
        let list = |inner: DataType| DataType::List(Box::new(inner));
        match self {
            Self::Int8 => DataType::Int8,
            Self::Int16 => DataType::Int16,
            Self::Int32 => DataType::Int32,
            Self::Int64 => DataType::Int64,
            Self::Int128 => DataType::Int128,
            Self::UInt8 => DataType::UInt8,
            Self::UInt16 => DataType::UInt16,
            Self::UInt32 => DataType::UInt32,
            Self::UInt64 => DataType::UInt64,
            Self::UInt128 => DataType::UInt128,
            Self::Float16 => DataType::Float16,
            Self::Float32 => DataType::Float32,
            Self::Float64 => DataType::Float64,
            Self::String => DataType::String,
            Self::Boolean => DataType::Boolean,
            Self::Date => DataType::Date,
            Self::Time => DataType::Time,
            Self::Datetime => DataType::Datetime(TimeUnit::Microseconds, None),
            Self::DatetimeZoned => DataType::Datetime(TimeUnit::Microseconds, Some(TimeZone::UTC)),
            Self::Duration => DataType::Duration(TimeUnit::Microseconds),
            Self::Null => DataType::Null,
            Self::ListOfInteger => list(DataType::Int64),
            Self::ListOfString => list(DataType::String),
            Self::Struct => DataType::Struct(vec![
                Field::new(PlSmallStr::from_static("x"), DataType::Int64),
                Field::new(PlSmallStr::from_static("y"), DataType::String),
            ]),
            Self::Array => DataType::Array(Box::new(DataType::Int64), 2),
            Self::Decimal => DataType::Decimal(10, 2),
        }
    }
}

impl fmt::Display for Representative {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

const fn unit_name(unit: TimeUnit) -> &'static str {
    match unit {
        TimeUnit::Nanoseconds => "ns",
        TimeUnit::Microseconds => "us",
        TimeUnit::Milliseconds => "ms",
    }
}

/// How the tables write `dtype`: Polars' own name, with the parameters
/// spelled out and no symbol outside ASCII, such as `Datetime(us, UTC)`,
/// `Duration(ns)`, `List(Int64)` and `Array(Int64, 2)`.
#[must_use]
pub fn dtype_name(dtype: &DataType) -> String {
    match dtype {
        DataType::Datetime(unit, None) => format!("Datetime({})", unit_name(*unit)),
        DataType::Datetime(unit, Some(zone)) => {
            format!("Datetime({}, {zone})", unit_name(*unit))
        }
        DataType::Duration(unit) => format!("Duration({})", unit_name(*unit)),
        DataType::List(inner) => format!("List({})", dtype_name(inner)),
        DataType::Array(inner, width) => format!("Array({}, {width})", dtype_name(inner)),
        DataType::Decimal(precision, scale) => format!("Decimal({precision}, {scale})"),
        DataType::Struct(fields) => {
            let fields: Vec<String> = fields
                .iter()
                .map(|field| format!("{}: {}", field.name(), dtype_name(field.dtype())))
                .collect();
            format!("Struct({})", fields.join(", "))
        }
        other => <&'static str>::from(other).to_owned(),
    }
}

/// The dtype a result is written as, and its class if it has one.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResultDtype {
    name: String,
    class: Option<Representative>,
}

impl ResultDtype {
    /// The result dtype written `name` in a table.
    pub(super) fn from_name(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            class: class_of_name(name),
        }
    }

    /// The dtype as the tables write it, such as `Duration(ns)`.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The representative class of the dtype, if it has one. Where a result
    /// has none, an expression that continues from it cannot be asked about.
    #[must_use]
    pub const fn class(&self) -> Option<Representative> {
        self.class
    }
}

impl fmt::Display for ResultDtype {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.name)
    }
}

/// The class of a dtype written as [`dtype_name`] writes it.
fn class_of_name(name: &str) -> Option<Representative> {
    if let Some(simple) = Representative::ALL
        .into_iter()
        .find(|dtype| dtype.name() == name)
    {
        return Some(simple);
    }
    if name.starts_with("Datetime(") {
        return Some(if name.contains(',') {
            Representative::DatetimeZoned
        } else {
            Representative::Datetime
        });
    }
    if name.starts_with("Duration(") {
        return Some(Representative::Duration);
    }
    if name.starts_with("Decimal(") {
        return Some(Representative::Decimal);
    }
    if name.starts_with("Struct(") {
        return Some(Representative::Struct);
    }
    if name.starts_with("Array(") {
        return Some(Representative::Array);
    }
    let inner = name.strip_prefix("List(")?.strip_suffix(')')?;
    match class_of_name(inner)? {
        Representative::String => Some(Representative::ListOfString),
        Representative::Int8
        | Representative::Int16
        | Representative::Int32
        | Representative::Int64
        | Representative::Int128
        | Representative::UInt8
        | Representative::UInt16
        | Representative::UInt32
        | Representative::UInt64
        | Representative::UInt128 => Some(Representative::ListOfInteger),
        _ => None,
    }
}
