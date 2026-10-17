//! Language of the messages produced by the Rust core: errors, validation
//! messages, monitor descriptions. The UI sends its language with
//! `set_locale`; until then the system language is used.
//!
//! Messages are written with the [`t!`] macro, which keeps the three
//! translations side by side:
//!
//! ```ignore
//! t!("channel {channel} is invalid", "canal {channel} invalide", "canal {channel} no válido")
//! ```

use std::cell::Cell;
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

/// Formats the message in the current language. Takes three format string
/// literals, English, French and Spanish, which can capture variables.
#[macro_export]
macro_rules! t {
    ($en:literal, $fr:literal, $es:literal $(,)?) => {
        match $crate::i18n::current() {
            $crate::i18n::Lang::En => format!($en),
            $crate::i18n::Lang::Fr => format!($fr),
            $crate::i18n::Lang::Es => format!($es),
        }
    };
}

#[cfg(test)]
mod tests {
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
    fn formats_in_the_current_language() {
        let channel = 3;
        let message = || t!("channel {channel}", "canal {channel}", "canal {channel} (es)");
        assert_eq!(with_lang(Lang::En, message), "channel 3");
        assert_eq!(with_lang(Lang::Fr, message), "canal 3");
        assert_eq!(with_lang(Lang::Es, message), "canal 3 (es)");
    }
}
