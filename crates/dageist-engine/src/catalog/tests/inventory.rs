//! The catalogue against `api/polars/python.txt`, the committed inventory of
//! Python Polars' public API, which is the authority for item names, their
//! parameters and their defaults.

use std::collections::BTreeSet;

use crate::catalog::{
    Applies, Family, Kind, Parameter, ParameterDefault, Reason, Shape, entries, entry, left_out,
};

const INVENTORY: &str = include_str!("../../../../../api/polars/python.txt");

/// One line of the inventory: an item and its signature.
pub(super) struct Item {
    pub(super) id: String,
    /// The parameters as written, `*` and `/` markers left out.
    pub(super) parameters: Vec<String>,
    pub(super) deprecated: bool,
}

/// Splits `text` at the commas that are not inside brackets or quotes.
fn split_parameters(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let (mut depth, mut quote, mut start) = (0_i32, None::<char>, 0);
    for (at, character) in text.char_indices() {
        match (quote, character) {
            (Some(open), _) if character == open => quote = None,
            (Some(_), _) => {}
            (None, '\'' | '"') => quote = Some(character),
            (None, '(' | '[' | '{') => depth += 1,
            (None, ')' | ']' | '}') => depth -= 1,
            (None, ',') if depth == 0 => {
                parts.push(text[start..at].trim().to_owned());
                start = at + 1;
            }
            _ => {}
        }
    }
    let last = text[start..].trim();
    if !last.is_empty() {
        parts.push(last.to_owned());
    }
    parts
}

pub(super) fn items() -> Vec<Item> {
    INVENTORY
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let signature = line.split(" | ").next().expect("a line has a signature");
            match signature.find('(') {
                Some(open) => {
                    let close = signature.rfind(')').expect("a signature closes");
                    Item {
                        id: signature[..open].to_owned(),
                        parameters: split_parameters(&signature[open + 1..close])
                            .into_iter()
                            .filter(|parameter| parameter != "*" && parameter != "/")
                            .collect(),
                        deprecated: signature[close + 1..].trim() == "deprecated",
                    }
                }
                None => Item {
                    id: signature
                        .strip_suffix(" property")
                        .expect("an item without parentheses is a property")
                        .to_owned(),
                    parameters: Vec::new(),
                    deprecated: false,
                },
            }
        })
        .collect()
}

/// A parameter as Python's signature writes it: `name`, `name=default`,
/// `*name` or `**name`.
fn written(parameter: &Parameter) -> String {
    let name = parameter.name();
    match (parameter.default(), parameter.shape(), parameter.kind()) {
        (ParameterDefault::Required, _, _) => name.to_owned(),
        (ParameterDefault::Python("()"), Shape::Many, _) => format!("*{name}"),
        (ParameterDefault::Python("{}"), Shape::One, Kind::Pairs(..)) => format!("**{name}"),
        (ParameterDefault::Python(text), _, _) => {
            assert!(
                text != "()" && text != "{}",
                "{name} has the default of a starred parameter without being one"
            );
            format!("{name}={text}")
        }
    }
}

#[test]
fn every_entry_has_the_parameters_and_defaults_of_its_python_signature() {
    let items = items();
    for entry in entries() {
        let item = items
            .iter()
            .find(|item| item.id == entry.id().as_str())
            .unwrap_or_else(|| panic!("{} is not in the inventory", entry.id().as_str()));
        let written: Vec<String> = entry.parameters().iter().map(written).collect();
        assert_eq!(
            written,
            item.parameters,
            "parameters of {}",
            entry.id().as_str()
        );
    }
}

#[test]
fn a_choice_default_is_one_of_its_words_and_a_flag_default_is_a_python_bool() {
    for entry in entries() {
        for parameter in entry.parameters() {
            let context = format!("{} {}", entry.id().as_str(), parameter.name());
            match (parameter.kind(), parameter.default()) {
                (Kind::Choice(words), ParameterDefault::Python(text)) if text != "None" => {
                    let word = text.trim_matches('\'');
                    assert!(
                        text.starts_with('\'') && text.ends_with('\'') && words.contains(&word),
                        "{context}: {text} is not one of {words:?}"
                    );
                }
                (Kind::Flag, ParameterDefault::Python(text)) => {
                    assert!(
                        ["True", "False", "None"].contains(&text),
                        "{context}: {text}"
                    );
                }
                _ => {}
            }
        }
    }
}

/// Every `Kind::Choice` held by `kind`, including those inside pairs.
fn choices(kind: Kind) -> Vec<&'static [&'static str]> {
    match kind {
        Kind::Choice(words) => vec![words],
        Kind::Pairs(key, value) => choices(*key).into_iter().chain(choices(*value)).collect(),
        _ => Vec::new(),
    }
}

#[test]
fn a_choice_has_words_and_none_is_empty_or_repeated() {
    for entry in entries() {
        for parameter in entry.parameters() {
            for words in choices(parameter.kind()) {
                let context = format!("{} {}", entry.id().as_str(), parameter.name());
                assert!(!words.is_empty(), "{context}: a choice without words");
                assert!(
                    words.iter().all(|word| !word.is_empty()),
                    "{context}: an empty word"
                );
                let unique: BTreeSet<_> = words.iter().collect();
                assert_eq!(unique.len(), words.len(), "{context}: repeated words");
            }
        }
    }
}

#[test]
fn only_expression_methods_accept_dtypes_and_each_of_them_does() {
    for entry in entries() {
        assert_eq!(
            !entry.accepts().is_empty(),
            entry.applies() == Applies::Expression,
            "{}: the dtypes an entry accepts belong to an expression method and to nothing else",
            entry.id().as_str()
        );
    }
}

#[test]
fn a_family_holds_the_entries_that_apply_to_what_it_is_for() {
    for entry in entries() {
        let id = entry.id().as_str();
        assert_eq!(
            entry.family() == Family::Steps,
            entry.applies() == Applies::Frame,
            "{id}: the Steps family is the frame entries"
        );
        assert_eq!(
            entry.family() == Family::Start,
            entry.applies() == Applies::Function,
            "{id}: the Start family is the functions"
        );
        assert_eq!(
            entry.family() == Family::Condition,
            entry.applies() == Applies::Condition,
            "{id}: the Condition family is the conditions"
        );
    }
}

#[test]
fn a_per_item_parameter_names_a_parameter_of_its_entry() {
    for entry in entries() {
        for parameter in entry.parameters() {
            if let Shape::PerItem(of) = parameter.shape() {
                assert!(
                    entry.parameters().iter().any(|other| other.name() == of),
                    "{} {} is per item of {of}, which it does not have",
                    entry.id().as_str(),
                    parameter.name()
                );
            }
        }
    }
}

#[test]
fn every_inventory_item_is_exactly_one_of_an_entry_or_left_out() {
    let inventory: BTreeSet<String> = items().into_iter().map(|item| item.id).collect();
    let entry_ids: BTreeSet<&str> = entries().iter().map(|entry| entry.id().as_str()).collect();
    let left_out_ids: BTreeSet<&str> = left_out().iter().map(|item| item.id()).collect();

    let named_twice: Vec<_> = entry_ids.intersection(&left_out_ids).collect();
    assert!(
        named_twice.is_empty(),
        "entries that are also left out: {named_twice:?}"
    );

    let covered: BTreeSet<String> = entry_ids
        .union(&left_out_ids)
        .map(|id| (*id).to_owned())
        .collect();
    let missing: Vec<_> = inventory.difference(&covered).collect();
    let unknown: Vec<_> = covered.difference(&inventory).collect();
    assert!(
        missing.is_empty(),
        "inventory items that are neither: {missing:?}"
    );
    assert!(unknown.is_empty(), "names the inventory lacks: {unknown:?}");
}

#[test]
fn entries_and_the_left_out_list_are_sorted_without_repeats() {
    let entry_ids: Vec<&str> = entries().iter().map(|entry| entry.id().as_str()).collect();
    assert!(
        entry_ids.windows(2).all(|pair| pair[0] < pair[1]),
        "entries"
    );
    let left_out_ids: Vec<&str> = left_out().iter().map(|item| item.id()).collect();
    assert!(
        left_out_ids.windows(2).all(|pair| pair[0] < pair[1]),
        "left out"
    );
}

#[test]
fn an_item_is_left_out_as_deprecated_exactly_when_the_inventory_says_so() {
    let deprecated: BTreeSet<String> = items()
        .into_iter()
        .filter(|item| item.deprecated)
        .map(|item| item.id)
        .collect();
    let left_out_as_deprecated: BTreeSet<String> = left_out()
        .iter()
        .filter(|item| item.reason() == Reason::Deprecated)
        .map(|item| item.id().to_owned())
        .collect();
    assert_eq!(left_out_as_deprecated, deprecated);
}

#[test]
fn an_entry_is_found_by_its_identifier_and_nothing_else_is() {
    for each in entries() {
        let found = entry(each.id().as_str()).expect("an entry is found");
        assert_eq!(found.id(), each.id());
    }
    assert!(entry("Expr.not_a_method").is_none());
    assert!(entry("").is_none());
}
