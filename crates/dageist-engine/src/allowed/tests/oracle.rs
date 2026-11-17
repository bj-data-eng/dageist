//! Running Polars by hand: the expected results of the tests below are what
//! Polars itself does when the calls are made straight on its API, not
//! through the catalogue or the generator's helpers.

use polars::prelude::{DataFrame, DataType, Expr, IntoLazy, LazyFrame};

use crate::allowed::dtype::Representative;
use crate::allowed::table::Stage;

use super::sample::Frames;

/// What Polars did with a plan on the sample tables.
#[derive(Debug, PartialEq)]
pub(super) enum Seen {
    /// The schema resolved, the rows ran and so did an empty table: the
    /// dtype of the one column.
    Ran(DataType),
    /// Polars refused it at this stage.
    Refused(Stage),
}

/// Runs `plan` as Polars does: the schema, then three rows, then none. A
/// panic of Polars is its refusing the rows.
pub(super) fn see(
    rows: &DataFrame,
    empty: &DataFrame,
    plan: impl Fn(LazyFrame) -> LazyFrame,
) -> Seen {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| seen(rows, empty, &plan)))
        .unwrap_or(Seen::Refused(Stage::Rows))
}

fn seen(rows: &DataFrame, empty: &DataFrame, plan: &dyn Fn(LazyFrame) -> LazyFrame) -> Seen {
    let mut frame = plan(rows.clone().lazy());
    let Ok(schema) = frame.collect_schema() else {
        return Seen::Refused(Stage::Schema);
    };
    if frame.collect().is_err() {
        return Seen::Refused(Stage::Rows);
    }
    if plan(empty.clone().lazy()).collect().is_err() {
        return Seen::Refused(Stage::Empty);
    }
    Seen::Ran(
        schema
            .iter()
            .next()
            .map(|(_, dtype)| dtype.clone())
            .expect("a column"),
    )
}

/// `expr` selected on a table whose column `a` has the dtype `receiver` and
/// whose column `b` has the dtype `argument`.
pub(super) fn selected(receiver: Representative, argument: Representative, expr: Expr) -> Seen {
    let frames = Frames::new(receiver, argument, &DataType::Int64);
    see(&frames.rows, &frames.empty, |frame| {
        frame.select([expr.clone()])
    })
}
