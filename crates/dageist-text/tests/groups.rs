//! A step that groups as text: `.group_by(…)` and the calls on the group by that follow it, as
//! Polars writes them, `.group_by(…).having(…).agg(…)`, written from a document and read back as
//! the same one, and an ill-formed chain refused as the document refuses it.
//!
//! The documents are written out as the engine reads them, by hand, not made by the reader; the
//! expected texts are written by hand from the form the crate documents.

use dageist_engine::document::{Cause, Document};
use dageist_text::{Problem, read, write};

/// A call of `entry` given `arguments`, as a document's JSON holds it.
fn call(entry: &str, arguments: &str) -> String {
    format!(r#"{{"entry":"{entry}","arguments":{{{arguments}}}}}"#)
}

/// An expression that starts from `start` and goes on with `then`.
fn chain(start: &str, then: &[String]) -> String {
    format!(
        r#"{{"expression":{{"start":{start},"then":[{}]}}}}"#,
        then.join(",")
    )
}

fn column(name: &str, then: &[String]) -> String {
    chain(
        &call("polars.col", &format!(r#""name":{{"columns":["{name}"]}}"#)),
        then,
    )
}

fn number(value: i64) -> String {
    chain(
        &call("polars.lit", &format!(r#""value":{{"integer":{value}}}"#)),
        &[],
    )
}

fn many(items: &[String]) -> String {
    format!(r#"{{"many":[{}]}}"#, items.join(","))
}

fn pairs(entries: &[(&str, String)]) -> String {
    let entries: Vec<String> = entries
        .iter()
        .map(|(name, value)| format!(r#"[{{"columns":["{name}"]}},{value}]"#))
        .collect();
    format!(r#"{{"pairs":[{}]}}"#, entries.join(","))
}

/// A step that groups: the call `group_by` is given `arguments`, then the calls that follow it.
fn grouping(id: &str, arguments: &str, then: &[String]) -> String {
    format!(
        r#"{{"id":"{id}","call":{},"then":[{}]}}"#,
        call("LazyFrame.group_by", arguments),
        then.join(",")
    )
}

fn step(id: &str, entry: &str, arguments: &str) -> String {
    format!(r#"{{"id":"{id}","call":{}}}"#, call(entry, arguments))
}

fn document(steps: &[String]) -> Document {
    Document::read(&format!(r#"{{"version":1,"steps":[{}]}}"#, steps.join(",")))
        .expect("a document")
}

fn by_k() -> String {
    format!(r#""by":{}"#, many(&[column("k", &[])]))
}

/// The text a chain of calls on `lf` is written as: one call to a line within brackets, where
/// there is more than one, as the notation lays a chain out.
fn laid_out(calls: &[&str]) -> String {
    let lines: Vec<String> = calls.iter().map(|call| format!("    .{call}")).collect();
    format!("(\n    lf\n{}\n)\n", lines.join("\n"))
}

/// The same chain on one line, which reads as the same document.
fn on_one_line(calls: &[&str]) -> String {
    format!("lf.{}", calls.join("."))
}

/// Checks that `held` is written as the chain of `calls` and read back from it, laid out and on
/// one line, as the same document.
fn written_as(held: &Document, calls: &[&str]) {
    assert_eq!(write(held).expect("written"), laid_out(calls));
    assert_eq!(read(&laid_out(calls)).expect("read"), *held);
    assert_eq!(read(&on_one_line(calls)).expect("read"), *held);
}

/// The document of one step that groups by `k`, finished by `finishing`, written as the group by
/// and `call`.
fn round_trips(finishing: &str, call: &str) {
    let held = document(&[grouping("group_by", &by_k(), &[finishing.to_owned()])]);
    written_as(&held, &[r#"group_by(pl.col("k"))"#, call]);
}

#[test]
fn each_shortcut_that_finishes_a_group_by_is_written_as_the_call_polars_makes() {
    for (entry, arguments, text) in [
        ("LazyGroupBy.len", String::new(), r#"len()"#),
        (
            "LazyGroupBy.len",
            r#""name":{"string":"rows"}"#.to_owned(),
            r#"len("rows")"#,
        ),
        ("LazyGroupBy.sum", String::new(), r#"sum()"#),
        ("LazyGroupBy.mean", String::new(), r#"mean()"#),
        ("LazyGroupBy.median", String::new(), r#"median()"#),
        ("LazyGroupBy.min", String::new(), r#"min()"#),
        ("LazyGroupBy.max", String::new(), r#"max()"#),
        ("LazyGroupBy.n_unique", String::new(), r#"n_unique()"#),
        ("LazyGroupBy.all", String::new(), r#"all()"#),
        (
            "LazyGroupBy.first",
            r#""ignore_nulls":{"flag":true}"#.to_owned(),
            r#"first(ignore_nulls=True)"#,
        ),
        ("LazyGroupBy.last", String::new(), r#"last()"#),
        (
            "LazyGroupBy.quantile",
            r#""quantile":{"float":0.5},"interpolation":{"choice":"linear"}"#.to_owned(),
            r#"quantile(0.5, interpolation="linear")"#,
        ),
        (
            "LazyGroupBy.head",
            r#""n":{"integer":3}"#.to_owned(),
            r#"head(3)"#,
        ),
        (
            "LazyGroupBy.tail",
            r#""n":{"integer":2}"#.to_owned(),
            r#"tail(2)"#,
        ),
    ] {
        round_trips(&call(entry, &arguments), text);
    }
}

#[test]
fn a_having_and_an_agg_follow_the_group_by_as_polars_writes_them() {
    let having = call(
        "LazyGroupBy.having",
        &format!(
            r#""predicates":{}"#,
            many(&[column(
                "x",
                &[
                    call("Expr.sum", ""),
                    call("Expr.gt", &format!(r#""other":{}"#, number(1))),
                ]
            )])
        ),
    );
    let agg = call(
        "LazyGroupBy.agg",
        &format!(
            r#""named_aggs":{}"#,
            pairs(&[("total", column("x", &[call("Expr.sum", "")]))])
        ),
    );
    let held = document(&[grouping("group_by", &by_k(), &[having, agg])]);
    written_as(
        &held,
        &[
            r#"group_by(pl.col("k"))"#,
            r#"having(pl.col("x").sum().gt(1))"#,
            r#"agg(total=pl.col("x").sum())"#,
        ],
    );
}

#[test]
fn aggregates_are_given_by_position_and_by_name() {
    let by_position = call(
        "LazyGroupBy.agg",
        &format!(
            r#""aggs":{}"#,
            many(&[
                column("x", &[call("Expr.sum", "")]),
                column("y", &[call("Expr.max", "")])
            ])
        ),
    );
    round_trips(&by_position, r#"agg(pl.col("x").sum(), pl.col("y").max())"#);
    let named = call(
        "LazyGroupBy.agg",
        &format!(
            r#""named_aggs":{}"#,
            pairs(&[
                ("total", column("x", &[call("Expr.sum", "")])),
                ("biggest", column("y", &[call("Expr.max", "")])),
            ])
        ),
    );
    round_trips(
        &named,
        r#"agg(total=pl.col("x").sum(), biggest=pl.col("y").max())"#,
    );
    let both = call(
        "LazyGroupBy.agg",
        &format!(
            r#""aggs":{},"named_aggs":{}"#,
            many(&[column("x", &[call("Expr.sum", "")])]),
            pairs(&[("biggest", column("y", &[call("Expr.max", "")]))])
        ),
    );
    round_trips(
        &both,
        r#"agg(pl.col("x").sum(), biggest=pl.col("y").max())"#,
    );
}

#[test]
fn keys_are_given_by_position_by_name_and_with_the_order_kept() {
    let finishing = call("LazyGroupBy.len", "");
    for (arguments, text) in [
        (
            format!(r#""by":{}"#, many(&[column("k", &[]), column("j", &[])])),
            r#"group_by(pl.col("k"), pl.col("j"))"#,
        ),
        (
            format!(r#""named_by":{}"#, pairs(&[("key", column("k", &[]))])),
            r#"group_by(key=pl.col("k"))"#,
        ),
        (
            format!(
                r#""by":{},"named_by":{}"#,
                many(&[column("k", &[])]),
                pairs(&[("other", column("j", &[]))])
            ),
            r#"group_by(pl.col("k"), other=pl.col("j"))"#,
        ),
        (
            format!(
                r#""by":{},"maintain_order":{{"flag":true}}"#,
                many(&[column("k", &[])])
            ),
            r#"group_by(pl.col("k"), maintain_order=True)"#,
        ),
    ] {
        let held = document(&[grouping(
            "group_by",
            &arguments,
            std::slice::from_ref(&finishing),
        )]);
        written_as(&held, &[text, "len()"]);
    }
}

#[test]
fn a_step_that_groups_stands_among_the_steps_before_and_after_it() {
    let held = document(&[
        step(
            "filter",
            "LazyFrame.filter",
            &format!(
                r#""predicates":{}"#,
                many(&[column(
                    "x",
                    &[call("Expr.gt", &format!(r#""other":{}"#, number(0)))]
                )])
            ),
        ),
        grouping(
            "group_by",
            &by_k(),
            &[call(
                "LazyGroupBy.agg",
                &format!(
                    r#""aggs":{}"#,
                    many(&[column("x", &[call("Expr.sum", "")])])
                ),
            )],
        ),
        step(
            "sort",
            "LazyFrame.sort",
            &format!(r#""by":{}"#, many(&[column("x", &[])])),
        ),
    ]);
    let text = write(&held).expect("written");
    assert!(!text.contains("# step:"), "{text}");
    for part in [
        ".filter(pl.col(\"x\").gt(0))",
        ".group_by(pl.col(\"k\"))",
        ".agg(pl.col(\"x\").sum())",
        ".sort(pl.col(\"x\"))",
    ] {
        assert!(text.contains(part), "{part} in {text}");
    }
    // The step after the group by is a step of its own: `agg` gives a table again.
    let order: Vec<usize> = [".filter", ".group_by", ".agg", ".sort"]
        .iter()
        .map(|part| text.find(part).expect("written"))
        .collect();
    assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{text}");
    assert_eq!(read(&text).expect("read"), held);
    assert_eq!(
        held.steps()
            .iter()
            .map(|step| step.links().len())
            .collect::<Vec<_>>(),
        [0, 1, 0]
    );
}

#[test]
fn a_step_that_groups_keeps_its_name_through_a_comment() {
    let held = document(&[grouping("totals", &by_k(), &[call("LazyGroupBy.len", "")])]);
    let text = write(&held).expect("written");
    assert_eq!(
        text,
        "(\n    lf\n    # step: totals\n    .group_by(pl.col(\"k\"))\n    .len()\n)\n"
    );
    assert_eq!(read(&text).expect("read"), held);
}

/// Reads `text`, which is refused as the document refuses a step that groups, and checks the
/// cause, the line and the columns of the span the refusal is at.
fn refused(text: &str, at: (usize, usize, usize), cause: fn(&Cause) -> bool) {
    let error = read(text).expect_err("the text is refused");
    let span = error.span();
    assert_eq!(
        (
            span.start().line(),
            span.start().column(),
            span.end().column()
        ),
        at,
        "{text}: {error}"
    );
    assert!(
        matches!(error.problem(), Problem::Refused(refusal) if cause(refusal.cause())),
        "{text}: {:?}",
        error.problem()
    );
}

#[test]
fn a_group_by_that_nothing_finishes_is_refused_as_the_document_refuses_it() {
    let unfinished = |cause: &Cause| matches!(cause, Cause::UnfinishedGrouping { .. });
    refused(r#"lf.group_by(pl.col("k"))"#, (1, 4, 12), unfinished);
    // What follows a group by and is no call on it ends nothing: the grouping stays unfinished.
    refused(
        r#"lf.group_by(pl.col("k")).sort(pl.col("x"))"#,
        (1, 4, 12),
        unfinished,
    );
    // A `having` that is the last call leaves it unfinished, and is where the refusal stands.
    refused(
        r#"lf.group_by(pl.col("k")).having(pl.col("x").sum().gt(1))"#,
        (1, 26, 32),
        unfinished,
    );
}

#[test]
fn calls_on_a_group_by_out_of_order_are_refused_as_the_document_refuses_them() {
    refused(
        r#"lf.group_by(pl.col("k")).having(pl.col("x").sum().gt(1)).having(pl.col("x").sum().gt(2)).len()"#,
        (1, 58, 64),
        |cause| matches!(cause, Cause::OutOfOrder { .. }),
    );
}

#[test]
fn a_call_on_a_group_by_that_stands_on_the_table_is_refused_with_the_catalogues_reason() {
    refused(r#"lf.agg(pl.col("x").sum())"#, (1, 4, 7), |cause| {
        matches!(
            cause,
            Cause::WrongPlace {
                entry: "LazyGroupBy.agg",
                ..
            }
        )
    });
}
