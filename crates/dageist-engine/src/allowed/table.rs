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

/// How many values a chain gives for the values of one group, or of the whole
/// table: measured on three rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Shape {
    /// One value: the chain reduces.
    One,
    /// A value for every row, or any other number of them.
    Many,
}

impl Shape {
    /// Every shape.
    pub const ALL: [Self; 2] = [Self::One, Self::Many];

    const fn name(self) -> &'static str {
        match self {
            Self::One => "one",
            Self::Many => "many",
        }
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
            let shape = match shape {
                "one" => Shape::One,
                "many" => Shape::Many,
                other => return Err(format!("{other:?} is not a shape")),
            };
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
/// dtype.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DtypeLine {
    pub entry: &'static str,
    pub parameter: Option<&'static str>,
    pub receiver: Representative,
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
            self.receiver,
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
            receiver: dtype_named(fields[2])?,
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
            "{} | {} | {} | {} | {} | {} | {}",
            self.entry,
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
        let rules = if fields[5] == "-" {
            Vec::new()
        } else {
            fields[5]
                .split(',')
                .map(|name| Rule::from_name(name).ok_or_else(|| format!("{name:?} is not a rule")))
                .collect::<Result<_, _>>()?
        };
        Ok(Self {
            entry: entry_named(fields[0])?.id().as_str(),
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
        let options = fields[2]
            .split(", ")
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
    Representative,
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
fn cells(
    lines: Vec<(usize, ContextLine)>,
) -> Result<HashMap<(&'static str, Context), ContextCell>, TableError> {
    let mut halves: HashMap<(&'static str, Context), (usize, HalfCell)> = HashMap::new();
    for (at, line) in lines {
        let (first, half) = halves.entry((line.entry, line.context)).or_insert_with(|| {
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
            |((entry, context), (at, half))| match (half.continues, half.ends) {
                (Some(continues), Some(ends)) => Ok((
                    (entry, context),
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

/// What the option table says of an entry with options set: the dtype it was
/// measured on, for a method, and what came of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct OptionCell {
    pub receiver: Option<Representative>,
    pub outcome: Outcome,
}

/// The committed tables, read.
#[derive(Debug)]
pub(super) struct Tables {
    pub dtypes: HashMap<DtypeKey, Outcome>,
    pub contexts: HashMap<(&'static str, Context), ContextCell>,
    pub options: HashMap<(&'static str, Vec<OptionValue>), OptionCell>,
}

impl Tables {
    pub(super) fn read(
        dtypes: &'static str,
        contexts: &'static str,
        options: &'static str,
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
            .map(|line| {
                (
                    (line.entry, line.options),
                    OptionCell {
                        receiver: line.receiver,
                        outcome: line.outcome,
                    },
                )
            })
            .collect();
        Ok(Self {
            dtypes,
            contexts,
            options,
        })
    }

    /// The committed tables. A committed table that does not read is a defect
    /// of the repository, which a test of every table reports.
    pub(super) fn committed() -> &'static Self {
        static TABLES: LazyLock<Tables> =
            LazyLock::new(|| match Tables::read(DTYPES, CONTEXTS, OPTIONS) {
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
