//! The comparisons a test makes, against the English language file and the catalogue.

use crate::catalog::{Comparison, SummaryPhrase, Words, english, entry};

#[test]
fn each_comparison_has_the_sign_the_english_file_gives_it() {
    // `!=`, `>=` and `<=` are written as one character each: `≠`, `≥` and `≤`.
    let signs: Vec<&str> = Comparison::ALL
        .iter()
        .map(|comparison| english().sign(*comparison).expect("a sign"))
        .collect();
    assert_eq!(signs, ["==", "≠", ">", "≥", "<", "≤"]);
}

#[test]
fn a_language_file_with_no_summary_gives_no_sign() {
    let words = Words::parse("").expect("a language file");
    assert_eq!(words.sign(Comparison::Equal), None);
}

#[test]
fn a_sign_is_the_phrase_a_summary_writes_it_with() {
    for comparison in Comparison::ALL {
        assert_eq!(
            english().sign(comparison),
            Some(english().say(comparison.phrase()).as_str()),
            "{comparison:?}"
        );
    }
    assert_eq!(english().say(SummaryPhrase::Equal), "==");
}

#[test]
fn each_comparison_is_an_entry_that_takes_what_it_is_compared_with() {
    for comparison in Comparison::ALL {
        let found = entry(comparison.entry()).expect("an entry of the catalogue");
        assert!(
            found
                .parameters()
                .iter()
                .any(|parameter| parameter.name() == "other"),
            "{}",
            comparison.entry()
        );
        assert_eq!(Comparison::of_entry(comparison.entry()), Some(comparison));
    }
}

#[test]
fn an_entry_that_is_no_comparison_makes_none() {
    for id in [
        "Expr.shift",
        "Expr.is_null",
        "polars.col",
        "Expr.nothing",
        "",
    ] {
        assert_eq!(Comparison::of_entry(id), None, "{id}");
    }
}
