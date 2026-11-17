//! What the tables of allowed combinations say of a step, without running
//! Polars: where each expression in it stands, what each call of it follows,
//! whether each call may stand there and whether the expression may end.
//!
//! The walk follows the document as running it does: the arguments of a call
//! first, each expression start by link, the way it is placed in the named
//! positions that hold it. Every call of an expression is asked of
//! [`allowed::offer`], at the place the chain has come to, with the dtypes of
//! the arguments chosen for its expression parameters, the options it is given,
//! and, for a start, the dtype of the column or literal it reads. Every
//! expression that a position holds is asked whether it may end there, and a
//! join's key whether it holds an aggregate beside a column, or a column compared
//! with a null. A join's keys are also asked whether they pair the same two output
//! names twice, which Polars refuses, and `on` is walked against both tables, as
//! Polars reads it. The options a step is given are asked of [`allowed::options`].
//!
//! The tables are measured, so what they cannot say is not guessed: a chain
//! whose dtype is not known where a method follows it is not asked from there
//! on, and the walk says it cannot tell.

use polars::prelude::{Schema, SchemaRef};

use crate::allowed::is_count;
use crate::allowed::{
    self, After, Chosen, ConditionState, Context, Ending, Literal, Nested, Outer,
    Place as OfferPlace, Position as Slot, Refusal, Representative, Shape, Unknown, Verdict,
};
use crate::catalog::{Applies, Dtypes, Entry, Kind};

use super::error::{Place, Segment};
use super::inputs::Inputs;
use super::model::{Call, Expression, Start, Step, Value};

/// A call of an expression: what it follows, and whether it may stand there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Standing {
    follows: Option<After>,
    verdict: Option<Verdict>,
}

impl Standing {
    const fn not_asked() -> Self {
        Self {
            follows: None,
            verdict: None,
        }
    }

    /// What the call follows: nothing for a start, otherwise the value or the
    /// condition before it. `None` where that is not known, because the dtype
    /// of what comes before it is not.
    #[must_use]
    pub const fn follows(&self) -> Option<After> {
        self.follows
    }

    /// Whether the call may stand there and what it gives, as [`allowed::offer`]
    /// answered. `None` where it was not asked. Where the dtype of an argument
    /// was not known it was asked with the arguments that were, so what it says
    /// the call gives is not what the chain has; the places say that.
    #[must_use]
    pub const fn verdict(&self) -> Option<&Verdict> {
        self.verdict.as_ref()
    }
}

/// An expression of a step, as the walk found it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkedExpression {
    place: Place,
    context: Option<Context>,
    calls: Vec<Standing>,
    next: Option<After>,
    ending: Option<Ending>,
}

impl WalkedExpression {
    /// Where the expression is: the step, then the way to the argument that
    /// holds it.
    #[must_use]
    pub const fn place(&self) -> &Place {
        &self.place
    }

    /// The context the expression stands in. `None` when no named position
    /// holds it.
    #[must_use]
    pub const fn context(&self) -> Option<Context> {
        self.context
    }

    /// The start of the expression, then each of its links.
    #[must_use]
    pub fn calls(&self) -> &[Standing] {
        &self.calls
    }

    /// The places the links stand in, in order, and after them the place a link
    /// added at the end would stand in. `None` where the context or what comes
    /// before is not known. They are what [`allowed::offers`] is asked at.
    #[must_use]
    pub fn places(&self) -> Vec<Option<OfferPlace>> {
        let follows = self
            .calls
            .iter()
            .skip(1)
            .map(Standing::follows)
            .chain(std::iter::once(self.next));
        follows
            .map(|after| Some(OfferPlace::new(self.context?, after?)))
            .collect()
    }

    /// Whether the expression may end where it does, when a position holds it
    /// and the walk could ask.
    #[must_use]
    pub const fn ending(&self) -> Option<&Ending> {
        self.ending.as_ref()
    }
}

/// A refusal the walk found, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkRefusal {
    place: Place,
    refusal: Refusal,
    at_end: bool,
    argument_unknown: bool,
}

impl WalkRefusal {
    /// Where, in the document.
    #[must_use]
    pub const fn place(&self) -> &Place {
        &self.place
    }

    /// Who refused it, and why.
    #[must_use]
    pub const fn refusal(&self) -> &Refusal {
        &self.refusal
    }

    /// Whether the expression there may not end where it does, and not that a
    /// call may not stand.
    #[must_use]
    pub const fn at_end(&self) -> bool {
        self.at_end
    }

    /// Whether the call was refused without the dtype of an argument being known:
    /// no dtype that argument might have lets the call stand, and the reason is
    /// the one Polars gave for one of them, not for the dtype the argument has.
    #[must_use]
    pub const fn argument_unknown(&self) -> bool {
        self.argument_unknown
    }
}

/// What the tables say of a step: its expressions, and what is refused.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Walk {
    expressions: Vec<WalkedExpression>,
    refusal: Option<WalkRefusal>,
    not_known: Option<String>,
}

impl Walk {
    /// The expressions of the step, those inside another expression's
    /// arguments first.
    #[must_use]
    pub fn expressions(&self) -> &[WalkedExpression] {
        &self.expressions
    }

    /// The first thing refused: a call that may not stand, an expression that
    /// may not end, or an option Polars refuses.
    #[must_use]
    pub const fn refusal(&self) -> Option<&WalkRefusal> {
        self.refusal.as_ref()
    }

    /// Why the tables could not be asked past a point, where they could not.
    #[must_use]
    pub fn not_known(&self) -> Option<&str> {
        self.not_known.as_deref()
    }
}

impl Step {
    /// What the tables of allowed combinations say of the step as the
    /// `index`th of a document, when it starts from a table with the schema
    /// `source`. `inputs` are what its `table` arguments name.
    #[must_use]
    pub fn walk(&self, index: usize, source: &Schema, inputs: &Inputs) -> Walk {
        let other = self
            .call()
            .argument("other")
            .and_then(|value| match value {
                Value::Table(name) => inputs.table(name),
                _ => None,
            })
            .and_then(|mut table| table.collect_schema().ok());
        let mut walker = Walker {
            step: self.call(),
            source,
            inputs,
            other,
            base: Place::of_step(index, Some(self.id().clone())),
            path: Vec::new(),
            slots: Vec::new(),
            walked: Vec::new(),
            refusal: None,
            not_known: None,
        };
        walker.arguments(self.call(), false);
        walker.step_options(self.call());
        walker.key_names();
        Walk {
            expressions: walker.walked,
            refusal: walker.refusal,
            not_known: walker.not_known,
        }
    }
}

/// What an expression held by an argument has: the class it gives, if known,
/// and whether it gives one value.
#[derive(Clone, Copy)]
struct Operand {
    dtype: Option<Representative>,
    /// Whether it is a number typed without a dtype and alone, which adapts to
    /// the operand it is used with.
    loose: Option<Loose>,
    shape: Shape,
    /// Whether the expression was walked to its end: a refused or unwalked one
    /// has no shape either.
    known: bool,
}

/// A number typed without a dtype, which is a dyn literal: Polars gives it the
/// dtype of the operand it is used with where it fits, and its own alone. The
/// tables are measured with typed columns, so they cannot say what it gives
/// beside another operand.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Loose {
    /// A whole number, by its value.
    Integer(i64),
    /// A number with a fraction.
    Float,
}

/// The class of one value of a condition, and whether it is a number typed
/// without a dtype, which adapts to the numbers beside it.
type Valued = (Option<Representative>, Option<Loose>);

/// What surrounds a link of an expression besides the chain before it.
#[derive(Clone, Copy)]
struct Around {
    /// Whether the values of the condition the link is part of are one expression.
    branches_are_one: bool,
    /// What the chain starts from, when that is a number typed without a dtype and the
    /// link follows it.
    receiver_loose: Option<Loose>,
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
        values: Vec<Valued>,
        /// Whether every predicate and value so far gives one value, which a
        /// condition of them gives too.
        single: bool,
        /// Whether every value so far gives one value: what the condition is
        /// where its values are one expression, and the predicates are dropped.
        values_single: bool,
        /// Whether every predicate and value so far was walked to its end: a
        /// refused one has no shape, so the condition has none.
        known: bool,
    },
    /// Refused: nothing more is asked of it.
    Dead,
}

struct Walker<'a> {
    step: &'a Call,
    source: &'a Schema,
    inputs: &'a Inputs,
    other: Option<SchemaRef>,
    base: Place,
    path: Vec<Segment>,
    slots: Vec<Slot>,
    walked: Vec<WalkedExpression>,
    refusal: Option<WalkRefusal>,
    not_known: Option<String>,
}

/// The output names of a join's keys before the first whose name is not known, in
/// order. Polars expands each side's keys before it pairs them, so a key that may
/// expand to several columns, or that is not picked yet, shifts every pair after
/// it: only the keys before it pair by position. Such a key is one whose name is
/// not known, as is an item that is no expression.
fn known_key_names(value: &Value) -> Vec<String> {
    let names = |items: &[&Value]| {
        items
            .iter()
            .map_while(|item| match item {
                Value::Expression(expression) => key_name(expression),
                _ => None,
            })
            .collect()
    };
    match value {
        Value::Many(items) => names(&items.iter().collect::<Vec<_>>()),
        other => names(&[other]),
    }
}

/// Whether the parameter `parameter` of `step` holds a list of keys, whose items
/// are each a place.
fn left_is_many(step: &Call, parameter: &str) -> bool {
    matches!(step.argument(parameter), Some(Value::Many(_)))
}

/// The output name Polars gives a join's key: the column the expression reads
/// first, whatever methods follow it, `literal` for a literal, and for a
/// condition the name of the first value it gives. `None` where it is not known:
/// a start that expands to several columns or names them by a pattern, an input,
/// a hole. A key is never given a name by an alias, which Polars refuses in a
/// join's key and the catalogue does not hold.
fn key_name(key: &Expression) -> Option<String> {
    let Start::Call(start) = key.start() else {
        return None;
    };
    match start.entry().id().as_str() {
        "polars.col" => {
            if Walker::names_several(start) {
                return None;
            }
            let Some(Value::Columns(names)) = start.argument("name") else {
                return None;
            };
            let name = names.first()?;
            (!(name.starts_with('^') && name.ends_with('$'))).then(|| name.clone())
        }
        "polars.lit" => Some("literal".to_owned()),
        "polars.when" => key
            .links()
            .iter()
            .find(|link| link.entry().id().as_str() == "When.then")
            .and_then(|link| match link.argument("statement") {
                Some(Value::Expression(value)) => key_name(value),
                _ => None,
            }),
        _ => None,
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

/// The options `call` was given, each by the parameter and its word as the
/// option table writes it.
fn option_words(call: &Call) -> Vec<(String, String)> {
    let entry = call.entry();
    call.arguments()
        .iter()
        .filter(|given| {
            entry.parameters().iter().any(|parameter| {
                parameter.name() == given.parameter()
                    && matches!(parameter.kind(), Kind::Flag | Kind::Choice(_))
            })
        })
        .filter_map(|given| {
            option_word(given.value()).map(|word| (given.parameter().to_owned(), word))
        })
        .collect()
}

/// The class a literal has, when its dtype is known without another operand:
/// one it was given, or text, a flag, or a null, which have one.
fn literal_class(call: &Call) -> Option<Representative> {
    if let Some(Value::Dtype(dtype)) = call.argument("dtype") {
        return Representative::of(dtype.as_polars());
    }
    match call.argument("value")? {
        Value::String(_) => Some(Representative::String),
        Value::Flag(Some(_)) => Some(Representative::Boolean),
        Value::Flag(None) => Some(Representative::Null),
        // A number typed without a dtype has the one Polars gives it by itself.
        Value::Integer(number) => Literal::Integer(*number).class(),
        Value::Float(number) => Literal::Float(*number).class(),
        _ => None,
    }
}

/// What `call` is when it is a number typed without a dtype.
fn loose_number(call: &Call) -> Option<Loose> {
    if call.entry().id().as_str() != "polars.lit" || call.argument("dtype").is_some() {
        return None;
    }
    match call.argument("value") {
        Some(Value::Integer(number)) => Some(Loose::Integer(*number)),
        Some(Value::Float(_)) => Some(Loose::Float),
        _ => None,
    }
}

/// Whether a method takes an operand besides what it follows, which a number
/// typed without a dtype adapts to: arithmetic, comparisons, a fill value.
fn takes_an_operand(entry: &Entry) -> bool {
    entry.applies() == Applies::Expression
        && entry.parameters().iter().any(|parameter| {
            matches!(parameter.kind(), Kind::Expression(dtypes) if dtypes != [Dtypes::Integer])
        })
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

/// The class every value of a condition has, when they have the same one. A
/// number typed without a dtype adapts to the values beside it that are typed,
/// as Polars gives it: a whole number to whole numbers and numbers with a
/// fraction it fits, and a number with a fraction only to numbers with a
/// fraction. Among such numbers alone it has no class. A condition of values
/// of different dtypes has the supertype, which the tables do not give.
fn unify(values: &[Valued]) -> Option<Representative> {
    let classes: Option<Vec<(Representative, Option<Loose>)>> = values
        .iter()
        .map(|(class, loose)| class.map(|class| (class, *loose)))
        .collect();
    let classes = classes?;
    let first_strict = classes
        .iter()
        .find(|(_, loose)| loose.is_none())
        .map(|(class, _)| *class);
    // Numbers typed without a dtype alone have none to give: Polars cannot resolve them where
    // it runs, in a sort key, for one.
    let first = first_strict?;
    let same = |(class, loose): &(Representative, Option<Loose>)| match loose {
        None => *class == first,
        Some(Loose::Integer(value)) => integer_fits(*value, first),
        Some(Loose::Float) => is_float(first),
    };
    classes.iter().all(same).then_some(first)
}

impl Walker<'_> {
    fn here(&self) -> Place {
        self.path
            .iter()
            .cloned()
            .fold(self.base.clone(), Place::then)
    }

    fn refuse(&mut self, refusal: &Refusal, at_end: bool) {
        self.refuse_given(refusal, at_end, false);
    }

    /// Refuses a call that was asked without the dtype of one of its arguments.
    fn refuse_blind(&mut self, refusal: &Refusal) {
        self.refuse_given(refusal, false, true);
    }

    fn refuse_given(&mut self, refusal: &Refusal, at_end: bool, argument_unknown: bool) {
        if self.refusal.is_none() {
            self.refusal = Some(WalkRefusal {
                place: self.here(),
                refusal: refusal.clone(),
                at_end,
                argument_unknown,
            });
        }
    }

    fn blind(&mut self, reason: impl Into<String>) {
        if self.not_known.is_none() {
            self.not_known = Some(format!("{}: {}", self.here(), reason.into()));
        }
    }

    /// The options a step was given, asked of the option table. The options of
    /// a method are asked of [`allowed::offer`], with the call.
    fn step_options(&mut self, step: &Call) {
        let chosen = option_words(step);
        let chosen: Vec<(&str, &str)> = chosen
            .iter()
            .map(|(name, word)| (name.as_str(), word.as_str()))
            .collect();
        if let Err(refusal) = allowed::options(step.entry(), &chosen) {
            self.refuse(&refusal, false);
        }
    }

    /// Asks whether a join's keys pair the same two output names twice, which
    /// Polars refuses, at the later key that makes the pair again. Only the pairs
    /// before the first key on either side whose name is not known are judged:
    /// Polars pairs the keys after expanding each side, and a key that may expand
    /// or is not picked yet shifts every pair after it.
    fn key_names(&mut self) {
        if self.step.entry().id().as_str() != "LazyFrame.join" {
            return;
        }
        let on = self.step.argument("on");
        let left = self.step.argument("left_on").or(on);
        let right = self.step.argument("right_on").or(on);
        let (Some(left), Some(right)) = (left, right) else {
            return;
        };
        let left_parameter = if self.step.argument("left_on").is_some() {
            "left_on"
        } else {
            "on"
        };
        let (left, right) = (known_key_names(left), known_key_names(right));
        let mut paired: Vec<(&String, &String)> = Vec::new();
        for (at, pair) in left.iter().zip(&right).enumerate() {
            if !paired.contains(&pair) {
                paired.push(pair);
                continue;
            }
            let context = Context::step(Outer::JoinLeft);
            let Err(refusal) = allowed::rule_forbids_a_repeated_pair_of_key_names(context) else {
                return;
            };
            let mut path = vec![Segment::Parameter(left_parameter.to_owned())];
            if left_is_many(self.step, left_parameter) {
                path.push(Segment::Item(at));
            }
            let place = path.into_iter().fold(self.base.clone(), Place::then);
            // A key already refused keeps its refusal: this one is the later reason.
            if let Some(walked) = self
                .walked
                .iter_mut()
                .find(|walked| walked.place == place && walked.context == Some(context))
                && !matches!(walked.ending, Some(Ending::Refused(_)))
            {
                walked.ending = Some(Ending::Refused(refusal.clone()));
            }
            if self.refusal.is_none() {
                self.refusal = Some(WalkRefusal {
                    place,
                    refusal,
                    at_end: true,
                    argument_unknown: false,
                });
            }
            return;
        }
    }

    /// Walks the arguments of `call`, and gives what each expression it holds
    /// has, by the parameter that holds it. `values_end` says whether the values
    /// of a condition that `call` is a link of end the predicate: the condition
    /// is all the expression is.
    fn arguments(&mut self, call: &Call, values_end: bool) -> Vec<(String, Operand)> {
        let entry = call.entry().id().as_str();
        let mut found = Vec::new();
        for given in call.arguments() {
            self.path
                .push(Segment::Parameter(given.parameter().to_owned()));
            let slot = Slot::of_parameter(entry, given.parameter());
            self.slots.extend(slot);
            // What an expression held here ends: a filter's predicate does, and
            // a value of a condition does when the condition does.
            let ends = match slot {
                Some(Slot::Outer(Outer::Filter)) => true,
                Some(Slot::Nested(Nested::ConditionValue)) => values_end,
                _ => false,
            };
            self.value(
                given.value(),
                given.parameter(),
                slot.is_some().then_some(ends),
                &mut found,
            );
            // `on` is the keys of both tables: Polars evaluates it against the other
            // one as well, so what the tables say of it there is asked too. Its
            // expressions are the left's first, at the same places.
            if slot == Some(Slot::Outer(Outer::JoinLeft)) && given.parameter() == "on" {
                self.slots.pop();
                self.slots.push(Slot::Outer(Outer::JoinRight));
                self.value(
                    given.value(),
                    given.parameter(),
                    Some(false),
                    &mut Vec::new(),
                );
            }
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
        held: Option<bool>,
        found: &mut Vec<(String, Operand)>,
    ) {
        match value {
            Value::Expression(expression) => {
                let operand = self.expression(expression, held);
                found.push((parameter.to_owned(), operand));
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

    /// Whether a `col` names more than one column, which expands it to an
    /// expression for each, whatever the first one is.
    fn names_several(call: &Call) -> bool {
        let several = |value: Option<&Value>| match value {
            Some(Value::Columns(names)) => names.len() > 1,
            _ => false,
        };
        let more = match call.argument("more_names") {
            Some(Value::Many(items)) => !items.is_empty(),
            Some(_) => true,
            None => false,
        };
        call.entry().id().as_str() == "polars.col" && (several(call.argument("name")) || more)
    }

    /// The class of the column an argument names, in the table the expression
    /// is evaluated against, where it names one.
    fn column_of(&self, call: &Call) -> Option<Representative> {
        let Some(Value::Columns(names)) = call.argument("name") else {
            return None;
        };
        if Self::names_several(call) {
            return None;
        }
        Representative::of(self.read_against()?.get(names.first()?)?)
    }

    /// The schema of the table the expression being walked reads its columns
    /// from: the other table's for a join's `right_on`, and for `on`, which is
    /// read against it as well, and the table the step starts from otherwise.
    /// `None` where it is the other table and that is not known.
    fn read_against(&self) -> Option<&Schema> {
        if self.slots.first() == Some(&Slot::Outer(Outer::JoinRight)) {
            self.other.as_deref()
        } else {
            Some(self.source)
        }
    }

    /// Says the tables cannot tell what follows a column of a join's `on` that
    /// the table it is read against does not have, which Polars refuses as not
    /// found. `on` is read against both tables, so a column in only one of them is
    /// a mistake the walk can see. The editor picks the columns of other positions
    /// from the table they are read against, which the walk does not ask.
    fn blind_to_a_missing_column(&mut self, call: &Call) {
        let in_on = matches!(
            self.path.first(),
            Some(Segment::Parameter(parameter)) if parameter == "on"
        ) && self.step.entry().id().as_str() == "LazyFrame.join";
        if !in_on || call.entry().id().as_str() != "polars.col" || Self::names_several(call) {
            return;
        }
        let Some(Value::Columns(names)) = call.argument("name") else {
            return;
        };
        let Some(name) = names.first() else {
            return;
        };
        match self.read_against() {
            Some(schema) if schema.get(name).is_some() => {}
            Some(_) => self.blind(format!("the table has no column called {name:?}")),
            None => self.blind("the other table's columns are not known"),
        }
    }

    /// The class of what a pivot's group holds, which `element` stands for: the
    /// column the pivot takes values from, where it takes them from one.
    fn group_of(&self) -> Option<Representative> {
        if self.step.entry().id().as_str() != "LazyFrame.pivot" {
            return None;
        }
        let Some(Value::Columns(names)) = self.step.argument("values") else {
            return None;
        };
        // A group of several columns has the dtype of none of them alone.
        let [name] = names.as_slice() else {
            return None;
        };
        Representative::of(self.source.get(name)?)
    }

    /// The class a start gives, where its arguments decide it.
    fn start_class(&self, start: &Call) -> Option<Representative> {
        match start.entry().id().as_str() {
            "polars.col" => self.column_of(start),
            "polars.lit" => literal_class(start),
            "polars.element" => self.group_of(),
            _ => None,
        }
    }

    /// Walks `expression`, which a position holds when `held` says whether it ends
    /// the predicate there, and which is an argument of a call otherwise.
    fn expression(&mut self, expression: &Expression, held: Option<bool>) -> Operand {
        // The values of a condition end the predicate where the condition is
        // all the expression is: a method that goes on from it is what ends it.
        let values_end = held == Some(true)
            && matches!(expression.start(), Start::Call(start) if start.entry().applies() == Applies::Condition)
            && expression
                .links()
                .iter()
                .all(|link| link.entry().applies() == Applies::Condition);
        let place = self.here();
        let context = Context::from_positions(&self.slots);
        let unsaid = Operand {
            dtype: None,
            loose: None,
            shape: Shape::Many,
            known: false,
        };
        let mut calls: Vec<Standing> = Vec::new();
        let mut last_end: Option<Ending> = None;
        let mut cur = Cur::Dead;
        let mut start_arguments: Vec<(String, Operand)> = Vec::new();
        match expression.start() {
            Start::Call(start) => {
                self.path.push(Segment::Start);
                start_arguments = self.arguments(start, values_end);
                self.path.pop();
            }
            Start::Input(_) | Start::Hole => {}
        }
        let Some(context) = context else {
            self.blind("no named position holds the expression");
            self.walked.push(WalkedExpression {
                place,
                context: None,
                calls,
                next: None,
                ending: None,
            });
            return unsaid;
        };
        match expression.start() {
            Start::Call(start) => {
                self.path.push(Segment::Start);
                let class = self.start_class(start);
                self.blind_to_a_missing_column(start);
                let chosen = Chosen::default();
                let chosen = match class {
                    Some(dtype) => chosen.giving(dtype),
                    None => chosen,
                };
                let verdict = allowed::offer(
                    start.entry(),
                    &OfferPlace::new(context, After::Start),
                    &chosen,
                );
                match &verdict {
                    Verdict::Refused(refusal) => self.refuse(refusal, false),
                    Verdict::Allowed(allowance) => {
                        last_end = Some(allowance.can_end().clone());
                        // An expansion has a dtype for each column, so whether it
                        // may end is not known, whatever the table measured.
                        if Self::names_several(start) && last_end == Some(Ending::Allowed) {
                            last_end = Some(Ending::NotKnown(Unknown::TheStartsDtype));
                        }
                        cur = if start.entry().id().as_str() == "polars.when" {
                            Cur::Condition {
                                state: ConditionState::When,
                                values: Vec::new(),
                                single: start_arguments
                                    .iter()
                                    .all(|(_, operand)| operand.shape == Shape::One),
                                values_single: true,
                                known: start_arguments.iter().all(|(_, operand)| operand.known),
                            }
                        } else {
                            Cur::Value {
                                dtype: class,
                                shape: allowance.shape(),
                            }
                        };
                    }
                }
                calls.push(Standing {
                    follows: Some(After::Start),
                    verdict: Some(verdict),
                });
                self.path.pop();
            }
            Start::Hole => {
                // Nothing is picked, so nothing is asked of what follows.
                calls.push(Standing::not_asked());
                cur = Cur::Value {
                    dtype: None,
                    shape: Shape::Many,
                };
            }
            Start::Input(name) => {
                cur = Cur::Value {
                    dtype: None,
                    shape: Shape::Many,
                };
                let standing = self.input_start(name, context, &mut cur, &mut last_end);
                calls.push(standing);
            }
        }
        // Values that are one expression make the condition that expression, and
        // its predicates are dropped.
        let statements: Vec<&Expression> = expression
            .links()
            .iter()
            .filter_map(|link| match link.argument("statement") {
                Some(Value::Expression(statement)) => Some(statement),
                _ => None,
            })
            .collect();
        let branches_are_one =
            statements.len() > 1 && statements.windows(2).all(|pair| pair[0] == pair[1]);
        for (at, link) in expression.links().iter().enumerate() {
            self.path.push(Segment::Link(at));
            let arguments = self.arguments(link, values_end);
            // The first link follows the start, which is a number typed without a dtype when
            // it is one.
            let receiver_loose = match expression.start() {
                Start::Call(start) if at == 0 => loose_number(start),
                _ => None,
            };
            let (next, standing) = self.link(
                link,
                &arguments,
                context,
                cur,
                &mut last_end,
                Around {
                    branches_are_one,
                    receiver_loose,
                },
            );
            cur = next;
            calls.push(standing);
            self.path.pop();
        }
        // A condition that has no `otherwise` and holds a number typed without a dtype has no
        // dtype to give it: Polars cannot resolve the number, and refuses where it runs.
        if let Cur::Condition { values, .. } = &cur
            && values.iter().any(|(_, loose)| loose.is_some())
        {
            last_end = Some(Ending::NotKnown(Unknown::TheResultsDtype));
        }
        let mut ending = None;
        if held.is_some() && !matches!(cur, Cur::Dead) {
            // A value of a condition in a filter need be a Boolean only where
            // the condition ends the predicate.
            let asked = held == Some(true)
                || context != Context::inside(Outer::Filter, Nested::ConditionValue);
            if asked {
                ending = last_end;
            }
            if let Some(Ending::Refused(refusal)) = &ending {
                self.refuse(&refusal.clone(), true);
            } else if let Some(Ending::NotKnown(unknown)) = &ending {
                self.blind(format!("whether it may end is not known: {unknown}"));
            }
            // A join's key that holds an aggregate beside a column, or a column
            // compared with a null, is not elementwise, which only the whole key
            // shows.
            if is_not_elementwise(expression)
                && let Err(refusal) = allowed::rule_forbids_a_key_known_not_elementwise(context)
            {
                self.refuse(&refusal, true);
                ending = Some(Ending::Refused(refusal));
            }
        }
        let next = match &cur {
            Cur::Value {
                dtype: Some(dtype),
                shape,
            } => Some(After::Value {
                dtype: *dtype,
                shape: *shape,
            }),
            Cur::Condition { state, .. } => Some(After::Condition(*state)),
            _ => None,
        };
        self.walked.push(WalkedExpression {
            place,
            context: Some(context),
            calls,
            next,
            ending,
        });
        match cur {
            Cur::Value { dtype, shape } => Operand {
                dtype,
                loose: match expression.start() {
                    Start::Call(start) if expression.links().is_empty() => loose_number(start),
                    _ => None,
                },
                shape,
                // What was not asked has no shape that can be counted on.
                known: dtype.is_some(),
            },
            Cur::Condition { values, .. } => Operand {
                dtype: unify(&values),
                loose: None,
                shape: Shape::Many,
                known: true,
            },
            Cur::Dead => unsaid,
        }
    }

    /// Asks the tables about a chain that starts from the connected input
    /// `name`, which the run makes a literal of its dtype. `cur` and `last_end`
    /// are what the chain has afterwards, as for any other start.
    fn input_start(
        &mut self,
        name: &str,
        context: Context,
        cur: &mut Cur,
        last_end: &mut Option<Ending>,
    ) -> Standing {
        let Some(literal) = self.inputs.start_literal(name) else {
            self.blind(format!(
                "there is no connected value or list called {name:?}"
            ));
            return Standing::not_asked();
        };
        let Some(class) = literal.class else {
            self.blind(format!(
                "the dtype of the connected input {name:?} is not one the tables cover"
            ));
            return Standing::not_asked();
        };
        let verdict = allowed::offer_input(
            &OfferPlace::new(context, After::Start),
            class,
            literal.shape,
        );
        match &verdict {
            Verdict::Refused(refusal) => {
                self.refuse(refusal, false);
                *cur = Cur::Dead;
            }
            Verdict::Allowed(allowance) => {
                *last_end = Some(allowance.can_end().clone());
                *cur = Cur::Value {
                    dtype: Some(class),
                    shape: allowance.shape(),
                };
            }
        }
        Standing {
            follows: Some(After::Start),
            verdict: Some(verdict),
        }
    }

    /// Asks whether `link` may stand after what `cur` has, and gives what the
    /// chain has after it.
    fn link(
        &mut self,
        link: &Call,
        arguments: &[(String, Operand)],
        context: Context,
        cur: Cur,
        last_end: &mut Option<Ending>,
        around: Around,
    ) -> (Cur, Standing) {
        let Around {
            branches_are_one,
            receiver_loose,
        } = around;
        let entry = link.entry();
        // A number typed without a dtype that a method takes another operand beside adapts to
        // it, so what it follows is not known to the tables, which are measured with typed
        // columns.
        let cur = match cur {
            Cur::Value { shape, .. } if receiver_loose.is_some() && takes_an_operand(entry) => {
                Cur::Value { dtype: None, shape }
            }
            other => other,
        };
        let after = match &cur {
            Cur::Dead => return (Cur::Dead, Standing::not_asked()),
            Cur::Value { dtype: None, shape } => {
                self.blind(format!(
                    "the dtype before {} is not known",
                    entry.id().as_str()
                ));
                *last_end = None;
                return (
                    Cur::Value {
                        dtype: None,
                        shape: *shape,
                    },
                    Standing::not_asked(),
                );
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
        // A number typed without a dtype that a method takes as an operand adapts to what it
        // follows, so its dtype is not known to the tables, unless it counts rows.
        let given: Vec<(&str, Representative)> = arguments
            .iter()
            .filter_map(|(name, operand)| {
                let counts = matches!(operand.loose, Some(Loose::Integer(_)));
                let hidden = entry.applies() == Applies::Expression
                    && operand.loose.is_some()
                    && !(counts && is_count(entry, name));
                operand
                    .dtype
                    .filter(|_| !hidden)
                    .map(|dtype| (name.as_str(), dtype))
            })
            .collect();
        let words = option_words(link);
        let words: Vec<(&str, &str)> = words
            .iter()
            .map(|(name, word)| (name.as_str(), word.as_str()))
            .collect();
        let place = OfferPlace::new(context, after);
        // What the tables are not told is not guessed: an argument whose dtype
        // is not known leaves the call to be offered for any dtype that fits,
        // which says it stands, not that this argument fits.
        if given.len() < arguments.len() {
            self.blind(format!(
                "the dtype of an argument of {} is not known",
                entry.id().as_str()
            ));
            *last_end = None;
            // The call is still offered, for a refusal is a refusal whatever
            // the argument, but nothing is said of what it gives.
            let verdict = allowed::offer(
                entry,
                &place,
                &Chosen::default()
                    .with_arguments(&given)
                    .with_options(&words),
            );
            let standing = Standing {
                follows: Some(after),
                verdict: Some(verdict.clone()),
            };
            if let Verdict::Refused(refusal) = &verdict {
                self.refuse_blind(refusal);
                return (Cur::Dead, standing);
            }
            return (
                Cur::Value {
                    dtype: None,
                    shape: Shape::Many,
                },
                standing,
            );
        }
        let statement = arguments
            .iter()
            .find(|(name, _)| name == "statement")
            .map(|(_, operand)| *operand);
        let statement_class: Valued = (
            statement.and_then(|operand| operand.dtype),
            statement.and_then(|operand| operand.loose),
        );
        // Whether the predicates and the value this link is given give one
        // value.
        let given_known = arguments
            .iter()
            .filter(|(name, _)| matches!(name.as_str(), "predicates" | "statement"))
            .all(|(_, operand)| operand.known);
        let given_values_single = arguments
            .iter()
            .filter(|(name, _)| name == "statement")
            .all(|(_, operand)| operand.shape == Shape::One);
        let given_single = arguments
            .iter()
            .filter(|(name, _)| matches!(name.as_str(), "predicates" | "statement"))
            .all(|(_, operand)| operand.shape == Shape::One);
        // A condition that ends gives the dtype its values have, when they
        // have one.
        let gives = match (&cur, entry.id().as_str()) {
            (Cur::Condition { values, .. }, "Then.otherwise" | "ChainedThen.otherwise") => {
                let mut all = values.clone();
                all.push(statement_class);
                unify(&all)
            }
            _ => None,
        };
        let chosen = Chosen::default()
            .with_arguments(&given)
            .with_options(&words);
        let chosen = match gives {
            Some(dtype) => chosen.giving(dtype),
            None => chosen,
        };
        let verdict = allowed::offer(entry, &place, &chosen);
        let standing = Standing {
            follows: Some(after),
            verdict: Some(verdict.clone()),
        };
        let allowance = match &verdict {
            Verdict::Refused(refusal) => {
                self.refuse(refusal, false);
                return (Cur::Dead, standing);
            }
            Verdict::Allowed(allowance) => allowance,
        };
        *last_end = Some(allowance.can_end().clone());
        let next = match (entry.applies(), cur) {
            (Applies::Expression, _) => Cur::Value {
                dtype: allowance.result().and_then(allowed::ResultDtype::class),
                shape: allowance.shape(),
            },
            (
                Applies::Condition,
                Cur::Condition {
                    mut values,
                    single,
                    values_single,
                    known,
                    ..
                },
            ) => {
                let single = single && given_single;
                let values_single = values_single && given_values_single;
                let known = known && given_known;
                let state = match entry.id().as_str() {
                    "When.then" => {
                        values.push(statement_class);
                        ConditionState::Then
                    }
                    "ChainedWhen.then" => {
                        values.push(statement_class);
                        ConditionState::ChainedThen
                    }
                    "Then.when" | "ChainedThen.when" => ConditionState::ChainedWhen,
                    _ => {
                        values.push(statement_class);
                        let dtype = if known { unify(&values) } else { None };
                        // Numbers typed without a dtype that have no typed value beside them
                        // have none to give, and Polars cannot always resolve them where it
                        // runs, so whether the expression may end here is not known.
                        if dtype.is_none() && values.iter().any(|(_, loose)| loose.is_some()) {
                            *last_end = Some(Ending::NotKnown(Unknown::TheResultsDtype));
                        }
                        return (
                            Cur::Value {
                                dtype,
                                // Where the values are one expression the predicates
                                // are dropped, and the values give the shape.
                                shape: if (branches_are_one && values_single)
                                    || (!branches_are_one && single)
                                {
                                    Shape::One
                                } else {
                                    Shape::Many
                                },
                            },
                            standing,
                        );
                    }
                };
                Cur::Condition {
                    state,
                    values,
                    single,
                    values_single,
                    known,
                }
            }
            _ => Cur::Dead,
        };
        (next, standing)
    }
}

// ---------------------------------------------------------------------------
// Keys

/// The expressions a call is given.
fn operands(call: &Call) -> Vec<&Expression> {
    fn collect<'a>(value: &'a Value, found: &mut Vec<&'a Expression>) {
        match value {
            Value::Expression(expression) => found.push(expression),
            Value::Many(items) => items.iter().for_each(|item| collect(item, found)),
            Value::Pairs(pairs) => pairs.iter().for_each(|(key, entry)| {
                collect(key, found);
                collect(entry, found);
            }),
            _ => {}
        }
    }
    let mut found = Vec::new();
    for given in call.arguments() {
        collect(given.value(), &mut found);
    }
    found
}

const ONE_VALUE: [&str; 6] = [
    "Expr.sum",
    "Expr.min",
    "Expr.max",
    "Expr.len",
    "Expr.first",
    "Expr.last",
];

fn is_one_value(call: &Call) -> bool {
    ONE_VALUE.contains(&call.entry().id().as_str())
}

/// Whether the first `upto` links of `expression` give one value and nothing
/// beside them does not: an aggregate among them with only single values after
/// it; with `literals`, a start made of literals too; or a condition whose parts
/// are single.
fn wholly_single(expression: &Expression, upto: usize, literals: bool) -> bool {
    let links = &expression.links()[..upto.min(expression.links().len())];
    let single = |call: &Call| {
        operands(call)
            .into_iter()
            .all(|inside| wholly_single(inside, inside.links().len(), true))
    };
    if let Some(last) = links.iter().rposition(is_one_value) {
        return links[last + 1..].iter().all(single);
    }
    match expression.start() {
        Start::Call(start) if start.entry().id().as_str() == "polars.lit" => {
            literals && links.iter().all(single)
        }
        Start::Call(start) if start.entry().id().as_str() == "polars.when" => {
            single(start) && links.iter().all(single)
        }
        _ => false,
    }
}

/// The comparisons, which with a null literal give a null.
const COMPARISONS: [&str; 6] = [
    "Expr.eq", "Expr.ne", "Expr.gt", "Expr.ge", "Expr.lt", "Expr.le",
];

/// Whether `link` is a comparison.
fn is_a_comparison(link: &Call) -> bool {
    COMPARISONS.contains(&link.entry().id().as_str())
}

/// The calls of an expression: its start, then its links.
fn calls_of(expression: &Expression) -> impl Iterator<Item = &Call> {
    std::iter::once(expression.start())
        .filter_map(|start| match start {
            Start::Call(call) => Some(call),
            Start::Input(_) | Start::Hole => None,
        })
        .chain(expression.links())
}

/// Whether an expression holds, at any depth, a comparison with a null literal:
/// one given it, or one that follows it at the start. Polars folds it into a
/// null.
fn folds_a_null(expression: &Expression) -> bool {
    let from_a_null = starts_from_a_null(expression);
    expression.links().iter().enumerate().any(|(at, link)| {
        is_a_comparison(link)
            && ((from_a_null && at == 0) || operands(link).into_iter().any(is_a_null))
    }) || calls_of(expression).any(|call| operands(call).into_iter().any(folds_a_null))
}

/// Whether a null fold is in the first `upto` calls of an expression, or in what
/// they are given.
fn folds_a_null_before(expression: &Expression, upto: usize) -> bool {
    let from_a_null = starts_from_a_null(expression);
    let links = &expression.links()[..upto];
    links.iter().enumerate().any(|(at, link)| {
        is_a_comparison(link)
            && ((from_a_null && at == 0) || operands(link).into_iter().any(is_a_null))
    }) || calls_of(expression)
        .take(upto + 1)
        .any(|call| operands(call).into_iter().any(folds_a_null))
}

/// Whether the comparison `at` of an expression is one that a null reaches:
/// Polars folds it into a null. It is reached where it is given a null or what
/// folds into one, follows a fold, or starts from a null. What Polars folds
/// depends on dtypes, so any fold that can reach a comparison counts.
fn is_reached_by_a_null(expression: &Expression, at: usize) -> bool {
    is_a_comparison(&expression.links()[at])
        && (operands(&expression.links()[at])
            .into_iter()
            .any(|operand| is_a_null(operand) || folds_a_null(operand))
            || (starts_from_a_null(expression) && at == 0)
            || folds_a_null_before(expression, at))
}

/// Whether an expression holds an aggregate that Polars evaluates. A comparison
/// that a null reaches is a null. The first of them drops what it follows and is
/// given where that is one value or literals alone, and the comparisons after it
/// compare a null, so they drop only what they are given where it is one value or
/// literals alone, except one that is given a null, which drops what it follows
/// too where that is one value or literals alone: what comes before the last that
/// does is not evaluated. A side that is more is evaluated, and its aggregate counts.
/// Where a join's key is judged elementwise a subtraction with a null drops
/// nothing: the key `k != f.len() - null` is not elementwise.
fn holds_an_aggregate(expression: &Expression) -> bool {
    let evaluated =
        |call: &Call| is_one_value(call) || operands(call).into_iter().any(holds_an_aggregate);
    let start = match expression.start() {
        Start::Call(call) => Some(call),
        Start::Input(_) | Start::Hole => None,
    };
    let links = expression.links();
    let Some(first) = (0..links.len()).find(|at| is_reached_by_a_null(expression, *at)) else {
        return start.into_iter().chain(links).any(evaluated);
    };
    // A comparison that is given a null drops what it follows where that is one
    // value or literals alone, as the first fold does, so what a later one drops
    // is not evaluated either: the last of them that does is where the key is
    // judged from.
    let first = (first..links.len())
        .rev()
        .find(|at| {
            is_a_comparison(&links[*at])
                && operands(&links[*at]).into_iter().any(is_a_null)
                && wholly_single(expression, *at, true)
        })
        .unwrap_or(first);
    // What the first comparison follows is the start and the links before it,
    // dropped as a whole where they are one value or literals alone.
    let followed = !wholly_single(expression, first, true)
        && start.into_iter().chain(&links[..first]).any(evaluated);
    // What a comparison that a null reaches is given is dropped where it is one
    // value or literals alone, and what is not reached is evaluated.
    let after = links[first..].iter().enumerate().any(|(at, link)| {
        if is_reached_by_a_null(expression, first + at) {
            operands(link).into_iter().any(|operand| {
                !wholly_single(operand, operand.links().len(), true) && holds_an_aggregate(operand)
            })
        } else {
            evaluated(link)
        }
    });
    followed || after
}

/// Whether an expression reads a column: it starts from one, or a call in it is
/// given an expression that does.
fn reads_a_column(expression: &Expression) -> bool {
    let calls = std::iter::once(expression.start())
        .filter_map(|start| match start {
            Start::Call(call) => Some(call),
            Start::Input(_) | Start::Hole => None,
        })
        .chain(expression.links());
    matches!(
        expression.start(),
        Start::Call(start) if start.entry().id().as_str() == "polars.col"
    ) || calls
        .into_iter()
        .any(|call| operands(call).into_iter().any(reads_a_column))
}

/// Whether an expression is a null literal and no more.
fn is_a_null(expression: &Expression) -> bool {
    expression.links().is_empty() && starts_from_a_null(expression)
}

/// Whether an expression starts from a null literal.
fn starts_from_a_null(expression: &Expression) -> bool {
    matches!(
        expression.start(),
        Start::Call(start)
            if start.entry().id().as_str() == "polars.lit"
                && matches!(start.argument("value"), Some(Value::Flag(None)))
    )
}

/// Whether an expression holds, at any depth, a comparison of a null literal
/// with what reads a column and is not one value. Polars folds the comparison
/// into a null, and a key with a null where a column was is not elementwise. A
/// comparison with what an earlier one folded is left to a limit: what Polars
/// does with it depends on dtypes that the document does not decide.
fn holds_a_null_comparison(expression: &Expression) -> bool {
    let start = match expression.start() {
        Start::Call(call) => Some(call),
        Start::Input(_) | Start::Hole => None,
    };
    let mut reads = matches!(
        start,
        Some(start) if start.entry().id().as_str() == "polars.col"
    ) || start.is_some_and(|start| operands(start).into_iter().any(reads_a_column));
    for (at, link) in expression.links().iter().enumerate() {
        if COMPARISONS.contains(&link.entry().id().as_str()) {
            let given = operands(link);
            let null_given = given.iter().any(|operand| is_a_null(operand));
            let column_given = given.iter().any(|operand| {
                reads_a_column(operand) && !wholly_single(operand, operand.links().len(), true)
            });
            let column_before = reads && !wholly_single(expression, at, true);
            let null_start = at == 0 && starts_from_a_null(expression);
            if (null_given && column_before) || (null_start && column_given) {
                return true;
            }
        }
        reads |= operands(link).into_iter().any(reads_a_column);
    }
    start
        .into_iter()
        .chain(expression.links())
        .any(|call| operands(call).into_iter().any(holds_a_null_comparison))
}

/// Whether a key is not elementwise, as Polars takes it: it holds an aggregate
/// beside what is not one value, or a column compared with a null. A key that is
/// wholly one value is the whole key, which the tables measured, and Polars
/// allows it. It takes the null literal beside a column as elementwise, and an
/// aggregate as not.
fn is_not_elementwise(key: &Expression) -> bool {
    !wholly_single(key, key.links().len(), true)
        && (holds_an_aggregate(key) || holds_a_null_comparison(key))
}
