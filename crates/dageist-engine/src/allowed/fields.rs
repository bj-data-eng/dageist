//! What a struct holds, and what the calls that read, name and make structs say of it.
//!
//! The tables hold one class for every struct, [`Representative::Struct`], measured on a struct of
//! two fields, so what depends on the fields is not the tables': it is worked out here from the
//! fields the walk carries, the way an array's width and a moment's zone are carried beside the
//! class. A struct's fields are the names in order and the dtype of each, as far as they are
//! known; a field whose dtype no class of the tables covers has no class. Where the walk lost the
//! fields it carries none, and says so, as it does for any dtype it does not know.

use polars::prelude::DataType;

use crate::catalog::Entry;

use super::dtype::{Representative, ResultDtype, dtype_name};
use super::offers::Chosen;
use super::rules::Rule;

/// How many fields a struct the walk makes holds at most: a call that makes more, as `splitn`
/// does of a number far beyond any text, gives a struct whose fields are not carried.
const MOST_FIELDS: usize = 4096;

/// One field of a struct: its name, and its dtype as far as it is known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    name: String,
    /// The dtype as the tables write it, none where it is not known.
    written: Option<String>,
    /// The class of the dtype, none where the tables cover none.
    class: Option<Representative>,
    /// What a field that is a struct holds, where that is known.
    fields: Option<Fields>,
    /// Whether the field was made of a number typed without a dtype, which Polars keeps open: what
    /// reads the field and meets another number gives the dtype of that number, not the one the
    /// struct is written with.
    loose: bool,
    /// The width of the array the field is, where it is one whose width is known.
    width: Option<usize>,
}

impl Field {
    /// The field `name` of the dtype `dtype`, which Polars gave: written as Polars names it.
    #[must_use]
    pub fn of_dtype(name: impl Into<String>, dtype: &DataType) -> Self {
        Self {
            name: name.into(),
            written: Some(dtype_name(dtype)),
            class: Representative::of(dtype),
            fields: Fields::of_dtype(dtype),
            loose: false,
            width: match dtype {
                DataType::Array(_, width) => Some(*width),
                _ => None,
            },
        }
    }

    /// The field `name` of a class the walk knows, or of none where it does not: written as the
    /// tables write the class.
    #[must_use]
    pub fn of_class(name: impl Into<String>, class: Option<Representative>) -> Self {
        Self {
            name: name.into(),
            written: class.map(|class| class.name().to_owned()),
            class,
            fields: None,
            loose: false,
            width: None,
        }
    }

    /// Whether the field was made of a number typed without a dtype, which stays open.
    pub(super) const fn is_loose(&self) -> bool {
        self.loose
    }

    /// The width of the array the field is, where it is known.
    #[must_use]
    pub const fn width(&self) -> Option<usize> {
        self.width
    }

    /// The field's name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The class of the field's dtype, none where the tables cover none.
    #[must_use]
    pub const fn class(&self) -> Option<Representative> {
        self.class
    }

    /// The dtype as the tables write it, and `?` where it is not known, which is how it is shown.
    #[must_use]
    pub fn written(&self) -> &str {
        self.written.as_deref().unwrap_or("?")
    }

    /// What the field holds where it is a struct and that is known.
    #[must_use]
    pub const fn fields(&self) -> Option<&Fields> {
        self.fields.as_ref()
    }

    /// The same field under another name.
    fn named(&self, name: &str) -> Self {
        Self {
            name: name.to_owned(),
            ..self.clone()
        }
    }
}

/// A name that two fields of a struct would have, which Polars refuses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepeatedName(String);

impl RepeatedName {
    /// The name two fields would have.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for RepeatedName {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "two fields would be named {:?}", self.0)
    }
}

impl std::error::Error for RepeatedName {}

/// The fields of a struct, in order. No two of them have one name, which Polars refuses.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fields(Vec<Field>);

impl Fields {
    /// The fields of `dtype`, if it is a struct.
    #[must_use]
    pub fn of_dtype(dtype: &DataType) -> Option<Self> {
        let DataType::Struct(fields) = dtype else {
            return None;
        };
        let fields = fields
            .iter()
            .map(|field| Field::of_dtype(field.name().as_str(), field.dtype()))
            .collect();
        Self::new(fields).ok()
    }

    /// The fields `fields`, in order.
    ///
    /// # Errors
    ///
    /// The first name that two of them share, which Polars refuses.
    pub fn new(fields: Vec<Field>) -> Result<Self, RepeatedName> {
        for (at, field) in fields.iter().enumerate() {
            if fields[..at].iter().any(|before| before.name == field.name) {
                return Err(RepeatedName(field.name.clone()));
            }
        }
        Ok(Self(fields))
    }

    /// The fields, in order.
    pub fn iter(&self) -> std::slice::Iter<'_, Field> {
        self.0.iter()
    }

    /// How many fields there are.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there is none.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The field called `name`.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Field> {
        self.0.iter().find(|field| field.name == name)
    }

    /// Whether a field was made of a number typed without a dtype, which stays open.
    pub(super) fn has_a_loose_field(&self) -> bool {
        self.0.iter().any(|field| {
            field.is_loose() || field.fields.as_ref().is_some_and(Fields::has_a_loose_field)
        })
    }

    /// The names of the fields, in order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(Field::name)
    }

    /// The struct as the tables write the dtype of it: `Struct(x: Int64, y: String)`.
    #[must_use]
    pub fn written(&self) -> String {
        let fields: Vec<String> = self
            .0
            .iter()
            .map(|field| format!("{}: {}", field.name, field.written()))
            .collect();
        format!("Struct({})", fields.join(", "))
    }

    /// The names of the fields as a sentence says them: `x and y`, or `none`.
    #[must_use]
    pub fn listed(&self) -> String {
        let names: Vec<String> = self.names().map(|name| format!("{name:?}")).collect();
        match names.as_slice() {
            [] => "none".to_owned(),
            [only] => only.clone(),
            [first @ .., last] => format!("{} and {last}", first.join(", ")),
        }
    }
}

impl<'a> IntoIterator for &'a Fields {
    type Item = &'a Field;
    type IntoIter = std::slice::Iter<'a, Field>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

/// One input of a call that makes a struct of what it is given, as `polars.struct` does: the name it
/// writes, where that is known, and the dtype it has, where that is.
#[derive(Debug, Clone, Copy)]
pub struct Input<'a> {
    name: Option<&'a str>,
    class: Option<Representative>,
    fields: Option<&'a Fields>,
    loose: bool,
    width: Option<usize>,
}

impl<'a> Input<'a> {
    /// An input that writes the name `name`, if it is known, and is of the class `class`, with the
    /// fields `fields` where it is a struct whose fields are known.
    #[must_use]
    pub const fn new(
        name: Option<&'a str>,
        class: Option<Representative>,
        fields: Option<&'a Fields>,
    ) -> Self {
        Self {
            name,
            class,
            fields,
            loose: false,
            width: None,
        }
    }

    /// The same input, which is an array of the width `width` where that is known.
    #[must_use]
    pub const fn with_width(mut self, width: Option<usize>) -> Self {
        self.width = width;
        self
    }

    /// Whether the input is a number typed without a dtype, or a struct that holds one.
    pub(super) fn is_loose(&self) -> bool {
        self.loose || self.fields.is_some_and(Fields::has_a_loose_field)
    }

    /// The same input, which is a number typed without a dtype where `loose`.
    #[must_use]
    pub const fn loose_if(mut self, loose: bool) -> Self {
        self.loose = loose;
        self
    }
}

/// What a call gives of a struct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Gives {
    /// The call decides nothing of fields: it gives what the tables say, and keeps the fields of the
    /// struct it follows where it gives one.
    Nothing,
    /// A struct, whose fields are known or not.
    Struct(Option<Fields>),
    /// One value, the field of a struct that was read, or none where the walk does not know it.
    Field(Option<Field>),
    /// One column for each field, which an unnest writes.
    Columns(Option<Fields>),
    /// A struct the call gives, whose dtype the call does not yet say: the pattern that names the
    /// fields of `extract_groups` is not given.
    Unknown,
    /// A call that reads, drops or renames fields of a struct whose fields the chain does not know:
    /// whether Polars refuses the names it is given depends on them, so the walk cannot say, and
    /// neither can it say what the call gives.
    Undecided,
}

/// The refusal of a call: the rule and what it says of the case.
pub(super) type Said = (Rule, String);

/// The names an option names, as the walk gives them: the texts the call is given for the parameter.
fn given<'a>(chosen: &Chosen<'a>, parameter: &str) -> Vec<&'a str> {
    chosen
        .names
        .iter()
        .filter(|(name, _)| *name == parameter)
        .map(|(_, text)| *text)
        .collect()
}

/// The word a flag is set to, if it is set.
fn word<'a>(chosen: &Chosen<'a>, parameter: &str) -> Option<&'a str> {
    chosen
        .options
        .iter()
        .find(|(name, _)| *name == parameter)
        .map(|(_, word)| *word)
}

/// The whole number a call is given for `parameter`.
fn number(chosen: &Chosen<'_>, parameter: &str) -> Option<i64> {
    chosen
        .numbers
        .iter()
        .find(|(name, _)| *name == parameter)
        .map(|(_, number)| *number)
}

/// Fields `field_0` and on, each of text, `count` of them.
fn text_fields(count: i64) -> Option<Fields> {
    let count = usize::try_from(count)
        .ok()
        .filter(|count| *count <= MOST_FIELDS)?;
    let fields = (0..count)
        .map(|at| Field::of_class(format!("field_{at}"), Some(Representative::String)))
        .collect();
    Fields::new(fields).ok()
}

/// Fields that a list or an array is made, named `names`, each of the class `class`: no two names
/// are alike, which Polars accepts and the rule `to-struct-names-are-distinct` refuses.
fn named_fields(names: &[&str], class: Option<Representative>) -> Result<Fields, Said> {
    let fields = names
        .iter()
        .map(|name| Field::of_class(*name, class))
        .collect();
    Fields::new(fields).map_err(|error| {
        (
            Rule::ToStructNamesAreDistinct,
            format!("two fields would be named {:?}", error.name()),
        )
    })
}

fn repeated(name: &str) -> Said {
    (
        Rule::FieldNamesFitTheStruct,
        format!("two fields would be named {name:?}"),
    )
}

/// What the call of `entry` gives of a struct, after a value of the class `receiver`, from what it
/// was chosen to follow and be given, or why a rule refuses it.
///
/// # Errors
///
/// The rule that refuses the call, with what it says of the case.
pub(super) fn gives(
    entry: &Entry,
    receiver: Option<Representative>,
    chosen: &Chosen<'_>,
) -> Result<Gives, Said> {
    let held = chosen.fields;
    let follows_a_struct = receiver == Some(Representative::Struct);
    match entry.id().as_str() {
        "Expr.struct.field" if follows_a_struct => {
            let names: Vec<&str> = given(chosen, "name")
                .into_iter()
                .chain(given(chosen, "more_names"))
                .collect();
            let Some(fields) = held else {
                return Ok(Gives::Undecided);
            };
            let mut read = Vec::new();
            for name in &names {
                let Some(field) = fields.get(name) else {
                    return Err(not_a_field(name, fields));
                };
                read.push(field.clone());
            }
            Ok(match read.as_slice() {
                [only] => Gives::Field(Some(only.clone())),
                _ => Gives::Columns(Fields::new(read).ok()),
            })
        }
        "Expr.fill_null" if follows_a_struct => match word(chosen, "strategy") {
            Some(strategy @ ("min" | "max" | "mean" | "zero" | "one")) => Err((
                Rule::StructIsFilledForwardOrBackward,
                format!("a struct is filled by {strategy}"),
            )),
            _ => Ok(Gives::Nothing),
        },
        "Expr.struct.drop" if follows_a_struct => {
            let names = given(chosen, "names");
            let Some(fields) = held else {
                // Names that are no field are left alone where the drop is not strict.
                return Ok(if word(chosen, "strict") == Some("false") {
                    Gives::Struct(None)
                } else {
                    Gives::Undecided
                });
            };
            if word(chosen, "strict") != Some("false")
                && let Some(name) = names.iter().find(|name| fields.get(name).is_none())
            {
                return Err(not_a_field(name, fields));
            }
            let kept = fields
                .iter()
                .filter(|field| !names.contains(&field.name()))
                .cloned()
                .collect();
            Ok(Gives::Struct(Fields::new(kept).ok()))
        }
        "Expr.struct.rename_fields" if follows_a_struct => {
            let new = given(chosen, "names");
            let Some(fields) = held else {
                return Ok(Gives::Undecided);
            };
            if new.len() != fields.len() {
                return Err((
                    Rule::FieldNamesFitTheStruct,
                    format!(
                        "{} names are given to a struct of {} fields, {}",
                        new.len(),
                        fields.len(),
                        fields.listed()
                    ),
                ));
            }
            let renamed = fields
                .iter()
                .zip(&new)
                .map(|(field, name)| field.named(name))
                .collect();
            Fields::new(renamed)
                .map(|fields| Gives::Struct(Some(fields)))
                .map_err(|error| repeated(error.name()))
        }
        "Expr.struct.unnest" if follows_a_struct => Ok(match held {
            // A struct of one field is one column, as the field it holds is read.
            Some(fields) => match fields.0.as_slice() {
                [only] => Gives::Field(Some(only.clone())),
                _ => Gives::Columns(Some(fields.clone())),
            },
            None => Gives::Columns(None),
        }),
        "polars.struct" => {
            let made: Option<Vec<Field>> = chosen
                .inputs
                .iter()
                .map(|input| {
                    input.name.map(|name| {
                        let mut field = Field::of_class(name, input.class);
                        field.loose = input.loose;
                        if let (Some(width), Some(Representative::Array(_))) =
                            (input.width, input.class)
                        {
                            field.width = Some(width);
                            field.written = Some(
                                ResultDtype::new(field.written().to_owned(), input.class)
                                    .of_width(width)
                                    .to_string(),
                            );
                        }
                        if let (Some(fields), Some(Representative::Struct)) =
                            (input.fields, input.class)
                        {
                            field.written = Some(fields.written());
                            field.fields = Some(fields.clone());
                        }
                        field
                    })
                })
                .collect();
            match made.map(Fields::new) {
                Some(Ok(fields)) => Ok(Gives::Struct(Some(fields))),
                Some(Err(error)) => Err(repeated(error.name())),
                // A value whose name the chain does not know, as the columns an unnest writes,
                // may be named as another is, which Polars refuses; two whose names are known and
                // alike are refused whatever it is named.
                None => {
                    let mut seen: Vec<&str> = Vec::new();
                    for name in chosen.inputs.iter().filter_map(|input| input.name) {
                        if seen.contains(&name) {
                            return Err(repeated(name));
                        }
                        seen.push(name);
                    }
                    Ok(Gives::Undecided)
                }
            }
        }
        "Expr.str.extract_groups" => {
            let pattern = chosen
                .patterns
                .iter()
                .find(|(name, _)| *name == "pattern")
                .map(|(_, text)| *text);
            Ok(match pattern {
                // The groups of a pattern are fields of text, named as the groups are.
                Some(text) => Gives::Struct(
                    crate::catalog::groups_dtype(text)
                        .ok()
                        .and_then(|dtype| Fields::of_dtype(&dtype)),
                ),
                None => Gives::Unknown,
            })
        }
        "Expr.str.split_exact" => {
            let fields = number(chosen, "n").and_then(|parts| text_fields(parts.saturating_add(1)));
            Ok(Gives::Struct(fields))
        }
        "Expr.str.splitn" => Ok(Gives::Struct(number(chosen, "n").and_then(text_fields))),
        "Expr.list.to_struct" => {
            let names = given(chosen, "fields");
            let element = receiver
                .and_then(Representative::element)
                .map(|inner| inner.class());
            named_fields(&names, element).map(|fields| Gives::Struct(Some(fields)))
        }
        "Expr.arr.to_struct" => {
            let element = receiver
                .and_then(Representative::element)
                .map(|inner| inner.class());
            let names = given(chosen, "fields");
            if names.is_empty() {
                // Named after their places, which the width of the array says.
                let Some(width) = chosen.width.filter(|width| *width <= MOST_FIELDS) else {
                    return Ok(Gives::Struct(None));
                };
                let fields = (0..width)
                    .map(|at| Field::of_class(format!("field_{at}"), element))
                    .collect();
                return Ok(Gives::Struct(Fields::new(fields).ok()));
            }
            // Names are counted against the width of the array, which Polars knows and the walk may
            // not: they are no more judged than the fields they would name are known.
            let Some(width) = chosen.width else {
                return Ok(Gives::Undecided);
            };
            if width != names.len() {
                return Err((
                    Rule::FieldNamesFitTheStruct,
                    format!(
                        "{} names are given to an array of {width} places",
                        names.len()
                    ),
                ));
            }
            named_fields(&names, element).map(|fields| Gives::Struct(Some(fields)))
        }
        "Expr.value_counts" => {
            let Some(name) = chosen.output_name else {
                // A count given a name may be named as the values are, which a pivot's group does
                // not say, and Polars refuses two fields of one name.
                return Ok(if given(chosen, "name").is_empty() {
                    Gives::Struct(None)
                } else {
                    Gives::Undecided
                });
            };
            let counted = given(chosen, "name").first().copied().unwrap_or(
                if word(chosen, "normalize") == Some("true") {
                    "proportion"
                } else {
                    "count"
                },
            );
            // The values keep what the receiver is: a number left open, the fields of a struct, with
            // a number left open in it, and the width of an array.
            let mut values = Field::of_class(name, receiver);
            values.loose = chosen.follows_an_open_number;
            if receiver == Some(Representative::Struct)
                && let Some(held) = chosen.fields
            {
                values.written = Some(held.written());
                values.fields = Some(held.clone());
            }
            if let (Some(width), Some(Representative::Array(_))) = (chosen.width, receiver) {
                values.width = Some(width);
                values.written = Some(
                    ResultDtype::new(values.written().to_owned(), receiver)
                        .of_width(width)
                        .to_string(),
                );
            }
            let counts = Field::of_class(
                counted,
                Some(if word(chosen, "normalize") == Some("true") {
                    Representative::Float64
                } else {
                    Representative::UInt32
                }),
            );
            Fields::new(vec![values, counts])
                .map(|fields| Gives::Struct(Some(fields)))
                .map_err(|error| repeated(error.name()))
        }
        _ => Ok(Gives::Nothing),
    }
}

/// Why Polars always refuses a call of arithmetic that follows a struct, or is given one, from what
/// the walk carries: the five calls it does not do on a struct at all, and the operators on a struct
/// that has a field that is no number. `None` for the rest, which it does irregularly, field by field.
pub(super) fn arithmetic_refused(
    entry: &Entry,
    receiver: Option<Representative>,
    chosen: &Chosen<'_>,
) -> Option<String> {
    let id = entry.id().as_str();
    let struct_receiver = receiver == Some(Representative::Struct);
    let struct_argument = chosen
        .arguments
        .iter()
        .any(|(_, class)| *class == Representative::Struct);
    let symbol = match id {
        "Expr.add" => "+",
        "Expr.sub" => "-",
        "Expr.mul" => "*",
        "Expr.truediv" => "/",
        "Expr.floordiv" => "//",
        "Expr.mod" => "%",
        "Expr.abs" | "Expr.neg" | "Expr.sign" | "Expr.clip" if struct_receiver => {
            return Some(format!(
                "`{}` is not done on a struct, whatever its fields are",
                id.trim_start_matches("Expr.")
            ));
        }
        "Expr.pow" if struct_receiver || struct_argument => {
            return Some("a struct is neither raised to a power nor a power of one".to_owned());
        }
        _ => return None,
    };
    // A struct that meets a struct is done field by field, and a sum with a text is one Polars does.
    if struct_receiver && struct_argument {
        return None;
    }
    // Only a number meeting it is a case Polars refuses always: a text, a null and the rest are
    // met field by field, which it does irregularly.
    let is_a_number = |class: &Representative| {
        matches!(
            class,
            Representative::Int8
                | Representative::Int16
                | Representative::Int32
                | Representative::Int64
                | Representative::Int128
                | Representative::UInt8
                | Representative::UInt16
                | Representative::UInt32
                | Representative::UInt64
                | Representative::UInt128
                | Representative::Float16
                | Representative::Float32
                | Representative::Float64
                | Representative::Decimal
                | Representative::Boolean
        )
    };
    let met_by_a_number = chosen.arguments.iter().any(|(_, class)| is_a_number(class))
        || !chosen.open_numbers.is_empty()
        || (!struct_receiver && struct_argument && receiver.as_ref().is_some_and(is_a_number));
    if !met_by_a_number {
        return None;
    }
    let sides = chosen
        .fields
        .filter(|_| struct_receiver)
        .into_iter()
        .chain(chosen.argument_fields.iter().map(|(_, fields)| *fields));
    sides
        .flat_map(Fields::iter)
        .find(|field| {
            matches!(
                field.class,
                Some(
                    Representative::String
                        | Representative::Date
                        | Representative::Time
                        | Representative::Datetime
                        | Representative::DatetimeZoned
                )
            )
        })
        .map(|field| {
            format!(
                "cannot {symbol} a struct with non-numeric field: (field: {}, dtype: {})",
                field.name,
                field.written()
            )
        })
}

/// The refusal of a name that is no field of `fields`.
fn not_a_field(name: &str, fields: &Fields) -> Said {
    (
        Rule::FieldIsOfTheStruct,
        format!(
            "{name:?} is no field of the struct, whose fields are {}",
            fields.listed()
        ),
    )
}

impl Gives {
    /// The dtype a call gives, and the fields of the struct it gives, where it decides them.
    pub(super) fn result(&self) -> Option<(Option<ResultDtype>, Option<Fields>)> {
        Some(match self {
            Self::Nothing => return None,
            Self::Struct(Some(fields)) => (
                Some(ResultDtype::new(
                    fields.written(),
                    Some(Representative::Struct),
                )),
                Some(fields.clone()),
            ),
            Self::Struct(None) => (
                Some(ResultDtype::new(
                    "Struct".to_owned(),
                    Some(Representative::Struct),
                )),
                None,
            ),
            Self::Field(Some(field)) => (Some(ResultDtype::of_field(field)), field.fields.clone()),
            Self::Field(None) | Self::Columns(_) | Self::Unknown | Self::Undecided => (None, None),
        })
    }

    /// Whether the call reads a field made of a number typed without a dtype.
    pub(super) fn reads_a_loose_field(&self) -> bool {
        matches!(self, Self::Field(Some(field)) if field.is_loose())
    }

    /// The width of the array a field read holds, where it is known.
    pub(super) fn width(&self) -> Option<usize> {
        match self {
            Self::Field(Some(field)) => field.width,
            _ => None,
        }
    }

    /// Whether the call reads or renames fields that the chain does not know, so that what Polars
    /// says of it cannot be said.
    pub(super) const fn is_undecided(&self) -> bool {
        matches!(self, Self::Undecided)
    }

    /// The columns an unnest gives, where it gives several and they are known.
    pub(super) const fn columns(&self) -> Option<&Fields> {
        match self {
            Self::Columns(columns) => columns.as_ref(),
            _ => None,
        }
    }
}

impl ResultDtype {
    /// The dtype of a field, as the tables write it.
    fn of_field(field: &Field) -> Self {
        // A number left open meets another as that one's dtype, which the tables do not say.
        Self::new(
            field.written().to_owned(),
            field.class.filter(|_| !field.loose),
        )
    }
}

/// The result a struct-keeping call gives: the struct the call follows, where its fields are known.
pub(super) fn kept(fields: Option<&Fields>) -> (ResultDtype, Option<Fields>) {
    match fields {
        Some(fields) => (
            ResultDtype::new(fields.written(), Some(Representative::Struct)),
            Some(fields.clone()),
        ),
        None => (
            ResultDtype::new("Struct".to_owned(), Some(Representative::Struct)),
            None,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_fields_of_one_name_are_refused_with_the_name() {
        let fields = vec![
            Field::of_class("x", None),
            Field::of_class("y", None),
            Field::of_class("x", None),
        ];

        let error = Fields::new(fields).expect_err("a name repeated is refused");

        assert_eq!(error.name(), "x");
        assert_eq!(error.to_string(), "two fields would be named \"x\"");
    }
}
