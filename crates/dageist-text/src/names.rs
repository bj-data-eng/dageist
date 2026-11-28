//! How the text names what the catalogue names: entries, inputs and steps.
//!
//! An entry's identifier is Python Polars' qualified name. A step `LazyFrame.sort` is written
//! `.sort(...)` on `lf`; a function `polars.col` is `pl.col(...)`; a method `Expr.str.contains` is
//! `.str.contains(...)`, and a link of a condition, `When.then`, is `.then(...)`. dageist's own
//! functions are named as a Polars plugin names its own: `dageist.<name>` is written
//! `dg.<name>(...)`, and a method in their namespace, `Expr.dg.<name>`, `.dg.<name>(...)`.

use std::collections::{HashMap, HashSet};

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
    let mut taken: HashSet<String> = given
        .iter()
        .flatten()
        .map(|name| (*name).to_owned())
        .collect();
    // Where each stem's numbering has come to. A name before it was taken when it was passed, and
    // a taken name stays taken, so the search for a stem's next name goes on from there rather
    // than from the stem, which keeps naming many steps of one entry linear.
    let mut next: HashMap<&str, u64> = HashMap::new();
    stems
        .iter()
        .zip(given)
        .map(|(stem, given)| {
            if let Some(name) = given {
                return (*name).to_owned();
            }
            let number = next.entry(stem).or_insert(1);
            loop {
                let name = if *number == 1 {
                    (*stem).to_owned()
                } else {
                    format!("{stem}_{number}")
                };
                *number += 1;
                if taken.insert(name.clone()) {
                    return name;
                }
            }
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

#[cfg(test)]
mod tests {
    use super::default_names as named;

    #[test]
    fn steps_are_named_as_the_editor_names_steps_added_at_the_end() {
        assert_eq!(
            named(&["sort", "head", "sort", "sort", "head"], &[None; 5]),
            ["sort", "head", "sort_2", "sort_3", "head_2"]
        );
    }

    #[test]
    fn a_name_the_text_gives_is_taken_before_any_is_made() {
        assert_eq!(
            named(&["sort", "sort", "sort"], &[Some("sort_2"), None, None]),
            ["sort_2", "sort", "sort_3"]
        );
        assert_eq!(
            named(&["sort", "sort", "sort"], &[None, None, Some("sort")]),
            ["sort_2", "sort_3", "sort"]
        );
        assert_eq!(
            named(&["sort", "sort", "head"], &[None, None, Some("sort_3")]),
            ["sort", "sort_2", "sort_3"]
        );
    }

    #[test]
    fn a_stem_that_is_another_steps_made_name_shares_the_names_taken() {
        assert_eq!(
            named(&["sort", "sort_2", "sort", "sort_by"], &[None; 4]),
            ["sort", "sort_2", "sort_3", "sort_by"]
        );
        assert_eq!(
            named(&["sort_2", "sort", "sort"], &[None, None, Some("sort_2")]),
            ["sort_2_2", "sort", "sort_2"]
        );
    }

    #[test]
    fn many_steps_of_one_entry_are_each_named_once() {
        let stems = vec!["with_columns"; 20_000];
        let names = named(&stems, &vec![None; stems.len()]);
        assert_eq!(names[0], "with_columns");
        assert_eq!(names[19_999], "with_columns_20000");
        let distinct: std::collections::HashSet<&String> = names.iter().collect();
        assert_eq!(distinct.len(), names.len());
    }
}
