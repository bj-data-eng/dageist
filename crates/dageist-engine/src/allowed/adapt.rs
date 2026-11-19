//! A number typed without a dtype, and the dtype Polars gives it beside another
//! operand.
//!
//! Such a number is a dyn literal. Alone it is an `Int32`, an `Int64` where it is
//! too large for one, or a `Float64`, but beside a column it adapts to the dtype
//! of the column where it fits it, so `u8 - 1` is a `UInt8`. The tables are measured
//! with typed columns, so they say what a call gives only for an operand of a dtype,
//! and this module says which dtype that is for such a number.

use crate::catalog::{Entry, Role};

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

    /// The dtype the number has as a bound of a value of the class `other`, which is cast to it:
    /// the class itself where the number fits it, and for a number with a fraction beside whole
    /// numbers, which Polars cuts to a whole number, that class too. A whole number the class
    /// cannot hold is refused when rows run, where an operand's would widen the dtype, so the
    /// tables cannot say what it gives, and neither can they beside what is no number.
    pub(crate) fn cut_to(self, other: Representative) -> Option<Representative> {
        let cut = matches!(self.0, Number::Float) && is_integer(other);
        (self.fits(other) || cut).then_some(other)
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

/// Whether a number typed without a dtype that is the parameter `name` of `entry` adapts only to
/// the dtype of a number that the method follows: beside another dtype Polars refuses it as
/// ambiguous, which the tables, measured with numbers of a dtype, do not say.
pub(crate) fn adapts_numbers_only(entry: &Entry, name: &str) -> bool {
    entry
        .parameters()
        .iter()
        .find(|parameter| parameter.name() == name)
        .is_some_and(crate::catalog::Parameter::adapts_numbers_only)
}

/// Whether the parameter `name` of `entry` counts or indexes, which a whole
/// number is the dtype of whatever it adapts to: `shift`'s rows.
pub(crate) fn is_count(entry: &Entry, name: &str) -> bool {
    role(entry, name) == Some(Role::Count)
}

/// Whether Polars adapts a number typed without a dtype that is the parameter
/// `name` of the method `entry` to what the method follows, as it does the operand
/// of arithmetic and of a comparison, the value `shift` fills with, an exponent, what a value
/// is divided by and a bound. A method takes other expressions that do not meet what
/// it follows as an operand does: a key to group or order by, or the duration to
/// offset a date by.
pub(crate) fn operand_adapts(entry: &Entry, name: &str) -> bool {
    matches!(
        role(entry, name),
        Some(
            Role::Operand
                | Role::Filler
                | Role::Exponent
                | Role::Divisor
                | Role::FloorDivisor
                | Role::Base
                | Role::Bound
        )
    )
}

/// Whether the number typed without a dtype that is the parameter `name` of `entry` leaves what
/// the method gives unresolved: an integer raised to a number with a fraction is the unresolved
/// float the number is. A select resolves it to a `Float64` where it ends, a sort key or a join's
/// key does not, and an operand it meets adapts it as it does the number, so `f32 / (i8 ** 1.5)`
/// is a `Float32`: what it gives is not a dtype the tables can say. The same holds for a
/// logarithm of a whole number, a `Boolean` or a `Decimal` to such a base.
pub(crate) fn is_left_unresolved(
    entry: &Entry,
    name: &str,
    number: Loose,
    receiver: Representative,
) -> bool {
    match role(entry, name) {
        Some(Role::Exponent) => !number.is_whole() && is_integer(receiver),
        Some(Role::Base) => !number.is_whole() && !is_float(receiver),
        _ => false,
    }
}

/// The dtype of the number typed without a dtype that is the parameter `name` of the method
/// `entry`, beside a receiver of the dtype `receiver`, where the tables can say it: that of the
/// receiver where it fits it, a Boolean for a number with a fraction that a Boolean is floor
/// divided by, which Polars adapts it to and so refuses the call, the receiver for a bound that
/// it is cast to, and otherwise none.
pub(crate) fn adapted_dtype(
    entry: &Entry,
    name: &str,
    number: Loose,
    receiver: Representative,
) -> Option<Representative> {
    if role(entry, name) == Some(Role::FloorDivisor)
        && !number.is_whole()
        && receiver == Representative::Boolean
    {
        return Some(Representative::Boolean);
    }
    if role(entry, name) == Some(Role::Bound) {
        return number.cut_to(receiver);
    }
    number.beside(receiver)
}

/// What a number typed without a dtype that the method `entry` follows becomes beside the
/// operand `name` it is given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Receiver {
    /// It adapts to the dtype of the operand where it fits it, as the other side of
    /// arithmetic and of a comparison does.
    Adapts,
    /// It keeps the dtype it has alone: the base of a power, and a number with a
    /// fraction that is divided.
    Keeps,
}

/// How the number typed without a dtype that the method `entry` follows meets the operand
/// `name`, where the operand says. `None` where Polars does not adapt it as an operand, or keep
/// its dtype as one, that is, where the tables cannot say what it becomes: the value `shift`
/// fills with, the rows it counts, or an expression that meets nothing.
pub(crate) fn receiver_beside(entry: &Entry, name: &str, number: Loose) -> Option<Receiver> {
    match role(entry, name)? {
        Role::Operand | Role::FloorDivisor => Some(Receiver::Adapts),
        Role::Exponent | Role::Base | Role::Bound => Some(Receiver::Keeps),
        Role::Divisor if number.is_whole() => Some(Receiver::Adapts),
        Role::Divisor => Some(Receiver::Keeps),
        Role::Plain | Role::Filler | Role::Count => None,
    }
}

/// Whether the parameter `name` of `entry` must be given.
pub(crate) fn is_required(entry: &Entry, name: &str) -> bool {
    entry
        .parameters()
        .iter()
        .any(|parameter| parameter.name() == name && parameter.is_needed())
}
