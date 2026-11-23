//! A sample of the committed lines, rebuilt on Polars' own API and run again:
//! what Polars does now is what the line says.

use polars::prelude::{DataType, Expr, NamedFrom, PolarsError, Scalar, Series, col, lit};

use crate::allowed::dtype::{Representative, dtype_name};
use crate::allowed::table::{DtypeLine, Outcome, Stage, lines};

use super::oracle::{Seen, see};
use super::sample::Frames;
use crate::allowed::table;

/// The call of a line of the dtype table made by hand, for the entries listed.
///
/// `Err` is a call that cannot be made: Polars takes `shift`'s `n` and
/// `fill_value` as one value, the number 1 of the argument's dtype, and
/// refuses to cast a number to some dtypes.
fn by_hand(
    entry: &str,
    parameter: Option<&str>,
    argument: Representative,
) -> Option<Result<Expr, PolarsError>> {
    let (a, b) = (col("a"), col("b"));
    let one_value = || {
        Series::new("literal".into(), [1_i64])
            .strict_cast(&argument.dtype())
            .and_then(|series| {
                Ok(lit(Scalar::new(
                    series.dtype().clone(),
                    series.get(0)?.into_static(),
                )))
            })
    };
    Some(Ok(match (entry, parameter) {
        ("Expr.cum_count", None) => a.cum_count(false),
        ("Expr.cum_max", None) => a.cum_max(false),
        ("Expr.cum_sum", None) => a.cum_sum(false),
        ("Expr.dt.date", None) => a.dt().date(),
        ("Expr.dt.month_start", None) => a.dt().month_start(),
        ("Expr.dt.time", None) => a.dt().time(),
        ("Expr.dt.total_days", None) => a.dt().total_days(false),
        ("Expr.first", None) => a.first(),
        ("Expr.last", None) => a.last(),
        ("Expr.len", None) => a.len(),
        ("Expr.max", None) => a.max(),
        ("Expr.min", None) => a.min(),
        ("Expr.sum", None) => a.sum(),
        ("Expr.is_null", None) => a.is_null(),
        ("Expr.is_not_null", None) => a.is_not_null(),
        ("Expr.shift", None) => a.shift(lit(1)),
        ("Expr.shift", Some("n")) => match one_value() {
            Ok(one) => a.shift(one),
            Err(error) => return Some(Err(error)),
        },
        ("Expr.shift", Some("fill_value")) => match one_value() {
            Ok(one) => a.shift_and_fill(lit(1), one),
            Err(error) => return Some(Err(error)),
        },
        ("Expr.eq", Some("other")) => a.eq(b),
        ("Expr.ne", Some("other")) => a.neq(b),
        ("Expr.gt", Some("other")) => a.gt(b),
        ("Expr.ge", Some("other")) => a.gt_eq(b),
        ("Expr.lt", Some("other")) => a.lt(b),
        ("Expr.le", Some("other")) => a.lt_eq(b),
        ("Expr.sub", Some("other")) => a - b,
        ("Expr.dt.offset_by", Some("by")) => a.dt().offset_by(b),
        _ => return None,
    }))
}

#[test]
fn a_sample_of_the_dtype_table_is_what_polars_does_when_the_call_is_made_by_hand() {
    let committed = lines(table::DTYPES, DtypeLine::parse).expect("the table reads");
    let mut checked = 0;
    let mut gaps = 0;
    for line in committed.iter().step_by(23) {
        // A function that starts an expression has no receiver, and no line is rebuilt for it.
        let Some(receiver) = line.receiver else {
            continue;
        };
        let argument = line.argument.unwrap_or(receiver);
        let Some(call) = by_hand(line.entry, line.parameter, argument) else {
            continue;
        };
        let frames = Frames::new(receiver, argument, &DataType::Int64);
        let seen = match &call {
            Ok(call) => see(&frames.rows, &frames.empty, |frame| {
                frame.select([call.clone()])
            }),
            Err(_) => Seen::Refused(Stage::Schema),
        };
        let context = format!(
            "{} on {} with {:?} {}",
            line.entry, receiver, line.parameter, argument
        );
        match (&line.outcome, seen) {
            (Outcome::Allowed(Some(produces)), Seen::Ran(dtype)) => {
                assert_eq!(produces.result.name(), dtype_name(&dtype), "{context}");
            }
            (Outcome::Refused(reason), Seen::Refused(stage)) => {
                assert_eq!(reason.stage, stage, "{context}: {}", reason.text);
                gaps += usize::from(stage == Stage::Rows);
            }
            // The schema and the rows gave different dtypes: the table calls that a refusal.
            (Outcome::Refused(reason), Seen::Ran(_))
                if reason.text.starts_with("the schema says") => {}
            (outcome, seen) => panic!("{context}: the table says {outcome}, Polars gave {seen:?}"),
        }
        checked += 1;
    }
    assert!(checked > 200, "only {checked} lines were rebuilt");
    assert!(
        gaps > 10,
        "the sample found {gaps} that only the rows refuse"
    );
}
