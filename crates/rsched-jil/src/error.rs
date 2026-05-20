//! JIL parse errors.

use thiserror::Error;

/// Error returned by [`crate::parse`].
#[derive(Debug, Error, PartialEq)]
pub enum JilError {
    /// Input ended inside a block comment.
    #[error("unterminated block comment")]
    UnterminatedComment,

    /// A verb token was present but unknown.
    #[error("unknown verb {0:?} at line {1}")]
    UnknownVerb(String, usize),

    /// A required attribute is missing from an `insert_job` block.
    #[error("insert_job {0:?} is missing required attribute {1:?}")]
    MissingAttribute(String, &'static str),

    /// `insert_job` has an unknown `job_type`.
    #[error("unknown job_type {0:?} in job {1:?}")]
    UnknownJobType(String, String),

    /// An attribute value failed to parse (e.g. non-numeric n_retrys).
    #[error("bad value for {attr:?}: {detail}")]
    BadValue {
        /// Attribute name.
        attr: String,
        /// Human-readable detail.
        detail: String,
    },
}
