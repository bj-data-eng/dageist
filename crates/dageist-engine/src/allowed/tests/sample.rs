//! The sample tables the combinations are run on.
//!
//! Every column has three rows with a null among them, so a combination is
//! run on values and on a missing one; the same tables without rows are the
//! empty ones. The left table has the column under test (`a`), the column that
//! stands for an argument (`b`), and columns that give an expression
//! something to group, order and test by: `k` (whole numbers, with a repeat),
//! `f` (`Boolean`), `g` (text, with a repeat), `h` (text that groups the first
//! two rows), `u` (whole numbers all different, which a join of every kind can
//! be made on) and `key` (a column of the dtype a join key must have). The other table has the same, under the names
//! `ra`, `rb`, `rf`, `rk` and `key`, so that an expression evaluated against
//! the wrong table finds no column.

use std::sync::LazyLock;

use polars::prelude::{
    DataFrame, DataType, IntoLazy, IntoSeries, LazyFrame, ListChunked, NamedFrom, Series,
    StructChunked,
};

use super::super::dtype::{Inner, Representative};

/// How many rows the sample's tables have.
pub(super) const SAMPLE_ROWS: usize = 3;

fn ints(name: &str) -> Series {
    Series::new(name.into(), [Some(1_i64), None, Some(3)])
}

fn integer_list(values: [Option<&[i64]>; 3], name: &str) -> Series {
    ListChunked::from_iter(
        values
            .into_iter()
            .map(|row| row.map(|row| Series::new("".into(), row))),
    )
    .into_series()
    .with_name(name.into())
}

/// The three rows of a column of `dtype`, named `name`: a value, a null and a
/// value. Text is a duration in days, so it is fit to offset a date by.
pub(super) fn series(dtype: Representative, name: &str) -> Series {
    static MADE: LazyLock<Vec<Series>> = LazyLock::new(|| {
        Representative::ALL
            .into_iter()
            .map(|dtype| made(dtype, "values"))
            .collect()
    });
    let at = Representative::ALL
        .iter()
        .position(|known| *known == dtype)
        .expect("a representative dtype is one of them");
    MADE[at].clone().with_name(name.into())
}

fn made(dtype: Representative, name: &str) -> Series {
    match dtype {
        Representative::String => Series::new(name.into(), [Some("1d"), None, Some("2d")]),
        Representative::Boolean => Series::new(name.into(), [Some(true), None, Some(false)]),
        Representative::Null => Series::new_null(name.into(), 3),
        Representative::List(Inner::Int64) => {
            integer_list([Some(&[1, 2][..]), None, Some(&[3][..])], name)
        }
        Representative::List(Inner::String) => ListChunked::from_iter([
            Some(Series::new("".into(), ["a", "b"])),
            None,
            Some(Series::new("".into(), ["c"])),
        ])
        .into_series()
        .with_name(name.into()),
        Representative::Struct => StructChunked::from_series(
            name.into(),
            3,
            [
                ints("x"),
                Series::new("y".into(), [Some("a"), None, Some("c")]),
            ]
            .iter(),
        )
        .expect("two columns of three rows make a struct")
        .into_series(),
        Representative::Array(Inner::Int64) => {
            integer_list([Some(&[1, 2][..]), None, Some(&[3, 4][..])], name)
                .cast(&dtype.dtype())
                .expect("lists of two cast to an array of two")
        }
        Representative::List(inner) => list_of(inner, name, false),
        Representative::Array(inner) => list_of(inner, name, true)
            .cast(&dtype.dtype())
            .unwrap_or_else(|error| panic!("lists of two cast to {dtype}: {error}")),
        other => ints(name)
            .cast(&other.dtype())
            .unwrap_or_else(|error| panic!("whole numbers cast to {other}: {error}")),
    }
}

/// A column of lists of the dtype `inner`: of the two values that its own sample holds in the
/// first row, a null, and of the second value in the last, which is joined by the first where the
/// lists must be as long as one another, as an array's are.
fn list_of(inner: Inner, name: &str, equal_lengths: bool) -> Series {
    let values = made(inner.class(), "");
    let (first, second) = (values.slice(0, 1), values.slice(2, 1));
    let mut both = first.clone();
    both.append(&second)
        .expect("two values of one dtype are appended");
    let mut last = second.clone();
    if equal_lengths {
        last.append(&first)
            .expect("two values of one dtype are appended");
    }
    ListChunked::from_iter([Some(both), None, Some(last)])
        .into_series()
        .with_name(name.into())
}

fn values_of(name: &str) -> DataFrame {
    DataFrame::new(2, vec![Series::new(name.into(), ["x", "y"]).into()])
        .expect("a column of two rows")
}

/// The tables a combination is run on, with rows and without.
pub(super) struct Frames {
    /// The left table, with rows.
    pub rows: DataFrame,
    /// The left table, empty.
    pub empty: DataFrame,
    /// The other table, with rows.
    pub other_rows: DataFrame,
    /// The other table, empty.
    pub other_empty: DataFrame,
    /// The values a pivot on `g` makes columns for.
    pub on_g: DataFrame,
    /// The values a pivot on `h` makes columns for.
    pub on_h: DataFrame,
}

impl Frames {
    /// The tables with `a` of the dtype `receiver`, `b` of `argument`, and a
    /// `key` column of the dtype `key`, all null.
    pub(super) fn new(receiver: Representative, argument: Representative, key: &DataType) -> Self {
        let side = |names: [&str; 6], key_column: Series| {
            let [a, b, f, k, g, h] = names;
            DataFrame::new(
                3,
                vec![
                    series(receiver, a).into(),
                    series(argument, b).into(),
                    Series::new(f.into(), [Some(true), Some(false), Some(true)]).into(),
                    Series::new(k.into(), [1_i64, 1, 2]).into(),
                    Series::new(g.into(), ["x", "y", "x"]).into(),
                    Series::new(h.into(), ["x", "x", "y"]).into(),
                    key_column.into(),
                    Series::new("u".into(), [1_i64, 2, 3]).into(),
                ],
            )
            .expect("columns of three rows")
        };
        let key_of = |name: &str| Series::full_null(name.into(), 3, key);
        let rows = side(["a", "b", "f", "k", "g", "h"], key_of("key"));
        let other_rows = side(["ra", "rb", "rf", "rk", "rg", "rh"], key_of("key"));
        Self {
            empty: rows.clear(),
            other_empty: other_rows.clear(),
            rows,
            other_rows,
            on_g: values_of("g"),
            on_h: values_of("h"),
        }
    }

    /// The left table as a lazy frame, with rows or without.
    pub(super) fn left(&self, empty: bool) -> LazyFrame {
        if empty { &self.empty } else { &self.rows }.clone().lazy()
    }

    /// The other table as a lazy frame, with rows or without.
    pub(super) fn other(&self, empty: bool) -> LazyFrame {
        if empty {
            &self.other_empty
        } else {
            &self.other_rows
        }
        .clone()
        .lazy()
    }
}
