//! Sim-side i18n: [`LocaleResources`] as a resource.
//!
//! Register embedded FTL at boot, switch locales from settings/UI code,
//! read strings via [`I18nStrings::get`]. Rendering stays in Repose views.

use bevy_ecs::prelude::*;
use game_utils::i18n::LocaleResources;

/// Current translations as a sim resource.
#[derive(Resource, Clone, Default)]
pub struct I18nStrings(pub LocaleResources);

impl I18nStrings {
    pub fn new(keys: &[&str], locales: &[(&str, &str)]) -> Self {
        Self::from_iter(keys, locales.iter().copied())
    }

    /// Build from any iterator of `(locale, ftl)` pairs. Accepts the same
    /// `include_str!` sources as [`Self::new`] but also works with
    /// `OnceLock`-cached tables (`Vec`, arrays, maps) without an
    /// intermediate slice.
    pub fn from_iter<'a>(
        keys: &[&str],
        locales: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Self {
        let mut res = LocaleResources::default();
        for (locale, ftl) in locales {
            res.register(locale, ftl, keys);
        }
        if !res.available.contains(&res.current) {
            let default_locale = if res.available.contains(&"en".to_string()) {
                "en".to_string()
            } else {
                res.available
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "en".to_string())
            };
            res.set_locale(&default_locale);
        }
        Self(res)
    }

    /// Borrowed lookup with the `current -> fallback -> key` chain. Prefer
    /// this in hot paths: unlike [`Self::get`] it never allocates.
    /// Note: the key must live as long as the borrow (the miss fallback
    /// returns the key itself); for temporary keys use [`Self::get`].
    pub fn get_str<'a>(&'a self, key: &'a str) -> &'a str {
        game_utils::i18n::translate(&self.0, key)
    }

    /// Look up a key with the `current -> fallback -> key` chain.
    /// Allocates; prefer [`Self::get_str`] when the caller only needs `&str`.
    pub fn get(&self, key: &str) -> String {
        self.get_str(key).to_string()
    }

    pub fn set_locale(&mut self, locale: &str) -> bool {
        self.0.set_locale(locale)
    }
}

/// Insert translations built from embedded FTL.
pub fn register_i18n(world: &mut World, keys: &[&str], locales: &[(&str, &str)]) {
    world.insert_resource(I18nStrings::new(keys, locales));
}
