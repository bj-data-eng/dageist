//! The rules that read a literal's value or dtype, each crossed with every literal a document can
//! write and every dtype of column it meets, with Polars as the judge.
//!
//! A rule reads a literal where Polars treats a number typed without a dtype differently from a
//! column or a typed literal: it adapts to the dtype of the value it meets, or is cast to it
//! strictly, or is read as a count or as a share. The literals worth crossing are those at the
//! edges of the dtypes, and those written as another kind than the dtype they are given, which
//! the step engine casts strictly before Polars sees them. The grids make every one at once, so
//! a gap shows up as one cell and not one review round later.
//!
//! The grids are built with [`crate::grid`], which says how they are written. The rules they
//! cover are every one the walk reads a literal by, which is the role of the parameter it is
//! given as: what a number is added to, divided by, raised to, a logarithm is taken to, a bound
//! it is cut to, a share, a count or a length, a value that fills; the values `replace` and
//! `replace_strict` list; the counts and places that `head`, `tail`, `slice` and `gather_every`
//! cut by, as expressions and as steps; and the comparisons with a number in a filter and in a
//! condition. The grids of the normal tests ask Polars what it says of the schema, and of the
//! rows where the walk says it refuses on them, and take seconds each. The large grids cross every
//! value with every dtype, ask the rows of every cell, and run on request:
//!
//! ```sh
//! cargo test --offline --locked -p dageist-engine --test document the_large_grid \
//!     -- --ignored --nocapture --test-threads 1
//! ```
//!
//! A line of a test that names cells says why the walk and Polars part there, and a reason that
//! ends in a candidate to file is a difference this repository has not settled: the walk allows
//! or refuses what Polars does not, for a value it does not read.
//!
//! What `is_in` is given is not a literal that a rule reads, and its grid is in
//! `membership_grid.rs`, built on the same helper.

use polars::prelude::{DataFrame, DataType, NamedFrom, Series, TimeUnit};
use serde_json::{Value, json};

use crate::grid::literals::{self, Written};
use crate::grid::{
    Case, Chosen, Connections, Depth, Grid, HoldsColumn, HoldsLiteral, Kind, Known, Pattern,
    Question, Verdicts, column_dtypes, one_column_table,
};
use crate::support::{call, chain, field, field_then, many};

/// The dtypes a literal is given, or none.
fn words() -> Vec<Option<&'static str>> {
    std::iter::once(None)
        .chain(literals::DTYPES.iter().copied().map(Some))
        .collect()
}

/// The cells whose literal the catalogue cannot build: Python's `lit` casts the value to the dtype
/// it is given strictly, so a value that does not fit, a text that is no number of the dtype or a
/// text given `Boolean` is refused when the step is built, with Polars' words for the cast, before
/// the walk is asked, and every step that holds it is refused the same. A float with a fraction
/// given to whole numbers is cut, and is no such value.
fn known_casts() -> Vec<Known> {
    vec![
        Known::new(
            Kind::BuildRefusesOnly,
            Pattern::new(),
            "the value does not cast to the dtype it is given: a whole number past its edges, a \
             text that is no number of it, a text given Boolean, a float that overflows it. The \
             step engine refuses the step when it builds the literal, as Python's lit does, and \
             the walk allows it, though it could foretell it by casting the value as the \
             catalogue does (candidate to file)",
        )
        .saying_one_of(&["conversion from `", "casting from ", "cannot cast "]),
    ]
}

fn select(item: Value) -> Value {
    json!({ "exprs": many(vec![item]) })
}

/// A point of a method whose parameter a grid varies: the column, the literal, and which
/// parameter of the method the literal is given as.
#[derive(Default)]
struct Given {
    column: Option<DataType>,
    literal: Chosen,
    method: &'static str,
    parameter: &'static str,
    /// Whether the literal is what the method follows, and the column what it is given.
    first: bool,
}

impl HoldsLiteral for Given {
    fn literal_mut(&mut self) -> &mut Chosen {
        &mut self.literal
    }
}

impl HoldsColumn for Given {
    fn column_mut(&mut self) -> &mut Option<DataType> {
        &mut self.column
    }
}

/// The case of `method` run on the column `x` and given the literal as `parameter`.
fn method_given_the_literal(point: &Given) -> Result<Case, String> {
    let column = point.column.as_ref().expect("a column");
    let item = field_then(
        "x",
        vec![call(
            point.method,
            json!({ point.parameter: point.literal.expression() }),
        )],
    );
    Case::of_step(
        "LazyFrame.select",
        select(item),
        one_column_table(column)?,
        Connections::new(),
    )
}

/// The columns of dates, moments, lengths of time and times of day.
fn moments() -> Vec<DataType> {
    column_dtypes()
        .into_iter()
        .filter(|dtype| {
            matches!(
                dtype,
                DataType::Date | DataType::Datetime(..) | DataType::Duration(_) | DataType::Time
            )
        })
        .collect()
}

#[test]
fn a_number_typed_without_a_dtype_that_meets_a_date_or_a_time_is_refused_as_polars_refuses_it() {
    // Polars gives such a number no dtype beside a date, a moment, a length of time or a time of
    // day, and refuses to fill them with it or to add it to them, whatever the number is.
    let methods: Vec<(String, (&'static str, &'static str))> =
        [("Expr.fill_null", "value"), ("Expr.add", "other")]
            .into_iter()
            .map(|(method, parameter)| (format!("{method}({parameter})"), (method, parameter)))
            .collect();
    let numbers = [
        Written::Integer(-1),
        Written::Integer(1),
        Written::Integer(300),
        Written::Integer(i64::MAX),
        Written::Float(1.0),
        Written::Float(1.5),
        Written::Float(2e38),
    ];
    let grid = Grid::<Given>::new()
        .column_axis(moments())
        .axis("method", methods, |point, (method, parameter)| {
            point.method = method;
            point.parameter = parameter;
        })
        .literal_axes(numbers.to_vec(), &[None]);
    let verdicts = grid.judge(
        &Question::any_refusal().at(Depth::Rows),
        method_given_the_literal,
    );
    verdicts.assert_agree(&[], 50);
}

// ---------------------------------------------------------------------------
// Parts of a moment typed in, in a join's key

/// Which function makes the moment or the length of time, and which of its parts the literal is.
#[derive(Default)]
struct Made {
    function: &'static str,
    literal: Chosen,
}

impl HoldsLiteral for Made {
    fn literal_mut(&mut self) -> &mut Chosen {
        &mut self.literal
    }
}

fn parts_of_a_moment_grid(values: Vec<Written>) -> Verdicts {
    let functions: Vec<(String, &'static str)> =
        ["polars.duration(weeks)", "polars.datetime(year)"]
            .into_iter()
            .map(|function| (function.to_owned(), function))
            .collect();
    let grid = Grid::<Made>::new()
        .axis("function", functions, |point, function| {
            point.function = function;
        })
        .literal_axes(values, &words());
    grid.judge(&Question::any_refusal(), |point| {
        let literal = point.literal.expression();
        let whole = |number: i64| literals::expression(&Written::Integer(number), None);
        let made = if point.function == "polars.duration(weeks)" {
            call("polars.duration", json!({ "weeks": literal }))
        } else {
            call(
                "polars.datetime",
                json!({ "year": literal, "month": whole(1), "day": whole(1) }),
            )
        };
        let working = one_column_table(&DataType::Int64)?;
        // The other table holds a key of the dtype each function makes, so that a call that is not
        // folded is paired with a key of its own dtype.
        let ints = Series::new("ints".into(), [1_i64, 2, 3]);
        let key = |name: &str, dtype: &DataType| {
            ints.cast(dtype)
                .map(|series| series.with_name(name.into()).into())
                .map_err(|error| error.to_string())
        };
        let other = DataFrame::new(
            3,
            vec![
                key("rt", &DataType::Datetime(TimeUnit::Microseconds, None))?,
                key("rd", &DataType::Duration(TimeUnit::Microseconds))?,
            ],
        )
        .map_err(|error| error.to_string())?;
        let right = if point.function == "polars.duration(weeks)" {
            "rd"
        } else {
            "rt"
        };
        Case::of_step(
            "LazyFrame.join",
            json!({
                "other": { "table": "other" },
                "left_on": many(vec![chain(made, Vec::new())]),
                "right_on": many(vec![field(right)]),
            }),
            working,
            Connections::new().table("other", other),
        )
    })
}

#[test]
fn a_moment_or_a_length_of_time_made_of_numbers_typed_in_is_no_join_key_however_they_are_written() {
    // Polars folds the call into a literal where every part is a number, as the step engine makes
    // it of a value given a dtype: a whole float or a text of a number given a dtype of numbers is
    // one. The literal is named by an alias, which a key may not hold, so the join is refused.
    let mut values = literals::edge_integers();
    values.extend(literals::edge_floats());
    values.extend(literals::edge_texts());
    let mut known = known_casts();
    known.push(
        Known::new(
            Kind::WalkRefusesOnly,
            Pattern::new().is("function", "polars.datetime(year)"),
            "a year the calendar cannot hold makes no moment, so Polars keeps the call and does not \
             fold it, which the document does not decide (the walk takes every number as a part \
             that folds)",
        )
        .saying("rule folded-function-is-no-join-key"),
    );
    parts_of_a_moment_grid(values).assert_agree(&known, 500);
}

// ---------------------------------------------------------------------------
// Arithmetic with a number typed without a dtype, on either side

fn arithmetic_grid(values: Vec<Written>) -> Verdicts {
    let methods: Vec<(String, (&'static str, &'static str))> = [
        "Expr.add",
        "Expr.sub",
        "Expr.mul",
        "Expr.truediv",
        "Expr.floordiv",
        "Expr.mod",
    ]
    .into_iter()
    .map(|method| (method.to_owned(), (method, "other")))
    .collect();
    let grid = Grid::<Given>::new()
        .column_axis(moments())
        .axis("method", methods, |point, (method, parameter)| {
            point.method = method;
            point.parameter = parameter;
        })
        .axis("order", orders(), |point, first| point.first = *first)
        .literal_axes(values, &[None]);
    grid.judge(&Question::any_refusal().at(Depth::Rows), |point| {
        if point.first {
            literal_first(point)
        } else {
            method_given_the_literal(point)
        }
    })
}

#[test]
fn a_number_typed_without_a_dtype_beside_a_date_is_refused_in_arithmetic_on_either_side() {
    // Polars gives the number no dtype beside a date, a moment, a length of time or a time of day,
    // and refuses to do arithmetic of them, whichever of the two is given to the other, except
    // where a length of time is multiplied or divided by a number, which it allows.
    let numbers = [
        Written::Integer(-1),
        Written::Integer(1),
        Written::Integer(300),
        Written::Float(1.0),
        Written::Float(1.5),
        Written::Float(2e38),
    ];
    arithmetic_grid(numbers.to_vec()).assert_agree(&[], 100);
}

// ---------------------------------------------------------------------------
// Cuts

/// Which cut, and which of its parameters the literal is given as.
#[derive(Default)]
struct Cut {
    cut: &'static str,
    literal: Chosen,
}

impl HoldsLiteral for Cut {
    fn literal_mut(&mut self) -> &mut Chosen {
        &mut self.literal
    }
}

fn counts_of_a_column() -> Result<polars::prelude::DataFrame, String> {
    one_column_table(&DataType::Int64)
}

/// The methods that cut the values they follow and are given a literal: `head` and `tail` take a
/// count, `slice` a place and a length.
fn cut_grid(values: Vec<Written>) -> Verdicts {
    let cuts: Vec<(String, &'static str)> =
        ["head(n)", "tail(n)", "slice(offset)", "slice(length)"]
            .into_iter()
            .map(|cut| (cut.to_owned(), cut))
            .collect();
    let grid = Grid::<Cut>::new()
        .axis("cut", cuts, |point, cut| point.cut = cut)
        .literal_axes(values, &words());
    // The length a value is cut by is read when the rows run.
    grid.judge(&Question::any_refusal().at(Depth::Rows), |point| {
        let literal = point.literal.expression();
        let (method, arguments) = match point.cut {
            "head(n)" => ("Expr.head", json!({ "n": literal })),
            "tail(n)" => ("Expr.tail", json!({ "n": literal })),
            "slice(offset)" => ("Expr.slice", json!({ "offset": literal })),
            _ => (
                "Expr.slice",
                json!({ "offset": literals::expression(&Written::Integer(1), None), "length": literal }),
            ),
        };
        Case::of_step(
            "LazyFrame.select",
            select(field_then("x", vec![call(method, arguments)])),
            counts_of_a_column()?,
            Connections::new(),
        )
    })
}

/// The cells the walk leaves to Polars, which cannot cut by a number of 128 bits that does not fit
/// 64: the dtype is not read as a number of its own, though a number that fits is.
fn known_wide_cuts() -> Vec<Known> {
    vec![
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new(),
            "Polars cannot extract a length from a value of 128 bits that does not fit 64, which \
             the walk does not read",
        )
        .saying("unable to extract length from UInt128"),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new(),
            "Polars cannot extract a length from the greatest value of 128 bits, which the walk \
             does not read",
        )
        .saying("unable to extract length from Int128(1701"),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new(),
            "Polars cannot extract an offset from a value of 128 bits that does not fit 64, \
             which the walk does not read",
        )
        .saying("unable to extract offset from"),
    ]
}

#[test]
fn a_length_or_a_place_that_cannot_cut_values_is_refused_as_polars_refuses_it() {
    // A number typed without a dtype that has a fraction is no length: Polars reads one as a whole
    // number and refuses what is not. A number below nothing is no length either, whichever
    // dtype it is given and however it is written, as a float or a text of one.
    let mut values = literals::edge_integers();
    values.extend(literals::edge_floats());
    values.extend(literals::edge_texts());
    let mut known = known_casts();
    known.extend(known_wide_cuts());
    cut_grid(values).assert_agree(&known, 1000);
}

// ---------------------------------------------------------------------------
// Whole numbers written as values

/// A whole number written as a value, which the steps that cut a table and `gather_every` take.
#[derive(Default)]
struct Counted {
    step: &'static str,
    number: Option<Written>,
}

fn counted_grid() -> Verdicts {
    let steps: Vec<(String, &'static str)> = [
        "LazyFrame.head(n)",
        "LazyFrame.tail(n)",
        "LazyFrame.slice(offset)",
        "LazyFrame.slice(length)",
        "LazyFrame.slice(offset, length 1)",
        "LazyFrame.gather_every(n)",
        "LazyFrame.gather_every(offset)",
        "Expr.gather_every(n)",
        "Expr.gather_every(offset)",
    ]
    .into_iter()
    .map(|step| (step.to_owned(), step))
    .collect();
    let mut numbers = literals::edge_integers();
    // The places a slice starts at near the last row number Polars counts, 2^63 - 1, and counts
    // near the greatest of 32 bits.
    numbers.extend([
        Written::Integer(i64::MAX - i64::from(u32::MAX)),
        Written::Integer(i64::MAX - i64::from(u32::MAX) + 1),
        Written::Integer(i64::from(u32::MAX)),
        Written::Integer(i64::from(u32::MAX) + 1),
    ]);
    numbers.sort_by_key(Written::label);
    numbers.dedup();
    // The document reads a whole number there and no other kind, so the floats and the texts the
    // other grids cross are not cells of this one.
    let grid = Grid::<Counted>::new()
        .axis("step", steps, |point, step| point.step = step)
        .axis(
            "number",
            numbers
                .into_iter()
                .map(|number| (number.label(), number))
                .collect(),
            |point, number| point.number = Some(number.clone()),
        );
    grid.judge(&Question::any_refusal().at(Depth::Rows), |point| {
        let number = point.number.as_ref().expect("a number").json();
        let (entry, arguments) = match point.step {
            "LazyFrame.head(n)" => ("LazyFrame.head", json!({ "n": number })),
            "LazyFrame.tail(n)" => ("LazyFrame.tail", json!({ "n": number })),
            "LazyFrame.slice(offset)" => ("LazyFrame.slice", json!({ "offset": number })),
            "LazyFrame.slice(offset, length 1)" => (
                "LazyFrame.slice",
                json!({ "offset": number, "length": { "integer": 1 } }),
            ),
            "LazyFrame.slice(length)" => (
                "LazyFrame.slice",
                json!({ "offset": { "integer": 0 }, "length": number }),
            ),
            "LazyFrame.gather_every(n)" => ("LazyFrame.gather_every", json!({ "n": number })),
            "LazyFrame.gather_every(offset)" => (
                "LazyFrame.gather_every",
                json!({ "n": { "integer": 1 }, "offset": number }),
            ),
            "Expr.gather_every(n)" => return gather_every_case(json!({ "n": number })),
            _ => {
                return gather_every_case(json!({ "n": { "integer": 1 }, "offset": number }));
            }
        };
        Case::of_step(entry, arguments, counts_of_a_column()?, Connections::new())
    })
}

fn gather_every_case(arguments: Value) -> Result<Case, String> {
    Case::of_step(
        "LazyFrame.select",
        select(field_then("x", vec![call("Expr.gather_every", arguments)])),
        counts_of_a_column()?,
        Connections::new(),
    )
}

#[test]
fn a_step_that_cuts_a_table_by_a_whole_number_polars_cannot_cut_by_is_refused() {
    // Polars takes every nth row for an n above nothing. A slice that is given no length runs to the
    // end of the table, whatever its place is, and one that is given a length is refused where its
    // place and length together run past the last row number Polars counts, 2^63 - 1.
    let known = vec![
        Known::new(
            Kind::BuildRefusesOnly,
            Pattern::new().starts("number", "-"),
            "a count or a place below nothing is refused by Python's binding too, as an overflow \
             of an unsigned number, which the walk does not read (candidate to file)",
        )
        .saying("takes a whole number from"),
        Known::new(
            Kind::BuildRefusesOnly,
            Pattern::new().is("step", "LazyFrame.tail(n)"),
            "the rows a tail takes are an index, 32 bits wide in the Polars the engine builds and \
             in Python's, which refuses a count above 4294967295 too, as the catalogue does",
        )
        .saying("to 4294967295"),
    ];
    counted_grid().assert_agree(&known, 150);
}

// ---------------------------------------------------------------------------
// The grids of the rules that read a literal

/// How many values a grid crosses: the normal grids run with the tests, the large ones on request.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Scale {
    Normal,
    Large,
}

impl Scale {
    /// How far Polars is asked: the normal grids ask what it says of the schema, and of the rows
    /// where the walk says it refuses on them; the large ones ask the rows of every cell.
    const fn depth(self) -> Depth {
        match self {
            Self::Normal => Depth::Plan,
            Self::Large => Depth::Rows,
        }
    }
}

/// The values a grid writes literals of.
fn values(scale: Scale) -> Vec<Written> {
    let mut values = literals::edge_integers();
    values.extend(literals::edge_floats());
    values.extend(literals::edge_texts());
    values.extend([Written::Flag(true), Written::Flag(false), Written::Null]);
    if scale == Scale::Normal {
        // Every third value keeps the grid to minutes and each of its kinds in it.
        values = values
            .into_iter()
            .enumerate()
            .filter(|(at, _)| at % 3 == 0)
            .map(|(_, value)| value)
            .collect();
    }
    values
}

/// The parameters a method is given, each by its name, as the grid names them on the axis
/// `parameter`.
fn parameters_axis(grid: Grid<Given>, names: &[&'static str]) -> Grid<Given> {
    grid.axis(
        "parameter",
        names
            .iter()
            .map(|name| ((*name).to_owned(), *name))
            .collect(),
        |point, name| point.parameter = name,
    )
}

/// What a method's other parameters are given, so that the one a grid varies is the only one that
/// differs from cell to cell: a literal that is of the dtype of the column.
fn companion(column: &DataType) -> Value {
    let (written, word) = match column {
        DataType::Boolean => (Written::Flag(true), None),
        DataType::String => (Written::Text("1"), None),
        DataType::Float16 | DataType::Float32 | DataType::Float64 => (Written::Float(1.0), None),
        _ => (Written::Integer(1), None),
    };
    literals::expression(&written, word)
}

/// The case of `method` run on the column with each of its parameters given the companion of the
/// column, except `varied`, which is given the cell's literal.
fn method_case(method: &str, names: &[&str], point: &Given) -> Result<Case, String> {
    let column = point.column.as_ref().expect("a column");
    let mut arguments = serde_json::Map::new();
    for name in names {
        let value = if *name == point.parameter {
            point.literal.expression()
        } else if *name == "offset" {
            // The place a text is cut from is a whole number whatever the column is.
            literals::expression(&Written::Integer(0), None)
        } else {
            companion(column)
        };
        // What a method takes any number of is a list.
        let value = if *name == "others" {
            many(vec![value])
        } else {
            value
        };
        arguments.insert((*name).to_owned(), value);
    }
    let item = field_then("x", vec![call(method, Value::Object(arguments))]);
    Case::of_step(
        "LazyFrame.select",
        select(item),
        one_column_table(column)?,
        Connections::new(),
    )
}

fn clip_grid(scale: Scale) -> Verdicts {
    let grid = parameters_axis(
        Grid::<Given>::new().column_axis(column_dtypes()),
        &["lower_bound", "upper_bound"],
    )
    .literal_axes(values(scale), &words());
    grid.judge(&Question::any_refusal().at(scale.depth()), |point| {
        // A clip is given a bound, and the other is left out.
        let column = point.column.as_ref().expect("a column");
        let item = field_then(
            "x",
            vec![call(
                "Expr.clip",
                json!({ point.parameter: point.literal.expression() }),
            )],
        );
        Case::of_step(
            "LazyFrame.select",
            select(item),
            one_column_table(column)?,
            Connections::new(),
        )
    })
}

fn replace_grid(scale: Scale) -> Verdicts {
    let grid = parameters_axis(
        Grid::<Given>::new().column_axis(column_dtypes()),
        &["old", "new"],
    )
    .literal_axes(values(scale), &words());
    grid.judge(&Question::any_refusal().at(scale.depth()), |point| {
        method_case("Expr.replace", &["old", "new"], point)
    })
}

fn replace_strict_grid(scale: Scale) -> Verdicts {
    let grid = parameters_axis(
        Grid::<Given>::new().column_axis(column_dtypes()),
        &["old", "new", "default"],
    )
    .literal_axes(values(scale), &words());
    grid.judge(&Question::any_refusal().at(scale.depth()), |point| {
        method_case("Expr.replace_strict", &["old", "new", "default"], point)
    })
}

fn shift_grid(scale: Scale) -> Verdicts {
    let grid = parameters_axis(
        Grid::<Given>::new().column_axis(column_dtypes()),
        &["fill_value"],
    )
    .literal_axes(values(scale), &words());
    grid.judge(&Question::any_refusal().at(scale.depth()), |point| {
        method_case("Expr.shift", &["fill_value"], point)
    })
}

/// Rules that read a literal by the role of the parameter it is given as, one method of each:
/// what a number is added to, divided by, raised to, a logarithm is taken to, a share a quantile
/// reads, a length a text is padded to.
/// The two orders of the operands of a method that meets the column: the column first, or the
/// literal first.
fn orders() -> Vec<(String, bool)> {
    vec![
        ("the column first".to_owned(), false),
        ("the literal first".to_owned(), true),
    ]
}

/// The case where the literal is what the method follows and the column is what it is given, as
/// `1 + x` is. A method with no operand of the kind to turn round has no such case.
fn literal_first(point: &Given) -> Result<Case, String> {
    let column = point.column.as_ref().expect("a column");
    if !matches!(point.parameter, "other" | "exponent" | "base" | "others") {
        return Err(format!(
            "{} takes no operand that the column can be, beside a literal it follows",
            point.method
        ));
    }
    let operand = if point.parameter == "others" {
        many(vec![field("x")])
    } else {
        field("x")
    };
    let item = chain(
        point.literal.call(),
        vec![call(point.method, json!({ point.parameter: operand }))],
    );
    Case::of_step(
        "LazyFrame.select",
        select(item),
        one_column_table(column)?,
        Connections::new(),
    )
}

fn role_grid(scale: Scale) -> Verdicts {
    let methods: Vec<(String, (&'static str, &'static str))> = [
        ("Expr.add", "other"),
        ("Expr.truediv", "other"),
        ("Expr.floordiv", "other"),
        ("Expr.pow", "exponent"),
        ("Expr.log", "base"),
        ("Expr.quantile", "quantile"),
        ("Expr.is_between", "lower_bound"),
        ("Expr.str.pad_start", "length"),
    ]
    .into_iter()
    .map(|(method, parameter)| (format!("{method}({parameter})"), (method, parameter)))
    .collect();
    let grid = Grid::<Given>::new()
        .column_axis(column_dtypes())
        .axis("method", methods, |point, (method, parameter)| {
            point.method = method;
            point.parameter = parameter;
        })
        .axis("order", orders(), |point, first| point.first = *first)
        .literal_axes(values(scale), &words());
    grid.judge(&Question::any_refusal().at(scale.depth()), |point| {
        if point.first {
            return literal_first(point);
        }
        if point.method == "Expr.str.pad_start" && point.literal.is_at_least(f64::from(1_u32 << 31))
        {
            return Err(
                "Polars never ends: pad_start builds a text of the length it is given".to_owned(),
            );
        }
        let names: &[&str] = match point.method {
            "Expr.is_between" => &["lower_bound", "upper_bound"],
            _ => &[point.parameter],
        };
        method_case(point.method, names, point)
    })
}

/// The methods whose parameter the walk reads by its role and that the grid of the roles does not
/// cross: the value `fill_nan` fills with, the share an approximate quantile reads, how many values
/// `top_k` and `bottom_k` keep, how far `diff`, `pct_change` and `shift` look back, the length
/// `str.slice` cuts, and the operands of `and_` and `or_`.
fn more_roles_grid(scale: Scale) -> Verdicts {
    let methods: Vec<(String, (&'static str, &'static str))> = [
        ("Expr.fill_nan", "value"),
        ("Expr.approx_quantile", "quantile"),
        ("Expr.top_k", "k"),
        ("Expr.bottom_k", "k"),
        ("Expr.diff", "n"),
        ("Expr.pct_change", "n"),
        ("Expr.shift", "n"),
        ("Expr.str.slice", "length"),
        ("Expr.and_", "others"),
        ("Expr.or_", "others"),
    ]
    .into_iter()
    .map(|(method, parameter)| (format!("{method}({parameter})"), (method, parameter)))
    .collect();
    let grid = Grid::<Given>::new()
        .column_axis(column_dtypes())
        .axis("method", methods, |point, (method, parameter)| {
            point.method = method;
            point.parameter = parameter;
        })
        .axis("order", orders(), |point, first| point.first = *first)
        .literal_axes(values(scale), &words());
    grid.judge(&Question::any_refusal().at(scale.depth()), |point| {
        if point.first {
            return literal_first(point);
        }
        let names: &[&str] = match point.method {
            "Expr.str.slice" => &["offset", "length"],
            _ => &[point.parameter],
        };
        method_case(point.method, names, point)
    })
}

fn fill_null_grid(scale: Scale) -> Verdicts {
    let grid = parameters_axis(
        Grid::<Given>::new().column_axis(column_dtypes()),
        &["value"],
    )
    .literal_axes(values(scale), &words());
    grid.judge(&Question::any_refusal().at(scale.depth()), |point| {
        method_case("Expr.fill_null", &["value"], point)
    })
}

// ---------------------------------------------------------------------------
// Comparisons with a number in a filter and a condition

#[derive(Default)]
struct Comparison {
    column: Option<DataType>,
    literal: Chosen,
    method: &'static str,
    form: &'static str,
    swapped: bool,
}

impl HoldsLiteral for Comparison {
    fn literal_mut(&mut self) -> &mut Chosen {
        &mut self.literal
    }
}

impl HoldsColumn for Comparison {
    fn column_mut(&mut self) -> &mut Option<DataType> {
        &mut self.column
    }
}

fn comparison_grid(scale: Scale, methods: &[&'static str]) -> Verdicts {
    let methods: Vec<(String, &'static str)> = methods
        .iter()
        .map(|method| ((*method).to_owned(), *method))
        .collect();
    let grid = Grid::<Comparison>::new()
        .column_axis(column_dtypes())
        .axis("method", methods, |point, method| point.method = method)
        .axis(
            "form",
            vec![
                ("a filter".to_owned(), "filter"),
                ("a condition".to_owned(), "when"),
            ],
            |point, form| point.form = form,
        )
        .axis(
            "order",
            vec![
                ("the column first".to_owned(), false),
                ("the literal first".to_owned(), true),
            ],
            |point, swapped| point.swapped = *swapped,
        )
        .literal_axes(values(scale), &words());
    grid.judge(&Question::any_refusal().at(scale.depth()), |point| {
        let column = point.column.as_ref().expect("a column");
        let predicate = if point.swapped {
            chain(
                point.literal.call(),
                vec![call(point.method, json!({ "other": field("x") }))],
            )
        } else {
            field_then(
                "x",
                vec![call(
                    point.method,
                    json!({ "other": point.literal.expression() }),
                )],
            )
        };
        let (entry, arguments) = if point.form == "filter" {
            (
                "LazyFrame.filter",
                json!({ "predicates": many(vec![predicate]) }),
            )
        } else {
            let one = || literals::expression(&Written::Integer(1), None);
            let item = chain(
                call(
                    "polars.when",
                    json!({ "predicates": many(vec![predicate]) }),
                ),
                vec![
                    call("When.then", json!({ "statement": one() })),
                    call("Then.otherwise", json!({ "statement": one() })),
                ],
            );
            ("LazyFrame.select", select(item))
        };
        Case::of_step(
            entry,
            arguments,
            one_column_table(column)?,
            Connections::new(),
        )
    })
}

// ---------------------------------------------------------------------------
// What the walk leaves to Polars

/// A text of a number, given where Polars casts it to the dtype of what it meets: the table of
/// rows was measured with the text `1d`, which no number is, so the walk refuses at rows what
/// Polars takes.
fn known_numbers_written_as_text() -> Known {
    Known::new(
        Kind::WalkRefusesOnly,
        Pattern::new(),
        "the tables were measured with the text 1d, which is no number, so they refuse on rows a \
         text of a number that Polars casts to the dtype it meets (candidate to file)",
    )
    .saying_one_of(&[
        "tables of Polars at Rows: conversion from `str`",
        "tables of Polars at Rows: cannot unpack series of type `str`",
    ])
}

/// The limits of the rules that read a literal by the role of its parameter. The ones the rows of
/// every cell show, of the large grid, are named only there.
fn known_role_limits(scale: Scale) -> Vec<Known> {
    let mut known = vec![
        Known::new(
            Kind::Unbuildable,
            Pattern::new().is("order", "the literal first"),
            "a method with no operand that the column can be, beside a literal it follows, has no \
             cell with the literal first",
        )
        .saying("takes no operand that the column can be"),
        known_numbers_written_as_text(),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().one_of(
                "method",
                &["Expr.quantile(quantile)", "Expr.approx_quantile(quantile)"],
            ),
            "a share outside 0 to 1 is refused by Polars, and the walk does not read the value of \
             a share (candidate to file)",
        )
        .saying("`quantile` should be between"),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().is("column", "String"),
            "Polars refuses arithmetic of text and a number typed without a dtype, on either side, \
             which the walk leaves to the tables because the kept cases of the fuzz campaign settle \
             such a prefix as one they could not judge (candidate to file)",
        )
        .saying_one_of(&[
            "arithmetic on dtypes str and dyn",
            "arithmetic on dtypes dyn",
            "division with 'String' datatypes",
        ]),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().starts("method", "Expr.str.pad_start"),
            "Polars converts the length of a padding to an unsigned 32 bit number and refuses a \
             number below nothing, which the walk does not read (candidate to file)",
        )
        .saying("failed in column 'scalar'"),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().starts("method", "Expr.pow"),
            "Polars converts the exponent of whole numbers to an unsigned 32 bit number and \
             refuses one below nothing or above it, which the walk does not read (candidate to \
             file)",
        )
        .saying("failed in column 'literal'"),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().starts("method", "Expr.pow"),
            "Polars panics in a debug build where whole numbers raised to a power overflow, the \
             same as a plain overflow of arithmetic, which is no refusal the walk can foretell: \
             the release builds run the same cells without the check",
        )
        .saying("attempt to exponentiate with overflow"),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().starts("method", "Expr.is_between"),
            "the two bounds of an interval meet in one dtype, and a bound typed without a dtype \
             beside one of text is refused by Polars, which the walk, judging each bound apart \
             beside the other it is given, does not say (candidate to file)",
        )
        .saying("cannot compare string with numeric"),
        Known::new(
            Kind::Unbuildable,
            Pattern::new().starts("method", "Expr.str.pad_start"),
            "Polars never ends: it builds a text of the length it is given, so a length above what \
             memory holds is no cell to run (a Polars bug candidate)",
        )
        .saying("Polars never ends"),
    ];
    if scale == Scale::Normal {
        // Only the rows of a cell, and the bounds of an interval that the normal values leave out,
        // show these.
        known.retain(|line| {
            !line.reason.starts_with("Polars converts the exponent")
                && !line.reason.starts_with("Polars panics in a debug build")
                && !line.reason.starts_with("the two bounds of an interval")
        });
    }
    known
}

fn casts_and_numbers_written_as_text() -> Vec<Known> {
    let mut known = known_casts();
    known.push(known_numbers_written_as_text());
    known
}

/// Where Polars casts a bound or a listed value to the dtype of what it follows, strictly, and
/// refuses one that dtype cannot hold when the rows run: a whole number past its edges, a float
/// whose cast overflows or is not finite. The walk reads no value of a bound, which the fuzz
/// target's limit `a-bound-the-dtype-cannot-hold` holds the same refusal of.
fn known_a_value_the_dtype_cannot_hold() -> Known {
    Known::new(
        Kind::PolarsRefusesOnly,
        Pattern::new(),
        "Polars casts a bound, and the values a replacement lists, to the dtype of what they \
         follow strictly and refuses one that dtype cannot hold when the rows run, which the \
         walk does not read (candidate to file)",
    )
    .saying("failed in column 'literal'")
}

/// A null typed with a dtype that a replacement lists as what it replaces.
fn known_a_typed_null_listed_to_replace() -> Known {
    Known::new(
        Kind::WalkRefusesOnly,
        Pattern::new().is("value", "null"),
        "the table of rows refuses a typed null listed as what is replaced for a duplicate, \
         which Polars allows (candidate to file)",
    )
    .saying("must not contain duplicates")
}

/// A null typed with a dtype, which Polars gives a dtype where it meets a date, a moment, a
/// length of time or a time of day, and refuses.
fn known_a_typed_null_beside_a_moment() -> Known {
    Known::new(
        Kind::PolarsRefusesOnly,
        Pattern::new().is("value", "null"),
        "Polars finds no supertype of a date, a moment, a length of time or a time of day and a \
         null given the dtype of a number or of `Boolean`, which the walk leaves not known for a \
         null of another dtype than what it fills, as the kept cases of the fuzz target settle \
         (candidate to file)",
    )
    .saying("failed to determine supertype of")
}

/// A null that the table of the schema refuses a conversion of, where Polars folds it.
fn known_a_null_converted_to_null() -> Known {
    Known::new(
        Kind::WalkRefusesOnly,
        Pattern::new().is("value", "null"),
        "the table of the schema refuses a conversion of a whole number to the dtype Null, \
         which the null given as the fill, the share or the value is, where Polars folds it \
         (candidate to file)",
    )
    .saying("conversion from `i")
}

/// The limits of the grid of the roles the others do not cross.
fn known_more_role_limits() -> Vec<Known> {
    vec![
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().one_of("method", &["Expr.and_(others)", "Expr.or_(others)"]).is("column", "Boolean"),
            "Polars refuses to meet a Boolean with a number typed without a dtype in a bitwise call, \
             on either side, which the walk leaves not known as it does the arithmetic of text \
             (candidate to file)",
        )
        .saying_one_of(&["on Boolean and Unknown(", "and Boolean is not supported"]),
        Known::new(
            Kind::WalkRefusesOnly,
            Pattern::new().starts("method", "Expr.fill_nan"),
            "the walk refuses fill_nan of a date, a moment, a length of time or a time of day filled \
             with a number typed without a dtype at the schema, as no supertype of the two, where \
             Polars gets to the rows and refuses them there, as it refuses any value, for the test of \
             NaN is not supported for them: the verdict is right and its stage is not (the refusal \
             comes from the tables' offer of the call for an argument of no known dtype, and was \
             there before the grids; where the rows are asked too, as the large grid does, both \
             refuse) (candidate to file)",
        )
        .saying("failed to determine supertype of")
        .shared(),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().starts("method", "Expr.fill_nan"),
            "Polars casts the value fill_nan fills with to the dtype of the column and refuses one \
             that dtype cannot hold, which the walk does not read (candidate to file)",
        )
        .saying("failed in column 'scalar'"),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new()
                .one_of("method", &["Expr.and_(others)", "Expr.or_(others)"])
                .is("order", "the literal first"),
            "Polars refuses a bitwise call of a number typed without a dtype that follows a float or a \
             text, which the walk leaves not known (candidate to file)",
        )
        .saying("operation not supported for dtype `f")
        .shared(),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().one_of(
                "method",
                &[
                    "Expr.top_k(k)",
                    "Expr.bottom_k(k)",
                    "Expr.shift(n)",
                    "Expr.diff(n)",
                    "Expr.str.slice(length)",
                ],
            ),
            "Polars reads the count of top_k, bottom_k and shift as a whole number that fits an index \
             and is no null, and casts the count of diff and the length of str.slice to a whole \
             number strictly, when the rows run, and refuses a float, a text, a null or a number \
             beyond them; the walk reads no value of a count (candidate to file)",
        )
        .saying_one_of(&[
            "invalid value of 'k' in top_k",
            "invalid value of 'n' in shift",
            "must not be null",
            "failed in column 'literal'",
        ])
        .shared(),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().is("method", "Expr.pct_change(n)"),
            "Polars negates the count of pct_change, which overflows for the lowest whole number: a \
             check of the debug build, and the release builds run the same cells",
        )
        .saying("attempt to negate with overflow")
        .shared(),
    ]
}

/// The limits of the large grid of the roles that the normal one does not reach.
fn known_large_role_limits() -> Vec<Known> {
    vec![
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new()
                .is("column", "Boolean")
                .is("order", "the literal first")
                .starts("method", "Expr.floordiv"),
            "Polars panics where a number typed without a dtype is floor divided by a Boolean, an \
             implementation error that is no check of the debug build (a Polars bug candidate)",
        )
        .saying("implementation error"),
        Known::new(
            Kind::WalkRefusesOnly,
            Pattern::new().is("column", "Boolean").is("value", "null"),
            "the table of rows measured what a Boolean added to or divided by a number gives, \
             which a null given no dtype does not change in Polars' plan (candidate to file)",
        )
        .saying("the schema says Boolean and the rows give"),
        known_a_null_converted_to_null(),
    ]
}

// ---------------------------------------------------------------------------
// The tests of the normal grids

#[test]
fn the_walk_takes_fill_null_with_a_literal_as_polars_does() {
    fill_null_grid(Scale::Normal).assert_agree(&known_casts(), 5_000);
}

#[test]
fn the_walk_takes_clip_with_a_literal_bound_as_polars_does() {
    clip_grid(Scale::Normal).assert_agree(&casts_and_numbers_written_as_text(), 9_000);
}

#[test]
fn the_walk_takes_replace_with_literal_values_as_polars_does() {
    replace_grid(Scale::Normal).assert_agree(&casts_and_numbers_written_as_text(), 9_000);
}

#[test]
fn the_walk_takes_replace_strict_with_literal_values_as_polars_does() {
    replace_strict_grid(Scale::Normal).assert_agree(&casts_and_numbers_written_as_text(), 9_000);
}

#[test]
fn the_walk_takes_shift_with_a_literal_fill_value_as_polars_does() {
    shift_grid(Scale::Normal).assert_agree(&known_casts(), 5_000);
}

#[test]
fn the_walk_takes_a_literal_by_the_role_of_its_parameter_as_polars_does() {
    let mut known = known_casts();
    known.extend(known_role_limits(Scale::Normal));
    role_grid(Scale::Normal).assert_agree(&known, 20_000);
}

#[test]
fn the_walk_takes_a_comparison_with_a_literal_in_a_filter_and_a_condition_as_polars_does() {
    comparison_grid(Scale::Normal, &["Expr.eq", "Expr.gt", "Expr.eq_missing"])
        .assert_agree(&known_casts(), 40_000);
}

#[test]
fn the_walk_takes_the_other_literal_roles_as_polars_does() {
    let mut known = known_casts();
    known.extend(
        known_role_limits(Scale::Normal)
            .into_iter()
            .map(Known::shared),
    );
    known.extend(known_more_role_limits());
    more_roles_grid(Scale::Normal).assert_agree(&known, 5_000);
}

// ---------------------------------------------------------------------------
// The large grids

// Every value, asked the rows of every cell. Run on request:
//
// cargo test --offline --locked -p dageist-engine --test document literal_grid \
//     -- --ignored --nocapture --test-threads 1

#[test]
#[ignore = "the large grid, run on request"]
fn the_large_grid_of_fill_null_with_a_literal_is_taken_as_polars_does() {
    let mut known = known_casts();
    known.push(known_a_typed_null_beside_a_moment());
    fill_null_grid(Scale::Large).assert_agree(&known, 15_000);
}

#[test]
#[ignore = "the large grid, run on request"]
fn the_large_grid_of_clip_with_a_literal_bound_is_taken_as_polars_does() {
    let mut known = casts_and_numbers_written_as_text();
    known.push(known_a_value_the_dtype_cannot_hold());
    clip_grid(Scale::Large).assert_agree(&known, 30_000);
}

#[test]
#[ignore = "the large grid, run on request"]
fn the_large_grid_of_replace_with_literal_values_is_taken_as_polars_does() {
    let mut known = casts_and_numbers_written_as_text();
    known.push(known_a_value_the_dtype_cannot_hold());
    known.push(known_a_typed_null_listed_to_replace());
    replace_grid(Scale::Large).assert_agree(&known, 30_000);
}

#[test]
#[ignore = "the large grid, run on request"]
fn the_large_grid_of_replace_strict_with_literal_values_is_taken_as_polars_does() {
    let mut known = casts_and_numbers_written_as_text();
    known.push(known_a_value_the_dtype_cannot_hold());
    known.push(known_a_typed_null_listed_to_replace());
    replace_strict_grid(Scale::Large).assert_agree(&known, 40_000);
}

#[test]
#[ignore = "the large grid, run on request"]
fn the_large_grid_of_shift_with_a_literal_fill_value_is_taken_as_polars_does() {
    let mut known = known_casts();
    known.push(known_a_null_converted_to_null());
    known.push(Known::new(
        Kind::PolarsRefusesOnly,
        Pattern::new().one_of("column", &["Date", "Datetime(time_unit='us', time_zone=None)", "Duration(time_unit='us')", "Time"]),
        "Polars panics when the rows run, in a debug build, where a date, a moment, a length of \
         time or a time of day is shifted and filled with a number typed without a dtype: it \
         builds the fill as a number of the dtype of the number and meets the dtype of the column: \
         a check of the debug build, and the release builds run the same cells",
    )
    .saying("should not fail: SchemaMismatch"));
    shift_grid(Scale::Large).assert_agree(&known, 15_000);
}

#[test]
#[ignore = "the large grid, run on request"]
fn the_large_grid_of_a_literal_by_the_role_of_its_parameter_is_taken_as_polars_does() {
    let mut known = known_casts();
    known.extend(known_role_limits(Scale::Large));
    known.extend(known_large_role_limits());
    role_grid(Scale::Large).assert_agree(&known, 50_000);
}

#[test]
#[ignore = "the large grid, run on request"]
fn the_large_grid_of_a_comparison_with_a_literal_is_taken_as_polars_does() {
    let mut known = known_casts();
    known.push(
        Known::new(
            Kind::WalkRefusesOnly,
            Pattern::new().is("column", "String").is("value", "null"),
            "the table of rows measured text compared with a number, which Polars folds where the \
             number is a null given a dtype (candidate to file)",
        )
        .saying("cannot compare string with numeric"),
    );
    known.push(
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().is("column", "String").is("dtype", "none"),
            "Polars refuses to compare text with a number typed without a dtype when the rows run, \
             which the walk leaves to the tables as it does the arithmetic of text with one \
             (candidate to file)",
        )
        .saying("cannot compare string with numeric"),
    );
    comparison_grid(
        Scale::Large,
        &[
            "Expr.eq",
            "Expr.ne",
            "Expr.gt",
            "Expr.ge",
            "Expr.lt",
            "Expr.le",
            "Expr.eq_missing",
            "Expr.ne_missing",
        ],
    )
    .assert_agree(&known, 200_000);
}

#[test]
#[ignore = "the large grid, run on request"]
fn the_large_grid_of_the_other_literal_roles_is_taken_as_polars_does() {
    let mut known = known_casts();
    known.extend(
        known_role_limits(Scale::Large)
            .into_iter()
            .map(Known::shared),
    );
    known.extend(known_large_role_limits().into_iter().map(Known::shared));
    known.extend(known_more_role_limits());
    more_roles_grid(Scale::Large).assert_agree(&known, 100_000);
}
