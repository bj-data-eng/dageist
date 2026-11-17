//! A column as the view is told of it.

use polars::prelude::Schema;

use crate::allowed::dtype_name;

/// A column of a table: its name and the short name of its dtype, as
/// [`dtype_name`] writes it, such as `Datetime(us)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnDescription {
    name: String,
    dtype: String,
}

impl ColumnDescription {
    /// The columns of `schema`, in its order.
    #[must_use]
    pub fn of_schema(schema: &Schema) -> Vec<Self> {
        schema
            .iter()
            .map(|(name, dtype)| Self {
                name: name.to_string(),
                dtype: dtype_name(dtype),
            })
            .collect()
    }

    /// The column's name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The short name of the column's dtype.
    #[must_use]
    pub fn dtype(&self) -> &str {
        &self.dtype
    }
}
