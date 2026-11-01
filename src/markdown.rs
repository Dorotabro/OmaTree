//! Markdown preview support: how a note body is turned into something to
//! look at, and which links may be opened. Presentation only: the note body
//! stays exactly the raw text it always was.

use cxx_qt_lib::{QString, QStringList};
use omatree_core::link::is_safe_external_link;

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
