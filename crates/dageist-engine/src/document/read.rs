//! Reading a document from JSON, strictly.
//!
//! The reader refuses what it does not know instead of guessing: a field the
//! document has no place for, a version other than [`VERSION`], an entry the
//! catalogue lacks, a call standing where it cannot, arguments the catalogue
//! refuses and a condition whose links are out of order. Each refusal names
//! its place down to the link of an expression.

use crate::catalog::entry;

use super::VERSION;
use super::dtype::Dtype;
use super::error::{Cause, Position, ReadError, Refusal, Segment};
use super::json::{Json, Number};
use super::model::{Call, Document, Expression, Start, Step, StepId, Value};

impl Document {
    /// Reads a document from JSON text.
    ///
    /// The text is an object of the fields `version`, which is 1, and `steps`.
    /// A step is `{ "id": …, "call": … }` and a call is
    /// `{ "entry": "<catalogue identifier>", "arguments": { "<parameter>": <argument> } }`.
    /// An argument is an object of one key, which says what it is: `expression`,
    /// `columns`, `integer`, `float`, `string`, `flag`, `choice`, `dtype`,
    /// `duration`, `table`, `path`, `pairs` or `many`. An expression is
    /// `{ "start": <call or { "input": "<name>" }>, "then": [<call>, …] }`.
    ///
    /// # Errors
    ///
    /// When the text is not JSON by RFC 8259, repeats a field, nests arrays
    /// and objects deeper than [`MAX_NESTING`](super::MAX_NESTING), or is not
    /// a document in every respect above. A syntax error names its line and
    /// column; a refusal names its place.
    pub fn read(text: &str) -> Result<Self, ReadError> {
        let json = Json::parse(text).map_err(ReadError::Syntax)?;
        read_document(&json).map_err(ReadError::Refused)
    }
}

fn shape(what: &'static str, expected: &'static str, found: &Json) -> Refusal {
    Refusal::new(Cause::Shape {
        what,
        expected,
        found: found.kind(),
    })
}

/// The entries of an object, whatever they are.
fn entries<'a>(what: &'static str, json: &'a Json) -> Result<&'a [(String, Json)], Refusal> {
    match json {
        Json::Object(entries) => Ok(entries),
        other => Err(shape(what, "an object", other)),
    }
}

/// The entries of an object that has only `fields`.
fn object<'a>(
    what: &'static str,
    json: &'a Json,
    fields: &[&str],
) -> Result<&'a [(String, Json)], Refusal> {
    let entries = entries(what, json)?;
    match entries
        .iter()
        .find(|(key, _)| !fields.contains(&key.as_str()))
    {
        Some((unknown, _)) => Err(Refusal::new(Cause::UnknownField(unknown.clone()))),
        None => Ok(entries),
    }
}

fn find<'a>(entries: &'a [(String, Json)], name: &str) -> Option<&'a Json> {
    entries
        .iter()
        .find_map(|(key, value)| (key == name).then_some(value))
}

fn field<'a>(entries: &'a [(String, Json)], name: &'static str) -> Result<&'a Json, Refusal> {
    find(entries, name).ok_or_else(|| Refusal::new(Cause::MissingField(name)))
}

fn string<'a>(what: &'static str, json: &'a Json) -> Result<&'a str, Refusal> {
    match json {
        Json::String(text) => Ok(text),
        other => Err(shape(what, "a string", other)),
    }
}

fn array<'a>(what: &'static str, json: &'a Json) -> Result<&'a [Json], Refusal> {
    match json {
        Json::Array(items) => Ok(items),
        other => Err(shape(what, "an array", other)),
    }
}

fn read_document(json: &Json) -> Result<Document, Refusal> {
    let entries = entries("the document", json)?;
    let version = field(entries, "version")?;
    match version {
        Json::Number(Number::Integer(number)) if *number == i64::from(VERSION) => {}
        other => {
            return Err(Refusal::new(Cause::UnsupportedVersion(other.to_text())));
        }
    }
    let entries = object("the document", json, &["version", "steps"])?;
    let steps = array("steps", field(entries, "steps")?)?
        .iter()
        .enumerate()
        .map(|(at, step)| read_step(at, step))
        .collect::<Result<Vec<_>, _>>()?;
    Document::new(steps)
}

fn read_step(index: usize, json: &Json) -> Result<Step, Refusal> {
    let early = |refusal: Refusal| refusal.in_step(index, None);
    let entries = entries("a step", json).map_err(early)?;
    let id = string("id", field(entries, "id").map_err(early)?).map_err(early)?;
    let id = StepId::new(id).map_err(|_| early(Refusal::new(Cause::EmptyStepId)))?;
    let at = |refusal: Refusal| refusal.in_step(index, Some(id.clone()));
    object("a step", json, &["id", "call"]).map_err(at)?;
    let call = read_call(field(entries, "call").map_err(at)?, Position::Step).map_err(at)?;
    Step::new(id.clone(), call).map_err(at)
}

fn read_call(json: &Json, position: Position) -> Result<Call, Refusal> {
    let fields = object("a call", json, &["entry", "arguments"])?;
    let name = string("entry", field(fields, "entry")?)?;
    let entry = entry(name).ok_or_else(|| Refusal::new(Cause::UnknownEntry(name.to_owned())))?;
    position.check(entry).map_err(Refusal::new)?;
    let given = entries("arguments", field(fields, "arguments")?)?;
    let arguments = given
        .iter()
        .map(|(parameter, argument)| {
            read_value(argument)
                .map(|value| (parameter.as_str(), value))
                .map_err(|refusal| refusal.under(Segment::Parameter(parameter.clone())))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Call::new(entry, arguments)
}

fn strings(what: &'static str, json: &Json) -> Result<Vec<String>, Refusal> {
    array(what, json)?
        .iter()
        .enumerate()
        .map(|(at, item)| {
            string("a column name", item)
                .map(str::to_owned)
                .map_err(|refusal| refusal.under(Segment::Item(at)))
        })
        .collect()
}

fn read_value(json: &Json) -> Result<Value, Refusal> {
    let [(kind, inner)] = entries("an argument", json)? else {
        return Err(Refusal::new(Cause::ArgumentKeys(
            entries("an argument", json)?.len(),
        )));
    };
    Ok(match kind.as_str() {
        "expression" => Value::Expression(read_expression(inner)?),
        "columns" => Value::Columns(strings("columns", inner)?),
        "integer" => match inner {
            Json::Number(Number::Integer(number)) => Value::Integer(*number),
            other => return Err(shape("integer", "a whole number", other)),
        },
        "float" => match inner {
            Json::Number(Number::Float(number)) => Value::Float(*number),
            other => {
                return Err(shape(
                    "float",
                    "a number written with a fraction or an exponent",
                    other,
                ));
            }
        },
        "string" => Value::String(string("string", inner)?.to_owned()),
        "flag" => match inner {
            Json::Bool(flag) => Value::Flag(Some(*flag)),
            Json::Null => Value::Flag(None),
            other => return Err(shape("flag", "true, false or null", other)),
        },
        "choice" => Value::Choice(string("choice", inner)?.to_owned()),
        "dtype" => Value::Dtype(
            string("dtype", inner)?
                .parse::<Dtype>()
                .map_err(|error| Refusal::new(Cause::Dtype(error)))?,
        ),
        "duration" => Value::Duration(string("duration", inner)?.to_owned()),
        "pattern" => Value::Pattern(string("pattern", inner)?.to_owned()),
        "table" => Value::Table(string("table", inner)?.to_owned()),
        "path" => Value::Path(string("path", inner)?.to_owned()),
        "pairs" => Value::Pairs(
            array("pairs", inner)?
                .iter()
                .enumerate()
                .map(|(at, pair)| read_pair(at, pair))
                .collect::<Result<_, _>>()?,
        ),
        "many" => Value::Many(
            array("many", inner)?
                .iter()
                .enumerate()
                .map(|(at, item)| {
                    read_value(item).map_err(|refusal| refusal.under(Segment::Item(at)))
                })
                .collect::<Result<_, _>>()?,
        ),
        unknown => return Err(Refusal::new(Cause::UnknownArgument(unknown.to_owned()))),
    })
}

/// One entry of pairs: an array of a key and a value, each an argument.
fn read_pair(at: usize, json: &Json) -> Result<(Value, Value), Refusal> {
    let [key, value] = array("a pair", json).map_err(|refusal| refusal.under(Segment::Item(at)))?
    else {
        let size = array("a pair", json)?.len();
        return Err(Refusal::new(Cause::PairSize(size)).under(Segment::Item(at)));
    };
    Ok((
        read_value(key).map_err(|refusal| refusal.under(Segment::Key(at)))?,
        read_value(value).map_err(|refusal| refusal.under(Segment::Value(at)))?,
    ))
}

fn read_expression(json: &Json) -> Result<Expression, Refusal> {
    let fields = object("an expression", json, &["start", "then"])?;
    let start =
        read_start(field(fields, "start")?).map_err(|refusal| refusal.under(Segment::Start))?;
    let links = array("then", field(fields, "then")?)?
        .iter()
        .enumerate()
        .map(|(at, link)| {
            read_call(link, Position::Link).map_err(|refusal| refusal.under(Segment::Link(at)))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Expression::new(start, links)
}

/// A start is a call, `{ "input": "<name>" }`, or `{ "hole": null }`.
fn read_start(json: &Json) -> Result<Start, Refusal> {
    if let Json::Object(fields) = json
        && find(fields, "hole").is_some()
        && find(fields, "entry").is_none()
    {
        let fields = object("a hole", json, &["hole"])?;
        return match field(fields, "hole")? {
            Json::Null => Ok(Start::Hole),
            other => Err(shape("hole", "null", other)),
        };
    }
    if let Json::Object(fields) = json
        && find(fields, "input").is_some()
        && find(fields, "entry").is_none()
    {
        let fields = object("an input", json, &["input"])?;
        let name = string("input", field(fields, "input")?)?;
        return Ok(Start::Input(name.to_owned()));
    }
    read_call(json, Position::Start).map(Start::Call)
}
