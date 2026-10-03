#pragma once

#include <QtCore/QString>

// Renders Markdown to a small, safe rich-text string for a read-only viewer.
//
// The Markdown is only ever read, never changed. The result contains no
// images or other resources, so showing it cannot load anything: images in
// the Markdown become their alt text, and raw HTML is not interpreted.
//
// `linkColor`, `mutedColor` and `codeBackground` are `#rrggbb` strings taken
// from the application's semantic theme.
QString omatree_render_markdown(const QString &markdown, const QString &linkColor,
                                const QString &mutedColor, const QString &codeBackground);
