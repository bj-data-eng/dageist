//! What may be given where an expression, or the argument of a method, is
//! picked: a column of a table, a connected input, a number or text typed in.
//!
//! The editor picks such a value from a list and needs to know which of them
//! the tables allow there. The dtype of each is the engine's to decide, not the
//! caller's: a column and an input have the class of their dtype, and a typed
//! literal has the dtype Polars gives it when it is built, which is not always
//! the widest: a whole number is the narrowest integer it fits, a number with a
//! fraction is a float.

use polars::prelude::{DataFrame, DataType, IntoLazy};

use crate::catalog::{self, Argument, Build, DefaultValue, Entry, Kind, ParameterDefault};

use super::context::Context;
use super::dtype::Representative;
use super::offers::{
    After, Chosen, Place, Refusal, Verdict, default_word, offer, offer_input, options,
};
use super::table::Shape;

/// A number or text typed in, as a document writes it.
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    /// A whole number.
    Integer(i64),
    /// A number with a fraction, or too large for a whole number.
    Float(f64),
    /// Text.
    String(String),
}

impl Literal {
    /// The class of the dtype Polars gives the literal, worked out as the
    /// schema of the literal on a table with no rows: no data is read. `None`
    /// where Polars refuses the literal or gives it a dtype that has no class.
    #[must_use]
    pub fn class(&self) -> Option<Representative> {
        let literal = catalog::entry("polars.lit")?;
        let Build::Function(build) = literal.build() else {
            return None;
        };
        let value = match self {
            Self::Integer(value) => Argument::Integer(*value),
            Self::Float(value) => Argument::Float(*value),
            Self::String(value) => Argument::String(value.clone()),
        };
        let expression = build(&literal.arguments(vec![("value", value)]).ok()?).ok()?;
        let schema = DataFrame::empty()
            .lazy()
            .select([expression])
            .collect_schema()
            .ok()?;
        let (_, dtype): (_, &DataType) = schema.iter().next()?;
        Representative::of(dtype)
    }
}

/// A value that may be given where an expression, or an argument, is picked.
#[derive(Debug, Clone, PartialEq)]
pub enum Candidate {
    /// A column of the table the expression is read against, by the class of
    /// its dtype.
    Field(Representative),
    /// A connected value or list, by the class of its dtype.
    Input {
        /// The class of the dtype of the value, or of each value of the list.
        dtype: Representative,
        /// How many values it gives: one for a value, many for a list.
        shape: Shape,
    },
    /// A number or text typed in.
    Literal(Literal),
}

impl Candidate {
    /// The class of the dtype the candidate has, which is the engine's to say
    /// for a literal. `None` where it has none.
    #[must_use]
    pub fn class(&self) -> Option<Representative> {
        match self {
            Self::Field(dtype) | Self::Input { dtype, .. } => Some(*dtype),
            Self::Literal(literal) => literal.class(),
        }
    }
}

/// The argument a candidate would be given: a parameter of a method that
/// follows what the place says.
#[derive(Debug, Clone, Copy)]
pub struct ArgumentSlot<'a> {
    entry: &'static Entry,
    parameter: &'a str,
    method_place: Place,
}

impl<'a> ArgumentSlot<'a> {
    /// The parameter `parameter` of the method `entry`, where the method stands
    /// at `method_place`.
    #[must_use]
    pub const fn new(entry: &'static Entry, parameter: &'a str, method_place: Place) -> Self {
        Self {
            entry,
            parameter,
            method_place,
        }
    }
}

/// Where a candidate would be given: as an expression that stands in a context,
/// as the argument of a method, or as both, as `over`'s key is.
#[derive(Debug, Clone, Copy)]
pub struct Slot<'a> {
    context: Option<Context>,
    argument: Option<ArgumentSlot<'a>>,
}

impl<'a> Slot<'a> {
    /// The slot for an expression that stands in `context`, which is the
    /// argument `argument` of a method when it is given. `None` when neither is
    /// given: no place is then asked about.
    #[must_use]
    pub const fn new(context: Option<Context>, argument: Option<ArgumentSlot<'a>>) -> Option<Self> {
        if context.is_none() && argument.is_none() {
            return None;
        }
        Some(Self { context, argument })
    }
}

/// What the tables say of a candidate in a slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fit {
    dtype: Representative,
    verdict: Verdict,
}

impl Fit {
    /// The class of the dtype the candidate has.
    #[must_use]
    pub const fn dtype(&self) -> Representative {
        self.dtype
    }

    /// Whether the candidate may be given there, or why not. A candidate
    /// that stands as an expression is judged as the start of one: a column as
    /// `polars.col` of its dtype, a typed literal and a connected input as a
    /// literal, and the verdict says whether the expression may end there.
    /// Where the slot is only an argument, the verdict is the method's.
    #[must_use]
    pub const fn verdict(&self) -> &Verdict {
        &self.verdict
    }
}

/// Whether `candidate` may be given in `slot`: first as the argument of the
/// method, when it is one, and then as an expression that starts in the
/// context, when it stands in one. `None` where the candidate has no class, so
/// that the tables cannot judge it.
#[must_use]
pub fn fit(candidate: &Candidate, slot: &Slot<'_>) -> Option<Fit> {
    let dtype = candidate.class()?;
    let mut verdict = None;
    if let Some(argument) = slot.argument {
        let given = [(argument.parameter, dtype)];
        let found = offer(
            argument.entry,
            &argument.method_place,
            &Chosen::default().with_arguments(&given),
        );
        if !found.is_allowed() {
            return Some(Fit {
                dtype,
                verdict: found,
            });
        }
        verdict = Some(found);
    }
    if let Some(context) = slot.context {
        let place = Place::new(context, After::Start);
        let found = match candidate {
            Candidate::Field(_) => catalog::entry("polars.col").map_or_else(
                || Verdict::Refused(Refusal::structure("the catalogue has no polars.col")),
                |column| offer(column, &place, &Chosen::default().giving(dtype)),
            ),
            Candidate::Input { shape, .. } => offer_input(&place, dtype, *shape),
            Candidate::Literal(_) => offer_input(&place, dtype, Shape::One),
        };
        verdict = Some(found);
    }
    verdict.map(|verdict| Fit { dtype, verdict })
}

/// What an argument is given when a method is added, if its first parameter
/// takes an expression: Polars' default for it, when that is a number or text
/// that the tables allow there.
#[derive(Debug, Clone, PartialEq)]
pub struct DefaultArgument {
    parameter: &'static str,
    literal: Literal,
}

impl DefaultArgument {
    /// The parameter that is given it.
    #[must_use]
    pub const fn parameter(&self) -> &'static str {
        self.parameter
    }

    /// The number or text it is given.
    #[must_use]
    pub const fn literal(&self) -> &Literal {
        &self.literal
    }
}

/// The argument the method `entry` starts with where it stands at `place`:
/// Polars' default for its first parameter, when that takes an expression and
/// the default is a number or text that the tables allow as its argument there.
/// `None` where the first parameter has no such default, which is then left to be
/// picked.
#[must_use]
pub fn default_argument(entry: &'static Entry, place: &Place) -> Option<DefaultArgument> {
    let parameter = entry.parameters().first()?;
    if !matches!(parameter.kind(), Kind::Expression(_)) {
        return None;
    }
    let ParameterDefault::Python(_) = parameter.default() else {
        return None;
    };
    let literal = match parameter.default().value() {
        DefaultValue::Integer(value) => Literal::Integer(value),
        DefaultValue::Float(value) => Literal::Float(value),
        DefaultValue::Text(value) => Literal::String(value),
        _ => return None,
    };
    let slot = Slot::new(
        None,
        Some(ArgumentSlot::new(entry, parameter.name(), *place)),
    )?;
    fit(&Candidate::Literal(literal.clone()), &slot)?
        .verdict
        .is_allowed()
        .then_some(DefaultArgument {
            parameter: parameter.name(),
            literal,
        })
}

/// One word an option may be set to, and why Polars refuses it, if it does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionWord {
    word: String,
    refusal: Option<Refusal>,
}

impl OptionWord {
    /// The word as the option table writes it: `true`, `false`, or the word of
    /// a choice.
    #[must_use]
    pub fn word(&self) -> &str {
        &self.word
    }

    /// Why Polars refuses the option set to this word, with the others at their
    /// defaults. `None` when it does not.
    #[must_use]
    pub const fn refusal(&self) -> Option<&Refusal> {
        self.refusal.as_ref()
    }
}

/// An option of an entry, a flag or a choice, with its default and the words it
/// may be set to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionOffer {
    parameter: &'static str,
    default: Option<String>,
    words: Vec<OptionWord>,
}

impl OptionOffer {
    /// The parameter, by Python's name.
    #[must_use]
    pub const fn parameter(&self) -> &'static str {
        self.parameter
    }

    /// The word the option has when it is not set, as the option table writes
    /// it; `unset` for a flag Polars leaves to decide.
    #[must_use]
    pub fn default_word(&self) -> Option<&str> {
        self.default.as_deref()
    }

    /// The words it may be set to, in the order of the choice, `true` before
    /// `false` for a flag.
    #[must_use]
    pub fn words(&self) -> &[OptionWord] {
        &self.words
    }
}

/// The options of `entry`, its flags and its choices, each with its default
/// and, for each word it may be set to, what the option table says of it set
/// beside the options `chosen`, each by the parameter's name and its word as
/// the table writes it. The option being judged is not held to its own chosen
/// word, so that every word of it is judged against the others.
#[must_use]
pub fn option_offers(entry: &Entry, chosen: &[(&str, &str)]) -> Vec<OptionOffer> {
    entry
        .parameters()
        .iter()
        .filter_map(|parameter| {
            let words: Vec<String> = match parameter.kind() {
                Kind::Flag => vec!["true".to_owned(), "false".to_owned()],
                Kind::Choice(choices) => choices.iter().map(|word| (*word).to_owned()).collect(),
                _ => return None,
            };
            let others: Vec<(&str, &str)> = chosen
                .iter()
                .copied()
                .filter(|(name, _)| *name != parameter.name())
                .collect();
            Some(OptionOffer {
                parameter: parameter.name(),
                default: default_word(parameter),
                words: words
                    .into_iter()
                    .map(|word| {
                        let mut together = others.clone();
                        together.push((parameter.name(), word.as_str()));
                        OptionWord {
                            refusal: options(entry, &together).err(),
                            word,
                        }
                    })
                    .collect(),
            })
        })
        .collect()
}
