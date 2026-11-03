use md5::{Digest, Md5};

/// Gravatar avatar URL for a user's email, matching the platform convention in
/// the user service (`app/oauth2_server/views.py`): MD5 of the trimmed,
/// lowercased email. `d=identicon` serves a generated image when the email has
/// no registered Gravatar, so the URL never 404s.
pub fn gravatar_url(email: &str) -> String {
    let hash = Md5::digest(email.trim().to_lowercase());
    format!("https://www.gravatar.com/avatar/{hash:x}?s=230&d=identicon&r=pg")
}

/// Whether a URL is an https Gravatar avatar URL. Vendor profile avatars are
/// restricted to this host so user-supplied values can never introduce
/// arbitrary external images on public pages.
pub fn is_gravatar_url(url: &str) -> bool {
    match url.strip_prefix("https://") {
        Some(rest) => {
            let host = rest.split(['/', '?', '#']).next().unwrap_or("");
            host.eq_ignore_ascii_case("gravatar.com")
                || (host.len() > "gravatar.com".len()
                    && host.to_ascii_lowercase().ends_with(".gravatar.com"))
        }
        None => false,
    }
}

/// Whether a URL is a plain http(s) URL, rejecting `javascript:` and other
/// schemes that must never reach an `href` attribute.
pub fn is_http_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gravatar_url_hashes_trimmed_lowercase_email() {
        // md5("test@example.com")
        assert_eq!(
            gravatar_url("  Test@Example.COM  "),
            "https://www.gravatar.com/avatar/55502f40dc8b7c769880b10874abc9d0?s=230&d=identicon&r=pg"
        );
    }

    #[test]
    fn gravatar_url_accepts_any_email_shape() {
        assert!(gravatar_url("").starts_with("https://www.gravatar.com/avatar/"));
    }

    #[test]
    fn is_gravatar_url_accepts_gravatar_hosts_only() {
        assert!(is_gravatar_url("https://www.gravatar.com/avatar/abc?s=230"));
        assert!(is_gravatar_url("https://gravatar.com/avatar/abc"));
        assert!(is_gravatar_url("https://secure.gravatar.com/avatar/abc"));
        assert!(!is_gravatar_url("http://www.gravatar.com/avatar/abc"));
        assert!(!is_gravatar_url(
            "https://evil.example/gravatar.com/avatar/abc"
        ));
        assert!(!is_gravatar_url("https://gravatar.com.evil.example/abc"));
        assert!(!is_gravatar_url("https://example.com/avatar.png"));
        assert!(!is_gravatar_url("javascript:alert(1)"));
        assert!(!is_gravatar_url("data:image/svg+xml,<svg/>"));
        assert!(!is_gravatar_url(""));
    }

    #[test]
    fn is_http_url_rejects_non_http_schemes() {
        assert!(is_http_url("https://example.com"));
        assert!(is_http_url("http://example.com"));
        assert!(!is_http_url("javascript:alert(1)"));
        assert!(!is_http_url("data:text/html,<script>"));
        assert!(!is_http_url("vbscript:msgbox(1)"));
        assert!(!is_http_url("example.com"));
        assert!(!is_http_url(""));
    }
}
