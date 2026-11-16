//! Dtypes as a document spells them: Python Polars' own text for each.

use dageist_engine::document::{Dtype, DtypeError};
use polars::prelude::*;

/// Each spelling with the dtype it names. The spellings are the text Python
/// Polars prints for the dtype, such as `Datetime(time_unit='us', time_zone=None)`.
fn spellings() -> Vec<(&'static str, DataType)> {
    let paris = TimeZone::opt_try_new(Some("Europe/Paris"))
        .expect("a time zone")
        .expect("a zone");
    vec![
        ("Int8", DataType::Int8),
        ("Int16", DataType::Int16),
        ("Int32", DataType::Int32),
        ("Int64", DataType::Int64),
        ("Int128", DataType::Int128),
        ("UInt8", DataType::UInt8),
        ("UInt16", DataType::UInt16),
        ("UInt32", DataType::UInt32),
        ("UInt64", DataType::UInt64),
        ("UInt128", DataType::UInt128),
        ("Float16", DataType::Float16),
        ("Float32", DataType::Float32),
        ("Float64", DataType::Float64),
        ("String", DataType::String),
        ("Boolean", DataType::Boolean),
        ("Date", DataType::Date),
        ("Time", DataType::Time),
        ("Null", DataType::Null),
        (
            "Datetime(time_unit='us', time_zone=None)",
            DataType::Datetime(TimeUnit::Microseconds, None),
        ),
        (
            "Datetime(time_unit='ms', time_zone=None)",
            DataType::Datetime(TimeUnit::Milliseconds, None),
        ),
        (
            "Datetime(time_unit='ns', time_zone='UTC')",
            DataType::Datetime(TimeUnit::Nanoseconds, Some(TimeZone::UTC)),
        ),
        (
            "Datetime(time_unit='us', time_zone='Europe/Paris')",
            DataType::Datetime(TimeUnit::Microseconds, Some(paris)),
        ),
        (
            "Duration(time_unit='us')",
            DataType::Duration(TimeUnit::Microseconds),
        ),
        (
            "Duration(time_unit='ns')",
            DataType::Duration(TimeUnit::Nanoseconds),
        ),
        ("List(Int64)", DataType::List(Box::new(DataType::Int64))),
        (
            "List(List(String))",
            DataType::List(Box::new(DataType::List(Box::new(DataType::String)))),
        ),
        (
            "List(Datetime(time_unit='ms', time_zone=None))",
            DataType::List(Box::new(DataType::Datetime(TimeUnit::Milliseconds, None))),
        ),
    ]
}

#[test]
fn every_spelling_reads_as_its_dtype_and_writes_back_as_itself() {
    for (spelling, dtype) in spellings() {
        let read: Dtype = spelling
            .parse()
            .unwrap_or_else(|error| panic!("{spelling}: {error}"));
        assert_eq!(read.as_polars(), &dtype, "{spelling}");
        assert_eq!(read.to_string(), spelling);
        let made = Dtype::new(dtype.clone()).expect("a dtype of the document");
        assert_eq!(made, read, "{spelling}");
        assert_eq!(made.into_polars(), dtype);
    }
}

#[test]
fn a_spelling_that_is_not_exactly_one_of_those_is_refused() {
    for spelling in [
        "",
        "int64",
        "Int65",
        "Utf8",
        " Int64",
        "Int64 ",
        "Datetime",
        "Datetime(time_unit='us')",
        "Datetime(time_unit='us',time_zone=None)",
        "Datetime(time_unit='s', time_zone=None)",
        "Datetime(time_unit=\"us\", time_zone=None)",
        "Datetime(time_unit='us', time_zone=UTC)",
        "Datetime(time_unit='us', time_zone='')",
        "Duration",
        "Duration(time_unit='s')",
        "List",
        "List()",
        "List(Nope)",
        "List(Int64",
        "Struct",
    ] {
        assert!(spelling.parse::<Dtype>().is_err(), "{spelling:?} was read");
    }
}

#[test]
fn an_unknown_time_zone_is_refused_with_polars_reason() {
    let error = "Datetime(time_unit='us', time_zone='Nowhere/City')"
        .parse::<Dtype>()
        .expect_err("not a zone");
    let DtypeError::TimeZone { zone, source } = error else {
        panic!("not a time zone error: {error:?}");
    };
    assert_eq!(zone, "Nowhere/City");
    let polars = TimeZone::opt_try_new(Some("Nowhere/City")).expect_err("Polars refuses it");
    assert_eq!(source.to_string(), polars.to_string());
}

#[test]
fn a_dtype_the_document_cannot_hold_is_refused() {
    for dtype in [
        DataType::Binary,
        DataType::List(Box::new(DataType::Binary)),
        DataType::Struct(vec![Field::new("a".into(), DataType::Int64)]),
    ] {
        assert!(
            matches!(Dtype::new(dtype.clone()), Err(DtypeError::Unsupported(refused)) if refused == dtype),
            "{dtype} was accepted"
        );
    }
}

fn nested_lists(depth: usize, inner: &str) -> String {
    format!("{}{inner}{}", "List(".repeat(depth), ")".repeat(depth))
}

#[test]
fn lists_nest_up_to_thirty_two_deep_and_no_deeper() {
    let allowed = nested_lists(32, "Int64");
    let dtype: Dtype = allowed.parse().expect("thirty-two lists");
    assert_eq!(dtype.to_string(), allowed);

    let beyond = nested_lists(33, "Int64");
    assert!(matches!(
        beyond.parse::<Dtype>(),
        Err(DtypeError::Unknown(_))
    ));
}

#[test]
fn a_hostile_dtype_text_is_refused_without_exhausting_the_stack() {
    let hostile = nested_lists(200_000, "Int64");
    assert!(matches!(
        hostile.parse::<Dtype>(),
        Err(DtypeError::Unknown(_))
    ));
}

#[test]
fn a_polars_dtype_of_lists_nests_up_to_the_same_limit() {
    let wrap =
        |depth: usize| (0..depth).fold(DataType::Int64, |inner, _| DataType::List(Box::new(inner)));
    let allowed = Dtype::new(wrap(32)).expect("thirty-two lists");
    assert_eq!(
        allowed.to_string().parse::<Dtype>().expect("reads back"),
        allowed
    );
    assert!(matches!(
        Dtype::new(wrap(33)),
        Err(DtypeError::Unsupported(_))
    ));
}
