//! A case: the dtypes of the two columns a step document is run on, and the
//! document itself.
//!
//! The document is the case's own codec. Its writer and reader are the step
//! document's, so a kept case is exactly the text the editor would save, and
//! reading it applies every check the document makes of its own. The case adds
//! one line above it that says what `a` and `b` are:
//!
//! ```text
//! {"a":"Int64","b":"String","revision":1}
//! {"version":1,"steps":[ … ]}
//! ```
//!
//! Replay needs the bytes to be what writing the case gives, so a text that
//! reads but is not written the same way is refused.

use dageist_engine::allowed::Representative;
use dageist_engine::document::Document;
use dageist_fuzz::{FuzzError, FuzzErrorKind, Result};

/// The revision of the line above the document. A case of another revision is
/// refused; there is no migration.
pub const REVISION: u64 = 1;

/// The id of the step a generated case holds. A case holds exactly one step.
pub const STEP: &str = "s";

#[derive(Debug, Clone, PartialEq)]
pub struct Case {
    /// The class of the column `a`, which the chain under test usually starts
    /// from.
    pub receiver: Representative,
    /// The class of the column `b`, which stands for an argument.
    pub argument: Representative,
    /// The document: one step.
    pub document: Document,
}

fn invalid(detail: impl Into<String>) -> FuzzError {
    FuzzError::new(
        FuzzErrorKind::InvalidCase,
        "allowed combinations case",
        detail,
    )
}

fn class(name: &str, field: &str) -> Result<Representative> {
    Representative::from_name(name).ok_or_else(|| {
        invalid(format!(
            "{field} is {name:?}, which is no representative dtype"
        ))
    })
}

impl Case {
    /// The canonical bytes of the case.
    pub fn encode(&self) -> Vec<u8> {
        let mut header = serde_json::Map::new();
        header.insert("a".into(), self.receiver.name().into());
        header.insert("b".into(), self.argument.name().into());
        header.insert("revision".into(), REVISION.into());
        let mut text = serde_json::Value::Object(header).to_string();
        text.push('\n');
        text.push_str(&self.document.write());
        text.into_bytes()
    }

    /// The case `bytes` hold, if they are exactly what [`Case::encode`] gives
    /// for one.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let text = std::str::from_utf8(bytes).map_err(|_| invalid("the case is not text"))?;
        let (header, document) = text
            .split_once('\n')
            .ok_or_else(|| invalid("the case has no document under its first line"))?;
        let header: serde_json::Value =
            serde_json::from_str(header).map_err(|error| invalid(error.to_string()))?;
        let serde_json::Value::Object(fields) = header else {
            return Err(invalid("the first line is not an object"));
        };
        if fields.len() != 3 {
            return Err(invalid(
                "the first line holds a, b and revision, and no more",
            ));
        }
        let field = |name: &str| {
            fields
                .get(name)
                .ok_or_else(|| invalid(format!("the first line has no {name}")))
        };
        let revision = field("revision")?.as_u64();
        if revision != Some(REVISION) {
            return Err(FuzzError::new(
                FuzzErrorKind::UnsupportedFormat,
                "allowed combinations case",
                format!("revision {revision:?} is not {REVISION}"),
            ));
        }
        let name = |field_name: &str| -> Result<&str> {
            field(field_name)?
                .as_str()
                .ok_or_else(|| invalid(format!("{field_name} is not text")))
        };
        let case = Self {
            receiver: class(name("a")?, "a")?,
            argument: class(name("b")?, "b")?,
            document: Document::read(document).map_err(|error| invalid(error.to_string()))?,
        };
        if case.encode() != bytes {
            return Err(invalid("the case is not written as the codec writes it"));
        }
        if case.document.steps().len() != 1 {
            return Err(invalid("a case holds exactly one step"));
        }
        Ok(case)
    }
}
