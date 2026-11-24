//! The tables of allowed combinations as text: their lines, and reading them.
//!
//! Each table is a plain text artifact under `api/catalogue/`, one combination
//! to a line, sorted, with `#` lines of header above. A line's fields are
//! separated by ` | `, and the last field is free text, so a reason may hold
//! any character. The engine embeds the committed files, so answering reads no
//! data and runs no Polars.

use std::collections::HashMap;
use std::fmt;
use std::sync::LazyLock;

use crate::catalog::{self, Applies};

use super::context::Context;
use super::dtype::{Representative, ResultDtype};
use super::rules::Rule;

/// The committed dtype table.
pub(super) const DTYPES: &str = include_str!("../../../../api/catalogue/dtypes.txt");
/// The committed context table.
pub(super) const CONTEXTS: &str = include_str!("../../../../api/catalogue/contexts.txt");
/// The committed option table.
pub(super) const OPTIONS: &str = include_str!("../../../../api/catalogue/options.txt");
/// The committed table of counts.
pub(super) const COUNTS: &str = include_str!("../../../../api/catalogue/counts.txt");

/// How many values a chain gives for the values of one group, or of the whole
/// table: measured on three rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Shape {
    /// One value: the chain reduces.
    One,
    /// A value for every row: as many values as the table, or the group, has rows.
    Many,
    /// Another number of values, which the data or the arguments decide: the rows that pass a
    /// filter, the first `n` of them, the places of a range. It is neither one value nor the
    /// table's rows, so a place that takes only those refuses it. [`Count`](super::Count) says what is known
    /// of the number.
    Counted,
}

impl Shape {
    /// Every shape.
    pub const ALL: [Self; 3] = [Self::One, Self::Many, Self::Counted];

    /// The word the tables use.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::One => "one",
            Self::Many => "many",
            Self::Counted => "counted",
        }
    }

    /// The shape the tables write as `word`.
    #[must_use]
    pub fn from_name(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|shape| shape.name() == word)
    }
}

/// Where Polars refused a combination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stage {
    /// When it built the plan or worked out its schema, before any data.
    Schema,
    /// When it ran on a few rows, though its schema was fine. This is a
    /// mistake the schema alone does not show.
    Rows,
    /// When it ran on a table with no rows, though it ran on rows.
    Empty,
}

impl Stage {
    /// Every stage, in the order Polars meets them.
    pub const ALL: [Self; 3] = [Self::Schema, Self::Rows, Self::Empty];

    /// The word the tables use.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Schema => "schema",
            Self::Rows => "rows",
            Self::Empty => "empty",
        }
    }
}

/// What an allowed combination gives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Produces {
    /// The dtype of the value.
    pub result: ResultDtype,
    /// How many values it is.
    pub shape: Shape,
}

/// Why Polars refused a combination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reason {
    /// Where Polars refused it.
    pub stage: Stage,
    /// Polars' message, with its context cut off and the whitespace of its
    /// lines made single spaces.
    pub text: String,
}

/// What Polars did with a combination, built on a few rows including a null
/// and on an empty table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The schema resolved and the rows ran. A step gives no single value, so
    /// it produces nothing.
    Allowed(Option<Produces>),
    /// Polars refused it.
    Refused(Reason),
}

impl fmt::Display for Outcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Allowed(None) => formatter.write_str("allowed -"),
            Self::Allowed(Some(produces)) => write!(
                formatter,
                "allowed {} {}",
                produces.result,
                produces.shape.name()
            ),
            Self::Refused(reason) => {
                write!(
                    formatter,
                    "refused {}: {}",
                    reason.stage.name(),
                    reason.text
                )
            }
        }
    }
}

impl Outcome {
    fn parse(text: &str) -> Result<Self, String> {
        if let Some(rest) = text.strip_prefix("allowed ") {
            if rest == "-" {
                return Ok(Self::Allowed(None));
            }
            let (result, shape) = rest
                .rsplit_once(' ')
                .ok_or_else(|| format!("{text:?} gives no shape"))?;
            let shape =
                Shape::from_name(shape).ok_or_else(|| format!("{shape:?} is not a shape"))?;
            return Ok(Self::Allowed(Some(Produces {
                result: ResultDtype::from_name(result),
                shape,
            })));
        }
        let rest = text
            .strip_prefix("refused ")
            .ok_or_else(|| format!("{text:?} is neither allowed nor refused"))?;
        let (stage, reason) = rest
            .split_once(": ")
            .ok_or_else(|| format!("{text:?} gives no reason"))?;
        let stage = match stage {
            "schema" => Stage::Schema,
            "rows" => Stage::Rows,
            "empty" => Stage::Empty,
            other => return Err(format!("{other:?} is not a stage")),
        };
        Ok(Self::Refused(Reason {
            stage,
            text: reason.to_owned(),
        }))
    }
}

/// An entry and parameter of the catalogue, as `&'static str`s.
fn entry_named(name: &str) -> Result<&'static catalog::Entry, String> {
    catalog::entry(name).ok_or_else(|| format!("{name:?} is not an entry of the catalogue"))
}

fn parameter_named(entry: &'static catalog::Entry, name: &str) -> Result<&'static str, String> {
    entry
        .parameters()
        .iter()
        .find(|parameter| parameter.name() == name)
        .map(catalog::Parameter::name)
        .ok_or_else(|| format!("{name:?} is not a parameter of {}", entry.id().as_str()))
}

/// The text that follows an entry on a line of the context table where it was measured in the
/// form that an option gives, such as `[strategy=forward]`.
#[cfg(test)]
fn render_form(form: &[OptionValue]) -> String {
    if form.is_empty() {
        return String::new();
    }
    let options: Vec<String> = form
        .iter()
        .map(|(name, word)| format!("{name}={word}"))
        .collect();
    format!("[{}]", options.join(", "))
}

/// An entry and the form it was measured in, as the first field of a line of the context table
/// writes them: `Expr.fill_null`, or `Expr.fill_null[strategy=forward]`.
fn entry_and_form(field: &str) -> Result<(&'static catalog::Entry, Vec<OptionValue>), String> {
    let Some((name, rest)) = field.split_once('[') else {
        return Ok((entry_named(field)?, Vec::new()));
    };
    let entry = entry_named(name)?;
    let options = rest
        .strip_suffix(']')
        .ok_or_else(|| format!("{field:?} does not close its form"))?;
    let form = split_options(options)
        .into_iter()
        .map(|option| {
            let (name, word) = option
                .split_once('=')
                .ok_or_else(|| format!("{option:?} is not name=value"))?;
            Ok((parameter_named(entry, name)?, word.to_owned()))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok((entry, form))
}

/// The options a field writes, each as `name=word`, apart. A word may hold a comma, as the name
/// of a dtype does (`Datetime(us, UTC)`), so a comma separates two options only where what
/// follows it is the name of one, written `name=`.
fn split_options(field: &str) -> Vec<&str> {
    let mut options = Vec::new();
    let mut start = 0;
    let bytes = field.as_bytes();
    for at in 0..bytes.len() {
        if !field[at..].starts_with(", ") {
            continue;
        }
        let rest = &field[at + 2..];
        let name = rest.find('=').is_some_and(|end| {
            end > 0
                && rest[..end]
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b == b'_')
        });
        if name {
            options.push(&field[start..at]);
            start = at + 2;
        }
    }
    options.push(&field[start..]);
    options
}

fn dtype_named(name: &str) -> Result<Representative, String> {
    Representative::from_name(name).ok_or_else(|| format!("{name:?} is not a representative dtype"))
}

fn optional<T>(
    field: &str,
    read: impl FnOnce(&str) -> Result<T, String>,
) -> Result<Option<T>, String> {
    if field == "-" {
        Ok(None)
    } else {
        read(field).map(Some)
    }
}

/// The fields of a line of `count` fields, the last of them free text.
fn fields(line: &str, count: usize) -> Result<Vec<&str>, String> {
    let fields: Vec<&str> = line.splitn(count, " | ").collect();
    if fields.len() == count {
        Ok(fields)
    } else {
        Err(format!("expected {count} fields separated by \" | \""))
    }
}

/// One line of the dtype table: an expression entry with an expression
/// parameter given an argument of one dtype (or none), on a receiver of one
/// dtype. A function that starts an expression has no receiver, and the line
/// writes none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DtypeLine {
    pub entry: &'static str,
    pub parameter: Option<&'static str>,
    pub receiver: Option<Representative>,
    pub argument: Option<Representative>,
    pub outcome: Outcome,
}

impl DtypeLine {
    #[cfg(test)]
    pub(super) fn render(&self) -> String {
        format!(
            "{} | {} | {} | {} | {}",
            self.entry,
            self.parameter.unwrap_or("-"),
            self.receiver
                .map_or_else(|| "-".to_owned(), |dtype| dtype.to_string()),
            self.argument
                .map_or_else(|| "-".to_owned(), |dtype| dtype.to_string()),
            self.outcome
        )
    }

    pub(super) fn parse(line: &str) -> Result<Self, String> {
        let fields = fields(line, 5)?;
        let entry = entry_named(fields[0])?;
        Ok(Self {
            entry: entry.id().as_str(),
            parameter: optional(fields[1], |name| parameter_named(entry, name))?,
            receiver: optional(fields[2], dtype_named)?,
            argument: optional(fields[3], dtype_named)?,
            outcome: Outcome::parse(fields[4])?,
        })
    }
}

/// What a line of the context table measured of an entry in a context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum Chain {
    /// The chain goes on from the entry, with the neutral continuation the
    /// context needs: a test for a value where a `Boolean` is required, a
    /// cast to the key's dtype in a join key, the first value of a group in a
    /// pivot's aggregate. Whether the entry may stand there at all.
    Continues,
    /// The chain ends at the entry, as it is. Whether the entry may end an
    /// expression there.
    Ends,
}

impl Chain {
    /// Both measurements, in the order the tables list them.
    pub(super) const ALL: [Self; 2] = [Self::Continues, Self::Ends];

    /// The word the tables use.
    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::Continues => "continues",
            Self::Ends => "ends",
        }
    }

    fn from_name(name: &str) -> Result<Self, String> {
        Self::ALL
            .into_iter()
            .find(|chain| chain.name() == name)
            .ok_or_else(|| format!("{name:?} is neither continues nor ends"))
    }
}

/// One line of the context table: an entry standing in a context, with the
/// chain continued or ended there, on the receiver and with the arguments it
/// was measured with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ContextLine {
    pub entry: &'static str,
    /// The option given in place of another parameter that the line was measured with, such as
    /// the strategy a fill takes instead of a value: empty for an entry measured as it is.
    pub form: Vec<OptionValue>,
    pub context: Context,
    pub chain: Chain,
    pub receiver: Option<Representative>,
    pub argument: Option<Representative>,
    pub rules: Vec<Rule>,
    pub outcome: Outcome,
}

impl ContextLine {
    #[cfg(test)]
    pub(super) fn render(&self) -> String {
        let rules = if self.rules.is_empty() {
            "-".to_owned()
        } else {
            self.rules
                .iter()
                .map(|rule| rule.name())
                .collect::<Vec<_>>()
                .join(",")
        };
        format!(
            "{}{} | {} | {} | {} | {} | {} | {}",
            self.entry,
            render_form(&self.form),
            self.context,
            self.chain.name(),
            self.receiver
                .map_or_else(|| "-".to_owned(), |dtype| dtype.to_string()),
            self.argument
                .map_or_else(|| "-".to_owned(), |dtype| dtype.to_string()),
            rules,
            self.outcome
        )
    }

    pub(super) fn parse(line: &str) -> Result<Self, String> {
        let fields = fields(line, 7)?;
        let (entry, form) = entry_and_form(fields[0])?;
        let rules = if fields[5] == "-" {
            Vec::new()
        } else {
            fields[5]
                .split(',')
                .map(|name| Rule::from_name(name).ok_or_else(|| format!("{name:?} is not a rule")))
                .collect::<Result<_, _>>()?
        };
        Ok(Self {
            entry: entry.id().as_str(),
            form,
            context: Context::from_name(fields[1])
                .ok_or_else(|| format!("{:?} is not a context", fields[1]))?,
            chain: Chain::from_name(fields[2])?,
            receiver: optional(fields[3], dtype_named)?,
            argument: optional(fields[4], dtype_named)?,
            rules,
            outcome: Outcome::parse(fields[6])?,
        })
    }
}

/// One option of an entry set to one of its values.
pub(super) type OptionValue = (&'static str, String);

/// One line of the option table: an entry with one option, or two, set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct OptionLine {
    pub entry: &'static str,
    pub receiver: Option<Representative>,
    pub options: Vec<OptionValue>,
    pub outcome: Outcome,
}

impl OptionLine {
    #[cfg(test)]
    pub(super) fn render(&self) -> String {
        let options: Vec<String> = self
            .options
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect();
        format!(
            "{} | {} | {} | {}",
            self.entry,
            self.receiver
                .map_or_else(|| "-".to_owned(), |dtype| dtype.to_string()),
            options.join(", "),
            self.outcome
        )
    }

    pub(super) fn parse(line: &str) -> Result<Self, String> {
        let fields = fields(line, 4)?;
        let entry = entry_named(fields[0])?;
        let options = split_options(fields[2])
            .into_iter()
            .map(|option| {
                let (name, value) = option
                    .split_once('=')
                    .ok_or_else(|| format!("{option:?} is not name=value"))?;
                Ok((parameter_named(entry, name)?, value.to_owned()))
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Self {
            entry: entry.id().as_str(),
            receiver: optional(fields[1], dtype_named)?,
            options,
            outcome: Outcome::parse(fields[3])?,
        })
    }
}

/// One line of the table of counts: an expression of a [`Shape`] standing in a context, with the
/// chain ending there.
///
/// The context table measures each entry as the entry gives it. How many values a chain gives is
/// not the entry's alone, since a method that follows a count keeps it, so this table measures
/// the counts themselves: a chain of one value, of as many as the table has rows and of another
/// number of them, in each context, where Polars takes one value or the rows and refuses the rest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CountLine {
    pub context: Context,
    pub shape: Shape,
    pub outcome: Outcome,
}

impl CountLine {
    #[cfg(test)]
    pub(super) fn render(&self) -> String {
        format!(
            "{} | {} | {}",
            self.context,
            self.shape.name(),
            self.outcome
        )
    }

    pub(super) fn parse(line: &str) -> Result<Self, String> {
        let fields = fields(line, 3)?;
        Ok(Self {
            context: Context::from_name(fields[0])
                .ok_or_else(|| format!("{:?} is not a context", fields[0]))?,
            shape: Shape::from_name(fields[1])
                .ok_or_else(|| format!("{:?} is not a shape", fields[1]))?,
            outcome: Outcome::parse(fields[2])?,
        })
    }
}

/// The lines of a committed table, header and blank lines left out, each with
/// its number in the file.
fn numbered<T>(
    text: &'static str,
    parse: impl Fn(&str) -> Result<T, String>,
) -> Result<Vec<(usize, T)>, TableError> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.is_empty() && !line.starts_with('#'))
        .map(|(at, line)| {
            parse(line)
                .map(|parsed| (at + 1, parsed))
                .map_err(|problem| TableError {
                    line: at + 1,
                    problem,
                })
        })
        .collect()
}

/// The lines of a committed table, header and blank lines left out.
pub(super) fn lines<T>(
    text: &'static str,
    parse: impl Fn(&str) -> Result<T, String>,
) -> Result<Vec<T>, TableError> {
    numbered(text, parse).map(|numbered| numbered.into_iter().map(|(_, line)| line).collect())
}

/// A committed table that does not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableError {
    line: usize,
    problem: String,
}

impl fmt::Display for TableError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "line {}: {}", self.line, self.problem)
    }
}

impl std::error::Error for TableError {}

/// The key of a line of the dtype table.
pub(super) type DtypeKey = (
    &'static str,
    Option<&'static str>,
    Option<Representative>,
    Option<Representative>,
);

/// What the context table says of an entry in a context: whether the chain
/// may go on from it, and whether it may end there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ContextCell {
    pub continues: Outcome,
    pub ends: Outcome,
    pub rules: Vec<Rule>,
}

/// The key of the cell of the context table for an entry in a form, in a context.
pub(super) type ContextKey = (&'static str, Vec<OptionValue>, Context);

/// The two lines of the context table for an entry in a context, as far as
/// they have been read.
#[derive(Default)]
struct HalfCell {
    continues: Option<Outcome>,
    ends: Option<Outcome>,
    rules: Vec<Rule>,
}

/// The cells of the lines of the context table: each entry in each context
/// has a line for each [`Chain`], and the rules that govern the two agree.
fn cells(lines: Vec<(usize, ContextLine)>) -> Result<HashMap<ContextKey, ContextCell>, TableError> {
    let mut halves: HashMap<ContextKey, (usize, HalfCell)> = HashMap::new();
    for (at, line) in lines {
        let key = (line.entry, line.form.clone(), line.context);
        let (first, half) = halves.entry(key).or_insert_with(|| {
            (
                at,
                HalfCell {
                    rules: line.rules.clone(),
                    ..HalfCell::default()
                },
            )
        });
        let slot = match line.chain {
            Chain::Continues => &mut half.continues,
            Chain::Ends => &mut half.ends,
        };
        let problem = |problem: String| TableError { line: at, problem };
        if slot.replace(line.outcome).is_some() {
            return Err(problem(format!(
                "{} in {} has two lines that {}",
                line.entry,
                line.context,
                line.chain.name()
            )));
        }
        if half.rules != line.rules {
            return Err(problem(format!(
                "{} in {} names other rules than its line at {first}",
                line.entry, line.context
            )));
        }
    }
    halves
        .into_iter()
        .map(
            |((entry, form, context), (at, half))| match (half.continues, half.ends) {
                (Some(continues), Some(ends)) => Ok((
                    (entry, form, context),
                    ContextCell {
                        continues,
                        ends,
                        rules: half.rules,
                    },
                )),
                _ => Err(TableError {
                    line: at,
                    problem: format!("{entry} in {context} lacks a line that continues or ends"),
                }),
            },
        )
        .collect()
}

/// The key of a line of the option table: the entry, the dtype of the value a method
/// follows (none for a step), and the options set.
pub(super) type OptionKey = (&'static str, Option<Representative>, Vec<OptionValue>);

/// The committed tables, read.
#[derive(Debug)]
pub(super) struct Tables {
    pub dtypes: HashMap<DtypeKey, Outcome>,
    pub contexts: HashMap<ContextKey, ContextCell>,
    pub options: HashMap<OptionKey, Outcome>,
    pub counts: HashMap<(Context, Shape), Outcome>,
}

impl Tables {
    pub(super) fn read(
        dtypes: &'static str,
        contexts: &'static str,
        options: &'static str,
        counts: &'static str,
    ) -> Result<Self, (&'static str, TableError)> {
        let named = |table: &'static str| move |error| (table, error);
        let dtypes = lines(dtypes, DtypeLine::parse)
            .map_err(named("dtypes.txt"))?
            .into_iter()
            .map(|line| {
                (
                    (line.entry, line.parameter, line.receiver, line.argument),
                    line.outcome,
                )
            })
            .collect();
        let contexts = numbered(contexts, ContextLine::parse)
            .and_then(cells)
            .map_err(named("contexts.txt"))?;
        let options = lines(options, OptionLine::parse)
            .map_err(named("options.txt"))?
            .into_iter()
            .map(|line| ((line.entry, line.receiver, line.options), line.outcome))
            .collect();
        let counts = lines(counts, CountLine::parse)
            .map_err(named("counts.txt"))?
            .into_iter()
            .map(|line| ((line.context, line.shape), line.outcome))
            .collect();
        Ok(Self {
            dtypes,
            contexts,
            options,
            counts,
        })
    }

    /// The committed tables. A committed table that does not read is a defect
    /// of the repository, which a test of every table reports.
    pub(super) fn committed() -> &'static Self {
        static TABLES: LazyLock<Tables> =
            LazyLock::new(|| match Tables::read(DTYPES, CONTEXTS, OPTIONS, COUNTS) {
                Ok(tables) => tables,
                Err((table, error)) => {
                    panic!("api/catalogue/{table} does not read, {error}")
                }
            });
        &TABLES
    }
}

/// Whether the entry takes an expression as the value it follows: a method of
/// an expression.
pub(super) fn is_method(entry: &catalog::Entry) -> bool {
    entry.applies() == Applies::Expression
}

/// Whether the entry is measured on a dtype that its call is given: a method, on the value it
/// follows, and a function that starts an expression and takes expressions, on the first of
/// them, which is its subject. The options of such an entry are measured on each dtype.
pub(super) fn has_a_subject(entry: &catalog::Entry) -> bool {
    is_method(entry) || subject_of(entry).is_some()
}

/// The first expression parameter of a function that starts an expression, which what the
/// options of the function are measured on, if it has one.
pub(super) fn subject_of(entry: &catalog::Entry) -> Option<&'static str> {
    (entry.applies() == Applies::Function)
        .then(|| {
            entry
                .parameters()
                .iter()
                .find(|parameter| matches!(parameter.kind(), catalog::Kind::Expression(_)))
                .map(catalog::Parameter::name)
        })
        .flatten()
}
