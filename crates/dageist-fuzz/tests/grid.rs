//! The grid's contracts, on a toy judge that says two numbers agree where their sum is even.
//!
//! The toy is no product property: it is a rule the test states, so that what each cell must
//! become is known without the grid.

use std::collections::BTreeSet;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Mutex;

use dageist_fuzz::generation::Entropy;
use dageist_fuzz::grid::{Choices, Grid, Judged, Kind, Known, Outcome, Pattern, Scale};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Toy {
    Agree,
    Differ,
    Unbuildable,
}

impl Kind for Toy {
    const ALL: &'static [Self] = &[Self::Agree, Self::Differ, Self::Unbuildable];

    fn words(self) -> &'static str {
        match self {
            Self::Agree => "agree",
            Self::Differ => "differ",
            Self::Unbuildable => "unbuildable",
        }
    }

    fn is_agreed(self) -> bool {
        self == Self::Agree
    }

    fn is_judged(self) -> bool {
        self != Self::Unbuildable
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Said {
    Agree,
    Differ(u32),
    Unbuildable(&'static str),
}

impl Outcome for Said {
    type Kind = Toy;

    fn kind(&self) -> Toy {
        match self {
            Self::Agree => Toy::Agree,
            Self::Differ(_) => Toy::Differ,
            Self::Unbuildable(_) => Toy::Unbuildable,
        }
    }

    fn said(&self) -> String {
        match self {
            Self::Agree => String::new(),
            Self::Differ(sum) => format!("the sum {sum} is odd"),
            Self::Unbuildable(why) => (*why).to_owned(),
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct Point {
    left: u32,
    right: u32,
}

fn numbers(name: &'static str, values: &[u32]) -> Choices<u32> {
    Choices::of(name, values.iter().map(|value| (value.to_string(), *value)))
}

fn grid(left: &[u32], right: &[u32]) -> Grid<Point> {
    Grid::<Point>::new("toy")
        .axis(numbers("left", left), |point, value| point.left = *value)
        .axis(numbers("right", right), |point, value| point.right = *value)
}

/// The toy judge: an even sum agrees, an odd one differs, and a left of 9 cannot be built.
fn toy(point: &Point) -> Judged<Said> {
    let said = if point.left == 9 {
        Said::Unbuildable("nine cannot be built")
    } else if (point.left + point.right).is_multiple_of(2) {
        Said::Agree
    } else {
        Said::Differ(point.left + point.right)
    };
    Judged::new(said).decided_by(if point.right > 1 { "large" } else { "small" })
}

fn failure_of(run: impl FnOnce()) -> String {
    let payload = catch_unwind(AssertUnwindSafe(run)).expect_err("the assertion fails");
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| {
            payload
                .downcast_ref::<&str>()
                .map(|text| (*text).to_owned())
        })
        .expect("a failure message")
}

#[test]
fn every_combination_of_one_choice_from_each_axis_is_judged_once_with_the_last_axis_fastest() {
    let grid = grid(&[1, 2], &[10, 20, 30]);
    assert_eq!(grid.size(), 6);
    let seen = Mutex::new(Vec::new());
    let verdicts = grid.judge(|point| {
        seen.lock().expect("no panic").push(point.clone());
        Judged::new(Said::Agree)
    });
    let seen: BTreeSet<(u32, u32)> = seen
        .into_inner()
        .expect("no panic")
        .into_iter()
        .map(|point| (point.left, point.right))
        .collect();
    let expected: BTreeSet<(u32, u32)> = [1, 2]
        .into_iter()
        .flat_map(|left| [10, 20, 30].into_iter().map(move |right| (left, right)))
        .collect();
    assert_eq!(seen, expected);
    assert_eq!(verdicts.count(Toy::Agree), 6);
    assert_eq!(
        grid.labels(1),
        vec![("left", "1".to_owned()), ("right", "20".to_owned())]
    );
    assert_eq!(grid.point(3), Point { left: 2, right: 10 });
}

#[test]
fn the_cells_not_agreed_on_are_kept_in_the_grids_order_and_the_agreed_ones_only_counted() {
    let verdicts = grid(&[1, 2, 9], &[1, 2]).judge(toy);
    let listed: Vec<String> = verdicts.listed().map(|cell| cell.name()).collect();
    assert_eq!(
        listed,
        [
            "left=1, right=2",
            "left=2, right=1",
            "left=9, right=1",
            "left=9, right=2"
        ]
    );
    assert_eq!(verdicts.count(Toy::Agree), 2);
    assert_eq!(verdicts.count(Toy::Differ), 2);
    assert_eq!(verdicts.count(Toy::Unbuildable), 2);
    assert_eq!(verdicts.judged(), 4);
}

#[test]
fn a_grid_whose_every_cell_is_agreed_on_or_named_passes() {
    let verdicts = grid(&[1, 2, 9], &[1, 2]).judge(toy);
    verdicts.assert_agree(
        &[
            Known::new(Toy::Differ, Pattern::new(), "an odd sum differs").saying("is odd"),
            Known::new(
                Toy::Unbuildable,
                Pattern::new().is("left", "9"),
                "nine is no number",
            ),
        ],
        4,
    );
}

#[test]
fn a_cell_no_line_names_fails_the_grid_with_its_name_and_what_was_said() {
    let verdicts = grid(&[1, 2], &[1, 2]).judge(toy);
    let message = failure_of(|| verdicts.assert_agree(&[], 0));
    assert!(message.contains("2 cells that no line names"), "{message}");
    assert!(
        message.contains("left=1, right=2: the sum 3 is odd"),
        "{message}"
    );
    assert!(
        message.contains("left=2, right=1: the sum 3 is odd"),
        "{message}"
    );
}

#[test]
fn a_line_whose_words_are_not_said_of_a_cell_does_not_name_it() {
    let verdicts = grid(&[1, 2], &[1, 2]).judge(toy);
    let message = failure_of(|| {
        verdicts.assert_agree(
            &[Known::new(Toy::Differ, Pattern::new(), "an even sum").saying("is even")],
            0,
        );
    });
    assert!(message.contains("2 cells that no line names"), "{message}");
}

#[test]
fn a_line_that_names_no_cell_is_stale_unless_it_is_shared() {
    let verdicts = grid(&[2], &[2]).judge(toy);
    let line = || Known::new(Toy::Differ, Pattern::new().is("left", "3"), "three differs");
    let message = failure_of(|| verdicts.assert_agree(&[line()], 0));
    assert!(message.contains("stale: three differs"), "{message}");
    verdicts.assert_agree(&[line().shared()], 0);
}

#[test]
fn a_line_that_names_an_axis_the_grid_lacks_fails_the_grid() {
    let verdicts = grid(&[1], &[2]).judge(toy);
    let message = failure_of(|| {
        verdicts.assert_agree(
            &[
                Known::new(Toy::Differ, Pattern::new().is("left", "1"), "one differs"),
                Known::new(
                    Toy::Differ,
                    Pattern::new().is("lfet", "1"),
                    "a slip of the axis",
                ),
            ],
            0,
        );
    });
    assert!(
        message.contains("names the axis lfet, which the grid does not have"),
        "{message}"
    );
}

#[test]
fn the_most_specific_line_counts_a_cell_that_several_lines_name() {
    let verdicts = grid(&[1, 2], &[1, 2]).judge(toy);
    let known = [
        Known::new(Toy::Differ, Pattern::new(), "every odd sum"),
        Known::new(
            Toy::Differ,
            Pattern::new().is("left", "1").is("right", "2"),
            "one and two",
        ),
    ];
    let tally = verdicts.tally(&known);
    let counted: Vec<(&str, usize)> = tally
        .known
        .iter()
        .map(|line| (line.reason, line.cells))
        .collect();
    assert_eq!(counted, [("every odd sum", 1), ("one and two", 1)]);
}

#[test]
fn a_grid_that_judged_fewer_cells_than_it_must_fails_and_unbuildable_cells_do_not_count() {
    let verdicts = grid(&[2, 9], &[2]).judge(toy);
    let known = [Known::new(Toy::Unbuildable, Pattern::new(), "nine")];
    verdicts.assert_agree(&known, 1);
    let message = failure_of(|| verdicts.assert_agree(&known, 2));
    assert!(
        message.contains("only 1 cells were judged, fewer than 2"),
        "{message}"
    );
}

#[test]
fn the_patterns_match_a_label_whole_by_its_start_among_several_by_a_word_in_it_and_by_a_part() {
    let chains = Choices::of(
        "chain",
        [("abs, sum", 0_u32), ("neg", 1), ("abs_diff, max", 2)],
    );
    let verdicts = Grid::new("chains")
        .axis(chains, |point: &mut Point, value| point.left = *value)
        .judge(|_| Judged::new(Said::Differ(1)));
    let cells = |pattern: Pattern| {
        verdicts
            .tally(&[Known::new(Toy::Differ, pattern, "the line")])
            .known[0]
            .cells
    };
    assert_eq!(cells(Pattern::new().is("chain", "neg")), 1);
    assert_eq!(cells(Pattern::new().starts("chain", "abs")), 2);
    assert_eq!(
        cells(Pattern::new().one_of("chain", &["neg", "abs, sum"])),
        2
    );
    assert_eq!(cells(Pattern::new().contains_one_of("chain", &["diff"])), 1);
    // A part is a whole part of the label: `abs` is one of `abs, sum`, not of `abs_diff, max`.
    assert_eq!(cells(Pattern::new().has_one_of("chain", &["abs"])), 1);
    assert_eq!(
        cells(Pattern::new().has_one_of("chain", &["max", "sum"])),
        2
    );
    assert_eq!(cells(Pattern::new()), 3);
}

#[test]
fn an_axis_names_what_it_leaves_out_and_a_choice_it_refuses_is_left_out_with_the_reason() {
    let axis = numbers("n", &[1, 2, 3, 4])
        .leave_out("0", "zero is no count")
        .keep_unless(|_, value| (*value > 3).then(|| format!("{value} is too many")));
    assert_eq!(axis.labels().collect::<Vec<_>>(), ["1", "2", "3"]);
    let left: Vec<(&str, &str)> = axis
        .left_out()
        .iter()
        .map(|left| (left.label.as_str(), left.reason.as_str()))
        .collect();
    assert_eq!(left, [("0", "zero is no count"), ("4", "4 is too many")]);
    let verdicts = Grid::new("left out")
        .axis(axis, |point: &mut Point, value| point.left = *value)
        .judge(|_| Judged::new(Said::Agree));
    let tally = verdicts.tally(&[]);
    assert_eq!(tally.axes[0].choices, 3);
    assert_eq!(tally.axes[0].left_out.len(), 2);
}

#[test]
#[should_panic(expected = "the label \"2\" is on the axis n twice")]
fn a_label_given_twice_on_an_axis_is_refused() {
    let _ = numbers("n", &[1, 2]).with("2", 7);
}

#[test]
#[should_panic(expected = "the label \"1\" is on the axis n twice")]
fn a_label_left_out_and_kept_is_refused() {
    let _ = numbers("n", &[1, 2]).leave_out("1", "one");
}

#[test]
#[should_panic(expected = "the axis left is on the grid twice")]
fn an_axis_given_twice_is_refused() {
    let _ = grid(&[1], &[1]).axis(numbers("left", &[2]), |point, value| point.left = *value);
}

#[test]
fn a_grid_of_normal_scale_keeps_every_nth_choice_and_a_large_one_keeps_them_all() {
    let values: Vec<u32> = (0..10).collect();
    let normal = numbers("n", &values).thin(Scale::Normal, 3);
    assert_eq!(normal.labels().collect::<Vec<_>>(), ["0", "3", "6", "9"]);
    let thinned = normal.thinned().expect("thinned");
    assert_eq!(
        (thinned.rule.as_str(), thinned.kept, thinned.of),
        ("one choice in every 3", 4, 10)
    );
    let large = numbers("n", &values).thin(Scale::Large, 3);
    assert_eq!(large.len(), 10);
    assert!(large.thinned().is_none());
}

#[test]
fn a_grid_of_normal_scale_keeps_the_first_choice_of_each_kind_and_a_large_one_keeps_them_all() {
    let values: Vec<u32> = (0..10).collect();
    let parity = |_: &str, value: &u32| value % 3;
    let normal = numbers("n", &values).one_of_each(Scale::Normal, "remainder of three", parity);
    assert_eq!(normal.labels().collect::<Vec<_>>(), ["0", "1", "2"]);
    let thinned = normal.thinned().expect("thinned");
    assert_eq!(thinned.rule, "one of each remainder of three");
    let large = numbers("n", &values).one_of_each(Scale::Large, "remainder of three", parity);
    assert_eq!(large.len(), 10);
}

#[test]
fn two_axes_join_with_what_each_leaves_out() {
    let joined = numbers("n", &[1])
        .leave_out("0", "zero")
        .and(numbers("other", &[2]).leave_out("3", "three"));
    assert_eq!(joined.name(), "n");
    assert_eq!(joined.labels().collect::<Vec<_>>(), ["1", "2"]);
    assert_eq!(joined.left_out().len(), 2);
}

#[test]
fn the_tally_counts_each_kind_what_decided_each_cell_and_is_written_as_json() {
    let verdicts = grid(&[1, 2, 9], &[1, 2]).judge(toy);
    let known = [
        Known::new(Toy::Differ, Pattern::new(), "an odd sum"),
        Known::new(Toy::Unbuildable, Pattern::new(), "nine"),
        Known::new(Toy::Differ, Pattern::new().is("left", "7"), "seven").shared(),
    ];
    let tally = verdicts.tally(&known);
    assert_eq!(tally.cells, 6);
    assert_eq!(tally.judged, 4);
    assert_eq!(tally.kinds["agree"], 2);
    assert_eq!(tally.kinds["differ"], 2);
    assert_eq!(tally.kinds["unbuildable"], 2);
    assert_eq!(tally.decided_by["small"], 3);
    assert_eq!(tally.decided_by["large"], 3);
    assert_eq!(tally.unnamed, 0);
    assert!(tally.stale.is_empty());
    let folder = std::env::temp_dir().join(format!("dageist-grid-tally-{}", std::process::id()));
    let path = tally.write_to(&folder).expect("the tally is written");
    assert_eq!(path, folder.join("toy.json"));
    let written: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("the tally is read")).expect("JSON");
    assert_eq!(written["cells"], 6);
    assert_eq!(written["known"][0]["cells"], 2);
    assert_eq!(written["axes"][1]["name"], "right");
    std::fs::remove_dir_all(&folder).expect("the folder is removed");
}

#[test]
fn a_campaign_draws_a_cell_of_the_grid_from_its_entropy() {
    let grid = grid(&[1, 2], &[10, 20, 30]);
    let bytes: Vec<u8> = (0_u8..64).collect();
    let mut entropy = Entropy::new(&bytes);
    let mut drawn = BTreeSet::new();
    for _ in 0..8 {
        let index = grid.draw(&mut entropy).expect("a cell");
        assert!(index < grid.size());
        drawn.insert(index);
    }
    assert!(grid.draw(&mut entropy).is_err(), "the entropy has run out");
    assert!(!drawn.is_empty());
    let empty: Grid<Point> = Grid::new("empty").axis(Choices::new("none"), |_, _: &u32| {});
    assert!(empty.draw(&mut Entropy::new(&bytes)).is_err());
}
