//! The one judge of a grid's cell: the case of one step, asked of the engine's walk and of Polars,
//! and sorted into one outcome.
//!
//! - both allow it, or both refuse it;
//! - the walk refuses it and Polars allows it, or Polars refuses it and the walk allows it, which
//!   are the disagreements;
//! - the walk panics, or the case cannot be built, with the words of why;
//! - the catalogue refuses to build the step, which the engine refuses before the walk is asked,
//!   and the walk allows it;
//! - Polars refuses it for another reason than the one the grid asks about, which sets it aside.
//!
//! Most refusals happen while Polars works out the dtypes, so a cell is judged from the schema
//! Polars works out of its query, with no row read. A question that asks for [`Depth::Rows`], and
//! a cell the walk says Polars refuses on rows, runs the rows of the table too, which is where a
//! strict cast of a value, a length and a panic of Polars show. A panic of Polars or of the walk is
//! a cell's outcome.

use std::time::{Duration, Instant};

use dageist_engine::allowed::{RefusalSource, Stage};
use dageist_engine::document::{Document, Inputs, ListOrigin, WalkRefusal};
use dageist_fuzz::grid::{self, Judged};
use polars::prelude::{DataFrame, IntoLazy, Series};
use serde_json::Value;

use crate::ask::{self, Connected};
use crate::doc;

/// What is connected to a step by name: its tables and its lists.
#[derive(Default, Clone)]
pub struct Connections {
    tables: Vec<(String, DataFrame)>,
    lists: Vec<(String, Series)>,
}

impl Connections {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The same, with `table` connected as `name`.
    #[must_use]
    pub fn table(mut self, name: &str, table: DataFrame) -> Self {
        self.tables.push((name.to_owned(), table));
        self
    }

    /// The same, with a list connected as `name`, which `is_in` looks a value up in. Polars is
    /// given it as the run builds it, gathered into one list, for a series of the dtype of the
    /// values is the deprecated form.
    #[must_use]
    pub fn list(mut self, name: &str, list: Series) -> Self {
        self.lists.push((name.to_owned(), list));
        self
    }

    fn inputs(&self) -> Inputs {
        let mut inputs = Inputs::new();
        for (name, list) in &self.lists {
            inputs
                .add_list(name, ListOrigin::Parameter, list.clone())
                .expect("a list that is connected once");
        }
        for (name, table) in &self.tables {
            inputs
                .add_table(name, name, table.clone().lazy())
                .expect("a table that is connected once");
        }
        inputs
    }
}

/// One cell's case: a document of one step, the table it runs on, and what is connected to it.
pub struct Case {
    pub document: Document,
    pub working: DataFrame,
    pub connections: Connections,
}

impl Case {
    /// The case of the one step that calls `entry` with `arguments`, run on `working`.
    ///
    /// # Errors
    ///
    /// The words of why the document cannot be read, which makes the cell unbuildable.
    pub fn of_step(
        entry: &str,
        arguments: Value,
        working: DataFrame,
        connections: Connections,
    ) -> Result<Self, String> {
        Self::of_document(
            &doc::document(vec![doc::step("s", entry, arguments)]),
            working,
            connections,
        )
    }

    /// The case of the step that groups by `arguments` and goes on with the calls `then`, the last
    /// of which finishes the grouping, run on `working`.
    ///
    /// # Errors
    ///
    /// The words of why the document cannot be read, which makes the cell unbuildable.
    pub fn of_grouping(
        arguments: Value,
        then: Vec<Value>,
        working: DataFrame,
        connections: Connections,
    ) -> Result<Self, String> {
        Self::of_document(
            &doc::document(vec![doc::grouping("s", arguments, then)]),
            working,
            connections,
        )
    }

    fn of_document(
        text: &str,
        working: DataFrame,
        connections: Connections,
    ) -> Result<Self, String> {
        let document = Document::read(text).map_err(|error| format!("not a document: {error}"))?;
        Ok(Self {
            document,
            working,
            connections,
        })
    }
}

/// How far Polars is asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Depth {
    /// Only what it says when it works out the schema of the step.
    Plan,
    /// And, where the schema is worked out, the rows of the table. A cell is asked this far too
    /// where the walk says Polars refuses it on rows, whatever the depth is.
    Rows,
}

/// What a grid asks, which sets apart the refusals that answer it from the ones that do not.
#[derive(Clone, Copy)]
pub struct Question {
    /// Whether a refusal by the walk answers the question.
    pub walk_counts: fn(&WalkRefusal) -> bool,
    /// Whether Polars' words are a refusal that answers the question. Another refusal sets the
    /// cell aside, and the grid names those it expects.
    pub polars_counts: fn(&str) -> bool,
    /// How far each cell asks Polars, unless its case says it needs the rows.
    pub depth: Depth,
}

impl Question {
    /// Every refusal answers: whether the walk and Polars both take a step, whatever the reason.
    #[must_use]
    pub const fn any_refusal() -> Self {
        Self {
            walk_counts: |_| true,
            polars_counts: |_| true,
            depth: Depth::Plan,
        }
    }

    /// The same question, asked to the depth.
    #[must_use]
    pub const fn at(mut self, depth: Depth) -> Self {
        self.depth = depth;
        self
    }

    /// Judges the case a cell built, or names the cell unbuildable with why it could not be.
    #[must_use]
    pub fn judge(&self, built: Result<Case, String>) -> Judged<Outcome> {
        match built {
            Ok(case) => judge_case(&case, self),
            Err(reason) => Judged::new(Outcome::Unbuildable(reason)).decided_by(SCHEMA),
        }
    }
}

/// What became of a cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    BothAllow,
    BothRefuse,
    /// The walk refuses and Polars allows the step.
    WalkRefusesOnly {
        walk: String,
    },
    /// Polars refuses and the walk allows the step.
    PolarsRefusesOnly {
        polars: String,
    },
    /// The walk panicked.
    WalkPanicked(String),
    Unbuildable(String),
    /// The catalogue refuses to build the step, which the engine refuses before the walk is
    /// asked, and the walk allows it: a literal whose value does not cast to its dtype, a count
    /// the catalogue takes in a narrower range than Python does.
    BuildRefusesOnly(String),
    /// Polars refused it for another reason than the one the grid asks about.
    SetAside(String),
}

/// The kinds of a cell's outcome, in the order a tally lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    BothAllow,
    BothRefuse,
    WalkRefusesOnly,
    PolarsRefusesOnly,
    WalkPanicked,
    Unbuildable,
    BuildRefusesOnly,
    SetAside,
}

impl grid::Kind for Kind {
    const ALL: &'static [Self] = &[
        Self::BothAllow,
        Self::BothRefuse,
        Self::WalkRefusesOnly,
        Self::PolarsRefusesOnly,
        Self::WalkPanicked,
        Self::Unbuildable,
        Self::BuildRefusesOnly,
        Self::SetAside,
    ];

    fn words(self) -> &'static str {
        match self {
            Self::BothAllow => "both allow",
            Self::BothRefuse => "both refuse",
            Self::WalkRefusesOnly => "the walk refuses only",
            Self::PolarsRefusesOnly => "Polars refuses only",
            Self::WalkPanicked => "the walk panicked",
            Self::Unbuildable => "unbuildable",
            Self::BuildRefusesOnly => "the catalogue refuses only",
            Self::SetAside => "set aside",
        }
    }

    fn is_agreed(self) -> bool {
        matches!(self, Self::BothAllow | Self::BothRefuse)
    }

    fn is_judged(self) -> bool {
        matches!(
            self,
            Self::BothAllow | Self::BothRefuse | Self::WalkRefusesOnly | Self::PolarsRefusesOnly
        )
    }
}

impl grid::Outcome for Outcome {
    type Kind = Kind;

    fn kind(&self) -> Kind {
        match self {
            Self::BothAllow => Kind::BothAllow,
            Self::BothRefuse => Kind::BothRefuse,
            Self::WalkRefusesOnly { .. } => Kind::WalkRefusesOnly,
            Self::PolarsRefusesOnly { .. } => Kind::PolarsRefusesOnly,
            Self::WalkPanicked(_) => Kind::WalkPanicked,
            Self::Unbuildable(_) => Kind::Unbuildable,
            Self::BuildRefusesOnly(_) => Kind::BuildRefusesOnly,
            Self::SetAside(_) => Kind::SetAside,
        }
    }

    fn said(&self) -> String {
        match self {
            Self::BothAllow | Self::BothRefuse => String::new(),
            Self::WalkRefusesOnly { walk } => format!("walk: {walk}; Polars allows it"),
            Self::PolarsRefusesOnly { polars } => format!("Polars: {polars}; the walk allows it"),
            Self::WalkPanicked(reason) => format!("the walk panicked: {reason}"),
            Self::BuildRefusesOnly(reason) => {
                format!("the catalogue refuses to build the step: {reason}; the walk allows it")
            }
            Self::Unbuildable(reason) | Self::SetAside(reason) => reason.clone(),
        }
    }
}

/// What decided a cell: Polars' schema, with no row read.
const SCHEMA: &str = "the schema";
/// What decided a cell: the rows of the table, where the schema was worked out.
const ROWS: &str = "the rows";
/// What a tally counts of a cell in which Polars panicked.
const POLARS_PANICKED: &str = "Polars panicked";

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or_default().trim().to_owned()
}

fn walk_words(refusal: &WalkRefusal) -> String {
    let source = match refusal.refusal().source() {
        RefusalSource::Rule(rule) => format!("rule {}", rule.name()),
        RefusalSource::Polars(stage) => format!("tables of Polars at {stage:?}"),
        RefusalSource::Structure => "structure".to_owned(),
    };
    format!("{source}: {}", first_line(refusal.refusal().reason()))
}

fn panic_words(payload: &(dyn std::any::Any + Send)) -> String {
    let said = payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "something that is no text".to_owned());
    first_line(&said)
}

fn judge_case(case: &Case, question: &Question) -> Judged<Outcome> {
    let step = &case.document.steps()[0];
    let inputs = case.connections.inputs();
    let walk_started = Instant::now();
    let walked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        step.walk(0, case.working.schema(), &inputs)
    }));
    let walk_spent = walk_started.elapsed();
    // The walk says Polars refuses it on rows, so the rows are what to ask.
    let (walk_said, walk_said_rows) = match walked {
        Ok(walk) => {
            let rows = walk.refusal().is_some_and(|refusal| {
                matches!(
                    refusal.refusal().source(),
                    RefusalSource::Polars(Stage::Rows | Stage::Empty)
                )
            });
            let said = walk
                .refusal()
                .filter(|refusal| (question.walk_counts)(refusal))
                .map(walk_words);
            (said, rows)
        }
        Err(payload) => {
            return Judged::new(Outcome::WalkPanicked(panic_words(&*payload)))
                .decided_by(SCHEMA)
                .spent("the walk", walk_spent);
        }
    };
    let tables = |name: &str| {
        case.connections
            .tables
            .iter()
            .find(|(held, _)| held == name)
            .map(|(_, table)| table.clone().lazy())
    };
    let literals = |name: &str| {
        case.connections
            .lists
            .iter()
            .find(|(held, _)| held == name)
            .map(|(_, list)| polars::prelude::lit(list.clone()).implode(true))
    };
    let connected = Connected::new(&tables, &literals);
    let rows = question.depth == Depth::Rows || walk_said_rows;
    let decided = std::cell::Cell::new(SCHEMA);
    let polars_started = Instant::now();
    // A panic of Polars is its answer, as a refusal is.
    let asked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        match ask::schema_of(step, 0, &case.working, connected) {
            None => None,
            Some(Err(refused)) => Some((Some(refused.reason), refused.at_build)),
            Some(Ok(_)) if !rows => Some((None, false)),
            Some(Ok(_)) => {
                decided.set(ROWS);
                Some((
                    ask::collect(step, 0, &case.working, connected, 100).err(),
                    false,
                ))
            }
        }
    }));
    let polars_spent: Duration = polars_started.elapsed();
    let judged = |outcome: Outcome| {
        Judged::new(outcome)
            .decided_by(decided.get())
            .spent("the walk", walk_spent)
            .spent("Polars", polars_spent)
    };
    let (polars, at_build, panicked) = match asked {
        // A step that holds what cannot be asked, a hole, is no cell that Polars judged.
        Ok(None) => {
            return judged(Outcome::Unbuildable(
                "Polars cannot be asked of the step: it holds a hole".to_owned(),
            ));
        }
        Ok(Some((polars, at_build))) => (polars, at_build, false),
        Err(payload) => (
            Some(format!("Polars panicked: {}", panic_words(&*payload))),
            false,
            true,
        ),
    };
    let outcome = match (walk_said, polars) {
        // The catalogue refuses the step itself where it cannot build it, before the walk is asked
        // and whatever Polars would say: the cell is both refuse where the walk refuses it too, and
        // the catalogue's own kind where the walk allows it.
        (Some(_), Some(_)) if at_build => Outcome::BothRefuse,
        (None, Some(reason)) if at_build => Outcome::BuildRefusesOnly(reason),
        (_, Some(reason)) if !(question.polars_counts)(&reason) => Outcome::SetAside(reason),
        (None, None) => Outcome::BothAllow,
        (Some(_), Some(_)) => Outcome::BothRefuse,
        (Some(walk), None) => Outcome::WalkRefusesOnly { walk },
        (None, Some(polars)) => Outcome::PolarsRefusesOnly { polars },
    };
    let judged = judged(outcome);
    if panicked {
        judged.note(POLARS_PANICKED)
    } else {
        judged
    }
}
