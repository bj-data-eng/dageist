//! The tables a case runs on, which are the sample tables the tables of allowed
//! combinations were measured on: every column has three rows, the columns of
//! the dtypes under test (`a`, `b`, `ra`, `rb`) hold a null in the middle one,
//! and the same tables without rows are the empty ones.
//!
//! The left table has `a`, of the dtype under test, `b`, of the dtype that
//! stands for an argument, and columns that give an expression something to
//! test, group and order by: `f` (`Boolean`), `k` (whole numbers with a
//! repeat), `g` (text with a repeat), `h` (text that groups the first two rows)
//! and `u` (whole numbers all different), and `key`, a column of the dtype a
//! join key must have. The other table has the same under the names `ra`,
//! `rb`, `rf`, `rk`, `rg`, `rh`, so that an expression evaluated against the
//! wrong table finds no column.

use dageist_engine::allowed::Representative;
use dageist_engine::document::Inputs;
use polars::prelude::{
    DataFrame, DataType, IntoLazy, IntoSeries, ListChunked, NamedFrom, Series, StructChunked,
};

/// The name of the column the join's `index`th key is paired with on the other
/// side, when a case pairs a key with a bare column: `key`, then `key2`, `key3`.
pub fn key_column(index: usize) -> String {
    if index == 0 {
        "key".to_owned()
    } else {
        format!("key{}", index + 1)
    }
}

/// The index of the column `name` as [`key_column`] names it, if it is one.
pub fn key_column_index(name: &str) -> Option<usize> {
    let digits = name.strip_prefix("key")?;
    if digits.is_empty() {
        return Some(0);
    }
    let number: usize = digits.parse().ok()?;
    (number >= 2 && key_column(number - 1) == name).then(|| number - 1)
}

/// The name of the connected table a join takes.
pub const OTHER: &str = "other";
/// The name of the column a probe stands a value of some dtype in with.
pub const STAND_IN: &str = "p";

/// The name of the `index`th column that stands a value in: [`STAND_IN`], then
/// `p2`, `p3`.
pub fn stand_in_column(index: usize) -> String {
    if index == 0 {
        STAND_IN.to_owned()
    } else {
        format!("{STAND_IN}{}", index + 1)
    }
}
/// The name of the connected table a pivot takes its columns from.
pub const ON_COLUMNS: &str = "on_h";

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

/// The three rows of a column of `dtype`: a value, a null and a value. Text is
/// a duration in days, so it can offset a date.
fn column(dtype: Representative, name: &str) -> Series {
    match dtype {
        Representative::String => Series::new(name.into(), [Some("1d"), None, Some("2d")]),
        Representative::Boolean => Series::new(name.into(), [Some(true), None, Some(false)]),
        Representative::Null => Series::new_null(name.into(), 3),
        Representative::ListOfInteger => {
            integer_list([Some(&[1, 2][..]), None, Some(&[3][..])], name)
        }
        Representative::ListOfString => ListChunked::from_iter([
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
        Representative::Array => integer_list([Some(&[1, 2][..]), None, Some(&[3, 4][..])], name)
            .cast(&dtype.dtype())
            .expect("lists of two cast to an array of two"),
        other => ints(name)
            .cast(&other.dtype())
            .unwrap_or_else(|error| panic!("whole numbers cast to {other}: {error}")),
    }
}

/// `dtype` as a column can hold it: a list whose element dtype Polars has not worked out is a list
/// of nulls, for Polars panics making a column of the one it has not.
fn resolved(dtype: &DataType) -> DataType {
    match dtype {
        DataType::List(inner) => DataType::List(Box::new(match &**inner {
            DataType::Unknown(_) => DataType::Null,
            other => resolved(other),
        })),
        DataType::Array(inner, width) => DataType::Array(
            Box::new(match &**inner {
                DataType::Unknown(_) => DataType::Null,
                other => resolved(other),
            }),
            *width,
        ),
        other => other.clone(),
    }
}

/// The tables of one case, with rows and without.
pub struct Frames {
    /// The left table, with rows.
    pub rows: DataFrame,
    /// The left table, empty.
    pub empty: DataFrame,
    other_rows: DataFrame,
    other_empty: DataFrame,
    on_columns: DataFrame,
}

impl Frames {
    /// The tables with `a` of the class `receiver`, `b` of `argument`, and a
    /// column for each of the dtypes `keys`, named by [`key_column`], all null.
    pub fn new(receiver: Representative, argument: Representative, keys: &[DataType]) -> Self {
        Self::with_key(receiver, argument, Some(keys))
    }

    /// The tables as [`Frames::new`] makes them, but with no key columns when
    /// `keys` is `None`: their dtypes are the join keys', which are known only
    /// when the join is run, so a prediction made before that does not know them.
    pub fn with_key(
        receiver: Representative,
        argument: Representative,
        keys: Option<&[DataType]>,
    ) -> Self {
        let side = |names: [&str; 6]| {
            let [a, b, f, k, g, h] = names;
            let mut columns = vec![
                column(receiver, a).into(),
                column(argument, b).into(),
                Series::new(f.into(), [Some(true), Some(false), Some(true)]).into(),
                Series::new(k.into(), [1_i64, 1, 2]).into(),
                Series::new(g.into(), ["x", "y", "x"]).into(),
                Series::new(h.into(), ["x", "x", "y"]).into(),
                Series::new("u".into(), [1_i64, 2, 3]).into(),
            ];
            for (at, key) in keys.unwrap_or_default().iter().enumerate() {
                columns.push(
                    Series::full_null(key_column(at).as_str().into(), 3, &resolved(key)).into(),
                );
            }
            DataFrame::new(3, columns).expect("columns of three rows")
        };
        let rows = side(["a", "b", "f", "k", "g", "h"]);
        let other_rows = side(["ra", "rb", "rf", "rk", "rg", "rh"]);
        Self {
            empty: rows.clear(),
            other_empty: other_rows.clear(),
            rows,
            other_rows,
            on_columns: DataFrame::new(2, vec![Series::new("h".into(), ["x", "y"]).into()])
                .expect("a column of two rows"),
        }
    }

    /// Whether the column `name` of the left table, or of the other, holds a null
    /// in the row `row`; `None` for a name that is no column of either, or a row
    /// that is none.
    pub fn holds_a_null_at(&self, name: &str, row: usize) -> Option<bool> {
        let column = self
            .rows
            .column(name)
            .or_else(|_| self.other_rows.column(name))
            .ok()?;
        column.is_null().get(row)
    }

    /// The dtype of the column `name` of the left table, or of the other; `None` for a name that is
    /// no column of either.
    pub fn dtype_of(&self, name: &str) -> Option<&DataType> {
        self.rows
            .column(name)
            .or_else(|_| self.other_rows.column(name))
            .ok()
            .map(|column| column.dtype())
    }

    /// The same tables with a column of each class added to each, named by
    /// [`stand_in_column`] in order: values of the dtypes that a probe needs where
    /// the case's own tables have none, which Polars cannot fold as it can a
    /// literal.
    pub fn with_stand_ins(self, classes: &[Representative]) -> Self {
        let added: Vec<polars::prelude::Column> = classes
            .iter()
            .enumerate()
            .map(|(at, class)| column(*class, &stand_in_column(at)).into())
            .collect();
        let add = |frame: &DataFrame| {
            frame
                .hstack(&added)
                .expect("columns of three rows beside columns of three rows")
        };
        let rows = add(&self.rows);
        let other_rows = add(&self.other_rows);
        Self {
            empty: rows.clear(),
            other_empty: other_rows.clear(),
            rows,
            other_rows,
            on_columns: self.on_columns,
        }
    }

    /// The same tables with the other table as the one a step starts from, for
    /// an expression that a join's `right_on` holds, which reads that table's
    /// columns.
    pub fn other_as_left(&self) -> Self {
        Self {
            rows: self.other_rows.clone(),
            empty: self.other_empty.clone(),
            other_rows: self.rows.clone(),
            other_empty: self.empty.clone(),
            on_columns: self.on_columns.clone(),
        }
    }

    /// What the steps' table arguments name: the other table, with rows or
    /// without, and the values a pivot makes columns for.
    pub fn inputs(&self, empty: bool) -> Inputs {
        let mut inputs = Inputs::new();
        let other = if empty {
            &self.other_empty
        } else {
            &self.other_rows
        };
        inputs
            .add_table(OTHER, "Other", other.clone().lazy())
            .expect("a first input");
        inputs
            .add_table(ON_COLUMNS, "Columns", self.on_columns.clone().lazy())
            .expect("a second input");
        inputs
    }
}

/// The class of the column `name` of the left or the other table, which the
/// case's `a` and `b` give; `None` for a name that is no column of either.
pub fn column_class(
    name: &str,
    receiver: Representative,
    argument: Representative,
) -> Option<Representative> {
    Some(match name {
        "a" | "ra" => receiver,
        "b" | "rb" => argument,
        "f" | "rf" => Representative::Boolean,
        "k" | "rk" | "u" => Representative::Int64,
        "g" | "rg" | "h" | "rh" => Representative::String,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use polars::prelude::{DataType, UnknownKind};

    use super::*;

    #[test]
    fn a_key_column_of_every_dtype_a_key_may_have_is_made_of_nulls() {
        // The dtypes of a join's keys are what Polars gives them, and a list of text is one:
        // making the column of nulls of such a dtype must not end the process.
        let keys = [
            DataType::List(Box::new(DataType::String)),
            DataType::List(Box::new(DataType::Boolean)),
            DataType::List(Box::new(DataType::Float32)),
            DataType::List(Box::new(DataType::Null)),
            DataType::List(Box::new(DataType::Unknown(UnknownKind::Any))),
            DataType::List(Box::new(DataType::List(Box::new(DataType::String)))),
            DataType::List(Box::new(DataType::Int64)),
            DataType::Array(Box::new(DataType::Int64), 2),
            DataType::String,
            DataType::Null,
        ];
        let frames = Frames::new(Representative::Int64, Representative::Int64, &keys);
        for (at, key) in keys.iter().enumerate() {
            let column = frames.rows.column(&key_column(at)).expect("a key column");
            // A dtype that Polars has not worked out is a list of nothing, of nulls.
            let made = match key {
                DataType::List(inner) if matches!(**inner, DataType::Unknown(_)) => {
                    DataType::List(Box::new(DataType::Null))
                }
                other => other.clone(),
            };
            assert_eq!(column.dtype(), &made);
            assert_eq!(column.null_count(), 3);
        }
    }
}
