//! The fields of a struct column that a case names.
//!
//! The tables hold one class for every struct, measured on `Struct(x: Int64, y: String)`, and the
//! walk carries the fields of a struct beside it, read from the schema of the table. A case that is
//! to check what depends on the fields gives Polars a column of fields of its own: the header of the
//! case names them, under `a_struct` and `b_struct` for the columns `a` and `b`, and the sample
//! tables are made with them, so that the walk and Polars see the same schema.
//!
//! A field is a name and a dtype: a scalar, a list of a scalar, or a struct of fields of its own.
//! A column that is a struct and names no fields has the struct the tables measured.

use dageist_engine::allowed::{Fields, Inner, Representative};
use polars::prelude::{DataType, Field as PolarsField};

/// The dtype of one field.
#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    /// A scalar dtype.
    Scalar(Inner),
    /// A list of a scalar dtype.
    List(Inner),
    /// A struct of fields of its own.
    Struct(Fieldset),
}

impl Kind {
    /// The dtype Polars gives a column of this kind.
    pub fn dtype(&self) -> DataType {
        match self {
            Self::Scalar(inner) => inner.class().dtype(),
            Self::List(inner) => Representative::List(*inner).dtype(),
            Self::Struct(fields) => fields.dtype(),
        }
    }

    fn to_json(&self) -> serde_json::Value {
        match self {
            Self::Scalar(inner) => inner.class().name().into(),
            Self::List(inner) => Representative::List(*inner).name().into(),
            Self::Struct(fields) => fields.to_json(),
        }
    }

    fn from_json(value: &serde_json::Value, depth: usize) -> Result<Self, String> {
        match value {
            serde_json::Value::String(text) => match Representative::from_name(text) {
                Some(Representative::List(inner)) => Some(Self::List(inner)),
                Some(class) => Inner::of_class(class).map(Self::Scalar),
                None => None,
            }
            .ok_or_else(|| format!("{text} is no dtype that a field of a case may have")),
            serde_json::Value::Array(_) => {
                if depth >= MOST_DEPTH {
                    return Err(format!(
                        "a struct inside {MOST_DEPTH} structs is deeper than a case goes"
                    ));
                }
                Fieldset::read(value, depth + 1).map(Self::Struct)
            }
            other => Err(format!(
                "{other} is no dtype that a field of a case may have"
            )),
        }
    }
}

/// How many structs may hold one another in a case.
const MOST_DEPTH: usize = 3;

/// One field: a name and a dtype.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    name: String,
    kind: Kind,
}

impl Field {
    pub fn new(name: impl Into<String>, kind: Kind) -> Self {
        Self {
            name: name.into(),
            kind,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub const fn kind(&self) -> &Kind {
        &self.kind
    }
}

/// The fields of a struct, in order, of which no two have one name, which Polars refuses.
#[derive(Debug, Clone, PartialEq)]
pub struct Fieldset(Vec<Field>);

impl Fieldset {
    /// The fields `fields`, in order.
    ///
    /// # Errors
    ///
    /// The first name that two of them share.
    pub fn new(fields: Vec<Field>) -> Result<Self, String> {
        for (at, field) in fields.iter().enumerate() {
            if fields[..at].iter().any(|before| before.name == field.name) {
                return Err(format!("{:?} is the name of two fields", field.name));
            }
        }
        Ok(Self(fields))
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Field> {
        self.0.iter()
    }

    pub const fn len(&self) -> usize {
        self.0.len()
    }

    /// The dtype Polars gives a column of these fields.
    pub fn dtype(&self) -> DataType {
        DataType::Struct(
            self.0
                .iter()
                .map(|field| PolarsField::new(field.name.as_str().into(), field.kind.dtype()))
                .collect(),
        )
    }

    /// The fields as the walk carries them, which it reads from the dtype.
    pub fn as_walked(&self) -> Option<Fields> {
        Fields::of_dtype(&self.dtype())
    }

    /// The names of the fields, and of the fields of the structs they are, in order.
    pub fn names(&self) -> Vec<&str> {
        let mut names = Vec::new();
        for field in &self.0 {
            names.push(field.name());
            if let Kind::Struct(inner) = field.kind() {
                names.extend(inner.names());
            }
        }
        names
    }

    /// The fields as a header writes them: each a name and its dtype, a struct as its fields.
    pub fn to_json(&self) -> serde_json::Value {
        self.0
            .iter()
            .map(|field| {
                serde_json::Value::Array(vec![field.name.clone().into(), field.kind.to_json()])
            })
            .collect::<Vec<_>>()
            .into()
    }

    /// The fields a header writes.
    ///
    /// # Errors
    ///
    /// What is wrong with them: a field that is not a name and a dtype, a dtype no field may have,
    /// or a name that two fields share.
    pub fn from_json(value: &serde_json::Value) -> Result<Self, String> {
        Self::read(value, 0)
    }

    fn read(value: &serde_json::Value, depth: usize) -> Result<Self, String> {
        let items = value
            .as_array()
            .ok_or_else(|| format!("{value} is no list of fields"))?;
        let fields = items
            .iter()
            .map(|item| match item.as_array().map(Vec::as_slice) {
                Some([serde_json::Value::String(name), kind]) => {
                    Ok(Field::new(name.clone(), Kind::from_json(kind, depth)?))
                }
                _ => Err(format!("{item} is no field, which is a name and a dtype")),
            })
            .collect::<Result<Vec<_>, String>>()?;
        Self::new(fields)
    }
}

/// The fields of the struct columns of the tables a case runs on: those of `a` and of `b`, for the
/// column that is a struct and names them. A struct column that names none has the struct the tables
/// measured.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Structs {
    pub receiver: Option<Fieldset>,
    pub argument: Option<Fieldset>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_read_back_as_they_were_written_at_every_depth() {
        let inner = Fieldset::new(vec![
            Field::new("p", Kind::Scalar(Inner::Boolean)),
            Field::new("q", Kind::List(Inner::Int8)),
        ])
        .expect("fields");
        let set = Fieldset::new(vec![
            Field::new("k", Kind::Scalar(Inner::DatetimeZoned)),
            Field::new("n", Kind::Struct(inner)),
            Field::new("z", Kind::Scalar(Inner::Decimal)),
        ])
        .expect("fields");
        assert_eq!(Fieldset::from_json(&set.to_json()), Ok(set.clone()));
        assert_eq!(set.names(), ["k", "n", "p", "q", "z"]);
        assert_eq!(
            Fieldset::from_json(&Fieldset::new(vec![]).unwrap().to_json())
                .unwrap()
                .len(),
            0
        );
    }

    #[test]
    fn the_walk_reads_the_dtype_of_each_field_as_polars_names_it() {
        let set = Fieldset::new(vec![
            Field::new("k", Kind::Scalar(Inner::Int64)),
            Field::new("l", Kind::List(Inner::String)),
        ])
        .expect("fields");
        let walked = set.as_walked().expect("the walk reads the fields");
        let names: Vec<&str> = walked.names().collect();
        assert_eq!(names, ["k", "l"]);
        assert_eq!(
            walked.get("k").and_then(|field| field.class()),
            Some(Representative::Int64)
        );
        assert_eq!(
            walked.get("l").and_then(|field| field.class()),
            Some(Representative::List(Inner::String))
        );
    }

    #[test]
    fn a_dtype_no_field_may_be_or_a_name_twice_is_refused() {
        for kind in ["Array(Int64, 2)", "Struct(x: Int64, y: String)", "Nothing"] {
            let written: serde_json::Value = serde_json::json!([["k", kind]]);
            let refusal = Fieldset::from_json(&written).unwrap_err();
            assert!(refusal.contains(kind), "{refusal}");
        }
        let twice = serde_json::json!([["k", "Int8"], ["k", "Int16"]]);
        assert!(Fieldset::from_json(&twice).unwrap_err().contains("\"k\""));
        assert!(Fieldset::from_json(&serde_json::json!([["k"]])).is_err());
        assert!(Fieldset::from_json(&serde_json::json!("k")).is_err());
        let mut deep = serde_json::json!([["leaf", "Int8"]]);
        for _ in 0..MOST_DEPTH + 1 {
            deep = serde_json::json!([["inner", deep]]);
        }
        assert!(Fieldset::from_json(&deep).unwrap_err().contains("deeper"));
    }
}
