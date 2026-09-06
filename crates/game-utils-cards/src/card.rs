use serde::{Deserialize, Serialize};

use crate::energy::Cost;

/// Opaque card identifier. Registries resolve ids to definitions.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CardId(pub String);

impl CardId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for CardId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for CardId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl std::fmt::Display for CardId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::borrow::Borrow<str> for CardId {
    fn borrow(&self) -> &str {
        &self.0
    }
}

/// Open card rarity. Unlisted tiers are [`Rarity::Custom`]; never
/// match exhaustively from outside the crate.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    Custom(String),
}

impl Rarity {
    /// Parse a (case-insensitive) rarity name; unknown names become `Custom`.
    pub fn of(name: &str) -> Self {
        match name.to_ascii_lowercase().as_str() {
            "common" => Self::Common,
            "uncommon" => Self::Uncommon,
            "rare" => Self::Rare,
            other => Self::Custom(other.to_string()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Common => "common",
            Self::Uncommon => "uncommon",
            Self::Rare => "rare",
            Self::Custom(s) => s,
        }
    }
}

impl std::fmt::Display for Rarity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Open card kind (creature, spell, item, ...). Plain string;
/// taxonomies differ per game (see `presets`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CardKind(pub String);

impl CardKind {
    pub fn new(kind: impl Into<String>) -> Self {
        Self(kind.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for CardKind {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for CardKind {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl std::fmt::Display for CardKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Static card definition. `P` carries game-specific data beyond the
/// common header; plain data cards leave `P = ()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound(
    serialize = "P: Serialize",
    deserialize = "P: Deserialize<'de> + Default"
))]
pub struct CardDef<P = ()> {
    pub id: CardId,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub cost: Cost,
    #[serde(default = "default_rarity")]
    pub rarity: Rarity,
    #[serde(default = "default_kind")]
    pub kind: CardKind,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub payload: P,
}

fn default_rarity() -> Rarity {
    Rarity::Common
}

fn default_kind() -> CardKind {
    CardKind::new("other")
}

impl<P> CardDef<P> {
    pub fn new(id: impl Into<String>, name: impl Into<String>, cost: Cost) -> Self
    where
        P: Default,
    {
        Self {
            id: CardId::new(id),
            name: name.into(),
            description: String::new(),
            cost,
            rarity: Rarity::Common,
            kind: CardKind::new("other"),
            tags: Vec::new(),
            payload: P::default(),
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn with_rarity(mut self, rarity: Rarity) -> Self {
        self.rarity = rarity;
        self
    }

    pub fn with_kind(mut self, kind: impl Into<String>) -> Self {
        self.kind = CardKind::new(kind);
        self
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    pub fn with_payload<Q>(self, payload: Q) -> CardDef<Q> {
        CardDef {
            id: self.id,
            name: self.name,
            description: self.description,
            cost: self.cost,
            rarity: self.rarity,
            kind: self.kind,
            tags: self.tags,
            payload,
        }
    }

    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }
}

/// Named starting-deck recipe: card ids with per-card counts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeckTemplate {
    pub id: String,
    pub cards: Vec<CardId>,
    #[serde(default)]
    pub counts: Vec<u32>,
}

impl DeckTemplate {
    /// One copy of each id.
    pub fn new(id: impl Into<String>, cards: Vec<CardId>) -> Self {
        let counts = vec![1; cards.len()];
        Self {
            id: id.into(),
            cards,
            counts,
        }
    }

    /// Explicit counts; short lists pad with 1.
    pub fn with_counts(id: impl Into<String>, cards: Vec<CardId>, counts: Vec<u32>) -> Self {
        let mut counts = counts;
        counts.resize_with(cards.len(), || 1);
        Self {
            id: id.into(),
            cards,
            counts,
        }
    }

    /// Materialize the full id list, repeating each card by its count.
    pub fn expand(&self) -> Vec<CardId> {
        let mut out = Vec::new();
        for (id, count) in self.cards.iter().zip(self.counts.iter()) {
            for _ in 0..(*count).max(1) {
                out.push(id.clone());
            }
        }
        out
    }

    pub fn total(&self) -> usize {
        self.counts.iter().map(|c| (*c).max(1) as usize).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_def_roundtrip() {
        let c: CardDef<()> = CardDef::new("c1", "Test Card", Cost::Fixed(2));
        let s = ron::ser::to_string(&c).unwrap();
        let de: CardDef<()> = ron::from_str(&s).unwrap();
        assert_eq!(de.id.as_str(), "c1");
        assert_eq!(de.cost.effective(&[]), 2);
    }

    #[test]
    fn card_def_payload_and_tags() {
        let c = CardDef::<()>::new("c1", "Spear", Cost::Fixed(1))
            .with_kind("unit")
            .with_rarity(Rarity::of("mythic"))
            .with_tags(vec!["fast".to_string()])
            .with_payload(42u32);
        assert!(c.has_tag("fast"));
        assert!(!c.has_tag("slow"));
        assert_eq!(c.rarity, Rarity::Custom("mythic".to_string()));
        assert_eq!(c.payload, 42u32);
    }

    #[test]
    fn deck_template_expand() {
        let d = DeckTemplate::with_counts("starter", vec!["c1".into(), "c2".into()], vec![2, 3]);
        assert_eq!(d.total(), 5);
        assert_eq!(d.expand().len(), 5);
    }
}
