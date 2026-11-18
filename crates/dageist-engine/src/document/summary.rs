//! A step said in a few words: what it works on, without the pieces under it.
//! A list of steps shows this where there is no room for the steps
//! themselves.
//!
//! The words come from a language file, as [`SummaryWord`] lists them; what
//! decides which are used and what they are filled with is here. A summary
//! names fields, tables and values as the document holds them and says `…`
//! for anything that is worked out, so it never takes the place of the step.

use crate::catalog::{SummaryWord, Words};

use super::model::{Expression, Start, Step, Value};
use super::source::{field_alone, scalar};

/// How many names a list names before it says how many there are instead.
const NAMED: usize = 3;

/// How many renames are named before the names renamed are listed.
const RENAMES_NAMED: usize = 2;

impl Step {
    /// What the step works on, in the words of `words`, such as `by dcn,
    /// timestamp` for a sort or `Complete` for a column that is added: the
    /// words that follow the step's head. A step the catalogue gives no
    /// summary for is the empty text.
    #[must_use]
    pub fn summary(&self, words: &Words) -> String {
        let say = Say { words };
        let call = self.call();
        let keys = |names: &[&str]| -> Vec<&Value> {
            names
                .iter()
                .filter_map(|name| call.argument(name))
                .flat_map(items)
                .collect()
        };
        match call.entry().id().as_str() {
            "LazyFrame.with_columns" | "LazyFrame.select" => {
                let named =
                    call.argument("named_exprs")
                        .map_or_else(Vec::new, |pairs| match pairs {
                            Value::Pairs(pairs) => {
                                pairs.iter().map(|(name, _)| say.key(name)).collect()
                            }
                            _ => Vec::new(),
                        });
                let listed = keys(&["exprs"]).into_iter().map(|key| say.key(key));
                say.fields(&listed.chain(named).collect::<Vec<_>>())
            }
            "LazyFrame.filter" => {
                say.conditions(call.argument("predicates"), call.argument("constraints"))
            }
            "LazyFrame.sort" => say.words.summary(
                SummaryWord::SortedBy,
                &[(
                    "fields",
                    &say.fields(
                        &keys(&["by", "more_by"])
                            .into_iter()
                            .map(|key| say.key(key))
                            .collect::<Vec<_>>(),
                    ),
                )],
            ),
            "LazyFrame.rename" => say.renames(call.argument("mapping")),
            "LazyFrame.join" => {
                let table = match call.argument("other") {
                    Some(Value::Table(name)) => name.clone(),
                    _ => String::new(),
                };
                let matched = if call.argument("left_on").is_some() {
                    keys(&["left_on"])
                } else {
                    keys(&["on"])
                };
                if matched.is_empty() {
                    return table;
                }
                let fields: Vec<String> = matched.into_iter().map(|key| say.key(key)).collect();
                say.words.summary(
                    SummaryWord::JoinedOn,
                    &[("table", &table), ("fields", &fields.join(", "))],
                )
            }
            "LazyFrame.pivot" => {
                let field = match call.argument("on") {
                    Some(Value::Columns(names)) => names.join(", "),
                    _ => String::new(),
                };
                say.words
                    .summary(SummaryWord::PivotedOn, &[("field", &field)])
            }
            _ => String::new(),
        }
    }
}

/// The items of a list argument, or the argument itself when it is not one.
fn items(argument: &Value) -> Vec<&Value> {
    match argument {
        Value::Many(items) => items.iter().collect(),
        other => vec![other],
    }
}

struct Say<'a> {
    words: &'a Words,
}

impl Say<'_> {
    fn word(&self, word: SummaryWord) -> String {
        self.words.summary(word, &[])
    }

    /// `names` as a short list, or how many there are when they are too many
    /// to list.
    fn fields(&self, names: &[String]) -> String {
        match names.len() {
            0 => self.word(SummaryWord::NoFields),
            count if count <= NAMED => names.join(", "),
            count => self
                .words
                .summary(SummaryWord::Fields, &[("count", &count.to_string())]),
        }
    }

    /// A value that is a key or an argument: a field by its name, a typed
    /// value as Python writes it, an input by its name, anything worked out
    /// as `…`, and what is still to be picked as `value`.
    fn key(&self, value: &Value) -> String {
        match value {
            Value::Expression(expression) => self.expression(expression),
            // A column's name, or text that stands for one.
            Value::Columns(names) => names.join(", "),
            other => scalar(other).unwrap_or_else(|| self.word(SummaryWord::Worked)),
        }
    }

    fn expression(&self, expression: &Expression) -> String {
        if !expression.links().is_empty() {
            return self.word(SummaryWord::Worked);
        }
        match expression.start() {
            Start::Hole => self.word(SummaryWord::Value),
            Start::Input(name) => name.clone(),
            Start::Call(start) => match start.entry().id().as_str() {
                "polars.col" => field_alone(expression)
                    .map_or_else(|| self.word(SummaryWord::Worked), str::to_owned),
                "polars.lit" => start
                    .argument("value")
                    .and_then(scalar)
                    .unwrap_or_else(|| self.word(SummaryWord::Worked)),
                _ => self.word(SummaryWord::Worked),
            },
        }
    }

    /// The conditions a filter keeps rows by: the one condition, or how many
    /// must all hold.
    fn conditions(&self, predicates: Option<&Value>, constraints: Option<&Value>) -> String {
        let predicates: Vec<&Value> = predicates.map_or_else(Vec::new, items);
        let constraints: &[(Value, Value)] = match constraints {
            Some(Value::Pairs(pairs)) => pairs,
            _ => &[],
        };
        match (predicates.as_slice(), constraints) {
            ([], []) => self.word(SummaryWord::Condition),
            ([only], []) => self.condition(only),
            ([], [(name, value)]) => self.compared(&self.key(name), SummaryWord::Equal, value),
            _ => self.words.summary(
                SummaryWord::AllOf,
                &[("count", &(predicates.len() + constraints.len()).to_string())],
            ),
        }
    }

    /// A condition: a field compared with a value, or `…` for anything else.
    fn condition(&self, condition: &Value) -> String {
        let Value::Expression(expression) = condition else {
            return self.word(SummaryWord::Worked);
        };
        let (Some(link), 1) = (expression.links().first(), expression.links().len()) else {
            return self.word(SummaryWord::Worked);
        };
        let sign = match link.entry().id().as_str() {
            "Expr.eq" => SummaryWord::Equal,
            "Expr.ne" => SummaryWord::NotEqual,
            "Expr.gt" => SummaryWord::Greater,
            "Expr.ge" => SummaryWord::AtLeast,
            "Expr.lt" => SummaryWord::Less,
            "Expr.le" => SummaryWord::AtMost,
            _ => return self.word(SummaryWord::Worked),
        };
        let field = match expression.start() {
            Start::Hole => self.word(SummaryWord::Field),
            Start::Call(start) if start.entry().id().as_str() == "polars.col" => {
                match start.argument("name") {
                    Some(Value::Columns(names)) if names.len() == 1 => names.join(", "),
                    _ => return self.word(SummaryWord::Worked),
                }
            }
            _ => return self.word(SummaryWord::Worked),
        };
        match link.argument("other") {
            Some(other) => self.compared(&field, sign, other),
            None => self.word(SummaryWord::Worked),
        }
    }

    fn compared(&self, field: &str, sign: SummaryWord, value: &Value) -> String {
        self.words.summary(
            SummaryWord::Compared,
            &[
                ("field", field),
                ("sign", &self.word(sign)),
                ("value", &self.key(value)),
            ],
        )
    }

    /// The renames of a mapping: each as `from → to` when there are few, and
    /// otherwise the names that are renamed.
    fn renames(&self, mapping: Option<&Value>) -> String {
        let pairs: &[(Value, Value)] = match mapping {
            Some(Value::Pairs(pairs)) => pairs,
            _ => &[],
        };
        let text = |value: &Value, missing: SummaryWord| match value {
            Value::Columns(names) if names.len() == 1 => names.join(", "),
            Value::String(text) => text.clone(),
            _ => self.word(missing),
        };
        let from: Vec<String> = pairs
            .iter()
            .map(|(name, _)| text(name, SummaryWord::Field))
            .collect();
        if pairs.is_empty() || pairs.len() > RENAMES_NAMED {
            return self.fields(&from);
        }
        pairs
            .iter()
            .zip(&from)
            .map(|((_, to), from)| {
                self.words.summary(
                    SummaryWord::Renamed,
                    &[("from", from), ("to", &text(to, SummaryWord::Name))],
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}
