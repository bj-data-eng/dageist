//! The catalogue of operations, as the view receives it.
//!
//! The engine's catalogue describes every Polars operation the step editor can
//! offer, and its language file gives the words shown for them. The view draws
//! each parameter by the rule for its kind, so this module translates both
//! into one serializable value. It mirrors `CatalogueView` in
//! `ui/packages/host/src/contract.ts`.
//!
//! The view's decoder tests and the bridge's tests read
//! `ui/packages/host/fixtures/catalogue.json`, a snapshot of exactly what the
//! command returns. A test keeps the two equal.

use std::fmt;

use dageist_engine::allowed::{Literal, Nested, Outer, Position};
use dageist_engine::catalog::{
    Applies, Comparison, DefaultValue, Entry, Family, Kind, Laziness, Parameter, ParameterDefault,
    Setting, Shape, ValueType, Words, english, entries,
};
use dageist_engine::document::Dtype;
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
    Expression,
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

/// What a parameter holds, in the one shape the view draws every kind of
/// parameter from. It mirrors `SettingView` in `ui/packages/host/src/contract.ts`. A number or text
/// comes with the words the engine shows it as.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum SettingView {
    /// Still to be given. The view's own word for it is `unset`.
    #[serde(rename = "unset")]
    ToGive,
    Flag {
        value: bool,
    },
    Integer {
        value: i64,
        shown: String,
    },
    Float {
        value: f64,
        shown: String,
    },
    /// Text that is typed: a string, a duration or a path.
    Text {
        value: String,
        shown: String,
    },
    /// A name picked from a list: a choice's word, a dtype, a column or a table.
    Word {
        value: String,
    },
    /// An expression that reads this column.
    Field {
        column: String,
    },
    /// An expression that starts from `polars.element` and calls this method.
    Element {
        method: &'static str,
    },
    /// A comparison of a field with something else, by the entry that compares.
    Comparison {
        entry: &'static str,
        field: Box<SettingView>,
        other: Box<SettingView>,
    },
    List {
        items: Vec<SettingView>,
    },
    Pairs {
        entries: Vec<PairView>,
    },
}

/// One entry of pairs.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct PairView {
    key: SettingView,
    value: SettingView,
}

/// The view of a setting the engine gives.
pub(crate) fn setting_view(setting: &Setting) -> SettingView {
    match setting {
        Setting::ToGive => SettingView::ToGive,
        Setting::Flag(value) => SettingView::Flag { value: *value },
        Setting::Integer(value) => SettingView::Integer {
            value: *value,
            shown: Literal::Integer(*value).shown(),
        },
        Setting::Float(value) => SettingView::Float {
            value: *value,
            shown: Literal::Float(*value).shown(),
        },
        Setting::Text(value) => SettingView::Text {
            value: value.clone(),
            shown: Literal::String(value.clone()).shown(),
        },
        Setting::Word(value) => SettingView::Word {
            value: value.clone(),
        },
        Setting::Field(column) => SettingView::Field {
            column: column.clone(),
        },
        Setting::Element(method) => SettingView::Element { method },
        Setting::Comparison {
            entry,
            field,
            other,
        } => SettingView::Comparison {
            entry,
            field: Box::new(setting_view(field)),
            other: Box::new(setting_view(other)),
        },
        Setting::List(items) => SettingView::List {
            items: items.iter().map(setting_view).collect(),
        },
        Setting::Pairs(entries) => SettingView::Pairs {
            entries: entries
                .iter()
                .map(|(key, value)| PairView {
                    key: setting_view(key),
                    value: setting_view(value),
                })
                .collect(),
        },
    }
}

/// A position that holds an expression, as the engine's tables name it: of a
/// step, or inside an expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum PositionView {
    /// A position of a step.
    Outer { name: &'static str },
    /// A position inside an expression.
    Nested { name: &'static str },
}

const fn position_view(position: Position) -> PositionView {
    match position {
        Position::Outer(outer) => PositionView::Outer {
            name: Outer::name(outer),
        },
        Position::Nested(nested) => PositionView::Nested {
            name: Nested::name(nested),
        },
    }
}

/// One parameter of an entry.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
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
    /// What the parameter holds when it is not given, as a setting, or `None`
    /// where it must be given, Polars decides or the default is not a value.
    default_setting: Option<SettingView>,
    /// What the parameter is set to when it is first added, which differs from
    /// its default.
    start: SettingView,
    /// Where the expression the parameter holds stands, when a named position
    /// holds it. The tables of allowed combinations are asked at that place.
    position: Option<PositionView>,
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
    lazy: bool,
    parameters: Vec<ParameterView>,
}

/// A family or a dtype group: its key and the words shown for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct WordsView {
    key: &'static str,
    title: String,
}

/// A comparison of a field with a value: the entry that makes it, and its sign.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ComparisonView {
    entry: &'static str,
    sign: String,
}

/// The whole catalogue with its words.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogueView {
    families: Vec<WordsView>,
    /// The dtypes a document names by one word, in the order a list gives them.
    listed_dtypes: Vec<String>,
    /// The comparisons a test makes, in the order they are offered, each by the entry that makes
    /// it and the sign the language file gives it.
    comparisons: Vec<ComparisonView>,
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
        Kind::Expression(_) => KindView::Expression,
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
        default_setting: parameter.default_setting().as_ref().map(setting_view),
        start: setting_view(&parameter.start_setting()),
        position: Position::of_parameter(id.as_str(), parameter.name()).map(position_view),
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
        listed_dtypes: Dtype::listed().iter().map(ToString::to_string).collect(),
        comparisons: Comparison::ALL
            .iter()
            .map(|&comparison| {
                Ok(ComparisonView {
                    entry: comparison.entry(),
                    sign: given(words.sign(comparison), || {
                        format!("sign of {}", comparison.entry())
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

/// The catalogue of operations with its English words.
pub fn english_catalogue() -> Result<CatalogueView, ErrorView> {
    Ok(catalogue_view(english())?)
}

/// Reports the catalogue of operations, with its English words, to the view.
#[tauri::command]
pub fn operation_catalogue() -> Result<CatalogueView, ErrorView> {
    english_catalogue()
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

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
                "lazy": true,
                "parameters": [
                    {
                        "name": "n",
                        "shown": "n",
                        "label": "rows",
                        "kind": { "kind": "expression" },
                        "shape": { "shape": "one" },
                        "default": { "kind": "integer", "value": 1 },
                        "defaultSetting": { "kind": "integer", "value": 1, "shown": "1" },
                        "start": { "kind": "unset" },
                        "position": null
                    },
                    {
                        "name": "fill_value",
                        "shown": "fill_value",
                        "label": "fill with",
                        "kind": { "kind": "expression" },
                        "shape": { "shape": "one" },
                        "default": { "kind": "none" },
                        "defaultSetting": null,
                        "start": { "kind": "unset" },
                        "position": null
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
                "defaultSetting": { "kind": "word", "value": "inner" },
                "start": { "kind": "word", "value": "left" },
                "position": null
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
                "defaultSetting": { "kind": "flag", "value": false },
                "start": { "kind": "flag", "value": true },
                "position": null
            })
        );
    }

    #[test]
    fn the_comparisons_a_test_makes_come_with_the_signs_of_the_language_file() {
        // Each is an entry of the catalogue that takes what a field is compared with as `other`,
        // in the order they are offered. The signs are written as on paper: `≠`, `≥` and `≤`
        // each take one character, while Polars' own text keeps `!=`, `>=` and `<=`.
        let view = view();
        assert_eq!(
            view["comparisons"],
            json!([
                { "entry": "Expr.eq", "sign": "==" },
                { "entry": "Expr.ne", "sign": "≠" },
                { "entry": "Expr.gt", "sign": ">" },
                { "entry": "Expr.ge", "sign": "≥" },
                { "entry": "Expr.lt", "sign": "<" },
                { "entry": "Expr.le", "sign": "≤" },
            ])
        );
        for comparison in view["comparisons"].as_array().expect("comparisons") {
            let id = comparison["entry"].as_str().expect("an entry");
            assert_eq!(
                parameter(&view, id, "other")["kind"],
                json!({ "kind": "expression" }),
                "{id}"
            );
        }
    }

    #[test]
    fn a_required_parameter_has_a_required_default_and_a_step_has_a_head() {
        let view = view();
        // `LazyFrame.join(other, ...)`: `other` has no default.
        let other = parameter(&view, "LazyFrame.join", "other");
        assert_eq!(other["default"], json!({ "kind": "required" }));
        assert_eq!(other["kind"], json!({ "kind": "table" }));
        // A frame step is the only entry with a plain head.
        assert!(entry(&view, "LazyFrame.join")["head"].is_string());
        assert_eq!(entry(&view, "LazyFrame.join")["applies"], "frame");
        assert_eq!(entry(&view, "polars.col")["applies"], "function");
    }

    #[test]
    fn defaults_serialize_as_the_value_the_engine_read() {
        let view = view();
        // Each case: entry, parameter, and the value the engine read from Polars' default.
        // `LazyFrame.join(..., coalesce=None, allow_parallel=True)`, `filter(*predicates)`,
        // `filter(**constraints)`, and `LazyFrame.pivot(separator='_')`.
        let cases = [
            ("LazyFrame.join", "coalesce", json!({ "kind": "none" })),
            (
                "LazyFrame.join",
                "allow_parallel",
                json!({ "kind": "flag", "value": true }),
            ),
            ("LazyFrame.filter", "predicates", json!({ "kind": "empty" })),
            (
                "LazyFrame.filter",
                "constraints",
                json!({ "kind": "empty" }),
            ),
            (
                "LazyFrame.pivot",
                "separator",
                json!({ "kind": "text", "value": "_" }),
            ),
        ];
        for (id, name, read) in cases {
            let found = parameter(&view, id, name);
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
        assert_eq!(when["head"], Value::Null);
        let statement = parameter(&view, "Then.otherwise", "statement");
        assert_eq!(statement["default"], json!({ "kind": "required" }));
        assert_eq!(statement["kind"], json!({ "kind": "expression" }));
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
                "value": { "kind": "expression" }
            })
        );
    }

    #[test]
    fn an_expression_parameter_that_a_named_position_holds_carries_the_position() {
        // The position the engine's tables name for the expression a parameter holds: the
        // positions of a step, and those inside an expression.
        let view = view();
        let cases = [
            (
                "LazyFrame.sort",
                "by",
                json!({ "kind": "outer", "name": "sort" }),
            ),
            (
                "LazyFrame.select",
                "exprs",
                json!({ "kind": "outer", "name": "select" }),
            ),
            (
                "LazyFrame.filter",
                "predicates",
                json!({ "kind": "outer", "name": "filter" }),
            ),
            (
                "LazyFrame.join",
                "left_on",
                json!({ "kind": "outer", "name": "join_left" }),
            ),
            (
                "LazyFrame.join",
                "right_on",
                json!({ "kind": "outer", "name": "join_right" }),
            ),
            (
                "LazyFrame.pivot",
                "aggregate_function",
                json!({ "kind": "outer", "name": "pivot_aggregate" }),
            ),
            (
                "Expr.over",
                "partition_by",
                json!({ "kind": "nested", "name": "over.partition_by" }),
            ),
            (
                "Expr.over",
                "order_by",
                json!({ "kind": "nested", "name": "over.order_by" }),
            ),
            (
                "polars.when",
                "predicates",
                json!({ "kind": "nested", "name": "condition.predicate" }),
            ),
            (
                "Then.otherwise",
                "statement",
                json!({ "kind": "nested", "name": "condition.value" }),
            ),
        ];
        for (id, name, position) in cases {
            assert_eq!(
                parameter(&view, id, name)["position"],
                position,
                "{id} {name}"
            );
        }
        // An argument of a method has no position of its own, nor has a parameter that is not
        // an expression.
        for (id, name) in [
            ("Expr.shift", "n"),
            ("Expr.sub", "other"),
            ("LazyFrame.pivot", "on"),
            ("LazyFrame.join", "how"),
        ] {
            assert_eq!(
                parameter(&view, id, name)["position"],
                Value::Null,
                "{id} {name}"
            );
        }
    }

    #[test]
    fn the_catalogue_lists_the_dtypes_a_document_names_by_one_word() {
        let listed = view()["listedDtypes"].clone();
        assert_eq!(
            listed,
            json!([
                "Boolean", "Date", "Time", "Null", "String", "Float16", "Float32", "Float64",
                "Int8", "Int16", "Int32", "Int64", "Int128", "UInt8", "UInt16", "UInt32", "UInt64",
                "UInt128"
            ])
        );
    }

    #[test]
    fn a_parameter_carries_what_it_holds_by_default_and_what_it_is_first_set_to() {
        let view = view();
        let setting = |id: &str, name: &str, which: &str| parameter(&view, id, name)[which].clone();
        // Worked out from Python Polars' signatures: `join(how="inner", suffix="_right",
        // coalesce=None)` and `sort(descending=False)`, `shift(n=1)`.
        assert_eq!(
            setting("LazyFrame.join", "how", "defaultSetting"),
            json!({ "kind": "word", "value": "inner" })
        );
        assert_eq!(
            setting("LazyFrame.join", "suffix", "defaultSetting"),
            json!({ "kind": "text", "value": "_right", "shown": "_right" })
        );
        assert_eq!(
            setting("LazyFrame.sort", "descending", "defaultSetting"),
            json!({ "kind": "flag", "value": false })
        );
        assert_eq!(
            setting("Expr.shift", "n", "defaultSetting"),
            json!({ "kind": "integer", "value": 1, "shown": "1" })
        );
        // Where Polars decides, or the parameter must be given, there is none.
        assert!(setting("LazyFrame.join", "coalesce", "defaultSetting").is_null());
        assert!(setting("LazyFrame.join", "other", "defaultSetting").is_null());
        // A first setting differs from the default: the first other word, the opposite flag,
        // a list with one item to give, pairs with none, and otherwise nothing yet.
        assert_eq!(
            setting("LazyFrame.join", "how", "start"),
            json!({ "kind": "word", "value": "left" })
        );
        assert_eq!(
            setting("LazyFrame.sort", "descending", "start"),
            json!({ "kind": "flag", "value": true })
        );
        assert_eq!(
            setting("LazyFrame.join", "left_on", "start"),
            json!({ "kind": "list", "items": [{ "kind": "unset" }] })
        );
        assert_eq!(
            setting("LazyFrame.rename", "mapping", "start"),
            json!({ "kind": "pairs", "entries": [] })
        );
        assert_eq!(
            setting("Expr.shift", "n", "start"),
            json!({ "kind": "unset" })
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
        // What an entry accepts on what it follows is the engine's to say through its offers, so
        // neither the groups of dtypes nor what an entry accepts of them is sent.
        assert!(view.get("dtypes").is_none());
        assert!(entry(&view, "Expr.shift").get("accepts").is_none());
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
            json!({
                "kind": "failed",
                "message": "the language file gives no title of the family steps"
            })
        );
    }

    #[test]
    fn the_snapshot_is_what_the_command_returns() {
        crate::snapshot::check(
            "catalogue.json",
            &view(),
            REFRESH,
            "cargo test --offline --locked -p dageist-tauri \
             catalogue::tests::the_snapshot_is_what_the_command_returns",
        );
    }
}

#[cfg(test)]
pub(crate) mod fixtures;
