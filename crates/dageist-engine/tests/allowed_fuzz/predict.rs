//! What the tables and the named rules say of a case, without running Polars.
//!
//! The walk follows the document as running it does: the arguments of a call
//! first, each expression start by link, the way it is placed in the named
//! positions that hold it. Every call of an expression is asked of
//! [`allowed::offer`], at the place the chain has come to, with the dtypes of
//! the arguments the case chose for its expression parameters; every
//! expression a position holds is asked whether it may end there
//! ([`Allowance::can_end`]); and the options a call is given are asked of
//! [`allowed::options`]. The case is predicted allowed when nothing is refused.
//!
//! The tables are measured, so what they cannot say is not guessed: a chain
//! whose dtype is not known where a method follows it is *undetermined* from
//! there on, and a case that is only undetermined is compared for what was
//! predicted before that.

use dageist_engine::allowed::{
    self, After, Chosen, ConditionState, Context, Place as OfferPlace, Position as Slot,
    RefusalSource, Representative, Shape, Verdict as OfferVerdict,
};
use dageist_engine::catalog::{Applies, Entry, Kind};
use dageist_engine::document::{Call, Expression, Place, Segment, Start, Value};

use super::case::{Case, STEP};
use super::frames::column_class;

/// Who refused a case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// Polars, as the tables recorded it.
    Polars,
    /// A named rule, which Polars does not hold.
    Rule(&'static str),
    /// The tables hold nothing for what was asked: a gap in them, or a case
    /// they cannot be asked about.
    Silent,
}

/// A refusal the tables and the rules give, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    pub source: Source,
    /// Where, in words.
    pub at: String,
    /// Where, in the document.
    pub place: Place,
    /// The stage at which Polars refused it, as the tables recorded it, when
    /// Polars did.
    pub stage: Option<allowed::Stage>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Every call may stand where it does, and every chain may end where it
    /// ends.
    Allows,
    /// Something is refused.
    Refuses(Refused),
    /// Nothing is refused, but the tables could not be asked past a point.
    Undetermined(String),
}

/// What the tables say an expression has at its start and after each link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Traced {
    pub place: Place,
    /// How many positions inside an expression hold it, of which the tables
    /// judge the first.
    pub nested: usize,
    pub start: Option<Representative>,
    pub links: Vec<Option<Representative>>,
    /// Whether the dtype after the last link is one the tables gave, and not one
    /// the walk worked out: a start has the dtype of its column, and a condition
    /// the dtype of its values, which [`allowed::offer`] is not told.
    pub told: bool,
}

#[derive(Debug, Clone)]
pub struct Prediction {
    pub verdict: Verdict,
    pub expressions: Vec<Traced>,
    /// Whether a call was given an option that changes what a method gives, so
    /// that the dtype after it is not predicted (`over`'s `mapping_strategy` as
    /// `join`, `dt.total_days`'s `fractional`, which `offer` is not told).
    pub stopped_by_options: bool,
}

/// What a chain has so far.
#[derive(Clone)]
enum Cur {
    Value {
        dtype: Option<Representative>,
        shape: Shape,
    },
    Condition {
        state: ConditionState,
        values: Vec<Option<Representative>>,
    },
    /// Refused: nothing more is asked of it.
    Dead,
}

struct Walker<'a> {
    case: &'a Case,
    base: Place,
    path: Vec<Segment>,
    slots: Vec<Slot>,
    traced: Vec<Traced>,
    refusal: Option<Refused>,
    undetermined: Option<String>,
    stopped_by_options: bool,
    /// Whether the last call walked gave its dtype from the tables.
    told: bool,
}

pub fn predict(case: &Case) -> Prediction {
    let step = &case.document.steps()[0];
    let mut walker = Walker {
        case,
        base: Place::of_step(0, Some(step.id().clone())),
        path: Vec::new(),
        slots: Vec::new(),
        traced: Vec::new(),
        refusal: None,
        undetermined: None,
        stopped_by_options: false,
        told: false,
    };
    walker.arguments(step.call());
    walker.options(step.call());
    let verdict = match (walker.refusal, walker.undetermined) {
        (Some(refused), _) => Verdict::Refuses(refused),
        (None, Some(reason)) => Verdict::Undetermined(reason),
        (None, None) => Verdict::Allows,
    };
    debug_assert_eq!(step.id().as_str(), STEP);
    Prediction {
        verdict,
        expressions: walker.traced,
        stopped_by_options: walker.stopped_by_options,
    }
}

/// The word of a flag or a choice, as the option table writes it.
fn option_word(value: &Value) -> Option<String> {
    match value {
        Value::Flag(Some(true)) => Some("true".to_owned()),
        Value::Flag(Some(false)) => Some("false".to_owned()),
        Value::Flag(None) => Some("unset".to_owned()),
        Value::Choice(word) => Some(word.clone()),
        _ => None,
    }
}

impl Walker<'_> {
    fn here(&self) -> Place {
        self.path
            .iter()
            .cloned()
            .fold(self.base.clone(), Place::then)
    }

    fn refuse(&mut self, source: Source, stage: Option<allowed::Stage>, reason: String) {
        if self.refusal.is_none() {
            self.refusal = Some(Refused {
                source,
                at: self.here().to_string(),
                place: self.here(),
                stage,
                reason,
            });
        }
    }

    fn blind(&mut self, reason: impl Into<String>) {
        if self.undetermined.is_none() {
            self.undetermined = Some(format!("{}: {}", self.here(), reason.into()));
        }
    }

    /// The options `call` was given, asked of the option table.
    fn options(&mut self, call: &Call) {
        let entry = call.entry();
        if !matches!(entry.applies(), Applies::Expression | Applies::Frame) {
            return;
        }
        let mut chosen: Vec<(&str, String)> = Vec::new();
        for given in call.arguments() {
            let is_option = entry.parameters().iter().any(|parameter| {
                parameter.name() == given.parameter()
                    && matches!(parameter.kind(), Kind::Flag | Kind::Choice(_))
            });
            if is_option && let Some(word) = option_word(given.value()) {
                chosen.push((given.parameter(), word));
            }
        }
        let chosen: Vec<(&str, &str)> = chosen
            .iter()
            .map(|(name, word)| (*name, word.as_str()))
            .collect();
        if let Err(refusal) = allowed::options(entry, &chosen) {
            self.refused(&refusal);
        }
    }

    /// Walks the arguments of `call`, and gives the dtype of each expression
    /// it holds, by the parameter that holds it.
    fn arguments(&mut self, call: &Call) -> Vec<(String, Option<Representative>)> {
        let entry = call.entry().id().as_str();
        let mut found = Vec::new();
        for given in call.arguments() {
            self.path
                .push(Segment::Parameter(given.parameter().to_owned()));
            let slot = Slot::of_parameter(entry, given.parameter());
            self.slots.extend(slot);
            self.value(given.value(), given.parameter(), slot.is_some(), &mut found);
            if slot.is_some() {
                self.slots.pop();
            }
            self.path.pop();
        }
        found
    }

    fn value(
        &mut self,
        value: &Value,
        parameter: &str,
        held: bool,
        found: &mut Vec<(String, Option<Representative>)>,
    ) {
        match value {
            Value::Expression(expression) => {
                let dtype = self.expression(expression, held);
                found.push((parameter.to_owned(), dtype));
            }
            Value::Many(items) => {
                for (at, item) in items.iter().enumerate() {
                    self.path.push(Segment::Item(at));
                    self.value(item, parameter, held, found);
                    self.path.pop();
                }
            }
            Value::Pairs(pairs) => {
                for (at, (key, entry)) in pairs.iter().enumerate() {
                    self.path.push(Segment::Key(at));
                    self.value(key, parameter, held, found);
                    self.path.pop();
                    self.path.push(Segment::Value(at));
                    self.value(entry, parameter, held, found);
                    self.path.pop();
                }
            }
            _ => {}
        }
    }

    /// The class of the first column an argument names.
    fn column_of(&self, call: &Call) -> Option<Representative> {
        match call.argument("name") {
            Some(Value::Columns(names)) => {
                column_class(names.first()?, self.case.receiver, self.case.argument)
            }
            _ => None,
        }
    }

    /// The class a literal has, when its dtype is known without another
    /// operand: one it was given, or text, a flag, or a null, which have one.
    fn literal_class(call: &Call) -> Option<Representative> {
        if let Some(Value::Dtype(dtype)) = call.argument("dtype") {
            return Representative::of(dtype.as_polars());
        }
        match call.argument("value")? {
            Value::String(_) => Some(Representative::String),
            Value::Flag(Some(_)) => Some(Representative::Boolean),
            Value::Flag(None) => Some(Representative::Null),
            _ => None,
        }
    }

    fn expression(&mut self, expression: &Expression, held: bool) -> Option<Representative> {
        let place = self.here();
        let context = Context::from_positions(&self.slots);
        let mut start_class = None;
        let mut last_end: Option<allowed::Refusal> = None;
        let mut cur = Cur::Dead;
        let Start::Call(start) = expression.start() else {
            self.blind("a connected input starts the chain");
            return None;
        };
        self.path.push(Segment::Start);
        self.arguments(start);
        self.options(start);
        self.path.pop();
        let Some(context) = context else {
            self.blind("no named position holds the expression");
            return None;
        };
        self.path.push(Segment::Start);
        match allowed::offer(
            start.entry(),
            &OfferPlace::new(context, After::Start),
            &Chosen::default(),
        ) {
            OfferVerdict::Refused(refusal) => self.refused(&refusal),
            OfferVerdict::Allowed(allowance) => {
                last_end = allowance.can_end().err().cloned();
                match start.entry().id().as_str() {
                    "polars.when" => {
                        cur = Cur::Condition {
                            state: ConditionState::When,
                            values: Vec::new(),
                        };
                    }
                    id => {
                        let dtype = match id {
                            "polars.col" => self.column_of(start),
                            "polars.lit" => Self::literal_class(start),
                            "polars.element" => Some(self.case.receiver),
                            _ => None,
                        };
                        start_class = dtype;
                        cur = Cur::Value {
                            dtype,
                            shape: allowance.shape(),
                        };
                    }
                }
            }
        }
        self.path.pop();
        self.told = false;
        let mut links: Vec<Option<Representative>> = Vec::new();
        for (at, link) in expression.links().iter().enumerate() {
            self.path.push(Segment::Link(at));
            let arguments = self.arguments(link);
            self.options(link);
            cur = self.link(link, arguments, context, cur, &mut last_end);
            self.path.pop();
            links.push(match &cur {
                Cur::Value { dtype, .. } => *dtype,
                _ => None,
            });
        }
        if held
            && !matches!(cur, Cur::Dead)
            && let Some(refusal) = last_end
        {
            self.refuse_by_end(&refusal);
        }
        self.traced.push(Traced {
            place,
            nested: self
                .slots
                .iter()
                .filter(|slot| matches!(slot, Slot::Nested(_)))
                .count(),
            start: start_class,
            links,
            told: self.told,
        });
        match cur {
            Cur::Value { dtype, .. } => dtype,
            Cur::Condition { values, .. } => unify(&values),
            Cur::Dead => None,
        }
    }

    fn refused(&mut self, refusal: &allowed::Refusal) {
        let (source, stage) = match refusal.source() {
            RefusalSource::Polars(stage) => (Source::Polars, Some(stage)),
            RefusalSource::Rule(rule) => (Source::Rule(rule.name()), None),
            RefusalSource::Structure => (Source::Silent, None),
        };
        self.refuse(source, stage, refusal.reason().to_owned());
    }

    /// The expression may not end where it does: a rule's words, or Polars' as
    /// the table of contexts recorded it for a chain that ends.
    fn refuse_by_end(&mut self, refusal: &allowed::Refusal) {
        let (source, stage) = match refusal.source() {
            RefusalSource::Polars(stage) => (Source::Polars, Some(stage)),
            RefusalSource::Rule(rule) => (Source::Rule(rule.name()), None),
            RefusalSource::Structure => (Source::Silent, None),
        };
        self.refuse(
            source,
            stage,
            format!("it cannot end here: {}", refusal.reason()),
        );
    }

    fn link(
        &mut self,
        link: &Call,
        arguments: Vec<(String, Option<Representative>)>,
        context: Context,
        cur: Cur,
        last_end: &mut Option<allowed::Refusal>,
    ) -> Cur {
        let entry: &'static Entry = link.entry();
        let after = match &cur {
            Cur::Dead => return Cur::Dead,
            Cur::Value { dtype: None, shape } => {
                self.blind(format!(
                    "the dtype before {} is not known",
                    entry.id().as_str()
                ));
                return Cur::Value {
                    dtype: None,
                    shape: *shape,
                };
            }
            Cur::Value {
                dtype: Some(dtype),
                shape,
            } => After::Value {
                dtype: *dtype,
                shape: *shape,
            },
            Cur::Condition { state, .. } => After::Condition(*state),
        };
        let given: Vec<(&str, Representative)> = arguments
            .iter()
            .filter_map(|(name, dtype)| dtype.map(|dtype| (name.as_str(), dtype)))
            .collect();
        // What the tables are not told is not guessed: an argument whose dtype
        // is not known leaves the call to be offered for any dtype that fits,
        // which says it stands, not that this argument fits.
        if given.len() < arguments.len() {
            self.blind(format!(
                "the dtype of an argument of {} is not known",
                entry.id().as_str()
            ));
            // The call is still offered, for a refusal is a refusal whatever
            // the argument, but nothing is said of what it gives.
            if let OfferVerdict::Refused(refusal) = allowed::offer(
                entry,
                &OfferPlace::new(context, after),
                &Chosen::default().with_arguments(&given),
            ) {
                self.refused(&refusal);
                return Cur::Dead;
            }
            return Cur::Value {
                dtype: None,
                shape: Shape::Many,
            };
        }
        let allowance = match allowed::offer(
            entry,
            &OfferPlace::new(context, after),
            &Chosen::default().with_arguments(&given),
        ) {
            OfferVerdict::Refused(refusal) => {
                self.refused(&refusal);
                return Cur::Dead;
            }
            OfferVerdict::Allowed(allowance) => allowance,
        };
        *last_end = allowance.can_end().err().cloned();
        let statement = arguments
            .iter()
            .find(|(name, _)| name == "statement")
            .and_then(|(_, dtype)| *dtype);
        match (entry.applies(), cur) {
            (Applies::Expression, _) => {
                // Documented limit ([`super::limits::Limit::OptionsChangeTheResult`]):
                // `options.txt` records that `over`'s `mapping_strategy=join` gives
                // a `List` and `dt.total_days(fractional=true)` a `Float64`, but
                // `offer` is asked of a method without its options and gives what
                // it gives at their defaults, so the dtype after it is not asked of
                // the tables.
                let changed =
                    matches!(
                        link.argument("mapping_strategy"),
                        Some(Value::Choice(word)) if word == "join"
                    ) || matches!(link.argument("fractional"), Some(Value::Flag(Some(true))));
                self.stopped_by_options |= changed;
                self.told = allowance.result().is_some() && !changed;
                let dtype = allowance
                    .result()
                    .and_then(allowed::ResultDtype::class)
                    .filter(|_| !changed);
                Cur::Value {
                    dtype,
                    shape: allowance.shape(),
                }
            }
            (Applies::Condition, Cur::Condition { mut values, .. }) => {
                self.told = false;
                let next = match entry.id().as_str() {
                    "When.then" => {
                        values.push(statement);
                        ConditionState::Then
                    }
                    "ChainedWhen.then" => {
                        values.push(statement);
                        ConditionState::ChainedThen
                    }
                    "Then.when" | "ChainedThen.when" => ConditionState::ChainedWhen,
                    _ => {
                        values.push(statement);
                        return Cur::Value {
                            dtype: unify(&values),
                            shape: Shape::Many,
                        };
                    }
                };
                Cur::Condition {
                    state: next,
                    values,
                }
            }
            _ => Cur::Dead,
        }
    }
}

/// The class every value of a condition has, when they have the same one. A
/// condition of values of different dtypes has the supertype, which the tables
/// do not give.
fn unify(values: &[Option<Representative>]) -> Option<Representative> {
    let first = (*values.first()?)?;
    values
        .iter()
        .all(|value| *value == Some(first))
        .then_some(first)
}
