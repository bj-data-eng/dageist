//! What the host's tests write as fixtures for the view, built from the views
//! of a report: the words of its closed lists, and a report built by hand.

use serde::Serialize;

use dageist_engine::allowed::{After, ConditionState, Context, Nested, Outer, Shape, Stage};
use dageist_engine::document::Change;

use super::*;

/// The words of every closed vocabulary in a report, written out by the host
/// from the engine's own lists of variants, for the view's tests to check its
/// decoders against.
pub(crate) fn vocabulary() -> serde_json::Value {
    /// Every view of a list, serialized to its word.
    fn words<T: Serialize>(views: &[T]) -> Vec<serde_json::Value> {
        views
            .iter()
            .map(|view| serde_json::to_value(view).expect("a view serializes"))
            .collect()
    }

    use dageist_engine::allowed::Rule;

    serde_json::json!({
        "outer": Outer::ALL.map(Outer::name),
        "nested": Nested::ALL.map(Nested::name),
        "rules": Rule::ALL.map(Rule::name),
        "stages": words(&Stage::ALL.map(stage_view)),
        "conditionStates": words(&ConditionState::ALL.map(condition_state_view)),
        "valueShapes": words(&Shape::ALL.map(shape_view)),
        "changes": words(&Change::ALL.map(change_view)),
        "errorKinds": words(&crate::sample_table::ErrorKind::ALL),
    })
}

/// A run built by hand to hold every variant that no real document gives
/// together, so that the view's decoders are tested on each of them. It is not
/// something the engine reports; it is what the host would send if it did.
pub(crate) fn variants() -> RunView {
    use dageist_engine::allowed::Rule;

    let place = |path: Vec<SegmentView>| PlaceView {
        step_index: Some(0),
        step_id: Some("variants".to_owned()),
        path,
    };
    let refusal = |source: RefusalSourceView| RefusalView {
        source,
        reason: "the reason".to_owned(),
    };
    let refused = |id: &str, cause: CauseView| StepReportView {
        id: id.to_owned(),
        summary: "by a".to_owned(),
        lazy: true,
        outcome: OutcomeView::Refused {
            place: PlaceView {
                step_index: Some(1),
                step_id: Some(id.to_owned()),
                path: vec![SegmentView::Start],
            },
            cause,
        },
        count: CountView::NotCounted,
        expressions: Vec::new(),
        walk: WalkView {
            expressions: Vec::new(),
            refusal: None,
            not_known: None,
        },
        at_default: Vec::new(),
    };
    let path = vec![
        SegmentView::Parameter {
            name: "named_exprs".to_owned(),
        },
        SegmentView::Key { index: 1 },
        SegmentView::Start,
    ];
    let variants = StepReportView {
        id: "variants".to_owned(),
        summary: "b".to_owned(),
        lazy: false,
        outcome: OutcomeView::Valid {
            columns: vec![
                FieldView {
                    name: "a".to_owned(),
                    dtype: "Int64".to_owned(),
                },
                FieldView {
                    name: "b".to_owned(),
                    dtype: "String".to_owned(),
                },
            ],
            changes: vec![
                ColumnChangeView {
                    name: "a".to_owned(),
                    change: change_view(Change::Replaced),
                },
                ColumnChangeView {
                    name: "b".to_owned(),
                    change: change_view(Change::Added),
                },
            ],
        },
        count: CountView::Counted {
            received: 9,
            kept: 6,
            first: vec![ColumnValuesView {
                name: "b".to_owned(),
                values: ValuesView {
                    shown: vec![Some("x".to_owned()), None],
                    more: 4,
                },
            }],
        },
        expressions: vec![ExpressionDtypesView {
            place: place(path.clone()),
            start: ResolutionView::Several {
                dtypes: vec!["Int64".to_owned(), "String".to_owned()],
            },
            links: vec![ResolutionView::Refused {
                reason: "no".to_owned(),
            }],
        }],
        walk: WalkView {
            expressions: vec![WalkedExpressionView {
                place: place(path.clone()),
                context: Some(context_view(Context::inside(
                    Outer::Filter,
                    Nested::OrderBy,
                ))),
                calls: vec![
                    StandingView {
                        follows: Some(after_view(After::Condition(ConditionState::ChainedWhen))),
                        verdict: Some(VerdictView::Refused {
                            refusal: refusal(RefusalSourceView::Structure),
                        }),
                    },
                    StandingView {
                        follows: Some(after_view(After::Condition(ConditionState::ChainedThen))),
                        verdict: None,
                    },
                ],
                places: vec![
                    Some(OfferPlaceView {
                        context: context_view(Context::step(Outer::Sort)),
                        after: after_view(After::Condition(ConditionState::ChainedThen)),
                    }),
                    None,
                ],
                fields: vec![
                    Some(StructView {
                        dtype: "Struct(x: Int64)".to_owned(),
                        fields: Some(vec![FieldView {
                            name: "x".to_owned(),
                            dtype: "Int64".to_owned(),
                        }]),
                    }),
                    Some(StructView {
                        dtype: "Struct".to_owned(),
                        fields: None,
                    }),
                ],
                ending: Some(EndingView::NotKnown {
                    because: "the dtype is not known".to_owned(),
                }),
            }],
            refusal: Some(WalkRefusalView {
                place: place(path),
                refusal: refusal(RefusalSourceView::Polars {
                    stage: StageView::Empty,
                }),
                at_end: true,
            }),
            not_known: Some("why".to_owned()),
        },
        at_default: vec![
            place(vec![SegmentView::Parameter {
                name: "descending".to_owned(),
            }]),
            place(vec![
                SegmentView::Parameter {
                    name: "named_exprs".to_owned(),
                },
                SegmentView::Value { index: 0 },
                SegmentView::Link { index: 1 },
                SegmentView::Parameter {
                    name: "mapping_strategy".to_owned(),
                },
            ]),
        ],
    };
    let mut failed = refused(
        "failed",
        CauseView::Polars {
            reason: "the reason".to_owned(),
        },
    );
    failed.outcome = OutcomeView::Valid {
        columns: Vec::new(),
        changes: Vec::new(),
    };
    failed.count = CountView::Failed {
        reason: "no rows".to_owned(),
    };
    let mut not_reached = refused(
        "not_reached",
        CauseView::Polars {
            reason: "the reason".to_owned(),
        },
    );
    not_reached.outcome = OutcomeView::Valid {
        columns: Vec::new(),
        changes: Vec::new(),
    };
    not_reached.count = CountView::NotReached {
        failed: "failed".to_owned(),
    };
    let mut rows_stage = refused(
        "rows_stage",
        CauseView::NotAllowed {
            refusal: refusal(RefusalSourceView::Polars {
                stage: StageView::Rows,
            }),
        },
    );
    rows_stage.lazy = false;
    RunView {
        source: Vec::new(),
        steps: vec![
            variants,
            refused(
                "unknown_table",
                CauseView::UnknownTable {
                    name: "nope".to_owned(),
                },
            ),
            refused(
                "unknown_input",
                CauseView::UnknownInput {
                    name: "none".to_owned(),
                },
            ),
            refused(
                "build",
                CauseView::Build {
                    reason: "a parameter is missing".to_owned(),
                },
            ),
            refused(
                "rule",
                CauseView::NotAllowed {
                    refusal: refusal(RefusalSourceView::Rule {
                        rule: Rule::ElementOnlyInPivotAggregate.name(),
                    }),
                },
            ),
            rows_stage,
            // Polars refuses a step that the tables allow: a step the tables refuse is not run.
            refused(
                "polars",
                CauseView::Polars {
                    reason: "the reason".to_owned(),
                },
            ),
            failed,
            not_reached,
        ],
    }
}
