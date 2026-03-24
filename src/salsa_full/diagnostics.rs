//! Beautiful error messages (ariadne-compatible)
//!
//! Inspired by Rust's world-class error messages.
//! Note: Full ariadne integration requires careful span handling.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::{ErrorType, Location, TypeError};

/// Error severity for styling
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

/// A rich diagnostic message with context
#[derive(Clone, Debug)]
pub struct RichDiagnostic {
    pub severity: Severity,
    pub title: String,
    pub message: String,
    pub location: Location,
    pub labels: Vec<RichLabel>,
    pub notes: Vec<String>,
    pub help: Option<String>,
}

/// A label pointing to a specific location
#[derive(Clone, Debug)]
pub struct RichLabel {
    pub location: Location,
    pub message: String,
    pub color: Color,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    Red,
    Yellow,
    Blue,
    Green,
    Cyan,
    Magenta,
}

/// Convert a type error to a rich diagnostic
pub fn type_error_to_diagnostic(error: &TypeError, _source: &str, file: &Path) -> RichDiagnostic {
    let mut labels = vec![];
    let mut notes = vec![];
    let mut help = None;

    match &error.error_type {
        ErrorType::TypeMismatch { expected, found } => {
            labels.push(RichLabel {
                location: Location {
                    file: file.to_path_buf(),
                    span: error.span.clone(),
                },
                message: format!("expected `{}`, found `{}`", expected, found),
                color: Color::Red,
            });

            help = Some(format!(
                "try converting: `{}.from({})` or `{}.to({})`",
                expected, found, found, expected
            ));
        }
        ErrorType::UnknownIdentifier(name) => {
            labels.push(RichLabel {
                location: Location {
                    file: file.to_path_buf(),
                    span: error.span.clone(),
                },
                message: format!("`{}` not found in scope", name),
                color: Color::Red,
            });
        }
        ErrorType::UnknownField { ty, field } => {
            labels.push(RichLabel {
                location: Location {
                    file: file.to_path_buf(),
                    span: error.span.clone(),
                },
                message: format!("`{}` has no field `{}`", ty, field),
                color: Color::Red,
            });
        }
        ErrorType::WrongArity { expected, found } => {
            labels.push(RichLabel {
                location: Location {
                    file: file.to_path_buf(),
                    span: error.span.clone(),
                },
                message: format!("expected {} arguments, found {}", expected, found),
                color: Color::Red,
            });

            help = Some(format!("provide {} argument(s)", expected));
        }
        ErrorType::NotCallable(ty) => {
            labels.push(RichLabel {
                location: Location {
                    file: file.to_path_buf(),
                    span: error.span.clone(),
                },
                message: format!("`{}` is not callable", ty),
                color: Color::Red,
            });
        }
        ErrorType::InvalidOperation { op, ty } => {
            labels.push(RichLabel {
                location: Location {
                    file: file.to_path_buf(),
                    span: error.span.clone(),
                },
                message: format!("cannot apply `{}` to `{}`", op, ty),
                color: Color::Red,
            });
        }
        _ => {
            labels.push(RichLabel {
                location: Location {
                    file: file.to_path_buf(),
                    span: error.span.clone(),
                },
                message: error.message.clone(),
                color: Color::Red,
            });
        }
    }

    RichDiagnostic {
        severity: Severity::Error,
        title: "type error".to_string(),
        message: error.message.clone(),
        location: Location {
            file: file.to_path_buf(),
            span: error.span.clone(),
        },
        labels,
        notes,
        help,
    }
}

/// Render a diagnostic to a string (simplified, ariadne-compatible format)
pub fn render_diagnostic(diagnostic: &RichDiagnostic, _source: &str) -> String {
    let mut output = String::new();

    // Header
    let severity_str = match diagnostic.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
        Severity::Hint => "hint",
    };

    output.push_str(&format!("{}: {}\n", severity_str, diagnostic.title));

    // Location
    output.push_str(&format!(
        "  --> {}:{}:{}\n",
        diagnostic.location.file.display(),
        diagnostic.location.span.line,
        diagnostic.location.span.column
    ));

    // Message
    output.push_str(&format!("   |\n   = {}\n", diagnostic.message));

    // Labels
    for label in &diagnostic.labels {
        output.push_str(&format!(
            "   | {} at {}:{}\n",
            label.message, label.location.span.start, label.location.span.end
        ));
    }

    // Help
    if let Some(help) = &diagnostic.help {
        output.push_str(&format!("   = help: {}\n", help));
    }

    // Notes
    for note in &diagnostic.notes {
        output.push_str(&format!("   = note: {}\n", note));
    }

    output
}

/// Simple file cache
pub struct FileCache {
    files: HashMap<PathBuf, String>,
}

impl FileCache {
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
        }
    }

    pub fn insert(&mut self, path: PathBuf, source: String) {
        self.files.insert(path, source);
    }

    pub fn get(&self, path: &Path) -> Option<&str> {
        self.files.get(path).map(|s| s.as_str())
    }
}

impl Default for FileCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::salsa_full::{Span, Type};

    #[test]
    fn test_type_mismatch_diagnostic() {
        let error = TypeError {
            message: "Type mismatch".to_string(),
            span: Span {
                start: 10,
                end: 15,
                line: 1,
                column: 10,
            },
            error_type: ErrorType::TypeMismatch {
                expected: Type::Int,
                found: Type::String,
            },
        };

        let source = "x := \"hello\"\ny := x + 1";
        let file = Path::new("test.go");

        let diagnostic = type_error_to_diagnostic(&error, source, file);

        assert_eq!(diagnostic.severity, Severity::Error);
        assert!(!diagnostic.labels.is_empty());

        // Test rendering
        let rendered = render_diagnostic(&diagnostic, source);
        assert!(rendered.contains("error"));
        assert!(rendered.contains("test.go"));
    }
}
