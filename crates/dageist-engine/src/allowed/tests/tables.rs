//! The committed tables as artifacts: how they read, that they are complete,
//! and the findings the first part of issue 44 reported.

use std::collections::BTreeSet;

use crate::allowed::context::{Context, Nested, Outer, Position};
use crate::allowed::dtype::ResultDtype;
use crate::allowed::dtype::{Representative, dtype_name};
use crate::allowed::rules::Rule;
use crate::allowed::table::{
    self, ContextLine, DtypeLine, OptionLine, Outcome, Shape, Stage, Tables, lines,
};
use crate::catalog::{self, Applies, Kind, ParameterDefault};

/// Each line of a table, read and written again, is the line.
fn round_trips(text: &'static str, name: &str, read: impl Fn(&str) -> Option<String>) {
    let mut previous: Option<&str> = None;
    for line in text
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
    {
        assert_eq!(read(line).as_deref(), Some(line), "{name}: {line}");
        assert!(
            previous.is_none_or(|before| before < line),
            "{name}: the lines are sorted and none is repeated, but {line:?} follows {previous:?}"
        );
        previous = Some(line);
    }
    assert!(previous.is_some(), "{name} has lines");
}

#[test]
fn every_line_of_every_table_reads_and_is_written_back_exactly() {
    round_trips(table::DTYPES, "dtypes.txt", |line| {
        DtypeLine::parse(line).ok().map(|line| line.render())
    });
    round_trips(table::CONTEXTS, "contexts.txt", |line| {
        ContextLine::parse(line).ok().map(|line| line.render())
    });
    round_trips(table::OPTIONS, "options.txt", |line| {
        OptionLine::parse(line).ok().map(|line| line.render())
    });
}

#[test]
fn the_representative_dtypes_are_named_and_classed_consistently() {
    let names: BTreeSet<&str> = Representative::ALL
        .iter()
        .map(|dtype| dtype.name())
        .collect();
    assert_eq!(names.len(), Representative::ALL.len());
    for dtype in Representative::ALL {
        assert_eq!(Representative::from_name(dtype.name()), Some(dtype));
        assert_eq!(Representative::of(&dtype.dtype()), Some(dtype), "{dtype}");
        assert_eq!(dtype_name(&dtype.dtype()), dtype.name());
    }
}

#[test]
fn the_contexts_are_six_positions_and_each_with_four_inside_it() {
    let contexts = Context::all();
    assert_eq!(contexts.len(), 6 + 6 * 4);
    for context in &contexts {
        assert_eq!(Context::from_name(&context.name()).as_ref(), Some(context));
    }
    assert_eq!(
        Context::inside(Outer::Select, Nested::OrderBy).name(),
        "select > over.order_by"
    );
}

#[test]
fn every_position_names_the_entry_and_parameter_that_hold_it() {
    for outer in Outer::ALL {
        let (entry, parameter) = outer.host();
        let host = catalog::entry(entry).unwrap_or_else(|| panic!("no entry {entry}"));
        assert!(
            host.parameters().iter().any(|p| p.name() == parameter),
            "{entry} {parameter}"
        );
        assert_eq!(
            Position::of_parameter(entry, parameter),
            Some(Position::Outer(outer))
        );
    }
    for nested in Nested::ALL {
        let (entry, parameter) = nested.host();
        let host = catalog::entry(entry).unwrap_or_else(|| panic!("no entry {entry}"));
        assert!(
            host.parameters().iter().any(|p| p.name() == parameter),
            "{entry} {parameter}"
        );
        assert_eq!(
            Position::of_parameter(entry, parameter),
            Some(Position::Nested(nested))
        );
    }
    // Only a join's `right_on` is evaluated against the other table.
    for outer in Outer::ALL {
        assert_eq!(
            outer.evaluated_against(),
            (outer == Outer::JoinRight).then_some("other")
        );
    }
}

#[test]
fn only_the_first_two_positions_of_a_place_count() {
    let positions = [
        Position::Outer(Outer::Select),
        Position::Nested(Nested::OrderBy),
        Position::Nested(Nested::ConditionPredicate),
    ];
    assert_eq!(
        Context::from_positions(&positions),
        Some(Context::inside(Outer::Select, Nested::OrderBy))
    );
    assert_eq!(Context::from_positions(&positions[1..]), None);
    assert_eq!(Context::from_positions(&[]), None);
}

#[test]
fn the_dtype_table_has_a_line_for_every_method_receiver_and_argument() {
    let mut expected = 0;
    for entry in catalog::entries()
        .iter()
        .filter(|entry| entry.applies() == Applies::Expression)
    {
        let expressions = entry
            .parameters()
            .iter()
            .filter(|parameter| matches!(parameter.kind(), Kind::Expression(_)))
            .count();
        let must_be_given = entry.parameters().iter().any(|p| {
            p.default() == ParameterDefault::Required && matches!(p.kind(), Kind::Expression(_))
        });
        let n = Representative::ALL.len();
        expected += if must_be_given { 0 } else { n } + expressions * n * n;
    }
    assert_eq!(Tables::committed().dtypes.len(), expected);
}

#[test]
fn the_context_table_has_a_line_for_every_entry_that_is_not_a_step_in_every_context() {
    let entries: Vec<_> = catalog::entries()
        .iter()
        .filter(|entry| entry.applies() != Applies::Frame)
        .collect();
    let tables = Tables::committed();
    assert_eq!(tables.contexts.len(), entries.len() * Context::all().len());
    for entry in entries {
        for context in Context::all() {
            let cell = tables
                .contexts
                .get(&(entry.id().as_str(), context))
                .unwrap_or_else(|| panic!("{} in {context}", entry.id().as_str()));
            // The line names the rules that govern it, as the rules say.
            assert_eq!(
                cell.rules,
                Rule::governing(entry.id().as_str(), context),
                "{} in {context}",
                entry.id().as_str()
            );
        }
    }
}

/// The values of each option of `entry`, by the words the table uses.
fn option_values(entry: &catalog::Entry) -> Vec<usize> {
    entry
        .parameters()
        .iter()
        .filter_map(|parameter| match parameter.kind() {
            Kind::Flag => {
                Some(2 + usize::from(parameter.default() == ParameterDefault::Python("None")))
            }
            Kind::Choice(words) => Some(words.len()),
            _ => None,
        })
        .collect()
}

#[test]
fn the_option_table_covers_each_option_of_one_and_each_pair_of_values_of_more() {
    let mut expected = 0;
    for entry in catalog::entries()
        .iter()
        .filter(|entry| matches!(entry.applies(), Applies::Expression | Applies::Frame))
    {
        let values = option_values(entry);
        expected += match values.as_slice() {
            [] => 0,
            [only] => *only,
            many => {
                let sum: usize = many.iter().sum();
                let squares: usize = many.iter().map(|count| count * count).sum();
                (sum * sum - squares) / 2
            }
        };
    }
    assert_eq!(Tables::committed().options.len(), expected);
}

#[test]
fn the_mistakes_the_schema_does_not_show_are_refused_when_the_rows_ran() {
    // Issue 44 reported that Polars gives these a schema and fails only on rows.
    let tables = Tables::committed();
    let stage = |entry: &'static str, parameter, receiver, argument| match tables
        .dtypes
        .get(&(entry, parameter, receiver, argument))
    {
        Some(Outcome::Refused(reason)) => Some(reason.stage),
        other => panic!("{entry} on {receiver}: {other:?}"),
    };
    let text = Representative::String;
    assert_eq!(stage("Expr.dt.date", None, text, None), Some(Stage::Rows));
    assert_eq!(stage("Expr.cum_sum", None, text, None), Some(Stage::Rows));
    assert_eq!(
        stage("Expr.dt.total_days", None, Representative::Int64, None),
        Some(Stage::Rows)
    );
    assert_eq!(
        stage("Expr.eq", Some("other"), text, Some(Representative::Int64)),
        Some(Stage::Rows)
    );
    // And there are a great many more: every line that is refused only on rows.
    let at_rows = tables
        .dtypes
        .values()
        .filter(
            |outcome| matches!(outcome, Outcome::Refused(reason) if reason.stage == Stage::Rows),
        )
        .count();
    assert!(at_rows > 1000, "{at_rows}");
}

#[test]
fn a_method_that_reduces_gives_one_value_and_one_that_runs_gives_many() {
    let tables = Tables::committed();
    let shape =
        |entry: &'static str| match tables
            .dtypes
            .get(&(entry, None, Representative::Int64, None))
        {
            Some(Outcome::Allowed(Some(produces))) => produces.shape,
            other => panic!("{entry}: {other:?}"),
        };
    for reduces in [
        "Expr.sum",
        "Expr.min",
        "Expr.max",
        "Expr.len",
        "Expr.first",
        "Expr.last",
    ] {
        assert_eq!(shape(reduces), Shape::One, "{reduces}");
    }
    for runs in [
        "Expr.cum_sum",
        "Expr.cum_max",
        "Expr.cum_count",
        "Expr.is_null",
        "Expr.shift",
    ] {
        assert_eq!(shape(runs), Shape::Many, "{runs}");
    }
}

#[test]
fn the_named_rules_have_names_that_read_back() {
    for rule in Rule::ALL {
        assert_eq!(Rule::from_name(rule.name()), Some(rule));
        assert!(!rule.statement().is_empty());
    }
    assert!(lines(table::OPTIONS, |_| Ok(())).is_ok());
}

#[test]
fn an_entry_may_stand_in_a_context_where_it_may_not_end_a_chain() {
    let tables = Tables::committed();
    let cell = |entry: &'static str, context| {
        tables
            .contexts
            .get(&(entry, context))
            .unwrap_or_else(|| panic!("{entry} in {context}"))
    };
    // A sum is no `Boolean`, so it does not end a filter's predicate, and
    // a test for a value that follows it does.
    let sum = cell("Expr.sum", Context::step(Outer::Filter));
    assert!(matches!(sum.continues, Outcome::Allowed(_)), "{sum:?}");
    assert!(matches!(sum.ends, Outcome::Refused(_)), "{sum:?}");
    // A window is refused as a join key whatever follows it.
    let window = cell("Expr.over", Context::step(Outer::JoinLeft));
    assert!(
        matches!(window.continues, Outcome::Refused(_)),
        "{window:?}"
    );
    assert!(matches!(window.ends, Outcome::Refused(_)), "{window:?}");
    // And in a pivot's aggregate, where a group's values are one value.
    let window = cell("Expr.over", Context::step(Outer::PivotAggregate));
    assert!(
        matches!(window.continues, Outcome::Refused(_)),
        "{window:?}"
    );
}

#[test]
fn a_table_that_lacks_one_of_the_two_lines_of_a_cell_does_not_read() {
    let lone = "polars.col | select | continues | - | - | - | allowed Int64 many\n";
    let error = Tables::read(table::DTYPES, lone, table::OPTIONS)
        .expect_err("the line that ends is missing");
    assert_eq!(error.0, "contexts.txt");
    assert!(error.1.to_string().contains("lacks"), "{}", error.1);
}

#[test]
fn an_end_rule_about_a_dtype_does_not_refuse_what_it_does_not_know() {
    let predicate = Context::inside(Outer::Select, Nested::ConditionPredicate);
    let boolean = ResultDtype::from_name("Boolean");
    let number = ResultDtype::from_name("Int64");
    // A dtype that is known to be something else breaks the rule, even one
    // that has no representative class.
    let list = ResultDtype::from_name("List(Float64)");
    assert_eq!(list.class(), None);
    for context in [
        predicate,
        Context::step(Outer::Filter),
        Context::inside(Outer::Filter, Nested::ConditionValue),
    ] {
        let rule = Rule::ALL
            .into_iter()
            .find(|rule| rule.needs_boolean_end(context))
            .expect("a rule about the Boolean");
        assert_eq!(
            Rule::forbids_end(context, Some(&number), Shape::Many),
            Some(rule)
        );
        assert_eq!(
            Rule::forbids_end(context, Some(&list), Shape::Many),
            Some(rule)
        );
        assert_eq!(
            Rule::forbids_end(context, Some(&boolean), Shape::Many),
            None
        );
        // Not known: a start takes the dtype of a column, and an argument
        // decides what some methods give.
        assert_eq!(Rule::forbids_end(context, None, Shape::Many), None);
    }
    // The rule about how many values is not about a dtype.
    let aggregate = Context::step(Outer::PivotAggregate);
    assert_eq!(
        Rule::forbids_end(aggregate, None, Shape::Many),
        Some(Rule::PivotAggregateGivesOneValue)
    );
    assert_eq!(Rule::forbids_end(aggregate, None, Shape::One), None);
}
