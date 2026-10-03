//! Markdown preview support: how a note body is turned into something to
//! look at, and which links may be opened. Presentation only: the note body
//! stays exactly the raw text it always was.

use cxx_qt_lib::{QString, QStringList};

/// Whether activating this link may open it in the platform's normal
/// external handler. Only http, https and mailto links are allowed; anything
/// else (javascript:, data:, file:, relative or malformed text) is refused.
pub fn is_safe_external_link(link: &str) -> bool {
    // No whitespace or control characters anywhere, and nothing before the
    // scheme: a link that needs cleaning up is not one to open.
    if link.is_empty() || link.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    let Some((scheme, rest)) = link.split_once(':') else {
        return false;
    };
    match scheme.to_ascii_lowercase().as_str() {
        "http" | "https" => {
            // "//host[/...]" with a non-empty host.
            let Some(after) = rest.strip_prefix("//") else {
                return false;
            };
            let host = after.split(['/', '?', '#']).next().unwrap_or("");
            // Drop any userinfo and port; what is left must have a name.
            let host = host.rsplit('@').next().unwrap_or("");
            let host = host.split(':').next().unwrap_or("");
            !host.is_empty()
        }
        // "mailto:someone@example.com" (optionally with ?subject=...).
        "mailto" => {
            let address = rest.split('?').next().unwrap_or("");
            address.contains('@') && !address.starts_with('@') && !address.ends_with('@')
        }
        _ => false,
    }
}

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;

        include!("omatree/cpp/markdown_render.h");
        /// Markdown in, safe rich text out. See `cpp/markdown_render.h`.
        fn omatree_render_markdown(markdown: &QString, colors: &QStringList) -> QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        type Markdown = super::MarkdownRust;
    }

    unsafe extern "RustQt" {
        /// Renders Markdown for the read-only preview. The result has no
        /// images or other resources, so showing it can never load anything.
        /// `colors` are theme colours as "role=#rrggbb" entries.
        #[qinvokable]
        fn render(self: &Markdown, markdown: &QString, colors: &QStringList) -> QString;

        /// Whether a link from the preview may be opened externally.
        #[qinvokable]
        #[cxx_name = "isSafeLink"]
        fn is_safe_link(self: &Markdown, link: &QString) -> bool;
    }
}

#[derive(Default)]
pub struct MarkdownRust;

impl qobject::Markdown {
    fn render(&self, markdown: &QString, colors: &QStringList) -> QString {
        qobject::omatree_render_markdown(markdown, colors)
    }

    fn is_safe_link(&self, link: &QString) -> bool {
        is_safe_external_link(&String::from(link))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_https_and_mailto_links_are_allowed() {
        for link in [
            "http://example.com",
            "https://example.com/path?q=1#frag",
            "HTTPS://EXAMPLE.COM",
            "http://localhost:8080/x",
            "https://user@example.com/",
            "mailto:someone@example.com",
            "MAILTO:someone@example.com?subject=Hi%20there",
        ] {
            assert!(is_safe_external_link(link), "{link}");
        }
    }

    #[test]
    fn everything_else_is_refused() {
        for link in [
            "",
            "javascript:alert(1)",
            "JaVaScRiPt:alert(1)",
            "data:text/html;base64,PHNjcmlwdD4=",
            "file:///etc/passwd",
            "ftp://example.com/",
            "vbscript:x",
            "tel:+123",
            "x-custom://thing",
            "/relative/path",
            "relative.html",
            "#anchor",
            "example.com",
            "http:example.com",
            "http:///nohost",
            "https://",
            "http://:80/",
            "mailto:",
            "mailto:nobody",
            "mailto:@example.com",
            " https://example.com",
            "https://example.com/a b",
            "https://example.com/\n",
            "java\tscript:alert(1)",
        ] {
            assert!(!is_safe_external_link(link), "{link:?}");
        }
    }
}
