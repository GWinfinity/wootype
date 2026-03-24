//! Salsa input definitions
//!
//! Inputs are values that can change and trigger recomputation.
//! They represent the "roots" of the dependency graph.

use salsa::Setter;
use std::path::PathBuf;

/// A source file input - the root of all type checking
///
/// When the content changes, all dependent queries are re-executed.
#[salsa::input(debug)]
pub struct SourceFile {
    /// File path (acts as the identifier for this input)
    pub path: PathBuf,

    /// File content - changing this triggers re-computation
    pub content: String,

    /// Version number for tracking edits
    pub version: u64,
}

/// A file digest (hash) for quick change detection
#[salsa::input(debug)]
pub struct FileDigest {
    pub path: PathBuf,
    pub hash: u64,
}

/// Package manifest input
#[salsa::input(debug)]
pub struct PackageManifest {
    pub name: String,
    pub root: PathBuf,
    pub dependencies: Vec<String>,
}

/// Input for an incremental text change (for LSP)
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct TextChange {
    pub start: usize,
    pub end: usize,
    pub new_text: String,
}

impl SourceFile {
    /// Apply an incremental change to the file
    pub fn apply_change(self, db: &mut dyn salsa::Database, change: TextChange) {
        let current = self.content(db);
        let mut new_content = current.clone();

        // Apply the change
        if change.start <= new_content.len() && change.end <= new_content.len() {
            new_content.replace_range(change.start..change.end, &change.new_text);
        }

        // Update content and version
        let new_version = self.version(db) + 1;
        self.set_content(db).to(new_content);
        self.set_version(db).to(new_version);
    }

    /// Get line and column from byte offset
    pub fn offset_to_position(&self, db: &dyn salsa::Database, offset: usize) -> (usize, usize) {
        let content = self.content(db);
        let mut line = 0;
        let mut col = 0;

        for (i, ch) in content.char_indices() {
            if i >= offset {
                break;
            }
            if ch == '\n' {
                line += 1;
                col = 0;
            } else {
                col += 1;
            }
        }

        (line, col)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::salsa_full::database::TypeDatabase;

    #[test]
    fn test_source_file_creation() {
        let db = TypeDatabase::new();
        let file = SourceFile::new(&db, PathBuf::from("test.go"), "package main".to_string(), 1);

        assert_eq!(file.path(&db), PathBuf::from("test.go"));
        assert_eq!(file.content(&db), "package main");
        assert_eq!(file.version(&db), 1);
    }

    #[test]
    fn test_source_file_modification() {
        let mut db = TypeDatabase::new();
        let file = SourceFile::new(&db, PathBuf::from("test.go"), "package main".to_string(), 1);

        // Modify content
        file.set_content(&mut db).to("package foo".to_string());
        file.set_version(&mut db).to(2);

        assert_eq!(file.content(&db), "package foo");
        assert_eq!(file.version(&db), 2);
    }

    #[test]
    fn test_apply_change() {
        let mut db = TypeDatabase::new();
        let file = SourceFile::new(&db, PathBuf::from("test.go"), "Hello World".to_string(), 1);

        // Replace "World" with "Rust"
        let change = TextChange {
            start: 6,
            end: 11,
            new_text: "Rust".to_string(),
        };

        file.apply_change(&mut db, change);

        assert_eq!(file.content(&db), "Hello Rust");
        assert_eq!(file.version(&db), 2);
    }

    #[test]
    fn test_offset_to_position() {
        let db = TypeDatabase::new();
        let content = "line1\nline2\nline3";
        let file = SourceFile::new(&db, PathBuf::from("test.go"), content.to_string(), 1);

        assert_eq!(file.offset_to_position(&db, 0), (0, 0));
        assert_eq!(file.offset_to_position(&db, 6), (1, 0));
        assert_eq!(file.offset_to_position(&db, 12), (2, 0));
    }
}
