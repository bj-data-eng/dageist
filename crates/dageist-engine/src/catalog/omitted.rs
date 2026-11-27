//! The forms of a parameter that the catalogue omits although the entry is catalogued, each with
//! the reason. [`left_out`](super::left_out::left_out) lists whole items; this lists a way of giving a
//! parameter that Polars deprecates, which the engine refuses where it stands, with a rule whose
//! words say what to give instead.
//!
//! A form is omitted because Polars deprecates it, and where it has one because the form has a bug
//! besides, which Polars does not fix for a deprecated form. The deprecation and the bug are the
//! reason: the catalogue offers the current form, which gives the same answers. A test holds each
//! form to the walk: a step that holds it is refused, by the rule the form names, and the words of
//! the refusal give the Polars issue that deprecates it. A later omission of a deprecated form is
//! listed here, and the test is taught how to build a step that holds it.

/// One omitted form of a parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OmittedForm {
    entry: &'static str,
    parameter: &'static str,
    form: &'static str,
    deprecation: &'static str,
    issue: &'static str,
    bug: Option<&'static str>,
    rule: &'static str,
}

impl OmittedForm {
    /// The entry whose parameter takes the form, such as `Expr.is_in`.
    #[must_use]
    pub const fn entry(&self) -> &'static str {
        self.entry
    }

    /// The parameter that takes the form.
    #[must_use]
    pub const fn parameter(&self) -> &'static str {
        self.parameter
    }

    /// What the form is, in words.
    #[must_use]
    pub const fn form(&self) -> &'static str {
        self.form
    }

    /// What Polars says of the form where it deprecates it: its warning.
    #[must_use]
    pub const fn deprecation(&self) -> &'static str {
        self.deprecation
    }

    /// Where Polars deprecates the form: its issue, as `owner/repository#number`.
    #[must_use]
    pub const fn issue(&self) -> &'static str {
        self.issue
    }

    /// The bug that prompted the omission, where the form has one: what Polars does wrong with it.
    #[must_use]
    pub const fn bug(&self) -> Option<&'static str> {
        self.bug
    }

    /// The name of the rule that refuses the form, as the tables write it.
    #[must_use]
    pub const fn rule(&self) -> &'static str {
        self.rule
    }
}

/// Every omitted form, ordered by entry and parameter.
#[must_use]
pub fn omitted_forms() -> &'static [OmittedForm] {
    OMITTED
}

const OMITTED: &[OmittedForm] = &[OmittedForm {
    entry: "Expr.is_in",
    parameter: "other",
    form: "a collection nested as deeply as the value it follows, such as a column of its own dtype",
    deprecation: "`is_in` with a collection of the same datatype is ambiguous and deprecated. \
        Please use `implode` to return to previous behavior.",
    issue: "pola-rs/polars#22149",
    bug: Some(
        "`pl.lit(5).is_in(pl.lit(1))` panics in Polars 2.2.0: it constructs a series of the dtype \
        Unknown(Int(1)), which it does not support",
    ),
    rule: "is-in-takes-a-list",
}];
