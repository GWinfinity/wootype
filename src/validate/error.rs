//! Validation errors with soft error support
//! 
//! Soft errors allow AI agents to continue generation
//! while receiving type guidance.

use crate::core::{TypeId, SourcePosition};

/// Validation error types
#[derive(Debug, Clone)]
pub enum ValidationError {
    TypeMismatch {
        expected: TypeId,
        found: TypeId,
    },
    UndefinedIdentifier(String),
    UndefinedField {
        typ: TypeId,
        field: String,
    },
    UndefinedMethod {
        typ: TypeId,
        method: String,
    },
    ArityMismatch {
        expected: usize,
        found: usize,
    },
    InvalidOperation {
        op: String,
        typ: TypeId,
    },
    InterfaceNotSatisfied {
        typ: TypeId,
        interface: TypeId,
        missing_methods: Vec<String>,
    },
    CyclicType,
    GenericArityMismatch {
        expected: usize,
        found: usize,
    },
    ConstraintViolation {
        param: String,
        constraint: String,
    },
}

impl ValidationError {
    /// Convert to soft error with suggestion
    pub fn soften(self) -> SoftError {
        let (message, suggestion, severity) = match &self {
            Self::TypeMismatch { expected, found } => {
                (
                    format!("Type mismatch: expected {:?}, found {:?}", expected, found),
                    Some(format!("Consider converting or using a different type")),
                    ErrorSeverity::Warning,
                )
            }
            Self::UndefinedIdentifier(name) => {
                (
                    format!("'{}' is not defined", name),
                    Some(format!("Define '{}' or import it", name)),
                    ErrorSeverity::Error,
                )
            }
            Self::UndefinedField { typ: _, field } => {
                (
                    format!("Field '{}' not found", field),
                    None,
                    ErrorSeverity::Error,
                )
            }
            Self::ArityMismatch { expected, found } => {
                (
                    format!("Expected {} arguments, found {}", expected, found),
                    Some(format!("Adjust the number of arguments")),
                    ErrorSeverity::Warning,
                )
            }
            _ => (
                format!("{:?}", self),
                None,
                ErrorSeverity::Warning,
            )
        };
        
        SoftError {
            message,
            suggestion,
            severity,
        }
    }
}

/// Soft error - non-fatal type issue
#[derive(Debug, Clone)]
pub struct SoftError {
    pub message: String,
    pub suggestion: Option<String>,
    pub severity: ErrorSeverity,
}

impl SoftError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            suggestion: None,
            severity: ErrorSeverity::Warning,
        }
    }
    
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }
    
    pub fn with_severity(mut self, severity: ErrorSeverity) -> Self {
        self.severity = severity;
        self
    }
    
    pub fn is_blocking(&self) -> bool {
        matches!(self.severity, ErrorSeverity::Error | ErrorSeverity::Fatal)
    }
}

/// Error severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorSeverity {
    /// Just a hint, AI can ignore
    Hint,
    /// Warning, but generation can continue
    Warning,
    /// Error, should be fixed but can continue with soft types
    Error,
    /// Fatal, cannot proceed
    Fatal,
}

impl ErrorSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Hint => "hint",
            Self::Warning => "warning",
            Self::Error => "error",
            Self::Fatal => "fatal",
        }
    }
}

/// Error collection with soft error support
#[derive(Debug, Clone, Default)]
pub struct ErrorCollection {
    errors: Vec<ValidationError>,
    soft_errors: Vec<SoftError>,
}

impl ErrorCollection {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn add_error(&mut self, error: ValidationError) {
        self.errors.push(error);
    }
    
    pub fn add_soft_error(&mut self, error: SoftError) {
        self.soft_errors.push(error);
    }
    
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty() || self.soft_errors.iter().any(|e| e.is_blocking())
    }
    
    pub fn has_soft_errors(&self) -> bool {
        !self.soft_errors.is_empty()
    }
    
    pub fn is_empty(&self) -> bool {
        self.errors.is_empty() && self.soft_errors.is_empty()
    }
    
    pub fn len(&self) -> usize {
        self.errors.len() + self.soft_errors.len()
    }
    
    /// Convert all errors to soft errors
    pub fn soften(self) -> Vec<SoftError> {
        let mut result: Vec<SoftError> = self.errors.into_iter()
            .map(|e| e.soften())
            .collect();
        result.extend(self.soft_errors);
        result
    }
    
    /// Get errors above severity threshold
    pub fn filter_by_severity(&self, min_severity: ErrorSeverity) -> Vec<&SoftError> {
        let severity_order = |s: &ErrorSeverity| match s {
            ErrorSeverity::Fatal => 3,
            ErrorSeverity::Error => 2,
            ErrorSeverity::Warning => 1,
            ErrorSeverity::Hint => 0,
        };
        
        let min_order = severity_order(&min_severity);
        
        self.soft_errors.iter()
            .filter(|e| severity_order(&e.severity) >= min_order)
            .collect()
    }
    
    pub fn iter_errors(&self) -> impl Iterator<Item = &ValidationError> {
        self.errors.iter()
    }
    
    pub fn iter_soft_errors(&self) -> impl Iterator<Item = &SoftError> {
        self.soft_errors.iter()
    }
}

/// Error with source location
#[derive(Debug, Clone)]
pub struct LocatedError {
    pub error: ValidationError,
    pub position: SourcePosition,
    pub context: String,
}

impl LocatedError {
    pub fn new(error: ValidationError, position: SourcePosition, context: impl Into<String>) -> Self {
        Self {
            error,
            position,
            context: context.into(),
        }
    }
    
    pub fn format(&self) -> String {
        format!(
            "{}:{}: {}\n  Context: {}",
            self.position.line,
            self.position.column,
            format!("{:?}", self.error),
            self.context
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_soft_error_creation() {
        let error = SoftError::new("Type mismatch")
            .with_suggestion("Use int instead")
            .with_severity(ErrorSeverity::Warning);
        
        assert!(!error.is_blocking());
        assert_eq!(error.message, "Type mismatch");
    }
    
    #[test]
    fn test_error_collection() {
        let mut collection = ErrorCollection::new();
        
        collection.add_soft_error(SoftError::new("Warning 1"));
        collection.add_soft_error(
            SoftError::new("Error 1").with_severity(ErrorSeverity::Error)
        );
        
        assert!(collection.has_soft_errors());
        assert!(collection.has_errors()); // Blocking error
    }
    
    #[test]
    fn test_error_soften() {
        let error = ValidationError::UndefinedIdentifier("foo".to_string());
        let soft = error.soften();
        
        assert!(!soft.is_blocking());
        assert!(soft.suggestion.is_some());
    }
}
