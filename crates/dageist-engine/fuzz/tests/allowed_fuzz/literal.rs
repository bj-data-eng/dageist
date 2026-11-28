//! The literals of a case, which the tables measured as columns.
//!
//! A literal is one value, which Polars casts and broadcasts as it does not a
//! column, and the tables measured an argument, or a start, as a column of the
//! dtype. Where the tables refuse a part that holds a literal and Polars allows the
//! step, the literal may be why, or it may be a part Polars skips, or a value of
//! another dtype than the tables have. What tells them apart is the part with each
//! literal a column of its dtype, which this module makes: the case run again
//! ([`super::limits`]), or the refused call alone ([`super::probe`]).

use dageist_engine::allowed::Representative;
use dageist_engine::catalog;
use dageist_engine::document::{Call, Expression, Place, Segment, Start, Value};

use super::frames::stand_in_column;
use super::observe::{Observed, Seen};

fn is(call: &Call, id: &str) -> bool {
    call.entry().id().as_str() == id
}

/// Whether `expression` starts from a literal, or from a condition whose
/// values are literals, which the tables did not measure as a start.
pub fn starts_from_a_literal(expression: &Expression) -> bool {
    match expression.start() {
        Start::Call(start) if is(start, "polars.lit") => true,
        Start::Call(start) if is(start, "polars.when") => {
            let values: Vec<&Expression> = expression
                .links()
                .iter()
                .filter(|link| is_a_value_link(link))
                .filter_map(|link| match link.argument("statement") {
                    Some(Value::Expression(value)) => Some(value),
                    _ => None,
                })
                .collect();
            !values.is_empty() && values.iter().all(|value| starts_from_a_literal(value))
        }
        _ => false,
    }
}

fn is_a_value_link(call: &Call) -> bool {
    matches!(
        call.entry().id().as_str(),
        "When.then" | "Then.otherwise" | "ChainedWhen.then" | "ChainedThen.otherwise"
    )
}

/// The literals the tables measured are the values `shift` takes, and, in a
/// pivot's aggregate, every argument.
pub fn measured_literal(owner: &Call, parameter: &str, pivot: bool) -> bool {
    pivot || (is(owner, "Expr.shift") && matches!(parameter, "n" | "fill_value"))
}

/// The expressions a call is given, by parameter, and the place of each: the
/// call is at `call`.
pub fn placed_operands<'a>(call: &'a Call, at: &Place) -> Vec<(&'a str, Place, &'a Expression)> {
    fn collect<'a>(
        parameter: &'a str,
        value: &'a Value,
        place: Place,
        found: &mut Vec<(&'a str, Place, &'a Expression)>,
    ) {
        match value {
            Value::Expression(expression) => found.push((parameter, place, expression)),
            Value::Many(items) => items.iter().enumerate().for_each(|(item, inside)| {
                collect(
                    parameter,
                    inside,
                    place.clone().then(Segment::Item(item)),
                    found,
                );
            }),
            Value::Pairs(pairs) => pairs.iter().enumerate().for_each(|(at, (key, entry))| {
                collect(parameter, key, place.clone().then(Segment::Key(at)), found);
                collect(
                    parameter,
                    entry,
                    place.clone().then(Segment::Value(at)),
                    found,
                );
            }),
            _ => {}
        }
    }
    let mut found = Vec::new();
    for given in call.arguments() {
        let place = at
            .clone()
            .then(Segment::Parameter(given.parameter().to_owned()));
        collect(given.parameter(), given.value(), place, &mut found);
    }
    found
}

/// An expression that starts from a literal the tables did not measure, and where
/// it is.
pub struct LiteralCause<'a> {
    pub place: Place,
    pub expression: &'a Expression,
}

/// The expressions that `call`, at `at`, is given and that start from a literal, except the
/// literals the tables measured, and those inside what it is given, at any depth: Polars folds
/// `b / null` into a null as it does a null that is given to a comparison, and a literal that
/// the call it is given to is an operand of is as much a value of one as one that starts the
/// operand.
pub fn operand_causes<'a>(call: &'a Call, at: &Place, pivot: bool) -> Vec<LiteralCause<'a>> {
    let mut found = Vec::new();
    for (parameter, place, operand) in placed_operands(call, at) {
        if !measured_literal(call, parameter, pivot) {
            causes_inside(operand, place, pivot, &mut found);
        }
    }
    found
}

/// `expression`, at `place`, if it starts from a literal, and the expressions inside it that do.
/// What stands for a literal is replaced whole, with the links that belong to it, so only the links
/// after those are looked into.
fn causes_inside<'a>(
    expression: &'a Expression,
    place: Place,
    pivot: bool,
    found: &mut Vec<LiteralCause<'a>>,
) {
    let belonging = if starts_from_a_literal(expression) {
        found.push(LiteralCause {
            place: place.clone(),
            expression,
        });
        links_of_the_literal(expression)
    } else {
        if let Start::Call(start) = expression.start() {
            found.extend(operand_causes(
                start,
                &place.clone().then(Segment::Start),
                pivot,
            ));
        }
        0
    };
    for (at, link) in expression.links().iter().enumerate().skip(belonging) {
        found.extend(operand_causes(
            link,
            &place.clone().then(Segment::Link(at)),
            pivot,
        ));
    }
}

/// How many links of a chain that starts from a literal belong to what stands for
/// the literal: none for a literal, and for a condition its values and the
/// predicates between them.
pub fn links_of_the_literal(expression: &Expression) -> usize {
    match expression.start() {
        Start::Call(start) if is(start, "polars.when") => expression
            .links()
            .iter()
            .take_while(|link| {
                is_a_value_link(link) || is(link, "Then.when") || is(link, "ChainedThen.when")
            })
            .count(),
        _ => 0,
    }
}

/// The class of the dtype that Polars gave the literal, or the condition of
/// literals, which is what a column stands in for it with.
pub fn class_of(cause: &LiteralCause<'_>, observed: &Observed) -> Option<Representative> {
    let seen = observed
        .expressions
        .iter()
        .find(|seen| seen.place == cause.place)?;
    let found = match links_of_the_literal(cause.expression).checked_sub(1) {
        None => &seen.start,
        Some(last) => seen.links.get(last)?,
    };
    match found {
        Seen::Dtype(_, _, Some(class)) => Some(*class),
        _ => None,
    }
}

/// `call` with each expression of `standing` a column that stands in for it, which
/// is [`stand_in_column`] of its place in `standing`, counted from `first`. An
/// expression is found by being the one the case holds, so `call` must be a part of
/// the case.
pub fn call_with_columns(call: &Call, standing: &[&Expression], first: usize) -> Option<Call> {
    let given = call
        .arguments()
        .iter()
        .map(|given| {
            Some((
                given.parameter(),
                value_with_columns(given.value(), standing, first)?,
            ))
        })
        .collect::<Option<Vec<_>>>()?;
    Call::new(catalog::entry(call.entry().id().as_str())?, given).ok()
}

fn value_with_columns(value: &Value, standing: &[&Expression], first: usize) -> Option<Value> {
    Some(match value {
        Value::Expression(expression) => {
            Value::Expression(expression_with_columns(expression, standing, first)?)
        }
        Value::Many(items) => Value::Many(
            items
                .iter()
                .map(|item| value_with_columns(item, standing, first))
                .collect::<Option<Vec<_>>>()?,
        ),
        Value::Pairs(pairs) => Value::Pairs(
            pairs
                .iter()
                .map(|(key, entry)| {
                    Some((
                        value_with_columns(key, standing, first)?,
                        value_with_columns(entry, standing, first)?,
                    ))
                })
                .collect::<Option<Vec<_>>>()?,
        ),
        other => other.clone(),
    })
}

/// `expression` with each expression of `standing` a column that stands in for it.
pub fn expression_with_columns(
    expression: &Expression,
    standing: &[&Expression],
    first: usize,
) -> Option<Expression> {
    let replaced = standing
        .iter()
        .position(|stands| std::ptr::eq(*stands, expression));
    let (start, links) = match replaced {
        Some(at) => {
            let column = Call::new(
                catalog::entry("polars.col")?,
                vec![("name", Value::Columns(vec![stand_in_column(first + at)]))],
            )
            .ok()?;
            (
                Start::Call(column),
                &expression.links()[links_of_the_literal(expression)..],
            )
        }
        None => (
            match expression.start() {
                Start::Call(call) => Start::Call(call_with_columns(call, standing, first)?),
                other => other.clone(),
            },
            expression.links(),
        ),
    };
    let links = links
        .iter()
        .map(|link| call_with_columns(link, standing, first))
        .collect::<Option<Vec<_>>>()?;
    Expression::new(start, links).ok()
}

#[cfg(test)]
mod tests {
    use dageist_engine::document::Place;

    use super::*;

    fn call(id: &str, given: Vec<(&str, Value)>) -> Call {
        Call::new(catalog::entry(id).expect("an entry"), given).expect("a call")
    }

    fn chain(start: Call, links: Vec<Call>) -> Expression {
        Expression::new(Start::Call(start), links).expect("a chain")
    }

    fn column(name: &str) -> Call {
        call(
            "polars.col",
            vec![("name", Value::Columns(vec![name.to_owned()]))],
        )
    }

    fn null() -> Call {
        call("polars.lit", vec![("value", Value::Flag(None))])
    }

    fn given(id: &str, operand: Expression) -> Call {
        call(id, vec![("other", Value::Expression(operand))])
    }

    #[test]
    fn a_condition_of_literals_stands_for_a_column_whatever_the_number_of_its_branches() {
        // `when(f).then(null).when(f).then(null).when(f).then(null)`: the links of the condition,
        // chained ones included, all go, and the column has none.
        let predicates =
            |name: &str| Value::Many(vec![Value::Expression(chain(column(name), Vec::new()))]);
        let then = || {
            call(
                "When.then",
                vec![("statement", Value::Expression(chain(null(), Vec::new())))],
            )
        };
        let chained_then = || {
            call(
                "ChainedWhen.then",
                vec![("statement", Value::Expression(chain(null(), Vec::new())))],
            )
        };
        let condition = chain(
            call("polars.when", vec![("predicates", predicates("f"))]),
            vec![
                then(),
                call("Then.when", vec![("predicates", predicates("f"))]),
                chained_then(),
                call("ChainedThen.when", vec![("predicates", predicates("f"))]),
                chained_then(),
            ],
        );
        assert!(starts_from_a_literal(&condition));
        assert_eq!(links_of_the_literal(&condition), 5);
        let replaced = expression_with_columns(&condition, &[&condition], 0)
            .expect("the condition stands for a column");
        assert!(replaced.links().is_empty());
    }

    #[test]
    fn the_literals_inside_an_operand_are_causes_at_any_depth_and_the_measured_ones_are_not() {
        // `a <= b / null`: the null is given to a call of the operand, not the start of it.
        let operand = chain(
            column("b"),
            vec![given("Expr.truediv", chain(null(), Vec::new()))],
        );
        let refused = given("Expr.le", operand);
        let at = Place::default();
        let causes = operand_causes(&refused, &at, false);
        assert_eq!(causes.len(), 1);
        assert!(starts_from_a_literal(causes[0].expression));
        // In a pivot's aggregate every argument was measured.
        assert!(operand_causes(&refused, &at, true).is_empty());
        // A fill value of a shift was measured as a literal, and what it holds goes with it.
        let shifted = call(
            "Expr.shift",
            vec![(
                "fill_value",
                Value::Expression(chain(
                    column("b"),
                    vec![given("Expr.add", chain(null(), Vec::new()))],
                )),
            )],
        );
        assert!(operand_causes(&shifted, &at, false).is_empty());
        // No literal, no cause.
        let plain = given(
            "Expr.le",
            chain(
                column("b"),
                vec![given("Expr.add", chain(column("a"), Vec::new()))],
            ),
        );
        assert!(operand_causes(&plain, &at, false).is_empty());
    }
}
