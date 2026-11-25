//! The corpus: what it holds, held against the engine's own tables and the catalogue, and the
//! snapshot of what it is.
//!
//! Every expectation is written from what the catalogue and the tables say, not from what the
//! generator made: the generator plans from the same tables, so a test that read its plan back
//! would hold nothing. There is no earlier failing run of the corpus writer to show: the writer is
//! new, so these tests establish what it holds and do not repeat a regression.

use std::collections::{BTreeSet, HashSet};
use std::sync::OnceLock;

use dageist_engine::allowed::{After, Context, Representative, Rule, Shape, Stage};
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
        in_call(step.call(), &mut found);
    }
    found
}

/// The methods the engine allows after a value of a shape at some place that the corpus holds at
/// no place the walk allows them, each with why. A method the engine allows after a count of
/// values only with an argument that meets the count is one the corpus can give no such argument
/// of: a list of many values is refused by the rule that the values a call meets meet in count,
/// and the argument that would be allowed is the form of `is_in` that Polars has deprecated.
const NOT_PLACED: [(&str, &str); 1] = [("Expr.is_in", "counted")];

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
            Applies::Expression | Applies::Function | Applies::Frame => {}
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
    let held_steps: HashSet<&str> = documents()
        .iter()
        .flat_map(|held| held.model.steps())
        .map(|step| step.call().entry().id().as_str())
        .collect();
    for entry in catalog::entries() {
        if entry.applies() == Applies::Frame {
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
        for given in step.call().arguments() {
            in_value(given.value(), found);
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
/// a duration, a time zone, a range, a join's keys and a window given a count, for each of which a
/// document is made by editing one that is allowed. These five are the rest.
const NOT_HELD: &[(&str, &str)] = &[
    (
        "arrays-meet-at-one-width",
        "the arrays of the corpus's tables are all of the one width the tables measure, 2, so none meets one of another width",
    ),
    (
        "folded-function-is-no-join-key",
        "a date made of numbers typed in, as a join's key, is refused by Polars as the tables record it before the rule is asked",
    ),
    (
        "moments-share-a-zone",
        "a comparison of a moment of a zone with one of none is refused by Polars on the rows as the tables record it before the rule is asked",
    ),
    (
        "replacement-values-match-in-number",
        "it needs a list of one length beside a list of another, and the node is connected to one list",
    ),
    (
        "window-input-is-not-widened",
        "it needs a value of one row that a part widens, which no chain the corpus plans reaches",
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
