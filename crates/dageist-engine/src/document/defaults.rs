//! Which arguments are what their parameter holds when it is not given.
//!
//! Python calls a method the same whether a parameter is left out or given its
//! default, so an argument equal to the default says nothing a call without it
//! does not. Whether it is one is a rule about Polars' defaults, which the
//! engine holds: the step editor shows it.

use crate::catalog::{DefaultValue, Parameter, Role, Shape, Variadic};

use super::error::{Place, Segment};
use super::model::{Call, Expression, Start, Step, Value};

impl Call {
    /// Whether the argument given for `parameter` is what the parameter holds
    /// when it is not given. A parameter that was not given is not one.
    ///
    /// A flag, a number, text, a choice's word, a duration and a path are when
    /// they equal the default, and a flag that leaves the choice to Polars is
    /// when the default is `None`; a number is one whether it was written with
    /// a fraction or without. An expression is when the parameter counts rows,
    /// as `shift`'s `n` does, its default is a whole number and the expression
    /// is that whole number alone, as `polars.lit` makes it. A list or pairs are
    /// when they are empty where the default is the empty `*` or `**`
    /// parameter. The list a parameter that holds one value for each item of
    /// another holds is when it has one value for each of those items and every
    /// value is; a list of another length is refused by Polars and is never the
    /// default. Nothing else is: a column, a dtype and a table never are, and a
    /// hole is not given yet.
    #[must_use]
    pub fn is_default(&self, parameter: &str) -> bool {
        let Some(found) = self
            .entry()
            .parameters()
            .iter()
            .find(|candidate| candidate.name() == parameter)
        else {
            return false;
        };
        let Some(value) = self.argument(parameter) else {
            return false;
        };
        let default = found.default().value();
        match (value, found.shape()) {
            (Value::Many(items), Shape::PerItem(of)) => {
                items.len() == self.key_count(of)
                    && items.iter().all(|item| item.equals(&default, Role::Plain))
            }
            (Value::Many(items), _) => items.is_empty() && default == DefaultValue::Empty,
            (Value::Pairs(pairs), _) => pairs.is_empty() && default == DefaultValue::Empty,
            (value, _) => value.equals(&default, found.role()),
        }
    }

    /// How many items the parameter `of` holds, with the starred parameter
    /// that follows it in Python's signature, as `sort`'s `by` and `more_by`
    /// are the keys the flags for each key are given for.
    pub(super) fn key_count(&self, of: &str) -> usize {
        self.items_of(of) + self.starred_after(of).map_or(0, |next| self.items_of(next))
    }

    /// Where the items of the parameter `name` begin among the keys of `of`,
    /// which are the items of `of` and then those of the starred parameter after
    /// it: at the first for `of` itself, after the items of `of` for the
    /// starred parameter, and nowhere for any other.
    pub(super) fn key_offset(&self, of: &str, name: &str) -> Option<usize> {
        if name == of {
            Some(0)
        } else if self.starred_after(of) == Some(name) {
            Some(self.items_of(of))
        } else {
            None
        }
    }

    /// How many items the parameter `name` holds: those of a list, one for any
    /// other argument, and none when it is not given.
    pub(super) fn items_of(&self, name: &str) -> usize {
        match self.argument(name) {
            Some(Value::Many(items)) => items.len(),
            Some(_) => 1,
            None => 0,
        }
    }

    /// The name of the starred parameter that follows `of` in Python's
    /// signature, if one does.
    fn starred_after(&self, of: &str) -> Option<&'static str> {
        let parameters = self.entry().parameters();
        parameters
            .iter()
            .position(|parameter| parameter.name() == of)
            .and_then(|at| parameters.get(at + 1))
            .filter(|next| next.variadic() == Some(Variadic::Args))
            .map(Parameter::name)
    }
}

impl Value {
    /// Whether one value is the default `default` of a parameter that plays
    /// `role`.
    fn equals(&self, default: &DefaultValue, role: Role) -> bool {
        match (self, default) {
            (Self::Flag(Some(value)), DefaultValue::Flag(default)) => value == default,
            (Self::Flag(None), DefaultValue::Unset) => true,
            (Self::Integer(value), DefaultValue::Integer(default)) => value == default,
            (Self::Integer(value), DefaultValue::Float(default)) => {
                i32::try_from(*value).is_ok_and(|whole| f64::from(whole) == *default)
            }
            (Self::Float(value), DefaultValue::Float(default)) => value == default,
            (Self::Float(value), DefaultValue::Integer(default)) => {
                i32::try_from(*default).is_ok_and(|whole| f64::from(whole) == *value)
            }
            (
                Self::String(value)
                | Self::Choice(value)
                | Self::Duration(value)
                | Self::Pattern(value)
                | Self::Path(value),
                DefaultValue::Text(default),
            ) => value == default,
            (Self::Expression(expression), DefaultValue::Integer(default)) => {
                role.is_a_count()
                    && matches!(alone_in(expression), Some(Self::Integer(value)) if value == default)
            }
            _ => false,
        }
    }
}

/// The number or text an expression is made of when it is nothing else: a
/// `polars.lit` with no links and no other argument.
fn alone_in(expression: &Expression) -> Option<&Value> {
    if !expression.links().is_empty() {
        return None;
    }
    let Start::Call(call) = expression.start() else {
        return None;
    };
    if call.entry().id().as_str() != "polars.lit" || call.arguments().len() != 1 {
        return None;
    }
    call.argument("value")
}

impl Step {
    /// The places of the arguments in the step that are what their parameter
    /// holds when it is not given: see [`Call::is_default`]. They are the
    /// step's own arguments, and those of the calls inside the expressions it
    /// holds, in the order the step is read. `index` is where the step is in
    /// its document.
    #[must_use]
    pub fn at_default(&self, index: usize) -> Vec<Place> {
        let mut found = Vec::new();
        let base = Place::of_step(index, Some(self.id().clone()));
        in_call(self.call(), &base, &mut found);
        found
    }
}

fn in_call(call: &Call, at: &Place, found: &mut Vec<Place>) {
    for given in call.arguments() {
        let place = at
            .clone()
            .then(Segment::Parameter(given.parameter().to_owned()));
        if call.is_default(given.parameter()) {
            found.push(place.clone());
        }
        in_value(given.value(), &place, found);
    }
}

fn in_value(value: &Value, at: &Place, found: &mut Vec<Place>) {
    match value {
        Value::Expression(expression) => {
            if let Start::Call(start) = expression.start() {
                in_call(start, &at.clone().then(Segment::Start), found);
            }
            for (index, link) in expression.links().iter().enumerate() {
                in_call(link, &at.clone().then(Segment::Link(index)), found);
            }
        }
        Value::Many(items) => {
            for (index, item) in items.iter().enumerate() {
                in_value(item, &at.clone().then(Segment::Item(index)), found);
            }
        }
        Value::Pairs(pairs) => {
            for (index, (key, entry)) in pairs.iter().enumerate() {
                in_value(key, &at.clone().then(Segment::Key(index)), found);
                in_value(entry, &at.clone().then(Segment::Value(index)), found);
            }
        }
        _ => {}
    }
}
