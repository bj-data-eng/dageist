//! How the text names what the catalogue names: entries, inputs and steps.
//!
//! An entry's identifier is Python Polars' qualified name. A step `LazyFrame.sort` is written
//! `.sort(...)` on `lf`; a function `polars.col` is `pl.col(...)`; a method `Expr.str.contains` is
//! `.str.contains(...)`, and a link of a condition, `When.then`, is `.then(...)`. dageist's own
//! functions are named as a Polars plugin names its own: `dageist.<name>` is written
//! `dg.<name>(...)`, and a method in their namespace, `Expr.dg.<name>`, `.dg.<name>(...)`.

use dageist_engine::catalog::{self, Entry, LeftOut};
use dageist_syntax::KEYWORDS;

/// The table the steps run on, which the text starts from.
pub(crate) const FRAME: &str = "lf";

/// What the text writes before Polars' functions.
pub(crate) const POLARS: &str = "pl";

/// What the text writes before dageist's own functions.
pub(crate) const DAGEIST: &str = "dg";

/// Whether `name` can stand bare for an input or a table: a Python name that is none of the
/// text's own names and no word of Python's.
pub(crate) fn is_free(name: &str) -> bool {
    let mut characters = name.chars();
    let starts = characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_');
    starts
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
        && !KEYWORDS.contains(&name)
        && ![FRAME, POLARS, DAGEIST].contains(&name)
}

/// Where a name is looked up: what the text puts before it.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Owner<'a> {
    /// A step, after `lf`.
    Frame,
    /// A function, after `pl` or `dg`, which is `polars` or `dageist` in the catalogue.
    Function(&'static str),
    /// A method, after what gives the Python class: `Expr`, or a stage of a condition.
    Method(&'a str),
}

/// The qualified names to try for `path`, in order: the one its owner gives first, then where the
/// catalogue has it elsewhere, so that the engine refuses it for where it stands with its reason.
fn candidates(owner: Owner<'_>, path: &str) -> Vec<String> {
    let first = match owner {
        Owner::Frame => format!("LazyFrame.{path}"),
        Owner::Function(module) => format!("{module}.{path}"),
        Owner::Method(class) => format!("{class}.{path}"),
    };
    let mut names = vec![first];
    if matches!(owner, Owner::Function("dageist")) {
        return names;
    }
    for class in [
        "LazyFrame",
        "polars",
        "Expr",
        "When",
        "Then",
        "ChainedWhen",
        "ChainedThen",
    ] {
        let name = format!("{class}.{path}");
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// The entry `path` names after `owner`, or the left-out item it names, or neither.
pub(crate) fn resolve(owner: Owner<'_>, path: &str) -> Result<&'static Entry, Option<LeftOut>> {
    let names = candidates(owner, path);
    if let Some(entry) = names.iter().find_map(|name| catalog::entry(name)) {
        return Ok(entry);
    }
    let left_out = catalog::left_out();
    let found = names.iter().find_map(|name| {
        left_out
            .binary_search_by(|item| item.id().cmp(name.as_str()))
            .ok()
            .map(|at| left_out[at])
    });
    Err(found)
}

/// How the text writes `entry`: what stands before it, if anything, and the names of its
/// attributes. A step and a method have nothing before them; they follow what they are called
/// on.
pub(crate) fn written(entry: &Entry) -> (Option<&'static str>, Vec<&'static str>) {
    let id = entry.id().as_str();
    let (owner, rest) = id.split_once('.').unwrap_or(("", id));
    let before = match owner {
        "polars" => Some(POLARS),
        "dageist" => Some(DAGEIST),
        _ => None,
    };
    (before, rest.split('.').collect())
}

/// The Python class a call of `entry` gives, which owns the methods that may follow it: a
/// condition's stage, or `Expr`.
pub(crate) fn class_given(entry: &Entry) -> &'static str {
    match entry.id().as_str() {
        "polars.when" => "When",
        "When.then" => "Then",
        "ChainedWhen.then" => "ChainedThen",
        "Then.when" | "ChainedThen.when" => "ChainedWhen",
        _ => "Expr",
    }
}

/// The name a step of `entry` is given when the text names it not: the entry's own name, as the
/// step editor names a new step.
pub(crate) fn stem(entry: &Entry) -> &'static str {
    let id = entry.id().as_str();
    id.rsplit('.').next().unwrap_or(id)
}

/// The names of steps whose names the text does not give, made as the step editor makes the name
/// of a step added at the end: the stem, then the stem with `_2`, `_3` and on, the first that no
/// step has. A name the text gives is taken before any is made.
pub(crate) fn default_names(stems: &[&str], given: &[Option<&str>]) -> Vec<String> {
    let mut taken: Vec<String> = given
        .iter()
        .flatten()
        .map(|name| (*name).to_owned())
        .collect();
    stems
        .iter()
        .zip(given)
        .map(|(stem, given)| {
            if let Some(name) = given {
                return (*name).to_owned();
            }
            let name = std::iter::once((*stem).to_owned())
                .chain((2_u64..).map(|number| format!("{stem}_{number}")))
                .find(|name| !taken.contains(name))
                .unwrap_or_else(|| (*stem).to_owned());
            taken.push(name.clone());
            name
        })
        .collect()
}

/// The text after `step:` in a comment, if the comment is a step's name. Spaces around it are
/// not part of it.
pub(crate) fn step_comment(text: &str) -> Option<&str> {
    text.trim().strip_prefix("step:").map(str::trim)
}

/// Whether a step's name can be written bare after `# step:`.
pub(crate) fn is_bare_step_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "_-.".contains(character))
}
