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
mod defaults;
mod entries;
mod left_out;
mod starting;
#[cfg(test)]
mod tests;
mod words;

pub use arguments::{Argument, Arguments, BuildError};
pub use comparison::Comparison;
pub use condition::Conditional;
pub use defaults::{DefaultValue, ParameterDefault};
pub use left_out::{LeftOut, Reason, left_out};
pub use starting::{Setting, StepFrame, StepStart, start_step};
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
    /// Values taken from other rows: `shift`, `first`, `last`.
    Rows,
    /// Running values: `cum_sum`, `cum_count`, `cum_max`.
    Cumulative,
    /// Arithmetic on values: `add`, `sub`, `mul`, `neg`, `abs`, and the like.
    Arithmetic,
    /// One value for a whole group: `sum`, `min`, `max`, `len`.
    Aggregate,
    /// Dates and times: the `dt` namespace.
    Temporal,
    /// Working within groups: `over`.
    Window,
    /// Comparisons and tests that give a Boolean.
    Compare,
    /// Combining and negating truth: `and`, `or`, `xor` and `not`, which work bit
    /// by bit on whole numbers.
    Logic,
    /// Where an expression starts: `col`, `lit`.
    Start,
    /// Conditions: `when`, `then`, `otherwise`.
    Condition,
}

impl Family {
    /// Every family, in the order they are offered.
    pub const ALL: [Self; 11] = [
        Self::Steps,
        Self::Rows,
        Self::Cumulative,
        Self::Arithmetic,
        Self::Aggregate,
        Self::Temporal,
        Self::Window,
        Self::Compare,
        Self::Logic,
        Self::Start,
        Self::Condition,
    ];

    /// The key the language files give the family's words under.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Steps => "steps",
            Self::Rows => "rows",
            Self::Cumulative => "cumulative",
            Self::Arithmetic => "arithmetic",
            Self::Aggregate => "aggregate",
            Self::Temporal => "temporal",
            Self::Window => "window",
            Self::Compare => "compare",
            Self::Logic => "logic",
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
}

impl Dtypes {
    /// Every group, in the order they are listed.
    pub const ALL: [Self; 10] = [
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
        }
    }

    /// The same parameter, a starred one of which a step always gives at least one, though
    /// Python takes none: `and_` and `or_` are the expression they follow without an operand,
    /// which is no use to a step, so they are judged with one.
    const fn with_at_least_one(mut self) -> Self {
        assert!(
            matches!(self.variadic, Some(Variadic::Args)),
            "only a starred parameter is one of which a step gives at least one"
        );
        self.at_least_one = true;
        self
    }

    /// The same parameter, which Python writes with the stars that make it
    /// take any number of arguments.
    const fn with_variadic(mut self, variadic: Variadic) -> Self {
        self.variadic = Some(variadic);
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

    /// Whether a step gives the parameter: Python requires it, or it is a starred parameter of
    /// which a step always gives at least one. What is offered is judged with it given.
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
