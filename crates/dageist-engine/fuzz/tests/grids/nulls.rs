//! A comparison of a column of whole numbers with a literal, missing values compared too, as a
//! join's key: the walk refuses a key that is not elementwise where Polars does.
//!
//! Every class of whole numbers, every comparison of the catalogue given a literal on either side,
//! and every literal at the edges with every dtype, Polars as the oracle. The key is the comparison
//! cast to whole numbers, on both sides of the join.

use dageist_engine::allowed::{Context, Outer, RefusalSource, Rule};
use dageist_engine_fuzz::axes::{self, Param};
use dageist_engine_fuzz::doc::{call, then};
use dageist_engine_fuzz::judge::{Depth, Kind, Question};
use dageist_engine_fuzz::literals::{self, Literal};
use dageist_engine_fuzz::places;
use dageist_engine_fuzz::world::{Column, World};
use dageist_fuzz::grid::{Grid, Known, Pattern, Scale};
use serde_json::json;

#[derive(Default)]
struct Point {
    column: Option<Column>,
    param: Option<Param>,
    first: bool,
    literal: Literal,
}

fn grid(scale: Scale) -> Grid<Point> {
    let whole = World::get().classes("column").keep_unless(|_, column| {
        let kind = axes::kind_of_class(column.class.expect("a class"));
        (!kind.ends_with("whole number")).then(|| "no whole number".to_owned())
    });
    let comparisons = axes::literal_parameters("method").keep_unless(|_, param| {
        (param.entry.family() != dageist_engine::catalog::Family::Compare)
            .then(|| "it is no comparison".to_owned())
    });
    Grid::<Point>::new(if scale == Scale::Normal {
        "missing-comparisons"
    } else {
        "missing-comparisons-large"
    })
    .axis(whole, |point, column| point.column = Some(column.clone()))
    .axis(comparisons, |point, param| point.param = Some(*param))
    .axis(axes::orders("order"), |point, first| point.first = *first)
    .axis(literals::every_value().thin(scale, 4), |point, value| {
        point.literal.value = Some(value.clone());
    })
    .axis(
        literals::dtype_words().one_of_each(scale, "kind of class", |label, _| {
            dageist_engine::allowed::Representative::from_name(label)
                .map_or("none", axes::kind_of_class)
        }),
        |point, word| point.literal.word.clone_from(word),
    )
}

fn verdicts(scale: Scale) -> dageist_fuzz::grid::Verdicts<dageist_engine_fuzz::judge::Outcome> {
    let question = Question {
        walk_counts: |refusal| {
            matches!(
                refusal.refusal().source(),
                RefusalSource::Rule(Rule::KeyIsElementwise)
            )
        },
        polars_counts: |reason| reason.contains("elementwise"),
        depth: Depth::Plan,
        dtypes: false,
        cannot_tell_counts: |_| false,
    };
    grid(scale).judge(|point| {
        let column = point.column.as_ref().expect("a column");
        let key = axes::method_given(
            point.param.as_ref().expect("a comparison"),
            &point.literal,
            column,
            point.first,
        )
        .map(|compared| {
            then(
                compared,
                [call("Expr.cast", json!({ "dtype": { "dtype": "Int64" } }))],
            )
        });
        question.judge(key.and_then(|key| {
            places::at(
                Context::step(Outer::JoinLeft),
                key,
                World::get().of(&[&column.label]),
            )
        }))
    })
}

fn known() -> Vec<Known<Kind>> {
    vec![
        Known::new(
            Kind::BuildRefusesOnly,
            Pattern::new(),
            "the value does not fit the dtype it is given, or is a text given Boolean, so the \
             catalogue builds no literal to compare and the step engine refuses the step before \
             the walk is asked (#225)",
        )
        .saying_one_of(&["conversion from", "casting from", "cannot cast "]),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().is("value", "null").one_of(
                "method",
                [
                    "Expr.eq(other)",
                    "Expr.ne(other)",
                    "Expr.gt(other)",
                    "Expr.ge(other)",
                    "Expr.lt(other)",
                    "Expr.le(other)",
                    "Expr.is_between(lower_bound)",
                    "Expr.is_between(upper_bound)",
                ],
            ),
            "a column compared with a null given a dtype, by a comparison that does not compare \
             missing values, is folded by Polars into a null, which leaves the key no elementwise \
             one; the rule key-is-elementwise refuses a column compared with a null typed without \
             a dtype and not one given a dtype (#228)",
        )
        .saying("must be elementwise"),
        Known::new(
            Kind::SetAside,
            Pattern::new(),
            "Polars refuses to compare the two dtypes at all, a text or a date typed in with a \
             column of whole numbers, which the grid of comparisons with a literal judges; this \
             grid asks only whether the key is elementwise",
        )
        .saying_one_of(&[
            "cannot compare string with numeric",
            "could not determine supertype of",
            "could not evaluate",
            "got invalid or ambiguous dtypes",
        ]),
    ]
}

#[test]
fn the_walk_refuses_a_missing_comparison_with_a_literal_for_the_columns_polars_refuses_it_for() {
    // The grid this replaces judged 15,760 cells; it judges at least as many.
    verdicts(Scale::Normal).assert_agree(&known(), 15_760);
}

#[test]
#[ignore = "the large grid, run on request"]
fn the_large_grid_of_a_comparison_with_a_literal_as_a_join_key_is_taken_as_polars_does() {
    verdicts(Scale::Large).assert_agree(&known(), 200_000);
}
