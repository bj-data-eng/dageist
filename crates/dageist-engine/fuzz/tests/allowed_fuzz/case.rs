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
//!
//! A column that is a struct may name its fields, under `a_struct` and `b_struct`, which
//! [`Structs`] says; a column that names none is the struct the tables measured.

use dageist_engine::allowed::Representative;
use dageist_engine::document::Document;
use dageist_fuzz::{FuzzError, FuzzErrorKind, Result};

use super::structs::{Fieldset, Structs};

/// The revision of the line above the document. A case of another revision is
/// refused; there is no migration.
pub const REVISION: u64 = 1;

/// The id of the step a generated case holds. A case holds exactly one step.
pub const STEP: &str = "s";

/// The keys of the header that name the fields of the column `a`, and of `b`, which follow the
/// column's own.
const A_STRUCT: &str = "a_struct";
const B_STRUCT: &str = "b_struct";

#[derive(Debug, Clone, PartialEq)]
pub struct Case {
    /// The class of the column `a`, which the chain under test usually starts
    /// from.
    pub receiver: Representative,
    /// The class of the column `b`, which stands for an argument.
    pub argument: Representative,
    /// The fields of `a` and of `b` where they are structs that name them.
    pub structs: Structs,
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
        for (key, class, fields) in [
            ("a", self.receiver, &self.structs.receiver),
            ("b", self.argument, &self.structs.argument),
        ] {
            header.insert(key.into(), class.name().into());
            if let Some(fields) = fields {
                header.insert(format!("{key}_struct"), fields.to_json());
            }
        }
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
        if let Some(unknown) = fields
            .keys()
            .find(|key| !["a", "b", "revision", A_STRUCT, B_STRUCT].contains(&key.as_str()))
        {
            return Err(invalid(format!(
                "the first line holds a, b, revision and the fields of a struct, not {unknown}"
            )));
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
        let receiver = class(name("a")?, "a")?;
        let argument = class(name("b")?, "b")?;
        let named = |key: &str, column: &str, class: Representative| -> Result<Option<Fieldset>> {
            let Some(written) = fields.get(key) else {
                return Ok(None);
            };
            if class != Representative::Struct {
                return Err(invalid(format!(
                    "{key} names fields for {column}, which is {class} and no struct"
                )));
            }
            Fieldset::from_json(written)
                .map(Some)
                .map_err(|refusal| invalid(format!("{key}: {refusal}")))
        };
        let case = Self {
            receiver,
            argument,
            structs: Structs {
                receiver: named(A_STRUCT, "a", receiver)?,
                argument: named(B_STRUCT, "b", argument)?,
            },
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The document `select(a.struct.field("k"))`.
    const READS_K: &str = r#"{"version":1,"steps":[{"id":"s","call":{"entry":"LazyFrame.select","arguments":{"exprs":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["a"]}}},"then":[{"entry":"Expr.struct.field","arguments":{"name":{"field":"k"}}}]}}]}}}}]}"#;

    fn text(header: &str) -> String {
        format!("{header}\n{READS_K}")
    }

    #[test]
    fn a_case_that_names_the_fields_of_its_struct_columns_is_read_and_written_as_it_is() {
        let written = text(
            r#"{"a":"Struct(x: Int64, y: String)","a_struct":[["k","Int64"],["l","List(String)"],["n",[["p","Boolean"],["q","Datetime(us, UTC)"]]]],"b":"Struct(x: Int64, y: String)","b_struct":[],"revision":1}"#,
        );
        let case = Case::decode(written.as_bytes()).expect("a case");
        assert_eq!(case.receiver, Representative::Struct);
        assert_eq!(case.argument, Representative::Struct);
        assert_eq!(String::from_utf8(case.encode()).expect("text"), written);
    }

    #[test]
    fn a_case_that_names_no_fields_keeps_the_struct_the_tables_measured() {
        let written = text(r#"{"a":"Struct(x: Int64, y: String)","b":"Int8","revision":1}"#);
        let case = Case::decode(written.as_bytes()).expect("a case");
        assert_eq!(String::from_utf8(case.encode()).expect("text"), written);
    }

    #[test]
    fn fields_named_for_a_column_that_is_no_struct_are_refused_and_the_column_is_named() {
        let written = text(r#"{"a":"Int64","a_struct":[["k","Int64"]],"b":"Int8","revision":1}"#);
        let refusal = Case::decode(written.as_bytes()).expect_err("a refusal");
        assert!(refusal.to_string().contains("a_struct"), "{refusal}");
    }

    #[test]
    fn a_struct_given_one_name_twice_is_refused_and_the_name_is_said() {
        let written = text(
            r#"{"a":"Struct(x: Int64, y: String)","a_struct":[["k","Int64"],["m","String"],["k","Int8"]],"b":"Int8","revision":1}"#,
        );
        let refusal = Case::decode(written.as_bytes()).expect_err("a refusal");
        assert!(refusal.to_string().contains("\"k\""), "{refusal}");
    }

    #[test]
    fn a_field_of_a_dtype_no_field_may_be_is_refused_and_the_dtype_is_said() {
        for kind in [
            r#""Array(Int64, 2)""#,
            r#""Struct(x: Int64, y: String)""#,
            r#""Nothing""#,
        ] {
            let written = text(&format!(
                r#"{{"a":"Struct(x: Int64, y: String)","a_struct":[["k",{kind}]],"b":"Int8","revision":1}}"#
            ));
            let refusal = Case::decode(written.as_bytes()).expect_err("a refusal");
            assert!(
                refusal.to_string().contains(kind.trim_matches('"')),
                "{kind}: {refusal}"
            );
        }
    }
}
