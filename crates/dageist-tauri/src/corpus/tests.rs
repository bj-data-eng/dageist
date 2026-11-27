//! The corpus: what it holds, held against the engine's own tables and the catalogue, and the
//! snapshot of what it is.
//!
//! Every expectation is written from what the catalogue and the tables say, not from what the
//! generator made: the generator plans from the same tables, so a test that read its plan back
//! would hold nothing. There is no earlier failing run of the corpus writer to show: the writer is
//! new, so these tests establish what it holds and do not repeat a regression.

use std::collections::{BTreeSet, HashSet};
use std::sync::OnceLock;

use dageist_engine::allowed::{After, Context, Representative, Rule, Shape, Stage, Unknown};
use dageist_engine::catalog::{self, Applies};
use dageist_engine::document::{Call, Document, Start, Value};
use serde_json::Value as Json;

use super::facts::{Fact, Follows, Verdict, facts};
use super::grid::{Cell, grid};
use super::source::{LIST, VALUE};
use super::{Part, Reported, corpus};
use crate::snapshot::{self, committed};

/// Set to rewrite the snapshot instead of checking it.
const REFRESH: &str = "DAGEIST_REFRESH_CORPUS";

/// The command that refreshes the snapshot.
const REFRESH_COMMAND: &str = "cargo test --offline --locked -p dageist-tauri corpus::tests::the_snapshot_is_what_the_host_reports";

/// The corpus, made once for the whole test binary.
fn parts() -> &'static [Part] {
    static PARTS: OnceLock<Vec<Part>> = OnceLock::new();
    PARTS.get_or_init(corpus)
}

/// Every document of the corpus.
fn documents() -> Vec<&'static Reported> {
    parts().iter().flat_map(|part| &part.documents).collect()
}

/// Every call of every expression in the corpus.
fn held() -> &'static [Fact] {
    static FACTS: OnceLock<Vec<Fact>> = OnceLock::new();
    FACTS.get_or_init(|| facts(&documents()))
}

/// The calls of a document: its steps, and every start and link of every expression inside them.
fn calls_of(document: &Document) -> Vec<&Call> {
    fn in_value<'a>(value: &'a Value, found: &mut Vec<&'a Call>) {
        match value {
            Value::Expression(expression) => {
                if let Start::Call(start) = expression.start() {
                    in_call(start, found);
                }
                for link in expression.links() {
                    in_call(link, found);
                }
            }
            Value::Many(items) => items.iter().for_each(|item| in_value(item, found)),
            Value::Pairs(pairs) => {
                for (key, entry) in pairs {
                    in_value(key, found);
                    in_value(entry, found);
                }
            }
            _ => {}
        }
    }
    fn in_call<'a>(call: &'a Call, found: &mut Vec<&'a Call>) {
        found.push(call);
        for given in call.arguments() {
            in_value(given.value(), found);
        }
    }
    let mut found = Vec::new();
    for step in document.steps() {
        for call in step.calls() {
            in_call(call, &mut found);
        }
    }
    found
}

/// The methods the engine allows after a value of a shape at some place that the corpus holds at
/// no place the walk allows them, each with why. A method the engine allows after a count of
/// values only with an argument that meets the count is one the corpus can give no such argument
/// of: a list of many values is refused by the rule that the values a call meets meet in count,
/// and the argument that `is_in` is allowed is a collection imploded, which the corpus does not
/// give. A field
/// read after a count of structs is the other: the values that differ are counted by `value_counts`,
/// whose fields are the column's own name and the count, so the one name the corpus gives a field
/// is no field of it. Renaming the fields is the third: a method added to a chain is given no names,
/// and a struct is refused names that are not as many as its fields.
const NOT_PLACED: [(&str, &str); 5] = [
    ("Expr.is_in", "counted"),
    ("Expr.struct.field", "counted"),
    ("Expr.struct.rename_fields", "counted"),
    ("Expr.struct.rename_fields", "many"),
    ("Expr.struct.rename_fields", "one"),
];

/// What the engine allows of the methods, by the entry and the word of the shape, over every place
/// of the grid.
fn allowed_methods() -> BTreeSet<(&'static str, &'static str)> {
    let grid = grid();
    let mut found = BTreeSet::new();
    for (number, entry) in grid.entries.iter().enumerate() {
        if entry.applies() != Applies::Expression {
            continue;
        }
        for shape in Shape::ALL {
            let at_some_place = (0..grid.contexts.len()).any(|context| {
                Representative::ALL.iter().any(|class| {
                    let after = grid.after_at(After::Value {
                        dtype: *class,
                        shape,
                    });
                    matches!(grid.cell(context, after, number), Cell::Allowed { .. })
                })
            });
            if at_some_place {
                found.insert((entry.id().as_str(), shape.name()));
            }
        }
    }
    found
}

#[test]
fn every_method_is_held_after_every_shape_of_value_the_engine_allows_it_after() {
    let placed: BTreeSet<(&str, &str)> = held()
        .iter()
        .filter(|fact| fact.verdict == Verdict::Allowed)
        .filter_map(|fact| match &fact.follows {
            Follows::Value { shape, .. } => Some((fact.entry, shape.as_str())),
            _ => None,
        })
        .collect();
    let missing: BTreeSet<(&str, &str)> = allowed_methods().difference(&placed).copied().collect();
    let declared: BTreeSet<(&str, &str)> = NOT_PLACED.into_iter().collect();

    assert_eq!(
        missing, declared,
        "a method the corpus does not hold where the engine allows it, and is not declared; or one declared that it holds"
    );
}

#[test]
fn every_function_that_starts_a_chain_and_every_link_of_a_condition_is_held_where_the_engine_allows_it()
 {
    let grid = grid();
    let start = grid.after_at(After::Start);
    for (number, entry) in grid.entries.iter().enumerate() {
        let id = entry.id().as_str();
        let at_start = (0..grid.contexts.len())
            .any(|context| matches!(grid.cell(context, start, number), Cell::Allowed { .. }));
        match entry.applies() {
            Applies::Function | Applies::Condition if at_start => {
                assert!(
                    held().iter().any(|fact| fact.entry == id
                        && fact.follows == Follows::Start
                        && fact.verdict == Verdict::Allowed),
                    "{id} is not held as a start where the engine allows it"
                );
            }
            Applies::Condition => {
                assert!(
                    held().iter().any(|fact| fact.entry == id
                        && matches!(fact.follows, Follows::Condition(_))
                        && fact.verdict == Verdict::Allowed),
                    "{id} is not held in a condition where the engine allows it"
                );
            }
            Applies::Expression | Applies::Function | Applies::Frame | Applies::Grouped => {}
        }
    }
}

#[test]
fn every_context_the_tables_cover_holds_a_chain() {
    let held_in: HashSet<&str> = held()
        .iter()
        .filter_map(|fact| fact.context.as_deref())
        .collect();
    for context in Context::all() {
        assert!(
            held_in.contains(context.name().as_str()),
            "no chain stands in {context}"
        );
    }
}

#[test]
fn every_context_where_the_engine_allows_a_method_holds_one() {
    let grid = grid();
    let held_methods: HashSet<&str> = held()
        .iter()
        .filter(|fact| {
            fact.verdict == Verdict::Allowed
                && matches!(fact.follows, Follows::Value { .. })
                && catalog::entry(fact.entry)
                    .is_some_and(|entry| entry.applies() == Applies::Expression)
        })
        .filter_map(|fact| fact.context.as_deref())
        .collect();
    for (at, context) in grid.contexts.iter().enumerate() {
        let allows_one = (0..grid.entries.len()).any(|number| {
            grid.entries[number].applies() == Applies::Expression
                && Representative::ALL.iter().any(|class| {
                    Shape::ALL.iter().any(|shape| {
                        let after = grid.after_at(After::Value {
                            dtype: *class,
                            shape: *shape,
                        });
                        matches!(grid.cell(at, after, number), Cell::Allowed { .. })
                    })
                })
        });
        assert_eq!(
            held_methods.contains(context.name().as_str()),
            allows_one,
            "{context}: the engine allows a method there, or it does not, and the corpus holds one"
        );
    }
}

#[test]
fn every_step_of_the_catalogue_is_held() {
    // A step is held by the call that makes it, and by the calls on a group by that follow `group_by`.
    let held_steps: HashSet<&str> = documents()
        .iter()
        .flat_map(|held| held.model.steps())
        .flat_map(|step| step.calls())
        .map(|call| call.entry().id().as_str())
        .collect();
    for entry in catalog::entries() {
        if entry.applies().is_of_a_step() {
            assert!(
                held_steps.contains(entry.id().as_str()),
                "no document holds a step {}",
                entry.id().as_str()
            );
        }
    }
}

#[test]
fn every_parameter_the_engine_offers_is_given_in_a_document_that_calls_its_entry() {
    let mut given: HashSet<(&str, &str)> = HashSet::new();
    for held in documents() {
        for call in calls_of(&held.model) {
            for argument in call.arguments() {
                given.insert((call.entry().id().as_str(), argument.parameter()));
            }
        }
    }
    let mut missing = Vec::new();
    for entry in catalog::entries() {
        for parameter in entry.parameters().iter().filter(|held| held.offered()) {
            if !given.contains(&(entry.id().as_str(), parameter.name())) {
                missing.push(format!("{} {}", entry.id().as_str(), parameter.name()));
            }
        }
    }
    assert_eq!(missing, Vec::<String>::new());
}

#[test]
fn every_kind_of_start_the_census_lists_is_held() {
    let census = committed("census.json");
    let starts: Vec<&Json> = census["starts"]
        .as_array()
        .expect("starts")
        .iter()
        .collect();
    // Which starts of the corpus's documents there are, in the census's words.
    let mut found: HashSet<String> = HashSet::new();
    for held in documents() {
        starts_of(&held.model, &mut found);
    }
    for start in starts {
        let word = match start["kind"].as_str().expect("a kind") {
            "typed" => format!("typed {}", start["literal"].as_str().expect("a literal")),
            "function" => format!("function {}", start["entry"].as_str().expect("an entry")),
            other => other.to_owned(),
        };
        assert!(
            found.contains(&word),
            "no document starts a chain as {word}"
        );
    }
}

/// Notes how each chain of `document` starts, in the census's words.
fn starts_of(document: &Document, found: &mut HashSet<String>) {
    fn in_value(value: &Value, found: &mut HashSet<String>) {
        match value {
            Value::Expression(expression) => {
                found.insert(match expression.start() {
                    Start::Hole => "hole".to_owned(),
                    Start::Input(name) if name == VALUE => "connectedValue".to_owned(),
                    Start::Input(name) if name == LIST => "connectedList".to_owned(),
                    Start::Input(name) => format!("input {name}"),
                    Start::Call(call) => match call.entry().id().as_str() {
                        "polars.col" => "field".to_owned(),
                        "polars.lit" => match call.argument("value") {
                            Some(Value::Integer(_)) => "typed integer".to_owned(),
                            Some(Value::Float(_)) => "typed float".to_owned(),
                            Some(Value::String(_)) => "typed text".to_owned(),
                            Some(Value::Flag(_)) => "typed flag".to_owned(),
                            _ => "typed other".to_owned(),
                        },
                        other => format!("function {other}"),
                    },
                });
                if let Start::Call(call) = expression.start() {
                    for given in call.arguments() {
                        in_value(given.value(), found);
                    }
                }
                for link in expression.links() {
                    for given in link.arguments() {
                        in_value(given.value(), found);
                    }
                }
            }
            Value::Many(items) => items.iter().for_each(|item| in_value(item, found)),
            Value::Pairs(pairs) => {
                for (key, entry) in pairs {
                    in_value(key, found);
                    in_value(entry, found);
                }
            }
            _ => {}
        }
    }
    for step in document.steps() {
        for call in step.calls() {
            for given in call.arguments() {
                in_value(given.value(), found);
            }
        }
    }
}

#[test]
fn the_corpus_holds_each_outcome_of_a_step_each_state_of_its_rows_and_each_cause_of_a_refusal() {
    let mut outcomes = BTreeSet::new();
    let mut causes = BTreeSet::new();
    let mut rows = BTreeSet::new();
    for held in documents() {
        for step in held.run["steps"].as_array().expect("steps") {
            rows.insert(step["count"]["kind"].as_str().expect("a count").to_owned());
            outcomes.insert(
                step["outcome"]["status"]
                    .as_str()
                    .expect("a status")
                    .to_owned(),
            );
            if let Some(cause) = step["outcome"]["cause"]["kind"].as_str() {
                causes.insert(cause.to_owned());
            }
        }
    }
    let census = committed("census.json");
    let words = |name: &str| -> BTreeSet<String> {
        census[name]
            .as_array()
            .expect("a list")
            .iter()
            .map(|word| word.as_str().expect("a word").to_owned())
            .collect()
    };
    assert_eq!(outcomes, words("outcomes"));
    assert_eq!(causes, words("causes"));
    assert_eq!(rows, words("rowsStates"));
}

/// The rules that no document of the corpus is refused by, each with why. The offers at a place
/// give a call three rules and the walk the others the corpus meets, among them those of a pattern,
/// a duration, a time zone, a range, a join's keys, a window given a count of values, a replacement
/// and two moments of different zones, for each of which a document is made by editing one that is
/// allowed. These are the rest.
const NOT_HELD: &[(&str, &str)] = &[
    (
        "one-column-is-needed",
        "no document the corpus plans unnests a struct of several fields where a filter needs one column; one was not tried, and may be built",
    ),
    (
        "to-struct-names-are-distinct",
        "no document the corpus plans names the fields of a struct a list or an array is made alike; one was not tried, and may be built",
    ),
    (
        "struct-is-filled-forward-or-backward",
        "no document the corpus plans fills a struct by a strategy; one was not tried, and may be built",
    ),
    (
        "written-names-are-distinct",
        "no document the corpus plans writes one name twice by a field of a struct; one was not tried, and may be built",
    ),
    (
        "arrays-meet-at-one-width",
        "the arrays of the corpus's tables are all of the one width the tables measure, 2, so none meets one of another width",
    ),
    (
        "folded-function-is-no-join-key",
        "the one document tried, a date made of numbers typed in as a join's key, is refused by Polars as the tables record it before the rule is asked; another may hold it",
    ),
    (
        "window-input-is-not-widened",
        "no document the corpus plans gives a window a value of one row that a part widens; one was not tried, and may be built",
    ),
];

/// Every word of a rule that a report names as the source of a refusal.
fn rules_in(json: &Json, found: &mut BTreeSet<String>) {
    match json {
        Json::Object(fields) => {
            if fields.get("by").and_then(Json::as_str) == Some("rule")
                && let Some(rule) = fields.get("rule").and_then(Json::as_str)
            {
                found.insert(rule.to_owned());
            }
            fields.values().for_each(|inner| rules_in(inner, found));
        }
        Json::Array(items) => items.iter().for_each(|inner| rules_in(inner, found)),
        _ => {}
    }
}

#[test]
fn the_corpus_holds_a_refusal_by_each_rule_and_each_stage_of_polars_the_tables_give() {
    let grid = grid();
    // Every refusal the tables give a call at a place of the grid, by its source.
    let mut given: BTreeSet<String> = BTreeSet::new();
    for context in 0..grid.contexts.len() {
        for after in 0..grid.afters.len() {
            for entry in 0..grid.entries.len() {
                if let Cell::Refused(source) = grid.cell(context, after, entry) {
                    given.insert(source_word(source));
                }
            }
        }
    }
    let held_refusals: BTreeSet<String> = held()
        .iter()
        .filter_map(|fact| match &fact.verdict {
            Verdict::Refused(source) => Some(source.clone()),
            _ => None,
        })
        .collect();
    let unheld: Vec<&String> = given
        .iter()
        .filter(|source| !held_refusals.contains(*source))
        .collect();
    assert_eq!(unheld, Vec::<&String>::new());
    // The stage of a table that ran on no rows refuses no call at a place: what Polars refuses
    // only there it refuses at the schema or on the rows of the other tables as well.
    for stage in [Stage::Schema, Stage::Rows] {
        assert!(
            given.contains(&format!("polars {}", stage.name())),
            "no call is refused at the stage {}",
            stage.name()
        );
    }
    assert!(!given.contains(&format!("polars {}", Stage::Empty.name())));

    // Every rule is the source of a refusal in some report, or is declared not to be.
    let mut held_rules = BTreeSet::new();
    for held in documents() {
        rules_in(&held.run, &mut held_rules);
    }
    let not_held: BTreeSet<&str> = Rule::ALL
        .iter()
        .map(|rule| rule.name())
        .filter(|rule| !held_rules.contains(*rule))
        .collect();
    let declared: BTreeSet<&str> = NOT_HELD.iter().map(|(rule, _)| *rule).collect();
    assert_eq!(not_held, declared);
}

/// The reasons a walk says it cannot tell that no document of the corpus is told by, each with why.
const UNKNOWN_NOT_HELD: &[(&str, &str)] = &[
    (
        "the-fields-of-a-struct",
        "no document the corpus plans reads, drops or renames a field of a struct whose fields the walk lost; one was not tried, and may be built",
    ),
    (
        "an-unnest-as-a-key",
        "no document the corpus plans unnests a struct as a join's key; one was not tried, and may be built",
    ),
    (
        "an-unnest-of-no-field",
        "no document the corpus plans unnests a struct that has no field; one was not tried, and may be built",
    ),
    (
        "an-open-number-in-a-key",
        "no document the corpus plans makes a struct of a number typed without a dtype as a join's key; one was not tried, and may be built",
    ),
    (
        "several-columns-of-a-field-read",
        "no document the corpus plans reads a struct by several names outside a select or a sort; one was not tried, and may be built",
    ),
    (
        "an-unnest-among-predicates",
        "no document the corpus plans unnests a struct beside another predicate of a filter; one was not tried, and may be built",
    ),
    (
        "arithmetic-on-a-struct",
        "no document the corpus plans calculates with a struct, whose fields it does not make; one was not tried, and may be built",
    ),
    (
        "structs-of-other-fields-meet",
        "no document the corpus plans meets a struct with a struct of other fields; one was not tried, and may be built",
    ),
];

/// Collects the words of every ending that says it cannot tell, in a report.
fn cannot_tell_in(json: &Json, found: &mut BTreeSet<String>) {
    match json {
        Json::Object(fields) => {
            if fields.get("kind").and_then(Json::as_str) == Some("notKnown")
                && let Some(because) = fields.get("because").and_then(Json::as_str)
            {
                found.insert(because.to_owned());
            }
            fields
                .values()
                .for_each(|inner| cannot_tell_in(inner, found));
        }
        Json::Array(items) => items.iter().for_each(|inner| cannot_tell_in(inner, found)),
        _ => {}
    }
}

#[test]
fn the_corpus_holds_a_call_the_walk_cannot_tell_for_each_reason_or_declares_it_not_held() {
    let mut held_reasons = BTreeSet::new();
    for held in documents() {
        cannot_tell_in(&held.run, &mut held_reasons);
    }
    let not_held: BTreeSet<&str> = Unknown::ALL
        .iter()
        .filter(|unknown| !held_reasons.contains(&unknown.to_string()))
        .map(|unknown| unknown.name())
        .collect();
    let declared: BTreeSet<&str> = UNKNOWN_NOT_HELD.iter().map(|(name, _)| *name).collect();
    assert_eq!(not_held, declared);
}

/// The word `facts` writes a source of refusal as.
fn source_word(source: dageist_engine::allowed::RefusalSource) -> String {
    match source {
        dageist_engine::allowed::RefusalSource::Rule(rule) => format!("rule {}", rule.name()),
        dageist_engine::allowed::RefusalSource::Polars(stage) => {
            format!("polars {}", stage.name())
        }
        dageist_engine::allowed::RefusalSource::Structure => "structure".to_owned(),
    }
}

#[test]
fn what_the_walk_allows_where_the_corpus_puts_a_call_is_what_the_tables_allow_there() {
    let grid = grid();
    let mut disagree = Vec::new();
    let in_the_chains: HashSet<&str> = parts()
        .iter()
        .filter(|part| part.file == "corpus-methods.json" || part.file == "corpus-starts.json")
        .flat_map(|part| &part.documents)
        .map(|held| held.name.as_str())
        .collect();
    for fact in held() {
        if !in_the_chains.contains(fact.document.as_str()) {
            continue;
        }
        let (Follows::Value { dtype, shape }, Some(context)) = (&fact.follows, &fact.context)
        else {
            continue;
        };
        let (Some(class), Some(shape), Some(context)) = (
            Representative::from_name(dtype),
            Shape::from_name(shape),
            Context::from_name(context),
        ) else {
            continue;
        };
        if NOT_PLACED
            .iter()
            .any(|(id, word)| *id == fact.entry && *word == shape.name())
        {
            continue;
        }
        let entry = grid.entry_at(fact.entry);
        let there = grid.cell(
            grid.context_at(context),
            grid.after_at(After::Value {
                dtype: class,
                shape,
            }),
            entry,
        );
        let tables = matches!(there, Cell::Allowed { .. });
        if tables != (fact.verdict == Verdict::Allowed) {
            disagree.push(format!("{} in {}", fact.entry, fact.document));
        }
    }
    assert_eq!(disagree, Vec::<String>::new());
}

#[test]
fn every_document_writes_as_text_that_reads_back_as_the_same_document() {
    let held = documents();
    let mut failures = Vec::new();
    for reported in &held {
        let text = match dageist_text::write(&reported.model) {
            Ok(text) => text,
            Err(error) => {
                failures.push(format!("{} is not written: {error}", reported.name));
                continue;
            }
        };
        match dageist_text::read(&text) {
            Ok(read) if read == reported.model => {}
            Ok(read) => failures.push(format!(
                "{} reads back as another document:\n{text}\n{}\n{}",
                reported.name,
                read.write(),
                reported.model.write()
            )),
            Err(error) => {
                failures.push(format!("{} does not read: {error}\n{text}", reported.name))
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} documents:\n{}",
        failures.len(),
        held.len(),
        failures
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n\n")
    );
}

#[test]
fn the_snapshot_is_what_the_host_reports() {
    // What the node is connected to, which the starts that are inputs name.
    let inputs = serde_json::to_value(
        crate::node::inputs_view(&super::source::inputs(
            &Representative::ALL.into_iter().collect(),
        ))
        .expect("the inputs"),
    )
    .expect("a view serializes");
    for part in parts() {
        let documents: Vec<Json> = part
            .documents
            .iter()
            .map(|held| {
                serde_json::json!({
                    "name": held.name,
                    "document": held.document,
                    "run": held.run,
                })
            })
            .collect();
        snapshot::check_lines(
            part.file,
            &serde_json::json!({ "inputs": inputs, "documents": documents }),
            REFRESH,
            REFRESH_COMMAND,
        );
    }
}
