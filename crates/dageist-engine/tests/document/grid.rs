//! A grid of cases, which Polars judges as one, so that every place the walk and Polars part shows
//! at once and not one review round at a time.
//!
//! A grid is the product of named axes. Each axis holds labelled choices, and a choice sets one
//! field of a point, a plain value the test defines for its own question: the dtype of the column,
//! the kind and the dtype of a literal, the form a parameter is given in, the method, the order of
//! the operands. The grid makes every combination of one choice from each axis, builds a case of
//! each point (a one-step document, the table it runs on and what is connected to it), asks the
//! walk and asks Polars of the same case, and sorts the cell into one outcome:
//!
//! - both allow it, or both refuse it;
//! - the walk refuses it and Polars allows it, or Polars refuses it and the walk allows it, which
//!   are the disagreements;
//! - the walk panics, or the case cannot be built, with the words of why;
//! - Polars refuses it for another reason than the one the grid asks about, which sets it aside.
//!
//! Nothing is dropped without being said. The walk must accept what Polars accepts and refuse what
//! Polars refuses, and the cells that do not are listed, every one by name and all at once, with a
//! tally of the outcomes. A cell that is known to differ is listed in the test by the pattern of
//! its axes, with the reason, and each such line must still match a cell, so a limit that is gone
//! is found as a stale line and not left behind. Unbuildable and set aside cells are named the
//! same way, each kind with its reason.
//!
//! # Speed
//!
//! A grid is made for millions of cells. Cells are never collected: each is made from its number
//! when a thread takes it, and only the cells that are not agreed on are kept. Threads take them
//! in turn, so the grid is judged on every core the host gives it (`DAGEIST_GRID_THREADS` sets
//! the number). Most refusals happen while Polars works out the dtypes, so a cell is judged from
//! the schema Polars works out of its query, on the table's schema, with no row read. Only a grid
//! that asks for [`Depth::Rows`], runs the rows of
//! a table of three, which is where a strict cast of a value, a length and a panic of Polars
//! show. The tally says how many cells each depth decided, and how many cells a second the
//! grid was judged at. A panic of Polars or of the walk is a cell's outcome, not a crash of the
//! test.
//!
//! # Writing a grid
//!
//! 1. A `Default` struct, the point, holds what the axes choose.
//! 2. [`Grid::new`] gets one [`Grid::axis`] for each thing that varies, each with its labelled
//!    values and the function that sets the point's field from one. A fuzz finding that a
//!    campaign reduced becomes a new choice on the axis it varied, or a new axis if it varied
//!    something the grid does not, and the grid then holds the finding and its neighbours.
//! 3. [`Grid::judge`] is given the [`Question`] the grid asks and a function that builds the
//!    [`Case`] of a point, or says why it cannot.
//! 4. [`Verdicts::assert_agree`] is given the cells that are known to differ, with the reason, and
//!    the least number of cells that must be judged.
//!
//! The literal kinds and the edge values that rules which read a literal are crossed with are in
//! [`literals`], and the one-step cases in [`Case`].

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use dageist_engine::allowed::{After, RefusalSource, Representative, Stage};
use dageist_engine::document::{Document, Inputs, Step, Walk, WalkRefusal};
use polars::prelude::{DataFrame, IntoLazy, NamedFrom, Series};
use serde_json::Value;

use crate::ask_polars::{self, Connected};
use crate::support::{document, step};

// ---------------------------------------------------------------------------
// The grid

type Apply<P> = Box<dyn Fn(&mut P) + Send + Sync>;

/// One axis: its name and the choices on it, each with its label.
struct Axis<P> {
    name: &'static str,
    choices: Vec<(String, Apply<P>)>,
}

/// The product of axes. A cell is one choice from each, applied in order to a default point.
pub struct Grid<P> {
    axes: Vec<Axis<P>>,
}

impl<P: Default + 'static> Default for Grid<P> {
    fn default() -> Self {
        Self::new()
    }
}

/// The cells a thread takes at once.
const CHUNK: usize = 32;

/// The stack of a thread that asks Polars, which plans deeply in a debug build.
const STACK: usize = 64 << 20;

/// How long a cell may take before the test ends, naming it: `DAGEIST_GRID_DEADLINE_SECS`, or a
/// minute. Polars may not end a query, and a thread cannot be stopped, so the process ends.
fn deadline() -> Duration {
    Duration::from_secs(
        std::env::var("DAGEIST_GRID_DEADLINE_SECS")
            .ok()
            .and_then(|text| text.trim().parse().ok())
            .filter(|seconds| *seconds > 0)
            .unwrap_or(60),
    )
}

/// How many threads judge a grid: `DAGEIST_GRID_THREADS`, or the cores the host gives, up to 8.
fn thread_count() -> usize {
    std::env::var("DAGEIST_GRID_THREADS")
        .ok()
        .and_then(|text| text.trim().parse().ok())
        .filter(|threads| *threads > 0)
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map_or(1, std::num::NonZero::get)
                .min(8)
        })
}

impl<P: Default + 'static> Grid<P> {
    #[must_use]
    pub const fn new() -> Self {
        Self { axes: Vec::new() }
    }

    /// An axis named `name`: each of `choices` is a label and a value, which `set` writes to the
    /// point. Labels of one axis differ, for a pattern names a cell by them.
    ///
    /// # Panics
    ///
    /// When a name or a label of an axis is repeated.
    #[must_use]
    pub fn axis<V: Clone + Send + Sync + 'static>(
        mut self,
        name: &'static str,
        choices: Vec<(String, V)>,
        set: fn(&mut P, &V),
    ) -> Self {
        assert!(
            self.axes.iter().all(|axis| axis.name != name),
            "the axis {name} is on the grid twice"
        );
        let mut seen: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
        for (label, _) in &choices {
            assert!(
                seen.insert(label),
                "the label {label:?} is on the axis {name} twice"
            );
        }
        let choices = choices
            .into_iter()
            .map(|(label, value)| {
                let apply: Apply<P> = Box::new(move |point| set(point, &value));
                (label, apply)
            })
            .collect();
        self.axes.push(Axis { name, choices });
        self
    }

    /// How many cells the grid has.
    #[must_use]
    pub fn size(&self) -> usize {
        self.axes.iter().map(|axis| axis.choices.len()).product()
    }

    /// The point of the cell `index`, the last axis counting fastest.
    fn point(&self, mut index: usize) -> P {
        let mut chosen = vec![0_usize; self.axes.len()];
        for (axis, at) in self.axes.iter().zip(&mut chosen).rev() {
            *at = index % axis.choices.len();
            index /= axis.choices.len();
        }
        let mut point = P::default();
        for (axis, at) in self.axes.iter().zip(chosen) {
            (axis.choices[at].1)(&mut point);
        }
        point
    }

    /// The cell `index` by its axes.
    fn name_of(&self, mut index: usize) -> String {
        let mut parts = Vec::with_capacity(self.axes.len());
        for axis in self.axes.iter().rev() {
            parts.push(format!(
                "{}={}",
                axis.name,
                axis.choices[index % axis.choices.len()].0
            ));
            index /= axis.choices.len();
        }
        parts.reverse();
        parts.join(", ")
    }

    fn names(&self) -> Vec<(&'static str, Vec<String>)> {
        self.axes
            .iter()
            .map(|axis| {
                (
                    axis.name,
                    axis.choices
                        .iter()
                        .map(|(label, _)| label.clone())
                        .collect(),
                )
            })
            .collect()
    }

    /// Judges every cell: builds its case with `build`, asks the walk and Polars of it, and sorts
    /// it by `question`. The cells are judged on several threads, each taking the next ones when
    /// it is free.
    ///
    /// # Panics
    ///
    /// When `build` panics.
    pub fn judge(
        &self,
        question: &Question,
        build: impl Fn(&P) -> Result<Case, String> + Sync,
    ) -> Verdicts {
        let size = self.size();
        let threads = thread_count().min(size.div_ceil(CHUNK).max(1));
        let next = AtomicUsize::new(0);
        let started = Instant::now();
        // The cell each thread is on, and since when, which a watchdog reads.
        let on: Vec<Mutex<Option<(usize, Instant)>>> =
            (0..threads).map(|_| Mutex::new(None)).collect();
        let finished = AtomicBool::new(false);
        let work = |thread: usize| {
            let mut partial = Partial::default();
            loop {
                let from = next.fetch_add(CHUNK, Ordering::Relaxed);
                if from >= size {
                    return partial;
                }
                for index in from..(from + CHUNK).min(size) {
                    *on[thread].lock().expect("the watchdog holds no lock") =
                        Some((index, Instant::now()));
                    let judged = match build(&self.point(index)) {
                        Ok(case) => judge_case(&case, question),
                        Err(reason) => Judged::of(Outcome::Unbuildable(reason), Mode::Plan),
                    };
                    partial.record(index, judged);
                }
                *on[thread].lock().expect("the watchdog holds no lock") = None;
            }
        };
        let allowed = deadline();
        let watchdog = || {
            while !finished.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(200));
                for slot in &on {
                    let held = *slot.lock().expect("no thread panics holding it");
                    if let Some((index, since)) = held
                        && since.elapsed() > allowed
                    {
                        eprintln!(
                            "the cell {index} did not end in {allowed:?}: {}",
                            self.name_of(index)
                        );
                        std::process::exit(124);
                    }
                }
            }
        };
        let partials: Vec<Partial> = std::thread::scope(|scope| {
            let guard = scope.spawn(watchdog);
            let handles: Vec<_> = (0..threads)
                .map(|thread| {
                    let work = &work;
                    std::thread::Builder::new()
                        .stack_size(STACK)
                        .spawn_scoped(scope, move || work(thread))
                        .expect("a thread to judge cells on")
                })
                .collect();
            let partials: Vec<_> = handles.into_iter().map(|handle| handle.join()).collect();
            finished.store(true, Ordering::Relaxed);
            let _ = guard.join();
            partials
                .into_iter()
                .map(|joined| joined.unwrap_or_else(|payload| std::panic::resume_unwind(payload)))
                .collect()
        });
        let mut verdicts = Verdicts {
            size,
            names: self.names(),
            counts: Kind::ALL.iter().map(|kind| (*kind, 0)).collect(),
            by_depth: [0, 0],
            panics: 0,
            spent: [Duration::ZERO; 2],
            listed: Vec::new(),
            elapsed: started.elapsed(),
            threads,
        };
        for partial in partials {
            for (kind, count) in partial.counts {
                *verdicts.counts.entry(kind).or_default() += count;
            }
            verdicts.by_depth[0] += partial.by_depth[0];
            verdicts.by_depth[1] += partial.by_depth[1];
            verdicts.panics += partial.panics;
            verdicts.spent[0] += partial.spent[0];
            verdicts.spent[1] += partial.spent[1];
            verdicts.listed.extend(partial.listed);
        }
        verdicts.listed.sort_by_key(|(index, _)| *index);
        verdicts
    }
}

/// What one thread found.
#[derive(Default)]
struct Partial {
    counts: BTreeMap<Kind, usize>,
    by_depth: [usize; 2],
    panics: usize,
    spent: [Duration; 2],
    listed: Vec<(usize, Outcome)>,
}

impl Partial {
    fn record(&mut self, index: usize, judged: Judged) {
        let kind = judged.outcome.kind();
        *self.counts.entry(kind).or_default() += 1;
        self.by_depth[judged.mode as usize] += 1;
        self.panics += usize::from(judged.panicked);
        self.spent[0] += judged.spent[0];
        self.spent[1] += judged.spent[1];
        // The cells both sides agree on are counted and not kept.
        if kind.must_be_named() {
            self.listed.push((index, judged.outcome));
        }
    }
}

// ---------------------------------------------------------------------------
// The case of a cell

/// What is connected to a step by name: its tables.
#[derive(Default, Clone)]
pub struct Connections {
    tables: Vec<(String, DataFrame)>,
    /// The lists connected to the step, by name, which a step looks a value up in.
    lists: Vec<(String, Series)>,
}

impl Connections {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn table(mut self, name: &str, table: DataFrame) -> Self {
        self.tables.push((name.to_owned(), table));
        self
    }

    /// A list connected as `name`, which `is_in` looks a value up in. Polars is given it as the
    /// run builds it, gathered into one list, for a series of the dtype of the values is the
    /// deprecated form.
    #[must_use]
    pub fn membership_list(mut self, name: &str, list: Series) -> Self {
        self.lists.push((name.to_owned(), list));
        self
    }

    fn inputs(&self) -> Inputs {
        let mut inputs = Inputs::new();
        for (name, list) in &self.lists {
            inputs
                .add_list(
                    name,
                    dageist_engine::document::ListOrigin::Parameter,
                    list.clone(),
                )
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
        let text = document(vec![step("s", entry, arguments)]);
        let document = Document::read(&text).map_err(|error| format!("not a document: {error}"))?;
        Ok(Self {
            document,
            working,
            connections,
        })
    }
}

impl Case {
    /// The case of the step that groups by `arguments` and goes on with the calls `then`, the
    /// last of which finishes the grouping, run on `working`.
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
        let text = document(vec![crate::support::grouping("s", arguments, then)]);
        let document = Document::read(&text).map_err(|error| format!("not a document: {error}"))?;
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
pub struct Question {
    /// Whether a refusal by the walk answers the question.
    pub walk_counts: fn(&WalkRefusal) -> bool,
    /// Whether Polars' words are a refusal that answers the question. Another refusal sets the
    /// cell aside, and the test names those it expects.
    pub polars_counts: fn(&str) -> bool,
    /// How far each cell asks Polars, unless its case says it needs the rows.
    pub depth: Depth,
    /// Whether the words of what the walk cannot tell of a step are the ones that answer the
    /// question: a walk that refuses nothing and cannot tell a step for them does not allow it, as
    /// one that refuses nothing and tells nothing does.
    pub cannot_tell_counts: fn(&str) -> bool,
    /// Whether the dtype the walk gives at each place of a step that both allow is compared with
    /// the dtype Polars gives there, as the fuzz harness compares it.
    pub dtypes: bool,
}

impl Question {
    /// Every refusal answers: whether the walk and Polars both take a step, whatever the reason.
    #[must_use]
    pub const fn any_refusal() -> Self {
        Self {
            walk_counts: |_| true,
            polars_counts: |_| true,
            depth: Depth::Plan,
            cannot_tell_counts: |_| false,
            dtypes: false,
        }
    }

    /// The same question, asked to the depth.
    #[must_use]
    pub const fn at(mut self, depth: Depth) -> Self {
        self.depth = depth;
        self
    }

    /// The same question, where a walk that cannot tell a step for words that `counts` holds is
    /// not taken to allow it.
    #[must_use]
    pub const fn cannot_tell(mut self, counts: fn(&str) -> bool) -> Self {
        self.cannot_tell_counts = counts;
        self
    }

    /// The same question, which also compares the dtype at each place of a step that both allow.
    #[must_use]
    pub const fn with_dtypes(mut self) -> Self {
        self.dtypes = true;
        self
    }
}

// ---------------------------------------------------------------------------
// Judging a cell

/// What became of a cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    BothAllow,
    BothRefuse,
    /// The walk refuses and Polars allows the step.
    WalkRefusesOnly {
        walk: String,
    },
    /// The walk refuses nothing but cannot tell the step, for the words the question counts, and
    /// Polars refuses it: the step is not allowed as the walk's answer, which is what Polars says.
    CannotTellPolarsRefuses,
    /// The walk refuses nothing but cannot tell the step, for the words the question counts, and
    /// Polars allows it.
    CannotTellPolarsAllows {
        walk: String,
    },
    /// Both allow the step, and the dtype the walk gives at a place is not the one Polars gives.
    DtypeDiffers(String),
    /// Polars refuses and the walk allows the step.
    PolarsRefusesOnly {
        polars: String,
    },
    /// The walk panicked.
    WalkPanicked(String),
    Unbuildable(String),
    /// The catalogue refuses to build the step, which the step engine refuses before the walk is
    /// asked, and the walk allows it: a literal whose value does not cast to its dtype, a count
    /// the catalogue takes in a narrower range than Python does.
    BuildRefusesOnly(String),
    /// Polars refused it for another reason than the one the grid asks about.
    SetAside(String),
}

/// The kinds of a cell's outcome that a test names, in the order a tally lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    BothAllow,
    BothRefuse,
    CannotTellPolarsRefuses,
    CannotTellPolarsAllows,
    DtypeDiffers,
    WalkRefusesOnly,
    PolarsRefusesOnly,
    WalkPanicked,
    Unbuildable,
    BuildRefusesOnly,
    SetAside,
}

impl Kind {
    const ALL: [Self; 11] = [
        Self::BothAllow,
        Self::BothRefuse,
        Self::CannotTellPolarsRefuses,
        Self::CannotTellPolarsAllows,
        Self::DtypeDiffers,
        Self::WalkRefusesOnly,
        Self::PolarsRefusesOnly,
        Self::WalkPanicked,
        Self::Unbuildable,
        Self::BuildRefusesOnly,
        Self::SetAside,
    ];

    const fn words(self) -> &'static str {
        match self {
            Self::BothAllow => "both allow",
            Self::BothRefuse => "both refuse",
            Self::CannotTellPolarsRefuses => "the walk cannot tell, Polars refuses",
            Self::CannotTellPolarsAllows => "the walk cannot tell, Polars allows",
            Self::DtypeDiffers => "the dtype differs",
            Self::WalkRefusesOnly => "the walk refuses only",
            Self::PolarsRefusesOnly => "Polars refuses only",
            Self::WalkPanicked => "the walk panicked",
            Self::Unbuildable => "unbuildable",
            Self::BuildRefusesOnly => "the catalogue refuses only",
            Self::SetAside => "set aside",
        }
    }

    /// Whether the kind is one a test must name.
    const fn must_be_named(self) -> bool {
        !matches!(
            self,
            Self::BothAllow | Self::BothRefuse | Self::CannotTellPolarsRefuses
        )
    }
}

impl Outcome {
    #[must_use]
    pub const fn kind(&self) -> Kind {
        match self {
            Self::BothAllow => Kind::BothAllow,
            Self::BothRefuse => Kind::BothRefuse,
            Self::CannotTellPolarsRefuses => Kind::CannotTellPolarsRefuses,
            Self::CannotTellPolarsAllows { .. } => Kind::CannotTellPolarsAllows,
            Self::DtypeDiffers(_) => Kind::DtypeDiffers,
            Self::WalkRefusesOnly { .. } => Kind::WalkRefusesOnly,
            Self::PolarsRefusesOnly { .. } => Kind::PolarsRefusesOnly,
            Self::WalkPanicked(_) => Kind::WalkPanicked,
            Self::Unbuildable(_) => Kind::Unbuildable,
            Self::BuildRefusesOnly(_) => Kind::BuildRefusesOnly,
            Self::SetAside(_) => Kind::SetAside,
        }
    }

    /// The words that say what each side said, for a listing.
    #[must_use]
    pub fn said(&self) -> String {
        match self {
            Self::BothAllow | Self::BothRefuse | Self::CannotTellPolarsRefuses => String::new(),
            Self::CannotTellPolarsAllows { walk } => {
                format!("walk cannot tell: {walk}; Polars allows it")
            }
            Self::DtypeDiffers(words) => words.clone(),
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

/// An outcome with the depth that decided it.
struct Judged {
    outcome: Outcome,
    mode: Mode,
    panicked: bool,
    /// How long the walk and Polars took.
    spent: [Duration; 2],
}

impl Judged {
    const fn of(outcome: Outcome, mode: Mode) -> Self {
        Self {
            outcome,
            mode,
            panicked: false,
            spent: [Duration::ZERO; 2],
        }
    }
}

/// What decided a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Polars' schema, with no row read.
    Plan = 0,
    /// The rows of the table, where the schema was worked out.
    Rows = 1,
}

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

fn judge_case(case: &Case, question: &Question) -> Judged {
    let step = &case.document.steps()[0];
    let inputs = case.connections.inputs();
    let walk_started = Instant::now();
    let walked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        step.walk(0, case.working.schema(), &inputs)
    }));
    let walk_spent = walk_started.elapsed();
    // The walk says Polars refuses it on rows, so the rows are what to ask.
    let (walk_said, walk_said_rows, cannot_tell, walk) = match walked {
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
            let cannot_tell = walk
                .not_known()
                .filter(|words| (question.cannot_tell_counts)(words))
                .map(first_line);
            (said, rows, cannot_tell, walk)
        }
        Err(payload) => {
            return Judged::of(Outcome::WalkPanicked(panic_words(&*payload)), Mode::Plan);
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
    let mode = std::cell::Cell::new(Mode::Plan);
    let polars_started = Instant::now();
    // A panic of Polars is its answer, as a refusal is.
    let asked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || match ask_polars::schema_of(step, 0, &case.working, connected) {
            None => None,
            Some(Err(refused)) => Some((Some(refused.reason), refused.at_build)),
            Some(Ok(_)) if !rows => Some((None, false)),
            Some(Ok(_)) => {
                mode.set(Mode::Rows);
                Some((
                    ask_polars::collect(step, 0, &case.working, connected, 100).err(),
                    false,
                ))
            }
        },
    ));
    let polars_spent = polars_started.elapsed();
    let (polars, at_build, panicked) = match asked {
        // A step that holds what cannot be asked, a hole, is no cell that Polars judged.
        Ok(None) => {
            return Judged::of(
                Outcome::Unbuildable(
                    "Polars cannot be asked of the step: it holds a hole".to_owned(),
                ),
                Mode::Plan,
            );
        }
        Ok(Some((polars, at_build))) => (polars, at_build, false),
        Err(payload) => (
            Some(format!("Polars panicked: {}", panic_words(&*payload))),
            false,
            true,
        ),
    };
    // Where both allow it, and the walk tells the whole of it, the dtypes it gives are Polars'.
    let differing = if question.dtypes
        && walk_said.is_none()
        && cannot_tell.is_none()
        && polars.is_none()
        && walk.refusal().is_none()
        && walk.not_known().is_none()
    {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            dtype_difference(step, &walk, case, connected)
        }))
        .ok()
        .flatten()
    } else {
        None
    };
    let outcome = match (walk_said, polars) {
        // The catalogue refuses the step itself where it cannot build it, before the walk is asked
        // and whatever Polars would say: the cell is both refuse where the walk refuses it too, and
        // the catalogue's own kind where the walk allows it.
        (Some(_), Some(_)) if at_build => Outcome::BothRefuse,
        (None, Some(reason)) if at_build => Outcome::BuildRefusesOnly(reason),
        (_, Some(reason)) if !(question.polars_counts)(&reason) => Outcome::SetAside(reason),
        (None, None) if cannot_tell.is_some() => Outcome::CannotTellPolarsAllows {
            walk: cannot_tell.unwrap_or_default(),
        },
        (None, Some(_)) if cannot_tell.is_some() => Outcome::CannotTellPolarsRefuses,
        (None, None) => differing.map_or(Outcome::BothAllow, Outcome::DtypeDiffers),
        (Some(_), Some(_)) => Outcome::BothRefuse,
        (Some(walk), None) => Outcome::WalkRefusesOnly { walk },
        (None, Some(polars)) => Outcome::PolarsRefusesOnly { polars },
    };
    Judged {
        outcome,
        mode: mode.get(),
        panicked,
        spent: [walk_spent, polars_spent],
    }
}

/// The class the walk says an expression has at the `at`th of the places its links stand in.
fn claimed_at(
    expression: &dageist_engine::document::WalkedExpression,
    at: usize,
) -> Option<Representative> {
    match expression.places().get(at).copied().flatten()?.after() {
        After::Value { dtype, .. } => Some(dtype),
        _ => None,
    }
}

/// Where the dtype the walk gives at the start of an expression or after a link is not the dtype
/// Polars gives there, as the fuzz harness compares them: one word of the first place that differs.
fn dtype_difference(
    step: &Step,
    walk: &Walk,
    case: &Case,
    connected: Connected<'_>,
) -> Option<String> {
    let planned = ask_polars::plan(step, 0, &case.working, connected)?;
    for expression in walk.expressions() {
        let Some(trace) = planned
            .traces
            .iter()
            .find(|trace| trace.place == *expression.place())
        else {
            continue;
        };
        let differs =
            |what: String, claimed: Option<Representative>, said: &ask_polars::Said| match (
                claimed, said,
            ) {
                (Some(claimed), ask_polars::Said::Dtype(_, name, Some(class)))
                    if claimed != *class =>
                {
                    Some(format!(
                        "{} {what}: the tables give {}, Polars gives {name}",
                        expression.place(),
                        claimed.name()
                    ))
                }
                _ => None,
            };
        if let Some(difference) = differs(
            "at the start".to_owned(),
            ask_polars::class_at_start(step, expression, claimed_at(expression, 0)),
            &trace.start,
        ) {
            return Some(difference);
        }
        for (at, said) in trace.links.iter().enumerate() {
            if let Some(difference) = differs(
                format!("after link {}", at + 1),
                claimed_at(expression, at + 1),
                said,
            ) {
                return Some(difference);
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// The verdicts

/// How many cells a failure names one by one, unless `DAGEIST_GRID_ALL` is set.
const NAMED: usize = 80;

/// What the cells have in common, to read a failure of thousands: the cells grouped by outcome and
/// the words that were said with the numbers taken out, each group with what its cells hold on
/// each axis, and then the cells, one by one.
fn describe(cells: &[Cell]) -> String {
    let mut groups: BTreeMap<(Kind, String), Vec<&Cell>> = BTreeMap::new();
    for cell in cells {
        let said: String =
            cell.outcome
                .said()
                .chars()
                .fold(String::new(), |mut text, character| {
                    if character.is_ascii_digit() {
                        if !text.ends_with('#') {
                            text.push('#');
                        }
                    } else {
                        text.push(character);
                    }
                    text
                });
        groups
            .entry((cell.outcome.kind(), said))
            .or_default()
            .push(cell);
    }
    let mut text = String::new();
    for ((kind, said), members) in &groups {
        let _ = write!(
            text,
            "\n  {} cells, {}: {said}",
            members.len(),
            kind.words()
        );
        let mut axes: BTreeMap<&str, std::collections::BTreeSet<&str>> = BTreeMap::new();
        for cell in members {
            for (axis, label) in &cell.labels {
                axes.entry(axis).or_default().insert(label);
            }
        }
        for (axis, labels) in axes {
            let shown: Vec<&str> = labels.iter().copied().take(12).collect();
            let more = labels.len().saturating_sub(shown.len());
            let _ = write!(
                text,
                "\n      {axis} in {{{}{}}}",
                shown.join(" | "),
                if more == 0 {
                    String::new()
                } else {
                    format!(" | and {more} more")
                }
            );
        }
    }
    let limit = if std::env::var_os("DAGEIST_GRID_ALL").is_some() {
        usize::MAX
    } else {
        NAMED
    };
    text.push_str("\nthe cells:");
    for cell in cells.iter().take(limit) {
        let _ = write!(
            text,
            "\n  [{}] {}: {}",
            cell.outcome.kind().words(),
            cell.name(),
            cell.outcome.said()
        );
    }
    if cells.len() > limit {
        let _ = write!(
            text,
            "\n  and {} more, which DAGEIST_GRID_ALL=1 names",
            cells.len() - limit
        );
    }
    text
}

/// A cell: the choice it holds on each axis, and what became of it.
#[derive(Debug, Clone)]
pub struct Cell {
    pub labels: Vec<(&'static str, String)>,
    pub outcome: Outcome,
}

impl Cell {
    /// The cell by its axes: `axis=label, axis=label`.
    #[must_use]
    pub fn name(&self) -> String {
        self.labels
            .iter()
            .map(|(axis, label)| format!("{axis}={label}"))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn label(&self, axis: &str) -> Option<&str> {
        self.labels
            .iter()
            .find(|(name, _)| *name == axis)
            .map(|(_, label)| label.as_str())
    }
}

/// How a pattern matches the label of one axis.
#[derive(Debug, Clone, Copy)]
enum Matcher {
    Is(&'static str),
    Starts(&'static str),
    ContainsOneOf(&'static [&'static str]),
    HasOneOf(&'static [&'static str]),
    OneOf(&'static [&'static str]),
}

impl Matcher {
    fn matches(self, label: &str) -> bool {
        match self {
            Self::Is(word) => label == word,
            Self::Starts(prefix) => label.starts_with(prefix),
            Self::ContainsOneOf(words) => words.iter().any(|word| label.contains(word)),
            Self::HasOneOf(parts) => label.split(", ").any(|part| parts.contains(&part)),
            Self::OneOf(words) => words.contains(&label),
        }
    }
}

/// The cells that hold certain choices: every part must match the label of its axis. A pattern
/// with no part matches every cell.
#[derive(Debug, Clone, Default)]
pub struct Pattern {
    parts: Vec<(&'static str, Matcher)>,
}

impl Pattern {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The label of `axis` is `label`.
    #[must_use]
    pub fn is(mut self, axis: &'static str, label: &'static str) -> Self {
        self.parts.push((axis, Matcher::Is(label)));
        self
    }

    /// The label of `axis` starts with `prefix`.
    #[must_use]
    pub fn starts(mut self, axis: &'static str, prefix: &'static str) -> Self {
        self.parts.push((axis, Matcher::Starts(prefix)));
        self
    }

    /// The label of `axis`, a list whose parts are told apart by a comma and a space, has one of
    /// `parts` as a part: `Expr.log` names a chain with that call and not one with `Expr.log1p`.
    #[must_use]
    pub fn has_one_of(mut self, axis: &'static str, parts: &'static [&'static str]) -> Self {
        self.parts.push((axis, Matcher::HasOneOf(parts)));
        self
    }

    /// The label of `axis` holds one of `words`.
    #[must_use]
    pub fn contains_one_of(mut self, axis: &'static str, words: &'static [&'static str]) -> Self {
        self.parts.push((axis, Matcher::ContainsOneOf(words)));
        self
    }

    /// The label of `axis` is one of `labels`.
    #[must_use]
    pub fn one_of(mut self, axis: &'static str, labels: &'static [&'static str]) -> Self {
        self.parts.push((axis, Matcher::OneOf(labels)));
        self
    }

    fn matches(&self, cell: &Cell) -> bool {
        self.parts
            .iter()
            .all(|(axis, matcher)| cell.label(axis).is_some_and(|label| matcher.matches(label)))
    }
}

/// A line of a test that says why cells of one kind are expected, and which they are.
pub struct Known {
    pub kind: Kind,
    pub pattern: Pattern,
    /// Why, in words of the test's author.
    pub reason: &'static str,
    /// Words of which what was said of each cell holds one, so that a cell that is set aside or
    /// unbuildable for another reason than the one given is not passed over.
    pub saying: Vec<&'static str>,
    /// Whether the line is one that several grids share, so that a grid it names no cell of is
    /// not why it is stale.
    pub shared: bool,
}

impl Known {
    #[must_use]
    pub const fn new(kind: Kind, pattern: Pattern, reason: &'static str) -> Self {
        Self {
            kind,
            pattern,
            reason,
            saying: Vec::new(),
            shared: false,
        }
    }

    /// The same line, held by a grid that may have no cell for it.
    #[must_use]
    pub const fn shared(mut self) -> Self {
        self.shared = true;
        self
    }

    /// The same line, which names only the cells whose words hold `words`.
    #[must_use]
    pub fn saying(mut self, words: &'static str) -> Self {
        self.saying = vec![words];
        self
    }

    /// The same line, which names only the cells whose words hold one of `words`.
    #[must_use]
    pub fn saying_one_of(mut self, words: &[&'static str]) -> Self {
        self.saying = words.to_vec();
        self
    }

    /// How many things the line says of a cell: the parts of its pattern, and the words. Where a
    /// cell is named by more than one line, the most specific one counts it, so that a wide line
    /// cannot starve a narrow one.
    fn specificity(&self) -> usize {
        self.pattern.parts.len() + usize::from(!self.saying.is_empty())
    }

    fn names(&self, cell: &Cell) -> bool {
        self.kind == cell.outcome.kind()
            && self.pattern.matches(cell)
            && (self.saying.is_empty() || {
                let said = cell.outcome.said();
                self.saying.iter().any(|word| said.contains(word))
            })
    }
}

/// What every cell of a grid became.
pub struct Verdicts {
    size: usize,
    names: Vec<(&'static str, Vec<String>)>,
    counts: BTreeMap<Kind, usize>,
    by_depth: [usize; 2],
    panics: usize,
    /// The time the threads together gave the walk and Polars.
    spent: [Duration; 2],
    /// The cells that are to be named, by their number.
    listed: Vec<(usize, Outcome)>,
    elapsed: Duration,
    threads: usize,
}

impl Verdicts {
    /// The cell `index`, by its axes.
    fn cell(&self, index: usize, outcome: &Outcome) -> Cell {
        let mut rest = index;
        let mut labels: Vec<(&'static str, String)> = Vec::with_capacity(self.names.len());
        for (axis, choices) in self.names.iter().rev() {
            labels.push((axis, choices[rest % choices.len()].clone()));
            rest /= choices.len();
        }
        labels.reverse();
        Cell {
            labels,
            outcome: outcome.clone(),
        }
    }

    /// The cells that are not agreed on and not both allowed or both refused.
    pub fn listed(&self) -> impl Iterator<Item = Cell> + '_ {
        self.listed
            .iter()
            .map(|(index, outcome)| self.cell(*index, outcome))
    }

    /// How many cells Polars and the walk both said something of.
    #[must_use]
    pub fn judged(&self) -> usize {
        [
            Kind::BothAllow,
            Kind::BothRefuse,
            Kind::CannotTellPolarsRefuses,
            Kind::CannotTellPolarsAllows,
            Kind::DtypeDiffers,
            Kind::WalkRefusesOnly,
            Kind::PolarsRefusesOnly,
        ]
        .iter()
        .map(|kind| self.counts[kind])
        .sum()
    }

    /// How many cells were judged in a second.
    #[must_use]
    pub fn cells_a_second(&self) -> f64 {
        self.size as f64 / self.elapsed.as_secs_f64().max(1e-9)
    }

    fn tally_line(&self) -> String {
        let parts: Vec<String> = Kind::ALL
            .iter()
            .map(|kind| format!("{} {}", self.counts[kind], kind.words()))
            .collect();
        format!(
            "{} cells: {}\n  decided by the schema {}, by the rows {}; Polars panicked in {}; \
             {:.1?} on {} threads, {:.0} cells a second; the walk took {:.1?} and Polars {:.1?} \
             of the threads' time",
            self.size,
            parts.join(", "),
            self.by_depth[Mode::Plan as usize],
            self.by_depth[Mode::Rows as usize],
            self.panics,
            self.elapsed,
            self.threads,
            self.cells_a_second(),
            self.spent[0],
            self.spent[1]
        )
    }

    /// Asserts that the walk and Polars agree on every cell but the ones `known` names, each of
    /// which must still match a cell of its kind, and that every unbuildable cell and every cell
    /// set aside is named too. At least `judged_at_least` cells must have been judged, so a grid
    /// that has come to build nothing is no pass.
    ///
    /// The tally and every named line are printed, and every cell that is not named is listed at
    /// once in the failure, with what each side said.
    ///
    /// # Panics
    ///
    /// On a cell that is not named, a line that matches no cell, or too few judged cells.
    pub fn assert_agree(&self, known: &[Known], judged_at_least: usize) {
        let mut report = self.tally_line();
        let mut unnamed: Vec<Cell> = Vec::new();
        let mut used = vec![0_usize; known.len()];
        for cell in self.listed() {
            let naming = known
                .iter()
                .enumerate()
                .filter(|(_, line)| line.names(&cell))
                .max_by_key(|(at, line)| (line.specificity(), std::cmp::Reverse(*at)))
                .map(|(at, _)| at);
            match naming {
                Some(at) => used[at] += 1,
                None => unnamed.push(cell),
            }
        }
        for (line, count) in known.iter().zip(&used) {
            let _ = write!(report, "\n  {count} {}: {}", line.kind.words(), line.reason);
        }
        let mut failures = String::new();
        if !unnamed.is_empty() {
            let _ = write!(failures, "\n{} cells that no line names:", unnamed.len());
            failures.push_str(&describe(&unnamed));
        }
        for (line, _) in known
            .iter()
            .zip(&used)
            .filter(|(line, count)| **count == 0 && !line.shared)
        {
            let _ = write!(
                failures,
                "\na line that names {} matches no such cell, so it is stale: {}",
                line.kind.words(),
                line.reason
            );
        }
        let judged = self.judged();
        if judged < judged_at_least {
            let _ = write!(
                failures,
                "\nonly {judged} cells were judged, fewer than {judged_at_least}"
            );
        }
        eprintln!("{report}");
        assert!(failures.is_empty(), "{report}{failures}");
    }
}

// ---------------------------------------------------------------------------
// The axes most grids share

/// The literal a cell writes: its value, and the dtype it is given where it has one.
#[derive(Debug, Clone, Default)]
pub struct Chosen {
    pub value: Option<literals::Written>,
    pub word: Option<&'static str>,
}

impl Chosen {
    /// The call `polars.lit` of what the cell chose.
    ///
    /// # Panics
    ///
    /// When no value is chosen.
    #[must_use]
    pub fn call(&self) -> Value {
        literals::call(self.value.as_ref().expect("a value is chosen"), self.word)
    }

    /// The expression argument that is the literal.
    #[must_use]
    pub fn expression(&self) -> Value {
        literals::expression(self.value.as_ref().expect("a value is chosen"), self.word)
    }
}

impl Chosen {
    /// Whether the value is a number, or a text of one, that is at least `limit`.
    #[must_use]
    pub fn is_at_least(&self, limit: f64) -> bool {
        match self.value.as_ref() {
            Some(literals::Written::Integer(number)) => (*number as f64) >= limit,
            Some(literals::Written::Float(number)) => *number >= limit,
            Some(literals::Written::Text(text)) => text
                .trim()
                .parse::<f64>()
                .is_ok_and(|number| number >= limit),
            _ => false,
        }
    }
}

/// A point that holds the literal a grid varies.
pub trait HoldsLiteral {
    fn literal_mut(&mut self) -> &mut Chosen;
}

/// A point that holds the dtype of the column a grid varies.
pub trait HoldsColumn {
    fn column_mut(&mut self) -> &mut Option<polars::prelude::DataType>;
}

impl<P: Default + HoldsLiteral + 'static> Grid<P> {
    /// The two axes of a literal: its `value`, as the document writes it, and its `dtype`, which
    /// is none for a value written without one. The value is written as one kind, and the dtype
    /// may be of another: a float given a dtype of whole numbers, a text of a number given one.
    #[must_use]
    pub fn literal_axes(
        self,
        values: Vec<literals::Written>,
        words: &[Option<&'static str>],
    ) -> Self {
        let values = values
            .into_iter()
            .map(|written| (written.label(), written))
            .collect();
        let words = words
            .iter()
            .map(|word| ((*word).unwrap_or("none").to_owned(), *word))
            .collect();
        self.axis("value", values, |point, written| {
            point.literal_mut().value = Some(written.clone());
        })
        .axis("dtype", words, |point, word| {
            point.literal_mut().word = *word
        })
    }
}

impl<P: Default + HoldsColumn + 'static> Grid<P> {
    /// The axis of the column the step runs on, which is one column `x` of each dtype.
    #[must_use]
    pub fn column_axis(self, dtypes: Vec<polars::prelude::DataType>) -> Self {
        let choices = dtypes
            .into_iter()
            .map(|dtype| (dtype_word(&dtype), dtype))
            .collect();
        self.axis("column", choices, |point, dtype| {
            *point.column_mut() = Some(dtype.clone());
        })
    }
}

/// The word a document writes the dtype with.
///
/// # Panics
///
/// When a document cannot hold the dtype.
#[must_use]
pub fn dtype_word(dtype: &polars::prelude::DataType) -> String {
    dageist_engine::document::Dtype::new(dtype.clone())
        .unwrap_or_else(|error| panic!("a dtype of a document: {error}"))
        .to_string()
}

/// The dtypes of the columns a grid runs on: every whole number, the floats, `Boolean`, `String`
/// and the dates and times.
#[must_use]
pub fn column_dtypes() -> Vec<polars::prelude::DataType> {
    use polars::prelude::{DataType as D, TimeUnit};
    vec![
        D::Int8,
        D::Int16,
        D::Int32,
        D::Int64,
        D::Int128,
        D::UInt8,
        D::UInt16,
        D::UInt32,
        D::UInt64,
        D::UInt128,
        D::Float16,
        D::Float32,
        D::Float64,
        D::Boolean,
        D::String,
        D::Date,
        D::Datetime(TimeUnit::Microseconds, None),
        D::Duration(TimeUnit::Microseconds),
        D::Time,
    ]
}

/// A table of the one column `x` of `dtype`, which holds 1, 2 and a null, so that a method that
/// fills, compares or counts has a value of each kind to meet.
///
/// # Errors
///
/// When the whole numbers do not cast to the dtype.
pub fn one_column_table(dtype: &polars::prelude::DataType) -> Result<DataFrame, String> {
    let whole = Series::new("x".into(), [Some(1_i64), Some(2), None]);
    let column = whole
        .cast(dtype)
        .map_err(|error| format!("whole numbers do not cast to {dtype}: {error}"))?;
    DataFrame::new(3, vec![column.into()]).map_err(|error| format!("no table: {error}"))
}

// ---------------------------------------------------------------------------
// Literals

/// The literals a document can write, and the values at the edges of the dtypes a rule that reads
/// a literal is crossed with.
pub mod literals {
    use serde_json::{Value, json};

    /// A value a literal is written as, which is one kind of the document's: a whole number, a
    /// number with a fraction, a text, a flag or a null.
    #[derive(Debug, Clone, PartialEq)]
    pub enum Written {
        Integer(i64),
        Float(f64),
        Text(&'static str),
        Flag(bool),
        Null,
    }

    impl Written {
        /// The value as a document writes an argument.
        #[must_use]
        pub fn json(&self) -> Value {
            match self {
                Self::Integer(number) => json!({ "integer": number }),
                Self::Float(number) => json!({ "float": number }),
                Self::Text(text) => json!({ "string": text }),
                Self::Flag(flag) => json!({ "flag": flag }),
                Self::Null => json!({ "flag": null }),
            }
        }

        /// The value for a label: `1.5`, `"-1"`, `true` or `null`, the kind kept apart by its
        /// writing, so a whole float is `300.0`.
        #[must_use]
        pub fn label(&self) -> String {
            match self {
                Self::Integer(number) => number.to_string(),
                Self::Float(number) => format!("{number:?}"),
                Self::Text(text) => format!("{text:?}"),
                Self::Flag(flag) => flag.to_string(),
                Self::Null => "null".to_owned(),
            }
        }
    }

    /// The call `polars.lit` of `value`, given the dtype `word` where it is one.
    #[must_use]
    pub fn call(value: &Written, word: Option<&str>) -> Value {
        let mut arguments = json!({ "value": value.json() });
        if let Some(word) = word {
            arguments["dtype"] = json!({ "dtype": word });
        }
        crate::support::call("polars.lit", arguments)
    }

    /// The expression argument that is that literal.
    #[must_use]
    pub fn expression(value: &Written, word: Option<&str>) -> Value {
        crate::support::chain(call(value, word), Vec::new())
    }

    /// The label of a literal: its value, and its dtype where it has one.
    #[must_use]
    pub fn label(value: &Written, word: Option<&str>) -> String {
        word.map_or_else(
            || value.label(),
            |word| format!("{} as {word}", value.label()),
        )
    }

    /// The dtypes of whole numbers a document names, with their lowest and highest values.
    pub const WHOLE_DTYPES: [(&str, i128, i128); 10] = [
        ("Int8", i8::MIN as i128, i8::MAX as i128),
        ("Int16", i16::MIN as i128, i16::MAX as i128),
        ("Int32", i32::MIN as i128, i32::MAX as i128),
        ("Int64", i64::MIN as i128, i64::MAX as i128),
        ("Int128", i128::MIN, i128::MAX),
        ("UInt8", 0, u8::MAX as i128),
        ("UInt16", 0, u16::MAX as i128),
        ("UInt32", 0, u32::MAX as i128),
        ("UInt64", 0, u64::MAX as i128),
        ("UInt128", 0, i128::MAX),
    ];

    /// Every dtype a literal is given in a document: the whole numbers, the floats, `Boolean` and
    /// `String`.
    pub const DTYPES: [&str; 14] = [
        "Int8", "Int16", "Int32", "Int64", "Int128", "UInt8", "UInt16", "UInt32", "UInt64",
        "UInt128", "Float16", "Float32", "Float64", "Boolean",
    ];

    /// The whole numbers a document writes that sit at an edge: the lowest and highest of each
    /// dtype of whole numbers and one past each (where a whole number of 64 bits holds it), zero,
    /// and the numbers around the limits of exactness of the floats, 2^24 and 2^53.
    #[must_use]
    pub fn edge_integers() -> Vec<Written> {
        let mut numbers: Vec<i64> = vec![-1, 0, 1, 2, 127, 128, 255, 256, 300, 70_000];
        for (_, lowest, highest) in WHOLE_DTYPES {
            for edge in [
                lowest.checked_sub(1),
                Some(lowest),
                Some(highest),
                highest.checked_add(1),
            ]
            .into_iter()
            .flatten()
            {
                numbers.extend(i64::try_from(edge));
            }
        }
        for power in [1_i64 << 24, 1_i64 << 53] {
            numbers.extend([power - 1, power, power + 1, -power]);
        }
        numbers.sort_unstable();
        numbers.dedup();
        numbers.into_iter().map(Written::Integer).collect()
    }

    /// The numbers with a fraction a document writes at an edge: zero and minus zero, whole
    /// floats, fractions that round to a whole number, the limits of exactness 2^24 and 2^53 and
    /// the numbers past them, and numbers above 64 bits.
    #[must_use]
    pub fn edge_floats() -> Vec<Written> {
        let numbers = [
            0.0,
            -0.0,
            1.0,
            -1.0,
            0.5,
            1.5,
            -1.5,
            2.5,
            0.999_999_9,
            100.0,
            300.0,
            -129.0,
            65_504.0,
            65_520.0,
            16_777_215.0,
            16_777_216.0,
            16_777_217.5,
            16_777_218.0,
            9_007_199_254_740_991.0,
            9_007_199_254_740_992.0,
            9_007_199_254_740_994.0,
            9.223_372_036_854_776e18,
            1e18,
            2e38,
            3.5e38,
            f64::MAX,
        ];
        numbers.into_iter().map(Written::Float).collect()
    }

    /// The texts a rule that reads a text may meet: numbers written as text, at the edges of the
    /// whole-number dtypes and past them, a fraction, and words that are no number.
    #[must_use]
    pub fn edge_texts() -> Vec<Written> {
        [
            "",
            "1d",
            "abc",
            "0",
            "-0",
            "1",
            "-1",
            "1.5",
            "300",
            "1e3",
            " 1",
            "true",
            "255",
            "256",
            "-129",
            "100000000",
            "9007199254740993",
            "9223372036854775807",
            "9223372036854775808",
            "170141183460469231731687303715884105727",
            "170141183460469231731687303715884105728",
            "340282366920938463463374607431768211455",
            "340282366920938463463374607431768211456",
        ]
        .into_iter()
        .map(Written::Text)
        .collect()
    }
}
