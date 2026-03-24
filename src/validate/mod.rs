//! Streaming validation pipeline
//!
//! Provides expression-level incremental type checking
//! with look-ahead inference and soft error handling.

pub mod checker;
pub mod concurrent;
pub mod error;
pub mod infer;
pub mod stream;

pub use checker::StreamingChecker;
pub use error::{ErrorSeverity, SoftError, ValidationError};
pub use infer::{LookaheadContext, TypeInference};
pub use stream::{ValidationEvent, ValidationStream};
