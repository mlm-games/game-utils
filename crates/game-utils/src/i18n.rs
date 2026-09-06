use std::collections::HashMap;

use fluent_bundle::{FluentArgs, FluentBundle, FluentResource};

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
                if let Some(value) =
                    bundle
                        .get_message(key)
                        .and_then(|msg| msg.value())
                        .map(|pattern| {
                            bundle
                                .format_pattern(pattern, None, &mut Vec::new())
                                .into_owned()
                        })
                {
                    map.insert(key.to_string(), value);
                }
            }
            return (locale.to_string(), map);
        }
    }
    (locale.to_string(), HashMap::new())
}

/// Bevy-agnostic locale resources: holds registered languages and the current translations.
///
/// CSV workflows are handled by the external tool at `../tools/ftl-csv-convert`
/// (`ftl2csv`/`csv2ftl`, including Godot polyglot CSVs).
#[derive(Clone)]
pub struct LocaleResources {
    pub current: String,
    pub available: Vec<String>,
    pub translations: HashMap<String, String>,
    all: HashMap<String, HashMap<String, String>>,
    ftl_sources: HashMap<String, String>,
    fallback: String,
}

impl Default for LocaleResources {
    fn default() -> Self {
        Self {
            current: String::new(),
            available: Vec::new(),
            translations: HashMap::new(),
            all: HashMap::new(),
            ftl_sources: HashMap::new(),
            fallback: "en".to_string(),
        }
    }
}

impl LocaleResources {
    pub fn register(&mut self, locale: &str, ftl: &str, keys: &[&str]) {
        let (loc, map) = load_ftl(locale, ftl, keys);
        self.ftl_sources.insert(loc.clone(), ftl.to_string());
        if self.available.contains(&loc) {
            self.all.insert(loc.clone(), map);
            if self.current == loc {
                self.refresh();
            }
            return;
        }
        self.available.push(loc.clone());
        self.all.insert(loc.clone(), map);
        if self.current.is_empty() {
            self.current = loc.clone();
            self.translations = self.all.get(&loc).cloned().unwrap_or_default();
        }
    }

    /// Set current locale; returns `true` if the locale exists and was applied,
    /// `false` if unknown (previously silently no-oped).
    pub fn set_locale(&mut self, locale: &str) -> bool {
        if self.all.contains_key(locale) {
            self.current = locale.to_string();
            self.translations = self.all[locale].clone();
            true
        } else {
            false
        }
    }

    pub fn set_fallback(&mut self, locale: impl Into<String>) {
        self.fallback = locale.into();
    }

    pub fn has_locale(&self, locale: &str) -> bool {
        self.all.contains_key(locale)
    }

    /// Simple translate with fallback chain `current -> fallback -> key`.
    pub fn translate(&self, key: &str) -> Option<&str> {
        if let Some(v) = self.translations.get(key) {
            return Some(v);
        }
        if let Some(fb) = self.all.get(&self.fallback).and_then(|m| m.get(key)) {
            return Some(fb.as_str());
        }
        None
    }

    /// Fluent-aware translate with args and plurals. Builds bundle from stored FTL source (keep `LocaleResources: Sync`).
    pub fn translate_with_args(&self, key: &str, args: Option<&FluentArgs>) -> Option<String> {
        for loc in [self.current.as_str(), self.fallback.as_str()] {
            if let Some(ftl) = self.ftl_sources.get(loc) {
                if let Ok(res) = FluentResource::try_new(ftl.clone()) {
                    let langid: unic_langid::LanguageIdentifier =
                        loc.parse().unwrap_or_else(|_| "en".parse().unwrap());
                    let mut bundle = FluentBundle::new(vec![langid]);
                    bundle.set_use_isolating(false);
                    if bundle.add_resource(res).is_ok() {
                        if let Some(msg) = bundle.get_message(key).and_then(|m| m.value()) {
                            let mut errs = Vec::new();
                            return Some(bundle.format_pattern(msg, args, &mut errs).into_owned());
                        }
                    }
                }
            }
        }
        self.translate(key).map(|s| s.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;
    use fluent_bundle::FluentArgs;

    #[test]
    fn fallback_chain() {
        let mut res = LocaleResources::default();
        res.register("en", "hello = Hello\nbye = Goodbye", &["hello", "bye"]);
        res.register("fr", "hello = Bonjour", &["hello", "bye"]);
        res.set_locale("fr");
        assert_eq!(res.translate("hello"), Some("Bonjour"));
        assert_eq!(res.translate("bye"), Some("Goodbye"));
        // unknown key returns None, free fn returns key
        assert_eq!(res.translate("unknown"), None);
        assert_eq!(translate(&res, "unknown"), "unknown");
    }

    #[test]
    fn args_and_plural() {
        let ftl = r#"
hello = Hello { $name }!
items = { $count ->
    [one] One item
   *[other] { $count } items
}
"#;
        let mut res = LocaleResources::default();
        res.register("en", ftl, &["hello", "items"]);
        res.set_locale("en");
        let mut args = FluentArgs::new();
        args.set("name", "Ada");
        assert_eq!(
            res.translate_with_args("hello", Some(&args)).unwrap(),
            "Hello Ada!"
        );
        let mut one = FluentArgs::new();
        one.set("count", 1);
        assert_eq!(
            res.translate_with_args("items", Some(&one)).unwrap(),
            "One item"
        );
        let mut other = FluentArgs::new();
        other.set("count", 5);
        assert_eq!(
            res.translate_with_args("items", Some(&other)).unwrap(),
            "5 items"
        );
    }
}
