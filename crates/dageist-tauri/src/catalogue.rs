//! The catalogue of operations, as the view receives it.
//!
//! The engine's catalogue describes every Polars operation the step editor can
//! offer, and its language file gives the words shown for them. The view draws
//! each parameter by the rule for its kind, so this module translates both
//! into one serializable value. It mirrors `CatalogueView` in
//! `ui/packages/host/src/contract.ts`.
//!
//! The browser stand-in answers from `ui/apps/desktop/src/catalogue.json`, a
//! snapshot of exactly what the command returns. A test keeps the two equal.

use std::fmt;

use dageist_engine::catalog::{
    Applies, DefaultValue, Dtypes, Entry, Family, Kind, Laziness, Parameter, ParameterDefault,
    Shape, ValueType, Words, english, entries,
};
use serde::Serialize;

use crate::sample_table::ErrorView;

/// How many of its kind a parameter holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "shape", rename_all = "camelCase")]
enum ShapeView {
    One,
    Many,
    /// One, or one for each item of the parameter `of`.
    PerItem {
        of: &'static str,
    },
}

/// The type of a literal typed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum ValueTypeView {
    Integer,
    Float,
    String,
    Date,
    Any,
}

/// One word of a choice: as Polars spells it, and as it is shown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ChoiceView {
    word: &'static str,
    shown: String,
}

/// What a parameter holds, which decides how the view draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum KindView {
    Expression {
        dtypes: Vec<&'static str>,
    },
    Column,
    Value {
        #[serde(rename = "type")]
        value_type: ValueTypeView,
    },
    Flag,
    Choice {
        words: Vec<ChoiceView>,
    },
    Dtype,
    Duration,
    Table,
    Path,
    Pairs {
        key: Box<KindView>,
        value: Box<KindView>,
    },
}

/// What a parameter holds when it is not given, as the engine reads Polars'
/// default.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum DefaultView {
    /// It must be given.
    Required,
    /// Python's `None`: unset, which leaves the choice to Polars.
    #[serde(rename = "none")]
    Unset,
    Flag {
        value: bool,
    },
    Integer {
        value: i64,
    },
    Float {
        value: f64,
    },
    Text {
        value: String,
    },
    /// An empty `*` or `**` parameter.
    Empty,
    /// A default the engine does not read as a value.
    Other,
}

/// One parameter of an entry.
#[derive(Debug, Clone, PartialEq, Serialize)]
struct ParameterView {
    /// Python's name, which identifies the parameter.
    name: &'static str,
    /// The name shown for it.
    shown: String,
    label: String,
    kind: KindView,
    shape: ShapeView,
    /// Polars' default, read by the engine.
    default: DefaultView,
    /// Polars' default as Python writes it; `None` when it must be given.
    python: Option<&'static str>,
}

/// What an entry is applied to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum AppliesView {
    Expression,
    Frame,
    Function,
    /// One link of `when`, `then` and `otherwise`.
    Condition,
}

/// One operation of the catalogue.
#[derive(Debug, Clone, PartialEq, Serialize)]
struct EntryView {
    id: &'static str,
    applies: AppliesView,
    family: &'static str,
    name: String,
    hint: String,
    head: Option<String>,
    accepts: Vec<&'static str>,
    lazy: bool,
    parameters: Vec<ParameterView>,
}

/// A family or a dtype group: its key and the words shown for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct WordsView {
    key: &'static str,
    title: String,
}

/// The whole catalogue with its words.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CatalogueView {
    families: Vec<WordsView>,
    dtypes: Vec<WordsView>,
    entries: Vec<EntryView>,
}

/// A word the catalogue needs that the language file does not give.
///
/// The English file is part of the program and the engine's tests prove it
/// gives every word, so this does not happen in a built program. The view is
/// still built to answer with it, not to panic or to show a made-up word: a
/// command that fails tells the view what is missing, and a language file
/// loaded from outside would meet the same rule.
#[derive(Debug, Clone, PartialEq, Eq)]
struct MissingWord(String);

impl fmt::Display for MissingWord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "the language file gives no {}", self.0)
    }
}

impl From<MissingWord> for ErrorView {
    fn from(error: MissingWord) -> Self {
        Self::new(error.to_string())
    }
}

/// The word the language file gave, or what is missing.
fn given(word: Option<&str>, what: impl FnOnce() -> String) -> Result<String, MissingWord> {
    word.map(str::to_owned).ok_or_else(|| MissingWord(what()))
}

const fn applies_view(applies: Applies) -> AppliesView {
    match applies {
        Applies::Expression => AppliesView::Expression,
        Applies::Frame => AppliesView::Frame,
        Applies::Function => AppliesView::Function,
        Applies::Condition => AppliesView::Condition,
    }
}

fn default_view(default: ParameterDefault) -> DefaultView {
    match default.value() {
        DefaultValue::Required => DefaultView::Required,
        DefaultValue::Unset => DefaultView::Unset,
        DefaultValue::Flag(value) => DefaultView::Flag { value },
        DefaultValue::Integer(value) => DefaultView::Integer { value },
        DefaultValue::Float(value) => DefaultView::Float { value },
        DefaultValue::Text(value) => DefaultView::Text { value },
        DefaultValue::Empty => DefaultView::Empty,
        DefaultValue::Other => DefaultView::Other,
    }
}

const fn value_type_view(value_type: ValueType) -> ValueTypeView {
    match value_type {
        ValueType::Integer => ValueTypeView::Integer,
        ValueType::Float => ValueTypeView::Float,
        ValueType::String => ValueTypeView::String,
        ValueType::Date => ValueTypeView::Date,
        ValueType::Any => ValueTypeView::Any,
    }
}

fn shape_view(shape: Shape) -> ShapeView {
    match shape {
        Shape::One => ShapeView::One,
        Shape::Many => ShapeView::Many,
        Shape::PerItem(of) => ShapeView::PerItem { of },
    }
}

fn dtype_keys(dtypes: &[Dtypes]) -> Vec<&'static str> {
    dtypes.iter().map(|group| group.key()).collect()
}

fn kind_view(entry: &Entry, parameter: &Parameter, words: &Words) -> Result<KindView, MissingWord> {
    kind_of(entry, parameter, parameter.kind(), words)
}

fn kind_of(
    entry: &Entry,
    parameter: &Parameter,
    kind: Kind,
    words: &Words,
) -> Result<KindView, MissingWord> {
    Ok(match kind {
        Kind::Expression(dtypes) => KindView::Expression {
            dtypes: dtype_keys(dtypes),
        },
        Kind::Column => KindView::Column,
        Kind::Value(value_type) => KindView::Value {
            value_type: value_type_view(value_type),
        },
        Kind::Flag => KindView::Flag,
        Kind::Choice(choices) => KindView::Choice {
            words: choices
                .iter()
                .map(|&word| {
                    Ok(ChoiceView {
                        word,
                        shown: given(
                            words.choice_word(entry.id(), parameter.name(), word),
                            || {
                                format!(
                                    "word for {word} of {}#{}",
                                    entry.id().as_str(),
                                    parameter.name()
                                )
                            },
                        )?,
                    })
                })
                .collect::<Result<_, MissingWord>>()?,
        },
        Kind::Dtype => KindView::Dtype,
        Kind::Duration => KindView::Duration,
        Kind::Table => KindView::Table,
        Kind::Path => KindView::Path,
        Kind::Pairs(key, value) => KindView::Pairs {
            key: Box::new(kind_of(entry, parameter, *key, words)?),
            value: Box::new(kind_of(entry, parameter, *value, words)?),
        },
    })
}

fn parameter_view(
    entry: &Entry,
    parameter: &Parameter,
    words: &Words,
) -> Result<ParameterView, MissingWord> {
    let id = entry.id();
    let at = || format!("{}#{}", id.as_str(), parameter.name());
    Ok(ParameterView {
        name: parameter.name(),
        shown: given(words.parameter_name(id, parameter.name()), || {
            format!("name of {}", at())
        })?,
        label: given(words.parameter_label(id, parameter.name()), || {
            format!("label of {}", at())
        })?,
        kind: kind_view(entry, parameter, words)?,
        shape: shape_view(parameter.shape()),
        default: default_view(parameter.default()),
        python: match parameter.default() {
            ParameterDefault::Required => None,
            ParameterDefault::Python(text) => Some(text),
        },
    })
}

fn entry_view(entry: &Entry, words: &Words) -> Result<EntryView, MissingWord> {
    let id = entry.id();
    Ok(EntryView {
        id: id.as_str(),
        applies: applies_view(entry.applies()),
        family: entry.family().key(),
        name: given(words.entry_name(id), || format!("name of {}", id.as_str()))?,
        hint: given(words.entry_hint(id), || format!("hint of {}", id.as_str()))?,
        // Only the steps of a table's list have a head, so its absence is not a gap.
        head: words.entry_head(id).map(str::to_owned),
        accepts: dtype_keys(entry.accepts()),
        lazy: entry.laziness() == Laziness::Lazy,
        parameters: entry
            .parameters()
            .iter()
            .map(|parameter| parameter_view(entry, parameter, words))
            .collect::<Result<_, _>>()?,
    })
}

/// Builds the view of the catalogue with the words of `words`.
fn catalogue_view(words: &Words) -> Result<CatalogueView, MissingWord> {
    Ok(CatalogueView {
        families: Family::ALL
            .iter()
            .map(|&family| {
                Ok(WordsView {
                    key: family.key(),
                    title: given(words.family_title(family), || {
                        format!("title of the family {}", family.key())
                    })?,
                })
            })
            .collect::<Result<_, MissingWord>>()?,
        dtypes: Dtypes::ALL
            .iter()
            .map(|&group| {
                Ok(WordsView {
                    key: group.key(),
                    title: given(words.dtypes_name(group), || {
                        format!("name of the dtypes {}", group.key())
                    })?,
                })
            })
            .collect::<Result<_, MissingWord>>()?,
        entries: entries()
            .iter()
            .map(|entry| entry_view(entry, words))
            .collect::<Result<_, _>>()?,
    })
}

/// Reports the catalogue of operations, with its English words, to the view.
#[tauri::command]
pub fn operation_catalogue() -> Result<CatalogueView, ErrorView> {
    Ok(catalogue_view(english())?)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use serde_json::{Value, json};

    use super::*;

    /// The committed snapshot the browser stand-in answers from.
    const SNAPSHOT: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../ui/apps/desktop/src/catalogue.json"
    );

    /// Set to rewrite the snapshot instead of checking it.
    const REFRESH: &str = "DAGEIST_REFRESH_CATALOGUE";

    fn view() -> Value {
        serde_json::to_value(operation_catalogue().expect("the catalogue builds"))
            .expect("the catalogue serializes")
    }

    fn entry(view: &Value, id: &str) -> Value {
        view["entries"]
            .as_array()
            .expect("entries")
            .iter()
            .find(|entry| entry["id"] == id)
            .unwrap_or_else(|| panic!("no entry {id}"))
            .clone()
    }

    fn parameter(view: &Value, id: &str, name: &str) -> Value {
        entry(view, id)["parameters"]
            .as_array()
            .expect("parameters")
            .iter()
            .find(|parameter| parameter["name"] == name)
            .unwrap_or_else(|| panic!("no parameter {name} of {id}"))
            .clone()
    }

    #[test]
    fn shift_serializes_as_its_python_signature_and_the_catalogue_rules_give() {
        // `Expr.shift(n=1, *, fill_value=None)`: `n` is a whole number or an
        // expression, `fill_value` any expression, both one value and lazy-safe.
        // The words are those of the English file.
        assert_eq!(
            entry(&view(), "Expr.shift"),
            json!({
                "id": "Expr.shift",
                "applies": "expression",
                "family": "rows",
                "name": "shift",
                "hint": "value n rows earlier",
                "head": null,
                "accepts": ["any"],
                "lazy": true,
                "parameters": [
                    {
                        "name": "n",
                        "shown": "n",
                        "label": "rows",
                        "kind": { "kind": "expression", "dtypes": ["integer"] },
                        "shape": { "shape": "one" },
                        "default": { "kind": "integer", "value": 1 },
                        "python": "1"
                    },
                    {
                        "name": "fill_value",
                        "shown": "fill_value",
                        "label": "fill with",
                        "kind": { "kind": "expression", "dtypes": ["any"] },
                        "shape": { "shape": "one" },
                        "default": { "kind": "none" },
                        "python": "None"
                    }
                ]
            })
        );
    }

    #[test]
    fn a_choice_parameter_serializes_each_word_with_its_shown_word() {
        // `LazyFrame.join(..., how='inner')`, with `how` one of Polars' seven
        // join strategies.
        let words = ["inner", "left", "right", "full", "semi", "anti", "cross"]
            .map(|word| json!({ "word": word, "shown": word }));
        assert_eq!(
            parameter(&view(), "LazyFrame.join", "how"),
            json!({
                "name": "how",
                "shown": "how",
                "label": "kind of join",
                "kind": { "kind": "choice", "words": words },
                "shape": { "shape": "one" },
                "default": { "kind": "text", "value": "inner" },
                "python": "'inner'"
            })
        );
    }

    #[test]
    fn a_per_item_parameter_names_the_parameter_it_follows() {
        // `LazyFrame.sort(by, *more_by, descending=False, ...)`: one flag, or
        // one for each of `by`.
        assert_eq!(
            parameter(&view(), "LazyFrame.sort", "descending"),
            json!({
                "name": "descending",
                "shown": "descending",
                "label": "descending",
                "kind": { "kind": "flag" },
                "shape": { "shape": "perItem", "of": "by" },
                "default": { "kind": "flag", "value": false },
                "python": "False"
            })
        );
    }

    #[test]
    fn a_required_parameter_has_a_required_default_and_a_step_has_a_head() {
        let view = view();
        // `LazyFrame.join(other, ...)`: `other` has no default.
        let other = parameter(&view, "LazyFrame.join", "other");
        assert_eq!(other["default"], json!({ "kind": "required" }));
        assert_eq!(other["python"], Value::Null);
        assert_eq!(other["kind"], json!({ "kind": "table" }));
        // A frame step is the only entry with a plain head.
        assert!(entry(&view, "LazyFrame.join")["head"].is_string());
        assert_eq!(entry(&view, "LazyFrame.join")["applies"], "frame");
        assert_eq!(entry(&view, "polars.col")["applies"], "function");
    }

    #[test]
    fn defaults_serialize_as_the_value_the_engine_read_and_the_python_text() {
        let view = view();
        // Each case: entry, parameter, the default as Python writes it, and the value read.
        // `LazyFrame.join(..., coalesce=None, allow_parallel=True)`, `filter(*predicates)`,
        // `filter(**constraints)`, and `LazyFrame.pivot(separator='_')`.
        let cases = [
            (
                "LazyFrame.join",
                "coalesce",
                "None",
                json!({ "kind": "none" }),
            ),
            (
                "LazyFrame.join",
                "allow_parallel",
                "True",
                json!({ "kind": "flag", "value": true }),
            ),
            (
                "LazyFrame.filter",
                "predicates",
                "()",
                json!({ "kind": "empty" }),
            ),
            (
                "LazyFrame.filter",
                "constraints",
                "{}",
                json!({ "kind": "empty" }),
            ),
            (
                "LazyFrame.pivot",
                "separator",
                "'_'",
                json!({ "kind": "text", "value": "_" }),
            ),
        ];
        for (id, name, python, read) in cases {
            let found = parameter(&view, id, name);
            assert_eq!(found["python"], python, "{id} {name}");
            assert_eq!(found["default"], read, "{id} {name}");
        }
    }

    #[test]
    fn a_default_is_read_as_each_type_python_writes() {
        // The kinds that no catalogued parameter has yet, from Python's own spellings.
        let cases = [
            ("7", json!({ "kind": "integer", "value": 7 })),
            ("-2", json!({ "kind": "integer", "value": -2 })),
            ("0.5", json!({ "kind": "float", "value": 0.5 })),
            ("'x'", json!({ "kind": "text", "value": "x" })),
            ("DEFAULT_QUERY_OPT_FLAGS", json!({ "kind": "other" })),
        ];
        for (python, read) in cases {
            let python: &'static str = python;
            let value = serde_json::to_value(default_view(ParameterDefault::Python(python)))
                .expect("a default serializes");
            assert_eq!(value, read, "{python}");
        }
        assert_eq!(
            serde_json::to_value(default_view(ParameterDefault::Required))
                .expect("a default serializes"),
            json!({ "kind": "required" })
        );
    }

    #[test]
    fn the_links_of_a_condition_apply_to_a_condition() {
        let view = view();
        // `polars.when(*predicates, **constraints)` starts one, `Then.otherwise(statement)`
        // ends it, and both are of the condition family.
        let when = entry(&view, "polars.when");
        assert_eq!(when["applies"], "condition");
        assert_eq!(when["family"], "condition");
        assert_eq!(when["accepts"], json!([]));
        assert_eq!(when["head"], Value::Null);
        let statement = parameter(&view, "Then.otherwise", "statement");
        assert_eq!(statement["default"], json!({ "kind": "required" }));
        assert_eq!(
            statement["kind"],
            json!({ "kind": "expression", "dtypes": ["any"] })
        );
        assert_eq!(entry(&view, "Then.otherwise")["applies"], "condition");
        // `polars.element()` is a function of the start family, with no parameter.
        let element = entry(&view, "polars.element");
        assert_eq!(element["applies"], "function");
        assert_eq!(element["family"], "start");
        assert_eq!(element["parameters"], json!([]));
    }

    #[test]
    fn pairs_serialize_the_kind_of_each_side() {
        // `LazyFrame.with_columns(**named_exprs)`: a column name for each
        // expression.
        assert_eq!(
            parameter(&view(), "LazyFrame.with_columns", "named_exprs")["kind"],
            json!({
                "kind": "pairs",
                "key": { "kind": "column" },
                "value": { "kind": "expression", "dtypes": ["any"] }
            })
        );
    }

    #[test]
    fn families_come_in_the_order_they_are_offered() {
        let view = view();
        let keys: Vec<&str> = view["families"]
            .as_array()
            .expect("families")
            .iter()
            .map(|family| family["key"].as_str().expect("a key"))
            .collect();
        assert_eq!(
            keys,
            [
                "steps",
                "rows",
                "cumulative",
                "arithmetic",
                "aggregate",
                "temporal",
                "window",
                "compare",
                "start",
                "condition"
            ]
        );
        assert_eq!(view["families"][0]["title"], "Steps · change the table");
        assert_eq!(view["dtypes"][0], json!({ "key": "any", "title": "any" }));
    }

    #[test]
    fn entries_are_ordered_by_identifier() {
        let view = view();
        let ids: Vec<&str> = view["entries"]
            .as_array()
            .expect("entries")
            .iter()
            .map(|entry| entry["id"].as_str().expect("an id"))
            .collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert!(ids.len() > 1);
        assert_eq!(ids, sorted);
    }

    #[test]
    fn a_language_file_without_a_word_rejects_the_catalogue_naming_it() {
        // An empty language file is a valid one that gives no word at all.
        let words = Words::parse("").expect("an empty file is a language file");
        let error = catalogue_view(&words).expect_err("the first family has no title");
        assert_eq!(
            serde_json::to_value(ErrorView::from(error)).expect("the error serializes"),
            json!({ "message": "the language file gives no title of the family steps" })
        );
    }

    #[test]
    fn the_snapshot_is_what_the_command_returns() {
        let path = Path::new(SNAPSHOT);
        let returned = view();
        if std::env::var_os(REFRESH).is_some() {
            let mut text = serde_json::to_string_pretty(&returned).expect("it serializes");
            text.push('\n');
            std::fs::write(path, text).expect("the snapshot is written");
            return;
        }
        // The values are compared, not the text: the view's formatter lays short lists out
        // on one line, so the committed file is formatted after it is written.
        let committed: Option<Value> = std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok());
        assert!(
            committed.as_ref() == Some(&returned),
            "ui/apps/desktop/src/catalogue.json is not what the operation_catalogue command \
             returns. Refresh it with: {REFRESH}=1 cargo test --offline --locked -p \
             dageist-tauri catalogue::tests::the_snapshot_is_what_the_command_returns && npm \
             --prefix ui run format"
        );
    }
}
