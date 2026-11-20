//! What the catalogue says of the parameters that take a pattern, and that the entries that
//! hold them say it of the right ones.

use crate::catalog::{Kind, ParameterDefault, entries, entry};

#[test]
fn a_flag_that_makes_a_pattern_plain_text_is_a_flag_of_the_same_entry_with_a_default() {
    let mut seen = 0;
    for entry in entries() {
        for parameter in entry.parameters() {
            let Some(flag) = parameter.plain_text_flag() else {
                continue;
            };
            // The separator of a split is an expression, which text typed in makes a pattern.
            let id = entry.id().as_str();
            assert!(
                parameter.kind() == Kind::Pattern
                    || (id == "Expr.str.split" && matches!(parameter.kind(), Kind::Expression(_))),
                "{id}"
            );
            let flagged = entry
                .parameters()
                .iter()
                .find(|other| other.name() == flag)
                .unwrap_or_else(|| panic!("{} has no flag {flag}", entry.id().as_str()));
            assert_eq!(flagged.kind(), Kind::Flag, "{}", entry.id().as_str());
            assert!(
                matches!(flagged.default(), ParameterDefault::Python(_)),
                "{}",
                entry.id().as_str()
            );
            seen += 1;
        }
    }
    // `contains`, `count_matches`, `find`, `replace`, `replace_all` and `split`, which take
    // `literal`.
    assert_eq!(seen, 6);
}

#[test]
fn the_methods_that_take_literal_are_those_whose_pattern_is_plain_text_by_it() {
    // The signatures of `api/polars/python.txt`: each method that takes `literal` has a pattern
    // that the flag makes plain text, and no other method has a flag for it.
    for id in [
        "Expr.str.contains",
        "Expr.str.count_matches",
        "Expr.str.find",
        "Expr.str.replace",
        "Expr.str.replace_all",
    ] {
        let pattern = entry(id)
            .expect("an entry")
            .parameters()
            .iter()
            .find(|parameter| parameter.kind() == Kind::Pattern)
            .expect("a pattern");
        assert_eq!(pattern.plain_text_flag(), Some("literal"), "{id}");
    }
    for id in [
        "Expr.str.extract",
        "Expr.str.extract_all",
        "Expr.str.extract_groups",
    ] {
        let pattern = entry(id)
            .expect("an entry")
            .parameters()
            .iter()
            .find(|parameter| parameter.kind() == Kind::Pattern)
            .expect("a pattern");
        assert_eq!(pattern.plain_text_flag(), None, "{id}");
    }
}

#[test]
fn only_the_pattern_of_extract_groups_names_the_fields_of_what_a_method_gives() {
    let naming: Vec<&str> = entries()
        .iter()
        .filter(|entry| {
            entry
                .parameters()
                .iter()
                .any(|parameter| parameter.names_the_fields_of_the_result())
        })
        .map(|entry| entry.id().as_str())
        .collect();
    assert_eq!(naming, ["Expr.str.extract_groups"]);
}
