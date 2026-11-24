//! What the step editor starts from: what a parameter holds when it is not
//! given, what it is set to when it is first given, and what a new step is
//! given.
//!
//! These are Polars' defaults and the editor's choices of a first value, and
//! both are the engine's to say, so that no view reads them off a parameter
//! itself. A [`Setting`] is the one shape they are all given in, whatever kind
//! of parameter holds it.

use super::{Applies, Comparison, DefaultValue, Entry, Kind, Parameter, Shape};

/// What a parameter holds, in the one shape every kind of parameter is drawn
/// from.
#[derive(Debug, Clone, PartialEq)]
pub enum Setting {
    /// Still to be given: it stands for nothing until it is.
    ToGive,
    /// True or false.
    Flag(bool),
    /// A whole number.
    Integer(i64),
    /// A number with a fraction, which stays one though it is whole.
    Float(f64),
    /// Text that is typed: a string, a duration or a path.
    Text(String),
    /// A name picked from a list: the word of a choice, a dtype, a column or a
    /// table.
    Word(String),
    /// An expression that reads the column of this name.
    Field(String),
    /// An expression that starts from `polars.element` and calls the one method
    /// of this name, as the catalogue names it without its owner.
    Element(&'static str),
    /// A comparison of a field with something else, which `entry` names as the
    /// catalogue does, such as `Expr.eq`: the field it reads, and what it is
    /// compared with. A part still to be given is [`Setting::ToGive`].
    Comparison {
        /// The entry that compares.
        entry: &'static str,
        /// What is compared: a field.
        field: Box<Setting>,
        /// What it is compared with.
        other: Box<Setting>,
    },
    /// An ordered list of settings.
    List(Vec<Setting>),
    /// Key and value entries.
    Pairs(Vec<(Setting, Setting)>),
}

impl Parameter {
    /// What the parameter holds when it is not given, as a setting, or `None`
    /// where it must be given, where Polars decides, or where the default is
    /// not a value the engine reads: a number or text is that, a flag is a
    /// flag, and the text of a choice is its word.
    #[must_use]
    pub fn default_setting(&self) -> Option<Setting> {
        match self.default().value() {
            DefaultValue::Flag(value) => Some(Setting::Flag(value)),
            DefaultValue::Integer(value) => Some(Setting::Integer(value)),
            DefaultValue::Float(value) => Some(Setting::Float(value)),
            DefaultValue::Text(value) => Some(if matches!(self.kind(), Kind::Choice(_)) {
                Setting::Word(value)
            } else {
                Setting::Text(value)
            }),
            DefaultValue::Required
            | DefaultValue::Unset
            | DefaultValue::Empty
            | DefaultValue::Other => None,
        }
    }

    /// Whether a method that takes the parameter first starts without giving it: it is an
    /// expression, or a list of them, that a step need not give and that Polars leaves to its default
    /// when it is not given, `None`, as the characters a trim takes off are, which are then
    /// whitespace. The method is complete as it starts, and the document does not mention the
    /// parameter; giving it a value is the person's choice. A parameter that must be given, or
    /// that has a value of its own by default, is not left out: it is still to be given or starts
    /// with its default.
    #[must_use]
    pub fn starts_left_out(&self) -> bool {
        matches!(self.shape(), Shape::One | Shape::Many)
            && matches!(self.kind(), Kind::Expression(_))
            && !self.is_needed()
            && matches!(self.default().value(), DefaultValue::Unset)
    }

    /// What the parameter is set to when it is first added, which is something
    /// that differs from its default: a list starts with one item still to be
    /// given and pairs with none; a flag starts as the opposite of its default,
    /// or true where Polars decides; a choice starts with its first word that is
    /// not the default; anything else is still to be given.
    ///
    /// Where a flag or a choice stands beside other options, which words Polars
    /// allows is for the option table to say: see `allowed::option_offers`.
    #[must_use]
    pub fn start_setting(&self) -> Setting {
        if self.shape() == Shape::Many {
            return Setting::List(vec![Setting::ToGive]);
        }
        match self.kind() {
            Kind::Pairs(..) => Setting::Pairs(Vec::new()),
            Kind::Flag => {
                Setting::Flag(!matches!(self.default().value(), DefaultValue::Flag(true)))
            }
            Kind::Choice(words) => {
                let default = match self.default().value() {
                    DefaultValue::Text(word) => Some(word),
                    _ => None,
                };
                words
                    .iter()
                    .find(|word| default.as_deref() != Some(**word))
                    .map_or(Setting::ToGive, |word| Setting::Word((*word).to_owned()))
            }
            _ => Setting::ToGive,
        }
    }
}

/// The table a new step works on and the tables it can bring in.
#[derive(Debug, Clone, Copy)]
pub struct StepFrame<'a> {
    /// The names of the columns of the table, in its order.
    pub columns: &'a [String],
    /// The names of the connected tables, in the order the first is taken from.
    pub tables: &'a [String],
}

/// What a new step of an entry is given: the arguments it starts with.
#[derive(Debug, Clone, PartialEq)]
pub struct StepStart {
    arguments: Vec<(&'static str, Setting)>,
}

impl StepStart {
    /// The parameters the step starts with, by Python's name, each with what it
    /// is set to. A parameter that is not among them is not given: it holds its
    /// default.
    #[must_use]
    pub fn arguments(&self) -> &[(&'static str, Setting)] {
        &self.arguments
    }

    /// What the parameter `name` is set to, if the step starts with it.
    #[must_use]
    pub fn argument(&self, name: &str) -> Option<&Setting> {
        self.arguments
            .iter()
            .find(|(parameter, _)| *parameter == name)
            .map(|(_, setting)| setting)
    }
}

/// The method an aggregate of a pivot starts with: each cell of a pivot holds
/// the values of one group, and `first` takes the first of them, which is the
/// only one where a group has one row.
const PIVOT_AGGREGATE: &str = "first";

/// The word a new join starts with for `how`. Polars joins `inner` unless it is
/// asked, which drops the rows of the left table that have no match; `left`
/// keeps them.
const JOIN_HOW: &str = "left";

/// The comparison a new filter, and a new line of a condition, starts with: its field and what
/// it is compared with are still to be given.
const FILTER_COMPARISON: Comparison = Comparison::Equal;

impl Setting {
    /// A test that starts as a new filter's does, and a new line of a condition: an equality
    /// of a field and a value, each still to be given.
    #[must_use]
    pub(crate) fn started_test() -> Self {
        Self::Comparison {
            entry: FILTER_COMPARISON.entry(),
            field: Box::new(Self::ToGive),
            other: Box::new(Self::ToGive),
        }
    }
}

/// What the step the entry `entry` stands for starts with, on `frame`, or
/// `None` where the entry is not a step of the list.
///
/// A step starts with what it cannot be built without, each still to be given
/// where the engine cannot pick it: the keys of a sort or a join, the fields of
/// a pivot and what a new column holds. What can be picked is picked: a select
/// keeps every column, a filter starts with an equality of a field and a value
/// still to be given, a join and a pivot take the first connected table, a join
/// keeps the rows of the left table that have no match, which Polars does not
/// unless it is asked, and a pivot's cells hold their first value.
#[must_use]
pub fn start_step(entry: &Entry, frame: &StepFrame<'_>) -> Option<StepStart> {
    if entry.applies() != Applies::Frame {
        return None;
    }
    let start_of = |name: &'static str| {
        entry
            .parameters()
            .iter()
            .find(|parameter| parameter.name() == name)
            .map(|parameter| (name, parameter.start_setting()))
    };
    let first_table = || {
        frame
            .tables
            .first()
            .map_or(Setting::ToGive, |name| Setting::Word(name.clone()))
    };
    let arguments: Vec<(&'static str, Setting)> = match entry.id().as_str() {
        "LazyFrame.with_columns" => vec![(
            "named_exprs",
            Setting::Pairs(vec![(
                Setting::Text("new_column".to_owned()),
                Setting::ToGive,
            )]),
        )],
        "LazyFrame.filter" => vec![("predicates", Setting::List(vec![Setting::started_test()]))],
        "LazyFrame.sort" => vec![start_of("by")?],
        "LazyFrame.select" => vec![(
            "exprs",
            Setting::List(frame.columns.iter().cloned().map(Setting::Field).collect()),
        )],
        "LazyFrame.rename" => vec![start_of("mapping")?],
        "LazyFrame.join" => vec![
            ("other", first_table()),
            start_of("left_on")?,
            start_of("right_on")?,
            ("how", Setting::Word(JOIN_HOW.to_owned())),
        ],
        "LazyFrame.pivot" => vec![
            ("on", Setting::ToGive),
            ("on_columns", first_table()),
            ("index", Setting::ToGive),
            ("values", Setting::ToGive),
            ("aggregate_function", Setting::Element(PIVOT_AGGREGATE)),
        ],
        "LazyFrame.drop" => vec![start_of("columns")?],
        "LazyFrame.fill_nan" => vec![start_of("value")?],
        "LazyFrame.slice" => vec![start_of("offset")?],
        "LazyFrame.gather_every" => vec![start_of("n")?],
        // What Polars does with each of these as it is: five rows, every column, the rows as
        // they are, a count from zero.
        "LazyFrame.drop_nans"
        | "LazyFrame.drop_nulls"
        | "LazyFrame.first"
        | "LazyFrame.head"
        | "LazyFrame.last"
        | "LazyFrame.limit"
        | "LazyFrame.reverse"
        | "LazyFrame.tail"
        | "LazyFrame.with_row_index" => Vec::new(),
        _ => return None,
    };
    Some(StepStart { arguments })
}
