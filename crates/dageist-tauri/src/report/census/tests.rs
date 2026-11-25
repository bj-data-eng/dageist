//! The census holds every variant the host has. Its lists of the types the host serializes are
//! made by `every_variant!`, which does not compile while a variant lacks an arm, and these tests
//! hold what the lists say against what the host and the engine write elsewhere: the committed
//! snapshots of what the commands return, the hand-written lists of the vocabulary, and what the
//! engine answers where it is asked. None is derived from the census writer's own output, so a
//! variant a list lacks is one the other source still holds.

use std::collections::BTreeSet;

use serde_json::{Value, json};

use super::{census, typed_readings, typed_values};
use crate::snapshot::{self, committed};

/// Set to rewrite the snapshot instead of checking it.
const REFRESH: &str = "DAGEIST_REFRESH_CENSUS";

/// The command that refreshes the snapshot.
const REFRESH_COMMAND: &str = "cargo test --offline --locked -p dageist-tauri report::census::tests::the_snapshot_is_what_the_host_reports";

/// The list the census holds under `name`, which is words.
fn listed(census: &Value, name: &str) -> BTreeSet<String> {
    census[name]
        .as_array()
        .unwrap_or_else(|| panic!("{name} is a list"))
        .iter()
        .map(|word| {
            word.as_str()
                .unwrap_or_else(|| panic!("{name} holds words"))
                .to_owned()
        })
        .collect()
}

/// The words of a fixture's list `name`, which is words.
fn words_of(fixture: &Value, name: &str) -> BTreeSet<String> {
    listed(fixture, name)
}

/// The runs the engine reported, in the snapshots of what the host's commands return.
fn runs() -> Vec<Value> {
    [
        "step-report.json",
        "step-report-refused.json",
        "step-report-incomplete.json",
    ]
    .iter()
    .map(|file| committed(file))
    .collect()
}

/// The run the host built by hand to hold every variant that no real document gives together. It
/// holds each variant, in no order a document can run: its steps are no evidence of what follows
/// what.
fn variants() -> Value {
    committed("step-report-variants.json")
}

/// What the steps before `at` of `steps` say of how the step at `at` is run, in the census's words.
fn after_of(steps: &[Value], at: usize) -> &'static str {
    let Some(before) = at.checked_sub(1).map(|previous| &steps[previous]) else {
        return "nothing";
    };
    match before["outcome"]["status"].as_str().expect("a status") {
        "refused" => "refused",
        "incomplete" => "incomplete",
        "notRun" => "notRun",
        _ => {
            let failed = steps[..at]
                .iter()
                .any(|step| step["count"]["kind"] == "failed");
            if failed { "failed" } else { "nothing" }
        }
    }
}

/// The state a reported step is in, as the census writes one.
fn state_of(steps: &[Value], at: usize) -> Value {
    let step = &steps[at];
    json!({
        "outcome": step["outcome"]["status"],
        "cause": step["outcome"]["cause"]["kind"],
        "rows": step["count"]["kind"],
        "after": after_of(steps, at),
    })
}

#[test]
fn the_snapshot_is_what_the_host_reports() {
    snapshot::check("census.json", &census(), REFRESH, REFRESH_COMMAND);
}

#[test]
fn every_state_the_host_reported_of_a_step_is_in_the_census() {
    let held = census();
    let states = held["states"].as_array().expect("a list");
    let mut seen = 0;
    for run in runs() {
        let steps = run["steps"].as_array().expect("steps");
        for at in 0..steps.len() {
            let state = state_of(steps, at);
            assert!(states.contains(&state), "the census lacks {state}");
            seen += 1;
        }
    }
    assert!(seen > 0, "the snapshots hold no step");
}

#[test]
fn no_state_is_listed_twice() {
    let held = census();
    let states = held["states"].as_array().expect("a list");
    let distinct: BTreeSet<String> = states.iter().map(Value::to_string).collect();
    assert_eq!(distinct.len(), states.len());
}

#[test]
fn a_step_is_counted_or_fails_on_its_rows_only_when_it_is_accepted_after_steps_that_ran() {
    let held = census();
    for state in held["states"].as_array().expect("a list") {
        if state["rows"] != "notCounted" {
            assert_eq!(state["outcome"], "valid", "{state}");
            assert_eq!(state["after"], "nothing", "{state}");
        }
        // A step is not run after one that was refused or incomplete, and only then.
        let blocked =
            ["refused", "incomplete", "notRun"].contains(&state["after"].as_str().expect("a word"));
        assert_eq!(state["outcome"] == "notRun", blocked, "{state}");
    }
}

#[test]
fn every_refusal_and_outcome_the_host_hand_built_or_reported_is_in_the_census() {
    let held = census();
    let mut causes = BTreeSet::new();
    let mut outcomes = BTreeSet::new();
    let mut counts = BTreeSet::new();
    let mut sources = BTreeSet::new();
    for run in runs().into_iter().chain([variants()]) {
        for step in run["steps"].as_array().expect("steps") {
            outcomes.insert(
                step["outcome"]["status"]
                    .as_str()
                    .expect("a status")
                    .to_owned(),
            );
            counts.insert(step["count"]["kind"].as_str().expect("a kind").to_owned());
            if let Some(cause) = step["outcome"]["cause"]["kind"].as_str() {
                causes.insert(cause.to_owned());
            }
            if let Some(source) = step["outcome"]["cause"]["refusal"]["source"]["by"].as_str() {
                sources.insert(source.to_owned());
            }
            if let Some(source) = step["walk"]["refusal"]["refusal"]["source"]["by"].as_str() {
                sources.insert(source.to_owned());
            }
        }
    }
    assert_eq!(causes, listed(&held, "causes"));
    assert_eq!(outcomes, listed(&held, "outcomes"));
    assert_eq!(counts, listed(&held, "rowsStates"));
    // Polars, a rule, or the structure of an expression: the hand-built run holds a refusal of each.
    assert!(sources.is_subset(&listed(&held, "refusalSources")));
    assert_eq!(listed(&held, "refusalSources").len(), 3);
}

#[test]
fn the_words_of_a_closed_list_are_those_the_vocabulary_writes_from_the_engines_own_lists() {
    let held = census();
    let vocabulary = committed("vocabulary.json");
    for (name, in_vocabulary) in [
        ("stages", "stages"),
        ("conditionStates", "conditionStates"),
        ("valueShapes", "valueShapes"),
        ("valueTypes", "valueTypes"),
    ] {
        assert_eq!(
            listed(&held, name),
            words_of(&vocabulary, in_vocabulary),
            "{name}"
        );
    }
    let rules: BTreeSet<String> = held["rules"]
        .as_array()
        .expect("a list")
        .iter()
        .map(|rule| rule["name"].as_str().expect("a name").to_owned())
        .collect();
    assert_eq!(rules, words_of(&vocabulary, "rules"));
    for rule in held["rules"].as_array().expect("a list") {
        assert!(
            !rule["statement"].as_str().expect("words").is_empty(),
            "{rule}"
        );
    }
}

#[test]
fn the_dtype_classes_and_shapes_are_those_of_the_grid_of_places_the_engine_was_asked_about() {
    let held = census();
    let grid = committed("offers-grid.json");
    let mut classes = BTreeSet::new();
    let mut shapes = BTreeSet::new();
    let mut kinds = BTreeSet::new();
    let mut states = BTreeSet::new();
    for after in grid["afters"].as_array().expect("places") {
        kinds.insert(after["kind"].as_str().expect("a kind").to_owned());
        match after["kind"].as_str().expect("a kind") {
            "value" => {
                classes.insert(after["dtype"].as_str().expect("a class").to_owned());
                shapes.insert(after["shape"].as_str().expect("a shape").to_owned());
            }
            "condition" => {
                states.insert(after["state"].as_str().expect("a state").to_owned());
            }
            _ => {}
        }
    }
    assert_eq!(listed(&held, "dtypeClasses"), classes);
    assert_eq!(listed(&held, "valueShapes"), shapes);
    assert_eq!(listed(&held, "afterKinds"), kinds);
    assert_eq!(listed(&held, "conditionStates"), states);
}

#[test]
fn the_forms_of_parameter_are_those_of_the_committed_catalogue() {
    let held = census();
    let catalogue = committed("catalogue.json");
    let mut forms = BTreeSet::new();
    let mut kinds = BTreeSet::new();
    for entry in catalogue["entries"].as_array().expect("entries") {
        for parameter in entry["parameters"].as_array().expect("parameters") {
            if parameter["offered"] != true {
                continue;
            }
            forms.insert(
                json!({ "shape": parameter["shape"]["shape"], "kind": strip(&parameter["kind"]) })
                    .to_string(),
            );
            kinds.insert(
                parameter["kind"]["kind"]
                    .as_str()
                    .expect("a kind")
                    .to_owned(),
            );
        }
    }
    let held_forms: BTreeSet<String> = held["parameterForms"]
        .as_array()
        .expect("forms")
        .iter()
        .map(Value::to_string)
        .collect();
    assert_eq!(held_forms, forms);
    // Every kind the catalogue uses is listed, and the list has the twelve of the contract.
    assert!(kinds.is_subset(&listed(&held, "parameterKinds")));
    assert_eq!(listed(&held, "parameterKinds").len(), 12);
    assert_eq!(
        listed(&held, "optionKinds"),
        BTreeSet::from(["flag".to_owned(), "choice".to_owned()])
    );
}

/// A kind as the census writes it: its words, without the words of a choice or what a pattern and
/// a time zone say.
fn strip(kind: &Value) -> Value {
    let mut written = json!({ "kind": kind["kind"] });
    match kind["kind"].as_str().expect("a kind") {
        "value" => written["type"] = kind["type"].clone(),
        "pairs" => {
            written["key"] = strip(&kind["key"]);
            written["value"] = strip(&kind["value"]);
        }
        _ => {}
    }
    written
}

#[test]
fn the_kinds_of_input_and_origin_are_those_the_sample_node_reports() {
    let held = census();
    let node = committed("node-inputs.json");
    let mut kinds = BTreeSet::new();
    let mut origins = BTreeSet::new();
    for input in node["inputs"].as_array().expect("inputs") {
        kinds.insert(input["kind"].as_str().expect("a kind").to_owned());
        origins.insert(
            input["from"]["kind"]
                .as_str()
                .expect("an origin")
                .to_owned(),
        );
    }
    origins.insert(
        node["source"]["from"]["kind"]
            .as_str()
            .expect("an origin")
            .to_owned(),
    );
    assert!(kinds.is_subset(&listed(&held, "inputKinds")));
    assert!(origins.is_subset(&listed(&held, "origins")));
    assert_eq!(listed(&held, "inputKinds").len(), 3);
    assert_eq!(
        listed(&held, "cells"),
        BTreeSet::from(["null".to_owned(), "text".to_owned()])
    );
}

#[test]
fn every_start_is_a_field_an_input_a_typed_value_a_function_or_nothing_yet() {
    let held = census();
    let kinds: BTreeSet<&str> = held["starts"]
        .as_array()
        .expect("starts")
        .iter()
        .map(|start| start["kind"].as_str().expect("a kind"))
        .collect();
    assert_eq!(
        kinds,
        BTreeSet::from([
            "connectedList",
            "connectedValue",
            "field",
            "function",
            "hole",
            "typed",
        ])
    );
    // The functions are the entries of the catalogue that start a chain, `when` among them, and
    // not `col` and `lit`, which are the field and the typed value.
    let catalogue = committed("catalogue.json");
    let known: BTreeSet<&str> = catalogue["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .map(|entry| entry["id"].as_str().expect("an id"))
        .collect();
    let functions: Vec<&str> = held["starts"]
        .as_array()
        .expect("starts")
        .iter()
        .filter(|start| start["kind"] == "function")
        .map(|start| start["entry"].as_str().expect("an entry"))
        .collect();
    assert!(functions.contains(&"polars.when"));
    assert!(functions.contains(&"polars.element"));
    for entry in &functions {
        assert!(known.contains(entry), "{entry}");
        assert!(!["polars.col", "polars.lit"].contains(entry));
    }
    // Every entry the catalogue gives to functions starts a chain, except the two drawn as words.
    let catalogued: BTreeSet<&str> = catalogue["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .filter(|entry| entry["applies"] == "function")
        .map(|entry| entry["id"].as_str().expect("an id"))
        .filter(|id| !["polars.col", "polars.lit"].contains(id))
        .collect();
    assert!(catalogued.is_subset(&functions.iter().copied().collect()));
}

#[test]
fn a_typed_value_is_a_number_text_or_flag_and_the_host_reads_only_the_first_two_kinds() {
    let values = typed_values();
    let words: Vec<(&str, bool)> = values
        .iter()
        .map(|value| {
            (
                value["literal"].as_str().expect("a literal"),
                value["readable"].as_bool().expect("a flag"),
            )
        })
        .collect();
    assert_eq!(
        words,
        [
            ("integer", true),
            ("float", true),
            ("text", true),
            ("flag", false)
        ]
    );
    // The class Polars gives each, which the readings the host wrote of typed text hold as well.
    assert_eq!(values[0]["dtype"], "Int32");
    assert_eq!(values[1]["dtype"], "Float64");
    assert_eq!(values[2]["dtype"], "String");
    assert_eq!(values[3]["dtype"], "Boolean");
}

#[test]
fn typed_text_is_never_read_as_a_flag_or_a_moment_the_document_cannot_hold() {
    // What the host reads of the words that would be a Boolean or a moment is text, in every slot.
    for text in ["true", "True", "2026-03-10T10:00:00", "10:00:00"] {
        for reading in typed_readings_of(text) {
            assert!(
                ["integer", "float", "text"].contains(&reading.as_str()) || reading == "unread",
                "{text}: {reading}"
            );
        }
    }
}

/// How each slot reads `text`: the kind of literal, or `unread`.
fn typed_readings_of(text: &str) -> Vec<String> {
    use crate::offers::{TypedRequest, ValueTypeRequest, read_view};
    let slots = [
        TypedRequest::Expression,
        TypedRequest::Value {
            value_type: ValueTypeRequest::Any,
        },
        TypedRequest::Value {
            value_type: ValueTypeRequest::String,
        },
        TypedRequest::Pattern,
    ];
    slots
        .into_iter()
        .flat_map(|slot| {
            let readings = serde_json::to_value(read_view(text, slot, None, None).expect("read"))
                .expect("readings serialize");
            readings["readings"]
                .as_array()
                .expect("a list")
                .iter()
                .map(|reading| {
                    if reading["kind"] == "read" {
                        reading["literal"]["kind"]
                            .as_str()
                            .expect("a kind")
                            .to_owned()
                    } else {
                        "unread".to_owned()
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn every_reading_the_host_wrote_of_typed_text_is_among_the_census_readings() {
    let readings = typed_readings();
    let pairs: BTreeSet<(String, String)> = readings
        .iter()
        .filter(|reading| !reading["literal"].is_null())
        .map(|reading| {
            (
                reading["literal"].as_str().expect("a kind").to_owned(),
                reading["dtype"].as_str().unwrap_or("none").to_owned(),
            )
        })
        .collect();
    for file in [
        "typed-whole.json",
        "typed-expression.json",
        "typed-date.json",
        "typed-start-and-argument.json",
    ] {
        for reading in committed(file)["readings"].as_array().expect("readings") {
            if reading["kind"] != "read" {
                continue;
            }
            let pair = (
                reading["literal"]["kind"]
                    .as_str()
                    .expect("a kind")
                    .to_owned(),
                reading["dtype"].as_str().unwrap_or("none").to_owned(),
            );
            assert!(pairs.contains(&pair), "{file}: {pair:?}");
        }
    }
    // A text read as a date is the text of the day with the class Date, and a duration has no class.
    assert!(pairs.contains(&("text".to_owned(), "Date".to_owned())));
    assert!(pairs.contains(&("text".to_owned(), "none".to_owned())));
    // A reading that fails to read is listed with no literal, for each slot that can refuse a text.
    assert!(readings.iter().any(|reading| reading["literal"].is_null()));
}

#[test]
fn a_list_made_by_every_variant_holds_the_samples_of_each_arm_in_order() {
    #[derive(Debug, PartialEq)]
    enum Sample {
        Plain,
        Held(u8),
        Named { number: u8 },
    }
    let all = every_variant!(Sample;
        Plain => [Sample::Plain],
        Held => [Sample::Held(1), Sample::Held(2)],
        Named => [Sample::Named { number: 3 }],
    );

    assert_eq!(
        all,
        [
            Sample::Plain,
            Sample::Held(1),
            Sample::Held(2),
            Sample::Named { number: 3 }
        ]
    );
}

#[test]
fn no_list_of_words_names_a_variant_twice() {
    let held = census();
    let mut checked = 0;
    for (name, list) in held.as_object().expect("an object") {
        let Some(items) = list.as_array() else {
            continue;
        };
        let words: Vec<&str> = items.iter().filter_map(Value::as_str).collect();
        if words.len() != items.len() {
            continue;
        }
        let distinct: BTreeSet<&str> = words.iter().copied().collect();
        assert_eq!(distinct.len(), words.len(), "{name} names a word twice");
        checked += 1;
    }
    assert!(checked > 15, "{checked} lists of words");
}

#[test]
fn the_families_of_words_the_view_decodes_are_those_the_vocabulary_lists_by_hand() {
    let held = census();
    let vocabulary = committed("vocabulary.json");
    for (name, in_vocabulary) in [
        ("changes", "changes"),
        ("settingKinds", "settings"),
        ("problemKinds", "problemKinds"),
        ("thingKinds", "thingKinds"),
        ("wanted", "wanted"),
        ("valueKinds", "valueKinds"),
    ] {
        assert_eq!(
            listed(&held, name),
            words_of(&vocabulary, in_vocabulary),
            "{name}"
        );
    }
}

#[test]
fn every_resolution_default_and_answer_the_host_reported_is_in_the_census() {
    let held = census();
    let mut resolutions = BTreeSet::new();
    for run in runs().into_iter().chain([variants()]) {
        for step in run["steps"].as_array().expect("steps") {
            for expression in step["expressions"].as_array().expect("expressions") {
                resolutions.insert(
                    expression["start"]["kind"]
                        .as_str()
                        .expect("a kind")
                        .to_owned(),
                );
                for link in expression["links"].as_array().expect("links") {
                    resolutions.insert(link["kind"].as_str().expect("a kind").to_owned());
                }
            }
        }
    }
    assert!(
        resolutions.is_subset(&listed(&held, "resolutions")),
        "{resolutions:?}"
    );
    assert_eq!(listed(&held, "resolutions").len(), 5);

    let mut defaults = BTreeSet::new();
    for entry in committed("catalogue.json")["entries"]
        .as_array()
        .expect("entries")
    {
        for parameter in entry["parameters"].as_array().expect("parameters") {
            defaults.insert(
                parameter["default"]["kind"]
                    .as_str()
                    .expect("a kind")
                    .to_owned(),
            );
        }
    }
    assert!(
        defaults.is_subset(&listed(&held, "defaultKinds")),
        "{defaults:?}"
    );

    let answers: BTreeSet<String> = ["edit-edited.json", "edit-refused.json"]
        .iter()
        .map(|file| {
            committed(file)["status"]
                .as_str()
                .expect("a status")
                .to_owned()
        })
        .collect();
    assert_eq!(answers, listed(&held, "editAnswers"));
    let problem = committed("edit-refused.json")["problem"]["kind"]
        .as_str()
        .expect("a kind")
        .to_owned();
    assert!(listed(&held, "problemKinds").contains(&problem));
}

#[test]
fn the_words_written_by_hand_are_those_the_host_wrote_in_its_snapshots() {
    // `kind_word`, `value_type_word`, `shape_word`, `typed_word`, `literal_word`, `before_word`
    // and `slot_json` are exhaustive matches that write a word; the snapshots of the catalogue,
    // the readings of typed text and the reports hold the words the host serializes.
    let held = census();
    let catalogue = committed("catalogue.json");
    let mut kinds = BTreeSet::new();
    let mut shapes = BTreeSet::new();
    let mut types = BTreeSet::new();
    for entry in catalogue["entries"].as_array().expect("entries") {
        for parameter in entry["parameters"].as_array().expect("parameters") {
            kinds.insert(
                parameter["kind"]["kind"]
                    .as_str()
                    .expect("a kind")
                    .to_owned(),
            );
            shapes.insert(
                parameter["shape"]["shape"]
                    .as_str()
                    .expect("a shape")
                    .to_owned(),
            );
            if parameter["kind"]["kind"] == "value" {
                types.insert(
                    parameter["kind"]["type"]
                        .as_str()
                        .expect("a type")
                        .to_owned(),
                );
            }
        }
    }
    assert!(
        kinds.is_subset(&listed(&held, "parameterKinds")),
        "{kinds:?}"
    );
    assert!(
        shapes.is_subset(&listed(&held, "parameterShapes")),
        "{shapes:?}"
    );
    assert!(types.is_subset(&listed(&held, "valueTypes")), "{types:?}");
    assert_eq!(listed(&held, "parameterShapes").len(), 3);
    // The words of a literal and a slot are those of the readings the host wrote.
    let literals: BTreeSet<String> = [
        "typed-whole.json",
        "typed-expression.json",
        "typed-date.json",
    ]
    .iter()
    .flat_map(|file| {
        committed(file)["readings"]
            .as_array()
            .expect("readings")
            .iter()
            .filter(|reading| reading["kind"] == "read")
            .map(|reading| {
                reading["literal"]["kind"]
                    .as_str()
                    .expect("a kind")
                    .to_owned()
            })
            .collect::<Vec<_>>()
    })
    .collect();
    let census_literals: BTreeSet<String> = typed_values()
        .iter()
        .filter(|value| value["readable"] == true)
        .map(|value| value["literal"].as_str().expect("a word").to_owned())
        .collect();
    assert!(literals.is_subset(&census_literals), "{literals:?}");
}
