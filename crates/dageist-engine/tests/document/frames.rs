//! What the steps that keep, drop, reorder or number the rows and columns of a table do, on a table
//! of four rows: `a` holds `1, 2, null, 4`, `f` holds `1.0, NaN, 3.0, null` and `s` holds `x, y, z,
//! w`. The expected cells are what Python Polars 2.2.0 gives for the same call on the same rows,
//! worked out by hand and checked against `python -I` with that release, not read from a run of
//! the engine.

use dageist_engine::allowed::{RefusalSource, Stage};
use dageist_engine::document::{Document, Inputs, Outcome, Report, RowsError, StepId, StepRows};
use polars::prelude::{DataFrame, DataType, NamedFrom, Series};
use serde_json::{Value, json};

use crate::support::{document, many, step};

fn table() -> DataFrame {
    DataFrame::new(
        4,
        vec![
            Series::new("a".into(), [Some(1_i64), Some(2), None, Some(4)]).into(),
            Series::new("f".into(), [Some(1.0_f64), Some(f64::NAN), Some(3.0), None]).into(),
            Series::new("s".into(), ["x", "y", "z", "w"]).into(),
        ],
    )
    .expect("columns of four rows")
}

fn whole(number: i64) -> Value {
    json!({ "integer": number })
}

fn columns(names: &[&str]) -> Value {
    json!({ "columns": names })
}

/// The document of the one step `entry` given `arguments`.
fn one(entry: &str, arguments: Value) -> String {
    document(vec![step("s", entry, arguments)])
}

fn report(text: &str) -> Report {
    Document::read(text)
        .expect("a document")
        .run_on(&Inputs::new(), &table())
        .expect("the table has a schema")
}

/// The rows of the table the step `s` gives, as text, or the reason it gives none.
fn rows_through(text: &str, through: &str) -> Result<Vec<Vec<Option<String>>>, RowsError> {
    Document::read(text)
        .expect("a document")
        .preview(
            &Inputs::new(),
            &table(),
            &StepId::new(through).expect("an id"),
            100,
        )
        .map(|preview| preview.rows().to_vec())
}

fn rows(text: &str) -> Vec<Vec<Option<String>>> {
    rows_through(text, "s").unwrap_or_else(|error| panic!("{error}"))
}

fn cells(rows: &[&[Option<&str>]]) -> Vec<Vec<Option<String>>> {
    rows.iter()
        .map(|row| {
            row.iter()
                .map(|cell| cell.map(str::to_owned))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The columns the step gives and their dtypes.
fn schema(text: &str) -> Vec<(String, DataType)> {
    match report(text).steps()[0].outcome() {
        Outcome::Valid { schema, .. } => schema
            .iter()
            .map(|(name, dtype)| (name.to_string(), dtype.clone()))
            .collect(),
        other => panic!("{other:?}"),
    }
}

fn names(text: &str) -> Vec<String> {
    schema(text).into_iter().map(|(name, _)| name).collect()
}

/// Whether the step gives no table: Polars or the catalogue refuses it, or Polars cannot collect
/// its rows.
fn gives_no_table(text: &str) -> bool {
    let reported = report(text);
    let step = &reported.steps()[0];
    !matches!(step.outcome(), Outcome::Valid { .. }) || matches!(step.rows(), StepRows::Failed(_))
}

fn kept_of(text: &str) -> (usize, usize) {
    match report(text).steps()[0].rows() {
        StepRows::Counted(count) => (count.received(), count.kept()),
        other => panic!("{other:?}"),
    }
}

const A_F_S: [&str; 3] = ["a", "f", "s"];

fn all_rows() -> Vec<Vec<Option<String>>> {
    cells(&[
        &[Some("1"), Some("1.0"), Some("x")],
        &[Some("2"), Some("NaN"), Some("y")],
        &[None, Some("3.0"), Some("z")],
        &[Some("4"), None, Some("w")],
    ])
}

#[test]
fn head_keeps_the_first_rows_and_five_when_it_is_not_told() {
    let two = one("LazyFrame.head", json!({ "n": whole(2) }));
    assert_eq!(rows(&two), all_rows()[..2].to_vec());
    assert_eq!(names(&two), A_F_S);
    assert_eq!(kept_of(&two), (4, 2));
    // Five is more than the table has, so every row is kept, as Polars keeps them.
    assert_eq!(rows(&one("LazyFrame.head", json!({}))), all_rows());
    assert!(rows(&one("LazyFrame.head", json!({ "n": whole(0) }))).is_empty());
    // The schema of a table with no rows is the table's.
    assert_eq!(
        names(&one("LazyFrame.head", json!({ "n": whole(0) }))),
        A_F_S
    );
}

#[test]
fn limit_is_head_under_another_name() {
    let two = one("LazyFrame.limit", json!({ "n": whole(2) }));
    assert_eq!(rows(&two), all_rows()[..2].to_vec());
    assert_eq!(rows(&one("LazyFrame.limit", json!({}))), all_rows());
}

#[test]
fn head_and_limit_refuse_a_negative_count_as_python_does() {
    // Python: `ValueError: negative slice lengths (-1) are invalid for LazyFrame`.
    for entry in ["LazyFrame.head", "LazyFrame.limit"] {
        assert!(
            gives_no_table(&one(entry, json!({ "n": whole(-1) }))),
            "{entry}"
        );
        // Python takes the count as the length of a slice, a whole number of the platform, so a
        // count above 32 bits keeps every row; only `tail` takes an index of 32 bits.
        assert_eq!(
            rows(&one(entry, json!({ "n": whole(4_294_967_296) }))),
            all_rows(),
            "{entry}"
        );
    }
}

#[test]
fn tail_keeps_the_last_rows() {
    let three = one("LazyFrame.tail", json!({ "n": whole(3) }));
    assert_eq!(rows(&three), all_rows()[1..].to_vec());
    assert_eq!(kept_of(&three), (4, 3));
    assert_eq!(rows(&one("LazyFrame.tail", json!({}))), all_rows());
    assert!(rows(&one("LazyFrame.tail", json!({ "n": whole(0) }))).is_empty());
    // Python: `OverflowError: number too small to fit in target type`.
    assert!(gives_no_table(&one(
        "LazyFrame.tail",
        json!({ "n": whole(-1) })
    )));
}

#[test]
fn slice_starts_at_an_offset_that_counts_from_the_end_when_it_is_negative() {
    let slice = |offset: i64, length: Option<i64>| {
        let mut arguments = json!({ "offset": whole(offset) });
        if let Some(length) = length {
            arguments["length"] = whole(length);
        }
        one("LazyFrame.slice", arguments)
    };
    assert_eq!(rows(&slice(1, Some(2))), all_rows()[1..3].to_vec());
    assert_eq!(rows(&slice(1, None)), all_rows()[1..].to_vec());
    assert_eq!(rows(&slice(-1, None)), all_rows()[3..].to_vec());
    assert_eq!(rows(&slice(-3, Some(2))), all_rows()[1..3].to_vec());
    assert!(rows(&slice(9, None)).is_empty());
    assert!(rows(&slice(-9, Some(2))).is_empty());
    assert!(rows(&slice(0, Some(0))).is_empty());
    assert_eq!(kept_of(&slice(1, Some(2))), (4, 2));
    // Python: `ValueError: negative slice lengths (-1) are invalid for LazyFrame`.
    assert!(gives_no_table(&slice(0, Some(-1))));
    // The offset has no default, and Python's binding takes it as a signed 64-bit number.
    assert!(gives_no_table(&one("LazyFrame.slice", json!({}))));
}

#[test]
fn reverse_gives_the_last_row_first() {
    let reversed = one("LazyFrame.reverse", json!({}));
    let mut expected = all_rows();
    expected.reverse();
    assert_eq!(rows(&reversed), expected);
    assert_eq!(names(&reversed), A_F_S);
    assert_eq!(kept_of(&reversed), (4, 4));
}

#[test]
fn drop_removes_the_columns_it_names_and_refuses_one_the_table_lacks_only_when_strict() {
    let named = |names: &[&str]| json!({ "columns": many(names.iter().map(|name| columns(&[name])).collect()) });
    assert_eq!(names(&one("LazyFrame.drop", named(&["a"]))), ["f", "s"]);
    assert_eq!(names(&one("LazyFrame.drop", named(&["a", "s"]))), ["f"]);
    assert_eq!(
        rows(&one("LazyFrame.drop", named(&["a", "s"]))),
        cells(&[&[Some("1.0")], &[Some("NaN")], &[Some("3.0")], &[None]])
    );
    // Python: `ColumnNotFoundError: "zz" not found`.
    assert!(gives_no_table(&one("LazyFrame.drop", named(&["zz"]))));
    let lenient = one(
        "LazyFrame.drop",
        json!({ "columns": many(vec![columns(&["zz"]), columns(&["a"])]), "strict": { "flag": false } }),
    );
    assert_eq!(names(&lenient), ["f", "s"]);
    // Named twice, a column is dropped once; named by none, the table stays as it was.
    assert_eq!(
        names(&one("LazyFrame.drop", named(&["a", "a"]))),
        ["f", "s"]
    );
    assert_eq!(names(&one("LazyFrame.drop", json!({}))), A_F_S);
}

#[test]
fn drop_nulls_drops_the_rows_that_hold_a_null_in_the_columns_it_looks_at() {
    // Every column: `a` is null in the third row and `f` in the fourth.
    assert_eq!(
        rows(&one("LazyFrame.drop_nulls", json!({}))),
        all_rows()[..2].to_vec()
    );
    assert_eq!(
        rows(&one(
            "LazyFrame.drop_nulls",
            json!({ "subset": columns(&["f"]) })
        )),
        all_rows()[..3].to_vec()
    );
    assert_eq!(
        rows(&one(
            "LazyFrame.drop_nulls",
            json!({ "subset": columns(&["a", "f"]) })
        )),
        all_rows()[..2].to_vec()
    );
    assert_eq!(
        kept_of(&one(
            "LazyFrame.drop_nulls",
            json!({ "subset": columns(&["f"]) })
        )),
        (4, 3)
    );
    // Python: `ColumnNotFoundError: "zz" not found`.
    assert!(gives_no_table(&one(
        "LazyFrame.drop_nulls",
        json!({ "subset": columns(&["zz"]) })
    )));
}

#[test]
fn drop_nans_drops_the_rows_that_hold_a_nan_and_only_looks_at_columns_that_can() {
    let without_nan = [&all_rows()[0], &all_rows()[2], &all_rows()[3]]
        .map(Clone::clone)
        .to_vec();
    // Without a subset, the floating-point columns: `s` is text and is not looked at.
    assert_eq!(rows(&one("LazyFrame.drop_nans", json!({}))), without_nan);
    assert_eq!(
        rows(&one(
            "LazyFrame.drop_nans",
            json!({ "subset": columns(&["f"]) })
        )),
        without_nan
    );
    // A whole number holds no NaN, so no row is dropped for it.
    assert_eq!(
        rows(&one(
            "LazyFrame.drop_nans",
            json!({ "subset": columns(&["a"]) })
        )),
        all_rows()
    );
    // Python: `InvalidOperationError: is_nan operation not supported for dtype str`.
    assert!(gives_no_table(&one(
        "LazyFrame.drop_nans",
        json!({ "subset": columns(&["s"]) })
    )));
}

#[test]
fn fill_nan_puts_its_value_where_a_floating_point_column_holds_a_nan() {
    let filled = |value: Value| one("LazyFrame.fill_nan", json!({ "value": value }));
    let with = |cell: Option<&str>| {
        let mut expected = all_rows();
        expected[1][1] = cell.map(str::to_owned);
        expected
    };
    assert_eq!(rows(&filled(whole(0))), with(Some("0.0")));
    assert_eq!(rows(&filled(json!({ "float": 0.5 }))), with(Some("0.5")));
    // A null fills it with a null, and the dtype stays.
    assert_eq!(rows(&filled(json!({ "flag": null }))), with(None));
    // Only the floating-point column is looked at: the whole numbers and the text stay.
    assert_eq!(
        schema(&filled(whole(0))),
        [
            ("a".to_owned(), DataType::Int64),
            ("f".to_owned(), DataType::Float64),
            ("s".to_owned(), DataType::String)
        ]
    );
    // Text makes the column text, and the numbers in it are written as text.
    let text = filled(json!({ "string": "x" }));
    assert_eq!(schema(&text)[1], ("f".to_owned(), DataType::String));
    assert_eq!(
        rows(&text),
        cells(&[
            &[Some("1"), Some("1.0"), Some("x")],
            &[Some("2"), Some("x"), Some("y")],
            &[None, Some("3.0"), Some("z")],
            &[Some("4"), None, Some("w")],
        ])
    );
    // The value has no default.
    assert!(gives_no_table(&one("LazyFrame.fill_nan", json!({}))));
}

#[test]
fn with_row_index_puts_a_count_of_the_rows_first() {
    let counted = one("LazyFrame.with_row_index", json!({}));
    assert_eq!(schema(&counted)[0], ("index".to_owned(), DataType::UInt32));
    assert_eq!(names(&counted), ["index", "a", "f", "s"]);
    assert_eq!(
        rows(&counted),
        cells(&[
            &[Some("0"), Some("1"), Some("1.0"), Some("x")],
            &[Some("1"), Some("2"), Some("NaN"), Some("y")],
            &[Some("2"), None, Some("3.0"), Some("z")],
            &[Some("3"), Some("4"), None, Some("w")],
        ])
    );
    let named = one(
        "LazyFrame.with_row_index",
        json!({ "name": { "string": "i" }, "offset": whole(5) }),
    );
    assert_eq!(names(&named), ["i", "a", "f", "s"]);
    assert_eq!(
        rows(&named)
            .iter()
            .map(|row| row[0].clone())
            .collect::<Vec<_>>(),
        ["5", "6", "7", "8"].map(|cell| Some(cell.to_owned()))
    );
    // Python: `DuplicateError: duplicate column name a`.
    assert!(gives_no_table(&one(
        "LazyFrame.with_row_index",
        json!({ "name": { "string": "a" } })
    )));
    // Python: `ValueError: offset input for with_row_index cannot be negative`.
    assert!(gives_no_table(&one(
        "LazyFrame.with_row_index",
        json!({ "offset": whole(-1) })
    )));
}

#[test]
fn the_step_after_one_that_changes_the_rows_is_given_the_rows_it_kept() {
    let text = document(vec![
        step("s", "LazyFrame.tail", json!({ "n": whole(3) })),
        step("t", "LazyFrame.head", json!({ "n": whole(2) })),
        step("u", "LazyFrame.reverse", json!({})),
    ]);
    let mut expected = all_rows()[1..3].to_vec();
    assert_eq!(rows_through(&text, "t").expect("rows"), expected);
    expected.reverse();
    assert_eq!(rows_through(&text, "u").expect("rows"), expected);
    let reported = Document::read(&text)
        .expect("a document")
        .run_on(&Inputs::new(), &table())
        .expect("a schema");
    let counted: Vec<(usize, usize)> = reported
        .steps()
        .iter()
        .map(|step| match step.rows() {
            StepRows::Counted(count) => (count.received(), count.kept()),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(counted, [(4, 3), (3, 2), (2, 2)]);
}

/// What the tables say of the one step of `text` on the table, before any row is read.
fn walked(text: &str) -> dageist_engine::document::Walk {
    let document = Document::read(text).expect("a document");
    document.steps()[0].walk(0, table().schema(), &Inputs::new())
}

#[test]
fn the_walk_refuses_a_column_drop_nans_looks_at_where_a_test_for_a_nan_is_refused() {
    let looking_at = |names: &[&str]| {
        walked(&one(
            "LazyFrame.drop_nans",
            json!({ "subset": columns(names) }),
        ))
    };
    // `is_nan` of text is refused when the rows run, which the tables say of it as Python Polars
    // does of the step: `is_nan operation not supported for dtype str`.
    let walk = looking_at(&["a", "s"]);
    let refusal = walk.refusal().expect("a text column is refused");
    assert_eq!(refusal.place().to_string(), "step \"s\", subset");
    assert_eq!(
        refusal.refusal().source(),
        RefusalSource::Polars(Stage::Rows)
    );
    assert_eq!(
        refusal.refusal().reason(),
        "`is_nan` operation not supported for dtype `str`"
    );
    // A whole number and a float hold no NaN, or may, and are accepted, as is a table of no name.
    assert!(looking_at(&["a", "f"]).refusal().is_none());
    assert!(
        walked(&one("LazyFrame.drop_nans", json!({})))
            .refusal()
            .is_none()
    );
    // The other steps that look at columns refuse none by their dtype.
    assert!(
        walked(&one(
            "LazyFrame.drop_nulls",
            json!({ "subset": columns(&["s"]) })
        ))
        .refusal()
        .is_none()
    );
}

#[test]
fn first_and_last_keep_one_row() {
    assert_eq!(
        rows(&one("LazyFrame.first", json!({}))),
        all_rows()[..1].to_vec()
    );
    assert_eq!(
        rows(&one("LazyFrame.last", json!({}))),
        all_rows()[3..].to_vec()
    );
    assert_eq!(kept_of(&one("LazyFrame.first", json!({}))), (4, 1));
    assert_eq!(names(&one("LazyFrame.last", json!({}))), A_F_S);
    // A table with no rows has none to keep.
    for entry in ["LazyFrame.first", "LazyFrame.last"] {
        let text = document(vec![
            step("t", "LazyFrame.head", json!({ "n": whole(0) })),
            step("s", entry, json!({})),
        ]);
        assert!(
            rows_through(&text, "s").expect("rows").is_empty(),
            "{entry}"
        );
    }
}

#[test]
fn gather_every_keeps_each_nth_row_from_an_offset() {
    let every = |n: i64, offset: Option<i64>| {
        let mut arguments = json!({ "n": whole(n) });
        if let Some(offset) = offset {
            arguments["offset"] = whole(offset);
        }
        one("LazyFrame.gather_every", arguments)
    };
    assert_eq!(
        rows(&every(2, None)),
        [all_rows()[0].clone(), all_rows()[2].clone()]
    );
    assert_eq!(
        rows(&every(2, Some(1))),
        [all_rows()[1].clone(), all_rows()[3].clone()]
    );
    assert_eq!(rows(&every(9, None)), all_rows()[..1].to_vec());
    assert!(rows(&every(2, Some(9))).is_empty());
    assert_eq!(kept_of(&every(2, None)), (4, 2));
    assert_eq!(names(&every(2, None)), A_F_S);
    // Python: `gather_every(n): n should be positive`.
    assert!(gives_no_table(&every(0, None)));
    // Python's binding takes both as unsigned numbers: `OverflowError`.
    assert!(gives_no_table(&every(-1, None)));
    assert!(gives_no_table(&every(2, Some(-1))));
    // `n` has no default.
    assert!(gives_no_table(&one("LazyFrame.gather_every", json!({}))));
}

#[test]
fn gather_every_of_a_table_without_columns_keeps_the_height_it_gathers() {
    // Python keeps the height of a table that has no column, which `select` of every column does
    // not, by selecting beside a column of its own that it drops: `(2, 0)` for four rows.
    let text = document(vec![
        step(
            "t",
            "LazyFrame.drop",
            json!({ "columns": many(vec![columns(&["a", "f", "s"])]) }),
        ),
        step("s", "LazyFrame.gather_every", json!({ "n": whole(2) })),
    ]);
    assert_eq!(rows_through(&text, "s").expect("rows").len(), 2);
}

#[test]
fn reverse_of_a_table_without_columns_keeps_its_height() {
    // Python reverses beside a column of its own that it drops, as it gathers, so a table with no
    // column keeps its rows: `(4, 0)` for four rows, where `select` of every column gives none.
    let text = document(vec![
        step(
            "t",
            "LazyFrame.drop",
            json!({ "columns": many(vec![columns(&["a", "f", "s"])]) }),
        ),
        step("s", "LazyFrame.reverse", json!({})),
    ]);
    assert_eq!(rows_through(&text, "s").expect("rows").len(), 4);
}
