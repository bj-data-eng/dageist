//! The English language file against the catalogue and against the
//! storyboard's wording.

use std::collections::BTreeSet;

use crate::catalog::{Applies, Dtypes, Family, Kind, Words, WordsError, english, entries};

const ENGLISH: &str = include_str!("../en.toml");

/// What the editor shows that is not Polars' own name and `changed` does not
/// list: for an entry its name without the owner, for a parameter its Python
/// name, for a choice its word.
fn unlisted_differences(words: &Words) -> Vec<String> {
    let listed: BTreeSet<&str> = words.changed().iter().map(String::as_str).collect();
    let mut differences = Vec::new();
    for entry in entries() {
        let id = entry.id();
        let polars_name = id.as_str().split_once('.').expect("an owner").1;
        if words.entry_name(id) != Some(polars_name) && !listed.contains(id.as_str()) {
            differences.push(id.as_str().to_owned());
        }
        for parameter in entry.parameters() {
            let key = format!("{}#{}", id.as_str(), parameter.name());
            if words.parameter_name(id, parameter.name()) != Some(parameter.name())
                && !listed.contains(key.as_str())
            {
                differences.push(key.clone());
            }
            if let Kind::Choice(choices) = parameter.kind() {
                for word in choices {
                    let key = format!("{key}#{word}");
                    if words.choice_word(id, parameter.name(), word) != Some(*word)
                        && !listed.contains(key.as_str())
                    {
                        differences.push(key);
                    }
                }
            }
        }
    }
    differences
}

#[test]
fn every_entry_parameter_and_choice_has_its_words() {
    let words = english();
    for entry in entries() {
        let id = entry.id();
        let shown = id.as_str();
        assert!(words.entry_name(id).is_some(), "{shown} has no name");
        assert!(words.entry_hint(id).is_some(), "{shown} has no hint");
        // Only the steps of a table's list have a head.
        assert_eq!(
            words.entry_head(id).is_some(),
            entry.applies() == Applies::Frame,
            "{shown}: a head belongs to a frame step and to nothing else"
        );
        for parameter in entry.parameters() {
            let name = parameter.name();
            assert!(
                words.parameter_name(id, name).is_some(),
                "{shown} {name} has no name"
            );
            assert!(
                words.parameter_label(id, name).is_some(),
                "{shown} {name} has no label"
            );
            if let Kind::Choice(choices) = parameter.kind() {
                for word in choices {
                    assert!(
                        words.choice_word(id, name, word).is_some(),
                        "{shown} {name} has no word for {word}"
                    );
                }
            }
        }
    }
}

#[test]
fn every_family_and_group_of_dtypes_has_its_words() {
    let words = english();
    for family in Family::ALL {
        assert!(
            words.family_title(family).is_some(),
            "{} has no title",
            family.key()
        );
    }
    for dtypes in Dtypes::ALL {
        assert!(
            words.dtypes_name(dtypes).is_some(),
            "{} has no name",
            dtypes.key()
        );
        assert!(
            words.dtypes_hint(dtypes).is_some(),
            "{} has no hint",
            dtypes.key()
        );
    }
}

#[test]
fn the_file_names_nothing_the_catalogue_lacks() {
    let words = english();
    let named = words.named();

    let ids: BTreeSet<&str> = entries().iter().map(|entry| entry.id().as_str()).collect();
    for id in &named.entries {
        assert!(
            ids.contains(id),
            "the file names an entry, {id}, the catalogue lacks"
        );
    }

    let parameters: BTreeSet<(&str, &str)> = entries()
        .iter()
        .flat_map(|entry| {
            entry
                .parameters()
                .iter()
                .map(move |parameter| (entry.id().as_str(), parameter.name()))
        })
        .collect();
    for pair in &named.parameters {
        assert!(
            parameters.contains(pair),
            "the file names a parameter the catalogue lacks: {pair:?}"
        );
    }

    let choices: BTreeSet<(&str, &str, &str)> = entries()
        .iter()
        .flat_map(|entry| {
            entry.parameters().iter().flat_map(move |parameter| {
                let words: &[&str] = match parameter.kind() {
                    Kind::Choice(words) => words,
                    _ => &[],
                };
                words
                    .iter()
                    .map(move |word| (entry.id().as_str(), parameter.name(), *word))
            })
        })
        .collect();
    for triple in &named.choices {
        assert!(
            choices.contains(triple),
            "the file names a choice the catalogue lacks: {triple:?}"
        );
    }

    let families: BTreeSet<&str> = Family::ALL.iter().map(|family| family.key()).collect();
    for key in &named.families {
        assert!(
            families.contains(key),
            "the file names a family the catalogue lacks: {key}"
        );
    }
    let dtypes: BTreeSet<&str> = Dtypes::ALL.iter().map(|group| group.key()).collect();
    for key in &named.dtypes {
        assert!(
            dtypes.contains(key),
            "the file names a dtype group the catalogue lacks: {key}"
        );
    }
}

#[test]
fn changed_names_only_what_the_catalogue_has() {
    let known: BTreeSet<String> = entries()
        .iter()
        .flat_map(|entry| {
            let id = entry.id().as_str().to_owned();
            let parameters: Vec<String> = entry
                .parameters()
                .iter()
                .flat_map(|parameter| {
                    let key = format!("{id}#{}", parameter.name());
                    let words: &[&str] = match parameter.kind() {
                        Kind::Choice(words) => words,
                        _ => &[],
                    };
                    let mut keys: Vec<String> =
                        words.iter().map(|word| format!("{key}#{word}")).collect();
                    keys.push(key);
                    keys
                })
                .collect();
            std::iter::once(id).chain(parameters)
        })
        .collect();
    for name in english().changed() {
        assert!(
            known.contains(name),
            "changed lists {name}, which the catalogue lacks"
        );
    }
}

#[test]
fn english_shows_polars_names_unless_the_file_lists_them_as_changed() {
    assert_eq!(unlisted_differences(english()), Vec::<String>::new());
    assert!(english().changed().is_empty());
}

#[test]
fn a_name_that_differs_from_polars_is_caught_unless_it_is_listed_as_changed() {
    let renamed = ENGLISH.replacen("name = \"shift\"", "name = \"move\"", 1);
    assert_ne!(renamed, ENGLISH, "the file names shift");
    let words = Words::parse(&renamed).expect("a language file");
    assert_eq!(unlisted_differences(&words), ["Expr.shift"]);

    let listed = renamed.replacen("changed = []", "changed = [\"Expr.shift\"]", 1);
    let words = Words::parse(&listed).expect("a language file");
    assert_eq!(unlisted_differences(&words), Vec::<String>::new());

    let parameter = ENGLISH.replacen("name = \"fill_value\"", "name = \"fill\"", 1);
    let words = Words::parse(&parameter).expect("a language file");
    assert_eq!(unlisted_differences(&words), ["Expr.shift#fill_value"]);

    let choice = ENGLISH.replacen("join = \"join\"", "join = \"collect\"", 1);
    let words = Words::parse(&choice).expect("a language file");
    assert_eq!(
        unlisted_differences(&words),
        ["Expr.over#mapping_strategy#join"]
    );
}

#[test]
fn english_words_are_those_of_the_storyboard() {
    let words = english();
    let id = |text: &str| crate::catalog::entry(text).expect("an entry").id();
    // ui/packages/steps/src/model/expr.ts, METHOD_HINTS.
    assert_eq!(
        words.entry_hint(id("Expr.shift")),
        Some("value n rows earlier")
    );
    assert_eq!(
        words.entry_hint(id("Expr.dt.total_days")),
        Some("whole days in a duration")
    );
    assert_eq!(
        words.entry_hint(id("Expr.is_not_null")),
        Some("has a value")
    );
    // ui/packages/steps/src/verbs.ts, VERB_LABELS and VERB_NOTES.
    assert_eq!(
        words.entry_head(id("LazyFrame.with_columns")),
        Some("Add column")
    );
    assert_eq!(words.entry_head(id("LazyFrame.join")), Some("Join table"));
    assert_eq!(
        words.entry_hint(id("LazyFrame.pivot")),
        Some("the values of a field as columns")
    );
    // expr.ts, METHOD_GROUP_TITLES: the storyboard's dt group is the Temporal family.
    assert_eq!(
        words.family_title(Family::Temporal),
        Some("dt · dates and times")
    );
    assert_eq!(
        words.family_title(Family::Compare),
        Some("Comparison · true or false")
    );
    // Polars' own names.
    assert_eq!(
        words.entry_name(id("Expr.dt.offset_by")),
        Some("dt.offset_by")
    );
    assert_eq!(words.entry_name(id("polars.col")), Some("col"));
    assert_eq!(
        words.entry_name(id("LazyFrame.with_columns")),
        Some("with_columns")
    );
}

#[test]
fn a_word_the_file_lacks_is_none() {
    let words = english();
    let shift = crate::catalog::entry("Expr.shift").expect("an entry").id();
    assert_eq!(words.parameter_name(shift, "nope"), None);
    assert_eq!(words.choice_word(shift, "n", "word"), None);
    assert_eq!(words.entry_head(shift), None);
}

#[test]
fn a_language_file_with_a_fault_is_refused_with_where_it_is() {
    assert!(matches!(
        Words::parse("changed = ["),
        Err(WordsError::Syntax(_))
    ));

    let no_name = "[entries.\"Expr.shift\"]\nhint = \"h\"\n";
    assert_eq!(
        Words::parse(no_name).unwrap_err(),
        WordsError::Missing {
            path: "entries.\"Expr.shift\".name".to_owned()
        }
    );

    let wrong_type = "[families.rows]\ntitle = 3\n";
    assert_eq!(
        Words::parse(wrong_type).unwrap_err(),
        WordsError::WrongType {
            path: "families.rows.title".to_owned(),
            expected: "text"
        }
    );

    let unknown = "[entries.\"Expr.shift\"]\nname = \"n\"\nhint = \"h\"\nshout = \"x\"\n";
    assert_eq!(
        Words::parse(unknown).unwrap_err(),
        WordsError::Unknown {
            path: "entries.\"Expr.shift\".shout".to_owned()
        }
    );

    let in_parameter = "[entries.\"Expr.shift\"]\nname = \"n\"\nhint = \"h\"\n\
                        [entries.\"Expr.shift\".parameters.n]\nname = \"n\"\n";
    assert_eq!(
        Words::parse(in_parameter).unwrap_err(),
        WordsError::Missing {
            path: "entries.\"Expr.shift\".parameters.n.label".to_owned()
        }
    );

    assert_eq!(
        Words::parse("mystery = 1").unwrap_err(),
        WordsError::Unknown {
            path: "mystery".to_owned()
        }
    );
}
