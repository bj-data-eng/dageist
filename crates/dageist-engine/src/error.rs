//! The engine's error type.

use std::error::Error;
use std::fmt;
use std::io;

use polars::error::PolarsError;

/// An operation the engine could not complete.
#[derive(Debug)]
pub struct EngineError {
    context: &'static str,
    source: EngineErrorSource,
}

#[derive(Debug)]
enum EngineErrorSource {
    Polars(PolarsError),
    Io(io::Error),
    Request(String),
}

impl EngineError {
    pub(crate) fn polars(context: &'static str, source: PolarsError) -> Self {
        Self {
            context,
            source: EngineErrorSource::Polars(source),
        }
    }

    pub(crate) fn io(context: &'static str, source: io::Error) -> Self {
        Self {
            context,
            source: EngineErrorSource::Io(source),
        }
    }

    pub(crate) fn request(context: &'static str, detail: impl Into<String>) -> Self {
        Self {
            context,
            source: EngineErrorSource::Request(detail.into()),
        }
    }

    /// Returns whether the caller asked for something that does not exist,
    /// such as an unknown column.
    #[must_use]
    pub fn is_invalid_request(&self) -> bool {
        matches!(self.source, EngineErrorSource::Request(_))
    }
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.source {
            EngineErrorSource::Polars(source) => write!(f, "{}: {source}", self.context),
            EngineErrorSource::Io(source) => write!(f, "{}: {source}", self.context),
            EngineErrorSource::Request(detail) => write!(f, "{}: {detail}", self.context),
        }
    }
}

impl Error for EngineError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self.source {
            EngineErrorSource::Polars(source) => Some(source),
            EngineErrorSource::Io(source) => Some(source),
            EngineErrorSource::Request(_) => None,
        }
    }
}
