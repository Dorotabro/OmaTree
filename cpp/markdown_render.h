#pragma once

#include <QtCore/QString>
#include <QtCore/QStringList>

// Renders Markdown to a small, safe rich-text string for a read-only viewer.
//
// The Markdown is only ever read, never changed. The result contains no
// images or other resources, so showing it cannot load anything: images in
// the Markdown become their alt text, and raw HTML is not interpreted.
//
// `colors` are theme colours as "role=#rrggbb" entries: accent,
// accentSecondary, foreground, muted, raised, border, warning, positive.
// Roles that are missing or invalid are simply not applied.
QString omatree_render_markdown(const QString &markdown, const QStringList &colors);
