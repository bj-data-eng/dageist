//! The dtype of a number typed without a dtype that starts a comparison for missing values, as
//! `lit(n).eq_missing(col("x"))`, with Polars as the judge.
//!
//! Such a number has no dtype of its own beside a column: Polars gives the number the dtype of the
//! column where it fits it, and the one it falls back to where it stands alone. Where the walk
//! stands a call that follows the number it asks the tables at the dtype the number takes beside
//! the operand, so the place after the start holds that dtype, and Polars works out the start
//! alone, a literal, as the dtype it falls back to. The fuzz harness compares the two at the
//! start of an expression, as it does after each link, which is what this grid holds as the
//! dtype at each place: the walk and Polars are asked of the same case, and both allow it where
//! they say the same of the key, the dtype at the start and after the link are Polars'.
//!
//! The grid crosses a column of every whole number dtype with the two methods, with the number
//! first and last, with whole numbers at the edges of every dtype and past them and numbers with
//! a fraction, and with a join's key and an item of a select as the place.

use polars::prelude::{DataFrame, DataType, NamedFrom, Series};
use serde_json::{Value, json};

use crate::grid::literals::{self, Written};
use crate::grid::{
    Case, Chosen, Connections, Grid, HoldsColumn, HoldsLiteral, Known, Question, Verdicts,
    column_dtypes, one_column_table,
};
use crate::support::{call, chain, field, field_then, many};

/// The dtypes of the columns whole numbers are held in.
fn whole_dtypes() -> Vec<DataType> {
    column_dtypes()
        .into_iter()
        .filter(|dtype| dtype.is_integer())
        .collect()
}

#[derive(Default)]
struct Point {
    column: Option<DataType>,
    literal: Chosen,
    method: &'static str,
    literal_first: bool,
    key: bool,
}

impl HoldsLiteral for Point {
    fn literal_mut(&mut self) -> &mut Chosen {
        &mut self.literal
    }
}

impl HoldsColumn for Point {
    fn column_mut(&mut self) -> &mut Option<DataType> {
        &mut self.column
    }
}

fn other() -> DataFrame {
    DataFrame::new(
        3,
        vec![Series::new("rf".into(), [Some(true), Some(false), None]).into()],
    )
    .expect("a table of one column")
}

/// The comparison for missing values of the number and the column.
fn comparison(point: &Point) -> Value {
    if point.literal_first {
        chain(
            point.literal.call(),
            vec![call(point.method, json!({ "other": field("x") }))],
        )
    } else {
        field_then(
            "x",
            vec![call(
                point.method,
                json!({ "other": point.literal.expression() }),
            )],
        )
    }
}

fn case_of(point: &Point) -> Result<Case, String> {
    let column = point.column.as_ref().ok_or("no column")?;
    let (entry, arguments) = if point.key {
        (
            "LazyFrame.join",
            json!({
                "other": { "table": "other" },
                "left_on": many(vec![comparison(point)]),
                "right_on": many(vec![field("rf")]),
            }),
        )
    } else {
        (
            "LazyFrame.select",
            json!({ "exprs": many(vec![comparison(point)]) }),
        )
    };
    Case::of_step(
        entry,
        arguments,
        one_column_table(column)?,
        Connections::new().table("other", other()),
    )
}

/// The numbers the grid writes: whole numbers at the edges of the dtypes and past them, and
/// numbers with a fraction, some of them whole.
fn numbers(normal: bool) -> Vec<Written> {
    let mut numbers = literals::edge_integers();
    numbers.extend(literals::edge_floats());
    if normal {
        numbers = numbers
            .into_iter()
            .enumerate()
            .filter(|(at, _)| at % 2 == 0)
            .map(|(_, number)| number)
            .collect();
    }
    numbers
}

fn grid(normal: bool) -> Grid<Point> {
    let values = numbers(normal)
        .into_iter()
        .map(|written| (written.label(), written))
        .collect();
    Grid::<Point>::new()
        .column_axis(whole_dtypes())
        .axis("number", values, |point, written| {
            point.literal.value = Some(written.clone());
        })
        .axis(
            "method",
            ["Expr.eq_missing", "Expr.ne_missing"]
                .map(|method| (method.to_owned(), method))
                .to_vec(),
            |point, method| point.method = method,
        )
        .axis(
            "order",
            vec![
                ("the column first".to_owned(), false),
                ("the number first".to_owned(), true),
            ],
            |point, first| point.literal_first = *first,
        )
        .axis(
            "place",
            vec![
                ("a join's key".to_owned(), true),
                ("a select".to_owned(), false),
            ],
            |point, key| point.key = *key,
        )
}

fn verdicts(normal: bool) -> Verdicts {
    grid(normal).judge(&Question::any_refusal().with_dtypes(), case_of)
}

fn known() -> Vec<Known> {
    Vec::new()
}

#[test]
fn the_dtype_of_a_number_that_starts_a_missing_comparison_is_the_dtype_polars_gives() {
    verdicts(true).assert_agree(&known(), 2_000);
}

#[test]
#[ignore = "the large grid, run on request"]
fn the_large_grid_of_a_number_that_starts_a_missing_comparison_is_taken_as_polars_does() {
    verdicts(false).assert_agree(&known(), 4_000);
}
