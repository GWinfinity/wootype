//! Go parser integration
//!
//! Parses Go source code and populates the type universe.

pub mod ast;
pub mod converter;
pub mod importer;

pub use ast::GoAst;
pub use converter::TypeConverter;
pub use importer::{ImportResult, PackageImporter};
