//! Type query patterns and filters
//!
//! Declarative pattern matching for type queries.

use crate::core::{Type, TypeFlags, TypeId, TypeKind};
use regex::Regex;

/// Pattern for matching types
#[derive(Debug, Clone)]
pub enum TypePattern {
    /// Match any type
    Any,
    /// Match exact type by ID
    Exact(TypeId),
    /// Match by kind
    Kind(TypeKindPattern),
    /// Match by flags
    Flags(TypeFlags),
    /// Match named type by regex
    Name(Regex),
    /// Match structurally (similar shape)
    Structural(Box<TypePattern>),
    /// Match composite pattern (all must match)
    All(Vec<TypePattern>),
    /// Match any of patterns
    AnyOf(Vec<TypePattern>),
    /// Negated pattern
    Not(Box<TypePattern>),
}

impl TypePattern {
    /// Check if a type matches this pattern
    pub fn matches(&self, typ: &Type) -> bool {
        match self {
            Self::Any => true,
            Self::Exact(id) => typ.id == *id,
            Self::Kind(kind_pattern) => kind_pattern.matches(&typ.kind),
            Self::Flags(flags) => typ.flags.contains(*flags),
            Self::Name(regex) => Self::match_name(&typ.kind, regex),
            Self::Structural(subpattern) => {
                // Check structural similarity
                Self::match_structural(typ, subpattern)
            }
            Self::All(patterns) => patterns.iter().all(|p| p.matches(typ)),
            Self::AnyOf(patterns) => patterns.iter().any(|p| p.matches(typ)),
            Self::Not(pattern) => !pattern.matches(typ),
        }
    }

    fn match_name(kind: &TypeKind, regex: &Regex) -> bool {
        match kind {
            TypeKind::Named { name, .. } => regex.is_match(name),
            _ => false,
        }
    }

    fn match_structural(_typ: &Type, _pattern: &TypePattern) -> bool {
        // Structural matching implementation
        // Would involve recursive comparison
        true
    }

    /// Builder: match primitive types
    pub fn primitive() -> Self {
        Self::Flags(TypeFlags::BASIC)
    }

    /// Builder: match interface types
    pub fn interface() -> Self {
        Self::Flags(TypeFlags::INTERFACE)
    }

    /// Builder: match function types
    pub fn function() -> Self {
        Self::Flags(TypeFlags::FUNC)
    }

    /// Builder: match pointer types
    pub fn pointer() -> Self {
        Self::Flags(TypeFlags::POINTER)
    }

    /// Builder: match composite types
    pub fn composite() -> Self {
        Self::Flags(TypeFlags::COMPOSITE)
    }

    /// Builder: match comparable types
    pub fn comparable() -> Self {
        Self::Flags(TypeFlags::COMPARABLE)
    }

    /// Combine with AND
    pub fn and(self, other: TypePattern) -> Self {
        match self {
            Self::All(mut patterns) => {
                patterns.push(other);
                Self::All(patterns)
            }
            _ => Self::All(vec![self, other]),
        }
    }

    /// Combine with OR
    pub fn or(self, other: TypePattern) -> Self {
        match self {
            Self::AnyOf(mut patterns) => {
                patterns.push(other);
                Self::AnyOf(patterns)
            }
            _ => Self::AnyOf(vec![self, other]),
        }
    }

    /// Negate
    pub fn not(self) -> Self {
        Self::Not(Box::new(self))
    }
}

/// Pattern for matching type kinds
#[derive(Debug, Clone)]
pub enum TypeKindPattern {
    Primitive,
    Named,
    Pointer(Box<TypePattern>),
    Slice(Box<TypePattern>),
    Array {
        len: Option<u64>,
        elem: Box<TypePattern>,
    },
    Map {
        key: Box<TypePattern>,
        value: Box<TypePattern>,
    },
    Chan {
        dir: ChanDirPattern,
        elem: Box<TypePattern>,
    },
    Func {
        params: Vec<TypePattern>,
        results: Vec<TypePattern>,
    },
    Struct {
        fields: Vec<(String, TypePattern)>,
    },
    Interface {
        methods: Vec<(String, TypePattern)>,
    },
}

impl TypeKindPattern {
    pub fn matches(&self, kind: &TypeKind) -> bool {
        match (self, kind) {
            (Self::Primitive, TypeKind::Primitive(_)) => true,
            (Self::Named, TypeKind::Named { .. }) => true,
            (Self::Pointer(_elem_pat), TypeKind::Pointer { elem: _ }) => {
                // Would need to look up type
                true
            }
            (Self::Slice(_elem_pat), TypeKind::Slice { elem: _ }) => true,
            (
                Self::Array { len, elem: _ },
                TypeKind::Array {
                    len: arr_len,
                    elem: _arr_elem,
                },
            ) => len.map_or(true, |l| l == *arr_len),
            (
                Self::Map { key: _, value: _ },
                TypeKind::Map {
                    key: _m_key,
                    value: _m_value,
                },
            ) => true,
            (
                Self::Chan { dir, elem: _ },
                TypeKind::Chan {
                    dir: c_dir,
                    elem: _c_elem,
                },
            ) => dir.matches(c_dir),
            (
                Self::Func {
                    params: _,
                    results: _,
                },
                TypeKind::Func {
                    params: _f_params,
                    results: _f_results,
                    ..
                },
            ) => {
                // Check param/result count and patterns
                true
            }
            (Self::Struct { fields: _ }, TypeKind::Struct { fields: _s_fields }) => {
                // Check field count and names
                true
            }
            (
                Self::Interface { methods: _ },
                TypeKind::Interface {
                    methods: _i_methods,
                    ..
                },
            ) => {
                // Check method signatures
                true
            }
            _ => false,
        }
    }
}

/// Channel direction pattern
#[derive(Debug, Clone)]
pub enum ChanDirPattern {
    Send,
    Recv,
    Both,
    Any,
}

impl ChanDirPattern {
    fn matches(&self, dir: &crate::core::types::ChanDir) -> bool {
        match (self, dir) {
            (Self::Send, crate::core::types::ChanDir::Send) => true,
            (Self::Recv, crate::core::types::ChanDir::Recv) => true,
            (Self::Both, crate::core::types::ChanDir::Both) => true,
            (Self::Any, _) => true,
            _ => false,
        }
    }
}

/// Query filter for result refinement
#[derive(Debug, Clone, Default)]
pub struct QueryFilter {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub min_score: Option<f32>,
    pub package: Option<String>,
    pub exported_only: bool,
    pub sort_by: SortOrder,
}

impl QueryFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn limit(mut self, n: usize) -> Self {
        self.limit = Some(n);
        self
    }

    pub fn offset(mut self, n: usize) -> Self {
        self.offset = Some(n);
        self
    }

    pub fn min_score(mut self, score: f32) -> Self {
        self.min_score = Some(score);
        self
    }

    pub fn in_package(mut self, pkg: impl Into<String>) -> Self {
        self.package = Some(pkg.into());
        self
    }

    pub fn exported(mut self) -> Self {
        self.exported_only = true;
        self
    }

    pub fn sort(mut self, order: SortOrder) -> Self {
        self.sort_by = order;
        self
    }
}

/// Sort order for query results
#[derive(Debug, Clone, Copy, Default)]
pub enum SortOrder {
    #[default]
    Relevance,
    Name,
    Popularity,
    RecentlyUsed,
}

/// Query builder for fluent API
pub struct QueryBuilder {
    pattern: TypePattern,
    filter: QueryFilter,
}

impl QueryBuilder {
    pub fn new(pattern: TypePattern) -> Self {
        Self {
            pattern,
            filter: QueryFilter::default(),
        }
    }

    pub fn filter(mut self, filter: QueryFilter) -> Self {
        self.filter = filter;
        self
    }

    pub fn limit(self, n: usize) -> Self {
        Self {
            pattern: self.pattern,
            filter: self.filter.limit(n),
        }
    }

    pub fn build(self) -> (TypePattern, QueryFilter) {
        (self.pattern, self.filter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::PrimitiveType;

    #[test]
    fn test_pattern_any() {
        let pattern = TypePattern::Any;
        let typ = Type::new(TypeId(1), TypeKind::Primitive(PrimitiveType::Int));
        assert!(pattern.matches(&typ));
    }

    #[test]
    fn test_pattern_exact() {
        let pattern = TypePattern::Exact(TypeId(1));
        let typ = Type::new(TypeId(1), TypeKind::Primitive(PrimitiveType::Int));
        assert!(pattern.matches(&typ));

        let typ2 = Type::new(TypeId(2), TypeKind::Primitive(PrimitiveType::Int));
        assert!(!pattern.matches(&typ2));
    }

    #[test]
    fn test_pattern_flags() {
        let pattern = TypePattern::primitive();
        let typ = Type::new(TypeId(1), TypeKind::Primitive(PrimitiveType::Int));
        assert!(pattern.matches(&typ));
    }

    #[test]
    fn test_pattern_combinators() {
        let p1 = TypePattern::primitive();
        let p2 = TypePattern::comparable();

        let combined = p1.and(p2);
        let typ = Type::new(TypeId(1), TypeKind::Primitive(PrimitiveType::Int));
        assert!(combined.matches(&typ));
    }
}
