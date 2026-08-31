//! `String` is kept for cold paths, but hot paths like for the ones that use
//! `stats`/`unlock`/`achievements`/`codex` now use `TypedId<Tag>` so a `KillCount`
//! cannot be passed as a `CodexId`. Keep Ron: `TypedId` is `Serialize/Deserialize` as
//! its inner `String`, so on-disk format stays Ron `String`.

use std::borrow::Borrow;
use std::fmt;
use std::hash::Hash;
use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

/// Marker types for each ID domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StatTag;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CodexTag;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AchievementTag;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UnlockTag;

/// `Tag` is a zero-sized marker so `TypedId<StatTag>` and
/// `TypedId<CodexTag>` are distinct types. Serializes as a plain string (Ron ` "kills" `)
/// so on-disk stays compatible with the previous `String` id.
pub struct TypedId<Tag> {
    inner: String,
    _tag: PhantomData<Tag>,
}

impl<Tag> Clone for TypedId<Tag> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            _tag: PhantomData,
        }
    }
}
impl<Tag> PartialEq for TypedId<Tag> {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}
impl<Tag> Eq for TypedId<Tag> {}
impl<Tag> Hash for TypedId<Tag> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.inner.hash(state);
    }
}
impl<Tag> PartialOrd for TypedId<Tag> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<Tag> Ord for TypedId<Tag> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.inner.cmp(&other.inner)
    }
}
impl<Tag> Serialize for TypedId<Tag> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.inner.serialize(serializer)
    }
}
impl<'de, Tag> Deserialize<'de> for TypedId<Tag> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let inner = String::deserialize(deserializer)?;
        Ok(Self {
            inner,
            _tag: PhantomData,
        })
    }
}

impl<Tag> TypedId<Tag> {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            inner: id.into(),
            _tag: PhantomData,
        }
    }
    pub fn as_str(&self) -> &str {
        &self.inner
    }
    pub fn into_string(self) -> String {
        self.inner
    }
    pub fn as_string(&self) -> &String {
        &self.inner
    }
}

impl<Tag> fmt::Debug for TypedId<Tag> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple(std::any::type_name::<Tag>())
            .field(&self.inner)
            .finish()
    }
}

impl<Tag> fmt::Display for TypedId<Tag> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.inner)
    }
}

impl<Tag> From<String> for TypedId<Tag> {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}
impl<Tag> From<&str> for TypedId<Tag> {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}
impl<Tag> From<TypedId<Tag>> for String {
    fn from(id: TypedId<Tag>) -> Self {
        id.inner
    }
}
impl<Tag> AsRef<str> for TypedId<Tag> {
    fn as_ref(&self) -> &str {
        &self.inner
    }
}
impl<Tag> std::ops::Deref for TypedId<Tag> {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
impl<Tag> Borrow<str> for TypedId<Tag> {
    fn borrow(&self) -> &str {
        &self.inner
    }
}
impl<Tag> PartialEq<str> for TypedId<Tag> {
    fn eq(&self, other: &str) -> bool {
        self.inner == other
    }
}
impl<Tag> PartialEq<String> for TypedId<Tag> {
    fn eq(&self, other: &String) -> bool {
        self.inner == *other
    }
}
impl<Tag> PartialEq<TypedId<Tag>> for str {
    fn eq(&self, other: &TypedId<Tag>) -> bool {
        *self == other.inner
    }
}
impl<Tag> PartialEq<TypedId<Tag>> for String {
    fn eq(&self, other: &TypedId<Tag>) -> bool {
        *self == other.inner
    }
}

impl<Tag> Default for TypedId<Tag> {
    fn default() -> Self {
        Self::new(String::new())
    }
}

pub type StatId = TypedId<StatTag>;
pub type CodexId = TypedId<CodexTag>;
pub type AchievementId = TypedId<AchievementTag>;
pub type UnlockId = TypedId<UnlockTag>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_id_roundtrip_ron() {
        let id = StatId::new("kills");
        let s = ron::ser::to_string(&id).unwrap();
        let de: StatId = ron::from_str(&s).unwrap();
        assert_eq!(de.as_str(), "kills");
    }

    #[test]
    fn distinct_tags() {
        let s: StatId = "boss".into();
        let c: CodexId = "boss".into();
        assert_eq!(s.as_str(), c.as_str());
    }
}
