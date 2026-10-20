//! What the address bar accepts. Only web pages: a typed address that points at a file or a
//! script URL never reaches the engine.

use url::Url;

use super::browser_error::BrowserError;

/// Turns what the user typed into an address the engine can load.
///
/// A full `http` or `https` address is kept as is. Something that looks like a host
/// (`example.com/docs`, `localhost:3000`) gets a scheme: `http` for the machine itself, `https`
/// for everything else. Anything else is refused rather than guessed at.
pub fn normalize(input: &str) -> Result<String, BrowserError> {
    let input = input.trim();
    if input.is_empty() || input.chars().any(char::is_whitespace) {
        return Err(BrowserError::InvalidAddress);
    }
    if input == "about:blank" {
        return Ok(input.to_owned());
    }
    let candidate = if input.contains("://") {
        input.to_owned()
    } else if looks_like_local_host(input) {
        format!("http://{input}")
    } else if input.contains('.') {
        format!("https://{input}")
    } else {
        return Err(BrowserError::InvalidAddress);
    };
    let url = Url::parse(&candidate).map_err(|_| BrowserError::InvalidAddress)?;
    match (url.scheme(), url.host_str()) {
        ("http" | "https", Some(_)) => Ok(url.to_string()),
        _ => Err(BrowserError::InvalidAddress),
    }
}

fn looks_like_local_host(input: &str) -> bool {
    let host = input.split(['/', '?', '#']).next().unwrap_or(input);
    let name = host.rsplit_once(':').map_or(host, |(name, _)| name);
    matches!(name, "localhost" | "127.0.0.1" | "[::1]")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(input: &str) -> String {
        normalize(input).unwrap_or_else(|error| panic!("{input}: {error}"))
    }

    #[test]
    fn full_addresses_are_kept() {
        assert_eq!(
            ok("https://example.com/a?b=1#c"),
            "https://example.com/a?b=1#c"
        );
        assert_eq!(ok("http://example.com"), "http://example.com/");
    }

    #[test]
    fn a_bare_host_gets_https() {
        assert_eq!(ok("example.com"), "https://example.com/");
        assert_eq!(ok("docs.rs/url/latest"), "https://docs.rs/url/latest");
    }

    #[test]
    fn the_local_machine_gets_http() {
        assert_eq!(ok("localhost:1420"), "http://localhost:1420/");
        assert_eq!(ok("127.0.0.1:8080/health"), "http://127.0.0.1:8080/health");
        assert_eq!(ok("localhost"), "http://localhost/");
    }

    #[test]
    fn surrounding_space_is_ignored() {
        assert_eq!(ok("  example.com \n"), "https://example.com/");
    }

    #[test]
    fn about_blank_is_allowed() {
        assert_eq!(ok("about:blank"), "about:blank");
    }

    #[test]
    fn anything_that_is_not_a_web_page_is_refused() {
        for input in [
            "",
            "   ",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/html,<script>1</script>",
            "ftp://example.com",
            "chrome://gpu",
            "just some words",
            "word",
            "http://",
        ] {
            assert!(normalize(input).is_err(), "{input:?} should be refused");
        }
    }
}
