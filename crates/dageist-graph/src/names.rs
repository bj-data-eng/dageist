//! The names a graph is written with: node ids, the names shown on nodes, and
//! the identifiers a transform's steps and a Parameters node use.

use std::fmt;

/// Text refused as a name, with why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameError {
    /// What was being named: `node id`, `name`, `input name` or `parameter name`.
    pub what: &'static str,
    /// The text that was refused.
    pub text: String,
    /// Why, in a few words.
    pub reason: &'static str,
}

impl fmt::Display for NameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:?} is not a {}: {}",
            self.text, self.what, self.reason
        )
    }
}

impl std::error::Error for NameError {}

/// What identifies a node in its graph. It is made from the node's name when
/// the node is added, stays when the node is renamed, and is not given to
/// another node of the same graph after the node is removed.
///
/// An id is lowercase ASCII letters, digits and single hyphens between them.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(String);

impl NodeId {
    /// Reads `text` as a node id.
    ///
    /// # Errors
    ///
    /// When `text` is empty, holds anything other than lowercase ASCII
    /// letters, digits and hyphens, or starts, ends or doubles a hyphen.
    pub fn new(text: impl Into<String>) -> Result<Self, NameError> {
        let text = text.into();
        let refuse = |reason| {
            Err(NameError {
                what: "node id",
                text: text.clone(),
                reason,
            })
        };
        if text.is_empty() {
            return refuse("it is empty");
        }
        if !text
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            return refuse("it may hold only lowercase letters, digits and hyphens");
        }
        if text.starts_with('-') || text.ends_with('-') || text.contains("--") {
            return refuse("a hyphen must stand between two other characters");
        }
        Ok(Self(text))
    }

    /// The id as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The id made from `name`: its ASCII letters and digits in lowercase,
    /// with a hyphen for each run of anything else, and `node` when nothing is
    /// left.
    pub(crate) fn from_name(name: &Name) -> Self {
        let mut id = String::new();
        let mut gap = false;
        for character in name.as_str().chars() {
            if character.is_ascii_alphanumeric() {
                if gap && !id.is_empty() {
                    id.push('-');
                }
                gap = false;
                id.push(character.to_ascii_lowercase());
            } else {
                gap = true;
            }
        }
        if id.is_empty() {
            id.push_str("node");
        }
        Self(id)
    }

    /// This id with `-` and `number` after it.
    pub(crate) fn numbered(&self, number: usize) -> Self {
        Self(format!("{}-{number}", self.0))
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// The most characters a node's name may have.
const NAME_LIMIT: usize = 80;

/// The name a node shows on the canvas. It is kept without the spaces around
/// it, and no two nodes of a graph have names that differ only in case.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Name(String);

impl Name {
    /// Reads `text` as a node's name, without the spaces around it.
    ///
    /// # Errors
    ///
    /// When nothing but spaces is left, when it is longer than 80 characters,
    /// or when it holds a control character.
    pub fn new(text: &str) -> Result<Self, NameError> {
        let trimmed = text.trim();
        let refuse = |reason| {
            Err(NameError {
                what: "name",
                text: text.to_owned(),
                reason,
            })
        };
        if trimmed.is_empty() {
            return refuse("it is empty");
        }
        if trimmed.chars().count() > NAME_LIMIT {
            return refuse("it is longer than 80 characters");
        }
        if trimmed.chars().any(char::is_control) {
            return refuse("it holds a control character");
        }
        Ok(Self(trimmed.to_owned()))
    }

    /// The name as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether this name and `other` differ only in case.
    pub(crate) fn same_as(&self, other: &Self) -> bool {
        self.0.to_lowercase() == other.0.to_lowercase()
    }
}

impl fmt::Display for Name {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Checks `text` as an identifier: a letter or an underscore, then letters,
/// digits and underscores, all ASCII.
fn identifier(what: &'static str, text: String) -> Result<String, NameError> {
    let mut characters = text.chars();
    let first = characters.next();
    let valid = first.is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_');
    if valid {
        Ok(text)
    } else {
        Err(NameError {
            what,
            text,
            reason: "it must start with a letter or an underscore and hold only letters, digits and underscores",
        })
    }
}

/// Makes an identifier of `text`: its ASCII letters and digits, with an
/// underscore for each run of anything else, an underscore before a leading
/// digit, and `fallback` when nothing is left.
fn identifier_of(text: &str, fallback: &str) -> String {
    let mut made = String::new();
    let mut gap = false;
    for character in text.chars() {
        if character.is_ascii_alphanumeric() {
            if gap && !made.is_empty() {
                made.push('_');
            }
            gap = false;
            made.push(character.to_ascii_lowercase());
        } else {
            gap = true;
        }
    }
    if made.is_empty() {
        made.push_str(fallback);
    }
    if made.starts_with(|character: char| character.is_ascii_digit()) {
        made.insert(0, '_');
    }
    made
}

/// The name a transform's steps call one of its inputs by, as in
/// `{ "input": "holidays" }` or `{ "table": "step_sla" }`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InputName(String);

impl InputName {
    /// Reads `text` as an input name.
    ///
    /// # Errors
    ///
    /// When `text` is not an identifier: a letter or an underscore, then
    /// letters, digits and underscores.
    pub fn new(text: impl Into<String>) -> Result<Self, NameError> {
        identifier("input name", text.into()).map(Self)
    }

    /// The input name as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// An input name made from `text`.
    pub(crate) fn made_from(text: &str) -> Self {
        Self(identifier_of(text, "input"))
    }

    /// This name with `_` and `number` after it.
    pub(crate) fn numbered(&self, number: usize) -> Self {
        Self(format!("{}_{number}", self.0))
    }
}

impl fmt::Display for InputName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// The name of one value of a Parameters node.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ParameterName(String);

impl ParameterName {
    /// Reads `text` as a parameter name.
    ///
    /// # Errors
    ///
    /// When `text` is not an identifier: a letter or an underscore, then
    /// letters, digits and underscores.
    pub fn new(text: impl Into<String>) -> Result<Self, NameError> {
        identifier("parameter name", text.into()).map(Self)
    }

    /// The parameter name as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ParameterName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::{InputName, Name, NodeId, ParameterName};

    #[test]
    fn node_ids_take_lowercase_words_joined_by_single_hyphens() {
        assert!(NodeId::new("claims-2").is_ok());
        for refused in [
            "", "Claims", "claims_2", "-claims", "claims-", "a--b", "a b",
        ] {
            assert!(NodeId::new(refused).is_err(), "{refused:?} was taken");
        }
    }

    #[test]
    fn an_id_made_from_a_name_keeps_its_words() {
        let name = Name::new("SLA by step (2026)").unwrap();
        assert_eq!(NodeId::from_name(&name).as_str(), "sla-by-step-2026");
        let symbols = Name::new("→ ←").unwrap();
        assert_eq!(NodeId::from_name(&symbols).as_str(), "node");
    }

    #[test]
    fn names_are_kept_trimmed_and_refused_when_empty_or_long() {
        assert_eq!(Name::new("  Claims ").unwrap().as_str(), "Claims");
        assert!(Name::new("   ").is_err());
        assert!(Name::new(&"x".repeat(81)).is_err());
        assert!(Name::new(&"x".repeat(80)).is_ok());
        assert!(Name::new("a\tb").is_err());
    }

    #[test]
    fn names_that_differ_only_in_case_are_the_same() {
        let one = Name::new("Claims").unwrap();
        assert!(one.same_as(&Name::new("CLAIMS").unwrap()));
        assert!(!one.same_as(&Name::new("Claims 2").unwrap()));
    }

    #[test]
    fn input_and_parameter_names_are_identifiers() {
        assert!(InputName::new("step_sla").is_ok());
        assert!(InputName::new("_x1").is_ok());
        assert!(ParameterName::new("as_of").is_ok());
        for refused in ["", "1st", "step sla", "step-sla", "é"] {
            assert!(InputName::new(refused).is_err(), "{refused:?} was taken");
            assert!(
                ParameterName::new(refused).is_err(),
                "{refused:?} was taken"
            );
        }
    }

    #[test]
    fn an_input_name_made_from_text_is_an_identifier() {
        assert_eq!(InputName::made_from("SLA by step").as_str(), "sla_by_step");
        assert_eq!(
            InputName::made_from("2026 holidays").as_str(),
            "_2026_holidays"
        );
        assert_eq!(InputName::made_from("→").as_str(), "input");
        for text in ["SLA by step", "2026 holidays", "→"] {
            let made = InputName::made_from(text);
            assert!(InputName::new(made.as_str()).is_ok());
        }
    }
}
