//! The node the corpus runs on: one table with a column of every class of dtype the tables
//! cover, a second table of the same, and a connected value and list.
//!
//! The claim-timing sample has five columns of three dtypes, so a chain from it reaches few of the
//! places the engine allows an entry at. A column of every class puts every method at the places
//! the tables have lines for.

use std::collections::BTreeSet;

use dageist_engine::allowed::Representative;
use dageist_engine::document::{Call, Document, Inputs, ListOrigin, Start, Value};
use polars::prelude::{AnyValue, Column, DataFrame, DataType, IntoLazy, NamedFrom, Scalar, Series};

use crate::report::RunView;

/// The name of the table a join brings in.
pub(super) const OTHER: &str = "other";

/// The name of the table that gives a pivot its columns.
pub(super) const ON_COLUMNS: &str = "pivot_columns";

/// The name of the connected value.
pub(super) const VALUE: &str = "limit";

/// The name of the connected list.
pub(super) const LIST: &str = "names";

/// How many rows the tables hold. Every cell is null: the report is about the schema and what
/// running counts.
const ROWS: usize = 3;

/// The words a dtype's name is written with in a column's name: its letters and digits, lower
/// case, and one underscore between them.
fn slug(name: &str) -> String {
    let mut slug = String::new();
    for letter in name.chars() {
        if letter.is_ascii_alphanumeric() {
            slug.push(letter.to_ascii_lowercase());
        } else if !slug.ends_with('_') {
            slug.push('_');
        }
    }
    slug.trim_matches('_').to_owned()
}

/// The name of the column of the table the steps start from that has the class `class`.
pub(super) fn column_of(class: Representative) -> String {
    slug(class.name())
}

/// The name of the column of the other table that has the class `class`.
pub(super) fn right_column_of(class: Representative) -> String {
    format!("r_{}", slug(class.name()))
}

/// A table with a column of each class of dtype in `classes`, named by `name`.
fn table(classes: &BTreeSet<Representative>, name: fn(Representative) -> String) -> DataFrame {
    let columns: Vec<Column> = Representative::ALL
        .into_iter()
        .filter(|class| classes.contains(class))
        .map(|class| Column::full_null(name(class).into(), ROWS, &class.dtype()))
        .collect();
    DataFrame::new(ROWS, columns).expect("columns of one length")
}

/// The classes of the columns `document` names, of the table the steps start from and of the other
/// table, which it names by the words of [`column_of`] and [`right_column_of`].
fn classes_named(document: &Document) -> (BTreeSet<Representative>, BTreeSet<Representative>) {
    fn in_call(call: &Call, names: &mut BTreeSet<String>) {
        for given in call.arguments() {
            in_value(given.value(), names);
        }
    }
    fn in_value(value: &Value, names: &mut BTreeSet<String>) {
        match value {
            Value::Columns(columns) => names.extend(columns.iter().cloned()),
            Value::Many(items) => items.iter().for_each(|item| in_value(item, names)),
            Value::Pairs(pairs) => {
                for (key, entry) in pairs {
                    in_value(key, names);
                    in_value(entry, names);
                }
            }
            Value::Expression(expression) => {
                if let Start::Call(start) = expression.start() {
                    in_call(start, names);
                }
                expression
                    .links()
                    .iter()
                    .for_each(|link| in_call(link, names));
            }
            _ => {}
        }
    }
    let mut names = BTreeSet::new();
    for step in document.steps() {
        in_call(step.call(), &mut names);
    }
    let named = |name: fn(Representative) -> String| -> BTreeSet<Representative> {
        Representative::ALL
            .into_iter()
            .filter(|class| names.contains(&name(*class)))
            .collect()
    };
    (named(column_of), named(right_column_of))
}

/// The tables and values connected to the node, with the columns of the other table that the
/// document reads.
pub(super) fn inputs(other: &BTreeSet<Representative>) -> Inputs {
    let mut inputs = Inputs::new();
    inputs
        .add_table(OTHER, "Other", table(other, right_column_of).lazy())
        .expect("a name once");
    let on_columns = DataFrame::new(
        1,
        vec![Column::new(column_of(Representative::String).into(), ["a"])],
    )
    .expect("one column");
    inputs
        .add_table(ON_COLUMNS, "Pivot columns", on_columns.lazy())
        .expect("a name once");
    inputs
        .add_value(VALUE, Scalar::new(DataType::Int64, AnyValue::Int64(3)))
        .expect("a name once");
    inputs
        .add_list(
            LIST,
            ListOrigin::Parameter,
            Series::new("names".into(), ["a", "b"]),
        )
        .expect("a name once");
    inputs
}

/// What the engine says of `document` without running the rows: the walk of every step, and
/// whether Polars accepts it.
pub(super) fn report(document: &Document) -> dageist_engine::document::Report {
    let (left, right) = classes_named(document);
    document
        .run(&inputs(&right), table(&left, column_of).lazy())
        .expect("the schema of the corpus table is worked out")
}

/// What running `document` reports, on a table of the columns it reads. A report holds the
/// columns of the table and each step's, so a table of the whole set of classes would make every
/// report as long as the set.
pub(super) fn run(document: &Document) -> RunView {
    let (left, right) = classes_named(document);
    let report = document
        .run_on(&inputs(&right), &table(&left, column_of))
        .expect("the schema of the corpus table is worked out");
    RunView::of_report(&report)
}
