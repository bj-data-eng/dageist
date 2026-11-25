//! The dtypes the tables of allowed combinations are made for, and how a
//! result's dtype is written.

use std::fmt;

use polars::prelude::{DataType, Field, PlSmallStr, TimeUnit, TimeZone};

/// A scalar dtype that a list or an array holds: the element of a nested class.
///
/// It is the class of a scalar dtype of the tables, so a nested class of one level is made of
/// each of these, and no list, array or struct is held by another: a nested class holds a scalar
/// and nothing deeper.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Inner {
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
    /// `Decimal`.
    Decimal,
}

impl Inner {
    /// Every scalar dtype a list or an array holds, in the order the tables list them.
    pub const ALL: [Self; 22] = [
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
        Self::Decimal,
    ];

    /// The class of the scalar dtype itself.
    #[must_use]
    pub const fn class(self) -> Representative {
        match self {
            Self::Int8 => Representative::Int8,
            Self::Int16 => Representative::Int16,
            Self::Int32 => Representative::Int32,
            Self::Int64 => Representative::Int64,
            Self::Int128 => Representative::Int128,
            Self::UInt8 => Representative::UInt8,
            Self::UInt16 => Representative::UInt16,
            Self::UInt32 => Representative::UInt32,
            Self::UInt64 => Representative::UInt64,
            Self::UInt128 => Representative::UInt128,
            Self::Float16 => Representative::Float16,
            Self::Float32 => Representative::Float32,
            Self::Float64 => Representative::Float64,
            Self::String => Representative::String,
            Self::Boolean => Representative::Boolean,
            Self::Date => Representative::Date,
            Self::Time => Representative::Time,
            Self::Datetime => Representative::Datetime,
            Self::DatetimeZoned => Representative::DatetimeZoned,
            Self::Duration => Representative::Duration,
            Self::Null => Representative::Null,
            Self::Decimal => Representative::Decimal,
        }
    }

    /// The scalar dtype the class `dtype` is, if it is one a list or an array holds.
    #[must_use]
    pub fn of_class(class: Representative) -> Option<Self> {
        Self::ALL.into_iter().find(|inner| inner.class() == class)
    }

    /// The scalar dtype of `dtype`, if it is one the tables cover.
    #[must_use]
    pub fn of(dtype: &DataType) -> Option<Self> {
        Self::of_class(Representative::of(dtype)?)
    }

    /// The name of the list of it that the tables write.
    const fn list_name(self) -> &'static str {
        match self {
            Self::Int8 => "List(Int8)",
            Self::Int16 => "List(Int16)",
            Self::Int32 => "List(Int32)",
            Self::Int64 => "List(Int64)",
            Self::Int128 => "List(Int128)",
            Self::UInt8 => "List(UInt8)",
            Self::UInt16 => "List(UInt16)",
            Self::UInt32 => "List(UInt32)",
            Self::UInt64 => "List(UInt64)",
            Self::UInt128 => "List(UInt128)",
            Self::Float16 => "List(Float16)",
            Self::Float32 => "List(Float32)",
            Self::Float64 => "List(Float64)",
            Self::String => "List(String)",
            Self::Boolean => "List(Boolean)",
            Self::Date => "List(Date)",
            Self::Time => "List(Time)",
            Self::Datetime => "List(Datetime(us))",
            Self::DatetimeZoned => "List(Datetime(us, UTC))",
            Self::Duration => "List(Duration(us))",
            Self::Null => "List(Null)",
            Self::Decimal => "List(Decimal(10, 2))",
        }
    }

    /// The name of the array of it, of the width the tables measured arrays at, that they write.
    const fn array_name(self) -> &'static str {
        match self {
            Self::Int8 => "Array(Int8, 2)",
            Self::Int16 => "Array(Int16, 2)",
            Self::Int32 => "Array(Int32, 2)",
            Self::Int64 => "Array(Int64, 2)",
            Self::Int128 => "Array(Int128, 2)",
            Self::UInt8 => "Array(UInt8, 2)",
            Self::UInt16 => "Array(UInt16, 2)",
            Self::UInt32 => "Array(UInt32, 2)",
            Self::UInt64 => "Array(UInt64, 2)",
            Self::UInt128 => "Array(UInt128, 2)",
            Self::Float16 => "Array(Float16, 2)",
            Self::Float32 => "Array(Float32, 2)",
            Self::Float64 => "Array(Float64, 2)",
            Self::String => "Array(String, 2)",
            Self::Boolean => "Array(Boolean, 2)",
            Self::Date => "Array(Date, 2)",
            Self::Time => "Array(Time, 2)",
            Self::Datetime => "Array(Datetime(us), 2)",
            Self::DatetimeZoned => "Array(Datetime(us, UTC), 2)",
            Self::Duration => "Array(Duration(us), 2)",
            Self::Null => "Array(Null, 2)",
            Self::Decimal => "Array(Decimal(10, 2), 2)",
        }
    }
}

/// The width the tables measured arrays at.
pub(super) const MEASURED_WIDTH: usize = 2;

/// One of the representative dtypes the tables cover: every integer and float width, `String`,
/// `Boolean`, `Date`, `Time`, `Datetime` with and without a time zone, `Duration`, `Null`,
/// `Decimal`, `Struct`, and a `List` and an `Array` of each of the scalar ones.
///
/// A dtype that has parameters stands for its whole class: a `Datetime` of any
/// time unit and no zone is [`Representative::Datetime`], a `Decimal` of
/// any precision is [`Representative::Decimal`], and an `Array` of any width is the array
/// of its element. Polars treats the members of a class alike, and the tables were measured on
/// one of them, named by [`Representative::name`]. A list or an array of a list, an array or a
/// struct has no class.
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
    /// `Struct`.
    Struct,
    /// `Decimal`.
    Decimal,
    /// A `List` of a scalar dtype.
    List(Inner),
    /// An `Array` of a scalar dtype, of any width.
    Array(Inner),
}

impl Representative {
    /// The classes the tables hold lines for, in the order they list them: the scalar dtypes,
    /// the list of an integer and of `String`, `Struct`, an array of integers and `Decimal`.
    /// A list or an array of another element is read as the one of these that stands for it,
    /// where an ordinary method does not tell them apart.
    pub const MEASURED: [Self; 26] = [
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
        Self::List(Inner::Int64),
        Self::List(Inner::String),
        Self::Struct,
        Self::Array(Inner::Int64),
        Self::Decimal,
    ];

    /// Every class: those the tables hold lines for first, then the list and the array of each
    /// other scalar dtype.
    pub const ALL: [Self; 67] = {
        let mut all = [Self::Int8; 67];
        let mut at = 0;
        while at < Self::MEASURED.len() {
            all[at] = Self::MEASURED[at];
            at += 1;
        }
        let mut inner = 0;
        while inner < Inner::ALL.len() {
            let element = Inner::ALL[inner];
            if !matches!(element, Inner::Int64 | Inner::String) {
                all[at] = Self::List(element);
                at += 1;
            }
            inner += 1;
        }
        let mut inner = 0;
        while inner < Inner::ALL.len() {
            let element = Inner::ALL[inner];
            if !matches!(element, Inner::Int64) {
                all[at] = Self::Array(element);
                at += 1;
            }
            inner += 1;
        }
        all
    };

    /// The dtype as the tables write it. A parameterised dtype is written as
    /// the one the tables were measured on, an array of the width they measured arrays at.
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
            Self::Struct => "Struct(x: Int64, y: String)",
            Self::Decimal => "Decimal(10, 2)",
            Self::List(inner) => inner.list_name(),
            Self::Array(inner) => inner.array_name(),
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
            DataType::List(inner) => Self::List(Inner::of(inner)?),
            DataType::Struct(_) => Self::Struct,
            DataType::Array(inner, _) => Self::Array(Inner::of(inner)?),
            DataType::Decimal(..) => Self::Decimal,
            _ => return None,
        })
    }

    /// The dtype the tables were measured on for this class.
    #[must_use]
    pub fn dtype(self) -> DataType {
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
            Self::Struct => DataType::Struct(vec![
                Field::new(PlSmallStr::from_static("x"), DataType::Int64),
                Field::new(PlSmallStr::from_static("y"), DataType::String),
            ]),
            Self::Decimal => DataType::Decimal(10, 2),
            Self::List(inner) => DataType::List(Box::new(inner.class().dtype())),
            Self::Array(inner) => DataType::Array(Box::new(inner.class().dtype()), MEASURED_WIDTH),
        }
    }

    /// The scalar dtype a list or an array of this class holds, if it is one.
    #[must_use]
    pub const fn element(self) -> Option<Inner> {
        match self {
            Self::List(inner) | Self::Array(inner) => Some(inner),
            _ => None,
        }
    }

    /// Whether this class is a list or an array.
    #[must_use]
    pub const fn is_nested(self) -> bool {
        matches!(self, Self::List(_) | Self::Array(_))
    }

    /// The class this one is read as where the tables hold no lines of its own and an ordinary
    /// method judges it as it does another: a list of text is read as the list of `String`, any
    /// other list as the list of `Int64`, and any array as the array of `Int64`. A class that has
    /// lines of its own stands for itself.
    #[must_use]
    pub const fn sibling(self) -> Self {
        match self {
            Self::List(Inner::String) => Self::List(Inner::String),
            Self::List(_) => Self::List(Inner::Int64),
            Self::Array(_) => Self::Array(Inner::Int64),
            other => other,
        }
    }
}

impl fmt::Display for Representative {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

pub(super) const fn unit_name(unit: TimeUnit) -> &'static str {
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

impl ResultDtype {
    /// The dtype written for an array of `width`, where this is an array as the tables measured
    /// it, at the width they measured arrays at; any other dtype as it is.
    #[must_use]
    pub(super) fn of_width(&self, width: usize) -> Self {
        match self.name.strip_suffix(&format!(", {MEASURED_WIDTH})")) {
            Some(inside) if matches!(self.class, Some(Representative::Array(_))) => Self {
                name: format!("{inside}, {width})"),
                class: self.class,
            },
            _ => self.clone(),
        }
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
    if let Some(inside) = name
        .strip_prefix("Array(")
        .and_then(|rest| rest.strip_suffix(')'))
    {
        let (inner, width) = inside.rsplit_once(", ")?;
        width.parse::<usize>().ok()?;
        return Some(Representative::Array(Inner::of_class(class_of_name(
            inner,
        )?)?));
    }
    let inner = name.strip_prefix("List(")?.strip_suffix(')')?;
    Some(Representative::List(Inner::of_class(class_of_name(
        inner,
    )?)?))
}
