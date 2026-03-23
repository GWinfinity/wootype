//! Gradual typing support for Go
//!
//! Allows mixing typed and untyped code with gradual enforcement
//! of type safety. Supports migration from untyped to fully typed.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::{Type, TypeError, Span, ErrorType};

/// Gradual typing mode for a module or function
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GradualMode {
    /// Fully static - all types checked at compile time
    Static,
    /// Gradual - some `any` types allowed, warnings for untyped
    Gradual,
    /// Dynamic - runtime type checking at boundaries only
    Dynamic,
}

impl GradualMode {
    /// Get the strictness level (higher = more strict)
    pub fn strictness(&self) -> u8 {
        match self {
            GradualMode::Static => 3,
            GradualMode::Gradual => 2,
            GradualMode::Dynamic => 1,
        }
    }
    
    /// Check if this mode allows untyped code
    pub fn allows_untyped(&self) -> bool {
        matches!(self, GradualMode::Gradual | GradualMode::Dynamic)
    }
    
    /// Check if this mode requires full type annotations
    pub fn requires_annotations(&self) -> bool {
        matches!(self, GradualMode::Static)
    }
}

/// Annotation state of code
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AnnotationState {
    /// Fully annotated with types
    FullyAnnotated,
    /// Partially annotated (some types missing)
    PartiallyAnnotated,
    /// No type annotations
    Unannotated,
}

/// Type annotation information for a file or function
#[derive(Clone, Debug)]
pub struct TypeAnnotations {
    /// Overall annotation state
    pub state: AnnotationState,
    /// Percentage of symbols with type annotations (0-100)
    pub coverage_percent: u8,
    /// Annotated symbols
    pub annotated: HashSet<String>,
    /// Unannotated symbols
    pub unannotated: HashSet<String>,
}

/// Gradual type checker
pub struct GradualChecker {
    mode: GradualMode,
    /// Type annotations tracking
    annotations: HashMap<String, TypeAnnotations>,
    /// Python interop config (if needed)
    python_interop: Option<PythonInterop>,
    /// Migration tracking
    migration: MigrationTracker,
}

/// Migration tracker for gradual typing adoption
pub struct MigrationTracker {
    /// Original untyped code locations
    untyped_origins: HashMap<String, DocumentLocation>,
    /// Migration progress
    progress: MigrationProgress,
}

/// Migration progress
#[derive(Clone, Debug, Default)]
pub struct MigrationProgress {
    pub total_files: usize,
    pub fully_typed: usize,
    pub partially_typed: usize,
    pub untyped: usize,
    pub percentage_complete: f64,
}

/// Document location
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct DocumentLocation {
    pub path: String,
    pub line: usize,
    pub column: usize,
}

/// Python interop configuration
#[derive(Clone, Debug)]
pub struct PythonInterop {
    pub check_python_calls: bool,
    pub runtime_assertions: bool,
    pub type_mappings: HashMap<String, Type>,
}

impl Default for PythonInterop {
    fn default() -> Self {
        let mut type_mappings = HashMap::new();
        type_mappings.insert("int".to_string(), Type::Int);
        type_mappings.insert("float".to_string(), Type::Float);
        type_mappings.insert("str".to_string(), Type::String);
        type_mappings.insert("bool".to_string(), Type::Bool);
        type_mappings.insert("list".to_string(), Type::Array(Box::new(Type::Any)));
        type_mappings.insert("dict".to_string(), Type::Map(Box::new(Type::Any), Box::new(Type::Any)));
        
        Self {
            check_python_calls: true,
            runtime_assertions: true,
            type_mappings,
        }
    }
}

/// Runtime type tag for dynamic values
#[derive(Clone, Debug, PartialEq)]
pub enum RuntimeTag {
    Int,
    Float,
    String,
    Bool,
    List(Box<RuntimeTag>),
    Dict(Box<RuntimeTag>, Box<RuntimeTag>),
    Object(String),
    Any,
}

/// Type check result with gradual typing info
#[derive(Clone, Debug)]
pub struct GradualCheckResult {
    pub is_valid: bool,
    pub errors: Vec<TypeError>,
    pub warnings: Vec<GradualWarning>,
    pub annotation_state: AnnotationState,
}

/// Gradual typing specific warning
#[derive(Clone, Debug)]
pub struct GradualWarning {
    pub message: String,
    pub span: Span,
    pub kind: GradualWarningKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GradualWarningKind {
    MissingTypeAnnotation,
    UntypedCodeBlock,
    DynamicTypeUsage,
    PartialTypeInference,
    MigrationSuggestion,
}

/// Migration tool for converting untyped to typed code
pub struct MigrationTool {
    checker: GradualChecker,
}

/// Suggested type annotation
#[derive(Clone, Debug)]
pub struct TypeSuggestion {
    pub name: String,
    pub suggested_type: Type,
    pub confidence: f64,
    pub reason: String,
}

impl GradualChecker {
    pub fn new(mode: GradualMode) -> Self {
        Self {
            mode,
            annotations: HashMap::new(),
            python_interop: None,
            migration: MigrationTracker::new(),
        }
    }
    
    pub fn with_python_interop(mut self, interop: PythonInterop) -> Self {
        self.python_interop = Some(interop);
        self
    }
    
    /// Analyze type annotations in code
    pub fn analyze_annotations(&mut self, file: &str, content: &str) -> TypeAnnotations {
        let mut annotated = HashSet::new();
        let mut unannotated = HashSet::new();
        
        for line in content.lines() {
            let line = line.trim();
            
            // Check for function declarations
            if line.starts_with("func ") {
                if let Some(func_sig) = extract_func_signature(line) {
                    if func_sig.contains(":") || func_sig.contains("->") {
                        if let Some(name) = extract_func_name(line) {
                            annotated.insert(name);
                        }
                    } else {
                        if let Some(name) = extract_func_name(line) {
                            unannotated.insert(name);
                        }
                    }
                }
            }
            
            // Check for variable declarations
            if line.contains(":=") && !line.contains("// type:") {
                if let Some(name) = extract_var_name(line) {
                    unannotated.insert(name);
                }
            }
        }
        
        let total = annotated.len() + unannotated.len();
        let coverage = if total > 0 {
            (annotated.len() * 100 / total) as u8
        } else {
            100
        };
        
        let state = if coverage == 100 {
            AnnotationState::FullyAnnotated
        } else if coverage == 0 {
            AnnotationState::Unannotated
        } else {
            AnnotationState::PartiallyAnnotated
        };
        
        let annotations = TypeAnnotations {
            state,
            coverage_percent: coverage,
            annotated,
            unannotated,
        };
        
        self.annotations.insert(file.to_string(), annotations.clone());
        annotations
    }
    
    /// Check code with gradual typing rules
    pub fn check(&self, ty1: &Type, ty2: &Type, annotation_state: AnnotationState) -> GradualCheckResult {
        let mut errors = vec![];
        let mut warnings = vec![];
        
        match self.mode {
            GradualMode::Static => {
                // Static mode: require full annotations
                if annotation_state != AnnotationState::FullyAnnotated {
                    warnings.push(GradualWarning {
                        message: "Missing type annotations in static mode".to_string(),
                        span: Span::default(),
                        kind: GradualWarningKind::MissingTypeAnnotation,
                    });
                }
                
                // Strict type checking
                if !self.is_compatible_static(ty1, ty2) {
                    errors.push(TypeError {
                        message: format!("Type mismatch: {:?} vs {:?}", ty1, ty2),
                        span: Span::default(),
                        error_type: ErrorType::TypeMismatch {
                            expected: ty1.clone(),
                            found: ty2.clone(),
                        },
                    });
                }
            }
            
            GradualMode::Gradual => {
                // Gradual mode: allow any, but warn
                if matches!((ty1, ty2), (Type::Any, _) | (_, Type::Any)) {
                    warnings.push(GradualWarning {
                        message: "Dynamic type usage detected".to_string(),
                        span: Span::default(),
                        kind: GradualWarningKind::DynamicTypeUsage,
                    });
                } else if !self.is_compatible_gradual(ty1, ty2) {
                    errors.push(TypeError {
                        message: format!("Type mismatch: {:?} vs {:?}", ty1, ty2),
                        span: Span::default(),
                        error_type: ErrorType::TypeMismatch {
                            expected: ty1.clone(),
                            found: ty2.clone(),
                        },
                    });
                }
            }
            
            GradualMode::Dynamic => {
                // Dynamic mode: only check at boundaries
                if !self.is_compatible_dynamic(ty1, ty2) {
                    errors.push(TypeError {
                        message: "Runtime type check failed".to_string(),
                        span: Span::default(),
                        error_type: ErrorType::TypeMismatch {
                            expected: ty1.clone(),
                            found: ty2.clone(),
                        },
                    });
                }
            }
        }
        
        GradualCheckResult {
            is_valid: errors.is_empty(),
            errors,
            warnings,
            annotation_state,
        }
    }
    
    /// Static mode compatibility
    fn is_compatible_static(&self, expected: &Type, found: &Type) -> bool {
        expected == found
    }
    
    /// Gradual mode compatibility
    fn is_compatible_gradual(&self, expected: &Type, found: &Type) -> bool {
        // Allow any to be compatible with anything
        if matches!(expected, Type::Any) || matches!(found, Type::Any) {
            return true;
        }
        
        // Otherwise require exact match
        expected == found
    }
    
    /// Dynamic mode compatibility
    fn is_compatible_dynamic(&self, _expected: &Type, _found: &Type) -> bool {
        // In dynamic mode, everything is compatible at compile time
        // Runtime checks handle mismatches
        true
    }
    
    /// Get annotation info for a file
    pub fn get_annotations(&self, file: &str) -> Option<&TypeAnnotations> {
        self.annotations.get(file)
    }
    
    /// Get migration progress
    pub fn migration_progress(&self) -> MigrationProgress {
        self.migration.calculate_progress(&self.annotations)
    }
    
    /// Should this code be checked strictly?
    pub fn should_check_strictly(&self, annotation_state: AnnotationState) -> bool {
        match self.mode {
            GradualMode::Static => true,
            GradualMode::Gradual => annotation_state == AnnotationState::FullyAnnotated,
            GradualMode::Dynamic => false,
        }
    }
}

impl MigrationTracker {
    pub fn new() -> Self {
        Self {
            untyped_origins: HashMap::new(),
            progress: MigrationProgress::default(),
        }
    }
    
    fn calculate_progress(&self, annotations: &HashMap<String, TypeAnnotations>) -> MigrationProgress {
        let total = annotations.len();
        let fully = annotations.values()
            .filter(|a| a.state == AnnotationState::FullyAnnotated)
            .count();
        let partial = annotations.values()
            .filter(|a| a.state == AnnotationState::PartiallyAnnotated)
            .count();
        let untyped = total - fully - partial;
        
        let percentage = if total > 0 {
            (fully as f64 + partial as f64 * 0.5) / total as f64 * 100.0
        } else {
            0.0
        };
        
        MigrationProgress {
            total_files: total,
            fully_typed: fully,
            partially_typed: partial,
            untyped,
            percentage_complete: percentage,
        }
    }
}

impl Default for MigrationTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl MigrationTool {
    pub fn new(checker: GradualChecker) -> Self {
        Self { checker }
    }
    
    /// Analyze code and suggest type annotations
    pub fn suggest_types(&self, content: &str) -> Vec<TypeSuggestion> {
        let mut suggestions = vec![];
        
        for line in content.lines() {
            let line = line.trim();
            
            // Look for untyped functions (functions without return type annotation)
            if line.starts_with("func ") {
                if let Some(name) = extract_func_name(line) {
                    // Skip if already has return type annotation
                    if line.contains("->") {
                        continue;
                    }
                    // Infer return type from function name
                    if let Some(ty) = infer_return_type_from_name(&name) {
                        suggestions.push(TypeSuggestion {
                            name: name.clone(),
                            suggested_type: ty.clone(),
                            confidence: 0.7,
                            reason: format!("Function name '{}' suggests return type '{:?}'", name, ty),
                        });
                    }
                }
            }
            
            // Look for untyped variables
            if line.contains(":=") {
                if let Some(name) = extract_var_name(line) {
                    if let Some(ty) = infer_type_from_name(&name) {
                        suggestions.push(TypeSuggestion {
                            name: name.clone(),
                            suggested_type: ty.clone(),
                            confidence: 0.6,
                            reason: format!("Variable name '{}' suggests type '{:?}'", name, ty),
                        });
                    }
                }
            }
        }
        
        suggestions
    }
    
    /// Generate migration report
    pub fn generate_report(&self) -> String {
        let progress = self.checker.migration_progress();
        
        format!(
            r#"# Gradual Typing Migration Report

## Progress
- Total files: {}
- Fully typed: {} ({:.1}%)
- Partially typed: {} ({:.1}%)
- Untyped: {} ({:.1}%)
- Overall completion: {:.1}%

## Recommendations
1. Start with files that have high impact (exported functions)
2. Use type inference to auto-generate suggestions
3. Gradually increase strictness level
"#,
            progress.total_files,
            progress.fully_typed,
            progress.fully_typed as f64 / progress.total_files as f64 * 100.0,
            progress.partially_typed,
            progress.partially_typed as f64 / progress.total_files as f64 * 100.0,
            progress.untyped,
            progress.untyped as f64 / progress.total_files as f64 * 100.0,
            progress.percentage_complete
        )
    }
}

// Helper functions
fn extract_func_signature(line: &str) -> Option<&str> {
    line.split('{').next()
}

fn extract_func_name(line: &str) -> Option<String> {
    line.strip_prefix("func ")?
        .split('(')
        .next()
        .map(|s| s.split('.').last().unwrap_or(s).trim().to_string())
}

fn extract_var_name(line: &str) -> Option<String> {
    line.split(":=").next()?.trim().split_whitespace().last().map(|s| s.to_string())
}

fn infer_return_type_from_name(name: &str) -> Option<Type> {
    if name.starts_with("is") || name.starts_with("has") || name.contains("Enabled") {
        return Some(Type::Bool);
    }
    if name.contains("Count") || name.contains("Num") || name.contains("Len") {
        return Some(Type::Int);
    }
    if name.contains("Name") || name.contains("Text") || name.contains("String") {
        return Some(Type::String);
    }
    None
}

fn infer_type_from_name(name: &str) -> Option<Type> {
    if name.ends_with("Count") || name.ends_with("Index") || name.ends_with("Id") {
        return Some(Type::Int);
    }
    if name.contains("name") || name.contains("text") || name.contains("title") {
        return Some(Type::String);
    }
    if name.starts_with("is") || name.starts_with("has") {
        return Some(Type::Bool);
    }
    if name.ends_with("s") {
        return Some(Type::Array(Box::new(Type::Any)));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_gradual_mode_strictness() {
        assert!(GradualMode::Static.strictness() > GradualMode::Gradual.strictness());
        assert!(GradualMode::Gradual.strictness() > GradualMode::Dynamic.strictness());
    }
    
    #[test]
    fn test_analyze_annotations() {
        let mut checker = GradualChecker::new(GradualMode::Gradual);
        
        let content = r#"
func Annotated(x: int) -> int {
    return x + 1
}

func Unannotated(x) {
    return x + 1
}
"#;
        
        let annotations = checker.analyze_annotations("test.go", content);
        
        assert_eq!(annotations.state, AnnotationState::PartiallyAnnotated);
        assert_eq!(annotations.annotated.len(), 1);
        assert_eq!(annotations.unannotated.len(), 1);
    }
    
    #[test]
    fn test_gradual_check_any() {
        let checker = GradualChecker::new(GradualMode::Gradual);
        
        // In gradual mode, Any is compatible with anything
        let result = checker.check(&Type::Any, &Type::Int, AnnotationState::FullyAnnotated);
        assert!(result.is_valid);
        assert!(!result.warnings.is_empty()); // But should warn
    }
    
    #[test]
    fn test_static_check_strict() {
        let checker = GradualChecker::new(GradualMode::Static);
        
        // In static mode, require exact match
        let result = checker.check(&Type::Int, &Type::String, AnnotationState::FullyAnnotated);
        assert!(!result.is_valid);
    }
    
    #[test]
    fn test_migration_suggestions() {
        let checker = GradualChecker::new(GradualMode::Gradual);
        let tool = MigrationTool::new(checker);
        
        let content = r#"
func isValid() {
    return true
}

count := 42
name := "test"
"#;
        
        let suggestions = tool.suggest_types(content);
        
        // Should suggest types based on names
        assert!(!suggestions.is_empty());
        
        // isValid should suggest Bool
        let valid_suggestion = suggestions.iter()
            .find(|s| s.name == "isValid");
        assert!(valid_suggestion.is_some());
        assert_eq!(valid_suggestion.unwrap().suggested_type, Type::Bool);
    }
    
    #[test]
    fn test_migration_progress() {
        let mut checker = GradualChecker::new(GradualMode::Gradual);
        
        // Add some annotated files
        checker.analyze_annotations("a.go", "func A() -> int {}");
        checker.analyze_annotations("b.go", "func B(x) {}");
        checker.analyze_annotations("c.go", "x := 1");
        
        let progress = checker.migration_progress();
        
        assert_eq!(progress.total_files, 3);
        assert!(progress.percentage_complete > 0.0);
    }
}
