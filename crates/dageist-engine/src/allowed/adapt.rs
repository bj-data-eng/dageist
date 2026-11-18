//! A number typed without a dtype, and the dtype Polars gives it beside another
//! operand.
//!
//! Such a number is a dyn literal. Alone it is an `Int32`, an `Int64` where it is
//! too large for one, or a `Float64`, but beside a column it adapts to the dtype
//! of the column where it fits it, so `u8 - 1` is a `UInt8`. The tables are measured
//! with typed columns, so they say what a call gives only for an operand of a dtype,
//! and this module says which dtype that is for such a number.

use crate::catalog::{Entry, ParameterDefault, Role};

use super::dtype::Representative;
use super::literal::Literal;

/// A number typed without a dtype.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Loose(Number);

/// What a number typed without a dtype is, apart from its dtype.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Number {
    /// A whole number, by its value.
    Integer(i64),
    /// A number with a fraction.
    Float,
}

impl Loose {
    /// The number a literal is, when it is one typed without a dtype. Every
    /// reading of one is made here.
    pub(crate) fn of(literal: &Literal) -> Option<Self> {
        match literal {
            Literal::Integer(value) => Some(Self(Number::Integer(*value))),
            Literal::Float(_) => Some(Self(Number::Float)),
            Literal::String(_) => None,
        }
    }

    /// Whether it is a whole number.
    pub(crate) const fn is_whole(self) -> bool {
        matches!(self.0, Number::Integer(_))
    }

    /// Whether the number fits `class`, as Polars adapts a number typed without a
    /// dtype to a column of it: a whole number to a whole number type it fits, and to
    /// a float, and a number with a fraction to a float.
    pub(crate) fn fits(self, class: Representative) -> bool {
        match self.0 {
            Number::Integer(value) => integer_fits(value, class),
            Number::Float => is_float(class),
        }
    }

    /// The dtype the number has beside an operand of the class `other`, when the
    /// tables can say it: the class itself where the number fits it, and for a
    /// number with a fraction beside whole numbers the `Float64` it is alone, which
    /// Polars gives such a pair. Polars widens the column for a whole number that
    /// does not fit it, to a dtype that depends on the number, and a number beside
    /// what is no number is no number of that dtype, so there the dtype is not
    /// known.
    pub(crate) fn beside(self, other: Representative) -> Option<Representative> {
        if self.fits(other) {
            return Some(other);
        }
        match self.0 {
            Number::Float if is_integer(other) => Some(Representative::Float64),
            _ => None,
        }
    }
}

/// Whether the whole number `value` fits the class, as Polars adapts a number
/// typed without a dtype to a column of it.
fn integer_fits(value: i64, class: Representative) -> bool {
    match class {
        Representative::Int8 => i8::try_from(value).is_ok(),
        Representative::Int16 => i16::try_from(value).is_ok(),
        Representative::Int32 => i32::try_from(value).is_ok(),
        Representative::Int64 | Representative::Int128 => true,
        Representative::UInt8 => u8::try_from(value).is_ok(),
        Representative::UInt16 => u16::try_from(value).is_ok(),
        Representative::UInt32 => u32::try_from(value).is_ok(),
        Representative::UInt64 | Representative::UInt128 => value >= 0,
        Representative::Float16 => value.unsigned_abs() <= 65_504,
        Representative::Float32 | Representative::Float64 => true,
        _ => false,
    }
}

/// Whether the class is a number with a fraction.
const fn is_float(class: Representative) -> bool {
    matches!(
        class,
        Representative::Float16 | Representative::Float32 | Representative::Float64
    )
}

/// Whether the class is a whole number type.
const fn is_integer(class: Representative) -> bool {
    matches!(
        class,
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

/// Whether a number typed without a dtype could be a column of this class: a whole number
/// or float dtype, or a `Decimal`, which it adapts to.
pub(crate) const fn could_be_taken_by_a_number(class: Representative) -> bool {
    is_integer(class) || is_float(class) || matches!(class, Representative::Decimal)
}

/// How the parameter `name` of `entry` meets the value its method follows, as the
/// catalogue declares it.
fn role(entry: &Entry, name: &str) -> Option<Role> {
    entry
        .parameters()
        .iter()
        .find(|parameter| parameter.name() == name)
        .map(crate::catalog::Parameter::role)
}

/// Whether the parameter `name` of `entry` counts or indexes, which a whole
/// number is the dtype of whatever it adapts to: `shift`'s rows.
pub(crate) fn is_count(entry: &Entry, name: &str) -> bool {
    role(entry, name) == Some(Role::Count)
}

/// Whether Polars adapts a number typed without a dtype that is the parameter
/// `name` of the method `entry` to what the method follows, as it does the operand
/// of arithmetic and of a comparison, and the value `shift` fills with. A method
/// takes other expressions that do not meet what it follows as an operand does: a
/// key to group or order by, or the duration to offset a date by.
pub(crate) fn operand_adapts(entry: &Entry, name: &str) -> bool {
    matches!(role(entry, name), Some(Role::Operand | Role::Filler))
}

/// Whether Polars adapts a number that the method `entry` follows to the operand
/// `name` it is given: the other side of arithmetic and of a comparison. It does not
/// for the value `shift` fills with, which stands in for what the shift leaves
/// missing and is not combined with the receiver.
pub(crate) fn receiver_adapts(entry: &Entry, name: &str) -> bool {
    role(entry, name) == Some(Role::Operand)
}

/// Whether the parameter `name` of `entry` must be given.
pub(crate) fn is_required(entry: &Entry, name: &str) -> bool {
    entry.parameters().iter().any(|parameter| {
        parameter.name() == name && parameter.default() == ParameterDefault::Required
    })
}
