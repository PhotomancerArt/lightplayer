//! What this browser is called on your devices, and the words for "this
//! phone" (plan D13).
//!
//! Every browser that plugs a device in by USB leaves a key named after
//! itself. The default name is `<given name>'s <platform>` when signed in
//! ("Yona's Mac", "Yona's iPhone") and `<browser> on <platform>` when not
//! ("Chrome on Mac"); Settings renames it. A page cannot know the model
//! ("MacBook" is not knowable), so the platform is the family: Mac, iPhone,
//! iPad, Android, Windows, Linux.
//!
//! The platform comes from `navigator.userAgentData.platform` where the
//! browser has it (Chromium), else from the user-agent string.

/// The device family this page runs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BrowserPlatform {
    Mac,
    IPhone,
    IPad,
    Android,
    Windows,
    Linux,
    Unknown,
}

impl BrowserPlatform {
    /// Its name ("Mac", "iPhone").
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Mac => "Mac",
            Self::IPhone => "iPhone",
            Self::IPad => "iPad",
            Self::Android => "Android",
            Self::Windows => "Windows",
            Self::Linux => "Linux",
            Self::Unknown => "this device",
        }
    }

    /// The word after "this" in a sentence ("Remember on this phone",
    /// "Saved on this Mac").
    pub(crate) fn this_word(self) -> &'static str {
        match self {
            Self::IPhone | Self::Android => "phone",
            Self::IPad => "iPad",
            Self::Mac => "Mac",
            Self::Windows | Self::Linux => "computer",
            Self::Unknown => "browser",
        }
    }

    /// Whether to draw it as a phone rather than a laptop.
    pub(crate) fn is_phone(self) -> bool {
        matches!(self, Self::IPhone | Self::Android)
    }
}

/// The platform from the user-agent string (and the client-hints platform,
/// when the browser gives one).
pub(crate) fn platform_from(user_agent: &str, hint: Option<&str>) -> BrowserPlatform {
    let ua = user_agent.to_ascii_lowercase();
    // iPadOS asks for the desktop site and says "Macintosh"; a touch Mac
    // does not exist, so the caller's touch check is folded into `hint`
    // ("iPad") — see `detect_platform`.
    if ua.contains("iphone") || ua.contains("ipod") {
        return BrowserPlatform::IPhone;
    }
    if ua.contains("ipad") {
        return BrowserPlatform::IPad;
    }
    if ua.contains("android") {
        return BrowserPlatform::Android;
    }
    match hint.map(str::to_ascii_lowercase).as_deref() {
        Some("macos") => return BrowserPlatform::Mac,
        Some("ipad") => return BrowserPlatform::IPad,
        Some("windows") => return BrowserPlatform::Windows,
        Some("android") => return BrowserPlatform::Android,
        Some("linux" | "chrome os" | "chromeos") => return BrowserPlatform::Linux,
        _ => {}
    }
    if ua.contains("macintosh") || ua.contains("mac os x") {
        BrowserPlatform::Mac
    } else if ua.contains("windows") {
        BrowserPlatform::Windows
    } else if ua.contains("linux") || ua.contains("cros") {
        BrowserPlatform::Linux
    } else {
        BrowserPlatform::Unknown
    }
}

/// The browser's everyday name from its user-agent string. Brave hides in
/// Chrome's string, so the caller passes `is_brave` (`navigator.brave`).
pub(crate) fn browser_from(user_agent: &str, is_brave: bool) -> &'static str {
    let ua = user_agent.to_ascii_lowercase();
    if is_brave {
        "Brave"
    } else if ua.contains("bluefy") {
        "Bluefy"
    } else if ua.contains("edg/") || ua.contains("edga/") || ua.contains("edgios/") {
        "Edge"
    } else if ua.contains("firefox/") || ua.contains("fxios/") {
        "Firefox"
    } else if ua.contains("crios/") || ua.contains("chrome/") {
        "Chrome"
    } else if ua.contains("safari/") {
        "Safari"
    } else {
        "A browser"
    }
}

/// The default name for this browser's key: `<given name>'s <platform>`
/// when signed in, else `<browser> on <platform>`.
pub(crate) fn default_browser_name(
    given_name: Option<&str>,
    browser: &str,
    platform: BrowserPlatform,
) -> String {
    let platform_name = match platform {
        BrowserPlatform::Unknown => "a computer",
        known => known.name(),
    };
    match given_name.map(str::trim).filter(|name| !name.is_empty()) {
        Some(name) => match platform {
            BrowserPlatform::Unknown => format!("{name}'s browser"),
            known => format!("{name}'s {}", known.name()),
        },
        None => format!("{browser} on {platform_name}"),
    }
}

/// Whether a key's label reads like a phone ("Yona's iPhone", "Pixel 8"),
/// for its icon. A guess, and only an icon rides on it.
pub(crate) fn label_looks_like_phone(label: &str) -> bool {
    let label = label.to_ascii_lowercase();
    ["iphone", "phone", "pixel", "android", "galaxy", "bluefy"]
        .iter()
        .any(|word| label.contains(word))
}

/// This page's platform.
pub(crate) fn detect_platform() -> BrowserPlatform {
    #[cfg(target_arch = "wasm32")]
    {
        let Some(navigator) = web_sys::window().map(|window| window.navigator()) else {
            return BrowserPlatform::Unknown;
        };
        let ua = navigator.user_agent().unwrap_or_default();
        let hint = js_sys::Reflect::get(&navigator, &"userAgentData".into())
            .ok()
            .filter(|data| data.is_object())
            .and_then(|data| js_sys::Reflect::get(&data, &"platform".into()).ok())
            .and_then(|platform| platform.as_string());
        // iPadOS reports a Mac with touch.
        let touch = navigator.max_touch_points() > 1;
        let platform = platform_from(&ua, hint.as_deref());
        if platform == BrowserPlatform::Mac && touch {
            return BrowserPlatform::IPad;
        }
        platform
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        // A host build has no user agent to read.
        platform_from("", None)
    }
}

/// This page's browser name.
pub(crate) fn detect_browser() -> &'static str {
    #[cfg(target_arch = "wasm32")]
    {
        let Some(navigator) = web_sys::window().map(|window| window.navigator()) else {
            return "A browser";
        };
        let ua = navigator.user_agent().unwrap_or_default();
        let is_brave = js_sys::Reflect::has(&navigator, &"brave".into()).unwrap_or(false);
        browser_from(&ua, is_brave)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        browser_from("", false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAC_CHROME: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36";
    const IPHONE_SAFARI: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1";
    const ANDROID_CHROME: &str = "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Mobile Safari/537.36";
    const WINDOWS_EDGE: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36 Edg/140.0.0.0";
    const LINUX_FIREFOX: &str =
        "Mozilla/5.0 (X11; Linux x86_64; rv:130.0) Gecko/20100101 Firefox/130.0";

    #[test]
    fn the_platform_is_the_family() {
        assert_eq!(platform_from(MAC_CHROME, None), BrowserPlatform::Mac);
        assert_eq!(
            platform_from(MAC_CHROME, Some("macOS")),
            BrowserPlatform::Mac
        );
        assert_eq!(platform_from(IPHONE_SAFARI, None), BrowserPlatform::IPhone);
        // Android's UA says Linux too; Android wins.
        assert_eq!(
            platform_from(ANDROID_CHROME, None),
            BrowserPlatform::Android
        );
        assert_eq!(platform_from(WINDOWS_EDGE, None), BrowserPlatform::Windows);
        assert_eq!(platform_from(LINUX_FIREFOX, None), BrowserPlatform::Linux);
        assert_eq!(platform_from("", None), BrowserPlatform::Unknown);
    }

    #[test]
    fn the_browser_is_named_the_way_people_say_it() {
        assert_eq!(browser_from(MAC_CHROME, false), "Chrome");
        assert_eq!(browser_from(MAC_CHROME, true), "Brave");
        assert_eq!(browser_from(WINDOWS_EDGE, false), "Edge");
        assert_eq!(browser_from(LINUX_FIREFOX, false), "Firefox");
        assert_eq!(browser_from(IPHONE_SAFARI, false), "Safari");
        assert_eq!(
            browser_from(&format!("{IPHONE_SAFARI} Bluefy/5.0"), false),
            "Bluefy"
        );
    }

    #[test]
    fn the_default_name_is_yours_when_signed_in() {
        assert_eq!(
            default_browser_name(Some("Yona"), "Chrome", BrowserPlatform::Mac),
            "Yona's Mac"
        );
        assert_eq!(
            default_browser_name(Some("Yona"), "Bluefy", BrowserPlatform::IPhone),
            "Yona's iPhone"
        );
        assert_eq!(
            default_browser_name(None, "Chrome", BrowserPlatform::Mac),
            "Chrome on Mac"
        );
        assert_eq!(
            default_browser_name(Some("  "), "Firefox", BrowserPlatform::Unknown),
            "Firefox on a computer"
        );
    }

    #[test]
    fn phones_say_phone() {
        assert_eq!(BrowserPlatform::IPhone.this_word(), "phone");
        assert_eq!(BrowserPlatform::Mac.this_word(), "Mac");
        assert!(label_looks_like_phone("Mireille's Pixel 8 Pro"));
        assert!(!label_looks_like_phone("Yona's Mac"));
    }
}
