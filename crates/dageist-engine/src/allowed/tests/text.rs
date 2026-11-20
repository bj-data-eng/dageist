//! What the tables and the offers say of the methods of text, against what Polars does when its
//! own API is called straight, which is the oracle of each test.

use polars::prelude::{DataType, JoinType, col, lit};

use crate::allowed::dtype::Representative;
use crate::allowed::offers::RefusalSource;
use crate::allowed::{After, Chosen, Context, Outer, Place, Shape, Stage, Verdict, offer, options};
use crate::catalog;

use super::oracle::{Seen, selected};

fn entry(id: &str) -> &'static catalog::Entry {
    catalog::entry(id).unwrap_or_else(|| panic!("no entry {id}"))
}

fn at(context: Context, dtype: Representative, shape: Shape) -> Place {
    Place::new(context, After::Value { dtype, shape })
}

fn select_after(dtype: Representative) -> Place {
    at(Context::step(Outer::Select), dtype, Shape::Many)
}

fn result_of(verdict: &Verdict) -> Option<&str> {
    verdict
        .allowance()
        .and_then(|allowance| allowance.result())
        .map(|result| result.name())
}

#[test]
fn a_method_of_text_is_offered_after_text_and_refused_after_what_is_not() {
    for id in [
        "Expr.str.to_lowercase",
        "Expr.str.to_uppercase",
        "Expr.str.to_titlecase",
        "Expr.str.reverse",
        "Expr.str.strip_chars",
    ] {
        let after_text = offer(
            entry(id),
            &select_after(Representative::String),
            &Chosen::default(),
        );
        assert_eq!(
            result_of(&after_text),
            Some("String"),
            "{id}: {after_text:?}"
        );
        for receiver in [
            Representative::Int64,
            Representative::Boolean,
            Representative::Date,
            Representative::Null,
            Representative::ListOfString,
        ] {
            let refused = offer(entry(id), &select_after(receiver), &Chosen::default());
            let Some(refusal) = refused.refusal() else {
                panic!("{id} is refused after {receiver}");
            };
            // Polars resolves the schema and refuses the rows, as the oracle shows.
            assert_eq!(refusal.source(), RefusalSource::Polars(Stage::Rows), "{id}");
            let plan = match id {
                "Expr.str.to_lowercase" => col("a").str().to_lowercase(),
                "Expr.str.to_uppercase" => col("a").str().to_uppercase(),
                "Expr.str.to_titlecase" => col("a").str().to_titlecase(),
                "Expr.str.reverse" => col("a").str().reverse(),
                _ => col("a").str().strip_chars(lit(polars::prelude::NULL)),
            };
            assert_eq!(
                selected(receiver, Representative::Int64, plan),
                Seen::Refused(Stage::Rows),
                "{id} after {receiver}"
            );
        }
    }
}

#[test]
fn a_length_is_an_unsigned_whole_number_and_a_test_is_a_boolean() {
    let place = select_after(Representative::String);
    for id in ["Expr.str.len_chars", "Expr.str.len_bytes"] {
        let verdict = offer(entry(id), &place, &Chosen::default());
        assert_eq!(result_of(&verdict), Some("UInt32"), "{id}");
    }
    assert_eq!(
        selected(
            Representative::String,
            Representative::Int64,
            col("a").str().len_chars()
        ),
        Seen::Ran(polars::prelude::DataType::UInt32)
    );
    let starts = offer(
        entry("Expr.str.starts_with"),
        &place,
        &Chosen::default().with_arguments(&[("prefix", Representative::String)]),
    );
    assert_eq!(result_of(&starts), Some("Boolean"), "{starts:?}");
    // Where a test is wanted, a start or an end may be one: a filter takes a Boolean.
    let in_a_filter = offer(
        entry("Expr.str.ends_with"),
        &at(
            Context::step(Outer::Filter),
            Representative::String,
            Shape::Many,
        ),
        &Chosen::default().with_arguments(&[("suffix", Representative::String)]),
    );
    assert!(in_a_filter.is_allowed(), "{in_a_filter:?}");
    assert!(
        in_a_filter
            .allowance()
            .is_some_and(|allowance| allowance.can_end().is_allowed()),
        "{in_a_filter:?}"
    );
}

#[test]
fn what_is_trimmed_or_tested_for_must_be_text_or_something_polars_casts_to_it() {
    let place = select_after(Representative::String);
    // Trimming and splitting take text, or a null that is no characters.
    for (id, parameter) in [
        ("Expr.str.strip_chars", "characters"),
        ("Expr.str.strip_prefix", "prefix"),
        ("Expr.str.strip_suffix", "suffix"),
        ("Expr.str.split", "by"),
    ] {
        let text = offer(
            entry(id),
            &place,
            &Chosen::default().with_arguments(&[(parameter, Representative::String)]),
        );
        assert!(text.is_allowed(), "{id}: {text:?}");
        let number = offer(
            entry(id),
            &place,
            &Chosen::default().with_arguments(&[(parameter, Representative::Int64)]),
        );
        assert!(number.refusal().is_some(), "{id} is refused a number");
    }
    assert_eq!(
        selected(
            Representative::String,
            Representative::Int64,
            col("a").str().strip_chars(col("b"))
        ),
        Seen::Refused(Stage::Rows)
    );
    // A test for a start or an end is the exception: Polars casts what it is given to text, and
    // refuses only a dtype it cannot cast.
    for argument in [
        Representative::Int64,
        Representative::Date,
        Representative::Boolean,
    ] {
        let tested = offer(
            entry("Expr.str.starts_with"),
            &place,
            &Chosen::default().with_arguments(&[("prefix", argument)]),
        );
        assert!(tested.is_allowed(), "{argument}: {tested:?}");
        assert!(matches!(
            selected(
                Representative::String,
                argument,
                col("a").str().starts_with(col("b"))
            ),
            Seen::Ran(_)
        ));
    }
    let list = offer(
        entry("Expr.str.starts_with"),
        &place,
        &Chosen::default().with_arguments(&[("prefix", Representative::ListOfInteger)]),
    );
    assert!(list.refusal().is_some(), "{list:?}");
}

#[test]
fn a_count_of_characters_is_cast_to_a_whole_number_so_a_float_is_taken_and_text_is_not() {
    let place = select_after(Representative::String);
    for (id, parameter) in [
        ("Expr.str.head", "n"),
        ("Expr.str.tail", "n"),
        ("Expr.str.slice", "offset"),
        ("Expr.str.pad_start", "length"),
        ("Expr.str.pad_end", "length"),
        ("Expr.str.zfill", "length"),
    ] {
        for (argument, allowed) in [
            (Representative::Int64, true),
            (Representative::UInt8, true),
            (Representative::Float64, true),
            (Representative::Boolean, true),
            // Polars reads the offset of a slice from text, where it converts the other counts.
            (Representative::String, id == "Expr.str.slice"),
            (Representative::ListOfInteger, false),
        ] {
            let verdict = offer(
                entry(id),
                &place,
                &Chosen::default().with_arguments(&[(parameter, argument)]),
            );
            assert_eq!(
                verdict.is_allowed(),
                allowed,
                "{id} given {argument}: {verdict:?}"
            );
        }
    }
    assert!(matches!(
        selected(
            Representative::String,
            Representative::String,
            col("a").str().slice(col("b"), lit(polars::prelude::NULL))
        ),
        Seen::Ran(_)
    ));
    // Polars, called straight, takes a column of floats as the number of characters and refuses
    // one of text, whose rows are not numbers.
    assert!(matches!(
        selected(
            Representative::String,
            Representative::Float64,
            col("a").str().head(col("b"))
        ),
        Seen::Ran(_)
    ));
    assert_eq!(
        selected(
            Representative::String,
            Representative::String,
            col("a").str().head(col("b"))
        ),
        Seen::Refused(Stage::Rows)
    );
}

#[test]
fn a_split_gives_many_lists_of_text_and_a_join_gives_one_text() {
    let place = select_after(Representative::String);
    let split = offer(
        entry("Expr.str.split"),
        &place,
        &Chosen::default().with_arguments(&[("by", Representative::String)]),
    );
    assert_eq!(result_of(&split), Some("List(String)"), "{split:?}");
    assert_eq!(
        split.allowance().map(crate::allowed::Allowance::shape),
        Some(Shape::Many)
    );
    let join = offer(entry("Expr.str.join"), &place, &Chosen::default());
    assert_eq!(result_of(&join), Some("String"), "{join:?}");
    // One value for the rows: an aggregate, which Polars may hold beside a column in a select and
    // not in a join's own key.
    assert_eq!(
        join.allowance().map(crate::allowed::Allowance::shape),
        Some(Shape::One)
    );
}

#[test]
fn a_join_casts_what_it_joins_to_text_and_refuses_what_cannot_be_cast() {
    for (receiver, allowed) in [
        (Representative::Int64, true),
        (Representative::Float64, true),
        (Representative::Boolean, true),
        (Representative::Date, true),
        (Representative::String, true),
        (Representative::Duration, false),
        (Representative::ListOfInteger, false),
        (Representative::Array, false),
    ] {
        let verdict = offer(
            entry("Expr.str.join"),
            &select_after(receiver),
            &Chosen::default(),
        );
        assert_eq!(verdict.is_allowed(), allowed, "{receiver}: {verdict:?}");
        let seen = selected(
            receiver,
            Representative::Int64,
            col("a").str().join("-", true),
        );
        assert_eq!(
            matches!(seen, Seen::Ran(_)),
            allowed,
            "{receiver}: {seen:?}"
        );
    }
}

#[test]
fn a_split_by_a_pattern_is_refused_with_the_reason_and_a_split_by_text_is_not() {
    assert!(options(entry("Expr.str.split"), &[("literal", "true")]).is_ok());
    let refused = options(entry("Expr.str.split"), &[("literal", "false")])
        .expect_err("a split by a pattern is not offered");
    assert!(refused.reason().contains("pattern"), "{refused}");
    // The other flags are free beside a literal split.
    assert!(
        options(
            entry("Expr.str.split"),
            &[("inclusive", "true"), ("literal", "true")]
        )
        .is_ok()
    );
}

#[test]
fn text_a_join_makes_may_stand_in_a_select_beside_a_column_and_not_as_a_joins_own_key() {
    let key = offer(
        entry("Expr.str.join"),
        &at(
            Context::step(Outer::JoinLeft),
            Representative::String,
            Shape::Many,
        ),
        &Chosen::default(),
    );
    // A join key is elementwise, and a join of the values is an aggregate: the walk holds the
    // key to it; the entry itself may stand, as `sum` may.
    let sum = offer(
        entry("Expr.sum"),
        &at(
            Context::step(Outer::JoinLeft),
            Representative::Int64,
            Shape::Many,
        ),
        &Chosen::default(),
    );
    assert_eq!(key.is_allowed(), sum.is_allowed(), "{key:?} {sum:?}");
}

/// Whether Polars accepts a join on `left` and `right` as the keys of tables whose column `a`, and
/// `ra`, are text, when it works out the schema and runs the rows.
fn joins_on(left: polars::prelude::Expr, right: polars::prelude::Expr) -> bool {
    use super::sample::Frames;
    let frames = Frames::new(
        Representative::String,
        Representative::String,
        &DataType::Int64,
    );
    crate::startup::ensure();
    let Ok(mut joined) = frames
        .left(false)
        .join_builder()
        .with(frames.other(false))
        .left_on([left])
        .right_on([right])
        .how(JoinType::Inner)
        .finish()
    else {
        return false;
    };
    joined.collect_schema().is_ok() && joined.collect().is_ok()
}

#[test]
fn a_split_may_stand_in_a_join_key_as_polars_joins_on_a_list_of_text() {
    // Polars joins on lists, so a split of text is a key whose other side is a list of text too.
    let split = || col("a").str().split(lit(","));
    let other_split = || col("ra").str().split(lit(","));
    assert!(joins_on(split(), other_split()));
    for outer in [Outer::JoinLeft, Outer::JoinRight] {
        let verdict = offer(
            entry("Expr.str.split"),
            &at(Context::step(outer), Representative::String, Shape::Many),
            &Chosen::default().with_arguments(&[("by", Representative::String)]),
        );
        assert!(verdict.is_allowed(), "{outer:?}: {verdict:?}");
    }
    // A cast of text to a whole number is no way to meet it: the key that the other table has is
    // a list of the same text, and the list is not cast.
    assert!(!joins_on(
        split().cast(DataType::Int64),
        col("ra").cast(DataType::Int64)
    ));
}
