#include "markdown_render.h"

#include <QtGui/QColor>
#include <QtGui/QGuiApplication>
#include <QtGui/QTextBlock>
#include <QtGui/QTextCharFormat>
#include <QtGui/QTextCursor>
#include <QtGui/QTextDocument>
#include <QtGui/QTextFragment>
#include <QtGui/QTextImageFormat>

#include <QtCore/QList>

namespace {

struct Range {
    int position;
    int length;
};

struct ImageRange {
    int position;
    QString alt;
    bool anchor;
    QString href;
};

// Where a heading sits relative to the body text size. Restrained on purpose.
qreal headingScale(int level) {
    switch (level) {
    case 1:
        return 1.5;
    case 2:
        return 1.3;
    case 3:
        return 1.15;
    default:
        return 1.05;
    }
}

} // namespace

QString omatree_render_markdown(const QString &markdown, const QString &linkColor,
                                const QString &mutedColor, const QString &codeBackground) {
    // A document that is never shown is never laid out, so Qt never gets as
    // far as loading a resource for it. Raw HTML is switched off here too.
    QTextDocument doc;
    doc.setDefaultFont(QGuiApplication::font());
    QTextDocument::MarkdownFeatures features(QTextDocument::MarkdownDialectGitHub);
    features |= QTextDocument::MarkdownNoHTML;
    doc.setMarkdown(markdown, features);

    const QColor link(linkColor);
    const QColor muted(mutedColor);
    const QColor codeBg(codeBackground);
    const qreal baseSize = doc.defaultFont().pointSizeF();

    // 1. Images: collect them, then replace each with its alt text. Done
    //    back to front so earlier positions stay valid.
    QList<ImageRange> images;
    for (QTextBlock block = doc.begin(); block.isValid(); block = block.next()) {
        for (QTextBlock::iterator it = block.begin(); !it.atEnd(); ++it) {
            const QTextFragment fragment = it.fragment();
            if (!fragment.isValid() || !fragment.charFormat().isImageFormat())
                continue;
            const QTextCharFormat format = fragment.charFormat();
            ImageRange image;
            image.position = fragment.position();
            image.alt = format.stringProperty(QTextFormat::ImageAltText);
            image.anchor = format.isAnchor();
            image.href = format.anchorHref();
            images.append(image);
        }
    }
    for (int i = images.size() - 1; i >= 0; --i) {
        const ImageRange &image = images.at(i);
        QTextCursor cursor(&doc);
        cursor.setPosition(image.position);
        cursor.setPosition(image.position + 1, QTextCursor::KeepAnchor);
        QTextCharFormat plain;
        if (image.anchor) {
            plain.setAnchor(true);
            plain.setAnchorHref(image.href);
        }
        cursor.insertText(image.alt.isEmpty() ? QStringLiteral("[image]")
                                              : QStringLiteral("[%1]").arg(image.alt),
                          plain);
    }

    // 2. Colours and sizes from the theme, applied to what Markdown produced.
    //    Collected first, because editing while iterating would invalidate it.
    QList<Range> links;
    QList<Range> codeSpans;
    QList<Range> quotes;
    QList<Range> codeBlocks;
    QList<QPair<Range, int>> headings;
    for (QTextBlock block = doc.begin(); block.isValid(); block = block.next()) {
        const QTextBlockFormat blockFormat = block.blockFormat();
        const Range whole{block.position(), block.length() - 1};
        if (blockFormat.intProperty(QTextFormat::BlockQuoteLevel) > 0)
            quotes.append(whole);
        if (blockFormat.hasProperty(QTextFormat::BlockCodeFence) ||
            blockFormat.hasProperty(QTextFormat::BlockCodeLanguage))
            codeBlocks.append(whole);
        if (blockFormat.headingLevel() > 0)
            headings.append({whole, blockFormat.headingLevel()});
        for (QTextBlock::iterator it = block.begin(); !it.atEnd(); ++it) {
            const QTextFragment fragment = it.fragment();
            if (!fragment.isValid())
                continue;
            const Range range{fragment.position(), fragment.length()};
            if (fragment.charFormat().isAnchor())
                links.append(range);
            else if (fragment.charFormat().fontFixedPitch())
                codeSpans.append(range);
        }
    }

    auto apply = [&doc](const Range &range, const QTextCharFormat &format) {
        if (range.length <= 0)
            return;
        QTextCursor cursor(&doc);
        cursor.setPosition(range.position);
        cursor.setPosition(range.position + range.length, QTextCursor::KeepAnchor);
        cursor.mergeCharFormat(format);
    };

    QTextCharFormat mutedFormat;
    mutedFormat.setForeground(muted);
    for (const Range &range : quotes)
        apply(range, mutedFormat);

    QTextCharFormat linkFormat;
    linkFormat.setForeground(link);
    for (const Range &range : links)
        apply(range, linkFormat);

    QTextCharFormat codeFormat;
    codeFormat.setBackground(codeBg);
    for (const Range &range : codeSpans)
        apply(range, codeFormat);
    for (const Range &range : codeBlocks)
        apply(range, codeFormat);

    for (const auto &heading : headings) {
        QTextCharFormat headingFormat;
        headingFormat.setFontWeight(QFont::DemiBold);
        if (baseSize > 0)
            headingFormat.setFontPointSize(baseSize * headingScale(heading.second));
        apply(heading.first, headingFormat);
    }

    return doc.toHtml();
}
