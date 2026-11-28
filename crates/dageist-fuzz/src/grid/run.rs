//! The product of axes, and judging every cell of it.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use super::axis::{Choices, LeftOut, Thinned};
use super::verdicts::{AxisTally, Verdicts};
use super::{Kind, Outcome};
use crate::Result;
use crate::generation::Entropy;

type Apply<P> = Box<dyn Fn(&mut P) + Send + Sync>;

/// One axis of a grid: its name, its labels, what each choice sets, and what it left out.
struct Axis<P> {
    name: &'static str,
    labels: Vec<String>,
    apply: Vec<Apply<P>>,
    left_out: Vec<LeftOut>,
    thinned: Option<Thinned>,
}

/// The product of named axes. A cell is one choice from each, applied in order to a default
/// point, which the consumer's judge builds its case from.
pub struct Grid<P> {
    name: &'static str,
    axes: Vec<Axis<P>>,
    stack: usize,
}

/// The cells a thread takes at once.
const CHUNK: usize = 32;

/// The stack of a thread that judges cells, unless [`Grid::stack`] sets another.
const STACK: usize = 64 << 20;

/// How long a cell may take: `DAGEIST_GRID_DEADLINE_SECS`, or a minute.
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
    /// A grid named `name`, which its tally is written under, with no axis yet.
    #[must_use]
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            axes: Vec::new(),
            stack: STACK,
        }
    }

    /// The same grid, whose threads have `bytes` of stack.
    #[must_use]
    pub const fn stack(mut self, bytes: usize) -> Self {
        self.stack = bytes;
        self
    }

    /// The same grid with one more axis, whose every choice `set` writes to the point. The last
    /// axis counts fastest.
    ///
    /// # Panics
    ///
    /// When the grid already has an axis of that name.
    #[must_use]
    pub fn axis<V: Send + Sync + 'static>(
        mut self,
        choices: Choices<V>,
        set: fn(&mut P, &V),
    ) -> Self {
        let (name, choices, left_out, thinned) = choices.into_parts();
        assert!(
            self.axes.iter().all(|axis| axis.name != name),
            "the axis {name} is on the grid twice"
        );
        let mut labels = Vec::with_capacity(choices.len());
        let mut apply: Vec<Apply<P>> = Vec::with_capacity(choices.len());
        for (label, value) in choices {
            labels.push(label);
            apply.push(Box::new(move |point| set(point, &value)));
        }
        self.axes.push(Axis {
            name,
            labels,
            apply,
            left_out,
            thinned,
        });
        self
    }

    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// How many cells the grid has.
    #[must_use]
    pub fn size(&self) -> usize {
        self.axes.iter().map(|axis| axis.labels.len()).product()
    }

    /// The choice of each axis in the cell `index`.
    fn chosen(&self, mut index: usize) -> Vec<usize> {
        let mut chosen = vec![0_usize; self.axes.len()];
        for (axis, at) in self.axes.iter().zip(&mut chosen).rev() {
            *at = index % axis.labels.len();
            index /= axis.labels.len();
        }
        chosen
    }

    /// The point of the cell `index`.
    ///
    /// # Panics
    ///
    /// When the grid has no cell `index`.
    #[must_use]
    pub fn point(&self, index: usize) -> P {
        assert!(
            index < self.size(),
            "the grid {} has no cell {index}",
            self.name
        );
        let mut point = P::default();
        for (axis, at) in self.axes.iter().zip(self.chosen(index)) {
            (axis.apply[at])(&mut point);
        }
        point
    }

    /// The label each axis holds in the cell `index`.
    ///
    /// # Panics
    ///
    /// When the grid has no cell `index`.
    #[must_use]
    pub fn labels(&self, index: usize) -> Vec<(&'static str, String)> {
        assert!(
            index < self.size(),
            "the grid {} has no cell {index}",
            self.name
        );
        self.axes
            .iter()
            .zip(self.chosen(index))
            .map(|(axis, at)| (axis.name, axis.labels[at].clone()))
            .collect()
    }

    /// A cell drawn from campaign entropy, so that a campaign can sample the grid's space.
    ///
    /// # Errors
    ///
    /// When the entropy runs out, or the grid has no cell.
    pub fn draw(&self, entropy: &mut Entropy<'_>) -> Result<usize> {
        let size = self.size();
        if size == 0 {
            return Err(crate::FuzzError::new(
                crate::FuzzErrorKind::InvalidCase,
                "draw a grid cell",
                format!("the grid {} has no cell", self.name),
            ));
        }
        Ok(entropy.range(0, (size - 1) as u64)? as usize)
    }

    fn axis_tallies(&self) -> Vec<AxisTally> {
        self.axes
            .iter()
            .map(|axis| AxisTally {
                name: axis.name,
                labels: axis.labels.clone(),
                left_out: axis.left_out.clone(),
                thinned: axis.thinned.clone(),
            })
            .collect()
    }

    /// Judges every cell with `judge`, on several threads, each taking the next cells when it is
    /// free. A cell that has not been judged within the deadline ends the process with exit
    /// status 124, naming the cell: the judge may call into code that never returns, and a thread
    /// cannot be stopped.
    ///
    /// # Panics
    ///
    /// When `judge` panics: a judge catches the panics that are a cell's outcome itself.
    pub fn judge<O: Outcome>(&self, judge: impl Fn(&P) -> Judged<O> + Sync) -> Verdicts<O> {
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
                    partial.record(index, judge(&self.point(index)));
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
                        let name: Vec<String> = self
                            .labels(index)
                            .into_iter()
                            .map(|(axis, label)| format!("{axis}={label}"))
                            .collect();
                        eprintln!(
                            "the cell {index} of the grid {} did not end in {allowed:?}: {}",
                            self.name,
                            name.join(", ")
                        );
                        std::process::exit(124);
                    }
                }
            }
        };
        let partials: Vec<Partial<O>> = std::thread::scope(|scope| {
            let guard = scope.spawn(watchdog);
            let handles: Vec<_> = (0..threads)
                .map(|thread| {
                    let work = &work;
                    std::thread::Builder::new()
                        .stack_size(self.stack)
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
        let mut whole = Partial::default();
        for partial in partials {
            whole.merge(partial);
        }
        whole.listed.sort_by_key(|(index, _)| *index);
        Verdicts::new(
            self.name,
            size,
            self.axis_tallies(),
            whole.counts,
            whole.decided_by,
            whole.notes,
            whole.spent,
            whole.listed,
            started.elapsed(),
            threads,
        )
    }
}

/// What a judge says of one cell: its outcome, and what the tally counts of how it was judged.
#[derive(Debug, Clone)]
pub struct Judged<O> {
    outcome: O,
    decided_by: Option<&'static str>,
    notes: Vec<&'static str>,
    spent: Vec<(&'static str, Duration)>,
}

impl<O> Judged<O> {
    #[must_use]
    pub const fn new(outcome: O) -> Self {
        Self {
            outcome,
            decided_by: None,
            notes: Vec::new(),
            spent: Vec::new(),
        }
    }

    /// The same judgement, which says what decided it, such as the schema or the rows. The tally
    /// counts the cells each decided.
    #[must_use]
    pub const fn decided_by(mut self, what: &'static str) -> Self {
        self.decided_by = Some(what);
        self
    }

    /// The same judgement, which the tally counts among the cells of which `what` is so, such as
    /// that a side panicked.
    #[must_use]
    pub fn note(mut self, what: &'static str) -> Self {
        self.notes.push(what);
        self
    }

    /// The same judgement, which says that `who` took `time` of it. The tally sums each.
    #[must_use]
    pub fn spent(mut self, who: &'static str, time: Duration) -> Self {
        self.spent.push((who, time));
        self
    }

    #[must_use]
    pub const fn outcome(&self) -> &O {
        &self.outcome
    }
}

/// What one thread found.
struct Partial<O: Outcome> {
    counts: BTreeMap<O::Kind, usize>,
    decided_by: BTreeMap<&'static str, usize>,
    notes: BTreeMap<&'static str, usize>,
    spent: BTreeMap<&'static str, Duration>,
    listed: Vec<(usize, O)>,
}

impl<O: Outcome> Default for Partial<O> {
    fn default() -> Self {
        Self {
            counts: BTreeMap::new(),
            decided_by: BTreeMap::new(),
            notes: BTreeMap::new(),
            spent: BTreeMap::new(),
            listed: Vec::new(),
        }
    }
}

impl<O: Outcome> Partial<O> {
    fn record(&mut self, index: usize, judged: Judged<O>) {
        let kind = judged.outcome.kind();
        *self.counts.entry(kind).or_default() += 1;
        if let Some(what) = judged.decided_by {
            *self.decided_by.entry(what).or_default() += 1;
        }
        for note in judged.notes {
            *self.notes.entry(note).or_default() += 1;
        }
        for (who, time) in judged.spent {
            *self.spent.entry(who).or_default() += time;
        }
        // The cells that are agreed on are counted and not kept.
        if !kind.is_agreed() {
            self.listed.push((index, judged.outcome));
        }
    }

    fn merge(&mut self, other: Self) {
        for (kind, count) in other.counts {
            *self.counts.entry(kind).or_default() += count;
        }
        for (what, count) in other.decided_by {
            *self.decided_by.entry(what).or_default() += count;
        }
        for (what, count) in other.notes {
            *self.notes.entry(what).or_default() += count;
        }
        for (who, time) in other.spent {
            *self.spent.entry(who).or_default() += time;
        }
        self.listed.extend(other.listed);
    }
}
