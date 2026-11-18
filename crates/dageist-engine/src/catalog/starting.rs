//! What the step editor starts from: what a parameter holds when it is not
//! given, what it is set to when it is first given, and what a new step is
//! given.
//!
//! These are Polars' defaults and the editor's choices of a first value, and
//! both are the engine's to say, so that no view reads them off a parameter
//! itself. A [`Setting`] is the one shape they are all given in, whatever kind
//! of parameter holds it.

use super::{Applies, DefaultValue, Entry, Kind, Parameter, Shape};

/// What a parameter holds, in the one shape every kind of parameter is drawn
/// from.
#[derive(Debug, Clone, PartialEq)]
pub enum Setting {
    /// Still to be given: it stands for nothing until it is.
    Unset,
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
            return Setting::List(vec![Setting::Unset]);
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
                    .map_or(Setting::Unset, |word| Setting::Word((*word).to_owned()))
            }
            _ => Setting::Unset,
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

/// What the step the entry `entry` stands for starts with, on `frame`, or
/// `None` where the entry is not a step of the list.
///
/// A step starts with what it cannot be built without, each still to be given
/// where the engine cannot pick it: the keys of a sort or a join, the fields of
/// a pivot, a filter's condition and what a new column holds. What can be
/// picked is picked: a select keeps every column, a join and a pivot take the
/// first connected table, a join keeps the rows of the left table that have no
/// match, which Polars does not unless it is asked, and a pivot's cells hold
/// their first value.
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
            .map_or(Setting::Unset, |name| Setting::Word(name.clone()))
    };
    let arguments: Vec<(&'static str, Setting)> = match entry.id().as_str() {
        "LazyFrame.with_columns" => vec![(
            "named_exprs",
            Setting::Pairs(vec![(
                Setting::Text("new_column".to_owned()),
                Setting::Unset,
            )]),
        )],
        "LazyFrame.filter" => vec![start_of("predicates")?],
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
            start_of("how")?,
        ],
        "LazyFrame.pivot" => vec![
            ("on", Setting::Unset),
            ("on_columns", first_table()),
            ("index", Setting::Unset),
            ("values", Setting::Unset),
            ("aggregate_function", Setting::Element(PIVOT_AGGREGATE)),
        ],
        _ => return None,
    };
    Some(StepStart { arguments })
}
