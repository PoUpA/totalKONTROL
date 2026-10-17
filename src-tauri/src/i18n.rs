//! Language of the messages produced by the Rust core: errors, validation
//! messages, monitor descriptions. The UI sends its language with
//! `set_locale`; until then the system language is used.
//!
//! The texts live in `locales/<lang>.json` at the repository root, shared
//! with the UI and built into the binary. Core messages use `core.*` keys
//! and `{name}` placeholders, filled by the [`t!`] macro:
//!
//! ```ignore
//! t!("core.validation.invalidChannel", channel)            // variable of the same name
//! t!("core.osc.unreachable", host = host, port = port)     // explicit values
//! ```

use std::cell::Cell;
use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    En,
    Fr,
    Es,
}

impl Lang {
    /// Language of a BCP 47 tag such as `fr-CH`; English when unsupported.
    pub fn from_tag(tag: &str) -> Lang {
        match tag.split(['-', '_']).next().unwrap_or("").to_ascii_lowercase().as_str() {
            "fr" => Lang::Fr,
            "es" => Lang::Es,
            _ => Lang::En,
        }
    }

    fn from_u8(value: u8) -> Lang {
        match value {
            1 => Lang::Fr,
            2 => Lang::Es,
            _ => Lang::En,
        }
    }

    fn as_u8(self) -> u8 {
        match self {
            Lang::En => 0,
            Lang::Fr => 1,
            Lang::Es => 2,
        }
    }

    fn source(self) -> &'static str {
        match self {
            Lang::En => include_str!("../../locales/en.json"),
            Lang::Fr => include_str!("../../locales/fr.json"),
            Lang::Es => include_str!("../../locales/es.json"),
        }
    }

    /// The language's texts, parsed once.
    fn messages(self) -> &'static HashMap<String, String> {
        static TABLES: [OnceLock<HashMap<String, String>>; 3] = [OnceLock::new(), OnceLock::new(), OnceLock::new()];
        TABLES[self.as_u8() as usize].get_or_init(|| {
            serde_json::from_str(self.source()).unwrap_or_else(|e| panic!("locales/{self:?}.json: {e}"))
        })
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

thread_local! {
    /// Per-thread override, so tests can check a language without racing.
    static OVERRIDE: Cell<Option<Lang>> = const { Cell::new(None) };
}

pub fn current() -> Lang {
    OVERRIDE.with(Cell::get).unwrap_or_else(|| Lang::from_u8(CURRENT.load(Ordering::Relaxed)))
}

pub fn set(lang: Lang) {
    CURRENT.store(lang.as_u8(), Ordering::Relaxed);
}

/// Starts with the system language.
pub fn init_from_system() {
    set(sys_locale::get_locale().map_or(Lang::En, |tag| Lang::from_tag(&tag)));
}

/// Runs `f` with `lang` as the current language on this thread.
#[cfg(test)]
pub fn with_lang<T>(lang: Lang, f: impl FnOnce() -> T) -> T {
    OVERRIDE.with(|o| o.set(Some(lang)));
    let result = f();
    OVERRIDE.with(|o| o.set(None));
    result
}

/// Text of `key` in the current language (English, then the key itself, as
/// fallbacks), with each `{name}` replaced by its value.
pub fn tr(key: &str, args: &[(&str, String)]) -> String {
    let lang = current();
    let template = lang
        .messages()
        .get(key)
        .or_else(|| Lang::En.messages().get(key))
        .map_or(key, String::as_str);
    let mut text = template.to_string();
    for (name, value) in args {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}

/// Translated message: `t!("core.some.key", name, other = value)`. A bare
/// name fills `{name}` with the variable of that name; `name = expr` with
/// any value. Values are formatted with `Display`.
#[macro_export]
macro_rules! t {
    ($key:literal $(, $($args:tt)*)?) => {{
        #[allow(unused_mut)]
        let mut args: Vec<(&str, String)> = Vec::new();
        $crate::t_args!(args; $($($args)*)?);
        $crate::i18n::tr($key, &args)
    }};
}

#[doc(hidden)]
#[macro_export]
macro_rules! t_args {
    ($args:ident;) => {};
    ($args:ident; $name:ident = $value:expr $(, $($rest:tt)*)?) => {
        $args.push((stringify!($name), $value.to_string()));
        $crate::t_args!($args; $($($rest)*)?);
    };
    ($args:ident; $name:ident $(, $($rest:tt)*)?) => {
        $args.push((stringify!($name), $name.to_string()));
        $crate::t_args!($args; $($($rest)*)?);
    };
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn reads_language_tags() {
        assert_eq!(Lang::from_tag("fr-CH"), Lang::Fr);
        assert_eq!(Lang::from_tag("es_ES"), Lang::Es);
        assert_eq!(Lang::from_tag("en-US"), Lang::En);
        assert_eq!(Lang::from_tag("de-DE"), Lang::En);
        assert_eq!(Lang::from_tag(""), Lang::En);
    }

    #[test]
    fn fills_placeholders_in_the_current_language() {
        let tab = "x";
        let message = || t!("app.tabs.control", tab, unused = 1);
        assert_eq!(with_lang(Lang::En, message), "Control");
        assert_eq!(with_lang(Lang::Fr, message), "Contrôle");
        assert_eq!(tr("no.such.key", &[]), "no.such.key");
        let _ = tab;
    }

    /// `{name}` placeholders of a text.
    fn placeholders(text: &str) -> BTreeSet<&str> {
        text.split('{').skip(1).filter_map(|part| part.split_once('}').map(|(name, _)| name)).collect()
    }

    #[test]
    fn every_language_has_the_same_keys_and_placeholders() {
        let reference = Lang::En.messages();
        for lang in [Lang::Fr, Lang::Es] {
            let messages = lang.messages();
            let missing: Vec<_> = reference.keys().filter(|k| !messages.contains_key(*k)).collect();
            let extra: Vec<_> = messages.keys().filter(|k| !reference.contains_key(*k)).collect();
            assert!(missing.is_empty() && extra.is_empty(), "{lang:?}: missing {missing:?}, extra {extra:?}");
            for (key, text) in reference {
                assert_eq!(
                    placeholders(text),
                    placeholders(&messages[key]),
                    "{lang:?}: placeholders of {key} differ"
                );
            }
        }
    }

    #[test]
    fn every_key_used_in_the_code_exists() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut unknown = vec![];
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            // This file's own examples are not real keys.
            if path.file_name().is_some_and(|name| name == "i18n.rs") {
                continue;
            }
            let code = std::fs::read_to_string(&path).unwrap();
            for call in code.split("t!(\"").skip(1) {
                let key = call.split('"').next().unwrap_or_default();
                if key.starts_with("core.") && !Lang::En.messages().contains_key(key) {
                    unknown.push(format!("{}: {key}", path.display()));
                }
            }
        }
        assert!(unknown.is_empty(), "keys missing from locales/en.json: {unknown:#?}");
    }
}
