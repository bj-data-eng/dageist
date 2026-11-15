//! `when`, `then` and `otherwise` built through the catalogue's links, against
//! Polars' documented `when-then-otherwise` semantics (the examples of
//! `polars.when`): the value of the first `then` whose `when` holds, the
//! `otherwise` value where none does, null where there is no `otherwise`, and
//! the output name of the first `then`.

use polars::prelude::{DataFrame, DataType, Expr, IntoLazy, LazyFrame, col, df, lit};

use crate::catalog::{Applies, Argument, Build, BuildError, Conditional, Family, entry};

use super::build::{columns, expression_arg, integers, many, names, pairs, run, schema, text};

/// The documented example table: `foo` is 1, 3, 4 and `bar` is 3, 4, 0.
fn foo_bar() -> LazyFrame {
    df!("foo" => [1_i64, 3, 4], "bar" => [3_i64, 4, 0])
        .expect("a table")
        .lazy()
}

fn arg(expr: Expr) -> Argument {
    expression_arg(expr)
}

fn statement(value: Expr) -> Vec<(&'static str, Argument)> {
    vec![("statement", arg(value))]
}

fn conditions(predicates: Vec<Expr>) -> Vec<(&'static str, Argument)> {
    vec![(
        "predicates",
        many(predicates.into_iter().map(arg).collect()),
    )]
}

/// Starts a condition with entry `polars.when`.
fn start(given: Vec<(&str, Argument)>) -> Result<Conditional, BuildError> {
    let entry = entry("polars.when").expect("an entry");
    let arguments = entry.arguments(given)?;
    match entry.build() {
        Build::Condition(build) => build(&arguments),
        other => panic!("polars.when is {other:?}, not the start of a condition"),
    }
}

/// Continues `condition` with the link `id`.
fn link(
    id: &str,
    condition: Conditional,
    given: Vec<(&str, Argument)>,
) -> Result<Conditional, BuildError> {
    let entry = entry(id).expect("an entry");
    let arguments = entry.arguments(given)?;
    match entry.build() {
        Build::ConditionStep(build) => build(condition, &arguments),
        other => panic!("{id} is {other:?}, not a link that continues a condition"),
    }
}

/// Ends `condition` with the link `id`.
fn finish(
    id: &str,
    condition: Conditional,
    given: Vec<(&str, Argument)>,
) -> Result<Expr, BuildError> {
    let entry = entry(id).expect("an entry");
    let arguments = entry.arguments(given)?;
    match entry.build() {
        Build::ConditionEnd(build) => build(condition, &arguments),
        other => panic!("{id} is {other:?}, not a link that ends a condition"),
    }
}

/// `when(foo > 2).then(1).otherwise(1 + bar)`, as the documentation writes it.
fn documented_otherwise() -> Expr {
    let when = start(conditions(vec![col("foo").gt(lit(2))])).unwrap();
    let then = link("When.then", when, statement(lit(1))).unwrap();
    finish("Then.otherwise", then, statement(lit(1) + col("bar"))).unwrap()
}

/// `when(foo > 2).then(1).when(bar > 2).then(4)`: a chain with no `otherwise`.
fn documented_chain() -> Conditional {
    let when = start(conditions(vec![col("foo").gt(lit(2))])).unwrap();
    let then = link("When.then", when, statement(lit(1))).unwrap();
    let chained = then_when(then, vec![col("bar").gt(lit(2))]);
    then_chained("ChainedWhen.then", chained, statement(lit(4)))
}

fn then_when(after: Conditional, predicates: Vec<Expr>) -> Conditional {
    link("Then.when", after, conditions(predicates)).unwrap()
}

fn then_chained(id: &str, after: Conditional, given: Vec<(&str, Argument)>) -> Conditional {
    link(id, after, given).unwrap()
}

/// The rows of `expr` on `frame`, as Int64: a literal such as `1` is a loose whole number
/// that Polars gives the narrowest dtype it fits, as the documentation's `i32` shows.
fn select(frame: LazyFrame, expr: Expr) -> DataFrame {
    run(frame.select([expr.cast(DataType::Int64).alias("out")]))
}

#[test]
fn the_links_are_conditions_of_the_condition_family() {
    let links = [
        "polars.when",
        "When.then",
        "Then.when",
        "Then.otherwise",
        "ChainedWhen.then",
        "ChainedThen.when",
        "ChainedThen.otherwise",
    ];
    for id in links {
        let each = entry(id).expect("an entry");
        assert_eq!(each.applies(), Applies::Condition, "{id}");
        assert_eq!(each.family(), Family::Condition, "{id}");
    }
}

#[test]
fn otherwise_gives_the_value_where_no_condition_holds() {
    // The documentation: foo = [1, 3, 4], bar = [3, 4, 0] gives 1 + bar = 4 where foo is
    // not above 2, and 1 where it is, as the Int64 of the column in the sum.
    let expr = documented_otherwise();
    assert_eq!(dtype_of_frame(&foo_bar(), &expr), DataType::Int64);
    assert_eq!(
        integers(&select(foo_bar(), expr), "out"),
        [Some(4), Some(1), Some(1)]
    );
}

fn dtype_of_frame(frame: &LazyFrame, expr: &Expr) -> DataType {
    let schema = schema(frame.clone().select([expr.clone().alias("out")]));
    schema.get("out").expect("one column, out").clone()
}

#[test]
fn the_first_condition_that_holds_picks_its_value() {
    // The documentation: the results are folded left to right, picking the `then` value from
    // the first `when` that is true, even where a later one is true too. For foo = [1, 3, 4]
    // every row holds `foo > 0`, so listing it first gives 1 for all, and listing `foo > 2`
    // first gives 2 where that holds (3 and 4) and 1 for the row that only holds `foo > 0`.
    let pick = |first: (i64, i64), second: (i64, i64)| {
        let when = start(conditions(vec![col("foo").gt(lit(first.0))])).unwrap();
        let then = link("When.then", when, statement(lit(first.1))).unwrap();
        let chained = then_when(then, vec![col("foo").gt(lit(second.0))]);
        let both = then_chained("ChainedWhen.then", chained, statement(lit(second.1)));
        let expr = finish("ChainedThen.otherwise", both, statement(lit(3_i64))).unwrap();
        integers(&select(foo_bar(), expr), "out")
    };
    assert_eq!(pick((0, 1), (2, 2)), [Some(1), Some(1), Some(1)]);
    assert_eq!(pick((2, 2), (0, 1)), [Some(1), Some(2), Some(2)]);
}

#[test]
fn a_chain_ends_in_otherwise_with_the_documented_values() {
    // `when(foo > 2).then(1).when(bar > 2).then(4).otherwise(-1)`: [4, 1, 1].
    let expr = finish(
        "ChainedThen.otherwise",
        documented_chain(),
        statement(lit(-1)),
    )
    .unwrap();
    assert_eq!(
        integers(&select(foo_bar(), expr.clone()), "out"),
        [Some(4), Some(1), Some(1)]
    );
    // Where neither holds, the otherwise value is picked: foo = 1, bar = 0.
    let table = df!("foo" => [1_i64], "bar" => [0_i64]).unwrap().lazy();
    assert_eq!(integers(&select(table, expr), "out"), [Some(-1)]);
}

#[test]
fn a_then_without_otherwise_is_an_expression_that_is_null_where_no_condition_holds() {
    // The documentation: the `otherwise` is optional and defaults to `otherwise(None)`.
    let when = start(conditions(vec![col("foo").gt(lit(2))])).unwrap();
    let only = link("When.then", when, statement(col("foo"))).unwrap();
    let expr = only.into_expr().unwrap();
    assert_eq!(dtype_of_frame(&foo_bar(), &expr), DataType::Int64);
    assert_eq!(
        integers(&select(foo_bar(), expr), "out"),
        [None, Some(3), Some(4)]
    );

    // Likewise a chain: foo = [1, 3, 4] with 10 above 3, bar = [3, 4, 0] with 20 above 3.
    let when = start(conditions(vec![col("foo").gt(lit(3))])).unwrap();
    let first = link("When.then", when, statement(lit(10_i64))).unwrap();
    let chained = then_when(first, vec![col("bar").gt(lit(3))]);
    let both = then_chained("ChainedWhen.then", chained, statement(lit(20_i64)));
    let expr = both.into_expr().unwrap();
    assert_eq!(
        integers(&select(foo_bar(), expr), "out"),
        [None, Some(20), Some(10)]
    );
}

#[test]
fn a_when_alone_is_not_an_expression() {
    let when = start(conditions(vec![col("foo").gt(lit(2))])).unwrap();
    let error = when.clone().into_expr().unwrap_err();
    assert!(
        matches!(
            error,
            BuildError::OutOfOrder {
                expected: "then",
                found: "when"
            }
        ),
        "{error}"
    );
    let first = link("When.then", when, statement(lit(1))).unwrap();
    let chained = then_when(first, vec![col("bar").gt(lit(2))]);
    let error = chained.into_expr().unwrap_err();
    assert!(
        matches!(
            error,
            BuildError::OutOfOrder {
                expected: "then",
                found: "chained when"
            }
        ),
        "{error}"
    );
}

#[test]
fn several_predicates_and_constraints_must_all_hold() {
    // `when(foo > 1, bar=4)`: foo = [1, 3, 4] and bar = [3, 4, 0] hold together only for the
    // second row.
    let given = vec![
        ("predicates", many(vec![arg(col("foo").gt(lit(1)))])),
        (
            "constraints",
            pairs(vec![(columns(&["bar"]), arg(lit(4_i64)))]),
        ),
    ];
    let when = start(given).unwrap();
    let then = link("When.then", when, statement(lit(1_i64))).unwrap();
    let expr = finish("Then.otherwise", then, statement(lit(0_i64))).unwrap();
    assert_eq!(
        integers(&select(foo_bar(), expr), "out"),
        [Some(0), Some(1), Some(0)]
    );

    // A constraint alone is an equality on its column.
    let given = vec![(
        "constraints",
        pairs(vec![(columns(&["foo"]), arg(lit(3_i64)))]),
    )];
    let when = start(given).unwrap();
    let then = link("When.then", when, statement(lit(1_i64))).unwrap();
    let expr = finish("Then.otherwise", then, statement(lit(0_i64))).unwrap();
    assert_eq!(
        integers(&select(foo_bar(), expr), "out"),
        [Some(0), Some(1), Some(0)]
    );
}

#[test]
fn a_further_when_takes_predicates_and_constraints_too() {
    // `when(foo > 3).then(10).when(bar > 3, foo=3).then(20)`: only foo = 3, bar = 4 holds the
    // second condition, and foo = 4 holds the first.
    let when = start(conditions(vec![col("foo").gt(lit(3))])).unwrap();
    let first = link("When.then", when, statement(lit(10_i64))).unwrap();
    let given = vec![
        ("predicates", many(vec![arg(col("bar").gt(lit(3)))])),
        (
            "constraints",
            pairs(vec![(columns(&["foo"]), arg(lit(3_i64)))]),
        ),
    ];
    let chained = link("Then.when", first, given).unwrap();
    let second = link("ChainedWhen.then", chained, statement(lit(20_i64))).unwrap();
    // And a third condition after a chained then, which `ChainedThen.when` continues.
    let third = link(
        "ChainedThen.when",
        second,
        conditions(vec![col("foo").lt(lit(2))]),
    )
    .unwrap();
    let all = link("ChainedWhen.then", third, statement(lit(30_i64))).unwrap();
    assert_eq!(
        integers(&select(foo_bar(), all.into_expr().unwrap()), "out"),
        [Some(30), Some(20), Some(10)]
    );
}

#[test]
fn the_output_is_named_for_the_first_then() {
    // The documentation: the name is taken from the first `then`, not from the conditions.
    let when = start(conditions(vec![col("foo").gt(lit(2))])).unwrap();
    let first = link("When.then", when, statement(col("bar"))).unwrap();
    let expr = finish("Then.otherwise", first, statement(col("foo"))).unwrap();
    assert_eq!(names(foo_bar().select([expr])), ["bar"]);
}

#[test]
fn a_when_without_a_condition_is_refused_as_python_refuses_it() {
    let error = start(Vec::new()).unwrap_err();
    assert!(matches!(error, BuildError::Polars(_)), "{error}");
    let when = start(conditions(vec![col("foo").gt(lit(2))])).unwrap();
    let first = link("When.then", when, statement(lit(1))).unwrap();
    let error = link("Then.when", first, Vec::new()).err().unwrap();
    assert!(matches!(error, BuildError::Polars(_)), "{error}");
}

#[test]
fn a_then_without_a_value_names_the_parameter() {
    let when = start(conditions(vec![col("foo").gt(lit(2))])).unwrap();
    let error = link("When.then", when, Vec::new()).err().unwrap();
    assert!(
        matches!(
            error,
            BuildError::Missing {
                parameter: "statement"
            }
        ),
        "{error}"
    );
}

#[test]
fn a_link_after_a_state_it_cannot_follow_is_refused_with_the_state_it_needs() {
    let when = start(conditions(vec![col("foo").gt(lit(2))])).unwrap();
    let first = link("When.then", when.clone(), statement(lit(1))).unwrap();
    let chained = then_when(first.clone(), vec![col("bar").gt(lit(2))]);
    let second = then_chained("ChainedWhen.then", chained.clone(), statement(lit(4)));

    // Each link, given the other states: what it needs, and what it found.
    let needs = [
        ("When.then", "when"),
        ("Then.when", "then"),
        ("Then.otherwise", "then"),
        ("ChainedWhen.then", "chained when"),
        ("ChainedThen.when", "chained then"),
        ("ChainedThen.otherwise", "chained then"),
    ];
    let states = [
        ("when", when),
        ("then", first),
        ("chained when", chained),
        ("chained then", second),
    ];
    for (id, expected) in needs {
        let given = if id.ends_with("then") {
            statement(lit(1))
        } else if id.ends_with("otherwise") {
            statement(lit(0))
        } else {
            conditions(vec![col("foo").gt(lit(2))])
        };
        for (state, condition) in &states {
            if *state == expected {
                continue;
            }
            let error = match entry(id).expect("an entry").build() {
                Build::ConditionStep(_) => {
                    link(id, condition.clone(), given.clone()).err().unwrap()
                }
                Build::ConditionEnd(_) => {
                    finish(id, condition.clone(), given.clone()).err().unwrap()
                }
                other => panic!("{id} is {other:?}"),
            };
            assert!(
                matches!(
                    &error,
                    BuildError::OutOfOrder { expected: wanted, found } if *wanted == expected && found == state
                ),
                "{id} after {state}: {error}"
            );
        }
    }
}

#[test]
fn text_for_a_value_is_refused_before_it_is_built() {
    // A statement is an expression: Python parses text as a column name, which a step writes
    // as `col`, so text is not an argument of it.
    let entry = entry("When.then").expect("an entry");
    let error = entry
        .arguments([("statement", text("foo"))])
        .err()
        .expect("text is not an expression");
    assert!(
        matches!(
            error,
            BuildError::WrongKind {
                parameter: "statement",
                expected: "an expression",
                found: "text"
            }
        ),
        "{error}"
    );
}
