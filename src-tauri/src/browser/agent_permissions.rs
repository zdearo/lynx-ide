//! What an agent may do in the browser. The user sets this in the settings: how far an agent may
//! go on a page, and which sites it may visit. The check runs here, not in the agent, so a
//! prompt-injected page cannot talk its way past it.

use serde::{Deserialize, Serialize};
use specta::Type;
use url::Url;

/// How far an agent may go in the page.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum AgentBrowserAccess {
    /// The agent gets no browser tools at all.
    Off,
    /// It can open pages, read them and take screenshots, but not click, type or choose.
    ReadOnly,
    /// It can use the page like a person.
    #[default]
    Full,
}

/// Why a tool call was refused, in words the agent can act on.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    #[error("the user turned the browser off for agents in Lynx's settings")]
    Off,
    #[error(
        "the browser is read-only for agents: it can open and read pages but not click, type or choose"
    )]
    ReadOnly,
    #[error("{0} is not on the list of sites the user lets agents visit (Settings > Browser)")]
    SiteNotAllowed(String),
}

/// Whether an agent may visit `address` under the list of allowed sites. An empty list allows
/// every site. A listed name covers itself and its subdomains. A blank page is always fine.
pub fn site_allowed(allowed: &[String], address: &str) -> bool {
    if allowed.is_empty() || address.is_empty() || address == "about:blank" {
        return true;
    }
    let Some(host) = Url::parse(address)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
    else {
        return false;
    };
    allowed.iter().any(|entry| {
        let entry = entry.trim().trim_start_matches("*.").to_ascii_lowercase();
        !entry.is_empty() && (host == entry || host.ends_with(&format!(".{entry}")))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(entries: &[&str]) -> Vec<String> {
        entries.iter().map(|entry| (*entry).to_owned()).collect()
    }

    #[test]
    fn an_empty_list_allows_every_site() {
        assert!(site_allowed(&[], "https://anything.test/path"));
    }

    #[test]
    fn a_listed_site_covers_itself_and_its_subdomains_only() {
        let allowed = list(&["example.com"]);
        assert!(site_allowed(&allowed, "https://example.com/a"));
        assert!(site_allowed(&allowed, "https://docs.Example.com/a"));
        assert!(!site_allowed(&allowed, "https://notexample.com/"));
        assert!(!site_allowed(&allowed, "https://example.com.evil.test/"));
    }

    #[test]
    fn a_wildcard_prefix_means_the_same_as_the_bare_name() {
        assert!(site_allowed(
            &list(&["*.example.com"]),
            "https://a.example.com/"
        ));
    }

    #[test]
    fn a_blank_page_and_the_start_page_are_always_allowed() {
        assert!(site_allowed(&list(&["example.com"]), "about:blank"));
        assert!(site_allowed(&list(&["example.com"]), ""));
    }

    #[test]
    fn an_address_without_a_host_is_refused_when_a_list_is_set() {
        assert!(!site_allowed(&list(&["example.com"]), "data:text/html,hi"));
    }
}
