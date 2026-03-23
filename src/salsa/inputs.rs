//! Input management for incremental type checking

use std::path::PathBuf;

/// Manages source file inputs with revision tracking
#[derive(Default)]
pub struct InputManager {
    files: dashmap::DashMap<PathBuf, FileState>,
    current_revision: std::sync::atomic::AtomicU64,
}

#[derive(Clone, Debug)]
struct FileState {
    content: String,
    revision: u64,
}

impl InputManager {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Set or update a file
    pub fn set_file(&self, path: PathBuf, content: String) -> bool {
        let new_rev = self.current_revision.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        
        match self.files.entry(path) {
            dashmap::mapref::entry::Entry::Occupied(mut entry) => {
                let changed = entry.get().content != content;
                if changed {
                    entry.insert(FileState {
                        content,
                        revision: new_rev,
                    });
                }
                changed
            }
            dashmap::mapref::entry::Entry::Vacant(entry) => {
                entry.insert(FileState {
                    content,
                    revision: new_rev,
                });
                true
            }
        }
    }
    
    /// Get file content
    pub fn get_file(&self, path: &PathBuf) -> Option<String> {
        self.files.get(path).map(|f| f.content.clone())
    }
    
    /// Apply an incremental change (for LSP)
    pub fn apply_change(&self, change: IncrementalChange) -> Result<(), String> {
        let mut content = self.get_file(&change.file)
            .ok_or_else(|| format!("File not found: {:?}", change.file))?;
        
        // Convert line/col to byte offset
        let start = position_to_offset(&content, change.range.start_line, change.range.start_col);
        let end = position_to_offset(&content, change.range.end_line, change.range.end_col);
        
        // Apply change
        content.replace_range(start..end, &change.new_text);
        
        // Update
        self.set_file(change.file, content);
        
        Ok(())
    }
}

/// An incremental change for LSP
#[derive(Debug, Clone)]
pub struct IncrementalChange {
    pub file: PathBuf,
    pub range: ChangeRange,
    pub new_text: String,
}

#[derive(Debug, Clone)]
pub struct ChangeRange {
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
}

fn position_to_offset(content: &str, line: usize, col: usize) -> usize {
    let mut current_line = 0;
    let mut offset = 0;
    
    for (i, c) in content.char_indices() {
        if current_line == line {
            let line_start = i;
            let col_offset = content[line_start..]
                .chars()
                .take(col)
                .map(|c| c.len_utf8())
                .sum::<usize>();
            return line_start + col_offset;
        }
        if c == '\n' {
            current_line += 1;
        }
        offset = i + c.len_utf8();
    }
    
    offset
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_set_and_get() {
        let manager = InputManager::new();
        let path = PathBuf::from("test.go");
        
        assert!(manager.set_file(path.clone(), "package main".to_string()));
        assert_eq!(manager.get_file(&path), Some("package main".to_string()));
        
        // Same content doesn't change
        assert!(!manager.set_file(path.clone(), "package main".to_string()));
    }
}
