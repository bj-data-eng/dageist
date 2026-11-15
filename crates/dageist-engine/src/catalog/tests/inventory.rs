//! The catalogue against `api/polars/python.txt`, the committed inventory of
//! Python Polars' public API, which is the authority for item names, their
//! parameters and their defaults.

use std::collections::BTreeSet;

use crate::catalog::{Default, Kind, Parameter, Reason, Shape, entries, entry, left_out};

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
        (Default::Required, _, _) => name.to_owned(),
        (Default::Python("()"), Shape::Many, _) => format!("*{name}"),
        (Default::Python("{}"), Shape::One, Kind::Pairs(..)) => format!("**{name}"),
        (Default::Python(text), _, _) => {
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
                (Kind::Choice(words), Default::Python(text)) if text != "None" => {
                    let word = text.trim_matches('\'');
                    assert!(
                        text.starts_with('\'') && text.ends_with('\'') && words.contains(&word),
                        "{context}: {text} is not one of {words:?}"
                    );
                }
                (Kind::Flag, Default::Python(text)) => {
                    assert!(
                        ["True", "False", "None"].contains(&text),
                        "{context}: {text}"
                    );
                }
                (Kind::Choice(words), _) => {
                    let unique: BTreeSet<_> = words.iter().collect();
                    assert_eq!(unique.len(), words.len(), "{context}: repeated words");
                }
                _ => {}
            }
        }
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
