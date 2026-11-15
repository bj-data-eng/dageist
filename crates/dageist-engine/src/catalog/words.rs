//! The words the editor shows, read from a language file.
//!
//! Every word comes from the file, Polars' own names included, so that
//! renaming something for the editor, or translating the API, changes the
//! file alone and never the catalogue's identifiers or what was saved. A file
//! is TOML, keyed by entry identifier, parameter name and choice word:
//!
//! ```toml
//! changed = []                       # names deliberately not Polars' own
//!
//! [families.rows]
//! title = "Rows"
//!
//! [dtypes.numeric]
//! name = "numeric"
//! hint = "integers and floats"
//!
//! [entries."Expr.shift"]
//! name = "shift"                     # the shown name
//! hint = "value n rows earlier"
//! # head = "Add column"              # frame steps only: the step's plain head
//!
//! [entries."Expr.shift".parameters.n]
//! name = "n"
//! label = "rows"
//!
//! [entries."Expr.over".parameters.mapping_strategy.choices]
//! group_to_rows = "group_to_rows"    # choice word = shown word
//! ```
//!
//! `changed` names what is shown differently from Polars' name, as
//! `Entry.id`, `Entry.id#parameter` or `Entry.id#parameter#word`.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::OnceLock;

use toml::{Table, Value};

use super::{Dtypes, EntryId, Family};

/// Why a language file could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WordsError {
    /// The file is not valid TOML.
    Syntax(String),
    /// A word the file must give is missing.
    Missing {
        /// Where, such as `entries."Expr.shift".name`.
        path: String,
    },
    /// A value is not the kind the file holds there.
    WrongType {
        /// Where.
        path: String,
        /// What belongs there.
        expected: &'static str,
    },
    /// The file holds something no language file holds.
    Unknown {
        /// Where.
        path: String,
    },
}

impl fmt::Display for WordsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax(message) => write!(formatter, "the language file is not TOML: {message}"),
            Self::Missing { path } => write!(formatter, "the language file gives no {path}"),
            Self::WrongType { path, expected } => {
                write!(formatter, "{path} in the language file must be {expected}")
            }
            Self::Unknown { path } => write!(formatter, "the language file has an unknown {path}"),
        }
    }
}

impl std::error::Error for WordsError {}

#[derive(Debug, Clone)]
struct ParameterWords {
    name: String,
    label: String,
    choices: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
struct EntryWords {
    name: String,
    hint: String,
    head: Option<String>,
    parameters: BTreeMap<String, ParameterWords>,
}

#[derive(Debug, Clone)]
struct DtypesWords {
    name: String,
    hint: String,
}

/// The words of one language.
#[derive(Debug, Clone)]
pub struct Words {
    changed: Vec<String>,
    families: BTreeMap<String, String>,
    dtypes: BTreeMap<String, DtypesWords>,
    entries: BTreeMap<String, EntryWords>,
}

/// The English words, from `en.toml`.
///
/// # Panics
///
/// Never in a built program: a test reads the file the same way.
#[must_use]
pub fn english() -> &'static Words {
    static ENGLISH: OnceLock<Words> = OnceLock::new();
    ENGLISH
        .get_or_init(|| Words::parse(include_str!("en.toml")).expect("en.toml is a language file"))
}

impl Words {
    /// Reads a language file.
    ///
    /// # Errors
    ///
    /// When the text is not TOML, lacks a word every file gives, holds a
    /// value of the wrong type, or holds a key no language file has.
    pub fn parse(source: &str) -> Result<Self, WordsError> {
        let root: Table = source
            .parse()
            .map_err(|error: toml::de::Error| WordsError::Syntax(error.to_string()))?;
        only(&root, &["changed", "families", "dtypes", "entries"], "")?;
        let changed = match root.get("changed") {
            None => Vec::new(),
            Some(value) => array_of_text(value, "changed")?,
        };
        Ok(Self {
            changed,
            families: section(&root, "", "families", |path, table| {
                only(table, &["title"], path)?;
                text(table, "title", path)
            })?,
            dtypes: section(&root, "", "dtypes", |path, table| {
                only(table, &["name", "hint"], path)?;
                Ok(DtypesWords {
                    name: text(table, "name", path)?,
                    hint: text(table, "hint", path)?,
                })
            })?,
            entries: section(&root, "", "entries", entry_words)?,
        })
    }

    /// Names deliberately shown differently from Polars' own: an entry
    /// identifier, `identifier#parameter` or `identifier#parameter#word`.
    #[must_use]
    pub fn changed(&self) -> &[String] {
        &self.changed
    }

    fn entry(&self, id: EntryId) -> Option<&EntryWords> {
        self.entries.get(id.as_str())
    }

    fn parameter(&self, id: EntryId, parameter: &str) -> Option<&ParameterWords> {
        self.entry(id)?.parameters.get(parameter)
    }

    /// The shown name of an entry, such as `shift` or `dt.offset_by`.
    #[must_use]
    pub fn entry_name(&self, id: EntryId) -> Option<&str> {
        self.entry(id).map(|entry| entry.name.as_str())
    }

    /// The short plain description of an entry.
    #[must_use]
    pub fn entry_hint(&self, id: EntryId) -> Option<&str> {
        self.entry(id).map(|entry| entry.hint.as_str())
    }

    /// The plain head of a frame step, such as "Add column". Only frame
    /// steps have one.
    #[must_use]
    pub fn entry_head(&self, id: EntryId) -> Option<&str> {
        self.entry(id)?.head.as_deref()
    }

    /// The shown name of an entry's parameter.
    #[must_use]
    pub fn parameter_name(&self, id: EntryId, parameter: &str) -> Option<&str> {
        self.parameter(id, parameter)
            .map(|parameter| parameter.name.as_str())
    }

    /// The short label of an entry's parameter.
    #[must_use]
    pub fn parameter_label(&self, id: EntryId, parameter: &str) -> Option<&str> {
        self.parameter(id, parameter)
            .map(|parameter| parameter.label.as_str())
    }

    /// The shown word for one choice word of an entry's parameter.
    #[must_use]
    pub fn choice_word(&self, id: EntryId, parameter: &str, word: &str) -> Option<&str> {
        self.parameter(id, parameter)?
            .choices
            .get(word)
            .map(String::as_str)
    }

    /// The title of a family's group in a list.
    #[must_use]
    pub fn family_title(&self, family: Family) -> Option<&str> {
        self.families.get(family.key()).map(String::as_str)
    }

    /// The shown name of a group of dtypes.
    #[must_use]
    pub fn dtypes_name(&self, dtypes: Dtypes) -> Option<&str> {
        self.dtypes
            .get(dtypes.key())
            .map(|words| words.name.as_str())
    }

    /// The short description of a group of dtypes.
    #[must_use]
    pub fn dtypes_hint(&self, dtypes: Dtypes) -> Option<&str> {
        self.dtypes
            .get(dtypes.key())
            .map(|words| words.hint.as_str())
    }

    /// Everything the file names, for checking it against the catalogue.
    #[cfg(test)]
    pub(super) fn named(&self) -> Named<'_> {
        Named {
            families: self.families.keys().map(String::as_str).collect(),
            dtypes: self.dtypes.keys().map(String::as_str).collect(),
            entries: self.entries.keys().map(String::as_str).collect(),
            parameters: self
                .entries
                .iter()
                .flat_map(|(id, entry)| {
                    entry
                        .parameters
                        .keys()
                        .map(move |parameter| (id.as_str(), parameter.as_str()))
                })
                .collect(),
            choices: self
                .entries
                .iter()
                .flat_map(|(id, entry)| {
                    entry.parameters.iter().flat_map(move |(parameter, words)| {
                        words
                            .choices
                            .keys()
                            .map(move |word| (id.as_str(), parameter.as_str(), word.as_str()))
                    })
                })
                .collect(),
        }
    }
}

/// The keys a language file names.
#[cfg(test)]
pub(super) struct Named<'a> {
    pub(super) families: Vec<&'a str>,
    pub(super) dtypes: Vec<&'a str>,
    pub(super) entries: Vec<&'a str>,
    pub(super) parameters: Vec<(&'a str, &'a str)>,
    pub(super) choices: Vec<(&'a str, &'a str, &'a str)>,
}

fn join(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_owned()
    } else if !key
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        format!("{path}.\"{key}\"")
    } else {
        format!("{path}.{key}")
    }
}

fn only(table: &Table, keys: &[&str], path: &str) -> Result<(), WordsError> {
    match table.keys().find(|key| !keys.contains(&key.as_str())) {
        None => Ok(()),
        Some(key) => Err(WordsError::Unknown {
            path: join(path, key),
        }),
    }
}

fn as_table<'a>(value: &'a Value, path: &str) -> Result<&'a Table, WordsError> {
    value.as_table().ok_or_else(|| WordsError::WrongType {
        path: path.to_owned(),
        expected: "a table",
    })
}

fn as_text(value: &Value, path: &str) -> Result<String, WordsError> {
    value
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| WordsError::WrongType {
            path: path.to_owned(),
            expected: "text",
        })
}

fn array_of_text(value: &Value, path: &str) -> Result<Vec<String>, WordsError> {
    value
        .as_array()
        .ok_or_else(|| WordsError::WrongType {
            path: path.to_owned(),
            expected: "a list of text",
        })?
        .iter()
        .map(|item| as_text(item, path))
        .collect()
}

fn text(table: &Table, key: &str, path: &str) -> Result<String, WordsError> {
    let path = join(path, key);
    as_text(
        table
            .get(key)
            .ok_or(WordsError::Missing { path: path.clone() })?,
        &path,
    )
}

/// The table at `key` of `parent`, which is at `base`, read entry by entry. A
/// missing table is an empty one.
fn section<T>(
    parent: &Table,
    base: &str,
    key: &str,
    read: impl Fn(&str, &Table) -> Result<T, WordsError>,
) -> Result<BTreeMap<String, T>, WordsError> {
    let Some(value) = parent.get(key) else {
        return Ok(BTreeMap::new());
    };
    let section_path = join(base, key);
    as_table(value, &section_path)?
        .iter()
        .map(|(name, value)| {
            let path = join(&section_path, name);
            Ok((name.clone(), read(&path, as_table(value, &path)?)?))
        })
        .collect()
}

fn entry_words(path: &str, table: &Table) -> Result<EntryWords, WordsError> {
    only(table, &["name", "hint", "head", "parameters"], path)?;
    let head = match table.get("head") {
        None => None,
        Some(value) => Some(as_text(value, &join(path, "head"))?),
    };
    Ok(EntryWords {
        name: text(table, "name", path)?,
        hint: text(table, "hint", path)?,
        head,
        parameters: section(table, path, "parameters", |parameter_path, table| {
            only(table, &["name", "label", "choices"], parameter_path)?;
            let choices = match table.get("choices") {
                None => BTreeMap::new(),
                Some(value) => {
                    let choices_path = join(parameter_path, "choices");
                    as_table(value, &choices_path)?
                        .iter()
                        .map(|(word, shown)| {
                            Ok((word.clone(), as_text(shown, &join(&choices_path, word))?))
                        })
                        .collect::<Result<_, WordsError>>()?
                }
            };
            Ok(ParameterWords {
                name: text(table, "name", parameter_path)?,
                label: text(table, "label", parameter_path)?,
                choices,
            })
        })?,
    })
}
