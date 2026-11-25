//! A table a node of the graph gives, as a plan that has read no rows: what a
//! source scans from a file, or what a transform's steps make of their
//! inputs. A host that works out a graph holds these and binds them to the
//! inputs of the nodes that read them, without touching Polars itself.

use std::fmt;
use std::path::Path;

use polars::prelude::{
    AnyValue, DataFrame, DataType, IdxSize, IntoLazy, LazyCsvReader, LazyFileListReader, LazyFrame,
    PlRefPath, Scalar, ScanArgsParquet, StrptimeOptions, col, df,
};

use crate::document::{ColumnDescription, DuplicateInput, Inputs, ListOrigin};
use crate::error::EngineError;

/// How a file a source reads is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileFormat {
    /// Apache Parquet.
    Parquet,
    /// Comma-separated text with a header row.
    Csv,
}

/// A table a node gives, as a plan: nothing is read until rows are asked for.
#[derive(Clone)]
pub struct Frame {
    lazy: LazyFrame,
}

impl fmt::Debug for Frame {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Frame")
    }
}

impl Frame {
    pub(crate) const fn of_lazy(lazy: LazyFrame) -> Self {
        Self { lazy }
    }

    pub(crate) fn lazy(&self) -> LazyFrame {
        self.lazy.clone()
    }

    /// The table in the file at `path`, read as `format` when its rows are
    /// asked for. Its schema is read from the file by [`columns`](Self::columns).
    ///
    /// # Errors
    ///
    /// When Polars cannot plan a scan of the path.
    pub fn scan(path: &Path, format: FileFormat) -> Result<Self, EngineError> {
        crate::startup::ensure();
        let path = PlRefPath::try_from_path(path)
            .map_err(|error| EngineError::polars("naming the file", error))?;
        let lazy = match format {
            FileFormat::Parquet => LazyFrame::scan_parquet(path, ScanArgsParquet::default()),
            FileFormat::Csv => LazyCsvReader::new(path).with_has_header(true).finish(),
        }
        .map_err(|error| EngineError::polars("planning a scan of the file", error))?;
        Ok(Self::of_lazy(lazy))
    }

    /// The table's columns with their dtypes, worked out from the plan: a
    /// file's schema is read from the file, and no rows are.
    ///
    /// # Errors
    ///
    /// When Polars cannot work out the schema, as when a file cannot be read.
    pub fn columns(&self) -> Result<Vec<ColumnDescription>, EngineError> {
        let schema = self
            .lazy()
            .collect_schema()
            .map_err(|error| EngineError::polars("working out the columns", error))?;
        Ok(ColumnDescription::of_schema(&schema))
    }

    /// The first `rows` rows of the table, held in memory: the working rows a
    /// step editor previews and counts its steps on.
    ///
    /// # Errors
    ///
    /// When Polars cannot collect the rows.
    pub fn working(&self, rows: usize) -> Result<DataFrame, EngineError> {
        crate::startup::ensure();
        self.lazy()
            .limit(IdxSize::try_from(rows).unwrap_or(IdxSize::MAX))
            .collect()
            .map_err(|error| EngineError::polars("collecting the working rows", error))
    }
}

/// Why an input could not be connected.
#[derive(Debug)]
pub enum InputError {
    /// Another input already has the name.
    Duplicate(DuplicateInput),
    /// The value or values could not be read; the text says why.
    Unreadable(String),
}

impl fmt::Display for InputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Duplicate(duplicate) => duplicate.fmt(formatter),
            Self::Unreadable(reason) => formatter.write_str(reason),
        }
    }
}

impl std::error::Error for InputError {}

impl From<DuplicateInput> for InputError {
    fn from(duplicate: DuplicateInput) -> Self {
        Self::Duplicate(duplicate)
    }
}

/// The dtypes a parameter may have, by the names Polars gives them.
pub const PARAMETER_DTYPES: [&str; 5] = ["Int64", "Float64", "Boolean", "String", "Date"];

/// The value `text` is as a value of `dtype`, one of [`PARAMETER_DTYPES`].
fn parameter_value(dtype: &str, text: &str) -> Result<Scalar, String> {
    let trimmed = text.trim();
    match dtype {
        "Int64" => trimmed
            .parse::<i64>()
            .map(|value| Scalar::new(DataType::Int64, AnyValue::Int64(value)))
            .map_err(|_| format!("{text:?} is not a whole number")),
        "Float64" => trimmed
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .map(|value| Scalar::new(DataType::Float64, AnyValue::Float64(value)))
            .ok_or_else(|| format!("{text:?} is not a number")),
        "Boolean" => match trimmed {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(format!("{text:?} is neither true nor false")),
        }
        .map(|value| Scalar::new(DataType::Boolean, AnyValue::Boolean(value))),
        "String" => Ok(Scalar::new(
            DataType::String,
            AnyValue::StringOwned(text.into()),
        )),
        "Date" => {
            let unread = |_| format!("{text:?} is not a date");
            let frame = df!("date" => [trimmed])
                .and_then(|frame| {
                    frame
                        .lazy()
                        .with_column(col("date").str().to_date(StrptimeOptions::default()))
                        .collect()
                })
                .map_err(unread)?;
            let value = frame
                .column("date")
                .and_then(|column| column.get(0))
                .map_err(unread)?
                .into_static();
            if matches!(value, AnyValue::Null) {
                return Err(format!("{text:?} is not a date"));
            }
            Ok(Scalar::new(DataType::Date, value))
        }
        _ => Err(format!(
            "{dtype:?} is not a dtype a parameter may have: {}",
            PARAMETER_DTYPES.join(", ")
        )),
    }
}

impl Inputs {
    /// Connects the table `frame`, which the node called `node` gives.
    ///
    /// # Errors
    ///
    /// When `name` is already an input.
    pub fn add_frame(
        &mut self,
        name: impl Into<String>,
        node: impl Into<String>,
        frame: &Frame,
    ) -> Result<(), DuplicateInput> {
        self.add_table(name, node, frame.lazy())
    }

    /// Connects the values of the column `column` of `frame`, which the node
    /// called `node` gives, as a list. The column is read whole.
    ///
    /// # Errors
    ///
    /// When `name` is already an input, or the column cannot be read.
    pub fn add_column(
        &mut self,
        name: impl Into<String>,
        node: impl Into<String>,
        column: &str,
        frame: &Frame,
    ) -> Result<(), InputError> {
        crate::startup::ensure();
        let collected = frame
            .lazy()
            .select([col(column)])
            .collect()
            .map_err(|error| InputError::Unreadable(format!("reading the column: {error}")))?;
        let values = collected
            .column(column)
            .map_err(|error| InputError::Unreadable(format!("reading the column: {error}")))?
            .as_materialized_series()
            .clone();
        let origin = ListOrigin::Column {
            node: node.into(),
            column: column.to_owned(),
        };
        Ok(self.add_list(name, origin, values)?)
    }

    /// Connects a value set on the graph: `text` read as a value of `dtype`,
    /// one of [`PARAMETER_DTYPES`].
    ///
    /// # Errors
    ///
    /// When `name` is already an input, `dtype` is not one a parameter may
    /// have, or `text` is not a value of it.
    pub fn add_parameter(
        &mut self,
        name: impl Into<String>,
        dtype: &str,
        text: &str,
    ) -> Result<(), InputError> {
        let value = parameter_value(dtype, text).map_err(InputError::Unreadable)?;
        Ok(self.add_value(name, value)?)
    }
}

#[cfg(test)]
mod tests {
    use super::{PARAMETER_DTYPES, parameter_value};

    #[test]
    fn a_parameter_is_read_as_its_dtype() {
        let shown =
            |dtype, text| parameter_value(dtype, text).map(|value| value.value().to_string());
        assert_eq!(shown("Int64", " 12 ").as_deref(), Ok("12"));
        assert_eq!(shown("Float64", "1.5").as_deref(), Ok("1.5"));
        assert_eq!(shown("Boolean", "true").as_deref(), Ok("true"));
        assert_eq!(shown("String", " a b ").as_deref(), Ok("\" a b \""));
        assert_eq!(shown("Date", "2026-03-02").as_deref(), Ok("2026-03-02"));
    }

    #[test]
    fn a_parameter_that_is_not_its_dtype_is_refused_with_why() {
        for (dtype, text) in [
            ("Int64", "1.5"),
            ("Float64", "NaN"),
            ("Boolean", "yes"),
            ("Date", "the second"),
            ("Duration", "1d"),
        ] {
            assert!(parameter_value(dtype, text).is_err(), "{dtype} {text}");
        }
        assert!(PARAMETER_DTYPES.contains(&"Date"));
    }
}
