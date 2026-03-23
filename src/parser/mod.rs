//! Go parser integration
//! 
//! Parses Go source code and populates the type universe.

pub mod ast;
pub mod importer;
pub mod converter;

pub use ast::GoAst;
pub use importer::{PackageImporter, ImportResult};
pub use converter::TypeConverter;
