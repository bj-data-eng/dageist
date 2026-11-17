//! Each entry built through its `Build` function against a small in-memory
//! table, then asked of Polars: the result dtype comes from `collect_schema`,
//! which reads no data, and is the dtype Polars documents for the operation.
//! Steps whose result is rows are also run on the table's three rows.

use polars::prelude::{
    DataFrame, DataType, Expr, IntoLazy, LazyFrame, PlSmallStr, Schema, TimeUnit, col, df, lit,
};

use crate::catalog::{Argument, Arguments, Build, BuildError, entry};

pub(super) fn micros() -> TimeUnit {
    TimeUnit::Microseconds
}

pub(super) fn datetime() -> DataType {
    DataType::Datetime(micros(), None)
}

/// Three rows, one column of each dtype the entries work on.
pub(super) fn sample() -> LazyFrame {
    df!(
        "i" => [1_i64, 2, 3],
        "f" => [0.5_f64, 1.5, 2.5],
        "s" => ["a", "b", "a"],
        "ts" => [0_i64, 86_400_000_000, 172_800_000_000],
        "d" => [0_i32, 1, 2],
        "dur" => [1_000_000_i64, 2_000_000, 3_000_000],
        "b" => [true, false, true],
    )
    .expect("a table")
    .lazy()
    .with_columns([
        col("ts").cast(datetime()),
        col("d").cast(DataType::Date),
        col("dur").cast(DataType::Duration(micros())),
    ])
}

pub(super) fn expression_arg(expr: Expr) -> Argument {
    Argument::Expression(expr)
}

pub(super) fn flag(value: bool) -> Argument {
    Argument::Flag(Some(value))
}

pub(super) fn columns(names: &[&str]) -> Argument {
    Argument::Columns(names.iter().map(|name| (*name).to_owned()).collect())
}

pub(super) fn many(items: Vec<Argument>) -> Argument {
    Argument::Many(items)
}

pub(super) fn choice(word: &str) -> Argument {
    Argument::Choice(word.to_owned())
}

pub(super) fn text(value: &str) -> Argument {
    Argument::String(value.to_owned())
}

pub(super) fn pairs(entries: Vec<(Argument, Argument)>) -> Argument {
    Argument::Pairs(entries)
}

/// The expression entry `id` makes of `receiver`.
pub(super) fn method(id: &str, receiver: Expr, arguments: &Arguments) -> Result<Expr, BuildError> {
    match entry(id).expect("an entry").build() {
        Build::Expression(build) => build(receiver, arguments),
        other => panic!("{id} is {other:?}, not an expression method"),
    }
}

/// The expression function entry `id` makes.
pub(super) fn function(id: &str, arguments: &Arguments) -> Result<Expr, BuildError> {
    match entry(id).expect("an entry").build() {
        Build::Function(build) => build(arguments),
        other => panic!("{id} is {other:?}, not a function"),
    }
}

/// The frame step entry `id` makes of `frame`.
pub(super) fn step(
    id: &str,
    frame: LazyFrame,
    arguments: &Arguments,
) -> Result<LazyFrame, BuildError> {
    match entry(id).expect("an entry").build() {
        Build::Frame(build) => build(frame, arguments),
        other => panic!("{id} is {other:?}, not a step"),
    }
}

pub(super) fn schema(mut frame: LazyFrame) -> Schema {
    (*frame.collect_schema().expect("a schema")).clone()
}

/// The dtype Polars gives `expr` selected from the sample table.
pub(super) fn dtype_of(expr: &Expr) -> DataType {
    let schema = schema(sample().select([expr.clone().alias("out")]));
    schema.get("out").expect("one column, out").clone()
}

pub(super) fn names(frame: LazyFrame) -> Vec<String> {
    schema(frame)
        .iter_names()
        .map(PlSmallStr::to_string)
        .collect()
}

pub(super) fn run(frame: LazyFrame) -> DataFrame {
    frame.collect().expect("rows")
}

pub(super) fn integers(frame: &DataFrame, name: &str) -> Vec<Option<i64>> {
    frame
        .column(name)
        .expect("a column")
        .i64()
        .expect("integers")
        .iter()
        .collect()
}

pub(super) fn none() -> Arguments {
    Arguments::new()
}

// ---- expression methods ----------------------------------------------------

#[test]
fn shift_keeps_the_dtype() {
    let shifted = method("Expr.shift", col("i"), &none()).unwrap();
    assert_eq!(dtype_of(&shifted), DataType::Int64);
    let filled = Arguments::new()
        .with("n", expression_arg(lit(2)))
        .with("fill_value", expression_arg(lit(0)));
    let shifted = method("Expr.shift", col("i"), &filled).unwrap();
    assert_eq!(
        integers(&run(sample().select([shifted.alias("out")])), "out"),
        [Some(0), Some(0), Some(1)]
    );
}

#[test]
fn first_and_last_keep_the_dtype_of_what_they_take() {
    for ignore_nulls in [false, true] {
        let given = Arguments::new().with("ignore_nulls", flag(ignore_nulls));
        let first = method("Expr.first", col("s"), &given).unwrap();
        assert_eq!(dtype_of(&first), DataType::String);
        let last = method("Expr.last", col("ts"), &given).unwrap();
        assert_eq!(dtype_of(&last), datetime());
    }
    let last = method("Expr.last", col("i"), &none()).unwrap();
    assert_eq!(
        integers(&run(sample().select([last.alias("out")])), "out"),
        [Some(3)]
    );
}

#[test]
fn first_skips_nulls_only_when_asked() {
    let with_null = df!("x" => [None, Some(7_i64), Some(8)]).unwrap().lazy();
    let plain = method("Expr.first", col("x"), &none()).unwrap();
    assert_eq!(
        integers(&run(with_null.clone().select([plain])), "x"),
        [None]
    );
    let skipping = Arguments::new().with("ignore_nulls", flag(true));
    let skipped = method("Expr.first", col("x"), &skipping).unwrap();
    assert_eq!(integers(&run(with_null.select([skipped])), "x"), [Some(7)]);
}

#[test]
fn cum_sum_gives_the_documented_dtypes() {
    let cases = [
        ("i", DataType::Int64),
        ("f", DataType::Float64),
        ("dur", DataType::Duration(micros())),
        // Booleans are counted as whole numbers.
        ("b", DataType::UInt32),
    ];
    for (column, expected) in cases {
        let built = method("Expr.cum_sum", col(column), &none()).unwrap();
        assert_eq!(dtype_of(&built), expected, "cum_sum of {column}");
    }
    let reversed = Arguments::new().with("reverse", flag(true));
    let built = method("Expr.cum_sum", col("i"), &reversed).unwrap();
    assert_eq!(
        integers(&run(sample().select([built.alias("out")])), "out"),
        [Some(6), Some(5), Some(3)]
    );
}

#[test]
fn cum_count_counts_values_as_an_index_type() {
    for column in ["i", "s", "ts"] {
        let built = method("Expr.cum_count", col(column), &none()).unwrap();
        assert_eq!(dtype_of(&built), DataType::UInt32, "cum_count of {column}");
    }
    let with_null = df!("x" => [Some(1_i64), None, Some(3)]).unwrap().lazy();
    let built = method("Expr.cum_count", col("x"), &none()).unwrap();
    let counted = run(with_null.select([built.alias("n")]));
    let counts: Vec<_> = counted.column("n").unwrap().u32().unwrap().iter().collect();
    assert_eq!(counts, [Some(1), Some(1), Some(2)]);
}

#[test]
fn cum_max_keeps_the_dtype_of_what_it_takes() {
    let cases = [
        ("i", DataType::Int64),
        ("f", DataType::Float64),
        ("d", DataType::Date),
        ("ts", datetime()),
    ];
    for (column, expected) in cases {
        let built = method("Expr.cum_max", col(column), &none()).unwrap();
        assert_eq!(dtype_of(&built), expected, "cum_max of {column}");
    }
}

#[test]
fn sub_gives_the_documented_dtypes() {
    let cases = [
        ("i", "i", DataType::Int64),
        ("f", "i", DataType::Float64),
        // The difference of two points in time is a length of time.
        ("ts", "ts", DataType::Duration(micros())),
        ("ts", "dur", datetime()),
    ];
    for (left, right, expected) in cases {
        let given = Arguments::new().with("other", expression_arg(col(right)));
        let built = method("Expr.sub", col(left), &given).unwrap();
        assert_eq!(dtype_of(&built), expected, "{left} - {right}");
    }
}

#[test]
fn dt_methods_give_the_documented_dtypes() {
    let date = method("Expr.dt.date", col("ts"), &none()).unwrap();
    assert_eq!(dtype_of(&date), DataType::Date);
    let time = method("Expr.dt.time", col("ts"), &none()).unwrap();
    assert_eq!(dtype_of(&time), DataType::Time);
    let month_start = method("Expr.dt.month_start", col("ts"), &none()).unwrap();
    assert_eq!(dtype_of(&month_start), datetime());
    let month_start = method("Expr.dt.month_start", col("d"), &none()).unwrap();
    assert_eq!(dtype_of(&month_start), DataType::Date);
    let days = method("Expr.dt.total_days", col("dur"), &none()).unwrap();
    assert_eq!(dtype_of(&days), DataType::Int64);
    let fractional = Arguments::new().with("fractional", flag(true));
    let days = method("Expr.dt.total_days", col("dur"), &fractional).unwrap();
    assert_eq!(dtype_of(&days), DataType::Float64);
}

#[test]
fn dt_offset_by_moves_by_a_duration_and_keeps_the_dtype() {
    let by = Arguments::new().with("by", expression_arg(lit("1mo")));
    let moved = method("Expr.dt.offset_by", col("ts"), &by).unwrap();
    assert_eq!(dtype_of(&moved), datetime());
    let moved = method("Expr.dt.offset_by", col("d"), &by).unwrap();
    assert_eq!(dtype_of(&moved), DataType::Date);
    // One day after the first of January 1970 is the second.
    let by_day = Arguments::new().with("by", expression_arg(lit("1d")));
    let moved = method("Expr.dt.offset_by", col("d"), &by_day).unwrap();
    let rows = run(sample().select([moved.cast(DataType::Int32).alias("days")]));
    let days: Vec<_> = rows.column("days").unwrap().i32().unwrap().iter().collect();
    assert_eq!(days, [Some(1), Some(2), Some(3)]);
}

#[test]
fn over_maps_groups_to_rows_unless_told_otherwise() {
    let by_group = Arguments::new().with("partition_by", many(vec![expression_arg(col("s"))]));
    let total = method("Expr.over", col("f").sum(), &by_group).unwrap();
    assert_eq!(dtype_of(&total), DataType::Float64);
    let totals = run(sample().select([total.alias("out")]));
    let sums: Vec<_> = totals
        .column("out")
        .unwrap()
        .f64()
        .unwrap()
        .iter()
        .collect();
    assert_eq!(sums, [Some(3.0), Some(1.5), Some(3.0)]);

    let joined = Arguments::new()
        .with("partition_by", many(vec![expression_arg(col("s"))]))
        .with("mapping_strategy", choice("join"));
    let listed = method("Expr.over", col("i"), &joined).unwrap();
    assert_eq!(dtype_of(&listed), DataType::List(Box::new(DataType::Int64)));
}

#[test]
fn over_orders_and_partitions_with_every_expression_given() {
    let given = Arguments::new()
        .with("partition_by", expression_arg(col("s")))
        .with("more_exprs", many(vec![expression_arg(col("b"))]))
        .with("order_by", expression_arg(col("i")))
        .with("descending", flag(true));
    // Within each (s, b) group the rows are taken from the largest i down: the group of
    // i = 1 and 3 totals 3 then 4, and each row gets the total at its own place.
    let running = method("Expr.cum_sum", col("i"), &none()).unwrap();
    let windowed = method("Expr.over", running, &given).unwrap();
    assert_eq!(dtype_of(&windowed), DataType::Int64);
    let rows = run(sample().select([windowed.alias("out")]));
    assert_eq!(integers(&rows, "out"), [Some(4), Some(2), Some(3)]);
}

#[test]
fn over_without_groups_or_an_order_is_refused_by_polars() {
    let error = method("Expr.over", col("i"), &none()).unwrap_err();
    assert!(matches!(error, BuildError::Polars(_)), "{error}");
}

#[test]
fn comparisons_and_null_tests_give_booleans() {
    for id in [
        "Expr.eq", "Expr.ne", "Expr.gt", "Expr.ge", "Expr.lt", "Expr.le",
    ] {
        let given = Arguments::new().with("other", expression_arg(lit(2)));
        let built = method(id, col("i"), &given).unwrap();
        assert_eq!(dtype_of(&built), DataType::Boolean, "{id}");
    }
    for id in ["Expr.is_null", "Expr.is_not_null"] {
        let built = method(id, col("s"), &none()).unwrap();
        assert_eq!(dtype_of(&built), DataType::Boolean, "{id}");
    }
}

#[test]
fn each_comparison_keeps_the_rows_polars_says_it_does() {
    // i = [1, 2, 3] compared with 2.
    let expected = [
        ("Expr.eq", [false, true, false]),
        ("Expr.ne", [true, false, true]),
        ("Expr.gt", [false, false, true]),
        ("Expr.ge", [false, true, true]),
        ("Expr.lt", [true, false, false]),
        ("Expr.le", [true, true, false]),
    ];
    for (id, rows) in expected {
        let given = Arguments::new().with("other", expression_arg(lit(2)));
        let built = method(id, col("i"), &given).unwrap();
        let result = run(sample().select([built.alias("out")]));
        let found: Vec<_> = result
            .column("out")
            .unwrap()
            .bool()
            .unwrap()
            .iter()
            .collect();
        assert_eq!(found, rows.map(Some), "{id}");
    }
}

// ---- functions -------------------------------------------------------------

#[test]
fn col_names_a_column_or_several() {
    let one = Arguments::new().with("name", columns(&["f"]));
    assert_eq!(
        dtype_of(&function("polars.col", &one).unwrap()),
        DataType::Float64
    );

    let several = Arguments::new()
        .with("name", columns(&["i"]))
        .with("more_names", many(vec![columns(&["s"]), columns(&["b"])]));
    let selected = schema(sample().select([function("polars.col", &several).unwrap()]));
    let found: Vec<_> = selected
        .iter()
        .map(|(name, dtype)| (name.to_string(), dtype.clone()))
        .collect();
    assert_eq!(
        found,
        [
            ("i".to_owned(), DataType::Int64),
            ("s".to_owned(), DataType::String),
            ("b".to_owned(), DataType::Boolean),
        ]
    );
}

#[test]
fn lit_gives_the_dtype_polars_gives_a_typed_value() {
    let cases = [
        (Argument::Integer(5), None, DataType::Int32),
        (Argument::Integer(5_000_000_000), None, DataType::Int64),
        (Argument::Float(2.5), None, DataType::Float64),
        (text("x"), None, DataType::String),
        (flag(true), None, DataType::Boolean),
        (Argument::Flag(None), None, DataType::Null),
        (Argument::Integer(5), Some(DataType::Int8), DataType::Int8),
        (text("5"), Some(DataType::Int64), DataType::Int64),
        (
            Argument::Float(2.5),
            Some(DataType::Float32),
            DataType::Float32,
        ),
    ];
    for (value, dtype, expected) in cases {
        let mut given = Arguments::new().with("value", value);
        if let Some(dtype) = dtype {
            given = given.with("dtype", Argument::Dtype(dtype));
        }
        let built = function("polars.lit", &given).unwrap();
        assert_eq!(dtype_of(&built), expected);
    }
}

#[test]
fn lit_with_a_dtype_refuses_a_value_that_does_not_fit_it() {
    let given = Arguments::new()
        .with("value", text("not a number"))
        .with("dtype", Argument::Dtype(DataType::Int64));
    let error = function("polars.lit", &given).unwrap_err();
    assert!(matches!(error, BuildError::Polars(_)), "{error}");
}

#[test]
fn lit_refuses_allow_object_because_a_step_holds_no_python_object() {
    let given = Arguments::new()
        .with("value", Argument::Integer(1))
        .with("allow_object", flag(true));
    let error = function("polars.lit", &given).unwrap_err();
    assert!(matches!(error, BuildError::Polars(_)), "{error}");
}

// ---- frame steps -----------------------------------------------------------

#[test]
fn with_columns_adds_named_columns_and_replaces_those_it_names_again() {
    let given = Arguments::new()
        .with("exprs", many(vec![expression_arg(col("f").is_null())]))
        .with(
            "named_exprs",
            pairs(vec![(
                columns(&["running"]),
                expression_arg(col("i").cum_sum(false)),
            )]),
        );
    let widened = step("LazyFrame.with_columns", sample(), &given).unwrap();
    let schema = schema(widened);
    let found: Vec<_> = schema
        .iter()
        .map(|(name, dtype)| (name.as_str(), dtype.clone()))
        .collect();
    assert_eq!(
        found,
        [
            ("i", DataType::Int64),
            // `f` now holds the boolean that says it has no value.
            ("f", DataType::Boolean),
            ("s", DataType::String),
            ("ts", datetime()),
            ("d", DataType::Date),
            ("dur", DataType::Duration(micros())),
            ("b", DataType::Boolean),
            ("running", DataType::Int64),
        ]
    );
}

#[test]
fn select_keeps_the_columns_it_is_given_in_that_order() {
    let given = Arguments::new()
        .with(
            "exprs",
            many(vec![expression_arg(col("s")), expression_arg(col("i"))]),
        )
        .with(
            "named_exprs",
            pairs(vec![(columns(&["flag"]), expression_arg(col("b")))]),
        );
    let selected = step("LazyFrame.select", sample(), &given).unwrap();
    assert_eq!(names(selected.clone()), ["s", "i", "flag"]);
    let schema = schema(selected);
    assert_eq!(schema.get("flag"), Some(&DataType::Boolean));
}

#[test]
fn filter_keeps_the_rows_every_condition_holds_for() {
    let by_flag = Arguments::new().with("predicates", many(vec![expression_arg(col("b"))]));
    let rows = run(step("LazyFrame.filter", sample(), &by_flag).unwrap());
    assert_eq!(integers(&rows, "i"), [Some(1), Some(3)]);

    let both = Arguments::new().with(
        "predicates",
        many(vec![
            expression_arg(col("b")),
            expression_arg(col("i").gt(lit(1))),
        ]),
    );
    let rows = run(step("LazyFrame.filter", sample(), &both).unwrap());
    assert_eq!(integers(&rows, "i"), [Some(3)]);

    let equal = Arguments::new().with(
        "constraints",
        pairs(vec![(columns(&["s"]), expression_arg(lit("b")))]),
    );
    let rows = run(step("LazyFrame.filter", sample(), &equal).unwrap());
    assert_eq!(integers(&rows, "i"), [Some(2)]);

    let nothing = step("LazyFrame.filter", sample(), &none()).unwrap();
    assert_eq!(run(nothing).height(), 3);
    assert_eq!(
        names(sample()),
        names(step("LazyFrame.filter", sample(), &by_flag).unwrap())
    );
}

#[test]
fn sort_orders_by_each_key_with_its_own_direction() {
    let by_s_then_i = Arguments::new()
        .with("by", many(vec![expression_arg(col("s"))]))
        .with("more_by", many(vec![expression_arg(col("i"))]))
        .with("descending", many(vec![flag(false), flag(true)]));
    let rows = run(step("LazyFrame.sort", sample(), &by_s_then_i).unwrap());
    assert_eq!(integers(&rows, "i"), [Some(3), Some(1), Some(2)]);

    // One flag applies to every key, and ties keep their order when asked to.
    let backwards = Arguments::new()
        .with("by", many(vec![expression_arg(col("s"))]))
        .with("descending", flag(true))
        .with("maintain_order", flag(true));
    let rows = run(step("LazyFrame.sort", sample(), &backwards).unwrap());
    assert_eq!(integers(&rows, "i"), [Some(2), Some(1), Some(3)]);
}

#[test]
fn sort_puts_nulls_where_asked() {
    let with_null = df!("x" => [Some(2_i64), None, Some(1)]).unwrap().lazy();
    let last = Arguments::new()
        .with("by", many(vec![expression_arg(col("x"))]))
        .with("nulls_last", flag(true));
    let rows = run(step("LazyFrame.sort", with_null.clone(), &last).unwrap());
    assert_eq!(integers(&rows, "x"), [Some(1), Some(2), None]);
    let first = Arguments::new().with("by", many(vec![expression_arg(col("x"))]));
    let rows = run(step("LazyFrame.sort", with_null, &first).unwrap());
    assert_eq!(integers(&rows, "x"), [None, Some(1), Some(2)]);
}

#[test]
fn sort_refuses_a_list_of_directions_that_does_not_match_its_keys() {
    let given = Arguments::new()
        .with("by", many(vec![expression_arg(col("s"))]))
        .with("descending", many(vec![flag(true), flag(false)]));
    let error = step("LazyFrame.sort", sample(), &given).err().unwrap();
    assert!(
        matches!(
            error,
            BuildError::WrongKind {
                parameter: "descending",
                ..
            }
        ),
        "{error}"
    );
}

#[test]
fn rename_gives_columns_their_new_names() {
    let given = Arguments::new().with(
        "mapping",
        pairs(vec![
            (columns(&["i"]), text("index")),
            (columns(&["s"]), text("label")),
        ]),
    );
    let renamed = step("LazyFrame.rename", sample(), &given).unwrap();
    assert_eq!(
        names(renamed),
        ["index", "f", "label", "ts", "d", "dur", "b"]
    );
}

#[test]
fn rename_of_a_column_the_table_lacks_fails_only_when_strict() {
    let mapping = pairs(vec![(columns(&["nope"]), text("other"))]);
    let strict = Arguments::new().with("mapping", mapping.clone());
    let renamed = step("LazyFrame.rename", sample(), &strict).unwrap();
    assert!(renamed.clone().collect_schema().is_err());
    let lenient = Arguments::new()
        .with("mapping", mapping)
        .with("strict", flag(false));
    let renamed = step("LazyFrame.rename", sample(), &lenient).unwrap();
    assert_eq!(names(renamed), names(sample()));
}

pub(super) fn other_table() -> LazyFrame {
    sample().select([col("i"), col("f"), col("s").alias("t")])
}

pub(super) fn table_arg() -> Argument {
    Argument::Table(other_table())
}

pub(super) fn on_i() -> Argument {
    many(vec![expression_arg(col("i"))])
}

#[test]
fn join_by_a_shared_key_merges_the_key_and_suffixes_repeated_names() {
    let given = Arguments::new()
        .with("other", table_arg())
        .with("on", on_i());
    let joined = step("LazyFrame.join", sample(), &given).unwrap();
    assert_eq!(
        names(joined.clone()),
        ["i", "f", "s", "ts", "d", "dur", "b", "f_right", "t"]
    );
    assert_eq!(run(joined).height(), 3);

    let suffixed = given.with("suffix", text("_other"));
    let joined = step("LazyFrame.join", sample(), &suffixed).unwrap();
    assert!(names(joined).contains(&"f_other".to_owned()));
}

#[test]
fn join_by_different_keys_keeps_the_right_key_only_when_not_coalescing() {
    let right = Argument::Table(sample().select([col("i").alias("j"), col("f").alias("g")]));
    let keys = |coalesce| {
        Arguments::new()
            .with("other", right.clone())
            .with("left_on", many(vec![expression_arg(col("i"))]))
            .with("right_on", many(vec![expression_arg(col("j"))]))
            .with("coalesce", Argument::Flag(coalesce))
    };
    let kept = step("LazyFrame.join", sample(), &keys(Some(false))).unwrap();
    assert!(names(kept).contains(&"j".to_owned()));
    let merged = step("LazyFrame.join", sample(), &keys(Some(true))).unwrap();
    assert!(!names(merged).contains(&"j".to_owned()));
}

#[test]
fn join_how_decides_which_rows_and_columns_are_kept() {
    let only_a = other_table().filter(col("t").eq(lit("a")));
    let keys = |how: &str| {
        Arguments::new()
            .with("other", Argument::Table(only_a.clone()))
            .with("on", on_i())
            .with("how", choice(how))
    };
    // The sample has i = 1, 2, 3 and `s` = a, b, a; the other table keeps i = 1 and 3.
    let inner = run(step("LazyFrame.join", sample(), &keys("inner")).unwrap());
    assert_eq!(sorted(integers(&inner, "i")), [Some(1), Some(3)]);
    let semi = step("LazyFrame.join", sample(), &keys("semi")).unwrap();
    assert_eq!(names(semi.clone()), names(sample()));
    assert_eq!(sorted(integers(&run(semi), "i")), [Some(1), Some(3)]);
    let anti = run(step("LazyFrame.join", sample(), &keys("anti")).unwrap());
    assert_eq!(sorted(integers(&anti, "i")), [Some(2)]);
    let left = run(step(
        "LazyFrame.join",
        sample(),
        &keys("left").with("maintain_order", choice("left")),
    )
    .unwrap());
    assert_eq!(integers(&left, "i"), [Some(1), Some(2), Some(3)]);
}

#[test]
fn a_cross_join_pairs_every_row_and_takes_no_keys() {
    let given = Arguments::new()
        .with("other", table_arg())
        .with("how", choice("cross"));
    let crossed = step("LazyFrame.join", sample(), &given).unwrap();
    assert_eq!(run(crossed).height(), 9);

    let with_keys = given.with("on", on_i());
    let error = step("LazyFrame.join", sample(), &with_keys).err().unwrap();
    assert!(matches!(error, BuildError::Polars(_)), "{error}");
}

#[test]
fn join_validate_rejects_keys_that_repeat_when_a_side_must_be_unique() {
    // `s` and `t` both hold a, b, a, so a one-to-one join of them cannot hold.
    let on_s_and_t = |validate: &str| {
        Arguments::new()
            .with("other", table_arg())
            .with("left_on", many(vec![expression_arg(col("s"))]))
            .with("right_on", many(vec![expression_arg(col("t"))]))
            .with("validate", choice(validate))
    };
    let many_to_many = step("LazyFrame.join", sample(), &on_s_and_t("m:m")).unwrap();
    assert!(many_to_many.collect().is_ok());
    let one_to_one = step("LazyFrame.join", sample(), &on_s_and_t("1:1")).unwrap();
    assert!(one_to_one.collect().is_err());
}

#[test]
fn join_refuses_keys_the_way_python_does() {
    let both = Arguments::new()
        .with("other", table_arg())
        .with("on", on_i())
        .with("left_on", on_i())
        .with("right_on", on_i());
    let lone = Arguments::new()
        .with("other", table_arg())
        .with("left_on", on_i());
    let none_given = Arguments::new().with("other", table_arg());
    for (given, case) in [
        (both, "on with left_on"),
        (lone, "left_on alone"),
        (none_given, "no keys"),
    ] {
        let error = step("LazyFrame.join", sample(), &given).err().unwrap();
        assert!(matches!(error, BuildError::Polars(_)), "{case}: {error}");
    }
}

fn pairs_table() -> LazyFrame {
    df!(
        "i" => [1_i64, 1, 2],
        "s" => ["a", "b", "a"],
        "f" => [1.0_f64, 2.0, 3.0],
    )
    .expect("a table")
    .lazy()
}

fn values_of_s() -> Argument {
    Argument::Table(df!("s" => ["a", "b"]).expect("a table").lazy())
}

fn sorted(mut values: Vec<Option<i64>>) -> Vec<Option<i64>> {
    values.sort();
    values
}

pub(super) fn floats(frame: &DataFrame, name: &str) -> Vec<Option<f64>> {
    frame
        .column(name)
        .expect("a column")
        .f64()
        .expect("floats")
        .iter()
        .collect()
}

#[test]
fn pivot_spreads_the_values_of_a_column_into_columns() {
    let given = Arguments::new()
        .with("on", columns(&["s"]))
        .with("on_columns", values_of_s())
        .with("index", columns(&["i"]))
        .with("values", columns(&["f"]))
        .with("maintain_order", flag(true));
    let pivoted = step("LazyFrame.pivot", pairs_table(), &given).unwrap();
    let schema = schema(pivoted.clone());
    let found: Vec<_> = schema
        .iter()
        .map(|(name, dtype)| (name.as_str(), dtype.clone()))
        .collect();
    assert_eq!(
        found,
        [
            ("i", DataType::Int64),
            ("a", DataType::Float64),
            ("b", DataType::Float64),
        ]
    );
    let rows = run(pivoted);
    assert_eq!(integers(&rows, "i"), [Some(1), Some(2)]);
    assert_eq!(floats(&rows, "a"), [Some(1.0), Some(3.0)]);
    assert_eq!(floats(&rows, "b"), [Some(2.0), None]);
}

#[test]
fn pivot_takes_the_other_columns_as_values_or_as_the_index() {
    let table = pairs_table();
    let by_index = Arguments::new()
        .with("on", columns(&["s"]))
        .with("on_columns", values_of_s())
        .with("index", columns(&["i"]));
    let by_values = Arguments::new()
        .with("on", columns(&["s"]))
        .with("on_columns", values_of_s())
        .with("values", columns(&["f"]));
    for given in [by_index, by_values] {
        let pivoted = step("LazyFrame.pivot", table.clone(), &given).unwrap();
        let mut found = names(pivoted);
        found.sort();
        assert_eq!(found, ["a", "b", "i"]);
    }
}

/// Two `a` values and one `b` value for the same `i`, which a pivot must combine.
fn repeated_values() -> LazyFrame {
    df!(
        "i" => [1_i64, 1, 1],
        "s" => ["a", "a", "b"],
        "f" => [1.0_f64, 2.0, 5.0],
    )
    .unwrap()
    .lazy()
}

/// A pivot of `repeated_values` that combines the values of a cell with the expression
/// `combine` makes of `polars.element`, which stands for the values of the cell.
fn pivot_combining(combine: fn(Expr) -> Expr) -> DataFrame {
    let element = function("polars.element", &none()).unwrap();
    let given = entry("LazyFrame.pivot")
        .expect("an entry")
        .arguments([
            ("on", columns(&["s"])),
            ("on_columns", values_of_s()),
            ("index", columns(&["i"])),
            ("aggregate_function", expression_arg(combine(element))),
            ("column_naming", choice("combine")),
            ("separator", text(".")),
        ])
        .expect("arguments that fit");
    let pivoted = step("LazyFrame.pivot", repeated_values(), &given).unwrap();
    assert_eq!(names(pivoted.clone()), ["i", "f.a", "f.b"]);
    run(pivoted)
}

#[test]
fn pivot_combines_repeated_values_with_the_aggregate_given() {
    // The `a` cell holds 1.0 and 2.0 and the `b` cell 5.0.
    let rows = pivot_combining(Expr::sum);
    assert_eq!(floats(&rows, "f.a"), [Some(3.0)]);
    assert_eq!(floats(&rows, "f.b"), [Some(5.0)]);
    let rows = pivot_combining(Expr::max);
    assert_eq!(floats(&rows, "f.a"), [Some(2.0)]);
    assert_eq!(floats(&rows, "f.b"), [Some(5.0)]);
}

#[test]
fn pivot_without_an_aggregate_refuses_a_cell_with_two_values() {
    let given = Arguments::new()
        .with("on", columns(&["s"]))
        .with("on_columns", values_of_s())
        .with("index", columns(&["i"]));
    let table = df!("i" => [1_i64, 1], "s" => ["a", "a"], "f" => [1.0_f64, 2.0])
        .unwrap()
        .lazy();
    let pivoted = step("LazyFrame.pivot", table, &given).unwrap();
    assert!(pivoted.collect().is_err());
}

#[test]
fn pivot_needs_an_index_or_values() {
    let given = Arguments::new()
        .with("on", columns(&["s"]))
        .with("on_columns", values_of_s());
    let error = step("LazyFrame.pivot", pairs_table(), &given)
        .err()
        .unwrap();
    assert!(matches!(error, BuildError::Polars(_)), "{error}");
}

// ---- errors ----------------------------------------------------------------

#[test]
fn a_missing_required_argument_names_the_parameter() {
    let missing = |error: BuildError| match error {
        BuildError::Missing { parameter } => parameter,
        other => panic!("not a missing argument: {other}"),
    };
    assert_eq!(
        missing(method("Expr.eq", col("i"), &none()).unwrap_err()),
        "other"
    );
    assert_eq!(
        missing(method("Expr.sub", col("i"), &none()).unwrap_err()),
        "other"
    );
    assert_eq!(
        missing(method("Expr.dt.offset_by", col("ts"), &none()).unwrap_err()),
        "by"
    );
    assert_eq!(
        missing(function("polars.col", &none()).unwrap_err()),
        "name"
    );
    assert_eq!(
        missing(function("polars.lit", &none()).unwrap_err()),
        "value"
    );
    assert_eq!(
        missing(step("LazyFrame.sort", sample(), &none()).err().unwrap()),
        "by"
    );
    assert_eq!(
        missing(step("LazyFrame.rename", sample(), &none()).err().unwrap()),
        "mapping"
    );
    assert_eq!(
        missing(step("LazyFrame.join", sample(), &none()).err().unwrap()),
        "other"
    );
    assert_eq!(
        missing(step("LazyFrame.pivot", sample(), &none()).err().unwrap()),
        "on"
    );
}

#[test]
fn an_argument_of_the_wrong_kind_names_the_parameter_and_what_was_given() {
    let wrong = |error: BuildError| match error {
        BuildError::WrongKind {
            parameter,
            expected,
            found,
        } => (parameter, expected, found),
        other => panic!("not a wrong kind: {other}"),
    };
    let integer = Arguments::new().with("other", Argument::Integer(1));
    assert_eq!(
        wrong(method("Expr.eq", col("i"), &integer).unwrap_err()),
        ("other", "an expression", "a whole number")
    );
    let not_a_flag = Arguments::new().with("ignore_nulls", text("yes"));
    assert_eq!(
        wrong(method("Expr.first", col("i"), &not_a_flag).unwrap_err()),
        ("ignore_nulls", "a flag", "text")
    );
    let not_a_table = Arguments::new().with("other", Argument::Integer(1));
    assert_eq!(
        wrong(
            step("LazyFrame.join", sample(), &not_a_table)
                .err()
                .unwrap()
        ),
        ("other", "a table", "a whole number")
    );
    let two_names =
        Arguments::new().with("mapping", pairs(vec![(columns(&["a", "b"]), text("c"))]));
    assert_eq!(
        wrong(
            step("LazyFrame.rename", sample(), &two_names)
                .err()
                .unwrap()
        ),
        ("mapping", "one column name", "column names")
    );
}

#[test]
fn a_word_that_is_not_one_of_the_choices_is_refused() {
    let given = Arguments::new()
        .with("other", table_arg())
        .with("on", on_i())
        .with("how", choice("outer"));
    let error = step("LazyFrame.join", sample(), &given).err().unwrap();
    assert!(
        matches!(&error, BuildError::UnknownChoice { parameter: "how", word } if word == "outer"),
        "{error}"
    );
    let given = Arguments::new().with("mapping_strategy", choice("explode_all"));
    let error = method("Expr.over", col("i"), &given).unwrap_err();
    assert!(
        matches!(
            error,
            BuildError::UnknownChoice {
                parameter: "mapping_strategy",
                ..
            }
        ),
        "{error}"
    );
}
