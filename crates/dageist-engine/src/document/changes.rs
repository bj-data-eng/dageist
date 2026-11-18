//! Which columns a step added and which it replaced.

use polars::prelude::Schema;

/// How a step changed a column of the table it was given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// The table the step gives has the column and the table it was given did
    /// not.
    Added,
    /// Both tables have the column, and the step wrote it or gave it another
    /// dtype.
    Replaced,
}

impl Change {
    /// Every kind of change.
    pub const ALL: [Self; 2] = [Self::Added, Self::Replaced];
}

/// A column a step added or replaced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnChange {
    name: String,
    change: Change,
}

impl ColumnChange {
    /// The changes a step made, in the order of the columns it gives: the
    /// columns of `after` that `before` lacks are added; those `before` has
    /// are replaced when `written` names them or their dtype differs.
    ///
    /// `written` is the names of the columns the step writes, by what its
    /// arguments say: the names `with_columns` gives its expressions.
    pub(super) fn between(before: &Schema, after: &Schema, written: &[String]) -> Vec<Self> {
        after
            .iter()
            .filter_map(|(name, dtype)| {
                let change = match before.get(name) {
                    None => Change::Added,
                    Some(was) if was != dtype || written.iter().any(|own| own == name.as_str()) => {
                        Change::Replaced
                    }
                    Some(_) => return None,
                };
                Some(Self {
                    name: name.to_string(),
                    change,
                })
            })
            .collect()
    }

    /// The column's name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// How the step changed it.
    #[must_use]
    pub const fn change(&self) -> Change {
        self.change
    }
}
