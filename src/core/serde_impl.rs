//! Serde serialization implementations for custom types

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::sync::Arc;

/// Serialize Arc<str> as String
pub fn serialize_arc_str<S>(arc: &Arc<str>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    arc.as_ref().serialize(serializer)
}

/// Deserialize Arc<str> from String
pub fn deserialize_arc_str<'de, D>(deserializer: D) -> Result<Arc<str>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    Ok(Arc::from(s))
}

/// Serialize Option<Arc<str>> as Option<String>
pub fn serialize_option_arc_str<S>(opt: &Option<Arc<str>>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match opt {
        Some(arc) => arc.as_ref().serialize(serializer),
        None => serializer.serialize_none(),
    }
}

/// Deserialize Option<Arc<str>> from Option<String>
pub fn deserialize_option_arc_str<'de, D>(deserializer: D) -> Result<Option<Arc<str>>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt: Option<String> = Deserialize::deserialize(deserializer)?;
    Ok(opt.map(|s| Arc::from(s)))
}
