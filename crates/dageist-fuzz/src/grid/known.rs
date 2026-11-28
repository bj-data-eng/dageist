//! The lines that name the cells a grid expects not to be agreed on.

use super::Kind;
use super::verdicts::Cell;

/// How a pattern matches the label of one axis.
#[derive(Debug, Clone, Copy)]
enum Matcher {
    Is(&'static str),
    Starts(&'static str),
    OneOf(&'static [&'static str]),
    ContainsOneOf(&'static [&'static str]),
    HasOneOf(&'static [&'static str]),
}

impl Matcher {
    fn matches(self, label: &str) -> bool {
        match self {
            Self::Is(word) => label == word,
            Self::Starts(prefix) => label.starts_with(prefix),
            Self::OneOf(words) => words.contains(&label),
            Self::ContainsOneOf(words) => words.iter().any(|word| label.contains(word)),
            Self::HasOneOf(parts) => label.split(", ").any(|part| parts.contains(&part)),
        }
    }
}

/// The cells that hold certain choices: every part must match the label of its axis. A pattern
/// with no part matches every cell.
#[derive(Debug, Clone, Default)]
pub struct Pattern {
    parts: Vec<(&'static str, Matcher)>,
}

impl Pattern {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The label of `axis` is `label`.
    #[must_use]
    pub fn is(self, axis: &'static str, label: &'static str) -> Self {
        self.part(axis, Matcher::Is(label))
    }

    /// The label of `axis` starts with `prefix`.
    #[must_use]
    pub fn starts(self, axis: &'static str, prefix: &'static str) -> Self {
        self.part(axis, Matcher::Starts(prefix))
    }

    /// The label of `axis` is one of `labels`.
    #[must_use]
    pub fn one_of(self, axis: &'static str, labels: &'static [&'static str]) -> Self {
        self.part(axis, Matcher::OneOf(labels))
    }

    /// The label of `axis` holds one of `words` anywhere in it.
    #[must_use]
    pub fn contains_one_of(self, axis: &'static str, words: &'static [&'static str]) -> Self {
        self.part(axis, Matcher::ContainsOneOf(words))
    }

    /// The label of `axis`, made of parts joined by `", "`, has one of `parts` among them.
    #[must_use]
    pub fn has_one_of(self, axis: &'static str, parts: &'static [&'static str]) -> Self {
        self.part(axis, Matcher::HasOneOf(parts))
    }

    fn part(mut self, axis: &'static str, matcher: Matcher) -> Self {
        self.parts.push((axis, matcher));
        self
    }

    /// The axes the pattern names.
    pub(super) fn axes(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.parts.iter().map(|(axis, _)| *axis)
    }

    pub(super) fn matches(&self, cell: &Cell<impl Clone>) -> bool {
        self.parts
            .iter()
            .all(|(axis, matcher)| cell.label(axis).is_some_and(|label| matcher.matches(label)))
    }
}

/// A line that says why the cells of one kind are expected, and which they are.
#[derive(Debug, Clone)]
pub struct Known<K> {
    kind: K,
    pattern: Pattern,
    reason: &'static str,
    saying: Vec<&'static str>,
    shared: bool,
}

impl<K: Kind> Known<K> {
    /// The cells of `kind` that `pattern` matches are expected, for `reason`.
    #[must_use]
    pub const fn new(kind: K, pattern: Pattern, reason: &'static str) -> Self {
        Self {
            kind,
            pattern,
            reason,
            saying: Vec::new(),
            shared: false,
        }
    }

    /// The same line, held by grids that may have no cell for it, so that one with none does
    /// not make it stale there.
    #[must_use]
    pub const fn shared(mut self) -> Self {
        self.shared = true;
        self
    }

    /// The same line, which names only the cells whose words hold `words`.
    #[must_use]
    pub fn saying(mut self, words: &'static str) -> Self {
        self.saying = vec![words];
        self
    }

    /// The same line, which names only the cells whose words hold one of `words`.
    #[must_use]
    pub fn saying_one_of(mut self, words: &[&'static str]) -> Self {
        self.saying = words.to_vec();
        self
    }

    #[must_use]
    pub const fn kind(&self) -> K {
        self.kind
    }

    #[must_use]
    pub const fn reason(&self) -> &'static str {
        self.reason
    }

    #[must_use]
    pub const fn is_shared(&self) -> bool {
        self.shared
    }

    pub(super) const fn pattern(&self) -> &Pattern {
        &self.pattern
    }

    /// How many things the line says of a cell: the parts of its pattern, and the words. Where a
    /// cell is named by more than one line, the most specific one counts it, so that a wide line
    /// cannot starve a narrow one.
    pub(super) fn specificity(&self) -> usize {
        self.pattern.parts.len() + usize::from(!self.saying.is_empty())
    }

    pub(super) fn names<O: super::Outcome<Kind = K>>(&self, cell: &Cell<O>) -> bool {
        self.kind == cell.outcome.kind()
            && self.pattern.matches(cell)
            && (self.saying.is_empty() || {
                let said = cell.outcome.said();
                self.saying.iter().any(|word| said.contains(word))
            })
    }
}
