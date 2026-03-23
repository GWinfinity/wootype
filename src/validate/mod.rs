//! Streaming validation pipeline
//! 
//! Provides expression-level incremental type checking
//! with look-ahead inference and soft error handling.

pub mod stream;
pub mod checker;
pub mod error;
pub mod infer;

pub use stream::{ValidationStream, ValidationEvent};
pub use checker::StreamingChecker;
pub use error::{ValidationError, SoftError, ErrorSeverity};
pub use infer::{TypeInference, LookaheadContext};
