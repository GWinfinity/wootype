//! Gradual typing support for Python interoperability
//!
//! Wootype allows mixing typed and untyped code, with gradual
//! enforcement of type safety at boundaries.

use std::collections::HashMap;
use std::sync::Arc;

use super::{Type, TypeError, Span, ErrorType};

/// Gradual typing mode for a module or function
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GradualMode {
    /// Fully static - all types checked at compile time
    Static,
    /// Gradual - some `any` types allowed
    Gradual,
    /// Dynamic - runtime type checking at boundaries
    Dynamic,
}

/// Type checking boundary
#[derive(Clone, Debug)]
pub struct Boundary {
    /// Source of the boundary (e.g., "Python call", "JSON decode")
    pub source: String,
    /// Expected type at boundary
    pub expected: Type,
    /// Whether runtime checks are inserted
    pub runtime_checks: bool,
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
    Object(String),  // Python object type name
    Any,
}

impl RuntimeTag {
    /// Check if runtime tag matches expected static type
    pub fn matches(&self, ty: &Type) -> bool {
        match (self, ty) {
            (RuntimeTag::Int, Type::Int) => true,
            (RuntimeTag::Float, Type::Float) => true,
            (RuntimeTag::String, Type::String) => true,
            (RuntimeTag::Bool, Type::Bool) => true,
            (RuntimeTag::List(inner), Type::Array(elem)) => inner.matches(elem),
            (RuntimeTag::Dict(k, v), Type::Map(kt, vt)) => {
                k.matches(kt) && v.matches(vt)
            }
            (RuntimeTag::Object(_), Type::Any) => true,
            (RuntimeTag::Any, _) => true,
            _ => false,
        }
    }
    
    /// Convert static type to runtime tag
    pub fn from_type(ty: &Type) -> Self {
        match ty {
            Type::Int => RuntimeTag::Int,
            Type::Float => RuntimeTag::Float,
            Type::String => RuntimeTag::String,
            Type::Bool => RuntimeTag::Bool,
            Type::Array(elem) => RuntimeTag::List(Box::new(RuntimeTag::from_type(elem))),
            Type::Map(k, v) => RuntimeTag::Dict(
                Box::new(RuntimeTag::from_type(k)),
                Box::new(RuntimeTag::from_type(v)),
            ),
            Type::Any => RuntimeTag::Any,
            Type::Named(name) => RuntimeTag::Object(name.clone()),
            _ => RuntimeTag::Any,
        }
    }
}

/// Python interop configuration
#[derive(Clone, Debug)]
pub struct PythonInterop {
    /// Enable type checking at Python boundaries
    pub check_python_calls: bool,
    /// Generate runtime type assertions
    pub runtime_assertions: bool,
    /// Type mappings (Python type -> Wootype type)
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
        type_mappings.insert("torch.Tensor".to_string(), Type::Tensor);
        type_mappings.insert("numpy.ndarray".to_string(), Type::Tensor);
        
        Self {
            check_python_calls: true,
            runtime_assertions: true,
            type_mappings,
        }
    }
}

impl PythonInterop {
    /// Convert Python type annotation to Wootype
    pub fn python_to_wootype(&self, py_type: &str) -> Type {
        // Handle generic types like list[int]
        if let Some((base, inner)) = parse_generic(py_type) {
            if base == "list" || base == "List" {
                let inner_ty = self.python_to_wootype(&inner);
                return Type::Array(Box::new(inner_ty));
            }
            if base == "dict" || base == "Dict" {
                if let Some((k, v)) = parse_dict_args(&inner) {
                    let kt = self.python_to_wootype(&k);
                    let vt = self.python_to_wootype(&v);
                    return Type::Map(Box::new(kt), Box::new(vt));
                }
            }
            if base == "tuple" || base == "Tuple" {
                // Parse tuple elements
                let elems: Vec<Type> = inner
                    .split(',')
                    .map(|s| self.python_to_wootype(s.trim()))
                    .collect();
                return Type::Tuple(elems);
            }
            if base == "Optional" {
                let inner_ty = self.python_to_wootype(&inner);
                return Type::Option(Box::new(inner_ty));
            }
        }
        
        // Direct mapping
        self.type_mappings.get(py_type).cloned().unwrap_or(Type::Any)
    }
    
    /// Check if a Python value can be passed to a Wootype function
    pub fn check_boundary(&self, py_value: &RuntimeTag, expected: &Type) -> Result<(), BoundaryError> {
        if py_value.matches(expected) {
            return Ok(());
        }
        
        Err(BoundaryError {
            runtime_tag: py_value.clone(),
            expected: expected.clone(),
            message: format!("Type mismatch at Python boundary: got {:?}, expected {:?}", py_value, expected),
        })
    }
}

/// Boundary type error
#[derive(Clone, Debug)]
pub struct BoundaryError {
    pub runtime_tag: RuntimeTag,
    pub expected: Type,
    pub message: String,
}

/// Gradual type checker
pub struct GradualChecker {
    mode: GradualMode,
    python_interop: PythonInterop,
    boundaries: Vec<Boundary>,
}

impl GradualChecker {
    pub fn new(mode: GradualMode) -> Self {
        Self {
            mode,
            python_interop: PythonInterop::default(),
            boundaries: vec![],
        }
    }
    
    /// Set Python interop configuration
    pub fn with_python_interop(mut self, interop: PythonInterop) -> Self {
        self.python_interop = interop;
        self
    }
    
    /// Check if two types are compatible in gradual mode
    pub fn is_compatible(&self, t1: &Type, t2: &Type) -> bool {
        match self.mode {
            GradualMode::Static => t1 == t2 || self.is_subtype(t1, t2),
            GradualMode::Gradual | GradualMode::Dynamic => {
                // In gradual/dynamic mode, any is compatible with everything
                matches!((t1, t2), (Type::Any, _) | (_, Type::Any)) || 
                    t1 == t2 || 
                    self.is_subtype(t1, t2)
            }
        }
    }
    
    /// Subtype checking
    fn is_subtype(&self, sub: &Type, sup: &Type) -> bool {
        match (sub, sup) {
            (a, b) if a == b => true,
            (Type::Int, Type::Float) => true,  // int <: float
            (Type::Array(a), Type::Array(b)) => self.is_subtype(a, b),
            (Type::Option(a), Type::Option(b)) => self.is_subtype(a, b),
            (Type::Func(a_args, a_ret), Type::Func(b_args, b_ret)) => {
                // Contravariant in arguments, covariant in return
                if a_args.len() != b_args.len() {
                    return false;
                }
                a_args.iter().zip(b_args.iter()).all(|(a, b)| self.is_subtype(b, a)) &&
                    self.is_subtype(a_ret, b_ret)
            }
            (Type::Struct(a_fields), Type::Struct(b_fields)) => {
                // Structural subtyping
                b_fields.iter().all(|(name, b_ty)| {
                    a_fields.get(name)
                        .map(|a_ty| self.is_subtype(a_ty, b_ty))
                        .unwrap_or(false)
                })
            }
            _ => false,
        }
    }
    
    /// Generate runtime type check code
    pub fn generate_runtime_check(&self, value: &str, expected: &Type) -> Option<String> {
        if self.mode == GradualMode::Static {
            return None;
        }
        
        if !self.python_interop.runtime_assertions {
            return None;
        }
        
        let check = match expected {
            Type::Int => format!("assert isinstance({}, int), 'Expected int'", value),
            Type::Float => format!("assert isinstance({}, (int, float)), 'Expected float'", value),
            Type::String => format!("assert isinstance({}, str), 'Expected str'", value),
            Type::Bool => format!("assert isinstance({}, bool), 'Expected bool'", value),
            Type::Array(elem) => {
                let elem_check = self.generate_runtime_check("x", elem)?;
                format!(
                    "assert isinstance({}, list)\nfor x in {}: {}",
                    value, value, elem_check
                )
            }
            Type::Option(inner) => {
                // Check if None or match inner type
                let inner_check = self.generate_runtime_check(value, inner)?;
                format!(
                    "if {} is not None:\n    {}",
                    value, inner_check
                )
            }
            _ => return None,
        };
        
        Some(check)
    }
    
    /// Add a type boundary
    pub fn add_boundary(&mut self, source: &str, expected: Type, runtime_checks: bool) {
        self.boundaries.push(Boundary {
            source: source.to_string(),
            expected,
            runtime_checks,
        });
    }
    
    /// Get all boundaries
    pub fn boundaries(&self) -> &[Boundary] {
        &self.boundaries
    }
}

/// Parse a generic type like "list[int]"
fn parse_generic(s: &str) -> Option<(String, String)> {
    let start = s.find('[')?;
    let end = s.rfind(']')?;
    
    let base = s[..start].trim().to_string();
    let inner = s[start + 1..end].trim().to_string();
    
    Some((base, inner))
}

/// Parse dict arguments like "str, int"
fn parse_dict_args(s: &str) -> Option<(String, String)> {
    let comma = s.find(',')?;
    Some((
        s[..comma].trim().to_string(),
        s[comma + 1..].trim().to_string(),
    ))
}

/// Convert type errors for gradual mode
pub fn convert_error_for_mode(error: TypeError, mode: GradualMode) -> Option<TypeError> {
    match mode {
        GradualMode::Static => Some(error),
        GradualMode::Gradual => {
            // In gradual mode, allow some type mismatches involving any
            match &error.error_type {
                ErrorType::TypeMismatch { expected, found } => {
                    if matches!((expected, found), (Type::Any, _) | (_, Type::Any)) {
                        // Suppress error in gradual mode
                        None
                    } else {
                        Some(error)
                    }
                }
                _ => Some(error),
            }
        }
        GradualMode::Dynamic => {
            // In dynamic mode, only report serious errors
            match &error.error_type {
                ErrorType::TypeMismatch { .. } => None,
                ErrorType::UnknownIdentifier(_) => None,
                _ => Some(error),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_python_to_wootype() {
        let interop = PythonInterop::default();
        
        assert_eq!(interop.python_to_wootype("int"), Type::Int);
        assert_eq!(interop.python_to_wootype("str"), Type::String);
        assert_eq!(interop.python_to_wootype("list[int]"), Type::Array(Box::new(Type::Int)));
        assert_eq!(interop.python_to_wootype("dict[str, float]"), 
            Type::Map(Box::new(Type::String), Box::new(Type::Float)));
    }
    
    #[test]
    fn test_runtime_tag_matching() {
        assert!(RuntimeTag::Int.matches(&Type::Int));
        assert!(RuntimeTag::List(Box::new(RuntimeTag::Int)).matches(&Type::Array(Box::new(Type::Int))));
        assert!(!RuntimeTag::Int.matches(&Type::String));
    }
    
    #[test]
    fn test_gradual_compatibility() {
        let checker = GradualChecker::new(GradualMode::Gradual);
        
        assert!(checker.is_compatible(&Type::Any, &Type::Int));
        assert!(checker.is_compatible(&Type::Int, &Type::Any));
        assert!(checker.is_compatible(&Type::Int, &Type::Int));
    }
    
    #[test]
    fn test_static_incompatibility() {
        let checker = GradualChecker::new(GradualMode::Static);
        
        assert!(!checker.is_compatible(&Type::Any, &Type::Int));
        assert!(!checker.is_compatible(&Type::Int, &Type::String));
    }
    
    #[test]
    fn test_parse_generic() {
        assert_eq!(
            parse_generic("list[int]"),
            Some(("list".to_string(), "int".to_string()))
        );
        assert_eq!(
            parse_generic("dict[str, float]"),
            Some(("dict".to_string(), "str, float".to_string()))
        );
    }
}