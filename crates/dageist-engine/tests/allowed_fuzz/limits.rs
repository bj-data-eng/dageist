//! What the tables say they do not cover, which the target does not hold them
//! to.
//!
//! The tables are measured one combination at a time, on columns of three rows
//! with a null, by running each in `select` and in the position that holds it
//! (docs/explanation.md, "Which Combinations Polars Allows", says what they
//! hold). A disagreement that comes from something they were not measured on
//! is not a defect of them. Each limit below names what it is, says which way
//! it excuses a disagreement, and is counted when it does, so that a limit that
//! excuses everything shows. A disagreement no limit explains is a finding.
//!
//! A limit excuses a disagreement only when it is the cause of it. The tables
//! always say where they refused, and the place must lie inside the expression
//! that holds what the limit is about. Where the feature may be there beside
//! another cause, Polars is asked: the case is run again without the feature, with
//! no `explode` or with each literal a column of its dtype, and the limit excuses
//! it only where that shows the feature is why. Polars says where only when it
//! refused while it built the step and worked out its schema, and then not always: a
//! refusal of the whole step has the step for its place. A refusal of the rows
//! (the empty table's too) or a panic has none, and for those the reason and
//! the feature in the case are what there is; a limit that can only be gated so
//! says it in its documentation, and its tests gate the reason. A feature
//! elsewhere in the case excuses nothing. Whether a limit applies does not
//! depend on the order the limits are asked in, and a case that two apply to is
//! excused by both.
//!
//! A limit is never for a part Polars skips. Where the tables refuse a part and
//! Polars allows the step, the target first asks Polars about the part as it is
//! written ([`super::probe`]): a part that Polars does not evaluate is not a limit
//! of the tables, for the editor holds every part to its types. A limit is for what
//! Polars evaluates and accepts, which the part alone does not show, such as a
//! literal it broadcasts or the nulls that the sample's rows leave, and for the
//! other direction, where the tables allow and Polars refuses. The limits are
//! asked after the part as written and before the probes apart from the case,
//! which cannot tell what Polars evaluates from what it skips.

use dageist_engine::allowed::Representative;
use dageist_engine::allowed::Stage as TableStage;
use dageist_engine::catalog;
use dageist_engine::document::{
    Call, Document, Expression, Place, Segment, Start, Step, StepId, Value,
};

use super::asked;
use super::case::Case;
use super::frames::Frames;
use super::frames::stand_in_column;
use super::literal::{
    LiteralCause, call_with_columns, class_of, expression_with_columns, operand_causes,
    placed_operands, starts_from_a_literal,
};
use super::observe::{Observed, Refusal, SCHEMA_SAYS, Stage, Tables, observe, observe_probe};
use super::predict::{Prediction, Refused, Source, Verdict, predict};
use super::probe::{is_the_same_comparison, same_reason, settle_in_the_position};

/// A limit of the tables, and what it says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    /// `Expr.over`'s `mapping_strategy` `explode` gives a different number of
    /// rows, which meet another expression's with lengths that do not match. The
    /// tables do not measure what meets them. Polars refusing the rows, with a
    /// reason about lengths, cannot be placed, so the reason gates it, and an
    /// `explode` in the case is its cause only where Polars stops refusing for
    /// lengths when the case is run again with each `explode` undone; the tables
    /// refusing is placed, inside an expression that holds an `explode`.
    AnExplodeChangesTheLengths,
    /// A literal is one value, which Polars broadcasts and casts as it does not a
    /// column: it accepts a text compared with a typed number, or a window over a
    /// constant, where a column of the dtype is refused, and joins it with a
    /// column of other length. The tables measure an argument as a column of the
    /// dtype, except the values of `shift` and a pivot's group. The tables refusing
    /// at a call that follows a literal or is given one, or, when they run rows,
    /// inside what a call after a literal is given, which Polars allows. The
    /// literal is why only where Polars refuses the case for the tables' reason
    /// once each literal at that place is a column of the dtype Polars gave it, and
    /// nothing else is changed: Polars allowing the column too is a part it skips
    /// for another cause, which a probe or another limit settles. A pivot's
    /// aggregate reads no column to stand in, so it is not this.
    ALiteral,
    /// A pivot's aggregate is run on the values of each group, not on a
    /// column in `select`, and Polars uses other kernels for them: it takes the
    /// `max` of a `Struct` there, which `select` refuses, and compares, casts
    /// and windows what `select` does not. A table with no rows has no groups,
    /// and the values of an aggregate over none have no dtype. The empty table's
    /// refusal is gated by its reason, which has no place; the tables refusing
    /// is placed, in the aggregate, with a reason about a kernel.
    ///
    /// The reasons the gate accepts, each at a call of the aggregate of a pivot:
    /// - the tables refusing, which Polars allows, with a reason that says a dtype an operation
    ///   does not support or a function that makes a length of time expects numbers, a comparison,
    ///   an operation, an evaluation, a cast, a struct field, the
    ///   lengths of a window over a group, a replacement pattern per row, duplicates in what a
    ///   replacement replaces, no supertype of two dtypes, a fill by a strategy for a dtype, or
    ///   the plan saying another dtype than the rows give;
    /// - the tables refusing a failed conversion, which Polars allows, of the values a
    ///   replacement lists, of a cast of a literal, or of a cast that follows a comparison or an
    ///   arithmetic with a null or a bitwise call of whole numbers given a null, and of a skew or
    ///   a kurtosis in the same places, which convert what they follow to a float as a cast
    ///   does, and of the text of a count or a position, made by a fill with a text literal after
    ///   the call that counts, or of the numbers of a group cast or filled to text by the first
    ///   call, which a fill, a reverse, a sort, a drop of the nulls, a running maximum or
    ///   minimum or one value picked may follow, and which read as the numbers, or of the
    ///   values of a group written by a first call with a format of digits alone, as the year,
    ///   which read as numbers, or of what
    ///   a column of the dtype Null is compared or calculated with, which stays null;
    /// - Polars refusing the empty table, with no place, for an operation not supported, a null
    ///   dtype, or a conversion to `null`;
    /// - Polars refusing the rows, with no place, of an aggregate that holds a quantile, for a
    ///   share that is no number: a select reads the text the share is written as, and the
    ///   kernel that takes the values of a group does not.
    ///
    /// A cast of a value that literals alone make, by a function that makes a date, a moment or a
    /// length of time of them, is one too where the same chain through the cast, run in a select on
    /// the sample, is converted without a failure: it converts a null or a number that no group's
    /// text is.
    ///
    /// None of the others has a control that a rerun of Polars could give: the cause is that the group's
    /// kernels run what `select` refuses or converts otherwise, which no case in `select` shows,
    /// so the gate is the pivot and the reason, and the kept cases and tests state each.
    AGroupAggregate,
    /// A key that is wholly a single value, an aggregate or a null, is not one
    /// that has to be elementwise: Polars allows in it what a key may not hold,
    /// such as a running count under a `last`. The tables measured the entry as
    /// the whole key. The tables refusing it, with a reason about elementwise,
    /// placed inside that key. A key that holds an aggregate beside a column, or
    /// a column compared with a null, is refused by a named rule, and is not
    /// this.
    ASingleValueInAKey,
    /// Polars folds a comparison with a null literal into a null, before it asks
    /// whether a join's key is elementwise, and what it leaves depends on the
    /// dtypes and on what the comparison follows, which the tables did not
    /// measure and the named rule, which holds what the evidence settles, does
    /// not model: a column compared with a null is refused by the rule, and an
    /// aggregate compared with a null, or a comparison that follows one, is
    /// found by running. It decides at once, too, an `eq_missing` or an `ne_missing`
    /// of a value and a whole number that some dtype cannot hold, below zero or
    /// above 127, where the dtype of the value cannot hold it, as an unsigned
    /// count cannot, and so leaves a literal where an aggregate was. Polars
    /// refusing the join as not elementwise, with no place, in a case whose join
    /// key holds a comparison with a null literal, or one of those, at any depth.
    ANullComparisonInAKey,
    /// The tables ran every call on columns that hold values, so what Polars does
    /// with a call that has only nulls to meet was not measured: it does not meet
    /// the dtype that the tables refuse when they run rows. A shift fills with a
    /// null unless it is given a value, and the sample's three rows decide what
    /// is left: the target follows the rows of the sample through the chain. A
    /// start column holds a value in each row, and a null in the middle one for
    /// `a` and `b`. A shift moves the rows down for a positive number and up for a
    /// negative one, and the rows it empties hold its `fill_value` (a null where
    /// there is none). A running sum or maximum, and the parts of a date, keep
    /// each row where it is and null where it was; `is_null`, `is_not_null` and
    /// `cum_count` give a value in every row; a comparison, a subtraction, an
    /// `offset_by`, a condition and a window that is not an `explode` keep the
    /// rows in place and leave what they hold unknown, and give the one value that
    /// an aggregate leaves to each of the sample's rows; any other call leaves
    /// nothing known. A function that makes a date, a time or a moment of parts gives a null in each
    /// row where a part that is a column is null, and in each row of one that is a column of text,
    /// which is no number, and a count since 1970 keeps the nulls of what it counts. A drop of the
    /// nulls of rows that are all null leaves none, which meets nothing as a null does. The first and
    /// the last take a row (the first or last that
    /// is not null, where they skip nulls), the minimum and the maximum are null
    /// of rows that are all null and a value of any that holds one, and a sum is
    /// not known of rows that are all null. The tables refusing, when they run
    /// rows, at a call that follows a chain whose rows are all null, or is given
    /// one. A comparison of a list with a `Boolean` is refused after none, one or
    /// two shifts of the sample and allowed after three. Nulls that Polars folds
    /// whatever the rows are, as those of a comparison with a null, are no matter
    /// of the sample: a probe settles them. A literal that holds a value is one value, where the
    /// sample's columns hold three, so a shift that moves it away leaves a null.
    AShiftThatLeavesNulls,
    /// Whether values of different lengths combine, as the lists of two rows
    /// do not, depends on the rows, which the tables measured on one sample.
    /// Polars refusing the rows with a reason about lengths, in a case with no
    /// `explode`, or one that Polars refuses for lengths still when each `explode`
    /// is undone.
    TheLengthsOfTheRows,
    /// Polars raises an integer to a power by taking the exponent as a `UInt32`, so
    /// it refuses a negative whole number as the exponent of an integer when the rows
    /// run: the number is a value, which the tables measured as the dtype and the
    /// number 1. Polars refusing the rows with a failed conversion to `u32`, which
    /// has no place, in a case that holds a `pow` whose exponent is a whole number
    /// typed below zero or holds a subtraction, a negation, a not or a difference, which can
    /// take the values of a column below zero; the sample's rows are 1, a null and 3, which a
    /// sum or a product of them does not.
    ANegativeExponent,
    /// A value of the dtype Null, a column or a literal, typed as it or not, or what a call
    /// gives that dtype, is an operand of `and`, `or` and `xor` that the tables allow,
    /// giving a `Boolean`, as `select` shows. Polars treats it in ways `select` hides. It
    /// negates what a bitwise call combines operand by operand, as the negation of a
    /// conjunction is the disjunction of the negations, so a `not` meets each operand,
    /// which it refuses for the dtype Null. Polars refusing the rows with the reason that
    /// the dtype Null is not supported in `not`, which has no place, in a case that holds
    /// a `not` after a run of `and` and `or`, any call of which meets a null, as the tables
    /// give the dtypes and as a null goes on through the `and` and the `or` after it. It
    /// gives a filter's predicate the dtype Null, which Polars refuses with the reason that
    /// a predicate must be a `Boolean`: an `and` that meets a null does, as does an `or` of
    /// nulls, and the dtype goes on through the `and` and the `or` that follow it, so the
    /// predicate is one that ends in bitwise calls the first of which meets a null. An `or`
    /// or an `xor` of a `Boolean` and a null is a `Boolean`.
    ABitwiseCallOnANull,
    /// A `UInt128` is compared with a `Decimal` by casting it to `decimal[38,0]`,
    /// which holds up to 38 digits, and a `UInt128` holds more. The sample's values
    /// are within the decimal, so the tables did not measure what a value beyond
    /// it meets, which a subtraction that wraps below zero makes. Polars refusing
    /// the rows with a failed conversion of `u128` to a decimal, which has no
    /// place, in a case that holds a subtraction.
    AValueBeyondADecimal,
    /// Polars refuses to divide a `Decimal` by zero, where a whole number divided by
    /// zero is null and a float is infinite: the divisor is a value, which the tables
    /// measured as the dtype and the sample's values, none of them zero. A remainder
    /// or a difference of the sample's values can be. Polars refusing the rows with
    /// the reason that it divides a Decimal by zero, which has no place, in a case with a
    /// Decimal as a column, that holds a division, a floor division or a remainder.
    /// The empty table holds one too: the sum of no rows is zero.
    ADivisionByZero,
    /// Polars panicking has no place, and is gated by its reason. A panic of
    /// Polars is an internal check that failed. The tables record
    /// one for the plan Polars made of a combination in `select`, and the plan
    /// of another place may not reach it; a combination of what they measured
    /// singly, such as two keys of a window, may reach another.
    APanic,
    /// The other table's key is made of the dtype its chain gives on its own,
    /// and the join may resolve the chain to another, as the precision of a
    /// decimal that a null is taken from. Polars' message names the decimal, and
    /// the refusal is of the step.
    AJoinKeyDtype,
    /// A window whose receiver holds another window is run by Polars in the
    /// aggregation context of the outer one, where a key that is not elementwise
    /// is refused; the tables measured one window. The receiver is what comes
    /// before the outer `over` in its chain, with the arguments of those calls.
    /// Polars refuses the rows, so there is no place and the reason gates it.
    AWindowOverAWindow,
    /// The tables hold the schema of a method and what its rows give as one thing,
    /// and refuse a method whose plan says another dtype than its rows give: the
    /// target's own comparison, not a refusal of Polars, which shows only where the
    /// method is the output of the step. Inside an expression that goes on from it
    /// or compares it, Polars evaluates the method and the step's rows do not show
    /// the difference; and a sort key or a join's key is never an output of the step,
    /// which a probe of the call on a column in the key may show refused as well, and
    /// then settles it. The tables refusing, with the reason that the schema says one
    /// dtype and the rows give another, at a call that is not the last of the step's
    /// own expression or in a key that no probe settles, which Polars allows.
    AHiddenSchema,
    /// Polars casts a bound of `clip` to the dtype it clips, and the values `replace` and
    /// `replace_strict` list to the dtype they replace in, as strictly as it can, and refuses
    /// the rows when one holds a value that dtype cannot hold: a number typed without a dtype
    /// that is out of its range or negative beside an unsigned dtype, or a value of a float
    /// that is not a whole number it can hold, as the infinity and the not-a-number that a
    /// logarithm to the base 1 or a root of a negative number gives, or a value out of its
    /// range. The tables measured a bound and a listed value as a column of the sample's rows,
    /// whose numbers every number dtype holds. Polars refusing the rows, with a failed
    /// conversion that has no place, in a case whose refusal is the bound's or the listed
    /// value's: the case run again with the bounds of every `clip` undone and the values of
    /// every replacement made null is not refused for a failed conversion. A table with no rows
    /// holds no value to convert, except for a bound that holds an aggregate, whose value of
    /// nothing, as the not-a-number of an entropy, is there: the empty table is asked for a `clip`
    /// so bounded.
    ABoundTheDtypeCannotHold,
    /// Polars casts a bound of `clip` to the dtype it clips, and a count of characters that a
    /// method of text takes to a whole number, and a bound or a count whose values that dtype
    /// holds is cast whatever its dtype is: text that reads as the number, a list or another
    /// dtype that holds nothing but nulls. The tables measured a bound or a count as a column of
    /// the sample's rows, whose text is a duration no number reads and whose lists no dtype
    /// holds, so they refuse the cast of a dtype to another that the values of another bound pass.
    /// The tables refusing, when they run rows, with a failed conversion or a failed cast of a
    /// dtype, at a `clip` that has a bound, or a method of text that has a count, that is not a
    /// bare column, which Polars allows, and where the tables, asked again with the computed
    /// bounds of every clip undone and the computed counts the whole number 1, no longer refuse
    /// the call. A bound or a count that is a bare column holds the sample's rows, as the tables
    /// measured it, so a refusal that is its own is not this, whatever the other one is.
    ABoundWithValuesOfItsOwn,
    /// A drop of nulls can leave no rows, as one of a null literal does, so the rows do not show
    /// a plan that they contradict. The tables hold the schema of a method and what its rows
    /// give as one thing, and refuse a method whose schema says one dtype and whose rows give
    /// another, as the cube root of a null does, which is the target's own comparison and shows
    /// only where rows are given. The tables refusing, when they run rows, with the reason that
    /// the schema says one dtype and the rows give another, at a call after a `drop_nulls` or a
    /// `drop_nans` in its chain, or in what the chain's start is given, which Polars allows; the
    /// drop is why where Polars, asked again with each drop a fill of nulls forward, refuses the
    /// case for that reason.
    ADropLeavesNoRows,
    /// Polars refuses some dtypes of `and`, `or` and `xor` when it works out the schema of a
    /// select, a filter or a sort key, and accepts them in a join's key, as `date ^ int64` and
    /// `bool | int64`: the docs list the pairs measured. The tables measured a bitwise call in
    /// select, and refuse it for the dtypes it is given. The tables refusing, with a reason that
    /// names the two dtypes, at a bitwise call in one of a join's own keys, which Polars allows;
    /// the key is why where Polars, asked to select the refused call on a column of each of the
    /// two dtypes, refuses it at the call for the tables' reason.
    ABitwiseCallInAJoinKey,
    /// A strict `cast`, a test of closeness and a fill by a value or of NaN, which convert what
    /// they meet to the dtype that holds both as one does, a method of text that is given a count
    /// of characters (`str.head`, `str.tail`, `str.slice`, `str.pad_start`, `str.pad_end` and
    /// `str.zfill`), which converts it to a whole number as one does, and a `replace_strict` asked
    /// for a dtype and given no default, which converts what replaces to it as one (given a default
    /// it converts as a cast that gives a null does), refuse the rows when a value of the column
    /// they follow or give is one the dtype cannot hold: a number out of its range, or below zero beside an unsigned dtype, the
    /// infinity or the not-a-number of a float beside a whole number, or text that does not read
    /// as the number. The tables measured a cast on the sample's rows, whose values every dtype
    /// that can hold a number holds, and a column computed from them holds what they did not,
    /// and so may a table with no rows, where the sum of nothing is a value for a root or a
    /// logarithm to give an infinity. Polars refusing the rows, the empty table's too, with a
    /// failed conversion that has no place, in a case with a strict `cast`, a test of closeness,
    /// a fill by a value or of NaN, a difference where the failed conversion is of an unsigned
    /// number to a signed one (it converts them to the dtype of its result, so a subtraction that
    /// wraps to a number beyond it is refused), a count of characters, or a strict replacement
    /// given a dtype and no default: the case run again with each cast made not strict, each test
    /// a comparison, each fill and each such difference a fill forward, each count the whole
    /// number 1 and each replacement not asked for
    /// a dtype is not refused for a failed conversion. A literal is one value that Polars converts
    /// when it works out the schema, so the refusal may be the schema's, and then it names the
    /// scalar the literal is, and the strict cast it is given to where it says where. A date made of
    /// numbers typed in is such a value, the first of January of the year 1 for 1, 1 and 1.
    ACastOfAValueTheDtypeCannotHold,
    /// `drop_nulls` and `drop_nans` leave the values that are not null or not NaN, so a column
    /// that holds one has fewer values than the table has rows, which Polars refuses to meet the
    /// table's rows or another expression's values. The tables measure the two in a context on
    /// a column that holds no null, where they drop nothing: how many values a column has is for
    /// its rows to say. Polars refusing the rows, with a reason about lengths that has no place
    /// (a replacement's `default` of other length than its input is one), in a case with a
    /// `drop_nulls` or a `drop_nans`: the case run again with each of them
    /// made a call that fills nulls forward, which keeps every value, is not refused for lengths.
    ADropChangesTheLengths,
    /// A window evaluates what it follows once for each group, and what a replacement lists as
    /// the values of a column is then a list for each group, which Polars refuses as a
    /// replacement pattern per row where what it replaces is one value for each group, as an
    /// aggregate is, and as an output of the length of the column where what it replaces is one
    /// value, as a literal is. The tables measure a replacement in a context with the receiver's
    /// own rows, where the columns are one list. Polars refusing the rows, with a reason that
    /// names a pattern per row or an output of another length than its input, in a case that
    /// has a window and a replacement that lists the values of a column: the case run again with
    /// the values of each replacement made null literals is not refused for it.
    AReplacementOfColumnsInAWindow,
    /// A fill by a strategy refuses a `Struct` only where it meets a null to fill, and the
    /// sample's `Struct` holds none. A shift leaves nulls, and the first, the last, the minimum
    /// and the maximum of a table with no rows, or of a group of a pivot that holds nulls alone,
    /// are one null, where those of the sample's rows are a value. Polars refusing the rows or
    /// the empty table with the refusal that a strategy of a fill is not supported for a struct,
    /// which has no place, in a case with a fill by a strategy that follows one of the five: the
    /// case run again with each of the five made a fill of nulls forward, which leaves the nulls
    /// of the column and no more, is not refused for it. Inside a condition Polars refuses the
    /// strategy for a struct where it fills nothing, which it runs in a plain select: the case
    /// with a condition and a fill by a strategy, run again with each fill a fill forward, is
    /// not refused for it.
    ANullStructToFill,
    /// A strict replacement that is given no default is held to more than one that is: it refuses
    /// a value that it does not list, which depends on the rows, and it refuses what it replaces
    /// when the dtype of that cannot become the dtype of what it follows, which a default lets
    /// pass. The tables measured it with a default and with what it replaces the column it
    /// follows. Polars refusing the rows or the empty table, with the refusal of an incomplete
    /// mapping, of nothing listed to replace, or of values of a type that cannot replace the
    /// column's, which has no place, in a case with a
    /// `replace_strict` given no default: the case run again with each given a null default is
    /// not refused for it.
    AStrictReplacementGivenNoDefault,
    /// A replacement of every match refuses a value that is null where Polars reads the value
    /// as one value, which it does of a literal, and of a piece of one row where its streaming
    /// engine runs the rows in pieces, whose length depends on how the rows are shared among the
    /// threads. The tables measured the value as a column of three rows in one piece, which Polars
    /// meets row by row and leaves a null row of unchanged. Polars refusing the rows or the empty table with the refusal that
    /// the value cannot be null, which has no place, in a case with a `str.replace_all`, or a
    /// `str.replace` given a count below zero, which replaces every match: the case run again with
    /// the value of each made a text literal is not refused for it.
    AReplacementOfEveryMatchGivenANullValue,
    /// Polars replaces more than the first match only of plain text and of one value: a pattern
    /// that is a regular expression, or a value that is many, is refused for a count above one.
    /// The tables measured the replacement of the first match, which is the count Python gives it
    /// by default. Polars refusing the rows or the empty table with the refusal that a replacement
    /// of a pattern or of many values with a count above one is not yet supported, which has no
    /// place, in a case with a `str.replace` that is given a count above one: the case run again
    /// with each count left out is not refused for it.
    AReplacementOfMoreThanTheFirstMatch,
    /// A literal text is one value, which Polars' streaming engine broadcasts over the rows of a table
    /// only where each piece it runs them in is no longer than that one value, and never over a table
    /// with no rows, so a replacement of it given many values is refused where the column of text the
    /// tables measured it with is not: its rows and the table's agree. How long a piece is depends on
    /// how the rows are shared among the threads, so the three rows of the target's tables are refused
    /// at one thread and at two, where a piece is longer than one, and pass at three and more. Polars
    /// refusing the rows, or the
    /// empty table, with the refusal that the value's length does not match the text's, which has no
    /// place, in a case with a replacement in a chain that starts from a literal: the case run again
    /// with each such literal a column of text is not refused for it.
    ALiteralTextBesideManyReplacementValues,
    /// Polars' streaming engine refuses, for the order key of a window and for a key inside it, a
    /// replacement of a text of one value, a literal or an aggregate, given many values, which it
    /// meets with each of them in a select, and does so at every thread count. The tables measured
    /// the replacement in select. Polars refusing the rows or the empty table with the refusal that
    /// the length of the value does not match that of the text, which has no place, in a case with a
    /// window and a replacement: the case run again with the value of each replacement made a text
    /// literal is not refused for it, and nor is it run again with each window made a fill.
    AReplacementOfAnAggregateInAWindowKey,
    /// A format writes the fields of the calendar and of the clock that a dtype has, and Polars refuses
    /// one it has no field for, such as a year for a time of day, an hour for a date, a field that does
    /// not exist, and any but the two ISO words for a duration. The tables measured the format `iso`,
    /// which every dtype takes. Polars refusing the rows or the empty table with the refusal that it
    /// cannot format the dtype with the format, that a duration has no such format or that the format
    /// is not a valid one, which has no place, in a case that holds a method that writes a moment as
    /// text, given a format: the case run again with each format made `iso` is not refused for it.
    AFormatTheDtypeCannotWrite,
    /// `dt.replace`, and the functions that make a date, a moment, a time or a length of time of
    /// parts, convert each part they are given to a whole number of the width of the part and
    /// refuse a value outside the calendar's range, and one that does not convert, when the rows
    /// run, so what they do with a part that is no number written in depends on the values of
    /// the rows. The tables measured the column that stands for an argument, whose values are 1
    /// and 3, or true and false, of which false is a day of 0 that no date has. Polars refusing
    /// the rows or the
    /// empty table with the refusal that the components of a date or a time are invalid, or that a
    /// value does not convert, which has no place, in a case with a call given a part: the
    /// case run again with each part made the number 1 is not refused for it. The tables refusing
    /// a call at the rows for invalid components that Polars allows, in a case with a call
    /// given a part: the tables then no longer refuse it for them with each part made the number 1,
    /// and Polars refuses it for them with each part made `false`, which shows that it evaluates the
    /// part and does not skip it.
    APartOfADateThatDependsOnTheRows,
    /// `dt.cast_time_unit` to nanoseconds multiplies the count of a moment, and Polars refuses one
    /// outside the years that nanoseconds hold, such as the first of January of the year 1, when the
    /// rows run, so it depends on the values of the rows, which the tables measured as moments of
    /// 1970 and after. Polars refusing the rows or the empty table with the refusal that a conversion
    /// from a moment in one unit to one in nanoseconds failed, which has no place, in a case with a
    /// `dt.cast_time_unit` given nanoseconds: the case run again with each unit made milliseconds,
    /// which a moment cannot overflow, is not refused for it.
    AMomentBeyondTheRangeOfAUnit,
    /// Polars shifts a moment that is one value, which a function of literals alone or an
    /// aggregate of a column makes, with the number it is filled with, whatever the time zone of
    /// the fill: it takes the number as it is. Its schema names the zone of the receiver and so do
    /// the rows in memory; the streaming engine gives rows in the zone of the fill for an aggregate
    /// and for a moment of literals in `with_columns`, and the zone of the receiver for a moment of
    /// literals in a `select`. The streaming engine panics when the same shift follows a column of
    /// moments, which the tables record and the named rule that moments share a zone refuses for
    /// every receiver. The rule is the tables' own, so the disagreement is of a rule that refuses
    /// what Polars allows, at a shift that is filled, whose receiver gives one value.
    AMomentOfOneValueFilledInAnotherZone,
    /// Polars folds a comparison or an arithmetic with the null literal into a null before it runs
    /// what the call followed or what an argument was made of, so a part inside it is never met:
    /// `h + null` and `null + h` are a null whatever `h` is, and what is made of it is made of a
    /// null. The tables judge the part as written and refuse a call for the dtype of what it is
    /// given. Polars allowing the step, where the refused call is given an expression that holds
    /// such a call, at any depth, with another call after it (a fold that ends a chain leaves a
    /// null literal, a literal's), and Polars refusing the case run again with each such call left
    /// out (a null receiver is replaced by the other operand) for the reason the tables gave,
    /// where it lies inside the expression that holds the refused call or has no place: the fold
    /// is then why Polars allows it. A case that is the same without them, or that Polars still
    /// allows or refuses for another reason, is excused by nothing, and a call that follows a fold
    /// in its own chain is a probe's, which asks it of a value of its dtype. A panic is the panic
    /// limit's, whatever its control shows.
    APartPolarsFoldsToANull,
    /// A place that a gather takes beyond what it follows is refused by Polars when the rows run,
    /// and every place is where the table has none, where the tables measured a gather that gives
    /// a null for it. Polars refusing the rows or the empty table with the words for a place out of
    /// bounds, which have no place, in a case with a gather that is not told to give a null, where
    /// the case run again with each gather told to is no longer refused for them: the places the
    /// rows leave it are why.
    AGatherBeyondTheRows,
}

impl Limit {
    /// Every limit, which the kept cases cover each by one.
    pub const ALL: [Self; 34] = [
        Self::AnExplodeChangesTheLengths,
        Self::ALiteral,
        Self::AGroupAggregate,
        Self::ASingleValueInAKey,
        Self::ANullComparisonInAKey,
        Self::AShiftThatLeavesNulls,
        Self::TheLengthsOfTheRows,
        Self::ANegativeExponent,
        Self::ABitwiseCallOnANull,
        Self::ADivisionByZero,
        Self::AValueBeyondADecimal,
        Self::APanic,
        Self::AWindowOverAWindow,
        Self::AJoinKeyDtype,
        Self::AHiddenSchema,
        Self::ABoundTheDtypeCannotHold,
        Self::ABoundWithValuesOfItsOwn,
        Self::ADropLeavesNoRows,
        Self::ABitwiseCallInAJoinKey,
        Self::ACastOfAValueTheDtypeCannotHold,
        Self::ADropChangesTheLengths,
        Self::AReplacementOfColumnsInAWindow,
        Self::ANullStructToFill,
        Self::AStrictReplacementGivenNoDefault,
        Self::AReplacementOfEveryMatchGivenANullValue,
        Self::AReplacementOfMoreThanTheFirstMatch,
        Self::ALiteralTextBesideManyReplacementValues,
        Self::AReplacementOfAnAggregateInAWindowKey,
        Self::AFormatTheDtypeCannotWrite,
        Self::APartOfADateThatDependsOnTheRows,
        Self::AMomentBeyondTheRangeOfAUnit,
        Self::AMomentOfOneValueFilledInAnotherZone,
        Self::APartPolarsFoldsToANull,
        Self::AGatherBeyondTheRows,
    ];

    /// The name a kept case is filed under.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::AnExplodeChangesTheLengths => "an-explode-changes-the-lengths",
            Self::ALiteral => "a-literal",
            Self::AGroupAggregate => "a-group-aggregate",
            Self::ASingleValueInAKey => "a-single-value-in-a-key",
            Self::ANullComparisonInAKey => "a-null-comparison-in-a-key",
            Self::AShiftThatLeavesNulls => "a-shift-that-leaves-nulls",
            Self::TheLengthsOfTheRows => "the-lengths-of-the-rows",
            Self::ANegativeExponent => "a-negative-exponent",
            Self::ABitwiseCallOnANull => "a-bitwise-call-on-a-null",
            Self::ADivisionByZero => "a-division-by-zero",
            Self::AValueBeyondADecimal => "a-value-beyond-a-decimal",
            Self::APanic => "a-panic",
            Self::AWindowOverAWindow => "a-window-over-a-window",
            Self::AJoinKeyDtype => "a-join-key-dtype",
            Self::AHiddenSchema => "a-hidden-schema",
            Self::ABoundTheDtypeCannotHold => "a-bound-the-dtype-cannot-hold",
            Self::ABoundWithValuesOfItsOwn => "a-bound-with-values-of-its-own",
            Self::ADropLeavesNoRows => "a-drop-leaves-no-rows",
            Self::ABitwiseCallInAJoinKey => "a-bitwise-call-in-a-join-key",
            Self::ACastOfAValueTheDtypeCannotHold => "a-cast-of-a-value-the-dtype-cannot-hold",
            Self::ADropChangesTheLengths => "a-drop-changes-the-lengths",
            Self::AReplacementOfColumnsInAWindow => "a-replacement-of-columns-in-a-window",
            Self::ANullStructToFill => "a-null-struct-to-fill",
            Self::AStrictReplacementGivenNoDefault => "a-strict-replacement-given-no-default",
            Self::AReplacementOfEveryMatchGivenANullValue => {
                "a-replacement-of-every-match-given-a-null-value"
            }
            Self::AReplacementOfMoreThanTheFirstMatch => {
                "a-replacement-of-more-than-the-first-match"
            }
            Self::ALiteralTextBesideManyReplacementValues => {
                "a-literal-text-beside-many-replacement-values"
            }
            Self::AReplacementOfAnAggregateInAWindowKey => {
                "a-replacement-of-an-aggregate-in-a-window-key"
            }
            Self::AFormatTheDtypeCannotWrite => "a-format-the-dtype-cannot-write",
            Self::APartOfADateThatDependsOnTheRows => "a-part-of-a-date-that-depends-on-the-rows",
            Self::AMomentBeyondTheRangeOfAUnit => "a-moment-beyond-the-range-of-a-unit",
            Self::AMomentOfOneValueFilledInAnotherZone => {
                "a-moment-of-one-value-filled-in-another-zone"
            }
            Self::APartPolarsFoldsToANull => "a-part-polars-folds-to-a-null",
            Self::AGatherBeyondTheRows => "a-gather-beyond-the-rows",
        }
    }

    pub const fn statement(self) -> &'static str {
        match self {
            Self::AnExplodeChangesTheLengths => {
                "an explode gives other lengths, which meet those of what follows it, as the tables did not measure"
            }
            Self::ALiteral => "a literal is one value, which the tables measured as a column",
            Self::TheLengthsOfTheRows => {
                "whether values of different lengths combine depends on the rows, which the tables measured on one sample"
            }
            Self::ANegativeExponent => {
                "Polars takes the exponent of an integer as a UInt32, so a negative one fails when the rows run, which the tables measured with the number 1"
            }
            Self::ABitwiseCallOnANull => {
                "Polars meets an operand of the dtype Null of a bitwise call where select does not, and refuses it, though the tables allowed the call as a Boolean"
            }
            Self::ADivisionByZero => {
                "Polars refuses to divide a Decimal by zero, which the sample's values never give the tables, though a sum of no rows is zero"
            }
            Self::AValueBeyondADecimal => {
                "a UInt128 beyond the digits of a decimal cannot be cast to it, which the tables measured on values within it"
            }
            Self::APanic => {
                "a panic of Polars depends on the plan it makes, which another place may not make"
            }
            Self::AGroupAggregate => {
                "a pivot's aggregate is run on groups, with kernels of its own, not on a column in select"
            }
            Self::AShiftThatLeavesNulls => {
                "a shift leaves nulls, which Polars does not meet with the dtype the tables refuse"
            }
            Self::ANullComparisonInAKey => {
                "Polars folds a comparison with a null in a join key into a null, which the rule does not wholly model"
            }
            Self::ASingleValueInAKey => {
                "a key that is wholly a single value, an aggregate or a null, need not be elementwise in what it holds"
            }
            Self::AJoinKeyDtype => {
                "the join resolves a key to a dtype other than the one its chain gives on its own"
            }
            Self::AWindowOverAWindow => {
                "a window over a window is run in an aggregation context, where a key that is not elementwise is refused"
            }
            Self::AHiddenSchema => {
                "a method whose plan and rows differ in dtype is hidden by what goes on from it"
            }
            Self::ABoundTheDtypeCannotHold => {
                "Polars casts a bound of clip, and the values a replacement lists, to the dtype of what they follow and refuses a value that dtype cannot hold, which the tables measured only with values it holds"
            }
            Self::ADropLeavesNoRows => {
                "a drop of nulls can leave no rows, so the rows do not show the plan of what follows it"
            }
            Self::ABitwiseCallInAJoinKey => {
                "Polars does not check the dtypes of a bitwise call in a join's key as it does in select, so it accepts a call the tables refuse for them"
            }
            Self::ABoundWithValuesOfItsOwn => {
                "Polars casts the values of a bound of clip to the dtype it clips, and a count of characters to a whole number, which the tables measured as the sample's rows, and a bound or a count with others passes"
            }
            Self::ACastOfAValueTheDtypeCannotHold => {
                "a strict cast, which a test of closeness, a fill by a value, a count of characters and a difference of unsigned numbers make of what they meet, refuses a value its dtype cannot hold, which the tables measured only with the sample's rows, whose values every number dtype holds"
            }
            Self::ADropChangesTheLengths => {
                "a drop of nulls or of NaNs leaves fewer values where the rows hold one, which the tables measured on a column that holds none"
            }
            Self::AStrictReplacementGivenNoDefault => {
                "a strict replacement given no default refuses a value it does not list and values of a dtype a default lets pass, where the tables measured it given a default and listing every value"
            }
            Self::AReplacementOfEveryMatchGivenANullValue => {
                "a replacement of every match refuses a value that is null where Polars reads it as one value, a literal or a piece of one row, where the tables measured the value as a column of three rows in one piece"
            }
            Self::AMomentBeyondTheRangeOfAUnit => {
                "a cast of a moment to nanoseconds refuses one outside the years nanoseconds hold, when the rows run, where the tables measured moments of 1970 and after"
            }
            Self::AMomentOfOneValueFilledInAnotherZone => {
                "Polars shifts a moment that is one value, of literals alone or an aggregate, with a fill in another time zone, and names the zone of the receiver in its schema, where the streaming engine panics for a column of moments, which the tables measured"
            }
            Self::AGatherBeyondTheRows => {
                "a gather refuses a place beyond the rows it follows when they run, and every place where the table has none, where the tables measured a gather that gives a null for it"
            }
            Self::APartPolarsFoldsToANull => {
                "Polars folds a comparison or an arithmetic with a null literal into a null before it runs what that call followed, so a part inside it is never met, where the tables judged the part as written"
            }
            Self::APartOfADateThatDependsOnTheRows => {
                "dt.replace, date, datetime and time refuse a part outside the calendar's range, or that does not convert, when the rows run, so it depends on the values of the rows, where the tables measured the values of one column"
            }
            Self::AFormatTheDtypeCannotWrite => {
                "a format writes the fields a dtype has and Polars refuses one it has none for, where the tables measured the format iso, which every dtype takes"
            }
            Self::AReplacementOfAnAggregateInAWindowKey => {
                "Polars' streaming engine does not meet a text of one value with each value of a replacement in the order key of a window, which it does in a select, where the tables measured it"
            }
            Self::ALiteralTextBesideManyReplacementValues => {
                "a literal text is one value that Polars does not broadcast over a table with no rows, nor over the rows at every thread count, where the tables measured the text as a column of the table's rows"
            }
            Self::AReplacementOfMoreThanTheFirstMatch => {
                "Polars replaces more than the first match only of plain text and of one value, and refuses a pattern or many values, where the tables measured the replacement of the first match"
            }
            Self::ANullStructToFill => {
                "a shift, and the first, the last, the minimum and the maximum of nothing, give a null, which a fill by a strategy refuses a struct for, where the sample's struct holds none"
            }
            Self::AReplacementOfColumnsInAWindow => {
                "a window gives a replacement of columns a list of values for each group, which Polars refuses as a pattern per row, where the tables measured it in select"
            }
        }
    }
}

/// Whether the limits excuse nothing: set by `DAGEIST_FUZZ_STRICT=1`.
pub fn strict() -> bool {
    std::env::var_os("DAGEIST_FUZZ_STRICT").is_some_and(|value| value == "1")
}

fn is(call: &Call, id: &str) -> bool {
    call.entry().id().as_str() == id
}

const ONE_VALUE: [&str; 31] = [
    "Expr.has_nulls",
    "Expr.null_count",
    "Expr.n_unique",
    "Expr.count",
    "Expr.str.join",
    "Expr.sum",
    "Expr.min",
    "Expr.max",
    "Expr.len",
    "Expr.first",
    "Expr.last",
    "Expr.mean",
    "Expr.median",
    "Expr.std",
    "Expr.var",
    "Expr.quantile",
    "Expr.product",
    "Expr.skew",
    "Expr.kurtosis",
    "Expr.entropy",
    "Expr.arg_max",
    "Expr.arg_min",
    "Expr.nan_max",
    "Expr.nan_min",
    "Expr.all",
    "Expr.any",
    "Expr.approx_n_unique",
    "Expr.approx_quantile",
    "Expr.bitwise_and",
    "Expr.bitwise_or",
    "Expr.bitwise_xor",
];

fn is_one_value(call: &Call) -> bool {
    ONE_VALUE.contains(&call.entry().id().as_str())
}

/// The expressions a call is given, by parameter.
fn operands(call: &Call) -> Vec<(&str, &Expression)> {
    fn collect<'a>(
        parameter: &'a str,
        value: &'a Value,
        found: &mut Vec<(&'a str, &'a Expression)>,
    ) {
        match value {
            Value::Expression(expression) => found.push((parameter, expression)),
            Value::Many(items) => items
                .iter()
                .for_each(|item| collect(parameter, item, found)),
            Value::Pairs(pairs) => pairs.iter().for_each(|(key, entry)| {
                collect(parameter, key, found);
                collect(parameter, entry, found);
            }),
            _ => {}
        }
    }
    let mut found = Vec::new();
    for given in call.arguments() {
        collect(given.parameter(), given.value(), &mut found);
    }
    found
}

/// The calls of an expression, and of the expressions inside it, as far as
/// `found` says, which is asked of each.
fn any_call(expression: &Expression, found: &dyn Fn(&Call) -> bool) -> bool {
    let calls = std::iter::once(expression.start())
        .filter_map(|start| match start {
            Start::Call(call) => Some(call),
            Start::Input(_) | Start::Hole => None,
        })
        .chain(expression.links());
    calls.into_iter().any(|call| {
        found(call)
            || operands(call)
                .into_iter()
                .any(|(_, inside)| any_call(inside, found))
    })
}

fn is_a_single_value(call: &Call) -> bool {
    is_one_value(call)
        || (is(call, "polars.lit") && matches!(call.argument("value"), Some(Value::Flag(None))))
}

/// Whether `call` gives numbers whatever the values it follows hold: a count, or the positions
/// or the runs of the values. Where the numbers are turned to text, the text reads as the numbers
/// again, which the sample's text, as a duration, does not.
fn gives_a_number_of_its_own(call: &Call) -> bool {
    matches!(
        call.entry().id().as_str(),
        "Expr.count"
            | "Expr.len"
            | "Expr.n_unique"
            | "Expr.null_count"
            | "Expr.approx_n_unique"
            | "Expr.arg_sort"
            | "Expr.arg_max"
            | "Expr.arg_min"
            | "Expr.rle_id"
            | "Expr.cum_count"
    )
}

/// Whether `links` give numbers of their own and then turn them to text: a call that gives a count
/// or positions, and after it a fill with a text literal, which makes the numbers the text of
/// themselves. The text then reads as the numbers, which the sample's text does not.
fn makes_text_of_a_count(links: &[Call]) -> bool {
    links
        .iter()
        .position(gives_a_number_of_its_own)
        .is_some_and(|counted| links[counted + 1..].iter().any(is_a_fill_with_text))
}

/// Whether every call of `links` keeps each value as it is and takes none away.
fn keeps_each_value(links: &[Call]) -> bool {
    links.iter().all(|link| {
        matches!(
            link.entry().id().as_str(),
            "Expr.forward_fill"
                | "Expr.backward_fill"
                | "Expr.reverse"
                | "Expr.sort"
                | "Expr.drop_nulls"
                | "Expr.first"
                | "Expr.last"
                | "Expr.min"
                | "Expr.max"
                | "Expr.cum_max"
                | "Expr.cum_min"
        )
    })
}

/// The directives of a format that write digits alone.
const DIRECTIVES_OF_DIGITS: &str = "YymdHMSIjfuwCUWGgV";

/// Whether the call writes a moment with a format of digits alone, directives that write digits
/// and digits, so that the text it gives reads as a number.
fn writes_digits_alone(call: &Call) -> bool {
    if !(is(call, "Expr.dt.to_string") || is(call, "Expr.dt.strftime")) {
        return false;
    }
    let Some(Value::String(format)) = call.argument("format") else {
        return false;
    };
    let mut characters = format.chars();
    let mut written = false;
    while let Some(character) = characters.next() {
        let digits = match character {
            '%' => characters
                .next()
                .is_some_and(|directive| DIRECTIVES_OF_DIGITS.contains(directive)),
            other => other.is_ascii_digit(),
        };
        if !digits {
            return false;
        }
        written = true;
    }
    written
}

/// Whether the call at `at` of the aggregate `chain` of a pivot follows the values of a group
/// written with a format of digits alone, and nothing else that takes a value away: the text reads
/// as a number, which the sample's does not.
fn is_a_date_written_as_digits(chain: &Expression, at: usize) -> bool {
    matches!(chain.start(), Start::Call(start) if is(start, "polars.element"))
        && at >= 1
        && keeps_each_value(&chain.links()[1..at])
        && chain.links().first().is_some_and(writes_digits_alone)
}

/// Whether the call at `at` of the aggregate `chain` of the pivot `step` follows the numbers of the
/// group cast to text and nothing else: the values the pivot reads are columns of numbers, the
/// first call casts them to text or fills them with a text literal, and any that follow it keep
/// each value as it is. The text reads as the numbers, which the sample's does not.
fn is_text_of_the_numbers_of_a_group(
    frames: &Frames,
    step: &Call,
    chain: &Expression,
    at: usize,
) -> bool {
    let Some(Value::Columns(values)) = step.argument("values") else {
        return false;
    };
    matches!(chain.start(), Start::Call(start) if is(start, "polars.element"))
        && at >= 1
        // What stands between keeps each value as it is and takes none away.
        && keeps_each_value(&chain.links()[1..at])
        && chain.links().first().is_some_and(|first| {
            // A cast to text, or a fill with a text literal, which gives numbers the dtype text.
            (is(first, "Expr.cast")
                && matches!(first.argument("dtype"), Some(Value::Dtype(dtype))
                    if dtype.to_string() == "String"))
                || is_a_fill_with_text(first)
        })
        && !values.is_empty()
        && values.iter().all(|name| {
            frames
                .dtype_of(name)
                .is_some_and(polars::prelude::DataType::is_primitive_numeric)
        })
}

/// Whether `call` fills with a text literal, which gives the dtype text to what it fills.
fn is_a_fill_with_text(call: &Call) -> bool {
    matches!(
        call.entry().id().as_str(),
        "Expr.fill_null" | "Expr.fill_nan"
    ) && matches!(call.argument("value"), Some(Value::Expression(value))
        if is_a_literal_of_text(value))
}

/// Whether the first `upto` links of the aggregate `chain` of the pivot `step` follow the values of
/// columns that are all of the dtype Null and only compare them, do arithmetic with them, cast
/// them, pick one of them or order them, which leaves nulls: the call at `upto` meets nulls, where
/// the tables measured the values of a column that holds some.
fn leaves_nulls_of_a_column_of_nulls(
    frames: &Frames,
    step: &Call,
    chain: &Expression,
    upto: usize,
) -> bool {
    let Some(Value::Columns(values)) = step.argument("values") else {
        return false;
    };
    matches!(chain.start(), Start::Call(start) if is(start, "polars.element"))
        && upto > 0
        && !values.is_empty()
        && values.iter().all(|name| {
            frames
                .dtype_of(name)
                .is_some_and(|dtype| *dtype == polars::prelude::DataType::Null)
        })
        && chain.links()[..upto].iter().all(|link| {
            let id = link.entry().id().as_str();
            // A comparison or an arithmetic leaves a null for each null, and a call that picks
            // one of the values or orders them leaves nothing else.
            carries_nulls(id)
                || matches!(
                    id,
                    "Expr.cast"
                        | "Expr.first"
                        | "Expr.last"
                        | "Expr.min"
                        | "Expr.max"
                        | "Expr.reverse"
                        | "Expr.sort"
                )
        })
}

/// Whether `call` is a comparison or an arithmetic given the null literal, which is a null in each
/// row, whatever the rows hold.
fn compares_with_a_null(call: &Call) -> bool {
    carries_nulls(call.entry().id().as_str())
        && operands(call)
            .into_iter()
            .any(|(_, operand)| is_the_bare_null_literal(operand))
}

/// Whether a link of `chain`, which is at `place`, before the call at `at` is a bitwise call given
/// the null literal that follows whole numbers, as the tables say: the bitwise call of a whole
/// number with a null is a null in each row, where one of a `Boolean` is not (a null with a true
/// is a true).
fn gives_a_null_of_whole_numbers(
    prediction: &Prediction,
    chain: &Expression,
    place: &Place,
    at: usize,
) -> bool {
    chain.links()[..at].iter().enumerate().any(|(link, call)| {
        is_a_bitwise_call(call)
            && operands(call)
                .into_iter()
                .any(|(_, operand)| is_the_bare_null_literal(operand))
            && class_after(prediction, place, link).is_some_and(|class| class.dtype().is_integer())
    })
}

/// Whether `expression` is the null literal and no more.
fn is_the_bare_null_literal(expression: &Expression) -> bool {
    expression.links().is_empty()
        && matches!(expression.start(), Start::Call(start)
            if is(start, "polars.lit") && matches!(start.argument("value"), Some(Value::Flag(None))))
}

/// Whether `call` is `and`, `or` or `xor`.
fn is_a_bitwise_call(call: &Call) -> bool {
    matches!(
        call.entry().id().as_str(),
        "Expr.and_" | "Expr.or_" | "Expr.xor"
    )
}

/// What the tables say the expression at `place` is after its first `done` links: its start for
/// none.
fn class_after(prediction: &Prediction, place: &Place, done: usize) -> Option<Representative> {
    let traced = prediction
        .expressions
        .iter()
        .find(|traced| traced.place == *place)?;
    match done.checked_sub(1) {
        None => traced.start,
        Some(last) => traced.links.get(last).copied().flatten(),
    }
}

/// Whether the tables give the whole of `expression`, at `place`, the dtype Null.
fn is_a_null(prediction: &Prediction, expression: &Expression, place: &Place) -> bool {
    class_after(prediction, place, expression.links().len()) == Some(Representative::Null)
}

/// Where a null goes along a chain, as the tables give the dtypes and Polars treats a null that a
/// bitwise call meets. For each link: whether it is a bitwise call that meets a null, as the value
/// it follows or an operand it is given, and whether what the chain is after it has the dtype Null
/// where the tables give a `Boolean`.
///
/// An `and` with a null on either side is a null, and goes on being one through the `and` and the
/// `or` of another null after it; an `or` of nulls is one; an `xor` of a `Boolean` and a null is a
/// `Boolean`, and the tables refuse an `or` or an `xor` of a receiver that is a null. Any other call
/// gives what the tables say. A filter's predicate takes the dtype Null through it, and a negation
/// reaches the null through the calls it is pushed through.
struct NullsAlong {
    meets: Vec<bool>,
    is_null_after: Vec<bool>,
}

fn nulls_along(prediction: &Prediction, expression: &Expression, place: &Place) -> NullsAlong {
    let null = Some(Representative::Null);
    let mut is_null = class_after(prediction, place, 0) == null;
    let mut along = NullsAlong {
        meets: Vec::new(),
        is_null_after: Vec::new(),
    };
    for (at, link) in expression.links().iter().enumerate() {
        let operands: Vec<bool> = placed_operands(link, &place.clone().then(Segment::Link(at)))
            .into_iter()
            .map(|(_, held_at, operand)| is_a_null(prediction, operand, &held_at))
            .collect();
        let meets = is_a_bitwise_call(link) && (is_null || operands.contains(&true));
        is_null = match link.entry().id().as_str() {
            "Expr.and_" => meets,
            "Expr.or_" => is_null && operands.iter().all(|null| *null),
            "Expr.xor" => false,
            _ => class_after(prediction, place, at + 1) == null,
        };
        along.meets.push(meets);
        along.is_null_after.push(is_null);
    }
    along
}

/// Every expression inside `expression`, which is at `place`, itself first, and where each is.
fn expressions_inside<'a>(
    expression: &'a Expression,
    place: &Place,
    found: &mut Vec<(Place, &'a Expression)>,
) {
    found.push((place.clone(), expression));
    let start = match expression.start() {
        Start::Call(call) => Some((call, place.clone().then(Segment::Start))),
        Start::Input(_) | Start::Hole => None,
    };
    let links = expression
        .links()
        .iter()
        .enumerate()
        .map(|(at, link)| (link, place.clone().then(Segment::Link(at))));
    for (call, call_at) in start.into_iter().chain(links) {
        for (_, held_at, inside) in placed_operands(call, &call_at) {
            expressions_inside(inside, &held_at, found);
        }
    }
}

/// Whether an expression has a `not` after a run of `and` and `or`, any call of which meets a
/// null, at any depth: the negation is pushed through the run and meets the null wherever it is
/// in it. Another call ends the run.
fn negates_a_bitwise_call_on_a_null(
    prediction: &Prediction,
    expression: &Expression,
    place: &Place,
) -> bool {
    let mut inside = Vec::new();
    expressions_inside(expression, place, &mut inside);
    let combines = |link: &Call| is(link, "Expr.and_") || is(link, "Expr.or_");
    inside.into_iter().any(|(at, inside)| {
        let along = nulls_along(prediction, inside, &at);
        let links = inside.links();
        (0..links.len()).any(|negation| {
            is(&links[negation], "Expr.not_")
                && (0..negation)
                    .rev()
                    .take_while(|link| combines(&links[*link]))
                    .any(|link| along.meets[link])
        })
    })
}

/// Whether Polars gives a filter's predicate the dtype Null where the tables give a `Boolean`: it
/// ends in a bitwise call that a null has reached, which [`nulls_along`] follows.
fn gives_a_predicate_the_dtype_null(
    prediction: &Prediction,
    predicate: &Expression,
    place: &Place,
) -> bool {
    let along = nulls_along(prediction, predicate, place);
    predicate.links().last().is_some_and(is_a_bitwise_call)
        && along.is_null_after.last().copied().unwrap_or(false)
}

/// Whether `call` is a literal that is a whole number below zero.
fn is_a_whole_number_below_zero(call: &Call) -> bool {
    is(call, "polars.lit")
        && matches!(call.argument("value"), Some(Value::Integer(number)) if *number < 0)
}

/// Whether `call` is a `pow` whose exponent holds a value that may be below zero: a whole number
/// typed below zero, or a call that can take the values of a column there, as the sample's own
/// values are not.
fn is_a_power_that_may_be_given_a_value_below_zero(call: &Call) -> bool {
    is(call, "Expr.pow")
        && matches!(call.argument("exponent"), Some(Value::Expression(exponent))
        if any_call(exponent, &|inner| {
            is_a_whole_number_below_zero(inner)
                || matches!(
                    inner.entry().id().as_str(),
                    "Expr.sub" | "Expr.neg" | "Expr.not_" | "Expr.diff"
                )
        }))
}

fn has_explode(call: &Call) -> bool {
    is(call, "Expr.over")
        && matches!(call.argument("mapping_strategy"), Some(Value::Choice(word)) if word == "explode")
}

/// The indexes of the links of `expression` that `found` says yes to.
fn links_where(expression: &Expression, found: &dyn Fn(&Call) -> bool) -> Vec<usize> {
    expression
        .links()
        .iter()
        .enumerate()
        .filter(|(_, link)| found(link))
        .map(|(at, _)| at)
        .collect()
}

/// Whether a place reached `holder` where an `explode` changes the rows it
/// meets: at the window or after it, and in the arguments of the links after.
fn reached_by_an_explode(holder: &Expression, reach: Reach) -> bool {
    links_where(holder, &has_explode)
        .into_iter()
        .any(|at| match reach {
            Reach::Whole => true,
            Reach::Start => false,
            Reach::Operand(j) => j > at,
            Reach::At(k) => k >= at,
        })
}

/// Whether the first `upto` links of `expression` give one value and nothing
/// beside them does not: an aggregate among them with only single values after
/// it; with `literals`, a start made of literals too; or a condition whose parts
/// are single.
fn wholly_single(expression: &Expression, upto: usize, literals: bool) -> bool {
    let links = &expression.links()[..upto.min(expression.links().len())];
    if let Some(last) = links.iter().rposition(is_one_value) {
        return links[last + 1..].iter().all(is_single);
    }
    match expression.start() {
        Start::Call(start) if is(start, "polars.lit") => literals && links.iter().all(is_single),
        Start::Call(start) if is(start, "polars.when") => {
            is_single(start) && links.iter().all(is_single)
        }
        _ => false,
    }
}

/// Whether `call` gives no more values than what it follows: what a replacement lists is a table
/// of values, and any other call is given nothing but single values.
fn is_single(call: &Call) -> bool {
    is(call, "Expr.replace")
        || is(call, "Expr.replace_strict")
        || operands(call)
            .into_iter()
            .all(|(_, inside)| wholly_single(inside, inside.links().len(), true))
}

/// Whether the first `upto` links of `expression` give one value, that a literal, an aggregate or
/// a function that makes a date, a time, a moment or a length of time of literals alone makes: the
/// chain is [`wholly_single`], or starts from such a function and gives no more values after it.
fn gives_one_value_of_literals(expression: &Expression, upto: usize) -> bool {
    let links = &expression.links()[..upto.min(expression.links().len())];
    wholly_single(expression, upto, true)
        || (matches!(expression.start(), Start::Call(start) if is_a_value_of_literals_alone(start))
            && links.iter().all(is_single))
}

/// The functions that start a chain and give one value of what they are given, which a column
/// does not stand in for: the literal, and the ones that make a date, a time, a moment, a length
/// of time or a moment from a count since 1970. A function is no value of literals alone until it
/// is named here; the catalogue's functions are checked against this list, so that a new one is
/// decided when it is added and not excused for what it gives many values of.
const FUNCTIONS_OF_LITERALS: [&str; 6] = [
    "polars.date",
    "polars.datetime",
    "polars.duration",
    "polars.from_epoch",
    "polars.lit",
    "polars.time",
];

/// Whether the start `start` makes one value of literals alone: one of the
/// [`FUNCTIONS_OF_LITERALS`] that is given no column and no group's values.
fn is_a_value_of_literals_alone(start: &Call) -> bool {
    FUNCTIONS_OF_LITERALS.contains(&start.entry().id().as_str())
        && !operands(start).into_iter().any(|(_, operand)| {
            any_call(operand, &|call| {
                is(call, "polars.col") || is(call, "polars.element")
            })
        })
}

/// Where a place is in an expression that holds it: in what the chain starts
/// from, in the arguments of one of its links, at one of its calls, or at the
/// expression as a whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reach {
    Whole,
    Start,
    /// Inside the arguments of the link.
    Operand(usize),
    /// At the call of the link itself.
    At(usize),
}

/// Where a place is in the document: the expression a position of the step
/// holds that it is inside, and the chain and the call it is at.
struct Locus<'a> {
    /// The expression the step holds, outermost.
    top: Option<&'a Expression>,
    /// The expression that holds the call, innermost.
    chain: Option<&'a Expression>,
    /// The link the place is at, `None` for the start.
    index: Option<usize>,
    call: Option<&'a Call>,
    /// Every expression the place is inside, outermost first.
    inside: Vec<&'a Expression>,
    /// How the place reaches each of them, in the same order.
    reach: Vec<Reach>,
    /// Where each of them is, in the same order.
    places: Vec<Place>,
}

enum Node<'a> {
    Call(&'a Call),
    Value(&'a Value),
    Expression(&'a Expression),
}

/// Whether the call at `place` is one that the tables judge by its values apart, each beside a
/// companion that stands for the others: a replacement, an interval, a test of closeness and a
/// replacement of the parts of a date, which they judge by what it follows and by what it is
/// given. Polars refuses the call for any of them, so what it says of the part is not the reason
/// the tables gave for the first they refused.
pub fn judges_its_values_apart(case: &Case, place: &Place) -> bool {
    locate(case, place).call.is_some_and(|call| {
        matches!(
            call.entry().id().as_str(),
            "Expr.replace"
                | "Expr.replace_strict"
                | "Expr.is_between"
                | "Expr.is_close"
                | "Expr.dt.replace"
        )
    })
}

/// Follows `place` through the document.
fn locate<'a>(case: &'a Case, place: &Place) -> Locus<'a> {
    let mut locus = Locus {
        top: None,
        chain: None,
        index: None,
        call: None,
        inside: Vec::new(),
        reach: Vec::new(),
        places: Vec::new(),
    };
    let mut node = Node::Call(case.document.steps()[0].call());
    let mut here = Place::of_step(0, Some(case.document.steps()[0].id().clone()));
    for segment in place.path() {
        here = here.then(segment.clone());
        let value = match (&node, segment) {
            (Node::Call(call), Segment::Parameter(name)) => call.argument(name),
            (Node::Value(Value::Many(items)), Segment::Item(at)) => items.get(*at),
            (Node::Value(Value::Pairs(pairs)), Segment::Key(at)) => {
                pairs.get(*at).map(|(key, _)| key)
            }
            (Node::Value(Value::Pairs(pairs)), Segment::Value(at)) => {
                pairs.get(*at).map(|(_, entry)| entry)
            }
            (Node::Expression(expression), Segment::Start) => {
                locus.chain = Some(expression);
                locus.index = None;
                if let Some(last) = locus.reach.last_mut() {
                    *last = Reach::Start;
                }
                node = match expression.start() {
                    Start::Call(call) => Node::Call(call),
                    Start::Input(_) | Start::Hole => return locus,
                };
                continue;
            }
            (Node::Expression(expression), Segment::Link(at)) => {
                locus.chain = Some(expression);
                locus.index = Some(*at);
                if let Some(last) = locus.reach.last_mut() {
                    *last = Reach::Operand(*at);
                }
                node = match expression.links().get(*at) {
                    Some(call) => Node::Call(call),
                    None => return locus,
                };
                continue;
            }
            _ => return locus,
        };
        node = match value {
            Some(Value::Expression(expression)) => {
                locus.top.get_or_insert(expression);
                locus.inside.push(expression);
                locus.reach.push(Reach::Whole);
                locus.places.push(here.clone());
                // Which call of this expression the place is at is not known
                // until the way says; it is not the link of the chain that holds
                // this one.
                locus.index = None;
                Node::Expression(expression)
            }
            Some(other) => Node::Value(other),
            None => return locus,
        };
    }
    // The path ends at a call: the innermost expression is reached at it.
    if let (Node::Call(_), Some(Reach::Operand(at)), Some(last)) =
        (&node, locus.reach.last().copied(), locus.reach.last_mut())
    {
        *last = Reach::At(at);
    }
    match node {
        Node::Call(call) => locus.call = Some(call),
        Node::Expression(expression) => locus.chain = Some(expression),
        Node::Value(_) => {}
    }
    locus
}

/// Which way a case disagrees.
enum Disagreement<'a> {
    /// The tables allow it and Polars refuses it, here.
    PolarsRefuses(&'a Refusal),
    /// The tables refuse it, here, and Polars allows it.
    PolarsAllows(&'a Refused),
    /// A named rule refuses it, here, and Polars allows it.
    RuleRefuses(&'a Refused),
}

fn is_lengths(reason: &str) -> bool {
    reason.contains("lengths don't match")
        || reason.contains("lengths differed")
        || reason.contains("does not match that of the base")
}

fn is_panic(reason: &str) -> bool {
    reason.starts_with("Polars panicked")
}

/// The keys of the step, each with its place: a join's, if `joins`, and the keys
/// of every window in it.
fn keys(case: &Case, joins: bool) -> Vec<(Place, &Expression)> {
    let step = &case.document.steps()[0];
    let origin = Place::of_step(0, Some(step.id().clone()));
    let mut found: Vec<(Place, &Expression)> = Vec::new();
    if joins && is(step.call(), "LazyFrame.join") {
        found.extend(
            placed_operands(step.call(), &origin)
                .into_iter()
                .filter(|(parameter, _, _)| ["on", "left_on", "right_on"].contains(parameter))
                .map(|(_, place, key)| (place, key)),
        );
    }
    fn windows<'a>(
        expression: &'a Expression,
        place: &Place,
        found: &mut Vec<(Place, &'a Expression)>,
    ) {
        let starts = match expression.start() {
            Start::Call(call) => Some((call, place.clone().then(Segment::Start))),
            Start::Input(_) | Start::Hole => None,
        };
        let links = expression
            .links()
            .iter()
            .enumerate()
            .map(|(at, link)| (link, place.clone().then(Segment::Link(at))));
        for (call, at) in starts.into_iter().chain(links) {
            let held = placed_operands(call, &at);
            if is(call, "Expr.over") {
                found.extend(
                    held.iter()
                        .filter(|(parameter, _, _)| {
                            ["partition_by", "more_exprs", "order_by"].contains(parameter)
                        })
                        .map(|(_, key_at, key)| (key_at.clone(), *key)),
                );
            }
            for (_, inside_at, inside) in held {
                windows(inside, &inside_at, found);
            }
        }
    }
    for (_, held_at, held) in placed_operands(step.call(), &origin) {
        windows(held, &held_at, &mut found);
    }
    found
}

/// Whether a key holds, at any depth, a comparison with a null literal: one that
/// is given it, or one that follows it.
fn holds_a_null_comparison(key: &Expression) -> bool {
    let comparisons = [
        "Expr.eq", "Expr.ne", "Expr.gt", "Expr.ge", "Expr.lt", "Expr.le",
    ];
    let is_null = |expression: &Expression| {
        expression.links().is_empty()
            && matches!(expression.start(), Start::Call(start)
                if is(start, "polars.lit") && matches!(start.argument("value"), Some(Value::Flag(None))))
    };
    // A whole number below zero bounds an interval of what is unsigned, as a count is, in a way
    // that Polars decides at once, and so folds the interval, which the document does not decide.
    let is_below_zero = |expression: &Expression| {
        expression.links().is_empty()
            && matches!(expression.start(), Start::Call(start)
                if is(start, "polars.lit") && matches!(start.argument("value"), Some(Value::Integer(number)) if *number < 0))
    };
    // A whole number that some dtype cannot hold, below zero or above what an `Int8` holds, is
    // one that Polars decides at once for a comparison of missing values where the dtype of what
    // it is compared with cannot hold it, so it folds the comparison as it does a null.
    let is_a_number_some_dtype_lacks = |expression: &Expression| {
        expression.links().is_empty()
            && matches!(expression.start(), Start::Call(start)
                if is(start, "polars.lit") && matches!(start.argument("value"), Some(Value::Integer(number)) if !(0..=127).contains(number)))
    };
    let compares = |expression: &Expression| {
        let null_start = matches!(expression.start(), Start::Call(start)
            if is(start, "polars.lit") && matches!(start.argument("value"), Some(Value::Flag(None))));
        let number_start = matches!(expression.start(), Start::Call(start)
            if is(start, "polars.lit") && matches!(start.argument("value"), Some(Value::Integer(number)) if !(0..=127).contains(number)));
        expression.links().iter().enumerate().any(|(at, link)| {
            let id = link.entry().id().as_str();
            // `is_between` compares each of its bounds with what it follows, so a bound that is
            // a null is a comparison with one, and a null it follows is not.
            // Casts after the null literal leave a null, which a comparison that follows them
            // folds for the dtype Null and not always for another, which the document does not
            // decide.
            (comparisons.contains(&id)
                && null_start
                && expression.links()[..at]
                    .iter()
                    .all(|before| is(before, "Expr.cast")))
                || ((comparisons.contains(&id) || id == "Expr.is_between")
                    && operands(link)
                        .into_iter()
                        .any(|(_, operand)| is_null(operand)))
                || (id == "Expr.is_between"
                    && operands(link)
                        .into_iter()
                        .any(|(_, operand)| is_below_zero(operand)))
                || (matches!(id, "Expr.eq_missing" | "Expr.ne_missing")
                    && (operands(link)
                        .into_iter()
                        .any(|(_, operand)| is_a_number_some_dtype_lacks(operand))
                        || (number_start && expression.links()[..at].is_empty())))
        })
    };
    fn walk(expression: &Expression, compares: &dyn Fn(&Expression) -> bool) -> bool {
        let calls = std::iter::once(expression.start())
            .filter_map(|start| match start {
                Start::Call(call) => Some(call),
                Start::Input(_) | Start::Hole => None,
            })
            .chain(expression.links());
        compares(expression)
            || calls.into_iter().any(|call| {
                operands(call)
                    .into_iter()
                    .any(|(_, inside)| walk(inside, compares))
            })
    }
    walk(key, &compares)
}

/// Whether an `over` has, before it in its chain, another window.
fn window_over_a_window(expression: &Expression) -> bool {
    let calls: Vec<&Call> = std::iter::once(expression.start())
        .filter_map(|start| match start {
            Start::Call(call) => Some(call),
            Start::Input(_) | Start::Hole => None,
        })
        .chain(expression.links())
        .collect();
    let windowed = |call: &Call| {
        is(call, "Expr.over")
            || operands(call)
                .into_iter()
                .any(|(_, inside)| any_call(inside, &|inner| is(inner, "Expr.over")))
    };
    let own = calls.iter().enumerate().any(|(at, call)| {
        is(call, "Expr.over") && calls[..at].iter().any(|before| windowed(before))
    });
    own || calls.iter().any(|call| {
        operands(call)
            .into_iter()
            .any(|(_, inside)| window_over_a_window(inside))
    })
}

/// Whether a refusal by Polars has no place inside the step: it refused the
/// rows or panicked, or refused the step as a whole.
fn unplaced(refusal: &Refusal) -> bool {
    refusal
        .place
        .as_ref()
        .is_none_or(|place| place.path().is_empty())
}

/// The rows of the sample tables the tables were measured on and the target
/// runs cases on.
const SAMPLE_ROWS: usize = 3;

/// What a row of the values a chain gives holds, as far as the sample says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cell {
    Null,
    Value,
    /// Not known to be either.
    Unknown,
}

/// What a call of a chain does to the rows it is given: the number of rows, where
/// each goes and which hold a null.
enum Effect {
    /// Each row stays where it is, null where it was null and a value where it was one:
    /// a running sum or maximum, and a part of a date.
    Keeps,
    /// Each row is a value, whatever it was.
    Values,
    /// The rows stay in place, and what they hold is not known: a comparison, a
    /// subtraction or a window. The number stays too, but where there is one row,
    /// which an aggregate leaves, the call gives it to each of the sample's rows, as
    /// what it meets may be a column of them.
    Blurs,
    /// What it gives is not known of the rows at all: another number, or another order.
    Lost,
}

fn effect(call: &Call) -> Effect {
    match call.entry().id().as_str() {
        "Expr.cum_sum"
        | "Expr.cum_max"
        | "Expr.abs"
        | "Expr.neg"
        | "Expr.not_"
        | "Expr.dt.date"
        | "Expr.dt.month_start"
        | "Expr.dt.time"
        | "Expr.dt.total_days" => Effect::Keeps,
        "Expr.is_null" | "Expr.is_not_null" | "Expr.cum_count" => Effect::Values,
        "Expr.eq"
        | "Expr.ne"
        | "Expr.gt"
        | "Expr.ge"
        | "Expr.lt"
        | "Expr.le"
        | "Expr.add"
        | "Expr.and_"
        | "Expr.floordiv"
        | "Expr.mod"
        | "Expr.mul"
        | "Expr.or_"
        | "Expr.pow"
        | "Expr.sub"
        | "Expr.truediv"
        | "Expr.xor"
        | "Expr.dt.offset_by"
        | "When.then"
        | "Then.otherwise"
        | "Then.when"
        | "ChainedWhen.then"
        | "ChainedThen.otherwise" => Effect::Blurs,
        "Expr.over" if !has_explode(call) => Effect::Blurs,
        _ => Effect::Lost,
    }
}

/// The rows a shift moves, `n` of them: positive down, negative up. The number is
/// 1 when it is not given, and not known when it is not written as a whole number.
fn shift_amount(link: &Call) -> Option<i64> {
    match link.argument("n") {
        None => Some(1),
        Some(Value::Expression(n)) if n.links().is_empty() => match n.start() {
            Start::Call(start) if is(start, "polars.lit") => match start.argument("value") {
                Some(Value::Integer(rows)) => Some(*rows),
                _ => None,
            },
            _ => None,
        },
        Some(_) => None,
    }
}

/// What the rows a shift moves in hold: the `fill_value`, a value where it is a
/// literal of one, a null where it is the null literal, and nothing known where it
/// is another expression; null when it is not given.
fn filled_with(link: &Call) -> Cell {
    match link.argument("fill_value") {
        None => Cell::Null,
        Some(Value::Expression(fill)) if fill.links().is_empty() => match fill.start() {
            Start::Call(start) if is(start, "polars.lit") => match start.argument("value") {
                Some(Value::Flag(None)) => Cell::Null,
                Some(_) => Cell::Value,
                None => Cell::Unknown,
            },
            _ => Cell::Unknown,
        },
        Some(_) => Cell::Unknown,
    }
}

/// `rows` after the shift `link`: each moves by `n`, and the rows left empty hold
/// what it fills them with.
fn shifted(rows: &[Cell], link: &Call) -> Option<Vec<Cell>> {
    let n = shift_amount(link)?;
    let (fill, count) = (filled_with(link), rows.len());
    let moved = usize::try_from(n.unsigned_abs()).unwrap_or(usize::MAX);
    Some(
        (0..count)
            .map(|row| {
                let from = if n >= 0 {
                    row.checked_sub(moved)
                } else {
                    row.checked_add(moved).filter(|from| *from < count)
                };
                from.map_or(fill, |from| rows[from])
            })
            .collect(),
    )
}

/// The one row that the first or last of `rows` is: the row itself, or with
/// `skipping`, the first or last that is not null.
fn picked(rows: &[Cell], skipping: bool, from_the_end: bool) -> Cell {
    let mut rows: Vec<Cell> = rows.to_vec();
    if from_the_end {
        rows.reverse();
    }
    if !skipping {
        return rows.first().copied().unwrap_or(Cell::Unknown);
    }
    rows.iter()
        .copied()
        .find(|cell| *cell != Cell::Null)
        .unwrap_or(Cell::Null)
}

/// The rows after the first `upto` links of `expression`, taken from the sample:
/// what each holds, by what the calls do to it. `None` when what the calls did to
/// the rows is not known.
fn rows_after(
    frames: &Frames,
    expression: &Expression,
    upto: usize,
    receiver: bool,
) -> Option<Vec<Cell>> {
    // A chain that starts from one column of the sample has the rows it holds, a
    // value or a null in each; any other start is the sample's number of rows, of
    // nothing known.
    let mut rows: Vec<Cell> = if receiver && starts_from_a_null_literal(expression) {
        vec![Cell::Null]
    } else if let Some(made) = rows_made(frames, expression) {
        made
    } else if starts_from_a_value_literal(expression) {
        // A literal is one value, whatever the sample's rows are.
        vec![Cell::Value]
    } else {
        (0..SAMPLE_ROWS)
            .map(|row| {
                starting_column(expression)
                    .and_then(|name| frames.holds_a_null_at(name, row))
                    .map_or(
                        Cell::Unknown,
                        |null| {
                            if null { Cell::Null } else { Cell::Value }
                        },
                    )
            })
            .collect()
    };
    for link in &expression.links()[..upto.min(expression.links().len())] {
        let skipping = matches!(link.argument("ignore_nulls"), Some(Value::Flag(Some(true))));
        rows = match link.entry().id().as_str() {
            "Expr.shift" => shifted(&rows, link)?,
            "Expr.first" => vec![picked(&rows, skipping, false)],
            "Expr.last" => vec![picked(&rows, skipping, true)],
            // The smallest and the largest of rows that are all null is null, and
            // of any that holds a value is a value. The sum of rows that are all
            // null is zero for numbers and null for a column of `Null`, so it is
            // known only where a row holds a value.
            "Expr.min" | "Expr.max" | "Expr.sum" => {
                let one = if rows.contains(&Cell::Value) {
                    Cell::Value
                } else if rows.contains(&Cell::Unknown) || link.entry().id().as_str() == "Expr.sum"
                {
                    Cell::Unknown
                } else {
                    Cell::Null
                };
                vec![one]
            }
            "Expr.len" => vec![Cell::Value],
            // A drop of the nulls leaves the rows that hold a value, which are none where they are
            // all null: a column of no rows meets nothing, as a null does not, so it stands for one
            // null here.
            "Expr.drop_nulls" if !rows.contains(&Cell::Unknown) => {
                let kept: Vec<Cell> = rows
                    .into_iter()
                    .filter(|cell| *cell != Cell::Null)
                    .collect();
                if kept.is_empty() {
                    vec![Cell::Null]
                } else {
                    kept
                }
            }
            // A fill of NaN keeps the null of a row that is one, and gives the value or the row
            // itself to the others; a cast keeps a null too, and gives a null or a value to the
            // others.
            "Expr.fill_nan" | "Expr.cast" => receiver_nulls_stay(frames, &rows, link),
            id if carries_nulls(id) => carried(frames, &rows, link),
            _ => match effect(link) {
                Effect::Keeps => rows,
                Effect::Values => vec![Cell::Value; rows.len()],
                Effect::Blurs => {
                    let count = if rows.len() == 1 {
                        SAMPLE_ROWS
                    } else {
                        rows.len()
                    };
                    vec![Cell::Unknown; count]
                }
                Effect::Lost => return None,
            },
        };
    }
    Some(rows)
}

/// Whether the call `id` gives a null in each row where it is given one, in the
/// rows it follows or in an operand: a comparison and arithmetic do. `and` and `or` do
/// not, for a null with a true is a true. `xor` does, and is left out, which only
/// excuses less. A call that chooses between operands does not either.
fn carries_nulls(id: &str) -> bool {
    matches!(
        id,
        "Expr.eq"
            | "Expr.ne"
            | "Expr.gt"
            | "Expr.ge"
            | "Expr.lt"
            | "Expr.le"
            | "Expr.add"
            | "Expr.sub"
            | "Expr.mul"
            | "Expr.truediv"
            | "Expr.floordiv"
            | "Expr.mod"
            | "Expr.pow"
    )
}

/// The rows of a call that carries nulls, which follows `rows` and is given
/// operands: null where the rows it follows hold a null, or an operand does,
/// and not known elsewhere. An operand of one row meets each row, and an operand of
/// which nothing is known, or of another number of rows, carries nothing.
fn carried(frames: &Frames, rows: &[Cell], link: &Call) -> Vec<Cell> {
    let held: Vec<Vec<Cell>> = operands(link)
        .into_iter()
        .filter_map(|(_, operand)| rows_after(frames, operand, operand.links().len(), false))
        .collect();
    let count = held
        .iter()
        .map(Vec::len)
        .chain(std::iter::once(rows.len()))
        .max()
        .map_or(SAMPLE_ROWS, |most| most.max(1));
    let nulls_at = |cells: &[Cell], row: usize| match cells {
        [only] => *only == Cell::Null,
        cells => cells.get(row) == Some(&Cell::Null),
    };
    let fits = |cells: &Vec<Cell>| cells.len() == 1 || cells.len() == count;
    (0..count)
        .map(|row| {
            if (rows.len() == 1 || rows.len() == count) && nulls_at(rows, row)
                || held.iter().any(|cells| fits(cells) && nulls_at(cells, row))
            {
                Cell::Null
            } else {
                Cell::Unknown
            }
        })
        .collect()
}

/// Whether the chain starts from a literal that holds a value, a number, a text or a flag, and
/// not a null.
fn starts_from_a_value_literal(expression: &Expression) -> bool {
    matches!(expression.start(), Start::Call(start)
    if is(start, "polars.lit")
        && matches!(
            start.argument("value"),
            Some(Value::Integer(_) | Value::Float(_) | Value::String(_) | Value::Flag(Some(_)))
        ))
}

/// Whether `expression` is the null literal, which is one null that every row meets, filled for
/// NaN as its first call: that fill keeps the null of what it follows whatever it is given, so
/// the rows are nulls, which are the sample's only where a column of `Null` is the start.
fn starts_from_a_null_literal(expression: &Expression) -> bool {
    matches!(expression.start(), Start::Call(start)
        if is(start, "polars.lit") && matches!(start.argument("value"), Some(Value::Flag(None))))
        && expression
            .links()
            .first()
            .is_some_and(|link| is(link, "Expr.fill_nan"))
}

/// The rows of a call that keeps the nulls of what it follows, as a fill of NaN and a cast do,
/// and gives another row something not known: a null where `rows` hold one. An operand that is
/// the value it fills with sets the number of rows, which `rows` of one row meet each of.
fn receiver_nulls_stay(frames: &Frames, rows: &[Cell], link: &Call) -> Vec<Cell> {
    let count = operands(link)
        .into_iter()
        .filter_map(|(_, operand)| rows_after(frames, operand, operand.links().len(), false))
        .map(|held| held.len())
        .chain(std::iter::once(rows.len()))
        .max()
        .map_or(SAMPLE_ROWS, |most| most.max(1));
    (0..count)
        .map(|row| match rows {
            [only] if *only == Cell::Null => Cell::Null,
            rows if rows.len() == count && rows[row] == Cell::Null => Cell::Null,
            _ => Cell::Unknown,
        })
        .collect()
}

/// The rows of a chain that starts from a function that makes a date, a time or a moment of parts,
/// or a moment of a count, where the rows are known: a row is null where a part is, and the text of
/// a column is no number, so a part that is a column of text is null in each row. A count since
/// 1970 keeps the nulls of the rows it counts. `None` for any other start, or where a part is not a
/// column or a number typed in.
fn rows_made(frames: &Frames, expression: &Expression) -> Option<Vec<Cell>> {
    let Start::Call(start) = expression.start() else {
        return None;
    };
    if is(start, "polars.from_epoch") {
        return match start.argument("column") {
            Some(Value::Expression(counted)) => {
                rows_after(frames, counted, counted.links().len(), false)
            }
            _ => None,
        };
    }
    if !(is(start, "polars.date") || is(start, "polars.time") || is(start, "polars.datetime")) {
        return None;
    }
    let mut rows = vec![Cell::Value; SAMPLE_ROWS];
    for part in PARTS_OF_A_DATE {
        let Some(given) = start.argument(part) else {
            continue;
        };
        let Value::Expression(part) = given else {
            return None;
        };
        let cells: Vec<Cell> = if part.links().is_empty()
            && matches!(part.start(), Start::Call(call)
                if is(call, "polars.lit")
                    && matches!(call.argument("value"), Some(Value::Integer(_) | Value::Float(_))))
        {
            vec![Cell::Value; SAMPLE_ROWS]
        } else if let Some(name) = starting_column(part).filter(|_| part.links().is_empty()) {
            if matches!(
                frames.dtype_of(name),
                Some(polars::prelude::DataType::String)
            ) {
                vec![Cell::Null; SAMPLE_ROWS]
            } else {
                (0..SAMPLE_ROWS)
                    .map(|row| match frames.holds_a_null_at(name, row) {
                        Some(true) => Cell::Null,
                        Some(false) => Cell::Value,
                        None => Cell::Unknown,
                    })
                    .collect()
            }
        } else {
            vec![Cell::Unknown; SAMPLE_ROWS]
        };
        for (row, cell) in rows.iter_mut().zip(cells) {
            *row = match (*row, cell) {
                (Cell::Null, _) | (_, Cell::Null) => Cell::Null,
                (Cell::Unknown, _) | (_, Cell::Unknown) => Cell::Unknown,
                (Cell::Value, Cell::Value) => Cell::Value,
            };
        }
    }
    Some(rows)
}

/// The name of the one column `expression` starts from, when it starts from one.
fn starting_column(expression: &Expression) -> Option<&str> {
    match expression.start() {
        Start::Call(start) if is(start, "polars.col") => match start.argument("name") {
            Some(Value::Columns(names)) => match names.as_slice() {
                [name] => Some(name),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    }
}

/// Whether the first `upto` links of `expression`, taken from the sample, leave only
/// nulls: shifts that move all its rows, in either direction, or the row that a
/// shift left null and a first, last, minimum or maximum then takes, or the one
/// value that an aggregate leaves, shifted away (`rows_after` follows each call:
/// the sample's columns hold their values and a null in the middle, or none, and a
/// call that is not known to keep the rows where they are, or the nulls in them,
/// leaves nothing known).
fn leaves_only_nulls(
    frames: &Frames,
    expression: &Expression,
    upto: usize,
    receiver: bool,
) -> bool {
    rows_after(frames, expression, upto, receiver)
        .is_some_and(|rows| !rows.is_empty() && rows.iter().all(|cell| *cell == Cell::Null))
}

/// Whether `expression` starts from a function that makes a value of what it is given, as the
/// ones that make a date, a moment or a length of time do, and what it is given reads no column and
/// no group's values: the value is made of literals alone.
fn starts_from_a_function_of_literals(expression: &Expression) -> bool {
    matches!(expression.start(), Start::Call(start)
    if start.entry().applies() == catalog::Applies::Function
        && !is(start, "polars.col")
        && !is(start, "polars.lit")
        && !is(start, "polars.element")
        && !operands(start).is_empty()
        && !operands(start).into_iter().any(|(_, operand)| {
            any_call(operand, &|call| is(call, "polars.col") || is(call, "polars.element"))
        }))
}

/// Whether the first `upto` links of `expression`, run alone in a select on the sample, are
/// converted without a failure: Polars then makes the value and the conversion where no group
/// is, so a group's kernels are no cause. A select that Polars refuses is no proof.
fn converts_when_selected(frames: &Frames, expression: &Expression, upto: usize) -> bool {
    let Ok(part) = Expression::new(
        expression.start().clone(),
        expression.links()[..upto.min(expression.links().len())].to_vec(),
    ) else {
        return false;
    };
    let (Some(select), Ok(id)) = (catalog::entry("LazyFrame.select"), StepId::new("s")) else {
        return false;
    };
    let Ok(call) = Call::new(
        select,
        vec![(
            "named_exprs",
            Value::Pairs(vec![(
                Value::Columns(vec!["out".to_owned()]),
                Value::Expression(part),
            )]),
        )],
    ) else {
        return false;
    };
    let Ok(step) = Step::new(id.clone(), call) else {
        return false;
    };
    let Ok(document) = Document::new(vec![step]) else {
        return false;
    };
    asked::first_rows(&document, frames).is_some()
}

/// Whether Polars gives a null in each row of the left table for the first `upto` links of
/// `expression`, which is run alone in a select on the sample: a call that the rows the target
/// follows know nothing of, as a slice by an offset that is no number, may give only nulls.
fn gives_only_nulls(frames: &Frames, expression: &Expression, upto: usize) -> bool {
    // A null literal is one that Polars folds, whatever the rows hold, which a probe settles: the
    // nulls here are those of the sample's rows.
    // A condition that no row takes gives a null that is no value of the rows: what is in its
    // branches is not met, which is a part that Polars skips and a probe settles.
    if any_call(expression, &|call| {
        (is(call, "polars.lit") && matches!(call.argument("value"), Some(Value::Flag(None))))
            || call.entry().applies() == catalog::Applies::Condition
    }) {
        return false;
    }
    let Ok(part) = Expression::new(
        expression.start().clone(),
        expression.links()[..upto.min(expression.links().len())].to_vec(),
    ) else {
        return false;
    };
    let (Some(select), Ok(id)) = (catalog::entry("LazyFrame.select"), StepId::new("s")) else {
        return false;
    };
    let Ok(call) = Call::new(
        select,
        vec![(
            "named_exprs",
            Value::Pairs(vec![(
                Value::Columns(vec!["out".to_owned()]),
                Value::Expression(part),
            )]),
        )],
    ) else {
        return false;
    };
    let Ok(step) = Step::new(id.clone(), call) else {
        return false;
    };
    let Ok(document) = Document::new(vec![step]) else {
        return false;
    };
    asked::first_rows(&document, frames).is_some_and(|rows| {
        rows.height() > 0
            && rows
                .columns()
                .first()
                .is_some_and(|column| column.null_count() == rows.height())
    })
}

/// Whether a call undoes the argument `parameter` of the call `call`.
type Undo<'a> = &'a dyn Fn(&Call, &str) -> bool;

/// `value` with every argument `undo` names undone, in the calls it holds.
fn value_without(value: &Value, undo: Undo<'_>) -> Value {
    match value {
        Value::Expression(expression) => Value::Expression(
            expression_without(expression, undo).unwrap_or_else(|| expression.clone()),
        ),
        Value::Many(items) => {
            Value::Many(items.iter().map(|item| value_without(item, undo)).collect())
        }
        Value::Pairs(pairs) => Value::Pairs(
            pairs
                .iter()
                .map(|(key, entry)| (value_without(key, undo), value_without(entry, undo)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// `call` with the arguments `undo` names, and those in the calls it holds, undone.
fn call_without(call: &Call, undo: Undo<'_>) -> Option<Call> {
    let given: Vec<(&str, Value)> = call
        .arguments()
        .iter()
        .filter(|given| !undo(call, given.parameter()))
        .map(|given| (given.parameter(), value_without(given.value(), undo)))
        .collect();
    Call::new(catalog::entry(call.entry().id().as_str())?, given).ok()
}

/// `expression` with the arguments `undo` names undone.
fn expression_without(expression: &Expression, undo: Undo<'_>) -> Option<Expression> {
    let start = match expression.start() {
        Start::Call(call) => Start::Call(call_without(call, undo)?),
        other => other.clone(),
    };
    let links = expression
        .links()
        .iter()
        .map(|link| call_without(link, undo))
        .collect::<Option<Vec<_>>>()?;
    Expression::new(start, links).ok()
}

/// The case with the arguments `undo` names undone, the same in all else.
fn case_without(case: &Case, undo: Undo<'_>) -> Option<Case> {
    let step = &case.document.steps()[0];
    let rebuilt = Step::new(step.id().clone(), call_without(step.call(), undo)?).ok()?;
    Some(Case {
        receiver: case.receiver,
        argument: case.argument,
        document: Document::new(vec![rebuilt]).ok()?,
    })
}

/// The case with no window that explodes: each is the window the same call makes when it is not
/// told how to map.
fn case_without_explodes(case: &Case) -> Option<Case> {
    case_without(case, &|call, parameter| {
        has_explode(call) && parameter == "mapping_strategy"
    })
}

/// A null literal, which every dtype holds, as an expression.
fn a_null_literal() -> Option<Expression> {
    let literal = Call::new(
        catalog::entry("polars.lit")?,
        vec![("value", Value::Flag(None))],
    )
    .ok()?;
    Expression::new(Start::Call(literal), Vec::new()).ok()
}

/// Whether `parameter` of the call is a value that Polars casts to the dtype of what the call
/// follows, as the values a replacement lists are.
fn is_a_replaced_value(call: &Call, parameter: &str) -> bool {
    matches!(
        call.entry().id().as_str(),
        "Expr.replace" | "Expr.replace_strict"
    ) && matches!(parameter, "old" | "new")
}

/// The case with no bound given to any `clip`, which is then the expression it follows, and the
/// values of every replacement made null, which every dtype holds, for a replacement is no
/// expression without them.
fn case_without_bounds(case: &Case) -> Option<Case> {
    case_changed(case, &|call| {
        let replaced = is_a_replaced_value(call, "old");
        let clipped = is(call, "Expr.clip");
        if !replaced && !clipped {
            return None;
        }
        let given: Vec<(&str, Value)> = call
            .arguments()
            .iter()
            .filter(|given| {
                !(clipped && matches!(given.parameter(), "lower_bound" | "upper_bound"))
            })
            .map(|given| {
                if replaced && is_a_replaced_value(call, given.parameter()) {
                    let null = a_null_literal().expect("a null literal");
                    (given.parameter(), Value::Expression(null))
                } else {
                    (given.parameter(), given.value().clone())
                }
            })
            .collect();
        Call::new(catalog::entry(call.entry().id().as_str())?, given).ok()
    })
}

/// `call` and the calls it holds, each made into what `change` gives it, which is the call itself
/// where it is `None`.
fn call_changed(call: &Call, change: &dyn Fn(&Call) -> Option<Call>) -> Option<Call> {
    let given: Vec<(&str, Value)> = call
        .arguments()
        .iter()
        .map(|given| (given.parameter(), value_changed(given.value(), change)))
        .collect();
    let rebuilt = Call::new(catalog::entry(call.entry().id().as_str())?, given).ok()?;
    Some(change(&rebuilt).unwrap_or(rebuilt))
}

fn value_changed(value: &Value, change: &dyn Fn(&Call) -> Option<Call>) -> Value {
    match value {
        Value::Expression(expression) => Value::Expression(
            expression_changed(expression, change).unwrap_or_else(|| expression.clone()),
        ),
        Value::Many(items) => Value::Many(
            items
                .iter()
                .map(|item| value_changed(item, change))
                .collect(),
        ),
        Value::Pairs(pairs) => Value::Pairs(
            pairs
                .iter()
                .map(|(key, entry)| (value_changed(key, change), value_changed(entry, change)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn expression_changed(
    expression: &Expression,
    change: &dyn Fn(&Call) -> Option<Call>,
) -> Option<Expression> {
    let start = match expression.start() {
        Start::Call(call) => Start::Call(call_changed(call, change)?),
        other => other.clone(),
    };
    let links = expression
        .links()
        .iter()
        .map(|link| call_changed(link, change))
        .collect::<Option<Vec<_>>>()?;
    Expression::new(start, links).ok()
}

/// The case with every call that `change` gives another call made into it, the same in all else.
pub fn case_changed(case: &Case, change: &dyn Fn(&Call) -> Option<Call>) -> Option<Case> {
    let step = &case.document.steps()[0];
    let rebuilt = Step::new(step.id().clone(), call_changed(step.call(), change)?).ok()?;
    Some(Case {
        receiver: case.receiver,
        argument: case.argument,
        document: Document::new(vec![rebuilt]).ok()?,
    })
}

/// Whether the call is a difference, which converts unsigned numbers to the signed dtype of its
/// result, strictly.
fn is_a_difference(call: &Call) -> bool {
    is(call, "Expr.diff")
}

/// Whether the reason is the failed conversion of an unsigned number to a signed dtype, as a
/// difference makes where a subtraction wrapped to a number beyond it.
fn is_an_unsigned_conversion_to_a_signed_one(reason: &str) -> bool {
    reason.starts_with("conversion from `u") && reason.contains("` to `i")
}

/// Whether the call converts strictly, so that it refuses a value its dtype cannot hold: a `cast`
/// that is neither told to wrap numbers that overflow nor to give a null where a value does not
/// convert, a test of closeness and a fill by a value or of NaN, which convert what they meet to
/// the one dtype that holds both as a strict cast does, and a strict replacement asked for a dtype
/// and given no default.
fn is_a_strict_cast(call: &Call) -> bool {
    is_a_count_of_characters(call)
        || is(call, "Expr.is_close")
        || (is(call, "Expr.fill_null") && call.argument("value").is_some())
        || is(call, "Expr.fill_nan")
        // Given no default it converts what replaces to the dtype strictly, and given one it
        // converts as a cast that gives a null does, which refuses no value.
        || (is(call, "Expr.replace_strict")
            && call.argument("return_dtype").is_some()
            && call.argument("default").is_none())
        || (is(call, "Expr.cast")
            && !matches!(
                call.argument("wrap_numerical"),
                Some(Value::Flag(Some(true)))
            )
            && !matches!(call.argument("strict"), Some(Value::Flag(Some(false)))))
}

/// The methods of text that take a count of characters, and the parameters that hold it: Polars
/// converts a count to a whole number strictly, to a signed one for where to start and how many
/// to keep from the start or the end, and to an unsigned one for how long, so that a negative
/// length, the infinity or the not-a-number of a float, or a value out of the range is refused
/// when the rows run.
const COUNTS_OF_CHARACTERS: [(&str, &[&str]); 6] = [
    ("Expr.str.head", &["n"]),
    ("Expr.str.pad_end", &["length"]),
    ("Expr.str.pad_start", &["length"]),
    ("Expr.str.slice", &["offset", "length"]),
    ("Expr.str.tail", &["n"]),
    ("Expr.str.zfill", &["length"]),
];

/// The parameters of the call that hold a count of characters, which are given, if it is a method
/// of text that takes one.
fn counts_of(call: &Call) -> Vec<&'static str> {
    COUNTS_OF_CHARACTERS
        .iter()
        .filter(|(id, _)| is(call, id))
        .flat_map(|(_, names)| names.iter().copied())
        .filter(|name| call.argument(name).is_some())
        .collect()
}

/// Whether the call is a method of text that is given a count of characters.
fn is_a_count_of_characters(call: &Call) -> bool {
    !counts_of(call).is_empty()
}

/// The whole number 1 as a literal, which every count converts without a failure.
fn the_number_one() -> Option<Expression> {
    let literal = Call::new(
        catalog::entry("polars.lit")?,
        vec![("value", Value::Integer(1))],
    )
    .ok()?;
    Expression::new(Start::Call(literal), Vec::new()).ok()
}

/// The call with the count of every parameter that holds one made the whole number 1, which
/// converts without a failure.
fn without_counts(call: &Call) -> Option<Call> {
    let counts = counts_of(call);
    let one = the_number_one()?;
    let given: Vec<(&str, Value)> = call
        .arguments()
        .iter()
        .map(|given| {
            let value = if counts.contains(&given.parameter()) {
                Value::Expression(one.clone())
            } else {
                given.value().clone()
            };
            (given.parameter(), value)
        })
        .collect();
    Call::new(catalog::entry(call.entry().id().as_str())?, given).ok()
}

/// The case with no strict cast: each gives a null where a value does not convert, a test of
/// closeness is the comparison of what it compares and a fill by a value a fill forward, which
/// convert nothing, a count of characters is the whole number 1, which converts without a failure,
/// and a replacement is not asked for a dtype, which converts nothing. Where `differences`, each
/// difference is a fill forward too, which converts nothing.
fn case_without_strict_casts(case: &Case, differences: bool) -> Option<Case> {
    case_changed(case, &|call| {
        if differences && is_a_difference(call) {
            return Call::new(catalog::entry("Expr.forward_fill")?, Vec::new()).ok();
        }
        if !is_a_strict_cast(call) {
            return None;
        }
        if is_a_count_of_characters(call) {
            return without_counts(call);
        }
        if is(call, "Expr.is_close") {
            let other = call.argument("other")?.clone();
            return Call::new(catalog::entry("Expr.gt")?, vec![("other", other)]).ok();
        }
        if is(call, "Expr.fill_null") || is(call, "Expr.fill_nan") {
            return Call::new(catalog::entry("Expr.forward_fill")?, Vec::new()).ok();
        }
        let strict = is(call, "Expr.cast");
        let given: Vec<(&str, Value)> = call
            .arguments()
            .iter()
            .filter(|given| given.parameter() != if strict { "strict" } else { "return_dtype" })
            .map(|given| (given.parameter(), given.value().clone()))
            .chain(strict.then(|| ("strict", Value::Flag(Some(false)))))
            .collect();
        Call::new(catalog::entry(call.entry().id().as_str())?, given).ok()
    })
}

/// Whether Polars refuses the case for a failed conversion still when no cast is strict: the
/// cast is then no cause of it. Which of two failures of a plan Polars names depends on which
/// thread gets there first, so the case is run again.
fn refused_for_a_failed_conversion_without_strict_casts(
    case: &Case,
    differences: bool,
) -> Option<bool> {
    changed(case_without_strict_casts(case, differences), case).map(|control| {
        (0..RUNS).any(|_| {
            observe(&control).refusal.is_some_and(|refusal| {
                unplaced(&refusal) && is_a_failed_conversion(&refusal.reason)
            })
        })
    })
}

/// The control, if it is a case other than the one asked about: where nothing in the case is
/// what the control undoes, it is the case itself, which shows nothing of its cause.
pub fn changed(control: Option<Case>, case: &Case) -> Option<Case> {
    control.filter(|control| control.document.write() != case.document.write())
}

/// Whether the reason is one that only a strict replacement given no default is refused for.
fn is_a_strict_replacement_refusal(reason: &str) -> bool {
    reason.contains("incomplete mapping")
        || reason.contains("cannot use values of type")
        || reason.contains("must specify which values to replace")
}

/// The case with each strict replacement given no default made one that is given a null default,
/// which leaves no value unmapped.
fn case_with_defaults(case: &Case) -> Option<Case> {
    case_changed(case, &|call| {
        if !is(call, "Expr.replace_strict") || call.argument("default").is_some() {
            return None;
        }
        let mut given: Vec<(&str, Value)> = call
            .arguments()
            .iter()
            .map(|given| (given.parameter(), given.value().clone()))
            .collect();
        given.push(("default", Value::Expression(a_null_literal()?)));
        Call::new(catalog::entry("Expr.replace_strict")?, given).ok()
    })
}

/// Whether Polars refuses the case for an incomplete mapping, or for values that cannot replace
/// the column's, still when each strict replacement has a default: the default is then no cause. `None` where no replacement lacks one.
fn refused_for_an_incomplete_mapping_with_defaults(case: &Case, reason: &str) -> Option<bool> {
    changed(case_with_defaults(case), case).map(|control| {
        (0..RUNS).any(|_| {
            observe(&control).refusal.is_some_and(|refusal| {
                is_a_strict_replacement_refusal(&refusal.reason) || refusal.reason == reason
            })
        })
    })
}

/// Whether the call replaces every match: `str.replace_all`, and `str.replace` given a count below
/// zero, which Polars takes to be all of them.
fn replaces_every_match(call: &Call) -> bool {
    is(call, "Expr.str.replace_all")
        || (is(call, "Expr.str.replace")
            && matches!(call.argument("n"), Some(Value::Integer(count)) if *count < 0))
}

/// What Polars says of a replacement of every match whose value is null where it reads it as one.
const A_NULL_VALUE_OF_A_REPLACEMENT: &str = "value cannot be 'null' in 'replace' expression";

/// Whether the reason is that a moment could not be converted to one counted in nanoseconds.
fn is_a_failed_conversion_to_nanoseconds(reason: &str) -> bool {
    is_a_failed_conversion(reason) && reason.contains("` to `datetime[ns")
}

/// Whether the call changes the unit of a moment to nanoseconds.
fn is_a_cast_to_nanoseconds(call: &Call) -> bool {
    is(call, "Expr.dt.cast_time_unit")
        && matches!(call.argument("time_unit"), Some(Value::Choice(word)) if word == "ns")
}

/// The case with each cast to nanoseconds made a cast to milliseconds.
fn case_with_milliseconds(case: &Case) -> Option<Case> {
    case_changed(case, &|call| {
        if !is_a_cast_to_nanoseconds(call) {
            return None;
        }
        Call::new(
            call.entry(),
            vec![("time_unit", Value::Choice("ms".to_owned()))],
        )
        .ok()
    })
}

/// Whether Polars refuses the case for a conversion to nanoseconds still when each cast is to
/// milliseconds: the unit is then no cause. `None` where no cast is to nanoseconds.
fn refused_for_nanoseconds_with_milliseconds(case: &Case) -> Option<bool> {
    changed(case_with_milliseconds(case), case).map(|control| {
        (0..RUNS).any(|_| {
            observe(&control)
                .refusal
                .is_some_and(|refusal| is_a_failed_conversion_to_nanoseconds(&refusal.reason))
        })
    })
}

/// The parts `dt.replace` and the functions that make a date, a moment or a time are given, which
/// they convert to the whole numbers of the calendar.
const PARTS_OF_A_DATE: [&str; 7] = [
    "year",
    "month",
    "day",
    "hour",
    "minute",
    "second",
    "microsecond",
];

/// The parts `duration` is given, which it converts to whole numbers as the calendar's are.
const PARTS_OF_A_LENGTH_OF_TIME: [&str; 8] = [
    "weeks",
    "days",
    "hours",
    "minutes",
    "seconds",
    "milliseconds",
    "microseconds",
    "nanoseconds",
];

/// The parts that the call is given to convert: a length of time's, or a date's.
fn parts_of(call: &Call) -> &'static [&'static str] {
    if is(call, "polars.duration") {
        &PARTS_OF_A_LENGTH_OF_TIME
    } else {
        &PARTS_OF_A_DATE
    }
}

/// Whether the parameter of the call holds a part that it converts.
fn is_a_part_of(call: &Call, parameter: &str) -> bool {
    parts_of(call).contains(&parameter)
}

/// Whether the reason is that the components of a date or of a time are invalid, as a day of 0 or
/// a minute below zero are.
fn is_a_refusal_of_the_components_of_a_date(reason: &str) -> bool {
    reason.contains("Invalid date components") || reason.contains("Invalid time components")
}

/// Whether the reason is one that Polars refuses a part of a date with: invalid components, or a
/// value that does not convert to the whole number of the part, as the NaN of an arc is not.
fn is_a_refusal_of_a_part_of_a_date(reason: &str) -> bool {
    is_a_refusal_of_the_components_of_a_date(reason) || is_a_failed_conversion(reason)
}

/// Whether the call is given a part of a date or a time to convert: a `dt.replace`, or one of the
/// functions that make a date, a moment, a time or a length of time of parts, which convert each
/// as it does.
fn is_given_a_part_of_a_date(call: &Call) -> bool {
    (is(call, "Expr.dt.replace")
        || is(call, "polars.date")
        || is(call, "polars.datetime")
        || is(call, "polars.time")
        || is(call, "polars.duration"))
        && parts_of(call)
            .iter()
            .any(|part| call.argument(part).is_some())
}

/// The case with each part that a call is given made the number 1.
fn case_with_ones_as_the_parts(case: &Case) -> Option<Case> {
    let one = Expression::new(
        Start::Call(
            Call::new(
                catalog::entry("polars.lit")?,
                vec![("value", Value::Integer(1))],
            )
            .ok()?,
        ),
        Vec::new(),
    )
    .ok()?;
    case_changed(case, &|call| {
        if !is_given_a_part_of_a_date(call) {
            return None;
        }
        let given: Vec<(&str, Value)> = call
            .arguments()
            .iter()
            .map(|given| {
                let value = if is_a_part_of(call, given.parameter()) {
                    Value::Expression(one.clone())
                } else {
                    given.value().clone()
                };
                (given.parameter(), value)
            })
            .collect();
        Call::new(call.entry(), given).ok()
    })
}

/// Whether Polars refuses the case for what `reason` says still when each part is the number 1: the
/// values of the part are then no cause. The reason is the case's own kind, for a case refused for
/// the components of a date is not shown to be refused for a part by being refused for the cast
/// of the date that the ones make. `None` where no call is given a part.
fn refused_for_a_part_of_a_date_with_ones(case: &Case, reason: fn(&str) -> bool) -> Option<bool> {
    changed(case_with_ones_as_the_parts(case), case).map(|control| {
        (0..RUNS).any(|_| {
            observe(&control)
                .refusal
                .is_some_and(|refusal| reason(&refusal.reason))
        })
    })
}

/// Whether the reason is that text could not be converted to the whole number of a part, as the
/// text of a column of durations is not.
fn is_a_failed_conversion_of_text(reason: &str) -> bool {
    is_a_failed_conversion(reason) && reason.starts_with("conversion from `str` to `")
}

/// Whether the expression is a literal of text, as `"1"` is, and nothing more.
fn is_a_literal_of_text(expression: &Expression) -> bool {
    expression.links().is_empty()
        && matches!(expression.start(), Start::Call(start)
            if is(start, "polars.lit")
                && (matches!(start.argument("value"), Some(Value::String(_)))
                    || matches!(start.argument("dtype"), Some(Value::Dtype(dtype)) if dtype.to_string() == "String")))
}

/// Whether the call is given parts that are each a literal, one of them of text, as `"1"` is, which
/// Polars converts to the number where the plan is made, whether or not it evaluates the part,
/// where the tables, which measured a column of text that is none, refuse it. A part that is a
/// column or a chain is not one: Polars converts it when it evaluates the part, which it may not.
fn is_given_only_literals_one_of_text(call: &Call) -> bool {
    let literal = |expression: &Expression| {
        is_a_literal_of_text(expression)
            || (expression.links().is_empty()
                && matches!(expression.start(), Start::Call(start) if is(start, "polars.lit")))
    };
    let mut text = false;
    is_given_a_part_of_a_date(call)
        && PARTS_OF_A_DATE
            .iter()
            .all(|part| match call.argument(part) {
                Some(Value::Expression(expression)) => {
                    text |= is_a_literal_of_text(expression);
                    literal(expression)
                }
                Some(_) => false,
                None => true,
            })
        && text
}

/// The case with each part that a call is given that is a literal of text made the literal
/// `value`, and the other parts as they are.
fn case_with_the_text_parts_as(case: &Case, value: Value) -> Option<Case> {
    let literal = Expression::new(
        Start::Call(Call::new(catalog::entry("polars.lit")?, vec![("value", value)]).ok()?),
        Vec::new(),
    )
    .ok()?;
    case_changed(case, &|call| {
        if !is_given_a_part_of_a_date(call) {
            return None;
        }
        let given: Vec<(&str, Value)> = call
            .arguments()
            .iter()
            .map(|given| {
                let value = match given.value() {
                    Value::Expression(part)
                        if is_a_part_of(call, given.parameter()) && is_a_literal_of_text(part) =>
                    {
                        Value::Expression(literal.clone())
                    }
                    other => other.clone(),
                };
                (given.parameter(), value)
            })
            .collect();
        Call::new(call.entry(), given).ok()
    })
}

/// Whether Polars refuses the case for what `reason` says with each literal of text that a part is
/// made the text `x`, which no part reads as a number: it then evaluates the part, and what it
/// refuses is a cause. `None` where no call is given a part.
fn refused_for_a_text_part_as_x(case: &Case, reason: fn(&str) -> bool) -> Option<bool> {
    changed(
        case_with_the_text_parts_as(case, Value::String("x".to_owned())),
        case,
    )
    .map(|control| {
        (0..RUNS).any(|_| {
            observe(&control)
                .refusal
                .is_some_and(|refusal| reason(&refusal.reason))
        })
    })
}

/// The case with each part that a call is given made the literal `value`.
fn case_with_the_parts_as(case: &Case, value: Value) -> Option<Case> {
    let literal = Expression::new(
        Start::Call(Call::new(catalog::entry("polars.lit")?, vec![("value", value)]).ok()?),
        Vec::new(),
    )
    .ok()?;
    case_changed(case, &|call| {
        if !is_given_a_part_of_a_date(call) {
            return None;
        }
        let given: Vec<(&str, Value)> = call
            .arguments()
            .iter()
            .map(|given| {
                let value = if is_a_part_of(call, given.parameter()) {
                    Value::Expression(literal.clone())
                } else {
                    given.value().clone()
                };
                (given.parameter(), value)
            })
            .collect();
        Call::new(call.entry(), given).ok()
    })
}

/// Whether Polars refuses the case for what `reason` says with each part the literal `value`: it
/// then evaluates the part, and what it refuses is a cause. Polars that never evaluates a part
/// allows the case. `None` where no call is given a part.
fn refused_for_a_part_with(case: &Case, value: Value, reason: fn(&str) -> bool) -> Option<bool> {
    changed(case_with_the_parts_as(case, value), case).map(|control| {
        (0..RUNS).any(|_| {
            observe(&control)
                .refusal
                .is_some_and(|refusal| reason(&refusal.reason))
        })
    })
}

/// Whether the tables stop refusing the case for what `reason` says once each part is the number 1,
/// which they then judge as the column of whole numbers they measured. The case may be refused for
/// another reason still, as a pivot's aggregate is by its rule.
fn the_tables_stop_refusing_the_parts_as_ones(case: &Case, reason: fn(&str) -> bool) -> bool {
    changed(case_with_the_parts_as(case, Value::Integer(1)), case).is_some_and(|control| {
        !matches!(
            predict(&control).verdict,
            Verdict::Refuses(refused) if reason(&refused.reason)
        )
    })
}

/// Whether the reason is one that Polars refuses a format with: that it cannot format the dtype
/// with it, that a duration has no such format, or that it is not a format of a method that
/// writes text.
fn is_a_format_refusal(reason: &str) -> bool {
    reason.contains("cannot format ")
        || reason.contains("not supported for Duration type")
        || reason.contains("is not a valid `to_string` format")
}

/// Whether the call writes a moment as text, given a format that is typed in.
fn is_a_writing_with_a_format(call: &Call) -> bool {
    (is(call, "Expr.dt.strftime") || is(call, "Expr.dt.to_string"))
        && matches!(call.argument("format"), Some(Value::String(_)))
}

/// The case with the format of each writing made `iso`.
fn case_with_iso_formats(case: &Case) -> Option<Case> {
    case_changed(case, &|call| {
        if !is_a_writing_with_a_format(call) {
            return None;
        }
        Call::new(
            catalog::entry(call.entry().id().as_str())?,
            vec![("format", Value::String("iso".to_owned()))],
        )
        .ok()
    })
}

/// Whether Polars refuses the case for a format still when each is `iso`: the format is then no
/// cause. `None` where no writing is given a format.
fn refused_for_a_format_with_iso(case: &Case) -> Option<bool> {
    changed(case_with_iso_formats(case), case).map(|control| {
        (0..RUNS).any(|_| {
            observe(&control)
                .refusal
                .is_some_and(|refusal| is_a_format_refusal(&refusal.reason))
        })
    })
}

/// What Polars says of a replacement whose value has other numbers of values than the text it
/// follows.
const A_VALUE_OF_ANOTHER_LENGTH: &str = "does not match string column length";

/// Whether `expression` replaces what a pattern matches in a chain that starts from a literal,
/// in it or in an expression it holds.
fn holds_a_replacement_of_a_literal(expression: &Expression) -> bool {
    let replaces = |link: &Call| is(link, "Expr.str.replace") || is(link, "Expr.str.replace_all");
    let starts_from_a_text =
        matches!(expression.start(), Start::Call(start) if is(start, "polars.lit"));
    (starts_from_a_text && expression.links().iter().any(replaces))
        || std::iter::once(expression.start())
            .filter_map(|start| match start {
                Start::Call(call) => Some(call),
                Start::Input(_) | Start::Hole => None,
            })
            .chain(expression.links())
            .any(|call| {
                operands(call)
                    .into_iter()
                    .any(|(_, inside)| holds_a_replacement_of_a_literal(inside))
            })
}

/// `call` with the start of each chain that is a literal and holds a replacement made the column
/// `g`, which holds text in every row.
fn call_with_text_columns(call: &Call) -> Option<Call> {
    call_with_columns_for(call, &holds_a_replacement_of_a_literal_alone, "g")
}

/// `call` with the start of each chain that `is_it` accepts made the column `column`.
fn call_with_columns_for(
    call: &Call,
    is_it: &dyn Fn(&Expression) -> bool,
    column: &str,
) -> Option<Call> {
    call_with_columns_cut_for(call, is_it, &|_| column.to_owned(), &|_| None, &|_| false)
}

/// As [`call_with_columns_for`], with the column each chain is made `column_of` says of it, cast
/// to the dtype `cast_to` says of it, if it does, and each chain made so is cut after its first
/// call that `cut_after` accepts: what follows is no part of what the column stands in for.
fn call_with_columns_cut_for(
    call: &Call,
    is_it: &dyn Fn(&Expression) -> bool,
    column_of: &dyn Fn(&Expression) -> String,
    cast_to: &dyn Fn(&Expression) -> Option<Value>,
    cut_after: &dyn Fn(&Call) -> bool,
) -> Option<Call> {
    struct Replacing<'a> {
        is_it: &'a dyn Fn(&Expression) -> bool,
        column_of: &'a dyn Fn(&Expression) -> String,
        cast_to: &'a dyn Fn(&Expression) -> Option<Value>,
        cut_after: &'a dyn Fn(&Call) -> bool,
    }
    fn value(held: &Value, replacing: &Replacing) -> Option<Value> {
        Some(match held {
            Value::Expression(expression) => {
                Value::Expression(expression_of_columns(expression, replacing)?)
            }
            Value::Many(items) => Value::Many(
                items
                    .iter()
                    .map(|item| value(item, replacing))
                    .collect::<Option<Vec<_>>>()?,
            ),
            Value::Pairs(pairs) => Value::Pairs(
                pairs
                    .iter()
                    .map(|(key, entry)| Some((value(key, replacing)?, value(entry, replacing)?)))
                    .collect::<Option<Vec<_>>>()?,
            ),
            other => other.clone(),
        })
    }
    fn expression_of_columns(expression: &Expression, replacing: &Replacing) -> Option<Expression> {
        let replaced = (replacing.is_it)(expression);
        let start = if replaced {
            Start::Call(
                Call::new(
                    catalog::entry("polars.col")?,
                    vec![(
                        "name",
                        Value::Columns(vec![(replacing.column_of)(expression)]),
                    )],
                )
                .ok()?,
            )
        } else {
            match expression.start() {
                Start::Call(start) => Start::Call(call_replaced(start, replacing)?),
                other => other.clone(),
            }
        };
        let mut links = Vec::new();
        if replaced && let Some(dtype) = (replacing.cast_to)(expression) {
            links.push(Call::new(catalog::entry("Expr.cast")?, vec![("dtype", dtype)]).ok()?);
        }
        for link in expression.links() {
            links.push(call_replaced(link, replacing)?);
            if replaced && (replacing.cut_after)(link) {
                break;
            }
        }
        Expression::new(start, links).ok()
    }
    fn call_replaced(call: &Call, replacing: &Replacing) -> Option<Call> {
        let given = call
            .arguments()
            .iter()
            .map(|given| Some((given.parameter(), value(given.value(), replacing)?)))
            .collect::<Option<Vec<_>>>()?;
        Call::new(catalog::entry(call.entry().id().as_str())?, given).ok()
    }
    call_replaced(
        call,
        &Replacing {
            is_it,
            column_of,
            cast_to,
            cut_after,
        },
    )
}

/// Whether the chain itself, and not an expression it holds, starts from a literal and replaces.
fn holds_a_replacement_of_a_literal_alone(expression: &Expression) -> bool {
    matches!(expression.start(), Start::Call(start) if is(start, "polars.lit"))
        && expression
            .links()
            .iter()
            .any(|link| is(link, "Expr.str.replace") || is(link, "Expr.str.replace_all"))
}

/// Whether Polars refuses the case for a value of another length still when the literal of each
/// replacement is a column of text: the literal is then no cause. `None` where no chain that
/// starts from a literal holds a replacement.
fn refused_for_a_value_of_another_length_with_columns(case: &Case) -> Option<bool> {
    let step = &case.document.steps()[0];
    let rebuilt = Step::new(step.id().clone(), call_with_text_columns(step.call())?).ok()?;
    let control = Case {
        receiver: case.receiver,
        argument: case.argument,
        document: Document::new(vec![rebuilt]).ok()?,
    };
    changed(Some(control), case).map(|control| {
        (0..RUNS).any(|_| {
            observe(&control)
                .refusal
                .is_some_and(|refusal| refusal.reason.contains(A_VALUE_OF_ANOTHER_LENGTH))
        })
    })
}

/// What Polars says of a replacement of a pattern, or of many values, that is given a count above
/// one.
const A_COUNT_OF_A_REPLACEMENT: &str = "with 'n > 1' not yet supported";

/// The case with each replacement given no count, which is the first match.
fn case_without_counts(case: &Case) -> Option<Case> {
    case_changed(case, &|call| {
        if !is(call, "Expr.str.replace") || call.argument("n").is_none() {
            return None;
        }
        let given: Vec<(&str, Value)> = call
            .arguments()
            .iter()
            .filter(|given| given.parameter() != "n")
            .map(|given| (given.parameter(), given.value().clone()))
            .collect();
        Call::new(catalog::entry("Expr.str.replace")?, given).ok()
    })
}

/// Whether Polars refuses the case for a count above one still when each replacement is given
/// none: the count is then no cause. `None` where no replacement has one to leave out.
fn refused_for_a_count_without_counts(case: &Case) -> Option<bool> {
    changed(case_without_counts(case), case).map(|control| {
        (0..RUNS).any(|_| {
            observe(&control)
                .refusal
                .is_some_and(|refusal| refusal.reason.contains(A_COUNT_OF_A_REPLACEMENT))
        })
    })
}

/// A text literal, which is no null.
fn a_text_literal() -> Option<Expression> {
    let literal = Call::new(
        catalog::entry("polars.lit")?,
        vec![("value", Value::String("x".to_owned()))],
    )
    .ok()?;
    Expression::new(Start::Call(literal), Vec::new()).ok()
}

/// The case with the value of each replacement of every match made a text literal.
fn case_with_text_values(case: &Case) -> Option<Case> {
    case_with_text_values_of(case, &replaces_every_match)
}

/// Whether the call replaces what a pattern matches, the first match or each.
fn is_a_text_replacement(call: &Call) -> bool {
    is(call, "Expr.str.replace") || is(call, "Expr.str.replace_all")
}

/// Whether Polars refuses the case for the length of a value still when the value of each
/// replacement is a text literal: the many values are then no cause. `None` where no replacement
/// has one to change.
fn refused_for_another_length_with_text_values(case: &Case) -> Option<bool> {
    changed(case_with_text_values_of(case, &is_a_text_replacement), case).map(|control| {
        (0..RUNS).any(|_| {
            observe(&control)
                .refusal
                .is_some_and(|refusal| refusal.reason.contains(A_VALUE_OF_ANOTHER_LENGTH))
        })
    })
}

/// Whether Polars refuses the case for the length of a value still when no call is a window: the
/// window is then no cause. `None` where no call is one.
fn refused_for_another_length_without_windows(case: &Case) -> Option<bool> {
    changed(case_without_windows(case), case).map(|control| {
        (0..RUNS).any(|_| {
            observe(&control)
                .refusal
                .is_some_and(|refusal| refusal.reason.contains(A_VALUE_OF_ANOTHER_LENGTH))
        })
    })
}

/// The case with the value of each call `which` accepts made a text literal.
fn case_with_text_values_of(case: &Case, which: &dyn Fn(&Call) -> bool) -> Option<Case> {
    case_changed(case, &|call| {
        if !which(call) {
            return None;
        }
        let given: Vec<(&str, Value)> = call
            .arguments()
            .iter()
            .map(|given| {
                let value = if given.parameter() == "value" {
                    Value::Expression(a_text_literal()?)
                } else {
                    given.value().clone()
                };
                Some((given.parameter(), value))
            })
            .collect::<Option<_>>()?;
        Call::new(catalog::entry(call.entry().id().as_str())?, given).ok()
    })
}

/// Whether Polars refuses the case for a null value still when the value of each replacement of
/// every match is text: the value is then no cause. `None` where no replacement of every match
/// has one to change.
fn refused_for_a_null_value_with_text_values(case: &Case) -> Option<bool> {
    changed(case_with_text_values(case), case).map(|control| {
        (0..RUNS).any(|_| {
            observe(&control)
                .refusal
                .is_some_and(|refusal| refusal.reason.contains(A_NULL_VALUE_OF_A_REPLACEMENT))
        })
    })
}

/// Whether the call drops the nulls or the NaNs of what it follows.
fn is_a_drop(call: &Call) -> bool {
    is(call, "Expr.drop_nulls") || is(call, "Expr.drop_nans")
}

/// The case with each drop made a fill of nulls forward, which gives as many values as it is
/// given, of the dtype it is given.
fn case_without_drops(case: &Case) -> Option<Case> {
    case_changed(case, &|call| {
        is_a_drop(call)
            .then(|| Call::new(catalog::entry("Expr.forward_fill")?, Vec::new()).ok())
            .flatten()
    })
}

/// Whether Polars refuses the case for lengths still when no call drops: the drop is then no cause
/// of the refusal, which the rows' lengths are.
fn refused_for_lengths_without_drops(case: &Case) -> Option<bool> {
    changed(case_without_drops(case), case).map(|control| {
        (0..RUNS).any(|_| {
            observe(&control)
                .refusal
                .is_some_and(|refusal| unplaced(&refusal) && is_lengths(&refusal.reason))
        })
    })
}

/// What a replacement is refused with when what replaces it has fewer values than what it replaces.
const NEW_AS_LONG_AS_OLD: &str = "must have the same length as `old`";

/// What a replacement is refused with when its default has neither as many values as its input nor
/// one.
const DEFAULT_AS_LONG_AS_INPUT: &str = "must have the same length as the input or have length 1";

/// Whether the reason is one of the two a replacement is refused with for a number of values.
fn is_a_replacement_count(reason: &str) -> bool {
    reason.contains(NEW_AS_LONG_AS_OLD) || reason.contains(DEFAULT_AS_LONG_AS_INPUT)
}

/// Whether Polars refuses the case for the reason given still when no call drops: for the words of
/// a replacement that it be given as many values as it replaces, or that its default be as many
/// as its input or one, those words, and not another refusal about lengths, which a fill in
/// place of the drop may meet for its own reasons.
fn refused_for_lengths_without_drops_as(case: &Case, reason: &str) -> Option<bool> {
    changed(case_without_drops(case), case).map(|control| {
        (0..RUNS).any(|_| {
            observe(&control).refusal.is_some_and(|refusal| {
                unplaced(&refusal)
                    && if reason.contains(NEW_AS_LONG_AS_OLD) {
                        refusal.reason.contains(NEW_AS_LONG_AS_OLD)
                    } else if reason.contains(DEFAULT_AS_LONG_AS_INPUT) {
                        refusal.reason.contains(DEFAULT_AS_LONG_AS_INPUT)
                    } else {
                        is_lengths(&refusal.reason)
                    }
            })
        })
    })
}

/// Whether Polars refuses the case for the reason the tables gave once each drop is a fill of
/// nulls forward: the rows are then given, and show the plan that they contradict, so the drop
/// was why they did not. Polars allows the case as it is.
fn refused_with_each_drop_a_fill(case: &Case, reason: &str) -> bool {
    changed(case_without_drops(case), case).is_some_and(|control| {
        (0..RUNS).any(|_| {
            observe(&control)
                .refusal
                .is_some_and(|refusal| refusal.reason == reason)
        })
    })
}

/// Whether Polars refuses the case for the test for NaN still when no call drops: the drop is
/// then no cause of it, and the dtype the test is not supported for is.
fn refused_for_a_test_of_nan_without_drops(case: &Case) -> Option<bool> {
    changed(case_without_drops(case), case).map(|control| {
        (0..RUNS).any(|_| {
            observe(&control).refusal.is_some_and(|refusal| {
                refusal.reason.contains("operation not supported for dtype")
                    && refusal.reason.contains("is_not_nan")
            })
        })
    })
}

/// The case with each window made a fill of nulls forward, which gives as many values as it
/// follows and leaves what it follows as it is.
fn case_without_windows(case: &Case) -> Option<Case> {
    case_changed(case, &|call| {
        is(call, "Expr.over")
            .then(|| Call::new(catalog::entry("Expr.forward_fill")?, Vec::new()).ok())
            .flatten()
    })
}

/// Whether the call is a shift, or the first, the last, the minimum or the maximum, which the table
/// with no rows gives as one null.
fn makes_a_null(call: &Call) -> bool {
    matches!(
        call.entry().id().as_str(),
        "Expr.shift" | "Expr.first" | "Expr.last" | "Expr.min" | "Expr.max"
    )
}

/// Whether an expression holds, at any depth, a fill by a strategy that follows one of those five
/// in its chain, not necessarily next to it.
fn holds_a_fill_after_a_null_is_made(expression: &Expression) -> bool {
    let mut made = false;
    for link in expression.links() {
        if makes_a_null(link) {
            made = true;
        } else if made && is(link, "Expr.fill_null") && link.argument("strategy").is_some() {
            return true;
        }
    }
    let calls = std::iter::once(expression.start())
        .filter_map(|start| match start {
            Start::Call(call) => Some(call),
            Start::Input(_) | Start::Hole => None,
        })
        .chain(expression.links());
    calls.into_iter().any(|call| {
        operands(call)
            .into_iter()
            .any(|(_, inside)| holds_a_fill_after_a_null_is_made(inside))
    })
}

/// The case with each of the five a fill of nulls forward, which leaves the nulls of what it is
/// given and no more.
fn case_without_nulls_made(case: &Case) -> Option<Case> {
    case_changed(case, &|call| {
        makes_a_null(call)
            .then(|| Call::new(catalog::entry("Expr.forward_fill")?, Vec::new()).ok())
            .flatten()
    })
}

/// Whether an expression holds, at any depth, a condition and a fill by a strategy: Polars refuses
/// a strategy for a struct inside a condition where it fills nothing, which it runs elsewhere.
fn holds_a_fill_by_a_strategy_in_a_condition(expression: &Expression) -> bool {
    any_call(expression, &|call| is(call, "polars.when"))
        && any_call(expression, &|call| {
            is(call, "Expr.fill_null") && call.argument("strategy").is_some()
        })
}

/// The case with each fill by a strategy made a fill of nulls forward, which a condition runs.
fn case_without_strategies(case: &Case) -> Option<Case> {
    case_changed(case, &|call| {
        (is(call, "Expr.fill_null") && call.argument("strategy").is_some())
            .then(|| Call::new(catalog::entry("Expr.forward_fill")?, Vec::new()).ok())
            .flatten()
    })
}

/// Whether Polars refuses the case for the strategy of a fill still when each is a fill forward.
fn refused_for_a_strategy_without_strategies(case: &Case) -> Option<bool> {
    changed(case_without_strategies(case), case).map(|control| {
        (0..RUNS).any(|_| {
            observe(&control).refusal.is_some_and(|refusal| {
                refusal
                    .reason
                    .contains("fill null strategy not yet supported")
            })
        })
    })
}

/// Whether Polars refuses the case for the strategy of a fill still when none of the five is one:
/// the null is then no cause of it.
fn refused_for_a_strategy_without_nulls_made(case: &Case) -> Option<bool> {
    changed(case_without_nulls_made(case), case).map(|control| {
        (0..RUNS).any(|_| {
            observe(&control).refusal.is_some_and(|refusal| {
                refusal
                    .reason
                    .contains("fill null strategy not yet supported")
            })
        })
    })
}

/// How often the case without its explodes is run again: which of two failures of
/// a plan Polars names depends on which thread gets there first.
pub const RUNS: usize = 8;

/// Whether the call is a `clip` that has a bound, or a replacement that lists values, which
/// Polars casts to the dtype of what the call follows.
fn has_a_bound(call: &Call) -> bool {
    (is(call, "Expr.clip")
        && ["lower_bound", "upper_bound"]
            .iter()
            .any(|bound| call.argument(bound).is_some()))
        || ["old", "new"]
            .iter()
            .any(|value| is_a_replaced_value(call, value) && call.argument(value).is_some())
}

/// Whether the call is a `clip` that has a bound which holds an aggregate: one value, which is
/// there when the table has no rows, where a column has none.
fn is_a_clip_bounded_by_an_aggregate(call: &Call) -> bool {
    is(call, "Expr.clip")
        && operands(call)
            .into_iter()
            .any(|(_, bound)| any_call(bound, &is_one_value))
}

/// Whether the expression is a column and nothing else, which holds the sample's rows.
fn is_a_bare_column(expression: &Expression) -> bool {
    expression.links().is_empty()
        && matches!(expression.start(), Start::Call(call) if is(call, "polars.col"))
}

/// The parameters of the call that hold an expression that is not a bare column: one that may hold
/// other values than the sample's rows.
fn computed(call: &Call, parameters: &[&str]) -> Vec<String> {
    parameters
        .iter()
        .filter(|parameter| {
            matches!(call.argument(parameter), Some(Value::Expression(expression)) if !is_a_bare_column(expression))
        })
        .map(|parameter| (*parameter).to_owned())
        .collect()
}

/// The parameters of a `clip` that hold a bound, and of a method of text that holds a count of
/// characters, that is not a bare column.
fn computed_values_cast(call: &Call) -> Vec<String> {
    if is(call, "Expr.clip") {
        computed(call, &["lower_bound", "upper_bound"])
    } else {
        let counts = counts_of(call);
        computed(call, &counts)
    }
}

/// Whether the call is a `clip` with a bound, or a method of text with a count of characters, that
/// is not a bare column: one that may hold other values than the sample's rows.
fn has_a_bound_of_its_own(call: &Call) -> bool {
    !computed_values_cast(call).is_empty()
}

/// The case with the computed bounds of every `clip` undone, and the computed counts of characters
/// of every method of text made the whole number 1, which are the bounds and the counts that hold
/// values of their own; a bound or a count that is a bare column stays.
fn case_without_computed_bounds(case: &Case) -> Option<Case> {
    case_changed(case, &|call| {
        let computed = computed_values_cast(call);
        if computed.is_empty() {
            return None;
        }
        let clipped = is(call, "Expr.clip");
        let one = the_number_one()?;
        let given: Vec<(&str, Value)> = call
            .arguments()
            .iter()
            .filter(|given| !(clipped && computed.iter().any(|name| name == given.parameter())))
            .map(|given| {
                if !clipped && computed.iter().any(|name| name == given.parameter()) {
                    (given.parameter(), Value::Expression(one.clone()))
                } else {
                    (given.parameter(), given.value().clone())
                }
            })
            .collect();
        Call::new(catalog::entry(call.entry().id().as_str())?, given).ok()
    })
}

/// Whether the tables still refuse a failed cast at a `clip` once its computed bounds are undone:
/// the refusal is then a bare column's, which holds the sample's rows as the tables measured it,
/// and no bound with values of its own is why. The tables are asked and Polars is not.
fn the_bare_bounds_alone_are_refused(case: &Case) -> bool {
    case_without_computed_bounds(case).is_some_and(|control| match predict(&control).verdict {
        Verdict::Refuses(refused) => {
            refused.stage == Some(TableStage::Rows)
                && is_a_failed_cast(&refused.reason)
                && locate(&control, &refused.place)
                    .call
                    .is_some_and(|call| is(call, "Expr.clip") || is_a_count_of_characters(call))
        }
        _ => false,
    })
}

/// Whether the reason is that a dtype could not be cast to another, in Polars' words for a
/// failed conversion of values or of a list's type.
fn is_a_failed_cast(reason: &str) -> bool {
    is_a_failed_conversion(reason) || reason.starts_with("cannot cast ")
}

/// Whether Polars refused a literal that it converts when it works out the schema, as it does one
/// value that it can fold, not a column that it converts when the rows run: it names the scalar
/// the literal is, and says it of the schema.
fn is_a_literal_that_failed_to_convert(refusal: &Refusal) -> bool {
    refusal.stage == Stage::Schema && refusal.reason.contains("failed in column 'scalar'")
}

/// Whether the reason is that of a value that could not be cast: Polars' words for it, which
/// name the dtypes.
fn is_a_failed_conversion(reason: &str) -> bool {
    reason.starts_with("conversion from `") && reason.contains("` failed")
}

/// Whether Polars refuses the case for a failed conversion still when no `clip` has a bound: the
/// bound is then no cause of it. Which of two failures of a plan Polars names depends on which
/// thread gets there first, so the case is run again.
fn refused_for_a_failed_conversion_without_bounds(case: &Case) -> Option<bool> {
    changed(case_without_bounds(case), case).map(|control| {
        (0..RUNS).any(|_| {
            observe(&control).refusal.is_some_and(|refusal| {
                unplaced(&refusal) && is_a_failed_conversion(&refusal.reason)
            })
        })
    })
}

/// Whether the reason is how Polars refuses a replacement that lists the values of a column where
/// it meets the values of a group: as a pattern for each row, or as a call whose output is the
/// length of the column and not of what it follows.
fn says_a_replacement_of_a_group_is_refused(reason: &str) -> bool {
    reason.contains("replacement pattern per row") || reason.contains("output length of `map`")
}

/// Whether the call is a replacement that lists what it replaces, or what replaces it, as the
/// values of a column.
fn is_a_replacement_of_columns(call: &Call) -> bool {
    matches!(
        call.entry().id().as_str(),
        "Expr.replace" | "Expr.replace_strict"
    ) && ["old", "new"].iter().any(|parameter| {
        matches!(
            call.argument(parameter),
            Some(Value::Expression(listed))
                if any_call(listed, &|inside| is(inside, "polars.col"))
        )
    })
}

/// The case with what each replacement of columns lists made null literals, which every dtype
/// holds.
fn case_without_columns_in_replacements(case: &Case) -> Option<Case> {
    case_changed(case, &|call| {
        if !is_a_replacement_of_columns(call) {
            return None;
        }
        let given: Vec<(&str, Value)> = call
            .arguments()
            .iter()
            .map(|given| {
                if is_a_replaced_value(call, given.parameter()) {
                    let null = a_null_literal().expect("a null literal");
                    (given.parameter(), Value::Expression(null))
                } else {
                    (given.parameter(), given.value().clone())
                }
            })
            .collect();
        Call::new(catalog::entry(call.entry().id().as_str())?, given).ok()
    })
}

/// Whether Polars refuses the case for a pattern per row still when no replacement lists the
/// values of a column: the columns are then no cause of it. Which of two refusals of a plan
/// Polars names depends on which thread gets there first, so the case is run again.
fn refused_for_a_pattern_per_row_without_columns(case: &Case) -> Option<bool> {
    changed(case_without_columns_in_replacements(case), case).map(|control| {
        (0..RUNS).any(|_| {
            observe(&control).refusal.is_some_and(|refusal| {
                unplaced(&refusal) && says_a_replacement_of_a_group_is_refused(&refusal.reason)
            })
        })
    })
}

/// Whether Polars refuses the case for lengths still when no window explodes: the
/// explode is then no cause of the refusal, which the rows' lengths are.
fn refused_for_lengths_without_explodes(case: &Case) -> Option<bool> {
    changed(case_without_explodes(case), case).map(|control| {
        (0..RUNS).any(|_| {
            observe(&control)
                .refusal
                .is_some_and(|refusal| unplaced(&refusal) && is_lengths(&refusal.reason))
        })
    })
}

/// Whether the expression, or one inside it, has a link that Polars folds into a null which
/// another link follows: what follows is run on a null, which the tables do not know. A fold that
/// ends a chain leaves a null literal, which is a literal's.
fn holds_a_fold_before_a_link(expression: &Expression) -> bool {
    let count = expression.links().len();
    (0..count.saturating_sub(1)).any(|at| is_a_fold_at(expression, at))
        || std::iter::once(expression.start())
            .filter_map(|start| match start {
                Start::Call(call) => Some(call),
                Start::Input(_) | Start::Hole => None,
            })
            .chain(expression.links())
            .any(|call| {
                operands(call)
                    .into_iter()
                    .any(|(_, inside)| holds_a_fold_before_a_link(inside))
            })
}

/// Whether the link at `at` of the chain is one that Polars folds into a null: a comparison or an
/// arithmetic given the null literal, or the first link where the chain starts from the null
/// literal and the link is one that carries nulls.
fn is_a_fold_at(expression: &Expression, at: usize) -> bool {
    let Some(link) = expression.links().get(at) else {
        return false;
    };
    compares_with_a_null(link)
        || (at == 0
            && carries_nulls(link.entry().id().as_str())
            && matches!(expression.start(), Start::Call(start)
                if is(start, "polars.lit") && matches!(start.argument("value"), Some(Value::Flag(None)))))
}

/// The call with every link that Polars folds into a null left out, in the call and in the
/// expressions it holds: what followed each is then given what the call followed.
fn call_without_folds(call: &Call) -> Option<Call> {
    fn value(held: &Value) -> Option<Value> {
        Some(match held {
            Value::Expression(inner) => Value::Expression(expression(inner)?),
            Value::Many(items) => Value::Many(items.iter().map(value).collect::<Option<Vec<_>>>()?),
            Value::Pairs(pairs) => Value::Pairs(
                pairs
                    .iter()
                    .map(|(key, entry)| Some((value(key)?, value(entry)?)))
                    .collect::<Option<Vec<_>>>()?,
            ),
            other => other.clone(),
        })
    }
    fn expression(held: &Expression) -> Option<Expression> {
        // A fold whose receiver is the null literal leaves the other operand in its place.
        let (start, rest) = match (held.start(), held.links().first()) {
            (Start::Call(_), Some(first))
                if is_a_fold_at(held, 0) && !compares_with_a_null(first) =>
            {
                let others = operands(first);
                let [(_, other)] = others.as_slice() else {
                    return None;
                };
                let other = expression(other)?;
                let mut links = other.links().to_vec();
                links.extend(held.links()[1..].iter().cloned());
                (other.start().clone(), links)
            }
            (Start::Call(start), _) => (
                Start::Call(call_without_folds(start)?),
                held.links().to_vec(),
            ),
            (other, _) => (other.clone(), held.links().to_vec()),
        };
        let links = rest
            .iter()
            .filter(|link| !compares_with_a_null(link))
            .map(call_without_folds)
            .collect::<Option<Vec<_>>>()?;
        Expression::new(start, links).ok()
    }
    let given = call
        .arguments()
        .iter()
        .map(|given| Some((given.parameter(), value(given.value())?)))
        .collect::<Option<Vec<_>>>()?;
    Call::new(catalog::entry(call.entry().id().as_str())?, given).ok()
}

/// The case with every call that Polars folds into a null left out.
fn case_without_folds(case: &Case) -> Option<Case> {
    let step = &case.document.steps()[0];
    let rebuilt = Step::new(step.id().clone(), call_without_folds(step.call())?).ok()?;
    Some(Case {
        receiver: case.receiver,
        argument: case.argument,
        document: Document::new(vec![rebuilt]).ok()?,
    })
}

/// Whether Polars refuses the case, with every call that it folds into a null left out, for the
/// reason the tables gave: where it has a place, it lies inside the expression that holds the
/// refused part. `None` where nothing is folded, which the case itself is.
fn refused_without_folds(case: &Case, refused: &Refused) -> Option<bool> {
    let locus = locate(case, &refused.place);
    let within = locus.places.first()?;
    changed(case_without_folds(case), case).map(|control| {
        (0..RUNS).any(|_| {
            observe(&control).refusal.is_some_and(|refusal| {
                same_reason(&refused.source, &refused.reason, &refusal.reason)
                    && refusal
                        .place
                        .as_ref()
                        .is_none_or(|place| place.path().starts_with(within.path()))
            })
        })
    })
}

/// The expressions that start from a literal and that the limit is about at the
/// place the tables refused: the chain the call is at, an argument the call is
/// given, and, where the tables refused when they ran rows, a chain that holds the
/// place in an argument of a call that follows the literal (what a call after a
/// literal is given, at any depth, is met by one value, which Polars broadcasts).
/// None in a pivot's aggregate, which reads no column to stand in for a literal.
fn literal_causes<'a>(case: &'a Case, refused: &Refused, pivot: bool) -> Vec<LiteralCause<'a>> {
    if pivot {
        return Vec::new();
    }
    let locus = locate(case, &refused.place);
    let mut found: Vec<LiteralCause<'a>> = Vec::new();
    let mut add = |cause: LiteralCause<'a>| {
        if !found.iter().any(|known| known.place == cause.place) {
            found.push(cause);
        }
    };
    if let (Some(chain), Some(place)) = (locus.chain, locus.places.last())
        && starts_from_a_literal(chain)
    {
        add(LiteralCause {
            place: place.clone(),
            expression: chain,
        });
    }
    if let Some(call) = locus.call {
        operand_causes(call, &refused.place, pivot)
            .into_iter()
            .for_each(&mut add);
    }
    if refused.stage == Some(TableStage::Rows) {
        for ((holder, reach), place) in locus.inside.iter().zip(&locus.reach).zip(&locus.places) {
            if matches!(reach, Reach::Operand(_)) && starts_from_a_literal(holder) {
                add(LiteralCause {
                    place: place.clone(),
                    expression: holder,
                });
            }
        }
    }
    found
}

/// What Polars' refusal of the case with columns for the literals must match for the literals to
/// be why it allows the case as it is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Matching {
    /// The reason the tables gave.
    Reason,
    /// The reason the tables gave, or the call they refused: a literal that makes Polars skip a
    /// call may be one that the call is refused for in the words of the tables, or one that
    /// the arguments are refused for where Polars evaluates them.
    ReasonOrCall,
    /// The call the tables refused, where Polars places its refusal, and any refusal of the rows:
    /// a call that is given two values that the tables refuse is refused by Polars for either,
    /// and which it names is not the tables' to say.
    Call,
}

/// Whether Polars refuses the case for the tables' reason when each of `causes` is
/// a column of its dtype and nothing else is changed. Polars allows the case as it
/// is, so a refusal is of what the literals were; the tables are then right about a
/// column, and the literal is why Polars differs. A case run again that Polars
/// allows still, or refuses for another reason, shows no such thing: the part is
/// skipped or the tables are wrong about the column as well, which no literal
/// explains. `false` where a literal has no class to stand in with.
///
/// `matching` says what Polars' refusal must match.
fn refused_with_columns(
    causes: &[LiteralCause<'_>],
    case: &Case,
    refused: &Refused,
    observed: &Observed,
    matching: Matching,
) -> bool {
    // Every literal a column, and then each alone, for a call given two literals that the tables
    // refuse may be refused by Polars for the one and not the other once both are columns.
    let alone: Vec<&[LiteralCause<'_>]> = if causes.len() > 1 {
        causes.chunks(1).collect()
    } else {
        Vec::new()
    };
    std::iter::once(causes)
        .chain(alone)
        .any(|standing| refused_with_these_columns(standing, case, refused, observed, matching))
}

fn refused_with_these_columns(
    causes: &[LiteralCause<'_>],
    case: &Case,
    refused: &Refused,
    observed: &Observed,
    matching: Matching,
) -> bool {
    let Some(classes) = causes
        .iter()
        .map(|cause| class_of(cause, observed))
        .collect::<Option<Vec<_>>>()
    else {
        return false;
    };
    let standing: Vec<&Expression> = causes.iter().map(|cause| cause.expression).collect();
    let Some((control, other_as_left)) = control_with_columns(case, refused, &standing) else {
        return false;
    };
    let Some(tables) = Tables::standing(other_as_left, &classes) else {
        return false;
    };
    (0..RUNS).any(|_| {
        observe_probe(&control, tables)
            .refusal
            .is_some_and(|refusal| match matching {
                // The two engines word the refusal of a comparison of two series of dtypes it
                // cannot compare differently, and a literal control runs in the other.
                Matching::Reason => {
                    same_reason(&refused.source, &refused.reason, &refusal.reason)
                        || is_the_same_comparison(&refused.reason, &refusal.reason)
                }
                Matching::ReasonOrCall => {
                    same_reason(&refused.source, &refused.reason, &refusal.reason)
                        || is_the_same_comparison(&refused.reason, &refusal.reason)
                        || refusal.place.as_ref().is_none_or(|place| {
                            *place == refused.place
                                || place.path().starts_with(refused.place.path())
                        })
                }
                // A refusal of the rows has no place, and a refusal that has one is at the call, or
                // in what the call is given, which a literal that the call follows makes Polars
                // skip with the call.
                Matching::Call => refusal.place.as_ref().is_none_or(|place| {
                    *place == refused.place || place.path().starts_with(refused.place.path())
                }),
            })
    })
}

/// The case run again with the literals in `standing` as columns, and whether it
/// reads the other table. A join's key must be elementwise, which a column beside
/// an aggregate is not and a literal is, so Polars refusing the key for that is of
/// the position and not of the call the tables refused. Unless the tables refused
/// the position, the key that holds the literals is then the only item of a
/// `select`, which Polars evaluates as the column it is.
fn control_with_columns(
    case: &Case,
    refused: &Refused,
    standing: &[&Expression],
) -> Option<(Case, bool)> {
    let step = &case.document.steps()[0];
    let key_of_a_join = match (
        is(step.call(), "LazyFrame.join") && !refused.reason.contains("elementwise"),
        refused.place.path().first(),
    ) {
        (true, Some(Segment::Parameter(name)))
            if ["on", "left_on", "right_on"].contains(&name.as_str()) =>
        {
            locate(case, &refused.place)
                .top
                .map(|key| (key, name == "right_on"))
        }
        _ => None,
    };
    let (call, other_as_left) = match key_of_a_join {
        Some((key, other_as_left)) => (
            Call::new(
                catalog::entry("LazyFrame.select")?,
                vec![(
                    "exprs",
                    Value::Many(vec![Value::Expression(expression_with_columns(
                        key, standing, 0,
                    )?)]),
                )],
            )
            .ok()?,
            other_as_left,
        ),
        None => (call_with_columns(step.call(), standing, 0)?, false),
    };
    let rebuilt = Step::new(step.id().clone(), call).ok()?;
    Some((
        Case {
            receiver: case.receiver,
            argument: case.argument,
            document: Document::new(vec![rebuilt]).ok()?,
        },
        other_as_left,
    ))
}

/// The class that Polars' name for a dtype, as its objections spell it, stands for.
fn class_of_polars_name(name: &str) -> Option<Representative> {
    Some(match name.trim().to_ascii_lowercase().as_str() {
        "i8" | "int8" => Representative::Int8,
        "i16" | "int16" => Representative::Int16,
        "i32" | "int32" => Representative::Int32,
        "i64" | "int64" => Representative::Int64,
        "u8" | "uint8" => Representative::UInt8,
        "u16" | "uint16" => Representative::UInt16,
        "u32" | "uint32" => Representative::UInt32,
        "u64" | "uint64" => Representative::UInt64,
        "f32" | "float32" => Representative::Float32,
        "f64" | "float64" => Representative::Float64,
        "bool" | "boolean" => Representative::Boolean,
        "str" | "string" => Representative::String,
        "date" => Representative::Date,
        "time" => Representative::Time,
        _ => return None,
    })
}

/// The dtypes that Polars' objection to a bitwise call names, which are the receiver's and the
/// argument's: `^ not allowed on date and i8`, or `| on Boolean and Int64 is not supported`.
fn dtypes_of_a_bitwise_objection(reason: &str) -> Option<(Representative, Representative)> {
    let symbol = reason
        .chars()
        .next()
        .filter(|symbol| "^|&".contains(*symbol))?;
    let rest = reason[symbol.len_utf8()..].trim_start();
    let named = rest
        .strip_prefix("not allowed on ")
        .or_else(|| {
            rest.strip_prefix("on ")
                .and_then(|rest| rest.strip_suffix(" is not supported"))
        })
        .or_else(|| {
            rest.strip_prefix("on ")
                .and_then(|rest| rest.split(" is not supported").next())
        })?;
    let (receiver, argument) = named.split_once(" and ")?;
    Some((
        class_of_polars_name(receiver)?,
        class_of_polars_name(argument)?,
    ))
}

/// `call` with every expression it is given a column that stands in for it, the first for the
/// second dtype of the objection and so on, and how many there are.
fn call_on_stand_ins(call: &Call) -> Option<Call> {
    fn columns_in(value: &Value, next: &mut usize) -> Option<Value> {
        Some(match value {
            Value::Expression(_) => {
                *next += 1;
                let name = Call::new(
                    catalog::entry("polars.col")?,
                    vec![("name", Value::Columns(vec![stand_in_column(*next)]))],
                )
                .ok()?;
                Value::Expression(Expression::new(Start::Call(name), Vec::new()).ok()?)
            }
            Value::Many(items) => Value::Many(
                items
                    .iter()
                    .map(|item| columns_in(item, next))
                    .collect::<Option<Vec<_>>>()?,
            ),
            other => other.clone(),
        })
    }
    let mut next = 0;
    let given = call
        .arguments()
        .iter()
        .map(|given| Some((given.parameter(), columns_in(given.value(), &mut next)?)))
        .collect::<Option<Vec<_>>>()?;
    Call::new(catalog::entry(call.entry().id().as_str())?, given).ok()
}

/// Whether Polars refuses the refused call as a select does, on columns of the two dtypes that
/// the tables' objection names, for the objection the tables gave and at the call: the call is then
/// one that select refuses for those dtypes, so the join key is why Polars allows it. A key that
/// holds another call that select refuses, or a call whose dtypes select accepts, is no cause.
fn refused_on_columns_as_a_select(case: &Case, refused: &Refused) -> bool {
    let Some((receiver, argument)) = dtypes_of_a_bitwise_objection(&refused.reason) else {
        return false;
    };
    let Some(link) = locate(case, &refused.place).call else {
        return false;
    };
    let (Some(on_columns), Some(entry), Some(tables)) = (
        call_on_stand_ins(link),
        catalog::entry("LazyFrame.select"),
        Tables::standing(false, &[receiver, argument, argument]),
    ) else {
        return false;
    };
    let Ok(start) = Call::new(
        catalog::entry("polars.col").expect("an entry"),
        vec![("name", Value::Columns(vec![stand_in_column(0)]))],
    ) else {
        return false;
    };
    let Ok(chain) = Expression::new(Start::Call(start), vec![on_columns]) else {
        return false;
    };
    let step = &case.document.steps()[0];
    let Ok(select) = Call::new(
        entry,
        vec![("exprs", Value::Many(vec![Value::Expression(chain)]))],
    ) else {
        return false;
    };
    let Ok(rebuilt) = Step::new(step.id().clone(), select) else {
        return false;
    };
    let Ok(document) = Document::new(vec![rebuilt]) else {
        return false;
    };
    let control = Case {
        receiver: case.receiver,
        argument: case.argument,
        document,
    };
    let at = Place::of_step(0, Some(step.id().clone()))
        .then(Segment::Parameter("exprs".to_owned()))
        .then(Segment::Item(0))
        .then(Segment::Link(0));
    (0..RUNS).any(|_| {
        observe_probe(&control, tables)
            .refusal
            .is_some_and(|refusal| {
                same_reason(&refused.source, &refused.reason, &refusal.reason)
                    && refusal.place.as_ref().is_none_or(|place| *place == at)
            })
    })
}

/// Whether the limit is why the tables and Polars disagree.
fn applies(
    limit: Limit,
    case: &Case,
    disagreement: &Disagreement,
    prediction: &Prediction,
    observed: &Observed,
) -> bool {
    let step = case.document.steps()[0].call();
    let pivot = is(step, "LazyFrame.pivot");
    let held = operands(step);
    let explodes_in = |expression: &Expression| any_call(expression, &has_explode);
    match (limit, disagreement) {
        (Limit::AnExplodeChangesTheLengths, Disagreement::PolarsRefuses(refusal)) => {
            // The explode is the cause where Polars stops refusing for lengths when
            // no window explodes.
            unplaced(refusal)
                && is_lengths(&refusal.reason)
                && held.iter().any(|(_, held)| explodes_in(held))
                && refused_for_lengths_without_explodes(case) == Some(false)
        }
        (Limit::AnExplodeChangesTheLengths, Disagreement::PolarsAllows(refused)) => {
            // An `explode` changes the rows that what follows it meets, which
            // Polars refuses with a reason about lengths.
            is_lengths(&refused.reason) && {
                let locus = locate(case, &refused.place);
                locus
                    .inside
                    .iter()
                    .zip(&locus.reach)
                    .any(|(holder, reach)| reached_by_an_explode(holder, *reach))
            }
        }
        (Limit::TheLengthsOfTheRows, Disagreement::PolarsRefuses(refusal)) => {
            // The rows' lengths are the cause where no window explodes, or Polars
            // refuses for lengths still when none does, and no drop is the cause either.
            unplaced(refusal)
                && is_lengths(&refusal.reason)
                && (!held.iter().any(|(_, held)| explodes_in(held))
                    || refused_for_lengths_without_explodes(case) == Some(true))
                && (!held.iter().any(|(_, held)| any_call(held, &is_a_drop))
                    || refused_for_lengths_without_drops(case) == Some(true))
        }
        (Limit::ANullStructToFill, Disagreement::PolarsRefuses(refusal)) => {
            // The null is the cause where Polars stops refusing the strategy of a fill once
            // none of the five makes one.
            unplaced(refusal)
                && matches!(refusal.stage, Stage::Rows | Stage::Empty)
                && refusal
                    .reason
                    .contains("fill null strategy not yet supported for dtype: struct[")
                && (held
                    .iter()
                    .any(|(_, held)| holds_a_fill_after_a_null_is_made(held))
                    && refused_for_a_strategy_without_nulls_made(case) == Some(false)
                    || held
                        .iter()
                        .any(|(_, held)| holds_a_fill_by_a_strategy_in_a_condition(held))
                        && refused_for_a_strategy_without_strategies(case) == Some(false))
        }
        (Limit::AReplacementOfAnAggregateInAWindowKey, Disagreement::PolarsRefuses(refusal)) => {
            // The window and the many values are the cause where Polars stops refusing the lengths
            // once each value is text, and once no call is a window.
            unplaced(refusal)
                && matches!(refusal.stage, Stage::Rows | Stage::Empty)
                && refusal.reason.contains(A_VALUE_OF_ANOTHER_LENGTH)
                && held.iter().any(|(_, held)| {
                    any_call(held, &|call| is(call, "Expr.over"))
                        && any_call(held, &is_a_text_replacement)
                })
                && refused_for_another_length_with_text_values(case) == Some(false)
                && refused_for_another_length_without_windows(case) == Some(false)
        }
        (Limit::AMomentBeyondTheRangeOfAUnit, Disagreement::PolarsRefuses(refusal)) => {
            // The unit is the cause where Polars stops refusing the conversion once each cast is to
            // milliseconds.
            unplaced(refusal)
                && matches!(refusal.stage, Stage::Rows | Stage::Empty)
                && is_a_failed_conversion_to_nanoseconds(&refusal.reason)
                && held
                    .iter()
                    .any(|(_, held)| any_call(held, &is_a_cast_to_nanoseconds))
                && refused_for_nanoseconds_with_milliseconds(case) == Some(false)
        }
        (Limit::APartOfADateThatDependsOnTheRows, Disagreement::PolarsRefuses(refusal)) => {
            // The part is the cause where Polars stops refusing the components once each part is
            // the number 1, which every part takes.
            unplaced(refusal)
                && matches!(refusal.stage, Stage::Rows | Stage::Empty)
                && is_a_refusal_of_a_part_of_a_date(&refusal.reason)
                && held
                    .iter()
                    .any(|(_, held)| any_call(held, &is_given_a_part_of_a_date))
                && refused_for_a_part_of_a_date_with_ones(
                    case,
                    if is_a_refusal_of_the_components_of_a_date(&refusal.reason) {
                        is_a_refusal_of_the_components_of_a_date
                    } else {
                        is_a_failed_conversion
                    },
                ) == Some(false)
        }
        (Limit::APartOfADateThatDependsOnTheRows, Disagreement::PolarsAllows(refused)) => {
            // The values of the column the tables measured are the cause where the tables stop
            // refusing the case once each part is the number 1, and where Polars refuses it, with
            // each part made a value that no part takes, for the reason the tables gave, which
            // shows that it evaluates the part and does not skip it: day 0 for the invalid
            // components, and text that is no number for a failed conversion of text.
            refused.stage == Some(TableStage::Rows)
                && locate(case, &refused.place)
                    .call
                    .is_some_and(is_given_a_part_of_a_date)
                && if is_a_refusal_of_the_components_of_a_date(&refused.reason) {
                    the_tables_stop_refusing_the_parts_as_ones(
                        case,
                        is_a_refusal_of_the_components_of_a_date,
                    ) && refused_for_a_part_with(
                        case,
                        Value::Flag(Some(false)),
                        is_a_refusal_of_the_components_of_a_date,
                    ) == Some(true)
                } else if is_a_failed_conversion_of_text(&refused.reason)
                    && locate(case, &refused.place)
                        .call
                        .is_some_and(is_given_only_literals_one_of_text)
                {
                    the_tables_stop_refusing_the_parts_as_ones(case, is_a_failed_conversion_of_text)
                        && refused_for_a_text_part_as_x(case, is_a_failed_conversion_of_text)
                            == Some(true)
                } else {
                    false
                }
        }
        (Limit::AFormatTheDtypeCannotWrite, Disagreement::PolarsRefuses(refusal)) => {
            // The format is the cause where Polars stops refusing the dtype once each format is
            // `iso`, which every dtype takes.
            unplaced(refusal)
                && matches!(refusal.stage, Stage::Rows | Stage::Empty)
                && is_a_format_refusal(&refusal.reason)
                && held
                    .iter()
                    .any(|(_, held)| any_call(held, &is_a_writing_with_a_format))
                && refused_for_a_format_with_iso(case) == Some(false)
        }
        (Limit::ALiteralTextBesideManyReplacementValues, Disagreement::PolarsRefuses(refusal)) => {
            // The literal is the cause where Polars stops refusing the empty table once the
            // text of each replacement is a column.
            unplaced(refusal)
                && matches!(refusal.stage, Stage::Rows | Stage::Empty)
                && refusal.reason.contains(A_VALUE_OF_ANOTHER_LENGTH)
                && held
                    .iter()
                    .any(|(_, held)| holds_a_replacement_of_a_literal(held))
                && refused_for_a_value_of_another_length_with_columns(case) == Some(false)
        }
        (Limit::AReplacementOfMoreThanTheFirstMatch, Disagreement::PolarsRefuses(refusal)) => {
            // The count is the cause where Polars stops refusing it once each replacement is
            // given none, which is the first match.
            unplaced(refusal)
                && matches!(refusal.stage, Stage::Rows | Stage::Empty)
                && refusal.reason.contains(A_COUNT_OF_A_REPLACEMENT)
                && held.iter().any(|(_, held)| {
                    any_call(held, &|call| {
                        is(call, "Expr.str.replace")
                            && matches!(call.argument("n"), Some(Value::Integer(count)) if *count > 1)
                    })
                })
                && refused_for_a_count_without_counts(case) == Some(false)
        }
        (Limit::AReplacementOfEveryMatchGivenANullValue, Disagreement::PolarsRefuses(refusal)) => {
            // The value is the cause where Polars stops refusing it as null once the value of
            // each replacement of every match is text.
            unplaced(refusal)
                && matches!(refusal.stage, Stage::Rows | Stage::Empty)
                && refusal.reason.contains(A_NULL_VALUE_OF_A_REPLACEMENT)
                && held
                    .iter()
                    .any(|(_, held)| any_call(held, &replaces_every_match))
                && refused_for_a_null_value_with_text_values(case) == Some(false)
        }
        (Limit::AStrictReplacementGivenNoDefault, Disagreement::PolarsRefuses(refusal)) => {
            unplaced(refusal)
                && matches!(refusal.stage, Stage::Rows | Stage::Empty)
                // Of an empty table, what it lists to replace is nothing, and Polars then gives
                // back what it follows, whatever dtype it was asked for, so what follows is
                // refused in words of its own.
                && (is_a_strict_replacement_refusal(&refusal.reason)
                    || refusal.stage == Stage::Empty)
                && held.iter().any(|(_, held)| {
                    any_call(held, &|call| {
                        is(call, "Expr.replace_strict") && call.argument("default").is_none()
                    })
                })
                && refused_for_an_incomplete_mapping_with_defaults(case, &refusal.reason) == Some(false)
        }
        (Limit::AReplacementOfColumnsInAWindow, Disagreement::PolarsRefuses(refusal)) => {
            // The columns are the cause where Polars stops refusing for a pattern per row once
            // what each replacement lists is a null literal.
            unplaced(refusal)
                && matches!(refusal.stage, Stage::Rows | Stage::Empty)
                && says_a_replacement_of_a_group_is_refused(&refusal.reason)
                && held.iter().any(|(_, held)| {
                    any_call(held, &|call| is(call, "Expr.over"))
                        && any_call(held, &is_a_replacement_of_columns)
                })
                && refused_for_a_pattern_per_row_without_columns(case) == Some(false)
        }
        (Limit::ADropChangesTheLengths, Disagreement::PolarsRefuses(refusal)) => {
            // The drop is the cause where Polars stops refusing for lengths once no call drops.
            // What a replacement is given to replace by must be as many as what it replaces, which
            // a drop makes fewer where the rows hold a null: that is a refusal about lengths here
            // and is not one for the rows' own, which the drop's control shows.
            unplaced(refusal)
                && (is_lengths(&refusal.reason) || is_a_replacement_count(&refusal.reason))
                && held.iter().any(|(_, held)| any_call(held, &is_a_drop))
                && refused_for_lengths_without_drops_as(case, &refusal.reason) == Some(false)
        }
        (Limit::ALiteral, Disagreement::PolarsAllows(refused)) => {
            // The literal is the cause where Polars refuses the case for the tables'
            // reason once each literal that could be is a column of its dtype.
            let causes = literal_causes(case, refused, pivot);
            // A call that is given values that the tables refuse apart, as a replacement is given
            // what it replaces and what replaces it, and an interval its two bounds, is refused
            // by Polars for any of them once they are columns, and which it names is not the
            // tables' to say. An interval names the comparison of its `closed` too, which the
            // tables measured with the default.
            // What follows the null literal is folded with it, and what the call after it is
            // given, so a column in its place meets the call or what it is given.
            let locus = locate(case, &refused.place);
            let by_values = locus.call.is_some_and(|call| {
                matches!(
                    call.entry().id().as_str(),
                    "Expr.replace" | "Expr.replace_strict" | "Expr.is_between" | "Expr.is_close"
                )
            });
            let after_the_null_literal =
                locus.call.is_some_and(|call| {
                    matches!(
                        call.entry().id().as_str(),
                        "Expr.eq" | "Expr.ne" | "Expr.gt" | "Expr.ge" | "Expr.lt" | "Expr.le"
                    )
                }) && locus.chain.zip(locus.index).is_some_and(|(chain, at)| {
                    at == 0
                        && matches!(chain.start(), Start::Call(start)
                        if is(start, "polars.lit")
                            && matches!(start.argument("value"), Some(Value::Flag(None))))
                });
            let matching = if by_values {
                Matching::Call
            } else if after_the_null_literal {
                Matching::ReasonOrCall
            } else {
                Matching::Reason
            };
            !causes.is_empty() && refused_with_columns(&causes, case, refused, observed, matching)
        }
        (Limit::AGroupAggregate, Disagreement::PolarsRefuses(refusal)) => {
            // A table with no rows has no groups, and the values of an
            // aggregate over none have no dtype.
            let of_nothing = refusal.stage == Stage::Empty
                && (refusal.reason.contains("not supported")
                    || refusal.reason.contains("got `null`")
                    || refusal.reason.contains("got null")
                    || refusal.reason.contains("` to `null` failed"));
            // A select reads the text a share is written as, and the kernel that takes the values
            // of a group refuses it as no number.
            let a_text_share = unplaced(refusal)
                && refusal.stage == Stage::Rows
                && refusal
                    .reason
                    .contains("expected expression of dtype 'numeric' for quantile")
                && held
                    .iter()
                    .any(|(_, held)| any_call(held, &|call| is(call, "Expr.quantile")));
            pivot && (of_nothing || a_text_share)
        }
        (Limit::AGroupAggregate, Disagreement::PolarsAllows(refused)) => {
            let frames = Frames::with_key(case.receiver, case.argument, None);
            // The kernels that take the values of a group accept what the ones of
            // `select` refuse: a dtype of an aggregate, a comparison, a cast, a
            // struct, or a window over a group.
            pivot
                && [
                    "not supported for dtype",
                    "cannot compare",
                    "cannot perform",
                    "could not evaluate",
                    "casting from",
                    // A function that makes a length of time reads the dtype of each part where a
                    // select works out its schema.
                    "expected integer or float dtype",
                    "lengths don't match",
                    "must specify one field",
                    "cannot cast",
                    "with a replacement pattern per row",
                    "failed to determine supertype",
                    "must not contain duplicates",
                    "fill null strategy not yet supported",
                    "fill-null strategy",
                    SCHEMA_SAYS,
                ]
                .iter()
                .any(|words| refused.reason.contains(words))
                && locate(case, &refused.place).call.is_some()
                // A replacement casts the values it lists, which are a pivot's literals, and a
                // cast converts a literal that it follows, and a text that reads as the number
                // converts where the sample's text, which the tables measured, does not.
                || (pivot
                    && is_a_failed_conversion(&refused.reason)
                    && {
                        let locus = locate(case, &refused.place);
                        locus.call.is_some_and(|call| {
                            matches!(
                                call.entry().id().as_str(),
                                "Expr.replace" | "Expr.replace_strict"
                            ) || ((is(call, "Expr.cast")
                                // A skew and a kurtosis convert what they follow to a float, as
                                // a cast does.
                                || is(call, "Expr.skew")
                                || is(call, "Expr.kurtosis"))
                                && locus.chain.zip(locus.index).is_some_and(|(chain, at)| {
                                    starts_from_a_literal(chain)
                                        || chain.links()[..at].iter().any(compares_with_a_null)
                                        || locus.places.last().is_some_and(|held| {
                                            gives_a_null_of_whole_numbers(
                                                prediction, chain, held, at,
                                            )
                                        })
                                        || (starts_from_a_function_of_literals(chain)
                                            && converts_when_selected(&frames, chain, at + 1))
                                        // The text of a count or a position, which reads as it.
                                        || (is_a_failed_conversion_of_text(&refused.reason)
                                            && (makes_text_of_a_count(&chain.links()[..at])
                                                || is_a_date_written_as_digits(chain, at)
                                                || is_text_of_the_numbers_of_a_group(
                                                    &frames, step, chain, at,
                                                )))
                                        // The values of a column of nulls, which stay null.
                                        || leaves_nulls_of_a_column_of_nulls(
                                            &frames, step, chain, at,
                                        )
                                }))
                        })
                    })
        }
        (Limit::ANullComparisonInAKey, Disagreement::PolarsRefuses(refusal)) => {
            is(step, "LazyFrame.join")
                && unplaced(refusal)
                && refusal.reason.contains("elementwise")
                && {
                    // Another key may be why Polars refuses the join: one that
                    // holds a running value, an aggregate or a window beside what
                    // is not one value. A key that is wholly one value is one
                    // Polars allows beside any other key that is elementwise.
                    let keys = keys(case, true);
                    let other_cause = |key: &Expression| {
                        !wholly_single(key, key.links().len(), true)
                            && any_call(key, &|call| {
                                is_one_value(call)
                                    || matches!(
                                        call.entry().id().as_str(),
                                        "Expr.over"
                                            | "Expr.cum_sum"
                                            | "Expr.cum_max"
                                            | "Expr.cum_count"
                                    )
                            })
                    };
                    keys.iter().any(|(_, key)| holds_a_null_comparison(key))
                        && keys
                            .iter()
                            .filter(|(_, key)| !holds_a_null_comparison(key))
                            .all(|(_, key)| !other_cause(key))
                }
        }
        (Limit::AShiftThatLeavesNulls, Disagreement::PolarsAllows(refused)) => {
            let locus = locate(case, &refused.place);
            let frames = Frames::with_key(case.receiver, case.argument, None);
            // What the call follows, or is given, is only nulls: as the rows of the sample, followed
            // through the calls that are known, say, or as Polars gives them where what the call
            // follows or is given is run alone on the left table.
            let on_the_left = !matches!(
                refused.place.path().first(),
                Some(Segment::Parameter(name)) if name == "right_on" || name == "aggregate_function"
            );
            refused.stage == Some(TableStage::Rows)
                && (locus.chain.zip(locus.index).is_some_and(|(chain, at)| {
                    leaves_only_nulls(&frames, chain, at, true)
                        || (on_the_left && gives_only_nulls(&frames, chain, at))
                }) || locus.call.is_some_and(|call| {
                    operands(call).into_iter().any(|(_, operand)| {
                        leaves_only_nulls(&frames, operand, operand.links().len(), false)
                            || (on_the_left
                                && gives_only_nulls(&frames, operand, operand.links().len()))
                    })
                }))
        }
        (Limit::ASingleValueInAKey, Disagreement::PolarsAllows(refused)) => {
            // The key the tables refused is wholly one value.
            refused.reason.contains("elementwise")
                && locate(case, &refused.place).top.is_some_and(|top| {
                    keys(case, true)
                        .iter()
                        .any(|(_, key)| std::ptr::eq(*key, top))
                        && any_call(top, &is_a_single_value)
                        && wholly_single(top, top.links().len(), true)
                })
        }
        (Limit::ANegativeExponent, Disagreement::PolarsRefuses(refusal)) => {
            // The rows hold the number; a table with no rows holds none to convert.
            unplaced(refusal)
                && refusal.stage == Stage::Rows
                && refusal.reason.starts_with("conversion from `")
                && refusal.reason.contains("` to `u32` failed")
                && held.iter().any(|(_, held)| {
                    any_call(held, &is_a_power_that_may_be_given_a_value_below_zero)
                })
        }
        (Limit::ABitwiseCallOnANull, Disagreement::PolarsRefuses(refusal)) => {
            let reason = refusal.reason.as_str();
            let origin = Place::of_step(0, Some(case.document.steps()[0].id().clone()));
            let placed = placed_operands(step, &origin);
            let negated = reason.contains("dtype Null not supported in 'not' operation")
                && placed
                    .iter()
                    .any(|(_, at, held)| negates_a_bitwise_call_on_a_null(prediction, held, at));
            let filtered = reason
                .contains("filter predicate must be of type `Boolean`, got `Null`")
                && is(step, "LazyFrame.filter")
                && placed.iter().any(|(parameter, at, held)| {
                    *parameter == "predicates"
                        && gives_a_predicate_the_dtype_null(prediction, held, at)
                });
            unplaced(refusal) && refusal.stage == Stage::Rows && (negated || filtered)
        }
        (Limit::ABitwiseCallInAJoinKey, Disagreement::PolarsAllows(refused)) => {
            // A bitwise call in one of the join's own keys, refused for the dtypes it names, which
            // select refuses the call on columns of too.
            is(step, "LazyFrame.join")
                && matches!(refused.place.path().first(), Some(Segment::Parameter(name))
                    if ["on", "left_on", "right_on"].contains(&name.as_str()))
                && locate(case, &refused.place)
                    .call
                    .is_some_and(is_a_bitwise_call)
                && refused_on_columns_as_a_select(case, refused)
        }
        (Limit::ADivisionByZero, Disagreement::PolarsRefuses(refusal)) => {
            // The rows hold the divisor, and so does the empty table, whose sum is zero.
            unplaced(refusal)
                && matches!(refusal.stage, Stage::Rows | Stage::Empty)
                && refusal.reason.contains("division by zero Decimal")
                && [case.receiver, case.argument].contains(&Representative::Decimal)
                && held.iter().any(|(_, held)| {
                    any_call(held, &|call| {
                        matches!(
                            call.entry().id().as_str(),
                            "Expr.truediv" | "Expr.floordiv" | "Expr.mod"
                        )
                    })
                })
        }
        (Limit::AValueBeyondADecimal, Disagreement::PolarsRefuses(refusal)) => {
            // The rows hold the values; a table with no rows holds none to fail.
            unplaced(refusal)
                && refusal.stage == Stage::Rows
                && refusal
                    .reason
                    .starts_with("conversion from `u128` to `decimal[")
                && held
                    .iter()
                    .any(|(_, held)| any_call(held, &|call| is(call, "Expr.sub")))
        }
        (Limit::APanic, Disagreement::PolarsRefuses(refusal)) => {
            unplaced(refusal)
                && is_panic(&refusal.reason)
                && !refusal.reason.contains("with overflow")
        }
        (Limit::APanic, Disagreement::PolarsAllows(refused)) => is_panic(&refused.reason),
        (Limit::AJoinKeyDtype, Disagreement::PolarsRefuses(refusal)) => {
            is(step, "LazyFrame.join")
                && unplaced(refusal)
                && refusal.reason.contains("datatypes of join keys")
                && refusal.reason.contains("decimal")
        }
        (Limit::AHiddenSchema, Disagreement::PolarsAllows(refused)) => {
            let locus = locate(case, &refused.place);
            let last = locus.inside.last().map(|chain| chain.links().len());
            refused.reason.starts_with(SCHEMA_SAYS)
                && (locus.inside.len() > 1
                    || locus.index.is_some_and(|at| Some(at + 1) != last)
                    // A sort key is no output of the step, unless a probe of the call on a
                    // column in the key settles it, which is asked of a case the limits do
                    // not excuse.
                    || (matches!(
                        step.entry().id().as_str(),
                        "LazyFrame.sort" | "LazyFrame.join"
                    ) && settle_in_the_position(case, refused, prediction, observed).is_none()))
        }
        (Limit::ABoundTheDtypeCannotHold, Disagreement::PolarsRefuses(refusal)) => {
            // The rows hold the value, and so may a table with no rows, where an aggregate of
            // nothing, as the entropy, is a value of its own for the bound to be cast from. The
            // bound is the cause where Polars stops refusing for a failed conversion when no
            // `clip` has one.
            unplaced(refusal)
                && (refusal.stage == Stage::Rows
                    || (refusal.stage == Stage::Empty
                        && held
                            .iter()
                            .any(|(_, held)| any_call(held, &is_a_clip_bounded_by_an_aggregate))))
                && is_a_failed_conversion(&refusal.reason)
                && held.iter().any(|(_, held)| any_call(held, &has_a_bound))
                && refused_for_a_failed_conversion_without_bounds(case) == Some(false)
        }
        (Limit::ABoundWithValuesOfItsOwn, Disagreement::PolarsAllows(refused)) => {
            // A clip casts nothing but its bounds, and a method of text its counts, so a failed
            // cast of the rows at the call is a bound's or a count's, and it is the one with
            // values of its own where it is no bare column and the tables do not refuse the call
            // without it.
            refused.stage == Some(TableStage::Rows)
                && is_a_failed_cast(&refused.reason)
                && locate(case, &refused.place)
                    .call
                    .is_some_and(has_a_bound_of_its_own)
                // The computed bound is why where the tables allow the clip without it: a bare
                // bound that the tables refuse is refused as Polars refuses it.
                && !the_bare_bounds_alone_are_refused(case)
        }
        (Limit::ADropLeavesNoRows, Disagreement::PolarsAllows(refused)) => {
            // The rows are none where a drop of nulls meets a value that is all nulls, so the
            // target's own comparison of a plan with rows is not shown for a call that follows
            // one in its chain.
            let locus = locate(case, &refused.place);
            refused.stage == Some(TableStage::Rows)
                && refused.reason.starts_with(SCHEMA_SAYS)
                && locus.chain.zip(locus.index).is_some_and(|(chain, at)| {
                    // A drop of nulls or of NaNs before the call in its chain, or in what the
                    // start of the chain is given, which the start is made of.
                    chain.links()[..at].iter().any(is_a_drop)
                        || matches!(chain.start(), Start::Call(start)
                            if operands(start)
                                .into_iter()
                                .any(|(_, given)| any_call(given, &is_a_drop)))
                })
                && refused_with_each_drop_a_fill(case, &refused.reason)
        }
        (Limit::ACastOfAValueTheDtypeCannotHold, Disagreement::PolarsRefuses(refusal)) => {
            // The rows hold the value, and so may a table with no rows, where an aggregate of
            // nothing, as the sum, is a value of its own for the cast to meet. A literal is
            // converted when the schema is worked out, so the refusal is the schema's, naming the
            // scalar. The cast is the cause where Polars stops refusing for a failed conversion
            // once no cast is strict.
            // The schema names the call that casts the literal where it works the literal out, so a
            // refusal of that kind at a strict cast is the cast's as one with no place is.
            (unplaced(refusal)
                && (matches!(refusal.stage, Stage::Rows | Stage::Empty)
                    || is_a_literal_that_failed_to_convert(refusal))
                || (is_a_literal_that_failed_to_convert(refusal)
                    && refusal.place.as_ref().is_some_and(|place| {
                        locate(case, place).call.is_some_and(is_a_strict_cast)
                    })))
                && is_a_failed_conversion(&refusal.reason)
                && {
                    // A difference converts an unsigned number to a signed one, which the other
                    // casts of the limit do not make of the failure.
                    let differences = is_an_unsigned_conversion_to_a_signed_one(&refusal.reason);
                    held.iter().any(|(_, held)| {
                        any_call(held, &|call| {
                            is_a_strict_cast(call) || (differences && is_a_difference(call))
                        })
                    }) && refused_for_a_failed_conversion_without_strict_casts(case, differences)
                        == Some(false)
                }
        }
        (Limit::AWindowOverAWindow, Disagreement::PolarsRefuses(refusal)) => {
            unplaced(refusal)
                && refusal.reason.contains("aggregation context")
                && held.iter().any(|(_, held)| window_over_a_window(held))
        }
        (Limit::AMomentOfOneValueFilledInAnotherZone, Disagreement::RuleRefuses(refused)) => {
            // The rule that moments share a zone refuses the fill of a shift, and what the shift
            // follows is one value, which Polars shifts as it does not a column.
            let locus = locate(case, &refused.place);
            matches!(refused.source, Source::Rule(rule) if rule == "moments-share-a-zone")
                && locus.call.is_some_and(|call| is(call, "Expr.shift"))
                && locus
                    .chain
                    .zip(locus.index)
                    .is_some_and(|(chain, at)| gives_one_value_of_literals(chain, at))
        }
        (Limit::APartPolarsFoldsToANull, Disagreement::PolarsAllows(refused)) => {
            // The refused call is given an expression that holds a call that Polars folds, which
            // a call follows, and Polars refuses the case, with every such call left out, for the
            // tables' reason. A call that follows a fold in its own chain is asked of a value of
            // its dtype instead ([`super::probe::settle_on_a_value`]), and a panic is the panic
            // limit's.
            !is_panic(&refused.reason)
                && locate(case, &refused.place).call.is_some_and(|call| {
                    operands(call)
                        .into_iter()
                        .any(|(_, given)| holds_a_fold_before_a_link(given))
                })
                && refused_without_folds(case, refused) == Some(true)
        }
        (Limit::AGatherBeyondTheRows, Disagreement::PolarsRefuses(refusal)) => {
            // The rows hold the places a gather is given, and a table with no rows has none that
            // is within them. The gather is the cause where Polars stops refusing for a place out
            // of bounds once each gather is told to give a null for one, as the tables measured it.
            unplaced(refusal)
                && matches!(refusal.stage, Stage::Rows | Stage::Empty)
                && refusal.reason.contains(A_PLACE_OUT_OF_BOUNDS)
                && held
                    .iter()
                    .any(|(_, held)| any_call(held, &is_a_gather_that_may_refuse))
                && refused_for_a_place_out_of_bounds_with_gathers_that_give_nulls(case)
                    == Some(false)
        }
        _ => false,
    }
}

/// What Polars refuses a gather with when a place is beyond what it follows.
const A_PLACE_OUT_OF_BOUNDS: &str = "gather indices are out of bounds";

/// Whether the call is a gather that is not told to give a null for a place out of bounds.
fn is_a_gather_that_may_refuse(call: &Call) -> bool {
    is(call, "Expr.gather")
        && !matches!(call.argument("null_on_oob"), Some(Value::Flag(Some(true))))
}

/// The case with each gather told to give a null for a place out of bounds, as the tables
/// measured it.
fn case_with_gathers_that_give_nulls(case: &Case) -> Option<Case> {
    case_changed(case, &|call| {
        if !is_a_gather_that_may_refuse(call) {
            return None;
        }
        let mut given: Vec<(&str, Value)> = call
            .arguments()
            .iter()
            .filter(|given| given.parameter() != "null_on_oob")
            .map(|given| (given.parameter(), given.value().clone()))
            .collect();
        given.push(("null_on_oob", Value::Flag(Some(true))));
        Call::new(catalog::entry("Expr.gather")?, given).ok()
    })
}

/// Whether Polars refuses the case for a place out of bounds still when each gather gives a null
/// for one: the gather is then no cause of the refusal. `None` where no gather can be told to.
fn refused_for_a_place_out_of_bounds_with_gathers_that_give_nulls(case: &Case) -> Option<bool> {
    changed(case_with_gathers_that_give_nulls(case), case).map(|control| {
        (0..RUNS).any(|_| {
            observe(&control)
                .refusal
                .is_some_and(|refusal| refusal.reason.contains(A_PLACE_OUT_OF_BOUNDS))
        })
    })
}

/// The limits that excuse the disagreement of `prediction` and `observed` for
/// `case`, in the order of [`Limit::ALL`]. None when the case does not
/// disagree, or no limit is why it does.
pub fn excuses(case: &Case, prediction: &Prediction, observed: &Observed) -> Vec<Limit> {
    // `DAGEIST_FUZZ_STRICT=1` holds the tables to everything, to see what the
    // limits excuse.
    if strict() {
        return Vec::new();
    }
    let disagreement = match (&prediction.verdict, &observed.refusal) {
        (Verdict::Allows, Some(refusal)) => Disagreement::PolarsRefuses(refusal),
        (Verdict::Refuses(refused), None) if matches!(refused.source, Source::Polars) => {
            Disagreement::PolarsAllows(refused)
        }
        (Verdict::Refuses(refused), None) if matches!(refused.source, Source::Rule(_)) => {
            Disagreement::RuleRefuses(refused)
        }
        _ => return Vec::new(),
    };
    Limit::ALL
        .into_iter()
        .filter(|limit| applies(*limit, case, &disagreement, prediction, observed))
        .collect()
}

#[cfg(test)]
mod tests {
    //! A limit excuses a disagreement only where it is the cause: the same feature
    //! somewhere else in the case excuses nothing. Each limit is tested so, or,
    //! where Polars' refusal has no place (the rows, a panic), by the reason that
    //! gates it.

    use dageist_engine::allowed::Representative;
    use dageist_engine::catalog;
    use dageist_engine::document::{Document, Dtype, Step, StepId};

    use super::super::predict::Traced;
    use super::*;

    // ----- Building cases and disagreements

    fn call(id: &str, given: Vec<(&str, Value)>) -> Call {
        Call::new(catalog::entry(id).expect("an entry"), given).expect("a call")
    }

    fn column(name: &str) -> Call {
        call(
            "polars.col",
            vec![("name", Value::Columns(vec![name.to_owned()]))],
        )
    }

    /// The whole number 1 as a literal of `Int8`.
    fn literal() -> Call {
        let dtype = Dtype::new(Representative::Int8.dtype()).expect("a dtype");
        call(
            "polars.lit",
            vec![("value", Value::Integer(1)), ("dtype", Value::Dtype(dtype))],
        )
    }

    fn null() -> Call {
        call("polars.lit", vec![("value", Value::Flag(None))])
    }

    fn chain(start: Call, links: &[&str]) -> Expression {
        chain_of(start, links.iter().map(|id| call(id, Vec::new())).collect())
    }

    fn chain_of(start: Call, links: Vec<Call>) -> Expression {
        Expression::new(Start::Call(start), links).expect("a chain")
    }

    fn many(items: Vec<Expression>) -> Value {
        Value::Many(items.into_iter().map(Value::Expression).collect())
    }

    fn compared_with(id: &str, other: Expression) -> Call {
        call(id, vec![("other", Value::Expression(other))])
    }

    fn over(key: &str, mapping: Option<&str>) -> Call {
        let mut given = vec![("partition_by", many(vec![chain(column(key), &[])]))];
        if let Some(word) = mapping {
            given.push(("mapping_strategy", Value::Choice(word.to_owned())));
        }
        call("Expr.over", given)
    }

    fn case_of(step: Call) -> Case {
        let step = Step::new(StepId::new("s").expect("an identifier"), step).expect("a step");
        Case {
            receiver: Representative::Int64,
            argument: Representative::Int64,
            document: Document::new(vec![step]).expect("a document"),
        }
    }

    fn select(items: Vec<Expression>) -> Case {
        case_of(call("LazyFrame.select", vec![("exprs", many(items))]))
    }
    fn join(left: Expression, right: Expression) -> Case {
        case_of(call(
            "LazyFrame.join",
            vec![
                ("other", Value::Table("other".to_owned())),
                ("left_on", many(vec![left])),
                ("right_on", many(vec![right])),
            ],
        ))
    }

    fn pivot(aggregate: Expression) -> Case {
        case_of(call(
            "LazyFrame.pivot",
            vec![
                ("on", Value::Columns(vec!["h".to_owned()])),
                ("on_columns", Value::Table("on_h".to_owned())),
                ("values", Value::Columns(vec!["a".to_owned()])),
                ("aggregate_function", Value::Expression(aggregate)),
            ],
        ))
    }

    fn step_place() -> Place {
        Place::of_step(0, Some(StepId::new("s").expect("an identifier")))
    }

    fn at(path: &[Segment]) -> Place {
        path.iter().cloned().fold(step_place(), Place::then)
    }

    fn parameter(name: &str) -> Segment {
        Segment::Parameter(name.to_owned())
    }

    /// The place of link `link` of item `item` of the parameter `name`.
    fn link_of(name: &str, item: usize, link: usize) -> Place {
        at(&[parameter(name), Segment::Item(item), Segment::Link(link)])
    }

    fn prediction(verdict: Verdict, expressions: Vec<Traced>) -> Prediction {
        Prediction {
            verdict,
            expressions,
        }
    }

    /// The tables refuse the case at `place`, at `stage`, and Polars allows it.
    fn tables_refuse(
        case: &Case,
        place: Place,
        stage: Option<TableStage>,
        reason: &str,
    ) -> Vec<Limit> {
        let verdict = Verdict::Refuses(Refused {
            source: Source::Polars,
            at: place.to_string(),
            place,
            stage,
            reason: reason.to_owned(),
            argument_unknown: false,
        });
        excuses(case, &prediction(verdict, Vec::new()), &Observed::default())
    }

    /// A named rule refuses the case at `place`, and Polars allows it.
    fn rule_refuses(case: &Case, place: Place, rule: &'static str, reason: &str) -> Vec<Limit> {
        let verdict = Verdict::Refuses(Refused {
            source: Source::Rule(rule),
            at: place.to_string(),
            place,
            stage: None,
            reason: reason.to_owned(),
            argument_unknown: false,
        });
        excuses(case, &prediction(verdict, Vec::new()), &Observed::default())
    }

    /// The tables allow the case and Polars refuses it, at `place` if it said.
    fn polars_refuses(
        case: &Case,
        place: Option<Place>,
        stage: Stage,
        reason: &str,
        traced: Vec<Traced>,
    ) -> Vec<Limit> {
        let observed = Observed {
            refusal: Some(Refusal {
                stage,
                at: String::new(),
                place,
                reason: reason.to_owned(),
            }),
            expressions: Vec::new(),
            ..Observed::default()
        };
        excuses(case, &prediction(Verdict::Allows, traced), &observed)
    }

    /// The tables allow the case, as they do, so the prediction is theirs and not made up, and
    /// Polars refuses it at `place`, if it said.
    fn polars_refuses_what_the_tables_allow(
        case: &Case,
        place: Option<Place>,
        stage: Stage,
        reason: &str,
    ) -> Vec<Limit> {
        let prediction = predict(case);
        assert_eq!(prediction.verdict, Verdict::Allows, "the tables allow it");
        let observed = Observed {
            refusal: Some(Refusal {
                stage,
                at: String::new(),
                place,
                reason: reason.to_owned(),
            }),
            expressions: Vec::new(),
            ..Observed::default()
        };
        excuses(case, &prediction, &observed)
    }

    const ROWS: Option<TableStage> = Some(TableStage::Rows);

    // ----- Literal, aggregate

    #[test]
    fn a_literal_in_another_expression_excuses_nothing() {
        let refused = chain(column("a"), &["Expr.cum_sum"]);
        let elsewhere = chain(literal(), &["Expr.first"]);
        let case = select(vec![refused, elsewhere]);
        assert_eq!(
            tables_refuse(&case, link_of("exprs", 0, 0), ROWS, "no"),
            vec![]
        );
    }

    #[test]
    fn a_literal_given_to_a_later_call_of_the_chain_excuses_nothing_before_it() {
        let refused = chain_of(
            column("a"),
            vec![
                call("Expr.cum_sum", Vec::new()),
                compared_with("Expr.eq", chain(literal(), &[])),
            ],
        );
        let case = select(vec![refused]);
        assert_eq!(
            tables_refuse(&case, link_of("exprs", 0, 0), ROWS, "no"),
            vec![]
        );
    }

    /// The limits that excuse the case a kept file holds, as the target finds them: with the tables'
    /// verdict and what Polars does, which no limit's own tests make up.
    fn excused_by_kept(name: &str) -> Vec<Limit> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/allowed_fuzz/regressions")
            .join(name);
        let case = Case::decode(&std::fs::read(path).expect("a kept case")).expect("a case");
        excused_by(&case)
    }

    fn excused_by(case: &Case) -> Vec<Limit> {
        std::thread::scope(|scope| {
            std::thread::Builder::new()
                .stack_size(256 << 20)
                .spawn_scoped(scope, || {
                    excuses(case, &super::super::predict::predict(case), &observe(case))
                })
                .expect("a thread")
                .join()
                .expect("the limits are asked")
        })
    }

    #[test]
    fn a_literal_is_excused_where_polars_allows_it_and_refuses_a_column_of_its_dtype() {
        // A literal that is given to a comparison, one that starts a chain, one that
        // starts the other table's key, and a call that follows a literal and is given
        // what the tables refuse when they run rows.
        for name in [
            "uncovered-a-literal--given-to-a-comparison.case",
            "uncovered-a-literal--starting-a-chain.case",
            "uncovered-a-literal--given-to-a-call-that-follows-a-literal.case",
            "uncovered-a-literal.case",
        ] {
            assert_eq!(excused_by_kept(name), vec![Limit::ALiteral], "{name}");
        }
    }

    #[test]
    fn a_literal_is_no_cause_where_polars_allows_the_column_of_its_dtype_too() {
        // Each holds a literal at the place the tables refused, as a start or an
        // argument, and Polars allows the part for another cause: a branch no row
        // takes, a sort key of one value, a key that is wholly one value, a panic.
        // The literal replaced by a column of its dtype is allowed all the same, so
        // it is not why, and a case of the other causes is excused by theirs alone.
        for name in [
            "stricter-a-literal-compared-in-a-then-no-row-takes.case",
            "stricter-a-literal-compared-in-a-then-no-row-takes-in-a-join-key.case",
            "stricter-a-literal-compared-in-a-sort-key-of-one-value.case",
        ] {
            assert_eq!(excused_by_kept(name), vec![], "{name}");
        }
        assert_eq!(
            excused_by_kept("uncovered-a-single-value-in-a-key--a-literal-in-the-key.case"),
            vec![Limit::ASingleValueInAKey]
        );
        assert_eq!(
            excused_by_kept("uncovered-a-panic--a-literal-in-a-sort-key.case"),
            vec![Limit::APanic]
        );
    }

    #[test]
    fn a_literal_nested_in_an_operand_of_the_refused_call_is_excused_where_a_column_is_refused() {
        // `a <= b / lit(null, Int64)`, `h > a * lit(null, Duration)` and
        // `g == null - lit(null, List(Int64))`: Polars folds the arithmetic with the literal into a
        // null and never meets the dtype the tables refuse the comparison for. Every literal the
        // operand holds is a column in the case run again, and the start of the operand is not
        // the only one. A condition of three branches stands for a column as one of one does.
        for name in [
            "uncovered-a-literal--given-to-a-call-of-an-operand.case",
            "uncovered-a-literal--given-to-a-call-of-an-operand-of-a-comparison-with-text.case",
            "uncovered-a-literal--a-literal-given-to-a-call-after-a-literal.case",
            "uncovered-a-literal--a-condition-of-three-branches.case",
        ] {
            assert_eq!(excused_by_kept(name), vec![Limit::ALiteral], "{name}");
        }
    }

    #[test]
    fn a_literal_nested_in_an_operand_is_no_cause_where_polars_allows_the_column_of_its_dtype_too()
    {
        // `sort(by=(a <= b / lit(null, Int64)).len())`: the key is one value, which Polars
        // drops whatever the comparison holds, so the column in the literal's place is allowed
        // as well and the literal is not why. A probe of the comparison settles it.
        assert_eq!(
            excused_by_kept(
                "stricter-a-literal-given-to-a-call-of-an-operand-in-a-sort-key-of-one-value.case"
            ),
            vec![]
        );
    }

    #[test]
    fn what_follows_an_aggregate_compared_with_the_length_of_a_literal_over_a_window_is_no_limit() {
        // `a.min() <= 1.len().over(order_by=[true])` then `.total_days()`: Polars
        // takes the length of a literal over a window for a constant, and does not
        // evaluate what follows the comparison of a value with it, where the tables
        // refuse a `Boolean`. That is a part Polars skips, which a probe settles
        // and no limit excuses, however the chain is made.
        let window = |length: &str, order_by: Expression| {
            chain_of(
                literal(),
                vec![
                    call(length, Vec::new()),
                    call("Expr.over", vec![("order_by", many(vec![order_by]))]),
                ],
            )
        };
        let item = |operand: Expression| {
            select(vec![chain_of(
                column("a"),
                vec![
                    call("Expr.min", Vec::new()),
                    compared_with("Expr.le", operand),
                    call("Expr.dt.total_days", Vec::new()),
                ],
            )])
        };
        let constant = || chain(literal(), &[]);
        for (name, operand) in [
            ("the length of a literal", window("Expr.len", constant())),
            ("a sum", window("Expr.sum", constant())),
            (
                "a window by a column",
                window("Expr.len", chain(column("k"), &[])),
            ),
            ("no window", chain(literal(), &["Expr.len"])),
        ] {
            assert_eq!(
                tables_refuse(&item(operand), link_of("exprs", 0, 2), ROWS, "no"),
                vec![],
                "{name}"
            );
        }
    }

    #[test]
    fn what_a_call_after_a_literal_is_given_is_excused_only_where_rows_refuse_it_and_a_literal_starts_the_chain()
     {
        // `1.cum_sum().le(b.max())`: the tables refuse `max` of a list when they run
        // rows, and Polars meets it with one value.
        let item = chain_of(
            literal(),
            vec![
                call("Expr.cum_sum", Vec::new()),
                compared_with("Expr.le", chain(column("b"), &["Expr.max"])),
            ],
        );
        let case = select(vec![item]);
        let inside = at(&[
            parameter("exprs"),
            Segment::Item(0),
            Segment::Link(1),
            parameter("other"),
            Segment::Link(0),
        ]);
        // What the tables refuse when they built the step is refused.
        assert_eq!(
            tables_refuse(&case, inside, Some(TableStage::Schema), "no"),
            vec![]
        );
        // Nothing starts from a literal.
        let plain = chain_of(
            column("a"),
            vec![
                call("Expr.cum_sum", Vec::new()),
                compared_with("Expr.le", chain(column("b"), &["Expr.max"])),
            ],
        );
        let inside = at(&[
            parameter("exprs"),
            Segment::Item(0),
            Segment::Link(1),
            parameter("other"),
            Segment::Link(0),
        ]);
        assert_eq!(
            tables_refuse(&select(vec![plain]), inside, ROWS, "no"),
            vec![]
        );
    }

    /// What the limits make of the tables refusing `case` at `place`, at the rows, for `reason`,
    /// asked on a stack that Polars' plans fit on: the limit runs Polars again.
    fn refused_by_the_tables_at(case: &Case, place: Place, reason: &str) -> Vec<Limit> {
        std::thread::scope(|scope| {
            std::thread::Builder::new()
                .stack_size(256 << 20)
                .spawn_scoped(scope, || tables_refuse(case, place, ROWS, reason))
                .expect("a thread")
                .join()
                .expect("the limits are asked")
        })
    }

    #[test]
    fn a_pivot_aggregate_is_excused_for_a_cast_of_what_literals_alone_make_where_a_select_converts_it()
     {
        // `from_epoch(null).str.join(ignore_nulls=false).cast(Int32)`: the value is made of a
        // literal alone, a null that the cast meets as a null, where the tables measured the text
        // of the sample. The same value is cast without a failure in a select.
        assert_eq!(
            excused_by_kept("uncovered-a-group-aggregate--a-cast-of-a-function-of-literals.case"),
            vec![Limit::AGroupAggregate]
        );
        let text = "conversion from `str` to `i32` failed in column 'a' for 2 out of 3 values: [\"1d\", \"2d\"]";
        let at_the_cast = || at(&[parameter("aggregate_function"), Segment::Link(0)]);
        let made_of = |part: Expression| {
            pivot(chain_of(
                call(
                    "polars.from_epoch",
                    vec![("column", Value::Expression(part))],
                ),
                vec![cast_to(Representative::Int32, Vec::new())],
            ))
        };
        // A null alone is converted as a null.
        assert_eq!(
            refused_by_the_tables_at(&made_of(chain(null(), &[])), at_the_cast(), text),
            vec![Limit::AGroupAggregate]
        );
        // The values of a group are no literal: the group's own values are what the tables
        // measured.
        let group = pivot(chain_of(
            call(
                "polars.from_epoch",
                vec![(
                    "column",
                    Value::Expression(chain(call("polars.element", Vec::new()), &[])),
                )],
            ),
            vec![cast_to(Representative::Int32, Vec::new())],
        ));
        assert_eq!(
            refused_by_the_tables_at(&group, at_the_cast(), text),
            vec![]
        );
        // A value that a select refuses to cast too is no group's: the control fails closed.
        let date = pivot(chain_of(
            call(
                "polars.date",
                vec![
                    ("year", Value::Expression(whole(1))),
                    ("month", Value::Expression(whole(1))),
                    ("day", Value::Expression(whole(1))),
                ],
            ),
            vec![cast_to(Representative::UInt8, Vec::new())],
        ));
        assert_eq!(
            refused_by_the_tables_at(
                &date,
                at_the_cast(),
                "conversion from `date` to `u8` failed in column 'scalar' for 1 out of 1 values: [0001-01-01]"
            ),
            vec![]
        );
    }

    #[test]
    fn a_pivot_aggregate_is_excused_for_a_skew_of_a_literal_text_and_not_for_the_values_of_a_group()
    {
        // The text literal 1 converts to the float a skew reads, where the tables measured the
        // sample's text, whose values no float reads.
        assert_eq!(
            excused_by_kept("uncovered-a-group-aggregate--a-skew-of-a-literal-text.case"),
            vec![Limit::AGroupAggregate]
        );
        let text = "conversion from `str` to `f64` failed in column 'a' for 2 out of 3 values: [\"1d\", \"2d\"]";
        let at_the_call = || at(&[parameter("aggregate_function"), Segment::Link(0)]);
        let one = || call("polars.lit", vec![("value", Value::String("1".to_owned()))]);
        for entry in ["Expr.skew", "Expr.kurtosis"] {
            let case = pivot(chain_of(one(), vec![call(entry, Vec::new())]));
            assert_eq!(
                refused_by_the_tables_at(&case, at_the_call(), text),
                vec![Limit::AGroupAggregate],
                "{entry}"
            );
            // The values of a group are no literal: they are what the tables measured.
            let group = pivot(chain_of(
                call("polars.element", Vec::new()),
                vec![call(entry, Vec::new())],
            ));
            assert_eq!(
                refused_by_the_tables_at(&group, at_the_call(), text),
                vec![],
                "{entry}"
            );
        }
        // The values of a group that a null text is added to are nulls, which convert: the same
        // reading as a cast's, so the chain the skew follows need not start from a literal.
        let text_dtype = Dtype::new(Representative::String.dtype()).expect("a dtype");
        let null_text = call(
            "polars.lit",
            vec![
                ("value", Value::Flag(None)),
                ("dtype", Value::Dtype(text_dtype)),
            ],
        );
        let summed = pivot(chain_of(
            call("polars.element", Vec::new()),
            vec![
                compared_with("Expr.add", chain(null_text, &[])),
                call("Expr.skew", Vec::new()),
            ],
        ));
        let at_the_skew = at(&[parameter("aggregate_function"), Segment::Link(1)]);
        assert_eq!(
            excused_by_kept("uncovered-a-group-aggregate--a-skew-of-a-sum-with-a-null-text.case"),
            vec![Limit::AGroupAggregate]
        );
        assert_eq!(
            refused_by_the_tables_at(&summed, at_the_skew, text),
            vec![Limit::AGroupAggregate]
        );
        // A call that does not convert what it follows is refused for it by no kernel of a group.
        let running = pivot(chain_of(one(), vec![call("Expr.cum_sum", Vec::new())]));
        assert_eq!(
            refused_by_the_tables_at(&running, at_the_call(), text),
            vec![]
        );
    }

    /// A pivot of the column `a`, of dtype `receiver`, whose aggregate is `aggregate`.
    fn pivot_of(receiver: Representative, aggregate: Expression) -> Case {
        Case {
            receiver,
            ..pivot(aggregate)
        }
    }

    fn text_of(text: &str) -> Call {
        call(
            "polars.lit",
            vec![("value", Value::String(text.to_owned()))],
        )
    }

    #[test]
    fn a_pivot_aggregate_is_excused_for_the_text_of_numbers_a_group_makes_and_for_no_other_text() {
        // A count or a position is a number whatever the group holds, which a fill with text turns
        // to the text of the number, and the text reads as the number it converts to, where the
        // tables measured the sample's text, which no number reads.
        for kept in [
            "uncovered-a-group-aggregate--text-made-of-a-count.case",
            "uncovered-a-group-aggregate--text-made-of-positions.case",
        ] {
            assert_eq!(
                excused_by_kept(kept),
                vec![Limit::AGroupAggregate],
                "{kept}"
            );
        }
        let text = "conversion from `str` to `f64` failed in column 'a' for 2 out of 3 values: [\"1d\", \"2d\"]";
        let filled = |making: &str| {
            vec![
                call(making, Vec::new()),
                call(
                    "Expr.fill_null",
                    vec![(
                        "value",
                        Value::Expression(chain_of(text_of("1d"), Vec::new())),
                    )],
                ),
                call("Expr.skew", Vec::new()),
            ]
        };
        let at_the_skew = || at(&[parameter("aggregate_function"), Segment::Link(2)]);
        let at_the_call = |link: usize| at(&[parameter("aggregate_function"), Segment::Link(link)]);
        for making in [
            "Expr.count",
            "Expr.len",
            "Expr.n_unique",
            "Expr.null_count",
            "Expr.arg_sort",
            "Expr.arg_max",
            "Expr.rle_id",
        ] {
            let case = pivot(chain_of(call("polars.element", Vec::new()), filled(making)));
            assert_eq!(
                refused_by_the_tables_at(&case, at_the_skew(), text),
                vec![Limit::AGroupAggregate],
                "{making}"
            );
        }
        // The text of the values themselves is the sample's, which the tables measured.
        let values = pivot(chain_of(
            call("polars.element", Vec::new()),
            filled("Expr.reverse"),
        ));
        assert_eq!(
            refused_by_the_tables_at(&values, at_the_skew(), text),
            vec![]
        );
        // The text must be made of the count: a count that no fill with text follows, a fill with
        // text that comes before the count, and a fill with a number are the sample's text, or no
        // text, still.
        let skewed = |links: Vec<Call>| {
            let at = links.len();
            let mut all = links;
            all.push(call("Expr.skew", Vec::new()));
            (
                pivot(chain_of(call("polars.element", Vec::new()), all)),
                at_the_call(at),
            )
        };
        let fill_with = |value: Call| {
            call(
                "Expr.fill_null",
                vec![("value", Value::Expression(chain_of(value, Vec::new())))],
            )
        };
        let one = || call("polars.lit", vec![("value", Value::Integer(1))]);
        for (what, links) in [
            ("a count and no fill", vec![call("Expr.count", Vec::new())]),
            (
                "a fill with text before the count",
                vec![fill_with(text_of("1d")), call("Expr.count", Vec::new())],
            ),
            (
                "a fill with a number after the count",
                vec![call("Expr.count", Vec::new()), fill_with(one())],
            ),
            (
                "a count, a calculation, and no fill",
                vec![
                    call("Expr.count", Vec::new()),
                    compared_with("Expr.add", chain_of(one(), Vec::new())),
                ],
            ),
        ] {
            let (case, place) = skewed(links);
            assert_eq!(
                refused_by_the_tables_at(&case, place, text),
                vec![],
                "{what}"
            );
        }
        // A count, a fill with text and then another call that keeps the text, is still the
        // count's.
        let (kept_on, place) = skewed(vec![
            call("Expr.count", Vec::new()),
            fill_with(text_of("1d")),
            call("Expr.reverse", Vec::new()),
        ]);
        assert_eq!(
            refused_by_the_tables_at(&kept_on, place, text),
            vec![Limit::AGroupAggregate]
        );
        // A count after the conversion makes nothing that is converted.
        let after = pivot(chain_of(
            call("polars.element", Vec::new()),
            vec![
                call("Expr.skew", Vec::new()),
                call("Expr.count", Vec::new()),
            ],
        ));
        assert_eq!(
            refused_by_the_tables_at(
                &after,
                at(&[parameter("aggregate_function"), Segment::Link(0)]),
                text
            ),
            vec![]
        );
        // Only a failed conversion of text is this: a count is no cause of another refusal.
        let other = "conversion from `bool` to `f64` failed in column 'a' for 2 out of 3 values: [true, false]";
        let counted = pivot(chain_of(
            call("polars.element", Vec::new()),
            filled("Expr.count"),
        ));
        assert_eq!(
            refused_by_the_tables_at(&counted, at_the_skew(), other),
            vec![]
        );
        // A select has no group whose kernels read the text.
        let selected = select(vec![chain_of(column("a"), filled("Expr.count"))]);
        assert_eq!(
            refused_by_the_tables_at(
                &selected,
                at(&[parameter("exprs"), Segment::Item(0), Segment::Link(2)]),
                text
            ),
            vec![]
        );
    }

    #[test]
    fn a_pivot_aggregate_is_excused_for_a_cast_of_a_bitwise_call_given_a_null_of_whole_numbers() {
        // `element().n_unique().or_(null).cast(Null)`: a bitwise call of whole numbers given the
        // null literal is a null in each row, which a cast to the dtype Null takes, where the
        // tables measured the count's values, which it cannot.
        assert_eq!(
            excused_by_kept(
                "uncovered-a-group-aggregate--a-cast-of-a-bitwise-call-given-a-null.case"
            ),
            vec![Limit::AGroupAggregate]
        );
        let cast = |links: Vec<Call>| {
            let mut all = links;
            all.push(cast_to(Representative::Null, Vec::new()));
            excused_by(&pivot(chain_of(call("polars.element", Vec::new()), all)))
        };
        let or_null = || {
            call(
                "Expr.or_",
                vec![(
                    "others",
                    Value::Many(vec![Value::Expression(chain(null(), &[]))]),
                )],
            )
        };
        // The call that gives no null is no cause of the failed conversion: another call of
        // whole numbers is not asked.
        assert!(!cast(vec![call("Expr.sum", Vec::new())]).contains(&Limit::AGroupAggregate));
        // A bitwise call after the count that is given another value is not a null.
        let given_a_count = call(
            "Expr.or_",
            vec![(
                "others",
                Value::Many(vec![Value::Expression(chain(
                    column("a"),
                    &["Expr.n_unique"],
                ))]),
            )],
        );
        assert!(
            !cast(vec![call("Expr.n_unique", Vec::new()), given_a_count])
                .contains(&Limit::AGroupAggregate)
        );
        assert!(
            cast(vec![call("Expr.n_unique", Vec::new()), or_null()])
                .contains(&Limit::AGroupAggregate)
        );
    }

    #[test]
    fn a_pivot_aggregate_is_excused_for_a_date_written_as_digits_and_for_no_other_format() {
        // `element().dt.to_string("%Y").skew()`: the year of a date written alone reads as a
        // number, where the tables measured the sample's text, which no number reads.
        assert_eq!(
            excused_by_kept("uncovered-a-group-aggregate--a-date-written-as-digits.case"),
            vec![Limit::AGroupAggregate]
        );
        let text = "conversion from `str` to `f64` failed in column 'a' for 2 out of 3 values: [\"1d\", \"2d\"]";
        let written = |entry: &str, format: &str, receiver: Representative| {
            let case = pivot_of(
                receiver,
                chain_of(
                    call("polars.element", Vec::new()),
                    vec![
                        call(entry, vec![("format", Value::String(format.to_owned()))]),
                        call("Expr.skew", Vec::new()),
                    ],
                ),
            );
            refused_by_the_tables_at(
                &case,
                at(&[parameter("aggregate_function"), Segment::Link(1)]),
                text,
            )
        };
        // A running maximum or minimum picks one of the values, so the text still reads as a
        // number.
        assert_eq!(
            excused_by_kept(
                "uncovered-a-group-aggregate--a-date-written-as-digits--kept-by-a-running-maximum.case"
            ),
            vec![Limit::AGroupAggregate]
        );
        for keeping in ["Expr.cum_max", "Expr.cum_min"] {
            let case = pivot_of(
                Representative::Date,
                chain_of(
                    call("polars.element", Vec::new()),
                    vec![
                        call(
                            "Expr.dt.to_string",
                            vec![("format", Value::String("%Y".to_owned()))],
                        ),
                        call(keeping, Vec::new()),
                        call("Expr.skew", Vec::new()),
                    ],
                ),
            );
            assert_eq!(
                refused_by_the_tables_at(
                    &case,
                    at(&[parameter("aggregate_function"), Segment::Link(2)]),
                    text
                ),
                vec![Limit::AGroupAggregate],
                "{keeping}"
            );
        }
        for entry in ["Expr.dt.to_string", "Expr.dt.strftime"] {
            for format in ["%Y", "%Y%m%d", "%H%M%S", "20%y"] {
                assert_eq!(
                    written(entry, format, Representative::Date),
                    vec![Limit::AGroupAggregate],
                    "{entry} {format}"
                );
            }
            // A format that writes a name or a separator does not read as a number.
            for format in ["%Y-%m-%d", "%b", "%Y %m", "%A", ""] {
                assert_eq!(
                    written(entry, format, Representative::Date),
                    vec![],
                    "{entry} {format}"
                );
            }
        }
    }

    #[test]
    fn a_pivot_aggregate_is_excused_for_the_text_of_the_numbers_of_a_group_and_for_no_other() {
        // The values of a group of numbers, cast to text, read as the numbers again, where the
        // tables measured the sample's text, which no number reads.
        assert_eq!(
            excused_by_kept(
                "uncovered-a-group-aggregate--text-made-of-the-numbers-of-a-group.case"
            ),
            vec![Limit::AGroupAggregate]
        );
        let text = "conversion from `str` to `f64` failed in column 'a' for 2 out of 3 values: [\"1d\", \"2d\"]";
        let chain_after = |receiver: Representative, links: Vec<Call>| {
            pivot_of(
                receiver,
                chain_of(call("polars.element", Vec::new()), links),
            )
        };
        let to_text = || cast_to(Representative::String, Vec::new());
        let at_the_call = |link: usize| at(&[parameter("aggregate_function"), Segment::Link(link)]);
        for entry in ["Expr.skew", "Expr.kurtosis"] {
            for receiver in [
                Representative::Int8,
                Representative::UInt64,
                Representative::Float64,
            ] {
                let case = chain_after(receiver, vec![to_text(), call(entry, Vec::new())]);
                assert_eq!(
                    refused_by_the_tables_at(&case, at_the_call(1), text),
                    vec![Limit::AGroupAggregate],
                    "{entry} of {receiver}"
                );
            }
            // The values of a group that are not numbers have the text the tables measured.
            for receiver in [
                Representative::Date,
                Representative::String,
                Representative::Boolean,
            ] {
                let case = chain_after(receiver, vec![to_text(), call(entry, Vec::new())]);
                assert_eq!(
                    refused_by_the_tables_at(&case, at_the_call(1), text),
                    vec![],
                    "{entry} of {receiver}"
                );
            }
        }
        // Calls that keep the text of the numbers, and take none away, may stand between the cast
        // and the conversion: a fill forward, a reverse, one of them picked.
        assert_eq!(
            excused_by_kept(
                "uncovered-a-group-aggregate--text-made-of-the-numbers-of-a-group--kept-by-a-fill.case"
            ),
            vec![Limit::AGroupAggregate]
        );
        for keeping in [
            "Expr.forward_fill",
            "Expr.backward_fill",
            "Expr.reverse",
            "Expr.sort",
            "Expr.drop_nulls",
            "Expr.first",
            "Expr.last",
            "Expr.min",
            "Expr.max",
        ] {
            let case = chain_after(
                Representative::Int8,
                vec![
                    to_text(),
                    call(keeping, Vec::new()),
                    call("Expr.skew", Vec::new()),
                ],
            );
            assert_eq!(
                refused_by_the_tables_at(&case, at_the_call(2), text),
                vec![Limit::AGroupAggregate],
                "{keeping}"
            );
        }
        // A fill with a null of text gives the numbers the dtype text as a cast does.
        assert_eq!(
            excused_by_kept(
                "uncovered-a-group-aggregate--text-made-of-the-numbers-of-a-group--by-a-fill.case"
            ),
            vec![Limit::AGroupAggregate]
        );
        let fill_text = || {
            call(
                "Expr.fill_nan",
                vec![(
                    "value",
                    Value::Expression(chain_of(text_of("1d"), Vec::new())),
                )],
            )
        };
        let filled = chain_after(
            Representative::Int8,
            vec![fill_text(), call("Expr.skew", Vec::new())],
        );
        assert_eq!(
            refused_by_the_tables_at(&filled, at_the_call(1), text),
            vec![Limit::AGroupAggregate]
        );
        let filled_dates = chain_after(
            Representative::Date,
            vec![fill_text(), call("Expr.skew", Vec::new())],
        );
        assert_eq!(
            refused_by_the_tables_at(&filled_dates, at_the_call(1), text),
            vec![]
        );
        // The text is the cast's only where it follows the values and what stands between keeps
        // each of them: a call before the cast, one between that changes the text, or another cast
        // is the sample's.
        let numbers = Representative::Int8;
        let reversed = chain_after(
            numbers,
            vec![
                call("Expr.reverse", Vec::new()),
                to_text(),
                call("Expr.skew", Vec::new()),
            ],
        );
        assert_eq!(
            refused_by_the_tables_at(&reversed, at_the_call(2), text),
            vec![]
        );
        let upper = chain_after(
            numbers,
            vec![
                to_text(),
                call("Expr.str.to_uppercase", Vec::new()),
                call("Expr.skew", Vec::new()),
            ],
        );
        assert_eq!(
            refused_by_the_tables_at(&upper, at_the_call(2), text),
            vec![]
        );
        let to_date = chain_after(
            numbers,
            vec![
                cast_to(Representative::Date, Vec::new()),
                call("Expr.skew", Vec::new()),
            ],
        );
        assert_eq!(
            refused_by_the_tables_at(&to_date, at_the_call(1), text),
            vec![]
        );
        // A select has no group.
        let selected = select(vec![chain_of(
            column("a"),
            vec![to_text(), call("Expr.skew", Vec::new())],
        )]);
        assert_eq!(
            refused_by_the_tables_at(
                &selected,
                at(&[parameter("exprs"), Segment::Item(0), Segment::Link(1)]),
                text
            ),
            vec![]
        );
    }

    #[test]
    fn the_text_of_numbers_in_a_pivot_is_excused_for_a_failed_conversion_of_text_and_no_other_reason()
     {
        // The gates for the text of a count and of the numbers of a group excuse the failed
        // conversion of text, which reads as the numbers. The same chains refused for a conversion
        // of another dtype, or for another objection, are no such text's.
        let fill = || {
            call(
                "Expr.fill_null",
                vec![(
                    "value",
                    Value::Expression(chain_of(text_of("1d"), Vec::new())),
                )],
            )
        };
        let counted = || {
            pivot(chain_of(
                call("polars.element", Vec::new()),
                vec![
                    call("Expr.count", Vec::new()),
                    fill(),
                    call("Expr.skew", Vec::new()),
                ],
            ))
        };
        let cast = || {
            pivot_of(
                Representative::Int8,
                chain_of(
                    call("polars.element", Vec::new()),
                    vec![
                        cast_to(Representative::String, Vec::new()),
                        call("Expr.skew", Vec::new()),
                    ],
                ),
            )
        };
        let reasons = [
            (
                "conversion from `str` to `f64` failed in column 'a' for 2 out of 3 values: [\"1d\", \"2d\"]",
                vec![Limit::AGroupAggregate],
            ),
            (
                "conversion from `str` to `i8` failed in column 'a' for 2 out of 3 values: [\"1d\", \"2d\"]",
                vec![Limit::AGroupAggregate],
            ),
            (
                "conversion from `bool` to `f64` failed in column 'a' for 2 out of 3 values: [true, false]",
                vec![],
            ),
            (
                "conversion from `i64` to `f64` failed in column 'a' for 1 out of 3 values: [1]",
                vec![],
            ),
            (
                "invalid series dtype: expected `Boolean`, got `i8` for series with name `a`",
                vec![],
            ),
            (
                "the length of the values does not match that of the keys",
                vec![],
            ),
        ];
        for (reason, expected) in reasons {
            assert_eq!(
                refused_by_the_tables_at(
                    &counted(),
                    at(&[parameter("aggregate_function"), Segment::Link(2)]),
                    reason
                ),
                expected,
                "a count: {reason}"
            );
            assert_eq!(
                refused_by_the_tables_at(
                    &cast(),
                    at(&[parameter("aggregate_function"), Segment::Link(1)]),
                    reason
                ),
                expected,
                "a cast: {reason}"
            );
        }
    }

    #[test]
    fn a_pivot_aggregate_is_excused_for_a_call_after_a_shift_of_one_value_that_leaves_a_null() {
        // `from_epoch(1).shift().dt.replace(month=true)`: a moment counted from a literal is one
        // value, which a shift by one moves away, so the call after it meets a null. The tables
        // measured the rows of the sample, which hold three.
        assert_eq!(
            excused_by_kept(
                "uncovered-a-shift-that-leaves-nulls--of-a-moment-counted-from-a-literal.case"
            ),
            vec![Limit::AShiftThatLeavesNulls]
        );
        let reason = "Invalid date components (1970, 0, 1) supplied";
        let counted = || {
            call(
                "polars.from_epoch",
                vec![(
                    "column",
                    Value::Expression(chain_of(
                        call("polars.lit", vec![("value", Value::Integer(1))]),
                        Vec::new(),
                    )),
                )],
            )
        };
        let replacing = call(
            "Expr.dt.replace",
            vec![(
                "month",
                Value::Expression(chain_of(
                    call("polars.lit", vec![("value", Value::Integer(0))]),
                    Vec::new(),
                )),
            )],
        );
        let shifted_by = |n: i64| call("Expr.shift", vec![("n", Value::Expression(whole(n)))]);
        // A shift by none leaves the value, and the call after it meets it, which another limit,
        // for the parts of a date, may excuse as it does the number 0.
        for (n, leaves_a_null) in [(1, true), (-1, true), (2, true), (0, false)] {
            let case = pivot(chain_of(counted(), vec![shifted_by(n), replacing.clone()]));
            assert_eq!(
                refused_by_the_tables_at(
                    &case,
                    at(&[parameter("aggregate_function"), Segment::Link(1)]),
                    reason
                )
                .contains(&Limit::AShiftThatLeavesNulls),
                leaves_a_null,
                "a shift by {n}"
            );
        }
        // No shift leaves the moment as it is.
        let unshifted = pivot(chain_of(counted(), vec![replacing]));
        assert!(
            !refused_by_the_tables_at(
                &unshifted,
                at(&[parameter("aggregate_function"), Segment::Link(0)]),
                reason
            )
            .contains(&Limit::AShiftThatLeavesNulls)
        );
    }

    #[test]
    fn a_pivot_aggregate_is_excused_for_a_cast_of_what_a_column_of_nulls_is_compared_with() {
        // The values of a group of a column of the dtype Null are nulls, which a comparison leaves
        // null and a cast converts, where the tables measured the comparison of a column that holds
        // values, whose booleans cannot become nulls.
        assert_eq!(
            excused_by_kept("uncovered-a-group-aggregate--a-cast-of-a-comparison-of-nulls.case"),
            vec![Limit::AGroupAggregate]
        );
        let reason = "conversion from `bool` to `null` failed in column 'a' for 2 out of 3 values: [true, false]";
        let at_the_cast = || at(&[parameter("aggregate_function"), Segment::Link(1)]);
        let cast_after = |link: Call, receiver: Representative| {
            pivot_of(
                receiver,
                chain_of(
                    call("polars.element", Vec::new()),
                    vec![link, cast_to(Representative::Null, Vec::new())],
                ),
            )
        };
        for id in ["Expr.ne", "Expr.eq", "Expr.lt", "Expr.add", "Expr.sub"] {
            let compared = || compared_with(id, chain_of(text_of("1d"), Vec::new()));
            assert_eq!(
                refused_by_the_tables_at(
                    &cast_after(compared(), Representative::Null),
                    at_the_cast(),
                    reason
                ),
                vec![Limit::AGroupAggregate],
                "{id}"
            );
            // A column that holds values has the booleans the tables measured.
            assert_eq!(
                refused_by_the_tables_at(
                    &cast_after(compared(), Representative::Date),
                    at_the_cast(),
                    reason
                ),
                vec![],
                "{id} of dates"
            );
        }
        // What picks or orders the nulls leaves nulls: the first or the last of them, the smallest
        // or the largest, and the nulls in another order.
        assert_eq!(
            excused_by_kept(
                "uncovered-a-group-aggregate--a-cast-of-a-comparison-of-nulls--through-a-last.case"
            ),
            vec![Limit::AGroupAggregate]
        );
        for id in [
            "Expr.first",
            "Expr.last",
            "Expr.min",
            "Expr.max",
            "Expr.reverse",
            "Expr.sort",
        ] {
            let case = pivot_of(
                Representative::Null,
                chain_of(
                    call("polars.element", Vec::new()),
                    vec![
                        compared_with("Expr.ne", chain_of(text_of("1d"), Vec::new())),
                        call(id, Vec::new()),
                        cast_to(Representative::Null, Vec::new()),
                    ],
                ),
            );
            assert_eq!(
                refused_by_the_tables_at(
                    &case,
                    at(&[parameter("aggregate_function"), Segment::Link(2)]),
                    reason
                ),
                vec![Limit::AGroupAggregate],
                "{id}"
            );
        }
        // A cast of the nulls is a column of nulls of the dtype it casts to, which a conversion
        // that follows meets as nulls.
        assert_eq!(
            excused_by_kept("uncovered-a-group-aggregate--text-made-of-a-column-of-nulls.case"),
            vec![Limit::AGroupAggregate]
        );
        let text_of_nulls = pivot_of(
            Representative::Null,
            chain_of(
                call("polars.element", Vec::new()),
                vec![
                    cast_to(Representative::String, Vec::new()),
                    call("Expr.skew", Vec::new()),
                ],
            ),
        );
        assert_eq!(
            refused_by_the_tables_at(
                &text_of_nulls,
                at(&[parameter("aggregate_function"), Segment::Link(1)]),
                "conversion from `str` to `f64` failed in column 'a' for 2 out of 3 values: [\"1d\", \"2d\"]"
            ),
            vec![Limit::AGroupAggregate]
        );
        // A call that counts them leaves a number, which is no null to keep.
        let counted = pivot_of(
            Representative::Null,
            chain_of(
                call("polars.element", Vec::new()),
                vec![
                    compared_with("Expr.ne", chain_of(text_of("1d"), Vec::new())),
                    call("Expr.count", Vec::new()),
                    cast_to(Representative::Null, Vec::new()),
                ],
            ),
        );
        assert_eq!(
            refused_by_the_tables_at(
                &counted,
                at(&[parameter("aggregate_function"), Segment::Link(2)]),
                reason
            ),
            vec![]
        );
        // A call that gives a value for a null, as the test for one, leaves booleans to cast.
        assert_eq!(
            refused_by_the_tables_at(
                &cast_after(call("Expr.is_null", Vec::new()), Representative::Null),
                at_the_cast(),
                reason
            ),
            vec![]
        );
        // The cast is the first call: there is no comparison that leaves the nulls null.
        let bare = pivot_of(
            Representative::Null,
            chain_of(
                call("polars.element", Vec::new()),
                vec![cast_to(Representative::Null, Vec::new())],
            ),
        );
        assert_eq!(
            refused_by_the_tables_at(
                &bare,
                at(&[parameter("aggregate_function"), Segment::Link(0)]),
                reason
            ),
            vec![]
        );
    }

    #[test]
    fn the_functions_that_make_one_value_of_literals_are_the_ones_the_catalogue_holds_and_named() {
        // A function that gives many values of literals, as a range of dates would, must not be
        // excused as one value: a new function of the catalogue fails here until it is decided.
        // The functions that count their values from their arguments make many values, so none of
        // them is a function of literals.
        let mut counted: Vec<&str> = catalog::entries()
            .iter()
            .filter(|entry| entry.applies() == catalog::Applies::Function)
            .filter(|entry| catalog::Counting::of(entry, &[]).is_some())
            .map(|entry| entry.id().as_str())
            .collect();
        counted.sort_unstable();
        assert_eq!(
            counted,
            [
                "polars.date_range",
                "polars.datetime_range",
                "polars.int_range",
                "polars.linear_space",
                "polars.repeat",
                "polars.time_range",
            ]
        );
        let mut functions: Vec<&str> = catalog::entries()
            .iter()
            .filter(|entry| entry.applies() == catalog::Applies::Function)
            .filter(|entry| catalog::Counting::of(entry, &[]).is_none())
            .map(|entry| entry.id().as_str())
            .filter(|id| !matches!(*id, "polars.col" | "polars.element"))
            .collect();
        functions.sort_unstable();
        assert_eq!(
            functions,
            [
                "polars.date",
                "polars.datetime",
                "polars.duration",
                "polars.from_epoch",
                "polars.lit",
                "polars.time",
            ]
        );
        assert_eq!(functions, FUNCTIONS_OF_LITERALS);
        // The value is of literals alone where no column is given to the function.
        let part = |start: Call| Value::Expression(chain_of(start, Vec::new()));
        let one = || call("polars.lit", vec![("value", Value::Integer(1))]);
        let of_literals = call("polars.datetime", vec![("year", part(one()))]);
        let of_a_column = call("polars.datetime", vec![("year", part(column("a")))]);
        assert!(is_a_value_of_literals_alone(&of_literals));
        assert!(!is_a_value_of_literals_alone(&of_a_column));
        // A column and a condition are no function of literals.
        assert!(!is_a_value_of_literals_alone(&column("a")));
    }

    /// `datetime(2000, 1, 1, time_zone=zone)`, a moment of one value.
    fn a_moment_in(zone: &str) -> Call {
        let part = |number: i64| {
            Value::Expression(chain_of(
                call("polars.lit", vec![("value", Value::Integer(number))]),
                Vec::new(),
            ))
        };
        call(
            "polars.datetime",
            vec![
                ("year", part(2000)),
                ("month", part(1)),
                ("day", part(1)),
                ("time_zone", Value::TimeZone(Some(zone.to_owned()))),
            ],
        )
    }

    /// The number 1 as a moment in UTC, a fill for a shift.
    fn a_fill_in_utc() -> Value {
        let dtype: Dtype = "Datetime(time_unit='us', time_zone='UTC')"
            .parse()
            .expect("a dtype");
        Value::Expression(chain_of(
            call(
                "polars.lit",
                vec![("value", Value::Integer(1)), ("dtype", Value::Dtype(dtype))],
            ),
            Vec::new(),
        ))
    }

    fn shifted_with(fill: Value) -> Call {
        call("Expr.shift", vec![("fill_value", fill)])
    }

    fn convert_to(zone: &str) -> Call {
        call(
            "Expr.dt.convert_time_zone",
            vec![("time_zone", Value::TimeZone(Some(zone.to_owned())))],
        )
    }

    fn slugs(limits: Vec<Limit>) -> Vec<&'static str> {
        limits.into_iter().map(Limit::slug).collect()
    }

    #[test]
    fn a_shift_of_one_moment_filled_in_another_zone_is_excused_and_a_shift_of_rows_is_not() {
        // Polars shifts one moment with the number it is filled with, whatever zone the fill is
        // in, where it panics for a column of moments, so the rule that the zones be one is
        // stricter than it for a receiver of one value.
        const SLUG: &str = "a-moment-of-one-value-filled-in-another-zone";
        let reason = "Etc/GMT-1 and UTC";
        for kept in [
            "uncovered-a-moment-of-one-value-filled-in-another-zone.case",
            "uncovered-a-moment-of-one-value-filled-in-another-zone--in-a-select.case",
        ] {
            assert_eq!(slugs(excused_by_kept(kept)), vec![SLUG], "{kept}");
        }
        let refused = |chain: Expression, at: usize| {
            rule_refuses(
                &select(vec![chain]),
                link_of("exprs", 0, at),
                "moments-share-a-zone",
                reason,
            )
        };
        // A moment made of literals, and the one value that an aggregate of a column gives.
        assert_eq!(
            slugs(refused(
                chain_of(a_moment_in("+01:00"), vec![shifted_with(a_fill_in_utc())]),
                0
            )),
            vec![SLUG]
        );
        assert_eq!(
            slugs(refused(
                chain_of(
                    column("b"),
                    vec![
                        convert_to("Europe/Berlin"),
                        call("Expr.max", Vec::new()),
                        shifted_with(a_fill_in_utc()),
                    ]
                ),
                2
            )),
            vec![SLUG]
        );
        // A column of moments is panicked on by Polars, which the rule is right to refuse.
        assert_eq!(
            slugs(refused(
                chain_of(
                    column("b"),
                    vec![convert_to("Europe/Berlin"), shifted_with(a_fill_in_utc())]
                ),
                1
            )),
            Vec::<&str>::new()
        );
        // Many values from a literal are no one value either.
        assert_eq!(
            slugs(refused(
                chain_of(
                    a_moment_in("+01:00"),
                    vec![
                        compared_with("Expr.add", chain(column("b"), &[])),
                        shifted_with(a_fill_in_utc()),
                    ]
                ),
                1
            )),
            Vec::<&str>::new()
        );
        // Another call that meets two zones is refused by Polars, and so is no shift.
        assert_eq!(
            slugs(refused(
                chain_of(
                    a_moment_in("+01:00"),
                    vec![compared_with(
                        "Expr.eq",
                        match a_fill_in_utc() {
                            Value::Expression(fill) => fill,
                            other => panic!("{other:?}"),
                        }
                    )]
                ),
                0
            )),
            Vec::<&str>::new()
        );
        // Another rule that refuses a shift is no zone rule.
        assert_eq!(
            slugs(rule_refuses(
                &select(vec![chain_of(
                    a_moment_in("+01:00"),
                    vec![shifted_with(a_fill_in_utc())]
                )]),
                link_of("exprs", 0, 0),
                "pivot-aggregate-gives-one-value",
                reason
            )),
            Vec::<&str>::new()
        );
    }

    #[test]
    fn a_text_literal_beside_another_literal_is_a_part_polars_converts_and_a_column_beside_it_is_not()
     {
        // `datetime(..).dt.replace(month=lit("1"), minute=lit(true))` in a pivot's aggregate: Polars
        // converts the text of the literal where it makes the plan, and the tables refused the text
        // of the sample's column. With each part made `x` it refuses the text, so it evaluates it.
        assert_eq!(
            excused_by_kept(
                "uncovered-a-part-of-a-date-that-depends-on-the-rows--a-text-literal-beside-another-literal-in-a-pivot.case"
            ),
            vec![Limit::APartOfADateThatDependsOnTheRows]
        );
        let text = "conversion from `str` to `i8` failed in column 'b' for 2 out of 3 values: [\"1d\", \"2d\"]";
        let at_the_replace = || at(&[parameter("aggregate_function"), Segment::Link(0)]);
        let replacing = |minute: Expression| {
            let text_literal = call("polars.lit", vec![("value", Value::String("1".to_owned()))]);
            pivot(chain_of(
                call("polars.element", Vec::new()),
                vec![call(
                    "Expr.dt.replace",
                    vec![
                        (
                            "month",
                            Value::Expression(chain_of(text_literal, Vec::new())),
                        ),
                        ("minute", Value::Expression(minute)),
                    ],
                )],
            ))
        };
        assert_eq!(
            refused_by_the_tables_at(&replacing(whole(1)), at_the_replace(), text),
            vec![Limit::APartOfADateThatDependsOnTheRows]
        );
        // A part that is a column is converted where Polars evaluates it, which the control for
        // the text of a literal does not show.
        assert_eq!(
            refused_by_the_tables_at(&replacing(chain(column("b"), &[])), at_the_replace(), text),
            vec![]
        );
    }

    #[test]
    fn a_pivot_aggregate_is_excused_for_a_length_of_time_of_text_and_a_select_is_not() {
        // The kernels of a group make a length of time of the text of a literal, where a select
        // refuses it for the dtype of the seconds.
        assert_eq!(
            excused_by_kept("uncovered-a-group-aggregate--a-function-given-text.case"),
            vec![Limit::AGroupAggregate]
        );
        let seconds = chain_of(
            call("polars.lit", vec![("value", Value::String("1".to_owned()))]),
            Vec::new(),
        );
        let duration = call(
            "polars.duration",
            vec![("seconds", Value::Expression(seconds))],
        );
        let reason = "expected integer or float dtype, (got str) in input 4 of duration";
        let selected = select(vec![chain_of(duration.clone(), Vec::new())]);
        assert_eq!(
            tables_refuse(
                &selected,
                at(&[parameter("exprs"), Segment::Item(0), Segment::Start]),
                Some(TableStage::Schema),
                reason
            ),
            vec![]
        );
    }

    #[test]
    fn a_pivot_aggregate_is_excused_for_a_fill_by_a_strategy_and_for_a_replacement_of_nothing() {
        let case = pivot(chain(column("a"), &["Expr.forward_fill"]));
        let at_the_call = at(&[parameter("aggregate_function"), Segment::Link(0)]);
        // The kernels of a group fill a list that select does not.
        assert_eq!(
            tables_refuse(
                &case,
                at_the_call,
                Some(TableStage::Rows),
                "fill null strategy not yet supported for dtype: list[i64]"
            ),
            vec![Limit::AGroupAggregate]
        );
        assert_eq!(
            tables_refuse(
                &case,
                at(&[parameter("aggregate_function"), Segment::Link(0)]),
                Some(TableStage::Rows),
                "fill-null strategy Mean is not supported"
            ),
            vec![Limit::AGroupAggregate]
        );
        // The plan of a call that Polars does not compare with its rows in a group's kernels.
        assert_eq!(
            tables_refuse(
                &case,
                at(&[parameter("aggregate_function"), Segment::Link(0)]),
                Some(TableStage::Rows),
                "the schema says Float16 and the rows give Struct(x: Float16, y: Float16)"
            ),
            vec![Limit::AGroupAggregate]
        );
        // A cast of a literal that reads as the number converts, where the sample's text, which
        // the tables measured, does not.
        let literal = pivot(chain_of(
            call("polars.lit", vec![("value", Value::Integer(1))]),
            vec![cast_to(Representative::UInt16, Vec::new())],
        ));
        assert_eq!(
            tables_refuse(
                &literal,
                at(&[parameter("aggregate_function"), Segment::Link(0)]),
                Some(TableStage::Rows),
                "conversion from `str` to `u16` failed in column 'a' for 2 out of 3 values: [\"1d\", \"2d\"]"
            ),
            vec![Limit::AGroupAggregate]
        );
        // A cast of a comparison, or an arithmetic, with a null converts the nulls it is.
        let compared = pivot(chain_of(
            call("polars.element", Vec::new()),
            vec![
                compared_with("Expr.eq", chain(null(), &[])),
                cast_to(Representative::Null, Vec::new()),
            ],
        ));
        assert_eq!(
            tables_refuse(
                &compared,
                at(&[parameter("aggregate_function"), Segment::Link(1)]),
                Some(TableStage::Rows),
                "conversion from `bool` to `null` failed in column 'a' for 2 out of 3 values: [true, false]"
            ),
            vec![Limit::AGroupAggregate]
        );
        let divided = pivot(chain_of(
            call("polars.element", Vec::new()),
            vec![
                compared_with("Expr.truediv", chain(null(), &[])),
                cast_to(Representative::Null, Vec::new()),
            ],
        ));
        assert_eq!(
            tables_refuse(
                &divided,
                at(&[parameter("aggregate_function"), Segment::Link(1)]),
                Some(TableStage::Rows),
                "conversion from `f64` to `null` failed in column 'a' for 2 out of 3 values: [1.0, 3.0]"
            ),
            vec![Limit::AGroupAggregate]
        );
        // A cast of the values of the group is refused as the tables refuse it.
        let group = pivot(chain_of(
            call("polars.element", Vec::new()),
            vec![cast_to(Representative::UInt16, Vec::new())],
        ));
        assert_eq!(
            tables_refuse(
                &group,
                at(&[parameter("aggregate_function"), Segment::Link(0)]),
                Some(TableStage::Rows),
                "conversion from `str` to `u16` failed in column 'a' for 2 out of 3 values: [\"1d\", \"2d\"]"
            ),
            vec![]
        );
        // A table with no rows has no groups, so what an aggregate of none replaces is a null
        // that no number converts to.
        let reason =
            "conversion from `i32` to `null` failed in column 'literal' for 1 out of 1 values: [1]";
        assert_eq!(
            polars_refuses(&case, None, Stage::Empty, reason, Vec::new()),
            vec![Limit::AGroupAggregate]
        );
        let select = select(vec![chain(column("a"), &["Expr.forward_fill"])]);
        assert_eq!(
            polars_refuses(&select, None, Stage::Empty, reason, Vec::new()),
            vec![]
        );
    }

    #[test]
    fn the_empty_table_of_a_pivot_is_excused_for_a_null_dtype_however_it_is_quoted() {
        let case = pivot(chain(column("a"), &["Expr.cum_sum"]));
        for reason in [
            "invalid series dtype: expected `Duration`, got `null` for series with name `a`",
            "expected Date or Datetime, got null",
        ] {
            assert_eq!(
                polars_refuses(&case, None, Stage::Empty, reason, Vec::new()),
                vec![Limit::AGroupAggregate],
                "{reason}"
            );
        }
    }

    // ----- Rules that refuse what Polars allows
    #[test]
    fn a_rule_refusing_a_whole_key_is_excused_by_no_limit_that_does_not_name_it() {
        // `true.first() <= (null != true) > b` and the same from a column: the
        // rule that a join's key is elementwise is refused for what the key holds,
        // which no literal start, null comparison or aggregate in it explains.
        let key = |start: Call| {
            chain_of(
                start,
                vec![
                    call("Expr.first", Vec::new()),
                    compared_with(
                        "Expr.le",
                        chain_of(
                            null(),
                            vec![compared_with("Expr.ne", chain(literal(), &[]))],
                        ),
                    ),
                    compared_with("Expr.gt", chain(column("b"), &[])),
                ],
            )
        };
        let rule = "key-is-elementwise";
        for start in [literal(), column("f")] {
            let case = join(key(start), chain(column("rf"), &[]));
            let place = at(&[parameter("left_on"), Segment::Item(0)]);
            assert_eq!(rule_refuses(&case, place, rule, "a rule"), vec![]);
        }
    }
    /// A join key that is a null literal of `Null` with a running count: Polars allows the key because it is
    /// wholly one value, and a column of `Null` as the key is not elementwise.
    const A_KEY_OF_ONE_LITERAL: &str = r#"{"a":"Int16","b":"Datetime(us)","revision":1}
{"version":1,"steps":[{"id":"s","call":{"entry":"LazyFrame.join","arguments":{"other":{"table":"other"},"on":{"many":[{"expression":{"start":{"entry":"polars.lit","arguments":{"value":{"flag":null},"dtype":{"dtype":"Null"}}},"then":[{"entry":"Expr.cum_count","arguments":{"reverse":{"flag":true}}}]}}]},"nulls_equal":{"flag":false},"build_side":{"choice":"prefer_left"}}}}]}"#;

    #[test]
    fn a_case_that_two_limits_explain_is_excused_by_both() {
        // The literal is why Polars allows the key: a column of its dtype is refused as
        // the tables say. So is the key being wholly one value, which has a limit of
        // its own, and nothing is gated on the other not applying.
        let case = Case::decode(A_KEY_OF_ONE_LITERAL.as_bytes()).expect("a case");
        assert_eq!(
            excused_by(&case),
            vec![Limit::ALiteral, Limit::ASingleValueInAKey]
        );
    }

    /// The window that explodes, alone: its groups are fewer than the rows.
    const AN_EXPLODE_ALONE: &str = r#"{"a":"Date","b":"Date","revision":1}
{"version":1,"steps":[{"id":"s","call":{"entry":"LazyFrame.filter","arguments":{"predicates":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["f"]}}},"then":[{"entry":"Expr.first","arguments":{}},{"entry":"Expr.over","arguments":{"partition_by":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["k"]}}},"then":[]}}]},"mapping_strategy":{"choice":"explode"}}}]}}]}}}}]}"#;

    #[test]
    fn a_lengths_refusal_that_remains_without_the_explode_is_the_rows_and_not_the_explode() {
        // Polars refuses `lit(1).over(order_by=null) < 1` as a filter for lengths with
        // no explode in sight; the explode in the second predicate is no cause of it,
        // and the same filter with that window not exploding is refused the same
        // way. Where the refusal goes when the explode does, it is the explode's.
        let case = |text: &str| Case::decode(text.as_bytes()).expect("a case");
        let refused = |case: &Case| {
            std::thread::scope(|scope| {
                std::thread::Builder::new()
                    .stack_size(256 << 20)
                    .spawn_scoped(scope, || {
                        polars_refuses(case, None, Stage::Rows, LENGTHS, Vec::new())
                    })
                    .expect("a thread")
                    .join()
                    .expect("the limits are asked")
            })
        };
        assert_eq!(
            refused(&case(AN_EXPLODE_ALONE)),
            vec![Limit::AnExplodeChangesTheLengths]
        );
    }

    // ----- Options, lengths, overflow, panic: the reason gates what has no place

    fn explode() -> Call {
        over("k", Some("explode"))
    }

    /// The place of link `inner` of the argument `name` of link `link` of item 0
    /// of `exprs`.
    fn in_operand(link: usize, name: &str, inner: usize) -> Place {
        at(&[
            parameter("exprs"),
            Segment::Item(0),
            Segment::Link(link),
            parameter(name),
            Segment::Link(inner),
        ])
    }

    const LENGTHS: &str = "lengths don't match: zip node received non-equal length inputs";

    #[test]
    fn an_explode_excuses_what_the_window_and_what_follows_it_meet_and_not_what_is_before() {
        // Window at link 1: the call after it, and the arguments of the links
        // after it, meet its rows.
        let case = select(vec![chain_of(
            column("a"),
            vec![
                call("Expr.cum_sum", Vec::new()),
                explode(),
                compared_with("Expr.eq", chain(column("b"), &["Expr.cum_sum"])),
            ],
        )]);
        assert_eq!(
            tables_refuse(&case, link_of("exprs", 0, 2), ROWS, LENGTHS),
            vec![Limit::AnExplodeChangesTheLengths]
        );
        assert_eq!(
            tables_refuse(&case, in_operand(2, "other", 0), ROWS, LENGTHS),
            vec![Limit::AnExplodeChangesTheLengths]
        );
        // Before the window, or in its own arguments, they are not.
        assert_eq!(
            tables_refuse(&case, link_of("exprs", 0, 0), ROWS, LENGTHS),
            vec![]
        );
        assert_eq!(
            tables_refuse(&case, in_operand(1, "partition_by", 0), ROWS, LENGTHS),
            vec![]
        );
        // Another reason is not about the rows an explode changes.
        assert_eq!(
            tables_refuse(&case, link_of("exprs", 0, 2), ROWS, "no"),
            vec![]
        );
    }

    #[test]
    fn an_explode_in_an_unrelated_later_operand_or_item_excuses_nothing() {
        let later = select(vec![chain_of(
            column("a"),
            vec![
                call("Expr.cum_sum", Vec::new()),
                compared_with("Expr.eq", chain_of(column("b"), vec![explode()])),
            ],
        )]);
        assert_eq!(
            tables_refuse(&later, link_of("exprs", 0, 0), ROWS, LENGTHS),
            vec![]
        );
        let elsewhere = select(vec![
            chain(column("a"), &["Expr.cum_sum"]),
            chain_of(column("b"), vec![explode()]),
        ]);
        assert_eq!(
            tables_refuse(&elsewhere, link_of("exprs", 0, 0), ROWS, LENGTHS),
            vec![]
        );
    }

    #[test]
    fn a_refusal_about_lengths_is_the_explodes_or_the_rows_by_whether_there_is_an_explode() {
        let reason = "lengths don't match: zip node received non-equal length inputs";
        let with = select(vec![chain_of(column("a"), vec![explode()])]);
        assert_eq!(
            polars_refuses(&with, None, Stage::Rows, reason, Vec::new()),
            vec![Limit::AnExplodeChangesTheLengths]
        );
        let without = select(vec![chain(column("a"), &["Expr.cum_sum"])]);
        assert_eq!(
            polars_refuses(&without, None, Stage::Rows, reason, Vec::new()),
            vec![Limit::TheLengthsOfTheRows]
        );
        // Another reason, or a refusal placed at a link, is neither.
        assert_eq!(
            polars_refuses(&without, None, Stage::Rows, "no", Vec::new()),
            vec![]
        );
        assert_eq!(
            polars_refuses(
                &without,
                Some(link_of("exprs", 0, 0)),
                Stage::Schema,
                reason,
                Vec::new()
            ),
            vec![]
        );
    }

    // ----- A negative exponent

    const NEGATIVE_EXPONENT: &str =
        "conversion from `i32` to `u32` failed in column 'literal' for 1 out of 1 values: [-1]";

    /// A literal that is the whole number `number`, typed without a dtype.
    fn whole(number: i64) -> Expression {
        chain(
            call("polars.lit", vec![("value", Value::Integer(number))]),
            &[],
        )
    }

    fn pow_of(exponent: Expression) -> Call {
        call("Expr.pow", vec![("exponent", Value::Expression(exponent))])
    }

    #[test]
    fn a_failed_conversion_to_a_u32_is_excused_where_a_power_is_given_a_negative_number() {
        let case = select(vec![chain_of(column("a"), vec![pow_of(whole(-1))])]);
        assert_eq!(
            polars_refuses(&case, None, Stage::Rows, NEGATIVE_EXPONENT, Vec::new()),
            vec![Limit::ANegativeExponent]
        );
        // A power in an expression the chain is given reaches it too.
        let nested = select(vec![chain_of(
            column("a"),
            vec![compared_with(
                "Expr.eq",
                chain_of(column("k"), vec![pow_of(whole(-1))]),
            )],
        )]);
        assert_eq!(
            polars_refuses(&nested, None, Stage::Rows, NEGATIVE_EXPONENT, Vec::new()),
            vec![Limit::ANegativeExponent]
        );
    }

    #[test]
    fn a_failed_conversion_to_a_u32_is_excused_where_a_power_is_given_a_value_below_zero() {
        // A subtraction, a negation, the bits of a number flipped, a difference of the numbers and
        // a number typed below zero take the values of a column below zero.
        assert_eq!(
            excused_by_kept("uncovered-a-negative-exponent--a-difference.case"),
            vec![Limit::ANegativeExponent]
        );
        for exponent in [
            chain_of(
                column("b"),
                vec![compared_with("Expr.sub", chain(column("k"), &[]))],
            ),
            chain(column("k"), &["Expr.neg"]),
            chain(column("k"), &["Expr.not_"]),
            chain(column("k"), &["Expr.diff"]),
            chain_of(column("k"), vec![compared_with("Expr.add", whole(-3))]),
            chain_of(column("k"), vec![compared_with("Expr.mul", whole(-1))]),
        ] {
            let case = select(vec![chain_of(column("a"), vec![pow_of(exponent)])]);
            assert_eq!(
                polars_refuses(&case, None, Stage::Rows, NEGATIVE_EXPONENT, Vec::new()),
                vec![Limit::ANegativeExponent]
            );
        }
    }

    #[test]
    fn a_failed_conversion_to_a_u32_is_not_excused_by_a_sum_or_a_product_of_columns() {
        // The sample's rows are 1, a null and 3, so a sum or a product of columns and of numbers
        // above zero never goes below it: something else made the number fail to convert.
        for exponent in [
            chain_of(
                column("k"),
                vec![compared_with("Expr.add", chain(column("a"), &[]))],
            ),
            chain_of(
                column("k"),
                vec![compared_with("Expr.mul", chain(column("a"), &[]))],
            ),
            chain_of(column("k"), vec![compared_with("Expr.add", whole(3))]),
        ] {
            let case = select(vec![chain_of(column("a"), vec![pow_of(exponent)])]);
            assert_eq!(
                polars_refuses(&case, None, Stage::Rows, NEGATIVE_EXPONENT, Vec::new()),
                vec![]
            );
        }
    }

    #[test]
    fn a_failed_conversion_to_a_u32_is_not_excused_without_a_power_of_a_negative_number() {
        // A power of a positive number or of a column, and a negative number given to another
        // call, are not why a number fails to become a `u32`.
        for case in [
            select(vec![chain_of(column("a"), vec![pow_of(whole(2))])]),
            select(vec![chain_of(
                column("a"),
                vec![pow_of(chain(column("b"), &[]))],
            )]),
            select(vec![chain_of(
                column("a"),
                vec![compared_with("Expr.sub", whole(-1))],
            )]),
        ] {
            assert_eq!(
                polars_refuses(&case, None, Stage::Rows, NEGATIVE_EXPONENT, Vec::new()),
                vec![]
            );
        }
    }

    #[test]
    fn only_a_failed_conversion_to_a_u32_of_the_rows_without_a_place_is_excused_for_a_power() {
        let case = select(vec![chain_of(column("a"), vec![pow_of(whole(-1))])]);
        for reason in [
            "conversion from `i32` to `u8` failed in column 'literal' for 1 out of 1 values: [-1]",
            "`pow` operation not supported for dtype `str` as base",
        ] {
            assert_eq!(
                polars_refuses(&case, None, Stage::Rows, reason, Vec::new()),
                vec![],
                "{reason}"
            );
        }
        // A refusal that Polars places at a call is the call's.
        assert_eq!(
            polars_refuses(
                &case,
                Some(link_of("exprs", 0, 0)),
                Stage::Rows,
                NEGATIVE_EXPONENT,
                Vec::new()
            ),
            vec![]
        );
        assert_eq!(
            polars_refuses(&case, None, Stage::Schema, NEGATIVE_EXPONENT, Vec::new()),
            vec![]
        );
    }

    // ----- A bound the dtype cannot hold

    const BOUND_NOT_HELD: &str =
        "conversion from `f64` to `i8` failed in column 'b' for 1 out of 1 values: [NaN]";

    fn clip_with(lower: Option<Expression>, upper: Option<Expression>) -> Call {
        let mut given = Vec::new();
        if let Some(lower) = lower {
            given.push(("lower_bound", Value::Expression(lower)));
        }
        if let Some(upper) = upper {
            given.push(("upper_bound", Value::Expression(upper)));
        }
        call("Expr.clip", given)
    }

    /// What the limits make of a refusal that Polars gives `case`, asked on a stack that Polars'
    /// plans fit on: the limit runs Polars again.
    fn refused_for_a_bound(
        case: &Case,
        place: Option<Place>,
        stage: Stage,
        reason: &str,
    ) -> Vec<Limit> {
        std::thread::scope(|scope| {
            std::thread::Builder::new()
                .stack_size(256 << 20)
                .spawn_scoped(scope, || {
                    polars_refuses(case, place, stage, reason, Vec::new())
                })
                .expect("a thread")
                .join()
                .expect("the limits are asked")
        })
    }

    #[test]
    fn a_failed_conversion_is_excused_where_a_bound_of_a_clip_is_why() {
        // A bound given to a clip, whatever it holds, a start or a link, is the cause where
        // Polars stops refusing for a failed conversion once the clip has none.
        for clip in [
            clip_with(Some(chain(column("b"), &["Expr.sqrt"])), None),
            clip_with(None, Some(whole(300))),
            clip_with(Some(whole(-1)), Some(chain(column("k"), &["Expr.log1p"]))),
        ] {
            let case = select(vec![chain_of(column("a"), vec![clip])]);
            assert_eq!(
                refused_for_a_bound(&case, None, Stage::Rows, BOUND_NOT_HELD),
                vec![Limit::ABoundTheDtypeCannotHold]
            );
        }
        // A clip inside what another call is given is the same.
        let nested = select(vec![chain_of(
            column("a"),
            vec![compared_with(
                "Expr.eq",
                chain_of(column("k"), vec![clip_with(Some(whole(-1)), None)]),
            )],
        )]);
        assert_eq!(
            refused_for_a_bound(&nested, None, Stage::Rows, BOUND_NOT_HELD),
            vec![Limit::ABoundTheDtypeCannotHold]
        );
    }

    #[test]
    fn a_failed_conversion_is_excused_in_the_empty_table_where_an_aggregate_is_a_bound() {
        // `lit(1, Datetime).clip(lower_bound=k.entropy())`: the entropy of no rows is a NaN, a
        // value of its own that the bound is cast to the dtype from, where a column of no rows has
        // none to convert.
        assert_eq!(
            excused_by_kept(
                "uncovered-a-bound-the-dtype-cannot-hold--an-aggregate-in-the-empty-table.case"
            ),
            vec![Limit::ABoundTheDtypeCannotHold]
        );
        let held = "conversion from `f64` to `datetime[μs, UTC]` failed in column 'k' for 1 out of 1 values: [NaN]";
        let case = select(vec![chain_of(
            column("a"),
            vec![clip_with(Some(chain(column("k"), &["Expr.entropy"])), None)],
        )]);
        assert_eq!(
            refused_for_a_bound(&case, None, Stage::Empty, held),
            vec![Limit::ABoundTheDtypeCannotHold]
        );
        // A clip with no bound is no cause of a failed conversion.
        let unbounded = select(vec![chain_of(column("a"), vec![clip_with(None, None)])]);
        assert_eq!(
            refused_for_a_bound(&unbounded, None, Stage::Empty, held),
            vec![]
        );
    }

    #[test]
    fn a_failed_conversion_that_remains_without_the_bounds_is_not_a_bounds() {
        // A negative exponent fails to become a `u32` whether or not a clip has a bound, so the
        // bound is no cause, and the power's limit excuses it alone.
        let case = select(vec![chain_of(
            column("a"),
            vec![clip_with(Some(whole(1)), None), pow_of(whole(-1))],
        )]);
        assert_eq!(
            refused_for_a_bound(&case, None, Stage::Rows, NEGATIVE_EXPONENT),
            vec![Limit::ANegativeExponent]
        );
    }

    #[test]
    fn a_failed_conversion_is_not_excused_by_a_bound_without_a_clip_or_without_a_bound_given() {
        let nothing = [
            // No clip at all, and a number that fails to convert elsewhere.
            select(vec![chain_of(
                column("a"),
                vec![compared_with(
                    "Expr.sub",
                    chain(column("b"), &["Expr.sqrt"]),
                )],
            )]),
            // A clip with no bound is the expression it follows.
            select(vec![chain_of(column("a"), vec![clip_with(None, None)])]),
        ];
        for case in nothing {
            assert_eq!(
                refused_for_a_bound(&case, None, Stage::Rows, BOUND_NOT_HELD),
                vec![]
            );
        }
    }

    #[test]
    fn only_a_failed_conversion_of_the_rows_without_a_place_is_excused_for_a_bound() {
        let case = select(vec![chain_of(
            column("a"),
            vec![clip_with(Some(chain(column("b"), &["Expr.sqrt"])), None)],
        )]);
        for reason in [
            "`clip` operation not supported for dtype `str` (expected: numeric or temporal)",
            "cannot compare string with numeric type (i64)",
        ] {
            assert_eq!(
                refused_for_a_bound(&case, None, Stage::Rows, reason),
                vec![],
                "{reason}"
            );
        }
        // A refusal that Polars places at a call is the call's, and a table with no rows holds
        // no value to convert.
        assert_eq!(
            refused_for_a_bound(
                &case,
                Some(link_of("exprs", 0, 0)),
                Stage::Rows,
                BOUND_NOT_HELD
            ),
            vec![]
        );
        for stage in [Stage::Schema, Stage::Empty] {
            assert_eq!(
                refused_for_a_bound(&case, None, stage, BOUND_NOT_HELD),
                vec![]
            );
        }
    }

    // ----- A bound with values of its own

    const TEXT_CAST: &str = "conversion from `str` to `i8` failed in column 'b' for 2 out of 3 values: [\"1d\", \"2d\"]";
    const LIST_CAST: &str = "cannot cast List type (inner: 'Int64', to: 'Date')";

    #[test]
    fn a_failed_cast_at_a_clip_is_excused_where_a_bound_is_no_bare_column() {
        let at_the_clip = link_of("exprs", 0, 0);
        for bound in [
            whole(1),
            chain(null(), &[]),
            chain(column("k"), &["Expr.sqrt"]),
        ] {
            let case = select(vec![chain_of(
                column("a"),
                vec![clip_with(Some(bound), None)],
            )]);
            for reason in [TEXT_CAST, LIST_CAST] {
                assert_eq!(
                    tables_refuse(&case, at_the_clip.clone(), ROWS, reason),
                    vec![Limit::ABoundWithValuesOfItsOwn],
                    "{reason}"
                );
            }
        }
        // The other bound is the one with values of its own, and a pivot's aggregate is the same.
        let upper = chain_of(
            column("a"),
            vec![clip_with(Some(chain(column("b"), &[])), Some(whole(1)))],
        );
        assert_eq!(
            tables_refuse(
                &select(vec![upper.clone()]),
                at_the_clip.clone(),
                ROWS,
                TEXT_CAST
            ),
            vec![Limit::ABoundWithValuesOfItsOwn]
        );
        assert_eq!(
            tables_refuse(
                &pivot(upper),
                at(&[parameter("aggregate_function"), Segment::Link(0)]),
                ROWS,
                TEXT_CAST
            ),
            vec![Limit::ABoundWithValuesOfItsOwn]
        );
    }

    #[test]
    fn a_failed_cast_at_a_clip_is_not_excused_by_a_computed_bound_beside_a_bare_one_the_tables_refuse()
     {
        // Beside a bound of `lit(1)`, the bare column `b` of text is refused by the tables for
        // the sample's duration text, as Polars refuses it: the refusal is the bare bound's, and
        // the computed one is no cause. Where `b` is a number the tables allow it, and the
        // computed bound is why.
        let at_the_clip = link_of("exprs", 0, 0);
        let mixed = select(vec![chain_of(
            column("a"),
            vec![clip_with(Some(chain(column("b"), &[])), Some(whole(1)))],
        )]);
        let mut text = mixed.clone();
        text.argument = Representative::String;
        assert_eq!(
            tables_refuse(&text, at_the_clip.clone(), ROWS, TEXT_CAST),
            vec![]
        );
        assert_eq!(
            tables_refuse(&mixed, at_the_clip, ROWS, TEXT_CAST),
            vec![Limit::ABoundWithValuesOfItsOwn]
        );
    }

    #[test]
    fn a_failed_cast_at_a_clip_is_not_excused_by_a_bound_that_is_a_bare_column_or_absent() {
        // A bare column holds the sample's rows as the tables measured them, and Polars refuses
        // it the same way; a clip with no bound casts nothing.
        let at_the_clip = link_of("exprs", 0, 0);
        for clip in [
            clip_with(Some(chain(column("b"), &[])), Some(chain(column("k"), &[]))),
            clip_with(None, None),
        ] {
            let case = select(vec![chain_of(column("a"), vec![clip])]);
            assert_eq!(
                tables_refuse(&case, at_the_clip.clone(), ROWS, TEXT_CAST),
                vec![]
            );
        }
    }

    #[test]
    fn only_a_failed_cast_of_the_rows_at_the_clip_itself_is_excused_for_a_bound_of_its_own() {
        let case = select(vec![chain_of(
            column("a"),
            vec![
                clip_with(Some(whole(1)), None),
                compared_with("Expr.sub", whole(1)),
            ],
        )]);
        // Another reason, another stage, and another call of the chain.
        assert_eq!(
            tables_refuse(&case, link_of("exprs", 0, 0), ROWS, "no"),
            vec![]
        );
        assert_eq!(
            tables_refuse(
                &case,
                link_of("exprs", 0, 0),
                Some(TableStage::Schema),
                TEXT_CAST
            ),
            vec![]
        );
        assert_eq!(
            tables_refuse(&case, link_of("exprs", 0, 1), ROWS, TEXT_CAST),
            vec![]
        );
    }

    const TEXT_COUNT: &str = "conversion from `str` to `u64` failed in column 'b' for 2 out of 3 values: [\"1d\", \"2d\"]";

    #[test]
    fn a_failed_cast_at_a_count_of_characters_is_excused_where_a_count_is_no_bare_column() {
        // Polars converts a count to a whole number, and text that reads as one is converted, where
        // the tables measured the text of the sample's rows, a duration no number reads.
        let at_the_call = link_of("exprs", 0, 0);
        for count in [
            whole(1),
            chain(null(), &[]),
            chain(column("k"), &["Expr.sqrt"]),
        ] {
            for (id, parameter) in [
                ("Expr.str.pad_end", "length"),
                ("Expr.str.zfill", "length"),
                ("Expr.str.head", "n"),
                ("Expr.str.slice", "offset"),
            ] {
                let counted = call(id, vec![(parameter, Value::Expression(count.clone()))]);
                let case = select(vec![chain_of(column("g"), vec![counted])]);
                assert_eq!(
                    tables_refuse(&case, at_the_call.clone(), ROWS, TEXT_COUNT),
                    vec![Limit::ABoundWithValuesOfItsOwn],
                    "{id}"
                );
            }
        }
    }

    #[test]
    fn a_failed_cast_at_a_count_is_not_excused_by_a_bare_column_or_another_call() {
        // A bare column holds the sample's rows as the tables measured them, and Polars refuses it
        // the same way.
        let at_the_call = link_of("exprs", 0, 0);
        let bare = call(
            "Expr.str.pad_end",
            vec![("length", Value::Expression(chain(column("b"), &[])))],
        );
        let case = select(vec![chain_of(column("g"), vec![bare])]);
        assert_eq!(
            tables_refuse(&case, at_the_call.clone(), ROWS, TEXT_COUNT),
            vec![]
        );
        // A text method given no count converts nothing, and a refusal of another call is not the
        // count's.
        let counted = call(
            "Expr.str.pad_end",
            vec![("length", Value::Expression(whole(1)))],
        );
        let case = select(vec![chain_of(
            column("g"),
            vec![counted, call("Expr.str.to_uppercase", Vec::new())],
        )]);
        assert_eq!(
            tables_refuse(&case, link_of("exprs", 0, 1), ROWS, TEXT_COUNT),
            vec![]
        );
        let plain = select(vec![chain_of(
            column("g"),
            vec![call("Expr.str.to_uppercase", Vec::new())],
        )]);
        assert_eq!(tables_refuse(&plain, at_the_call, ROWS, TEXT_COUNT), vec![]);
    }

    // ----- A negated bitwise call

    const NOT_OF_A_NULL: &str = "dtype Null not supported in 'not' operation";

    fn and_of(operand: Expression) -> Call {
        call("Expr.and_", vec![("others", many(vec![operand]))])
    }

    /// A null of the dtype Null, which is given as a dtype.
    fn typed_null() -> Call {
        let dtype = Dtype::new(Representative::Null.dtype()).expect("a dtype");
        call(
            "polars.lit",
            vec![("value", Value::Flag(None)), ("dtype", Value::Dtype(dtype))],
        )
    }

    fn truth() -> Call {
        call("polars.lit", vec![("value", Value::Flag(Some(true)))])
    }

    fn or_of(operand: Expression) -> Call {
        call("Expr.or_", vec![("others", many(vec![operand]))])
    }

    fn xor_of(operand: Expression) -> Call {
        call("Expr.xor", vec![("other", Value::Expression(operand))])
    }

    fn null_plus_null() -> Expression {
        chain_of(null(), vec![compared_with("Expr.add", chain(null(), &[]))])
    }

    fn not() -> Call {
        call("Expr.not_", Vec::new())
    }

    #[test]
    fn a_not_of_the_dtype_null_is_excused_whatever_gives_the_null_its_dtype() {
        for expression in [
            // A null given the dtype Null, which the tables give a literal with none.
            chain_of(
                typed_null(),
                vec![
                    or_of(chain(column("f"), &[])),
                    not(),
                    call("Expr.sum", vec![]),
                ],
            ),
            // A null that a call gives the dtype, as the receiver.
            chain_of(
                null(),
                vec![
                    call("Expr.max", Vec::new()),
                    and_of(chain(column("f"), &[])),
                    not(),
                ],
            ),
            // As an operand, given by a call and by a literal that has the dtype.
            chain_of(column("f"), vec![and_of(null_plus_null()), not()]),
            chain_of(
                column("f"),
                vec![
                    or_of(chain(typed_null(), &[])),
                    not(),
                    call("Expr.min", vec![]),
                ],
            ),
        ] {
            let case = select(vec![expression]);
            assert_eq!(
                polars_refuses_what_the_tables_allow(&case, None, Stage::Rows, NOT_OF_A_NULL),
                vec![Limit::ABitwiseCallOnANull],
                "{}",
                case.document.write()
            );
        }
    }

    #[test]
    fn a_not_of_the_dtype_null_is_not_excused_where_the_tables_give_the_bitwise_call_no_null() {
        for expression in [
            // A null in an operand whose own dtype is a `Boolean`.
            chain_of(
                column("f"),
                vec![
                    and_of(chain(null(), &["Expr.is_null"])),
                    not(),
                    call("Expr.min", vec![]),
                ],
            ),
            // A null that a comparison meets, before a conjunction of columns.
            chain_of(
                column("f"),
                vec![
                    compared_with("Expr.eq", chain(typed_null(), &[])),
                    and_of(chain(column("f"), &[])),
                    not(),
                ],
            ),
        ] {
            let case = select(vec![expression]);
            assert_eq!(
                polars_refuses_what_the_tables_allow(&case, None, Stage::Rows, NOT_OF_A_NULL),
                vec![],
                "{}",
                case.document.write()
            );
        }
    }

    #[test]
    fn a_not_after_a_run_of_and_and_or_is_excused_where_any_call_of_the_run_meets_a_null() {
        // `f & a & true` then `not`, with `a` a null: the null is met by the first call and
        // stays in what the run combines, which the negation reaches through the calls after it.
        let and_then_and = |first: Call, second: Call| {
            chain_of(
                column("f"),
                vec![first, second, not(), call("Expr.min", vec![])],
            )
        };
        for expression in [
            and_then_and(
                and_of(chain(typed_null(), &[])),
                and_of(chain(truth(), &[])),
            ),
            and_then_and(and_of(chain(typed_null(), &[])), or_of(chain(truth(), &[]))),
            and_then_and(or_of(chain(typed_null(), &[])), and_of(chain(truth(), &[]))),
            and_then_and(and_of(chain(truth(), &[])), or_of(chain(typed_null(), &[]))),
            // A null that a call gives the dtype, and a null the run starts from.
            and_then_and(and_of(null_plus_null()), or_of(chain(truth(), &[]))),
            chain_of(
                typed_null(),
                vec![
                    and_of(chain(truth(), &[])),
                    and_of(chain(truth(), &[])),
                    not(),
                    call("Expr.min", vec![]),
                ],
            ),
        ] {
            let case = select(vec![expression]);
            assert_eq!(
                polars_refuses_what_the_tables_allow(&case, None, Stage::Rows, NOT_OF_A_NULL),
                vec![Limit::ABitwiseCallOnANull],
                "{}",
                case.document.write()
            );
        }
    }

    #[test]
    fn a_not_after_bitwise_calls_that_meet_no_null_or_a_run_that_a_call_breaks_is_not_excused() {
        for expression in [
            // No null at all.
            chain_of(
                column("f"),
                vec![
                    and_of(chain(truth(), &[])),
                    or_of(chain(truth(), &[])),
                    not(),
                ],
            ),
            // The null is in an operand whose own dtype is a `Boolean`.
            chain_of(
                column("f"),
                vec![
                    and_of(chain(null(), &["Expr.is_null"])),
                    and_of(chain(truth(), &[])),
                    not(),
                ],
            ),
            // A call that is no `and` or `or` between the null and the negation ends the run.
            chain_of(
                column("f"),
                vec![
                    and_of(chain(typed_null(), &[])),
                    call("Expr.is_null", Vec::new()),
                    and_of(chain(truth(), &[])),
                    not(),
                ],
            ),
            chain_of(
                column("f"),
                vec![
                    and_of(chain(typed_null(), &[])),
                    xor_of(chain(truth(), &[])),
                    and_of(chain(truth(), &[])),
                    not(),
                ],
            ),
            // An `xor` is not what the negation is pushed through.
            chain_of(column("f"), vec![xor_of(chain(typed_null(), &[])), not()]),
        ] {
            let case = select(vec![expression]);
            assert_eq!(
                polars_refuses_what_the_tables_allow(&case, None, Stage::Rows, NOT_OF_A_NULL),
                vec![],
                "{}",
                case.document.write()
            );
        }
    }

    fn filtered(predicate: Expression) -> Case {
        case_of(call(
            "LazyFrame.filter",
            vec![("predicates", many(vec![predicate]))],
        ))
    }

    const NULL_PREDICATE: &str = "filter predicate must be of type `Boolean`, got `Null`";

    #[test]
    fn a_filter_predicate_that_ends_in_a_bitwise_call_that_meets_a_null_of_any_origin_is_excused() {
        for predicate in [
            // A null given the dtype Null, starting the chain, and as the operand.
            chain_of(typed_null(), vec![and_of(chain(truth(), &[]))]),
            chain_of(column("f"), vec![and_of(chain(typed_null(), &[]))]),
            // A null that a call gives the dtype, before the `and` and in it.
            chain_of(
                null(),
                vec![
                    compared_with("Expr.add", chain(null(), &[])),
                    and_of(chain(column("f"), &[])),
                ],
            ),
            chain_of(column("f"), vec![and_of(null_plus_null())]),
            // The dtype Null goes on through the `and` and through an `or` of another null.
            chain_of(
                column("f"),
                vec![and_of(chain(null(), &[])), and_of(chain(column("f"), &[]))],
            ),
            chain_of(
                column("f"),
                vec![and_of(chain(null(), &[])), or_of(chain(null(), &[]))],
            ),
        ] {
            let case = filtered(predicate);
            assert_eq!(
                polars_refuses_what_the_tables_allow(&case, None, Stage::Rows, NULL_PREDICATE),
                vec![Limit::ABitwiseCallOnANull],
                "{}",
                case.document.write()
            );
        }
    }

    #[test]
    fn a_filter_predicate_that_polars_gives_a_boolean_is_not_excused_for_the_dtype_null() {
        for predicate in [
            // An `or` or an `xor` of a `Boolean` and a null is a `Boolean`.
            chain_of(column("f"), vec![or_of(chain(null(), &[]))]),
            chain_of(null(), vec![or_of(chain(column("f"), &[]))]),
            chain_of(column("f"), vec![xor_of(chain(null(), &[]))]),
            // A call after the `and` gives what the tables say.
            chain_of(
                column("f"),
                vec![and_of(chain(null(), &[])), call("Expr.is_null", Vec::new())],
            ),
            // An `xor` of what an `and` of a null gave is no null.
            chain_of(
                column("f"),
                vec![and_of(chain(null(), &[])), xor_of(chain(column("f"), &[]))],
            ),
            // The `and` is inside another call, whose result is what the predicate is.
            chain_of(
                column("f"),
                vec![compared_with(
                    "Expr.eq",
                    chain_of(column("f"), vec![and_of(chain(null(), &[]))]),
                )],
            ),
        ] {
            let case = filtered(predicate);
            assert_eq!(
                polars_refuses_what_the_tables_allow(&case, None, Stage::Rows, NULL_PREDICATE),
                vec![],
                "{}",
                case.document.write()
            );
        }
    }

    #[test]
    fn a_not_of_the_dtype_null_is_excused_where_a_not_comes_straight_after_an_and_or_an_or() {
        for combined in [
            and_of(chain(column("f"), &[])),
            call(
                "Expr.or_",
                vec![("others", many(vec![chain(column("f"), &[])]))],
            ),
        ] {
            let case = select(vec![chain_of(
                null(),
                vec![
                    combined,
                    call("Expr.not_", Vec::new()),
                    call("Expr.min", Vec::new()),
                ],
            )]);
            assert_eq!(
                polars_refuses_what_the_tables_allow(&case, None, Stage::Rows, NOT_OF_A_NULL),
                vec![Limit::ABitwiseCallOnANull]
            );
        }
        // In an expression the chain is given too.
        let nested = select(vec![chain_of(
            column("a"),
            vec![compared_with(
                "Expr.eq",
                chain_of(
                    null(),
                    vec![
                        and_of(chain(column("f"), &[])),
                        call("Expr.not_", Vec::new()),
                    ],
                ),
            )],
        )]);
        assert_eq!(
            polars_refuses_what_the_tables_allow(&nested, None, Stage::Rows, NOT_OF_A_NULL),
            vec![Limit::ABitwiseCallOnANull]
        );
    }

    #[test]
    fn a_not_of_the_dtype_null_is_not_excused_without_a_not_straight_after_a_bitwise_call() {
        for case in [
            // A not, and no bitwise call.
            select(vec![chain(column("f"), &["Expr.not_"])]),
            // A bitwise call that no not follows.
            select(vec![chain_of(
                null(),
                vec![
                    and_of(chain(column("f"), &[])),
                    call("Expr.min", Vec::new()),
                ],
            )]),
            // A call between the two.
            select(vec![chain_of(
                null(),
                vec![
                    and_of(chain(column("f"), &[])),
                    call("Expr.cum_max", vec![]),
                    call("Expr.not_", Vec::new()),
                ],
            )]),
        ] {
            assert_eq!(
                polars_refuses_what_the_tables_allow(&case, None, Stage::Rows, NOT_OF_A_NULL),
                vec![]
            );
        }
    }

    #[test]
    fn a_bitwise_call_that_meets_no_null_excuses_neither_a_not_nor_a_filter_predicate_of_the_dtype_null()
     {
        // A conjunction of two columns, in a filter or before a not, is not what gave the dtype
        // Null: no null reaches it.
        let predicate = "filter predicate must be of type `Boolean`, got `Null`";
        let filtered = |predicates: Vec<Expression>| {
            case_of(call(
                "LazyFrame.filter",
                vec![("predicates", many(predicates))],
            ))
        };
        let columns = || chain_of(column("f"), vec![and_of(chain(column("f"), &[]))]);
        assert_eq!(
            polars_refuses_what_the_tables_allow(
                &filtered(vec![columns()]),
                None,
                Stage::Rows,
                predicate
            ),
            vec![]
        );
        // A null in another predicate, which no bitwise call meets, is no cause either.
        assert_eq!(
            polars_refuses_what_the_tables_allow(
                &filtered(vec![chain(null(), &["Expr.is_null"]), columns()]),
                None,
                Stage::Rows,
                predicate
            ),
            vec![]
        );
        let negated = select(vec![chain_of(
            column("f"),
            vec![
                and_of(chain(column("f"), &[])),
                call("Expr.not_", Vec::new()),
            ],
        )]);
        assert_eq!(
            polars_refuses_what_the_tables_allow(&negated, None, Stage::Rows, NOT_OF_A_NULL),
            vec![]
        );
    }

    #[test]
    fn only_a_refusal_of_a_not_of_the_dtype_null_at_the_rows_without_a_place_is_excused_for_a_not()
    {
        let case = select(vec![chain_of(
            null(),
            vec![
                and_of(chain(column("f"), &[])),
                call("Expr.not_", Vec::new()),
            ],
        )]);
        assert_eq!(
            polars_refuses_what_the_tables_allow(
                &case,
                None,
                Stage::Rows,
                "dtype Int8 not supported in 'not' operation"
            ),
            vec![]
        );
        assert_eq!(
            polars_refuses_what_the_tables_allow(
                &case,
                Some(link_of("exprs", 0, 1)),
                Stage::Rows,
                NOT_OF_A_NULL
            ),
            vec![]
        );
        assert_eq!(
            polars_refuses_what_the_tables_allow(&case, None, Stage::Schema, NOT_OF_A_NULL),
            vec![]
        );
    }

    #[test]
    fn a_filter_predicate_of_the_dtype_null_is_excused_where_a_bitwise_call_is_in_the_case() {
        // Polars gives `null & f` as a filter's predicate the dtype Null, where select shows a
        // `Boolean`.
        let reason = "filter predicate must be of type `Boolean`, got `Null`";
        let filtered = |predicate: Expression| {
            case_of(call(
                "LazyFrame.filter",
                vec![("predicates", many(vec![predicate]))],
            ))
        };
        let case = filtered(chain_of(null(), vec![and_of(chain(column("f"), &[]))]));
        assert_eq!(
            polars_refuses_what_the_tables_allow(&case, None, Stage::Rows, reason),
            vec![Limit::ABitwiseCallOnANull]
        );
        let other_side = filtered(chain_of(column("f"), vec![and_of(chain(null(), &[]))]));
        assert_eq!(
            polars_refuses_what_the_tables_allow(&other_side, None, Stage::Rows, reason),
            vec![Limit::ABitwiseCallOnANull]
        );
        // A predicate that holds no bitwise call, and another step, are not why.
        let plain = filtered(chain(null(), &["Expr.is_null"]));
        assert_eq!(
            polars_refuses_what_the_tables_allow(&plain, None, Stage::Rows, reason),
            vec![]
        );
        let selected = select(vec![chain_of(
            null(),
            vec![and_of(chain(column("f"), &[]))],
        )]);
        assert_eq!(
            polars_refuses_what_the_tables_allow(&selected, None, Stage::Rows, reason),
            vec![]
        );
        // Only the refusal of a predicate of the dtype Null, at the rows, without a place.
        assert_eq!(
            polars_refuses_what_the_tables_allow(
                &case,
                None,
                Stage::Rows,
                "filter predicate must be of type `Boolean`, got `Int64`"
            ),
            vec![]
        );
        assert_eq!(
            polars_refuses_what_the_tables_allow(&case, None, Stage::Schema, reason),
            vec![]
        );
    }

    // ----- A bitwise call in a join's key

    const A_BITWISE_OBJECTION: &str = "^ on Int64 and Boolean is not supported Hint: cast the Boolean to Int64 using pl.Expr.cast().";

    fn right_key_link() -> Place {
        at(&[parameter("right_on"), Segment::Item(0), Segment::Link(0)])
    }

    /// `join(left_on=[k], right_on=[<right>])`, which the tables refuse at the first link of the
    /// right key with `reason`, and Polars allows.
    fn refused_in_a_right_key(right: Expression, reason: &str) -> Vec<Limit> {
        let case = join(chain(column("k"), &[]), right);
        excused_by_the_tables_refusing(&case, right_key_link(), reason)
    }

    fn excused_by_the_tables_refusing(case: &Case, place: Place, reason: &str) -> Vec<Limit> {
        std::thread::scope(|scope| {
            std::thread::Builder::new()
                .stack_size(256 << 20)
                .spawn_scoped(scope, || {
                    tables_refuse(case, place, Some(TableStage::Schema), reason)
                })
                .expect("a thread")
                .join()
                .expect("the limits are asked")
        })
    }

    #[test]
    fn a_bitwise_call_in_a_join_key_is_excused_where_polars_refuses_the_key_when_it_is_selected() {
        // `rb ^ rf` of a whole number and a `Boolean`: Polars refuses it as a select item, and
        // the join accepts it.
        assert_eq!(
            refused_in_a_right_key(
                chain_of(
                    column("rb"),
                    vec![compared_with("Expr.xor", chain(column("rf"), &[]))],
                ),
                A_BITWISE_OBJECTION
            ),
            vec![Limit::ABitwiseCallInAJoinKey]
        );
        assert_eq!(
            excused_by_kept("uncovered-a-bitwise-call-in-a-join-key.case"),
            vec![Limit::ABitwiseCallInAJoinKey]
        );
    }

    #[test]
    fn a_bitwise_call_in_a_join_key_excuses_nothing_that_is_not_one_polars_refuses_in_select() {
        let xor = |left: &str, right: &str| {
            chain_of(
                column(left),
                vec![compared_with("Expr.xor", chain(column(right), &[]))],
            )
        };
        // A call that select accepts, so the position is not why Polars allows it.
        assert_eq!(
            refused_in_a_right_key(xor("rk", "rk"), "^ not allowed on i64 and i64"),
            vec![]
        );
        // A refusal that is not about a bitwise call's dtypes.
        assert_eq!(refused_in_a_right_key(xor("rb", "rf"), "no"), vec![]);
        // A call that is not a bitwise one, whatever the reason says.
        assert_eq!(
            refused_in_a_right_key(
                chain_of(
                    column("rb"),
                    vec![compared_with("Expr.add", chain(column("rf"), &[]))],
                ),
                A_BITWISE_OBJECTION
            ),
            vec![]
        );
        // A call that select accepts, with another bitwise call that it refuses in what the call
        // is given: the refusal of the tables is of the outer call, whose dtypes select accepts,
        // so that other call is not why.
        assert_eq!(
            refused_in_a_right_key(
                chain_of(
                    column("rk"),
                    vec![compared_with(
                        "Expr.xor",
                        chain_of(
                            column("rb"),
                            vec![compared_with("Expr.xor", chain(column("rf"), &[]))],
                        ),
                    )],
                ),
                "^ not allowed on i64 and i64"
            ),
            vec![]
        );
        // The same refusal where the step is no join, which Polars refuses as the tables do.
        let selected = select(vec![xor("a", "f")]);
        assert_eq!(
            excused_by_the_tables_refusing(&selected, link_of("exprs", 0, 0), A_BITWISE_OBJECTION),
            vec![]
        );
        // A key that is no join's own: the window's key in a select.
        let windowed = select(vec![chain_of(
            column("a"),
            vec![call(
                "Expr.over",
                vec![("partition_by", many(vec![xor("a", "f")]))],
            )],
        )]);
        let place = at(&[
            parameter("exprs"),
            Segment::Item(0),
            Segment::Link(0),
            parameter("partition_by"),
            Segment::Item(0),
            Segment::Link(0),
        ]);
        assert_eq!(
            excused_by_the_tables_refusing(&windowed, place, A_BITWISE_OBJECTION),
            vec![]
        );
    }

    // ----- A division by zero

    const DIVISION_BY_ZERO: &str = "division by zero Decimal";

    /// The case with a `Decimal` as the column `a`.
    fn with_a_decimal(mut case: Case) -> Case {
        case.receiver = Representative::Decimal;
        case
    }

    #[test]
    fn a_division_by_zero_is_excused_where_a_division_is_in_a_case_with_a_decimal() {
        for division in ["Expr.floordiv", "Expr.truediv", "Expr.mod"] {
            let case = with_a_decimal(select(vec![chain_of(
                column("b"),
                vec![compared_with(division, chain(column("a"), &[]))],
            )]));
            assert_eq!(
                polars_refuses(&case, None, Stage::Rows, DIVISION_BY_ZERO, Vec::new()),
                vec![Limit::ADivisionByZero],
                "{division}"
            );
        }
        // A division in an expression the chain is given reaches it too.
        let nested = with_a_decimal(select(vec![chain_of(
            column("a"),
            vec![compared_with(
                "Expr.eq",
                chain_of(
                    column("b"),
                    vec![compared_with("Expr.floordiv", chain(column("k"), &[]))],
                ),
            )],
        )]));
        assert_eq!(
            polars_refuses(&nested, None, Stage::Rows, DIVISION_BY_ZERO, Vec::new()),
            vec![Limit::ADivisionByZero]
        );
        // The Decimal may be the argument column.
        let mut other_side = select(vec![chain_of(
            column("a"),
            vec![compared_with("Expr.mod", chain(column("b"), &[]))],
        )]);
        other_side.argument = Representative::Decimal;
        assert_eq!(
            polars_refuses(&other_side, None, Stage::Rows, DIVISION_BY_ZERO, Vec::new()),
            vec![Limit::ADivisionByZero]
        );
    }

    #[test]
    fn a_division_by_zero_of_the_sum_of_nothing_is_excused_at_the_empty_table() {
        // The sum of no rows is zero, a divisor a table of rows does not give: Polars refuses the
        // empty table, which holds a zero where the sample's rows hold none.
        let case = with_a_decimal(select(vec![chain_of(
            column("b"),
            vec![
                call("Expr.sum", Vec::new()),
                compared_with("Expr.floordiv", chain(column("a"), &["Expr.sum"])),
            ],
        )]));
        assert_eq!(
            polars_refuses(&case, None, Stage::Empty, DIVISION_BY_ZERO, Vec::new()),
            vec![Limit::ADivisionByZero]
        );
        // Only the rows and the empty table hold a divisor: the schema holds none, and neither
        // does a place.
        assert_eq!(
            polars_refuses(&case, None, Stage::Schema, DIVISION_BY_ZERO, Vec::new()),
            vec![]
        );
        assert_eq!(
            polars_refuses(
                &case,
                Some(link_of("exprs", 0, 0)),
                Stage::Empty,
                DIVISION_BY_ZERO,
                Vec::new()
            ),
            vec![]
        );
    }

    #[test]
    fn a_division_by_zero_is_not_excused_without_a_decimal_nor_a_division_nor_at_a_place() {
        let divided = || {
            select(vec![chain_of(
                column("b"),
                vec![compared_with("Expr.floordiv", chain(column("a"), &[]))],
            )])
        };
        // No Decimal in the case: Polars divides whole numbers and floats by zero without refusing.
        assert_eq!(
            polars_refuses(&divided(), None, Stage::Rows, DIVISION_BY_ZERO, Vec::new()),
            vec![]
        );
        // A refusal that is not of a Decimal.
        assert_eq!(
            polars_refuses(
                &with_a_decimal(divided()),
                None,
                Stage::Rows,
                "division by zero Int64",
                Vec::new()
            ),
            vec![]
        );
        let none = with_a_decimal(select(vec![chain_of(
            column("b"),
            vec![compared_with("Expr.mul", chain(column("a"), &[]))],
        )]));
        assert_eq!(
            polars_refuses(&none, None, Stage::Rows, DIVISION_BY_ZERO, Vec::new()),
            vec![]
        );
        let case = with_a_decimal(divided());
        assert_eq!(
            polars_refuses(
                &case,
                Some(link_of("exprs", 0, 0)),
                Stage::Rows,
                DIVISION_BY_ZERO,
                Vec::new()
            ),
            vec![]
        );
        assert_eq!(
            polars_refuses(&case, None, Stage::Schema, DIVISION_BY_ZERO, Vec::new()),
            vec![]
        );
        assert_eq!(
            polars_refuses(&case, None, Stage::Rows, "no", Vec::new()),
            vec![]
        );
    }

    // ----- A value beyond a decimal

    const BEYOND_A_DECIMAL: &str = "conversion from `u128` to `decimal[38,0]` failed in column \
        '_POLARS_TMP_PHYS_0' for 1 out of 1 values: [340282366920938463463374607431768211454]";

    /// `a - a == b`: a subtraction, whose values wrap below zero in an unsigned
    /// dtype, compared with a column.
    fn a_subtraction_compared() -> Case {
        select(vec![chain_of(
            column("a"),
            vec![
                compared_with("Expr.sub", chain(column("a"), &[])),
                compared_with("Expr.eq", chain(column("b"), &[])),
            ],
        )])
    }

    #[test]
    fn a_failed_conversion_of_a_value_beyond_a_decimal_is_excused_where_a_subtraction_made_it() {
        assert_eq!(
            polars_refuses(
                &a_subtraction_compared(),
                None,
                Stage::Rows,
                BEYOND_A_DECIMAL,
                Vec::new()
            ),
            vec![Limit::AValueBeyondADecimal]
        );
        // A subtraction in an expression the chain is given reaches it too.
        let nested = select(vec![chain_of(
            column("a"),
            vec![compared_with(
                "Expr.eq",
                chain_of(
                    column("k"),
                    vec![compared_with("Expr.sub", chain(column("a"), &[]))],
                ),
            )],
        )]);
        assert_eq!(
            polars_refuses(&nested, None, Stage::Rows, BEYOND_A_DECIMAL, Vec::new()),
            vec![Limit::AValueBeyondADecimal]
        );
    }

    #[test]
    fn a_failed_conversion_of_a_value_beyond_a_decimal_without_a_subtraction_is_not_excused() {
        // The sample's own values are within a decimal: only a call that makes a
        // value beyond it explains the conversion failing.
        let case = select(vec![chain_of(
            column("a"),
            vec![compared_with("Expr.eq", chain(column("b"), &[]))],
        )]);
        assert_eq!(
            polars_refuses(&case, None, Stage::Rows, BEYOND_A_DECIMAL, Vec::new()),
            vec![]
        );
    }

    #[test]
    fn only_a_failed_conversion_of_a_u128_to_a_decimal_of_the_rows_is_excused() {
        let case = a_subtraction_compared();
        for reason in [
            "conversion from `i64` to `decimal[38,0]` failed in column 'a' for 1 out of 1 values: [1]",
            "conversion from `u128` to `i64` failed in column 'a' for 1 out of 1 values: [1]",
            "cannot compare u128 with decimal[10,2]",
        ] {
            assert_eq!(
                polars_refuses(&case, None, Stage::Rows, reason, Vec::new()),
                vec![],
                "{reason}"
            );
        }
        // A table with no rows holds no value to fail, and Polars saying where it
        // refused the step is another refusal.
        assert_eq!(
            polars_refuses(&case, None, Stage::Empty, BEYOND_A_DECIMAL, Vec::new()),
            vec![]
        );
        assert_eq!(
            polars_refuses(
                &case,
                Some(link_of("exprs", 0, 1)),
                Stage::Schema,
                BEYOND_A_DECIMAL,
                Vec::new()
            ),
            vec![]
        );
    }

    // ----- Keys

    #[test]
    fn a_join_key_dtype_is_the_refusal_of_a_join_that_names_a_decimal() {
        let reason = "datatypes of join keys don't match - `a`: decimal[38,2] on left does not match `key`: decimal[10,2] on right";
        let case = join(chain(column("a"), &[]), chain(column("key"), &[]));
        assert_eq!(
            polars_refuses(&case, Some(step_place()), Stage::Schema, reason, Vec::new()),
            vec![Limit::AJoinKeyDtype]
        );
        let not_decimal = "datatypes of join keys don't match - `a`: i64 on left";
        assert_eq!(
            polars_refuses(
                &case,
                Some(step_place()),
                Stage::Schema,
                not_decimal,
                Vec::new()
            ),
            vec![]
        );
        let select = select(vec![chain(column("a"), &[])]);
        assert_eq!(
            polars_refuses(&select, None, Stage::Schema, reason, Vec::new()),
            vec![]
        );
    }

    #[test]
    fn a_join_key_with_a_null_comparison_at_any_depth_excuses_the_refused_step() {
        let reason = "all join key expressions must be elementwise.";
        // The comparison is one level down, in an operand, and before another.
        let nested = chain_of(
            column("b"),
            vec![compared_with(
                "Expr.le",
                chain_of(
                    column("a"),
                    vec![compared_with("Expr.gt", chain(null(), &[]))],
                ),
            )],
        );
        let after_one = chain_of(
            column("rb"),
            vec![
                call("Expr.max", Vec::new()),
                compared_with("Expr.le", chain(null(), &[])),
                compared_with("Expr.eq", chain(column("rg"), &[])),
            ],
        );
        for key in [nested, after_one] {
            let case = join(key, chain(column("key"), &[]));
            assert_eq!(
                polars_refuses(&case, Some(step_place()), Stage::Schema, reason, Vec::new()),
                vec![Limit::ANullComparisonInAKey]
            );
        }
        // A comparison after the null literal with casts between them.
        let after_a_cast = join(
            chain_of(
                null(),
                vec![
                    cast_to(Representative::Null, Vec::new()),
                    compared_with("Expr.lt", chain(column("a"), &[])),
                ],
            ),
            chain(column("key"), &[]),
        );
        assert_eq!(
            polars_refuses(
                &after_a_cast,
                Some(step_place()),
                Stage::Schema,
                reason,
                Vec::new()
            ),
            vec![Limit::ANullComparisonInAKey]
        );
        // An interval with a whole number below zero as a bound, which Polars decides for what is
        // unsigned and keeps for what is not.
        let negative = join(
            chain_of(
                column("rb"),
                vec![
                    call("Expr.max", Vec::new()),
                    call(
                        "Expr.is_between",
                        vec![
                            ("lower_bound", Value::Expression(chain(column("rk"), &[]))),
                            ("upper_bound", Value::Expression(whole(-1))),
                        ],
                    ),
                ],
            ),
            chain(column("key"), &[]),
        );
        assert_eq!(
            polars_refuses(
                &negative,
                Some(step_place()),
                Stage::Schema,
                reason,
                Vec::new()
            ),
            vec![Limit::ANullComparisonInAKey]
        );
        // A comparison of missing values with a whole number below zero, which Polars decides for
        // what is unsigned and keeps for what is not; one with a number every dtype holds, which
        // it never decides, is no such comparison.
        let compared_by = |entry: &str, number: i64| {
            join(
                chain_of(
                    column("rb"),
                    vec![
                        call("Expr.max", Vec::new()),
                        compared_with(entry, whole(number)),
                        call(
                            "Expr.sub",
                            vec![("other", Value::Expression(chain(column("rk"), &[])))],
                        ),
                    ],
                ),
                chain(column("key"), &[]),
            )
        };
        assert_eq!(
            polars_refuses(
                &compared_by("Expr.ne_missing", -1),
                Some(step_place()),
                Stage::Schema,
                reason,
                Vec::new()
            ),
            vec![Limit::ANullComparisonInAKey]
        );
        for (entry, number) in [("Expr.ne_missing", 1), ("Expr.ne", -1)] {
            assert_eq!(
                polars_refuses(
                    &compared_by(entry, number),
                    Some(step_place()),
                    Stage::Schema,
                    reason,
                    Vec::new()
                ),
                vec![],
                "{entry}"
            );
        }
        // A key that compares nothing with a null, and another reason.
        let plain = join(
            chain(column("a"), &["Expr.cum_max"]),
            chain(column("key"), &[]),
        );
        assert_eq!(
            polars_refuses(
                &plain,
                Some(step_place()),
                Stage::Schema,
                reason,
                Vec::new()
            ),
            vec![]
        );
        let case = join(
            chain_of(
                column("a"),
                vec![compared_with("Expr.gt", chain(null(), &[]))],
            ),
            chain(column("key"), &[]),
        );
        assert_eq!(
            polars_refuses(&case, Some(step_place()), Stage::Schema, "no", Vec::new()),
            vec![]
        );
    }

    #[test]
    fn shifts_that_total_the_sample_excuse_the_tables_refusing_what_follows_them() {
        let shift = |rows: Option<i64>| {
            call(
                "Expr.shift",
                rows.map(|rows| vec![("n", Value::Expression(chain_with_integer(rows)))])
                    .unwrap_or_default(),
            )
        };
        let compare = compared_with("Expr.gt", chain(column("b"), &[]));
        let case = |shifts: Vec<Call>| {
            let mut links = shifts;
            links.push(compare.clone());
            select(vec![chain_of(column("a"), links)])
        };
        let at_the_comparison = |shifts: usize| link_of("exprs", 0, shifts);
        // Three shifts of one row, or one of three, empty the sample.
        let three = case(vec![shift(None), shift(None), shift(None)]);
        assert_eq!(
            tables_refuse(&three, at_the_comparison(3), ROWS, "no"),
            vec![Limit::AShiftThatLeavesNulls]
        );
        let one_of_three = case(vec![shift(Some(3))]);
        assert_eq!(
            tables_refuse(&one_of_three, at_the_comparison(1), ROWS, "no"),
            vec![Limit::AShiftThatLeavesNulls]
        );
        // Fewer leave rows, and the tables' refusal stands.
        let two = case(vec![shift(None), shift(None)]);
        assert_eq!(
            tables_refuse(&two, at_the_comparison(2), ROWS, "no"),
            vec![]
        );
        // The rows are filled, or the refusal is of the schema, or is before them.
        let filled = case(vec![
            shift(None),
            shift(None),
            call(
                "Expr.shift",
                vec![("fill_value", Value::Expression(chain(literal(), &[])))],
            ),
        ]);
        assert_eq!(
            tables_refuse(&filled, at_the_comparison(3), ROWS, "no"),
            vec![]
        );
        assert_eq!(
            tables_refuse(&three, at_the_comparison(3), Some(TableStage::Schema), "no"),
            vec![]
        );
        assert_eq!(
            tables_refuse(&three, at_the_comparison(1), ROWS, "no"),
            vec![]
        );
    }

    #[test]
    fn the_last_of_a_shift_is_null_where_the_sample_has_a_null_there() {
        // The sample's columns are a value, a null and a value. One shift puts the
        // null last, so `b.shift().last()` is null, and three shifts leave only
        // nulls; two shifts put a value last, and no shift leaves the value.
        let shift = || call("Expr.shift", Vec::new());
        let last = || call("Expr.last", Vec::new());
        let item = |links: Vec<Call>| {
            let mut all = links;
            all.push(compared_with("Expr.lt", chain(column("g"), &[])));
            select(vec![chain_of(column("b"), all)])
        };
        let excused = |links: Vec<Call>| {
            let at = links.len();
            tables_refuse(&item(links), link_of("exprs", 0, at), ROWS, "no")
        };
        assert_eq!(
            excused(vec![shift(), last()]),
            vec![Limit::AShiftThatLeavesNulls]
        );
        assert_eq!(
            excused(vec![shift(), shift(), shift(), last()]),
            vec![Limit::AShiftThatLeavesNulls]
        );
        assert_eq!(excused(vec![shift(), shift(), last()]), vec![]);
        assert_eq!(excused(vec![last()]), vec![]);
        assert_eq!(excused(vec![shift(), call("Expr.max", Vec::new())]), vec![]);
    }

    /// `select(polars.from_epoch(polars.time(microsecond=part)).<link>)`, which meets a moment that
    /// Polars makes null in each row where a part is.
    fn counting_a_time_of(part: Expression) -> Case {
        let time = call(
            "polars.time",
            vec![("microsecond", Value::Expression(part))],
        );
        counting(chain_of(time, Vec::new()))
    }

    /// `select(polars.from_epoch(counted))`.
    fn counting(counted: Expression) -> Case {
        let epoch = call(
            "polars.from_epoch",
            vec![("column", Value::Expression(counted))],
        );
        select(vec![chain_of(epoch, Vec::new())])
    }

    #[test]
    fn a_moment_made_of_text_is_only_nulls_and_one_made_of_numbers_is_not() {
        // The text of a column is no number, so Polars makes a null of it in each row, and a null
        // meets a cast that the tables refused for the dtype of a time of day.
        assert_eq!(
            excused_by_kept("uncovered-a-shift-that-leaves-nulls--a-moment-made-of-text.case"),
            vec![Limit::AShiftThatLeavesNulls]
        );
        let at_the_start = || at(&[parameter("exprs"), Segment::Item(0), Segment::Start]);
        let said = "casting from Time to Datetime('μs') not supported; consider using `dt.combine`";
        // A column of numbers holds a time of day in each row that has no null: nothing excused.
        assert_eq!(
            tables_refuse(
                &counting_a_time_of(chain(column("u"), &[])),
                at_the_start(),
                ROWS,
                said
            ),
            vec![]
        );
        // The sample's null is in the middle row of `a` and `b` only, and the other rows hold one.
        assert_eq!(
            tables_refuse(
                &counting_a_time_of(chain(column("a"), &[])),
                at_the_start(),
                ROWS,
                said
            ),
            vec![]
        );
        assert_eq!(
            tables_refuse(
                &counting_a_time_of(chain(column("g"), &[])),
                at_the_start(),
                ROWS,
                said
            ),
            vec![Limit::AShiftThatLeavesNulls]
        );
    }

    #[test]
    fn a_drop_of_only_nulls_leaves_nothing_to_meet_and_a_drop_of_numbers_does_not() {
        assert_eq!(
            excused_by_kept("uncovered-a-shift-that-leaves-nulls--a-drop-of-only-nulls.case"),
            vec![Limit::AShiftThatLeavesNulls]
        );
        let at_the_start = || at(&[parameter("exprs"), Segment::Item(0), Segment::Start]);
        let said = "casting from Time to Datetime('μs') not supported; consider using `dt.combine`";
        let dropped = |part: &str| {
            counting(chain_of(
                call(
                    "polars.time",
                    vec![("microsecond", Value::Expression(chain(column(part), &[])))],
                ),
                vec![call("Expr.drop_nulls", Vec::new())],
            ))
        };
        assert_eq!(
            tables_refuse(&dropped("g"), at_the_start(), ROWS, said),
            vec![Limit::AShiftThatLeavesNulls]
        );
        assert_eq!(
            tables_refuse(&dropped("u"), at_the_start(), ROWS, said),
            vec![]
        );
    }

    #[test]
    fn what_polars_gives_all_null_is_only_nulls_to_meet_and_what_it_gives_values_is_not() {
        // `g.str.slice(offset=g)` has the text of a column for an offset, which is no whole
        // number, so Polars gives a null in each row, and a count since 1970 of it meets only
        // nulls where the tables refused the dtype of text.
        assert_eq!(
            excused_by_kept(
                "uncovered-a-shift-that-leaves-nulls--a-slice-by-text-that-gives-only-nulls.case"
            ),
            vec![Limit::AShiftThatLeavesNulls]
        );
        let at_the_start = || at(&[parameter("exprs"), Segment::Item(0), Segment::Start]);
        let said =
            "casting from string to datetime is not supported. It was removed in Polars 2.0.";
        let sliced = |offset: Expression| {
            counting(chain_of(
                column("g"),
                vec![call(
                    "Expr.str.slice",
                    vec![("offset", Value::Expression(offset))],
                )],
            ))
        };
        // A whole number for the offset keeps the text, which is what the tables refused.
        assert_eq!(
            tables_refuse(&sliced(whole(1)), at_the_start(), ROWS, said),
            vec![]
        );
        assert_eq!(
            tables_refuse(&sliced(chain(column("g"), &[])), at_the_start(), ROWS, said),
            vec![Limit::AShiftThatLeavesNulls]
        );
    }

    #[test]
    fn the_last_of_a_shift_is_null_only_in_a_column_that_has_the_sample_null() {
        // `a` and `b` hold a null in the middle row, which one shift puts last. The
        // columns `f`, `k`, `g`, `h` and `u` hold no null, so what is last after one
        // shift is a value, and the limit excuses nothing after it. Three shifts
        // leave only nulls whatever the column holds.
        let shift = || call("Expr.shift", Vec::new());
        let last = || call("Expr.last", Vec::new());
        let excused = |start: &str, links: Vec<Call>| {
            let at = links.len();
            let mut all = links;
            all.push(compared_with("Expr.lt", chain(column("g"), &[])));
            tables_refuse(
                &select(vec![chain_of(column(start), all)]),
                link_of("exprs", 0, at),
                ROWS,
                "no",
            )
        };
        for start in ["a", "b"] {
            assert_eq!(
                excused(start, vec![shift(), last()]),
                vec![Limit::AShiftThatLeavesNulls],
                "{start}"
            );
        }
        for start in ["f", "k", "g", "h", "u"] {
            assert_eq!(excused(start, vec![shift(), last()]), vec![], "{start}");
            assert_eq!(
                excused(start, vec![shift(), shift(), shift(), last()]),
                vec![Limit::AShiftThatLeavesNulls],
                "{start}"
            );
        }
        // After an aggregate there is one value, which a shift moves away.
        assert_eq!(
            excused("k", vec![call("Expr.max", Vec::new()), shift(), last()]),
            vec![Limit::AShiftThatLeavesNulls]
        );
    }

    #[test]
    fn a_running_sum_of_a_comparison_with_a_null_is_no_limit_of_the_sample() {
        // `f > null` then `cum_sum` then `== b`: Polars folds the comparison with
        // a null into a null whatever the rows are, so what follows the running
        // sum meets a column of nulls however many rows there are. That is a part
        // Polars skips, which a probe settles and no limit excuses.
        let item = |links: Vec<Call>| {
            let mut all = links;
            all.push(compared_with("Expr.eq", chain(column("b"), &[])));
            select(vec![chain_of(column("f"), all)])
        };
        let null_comparison = || compared_with("Expr.gt", chain(null(), &[]));
        let sum = || call("Expr.cum_sum", Vec::new());
        let case = item(vec![null_comparison(), sum()]);
        assert_eq!(
            tables_refuse(&case, link_of("exprs", 0, 2), ROWS, "no"),
            vec![]
        );
    }

    #[test]
    fn what_a_shift_leaves_null_excuses_the_tables_refusing_a_call_it_is_given_to() {
        // `b.first().shift()` is a null, and `k.shift().first()` is the null
        // that the shift left first: a comparison with either is met with
        // nothing, which Polars allows where the tables refuse the dtype.
        let given = |operand: Expression| {
            select(vec![chain_of(
                column("g"),
                vec![compared_with("Expr.lt", operand)],
            )])
        };
        let shift = |links: &[&str]| {
            let mut all: Vec<Call> = Vec::new();
            for id in links {
                all.push(call(id, Vec::new()));
            }
            all
        };
        let at_the_call = link_of("exprs", 0, 0);
        let first_then_shift = given(chain_of(column("b"), shift(&["Expr.first", "Expr.shift"])));
        assert_eq!(
            tables_refuse(&first_then_shift, at_the_call.clone(), ROWS, "no"),
            vec![Limit::AShiftThatLeavesNulls]
        );
        let shift_then_first = given(chain_of(column("k"), shift(&["Expr.shift", "Expr.first"])));
        assert_eq!(
            tables_refuse(&shift_then_first, at_the_call.clone(), ROWS, "no"),
            vec![Limit::AShiftThatLeavesNulls]
        );
        // The first that skips nulls, a shift after a first that is not one, and
        // a first with no shift are not null.
        let skipping = given(chain_of(
            column("k"),
            vec![
                call("Expr.shift", Vec::new()),
                call(
                    "Expr.first",
                    vec![("ignore_nulls", Value::Flag(Some(true)))],
                ),
            ],
        ));
        assert_eq!(
            tables_refuse(&skipping, at_the_call.clone(), ROWS, "no"),
            vec![]
        );
        let plain = given(chain_of(column("k"), shift(&["Expr.first"])));
        assert_eq!(
            tables_refuse(&plain, at_the_call.clone(), ROWS, "no"),
            vec![]
        );
        // The last after one shift is the sample's null; after two it is a value.
        let last = given(chain_of(column("b"), shift(&["Expr.shift", "Expr.last"])));
        assert_eq!(
            tables_refuse(&last, at_the_call.clone(), ROWS, "no"),
            vec![Limit::AShiftThatLeavesNulls]
        );
        let without_the_null = given(chain_of(column("k"), shift(&["Expr.shift", "Expr.last"])));
        assert_eq!(
            tables_refuse(&without_the_null, at_the_call.clone(), ROWS, "no"),
            vec![]
        );
        let later = given(chain_of(
            column("k"),
            shift(&["Expr.shift", "Expr.shift", "Expr.last"]),
        ));
        assert_eq!(tables_refuse(&later, at_the_call, ROWS, "no"), vec![]);
    }

    #[test]
    fn a_first_that_skips_nulls_is_null_where_what_it_takes_is_only_nulls() {
        // The sample's columns are a value, a null and a value. A first that
        // skips nulls takes the first value, which is null only when every row
        // is: after three shifts, or after a first of what a shift moved, which
        // is one null. After one or two shifts a value is first.
        let shift = || call("Expr.shift", Vec::new());
        let first = || call("Expr.first", Vec::new());
        let skipping = || {
            call(
                "Expr.first",
                vec![("ignore_nulls", Value::Flag(Some(true)))],
            )
        };
        let excused = |links: Vec<Call>| {
            let at = links.len();
            let mut all = links;
            all.push(compared_with("Expr.gt", chain(column("h"), &[])));
            tables_refuse(
                &select(vec![chain_of(column("b"), all)]),
                link_of("exprs", 0, at),
                ROWS,
                "no",
            )
        };
        assert_eq!(
            excused(vec![shift(), first(), skipping()]),
            vec![Limit::AShiftThatLeavesNulls]
        );
        assert_eq!(
            excused(vec![shift(), shift(), shift(), skipping()]),
            vec![Limit::AShiftThatLeavesNulls]
        );
        assert_eq!(excused(vec![shift(), skipping()]), vec![]);
        assert_eq!(excused(vec![shift(), shift(), skipping()]), vec![]);
        assert_eq!(excused(vec![first(), skipping()]), vec![]);
    }

    /// The tables refuse `lt` against the text column `g` after `links` on the
    /// column `start`, and Polars allows the step: which limits excuse it.
    fn excused_after(start: &str, links: Vec<Call>) -> Vec<Limit> {
        let at = links.len();
        let mut all = links;
        all.push(compared_with("Expr.lt", chain(column("g"), &[])));
        tables_refuse(
            &select(vec![chain_of(column(start), all)]),
            link_of("exprs", 0, at),
            ROWS,
            "no",
        )
    }

    fn shift_by(rows: i64) -> Call {
        call(
            "Expr.shift",
            vec![("n", Value::Expression(chain_with_integer(rows)))],
        )
    }

    fn plain(id: &str) -> Call {
        call(id, Vec::new())
    }

    #[test]
    fn a_shift_back_leaves_its_nulls_at_the_end() {
        // Polars: `a.shift(-1)` of a value, a null and a value is `null, 3, null`
        // for the column `a` and `1, 1, 2` -> `1, 2, null` for `k`, and `shift(-2)`
        // of `a` is `3, null, null`. The nulls a negative shift leaves are the
        // last rows, not the first.
        let shifts = [Limit::AShiftThatLeavesNulls];
        // The last row is null after a shift back, whatever the column holds there.
        assert_eq!(
            excused_after("k", vec![shift_by(-1), plain("Expr.last")]),
            shifts
        );
        assert_eq!(
            excused_after("a", vec![shift_by(-2), plain("Expr.last")]),
            shifts
        );
        // The first row is a value: `k.shift(-1)` starts with `1`, and `a.shift(-2)` with `3`.
        assert_eq!(
            excused_after("k", vec![shift_by(-1), plain("Expr.first")]),
            vec![]
        );
        assert_eq!(
            excused_after("a", vec![shift_by(-2), plain("Expr.first")]),
            vec![]
        );
        // A shift back that moves every row leaves only nulls.
        assert_eq!(
            excused_after("k", vec![shift_by(-3), plain("Expr.first")]),
            shifts
        );
        // A shift back, then forward: `a` is `3, null, null`, then `null, 3, null`.
        assert_eq!(
            excused_after("a", vec![shift_by(-2), shift_by(1), plain("Expr.first")]),
            shifts
        );
        assert_eq!(
            excused_after("a", vec![shift_by(-2), shift_by(1), plain("Expr.last")]),
            shifts
        );
        // Forward, then back: `1, null, 3` is `null, 1, null`, then `1, null, null`.
        assert_eq!(
            excused_after("a", vec![shift_by(1), shift_by(-1), plain("Expr.first")]),
            vec![]
        );
        assert_eq!(
            excused_after("a", vec![shift_by(1), shift_by(-1), plain("Expr.last")]),
            shifts
        );
    }

    #[test]
    fn what_a_call_before_a_shift_does_to_the_nulls_decides_what_the_shift_leaves() {
        // Polars: `b.cum_sum()` keeps the null of the sample's middle row, so the
        // last row after a shift is null as it is for the column. `b.cum_count()` is
        // `1, 1, 2` and `b.is_null()` is `false, true, false`: neither holds a null,
        // so the last row after a shift is a value, and the column's null does not
        // reach it.
        let shifts = [Limit::AShiftThatLeavesNulls];
        let last = || plain("Expr.last");
        assert_eq!(
            excused_after("b", vec![plain("Expr.cum_sum"), shift_by(1), last()]),
            shifts
        );
        assert_eq!(
            excused_after("b", vec![plain("Expr.cum_max"), shift_by(1), last()]),
            shifts
        );
        for gives_values in ["Expr.cum_count", "Expr.is_null", "Expr.is_not_null"] {
            assert_eq!(
                excused_after("b", vec![plain(gives_values), shift_by(1), last()]),
                vec![],
                "{gives_values}"
            );
        }
        // A call whose effect on the rows is not known leaves nothing known of them, and Polars,
        // asked of the part alone, gives the last of them as a value and not as a null.
        assert_eq!(excused_after("b", vec![over("k", None), last()]), vec![]);
        // Shifts that empty the rows leave only nulls whatever came before them, as
        // long as the rows are the sample's.
        assert_eq!(
            excused_after(
                "b",
                vec![plain("Expr.cum_count"), shift_by(3), plain("Expr.first")]
            ),
            shifts
        );
    }

    #[test]
    fn a_fill_value_moves_the_rows_and_gives_the_rows_it_fills_a_value() {
        // Polars: `b.shift(1, fill_value=7)` is `7, 1, null`: the row that was
        // moved to the end is the sample's null, and the rows filled are values.
        let shifts = [Limit::AShiftThatLeavesNulls];
        let fills = |with: Expression| {
            call(
                "Expr.shift",
                vec![
                    ("n", Value::Expression(chain_with_integer(1))),
                    ("fill_value", Value::Expression(with)),
                ],
            )
        };
        let value = || chain(literal(), &[]);
        assert_eq!(
            excused_after("b", vec![fills(value()), plain("Expr.last")]),
            shifts
        );
        assert_eq!(
            excused_after("k", vec![fills(value()), plain("Expr.last")]),
            vec![]
        );
        assert_eq!(
            excused_after("k", vec![fills(value()), plain("Expr.first")]),
            vec![]
        );
        // Filled with a null, the first row is null.
        assert_eq!(
            excused_after("k", vec![fills(chain(null(), &[])), plain("Expr.first")]),
            shifts
        );
    }

    #[test]
    fn a_call_that_broadcasts_an_aggregate_gives_the_sample_rows_again() {
        // Polars: `k.min().sub(k).shift(1)` is `null, 0, 0` and
        // `k.min().over(k).shift(1)` is `null, 1, 1`: the one value of the aggregate
        // is given to each of the sample's three rows, so a shift leaves a value
        // last, where one row would be shifted away.
        let shifts = [Limit::AShiftThatLeavesNulls];
        let last = || plain("Expr.last");
        for aggregate in [
            "Expr.min",
            "Expr.max",
            "Expr.sum",
            "Expr.first",
            "Expr.last",
        ] {
            let broadcast =
                |call: Call| excused_after("k", vec![plain(aggregate), call, shift_by(1), last()]);
            assert_eq!(
                broadcast(compared_with("Expr.sub", chain(column("k"), &[]))),
                vec![],
                "{aggregate} then sub"
            );
            assert_eq!(broadcast(over("k", None)), vec![], "{aggregate} then over");
        }
        // The one value alone, shifted, leaves one null.
        assert_eq!(
            excused_after("k", vec![plain("Expr.min"), shift_by(1), last()]),
            shifts
        );
    }

    #[test]
    fn the_minimum_and_maximum_of_only_nulls_are_null_and_the_sum_is_not_known() {
        // Polars: `k.shift(3).min()` and `.max()` are null; `k.shift(3).sum()` is `0`
        // of whole numbers, and `null` of a column of the `Null` dtype.
        let shifts = [Limit::AShiftThatLeavesNulls];
        for id in ["Expr.min", "Expr.max"] {
            assert_eq!(
                excused_after("k", vec![shift_by(3), plain(id)]),
                shifts,
                "{id}"
            );
        }
        for id in ["Expr.sum", "Expr.len"] {
            assert_eq!(
                excused_after("k", vec![shift_by(3), plain(id)]),
                vec![],
                "{id}"
            );
        }
        // One shift leaves a value for them to meet.
        assert_eq!(
            excused_after("k", vec![shift_by(1), plain("Expr.min")]),
            vec![]
        );
    }

    #[test]
    fn a_null_in_an_operand_column_carries_through_a_binary_call() {
        // Polars: `b - x` is null where `b` or `x` is, so `b.sub(k)` of the sample's `b`
        // (a value, a null, a value) is `v, null, v`; one shift gives `null, v, null`; and
        // `.mul(a)` of the sample's `a` (a value, a null, a value) is null in every row.
        let shifts = [Limit::AShiftThatLeavesNulls];
        let binary = |id: &str, operand: &str| {
            let operand = chain(column(operand), &[]);
            match id {
                "Expr.and_" | "Expr.or_" => call(id, vec![("others", many(vec![operand]))]),
                _ => compared_with(id, operand),
            }
        };
        assert_eq!(
            excused_after(
                "b",
                vec![
                    binary("Expr.sub", "k"),
                    shift_by(1),
                    binary("Expr.mul", "a")
                ]
            ),
            shifts
        );
        // The receiver holds the value the shift moves to the end, and the operand's null
        // is in the middle: each row is null by one or the other.
        assert_eq!(
            excused_after(
                "k",
                vec![
                    binary("Expr.sub", "a"),
                    shift_by(1),
                    binary("Expr.mul", "a")
                ]
            ),
            shifts
        );
        // An operand that holds no null leaves the row the receiver holds a value in.
        assert_eq!(
            excused_after(
                "b",
                vec![
                    binary("Expr.sub", "k"),
                    shift_by(1),
                    binary("Expr.mul", "k")
                ]
            ),
            vec![]
        );
        // So does a literal, which is one value, and a column the sample has none of.
        let literal = compared_with("Expr.mul", chain_with_integer(2));
        assert_eq!(
            excused_after("b", vec![binary("Expr.sub", "k"), shift_by(1), literal]),
            vec![]
        );
        // A null in the operand alone does not empty the rows that the receiver holds.
        assert_eq!(excused_after("k", vec![binary("Expr.mul", "a")]), vec![]);
        // `or` and `and` do not carry a null through: a null with a true is a true. `xor`
        // does, and is left unknown, which only excuses less.
        for id in ["Expr.or_", "Expr.and_", "Expr.xor"] {
            assert_eq!(
                excused_after("k", vec![shift_by(3), binary(id, "f")]),
                vec![],
                "{id}"
            );
        }
        // A call whose effect on the rows is not known leaves nothing known of them, so the
        // null of the operand does not reach them.
        assert_eq!(
            excused_after(
                "k",
                vec![
                    binary("Expr.sub", "a"),
                    over("k", None),
                    binary("Expr.mul", "a")
                ]
            ),
            vec![]
        );
    }

    /// A whole number as an expression: a literal of it.
    fn chain_with_integer(rows: i64) -> Expression {
        chain(
            call("polars.lit", vec![("value", Value::Integer(rows))]),
            &[],
        )
    }

    #[test]
    fn a_null_comparison_in_one_key_does_not_excuse_what_another_key_may_have_caused() {
        let reason = "all join key expressions must be elementwise.";
        let compared = chain_of(
            column("f"),
            vec![compared_with("Expr.gt", chain(null(), &[]))],
        );
        // The other key holds a running value, or an aggregate beside a column,
        // either of which may be why Polars refuses the join.
        for other in [
            chain(column("rb"), &["Expr.cum_max"]),
            chain_of(
                column("rb"),
                vec![
                    call("Expr.sum", Vec::new()),
                    compared_with("Expr.gt", chain(column("rk"), &[])),
                ],
            ),
        ] {
            let case = case_of(call(
                "LazyFrame.join",
                vec![
                    ("other", Value::Table("other".to_owned())),
                    ("left_on", many(vec![compared.clone(), other])),
                    ("right_on", many(vec![chain(column("rk"), &[]); 2])),
                ],
            ));
            assert_eq!(
                polars_refuses(&case, Some(step_place()), Stage::Schema, reason, Vec::new()),
                vec![]
            );
        }
        // With the other key plain it is excused, and so it is beside a key that is
        // wholly one value, which Polars allows beside an elementwise key.
        let case = join(compared.clone(), chain(column("key"), &[]));
        assert_eq!(
            polars_refuses(&case, Some(step_place()), Stage::Schema, reason, Vec::new()),
            vec![Limit::ANullComparisonInAKey]
        );
        let case = case_of(call(
            "LazyFrame.join",
            vec![
                ("other", Value::Table("other".to_owned())),
                (
                    "left_on",
                    many(vec![compared, chain(column("rb"), &["Expr.sum"])]),
                ),
                ("right_on", many(vec![chain(column("rk"), &[]); 2])),
            ],
        ));
        assert_eq!(
            polars_refuses(&case, Some(step_place()), Stage::Schema, reason, Vec::new()),
            vec![Limit::ANullComparisonInAKey]
        );
    }

    #[test]
    fn the_tables_refusing_inside_a_key_that_is_wholly_one_value_are_excused() {
        let reason = "all join key expressions must be elementwise.";
        let whole = chain(column("a"), &["Expr.cum_max", "Expr.max"]);
        let plain = chain(column("a"), &["Expr.cum_max"]);
        let case = join(whole.clone(), plain.clone());
        let refuse = |case: &Case, parameter: &str, reason: &str| {
            tables_refuse(
                case,
                link_of(parameter, 0, 0),
                Some(TableStage::Schema),
                reason,
            )
        };
        assert_eq!(
            refuse(&case, "left_on", reason),
            vec![Limit::ASingleValueInAKey]
        );
        // The other key is not one value, so what is refused in it is refused.
        assert_eq!(refuse(&join(plain, whole), "left_on", reason), vec![]);
        // Another reason.
        assert_eq!(refuse(&case, "left_on", "no"), vec![]);
    }

    #[test]
    fn a_key_that_holds_a_single_value_beside_a_column_is_not_excused() {
        // That key is refused by a rule, and Polars refuses it too.
        let reason = "all join key expressions must be elementwise.";
        let mixed = chain_of(
            column("f"),
            vec![
                call("Expr.cum_max", Vec::new()),
                compared_with("Expr.le", chain(column("x"), &["Expr.max"])),
            ],
        );
        let case = join(mixed, chain(column("key"), &[]));
        assert_eq!(
            tables_refuse(
                &case,
                link_of("left_on", 0, 0),
                Some(TableStage::Schema),
                reason
            ),
            vec![]
        );
    }

    #[test]
    fn a_window_whose_receiver_holds_a_window_is_a_window_over_a_window() {
        let reason = "window expression with non-elementwise `partition_by` or `order_by` not allowed in aggregation context";
        let chained = select(vec![chain_of(
            column("a"),
            vec![
                over("k", None),
                call("Expr.cum_sum", Vec::new()),
                over("g", None),
            ],
        )]);
        assert_eq!(
            polars_refuses(&chained, None, Stage::Rows, reason, Vec::new()),
            vec![Limit::AWindowOverAWindow]
        );
        // Two windows that are not one over the other, and a window in a key.
        let apart = select(vec![
            chain_of(column("a"), vec![over("k", None)]),
            chain_of(column("b"), vec![over("g", None)]),
        ]);
        assert_eq!(
            polars_refuses(&apart, None, Stage::Rows, reason, Vec::new()),
            vec![]
        );
        let in_a_key = select(vec![chain_of(
            column("a"),
            vec![call(
                "Expr.over",
                vec![(
                    "partition_by",
                    many(vec![chain_of(column("g"), vec![over("k", None)])]),
                )],
            )],
        )]);
        assert_eq!(
            polars_refuses(&in_a_key, None, Stage::Rows, reason, Vec::new()),
            vec![]
        );
        assert_eq!(
            polars_refuses(&chained, None, Stage::Rows, "no", Vec::new()),
            vec![]
        );
    }

    // ----- Places, schemas, Booleans, pivots

    #[test]
    fn a_hidden_schema_is_excused_inside_an_expression_and_not_at_its_end() {
        let reason = "the schema says Int64 and the rows give UInt128";
        let case = select(vec![chain(column("a"), &["Expr.cum_sum", "Expr.sum"])]);
        assert_eq!(
            tables_refuse(&case, link_of("exprs", 0, 0), ROWS, reason),
            vec![Limit::AHiddenSchema]
        );
        // At the last link of the step's own expression it is the step's schema.
        let last = select(vec![chain(column("a"), &["Expr.cum_sum"])]);
        assert_eq!(
            tables_refuse(&last, link_of("exprs", 0, 0), ROWS, reason),
            vec![]
        );
        // Another reason.
        assert_eq!(
            tables_refuse(&case, link_of("exprs", 0, 0), ROWS, "no"),
            vec![]
        );
    }

    #[test]
    fn a_hidden_schema_is_excused_at_the_end_of_a_sort_key_which_is_no_output_of_the_step() {
        // Polars sorts by a key without making it a column of the step's table, so a plan that
        // says `Null` where its rows give `Float64`, as a null divided by a whole number does,
        // is not met where the key is the last call too.
        let reason = "the schema says Null and the rows give Float64";
        let sorted =
            |key: Expression| case_of(call("LazyFrame.sort", vec![("by", many(vec![key]))]));
        let divided = chain_of(column("a"), vec![compared_with("Expr.truediv", whole(1))]);
        assert_eq!(
            tables_refuse(&sorted(divided.clone()), link_of("by", 0, 0), ROWS, reason),
            vec![Limit::AHiddenSchema]
        );
        // The step's own schema is shown by what it selects, however the key is.
        let selected = select(vec![divided]);
        assert_eq!(
            tables_refuse(&selected, link_of("exprs", 0, 0), ROWS, reason),
            vec![]
        );
        // Another reason.
        let divided = chain_of(column("a"), vec![compared_with("Expr.truediv", whole(1))]);
        assert_eq!(
            tables_refuse(&sorted(divided), link_of("by", 0, 0), ROWS, "no"),
            vec![]
        );
    }

    #[test]
    fn a_hidden_schema_is_excused_at_the_end_of_a_join_key_which_is_no_output_of_the_step() {
        let reason = "the schema says Null and the rows give Float64";
        let divided = chain_of(column("a"), vec![compared_with("Expr.truediv", whole(1))]);
        let joined = join(chain(column("key"), &[]), divided.clone());
        assert_eq!(
            tables_refuse(&joined, link_of("right_on", 0, 0), ROWS, reason),
            vec![Limit::AHiddenSchema]
        );
        let selected = select(vec![divided]);
        assert_eq!(
            tables_refuse(&selected, link_of("exprs", 0, 0), ROWS, reason),
            vec![]
        );
    }

    #[test]
    fn an_unrelated_failed_conversion_is_not_excused_in_a_pivots_aggregate() {
        // The pivot's limit is for what a group runs with kernels of its own, not for any
        // conversion that fails: a struct that cannot become a struct, or a schema that cannot
        // be cast to the dtype Null, is no kernel of a group.
        let aggregate = chain_of(column("a"), vec![compared_with("Expr.sub", whole(1))]);
        let at_the_call = at(&[parameter("aggregate_function"), Segment::Link(0)]);
        for (stage, reason) in [
            (
                Some(TableStage::Rows),
                "conversion from `struct[2]` to `struct[2]` failed in column 'b' for 2 out of 3 values: [{1,\"a\"}, {3,\"c\"}]",
            ),
            (
                Some(TableStage::Schema),
                "conversion from `i64` to `null` failed in column 'b'",
            ),
            (
                Some(TableStage::Rows),
                "conversion from `str` to `i32` failed in column 'b' for 2 out of 3 values: [\"1d\", \"2d\"]",
            ),
        ] {
            assert_eq!(
                tables_refuse(
                    &pivot(aggregate.clone()),
                    at_the_call.clone(),
                    stage,
                    reason
                ),
                vec![],
                "{reason}"
            );
        }
    }

    #[test]
    fn a_text_share_of_a_quantile_is_excused_in_a_pivots_aggregate_and_nowhere_else() {
        // A select reads the text "1" as the share it writes, and the aggregate of a pivot, which
        // runs the values of each group, refuses it as no number.
        let reason = "expected expression of dtype 'numeric' for quantile, got 'str'";
        let quantile = chain(column("a"), &["Expr.quantile"]);
        let case = pivot(quantile.clone());
        assert_eq!(
            polars_refuses(&case, None, Stage::Rows, reason, Vec::new()),
            vec![Limit::AGroupAggregate]
        );
        // The same reason is no excuse in a select, in a pivot whose aggregate holds no quantile,
        // at the empty table, for a refusal Polars placed, or in other words.
        assert_eq!(
            polars_refuses(
                &select(vec![quantile]),
                None,
                Stage::Rows,
                reason,
                Vec::new()
            ),
            vec![]
        );
        let without = pivot(chain(column("a"), &["Expr.cum_sum"]));
        assert_eq!(
            polars_refuses(&without, None, Stage::Rows, reason, Vec::new()),
            vec![]
        );
        assert_eq!(
            polars_refuses(&case, None, Stage::Empty, reason, Vec::new()),
            vec![]
        );
        let placed = at(&[parameter("aggregate_function"), Segment::Link(0)]);
        assert_eq!(
            polars_refuses(&case, Some(placed), Stage::Rows, reason, Vec::new()),
            vec![]
        );
        assert_eq!(
            polars_refuses(&case, None, Stage::Rows, "no", Vec::new()),
            vec![]
        );
    }

    #[test]
    fn a_pivot_aggregate_is_excused_by_the_kernel_reason_and_by_the_empty_table_with_one() {
        let case = pivot(chain(column("a"), &["Expr.cum_sum"]));
        let at_the_call = at(&[parameter("aggregate_function"), Segment::Link(0)]);
        assert_eq!(
            tables_refuse(
                &case,
                at_the_call.clone(),
                Some(TableStage::Rows),
                "cannot compare string with numeric type (f64)"
            ),
            vec![Limit::AGroupAggregate]
        );
        assert_eq!(
            tables_refuse(&case, at_the_call, Some(TableStage::Rows), "no"),
            vec![]
        );
        assert_eq!(
            polars_refuses(
                &case,
                None,
                Stage::Empty,
                "`cum_max` operation not supported for dtype `null`",
                Vec::new()
            ),
            vec![Limit::AGroupAggregate]
        );
        assert_eq!(
            polars_refuses(
                &case,
                None,
                Stage::Empty,
                "invalid series dtype: expected `Duration`, got `null` for series with name `a`",
                Vec::new()
            ),
            vec![Limit::AGroupAggregate]
        );
        assert_eq!(
            polars_refuses(&case, None, Stage::Empty, "no", Vec::new()),
            vec![]
        );
        // The same reasons in a step that is no pivot.
        let select = select(vec![chain(column("a"), &["Expr.cum_sum"])]);
        assert_eq!(
            polars_refuses(
                &select,
                None,
                Stage::Empty,
                "`cum_max` operation not supported for dtype `null`",
                Vec::new()
            ),
            vec![]
        );
    }
    // ----- A cast of a value the dtype cannot hold

    const CAST_NOT_HELD: &str =
        "conversion from `f64` to `u32` failed in column 'a' for 1 out of 1 values: [-7.015253]";

    /// A `cast` to `class`, with the arguments `extra`.
    fn cast_to(class: Representative, extra: Vec<(&str, Value)>) -> Call {
        let dtype = Dtype::new(class.dtype()).expect("a dtype");
        let mut given = vec![("dtype", Value::Dtype(dtype))];
        given.extend(extra);
        call("Expr.cast", given)
    }

    #[test]
    fn a_date_made_of_literals_is_a_value_a_strict_cast_cannot_hold_and_the_cast_is_why() {
        // A date made of the numbers 1, 1 and 1.5 is the first of January of the year 1, which a
        // cast to a whole number of eight bits cannot hold, and Polars converts the literal when it
        // works out the schema, naming it the scalar and the call that casts it. The tables measured
        // the dates of 1970.
        assert_eq!(
            excused_by_kept(
                "uncovered-a-cast-of-a-value-the-dtype-cannot-hold--a-date-made-of-literals.case"
            ),
            vec![Limit::ACastOfAValueTheDtypeCannotHold]
        );
        let scalar = "conversion from `date` to `u8` failed in column 'scalar' for 1 out of 1 values: [0001-01-01]";
        let date = || {
            call(
                "polars.date",
                vec![
                    ("year", Value::Expression(whole(1))),
                    ("month", Value::Expression(whole(1))),
                    ("day", Value::Expression(whole(1))),
                ],
            )
        };
        let strict = select(vec![chain_of(
            date(),
            vec![cast_to(Representative::UInt8, Vec::new())],
        )]);
        let at_the_cast = || link_of("exprs", 0, 0);
        // A refusal at the cast that the case holds is the cast's.
        assert_eq!(
            refused_for_a_bound(&strict, Some(at_the_cast()), Stage::Schema, scalar,),
            vec![Limit::ACastOfAValueTheDtypeCannotHold]
        );
        // A refusal at another call, or of a column and not of a scalar, is not.
        let elsewhere = select(vec![chain_of(
            date(),
            vec![
                cast_to(Representative::UInt8, Vec::new()),
                call("Expr.cum_sum", Vec::new()),
            ],
        )]);
        assert_eq!(
            refused_for_a_bound(
                &elsewhere,
                Some(link_of("exprs", 0, 1)),
                Stage::Schema,
                scalar,
            ),
            vec![]
        );
        assert_eq!(
            refused_for_a_bound(
                &strict,
                Some(at_the_cast()),
                Stage::Schema,
                "conversion from `date` to `u8` failed in column 'a' for 1 out of 3 values: [0001-01-01]",
            ),
            vec![]
        );
    }

    #[test]
    fn a_failed_conversion_is_excused_where_a_strict_cast_is_why() {
        // The cast that is told nothing, and the one told to be strict, refuse a value that its
        // dtype cannot hold; Polars stops refusing once no cast is strict.
        for cast in [
            cast_to(Representative::UInt8, Vec::new()),
            cast_to(
                Representative::UInt8,
                vec![("strict", Value::Flag(Some(true)))],
            ),
        ] {
            let case = select(vec![chain_of(column("a"), vec![cast])]);
            assert_eq!(
                refused_for_a_bound(&case, None, Stage::Rows, CAST_NOT_HELD),
                vec![Limit::ACastOfAValueTheDtypeCannotHold]
            );
        }
        // A cast in what another call is given is the same.
        let nested = select(vec![chain_of(
            column("a"),
            vec![compared_with(
                "Expr.eq",
                chain_of(
                    column("k"),
                    vec![cast_to(Representative::UInt8, Vec::new())],
                ),
            )],
        )]);
        assert_eq!(
            refused_for_a_bound(&nested, None, Stage::Rows, CAST_NOT_HELD),
            vec![Limit::ACastOfAValueTheDtypeCannotHold]
        );
    }

    #[test]
    fn a_failed_conversion_is_not_excused_by_a_cast_that_gives_a_null_or_wraps_or_by_no_cast() {
        let nothing = [
            // A cast told to give a null where a value does not convert, and one told to wrap.
            select(vec![chain_of(
                column("a"),
                vec![cast_to(
                    Representative::UInt8,
                    vec![("strict", Value::Flag(Some(false)))],
                )],
            )]),
            select(vec![chain_of(
                column("a"),
                vec![cast_to(
                    Representative::UInt8,
                    vec![("wrap_numerical", Value::Flag(Some(true)))],
                )],
            )]),
            // A case with no cast at all.
            select(vec![chain(column("a"), &["Expr.sqrt"])]),
        ];
        for case in nothing {
            assert_eq!(
                refused_for_a_bound(&case, None, Stage::Rows, CAST_NOT_HELD),
                vec![]
            );
        }
    }

    #[test]
    fn a_failed_conversion_that_remains_without_a_strict_cast_is_not_the_casts() {
        // A negative exponent fails to become a `u32` whether or not a cast is strict, so the
        // cast is no cause, and the power's limit excuses it alone.
        let case = select(vec![chain_of(
            column("a"),
            vec![
                cast_to(Representative::UInt8, Vec::new()),
                pow_of(whole(-1)),
            ],
        )]);
        assert_eq!(
            refused_for_a_bound(&case, None, Stage::Rows, NEGATIVE_EXPONENT),
            vec![Limit::ANegativeExponent]
        );
    }

    #[test]
    fn only_a_failed_conversion_without_a_place_that_is_not_the_schemas_is_excused_for_a_cast() {
        let case = select(vec![chain_of(
            column("a"),
            vec![cast_to(Representative::UInt8, Vec::new())],
        )]);
        assert_eq!(
            refused_for_a_bound(
                &case,
                None,
                Stage::Rows,
                "cannot compare string with numeric type (i64)"
            ),
            vec![]
        );
        // A refusal that Polars places at a call is the call's, and one of the schema is no
        // value that fails to convert.
        assert_eq!(
            refused_for_a_bound(
                &case,
                Some(link_of("exprs", 0, 0)),
                Stage::Rows,
                CAST_NOT_HELD
            ),
            vec![]
        );
        assert_eq!(
            refused_for_a_bound(&case, None, Stage::Schema, CAST_NOT_HELD),
            vec![]
        );
        // A table with no rows meets the value that an aggregate of nothing is.
        assert_eq!(
            refused_for_a_bound(&case, None, Stage::Empty, CAST_NOT_HELD),
            vec![Limit::ACastOfAValueTheDtypeCannotHold]
        );
    }

    const CLOSE_NOT_HELD: &str = "conversion from `u128` to `i128` failed in column 'a' for 1 out of 1 values: [340282366920938463463374607431768211454]";

    #[test]
    fn a_failed_conversion_is_excused_where_a_test_of_closeness_is_why() {
        // The test of closeness converts what it compares to the dtype that holds both, strictly,
        // and Polars stops refusing once the comparison is one that converts nothing.
        let close =
            |other: Expression| call("Expr.is_close", vec![("other", Value::Expression(other))]);
        let case = select(vec![chain_of(
            column("a"),
            vec![close(chain(column("k"), &["Expr.sqrt"]))],
        )]);
        assert_eq!(
            refused_for_a_bound(&case, None, Stage::Rows, CLOSE_NOT_HELD),
            vec![Limit::ACastOfAValueTheDtypeCannotHold]
        );
        // A comparison that is no test of closeness is no cause of the failure.
        let comparison = select(vec![chain_of(
            column("a"),
            vec![compared_with("Expr.gt", chain(column("k"), &["Expr.sqrt"]))],
        )]);
        assert_eq!(
            refused_for_a_bound(&comparison, None, Stage::Rows, CLOSE_NOT_HELD),
            vec![]
        );
    }

    const DIFFERENCE_NOT_HELD: &str = "conversion from `u64` to `i64` failed in column 'g' for 1 out of 1 values: [18446744073709551614]";

    #[test]
    fn a_failed_conversion_is_excused_where_a_difference_of_unsigned_numbers_is_why() {
        // `diff` converts what it follows to the signed dtype of its result, strictly, so the
        // number that a subtraction of unsigned numbers wraps to is refused where the sample's
        // is not, and Polars stops refusing once nothing is a difference.
        assert_eq!(
            excused_by_kept(
                "uncovered-a-cast-of-a-value-the-dtype-cannot-hold--a-difference-of-unsigned-numbers.case"
            ),
            vec![Limit::ACastOfAValueTheDtypeCannotHold]
        );
        let difference = select(vec![chain_of(
            column("k"),
            vec![
                compared_with("Expr.sub", chain(column("k"), &[])),
                call("Expr.diff", Vec::new()),
            ],
        )]);
        assert_eq!(
            refused_for_a_bound(&difference, None, Stage::Rows, DIFFERENCE_NOT_HELD),
            vec![Limit::ACastOfAValueTheDtypeCannotHold]
        );
        // A subtraction with no difference after it is no cause of the failure.
        let subtraction = select(vec![chain_of(
            column("k"),
            vec![compared_with("Expr.sub", chain(column("k"), &[]))],
        )]);
        assert_eq!(
            refused_for_a_bound(&subtraction, None, Stage::Rows, DIFFERENCE_NOT_HELD),
            vec![]
        );
    }

    const COUNT_NOT_HELD: &str =
        "conversion from `f64` to `u64` failed in column 'k' for 1 out of 1 values: [-2.262654]";

    /// A count of characters that holds a negative number of a float: the cosine of `k`, cotangent
    /// of which is `-2.26` in the first row.
    fn a_negative_count() -> Expression {
        chain(column("k"), &["Expr.cos", "Expr.cot"])
    }

    #[test]
    fn a_failed_conversion_is_excused_where_a_count_of_characters_is_why() {
        // Polars converts each count to a whole number strictly, and stops refusing once each is
        // the whole number 1.
        for (id, parameter) in [
            ("Expr.str.head", "n"),
            ("Expr.str.tail", "n"),
            ("Expr.str.slice", "offset"),
            ("Expr.str.pad_start", "length"),
            ("Expr.str.pad_end", "length"),
            ("Expr.str.zfill", "length"),
        ] {
            let counted = call(id, vec![(parameter, Value::Expression(a_negative_count()))]);
            let case = select(vec![chain_of(column("g"), vec![counted])]);
            assert_eq!(
                refused_for_a_bound(&case, None, Stage::Rows, COUNT_NOT_HELD),
                vec![Limit::ACastOfAValueTheDtypeCannotHold],
                "{id}"
            );
        }
        // The length of a slice is converted too, beside an offset that is not the cause.
        let slice = call(
            "Expr.str.slice",
            vec![
                ("offset", Value::Expression(whole(1))),
                ("length", Value::Expression(a_negative_count())),
            ],
        );
        let case = select(vec![chain_of(column("g"), vec![slice])]);
        assert_eq!(
            refused_for_a_bound(&case, None, Stage::Rows, COUNT_NOT_HELD),
            vec![Limit::ACastOfAValueTheDtypeCannotHold]
        );
    }

    #[test]
    fn a_failed_conversion_is_not_excused_by_a_method_of_text_that_counts_nothing() {
        // Text that is not asked to count converts nothing, so the failed conversion is not its
        // count's, and the control is the case itself.
        let case = select(vec![chain_of(
            column("g"),
            vec![call("Expr.str.to_uppercase", Vec::new())],
        )]);
        assert_eq!(
            refused_for_a_bound(&case, None, Stage::Rows, COUNT_NOT_HELD),
            vec![]
        );
        // A method of text that is given a column to strip is no count either.
        let stripped = call(
            "Expr.str.strip_chars",
            vec![("characters", Value::Expression(a_negative_count()))],
        );
        let case = select(vec![chain_of(column("g"), vec![stripped])]);
        assert_eq!(
            refused_for_a_bound(&case, None, Stage::Rows, COUNT_NOT_HELD),
            vec![]
        );
    }

    const LITERAL_COUNT_NOT_HELD: &str =
        "conversion from `i128` to `u64` failed in column 'scalar' for 1 out of 1 values: [-1]";

    #[test]
    fn a_failed_conversion_of_a_literal_count_is_excused_at_the_schema_where_a_count_is_why() {
        // A literal is one value that Polars converts when it works out the schema, where a column
        // is converted when the rows run, so the refusal is of the schema and names the scalar.
        for (id, parameter) in [
            ("Expr.str.pad_start", "length"),
            ("Expr.str.zfill", "length"),
            ("Expr.str.slice", "length"),
        ] {
            let mut given = vec![(parameter, Value::Expression(whole(-1)))];
            if parameter == "length" && id == "Expr.str.slice" {
                given.push(("offset", Value::Expression(whole(1))));
            }
            let case = select(vec![chain_of(column("g"), vec![call(id, given)])]);
            assert_eq!(
                refused_for_a_bound(&case, None, Stage::Schema, LITERAL_COUNT_NOT_HELD),
                vec![Limit::ACastOfAValueTheDtypeCannotHold],
                "{id}"
            );
        }
    }

    #[test]
    fn a_failed_conversion_at_the_schema_is_excused_only_for_the_scalar_of_a_case_with_a_count() {
        let counted = call(
            "Expr.str.zfill",
            vec![("length", Value::Expression(whole(-1)))],
        );
        let case = select(vec![chain_of(column("g"), vec![counted])]);
        // A column that a conversion fails for is the rows', and Polars names it when it runs them.
        let of_a_column =
            "conversion from `i128` to `u64` failed in column 'k' for 1 out of 1 values: [-1]";
        assert_eq!(
            refused_for_a_bound(&case, None, Stage::Schema, of_a_column),
            vec![]
        );
        // A scalar that no count or cast is why: the text is not asked to count anything.
        let plain = select(vec![chain_of(
            column("g"),
            vec![call("Expr.str.to_uppercase", Vec::new())],
        )]);
        assert_eq!(
            refused_for_a_bound(&plain, None, Stage::Schema, LITERAL_COUNT_NOT_HELD),
            vec![]
        );
    }

    // ----- A replacement of columns in a window

    const PATTERN_PER_ROW: &str =
        "not yet implemented: `replace` with a replacement pattern per row";

    fn replace_by(old: Expression, new: Expression) -> Call {
        call(
            "Expr.replace",
            vec![
                ("old", Value::Expression(old)),
                ("new", Value::Expression(new)),
            ],
        )
    }

    #[test]
    fn a_pattern_per_row_is_excused_where_a_window_follows_a_replacement_of_columns() {
        let windowed = select(vec![chain_of(
            column("a"),
            vec![
                replace_by(chain(column("k"), &[]), chain(column("k"), &[])),
                call(
                    "Expr.over",
                    vec![("partition_by", many(vec![chain(column("k"), &[])]))],
                ),
            ],
        )]);
        assert_eq!(
            refused_for_a_bound(&windowed, None, Stage::Rows, PATTERN_PER_ROW),
            vec![Limit::AReplacementOfColumnsInAWindow]
        );
    }

    #[test]
    fn an_output_of_another_length_is_excused_where_a_window_follows_a_replacement_of_columns() {
        let windowed = select(vec![chain_of(
            null(),
            vec![
                replace_by(chain(column("k"), &[]), chain(column("k"), &[])),
                call(
                    "Expr.over",
                    vec![("partition_by", many(vec![chain(column("k"), &[])]))],
                ),
            ],
        )]);
        assert_eq!(
            refused_for_a_bound(
                &windowed,
                None,
                Stage::Rows,
                "output length of `map` (3) must be equal to the input length (1); consider using `apply` instead"
            ),
            vec![Limit::AReplacementOfColumnsInAWindow]
        );
    }

    #[test]
    fn a_pattern_per_row_is_not_excused_without_a_window_or_without_columns_in_the_replacement() {
        // No window: the columns are one list, which Polars takes.
        let plain = select(vec![chain_of(
            column("a"),
            vec![replace_by(chain(column("k"), &[]), chain(column("k"), &[]))],
        )]);
        assert_eq!(
            refused_for_a_bound(&plain, None, Stage::Rows, PATTERN_PER_ROW),
            vec![]
        );
        // A window and a replacement of values that are no column: Polars refuses it for no
        // columns, so they are no cause.
        let literals = select(vec![chain_of(
            column("a"),
            vec![
                replace_by(whole(1), whole(2)),
                call(
                    "Expr.over",
                    vec![("partition_by", many(vec![chain(column("k"), &[])]))],
                ),
            ],
        )]);
        assert_eq!(
            refused_for_a_bound(&literals, None, Stage::Rows, PATTERN_PER_ROW),
            vec![]
        );
    }

    #[test]
    fn an_incomplete_mapping_is_excused_for_a_strict_replacement_given_no_default_only() {
        let reason = "incomplete mapping specified for `replace_strict`";
        let strict = |default: bool| {
            let mut given = vec![
                ("old", Value::Expression(whole(5))),
                ("new", Value::Expression(whole(1))),
            ];
            if default {
                given.push(("default", Value::Expression(whole(2))));
            }
            select(vec![chain_of(
                column("a"),
                vec![call("Expr.replace_strict", given)],
            )])
        };
        assert_eq!(
            refused_for_a_bound(&strict(false), None, Stage::Rows, reason),
            vec![Limit::AStrictReplacementGivenNoDefault]
        );
        // A default maps every value, so the refusal is not for the want of one.
        assert_eq!(
            refused_for_a_bound(&strict(true), None, Stage::Rows, reason),
            vec![]
        );
    }

    // ----- A replacement of every match given a null value

    const NULL_VALUE: &str = "value cannot be 'null' in 'replace' expression";

    fn replacing_every_match_by(value: Call) -> Case {
        let replace = call(
            "Expr.str.replace_all",
            vec![
                ("pattern", Value::Pattern("a".to_owned())),
                ("value", Value::Expression(chain(value, &[]))),
            ],
        );
        select(vec![chain_of(column("g"), vec![replace])])
    }

    #[test]
    fn a_null_value_is_excused_for_a_replacement_of_every_match_given_a_null_literal_or_a_column() {
        // A literal is one value, and a null of it is refused as Polars refuses it. A column
        // is refused where Polars runs its rows in pieces of one, which the control cannot show:
        // the control is that the value made text is not refused, which holds for both.
        for value in [null(), column("b")] {
            for stage in [Stage::Rows, Stage::Empty] {
                assert_eq!(
                    refused_for_a_bound(
                        &replacing_every_match_by(value.clone()),
                        None,
                        stage,
                        NULL_VALUE
                    ),
                    vec![Limit::AReplacementOfEveryMatchGivenANullValue]
                );
            }
        }
    }

    #[test]
    fn a_null_value_is_not_excused_for_a_value_that_is_text_nor_for_another_replacement() {
        // The value is text already: the control is the case itself, which shows nothing, so
        // the limit does not apply.
        let text = call("polars.lit", vec![("value", Value::String("x".to_owned()))]);
        assert_eq!(
            refused_for_a_bound(
                &replacing_every_match_by(text),
                None,
                Stage::Rows,
                NULL_VALUE
            ),
            vec![]
        );
        // A replacement of the first match, and a strict one, are not one of every match.
        let first = call(
            "Expr.str.replace",
            vec![
                ("pattern", Value::Pattern("a".to_owned())),
                ("value", Value::Expression(chain(null(), &[]))),
            ],
        );
        let case = select(vec![chain_of(column("g"), vec![first])]);
        assert_eq!(
            refused_for_a_bound(&case, None, Stage::Rows, NULL_VALUE),
            vec![]
        );
        // Another refusal of the rows, or one that is at a place, is not the value's.
        assert_eq!(
            refused_for_a_bound(
                &replacing_every_match_by(null()),
                None,
                Stage::Rows,
                "invalid series dtype: expected `String`"
            ),
            vec![]
        );
        assert_eq!(
            refused_for_a_bound(
                &replacing_every_match_by(null()),
                Some(Place::of_step(0, None).then(Segment::Parameter("exprs".to_owned()))),
                Stage::Rows,
                NULL_VALUE
            ),
            vec![]
        );
    }

    #[test]
    fn a_null_value_is_excused_for_a_replacement_with_a_count_below_zero_which_is_every_match() {
        // Polars replaces every match where the count is below zero, as `str.replace_all` does, and
        // reads the value of it as one value just the same.
        let every = |count: i64| {
            let replace = call(
                "Expr.str.replace",
                vec![
                    ("pattern", Value::Pattern("a".to_owned())),
                    ("value", Value::Expression(chain(null(), &[]))),
                    ("n", Value::Integer(count)),
                ],
            );
            select(vec![chain_of(column("g"), vec![replace])])
        };
        assert_eq!(
            excused_by_kept(
                "uncovered-a-replacement-of-every-match-given-a-null-value--a-replacement-with-a-count-below-zero.case"
            ),
            vec![Limit::AReplacementOfEveryMatchGivenANullValue]
        );
        assert_eq!(
            refused_for_a_bound(&every(-1), None, Stage::Rows, NULL_VALUE),
            vec![Limit::AReplacementOfEveryMatchGivenANullValue]
        );
        // The first match, or a few, or none, is no replacement of every match.
        for count in [0, 1, 2] {
            assert_eq!(
                refused_for_a_bound(&every(count), None, Stage::Rows, NULL_VALUE),
                vec![],
                "{count}"
            );
        }
    }

    #[test]
    fn a_part_polars_folds_to_a_null_is_excused_where_removing_the_fold_makes_it_refuse() {
        // `datetime(year="1d", month=h + null .str.split("a"), day=null, minute=1)`: Polars folds
        // the sum with a null into a null literal and never meets the list of text the month
        // would be, which the tables refuse to cast to a whole number. Without the sum Polars
        // refuses it for the same reason.
        const SLUG: &str = "a-part-polars-folds-to-a-null";
        let reason = "cannot cast List type (inner: 'String', to: 'Int8')";
        assert_eq!(
            slugs(excused_by_kept(
                "uncovered-a-part-polars-folds-to-a-null.case"
            )),
            vec![SLUG]
        );
        let text = |value: &str| {
            chain_of(
                call(
                    "polars.lit",
                    vec![("value", Value::String(value.to_owned()))],
                ),
                Vec::new(),
            )
        };
        let datetime = |month: Expression| {
            chain_of(
                call(
                    "polars.datetime",
                    vec![
                        ("year", Value::Expression(text("1d"))),
                        ("month", Value::Expression(month)),
                        ("day", Value::Expression(chain(null(), &[]))),
                        ("minute", Value::Expression(whole(1))),
                    ],
                ),
                Vec::new(),
            )
        };
        let split = || call("Expr.str.split", vec![("by", Value::Expression(text("a")))]);
        let sum_with_null = || compared_with("Expr.add", chain(null(), &[]));
        let at_the_start =
            |item: usize| at(&[parameter("exprs"), Segment::Item(item), Segment::Start]);
        let asked = |items: Vec<Expression>, item: usize| {
            slugs(refused_by_the_tables_at(
                &select(items),
                at_the_start(item),
                reason,
            ))
        };
        // The fold, then the part: excused.
        assert_eq!(
            asked(
                vec![datetime(chain_of(
                    column("h"),
                    vec![sum_with_null(), split()]
                ))],
                0
            ),
            vec![SLUG]
        );
        // The null first, as the receiver of the sum: Polars folds that the same, and the control
        // puts the other operand in the null's place.
        assert_eq!(
            slugs(excused_by_kept(
                "uncovered-a-part-polars-folds-to-a-null--a-null-receiver.case"
            )),
            vec![SLUG]
        );
        assert_eq!(
            asked(
                vec![datetime(chain_of(
                    null(),
                    vec![compared_with("Expr.add", chain(column("h"), &[])), split()]
                ))],
                0
            ),
            vec![SLUG]
        );
        // The null is no receiver where something comes before it, and a null that the sum ends
        // the chain with is still a literal's.
        assert_eq!(
            asked(
                vec![datetime(chain_of(
                    null(),
                    vec![compared_with("Expr.add", chain(column("h"), &[]))]
                ))],
                0
            ),
            Vec::<&str>::new()
        );
        // No fold: the control is the case itself, which shows nothing.
        assert_eq!(
            asked(vec![datetime(chain_of(column("h"), vec![split()]))], 0),
            Vec::<&str>::new()
        );
        // The fold ends the chain: the month is then a null literal, which is a literal's.
        assert_eq!(
            asked(
                vec![datetime(chain_of(
                    column("h"),
                    vec![split(), sum_with_null()]
                ))],
                0
            ),
            Vec::<&str>::new()
        );
        // The fold, but the case is not refused for the tables' reason without it: Polars allows a
        // month that is a column, so the fold is no reason it allows the step.
        assert_eq!(
            asked(
                vec![datetime(chain_of(column("k"), vec![sum_with_null()]))],
                0
            ),
            Vec::<&str>::new()
        );
        // A call that follows a fold in its own chain, and is given none, is for the probe.
        assert_eq!(
            slugs(excused_by_kept(
                "stricter-a-running-sum-of-a-null-comparison.case"
            )),
            Vec::<&str>::new()
        );
        // A panic is never the fold's: the exponent is folded to a null, which Polars allows, and
        // is the number 100 without the fold, which it panics for, as the tables recorded, and
        // the limit for a panic is the one that excuses it.
        let overflow = "Polars panicked: attempt to exponentiate with overflow";
        let powered = select(vec![chain_of(
            column("k"),
            vec![call(
                "Expr.pow",
                vec![(
                    "exponent",
                    Value::Expression(chain_of(
                        call("polars.lit", vec![("value", Value::Integer(100))]),
                        vec![sum_with_null(), call("Expr.abs", Vec::new())],
                    )),
                )],
            )],
        )]);
        let found = slugs(refused_by_the_tables_at(
            &powered,
            at(&[parameter("exprs"), Segment::Item(0), Segment::Link(0)]),
            overflow,
        ));
        assert!(!found.contains(&SLUG), "{found:?}");
        assert!(found.contains(&"a-panic"), "{found:?}");
        // A fold in another expression of the step is no reason Polars allows this one.
        assert_eq!(
            asked(
                vec![
                    chain_of(column("k"), vec![sum_with_null()]),
                    datetime(chain_of(column("h"), vec![split()])),
                ],
                1
            ),
            Vec::<&str>::new()
        );
    }

    // ----- A replacement of an aggregate in a window key

    const ANOTHER_LENGTH_OF_AN_AGGREGATE: &str =
        "replacement value length (3) does not match string column length (1)";

    /// `b.over(order_by=[ h.max().replace_all(pattern, value) ])`, with the window as `key`.
    fn windowed_replacement(key: &str, value: Call, partition: bool) -> Case {
        let replace = call(
            "Expr.str.replace_all",
            vec![
                ("pattern", Value::Pattern("d".to_owned())),
                ("value", Value::Expression(chain(value, &[]))),
            ],
        );
        let keyed = chain_of(column("h"), vec![call("Expr.max", Vec::new()), replace]);
        let window = call(
            "Expr.over",
            vec![(
                if partition {
                    "partition_by"
                } else {
                    "order_by"
                },
                many(vec![keyed]),
            )],
        );
        select(vec![chain_of(column(key), vec![window])])
    }

    #[test]
    fn a_value_of_another_length_is_excused_for_an_aggregate_replaced_in_a_window_key() {
        assert_eq!(
            excused_by_kept("uncovered-a-replacement-of-an-aggregate-in-a-window-key.case"),
            vec![Limit::AReplacementOfAnAggregateInAWindowKey]
        );
        assert_eq!(
            refused_for_a_bound(
                &windowed_replacement("b", column("g"), false),
                None,
                Stage::Rows,
                ANOTHER_LENGTH_OF_AN_AGGREGATE
            ),
            vec![Limit::AReplacementOfAnAggregateInAWindowKey]
        );
    }

    #[test]
    fn a_value_of_another_length_is_not_excused_for_a_text_value_nor_without_a_window() {
        // The value is text: the control is the case itself, which shows nothing.
        let text = call("polars.lit", vec![("value", Value::String("x".to_owned()))]);
        assert_eq!(
            refused_for_a_bound(
                &windowed_replacement("b", text, false),
                None,
                Stage::Rows,
                ANOTHER_LENGTH_OF_AN_AGGREGATE
            ),
            vec![]
        );
        // No window: the replacement alone is the cause, and Polars meets it as it does in a select.
        let replace = call(
            "Expr.str.replace_all",
            vec![
                ("pattern", Value::Pattern("d".to_owned())),
                ("value", Value::Expression(chain(column("g"), &[]))),
            ],
        );
        let alone = select(vec![chain_of(
            column("h"),
            vec![call("Expr.max", Vec::new()), replace],
        )]);
        assert_eq!(
            refused_for_a_bound(&alone, None, Stage::Rows, ANOTHER_LENGTH_OF_AN_AGGREGATE),
            vec![]
        );
        // Another refusal, a place and the schema are not this one.
        let windowed = windowed_replacement("b", column("g"), false);
        assert_eq!(
            refused_for_a_bound(&windowed, None, Stage::Rows, "invalid series dtype"),
            vec![]
        );
        assert_eq!(
            refused_for_a_bound(
                &windowed,
                Some(link_of("exprs", 0, 0)),
                Stage::Rows,
                ANOTHER_LENGTH_OF_AN_AGGREGATE
            ),
            vec![]
        );
        assert_eq!(
            refused_for_a_bound(
                &windowed,
                None,
                Stage::Schema,
                ANOTHER_LENGTH_OF_AN_AGGREGATE
            ),
            vec![]
        );
    }

    // ----- A format the dtype cannot write

    const A_YEAR_OF_A_TIME: &str = "cannot format Time with format '%Y'";
    const A_FORMAT_OF_A_DURATION: &str =
        "format \"%Y\" not supported for Duration type (expected one of 'iso' or 'polars')";
    const NOT_A_FORMAT: &str =
        "'polars' is not a valid `to_string` format for date dtype expressions";

    fn written_with(id: &str, start: Call, format: &str) -> Case {
        let write = call(id, vec![("format", Value::String(format.to_owned()))]);
        select(vec![chain_of(start, vec![write])])
    }

    /// The whole number 1 as a literal of `Time`, which is a time of day.
    fn time_of_day() -> Call {
        let dtype = Dtype::new(Representative::Time.dtype()).expect("a dtype");
        call(
            "polars.lit",
            vec![("value", Value::Integer(1)), ("dtype", Value::Dtype(dtype))],
        )
    }

    #[test]
    fn a_format_the_dtype_has_no_field_for_is_excused_at_the_rows_and_the_empty_table() {
        assert_eq!(
            excused_by_kept("uncovered-a-format-the-dtype-cannot-write.case"),
            vec![Limit::AFormatTheDtypeCannotWrite]
        );
        for id in ["Expr.dt.strftime", "Expr.dt.to_string"] {
            for (said, format) in [
                (A_YEAR_OF_A_TIME, "%Y"),
                (A_FORMAT_OF_A_DURATION, "%Y"),
                (NOT_A_FORMAT, "polars"),
            ] {
                for stage in [Stage::Rows, Stage::Empty] {
                    assert_eq!(
                        refused_for_a_bound(
                            &written_with(id, time_of_day(), format),
                            None,
                            stage,
                            said
                        ),
                        vec![Limit::AFormatTheDtypeCannotWrite],
                        "{id} {format} {said}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_format_is_not_excused_where_iso_is_refused_too_nor_at_a_place_nor_for_another_reason() {
        // `iso` is the control, and Polars writes a time of day by it: the case itself shows
        // nothing, so the limit does not apply.
        let case = written_with("Expr.dt.to_string", time_of_day(), "iso");
        assert_eq!(
            refused_for_a_bound(&case, None, Stage::Rows, A_YEAR_OF_A_TIME),
            vec![]
        );
        // A refusal at a place, in the schema, or for another reason is not this one.
        let case = written_with("Expr.dt.to_string", time_of_day(), "%Y");
        assert_eq!(
            refused_for_a_bound(
                &case,
                Some(link_of("exprs", 0, 1)),
                Stage::Rows,
                A_YEAR_OF_A_TIME
            ),
            vec![]
        );
        assert_eq!(
            refused_for_a_bound(&case, None, Stage::Schema, A_YEAR_OF_A_TIME),
            vec![]
        );
        assert_eq!(
            refused_for_a_bound(&case, None, Stage::Rows, "invalid series dtype"),
            vec![]
        );
        // A case with no writing given a format has no format to blame.
        let other = select(vec![chain(column("a"), &["Expr.cum_sum"])]);
        assert_eq!(
            refused_for_a_bound(&other, None, Stage::Rows, A_YEAR_OF_A_TIME),
            vec![]
        );
    }

    // ----- A part of a date that depends on the rows

    const INVALID_TIME: &str = "Invalid time components (0, -2, 0, 1000) supplied";
    const NOT_A_NUMBER: &str =
        "conversion from `f64` to `i8` failed in column 'k' for 1 out of 1 values: [NaN]";

    fn replacing(part: &str, value: Expression) -> Case {
        let replace = call(
            "Expr.dt.replace",
            vec![(part_name(part), Value::Expression(value))],
        );
        select(vec![chain_of(column("a"), vec![replace])])
    }

    /// The parts of a date by name, as the call is given them.
    fn part_name(part: &str) -> &'static str {
        PARTS_OF_A_DATE
            .into_iter()
            .find(|known| *known == part)
            .expect("a part of a date")
    }

    #[test]
    fn a_part_that_the_rows_decide_is_excused_whether_polars_refuses_it_or_the_tables_do() {
        for name in [
            "uncovered-a-part-of-a-date-that-depends-on-the-rows.case",
            "uncovered-a-part-of-a-date-that-depends-on-the-rows--a-value-that-does-not-convert.case",
            "uncovered-a-part-of-a-date-that-depends-on-the-rows--the-tables-refuse-a-part-polars-runs.case",
        ] {
            assert_eq!(
                excused_by_kept(name),
                vec![Limit::APartOfADateThatDependsOnTheRows],
                "{name}"
            );
        }
        let case = replacing("minute", chain(column("k"), &["Expr.not_"]));
        for (said, stage) in [
            (INVALID_TIME, Stage::Rows),
            (INVALID_TIME, Stage::Empty),
            (NOT_A_NUMBER, Stage::Rows),
        ] {
            assert_eq!(
                refused_for_a_bound(&case, None, stage, said),
                vec![Limit::APartOfADateThatDependsOnTheRows],
                "{said}"
            );
        }
    }

    #[test]
    fn a_part_of_a_length_of_time_that_the_rows_decide_is_excused_like_a_part_of_a_date() {
        // `duration(days=a.not_())` of an unsigned `a`: the part is the bitwise complement of the
        // rows, a number beyond the signed dtype that the length of time converts it to, where the
        // tables measured values every dtype holds.
        let not_held = "conversion from `u64` to `i64` failed in column 'a' for 1 out of 1 values: [18446744073709551614]";
        assert_eq!(
            excused_by_kept(
                "uncovered-a-part-of-a-date-that-depends-on-the-rows--a-length-of-time.case"
            ),
            vec![Limit::APartOfADateThatDependsOnTheRows]
        );
        let lasting = |days: Expression| {
            select(vec![chain_of(
                call("polars.duration", vec![("days", Value::Expression(days))]),
                Vec::new(),
            )])
        };
        assert_eq!(
            refused_for_a_bound(
                &lasting(chain(column("k"), &["Expr.not_"])),
                None,
                Stage::Rows,
                not_held
            ),
            vec![Limit::APartOfADateThatDependsOnTheRows]
        );
        // A part that is the number 1 already: the control is the case itself.
        assert_eq!(
            refused_for_a_bound(&lasting(whole(1)), None, Stage::Rows, not_held),
            vec![]
        );
        // A length of time given no part has none to convert.
        assert_eq!(
            refused_for_a_bound(
                &select(vec![chain_of(
                    call("polars.duration", Vec::new()),
                    Vec::new()
                )]),
                None,
                Stage::Rows,
                not_held
            ),
            vec![]
        );
    }

    #[test]
    fn a_part_is_not_excused_where_the_number_1_is_refused_too_nor_at_a_place_nor_for_another_reason()
     {
        // Each part is already the number 1: the control is the case itself, which shows nothing.
        let case = replacing("minute", whole(1));
        assert_eq!(
            refused_for_a_bound(&case, None, Stage::Rows, INVALID_TIME),
            vec![]
        );
        let case = replacing("minute", chain(column("k"), &["Expr.not_"]));
        // A refusal at a place, in the schema, or for another reason is not this one.
        assert_eq!(
            refused_for_a_bound(
                &case,
                Some(link_of("exprs", 0, 1)),
                Stage::Rows,
                INVALID_TIME
            ),
            vec![]
        );
        assert_eq!(
            refused_for_a_bound(&case, None, Stage::Schema, INVALID_TIME),
            vec![]
        );
        assert_eq!(
            refused_for_a_bound(&case, None, Stage::Rows, "invalid series dtype"),
            vec![]
        );
        // A case with no `dt.replace` given a part has no part to blame.
        let other = select(vec![chain(column("a"), &["Expr.cum_sum"])]);
        assert_eq!(
            refused_for_a_bound(&other, None, Stage::Rows, INVALID_TIME),
            vec![]
        );
    }

    #[test]
    fn the_tables_are_blamed_for_a_part_only_where_the_number_1_is_not_refused_by_them() {
        let kept = |name: &str| {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/allowed_fuzz/regressions")
                .join(name);
            Case::decode(&std::fs::read(path).expect("a kept case")).expect("a case")
        };
        // The pivot's day is `true`, which the tables meet as a column of true and false: with the
        // number 1 they stop refusing it for the components.
        let pivot = kept(
            "uncovered-a-part-of-a-date-that-depends-on-the-rows--the-tables-refuse-a-part-polars-runs.case",
        );
        assert!(the_tables_stop_refusing_the_parts_as_ones(
            &pivot,
            is_a_refusal_of_the_components_of_a_date
        ));
        // Where each part is the number 1 already, the control is the case itself, which shows
        // nothing, and where no call is given a part there is no control.
        let ones = replacing("day", whole(1));
        assert!(!the_tables_stop_refusing_the_parts_as_ones(
            &ones,
            is_a_refusal_of_the_components_of_a_date
        ));
        assert_eq!(
            refused_for_a_part_of_a_date_with_ones(&ones, is_a_refusal_of_a_part_of_a_date),
            None
        );
        let none = select(vec![chain(column("a"), &["Expr.cum_sum"])]);
        assert!(!the_tables_stop_refusing_the_parts_as_ones(
            &none,
            is_a_refusal_of_the_components_of_a_date
        ));
        assert_eq!(
            refused_for_a_part_of_a_date_with_ones(&none, is_a_refusal_of_a_part_of_a_date),
            None
        );
    }

    const INVALID_DATE: &str = "Invalid date components (1, 1, 0) supplied";

    /// `select(polars.date(year=1, month=1, day=day))`, the parts of a date that a function makes
    /// of them.
    fn making_a_date(day: Expression) -> Case {
        let date = call(
            "polars.date",
            vec![
                ("year", Value::Expression(whole(1))),
                ("month", Value::Expression(whole(1))),
                ("day", Value::Expression(day)),
            ],
        );
        select(vec![chain_of(date, Vec::new())])
    }

    /// `select(polars.time(hour=hour))`.
    fn making_a_time(hour: Expression) -> Case {
        let time = call("polars.time", vec![("hour", Value::Expression(hour))]);
        select(vec![chain_of(time, Vec::new())])
    }

    #[test]
    fn a_part_a_function_makes_a_date_of_is_excused_where_polars_refuses_it() {
        let day = chain(column("k"), &["Expr.not_"]);
        for (case, said) in [
            (making_a_date(day.clone()), INVALID_DATE),
            (making_a_time(day), INVALID_TIME),
        ] {
            assert_eq!(
                refused_for_a_bound(&case, None, Stage::Rows, said),
                vec![Limit::APartOfADateThatDependsOnTheRows],
                "{said}"
            );
        }
    }

    #[test]
    fn a_date_given_numbers_no_calendar_has_is_excused_though_the_dates_with_ones_are_refused_for_a_cast()
     {
        // `pivot(aggregate=date(300, 1, -1).cast(UInt8))`: a day of -1 is no day, which Polars
        // refuses when the rows run, and the tables measured days that are. With each part made the
        // number 1 the date is the first of January of the year 1, which the cast to eight bits
        // cannot hold: that is another refusal, and not the components', so the parts are why.
        assert_eq!(
            excused_by_kept(
                "uncovered-a-part-of-a-date-that-depends-on-the-rows--a-date-given-numbers-no-calendar-has.case"
            ),
            vec![Limit::APartOfADateThatDependsOnTheRows]
        );
    }

    #[test]
    fn a_part_a_function_makes_a_date_of_is_not_excused_where_the_number_1_is_refused_too() {
        // Each part is already the number 1: the control is the case itself, which shows nothing.
        let ones = making_a_date(whole(1));
        assert_eq!(
            refused_for_a_bound(&ones, None, Stage::Rows, INVALID_DATE),
            vec![]
        );
        assert_eq!(
            refused_for_a_part_of_a_date_with_ones(&ones, is_a_refusal_of_a_part_of_a_date),
            None
        );
        // A refusal at a place, in the schema or for another reason is not this one.
        let case = making_a_date(chain(column("k"), &["Expr.not_"]));
        assert_eq!(
            refused_for_a_bound(
                &case,
                Some(link_of("exprs", 0, 1)),
                Stage::Rows,
                INVALID_DATE
            ),
            vec![]
        );
        assert_eq!(
            refused_for_a_bound(&case, None, Stage::Schema, INVALID_DATE),
            vec![]
        );
        assert_eq!(
            refused_for_a_bound(&case, None, Stage::Rows, "invalid series dtype"),
            vec![]
        );
    }

    /// `sort(by=[when(a.is_null()).then(a.dt.replace(day = lit(false))), len()])` over dates: a sort key
    /// of one value, whose part Polars never evaluates.
    fn a_part_that_a_sort_key_of_one_value_skips() -> Case {
        let replace = call(
            "Expr.dt.replace",
            vec![(
                "day",
                Value::Expression(chain(
                    call("polars.lit", vec![("value", Value::Flag(Some(false)))]),
                    &[],
                )),
            )],
        );
        let when = call(
            "polars.when",
            vec![(
                "predicates",
                many(vec![chain(column("a"), &["Expr.is_null"])]),
            )],
        );
        let then = call(
            "When.then",
            vec![(
                "statement",
                Value::Expression(chain_of(column("a"), vec![replace])),
            )],
        );
        let key = chain_of(when, vec![then]);
        let length = chain(column("a"), &["Expr.len"]);
        let sorted = case_of(call(
            "LazyFrame.sort",
            vec![("by", many(vec![key, length]))],
        ));
        Case {
            receiver: Representative::Date,
            ..sorted
        }
    }

    #[test]
    fn a_part_that_polars_never_evaluates_is_no_part_that_depends_on_the_rows() {
        // The tables refuse the day 0 that `false` is, and Polars allows the case because the key
        // is one value that it never evaluates the part of: the limit must not excuse it, for the
        // case run with the day `false` where Polars does evaluate it is allowed too.
        let case = a_part_that_a_sort_key_of_one_value_skips();
        let prediction = predict(&case);
        assert!(
            matches!(&prediction.verdict, Verdict::Refuses(refused)
                if is_a_refusal_of_the_components_of_a_date(&refused.reason)),
            "{:?}",
            prediction.verdict
        );
        assert!(observe(&case).refusal.is_none());
        assert_eq!(excused_by(&case), vec![]);
    }

    /// `sort(by=[when(a.is_null()).then(a.dt.replace(day = b)), len()])` over dates and text: a sort key
    /// of one value, whose text part, a column, Polars never evaluates.
    fn a_text_part_that_a_sort_key_of_one_value_skips() -> Case {
        let replace = call(
            "Expr.dt.replace",
            vec![("day", Value::Expression(chain(column("b"), &[])))],
        );
        let when = call(
            "polars.when",
            vec![(
                "predicates",
                many(vec![chain(column("a"), &["Expr.is_null"])]),
            )],
        );
        let then = call(
            "When.then",
            vec![(
                "statement",
                Value::Expression(chain_of(column("a"), vec![replace])),
            )],
        );
        let key = chain_of(when, vec![then]);
        let length = chain(column("a"), &["Expr.len"]);
        let sorted = case_of(call(
            "LazyFrame.sort",
            vec![("by", many(vec![key, length]))],
        ));
        Case {
            receiver: Representative::Date,
            argument: Representative::String,
            ..sorted
        }
    }

    #[test]
    fn a_text_part_is_excused_where_the_tables_refuse_its_conversion_and_polars_runs_it() {
        assert_eq!(
            excused_by_kept(
                "uncovered-a-part-of-a-date-that-depends-on-the-rows--a-text-part-the-tables-refuse.case"
            ),
            vec![Limit::APartOfADateThatDependsOnTheRows]
        );
    }

    #[test]
    fn a_text_part_that_polars_never_evaluates_is_no_part_that_depends_on_the_rows() {
        // The tables refuse the conversion of the column of durations they measured, and Polars
        // allows the case because the key is one value that it never evaluates the column of: the
        // part is no literal that writes a number, which is what the limit blames, so it does not
        // excuse it, though the case run with the part a text that is no number is refused, as a
        // literal is converted where the plan is made.
        let case = a_text_part_that_a_sort_key_of_one_value_skips();
        let prediction = predict(&case);
        assert!(
            matches!(&prediction.verdict, Verdict::Refuses(refused)
                if is_a_failed_conversion_of_text(&refused.reason)),
            "{:?}",
            prediction.verdict
        );
        assert!(observe(&case).refusal.is_none());
        assert_eq!(excused_by(&case), vec![]);
    }

    fn any_call_in_step(step: &Call, found: &dyn Fn(&Call) -> bool) -> bool {
        operands(step).iter().any(|(_, held)| any_call(held, found))
    }

    #[test]
    fn a_conversion_of_text_is_a_part_the_tables_blame_only_where_the_number_1_ends_the_refusal() {
        // Where each part is the number 1 already, or none is given, the control is the case itself,
        // which shows nothing.
        let ones = replacing("day", whole(1));
        assert!(!the_tables_stop_refusing_the_parts_as_ones(
            &ones,
            is_a_failed_conversion_of_text
        ));
        // A column is no literal of text, whatever it holds; a literal of text is one, whatever it
        // writes, for Polars reads it where the plan is made.
        let column_part = replacing("day", chain(column("b"), &[]));
        assert!(!any_call_in_step(
            column_part.document.steps()[0].call(),
            &is_given_only_literals_one_of_text
        ));
        for text in ["1", "x"] {
            let literal = replacing(
                "day",
                chain(
                    call(
                        "polars.lit",
                        vec![("value", Value::String(text.to_owned()))],
                    ),
                    &[],
                ),
            );
            assert!(any_call_in_step(
                literal.document.steps()[0].call(),
                &is_given_only_literals_one_of_text
            ));
        }
        let none = select(vec![chain(column("a"), &["Expr.cum_sum"])]);
        assert_eq!(
            refused_for_a_part_with(
                &none,
                Value::String("x".to_owned()),
                is_a_failed_conversion_of_text
            ),
            None
        );
    }

    // ----- A moment beyond the range of a unit

    const BEYOND_NANOSECONDS: &str = "conversion from `datetime[μs, UTC]` to `datetime[ns, UTC]` failed in column 'a' for 1 out of 1 values: [0001-01-01 00:00:00.000001 UTC]";

    fn cast_the_unit_to(unit: &str) -> Case {
        let cast = call(
            "Expr.dt.cast_time_unit",
            vec![("time_unit", Value::Choice(unit.to_owned()))],
        );
        select(vec![chain_of(column("a"), vec![cast])])
    }

    #[test]
    fn a_moment_beyond_nanoseconds_is_excused_at_the_rows_and_the_empty_table() {
        assert_eq!(
            excused_by_kept("uncovered-a-moment-beyond-the-range-of-a-unit.case"),
            vec![Limit::AMomentBeyondTheRangeOfAUnit]
        );
        for stage in [Stage::Rows, Stage::Empty] {
            assert_eq!(
                refused_for_a_bound(&cast_the_unit_to("ns"), None, stage, BEYOND_NANOSECONDS),
                vec![Limit::AMomentBeyondTheRangeOfAUnit]
            );
        }
    }

    #[test]
    fn a_conversion_is_not_excused_for_milliseconds_nor_at_a_place_nor_for_another_reason() {
        // The control is the case itself where the cast is to milliseconds already.
        assert_eq!(
            refused_for_a_bound(
                &cast_the_unit_to("ms"),
                None,
                Stage::Rows,
                BEYOND_NANOSECONDS
            ),
            vec![]
        );
        let case = cast_the_unit_to("ns");
        assert_eq!(
            refused_for_a_bound(
                &case,
                Some(link_of("exprs", 0, 1)),
                Stage::Rows,
                BEYOND_NANOSECONDS
            ),
            vec![]
        );
        assert_eq!(
            refused_for_a_bound(&case, None, Stage::Schema, BEYOND_NANOSECONDS),
            vec![]
        );
        // A conversion to another dtype is not the unit's.
        assert_eq!(
            refused_for_a_bound(
                &case,
                None,
                Stage::Rows,
                "conversion from `str` to `i8` failed in column 'a' for 1 out of 1 values: [x]"
            ),
            vec![]
        );
        let other = select(vec![chain(column("a"), &["Expr.cum_sum"])]);
        assert_eq!(
            refused_for_a_bound(&other, None, Stage::Rows, BEYOND_NANOSECONDS),
            vec![]
        );
    }

    // ----- A struct with no fields

    const NO_KEY: &str = "at least one key is required in a group_by operation";

    fn tested_after_extracting(pattern: &str, test: &str) -> Case {
        let extract = call(
            "Expr.str.extract_groups",
            vec![("pattern", Value::Pattern(pattern.to_owned()))],
        );
        select(vec![chain_of(
            column("g"),
            vec![extract, call(test, Vec::new())],
        )])
    }

    #[test]
    fn a_group_by_without_a_key_is_not_excused_for_a_struct_with_a_field_nor_at_a_place() {
        // The pattern has a group, so the control is the case itself, which shows nothing.
        for pattern in ["(x)", "(?<n>x)"] {
            assert_eq!(
                refused_for_a_bound(
                    &tested_after_extracting(pattern, "Expr.is_unique"),
                    None,
                    Stage::Rows,
                    NO_KEY
                ),
                vec![],
                "{pattern}"
            );
        }
        // A refusal at a place, or the schema, or another reason is not this one.
        let case = tested_after_extracting("x", "Expr.is_unique");
        assert_eq!(
            refused_for_a_bound(&case, Some(link_of("exprs", 0, 1)), Stage::Rows, NO_KEY),
            vec![]
        );
        assert_eq!(
            refused_for_a_bound(&case, None, Stage::Schema, NO_KEY),
            vec![]
        );
        assert_eq!(
            refused_for_a_bound(&case, None, Stage::Rows, "invalid series dtype"),
            vec![]
        );
    }

    // ----- A literal text beside many replacement values

    const ANOTHER_LENGTH: &str =
        "replacement value length (0) does not match string column length (1)";

    fn replacing_in(text_start: Call, value: Call) -> Case {
        let replace = call(
            "Expr.str.replace",
            vec![
                ("pattern", Value::Pattern("a|d".to_owned())),
                ("value", Value::Expression(chain(value, &[]))),
            ],
        );
        select(vec![chain_of(text_start, vec![replace])])
    }

    #[test]
    fn a_value_of_another_length_is_excused_at_the_empty_table_for_a_replacement_of_a_literal() {
        let text = call(
            "polars.lit",
            vec![("value", Value::String("1d".to_owned()))],
        );
        assert_eq!(
            excused_by_kept("uncovered-a-literal-text-beside-many-replacement-values.case"),
            vec![Limit::ALiteralTextBesideManyReplacementValues]
        );
        assert_eq!(
            refused_for_a_bound(
                &replacing_in(text, column("g")),
                None,
                Stage::Empty,
                ANOTHER_LENGTH
            ),
            vec![Limit::ALiteralTextBesideManyReplacementValues]
        );
    }

    #[test]
    fn a_value_of_another_length_is_excused_at_the_rows_where_polars_runs_them_in_one_piece() {
        // At one thread and at two Polars does not broadcast a literal over the rows either, and
        // refuses the replacement for the lengths of the rows: three, or the two of a piece.
        let text = call(
            "polars.lit",
            vec![("value", Value::String("1d".to_owned()))],
        );
        for reason in [
            "replacement value length (3) does not match string column length (1)",
            "replacement value length (2) does not match string column length (1)",
        ] {
            assert_eq!(
                refused_for_a_bound(
                    &replacing_in(text.clone(), column("g")),
                    None,
                    Stage::Rows,
                    reason
                ),
                vec![Limit::ALiteralTextBesideManyReplacementValues],
                "{reason}"
            );
        }
    }

    #[test]
    fn a_value_of_another_length_is_not_excused_for_a_column_nor_at_the_schema_nor_at_a_place() {
        let text = || {
            call(
                "polars.lit",
                vec![("value", Value::String("1d".to_owned()))],
            )
        };
        // The text is a column: the control is the case itself, which shows nothing.
        assert_eq!(
            refused_for_a_bound(
                &replacing_in(column("g"), column("h")),
                None,
                Stage::Empty,
                ANOTHER_LENGTH
            ),
            vec![]
        );
        // Only the rows and the empty table hold the literal against a value: a schema holds none.
        assert_eq!(
            refused_for_a_bound(
                &replacing_in(text(), column("g")),
                None,
                Stage::Schema,
                ANOTHER_LENGTH
            ),
            vec![]
        );
        assert_eq!(
            refused_for_a_bound(
                &replacing_in(text(), column("g")),
                Some(link_of("exprs", 0, 0)),
                Stage::Empty,
                ANOTHER_LENGTH
            ),
            vec![]
        );
        // Another refusal of the empty table is not this one.
        assert_eq!(
            refused_for_a_bound(
                &replacing_in(text(), column("g")),
                None,
                Stage::Empty,
                "invalid series dtype"
            ),
            vec![]
        );
    }

    // ----- A replacement of more than the first match

    const MORE_THAN_THE_FIRST: &str = "regex replacement with 'n > 1' not yet supported";
    const MORE_THAN_THE_FIRST_OF_MANY: &str =
        "multivalue replacement with 'n > 1' not yet supported";

    fn replacing_matches(pattern: &str, count: Option<i64>, value: Expression) -> Case {
        let mut given = vec![
            ("pattern", Value::Pattern(pattern.to_owned())),
            ("value", Value::Expression(value)),
        ];
        given.extend(count.map(|count| ("n", Value::Integer(count))));
        select(vec![chain_of(
            column("g"),
            vec![call("Expr.str.replace", given)],
        )])
    }

    #[test]
    fn a_count_above_one_is_excused_for_a_replacement_of_a_pattern_or_of_many_values() {
        for (reason, case) in [
            (
                MORE_THAN_THE_FIRST,
                replacing_matches("x*", Some(2), chain(column("h"), &[])),
            ),
            (
                MORE_THAN_THE_FIRST_OF_MANY,
                replacing_matches("x", Some(3), chain(column("h"), &[])),
            ),
        ] {
            for stage in [Stage::Rows, Stage::Empty] {
                assert_eq!(
                    refused_for_a_bound(&case, None, stage, reason),
                    vec![Limit::AReplacementOfMoreThanTheFirstMatch],
                    "{reason}"
                );
            }
        }
    }

    #[test]
    fn a_count_above_one_is_not_excused_without_one_given_nor_by_another_refusal() {
        // No count is given, or one that replaces the first match or none: the control is the
        // case itself, or Polars refuses it still, so the count is no cause.
        for count in [None, Some(1), Some(0), Some(-1)] {
            let case = replacing_matches("x*", count, chain(column("h"), &[]));
            assert_eq!(
                refused_for_a_bound(&case, None, Stage::Rows, MORE_THAN_THE_FIRST),
                vec![],
                "{count:?}"
            );
        }
        // A replacement of every match has no count, and another reason is no count's.
        let case = replacing_matches("x*", Some(2), chain(column("h"), &[]));
        assert_eq!(
            refused_for_a_bound(&case, None, Stage::Rows, "invalid series dtype"),
            vec![]
        );
    }

    // ----- A null struct to fill

    const STRATEGY_OF_NOTHING: &str = "fill null strategy not yet supported for dtype: struct[2]";

    fn filled_by(strategy: &str) -> Call {
        call(
            "Expr.fill_null",
            vec![("strategy", Value::Choice(strategy.to_owned()))],
        )
    }

    #[test]
    fn a_fill_by_a_strategy_after_a_call_that_makes_a_null_is_excused_for_a_struct() {
        for maker in [
            "Expr.shift",
            "Expr.first",
            "Expr.last",
            "Expr.min",
            "Expr.max",
        ] {
            let case = select(vec![chain_of(
                column("a"),
                vec![call(maker, Vec::new()), filled_by("one")],
            )]);
            for rows in [true, false] {
                let stage = if rows { Stage::Rows } else { Stage::Empty };
                assert_eq!(
                    polars_refuses(&case, None, stage, STRATEGY_OF_NOTHING, Vec::new()),
                    vec![Limit::ANullStructToFill],
                    "{maker} rows: {rows}"
                );
            }
        }
        // And in a pivot's aggregate, after the last of a group.
        let aggregate = chain_of(
            column("a"),
            vec![call("Expr.last", Vec::new()), filled_by("one")],
        );
        assert_eq!(
            polars_refuses(
                &pivot(aggregate),
                None,
                Stage::Rows,
                STRATEGY_OF_NOTHING,
                Vec::new()
            ),
            vec![Limit::ANullStructToFill]
        );
    }

    #[test]
    fn a_fill_by_a_strategy_in_a_condition_is_excused_for_a_struct() {
        let case = select(vec![chain_of(
            call(
                "polars.when",
                vec![("predicates", many(vec![chain(column("f"), &[])]))],
            ),
            vec![
                call(
                    "When.then",
                    vec![(
                        "statement",
                        Value::Expression(chain_of(column("a"), vec![filled_by("zero")])),
                    )],
                ),
                call(
                    "Then.otherwise",
                    vec![("statement", Value::Expression(chain(column("a"), &[])))],
                ),
            ],
        )]);
        assert_eq!(
            polars_refuses(&case, None, Stage::Rows, STRATEGY_OF_NOTHING, Vec::new()),
            vec![Limit::ANullStructToFill]
        );
    }

    #[test]
    fn a_fill_by_a_strategy_is_not_excused_without_a_call_that_makes_a_null_before_it_or_for_a_dtype_that_is_no_struct()
     {
        let plain = select(vec![chain_of(column("a"), vec![filled_by("one")])]);
        assert_eq!(
            polars_refuses(&plain, None, Stage::Empty, STRATEGY_OF_NOTHING, Vec::new()),
            vec![]
        );
        let after = select(vec![chain_of(
            column("a"),
            vec![call("Expr.last", Vec::new()), filled_by("one")],
        )]);
        assert_eq!(
            polars_refuses(
                &after,
                None,
                Stage::Rows,
                "fill null strategy not yet supported for dtype: list[i64]",
                Vec::new()
            ),
            vec![]
        );
    }
    // ----- A drop of NaNs in a window

    const NAN_OF_TEXT: &str = "`is_not_nan` operation not supported for dtype `str`";

    #[test]
    fn a_drop_of_nans_beside_a_window_that_does_not_follow_it_is_not_excused() {
        // The window is over another column, in another item, so the drop is not in it.
        let case = select(vec![
            chain(column("g"), &["Expr.drop_nans", "Expr.len"]),
            chain_of(
                column("h"),
                vec![
                    call("Expr.first", Vec::new()),
                    call(
                        "Expr.over",
                        vec![("partition_by", many(vec![chain(column("k"), &[])]))],
                    ),
                ],
            ),
        ]);
        assert_eq!(
            refused_for_a_bound(&case, None, Stage::Rows, NAN_OF_TEXT),
            vec![]
        );
    }

    #[test]
    fn a_test_for_nan_of_a_dtype_is_not_excused_without_a_window_or_where_it_is_asked_without_a_drop()
     {
        // No window: Polars leaves the drop out, so a refusal is no drop's.
        let plain = select(vec![chain(column("g"), &["Expr.drop_nans", "Expr.len"])]);
        assert_eq!(
            refused_for_a_bound(&plain, None, Stage::Rows, NAN_OF_TEXT),
            vec![]
        );
        // A test for NaN that is asked of text without a drop is refused with or without it.
        let asked = select(vec![chain_of(
            column("g"),
            vec![
                call("Expr.is_not_nan", Vec::new()),
                call(
                    "Expr.over",
                    vec![("partition_by", many(vec![chain(column("k"), &[])]))],
                ),
            ],
        )]);
        assert_eq!(
            refused_for_a_bound(&asked, None, Stage::Rows, NAN_OF_TEXT),
            vec![]
        );
        // The same test asked of text beside a drop in a window is refused without the drop,
        // which is then no cause.
        let beside = select(vec![
            chain_of(
                column("g"),
                vec![
                    call("Expr.drop_nans", Vec::new()),
                    call("Expr.len", Vec::new()),
                    call(
                        "Expr.over",
                        vec![("partition_by", many(vec![chain(column("k"), &[])]))],
                    ),
                ],
            ),
            chain(column("h"), &["Expr.is_not_nan"]),
        ]);
        assert_eq!(
            refused_for_a_bound(&beside, None, Stage::Rows, NAN_OF_TEXT),
            vec![]
        );
    }

    #[test]
    fn a_failed_conversion_is_excused_where_a_strict_replacement_asked_for_a_dtype_without_a_default_is_why()
     {
        let replace = |default: bool| {
            let mut given = vec![
                ("old", Value::Expression(chain(column("k"), &[]))),
                ("new", Value::Expression(chain(column("k"), &[]))),
                (
                    "return_dtype",
                    Value::Dtype(Dtype::new(Representative::UInt8.dtype()).expect("a dtype")),
                ),
            ];
            if default {
                given.push(("default", Value::Expression(chain(column("k"), &[]))));
            }
            select(vec![chain_of(
                column("a"),
                vec![call("Expr.replace_strict", given)],
            )])
        };
        // The values it lists are bounds too, which the other limit counts.
        assert!(
            refused_for_a_bound(&replace(false), None, Stage::Rows, CAST_NOT_HELD)
                .contains(&Limit::ACastOfAValueTheDtypeCannotHold)
        );
        // Given a default it converts leniently, and refuses no value for a cast.
        assert!(
            !refused_for_a_bound(&replace(true), None, Stage::Rows, CAST_NOT_HELD)
                .contains(&Limit::ACastOfAValueTheDtypeCannotHold)
        );
    }

    #[test]
    fn a_failed_conversion_is_excused_where_a_fill_by_a_value_is_why() {
        let fill = call(
            "Expr.fill_null",
            vec![("value", Value::Expression(whole(1)))],
        );
        let case = select(vec![chain_of(column("a"), vec![fill])]);
        assert_eq!(
            refused_for_a_bound(&case, None, Stage::Rows, CLOSE_NOT_HELD),
            vec![Limit::ACastOfAValueTheDtypeCannotHold]
        );
        // A fill of NaN meets its value as a fill by a value does.
        let nan = select(vec![chain_of(
            column("a"),
            vec![call(
                "Expr.fill_nan",
                vec![("value", Value::Expression(whole(1)))],
            )],
        )]);
        assert_eq!(
            refused_for_a_bound(&nan, None, Stage::Rows, CLOSE_NOT_HELD),
            vec![Limit::ACastOfAValueTheDtypeCannotHold]
        );
        // A fill by a strategy meets no value, so it converts nothing.
        let by_a_strategy = select(vec![chain_of(
            column("a"),
            vec![call(
                "Expr.fill_null",
                vec![("strategy", Value::Choice("forward".to_owned()))],
            )],
        )]);
        assert_eq!(
            refused_for_a_bound(&by_a_strategy, None, Stage::Rows, CLOSE_NOT_HELD),
            vec![]
        );
    }
    #[test]
    fn a_control_that_cannot_be_built_shows_nothing_and_no_limit_that_needs_it_applies() {
        // A case with no strict cast, no drop and no explode has nothing to make the control of,
        // so it is no evidence that the call is not the cause, which is not a refusal either.
        let plain = select(vec![chain(column("a"), &["Expr.sqrt"])]);
        assert_eq!(
            refused_for_a_failed_conversion_without_strict_casts(&plain, false),
            None
        );
        assert_eq!(refused_for_lengths_without_drops(&plain), None);
        assert_eq!(refused_for_lengths_without_explodes(&plain), None);
        assert_eq!(refused_for_a_test_of_nan_without_drops(&plain), None);
        assert_eq!(refused_for_a_strategy_without_nulls_made(&plain), None);
        assert_eq!(refused_for_a_pattern_per_row_without_columns(&plain), None);
        assert_eq!(refused_for_a_failed_conversion_without_bounds(&plain), None);
    }

    // ----- An interval of null bounds

    fn is_between(lower: Expression, upper: Expression) -> Call {
        call(
            "Expr.is_between",
            vec![
                ("lower_bound", Value::Expression(lower)),
                ("upper_bound", Value::Expression(upper)),
            ],
        )
    }

    #[test]
    fn an_interval_of_null_bounds_is_excused_as_a_literal_where_polars_refuses_columns_of_its_dtype()
     {
        // The null literal of a list dtype as both bounds of a text: Polars folds the interval
        // into a null, and refuses it when the bounds are columns.
        assert_eq!(
            excused_by_kept("uncovered-a-literal--an-interval-of-null-bounds.case"),
            vec![Limit::ALiteral]
        );
    }

    #[test]
    fn an_interval_that_follows_a_null_literal_is_excused_as_a_literal_whatever_its_closed_says() {
        // Polars folds the comparisons of a null literal into nulls, so the interval is allowed
        // where the tables refuse a list for the comparison of its `closed`.
        assert_eq!(
            excused_by_kept("uncovered-a-literal--an-interval-that-follows-a-null-literal.case"),
            vec![Limit::ALiteral]
        );
    }

    #[test]
    fn an_interval_is_not_excused_by_a_bound_that_is_not_a_null_or_by_a_null_beside_another_call() {
        let at_the_interval = link_of("exprs", 0, 0);
        let nothing = [
            // No bound is a null literal.
            (
                select(vec![chain_of(
                    column("a"),
                    vec![is_between(chain(column("b"), &[]), chain(column("k"), &[]))],
                )]),
                at_the_interval.clone(),
            ),
            // A bound that is a number, which is no null.
            (
                select(vec![chain_of(
                    column("a"),
                    vec![is_between(whole(1), chain(column("k"), &[]))],
                )]),
                at_the_interval.clone(),
            ),
            // A null literal that is the operand of a comparison, not of an interval.
            (
                select(vec![chain_of(
                    column("a"),
                    vec![compared_with("Expr.eq", chain(null(), &[]))],
                )]),
                at_the_interval.clone(),
            ),
            // A null bound of an interval that the tables refuse nothing at: the call refused
            // follows it.
            (
                select(vec![chain_of(
                    column("a"),
                    vec![
                        is_between(chain(null(), &[]), chain(column("k"), &[])),
                        call("Expr.sum", Vec::new()),
                    ],
                )]),
                link_of("exprs", 0, 1),
            ),
        ];
        for (case, place) in nothing {
            assert_eq!(
                tables_refuse(
                    &case,
                    place,
                    ROWS,
                    "cannot cast List type (inner: 'String', to: 'String')"
                ),
                vec![]
            );
        }
    }
    // ----- A value a replacement lists

    fn replace_with(id: &str, old: Expression, new: Expression) -> Call {
        let mut given = vec![
            ("old", Value::Expression(old)),
            ("new", Value::Expression(new)),
        ];
        if id == "Expr.replace_strict" {
            given.push(("default", Value::Expression(chain(column("k"), &[]))));
        }
        call(id, given)
    }

    #[test]
    fn a_failed_conversion_is_excused_where_a_value_a_replacement_lists_is_why() {
        // The values a replacement lists are cast to the dtype it follows, a number out of range
        // is refused, and Polars stops refusing once they are null.
        for id in ["Expr.replace", "Expr.replace_strict"] {
            for replacement in [
                replace_with(id, whole(300), whole(1)),
                replace_with(id, whole(1), whole(-1)),
            ] {
                let case = select(vec![chain_of(column("a"), vec![replacement])]);
                assert_eq!(
                    refused_for_a_bound(&case, None, Stage::Rows, BOUND_NOT_HELD),
                    vec![Limit::ABoundTheDtypeCannotHold],
                    "{id}"
                );
            }
        }
    }

    #[test]
    fn a_failed_conversion_that_remains_without_the_listed_values_is_not_a_replacements() {
        // A negative exponent fails to become a `u32` whether or not a replacement lists a value.
        let case = select(vec![chain_of(
            column("a"),
            vec![
                replace_with("Expr.replace", whole(1), whole(2)),
                pow_of(whole(-1)),
            ],
        )]);
        assert_eq!(
            refused_for_a_bound(&case, None, Stage::Rows, NEGATIVE_EXPONENT),
            vec![Limit::ANegativeExponent]
        );
    }
    // ----- A drop of nulls or of NaNs

    fn refused_for_lengths(case: &Case) -> Vec<Limit> {
        std::thread::scope(|scope| {
            std::thread::Builder::new()
                .stack_size(256 << 20)
                .spawn_scoped(scope, || {
                    polars_refuses(case, None, Stage::Rows, LENGTHS, Vec::new())
                })
                .expect("a thread")
                .join()
                .expect("the limits are asked")
        })
    }

    #[test]
    fn a_refusal_about_lengths_is_the_drops_where_polars_stops_refusing_once_no_call_drops() {
        // A column that holds a null, with the nulls dropped, is fewer values than the table's
        // rows, beside the columns another item of `with_columns` is.
        for drop in ["Expr.drop_nulls", "Expr.drop_nans"] {
            let case = case_of(call(
                "LazyFrame.with_columns",
                vec![("exprs", many(vec![chain(column("a"), &[drop])]))],
            ));
            assert_eq!(
                refused_for_lengths(&case),
                vec![Limit::ADropChangesTheLengths],
                "{drop}"
            );
        }
        // A drop inside what a call is given is the same.
        let nested = case_of(call(
            "LazyFrame.with_columns",
            vec![(
                "exprs",
                many(vec![chain_of(
                    column("a"),
                    vec![compared_with(
                        "Expr.eq",
                        chain(column("a"), &["Expr.drop_nulls"]),
                    )],
                )]),
            )],
        ));
        assert_eq!(
            refused_for_lengths(&nested),
            vec![Limit::ADropChangesTheLengths]
        );
    }

    #[test]
    fn what_replaces_by_fewer_values_after_a_drop_is_a_refusal_about_the_drop_and_not_the_rows() {
        let reason =
            "`new` input for `replace` must have the same length as `old` or have length 1";
        let replace = |new: Expression| {
            case_of(call(
                "LazyFrame.select",
                vec![(
                    "exprs",
                    many(vec![chain_of(
                        column("g"),
                        vec![call(
                            "Expr.replace",
                            vec![
                                ("old", Value::Expression(chain(column("b"), &[]))),
                                ("new", Value::Expression(new)),
                            ],
                        )],
                    )]),
                )],
            ))
        };
        assert_eq!(
            refused_for_a_bound(
                &replace(chain(column("b"), &["Expr.drop_nulls"])),
                None,
                Stage::Rows,
                reason
            ),
            vec![Limit::ADropChangesTheLengths]
        );
        // A replacement given what replaces it after a comparison and a drop: the fill that stands
        // for the drop is refused about lengths for another reason, which is no evidence for the
        // drop, so the control compares the words of the replacement alone.
        assert_eq!(
            excused_by_kept(
                "uncovered-a-drop-changes-the-lengths--what-replaces-after-a-comparison.case"
            ),
            vec![Limit::ADropChangesTheLengths]
        );
        // Without a drop the lengths are the rows' own, and the shared words are no limit's.
        assert_eq!(
            refused_for_a_bound(&replace(whole(1)), None, Stage::Rows, reason),
            vec![]
        );
    }

    #[test]
    fn an_exponent_of_another_length_than_its_base_is_a_refusal_about_lengths() {
        // A power given an exponent of as many values as the table has rows, after a drop that
        // left its base fewer, is refused for the lengths, as a window or another call is.
        let case = case_of(call(
            "LazyFrame.with_columns",
            vec![(
                "exprs",
                many(vec![chain_of(
                    column("a"),
                    vec![
                        call("Expr.drop_nulls", Vec::new()),
                        call(
                            "Expr.pow",
                            vec![("exponent", Value::Expression(chain(column("a"), &[])))],
                        ),
                    ],
                )]),
            )],
        ));
        assert!(is_lengths(
            "exponent shape: 3 in `pow` expression does not match that of the base: 2"
        ));
        assert_eq!(
            refused_for_a_bound(
                &case,
                None,
                Stage::Rows,
                "exponent shape: 3 in `pow` expression does not match that of the base: 2"
            ),
            vec![Limit::ADropChangesTheLengths]
        );
    }

    #[test]
    fn a_refusal_about_lengths_is_not_the_drops_where_the_case_has_none() {
        let case = case_of(call(
            "LazyFrame.with_columns",
            vec![("exprs", many(vec![chain(column("a"), &["Expr.cum_sum"])]))],
        ));
        assert!(!refused_for_lengths(&case).contains(&Limit::ADropChangesTheLengths));
    }
    /// A select of a null literal followed by the calls, the first of which is `links` named.
    fn select_of_a_null_literal(links: &[&str]) -> Case {
        let then: Vec<String> = links
            .iter()
            .map(|entry| format!(r#"{{"entry":"{entry}","arguments":{{}}}}"#))
            .collect();
        let text = format!(
            r#"{{"a":"Date","b":"Date","revision":1}}
{{"version":1,"steps":[{{"id":"s","call":{{"entry":"LazyFrame.select","arguments":{{"exprs":{{"many":[{{"expression":{{"start":{{"entry":"polars.lit","arguments":{{"value":{{"flag":null}}}}}},"then":[{}]}}}}]}}}}}}}}]}}"#,
            then.join(",")
        );
        Case::decode(text.as_bytes()).expect("a case")
    }

    #[test]
    fn a_call_after_a_drop_of_a_null_gives_the_tables_no_rows_to_contradict_the_plan() {
        // The drop leaves no rows, so the rows do not show that the cube root of a null is a
        // float where its plan says the dtype Null. Polars allows it, and the tables refuse it.
        assert_eq!(
            excused_by_kept("uncovered-a-drop-leaves-no-rows.case"),
            vec![Limit::ADropLeavesNoRows]
        );
        let case = select_of_a_null_literal(&["Expr.drop_nulls", "Expr.cbrt"]);
        assert_eq!(excused_by(&case), vec![Limit::ADropLeavesNoRows]);
    }

    #[test]
    fn a_call_before_a_drop_of_a_null_is_not_excused_by_the_drop() {
        // The drop follows the cube root, so it leaves no rows of what the root gave and the
        // call before it is no call after a drop: the limit is no cause of what Polars shows.
        let case = select_of_a_null_literal(&["Expr.cbrt", "Expr.drop_nulls", "Expr.cbrt"]);
        assert!(!excused_by(&case).contains(&Limit::ADropLeavesNoRows));
        let case = select_of_a_null_literal(&["Expr.cbrt", "Expr.drop_nulls"]);
        assert!(!excused_by(&case).contains(&Limit::ADropLeavesNoRows));
    }
    // ----- A gather beyond the rows

    const A_PLACE_OUT_OF_BOUNDS_SAID: &str = "gather indices are out of bounds";

    #[test]
    fn a_place_out_of_bounds_is_excused_where_each_gather_is_why_in_a_select_a_filter_and_a_pivot()
    {
        for name in [
            "uncovered-a-gather-beyond-the-rows.case",
            "uncovered-a-gather-beyond-the-rows--in-a-filter.case",
            "uncovered-a-gather-beyond-the-rows--in-a-pivot-aggregate.case",
        ] {
            assert_eq!(
                excused_by_kept(name),
                vec![Limit::AGatherBeyondTheRows],
                "{name}"
            );
        }
    }

    #[test]
    fn a_place_out_of_bounds_is_not_excused_where_nothing_gathers_nor_where_it_is_another_refusal()
    {
        let no_gather = select(vec![chain(column("a"), &["Expr.cum_sum"])]);
        assert_eq!(
            polars_refuses(
                &no_gather,
                None,
                Stage::Rows,
                A_PLACE_OUT_OF_BOUNDS_SAID,
                Vec::new()
            ),
            Vec::<Limit>::new()
        );
        let gather = select(vec![chain_of(
            column("g"),
            vec![call(
                "Expr.gather",
                vec![("indices", Value::Expression(chain(column("b"), &[])))],
            )],
        )]);
        for reason in [
            "dtype `null` has no time unit",
            "invalid series dtype: expected `UInt32`, got `str`",
        ] {
            assert_eq!(
                polars_refuses(&gather, None, Stage::Rows, reason, Vec::new()),
                Vec::<Limit>::new(),
                "{reason}"
            );
        }
        // A gather that is already told to give a null cannot be why, and is no control.
        let telling = select(vec![chain_of(
            column("g"),
            vec![call(
                "Expr.gather",
                vec![
                    ("indices", Value::Expression(chain(column("b"), &[]))),
                    ("null_on_oob", Value::Flag(Some(true))),
                ],
            )],
        )]);
        assert_eq!(
            polars_refuses(
                &telling,
                None,
                Stage::Rows,
                A_PLACE_OUT_OF_BOUNDS_SAID,
                Vec::new()
            ),
            Vec::<Limit>::new()
        );
    }
}
