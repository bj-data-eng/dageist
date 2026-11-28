//! What the engine says of every entry at every place: the offers of all 45 contexts, each at
//! nothing, at every class of dtype in each shape, and at each state of a condition.
//!
//! The corpus plans from it, which is the engine's own answer, and its tests hold the corpus
//! against it. Asking the engine for it takes a while, so it is asked once for the whole test
//! binary, a context to a thread.

use std::sync::OnceLock;

use dageist_engine::allowed::{
    self, After, ConditionState, Context, Place, RefusalSource, Representative, Shape,
};
use dageist_engine::catalog::{self, Entry};

/// What the engine says of one entry at one place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Cell {
    /// It may stand there, and gives this class of dtype where the tables know it, in this shape.
    Allowed {
        class: Option<Representative>,
        shape: Shape,
        /// Whether the expression may end at the entry in this context.
        ends: bool,
    },
    /// It may not, for this reason.
    Refused(RefusalSource),
}

/// The offers at every place.
pub(super) struct Grid {
    /// Every context, as the tables list them.
    pub contexts: Vec<Context>,
    /// Every kind of what an expression may have.
    pub afters: Vec<After>,
    /// The entries that stand in an expression, in the catalogue's order.
    pub entries: Vec<&'static Entry>,
    /// For each context, for each of `afters`, for each of `entries`, what the engine says.
    cells: Vec<Vec<Vec<Cell>>>,
}

impl Grid {
    /// What the engine says of the entry numbered `entry` at the `after`th kind of the `context`th
    /// context.
    pub(super) fn cell(&self, context: usize, after: usize, entry: usize) -> Cell {
        self.cells[context][after][entry]
    }

    /// The number of the context `context`.
    pub(super) fn context_at(&self, context: Context) -> usize {
        self.contexts
            .iter()
            .position(|held| *held == context)
            .expect("every context is in the grid")
    }

    /// The number of the kind `after`.
    pub(super) fn after_at(&self, after: After) -> usize {
        self.afters
            .iter()
            .position(|held| *held == after)
            .expect("every kind is in the grid")
    }

    /// The number of the entry called `id`.
    pub(super) fn entry_at(&self, id: &str) -> usize {
        self.entries
            .iter()
            .position(|held| held.id().as_str() == id)
            .unwrap_or_else(|| panic!("{id} does not stand in an expression"))
    }
}

/// Every kind of what an expression may have, in the order the grid is written in.
fn afters() -> Vec<After> {
    let mut afters = vec![After::Start];
    for dtype in Representative::ALL {
        for shape in Shape::ALL {
            afters.push(After::Value { dtype, shape });
        }
    }
    afters.extend(ConditionState::ALL.map(After::Condition));
    afters
}

/// What the engine says at every place of `context`.
fn row(context: Context, afters: &[After]) -> Vec<Vec<Cell>> {
    afters
        .iter()
        .map(|after| {
            allowed::offers(&Place::new(context, *after))
                .iter()
                .map(|offer| match offer.verdict() {
                    allowed::Verdict::Allowed(allowance) => Cell::Allowed {
                        class: allowance.result().and_then(allowed::ResultDtype::class),
                        shape: allowance.shape(),
                        ends: allowance.can_end().is_allowed(),
                    },
                    allowed::Verdict::Refused(refusal) => Cell::Refused(refusal.source()),
                })
                .collect()
        })
        .collect()
}

fn build() -> Grid {
    let contexts = Context::all();
    let afters = afters();
    let entries: Vec<&'static Entry> = catalog::entries()
        .iter()
        .filter(|entry| !entry.applies().is_of_a_step())
        .collect();
    let cells = std::thread::scope(|scope| {
        let workers: Vec<_> = contexts
            .iter()
            .map(|context| {
                let afters = &afters;
                scope.spawn(move || row(*context, afters))
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().expect("the offers of a context are asked"))
            .collect()
    });
    Grid {
        contexts,
        afters,
        entries,
        cells,
    }
}

/// The offers at every place, asked once.
pub(super) fn grid() -> &'static Grid {
    static GRID: OnceLock<Grid> = OnceLock::new();
    GRID.get_or_init(build)
}
