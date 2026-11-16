//! The probe of a part the tables refuse and Polars allows.
//!
//! The editor holds every part of an expression to its types, whether or not
//! Polars evaluates it. Polars skips parts: it folds a comparison with a null,
//! decides a condition by a constant, drops a sort key that is one value, runs a
//! window's keys on groups. Where the tables refuse a part and Polars allows the
//! step, the target does not model what Polars skips. It asks Polars about the
//! part alone, in a document where the part must be evaluated: the chain through
//! a refused link selected on its own, a condition with a refused predicate and
//! literal values selected on its own, either in the same input and, for a part
//! of a pivot's aggregate, in the same pivot. When Polars refuses that for the
//! reason the tables gave, the tables are stricter by design where Polars does not
//! evaluate the part; when it allows it, or refuses it for another reason, the
//! disagreement is a finding.

use super::case::Case;
use super::predict::Refused;

/// What a probe found: the document it ran, and why Polars refused it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settled {
    pub probe: String,
    pub reason: String,
}

/// Asks Polars about the part the tables refused at `refused`, alone. `None`
/// when there is nothing to ask, Polars allows the probe, or it refuses it for
/// another reason than the tables did.
pub fn settle(_case: &Case, _refused: &Refused) -> Option<Settled> {
    None
}

#[cfg(test)]
mod tests {
    //! The probe settles a case only where Polars refuses the part alone for the
    //! tables' reason: a probe that Polars allows, or refuses for another reason,
    //! settles nothing.

    use dageist_engine::catalog;
    use dageist_engine::document::{Place, Segment, StepId};

    use super::super::predict::{Source, Verdict, predict};
    use super::*;

    fn kept(name: &str) -> Case {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/allowed_fuzz/regressions")
            .join(name);
        Case::decode(&std::fs::read(path).expect("a kept case")).expect("a case")
    }

    fn refusal(case: &Case) -> Refused {
        match predict(case).verdict {
            Verdict::Refuses(refused) => refused,
            other => panic!("the tables refuse the case: {other:?}"),
        }
    }

    fn settled(name: &str) -> Settled {
        let case = kept(name);
        settle(&case, &refusal(&case)).expect("a probe that Polars refuses as the tables do")
    }

    #[test]
    fn a_later_predicate_after_a_null_fold_is_refused_alone_for_the_rule() {
        let found = settled("stricter-a-null-folded-before-a-later-predicate.case");
        assert!(found.reason.contains("Boolean"), "{}", found.reason);
        assert!(found.probe.contains("polars.when"), "{}", found.probe);
    }

    #[test]
    fn a_condition_decided_by_a_constant_is_refused_alone_for_its_predicate() {
        let found = settled("stricter-a-condition-decided-by-a-constant.case");
        assert!(found.reason.contains("Boolean"), "{}", found.reason);
        // The part is a pivot's, so the probe is the pivot.
        assert!(found.probe.contains("LazyFrame.pivot"), "{}", found.probe);
    }

    #[test]
    fn a_list_max_that_only_an_outer_window_allows_is_refused_alone_for_the_same_reason() {
        let found = settled("stricter-a-list-max-under-an-outer-window.case");
        assert!(found.reason.contains("not supported"), "{}", found.reason);
        assert!(found.probe.contains("Expr.max"), "{}", found.probe);
    }

    /// A case that selects the one chain `a.cum_sum()` on columns of `receiver`.
    fn cum_sum_of(receiver: &str) -> Case {
        let text = format!(
            "{{\"a\":\"{receiver}\",\"b\":\"Int64\",\"revision\":1}}\n{{\"version\":1,\"steps\":[{{\"id\":\"s\",\"call\":{{\"entry\":\"LazyFrame.select\",\"arguments\":{{\"exprs\":{{\"many\":[{{\"expression\":{{\"start\":{{\"entry\":\"polars.col\",\"arguments\":{{\"name\":{{\"columns\":[\"a\"]}}}}}},\"then\":[{{\"entry\":\"Expr.cum_sum\",\"arguments\":{{}}}}]}}}}]}}}}}}}}]}}"
        );
        Case::decode(text.as_bytes()).expect("a case")
    }

    fn refused_at_the_link(reason: &str) -> Refused {
        let place = Place::of_step(0, Some(StepId::new("s").expect("an identifier")))
            .then(Segment::Parameter("exprs".to_owned()))
            .then(Segment::Item(0))
            .then(Segment::Link(0));
        Refused {
            source: Source::Polars,
            at: place.to_string(),
            place,
            stage: None,
            reason: reason.to_owned(),
        }
    }

    #[test]
    fn a_probe_that_polars_allows_settles_nothing() {
        // `a.cum_sum()` of whole numbers is allowed, though the tables are said
        // to refuse it: nothing shows the tables right.
        let case = cum_sum_of("Int64");
        let refused = refused_at_the_link("`cum_sum` operation not supported for dtype `i64`");
        assert_eq!(settle(&case, &refused), None);
    }

    #[test]
    fn a_probe_that_polars_refuses_for_another_reason_settles_nothing() {
        // Polars refuses `cum_sum` of dates as not supported, which is not a
        // comparison of text with a number.
        let case = cum_sum_of("Date");
        let wrong = refused_at_the_link("cannot compare string with numeric type (u32)");
        assert_eq!(settle(&case, &wrong), None);
        // For the reason the tables give, it is refused alone.
        let right = refused_at_the_link("`cum_sum` operation not supported for dtype `date`");
        assert!(settle(&case, &right).is_some());
    }

    #[test]
    fn the_catalogue_has_what_a_probe_is_made_of() {
        for id in ["LazyFrame.select", "LazyFrame.pivot", "LazyFrame.filter"] {
            assert!(catalog::entry(id).is_some(), "{id}");
        }
    }
}
