use std::collections::HashMap;

use fluent_bundle::{FluentBundle, FluentResource};

/// Parses an FTL string for `locale` and extracts the values for the given `keys`.
pub fn load_ftl(locale: &str, ftl: &str, keys: &[&str]) -> (String, HashMap<String, String>) {
    if let Ok(res) = FluentResource::try_new(ftl.to_string()) {
        let langid: unic_langid::LanguageIdentifier =
            locale.parse().unwrap_or_else(|_| "en".parse().unwrap());
        let mut bundle = FluentBundle::new(vec![langid]);
        bundle.set_use_isolating(false);
        if bundle.add_resource(res).is_ok() {
            let mut map = HashMap::new();
            for key in keys {
                let value = bundle
                    .get_message(key)
                    .and_then(|msg| msg.value())
                    .map(|pattern| {
                        bundle
                            .format_pattern(pattern, None, &mut Vec::new())
                            .into_owned()
                    })
                    .unwrap_or_else(|| key.to_string());
                map.insert(key.to_string(), value);
            }
            return (locale.to_string(), map);
        }
    }
    (locale.to_string(), HashMap::new())
}

/// Bevy-agnostic locale resources: holds registered languages and the current translations.
#[derive(Default, Clone)]
pub struct LocaleResources {
    pub current: String,
    pub available: Vec<String>,
    pub translations: HashMap<String, String>,
    all: HashMap<String, HashMap<String, String>>,
}

impl LocaleResources {
    pub fn register(&mut self, locale: &str, ftl: &str, keys: &[&str]) {
        let (loc, map) = load_ftl(locale, ftl, keys);
        if self.available.contains(&loc) {
            return;
        }
        self.available.push(loc.clone());
        self.all.insert(loc, map);
        if self.current.is_empty() {
            self.current = locale.to_string();
            self.translations = self.all[locale].clone();
        }
    }

    pub fn set_locale(&mut self, locale: &str) {
        if self.all.contains_key(locale) {
            self.current = locale.to_string();
            self.translations = self.all[locale].clone();
        }
    }

    pub fn translate(&self, key: &str) -> Option<&str> {
        self.translations.get(key).map(String::as_str)
    }

    pub fn refresh(&mut self) {
        self.translations = self.all.get(&self.current).cloned().unwrap_or_default();
    }
}

pub fn get_current_translations(locale: &LocaleResources) -> HashMap<String, String> {
    locale.translations.clone()
}

pub fn translate<'a>(locale: &'a LocaleResources, key: &'a str) -> &'a str {
    locale.translate(key).unwrap_or(key)
}
