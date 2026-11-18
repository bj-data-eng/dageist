//! A cell of a block reads as the step editor's preview writes the same value:
//! as text, formatted by Polars, a null as no text at all.
//!
//! One table holds a column of every class of dtype a Parquet file can carry.
//! It is stored as Parquet and served in both ways, as a working window and as
//! a stored result. Each cell of the blocks is compared with the preview of the
//! same table, which the step document writes, and the common dtypes with the
//! text worked out by hand from how Polars writes a value.

use std::fs;
use std::io::Cursor;
use std::path::PathBuf;

use dageist_engine::document::{Document, Inputs, StepId};
use dageist_engine::{ParquetTable, Table};
use polars::prelude::*;

/// The directory a test's Parquet file is written in, removed afterwards.
struct Folder {
    path: PathBuf,
}

impl Folder {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("dageist-block-text-{}-{name}", std::process::id()));
        fs::create_dir_all(&path).expect("the folder is created");
        Self { path }
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn utc() -> TimeZone {
    TimeZone::opt_try_new(Some("UTC"))
        .expect("a time zone")
        .expect("a zone")
}

/// Microseconds since the Unix epoch at 09:15:00 on 2026-03-02, which is day
/// 20,514 of the epoch: 56 years of 365 days and the 14 leap days of 1972 to
/// 2024 make 20,454 days to 2026-01-01, and January and February add 59 and
/// 1 more day to reach the second of March.
const MARCH_SECOND_MICROS: i64 = (20_454 + 59 + 1) * 86_400 * 1_000_000;
const NINE_FIFTEEN_MICROS: i64 = (9 * 3_600 + 15 * 60) * 1_000_000;

/// A column of each class of dtype, four rows each, the last one null where the
/// dtype can hold a null.
fn columns() -> Vec<Series> {
    let strings: [Option<&str>; 4] = [Some(""), Some("héllo ✓"), Some("a \"quoted\" b"), None];
    let binaries: [Option<&[u8]>; 4] = [Some(b"ab"), Some(b""), Some(&[0, 255, 16]), None];
    let lists = [
        Some(Series::new("".into(), [1i64, 2])),
        Some(Series::new("".into(), Vec::<i64>::new())),
        Some(Series::new("".into(), [3i64])),
        None,
    ];
    let structs = df![
        "a" => [1i64, 2, 3, 4],
        "b" => ["x", "y", "z", "w"],
    ]
    .expect("a frame")
    .into_struct("structure".into())
    .into_series();
    let letters = Series::new("letters".into(), [Some("a"), Some("b"), Some("a"), None]);
    let pairs = [
        Some(Series::new("".into(), [1i32, 2])),
        Some(Series::new("".into(), [3i32, 4])),
        None,
        Some(Series::new("".into(), [5i32, 6])),
    ];
    let days = Series::new("date".into(), [Some(20_514i32), Some(0), Some(-1), None]);
    vec![
        Series::new("key".into(), [0i32, 1, 2, 3]),
        Series::new("int8".into(), [Some(-8i8), Some(0), Some(127), None]),
        Series::new("int16".into(), [Some(-300i16), Some(0), Some(300), None]),
        Series::new(
            "int32".into(),
            [Some(-70_000i32), Some(0), Some(70_000), None],
        ),
        Series::new(
            "int64".into(),
            [Some(i64::MIN), Some(0), Some(i64::MAX), None],
        ),
        Series::new("uint8".into(), [Some(0u8), Some(7), Some(255), None]),
        Series::new("uint16".into(), [Some(0u16), Some(7), Some(65_535), None]),
        Series::new(
            "uint32".into(),
            [Some(0u32), Some(7), Some(4_294_967_295), None],
        ),
        Series::new("uint64".into(), [Some(0u64), Some(7), Some(u64::MAX), None]),
        Series::new(
            "float32".into(),
            [Some(0.1f32), Some(f32::NAN), Some(f32::INFINITY), None],
        ),
        Series::new(
            "float64".into(),
            [Some(100.0f64), Some(1.25e-7), Some(f64::NEG_INFINITY), None],
        ),
        Series::new(
            "boolean".into(),
            [Some(true), Some(false), Some(true), None],
        ),
        Series::new("string".into(), strings),
        Series::new("binary".into(), binaries),
        days.cast(&DataType::Date).expect("dates"),
        Series::new(
            "datetime".into(),
            [
                Some(MARCH_SECOND_MICROS + NINE_FIFTEEN_MICROS),
                Some(MARCH_SECOND_MICROS + 1_500_000),
                Some(0),
                None,
            ],
        )
        .cast(&DataType::Datetime(TimeUnit::Microseconds, None))
        .expect("datetimes"),
        Series::new(
            "zoned".into(),
            [
                Some(MARCH_SECOND_MICROS / 1_000 + 3_600_000),
                Some(0),
                Some(1),
                None,
            ],
        )
        .cast(&DataType::Datetime(TimeUnit::Milliseconds, Some(utc())))
        .expect("datetimes with a zone"),
        Series::new(
            "time".into(),
            [Some(NINE_FIFTEEN_MICROS * 1_000), Some(0), Some(1), None],
        )
        .cast(&DataType::Time)
        .expect("times"),
        Series::new(
            "duration".into(),
            [
                Some((31 * 3_600 + 25 * 60) * 1_000_000i64),
                Some(0),
                Some(-1_500),
                None,
            ],
        )
        .cast(&DataType::Duration(TimeUnit::Microseconds))
        .expect("durations"),
        Series::new(
            "decimal".into(),
            [Some(1.5f64), Some(-0.25), Some(100.0), None],
        )
        .cast(&DataType::Decimal(10, 2))
        .expect("decimals"),
        Series::new("list".into(), lists),
        Series::new("array".into(), pairs)
            .cast(&DataType::Array(Box::new(DataType::Int32), 2))
            .expect("arrays"),
        structs,
        letters
            .cast(&DataType::from_categories(Categories::global()))
            .expect("categories")
            .with_name("categorical".into()),
        letters
            .cast(&DataType::from_frozen_categories(
                FrozenCategories::new(["a", "b"]).expect("categories"),
            ))
            .expect("an enum")
            .with_name("enumeration".into()),
        Series::new("int128".into(), [1i128, -2, 3, 4])
            .cast(&DataType::Int128)
            .expect("128-bit integers"),
        Series::new("uint128".into(), [Some(1u64), Some(2), Some(3), None])
            .cast(&DataType::UInt128)
            .expect("128-bit integers"),
        Series::new("float16".into(), [Some(1.5f32), Some(2.0), Some(3.0), None])
            .cast(&DataType::Float16)
            .expect("half-precision floats"),
        Series::new_null("nothing".into(), 4),
    ]
}

/// The table of [`columns`] written to Parquet in `folder`.
fn write_table(folder: &Folder) -> DataFrame {
    let mut frame = DataFrame::new_infer_height(columns().into_iter().map(Column::from).collect())
        .expect("a frame");
    let file = fs::File::create(folder.path.join("table.parquet")).expect("a file");
    ParquetWriter::new(file)
        .finish(&mut frame)
        .expect("the table is written");
    frame
}

fn read_ipc(bytes: Vec<u8>) -> DataFrame {
    IpcStreamReader::new(Cursor::new(bytes))
        .finish()
        .expect("the bytes are an Arrow IPC stream")
}

/// The preview of every row of `frame` as the step document writes it, found
/// through a step that selects every column.
fn preview_of(frame: &DataFrame) -> Vec<(String, Vec<Option<String>>)> {
    let fields: Vec<String> = frame
        .get_column_names()
        .iter()
        .map(|name| {
            format!(
                r#"{{"expression":{{"start":{{"entry":"polars.col","arguments":{{"name":{{"columns":["{name}"]}}}}}},"then":[]}}}}"#
            )
        })
        .collect();
    let text = format!(
        r#"{{"version":1,"steps":[{{"id":"all","call":{{"entry":"LazyFrame.select","arguments":{{"exprs":{{"many":[{}]}}}}}}}}]}}"#,
        fields.join(",")
    );
    let preview = Document::read(&text)
        .expect("a document")
        .preview(
            &Inputs::new(),
            frame,
            &StepId::new("all").expect("an id"),
            frame.height(),
        )
        .expect("the preview");
    preview
        .columns()
        .iter()
        .enumerate()
        .map(|(at, column)| {
            (
                column.name().to_owned(),
                preview.rows().iter().map(|row| row[at].clone()).collect(),
            )
        })
        .collect()
}

/// A column of a block read back as text, a null as `None`.
fn cells_of(block: &DataFrame, name: &str) -> Vec<Option<String>> {
    let column = block.column(name).expect("the column is in the block");
    assert_eq!(
        column.dtype(),
        &DataType::String,
        "{name} is written as text"
    );
    (0..block.height())
        .map(|row| match column.get(row).expect("a cell") {
            AnyValue::Null => None,
            AnyValue::String(text) => Some(text.to_owned()),
            AnyValue::StringOwned(text) => Some(text.to_string()),
            other => panic!("{name}: a cell is {other:?}, not text"),
        })
        .collect()
}

fn blocks(folder: &Folder) -> [(&'static str, DataFrame); 2] {
    let window = Table::load_keys(&folder.path, "key", 4).expect("the window loads");
    let stored = ParquetTable::open(&folder.path).expect("the table opens");
    [
        (
            "a working window",
            read_ipc(window.block_ipc(0, 4, &[]).expect("the window's block")),
        ),
        (
            "a stored result",
            read_ipc(stored.block_ipc(0, 4, &[]).expect("the stored block")),
        ),
    ]
}

#[test]
fn a_cell_of_every_dtype_class_reads_as_the_preview_writes_it() {
    let folder = Folder::new("preview");
    write_table(&folder);
    // What the preview works on is what Parquet gives back, so a dtype that
    // Parquet turns into another one is previewed as the other.
    let stored = LazyFrame::scan_parquet(
        PlRefPath::try_from_path(&folder.path.join("table.parquet")).expect("a path"),
        ScanArgsParquet::default(),
    )
    .and_then(LazyFrame::collect)
    .expect("the file is read");
    let preview = preview_of(&stored);

    for (served, block) in blocks(&folder) {
        assert_eq!(block.width(), preview.len(), "{served}");
        for (name, expected) in &preview {
            assert_eq!(&cells_of(&block, name), expected, "{name} in {served}");
        }
    }
}

#[test]
fn common_dtypes_are_written_as_polars_writes_them() {
    let folder = Folder::new("by-hand");
    write_table(&folder);
    // Each expectation is what Polars writes for the value: integers in
    // decimal, a float with the digits it needs, a Boolean as a word, a date
    // as `2026-03-02`, a timestamp as `2026-03-02 09:15:00` with its fraction
    // only where it has one and its zone after it, a time as `09:15:00`, a
    // duration in days, hours and minutes or the smaller unit it needs, a
    // decimal with its scale, a list in brackets and a struct in braces.
    let expected: Vec<(&str, [Option<&str>; 4])> = vec![
        ("int8", [Some("-8"), Some("0"), Some("127"), None]),
        (
            "int64",
            [
                Some("-9223372036854775808"),
                Some("0"),
                Some("9223372036854775807"),
                None,
            ],
        ),
        (
            "uint64",
            [Some("0"), Some("7"), Some("18446744073709551615"), None],
        ),
        ("int128", [Some("1"), Some("-2"), Some("3"), Some("4")]),
        ("uint128", [Some("1"), Some("2"), Some("3"), None]),
        ("float16", [Some("1.5"), Some("2.0"), Some("3.0"), None]),
        ("float32", [Some("0.1"), Some("NaN"), Some("inf"), None]),
        (
            "float64",
            [Some("100.0"), Some("1.2500e-7"), Some("-inf"), None],
        ),
        ("boolean", [Some("true"), Some("false"), Some("true"), None]),
        (
            "string",
            [Some(""), Some("héllo ✓"), Some("a \"quoted\" b"), None],
        ),
        (
            "binary",
            [
                Some("b\"ab\""),
                Some("b\"\""),
                Some("b\"\\x00\\xff\\x10\""),
                None,
            ],
        ),
        (
            "date",
            [
                Some("2026-03-02"),
                Some("1970-01-01"),
                Some("1969-12-31"),
                None,
            ],
        ),
        (
            "datetime",
            [
                Some("2026-03-02 09:15:00"),
                Some("2026-03-02 00:00:01.500"),
                Some("1970-01-01 00:00:00"),
                None,
            ],
        ),
        (
            "zoned",
            [
                Some("2026-03-02 01:00:00 UTC"),
                Some("1970-01-01 00:00:00 UTC"),
                Some("1970-01-01 00:00:00.001 UTC"),
                None,
            ],
        ),
        (
            "time",
            [
                Some("09:15:00"),
                Some("00:00:00"),
                Some("00:00:00.000000001"),
                None,
            ],
        ),
        (
            "duration",
            [Some("1d 7h 25m"), Some("0µs"), Some("-1500µs"), None],
        ),
        (
            "decimal",
            [Some("1.50"), Some("-0.25"), Some("100.00"), None],
        ),
        ("categorical", [Some("a"), Some("b"), Some("a"), None]),
        ("enumeration", [Some("a"), Some("b"), Some("a"), None]),
        ("list", [Some("[1, 2]"), Some("[]"), Some("[3]"), None]),
        (
            "array",
            [Some("[1, 2]"), Some("[3, 4]"), None, Some("[5, 6]")],
        ),
        (
            "structure",
            [
                Some("{1,\"x\"}"),
                Some("{2,\"y\"}"),
                Some("{3,\"z\"}"),
                Some("{4,\"w\"}"),
            ],
        ),
        ("nothing", [None, None, None, None]),
    ];

    for (served, block) in blocks(&folder) {
        for (name, cells) in &expected {
            let cells: Vec<Option<String>> =
                cells.iter().map(|cell| cell.map(str::to_owned)).collect();
            assert_eq!(cells_of(&block, name), cells, "{name} in {served}");
        }
    }
}

#[test]
fn a_null_is_a_missing_cell_whatever_the_dtype() {
    let folder = Folder::new("nulls");
    let frame = write_table(&folder);

    for (served, block) in blocks(&folder) {
        for column in frame.columns() {
            let name = column.name().as_str();
            // These have no null in their last row; the array has its null in the third.
            if ["key", "int128", "structure", "array"].contains(&name) {
                continue;
            }
            assert_eq!(
                cells_of(&block, name)[3],
                None,
                "{name} in {served}: a null is no text, not even \"null\" or an empty one"
            );
        }
    }
}

#[test]
fn a_block_of_a_selection_holds_only_the_columns_asked_for_as_text() {
    let folder = Folder::new("selection");
    write_table(&folder);
    let window = Table::load_keys(&folder.path, "key", 4).expect("the window loads");
    let asked = ["datetime".to_owned(), "int8".to_owned()];

    let block = read_ipc(window.block_ipc(1, 2, &asked).expect("the block"));

    let names: Vec<&str> = block
        .get_column_names()
        .iter()
        .map(|name| name.as_str())
        .collect();
    assert_eq!(names, ["datetime", "int8"]);
    assert_eq!(
        cells_of(&block, "int8"),
        [Some("0".to_owned()), Some("127".to_owned())]
    );
}
