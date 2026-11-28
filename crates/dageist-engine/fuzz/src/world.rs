//! The one table every grid runs on: a column of every class the tables know and of every dtype
//! they have no class of, three rows each, the middle one a null, with a few columns to group, key
//! and test by, and the table a join takes, which holds the same under other names.
//!
//! A class or dtype that no column can be made of is left out of the axis of columns, with why.

use std::sync::LazyLock;

use dageist_engine::allowed::{Inner, Representative};
use dageist_fuzz::grid::{Choices, LeftOut};
use polars::prelude::{
    Categories, Column as FrameColumn, DataFrame, DataType, FrozenCategories, IntoSeries,
    ListChunked, NamedFrom, Series, StructChunked, TimeUnit, TimeZone,
};

/// A column of the world.
#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    /// What the column is called in a label: the class's name, or the dtype's.
    pub label: String,
    /// Its name in the table.
    pub name: String,
    pub dtype: DataType,
    /// The class the tables read it as, where they have one.
    pub class: Option<Representative>,
}

impl Column {
    /// The name of the column of the same dtype in the table a join takes.
    #[must_use]
    pub fn other_name(&self) -> String {
        format!("r{}", self.name)
    }

    /// How deep the dtype nests: 0 for a scalar, 1 for a list or an array of one, 2 for a list of
    /// lists. A struct counts as a scalar.
    #[must_use]
    pub fn depth(&self) -> usize {
        fn depth(dtype: &DataType) -> usize {
            match dtype {
                DataType::List(inner) | DataType::Array(inner, _) => 1 + depth(inner),
                _ => 0,
            }
        }
        depth(&self.dtype)
    }
}

/// The name of the column `label`: `c_` and the label, with what a name cannot hold made `_`.
#[must_use]
pub fn name_of(label: &str) -> String {
    format!(
        "c_{}",
        label
            .replace(['(', ')', ',', '\'', ' '], "_")
            .replace('μ', "u")
    )
}

/// Three values of the dtype of `class`, named `name`: one, a null and another, where they can be
/// made of whole numbers. A list or an array holds `[1, 3]`, a null and `[3]` (an array `[3, 1]`),
/// and a struct `{x: 1, y: "a"}`, a null and `{x: 3, y: "c"}`.
#[must_use]
pub fn values_of(class: Representative, name: &str) -> Option<Series> {
    if let Some(inner) = class.element() {
        let is_array = matches!(class, Representative::Array(_));
        let rows: [Option<Vec<i64>>; 3] = [
            Some(vec![1, 3]),
            None,
            Some(if is_array { vec![3, 1] } else { vec![3] }),
        ];
        let lists = if inner == Inner::Null {
            ListChunked::from_iter(
                rows.map(|row| row.map(|row| Series::new_null("".into(), row.len()))),
            )
        } else {
            ListChunked::from_iter(rows.map(|row| row.map(|row| Series::new("".into(), row))))
        };
        return lists
            .into_series()
            .with_name(name.into())
            .cast(&class.dtype())
            .ok();
    }
    match class {
        Representative::Struct => {
            let fields = [
                Series::new("x".into(), [Some(1_i64), None, Some(3)]),
                Series::new("y".into(), [Some("a"), None, Some("c")]),
            ];
            StructChunked::from_series(name.into(), 3, fields.iter())
                .ok()
                .map(IntoSeries::into_series)
        }
        Representative::Null => Some(Series::new_null(name.into(), 3)),
        _ => Series::new(name.into(), [Some(1_i64), None, Some(3)])
            .cast(&class.dtype())
            .ok(),
    }
}

/// The dtypes the tables have no class of, or have one class of for several units and zones: a
/// moment and a length of time in every unit, with and without a zone, a decimal of the widest
/// precision, a binary, a categorical and an enum. The walk reads each as the class that stands
/// for it, or does not judge it.
#[must_use]
pub fn dtypes_without_a_class() -> Vec<(String, DataType)> {
    let mut dtypes = Vec::new();
    for (unit, word) in [
        (TimeUnit::Nanoseconds, "ns"),
        (TimeUnit::Microseconds, "us"),
        (TimeUnit::Milliseconds, "ms"),
    ] {
        for zone in [None, Some(TimeZone::UTC)] {
            let dtype = DataType::Datetime(unit, zone.clone());
            if Representative::of(&dtype).is_some_and(|class| class.dtype() == dtype) {
                continue;
            }
            let label = zone.map_or_else(
                || format!("Datetime({word})"),
                |_| format!("Datetime({word}, UTC)"),
            );
            dtypes.push((label, dtype));
        }
        let duration = DataType::Duration(unit);
        if Representative::of(&duration).is_none_or(|class| class.dtype() != duration) {
            dtypes.push((format!("Duration({word})"), duration));
        }
    }
    dtypes.push(("Decimal(38, 10)".to_owned(), DataType::Decimal(38, 10)));
    dtypes.push(("Binary".to_owned(), DataType::Binary));
    if let Ok(enumeration) = FrozenCategories::new(["1", "3"]) {
        dtypes.push((
            "Enum".to_owned(),
            DataType::from_frozen_categories(enumeration),
        ));
    }
    dtypes.push((
        "Categorical".to_owned(),
        DataType::from_categories(Categories::global()),
    ));
    dtypes
}

/// The columns besides those of the classes, which group, key and test: `k`, text with a repeat;
/// `n`, whole numbers with a repeat; `m`, whole numbers all different; `f`, flags with a null.
const HELPERS: [&str; 4] = ["k", "n", "m", "f"];

fn helpers(prefix: &str) -> Vec<Series> {
    vec![
        Series::new(format!("{prefix}k").as_str().into(), ["a", "b", "a"]),
        Series::new(format!("{prefix}n").as_str().into(), [1_i64, 1, 2]),
        Series::new(format!("{prefix}m").as_str().into(), [3_i64, 4, 5]),
        Series::new(
            format!("{prefix}f").as_str().into(),
            [Some(true), Some(false), None],
        ),
    ]
}

/// The world: the tables and the columns of the axis.
pub struct World {
    frame: DataFrame,
    other: DataFrame,
    columns: Vec<Column>,
    left_out: Vec<LeftOut>,
}

static WORLD: LazyLock<World> = LazyLock::new(World::make);

impl World {
    /// The world, made once for the process.
    #[must_use]
    pub fn get() -> &'static Self {
        &WORLD
    }

    fn make() -> Self {
        let mut series: Vec<Series> = Vec::new();
        let mut columns: Vec<Column> = Vec::new();
        let mut left_out: Vec<LeftOut> = Vec::new();
        for class in Representative::ALL {
            let label = class.name().to_owned();
            let name = name_of(&label);
            match values_of(class, &name) {
                Some(values) if values.dtype() == &class.dtype() => {
                    columns.push(Column {
                        label,
                        name,
                        dtype: values.dtype().clone(),
                        class: Some(class),
                    });
                    series.push(values);
                }
                Some(values) => left_out.push(LeftOut {
                    label,
                    reason: format!(
                        "its values made of whole numbers are {}, not {}",
                        values.dtype(),
                        class.dtype()
                    ),
                }),
                None => left_out.push(LeftOut {
                    label,
                    reason: "no column of it can be made of whole numbers".to_owned(),
                }),
            }
        }
        for (label, dtype) in dtypes_without_a_class() {
            let name = name_of(&label);
            let base = if matches!(
                dtype,
                DataType::Binary | DataType::Enum(..) | DataType::Categorical(..)
            ) {
                Series::new(name.as_str().into(), [Some("1"), None, Some("3")])
            } else {
                Series::new(name.as_str().into(), [Some(1_i64), None, Some(3)])
            };
            match base.cast(&dtype) {
                Ok(values) => {
                    columns.push(Column {
                        label,
                        name,
                        class: Representative::of(values.dtype()),
                        dtype: values.dtype().clone(),
                    });
                    series.push(values);
                }
                Err(error) => left_out.push(LeftOut {
                    label,
                    reason: format!("no column of it can be made: {error}"),
                }),
            }
        }
        let empty = ListChunked::from_iter([
            Some(Series::new("".into(), Vec::<i64>::new())),
            None,
            Some(Series::new("".into(), Vec::<i64>::new())),
        ])
        .into_series()
        .with_name("c_empty_lists".into());
        columns.push(Column {
            label: "empty lists".to_owned(),
            name: "c_empty_lists".to_owned(),
            class: Representative::of(empty.dtype()),
            dtype: empty.dtype().clone(),
        });
        series.push(empty);
        let others: Vec<Series> = series
            .iter()
            .map(|column| {
                column
                    .clone()
                    .with_name(format!("r{}", column.name()).as_str().into())
            })
            .chain(helpers("r"))
            .collect();
        series.extend(helpers(""));
        let frame = DataFrame::new(3, series.into_iter().map(FrameColumn::from).collect())
            .expect("columns of three rows");
        let other = DataFrame::new(3, others.into_iter().map(FrameColumn::from).collect())
            .expect("columns of three rows");
        Self {
            frame,
            other,
            columns,
            left_out,
        }
    }

    /// The table, with every column.
    #[must_use]
    pub const fn frame(&self) -> &DataFrame {
        &self.frame
    }

    /// The table a join takes, which holds every column of the world under the name
    /// [`Column::other_name`] gives it, and the helpers as `rk`, `rn`, `rm` and `rf`.
    #[must_use]
    pub const fn other(&self) -> &DataFrame {
        &self.other
    }

    /// The table of the helpers and the columns `labels`, which a grid that asks of one column at
    /// a time runs on, so that Polars is not given what a cell does not read.
    ///
    /// # Panics
    ///
    /// When a label is no column of the world.
    #[must_use]
    pub fn of(&self, labels: &[&str]) -> DataFrame {
        let names: Vec<String> = labels
            .iter()
            .map(|label| self.column(label).name.clone())
            .chain(HELPERS.iter().map(|name| (*name).to_owned()))
            .collect();
        self.frame
            .select(names)
            .expect("the world holds every column it names")
    }

    /// The column labelled `label`.
    ///
    /// # Panics
    ///
    /// When there is none.
    #[must_use]
    pub fn column(&self, label: &str) -> &Column {
        self.columns
            .iter()
            .find(|column| column.label == label)
            .unwrap_or_else(|| panic!("the world has no column {label}"))
    }

    /// Every column, as an axis named `name`, with what no column could be made of left out.
    #[must_use]
    pub fn columns(&self, name: &'static str) -> Choices<Column> {
        let mut axis = Choices::new(name);
        for column in &self.columns {
            axis = axis.with(column.label.clone(), column.clone());
        }
        for left in &self.left_out {
            axis = axis.leave_out(left.label.clone(), left.reason.clone());
        }
        axis
    }

    /// A list of the values of `class` connected as an input, as the run builds it, named
    /// `list_of_` and the name of its column: none for a struct, which no list is connected of,
    /// or a class whose values cannot be made.
    #[must_use]
    pub fn list_of(&self, class: Representative) -> Option<(String, Series)> {
        if class == Representative::Struct {
            return None;
        }
        let values = values_of(class, "list")?;
        (values.dtype() == &class.dtype())
            .then(|| (format!("list_of_{}", name_of(class.name())), values))
    }

    /// One column of each class, as an axis named `name`. The dtypes without a class of their own,
    /// the column of empty lists and the classes no column could be made of are left out.
    #[must_use]
    pub fn classes(&self, name: &'static str) -> Choices<Column> {
        self.columns(name)
            .keep_unless(|label, column| match column.class {
                Some(class) if class.name() == label && class.dtype() == column.dtype => None,
                Some(class) => Some(format!(
                    "the tables read it as {class}, which has its own column"
                )),
                None => Some("the tables have no class of the dtype".to_owned()),
            })
    }
}
