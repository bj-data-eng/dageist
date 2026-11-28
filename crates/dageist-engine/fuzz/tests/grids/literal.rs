//! The rules that read a literal's value or dtype, each crossed with every literal a document can
//! write and every class of column it meets, with Polars as the judge.
//!
//! A rule reads a literal where Polars treats a number typed without a dtype differently from a
//! column or a typed literal: it adapts to the dtype of the value it meets, or is cast to it
//! strictly, or is read as a count or as a share. The literals worth crossing are those at the
//! edges of the classes, and those written as another kind than the dtype they are given, which
//! the step engine casts strictly before Polars sees them.
//!
//! Every axis is the catalogue's or the world's: the parameters are every expression parameter of
//! a method that reads one by its role ([`axes::literal_parameters`]), each given the literal on
//! either side of the column ([`axes::orders`]); the columns are one of each class the tables
//! measure; the literals are every value at the edges of every class ([`literals::every_value`])
//! with every dtype a literal is given ([`literals::dtype_words`]); a quick run keeps one value in
//! several of each kind and one dtype of each kind of class. The grids of the literals by
//! their role and of the comparisons ask Polars of the schema, and of the rows where the walk
//! refuses; two more grids ask the rows of every cell, where a count or a length is read and where
//! a number meets a moment. The grids of the literals by their role and of the comparisons are
//! asked the rows of every cell too, on request:
//!
//! ```sh
//! cargo test --offline --locked -p dageist-engine-fuzz --test grids literal::at_the_rows \
//!     -- --ignored --nocapture --test-threads 1
//! ```
//!
//! A line of a test that names cells says why the walk and Polars part there; a reason that ends in
//! an issue is a difference this repository has not settled, and one that names a Polars bug
//! candidate is Polars' own.

use dageist_engine::allowed::{Context, Nested, Outer, Representative};
use dageist_engine::catalog::{self, Applies, Family, Kind as ParameterKind, ValueType};
use dageist_engine_fuzz::axes::{self, Param};
use dageist_engine_fuzz::doc::{call, chain, column_then, many};
use dageist_engine_fuzz::judge::{Case, Connections, Depth, Kind, Question};
use dageist_engine_fuzz::literals::{self, Literal, Written};
use dageist_engine_fuzz::places;
use dageist_engine_fuzz::world::{Column, World};
use dageist_fuzz::grid::{Choices, Grid, Known, Pattern, Scale, Verdicts};
use serde_json::{Map, Value, json};

/// One column of each class the tables measure. A class they read as another, a list or an array
/// of another element, is left out: the grid holds the column of the class that stands for it.
fn columns() -> Choices<Column> {
    World::get().classes("column").keep_unless(|_, column| {
        let class = column.class.expect("a column of a class");
        (class.sibling() != class).then(|| {
            format!(
                "the tables read it as {}, whose column the grid holds",
                class.sibling()
            )
        })
    })
}

/// The kind of class of a column.
fn kind_of(column: &Column) -> &'static str {
    axes::kind_of_class(column.class.expect("a column of a class"))
}

/// A cell of a method given a literal: the column, which parameter the literal is given as, on
/// which side of the column, and the literal.
#[derive(Default)]
struct Given {
    column: Option<Column>,
    param: Option<Param>,
    first: bool,
    literal: Literal,
}

/// The item of a select that is a method given a literal, with the column first, or with the
/// literal first and the column given as the parameter.
fn given_item(point: &Given) -> Result<Value, String> {
    axes::method_given(
        point.param.as_ref().expect("a parameter"),
        &point.literal,
        point.column.as_ref().expect("a column"),
        point.first,
    )
}

fn given_case(point: &Given) -> Result<Case, String> {
    let column = point.column.as_ref().expect("a column");
    Case::of_step(
        "LazyFrame.select",
        json!({ "exprs": many(vec![given_item(point)?]) }),
        World::get().of(&[&column.label]),
        Connections::new(),
    )
}

fn given_grid(
    name: &'static str,
    columns: Choices<Column>,
    parameters: Choices<Param>,
    values: Choices<Written>,
    words: Choices<Option<String>>,
) -> Grid<Given> {
    Grid::<Given>::new(name)
        .axis(columns, |point, column| point.column = Some(column.clone()))
        .axis(parameters, |point, param| point.param = Some(*param))
        .axis(axes::orders("order"), |point, first| point.first = *first)
        .axis(values, |point, value| {
            point.literal.value = Some(value.clone());
        })
        .axis(words, |point, word| point.literal.word.clone_from(word))
}

/// The grid of the literals by their role, asked Polars as far as `depth`.
fn literal_grid(depth: Depth) -> Verdicts<dageist_engine_fuzz::judge::Outcome> {
    let grid = given_grid(
        if depth == Depth::Rows {
            "literal-at-the-rows"
        } else {
            "literal"
        },
        columns(),
        axes::literal_parameters("parameter"),
        literals::values_of_run(8),
        literals::dtype_words_of_run(),
    );
    let question = Question::any_refusal().at(depth);
    grid.judge(|point| question.judge(given_case(point)))
}

/// The parameters that are a count or a length, which Polars reads when the rows run.
fn counts() -> Choices<Param> {
    axes::literal_parameters("parameter").keep_unless(|_, param| {
        (!param.role().is_a_count()).then(|| "Polars reads it before the rows run".to_owned())
    })
}

/// The grid of the counts and the lengths, asked the rows of every cell. A count is read the same
/// after a column of any class, so the column is the one of 64-bit whole numbers.
fn count_grid() -> Verdicts<dageist_engine_fuzz::judge::Outcome> {
    let columns = columns().keep_unless(|label, _| {
        (label != "Int64").then(|| {
            "a count is read the same after any column, and Int64 stands for them".to_owned()
        })
    });
    let grid = given_grid(
        "literal-counts",
        columns,
        counts(),
        literals::values_of_run(4),
        literals::dtype_words_of_run(),
    );
    let question = Question::any_refusal().at(Depth::Rows);
    grid.judge(|point| question.judge(given_case(point)))
}

/// The grid of a number typed without a dtype beside a date, a moment, a length of time or a time
/// of day, asked the rows of every cell.
fn moment_grid() -> Verdicts<dageist_engine_fuzz::judge::Outcome> {
    let columns = columns().keep_unless(|_, column| {
        (kind_of(column) != "moment").then(|| "no date, moment, length of time or time".to_owned())
    });
    let words = literals::dtype_words().keep_unless(|label, _| {
        (label != "none").then(|| {
            "a number given a dtype is one Polars gives that dtype; only one typed without a \
             dtype is given none beside a moment"
                .to_owned()
        })
    });
    let grid = given_grid(
        "literal-moments",
        columns,
        axes::literal_parameters("parameter"),
        literals::edge_integers()
            .and(literals::edge_floats())
            .thin(Scale::of_run(), 8),
        words,
    );
    let question = Question::any_refusal().at(Depth::Rows);
    grid.judge(|point| question.judge(given_case(point)))
}

// ---------------------------------------------------------------------------
// Comparisons with a literal where a predicate stands

/// A comparison with a literal and where it stands.
#[derive(Default)]
struct Compared {
    given: Given,
    place: Option<Context>,
}

/// The places that read a predicate: a filter, a group's `having` and a condition's predicate in a
/// select.
fn predicate_places() -> Choices<Context> {
    places::places("place").keep_unless(|_, context| {
        let reads = matches!(
            (context.outer(), context.nested()),
            (Outer::Filter | Outer::Having, None)
                | (Outer::Select, Some(Nested::ConditionPredicate))
        );
        (!reads).then(|| "it reads no predicate of its own".to_owned())
    })
}

/// The parameters of the comparisons.
fn comparisons() -> Choices<Param> {
    axes::literal_parameters("parameter").keep_unless(|_, param| {
        (param.entry.family() != Family::Compare).then(|| "it is no comparison".to_owned())
    })
}

/// The grid of the comparisons with a literal, asked Polars as far as `depth`.
fn comparison_grid(depth: Depth) -> Verdicts<dageist_engine_fuzz::judge::Outcome> {
    let grid = Grid::<Compared>::new(if depth == Depth::Rows {
        "literal-comparisons-at-the-rows"
    } else {
        "literal-comparisons"
    })
    .axis(predicate_places(), |point, place| {
        point.place = Some(*place)
    })
    .axis(columns(), |point, column| {
        point.given.column = Some(column.clone())
    })
    .axis(comparisons(), |point, param| {
        point.given.param = Some(*param)
    })
    .axis(axes::orders("order"), |point, first| {
        point.given.first = *first
    })
    .axis(literals::values_of_run(12), |point, value| {
        point.given.literal.value = Some(value.clone());
    })
    .axis(literals::dtype_words_of_run(), |point, word| {
        point.given.literal.word.clone_from(word)
    });
    let question = Question::any_refusal().at(depth);
    grid.judge(|point| {
        let column = point.given.column.as_ref().expect("a column");
        question.judge(given_item(&point.given).and_then(|item| {
            places::at(
                point.place.expect("a place"),
                item,
                World::get().of(&[&column.label]),
            )
        }))
    })
}

// ---------------------------------------------------------------------------
// Whole numbers typed into a parameter that is no expression

/// A whole number typed into a parameter that takes one, of a step or of a method.
#[derive(Default)]
struct Counted {
    param: Option<Param>,
    number: Option<Written>,
}

/// Every parameter of a step of the list or of a method of an expression that takes a whole
/// number typed in, and no expression.
fn integer_parameters() -> Choices<Param> {
    let mut axis = Choices::new("parameter");
    for entry in catalog::entries() {
        for parameter in entry.parameters() {
            if parameter.kind() != ParameterKind::Value(ValueType::Integer) {
                continue;
            }
            let param = Param { entry, parameter };
            axis = match entry.applies() {
                Applies::Frame | Applies::Expression => axis.with(param.label(), param),
                _ => axis.leave_out(
                    param.label(),
                    "a function or a call that follows a group by is no step and no method the \
                     world's table is given to",
                ),
            };
        }
    }
    axis
}

fn counted_case(point: &Counted) -> Result<Case, String> {
    let param = point.param.as_ref().expect("a parameter");
    let number = point.number.as_ref().expect("a number");
    // A call that gives a struct of text makes a field for each piece it is told to make.
    if number.is_at_least(1e6)
        && axes::gives_after(param.entry, Representative::String) == Some(Representative::Struct)
    {
        return Err(
            "Polars never ends: it makes a struct of as many fields as it is given".to_owned(),
        );
    }
    let column = World::get().column("Int64");
    let mut arguments = Map::new();
    for parameter in param.entry.parameters() {
        if parameter.name() == param.name() {
            arguments.insert(parameter.name().to_owned(), number.json());
        } else if parameter.is_needed() {
            arguments.insert(
                parameter.name().to_owned(),
                axes::filler(parameter, Some(column))?,
            );
        }
    }
    let working = World::get().of(&["Int64"]);
    match param.entry.applies() {
        Applies::Frame => Case::of_step(
            param.method(),
            Value::Object(arguments),
            working,
            Connections::new(),
        ),
        _ => Case::of_step(
            "LazyFrame.select",
            json!({ "exprs": many(vec![column_then(&column.name, vec![call(param.method(), Value::Object(arguments))])]) }),
            working,
            Connections::new(),
        ),
    }
}

fn counted_grid() -> Verdicts<dageist_engine_fuzz::judge::Outcome> {
    // The document reads a whole number there and no other kind, so the floats and the texts the
    // other grids cross are not cells of this one.
    let grid = Grid::<Counted>::new("literal-integers")
        .axis(integer_parameters(), |point, param| {
            point.param = Some(*param)
        })
        .axis(
            literals::edge_integers().named("number"),
            |point, number| {
                point.number = Some(number.clone());
            },
        );
    let question = Question::any_refusal().at(Depth::Rows);
    grid.judge(|point| question.judge(counted_case(point)))
}

// ---------------------------------------------------------------------------
// The parts of a function typed in, in a join's key

/// A function given the literal as one of its parts, as a join's key.
#[derive(Default)]
struct Made {
    param: Option<Param>,
    literal: Literal,
}

/// Every expression parameter of a function that starts an expression.
fn function_parts() -> Choices<Param> {
    let mut axis = Choices::new("part");
    for entry in catalog::entries()
        .iter()
        .filter(|entry| entry.applies() == Applies::Function)
    {
        for parameter in entry.parameters() {
            if matches!(parameter.kind(), ParameterKind::Expression(_)) {
                let param = Param { entry, parameter };
                axis = axis.with(param.label(), param);
            }
        }
    }
    axis
}

fn made_case(point: &Made) -> Result<Case, String> {
    let param = point.param.as_ref().expect("a part");
    let mut arguments = Map::new();
    for parameter in param.entry.parameters() {
        if parameter.name() == param.name() {
            let literal = point.literal.expression();
            let literal = match parameter.shape() {
                catalog::Shape::Many => many(vec![literal]),
                _ => literal,
            };
            arguments.insert(parameter.name().to_owned(), literal);
        } else if parameter.is_needed() {
            arguments.insert(parameter.name().to_owned(), axes::filler(parameter, None)?);
        }
    }
    let made = chain(call(param.method(), Value::Object(arguments)), Vec::new());
    places::at(Context::step(Outer::JoinLeft), made, World::get().of(&[]))
}

fn function_parts_grid() -> Verdicts<dageist_engine_fuzz::judge::Outcome> {
    let grid = Grid::<Made>::new("literal-function-parts")
        .axis(function_parts(), |point, param| point.param = Some(*param))
        .axis(literals::values_of_run(6), |point, value| {
            point.literal.value = Some(value.clone())
        })
        .axis(literals::dtype_words_of_run(), |point, word| {
            point.literal.word.clone_from(word)
        });
    let question = Question::any_refusal();
    grid.judge(|point| question.judge(made_case(point)))
}

// ---------------------------------------------------------------------------
// The lines

/// The cells whose literal the catalogue cannot build: Python's `lit` casts the value to the dtype
/// it is given strictly, so a value that does not fit, a text that is no number of the dtype or a
/// text given `Boolean` is refused when the step is built, with Polars' words for the cast, before
/// the walk is asked, and every step that holds it is refused the same. A float with a fraction
/// given to whole numbers is cut, and is no such value.
fn known_casts() -> Vec<Known<Kind>> {
    vec![
        Known::new(
            Kind::BuildRefusesOnly,
            Pattern::new(),
            "the value does not cast to the dtype it is given: a whole number past its edges, a \
             text that is no number of it, a text given Boolean, a float that overflows it. The \
             step engine refuses the step when it builds the literal, as Python's lit does, and \
             the walk allows it, though it could foretell it by casting the value as the \
             catalogue does (#225)",
        )
        .saying_one_of(&["conversion from `", "casting from ", "cannot cast "]),
    ]
}

/// The whole numbers typed in that the catalogue, Polars or the rows refuse and the walk does not
/// read.
fn known_integers() -> Vec<Known<Kind>> {
    vec![
        Known::new(
            Kind::BuildRefusesOnly,
            Pattern::new(),
            "the catalogue holds a whole number typed in to the range the parameter of Python's \
             binding takes, a count or a place below nothing or past an index of 32 bits, and \
             refuses one outside it when it builds the step, as Python does; the walk reads no \
             value typed in (#259)",
        )
        .saying("takes a whole number from"),
        Known::new(
            Kind::BuildRefusesOnly,
            Pattern::new().is("parameter", "Expr.fill_null(limit)"),
            "fill_null takes a limit only beside a strategy, which the grid does not choose: the \
             catalogue refuses the limit alone when it builds the step, and the walk allows it \
             (#259)",
        )
        .saying("can only specify `limit` when strategy"),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().is("parameter", "Expr.round_sig_figs(digits)"),
            "Polars rounds to one significant figure at least and refuses fewer when the rows \
             run, where the catalogue takes any whole number of 32 bits and the walk allows \
             it (#259)",
        )
        .saying("digits must be an integer >="),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().is("parameter", "LazyFrame.with_row_index(offset)"),
            "the index of the last row passes the largest index of 32 bits where the offset and \
             the rows of the table do together, which the rows decide and the walk cannot know",
        )
        .saying("overflows on dataframe with height"),
        Known::new(
            Kind::Unbuildable,
            Pattern::new(),
            "a call that splits text into a struct makes a field for each piece it is told to \
             make, so a count above what memory holds is no cell to run",
        )
        .saying("Polars never ends: it makes a struct"),
    ]
}

/// The cells of a function of numbers typed in, as a join's key, where the walk and Polars part.
fn known_function_parts() -> Vec<Known<Kind>> {
    vec![
        Known::new(
            Kind::WalkRefusesOnly,
            Pattern::new().starts("part", "polars.datetime("),
            "a part of a moment past what the calendar holds makes no moment, so Polars keeps \
             the call and does not fold it, which the document does not decide: the walk takes \
             every number typed in as a part that folds",
        )
        .saying("rule folded-function-is-no-join-key"),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().starts("part", "polars.datetime("),
            "Polars casts a text of a number given String, and a flag, to the part it stands for \
             and folds the call, which a join's key may not hold; the walk takes only a number \
             typed in as a part that folds (#259)",
        )
        .saying("'alias' is not allowed in a join key"),
        Known::new(
            Kind::WalkPanicked,
            Pattern::new().contains_one_of("part", ["_range("]),
            "the walk panics in a debug build where it works out the count of a range of dates, \
             moments or times whose bound is a number with a fraction past what a whole number of \
             64 bits holds, which overflows its arithmetic (#260)",
        )
        .saying_one_of(&[
            "attempt to subtract with overflow",
            "attempt to negate with overflow",
        ]),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().starts("part", "polars.datetime("),
            "Polars panics in a debug build where the parts of a moment overflow when they are \
             made into its count of the unit: a check of the debug build, and the release builds \
             run the same cells",
        )
        .saying("attempt to multiply with overflow"),
    ]
}

/// A count or a length typed in, which Polars reads when the rows run and the walk does not read.
fn known_counts() -> Vec<Known<Kind>> {
    vec![
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().is("order", "the column first"),
            "Polars reads the count of top_k, bottom_k and shift as a whole number that fits an \
             index and is no null, and casts the count of diff and the length of a slice to a \
             whole number strictly, when the rows run, and refuses a float, a text, a null or a \
             number beyond them; the walk reads no value of a count (#229)",
        )
        .saying_one_of(&[
            "invalid value of 'k' in top_k",
            "invalid value of 'n' in shift",
            "must not be null",
            "failed in column 'literal'",
        ]),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().is("order", "the column first"),
            "Polars cannot extract a length or an offset from a null, or from a value of 128 bits \
             that does not fit 64, which the walk does not read (#230)",
        )
        .saying("unable to extract"),
        Known::new(
            Kind::WalkRefusesOnly,
            Pattern::new().is("value", "null"),
            "the table of the schema refuses a conversion of a whole number to the dtype Null, \
             which the null given as the count, the fill, the share or the value is, where Polars \
             folds it (#228)",
        )
        .saying("to `null` failed"),
        Known::new(
            Kind::Unbuildable,
            Pattern::new(),
            "Polars never ends: it pads a text to the length it is given, so a length of millions \
             of characters is no cell to run within a cell's deadline",
        )
        .saying("Polars never ends: it pads"),
    ]
}

/// The overflows of the arithmetic Polars does with a value typed in, which a debug build checks
/// and panics on, and the release builds the application runs do not.
fn known_overflows() -> Vec<Known<Kind>> {
    vec![
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new(),
            "Polars panics in a debug build where the arithmetic it does with a value typed in \
             overflows: a check that only a debug build makes, which a release build does not",
        )
        .saying_one_of(&[
            "attempt to negate with overflow",
            "attempt to multiply with overflow",
            "attempt to exponentiate with overflow",
            "attempt to add with overflow",
            "attempt to subtract with overflow",
        ]),
    ]
}

/// The column given as a parameter that Polars reads as one value, where the call follows a
/// literal: Polars refuses it, and the walk allows it.
fn known_one_value_given_the_column() -> Known<Kind> {
    Known::new(
        Kind::PolarsRefusesOnly,
        Pattern::new().is("order", "the literal first"),
        "the walk does not refuse the column given as a parameter Polars reads as one value, a \
         count, a fill, a share or a place, where the call follows a literal, though it refuses \
         it after a column (#252)",
    )
    .saying_one_of(&["must be a scalar value", "should be a scalar"])
}

/// A number typed without a dtype beside a date, a moment, a length of time or a time of day.
fn known_moments() -> Vec<Known<Kind>> {
    vec![
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new()
                .one_of(
                    "column",
                    [
                        "Date",
                        "Datetime(us)",
                        "Datetime(us, UTC)",
                        "Duration(us)",
                        "Time",
                    ],
                )
                .is("parameter", "Expr.shift(n)"),
            "Polars panics when the rows run where a date, a moment, a length of time or a time \
             of day is shifted and filled with a whole number typed without a dtype, which the \
             grid gives shift as the fill beside its count (a Polars bug candidate)",
        )
        .saying("should not fail: SchemaMismatch"),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().one_of(
                "parameter",
                [
                    "Expr.clip(lower_bound)",
                    "Expr.clip(upper_bound)",
                    "Expr.replace(old)",
                    "Expr.replace(new)",
                    "Expr.replace_strict(old)",
                    "Expr.fill_nan(value)",
                ],
            ),
            "Polars casts a bound, the values a replacement lists and the value fill_nan fills \
             with to the dtype of what they follow strictly and refuses one that dtype cannot \
             hold when the rows run, which the walk does not read (#225)",
        )
        .saying("failed in column 'literal'"),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().one_of(
                "parameter",
                ["Expr.quantile(quantile)", "Expr.approx_quantile(quantile)"],
            ),
            "a share outside 0 to 1 is refused by Polars, and the walk does not read the value of \
             a share (#224)",
        )
        .saying("`quantile` should be between"),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new()
                .one_of(
                    "parameter",
                    ["Expr.quantile(quantile)", "Expr.approx_quantile(quantile)"],
                )
                .is("order", "the literal first"),
            "Polars reads a share of numbers only, and refuses a date, a moment, a length of time \
             or a time of day given as one, which the walk allows (#252)",
        )
        .saying("expected expression of dtype 'numeric' for quantile"),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new()
                .one_of(
                    "parameter",
                    ["Expr.replace(new)", "Expr.replace_strict(new)"],
                )
                .is("order", "the literal first"),
            "a replacement that follows a literal and is given the column as its new values, of \
             another number than its one old value, is refused by Polars where the column is of \
             dates, moments, lengths of time or times of day; the walk refuses it for every other \
             column (#252)",
        )
        .saying("`new` input for `replace` must have the same length"),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().one_of(
                "parameter",
                ["Expr.is_close(other)", "Expr.fill_null(value)"],
            ),
            "Polars finds no dtype for a number typed without a dtype beside a date, a moment, a \
             length of time or a time of day in is_close on either side, or in fill_null of the \
             number given the column, which the walk allows (#227)",
        )
        .saying("got invalid or ambiguous dtypes"),
    ]
}

fn known_texts_measured_as_1d() -> Known<Kind> {
    Known::new(
        Kind::WalkRefusesOnly,
        Pattern::new(),
        "the tables were measured with the text 1d, which is no number, so they refuse on rows \
         a text of a number that Polars casts to the dtype it meets (#226)",
    )
    .saying_one_of(&[
        "tables of Polars at Rows: conversion from `str`",
        "tables of Polars at Rows: cannot unpack series of type `str`",
    ])
}

fn known_literal_cast_as_one_value() -> Known<Kind> {
    Known::new(
        Kind::WalkRefusesOnly,
        Pattern::new(),
        "a literal is one value, which Polars casts or broadcasts as it does not a column, and \
         the tables measure the argument as a column of its dtype: a flag met bit by bit with a \
         list or an array, a text read as a bound of an interval of moments, the one new value \
         of a strict replacement of lists (#258)",
    )
    .saying_one_of(&[
        "`bitand` operation not supported for dtype",
        "tables of Polars at Schema: cannot compare 'date/datetime/time' to a string value",
        "tables of Polars at Schema: lengths don't match: 'new' must be a scalar value",
    ])
}

fn known_nulls_met_by_a_flag() -> Known<Kind> {
    Known::new(
        Kind::WalkRefusesOnly,
        Pattern::new().is("column", "Null"),
        "the table of rows measured what a Boolean added to or divided by a number gives, \
         which a column of nulls met by a typed flag does not change in Polars' plan \
         (#258)",
    )
    .saying("the schema says Boolean and the rows give")
}

fn known_shift_filled_with_another_dtype() -> Known<Kind> {
    Known::new(
        Kind::WalkRefusesOnly,
        Pattern::new(),
        "the table of rows measured a shift filled with a value of another dtype than what it \
         shifts, on which Polars panics there, where Polars casts a typed literal to the dtype \
         of what it fills (#258)",
    )
    .saying("tables of Polars at Rows: Polars panicked: should not fail: SchemaMismatch")
}

/// A literal the walk judges as a column of its dtype, which Polars casts or broadcasts as one
/// value: the tables measure an argument as a column of the dtype, and were measured with the text
/// `1d`, which is no number.
fn known_literals_are_one_value() -> Vec<Known<Kind>> {
    vec![
        known_texts_measured_as_1d(),
        known_literal_cast_as_one_value(),
        known_nulls_met_by_a_flag(),
        known_shift_filled_with_another_dtype(),
    ]
}

/// An exponent of whole numbers that does not fit an unsigned number of 32 bits, which only the
/// rows show.
fn known_exponents() -> Known<Kind> {
    Known::new(
        Kind::PolarsRefusesOnly,
        Pattern::new().is("parameter", "Expr.pow(exponent)"),
        "Polars converts the exponent of whole numbers to an unsigned 32 bit number when the rows \
         run and refuses one below nothing or above it, which the walk does not read (#224)",
    )
    .saying("failed in column 'literal'")
}

/// A refusal the walk gives at the schema that Polars gives only when the rows run: the verdict is
/// the same, and its stage is not.
fn known_stages() -> Vec<Known<Kind>> {
    vec![
        Known::new(
            Kind::WalkRefusesOnly,
            Pattern::new().is("parameter", "Expr.fill_nan(value)"),
            "the walk refuses fill_nan of a date, a moment, a length of time, a time of day, a \
             list or an array filled with a number typed without a dtype at the schema, as no \
             supertype of the two, where Polars refuses it only when the rows run, for its test \
             of NaN takes none of them (#231)",
        )
        .saying("failed to determine supertype of"),
        Known::new(
            Kind::WalkRefusesOnly,
            Pattern::new()
                .one_of("column", ["List(Int64)", "Array(Int64, 2)"])
                .one_of(
                    "parameter",
                    [
                        "Expr.replace_strict(old)",
                        "Expr.replace_strict(new)",
                        "Expr.replace_strict(default)",
                    ],
                ),
            "the walk refuses at the schema a strict replacement of a list or an array given a \
             value of another dtype, where Polars refuses it only when the rows run, as it cannot \
             replace the values of the column by it (#231)",
        )
        .saying("failed to determine supertype of"),
    ]
}

/// A share outside 0 to 1, or of another dtype than numbers.
fn known_shares() -> Vec<Known<Kind>> {
    let quantiles: &[&str] = &["Expr.quantile(quantile)", "Expr.approx_quantile(quantile)"];
    vec![
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().one_of("parameter", quantiles),
            "a share outside 0 to 1 is refused by Polars, and the walk does not read the value of \
             a share (#224)",
        )
        .saying("`quantile` should be between"),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new()
                .one_of("parameter", quantiles)
                .is("order", "the literal first"),
            "Polars reads a share of numbers only, and refuses a column of another dtype given as \
             one after a literal, which the walk allows (#252)",
        )
        .saying("expected expression of dtype 'numeric' for quantile"),
    ]
}

fn known_ambiguous_dtypes() -> Known<Kind> {
    Known::new(
        Kind::PolarsRefusesOnly,
        Pattern::new()
            .one_of(
                "parameter",
                ["Expr.is_close(other)", "Expr.fill_null(value)"],
            )
            .is("dtype", "none"),
        "Polars finds no dtype for a number typed without a dtype beside a value that is no \
         number, a date, a moment, a length of time, a time of day, a text, a list or an \
         array, in is_close on either side or in fill_null, which the walk allows (#227)",
    )
    .saying("got invalid or ambiguous dtypes")
}

fn known_bitwise_flags() -> Known<Kind> {
    Known::new(
        Kind::PolarsRefusesOnly,
        Pattern::new().is("column", "Boolean").is("dtype", "none"),
        "Polars refuses to meet a Boolean with a number typed without a dtype in a bitwise \
         call, on either side, which the walk leaves not known as it does the arithmetic of \
         text (#227)",
    )
    .saying("is not supported Hint: cast the Boolean")
}

fn known_text_arithmetic() -> Known<Kind> {
    Known::new(
        Kind::PolarsRefusesOnly,
        Pattern::new().is("column", "String"),
        "Polars refuses arithmetic of text and a number typed without a dtype, on either side, \
         which the walk leaves to the tables because the kept cases of the fuzz campaign settle \
         such a prefix as one they could not judge (#227)",
    )
    .saying_one_of(&[
        "arithmetic on dtypes str and dyn",
        "arithmetic on dtypes dyn",
    ])
}

fn known_text_division() -> Known<Kind> {
    Known::new(
        Kind::PolarsRefusesOnly,
        Pattern::new().is("parameter", "Expr.truediv(other)"),
        "Polars refuses to divide text, or a list or a struct that holds it, which the walk \
         leaves to the tables where a number typed without a dtype or a text meets it \
         (#227)",
    )
    .saying("division with 'String' datatypes")
}

fn known_padding_lengths() -> Known<Kind> {
    Known::new(
        Kind::PolarsRefusesOnly,
        Pattern::new()
            .one_of(
                "parameter",
                [
                    "Expr.str.pad_start(length)",
                    "Expr.str.pad_end(length)",
                    "Expr.str.zfill(length)",
                ],
            )
            .is("column", "String"),
        "Polars converts the length of a padding to an unsigned 64 bit number and refuses a \
         number below nothing, a fraction or a date, which the walk does not read (#224)",
    )
    .saying("failed in column 'scalar'")
}

fn known_struct_arithmetic_after_a_number() -> Known<Kind> {
    Known::new(
        Kind::PolarsRefusesOnly,
        Pattern::new()
            .starts("column", "Struct(")
            .is("order", "the literal first"),
        "a number typed without a dtype added to, multiplied by or divided by a struct with a \
         field of text is refused by Polars, which the rule struct-arithmetic-needs-numbers \
         refuses where the struct comes first and not where the number does (#227)",
    )
    .saying("a struct with non-numeric field")
}

fn known_struct_met_by_a_date() -> Known<Kind> {
    Known::new(
        Kind::PolarsRefusesOnly,
        Pattern::new()
            .starts("column", "Struct(")
            .is("dtype", "Date"),
        "a struct met by a date typed in, on either side, in arithmetic, is refused by \
         Polars, which the walk allows (#227)",
    )
    .saying_one_of(&[
        "not allowed on",
        "division of 'Date' datatype is not allowed",
    ])
}

fn known_interval_of_structs() -> Known<Kind> {
    Known::new(
        Kind::PolarsRefusesOnly,
        Pattern::new()
            .starts("column", "Struct(")
            .is("order", "the literal first"),
        "an interval of a number whose bounds are a struct is refused by Polars, which asks a \
         struct of one field there, and the walk allows it (#227)",
    )
    .saying("must specify one field in the struct")
}

fn known_typed_null_folded() -> Known<Kind> {
    Known::new(
        Kind::WalkRefusesOnly,
        Pattern::new().is("value", "null"),
        "a null given a dtype is one value, which Polars folds where it meets a value it cannot \
         compare, cast, list twice or find a dtype with; the tables measured a column of the \
         dtype there and refuse what Polars takes (#228)",
    )
    .saying("tables of Polars at")
}

fn known_typed_null_of_another_dtype() -> Known<Kind> {
    Known::new(
        Kind::PolarsRefusesOnly,
        Pattern::new().is("value", "null"),
        "Polars finds no dtype for a value and a null given a dtype it does not meet, a list or an \
         array filled with a null of a number, a flag or a date, a struct divided by a null, which \
         the walk allows (#228)",
    )
    .saying_one_of(&["failed to determine supertype of", "cannot div two structs"])
}

fn known_date_past_its_range() -> Known<Kind> {
    Known::new(
        Kind::PolarsRefusesOnly,
        Pattern::new().is("dtype", "Date"),
        "Polars panics where a date typed in is past the dates it holds, when it makes the value \
         to meet the column (a Polars bug candidate)",
    )
    .saying("out-of-range date")
}

fn known_text_bounded_by_a_date() -> Known<Kind> {
    Known::new(
        Kind::PolarsRefusesOnly,
        Pattern::new().is("column", "String").is("dtype", "Date"),
        "a text compared with a date typed in as a bound of an interval is refused by Polars, \
         which the walk allows (#227)",
    )
    .saying("cannot compare 'date/datetime/time' to a string value")
}

fn known_flags_replaced_by_a_date() -> Known<Kind> {
    Known::new(
        Kind::PolarsRefusesOnly,
        Pattern::new().is("column", "Boolean").is("dtype", "Date"),
        "a strict replacement of flags given a date typed in as its new values or its default \
         is refused by Polars, which finds no dtype for the two, and the walk allows it \
         (#227)",
    )
    .saying("failed to determine supertype of")
}

fn known_strict_default_of_the_column() -> Known<Kind> {
    Known::new(
        Kind::WalkRefusesOnly,
        Pattern::new()
            .is("parameter", "Expr.replace_strict(default)")
            .is("order", "the literal first"),
        "a strict replacement that follows a literal and is given the column as its default, \
         of more values than the literal, is allowed by Polars where the column is of flags or \
         text, which the rule replacement-values-match-in-number refuses (#252)",
    )
    .saying("rule replacement-values-match-in-number")
}

/// A number typed without a dtype beside a value that is no number, which the walk leaves to the
/// tables or allows, and arithmetic of a struct.
fn known_numbers_beside_what_is_no_number() -> Vec<Known<Kind>> {
    vec![
        known_ambiguous_dtypes(),
        known_bitwise_flags(),
        known_text_arithmetic(),
        known_text_division(),
        known_padding_lengths(),
        known_struct_arithmetic_after_a_number(),
        known_struct_met_by_a_date(),
        known_interval_of_structs(),
        known_text_bounded_by_a_date(),
        known_flags_replaced_by_a_date(),
        known_strict_default_of_the_column(),
    ]
}

// ---------------------------------------------------------------------------
// The tests

#[test]
fn the_walk_takes_a_literal_by_the_role_of_its_parameter_as_polars_does() {
    literal_grid(Depth::Plan).assert_agree(&known_literal_grid(), 100_000);
}

/// Every line of the grid of the literals by their role.
fn known_literal_grid() -> Vec<Known<Kind>> {
    let mut known = known_casts();
    // The counts are read when the rows run, which this grid asks only where the walk refuses.
    known.extend(known_counts().into_iter().map(Known::shared));
    known.extend(known_overflows().into_iter().map(Known::shared));
    known.push(known_exponents().shared());
    // The grid asked the rows of every cell meets Polars' refusal of these, and both refuse.
    known.extend(known_stages().into_iter().map(Known::shared));
    known.push(known_one_value_given_the_column());
    known.extend(known_literals_are_one_value());
    known.extend(known_shares());
    known.extend(known_numbers_beside_what_is_no_number());
    known.extend(known_moments().into_iter().map(Known::shared));
    // The grid asked the rows of every cell may judge these otherwise, so the lines are shared.
    known.push(known_date_past_its_range().shared());
    known.push(known_typed_null_folded().shared());
    known.push(known_typed_null_of_another_dtype().shared());
    known
}

#[test]
fn the_walk_takes_a_count_or_a_length_typed_in_as_polars_does_when_the_rows_run() {
    let mut known = known_casts();
    known.extend(known_counts());
    known.extend(known_overflows());
    known.push(known_one_value_given_the_column());
    count_grid().assert_agree(&known, 1_000);
}

#[test]
fn a_number_typed_without_a_dtype_beside_a_moment_is_taken_as_polars_takes_it_when_the_rows_run() {
    let mut known = known_moments();
    known.extend(known_counts().into_iter().map(Known::shared));
    known.extend(known_overflows());
    known.push(known_one_value_given_the_column());
    moment_grid().assert_agree(&known, 1_000);
}

#[test]
fn the_walk_takes_a_comparison_with_a_literal_where_a_predicate_stands_as_polars_does() {
    comparison_grid(Depth::Plan).assert_agree(&known_comparison_grid(), 50_000);
}

/// Every line of the grid of the comparisons with a literal.
fn known_comparison_grid() -> Vec<Known<Kind>> {
    let mut known = known_casts();
    known.extend([
        known_literal_cast_as_one_value(),
        known_ambiguous_dtypes(),
        known_interval_of_structs(),
        known_text_bounded_by_a_date(),
        // The grid asked the rows of every cell may judge these otherwise, so they are shared.
        known_date_past_its_range().shared(),
        known_typed_null_folded().shared(),
    ]);
    known
}

#[test]
fn a_step_or_a_method_given_a_whole_number_typed_in_is_taken_as_polars_takes_it() {
    counted_grid().assert_agree(&known_integers(), 600);
}

#[test]
fn a_function_of_numbers_typed_in_is_a_join_key_as_polars_takes_it_however_they_are_written() {
    let mut known = known_casts();
    known.extend(known_function_parts());
    function_parts_grid().assert_agree(&known, 2_000);
}

#[test]
#[ignore = "asks the rows of every cell, run on request"]
fn at_the_rows_of_every_cell_the_walk_takes_a_literal_by_the_role_of_its_parameter_as_polars_does()
{
    literal_grid(Depth::Rows).assert_agree(&known_literal_grid(), 1_000_000);
}

#[test]
#[ignore = "asks the rows of every cell, run on request"]
fn at_the_rows_of_every_cell_the_walk_takes_a_comparison_with_a_literal_as_polars_does() {
    comparison_grid(Depth::Rows).assert_agree(&known_comparison_grid(), 200_000);
}
