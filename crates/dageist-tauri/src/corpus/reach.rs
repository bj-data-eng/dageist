//! Which places a chain can be brought to, and by which methods: the engine's offers read as a
//! graph. A chain starts from a field of the class of dtype it is for, or from `element` in a
//! pivot, and each method the engine allows after a value of one class and shape and says gives
//! another is an edge to it.

use std::collections::{HashMap, VecDeque};

use dageist_engine::allowed::{After, Context, Outer, Representative, Shape};
use dageist_engine::catalog::{Applies, ParameterDefault};

use super::grid::{Cell, Grid};

/// What a chain is made of to stand after a value: where it starts, and the methods that follow,
/// each with what it follows.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Plan {
    /// The class of the field the chain starts from, or of the values of the pivot's group.
    pub root: Representative,
    /// The methods, in order, each with the place it stands at.
    pub links: Vec<(&'static str, After)>,
}

/// The function that starts a chain in `outer`: a field, or `element` in a pivot.
pub(super) const fn starting_entry(outer: Outer) -> &'static str {
    match outer {
        Outer::PivotAggregate => "polars.element",
        _ => "polars.col",
    }
}

/// The classes and shapes a chain in `context` can be brought to, by the way the engine's offers
/// lead from a start, shortest first and, among equals, in the catalogue's order.
pub(super) fn reach(grid: &Grid, context: Context) -> HashMap<(Representative, Shape), Plan> {
    let at = grid.context_at(context);
    let start = grid.after_at(After::Start);
    let source = grid.entry_at(starting_entry(context.outer()));
    // A way is made of the methods that need the least given first, so that a prefix is not made
    // of calls whose arguments are the matter of the chain.
    let mut order: Vec<usize> = (0..grid.entries.len())
        .filter(|number| grid.entries[*number].applies() == Applies::Expression)
        .collect();
    order.sort_by_key(|number| {
        grid.entries[*number]
            .parameters()
            .iter()
            .filter(|parameter| {
                parameter.default() == ParameterDefault::Required || parameter.is_needed()
            })
            .count()
    });
    let mut found: HashMap<(Representative, Shape), Plan> = HashMap::new();
    let mut queue: VecDeque<(Representative, Shape)> = VecDeque::new();
    if matches!(grid.cell(at, start, source), Cell::Allowed { .. }) {
        for class in Representative::ALL {
            found.insert(
                (class, Shape::Many),
                Plan {
                    root: class,
                    links: Vec::new(),
                },
            );
            queue.push_back((class, Shape::Many));
        }
    }
    while let Some((class, shape)) = queue.pop_front() {
        let after = After::Value {
            dtype: class,
            shape,
        };
        let from = grid.after_at(after);
        let plan = found[&(class, shape)].clone();
        for &number in &order {
            let entry = grid.entries[number];
            let Cell::Allowed {
                class: Some(next),
                shape: gives,
                ..
            } = grid.cell(at, from, number)
            else {
                continue;
            };
            if found.contains_key(&(next, gives)) {
                continue;
            }
            let mut links = plan.links.clone();
            links.push((entry.id().as_str(), after));
            found.insert(
                (next, gives),
                Plan {
                    root: plan.root,
                    links,
                },
            );
            queue.push_back((next, gives));
        }
    }
    found
}
