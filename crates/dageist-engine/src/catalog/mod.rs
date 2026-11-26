//! The catalogue: every Polars operation the step editor can offer, held as
//! data.
//!
//! An [`Entry`] describes one Python-level Polars item, such as `Expr.shift`
//! or `LazyFrame.filter`: where it applies, the family it is swapped within,
//! its parameters and the kind of each, the dtypes it accepts, whether it
//! needs a collect, and the function that builds it in Polars. A view draws
//! each parameter by the rule for its [`Kind`], never by a look made for one
//! operation. Saved steps name an entry by its [`EntryId`], so a change in
//! Polars is absorbed by the entry, not by what was saved.
//!
//! The words the editor shows, Polars' own names included, come from the
//! language files beside this module, keyed by the same identifiers.

mod arguments;
mod comparison;
mod condition;
mod counting;
mod defaults;
mod duration;
mod entries;
mod left_out;
mod nested;
mod pattern;
mod starting;
mod structs;
#[cfg(test)]
mod tests;
mod timezone;
mod words;

pub use arguments::{Argument, Arguments, BuildError, PatternMeans};
pub use comparison::Comparison;
pub use condition::Conditional;
pub use counting::Counting;
pub use defaults::{DefaultValue, ParameterDefault};
pub use duration::{DurationError, DurationUse, any_moment, check_duration};
pub(crate) use entries::{time_of_day, typed_literal};
pub use left_out::{LeftOut, Reason, left_out};
pub use pattern::{PatternError, check_pattern, groups_dtype};
pub use starting::{Setting, StepFrame, StepStart, start_step};
pub use timezone::{TimeZoneError, check_time_zone};
pub use words::{SummaryPhrase, SummaryWord, Words, WordsError, english};

use polars::prelude::{Expr, LazyFrame};

/// The identifier of an entry: the Python owner and name, such as
/// `Expr.shift`, `Expr.dt.offset_by`, `LazyFrame.filter` or `polars.col`.
/// Saved steps keep it, so it does not change when Polars renames an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntryId(&'static str);

impl EntryId {
    /// The identifier as text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

/// What an entry is applied to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applies {
    /// A method of an expression, such as `shift`, including the methods of
    /// its namespaces, such as `dt.offset_by`.
    Expression,
    /// A method of a lazy frame that gives a lazy frame: a step of the list.
    Frame,
    /// A function that makes an expression from its arguments, such as `col`.
    Function,
    /// A link of a `when-then-otherwise` expression: `when`, `then` or
    /// `otherwise`. It starts, continues or ends a [`Conditional`], and only
    /// `otherwise` gives an expression.
    Condition,
}

impl Applies {
    /// Every kind of thing an entry applies to.
    pub const ALL: [Self; 4] = [
        Self::Expression,
        Self::Frame,
        Self::Function,
        Self::Condition,
    ];
}

/// The families an entry is swapped within. An entry can be changed for
/// another of its family without the rest of the step changing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Family {
    /// Steps that change the table.
    Steps,
    /// Values taken from other rows, and compared with them: `shift`, `first`, `last`, `diff`.
    Rows,
    /// The values in another order, and their places in it: `sort`, `sort_by`, `reverse`,
    /// `arg_sort` and `rank`.
    Order,
    /// Running values: `cum_sum`, `cum_count`, `cum_max`, `cum_min`, `cum_prod`.
    Cumulative,
    /// Arithmetic on values: `add`, `sub`, `mul`, `neg`, `abs`, `sign`, `clip`, and the like.
    Arithmetic,
    /// Roots, logarithms and the exponential: `sqrt`, `log`, `exp`, and the like, which give a
    /// float.
    Maths,
    /// Angles: `sin`, `arccos`, `tanh`, `degrees`, and the like, which give a float.
    Trigonometry,
    /// Rounding a number: `round`, `ceil`, `floor`, `truncate` and `round_sig_figs`, which give
    /// the dtype they are given.
    Rounding,
    /// One value for a whole group: `sum`, `min`, `max`, `len`, `mean`, `median`, `std`, `quantile`,
    /// `arg_max`, `any`, `bitwise_and`.
    Aggregate,
    /// Dates and times: the `dt` namespace.
    Temporal,
    /// Reading, naming and unnesting the fields of a struct: `struct.field`, `rename_fields`,
    /// `drop`, `json_encode` and `unnest`.
    Structs,
    /// Changing the text itself: its case, the characters trimmed from its ends, padding,
    /// reversing, and replacing what matches a pattern, which are the methods of the `str`
    /// namespace that give text.
    Text,
    /// Taking text apart, and measuring it: `str.slice`, `str.head`, `str.split`, `str.join` and
    /// the lengths, which give a piece of the text, a list of pieces, one text or a count.
    Pieces,
    /// Finding in text by a pattern: where it matches, how often, and what it matches:
    /// `str.find`, `str.count_matches`, `str.extract` and the like, which each take the pattern.
    Search,
    /// Working within groups: `over`.
    Window,
    /// Comparisons and tests that give a Boolean: `eq`, `gt`, `eq_missing`, `is_between`,
    /// `is_close`, `is_null`, `str.starts_with`, `str.contains`, and the like.
    Compare,
    /// Tests of what a value is, or of whether it is repeated: `is_nan`, `is_infinite`,
    /// `is_unique`, `is_first_distinct`, and the like, which give a Boolean.
    Checks,
    /// Combining and negating truth: `and`, `or`, `xor` and `not`, which work bit
    /// by bit on whole numbers, and the counts of bits: `bitwise_count_ones` and its kin.
    Logic,
    /// Nulls and NaNs: `fill_null`, `forward_fill`, `drop_nulls`, `null_count`, and the like.
    Nulls,
    /// Changing what a value is: `cast`, `replace` and `replace_strict`.
    Conversion,
    /// Where an expression starts: `col`, `lit`.
    Start,
    /// Conditions: `when`, `then`, `otherwise`.
    Condition,
}

impl Family {
    /// Every family, in the order they are offered.
    pub const ALL: [Self; 22] = [
        Self::Steps,
        Self::Rows,
        Self::Order,
        Self::Cumulative,
        Self::Arithmetic,
        Self::Maths,
        Self::Trigonometry,
        Self::Rounding,
        Self::Aggregate,
        Self::Temporal,
        Self::Structs,
        Self::Text,
        Self::Pieces,
        Self::Search,
        Self::Window,
        Self::Compare,
        Self::Checks,
        Self::Logic,
        Self::Nulls,
        Self::Conversion,
        Self::Start,
        Self::Condition,
    ];

    /// The key the language files give the family's words under.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Steps => "steps",
            Self::Rows => "rows",
            Self::Order => "order",
            Self::Cumulative => "cumulative",
            Self::Arithmetic => "arithmetic",
            Self::Maths => "maths",
            Self::Trigonometry => "trigonometry",
            Self::Rounding => "rounding",
            Self::Aggregate => "aggregate",
            Self::Temporal => "temporal",
            Self::Structs => "structs",
            Self::Text => "text",
            Self::Pieces => "pieces",
            Self::Search => "search",
            Self::Window => "window",
            Self::Compare => "compare",
            Self::Checks => "checks",
            Self::Logic => "logic",
            Self::Nulls => "nulls",
            Self::Conversion => "conversion",
            Self::Start => "start",
            Self::Condition => "condition",
        }
    }
}

/// A group of Polars dtypes an expression may have where an entry takes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Dtypes {
    /// Every dtype.
    Any,
    /// The integer and float dtypes.
    Numeric,
    /// The integer dtypes.
    Integer,
    /// `Datetime`, `Date`, `Time` and `Duration`.
    Temporal,
    /// `Datetime`.
    Datetime,
    /// `Date`.
    Date,
    /// `Time`.
    Time,
    /// `Duration`.
    Duration,
    /// `String`.
    String,
    /// `Boolean`.
    Boolean,
    /// `List`, of any element dtype.
    List,
    /// `Array`, of any element dtype and any width.
    Array,
    /// `Struct`, of any fields.
    Struct,
}

impl Dtypes {
    /// Every group, in the order they are listed.
    pub const ALL: [Self; 13] = [
        Self::Any,
        Self::Numeric,
        Self::Integer,
        Self::Temporal,
        Self::Datetime,
        Self::Date,
        Self::Time,
        Self::Duration,
        Self::String,
        Self::Boolean,
        Self::List,
        Self::Array,
        Self::Struct,
    ];

    /// The key the language files give the group's words under.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Any => "any",
            Self::Numeric => "numeric",
            Self::Integer => "integer",
            Self::Temporal => "temporal",
            Self::Datetime => "Datetime",
            Self::Date => "Date",
            Self::Time => "Time",
            Self::Duration => "Duration",
            Self::String => "String",
            Self::Boolean => "Boolean",
            Self::List => "List",
            Self::Array => "Array",
            Self::Struct => "Struct",
        }
    }
}

/// The dtype of a [`Kind::Value`]: a literal typed in, not an expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueType {
    /// A whole number.
    Integer,
    /// A floating-point number.
    Float,
    /// Text.
    String,
    /// A calendar date.
    Date,
    /// A literal that has its own type: a whole number, a number, text or a
    /// flag, as typed, such as the value of `lit`.
    Any,
}

impl ValueType {
    /// Every type of literal a parameter takes.
    pub const ALL: [Self; 5] = [
        Self::Integer,
        Self::Float,
        Self::String,
        Self::Date,
        Self::Any,
    ];
}

/// What a parameter holds. The set is closed: each kind has one drawing and
/// one way of being edited, and a parameter that fits none is a design
/// question, not a reason for a new kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A chain: a column, a value or a connected input, then methods. A
    /// typed number or text is a chain of one value. It may have only the
    /// dtypes listed.
    Expression(&'static [Dtypes]),
    /// The names of input columns.
    Column,
    /// The name of a field of the struct a method follows, which the engine knows where the walk
    /// does: a name that is no field of the struct is refused before Polars is asked.
    Field,
    /// A literal of the given type that cannot be an expression.
    Value(ValueType),
    /// True or false. A flag whose default is `None` is also unset, which
    /// leaves the choice to Polars.
    Flag,
    /// One word of a closed set, written as Polars spells it.
    Choice(&'static [&'static str]),
    /// A Polars dtype.
    Dtype,
    /// A Polars duration string, such as `1mo` or `-2d`.
    Duration,
    /// A regular expression, as Polars reads one: the syntax of Rust's `regex` crate, compiled with
    /// the settings Polars compiles it with. It is text typed in, and a document that holds one
    /// that is no pattern is still a document: the step is refused at the parameter, with the
    /// reason, when it is built. Where the parameter has a flag that makes it plain text, as
    /// `literal` does, see [`Parameter::plain_text_flag`].
    Pattern,
    /// A time zone, as Polars reads one: a name of the time zone database such as `Europe/Berlin`,
    /// or a fixed offset. It is text typed in, and a document that holds one that is none is still a
    /// document: the step is refused at the parameter, with Polars' reason, when it is built. A
    /// parameter that [takes none](Parameter::takes_no_time_zone) may also be given no zone at all,
    /// which is Python's `None`.
    TimeZone,
    /// A frame from the graph.
    Table,
    /// A file, folder or cloud location.
    Path,
    /// Key and value entries, each side of its own kind.
    Pairs(&'static Kind, &'static Kind),
}

/// How many of its kind a parameter holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// One.
    One,
    /// An ordered list.
    Many,
    /// One, or one for each item of the named parameter of the same entry.
    PerItem(&'static str),
}

/// How an expression parameter meets the value its method follows, where Polars
/// treats a number typed without a dtype differently for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Nothing sets it apart: a key to group by, a duration to offset by.
    Plain,
    /// The other side of an operation with the value the method follows, as
    /// arithmetic and comparisons combine two values. A number typed without a
    /// dtype adapts to the other side, whichever side it is.
    Operand,
    /// What stands in for the values the method leaves missing, as the value
    /// `shift` fills with. A number typed without a dtype adapts to the value the
    /// method follows, but the value the method follows does not adapt to it.
    Filler,
    /// A count or an index of rows, as `shift`'s `n` is. A whole number there is
    /// that dtype and adapts to nothing.
    Count,
    /// How long a text is made, as `str.pad_start`'s length is: a count as [`Role::Count`] is, which
    /// Polars converts to an unsigned whole number, so a whole number typed without a dtype that
    /// is below zero is refused when the schema is worked out, where a count of rows or of
    /// characters from an end may be negative.
    Length,
    /// The power a value is raised to, as `pow`'s `exponent` is. A number typed
    /// without a dtype adapts to the value the method follows, but that value
    /// does not adapt to it: a number raised to a power is the dtype it has.
    Exponent,
    /// What a value is divided by, as `truediv`'s `other` is. A number typed
    /// without a dtype adapts to the value the method follows, and a whole number
    /// that the method follows adapts to it, but a number with a fraction that
    /// the method follows does not: it is a `Float64` divided by any float.
    Divisor,
    /// What a value is floor divided by, as `floordiv`'s `other` is. A number typed without
    /// a dtype adapts as an operand's does, except that a number with a fraction beside a
    /// Boolean is a Boolean, which Polars cannot floor divide, though it floor divides a
    /// Boolean by a typed float.
    FloorDivisor,
    /// The base of a logarithm, as `log`'s `base` is. A number typed without a dtype adapts to the
    /// value the method follows, where that is a float, as an operand's does, but the value the
    /// method follows does not adapt to it. A number with a fraction beside a value that is no
    /// float, a whole number, a `Boolean` or a `Decimal`, leaves the result the unresolved float
    /// the number is, which a select resolves to a `Float64` where it ends and a sort key or a
    /// join's key does not.
    Base,
    /// A share from 0 to 1 that a statistic reads, as `quantile`'s is. A number typed without a dtype
    /// is the number it is, whatever the value the method follows is, and adapts to nothing: an
    /// `Int32` where it is whole and a `Float64` where it has a fraction. Polars reads the number
    /// and not its dtype, so the call gives the same whichever it is.
    Share,
    /// A limit a value is held to, as `clip`'s bounds are. A number typed without a dtype is cast
    /// to the dtype of the value the method follows, as strictly as Polars can: it refuses a
    /// number that dtype cannot hold, where an operand's would widen the dtype, and cuts a number
    /// with a fraction to a whole number beside whole numbers. The value the method follows does
    /// not adapt to it, and keeps the dtype it has.
    Bound,
}

impl Role {
    /// Whether the parameter counts rows or characters, which a whole number typed without a dtype
    /// is that dtype of and adapts to nothing.
    #[must_use]
    pub const fn is_a_count(self) -> bool {
        matches!(self, Self::Count | Self::Length)
    }
}

/// How a parameter that Polars reads as one value must be given it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OneValue {
    /// One value that Polars can tell is one before the rows run: a number typed in or an aggregate,
    /// and neither the rows, nor a count of values, nor the one value that a method took of one value,
    /// as the first of them is.
    Scalar,
}

/// What a parameter that gives a number of values is given as many of as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsManyAs {
    /// The values of the value the method follows.
    TheReceiver,
    /// The values of the named parameter of the same entry.
    Parameter(&'static str),
}

/// Whether the expressions a parameter holds write columns of the table the
/// step gives. A column an expression writes is replaced, when the table the
/// step was given had it, even when the expression gives it the dtype it had;
/// an expression that is a bare column reference passes the column on and
/// writes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Writes {
    /// What the parameter holds is read, not written: a key, a condition, a
    /// table.
    Nothing,
    /// Each expression the parameter holds, and each entry of pairs of a
    /// column's name and an expression, writes the columns it names.
    Columns,
}

/// What a starred parameter of Python's signature takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variadic {
    /// `*name`: any number of arguments by position.
    Args,
    /// `**name`: any number of arguments by name, which the entry holds as
    /// pairs of a name and a value.
    Kwargs,
}

/// One parameter of an entry, named as Python names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Parameter {
    name: &'static str,
    kind: Kind,
    shape: Shape,
    default: ParameterDefault,
    role: Role,
    writes: Writes,
    keyword_only: bool,
    variadic: Option<Variadic>,
    at_least_one: bool,
    in_place_of: Option<&'static str>,
    shares_a_dtype_with: Option<&'static str>,
    as_many_as: Option<AsManyAs>,
    converts: &'static [&'static str],
    lenient_beside: Option<&'static str>,
    adapts_numbers_only: bool,
    broadcasts: bool,
    plain_text_flag: Option<&'static str>,
    names_the_fields_of_the_result: bool,
    offered: bool,
    duration_string: Option<DurationUse>,
    takes_no_time_zone: bool,
    broadcasts_in_a_window: bool,
    one_value: Option<OneValue>,
}

impl Parameter {
    const fn new(name: &'static str, kind: Kind, shape: Shape, default: ParameterDefault) -> Self {
        Self {
            name,
            kind,
            shape,
            default,
            role: Role::Plain,
            writes: Writes::Nothing,
            keyword_only: false,
            variadic: None,
            at_least_one: false,
            in_place_of: None,
            shares_a_dtype_with: None,
            as_many_as: None,
            converts: &[],
            lenient_beside: None,
            adapts_numbers_only: false,
            broadcasts: true,
            plain_text_flag: None,
            names_the_fields_of_the_result: false,
            offered: true,
            duration_string: None,
            takes_no_time_zone: false,
            broadcasts_in_a_window: true,
            one_value: None,
        }
    }

    /// The same parameter, of which a step always gives at least one, though Python takes none:
    /// `and_` and `or_` are the expression they follow without an operand, and `clip` is it
    /// without a bound, which is no use to a step, so they are judged with one. The parameter is
    /// a starred one, or one of several that stand for one another, as `clip`'s two bounds do.
    const fn with_at_least_one(mut self) -> Self {
        assert!(
            matches!(self.default, ParameterDefault::Python(_))
                && matches!(self.kind, Kind::Expression(_)),
            "only an optional parameter that holds expressions is one of which a step gives at least one"
        );
        self.at_least_one = true;
        self
    }

    /// The same parameter, of a dtype that the values of the expression parameters `named` are
    /// converted to, as `cast` converts what it follows: `replace_strict`'s `return_dtype` is
    /// what its `new` and its `default` are converted to. What Polars accepts of the conversion
    /// is what it accepts of a cast, which the tables measure for `cast` and not again for each
    /// method that converts.
    const fn converting(mut self, named: &'static [&'static str]) -> Self {
        self.converts = named;
        self
    }

    /// The same parameter, whose conversion is strict only where the expression parameter `other`
    /// is not given: given it, Polars converts the values as a `cast` that gives a null where a
    /// value does not convert, as `replace_strict` converts what replaces and its default to
    /// `return_dtype`.
    const fn converting_leniently_beside(mut self, other: &'static str) -> Self {
        self.lenient_beside = Some(other);
        self
    }

    /// The same parameter, which gives as many values as `meets` or one: Polars refuses a
    /// replacement whose `new` has other numbers of values than its `old` and is not one value,
    /// and a strict one whose `default` has other numbers than the value it follows.
    const fn given_as_many_as(mut self, meets: AsManyAs) -> Self {
        self.as_many_as = Some(meets);
        self
    }

    /// The same parameter, whose value meets the one the expression parameter `other` holds in a
    /// dtype that is the supertype of both, as `replace_strict`'s `default` meets its `new`, and
    /// `is_between`'s upper bound its lower one and the receiver. Polars refuses two dtypes that
    /// have none, and the tables, which judge each parameter apart, cannot say which pairs those
    /// are, so a call whose two dtypes differ is not judged.
    const fn sharing_a_dtype_with(mut self, other: &'static str) -> Self {
        self.shares_a_dtype_with = Some(other);
        self
    }

    /// The same parameter, which Python takes in place of the optional expression `other`: one
    /// of the two is given and not both, as `fill_null`'s `strategy` is given in place of its
    /// `value`. A step that gives it does not give `other`, so what the tables measured of the
    /// method with `other` does not judge it.
    const fn given_in_place_of(mut self, other: &'static str) -> Self {
        self.in_place_of = Some(other);
        self
    }

    /// The same parameter, which Polars gives a number typed without a dtype the dtype of the
    /// number it follows and no other: beside a date or a duration Polars refuses it as
    /// ambiguous, where it takes a number of a dtype, so the tables, which measured numbers of
    /// a dtype, say nothing of what the method gives with it.
    const fn adapting_numbers_only(mut self) -> Self {
        self.adapts_numbers_only = true;
        self
    }

    /// The same parameter, of which Polars does not meet a receiver of one value with each of
    /// many values: it takes one value only, as `shift`'s rows and fill, which Polars refuses
    /// many values of, or the values it takes are listed and met by a rule of their own, as the
    /// values of a replacement are. Every other expression parameter of a method is met by a
    /// receiver of one value with each of its values, so a method of one value given an operand
    /// of many gives many.
    const fn not_broadcasting(mut self) -> Self {
        self.broadcasts = false;
        self
    }

    /// The same parameter, a pattern that Polars takes as plain text where the flag `flag` of the
    /// same entry is true, as `literal` makes it: the text is then no regular expression, whatever
    /// it holds. An expression parameter that has one, as the separator of a split has, is a pattern
    /// where it is text typed in and the flag is not true.
    const fn with_plain_text_flag(mut self, flag: &'static str) -> Self {
        assert!(
            matches!(self.kind, Kind::Pattern | Kind::Expression(_)),
            "only a pattern, or an expression that may be typed as one, is plain text by a flag"
        );
        self.plain_text_flag = Some(flag);
        self
    }

    /// The same parameter, which the editor does not offer: it is in Python's signature and so in the
    /// catalogue, but setting it changes nothing here, as `strict` of a method that takes a pattern
    /// typed in, which is refused when it is none whatever `strict` says.
    const fn not_offered(mut self) -> Self {
        self.offered = false;
        self
    }

    /// The same parameter, an expression that holds a duration string where it is text typed in,
    /// as `truncate`'s `every` and `offset_by`'s `by` do: Polars reads it as one when it runs
    /// the rows, and the editor refuses the text it does not read at the parameter, with Polars'
    /// words, where it is built.
    const fn holding_a_duration_string(mut self, using: DurationUse) -> Self {
        assert!(
            matches!(self.kind, Kind::Expression(_)),
            "only an expression may hold a duration string typed in"
        );
        self.duration_string = Some(using);
        self
    }

    /// The same parameter, of which Polars meets a receiver of one value with each of many values where
    /// the expression is selected, as it does every other, but not where a window follows: a window
    /// maps what it is given to as many values as it has, and the one value a part widens is refused
    /// with the length of its output. `dt.replace`'s parts are so.
    const fn not_broadcasting_in_a_window(mut self) -> Self {
        self.broadcasts_in_a_window = false;
        self
    }

    /// The same parameter, a time zone that may also be none, which takes a zone off a moment, as
    /// Python's `replace_time_zone` is given `None`.
    const fn or_no_time_zone(mut self) -> Self {
        assert!(
            matches!(self.kind, Kind::TimeZone),
            "only a time zone may be none"
        );
        self.takes_no_time_zone = true;
        self
    }

    /// The same parameter, a pattern whose groups name the fields of the struct the method gives,
    /// so that what the method gives is not known until the pattern is.
    const fn naming_the_fields_of_the_result(mut self) -> Self {
        assert!(
            matches!(self.kind, Kind::Pattern),
            "only a pattern names the fields of a result"
        );
        self.names_the_fields_of_the_result = true;
        self
    }

    /// The same parameter, which Python writes with the stars that make it
    /// take any number of arguments.
    const fn with_variadic(mut self, variadic: Variadic) -> Self {
        self.variadic = Some(variadic);
        self
    }

    /// The same parameter, which Polars reads as one value and refuses to be given anything else, as
    /// `shift`'s count of rows is.
    const fn reading_one_value(mut self, how: OneValue) -> Self {
        self.one_value = Some(how);
        self
    }

    /// The same parameter, which Python takes only by its name: it follows a
    /// bare `*` or a starred parameter in Python's signature.
    const fn keyword_only(mut self) -> Self {
        self.keyword_only = true;
        self
    }

    /// The same parameter, whose expressions write columns.
    const fn writing(mut self) -> Self {
        self.writes = Writes::Columns;
        self
    }

    /// The same parameter with the role it plays beside the value its method follows.
    const fn with_role(mut self, role: Role) -> Self {
        self.role = role;
        self
    }

    /// Python's name for the parameter.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// How the parameter must be one value, where Polars reads it as one: it is refused the rows and
    /// a count of values.
    #[must_use]
    pub const fn one_value(&self) -> Option<OneValue> {
        self.one_value
    }

    /// What the parameter holds.
    #[must_use]
    pub const fn kind(&self) -> Kind {
        self.kind
    }

    /// How many of its kind the parameter holds.
    #[must_use]
    pub const fn shape(&self) -> Shape {
        self.shape
    }

    /// Whether a step gives the parameter: Python requires it, or a step always gives at least
    /// one of it, or of it and the parameters that stand for it. What is offered is judged with it
    /// given.
    #[must_use]
    pub const fn is_needed(&self) -> bool {
        matches!(self.default, ParameterDefault::Required) || self.at_least_one
    }

    /// What the parameter holds when it is not given, as Python writes it.
    /// [`ParameterDefault::value`] reads it as a value.
    #[must_use]
    pub const fn default(&self) -> ParameterDefault {
        self.default
    }

    /// How the parameter meets the value its method follows.
    #[must_use]
    pub const fn role(&self) -> Role {
        self.role
    }

    /// Whether the expressions the parameter holds write columns.
    #[must_use]
    pub const fn writes(&self) -> Writes {
        self.writes
    }

    /// The stars of the parameter in Python's signature, if it has any: `*name`
    /// takes the rest of the arguments by position and `**name` the rest by
    /// name.
    #[must_use]
    pub const fn variadic(&self) -> Option<Variadic> {
        self.variadic
    }

    /// The expression parameters whose values are converted to the dtype this parameter holds,
    /// none for a parameter that converts nothing.
    #[must_use]
    pub const fn converts(&self) -> &'static [&'static str] {
        self.converts
    }

    /// What the parameter is given as many values as, or one, if Polars holds it to a number.
    #[must_use]
    pub const fn as_many_as(&self) -> Option<AsManyAs> {
        self.as_many_as
    }

    /// The expression parameter that makes the conversion of this one lenient when it is given.
    #[must_use]
    pub const fn lenient_beside(&self) -> Option<&'static str> {
        self.lenient_beside
    }

    /// Whether a receiver of one value is met with each of the many values the parameter is given,
    /// so that the method gives as many: Polars broadcasts the receiver against the operand. It is
    /// a fact about the method in Polars, apart from how a number typed without a dtype adapts,
    /// which the role says. True for every expression parameter but those that take one value only
    /// or list the values they take.
    #[must_use]
    pub const fn broadcasts(&self) -> bool {
        self.broadcasts
    }

    /// Whether the editor offers the parameter: it lists it among the options a method can be given,
    /// and the engine says what words it may be set to. A parameter that is not offered is in the
    /// catalogue for Python's signature alone, and nothing that sets it changes what a step does.
    #[must_use]
    pub const fn offered(&self) -> bool {
        self.offered
    }

    /// The flag of the same entry that makes the pattern plain text where it is true, if there is
    /// one: Polars' `literal`. The text of a pattern whose flag is true is not read as a regular
    /// expression, so it is never refused for being no pattern. The separator of a split, an
    /// expression, has it too: typed text is then a pattern where the flag is false.
    #[must_use]
    pub const fn plain_text_flag(&self) -> Option<&'static str> {
        self.plain_text_flag
    }

    /// Whether the groups of the pattern name the fields of the struct the method gives, as
    /// `str.extract_groups` makes of a pattern, so that the dtype of what it gives depends on the
    /// pattern given and not on the dtype it follows alone.
    #[must_use]
    pub const fn names_the_fields_of_the_result(&self) -> bool {
        self.names_the_fields_of_the_result
    }

    /// Whether a receiver of one value is met with each of the many values the parameter is given
    /// where a window follows too, as it is where the expression is selected: false for the parts
    /// of `dt.replace`, whose widened value a window refuses.
    #[must_use]
    pub const fn broadcasts_in_a_window(&self) -> bool {
        self.broadcasts_in_a_window
    }

    /// Whether the time zone the parameter holds may be none: no zone at all, which a moment of no
    /// zone has, as `replace_time_zone` takes a zone off with.
    #[must_use]
    pub const fn takes_no_time_zone(&self) -> bool {
        self.takes_no_time_zone
    }

    /// The method that reads the expression the parameter holds as a duration string where it is
    /// text typed in, which Polars is asked to run it on: `truncate`'s `every`, `round`'s and `offset_by`'s `by`.
    #[must_use]
    pub const fn duration_string(&self) -> Option<DurationUse> {
        self.duration_string
    }

    /// Whether a number typed without a dtype that is given to the parameter takes the dtype of
    /// a number the method follows and, beside any other dtype, is refused by Polars as
    /// ambiguous where a number of a dtype is not.
    #[must_use]
    pub const fn adapts_numbers_only(&self) -> bool {
        self.adapts_numbers_only
    }

    /// The expression parameter whose value this one meets in the supertype of both dtypes, if
    /// it meets one.
    #[must_use]
    pub const fn shares_a_dtype_with(&self) -> Option<&'static str> {
        self.shares_a_dtype_with
    }

    /// The optional expression parameter this one is given in place of, when Python takes one of
    /// the two and refuses both: the parameter named is then not given.
    #[must_use]
    pub const fn in_place_of(&self) -> Option<&'static str> {
        self.in_place_of
    }

    /// Whether Python takes the parameter only by its name, because it follows
    /// a bare `*` or a starred parameter in Python's signature. A starred or
    /// double-starred parameter is not one: it takes what Python gives it
    /// without a name.
    #[must_use]
    pub const fn is_keyword_only(&self) -> bool {
        self.keyword_only
    }
}

/// Whether an entry can be part of a lazy plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Laziness {
    /// It works on a lazy frame.
    Lazy,
    /// It needs the data collected first.
    NeedsCollect,
}

/// The function that builds an entry in Polars from its filled-in arguments.
#[derive(Clone, Copy)]
pub enum Build {
    /// Applies an expression method to the expression it follows.
    Expression(fn(Expr, &Arguments) -> Result<Expr, BuildError>),
    /// Applies a step to the frame it follows.
    Frame(fn(LazyFrame, &Arguments) -> Result<LazyFrame, BuildError>),
    /// Makes an expression from its arguments alone.
    Function(fn(&Arguments) -> Result<Expr, BuildError>),
    /// Starts a condition from its arguments alone: `when`.
    Condition(fn(&Arguments) -> Result<Conditional, BuildError>),
    /// Continues the condition it follows: `then`, or a further `when`. The
    /// link that cannot follow the condition's state is refused.
    ConditionStep(fn(Conditional, &Arguments) -> Result<Conditional, BuildError>),
    /// Ends the condition it follows in an expression: `otherwise`. The link
    /// that cannot follow the condition's state is refused.
    ConditionEnd(fn(Conditional, &Arguments) -> Result<Expr, BuildError>),
}

impl std::fmt::Debug for Build {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Expression(_) => "Build::Expression",
            Self::Frame(_) => "Build::Frame",
            Self::Function(_) => "Build::Function",
            Self::Condition(_) => "Build::Condition",
            Self::ConditionStep(_) => "Build::ConditionStep",
            Self::ConditionEnd(_) => "Build::ConditionEnd",
        })
    }
}

/// One Polars operation the editor can offer.
#[derive(Debug, Clone, Copy)]
pub struct Entry {
    id: EntryId,
    family: Family,
    accepts: &'static [Dtypes],
    parameters: &'static [Parameter],
    laziness: Laziness,
    build: Build,
}

impl Entry {
    /// The entry's identifier.
    #[must_use]
    pub const fn id(&self) -> EntryId {
        self.id
    }

    /// What the entry is applied to.
    #[must_use]
    pub const fn applies(&self) -> Applies {
        match self.build {
            Build::Expression(_) => Applies::Expression,
            Build::Frame(_) => Applies::Frame,
            Build::Function(_) => Applies::Function,
            Build::Condition(_) | Build::ConditionStep(_) | Build::ConditionEnd(_) => {
                Applies::Condition
            }
        }
    }

    /// The family the entry is swapped within.
    #[must_use]
    pub const fn family(&self) -> Family {
        self.family
    }

    /// The dtypes an expression method accepts on the expression it follows.
    /// Empty for steps and functions.
    #[must_use]
    pub const fn accepts(&self) -> &'static [Dtypes] {
        self.accepts
    }

    /// The parameters, in Python's order.
    #[must_use]
    pub const fn parameters(&self) -> &'static [Parameter] {
        self.parameters
    }

    /// Whether the entry can be part of a lazy plan.
    #[must_use]
    pub const fn laziness(&self) -> Laziness {
        self.laziness
    }

    /// The function that builds the entry in Polars.
    #[must_use]
    pub const fn build(&self) -> Build {
        self.build
    }

    /// The expressions a step writes as columns of the table it gives, each
    /// named as it is written: those of the parameters whose [`Writes`] is
    /// [`Writes::Columns`], where a pair gives its expression the name of its
    /// key. An expression that is a bare column reference passes the column on
    /// and is not written.
    ///
    /// # Errors
    ///
    /// When `arguments` are not those the entry builds from.
    pub(crate) fn writes(&self, arguments: &Arguments) -> Result<Vec<Expr>, BuildError> {
        entries::writes(self, arguments)
    }
}

/// Every entry of the catalogue, ordered by identifier.
#[must_use]
pub fn entries() -> &'static [Entry] {
    entries::ENTRIES
}

/// The entry with identifier `id`, if the catalogue has one.
#[must_use]
pub fn entry(id: &str) -> Option<&'static Entry> {
    entries::ENTRIES
        .binary_search_by(|entry| entry.id.0.cmp(id))
        .ok()
        .map(|at| &entries::ENTRIES[at])
}
