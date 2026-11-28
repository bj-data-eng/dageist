//! The contracts of the parts the grids are built of: every class, place, parameter and literal
//! edge is on its axis or named as left out, and the judge sorts a few cells whose outcome Polars'
//! documented behaviour settles.

use dageist_engine::allowed::{Context, Representative};
use dageist_engine::catalog::{self, Applies, Kind, Role};
use dageist_engine_fuzz::axes::{self, Effect};
use dageist_engine_fuzz::doc::{call, column_then, many};
use dageist_engine_fuzz::judge::{Case, Connections, Depth, Outcome, Question};
use dageist_engine_fuzz::literals::{self, Written};
use dageist_engine_fuzz::places;
use dageist_engine_fuzz::world::{World, dtypes_without_a_class};
use dageist_fuzz::grid::{Choices, Outcome as _};
use serde_json::json;

/// Whether `label` is on `axis` or named as left out of it.
fn accounted<V>(axis: &Choices<V>, label: &str) -> bool {
    axis.get(label).is_some() || axis.left_out().iter().any(|left| left.label == label)
}

#[test]
fn every_class_and_every_dtype_without_one_has_a_column_or_is_named_as_left_out() {
    let world = World::get();
    let columns = world.columns("column");
    for class in Representative::ALL {
        assert!(
            accounted(&columns, class.name()),
            "{class} is not accounted for"
        );
    }
    for (label, _) in dtypes_without_a_class() {
        assert!(accounted(&columns, &label), "{label} is not accounted for");
    }
    for left in columns.left_out() {
        assert!(
            !left.reason.is_empty(),
            "{} is left out with no reason",
            left.label
        );
    }
    // The scalar classes are what every rule meets; none of them may be left out.
    for class in Representative::MEASURED {
        if !class.is_nested() {
            assert!(columns.get(class.name()).is_some(), "{class} has no column");
        }
    }
}

#[test]
fn the_axis_of_classes_holds_one_column_of_each_class_of_its_dtype() {
    let world = World::get();
    let classes = world.classes("class");
    for (label, column) in classes.iter() {
        let class = column.class.expect("a class");
        assert_eq!(class.name(), label);
        assert_eq!(class.dtype(), column.dtype, "{label}");
        assert_eq!(
            world
                .frame()
                .column(&column.name)
                .expect("the column")
                .dtype(),
            &column.dtype
        );
    }
    assert!(
        classes.len() >= Representative::MEASURED.len() - 1,
        "{}",
        classes.len()
    );
}

#[test]
fn the_table_a_join_takes_holds_every_column_under_its_other_name() {
    let world = World::get();
    for (_, column) in world.columns("column").iter() {
        let other = world
            .other()
            .column(&column.other_name())
            .unwrap_or_else(|_| panic!("{} has no column on the other side", column.label));
        assert_eq!(other.dtype(), &column.dtype);
    }
}

#[test]
fn the_edges_of_every_class_of_whole_numbers_are_written_as_whole_numbers_or_as_text() {
    let integers = literals::edge_integers();
    let texts = literals::edge_texts();
    for class in literals::whole_classes() {
        let (lowest, highest) = literals::whole_bounds(class).expect("bounds");
        let edges = literals::whole_edges_of(class);
        assert_eq!(edges[1], lowest.to_string());
        assert_eq!(edges[2], highest.to_string());
        for edge in edges {
            let written = edge
                .parse::<i64>()
                .map_or_else(|_| Written::Text(edge.clone()), Written::Integer);
            let axis = if matches!(written, Written::Integer(_)) {
                &integers
            } else {
                &texts
            };
            assert!(
                axis.get(&written.label()).is_some(),
                "the edge {edge} of {class} is not written"
            );
        }
    }
    assert_eq!(
        literals::whole_edges_of(Representative::UInt8),
        ["-1", "0", "255", "256"]
    );
    assert_eq!(
        literals::whole_edges_of(Representative::Int128)[0],
        "-170141183460469231731687303715884105729"
    );
    // One past the highest `UInt128` holds is past what any text of a whole number the walk reads.
    assert!(
        texts
            .get("\"170141183460469231731687303715884105728\"")
            .is_some()
    );
}

#[test]
fn the_edges_of_every_class_of_floats_are_written_and_what_json_cannot_hold_is_left_out() {
    let floats = literals::edge_floats();
    for class in literals::float_classes() {
        let (largest, bits) = literals::float_bounds(class).expect("bounds");
        assert!(
            floats.get(&Written::Float(largest).label()).is_some(),
            "{class}"
        );
        let limit = (1_i64 << bits) as f64;
        assert!(
            floats.get(&Written::Float(limit).label()).is_some(),
            "{class}"
        );
    }
    assert!(floats.get("-0.0").is_some());
    let left: Vec<&str> = floats
        .left_out()
        .iter()
        .map(|left| left.label.as_str())
        .collect();
    assert_eq!(left, ["NaN", "inf", "-inf"]);
}

#[test]
fn every_scalar_class_is_a_dtype_a_literal_is_given_or_is_named_as_left_out() {
    let words = literals::dtype_words();
    assert_eq!(words.get("none"), Some(&None));
    for class in Representative::MEASURED {
        assert!(accounted(&words, class.name()), "{class}");
    }
    for class in [
        Representative::Int8,
        Representative::UInt128,
        Representative::Float16,
        Representative::String,
        Representative::Boolean,
    ] {
        assert!(
            words.get(class.name()).is_some(),
            "{class} is a dtype of a literal"
        );
    }
}

#[test]
fn every_place_the_tables_measure_is_on_the_axis_or_named_as_left_out() {
    let axis = places::places("place");
    for context in Context::all() {
        assert!(accounted(&axis, &context.name()), "{context}");
    }
    assert!(axis.len() >= 30, "{}", axis.len());
}

#[test]
fn a_column_of_whole_numbers_is_allowed_at_every_place_that_takes_a_value() {
    // `n` is a whole number with a repeat: Polars takes it as a value, a key and a sort key; a
    // filter, a having and a condition's predicate need a flag, and refuse it, a condition's
    // predicate only when the rows run.
    let question = Question::any_refusal().at(Depth::Rows);
    for (label, context) in places::places("place").iter() {
        let case = places::at(
            *context,
            dageist_engine_fuzz::doc::column("n"),
            World::get().of(&[]),
        );
        let judged = question.judge(case);
        assert!(
            matches!(judged.outcome(), Outcome::BothAllow | Outcome::BothRefuse),
            "{label}: {}",
            judged.outcome().said()
        );
    }
}

#[test]
fn the_judge_sorts_a_cell_both_allow_both_refuse_and_unbuildable_as_polars_says() {
    let question = Question::any_refusal();
    let abs_of = |label: &str| {
        let column = World::get().column(label);
        Case::of_step(
            "LazyFrame.select",
            json!({ "exprs": many(vec![column_then(&column.name, vec![call("Expr.abs", json!({}))])]) }),
            World::get().of(&[label]),
            Connections::new(),
        )
    };
    // Polars takes the absolute value of a whole number, and has no absolute value of text.
    assert_eq!(
        question.judge(abs_of("Int64")).outcome(),
        &Outcome::BothAllow
    );
    assert_eq!(
        question.judge(abs_of("String")).outcome(),
        &Outcome::BothRefuse
    );
    assert_eq!(
        question.judge(Err("no case".to_owned())).outcome(),
        &Outcome::Unbuildable("no case".to_owned())
    );
}

#[test]
fn every_parameter_the_walk_reads_by_its_role_and_its_siblings_are_on_the_axis_of_literals() {
    let axis = axes::literal_parameters("parameter");
    for entry in catalog::entries()
        .iter()
        .filter(|entry| entry.applies() == Applies::Expression)
    {
        let reads = entry.parameters().iter().any(|parameter| {
            matches!(parameter.kind(), Kind::Expression(_)) && parameter.role() != Role::Plain
        });
        for parameter in entry.parameters() {
            if !matches!(parameter.kind(), Kind::Expression(_)) {
                continue;
            }
            let label = format!("{}({})", entry.id().as_str(), parameter.name());
            if reads {
                assert!(axis.get(&label).is_some(), "{label} is not on the axis");
            } else {
                assert!(accounted(&axis, &label), "{label} is not accounted for");
            }
        }
    }
    for label in [
        "Expr.add(other)",
        "Expr.clip(upper_bound)",
        "Expr.replace_strict(new)",
        "Expr.replace_strict(default)",
        "Expr.str.pad_start(length)",
    ] {
        assert!(axis.get(label).is_some(), "{label}");
    }
}

#[test]
fn every_needed_parameter_of_a_method_is_given_a_value_but_those_of_a_kind_no_grid_writes() {
    let column = World::get().column("Int64");
    for entry in catalog::entries()
        .iter()
        .filter(|entry| entry.applies() == Applies::Expression)
    {
        for parameter in entry
            .parameters()
            .iter()
            .filter(|parameter| parameter.is_needed())
        {
            let filled = axes::filler(parameter, Some(column));
            match parameter.kind() {
                Kind::Table | Kind::Path | Kind::Pairs(..) => assert!(filled.is_err()),
                _ => assert!(
                    filled.is_ok(),
                    "{}({})",
                    entry.id().as_str(),
                    parameter.name()
                ),
            }
        }
    }
}

#[test]
fn what_a_call_does_after_a_class_is_what_the_tables_say_polars_does() {
    // The absolute value of a whole number is one, for each row; its sum is one value of its dtype
    // for a 64-bit whole number; a test for a null is a flag for each row; text has no sum.
    let entry = |id: &str| catalog::entry(id).expect("an entry");
    assert_eq!(
        axes::effect(entry("Expr.abs"), Representative::Int64),
        Effect::Keeps
    );
    assert_eq!(
        axes::effect(entry("Expr.sum"), Representative::Int64),
        Effect::Aggregates
    );
    assert_eq!(
        axes::effect(entry("Expr.is_null"), Representative::Int64),
        Effect::Changes
    );
    assert_eq!(
        axes::effect(entry("Expr.sum"), Representative::String),
        Effect::Refused
    );
    let keeps = axes::calls_that(
        "call",
        Effect::Keeps,
        &[Representative::Int64, Representative::Float64],
    );
    assert!(keeps.get("Expr.abs").is_some());
    assert!(keeps.get("Expr.sum").is_none());
    assert!(keeps.left_out().iter().any(|left| left.label == "Expr.sum"));
    // A shift keeps the dtype and the number of the values it follows.
    assert!(keeps.get("Expr.shift").is_some());
    for entry in catalog::entries()
        .iter()
        .filter(|entry| entry.applies() == Applies::Expression)
    {
        assert!(
            accounted(&keeps, entry.id().as_str()),
            "{}",
            entry.id().as_str()
        );
    }
}
