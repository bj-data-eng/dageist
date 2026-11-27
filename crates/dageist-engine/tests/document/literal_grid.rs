//! The rules that read a literal's value or dtype, each crossed with every literal a document can
//! write and every dtype of column it meets, with Polars as the judge.
//!
//! A rule reads a literal where Polars treats a number typed without a dtype differently from a
//! column or a typed literal: it adapts to the dtype of the value it meets, or is cast to it
//! strictly, or is read as a count or as a share. The literals worth crossing are those at the
//! edges of the dtypes, and those written as another kind than the dtype they are given, which
//! the step engine casts strictly before Polars sees them. The grids make every one at once, so
//! a gap shows up as one cell and not one review round later.
//!
//! The grids are built with [`crate::grid`], which says how they are written.

use polars::prelude::DataType;
use serde_json::{Value, json};

use crate::grid::literals::Written;
use crate::grid::{
    Case, Chosen, Connections, Depth, Grid, HoldsColumn, HoldsLiteral, Kind, Known, Pattern,
    Question, Verdicts, column_dtypes, one_column_table,
};
use crate::support::{call, field_then, many};

fn select(item: Value) -> Value {
    json!({ "exprs": many(vec![item]) })
}

/// A point of a method whose parameter a grid varies: the column, the literal, and which
/// parameter of the method the literal is given as.
#[derive(Default)]
struct Given {
    column: Option<DataType>,
    literal: Chosen,
    method: &'static str,
    parameter: &'static str,
}

impl HoldsLiteral for Given {
    fn literal_mut(&mut self) -> &mut Chosen {
        &mut self.literal
    }
}

impl HoldsColumn for Given {
    fn column_mut(&mut self) -> &mut Option<DataType> {
        &mut self.column
    }
}

/// The case of `method` run on the column `x` and given the literal as `parameter`.
fn method_given_the_literal(point: &Given) -> Result<Case, String> {
    let column = point.column.as_ref().expect("a column");
    let item = field_then(
        "x",
        vec![call(
            point.method,
            json!({ point.parameter: point.literal.expression() }),
        )],
    );
    Case::of_step(
        "LazyFrame.select",
        select(item),
        one_column_table(column)?,
        Connections::new(),
    )
}

/// The columns of dates, moments, lengths of time and times of day.
fn moments() -> Vec<DataType> {
    column_dtypes()
        .into_iter()
        .filter(|dtype| {
            matches!(
                dtype,
                DataType::Date | DataType::Datetime(..) | DataType::Duration(_) | DataType::Time
            )
        })
        .collect()
}

#[test]
fn a_number_typed_without_a_dtype_that_meets_a_date_or_a_time_is_refused_as_polars_refuses_it() {
    // Polars gives such a number no dtype beside a date, a moment, a length of time or a time of
    // day, and refuses to fill them with it or to add it to them, whatever the number is.
    let methods: Vec<(String, (&'static str, &'static str))> =
        [("Expr.fill_null", "value"), ("Expr.add", "other")]
            .into_iter()
            .map(|(method, parameter)| (format!("{method}({parameter})"), (method, parameter)))
            .collect();
    let numbers = [
        Written::Integer(-1),
        Written::Integer(1),
        Written::Integer(300),
        Written::Integer(i64::MAX),
        Written::Float(1.0),
        Written::Float(1.5),
        Written::Float(2e38),
    ];
    let grid = Grid::<Given>::new()
        .column_axis(moments())
        .axis("method", methods, |point, (method, parameter)| {
            point.method = method;
            point.parameter = parameter;
        })
        .literal_axes(numbers.to_vec(), &[None]);
    let verdicts = grid.judge(
        &Question::any_refusal().at(Depth::Rows),
        method_given_the_literal,
    );
    verdicts.assert_agree(&[], 50);
}
