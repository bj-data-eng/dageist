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
//! The grids are built with [`crate::grid`], which says how they are written.

use polars::prelude::DataType;
use serde_json::{Value, json};

use crate::grid::literals::{self, Written};
use crate::grid::{
    Case, Chosen, Connections, Depth, Grid, HoldsColumn, HoldsLiteral, Kind, Known, Pattern,
    Question, Verdicts, column_dtypes, one_column_table,
};
use crate::support::{call, field_then, many};

/// The dtypes a literal is given, or none.
fn words() -> Vec<Option<&'static str>> {
    std::iter::once(None)
        .chain(literals::DTYPES.iter().copied().map(Some))
        .collect()
}

/// The cells whose literal the catalogue cannot build: Python's `lit` casts the value to the dtype
/// it is given strictly, so a value that does not fit, a text that is no number of the dtype or
/// a float with a fraction given to whole numbers is refused when the step is built, with
/// Polars' words for the cast, before the walk is asked, and every step that holds it is
/// refused the same.
fn known_casts() -> Vec<Known> {
    vec![
        Known::new(
            Kind::SetAside,
            Pattern::new(),
            "the value does not cast to the dtype it is given, and the step engine refuses the \
             step when it builds the literal",
        )
        .saying("refused when the step is built"),
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
