//! Exhaustive grids: every combination of a few named axes, each cell judged by an owner's judge,
//! and the cells that are not agreed on named, all at once, by the lines that expect them.
//!
//! A campaign samples a space and keeps what fails; a grid enumerates a space and accounts for
//! every cell of it. Both are the consumer's cases judged by the consumer's oracle: this module
//! owns the mechanics and knows nothing of what a cell is about.
//!
//! - [`Choices`] are the labelled values of one axis, with what the axis leaves out and why, and
//!   the rule by which a quick run thins it: a run is of full [`Scale`], every cell, unless
//!   `DAGEIST_GRID_SCALE=quick` asks for the choices each thinned axis keeps.
//! - [`Grid`] is the product of axes. Each cell is made from its number when a thread takes it,
//!   so a grid of millions of cells holds none of them in memory. [`Grid::judge`] judges every
//!   cell on several threads, each cell bounded by a deadline that ends the process with the
//!   cell's name when it is passed, for a thread cannot be stopped.
//! - The consumer's judge gives each cell a [`Judged`] outcome. The outcome's [`Kind`] says
//!   whether the cell is agreed on, which is counted and not kept, and whether it was judged at
//!   all, which counts towards the least number of cells a grid must judge.
//! - [`Known`] lines name, by a [`Pattern`] of the cells' labels and the words said of them, the
//!   cells that are expected not to be agreed on, each with its reason.
//!   [`Verdicts::assert_agree`] fails on a cell no line names, on a line that names no cell, and
//!   on a grid that judged fewer cells than it must.
//! - The [`Tally`] says what became of every cell, by kind, by line, by what decided it, and what
//!   each axis left out and thinned. It is printed, and written as JSON to the folder named by
//!   `DAGEIST_GRID_TALLY_DIR` where that is set, which is how a grid is shown to keep or widen
//!   its coverage across a change.
//! - [`Grid::sample`] judges about as many cells as it is given, spread over every axis, and the
//!   verdicts project how long the whole grid takes, which the tally records: a grid's cost is
//!   known before it is run whole.
//! - [`Grid::draw`] picks a cell from campaign entropy, so a campaign can sample the space a grid
//!   enumerates, and a finding it reduces is a cell with labels.
//!
//! Environment: `DAGEIST_GRID_THREADS` sets the number of threads (by default the cores the host
//! gives, at most 8), `DAGEIST_GRID_DEADLINE_SECS` the deadline of a cell (60 seconds),
//! `DAGEIST_GRID_ALL` names every unnamed cell of a failure rather than the first 80,
//! `DAGEIST_GRID_TALLY_DIR` the folder tallies are written to, `DAGEIST_GRID_SCALE=quick` a run
//! that thins the axes a grid says how to thin, and `DAGEIST_GRID_SAMPLE=<cells>` a run that
//! judges about that many cells of each grid, spread over every axis, and projects how long every
//! cell takes.

mod axis;
mod known;
mod run;
mod verdicts;

pub use axis::{Choices, LeftOut, Scale, Thinned};
pub use known::{Known, Pattern};
pub use run::{Grid, Judged};
pub use verdicts::{
    AxisRecord, AxisTally, Cell, KnownTally, LeftOutRecord, Tally, ThinnedRecord, Verdicts,
};

use std::fmt::Debug;

/// The kinds of a consumer's outcomes, in the order a tally lists them.
pub trait Kind: Copy + Ord + Debug + Send + Sync + 'static {
    /// Every kind.
    const ALL: &'static [Self];

    /// The words a tally and a failure name the kind with.
    fn words(self) -> &'static str;

    /// Whether a cell of the kind is agreed on: it is counted, not kept, and no line names it.
    fn is_agreed(self) -> bool;

    /// Whether a cell of the kind was judged, and so counts towards the least number of cells a
    /// grid must judge: a cell that could not be built, or was set aside, was not.
    fn is_judged(self) -> bool;
}

/// What a consumer's judge says of a cell.
pub trait Outcome: Clone + Send + 'static {
    type Kind: Kind;

    /// The kind of the outcome.
    fn kind(&self) -> Self::Kind;

    /// The words that say what each side said, which a failure shows and a [`Known`] line's
    /// words are matched against. Empty for an agreed outcome.
    fn said(&self) -> String;
}
