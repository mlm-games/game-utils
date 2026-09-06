//! Item definitions, stacks, and the definition registry.

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

/// Opaque item key (`"iron_sword"`, `"healing_herb"`).
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub struct ItemId(pub String);

impl ItemId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl From<&str> for ItemId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

impl fmt::Display for ItemId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Static item data. Categories/sockets are free-form `tags`.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ItemDef {
    pub id: ItemId,
    pub name: String,
    pub description: String,
    pub stackable: bool,
    pub max_stack: u32,
    pub weight: f32,
    pub value: i64,
    /// Shaped-inventory footprint in cells.
    pub cells_w: u8,
    pub cells_h: u8,
    pub tags: Vec<String>,
}

impl ItemDef {
    /// Effective per-stack cap: 1 for non-stackables.
    pub fn stack_limit(&self) -> u32 {
        if self.stackable {
            self.max_stack.max(1)
        } else {
            1
        }
    }

    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }
}

/// Durability/decay on a single stack (`None` = indestructible).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Condition {
    pub current: u32,
    pub max: u32,
}

impl Condition {
    pub fn full(max: u32) -> Self {
        Self { current: max, max }
    }

    pub fn broken(&self) -> bool {
        self.current == 0
    }

    /// Returns true when this hit breaks it.
    pub fn damage(&mut self, amount: u32) -> bool {
        self.current = self.current.saturating_sub(amount);
        self.broken()
    }

    pub fn repair(&mut self, amount: u32) {
        self.current = (self.current + amount).min(self.max);
    }
}

/// Owned pile of one item kind. `condition` gates merging.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ItemStack {
    pub def: ItemId,
    pub qty: u32,
    pub condition: Option<Condition>,
}

impl ItemStack {
    pub fn new(def: impl Into<ItemId>, qty: u32) -> Self {
        Self {
            def: def.into(),
            qty: qty.max(1),
            condition: None,
        }
    }

    pub fn with_condition(mut self, c: Condition) -> Self {
        self.condition = Some(c);
        self
    }

    pub fn can_merge_with(&self, o: &Self) -> bool {
        self.def == o.def && self.condition == o.condition
    }
}

/// Id -> def lookup. Games register loot tables here.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct ItemRegistry {
    defs: HashMap<ItemId, ItemDef>,
}

impl ItemRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, def: ItemDef) -> Option<ItemDef> {
        self.defs.insert(def.id.clone(), def)
    }

    pub fn def(&self, id: &ItemId) -> Option<&ItemDef> {
        self.defs.get(id)
    }

    pub fn contains(&self, id: &ItemId) -> bool {
        self.defs.contains_key(id)
    }

    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&ItemId, &ItemDef)> {
        self.defs.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(id: &str, stackable: bool, max: u32) -> ItemDef {
        ItemDef {
            id: ItemId::from(id),
            name: id.into(),
            description: String::new(),
            stackable,
            max_stack: max,
            weight: 1.0,
            value: 5,
            cells_w: 1,
            cells_h: 1,
            tags: vec!["weapon".into()],
        }
    }

    #[test]
    fn stack_limits() {
        assert_eq!(def("a", false, 99).stack_limit(), 1);
        assert_eq!(def("b", true, 0).stack_limit(), 1);
        assert_eq!(def("c", true, 7).stack_limit(), 7);
        assert!(def("c", true, 7).has_tag("weapon"));
    }

    #[test]
    fn condition_break_repair() {
        let mut c = Condition::full(3);
        assert!(!c.damage(2));
        assert!(c.damage(1));
        assert!(c.broken());
        c.repair(2);
        assert_eq!(c.current, 2);
    }

    #[test]
    fn merge_gates_on_condition() {
        let a = ItemStack::new("x", 1);
        let b = ItemStack::new("x", 1).with_condition(Condition::full(3));
        assert!(a.can_merge_with(&ItemStack::new("x", 2)));
        assert!(!a.can_merge_with(&b));
    }

    #[test]
    fn registry_roundtrip() {
        let mut r = ItemRegistry::new();
        r.register(def("x", true, 9));
        assert!(r.contains(&ItemId::from("x")));
        assert_eq!(r.def(&ItemId::from("x")).unwrap().max_stack, 9);
    }
}
