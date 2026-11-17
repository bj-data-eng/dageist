//! Polars dtypes as a document spells them.
//!
//! A dtype is written as Python Polars prints it: `Int64`, `String`,
//! `Datetime(time_unit='us', time_zone=None)`, `Duration(time_unit='ms')`,
//! `List(Int64)`. There is one spelling for each dtype, so what is read is
//! what is written back. The dtypes are the integers, the floats, `String`,
//! `Boolean`, `Date`, `Time`, `Null`, `Datetime`, `Duration` and `List` of any
//! of them, nested up to 32 deep.

use std::fmt;
use std::str::FromStr;

use polars::prelude::{DataType, PolarsError, TimeUnit, TimeZone};

/// How many `List` a dtype may nest, as in `List(List(Int64))`, which is two.
/// A text with more is no dtype of the document.
const MAX_LIST_NESTING: usize = 32;

/// A Polars dtype the document can hold.
///
/// Every value reads back, from its text, as itself: the dtype is one of those
/// above, and a time zone is written as Polars spells it.
#[derive(Debug, Clone, PartialEq)]
pub struct Dtype(DataType);

/// Why a dtype was refused.
#[derive(Debug)]
pub enum DtypeError {
    /// The text names no dtype of the document.
    Unknown(String),
    /// Polars does not know the time zone.
    TimeZone {
        /// The zone as written.
        zone: String,
        /// Polars' reason.
        source: PolarsError,
    },
    /// Polars spells the time zone another way.
    ZoneNotCanonical {
        /// The zone as written.
        written: String,
        /// The zone as Polars spells it.
        canonical: String,
    },
    /// A Polars dtype the document cannot hold.
    Unsupported(DataType),
}

impl fmt::Display for DtypeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown(text) => write!(formatter, "{text:?} is not a dtype"),
            Self::TimeZone { zone, source } => {
                write!(formatter, "{zone:?} is not a time zone: {source}")
            }
            Self::ZoneNotCanonical { written, canonical } => write!(
                formatter,
                "the time zone {written:?} is written {canonical:?}"
            ),
            Self::Unsupported(dtype) => write!(formatter, "{dtype} is not a dtype of a document"),
        }
    }
}

impl std::error::Error for DtypeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::TimeZone { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl Dtype {
    /// The dtype `dtype`.
    ///
    /// # Errors
    ///
    /// When the document cannot hold it, or when it would not read back as
    /// itself.
    pub fn new(dtype: DataType) -> Result<Self, DtypeError> {
        match text(&dtype, 0) {
            Some(written) if parse(&written, 0).is_ok_and(|read| read == dtype) => Ok(Self(dtype)),
            _ => Err(DtypeError::Unsupported(dtype)),
        }
    }

    /// The dtypes a document names by one word, for a list that offers them:
    /// those with no parameters, in the order a list gives them. `Datetime`,
    /// `Duration` and `List` have parameters to be set, and are not among them.
    #[must_use]
    pub fn listed() -> Vec<Self> {
        SIMPLE.iter().cloned().map(Self).collect()
    }

    /// The Polars dtype.
    #[must_use]
    pub const fn as_polars(&self) -> &DataType {
        &self.0
    }

    /// The Polars dtype, by value.
    #[must_use]
    pub fn into_polars(self) -> DataType {
        self.0
    }
}

impl fmt::Display for Dtype {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // `new` and `FromStr` only make dtypes that have text.
        formatter.write_str(&text(&self.0, 0).unwrap_or_default())
    }
}

impl FromStr for Dtype {
    type Err = DtypeError;

    fn from_str(written: &str) -> Result<Self, Self::Err> {
        parse(written, 0).map(Self)
    }
}

const fn unit_name(unit: TimeUnit) -> &'static str {
    match unit {
        TimeUnit::Nanoseconds => "ns",
        TimeUnit::Microseconds => "us",
        TimeUnit::Milliseconds => "ms",
    }
}

fn unit_named(name: &str) -> Option<TimeUnit> {
    match name {
        "ns" => Some(TimeUnit::Nanoseconds),
        "us" => Some(TimeUnit::Microseconds),
        "ms" => Some(TimeUnit::Milliseconds),
        _ => None,
    }
}

/// The dtypes with no parameters, in the order a list of them is given.
const SIMPLE: [DataType; 18] = [
    DataType::Boolean,
    DataType::Date,
    DataType::Time,
    DataType::Null,
    DataType::String,
    DataType::Float16,
    DataType::Float32,
    DataType::Float64,
    DataType::Int8,
    DataType::Int16,
    DataType::Int32,
    DataType::Int64,
    DataType::Int128,
    DataType::UInt8,
    DataType::UInt16,
    DataType::UInt32,
    DataType::UInt64,
    DataType::UInt128,
];

/// The dtypes with no parameters, by name.
fn simple(name: &str) -> Option<DataType> {
    Some(match name {
        "Int8" => DataType::Int8,
        "Int16" => DataType::Int16,
        "Int32" => DataType::Int32,
        "Int64" => DataType::Int64,
        "Int128" => DataType::Int128,
        "UInt8" => DataType::UInt8,
        "UInt16" => DataType::UInt16,
        "UInt32" => DataType::UInt32,
        "UInt64" => DataType::UInt64,
        "UInt128" => DataType::UInt128,
        "Float16" => DataType::Float16,
        "Float32" => DataType::Float32,
        "Float64" => DataType::Float64,
        "String" => DataType::String,
        "Boolean" => DataType::Boolean,
        "Date" => DataType::Date,
        "Time" => DataType::Time,
        "Null" => DataType::Null,
        _ => return None,
    })
}

/// The name of a dtype with no parameters.
const fn simple_name(dtype: &DataType) -> Option<&'static str> {
    Some(match dtype {
        DataType::Int8 => "Int8",
        DataType::Int16 => "Int16",
        DataType::Int32 => "Int32",
        DataType::Int64 => "Int64",
        DataType::Int128 => "Int128",
        DataType::UInt8 => "UInt8",
        DataType::UInt16 => "UInt16",
        DataType::UInt32 => "UInt32",
        DataType::UInt64 => "UInt64",
        DataType::UInt128 => "UInt128",
        DataType::Float16 => "Float16",
        DataType::Float32 => "Float32",
        DataType::Float64 => "Float64",
        DataType::String => "String",
        DataType::Boolean => "Boolean",
        DataType::Date => "Date",
        DataType::Time => "Time",
        DataType::Null => "Null",
        _ => return None,
    })
}

/// The text of `dtype`, if the document can hold it. `lists` is how many
/// `List` are open around it.
fn text(dtype: &DataType, lists: usize) -> Option<String> {
    if let Some(name) = simple_name(dtype) {
        return Some(name.to_owned());
    }
    match dtype {
        DataType::Datetime(unit, zone) => Some(format!(
            "Datetime(time_unit='{}', time_zone={})",
            unit_name(*unit),
            zone.as_ref()
                .map_or_else(|| "None".to_owned(), |zone| format!("'{zone}'")),
        )),
        DataType::Duration(unit) => Some(format!("Duration(time_unit='{}')", unit_name(*unit))),
        DataType::List(inner) if lists < MAX_LIST_NESTING => {
            text(inner, lists + 1).map(|inner| format!("List({inner})"))
        }
        _ => None,
    }
}

/// `lists` is how many `List` are open around `written`.
fn parse(written: &str, lists: usize) -> Result<DataType, DtypeError> {
    let unknown = || DtypeError::Unknown(written.to_owned());
    if let Some(dtype) = simple(written) {
        return Ok(dtype);
    }
    if let Some(inner) = written
        .strip_prefix("List(")
        .and_then(|rest| rest.strip_suffix(')'))
    {
        if lists >= MAX_LIST_NESTING {
            return Err(unknown());
        }
        return Ok(DataType::List(Box::new(parse(inner, lists + 1)?)));
    }
    if let Some(unit) = written
        .strip_prefix("Duration(time_unit='")
        .and_then(|rest| rest.strip_suffix("')"))
    {
        return unit_named(unit).map(DataType::Duration).ok_or_else(unknown);
    }
    if let Some((unit, zone)) = written
        .strip_prefix("Datetime(time_unit='")
        .and_then(|rest| rest.strip_suffix(')'))
        .and_then(|rest| rest.split_once("', time_zone="))
    {
        let unit = unit_named(unit).ok_or_else(unknown)?;
        return Ok(DataType::Datetime(unit, zone_named(zone, written)?));
    }
    Err(unknown())
}

/// The zone of a `Datetime`: `None`, or a name between single quotes.
fn zone_named(zone: &str, written: &str) -> Result<Option<TimeZone>, DtypeError> {
    if zone == "None" {
        return Ok(None);
    }
    let name = zone
        .strip_prefix('\'')
        .and_then(|rest| rest.strip_suffix('\''))
        .filter(|name| !name.contains('\''))
        .ok_or_else(|| DtypeError::Unknown(written.to_owned()))?;
    let canonical = TimeZone::opt_try_new(Some(name)).map_err(|source| DtypeError::TimeZone {
        zone: name.to_owned(),
        source,
    })?;
    match canonical {
        Some(zone) if zone.to_string() == name => Ok(Some(zone)),
        Some(zone) => Err(DtypeError::ZoneNotCanonical {
            written: name.to_owned(),
            canonical: zone.to_string(),
        }),
        None => Err(DtypeError::Unknown(written.to_owned())),
    }
}
