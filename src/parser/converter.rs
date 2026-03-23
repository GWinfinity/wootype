//! AST to TypeUniverse converter
//! 
//! Converts Go AST nodes to wooftype type representations.

use crate::core::{SharedUniverse, Type, TypeId, TypeKind, PrimitiveType, Entity};
use crate::core::types::{StructField, FuncParam, InterfaceMethod, ChanDir, TypeFingerprint};
use super::ast::*;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Converts AST types to TypeUniverse types
pub struct TypeConverter {
    universe: SharedUniverse,
    type_counter: AtomicU64,
}

impl TypeConverter {
    pub fn new(universe: SharedUniverse) -> Self {
        // Start counter after primitives
        Self {
            universe,
            type_counter: AtomicU64::new(1000),
        }
    }
    
    /// Convert a declaration to type universe
    pub async fn convert_decl(&self, decl: &Decl) -> Result<Option<TypeId>, ConvertError> {
        match decl {
            Decl::Type(spec) => {
                let typ = self.convert_type_spec(spec).await?;
                Ok(Some(typ))
            }
            Decl::Func(func) => {
                let typ = self.convert_func_decl(func).await?;
                Ok(Some(typ))
            }
            _ => Ok(None),
        }
    }
    
    /// Convert type specification
    async fn convert_type_spec(&self, spec: &TypeSpec) -> Result<TypeId, ConvertError> {
        let underlying = self.convert_type_expr(&spec.underlying).await?;
        
        let id = self.next_type_id();
        let kind = TypeKind::Named {
            pkg_path: Arc::from(""),
            name: Arc::from(spec.name.as_str()),
            underlying,
        };
        
        let typ = Type::new(id, kind);
        self.universe.insert_type(id, Arc::new(typ));
        
        // Register symbol
        let symbol = self.universe.symbols().intern(&spec.name);
        // Would associate type with symbol
        
        Ok(id)
    }
    
    /// Convert function declaration
    async fn convert_func_decl(&self, func: &FuncDecl) -> Result<TypeId, ConvertError> {
        let params = self.convert_fields(&func.params).await?;
        let results = self.convert_fields(&func.results).await?;
        
        let id = self.next_type_id();
        
        let kind = if let Some(_recv) = &func.recv {
            // Method - simplified for now
            TypeKind::Signature {
                recv: None, // Simplified
                params,
                results,
                variadic: false,
            }
        } else {
            // Regular function
            TypeKind::Func {
                params,
                results,
                variadic: false,
            }
        };
        
        let typ = Type::new(id, kind);
        self.universe.insert_type(id, Arc::new(typ));
        
        Ok(id)
    }
    
    /// Convert type expression
    fn convert_type_expr<'a>(&'a self, expr: &'a TypeExpr) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<TypeId, ConvertError>> + Send + 'a>> {
        Box::pin(async move {
        match expr {
            TypeExpr::Ident(name) => {
                self.resolve_type_by_name(name).await
            }
            TypeExpr::Selector { pkg, name } => {
                self.resolve_qualified_type(pkg, name).await
            }
            TypeExpr::Pointer(elem) => {
                let elem_id = self.convert_type_expr(elem).await?;
                let id = self.next_type_id();
                let kind = TypeKind::Pointer { elem: elem_id };
                let typ = Type::new(id, kind);
                self.universe.insert_type(id, Arc::new(typ));
                Ok(id)
            }
            TypeExpr::Slice(elem) => {
                let elem_id = self.convert_type_expr(elem).await?;
                let id = self.next_type_id();
                let kind = TypeKind::Slice { elem: elem_id };
                let typ = Type::new(id, kind);
                self.universe.insert_type(id, Arc::new(typ));
                Ok(id)
            }
            TypeExpr::Array { len: _, elem } => {
                let elem_id = self.convert_type_expr(elem).await?;
                let id = self.next_type_id();
                // Would evaluate len expression
                let kind = TypeKind::Array { len: 0, elem: elem_id };
                let typ = Type::new(id, kind);
                self.universe.insert_type(id, Arc::new(typ));
                Ok(id)
            }
            TypeExpr::Map { key, value } => {
                let key_id = self.convert_type_expr(key).await?;
                let value_id = self.convert_type_expr(value).await?;
                let id = self.next_type_id();
                let kind = TypeKind::Map { key: key_id, value: value_id };
                let typ = Type::new(id, kind);
                self.universe.insert_type(id, Arc::new(typ));
                Ok(id)
            }
            TypeExpr::Chan { dir, elem } => {
                let elem_id = self.convert_type_expr(elem).await?;
                let id = self.next_type_id();
                let chan_dir = match dir {
                    super::ast::ChanDir::Send => crate::core::types::ChanDir::Send,
                    super::ast::ChanDir::Recv => crate::core::types::ChanDir::Recv,
                    super::ast::ChanDir::Both => crate::core::types::ChanDir::Both,
                };
                let kind = TypeKind::Chan { dir: chan_dir, elem: elem_id };
                let typ = Type::new(id, kind);
                self.universe.insert_type(id, Arc::new(typ));
                Ok(id)
            }
            TypeExpr::Func { params, results } => {
                let params = self.convert_fields(params).await?;
                let results = self.convert_fields(results).await?;
                let id = self.next_type_id();
                let kind = TypeKind::Func {
                    params,
                    results,
                    variadic: false,
                };
                let typ = Type::new(id, kind);
                self.universe.insert_type(id, Arc::new(typ));
                Ok(id)
            }
            TypeExpr::Struct(fields) => {
                self.convert_struct_type(fields).await
            }
            TypeExpr::Interface(elems) => {
                self.convert_interface_type(elems).await
            }
            _ => Err(ConvertError::UnsupportedType(format!("{:?}", expr))),
        }
        })
    }
    
    /// Convert struct type
    async fn convert_struct_type(&self, fields: &[Field]) -> Result<TypeId, ConvertError> {
        let mut struct_fields = Vec::new();
        
        for field in fields {
            let typ = self.convert_type_expr(&field.typ).await?;
            
            if field.names.is_empty() {
                // Embedded field
                struct_fields.push(StructField {
                    name: Arc::from(""),
                    typ,
                    embedded: true,
                    tag: field.tag.clone().map(|t| Arc::from(t.as_str())),
                });
            } else {
                for name in &field.names {
                    struct_fields.push(StructField {
                        name: Arc::from(name.as_str()),
                        typ,
                        embedded: false,
                        tag: field.tag.clone().map(|t| Arc::from(t.as_str())),
                    });
                }
            }
        }
        
        let id = self.next_type_id();
        let kind = TypeKind::Struct { fields: struct_fields };
        let typ = Type::new(id, kind);
        self.universe.insert_type(id, Arc::new(typ));
        
        Ok(id)
    }
    
    /// Convert interface type
    async fn convert_interface_type(&self, elems: &[InterfaceElem]) -> Result<TypeId, ConvertError> {
        let mut methods = Vec::new();
        let mut embedded = Vec::new();
        
        for elem in elems {
            match elem {
                InterfaceElem::Method(method) => {
                    let params = self.convert_fields(&method.params).await?;
                    let results = self.convert_fields(&method.results).await?;
                    
                    // Create method signature type
                    let sig_id = self.next_type_id();
                    let sig_kind = TypeKind::Func { params, results, variadic: false };
                    let sig_typ = Type::new(sig_id, sig_kind);
                    self.universe.insert_type(sig_id, Arc::new(sig_typ));
                    
                    methods.push(InterfaceMethod {
                        name: Arc::from(method.name.as_str()),
                        sig: sig_id,
                    });
                }
                InterfaceElem::Type(type_elem) => {
                    // Handle type elements (embedded types, constraints)
                    match type_elem {
                        TypeElem::Type(te) => {
                            if let Ok(type_id) = self.convert_type_expr(te).await {
                                embedded.push(type_id);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        
        let id = self.next_type_id();
        let kind = TypeKind::Interface {
            methods,
            embedded,
            implicit: false,
        };
        let typ = Type::new(id, kind);
        self.universe.insert_type(id, Arc::new(typ));
        
        Ok(id)
    }
    
    /// Convert fields to FuncParams
    async fn convert_fields(&self, fields: &[Field]) -> Result<Vec<FuncParam>, ConvertError> {
        let mut params = Vec::new();
        
        for field in fields {
            let typ = self.convert_type_expr(&field.typ).await?;
            
            if field.names.is_empty() {
                // Unnamed parameter
                params.push(FuncParam {
                    name: None,
                    typ,
                });
            } else {
                for name in &field.names {
                    params.push(FuncParam {
                        name: Some(Arc::from(name.as_str())),
                        typ,
                    });
                }
            }
        }
        
        Ok(params)
    }
    
    /// Resolve type by name
    async fn resolve_type_by_name(&self, name: &str) -> Result<TypeId, ConvertError> {
        // Check primitives
        if let Some(prim) = self.parse_primitive(name) {
            let id = TypeId(prim as u64 + 1);
            return Ok(id);
        }
        
        // Look up in universe
        if let Some(symbol) = self.universe.symbols().lookup(None, name) {
            if let Some(typ) = self.universe.lookup_by_symbol(symbol) {
                return Ok(typ.id);
            }
        }
        
        // Create placeholder for unresolved type
        let id = self.next_type_id();
        let kind = TypeKind::Named {
            pkg_path: Arc::from(""),
            name: Arc::from(name),
            underlying: id, // Self-referential for now
        };
        let typ = Type::new(id, kind);
        self.universe.insert_type(id, Arc::new(typ));
        
        Ok(id)
    }
    
    /// Resolve qualified type (package.Name)
    async fn resolve_qualified_type(&self, pkg: &str, name: &str) -> Result<TypeId, ConvertError> {
        // Would look up in package
        // For now, create a placeholder
        let id = self.next_type_id();
        let kind = TypeKind::Named {
            pkg_path: Arc::from(pkg),
            name: Arc::from(name),
            underlying: id,
        };
        let typ = Type::new(id, kind);
        self.universe.insert_type(id, Arc::new(typ));
        
        Ok(id)
    }
    
    /// Parse primitive type name
    fn parse_primitive(&self, name: &str) -> Option<PrimitiveType> {
        match name {
            "bool" => Some(PrimitiveType::Bool),
            "int" => Some(PrimitiveType::Int),
            "int8" => Some(PrimitiveType::Int8),
            "int16" => Some(PrimitiveType::Int16),
            "int32" => Some(PrimitiveType::Int32),
            "int64" => Some(PrimitiveType::Int64),
            "uint" => Some(PrimitiveType::Uint),
            "uint8" => Some(PrimitiveType::Uint8),
            "uint16" => Some(PrimitiveType::Uint16),
            "uint32" => Some(PrimitiveType::Uint32),
            "uint64" => Some(PrimitiveType::Uint64),
            "uintptr" => Some(PrimitiveType::Uintptr),
            "float32" => Some(PrimitiveType::Float32),
            "float64" => Some(PrimitiveType::Float64),
            "complex64" => Some(PrimitiveType::Complex64),
            "complex128" => Some(PrimitiveType::Complex128),
            "string" => Some(PrimitiveType::String),
            "unsafe.Pointer" => Some(PrimitiveType::UnsafePointer),
            _ => None,
        }
    }
    
    fn next_type_id(&self) -> TypeId {
        TypeId(self.type_counter.fetch_add(1, Ordering::SeqCst))
    }
}

/// Conversion error
#[derive(Debug)]
pub enum ConvertError {
    UnsupportedType(String),
    UnresolvedType(String),
    InvalidSyntax(String),
}

impl std::fmt::Display for ConvertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedType(s) => write!(f, "Unsupported type: {}", s),
            Self::UnresolvedType(s) => write!(f, "Unresolved type: {}", s),
            Self::InvalidSyntax(s) => write!(f, "Invalid syntax: {}", s),
        }
    }
}

impl std::error::Error for ConvertError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::TypeUniverse;

    fn setup_converter() -> TypeConverter {
        let universe = Arc::new(TypeUniverse::new());
        TypeConverter::new(universe)
    }

    #[tokio::test]
    async fn test_primitive_conversion() {
        let converter = setup_converter();
        
        let expr = TypeExpr::Ident("int".to_string());
        let result = converter.convert_type_expr(&expr).await;
        
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_pointer_conversion() {
        let converter = setup_converter();
        
        let expr = TypeExpr::Pointer(Box::new(TypeExpr::Ident("int".to_string())));
        let result = converter.convert_type_expr(&expr).await;
        
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_primitive() {
        let converter = setup_converter();
        
        assert!(converter.parse_primitive("int").is_some());
        assert!(converter.parse_primitive("string").is_some());
        assert!(converter.parse_primitive("UnknownType").is_none());
    }
}
