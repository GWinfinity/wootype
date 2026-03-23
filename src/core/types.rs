//! Type representations for Go types
//! 
//! Implements a comprehensive type system supporting all Go types
//! with efficient fingerprinting for SIMD-accelerated lookups.

use std::sync::Arc;
use bitflags::bitflags;

/// Unique type identifier (interned)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct TypeId(pub u64);

impl TypeId {
    pub const INVALID: TypeId = TypeId(0);
    
    pub fn new(id: u64) -> Self {
        Self(id)
    }
    
    pub fn is_valid(&self) -> bool {
        self.0 != 0
    }
}

impl Default for TypeId {
    fn default() -> Self {
        Self::INVALID
    }
}

/// Type fingerprint for fast similarity comparison
/// Uses 64-bit hash for nanosecond-level type matching
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
pub struct TypeFingerprint(pub u64);

impl TypeFingerprint {
    /// Compute fingerprint from type components
    pub fn from_components(components: &[u64]) -> Self {
        // FNV-1a inspired hash
        let mut hash: u64 = 0xcbf29ce484222325;
        for &component in components {
            hash ^= component;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        Self(hash)
    }
    
    /// Check if this fingerprint likely matches another
    /// Used for pre-filtering before full type comparison
    pub fn likely_matches(&self, other: &TypeFingerprint) -> bool {
        self.0 == other.0
    }
}

/// Go primitive types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum PrimitiveType {
    Bool,
    Int,
    Int8,
    Int16,
    Int32,
    Int64,
    Uint,
    Uint8,
    Uint16,
    Uint32,
    Uint64,
    Uintptr,
    Float32,
    Float64,
    Complex64,
    Complex128,
    String,
    UnsafePointer,
    UntypedBool,
    UntypedInt,
    UntypedRune,
    UntypedFloat,
    UntypedComplex,
    UntypedString,
    UntypedNil,
}

impl PrimitiveType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::Int => "int",
            Self::Int8 => "int8",
            Self::Int16 => "int16",
            Self::Int32 => "int32",
            Self::Int64 => "int64",
            Self::Uint => "uint",
            Self::Uint8 => "uint8",
            Self::Uint16 => "uint16",
            Self::Uint32 => "uint32",
            Self::Uint64 => "uint64",
            Self::Uintptr => "uintptr",
            Self::Float32 => "float32",
            Self::Float64 => "float64",
            Self::Complex64 => "complex64",
            Self::Complex128 => "complex128",
            Self::String => "string",
            Self::UnsafePointer => "unsafe.Pointer",
            Self::UntypedBool => "untyped bool",
            Self::UntypedInt => "untyped int",
            Self::UntypedRune => "untyped rune",
            Self::UntypedFloat => "untyped float",
            Self::UntypedComplex => "untyped complex",
            Self::UntypedString => "untyped string",
            Self::UntypedNil => "untyped nil",
        }
    }
    
    pub fn fingerprint(&self) -> TypeFingerprint {
        TypeFingerprint(*self as u64)
    }
}

/// Type kind flags for quick classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
pub struct TypeFlags(pub u32);

impl TypeFlags {
    pub const BASIC: Self = Self(1 << 0);
    pub const COMPOSITE: Self = Self(1 << 1);
    pub const NAMED: Self = Self(1 << 2);
    pub const INTERFACE: Self = Self(1 << 3);
    pub const POINTER: Self = Self(1 << 4);
    pub const SLICE: Self = Self(1 << 5);
    pub const ARRAY: Self = Self(1 << 6);
    pub const MAP: Self = Self(1 << 7);
    pub const CHAN: Self = Self(1 << 8);
    pub const FUNC: Self = Self(1 << 9);
    pub const STRUCT: Self = Self(1 << 10);
    pub const TUPLE: Self = Self(1 << 11);
    pub const TYPE_PARAM: Self = Self(1 << 12);
    pub const GENERIC: Self = Self(1 << 13);
    pub const CONSTRAINT: Self = Self(1 << 14);
    pub const COMPARABLE: Self = Self(1 << 15);
    pub const ORDERED: Self = Self(1 << 16);
    pub const NILABLE: Self = Self(1 << 17);
    pub const CONST_TYPE: Self = Self(1 << 18);

    pub fn contains(&self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    pub fn intersects(&self, other: Self) -> bool {
        self.0 & other.0 != 0
    }
}

impl std::ops::BitOr for TypeFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for TypeFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl std::ops::BitAnd for TypeFlags {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}

impl std::ops::Not for TypeFlags {
    type Output = Self;
    fn not(self) -> Self {
        Self(!self.0)
    }
}

/// The kind of a Go type
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum TypeKind {
    /// Primitive/builtin type
    Primitive(PrimitiveType),
    
    /// Named type (type declaration)
    Named {
        #[serde(with = "arc_str")]
        pkg_path: Arc<str>,
        #[serde(with = "arc_str")]
        name: Arc<str>,
        underlying: TypeId,
    },
    
    /// Pointer type
    Pointer {
        elem: TypeId,
    },
    
    /// Slice type
    Slice {
        elem: TypeId,
    },
    
    /// Array type
    Array {
        len: u64,
        elem: TypeId,
    },
    
    /// Map type
    Map {
        key: TypeId,
        value: TypeId,
    },
    
    /// Channel type
    Chan {
        dir: ChanDir,
        elem: TypeId,
    },
    
    /// Function type
    Func {
        params: Vec<FuncParam>,
        results: Vec<FuncParam>,
        variadic: bool,
    },
    
    /// Struct type
    Struct {
        fields: Vec<StructField>,
    },
    
    /// Interface type
    Interface {
        methods: Vec<InterfaceMethod>,
        embedded: Vec<TypeId>,
        implicit: bool, // true for type constraints
    },
    
    /// Type parameter (generic)
    TypeParam {
        #[serde(with = "arc_str")]
        name: Arc<str>,
        constraint: TypeId,
    },
    
    /// Tuple (for multiple return values in type checking)
    Tuple {
        elems: Vec<TypeId>,
    },
    
    /// Signature with receiver (for methods)
    Signature {
        recv: Option<TypeId>,
        params: Vec<FuncParam>,
        results: Vec<FuncParam>,
        variadic: bool,
    },
}

impl TypeKind {
    pub fn flags(&self) -> TypeFlags {
        let mut flags = match self {
            Self::Primitive(p) => {
                let mut f = TypeFlags::BASIC;
                match p {
                    PrimitiveType::Bool | PrimitiveType::UntypedBool => {
                        f |= TypeFlags::COMPARABLE;
                    }
                    PrimitiveType::String | PrimitiveType::UntypedString => {
                        f |= TypeFlags::COMPARABLE | TypeFlags::ORDERED;
                    }
                    PrimitiveType::Int | PrimitiveType::Int8 | PrimitiveType::Int16 |
                    PrimitiveType::Int32 | PrimitiveType::Int64 |
                    PrimitiveType::Uint | PrimitiveType::Uint8 | PrimitiveType::Uint16 |
                    PrimitiveType::Uint32 | PrimitiveType::Uint64 | PrimitiveType::Uintptr |
                    PrimitiveType::UntypedInt | PrimitiveType::UntypedRune => {
                        f |= TypeFlags::COMPARABLE | TypeFlags::ORDERED;
                    }
                    PrimitiveType::Float32 | PrimitiveType::Float64 |
                    PrimitiveType::UntypedFloat => {
                        f |= TypeFlags::ORDERED;
                    }
                    PrimitiveType::Complex64 | PrimitiveType::Complex128 |
                    PrimitiveType::UntypedComplex => {
                        f |= TypeFlags::COMPARABLE;
                    }
                    PrimitiveType::UnsafePointer | PrimitiveType::UntypedNil => {
                        f |= TypeFlags::NILABLE;
                    }
                }
                f
            }
            Self::Named { .. } => TypeFlags::NAMED,
            Self::Pointer { .. } => TypeFlags::POINTER | TypeFlags::NILABLE,
            Self::Slice { .. } => TypeFlags::SLICE | TypeFlags::NILABLE,
            Self::Array { .. } => TypeFlags::ARRAY | TypeFlags::COMPARABLE,
            Self::Map { .. } => TypeFlags::MAP | TypeFlags::NILABLE,
            Self::Chan { .. } => TypeFlags::CHAN | TypeFlags::NILABLE,
            Self::Func { .. } => TypeFlags::FUNC | TypeFlags::NILABLE,
            Self::Struct { .. } => TypeFlags::STRUCT | TypeFlags::COMPOSITE,
            Self::Interface { .. } => TypeFlags::INTERFACE | TypeFlags::NILABLE | TypeFlags::COMPOSITE,
            Self::TypeParam { .. } => TypeFlags::TYPE_PARAM | TypeFlags::GENERIC,
            Self::Tuple { .. } => TypeFlags::TUPLE,
            Self::Signature { .. } => TypeFlags::FUNC,
        };
        
        // Add COMPOSITE flag for complex types
        if !flags.intersects(TypeFlags::BASIC | TypeFlags::NAMED) {
            flags |= TypeFlags::COMPOSITE;
        }
        
        flags
    }
}

/// Channel direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ChanDir {
    Send,
    Recv,
    Both,
}

/// Function parameter
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct FuncParam {
    #[serde(with = "arc_str_opt")]
    pub name: Option<Arc<str>>,
    pub typ: TypeId,
}

/// Struct field
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct StructField {
    #[serde(with = "arc_str")]
    pub name: Arc<str>,
    pub typ: TypeId,
    pub embedded: bool,
    #[serde(with = "arc_str_opt")]
    pub tag: Option<Arc<str>>,
}

/// Interface method
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct InterfaceMethod {
    #[serde(with = "arc_str")]
    pub name: Arc<str>,
    pub sig: TypeId,
}

mod arc_str {
    use serde::{self, Deserialize, Deserializer, Serializer};
    use std::sync::Arc;
    
    pub fn serialize<S>(arc: &Arc<str>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(arc)
    }
    
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Arc<str>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(deserializer)?;
        Ok(Arc::from(s))
    }
}

mod arc_str_opt {
    use serde::{self, Deserialize, Deserializer, Serializer};
    use std::sync::Arc;
    
    pub fn serialize<S>(opt: &Option<Arc<str>>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match opt {
            Some(arc) => serializer.serialize_some(arc.as_ref()),
            None => serializer.serialize_none(),
        }
    }
    
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Arc<str>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt: Option<String> = Deserialize::deserialize(deserializer)?;
        Ok(opt.map(|s| Arc::from(s)))
    }
}

/// A complete type representation
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Type {
    pub id: TypeId,
    pub kind: TypeKind,
    pub fingerprint: TypeFingerprint,
    pub flags: TypeFlags,
}

impl Type {
    pub fn new(id: TypeId, kind: TypeKind) -> Self {
        let fingerprint = Self::compute_fingerprint(&id, &kind);
        let flags = kind.flags();
        
        Self {
            id,
            kind,
            fingerprint,
            flags,
        }
    }
    
    fn compute_fingerprint(id: &TypeId, kind: &TypeKind) -> TypeFingerprint {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        id.hash(&mut hasher);
        std::mem::discriminant(kind).hash(&mut hasher);
        TypeFingerprint(hasher.finish())
    }
    
    /// Check if this type implements the given interface
    pub fn implements(&self, interface: &Type) -> bool {
        if !interface.flags.contains(TypeFlags::INTERFACE) {
            return false;
        }
        
        // TODO: Full interface satisfaction check
        // For now, return conservative result
        match (&self.kind, &interface.kind) {
            (TypeKind::Interface { .. }, TypeKind::Interface { .. }) => true,
            (_, TypeKind::Interface { implicit: true, .. }) => true, // type constraint
            _ => false,
        }
    }
    
    /// Get the underlying type (for named types)
    pub fn underlying(&self) -> Option<TypeId> {
        match &self.kind {
            TypeKind::Named { underlying, .. } => Some(*underlying),
            _ => None,
        }
    }
    
    /// Check if types are identical
    pub fn identical(&self, other: &Type) -> bool {
        self.id == other.id || self.fingerprint == other.fingerprint
    }
}

/// Type constraint for generic type parameters
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum TypeConstraint {
    /// Any type (no constraint)
    Any,
    /// Approximation constraint (~T)
    Approx(TypeId),
    /// Union constraint (A | B | C)
    Union(Vec<TypeId>),
    /// Intersection constraint (must satisfy all)
    Intersection(Vec<TypeId>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_primitive_fingerprint() {
        let int_fp = PrimitiveType::Int.fingerprint();
        let int64_fp = PrimitiveType::Int64.fingerprint();
        
        assert_ne!(int_fp, int64_fp);
    }
    
    #[test]
    fn test_type_flags() {
        let ptr = TypeKind::Pointer { elem: TypeId(1) };
        assert!(ptr.flags().contains(TypeFlags::POINTER));
        assert!(ptr.flags().contains(TypeFlags::NILABLE));
    }
}
