use std::collections::HashMap;

use bevy::prelude::*;
use game_utils::i18n as core_i18n;

/// Bevy resource wrapper around the bevy-agnostic [`core_i18n::LocaleResources`].
#[derive(Resource, Deref, DerefMut)]
pub struct LocaleResources(pub core_i18n::LocaleResources);

/// Registers embedded FTL translations for each locale.
///
/// The app supplies the translation keys and the embedded FTL content (via
/// `include_str!`), keeping i18n data app-specific while the parsing lives here.
#[derive(Clone, Default)]
pub struct I18nPlugin {
    pub keys: &'static [&'static str],
    pub locales: &'static [(&'static str, &'static str)],
}

impl I18nPlugin {
    pub fn new(
        keys: &'static [&'static str],
        locales: &'static [(&'static str, &'static str)],
    ) -> Self {
        Self { keys, locales }
    }
}

impl Plugin for I18nPlugin {
    fn build(&self, app: &mut App) {
        let mut resources = core_i18n::LocaleResources::default();
        for (locale, ftl) in self.locales {
            resources.register(locale, ftl, self.keys);
        }
        if !resources.available.contains(&resources.current) {
            let default_locale = if resources.available.contains(&"en".to_string()) {
                "en".to_string()
            } else {
                resources
                    .available
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "en".to_string())
            };
            resources.set_locale(&default_locale);
        }
        app.insert_resource(LocaleResources(resources));
    }
}

pub fn get_current_translations(locale: &LocaleResources) -> HashMap<String, String> {
    locale.translations.clone()
}

pub fn translate<'a>(locale: &'a LocaleResources, key: &'a str) -> &'a str {
    locale.translate(key).unwrap_or(key)
}
