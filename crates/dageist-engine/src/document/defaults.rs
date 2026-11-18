//! Which arguments are what their parameter holds when it is not given.
//!
//! Python calls a method the same whether a parameter is left out or given its
//! default, so an argument equal to the default says nothing a call without it
//! does not. Whether it is one is a rule about Polars' defaults, which the
//! engine holds: the step editor shows it, and the source does not write it.

use crate::catalog::{DefaultValue, Parameter, Shape};

use super::error::{Place, Segment};
use super::model::{Call, Expression, Start, Step, Value};

impl Value {
    /// Whether the argument is what `parameter` holds when it is not given.
    ///
    /// A flag, a number, text, a choice's word, a duration and a path are
    /// when they equal the default, and a flag that leaves the choice to Polars
    /// is when the default is `None`; a number is one whether it was written
    /// with a fraction or without. An expression is when it is the number or
    /// text alone, as `polars.lit` makes it. A list or pairs are when they are
    /// empty where the default is the empty `*` or `**` parameter. The list a
    /// parameter that holds one value for each item of another holds is when
    /// every value is. Nothing else is: a column, a dtype and a table never
    /// are, and a hole is not given yet.
    #[must_use]
    pub fn is_default_of(&self, parameter: &Parameter) -> bool {
        let default = parameter.default().value();
        match (self, parameter.shape()) {
            (Self::Many(items), Shape::PerItem(_)) => {
                items.iter().all(|item| item.equals(&default))
            }
            (Self::Many(items), _) => items.is_empty() && default == DefaultValue::Empty,
            (Self::Pairs(pairs), _) => pairs.is_empty() && default == DefaultValue::Empty,
            (value, _) => value.equals(&default),
        }
    }

    /// Whether one value is the default `default`.
    fn equals(&self, default: &DefaultValue) -> bool {
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
                | Self::Path(value),
                DefaultValue::Text(default),
            ) => value == default,
            (Self::Expression(expression), default) => {
                alone_in(expression).is_some_and(|literal| literal.equals(default))
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
    call.argument("value").filter(|value| {
        matches!(
            value,
            Value::Integer(_) | Value::Float(_) | Value::String(_)
        )
    })
}

impl Step {
    /// The places of the arguments in the step that are what their parameter
    /// holds when it is not given: see [`Value::is_default_of`]. They are the
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
        let Some(parameter) = call
            .entry()
            .parameters()
            .iter()
            .find(|parameter| parameter.name() == given.parameter())
        else {
            continue;
        };
        let place = at
            .clone()
            .then(Segment::Parameter(given.parameter().to_owned()));
        if given.value().is_default_of(parameter) {
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
