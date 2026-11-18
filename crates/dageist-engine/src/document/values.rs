//! Values written out as text, for the view to show as they are: the first
//! few of a column or of a connected list.

use polars::prelude::{AnyValue, Column, PolarsError, Series};

/// How many values of a column or a list are written out.
pub const FIRST_VALUES: usize = 4;

/// A value as the text Polars writes it, or `None` where it is null, which no
/// text stands for.
pub(super) fn text_of(value: &AnyValue<'_>) -> Option<String> {
    (!value.is_null()).then(|| value.str_value().into_owned())
}

/// The first values of a column or a list, each written as Polars writes it,
/// and how many more there are.
///
/// A value is `None` where it is null.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirstValues {
    shown: Vec<Option<String>>,
    more: usize,
}

impl FirstValues {
    /// The first [`FIRST_VALUES`] values of `column`.
    ///
    /// # Errors
    ///
    /// When Polars cannot read a value.
    pub(super) fn of_column(column: &Column) -> Result<Self, PolarsError> {
        Self::of(column.len(), |index| column.get(index))
    }

    /// The first [`FIRST_VALUES`] values of `series`.
    ///
    /// # Errors
    ///
    /// When Polars cannot read a value.
    pub(super) fn of_series(series: &Series) -> Result<Self, PolarsError> {
        Self::of(series.len(), |index| series.get(index))
    }

    fn of<'a>(
        length: usize,
        get: impl Fn(usize) -> Result<AnyValue<'a>, PolarsError>,
    ) -> Result<Self, PolarsError> {
        let shown = (0..length.min(FIRST_VALUES))
            .map(|index| get(index).map(|value| text_of(&value)))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            shown,
            more: length.saturating_sub(FIRST_VALUES),
        })
    }

    /// The values shown, in order.
    #[must_use]
    pub fn shown(&self) -> &[Option<String>] {
        &self.shown
    }

    /// How many values follow those shown.
    #[must_use]
    pub const fn more(&self) -> usize {
        self.more
    }
}
