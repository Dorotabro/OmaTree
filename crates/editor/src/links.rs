//! Which links may be opened, and with which click.

use egui::Modifiers;

/// The destination if it is a plain `http://` or `https://` URL with a host and nothing that could
/// be misread by the OS opener (whitespace, control characters, backslashes, userinfo tricks are
/// not interpreted: the string is passed through unchanged). Every other scheme (`file:`,
/// `javascript:`, `mailto:`, custom handlers) and every relative or malformed target is rejected.
pub fn safe_http_url(url: &str) -> Option<&str> {
    let url = url.trim();
    let (scheme_len, rest) = if url.len() >= 8 && url[..8].eq_ignore_ascii_case("https://") {
        (8, &url[8..])
    } else if url.len() >= 7 && url[..7].eq_ignore_ascii_case("http://") {
        (7, &url[7..])
    } else {
        return None;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let bad = |c: char| c.is_control() || c.is_whitespace() || matches!(c, '\\' | '<' | '>' | '"');
    let ok = !host.is_empty()
        && !host.starts_with(['.', ':', '@'])
        && !url.chars().any(bad)
        && url.len() > scheme_len;
    ok.then_some(url)
}

/// Does a click with these modifiers open a link? Ctrl on Linux and Windows, Command on macOS:
/// egui folds both into `command`, so Ctrl on macOS (a context-click there) does not qualify.
pub fn opens_link(m: Modifiers) -> bool {
    m.command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_http_and_https_with_a_host_are_opened() {
        for ok in [
            "https://example.com",
            "http://example.com/a?b=c#d",
            "HTTPS://Example.com/Příliš",
            "  https://example.com  ",
            "http://localhost:8080/",
        ] {
            assert!(safe_http_url(ok).is_some(), "{ok:?}");
        }
        for bad in [
            "",
            "https://",
            "http:///path",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "mailto:a@b.c",
            "ftp://example.com",
            "tel:123",
            "data:text/html,x",
            "x-custom://run?cmd=1",
            "relative/path.md",
            "//example.com",
            "https:example.com",
            "https://exa mple.com",
            "https://example.com/a b",
            "https://example.com/\u{7}",
            "https://example.com\\evil",
            "https://@evil",
            "https://.evil",
            "http://<script>",
        ] {
            assert!(safe_http_url(bad).is_none(), "{bad:?}");
        }
        assert_eq!(safe_http_url(" https://a.org "), Some("https://a.org"));
    }

    #[test]
    fn modifier_is_ctrl_on_linux_and_windows_and_command_on_macos() {
        // Linux / Windows: Ctrl is the command key.
        let ctrl = Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        };
        // macOS: Cmd is the command key; Ctrl alone is not.
        let cmd = Modifiers {
            mac_cmd: true,
            command: true,
            ..Default::default()
        };
        let mac_ctrl = Modifiers {
            ctrl: true,
            ..Default::default()
        };
        assert!(opens_link(ctrl) && opens_link(cmd));
        assert!(!opens_link(mac_ctrl));
        assert!(!opens_link(Modifiers::NONE));
        assert!(!opens_link(Modifiers::SHIFT) && !opens_link(Modifiers::ALT));
    }
}
