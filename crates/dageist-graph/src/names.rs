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

/// What identifies a node: a UUID, written as 36 lowercase characters with
/// hyphens. A new node's id is [minted](Self::mint) by whoever asks for the
/// node and carried by the edit that adds it, so making an edit is
/// deterministic. The id stays when the node is renamed, and no two nodes,
/// in any graph, are given the same one.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(String);

impl NodeId {
    /// A new id, a UUID of version 7, which holds the time it was made: ids
    /// minted later by the same process sort after it.
    #[must_use]
    pub fn mint() -> Self {
        Self(uuid::Uuid::now_v7().hyphenated().to_string())
    }

    /// Reads `text` as a node id.
    ///
    /// # Errors
    ///
    /// When `text` is not a UUID written as 36 lowercase characters with
    /// hyphens, the one way an id is written.
    pub fn new(text: impl Into<String>) -> Result<Self, NameError> {
        let text = text.into();
        match uuid::Uuid::try_parse(&text) {
            Ok(id) if id.hyphenated().to_string() == text => Ok(Self(text)),
            _ => Err(NameError {
                what: "node id",
                text,
                reason: "it must be a UUID written in lowercase with hyphens",
            }),
        }
    }

    /// The id as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
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
    pub fn made_from(text: &str) -> Self {
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
    fn a_node_id_is_a_uuid_written_in_lowercase_with_hyphens() {
        let text = "0197a3c2-5e10-7d4b-9c1e-2b6f0a8d4e31";
        assert_eq!(NodeId::new(text).unwrap().as_str(), text);
        for refused in [
            "",
            "documents",
            "0197A3C2-5E10-7D4B-9C1E-2B6F0A8D4E31",
            "0197a3c25e107d4b9c1e2b6f0a8d4e31",
            "{0197a3c2-5e10-7d4b-9c1e-2b6f0a8d4e31}",
            "urn:uuid:0197a3c2-5e10-7d4b-9c1e-2b6f0a8d4e31",
            "0197a3c2-5e10-7d4b-9c1e-2b6f0a8d4e3",
        ] {
            assert!(NodeId::new(refused).is_err(), "{refused:?} was taken");
        }
    }

    #[test]
    fn minted_ids_read_back_differ_and_sort_in_the_order_they_were_made() {
        let ids: Vec<NodeId> = (0..64).map(|_| NodeId::mint()).collect();
        for id in &ids {
            assert_eq!(&NodeId::new(id.as_str()).unwrap(), id);
            // Version 7, in the place RFC 9562 gives the version.
            assert_eq!(id.as_str().as_bytes()[14], b'7');
        }
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted, ids);
    }

    #[test]
    fn names_are_kept_trimmed_and_refused_when_empty_or_long() {
        assert_eq!(Name::new("  Documents ").unwrap().as_str(), "Documents");
        assert!(Name::new("   ").is_err());
        assert!(Name::new(&"x".repeat(81)).is_err());
        assert!(Name::new(&"x".repeat(80)).is_ok());
        assert!(Name::new("a\tb").is_err());
    }

    #[test]
    fn names_that_differ_only_in_case_are_the_same() {
        let one = Name::new("Documents").unwrap();
        assert!(one.same_as(&Name::new("DOCUMENTS").unwrap()));
        assert!(!one.same_as(&Name::new("Documents 2").unwrap()));
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
