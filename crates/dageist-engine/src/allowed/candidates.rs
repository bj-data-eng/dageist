//! What may be given where an expression, or the argument of a method, is
//! picked: a column of a table, a connected input, a number or text typed in.
//!
//! The editor picks such a value from a list and needs to know which of them
//! the tables allow there. The dtype of each is the engine's to decide, not the
//! caller's: a column and an input have the class of their dtype, and a typed
//! literal has the dtype Polars gives it when it is built, which is not always
//! the widest: a whole number is the narrowest integer it fits, a number with a
//! fraction is a float.

use crate::catalog::{self, DefaultValue, Entry, Kind, ParameterDefault};

use super::adapt::{Loose, is_count, is_left_unresolved, is_required, operand_adapts};
use super::context::Context;
use super::dtype::Representative;
use super::literal::Literal;
use super::offers::{
    After, Chosen, Place, Refusal, Verdict, default_word, offer, offer_input, options,
    same_for_any_number,
};
use super::table::Shape;

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
    /// Where the slot is only an argument, the verdict is the method's. Where it
    /// is both, the candidate is refused where either refuses it, and otherwise
    /// the verdict has the result and the shape of the method's and, as its
    /// ending, whether the expression the candidate starts may end there.
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
        let loose = match candidate {
            Candidate::Literal(literal) => Loose::of(literal),
            _ => None,
        };
        let place = argument.method_place;
        // A number typed without a dtype adapts to what the method follows where it fits it, so
        // the tables are asked of it as that dtype.
        let adapted = match (loose, place.after()) {
            (Some(number), After::Value { dtype, .. })
                if operand_adapts(argument.entry, argument.parameter)
                    && !is_left_unresolved(
                        argument.entry,
                        argument.parameter,
                        number,
                        dtype,
                        place.context(),
                    ) =>
            {
                number.beside(dtype)
            }
            _ => None,
        };
        let given = [(argument.parameter, adapted.unwrap_or(dtype))];
        let found = offer(
            argument.entry,
            &place,
            &Chosen::default().with_arguments(&given),
        );
        let Verdict::Allowed(allowance) = found else {
            return Some(Fit {
                dtype,
                verdict: found,
            });
        };
        let counts =
            loose.is_some_and(Loose::is_whole) && is_count(argument.entry, argument.parameter);
        verdict = Some(Verdict::Allowed(
            if loose.is_none() || adapted.is_some() || counts {
                allowance
            } else if operand_adapts(argument.entry, argument.parameter)
                && is_required(argument.entry, argument.parameter)
                && let Some((result, shape)) = same_for_any_number(
                    argument.entry,
                    &place,
                    &Chosen::default(),
                    argument.parameter,
                )
            {
                // A number that does not adapt takes a dtype the tables cannot say, so the call gives
                // what it gives for every dtype it could take, where that is one: a comparison is a
                // `Boolean`.
                allowance.with_result(result, shape)
            } else {
                allowance.without_result()
            },
        ));
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
        // Either side may refuse it. Where both allow it, what the call it is given to gives, its
        // result and its shape, is the argument's, and whether the expression the candidate
        // starts may end there is the start's: the argument's ending says whether the result of
        // the call may end the expression the call stands in, which is not the candidate's chain.
        verdict = match (verdict, found) {
            (_, refused @ Verdict::Refused(_)) => Some(refused),
            (Some(Verdict::Allowed(argument)), Verdict::Allowed(start)) => Some(Verdict::Allowed(
                argument.ending_as(start.can_end().clone()),
            )),
            (_, allowed @ Verdict::Allowed(_)) => Some(allowed),
        };
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

    /// The word the option starts with when it is added: the first word, in the
    /// order of [`words`](Self::words), that is not its default and that Polars
    /// allows beside the options it was asked with. `None` where Polars refuses
    /// every word but the default, so that adding the option would change nothing
    /// it can be given.
    #[must_use]
    pub fn starter(&self) -> Option<&str> {
        self.words
            .iter()
            .find(|word| {
                self.default.as_deref() != Some(word.word.as_str()) && word.refusal.is_none()
            })
            .map(|word| word.word.as_str())
    }

    /// Why the option cannot be added, where it has no [`starter`](Self::starter):
    /// the refusal Polars gives the first word that is not its default, which
    /// is then refused beside the options it was asked with like every other, or
    /// a refusal that there is no other word. `None` where the option can be
    /// added.
    #[must_use]
    pub fn stuck(&self) -> Option<Refusal> {
        if self.starter().is_some() {
            return None;
        }
        let other = self
            .words
            .iter()
            .find(|word| self.default.as_deref() != Some(word.word.as_str()));
        Some(
            other
                .and_then(|word| word.refusal.clone())
                .unwrap_or_else(|| Refusal::structure("there is nothing else to set it to")),
        )
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::allowed::RefusalSource;

    #[test]
    fn a_choice_with_no_word_but_its_default_cannot_be_added_and_says_there_is_none() {
        let offer = OptionOffer {
            parameter: "only",
            default: Some("a".to_owned()),
            words: vec![OptionWord {
                word: "a".to_owned(),
                refusal: None,
            }],
        };

        let stuck = offer.stuck().expect("nothing else to set it to");

        assert_eq!(offer.starter(), None);
        assert_eq!(stuck.source(), RefusalSource::Structure);
    }
}
