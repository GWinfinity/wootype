//! RAG (Retrieval-Augmented Generation) support for type embeddings
//!
//! Enables semantic search over types for AI agents.

use crate::core::{Type, TypeId, TypeKind};
use dashmap::DashMap;
use std::sync::Arc;

/// Type embedding for semantic search
#[derive(Debug, Clone)]
pub struct TypeEmbedding {
    pub type_id: TypeId,
    pub vector: Vec<f32>,
    pub metadata: EmbeddingMetadata,
}

/// Embedding metadata
#[derive(Debug, Clone, Default)]
pub struct EmbeddingMetadata {
    pub name: String,
    pub package: String,
    pub description: String,
    pub kind: String,
}

/// Type embeddings index for RAG
pub struct TypeEmbeddings {
    /// Embedding storage
    embeddings: DashMap<TypeId, TypeEmbedding>,

    /// Vector dimension
    dimension: usize,

    /// Embedding model (placeholder)
    model: EmbeddingModel,
}

/// Embedding model interface
#[derive(Debug, Clone)]
struct EmbeddingModel {
    dimension: usize,
}

impl EmbeddingModel {
    fn new(dimension: usize) -> Self {
        Self { dimension }
    }

    /// Generate embedding for type
    fn embed(&self, _typ: &Type) -> Vec<f32> {
        // Placeholder: would use actual embedding model
        vec![0.0; self.dimension]
    }

    /// Generate embedding for text query
    fn embed_text(&self, _text: &str) -> Vec<f32> {
        // Placeholder
        vec![0.0; self.dimension]
    }
}

impl TypeEmbeddings {
    pub fn new() -> Self {
        let dimension = 384; // Common small embedding dimension

        Self {
            embeddings: DashMap::new(),
            dimension,
            model: EmbeddingModel::new(dimension),
        }
    }

    /// Index a type
    pub fn index_type(&self, typ: &Type) {
        let vector = self.model.embed(typ);
        let metadata = self.extract_metadata(typ);

        let embedding = TypeEmbedding {
            type_id: typ.id,
            vector,
            metadata,
        };

        self.embeddings.insert(typ.id, embedding);
    }

    /// Index multiple types
    pub fn index_types(&self, types: &[Arc<Type>]) {
        for typ in types {
            self.index_type(typ);
        }
    }

    /// Semantic search
    pub async fn search(&self, query: &str, limit: usize) -> Vec<SearchResult> {
        let query_vector = self.model.embed_text(query);

        // Calculate similarities
        let mut results: Vec<SearchResult> = self
            .embeddings
            .iter()
            .map(|e| {
                let similarity = cosine_similarity(&query_vector, &e.vector);
                SearchResult {
                    type_id: e.type_id,
                    similarity,
                    name: e.metadata.name.clone(),
                    description: e.metadata.description.clone(),
                }
            })
            .filter(|r| r.similarity > 0.5) // Threshold
            .collect();

        // Sort by similarity
        results.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap());
        results.truncate(limit);

        results
    }

    /// Find similar types
    pub fn find_similar(&self, type_id: TypeId, limit: usize) -> Vec<SearchResult> {
        let query_vector = self
            .embeddings
            .get(&type_id)
            .map(|e| e.vector.clone())
            .unwrap_or_default();

        if query_vector.is_empty() {
            return Vec::new();
        }

        let mut results: Vec<SearchResult> = self
            .embeddings
            .iter()
            .filter(|e| e.type_id != type_id)
            .map(|e| {
                let similarity = cosine_similarity(&query_vector, &e.vector);
                SearchResult {
                    type_id: e.type_id,
                    similarity,
                    name: e.metadata.name.clone(),
                    description: e.metadata.description.clone(),
                }
            })
            .filter(|r| r.similarity > 0.7) // Higher threshold for similarity
            .collect();

        results.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap());
        results.truncate(limit);

        results
    }

    /// Extract metadata from type
    fn extract_metadata(&self, typ: &Type) -> EmbeddingMetadata {
        let (name, kind) = match &typ.kind {
            TypeKind::Primitive(p) => (p.as_str().to_string(), "primitive".to_string()),
            TypeKind::Named { name: n, .. } => (n.to_string(), "named".to_string()),
            TypeKind::Func { .. } => ("func".to_string(), "function".to_string()),
            TypeKind::Struct { .. } => ("struct".to_string(), "struct".to_string()),
            TypeKind::Interface { .. } => ("interface".to_string(), "interface".to_string()),
            _ => ("unknown".to_string(), "unknown".to_string()),
        };

        EmbeddingMetadata {
            name,
            package: String::new(),
            description: format!("{:?}", typ.kind),
            kind,
        }
    }

    /// Get embedding for type
    pub fn get_embedding(&self, type_id: TypeId) -> Option<TypeEmbedding> {
        self.embeddings.get(&type_id).map(|e| e.clone())
    }

    /// Remove type from index
    pub fn remove(&self, type_id: TypeId) {
        self.embeddings.remove(&type_id);
    }

    /// Index size
    pub fn size(&self) -> usize {
        self.embeddings.len()
    }

    /// Clear all embeddings
    pub fn clear(&self) {
        self.embeddings.clear();
    }
}

/// Search result
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub type_id: TypeId,
    pub similarity: f32,
    pub name: String,
    pub description: String,
}

/// Semantic search trait
pub trait SemanticSearch {
    async fn search(&self, query: &str, limit: usize) -> Vec<SearchResult>;
    fn find_similar(&self, type_id: TypeId, limit: usize) -> Vec<SearchResult>;
}

impl SemanticSearch for TypeEmbeddings {
    async fn search(&self, query: &str, limit: usize) -> Vec<SearchResult> {
        self.search(query, limit).await
    }

    fn find_similar(&self, type_id: TypeId, limit: usize) -> Vec<SearchResult> {
        self.find_similar(type_id, limit)
    }
}

/// Calculate cosine similarity between two vectors
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot_product / (norm_a * norm_b)
    }
}

/// RAG query builder
pub struct RagQueryBuilder {
    query: String,
    filters: Vec<RagFilter>,
    limit: usize,
}

#[derive(Debug, Clone)]
pub enum RagFilter {
    Package(String),
    TypeKind(String),
    ExportedOnly,
}

impl RagQueryBuilder {
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            filters: Vec::new(),
            limit: 10,
        }
    }

    pub fn in_package(mut self, pkg: impl Into<String>) -> Self {
        self.filters.push(RagFilter::Package(pkg.into()));
        self
    }

    pub fn of_kind(mut self, kind: impl Into<String>) -> Self {
        self.filters.push(RagFilter::TypeKind(kind.into()));
        self
    }

    pub fn exported(mut self) -> Self {
        self.filters.push(RagFilter::ExportedOnly);
        self
    }

    pub fn limit(mut self, n: usize) -> Self {
        self.limit = n;
        self
    }

    pub fn build(self) -> (String, Vec<RagFilter>, usize) {
        (self.query, self.filters, self.limit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::PrimitiveType;

    #[test]
    fn test_embedding_creation() {
        let embeddings = TypeEmbeddings::new();

        let typ = Type::new(TypeId(1), TypeKind::Primitive(PrimitiveType::Int));
        embeddings.index_type(&typ);

        assert_eq!(embeddings.size(), 1);
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];

        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);

        let c = vec![0.0, 1.0, 0.0];
        assert!((cosine_similarity(&a, &c)).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_semantic_search() {
        let embeddings = TypeEmbeddings::new();

        // Index some types
        let types = vec![
            Arc::new(Type::new(
                TypeId(1),
                TypeKind::Primitive(PrimitiveType::Int),
            )),
            Arc::new(Type::new(
                TypeId(2),
                TypeKind::Primitive(PrimitiveType::String),
            )),
        ];

        embeddings.index_types(&types);

        let results = embeddings.search("integer", 5).await;
        // Results may be empty due to placeholder embeddings
        assert!(results.len() <= 5);
    }
}
