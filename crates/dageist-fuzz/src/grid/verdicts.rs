//! What every cell of a grid became, its tally, and the assertion that accounts for every cell.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;

use super::axis::{LeftOut, Thinned};
use super::known::Known;
use super::{Kind, Outcome};

/// How many cells a failure names one by one, unless `DAGEIST_GRID_ALL` is set.
const NAMED: usize = 80;

/// A cell that is not agreed on: the choice it holds on each axis, and what became of it.
#[derive(Debug, Clone)]
pub struct Cell<O> {
    pub labels: Vec<(&'static str, String)>,
    pub outcome: O,
}

impl<O> Cell<O> {
    /// The cell by its axes: `axis=label, axis=label`.
    #[must_use]
    pub fn name(&self) -> String {
        self.labels
            .iter()
            .map(|(axis, label)| format!("{axis}={label}"))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The label the cell holds on `axis`.
    #[must_use]
    pub fn label(&self, axis: &str) -> Option<&str> {
        self.labels
            .iter()
            .find(|(name, _)| *name == axis)
            .map(|(_, label)| label.as_str())
    }
}

/// What an axis of a grid held, left out and thinned.
#[derive(Debug, Clone)]
pub struct AxisTally {
    pub name: &'static str,
    pub labels: Vec<String>,
    pub left_out: Vec<LeftOut>,
    pub thinned: Option<Thinned>,
}

/// How many cells a line named.
#[derive(Debug, Clone, Serialize)]
pub struct KnownTally {
    pub kind: &'static str,
    pub reason: &'static str,
    pub cells: usize,
}

/// What became of every cell of a grid, as it is written to `DAGEIST_GRID_TALLY_DIR`.
#[derive(Debug, Clone, Serialize)]
pub struct Tally {
    pub grid: &'static str,
    pub cells: usize,
    pub judged: usize,
    /// The cells of each kind, by the kind's words.
    pub kinds: BTreeMap<&'static str, usize>,
    pub decided_by: BTreeMap<&'static str, usize>,
    pub notes: BTreeMap<&'static str, usize>,
    pub known: Vec<KnownTally>,
    /// The cells no line names.
    pub unnamed: usize,
    /// The lines that name no cell and are not shared.
    pub stale: Vec<&'static str>,
    pub axes: Vec<AxisRecord>,
}

/// An axis as a tally records it.
#[derive(Debug, Clone, Serialize)]
pub struct AxisRecord {
    pub name: &'static str,
    pub choices: usize,
    pub left_out: Vec<LeftOutRecord>,
    pub thinned: Option<ThinnedRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LeftOutRecord {
    pub label: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThinnedRecord {
    pub rule: String,
    pub kept: usize,
    pub of: usize,
}

/// What every cell of a grid became.
pub struct Verdicts<O: Outcome> {
    grid: &'static str,
    size: usize,
    axes: Vec<AxisTally>,
    counts: BTreeMap<O::Kind, usize>,
    decided_by: BTreeMap<&'static str, usize>,
    notes: BTreeMap<&'static str, usize>,
    spent: BTreeMap<&'static str, Duration>,
    listed: Vec<(usize, O)>,
    elapsed: Duration,
    threads: usize,
}

impl<O: Outcome> Verdicts<O> {
    #[expect(
        clippy::too_many_arguments,
        reason = "the parts a run gathers, given once"
    )]
    pub(super) fn new(
        grid: &'static str,
        size: usize,
        axes: Vec<AxisTally>,
        counts: BTreeMap<O::Kind, usize>,
        decided_by: BTreeMap<&'static str, usize>,
        notes: BTreeMap<&'static str, usize>,
        spent: BTreeMap<&'static str, Duration>,
        listed: Vec<(usize, O)>,
        elapsed: Duration,
        threads: usize,
    ) -> Self {
        Self {
            grid,
            size,
            axes,
            counts,
            decided_by,
            notes,
            spent,
            listed,
            elapsed,
            threads,
        }
    }

    /// The cell `index`, by its axes.
    fn cell(&self, index: usize, outcome: &O) -> Cell<O> {
        let mut rest = index;
        let mut labels = Vec::with_capacity(self.axes.len());
        for axis in self.axes.iter().rev() {
            labels.push((axis.name, axis.labels[rest % axis.labels.len()].clone()));
            rest /= axis.labels.len();
        }
        labels.reverse();
        Cell {
            labels,
            outcome: outcome.clone(),
        }
    }

    /// The cells that are not agreed on, in the grid's order.
    pub fn listed(&self) -> impl Iterator<Item = Cell<O>> + '_ {
        self.listed
            .iter()
            .map(|(index, outcome)| self.cell(*index, outcome))
    }

    /// How many cells the grid has.
    #[must_use]
    pub const fn size(&self) -> usize {
        self.size
    }

    /// How many cells are of `kind`.
    #[must_use]
    pub fn count(&self, kind: O::Kind) -> usize {
        self.counts.get(&kind).copied().unwrap_or_default()
    }

    /// How many cells were judged.
    #[must_use]
    pub fn judged(&self) -> usize {
        self.counts
            .iter()
            .filter(|(kind, _)| kind.is_judged())
            .map(|(_, count)| count)
            .sum()
    }

    /// How many cells were judged in a second.
    #[must_use]
    pub fn cells_a_second(&self) -> f64 {
        self.size as f64 / self.elapsed.as_secs_f64().max(1e-9)
    }

    /// The axes, with what each left out and how it was thinned.
    #[must_use]
    pub fn axes(&self) -> &[AxisTally] {
        &self.axes
    }

    fn tally_lines(&self) -> String {
        let kinds: Vec<String> = O::Kind::ALL
            .iter()
            .map(|kind| format!("{} {}", self.count(*kind), kind.words()))
            .collect();
        let mut text = format!(
            "the grid {}: {} cells: {}",
            self.grid,
            self.size,
            kinds.join(", ")
        );
        let counted = |map: &BTreeMap<&'static str, usize>| {
            map.iter()
                .map(|(what, count)| format!("{what} {count}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        if !self.decided_by.is_empty() {
            let _ = write!(text, "\n  decided by: {}", counted(&self.decided_by));
        }
        if !self.notes.is_empty() {
            let _ = write!(text, "\n  of which: {}", counted(&self.notes));
        }
        let spent: Vec<String> = self
            .spent
            .iter()
            .map(|(who, time)| format!("{who} {time:.1?}"))
            .collect();
        let _ = write!(
            text,
            "\n  {:.1?} on {} threads, {:.0} cells a second{}",
            self.elapsed,
            self.threads,
            self.cells_a_second(),
            if spent.is_empty() {
                String::new()
            } else {
                format!("; of the threads' time {}", spent.join(", "))
            }
        );
        for axis in &self.axes {
            if let Some(thinned) = &axis.thinned {
                let _ = write!(
                    text,
                    "\n  the axis {} keeps {} at normal scale, {} of {}",
                    axis.name, thinned.rule, thinned.kept, thinned.of
                );
            }
            for left in &axis.left_out {
                let _ = write!(
                    text,
                    "\n  the axis {} leaves out {}: {}",
                    axis.name, left.label, left.reason
                );
            }
        }
        text
    }

    /// Asserts that every cell is agreed on or named by a line of `known`, that each line that is
    /// not shared names a cell and names only axes of the grid, and that at least
    /// `judged_at_least` cells were judged, so that a grid that has come to build nothing is no
    /// pass. A cell named by several lines is counted by the most specific.
    ///
    /// The tally and every line are printed, and written as JSON where `DAGEIST_GRID_TALLY_DIR`
    /// is set. A failure lists every cell no line names at once, grouped by what was said.
    ///
    /// # Panics
    ///
    /// On a cell no line names, a stale line, a line that names an axis the grid lacks, or too few
    /// judged cells.
    pub fn assert_agree(&self, known: &[Known<O::Kind>], judged_at_least: usize) {
        let (used, unnamed) = self.reckon(known);
        let mut report = self.tally_lines();
        for (line, count) in known.iter().zip(&used) {
            let _ = write!(
                report,
                "\n  {count} {}: {}",
                line.kind().words(),
                line.reason()
            );
        }
        let axes: BTreeSet<&str> = self.axes.iter().map(|axis| axis.name).collect();
        let mut failures = String::new();
        if !unnamed.is_empty() {
            let _ = write!(failures, "\n{} cells that no line names:", unnamed.len());
            failures.push_str(&describe(&unnamed));
        }
        for (line, count) in known.iter().zip(&used) {
            if line.is_shared() {
                continue;
            }
            if let Some(axis) = line.pattern().axes().find(|axis| !axes.contains(axis)) {
                let _ = write!(
                    failures,
                    "\na line names the axis {axis}, which the grid does not have: {}",
                    line.reason()
                );
            }
            if *count == 0 {
                let _ = write!(
                    failures,
                    "\na line that names {} matches no such cell, so it is stale: {}",
                    line.kind().words(),
                    line.reason()
                );
            }
        }
        let judged = self.judged();
        if judged < judged_at_least {
            let _ = write!(
                failures,
                "\nonly {judged} cells were judged, fewer than {judged_at_least}"
            );
        }
        if let Some(folder) = std::env::var_os("DAGEIST_GRID_TALLY_DIR") {
            let tally = self.tally_of(known, &used, unnamed.len());
            match tally.write_to(Path::new(&folder)) {
                Ok(path) => {
                    let _ = write!(report, "\n  the tally is written to {}", path.display());
                }
                Err(error) => {
                    let _ = write!(report, "\n  the tally is not written: {error}");
                }
            }
        }
        eprintln!("{report}");
        assert!(failures.is_empty(), "{report}{failures}");
    }

    /// The tally of the grid, with what each line of `known` named.
    #[must_use]
    pub fn tally(&self, known: &[Known<O::Kind>]) -> Tally {
        let (used, unnamed) = self.reckon(known);
        self.tally_of(known, &used, unnamed.len())
    }

    /// How many cells each line names, each cell counted by the most specific line that names
    /// it, and the cells no line names.
    fn reckon(&self, known: &[Known<O::Kind>]) -> (Vec<usize>, Vec<Cell<O>>) {
        let mut unnamed: Vec<Cell<O>> = Vec::new();
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
        (used, unnamed)
    }

    fn tally_of(&self, known: &[Known<O::Kind>], used: &[usize], unnamed: usize) -> Tally {
        Tally {
            grid: self.grid,
            cells: self.size,
            judged: self.judged(),
            kinds: O::Kind::ALL
                .iter()
                .map(|kind| (kind.words(), self.count(*kind)))
                .collect(),
            decided_by: self.decided_by.clone(),
            notes: self.notes.clone(),
            known: known
                .iter()
                .zip(used)
                .map(|(line, cells)| KnownTally {
                    kind: line.kind().words(),
                    reason: line.reason(),
                    cells: *cells,
                })
                .collect(),
            unnamed,
            stale: known
                .iter()
                .zip(used)
                .filter(|(line, cells)| **cells == 0 && !line.is_shared())
                .map(|(line, _)| line.reason())
                .collect(),
            axes: self
                .axes
                .iter()
                .map(|axis| AxisRecord {
                    name: axis.name,
                    choices: axis.labels.len(),
                    left_out: axis
                        .left_out
                        .iter()
                        .map(|left| LeftOutRecord {
                            label: left.label.clone(),
                            reason: left.reason.clone(),
                        })
                        .collect(),
                    thinned: axis.thinned.as_ref().map(|thinned| ThinnedRecord {
                        rule: thinned.rule.clone(),
                        kept: thinned.kept,
                        of: thinned.of,
                    }),
                })
                .collect(),
        }
    }
}

impl Tally {
    /// Writes the tally as `<grid>.json` in `folder`, which is made where it is missing, and
    /// gives the path.
    ///
    /// # Errors
    ///
    /// When the folder cannot be made or the file cannot be written.
    pub fn write_to(&self, folder: &Path) -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(folder)?;
        let path = folder.join(format!("{}.json", self.grid));
        let mut bytes = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        bytes.push(b'\n');
        std::fs::write(&path, bytes)?;
        Ok(path)
    }
}

/// What the cells have in common, to read a failure of thousands: the cells grouped by kind and
/// by the words that were said with the numbers taken out, each group with what its cells hold on
/// each axis, and then the cells, one by one.
fn describe<O: Outcome>(cells: &[Cell<O>]) -> String {
    let mut groups: BTreeMap<(O::Kind, String), Vec<&Cell<O>>> = BTreeMap::new();
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
        let mut axes: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
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
