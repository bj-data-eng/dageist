//! The choices of one axis.

/// How much of a grid a run judges. A run judges every cell of a grid unless it asks for a quick
/// one, which keeps of each thinned axis the choices its rule keeps; a grid says by [`Choices::thin`]
/// and [`Choices::one_of_each`] how an axis is thinned, and the tally says how it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scale {
    /// Every choice of every axis.
    Full,
    /// The choices each thinned axis keeps.
    Quick,
}

impl Scale {
    /// The scale `DAGEIST_GRID_SCALE` asks for: `quick`, or `full` where it is unset.
    ///
    /// # Panics
    ///
    /// When it is set to another word, which no run should mistake for either.
    #[must_use]
    pub fn of_run() -> Self {
        let words = std::env::var("DAGEIST_GRID_SCALE").ok();
        Self::from_words(words.as_deref()).unwrap_or_else(|why| panic!("{why}"))
    }

    /// The scale `words` name: `quick`, or `full` where they are none.
    ///
    /// # Errors
    ///
    /// When the words name neither.
    pub fn from_words(words: Option<&str>) -> Result<Self, String> {
        match words.map(str::trim) {
            None | Some("" | "full") => Ok(Self::Full),
            Some("quick") => Ok(Self::Quick),
            Some(other) => Err(format!(
                "DAGEIST_GRID_SCALE is `full` or `quick`, not {other:?}"
            )),
        }
    }
}

/// An axis taken apart: its name, choices, what it leaves out and how it was thinned.
pub(super) type Parts<V> = (
    &'static str,
    Vec<(String, V)>,
    Vec<LeftOut>,
    Option<Thinned>,
);

/// A choice an axis leaves out, and why. It is no cell: a tally names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeftOut {
    pub label: String,
    pub reason: String,
}

/// How a quick run thinned an axis: by `rule`, keeping `kept` of `of` choices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thinned {
    pub rule: String,
    pub kept: usize,
    pub of: usize,
}

/// The labelled values of one axis, with the choices it leaves out and why.
///
/// Labels of an axis differ, for a [`Pattern`](super::Pattern) names a cell by them. A label made
/// of parts, such as a chain of calls, joins them with `", "`, which
/// [`Pattern::has_one_of`](super::Pattern::has_one_of) reads.
#[derive(Debug, Clone)]
pub struct Choices<V> {
    name: &'static str,
    choices: Vec<(String, V)>,
    left_out: Vec<LeftOut>,
    thinned: Option<Thinned>,
}

impl<V> Choices<V> {
    /// An axis named `name` with no choice yet.
    #[must_use]
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            choices: Vec::new(),
            left_out: Vec::new(),
            thinned: None,
        }
    }

    /// An axis named `name` with `choices`.
    ///
    /// # Panics
    ///
    /// When a label is repeated.
    #[must_use]
    pub fn of<L: Into<String>>(
        name: &'static str,
        choices: impl IntoIterator<Item = (L, V)>,
    ) -> Self {
        choices
            .into_iter()
            .fold(Self::new(name), |axis, (label, value)| {
                axis.with(label, value)
            })
    }

    /// The same axis with one more choice.
    ///
    /// # Panics
    ///
    /// When the label is already on the axis, or already left out of it.
    #[must_use]
    pub fn with(mut self, label: impl Into<String>, value: V) -> Self {
        let label = label.into();
        assert!(
            !self.has(&label),
            "the label {label:?} is on the axis {} twice",
            self.name
        );
        self.choices.push((label, value));
        self
    }

    /// The same axis, which names `label` as left out of it for `reason`.
    ///
    /// # Panics
    ///
    /// When the label is already on the axis, or already left out of it.
    #[must_use]
    pub fn leave_out(mut self, label: impl Into<String>, reason: impl Into<String>) -> Self {
        let label = label.into();
        assert!(
            !self.has(&label),
            "the label {label:?} is on the axis {} twice",
            self.name
        );
        self.left_out.push(LeftOut {
            label,
            reason: reason.into(),
        });
        self
    }

    /// The same axis, with each choice that `refuses` gives a reason for left out with it.
    #[must_use]
    pub fn keep_unless(mut self, refuses: impl Fn(&str, &V) -> Option<String>) -> Self {
        let mut kept = Vec::with_capacity(self.choices.len());
        for (label, value) in self.choices {
            match refuses(&label, &value) {
                Some(reason) => self.left_out.push(LeftOut { label, reason }),
                None => kept.push((label, value)),
            }
        }
        self.choices = kept;
        self
    }

    /// The same axis, which at quick scale keeps every `every`th choice, the first among them,
    /// and at full scale keeps every choice. A tally says how it was thinned.
    ///
    /// # Panics
    ///
    /// When `every` is zero, or the axis is thinned twice.
    #[must_use]
    pub fn thin(self, scale: Scale, every: usize) -> Self {
        assert!(
            every > 0,
            "an axis is thinned to every first choice at least"
        );
        if every == 1 {
            return self;
        }
        self.thinned_by(scale, format!("one choice in every {every}"), |at, _, _| {
            at % every == 0
        })
    }

    /// The same axis, which at quick scale keeps the first choice of each kind that `kind` says,
    /// such as one parameter of each role, and at full scale keeps every choice. `rule` says
    /// in words what a kind is, which the tally says.
    ///
    /// # Panics
    ///
    /// When the axis is thinned twice.
    #[must_use]
    pub fn one_of_each<K: Ord>(
        self,
        scale: Scale,
        rule: &str,
        kind: impl Fn(&str, &V) -> K,
    ) -> Self {
        let mut seen = std::collections::BTreeSet::new();
        self.thinned_by(scale, format!("one of each {rule}"), |_, label, value| {
            seen.insert(kind(label, value))
        })
    }

    fn thinned_by(
        mut self,
        scale: Scale,
        rule: String,
        mut keeps: impl FnMut(usize, &str, &V) -> bool,
    ) -> Self {
        assert!(
            self.thinned.is_none(),
            "the axis {} is thinned twice",
            self.name
        );
        if scale == Scale::Full {
            return self;
        }
        let of = self.choices.len();
        self.choices = self
            .choices
            .into_iter()
            .enumerate()
            .filter(|(at, (label, value))| keeps(*at, label, value))
            .map(|(_, choice)| choice)
            .collect();
        self.thinned = Some(Thinned {
            rule,
            kept: self.choices.len(),
            of,
        });
        self
    }

    /// The same axis with the choices of `other` after its own, and what either leaves out that
    /// the other does not hold: a choice of one replaces the same label left out of the other.
    ///
    /// # Panics
    ///
    /// When a label is a choice of both, or either was thinned.
    #[must_use]
    pub fn and(mut self, other: Self) -> Self {
        assert!(
            self.thinned.is_none() && other.thinned.is_none(),
            "an axis is joined before it is thinned"
        );
        let held: Vec<String> = other
            .choices
            .iter()
            .map(|(label, _)| label.clone())
            .collect();
        self.left_out.retain(|left| !held.contains(&left.label));
        let mut joined = other
            .choices
            .into_iter()
            .fold(self, |axis, (label, value)| axis.with(label, value));
        for left in other.left_out {
            if !joined.has(&left.label) {
                joined = joined.leave_out(left.label, left.reason);
            }
        }
        joined
    }

    /// The same axis with each value made into another.
    #[must_use]
    pub fn map<W>(self, mut make: impl FnMut(V) -> W) -> Choices<W> {
        Choices {
            name: self.name,
            choices: self
                .choices
                .into_iter()
                .map(|(label, value)| (label, make(value)))
                .collect(),
            left_out: self.left_out,
            thinned: self.thinned,
        }
    }

    /// The same axis with each choice labelled by `label`, and what it leaves out as it was.
    ///
    /// # Panics
    ///
    /// When two choices are given one label.
    #[must_use]
    pub fn relabel(self, label: impl Fn(&str, &V) -> String) -> Self {
        let mut axis = Self::new(self.name);
        axis.thinned = self.thinned;
        for (old, value) in self.choices {
            axis = axis.with(label(&old, &value), value);
        }
        for left in self.left_out {
            axis = axis.leave_out(left.label, left.reason);
        }
        axis
    }

    /// The same choices on an axis of another name.
    #[must_use]
    pub fn named(mut self, name: &'static str) -> Self {
        self.name = name;
        self
    }

    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.choices.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.choices.is_empty()
    }

    /// The labels of the choices, in order.
    pub fn labels(&self) -> impl Iterator<Item = &str> {
        self.choices.iter().map(|(label, _)| label.as_str())
    }

    /// The choices, in order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &V)> {
        self.choices
            .iter()
            .map(|(label, value)| (label.as_str(), value))
    }

    /// The value of the choice labelled `label`.
    #[must_use]
    pub fn get(&self, label: &str) -> Option<&V> {
        self.choices
            .iter()
            .find(|(held, _)| held == label)
            .map(|(_, value)| value)
    }

    /// What the axis leaves out, with why.
    #[must_use]
    pub fn left_out(&self) -> &[LeftOut] {
        &self.left_out
    }

    /// How a quick run thinned the axis, if it did.
    #[must_use]
    pub const fn thinned(&self) -> Option<&Thinned> {
        self.thinned.as_ref()
    }

    fn has(&self, label: &str) -> bool {
        self.choices.iter().any(|(held, _)| held == label)
            || self.left_out.iter().any(|left| left.label == label)
    }

    pub(super) fn into_parts(self) -> Parts<V> {
        (self.name, self.choices, self.left_out, self.thinned)
    }
}
