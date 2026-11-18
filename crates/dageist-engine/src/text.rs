//! A value as the text Polars writes it.
//!
//! The step editor's previews and the blocks of a table both write a cell
//! this way, so that a cell reads the same wherever it is shown.

use polars::prelude::AnyValue;

/// A value as the text Polars writes it, or `None` where it is null, which no
/// text stands for.
pub(crate) fn text_of(value: &AnyValue<'_>) -> Option<String> {
    (!value.is_null()).then(|| value.str_value().into_owned())
}
