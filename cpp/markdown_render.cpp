#include "markdown_render.h"

#include <QtGui/QColor>
#include <QtGui/QFontMetricsF>
#include <QtGui/QGuiApplication>
#include <QtGui/QTextBlock>
#include <QtGui/QTextCharFormat>
#include <QtGui/QTextCursor>
#include <QtGui/QTextDocument>
#include <QtGui/QTextDocumentFragment>
#include <QtGui/QTextFragment>
#include <QtGui/QTextFrame>
#include <QtGui/QTextImageFormat>
#include <QtGui/QTextTable>
#include <QtGui/QTextTableCell>

#include <QtCore/QHash>
#include <algorithm>
#include <QtCore/QList>
#include <QtCore/QSet>
#include <QtCore/QRegularExpression>

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

// The theme, as the renderer sees it.
struct Palette {
    QHash<QString, QColor> colors;

    static Palette parse(const QStringList &entries) {
        Palette palette;
        for (const QString &entry : entries) {
            const int eq = entry.indexOf(QLatin1Char('='));
            if (eq <= 0)
                continue;
            const QColor color(entry.mid(eq + 1));
            if (color.isValid())
                palette.colors.insert(entry.left(eq), color);
        }
        return palette;
    }

    bool has(const char *role) const { return colors.contains(QLatin1String(role)); }
    QColor get(const char *role) const { return colors.value(QLatin1String(role)); }
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

// Vertical room around a heading: more above than below, so it belongs to
// what follows.
void headingMargins(int level, qreal &top, qreal &bottom) {
    top = level == 1 ? 16 : (level == 2 ? 14 : 10);
    bottom = level <= 2 ? 6 : 4;
}

void mergeChars(QTextDocument &doc, const Range &range, const QTextCharFormat &format) {
    if (range.length <= 0)
        return;
    QTextCursor cursor(&doc);
    cursor.setPosition(range.position);
    cursor.setPosition(range.position + range.length, QTextCursor::KeepAnchor);
    cursor.mergeCharFormat(format);
}

// A run of consecutive blocks, by position of the first and last block.
struct BlockRun {
    int first;
    int last;
};

// A borderless table. Its own background (unlike a cell's) also fills the
// padding around the cells, which is what the boxes below rely on.
QTextTableFormat flatTable(int padding, const QColor &background = QColor()) {
    QTextTableFormat format;
    if (background.isValid())
        format.setBackground(background);
    format.setBorder(0);
    format.setCellSpacing(0);
    format.setCellPadding(padding);
    // A little air above and below, so a box does not touch its neighbours.
    format.setTopMargin(6);
    format.setBottomMargin(8);
    format.setLeftMargin(0);
    format.setRightMargin(0);
    format.setWidth(QTextLength(QTextLength::PercentageLength, 100));
    return format;
}


bool isCodeBlock(const QTextBlock &block) {
    const QTextBlockFormat format = block.blockFormat();
    return format.hasProperty(QTextFormat::BlockCodeFence) ||
           format.hasProperty(QTextFormat::BlockCodeLanguage);
}

// Qt's Markdown importer turns a hard line break (two trailing spaces, or a
// backslash) into a new block with exactly the format of a paragraph, so the
// document itself cannot tell "one⏎two" from "one¶two". The importer is the
// only parser, so ask it: parse a copy of the source with the hard-break
// markers made soft (a copy, used only here; the note is never touched) and
// see which blocks of the real document merge into one. Those are the blocks
// that continue the previous line. Returns the block numbers of the
// continuations; empty if the two documents do not line up (then nothing is
// treated as a hard break, which only means looser spacing).
QSet<int> hardBreakContinuations(const QString &markdown, const QTextDocument &doc,
                                 QTextDocument::MarkdownFeatures features) {
    static const QRegularExpression trailingSpaces(QStringLiteral("[ ]{2,}(\\r?\\n)"));
    static const QRegularExpression backslash(QStringLiteral("\\\\(\\r?\\n)"));
    QString soft = markdown;
    soft.replace(trailingSpaces, QStringLiteral(" \\1"));
    soft.replace(backslash, QStringLiteral("\\1"));
    if (soft == markdown)
        return {};

    QTextDocument other;
    other.setMarkdown(soft, features);
    QSet<int> continuations;
    QTextBlock real = doc.begin();
    for (QTextBlock merged = other.begin(); merged.isValid(); merged = merged.next()) {
        if (!real.isValid())
            return {};
        if (isCodeBlock(merged)) {
            // One block each way, whatever trailing spaces its lines have.
            real = real.next();
            continue;
        }
        QString joined = real.text();
        real = real.next();
        while (joined != merged.text() && real.isValid() && !isCodeBlock(real)) {
            joined += QLatin1Char(' ') + real.text();
            continuations.insert(real.blockNumber());
            real = real.next();
        }
        if (joined != merged.text())
            return {};
    }
    return real.isValid() ? QSet<int>() : continuations;
}

enum class Box { Quote, Code };

// Replaces a run of blocks with the same content inside a table, because a
// plain paragraph cannot have a rule or a panel. The table is built from cell
// backgrounds only (Qt's viewer does not draw cell borders for a borderless
// table): a quote is a thin coloured cell beside its text, and a code block is
// a raised cell inside a one-pixel border-coloured cell.
// Back to front, so earlier positions stay valid.
void boxRun(QTextDocument &doc, const BlockRun &run, Box kind, const QColor &rule,
            const QColor &border, const QColor &raised) {
    const QTextBlock lastBlock = doc.findBlock(run.last);
    QTextCursor cursor(&doc);
    cursor.setPosition(run.first);
    cursor.setPosition(lastBlock.position() + lastBlock.length() - 1, QTextCursor::KeepAnchor);
    const QTextDocumentFragment fragment = cursor.selection();
    cursor.removeSelectedText();

    // One table, one cell: a quote is outlined in the secondary accent, a
    // code block is a raised panel with a border-colour outline. (Qt's rich
    // text cannot draw a rule on one side only.)
    QTextTableFormat format = flatTable(8, kind == Box::Code ? raised : QColor());
    // A table's background also fills its margins, so a panel has none; the
    // neighbouring paragraphs provide the spacing.
    if (kind == Box::Code) {
        format.setTopMargin(0);
        format.setBottomMargin(0);
    }
    format.setBorder(1);
    format.setBorderStyle(QTextFrameFormat::BorderStyle_Solid);
    format.setBorderBrush(kind == Box::Code ? border : rule);
    QTextTable *table = cursor.insertTable(1, 1, format);
    QTextCursor inside = table->cellAt(0, 0).firstCursorPosition();
    inside.insertFragment(fragment);

    // The text sits flush in its cell: the box itself provides the indentation.
    // The first and last paragraphs carry the vertical air, which the cell
    // background does paint.
    const qreal air = 0;
    const int length = fragment.toPlainText().size();
    QTextBlock first = inside.block();
    QTextBlock last = first;
    for (QTextBlock b = first; b.isValid() && b.position() <= first.position() + length; b = b.next())
        last = b;
    for (QTextBlock b = first; b.isValid(); b = b.next()) {
        QTextCursor one(b);
        QTextBlockFormat flush = b.blockFormat();
        flush.setLeftMargin(0);
        flush.setRightMargin(0);
        flush.setIndent(0);
        flush.setTopMargin(b == first ? air : 0);
        flush.setBottomMargin(b == last ? air : 0);
        one.setBlockFormat(flush);
        if (b == last)
            break;
    }
}

} // namespace

QString omatree_render_markdown(const QString &markdown, const QStringList &colorEntries) {
    // A document that is never shown is never laid out, so Qt never gets as
    // far as loading a resource for it. Raw HTML is switched off here too.
    QTextDocument doc;
    doc.setDefaultFont(QGuiApplication::font());
    QTextDocument::MarkdownFeatures features(QTextDocument::MarkdownDialectGitHub);
    features |= QTextDocument::MarkdownNoHTML;
    doc.setMarkdown(markdown, features);
    const QSet<int> continuations = hardBreakContinuations(markdown, doc, features);

    const Palette palette = Palette::parse(colorEntries);
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

    // 2. Look at what Markdown produced. Collected first, because editing
    //    while iterating would invalidate the iteration.
    QList<Range> links;
    QList<Range> codeSpans;
    QList<Range> quotes;
    QList<QPair<Range, int>> headings;
    QList<Range> listItems;
    QList<Range> checked;
    QList<Range> rules;
    QList<BlockRun> quoteRuns;
    QList<BlockRun> codeRuns;
    bool inQuote = false;
    bool inCode = false;
    for (QTextBlock block = doc.begin(); block.isValid(); block = block.next()) {
        const QTextBlockFormat blockFormat = block.blockFormat();
        const Range whole{block.position(), block.length() - 1};
        const bool quote = blockFormat.intProperty(QTextFormat::BlockQuoteLevel) > 0;
        const bool code = blockFormat.hasProperty(QTextFormat::BlockCodeFence) ||
                          blockFormat.hasProperty(QTextFormat::BlockCodeLanguage);
        if (quote) {
            quotes.append(whole);
            if (inQuote)
                quoteRuns.last().last = block.position();
            else
                quoteRuns.append({block.position(), block.position()});
        }
        inQuote = quote;
        if (code) {
            if (inCode)
                codeRuns.last().last = block.position();
            else
                codeRuns.append({block.position(), block.position()});
        }
        inCode = code;
        if (blockFormat.headingLevel() > 0)
            headings.append({whole, blockFormat.headingLevel()});
        if (block.textList() != nullptr)
            listItems.append(whole);
        if (blockFormat.marker() == QTextBlockFormat::MarkerType::Checked)
            checked.append(whole);
        if (blockFormat.hasProperty(QTextFormat::BlockTrailingHorizontalRulerWidth))
            rules.append(whole);
        for (QTextBlock::iterator it = block.begin(); !it.atEnd(); ++it) {
            const QTextFragment fragment = it.fragment();
            if (!fragment.isValid())
                continue;
            const Range range{fragment.position(), fragment.length()};
            if (fragment.charFormat().isAnchor())
                links.append(range);
            else if (fragment.charFormat().fontFixedPitch() && !code)
                codeSpans.append(range);
        }
    }

    const qreal gap = qRound(QFontMetricsF(doc.defaultFont()).height() * 0.7);
    // 2b. Paragraph spacing. Qt's own gap between paragraphs is a few pixels;
    //     here it is a fixed fraction of the text's line height, so it follows
    //     the font. A block that continues the previous line (a hard break) has
    //     none, so only real paragraphs are set apart. Blocks that are
    //     something else (headings, list items, quotes, code) keep their own
    //     margins; a paragraph next to them still leaves its own gap, which
    //     is what keeps prose from touching a code panel or a list.
    {
        for (QTextBlock block = doc.begin(); block.isValid(); block = block.next()) {
            const QTextBlockFormat format = block.blockFormat();
            const bool paragraph = format.headingLevel() == 0 && block.textList() == nullptr &&
                                   format.intProperty(QTextFormat::BlockQuoteLevel) == 0 &&
                                   !isCodeBlock(block) &&
                                   !format.hasProperty(QTextFormat::BlockTrailingHorizontalRulerWidth);
            if (!paragraph || gap <= 0)
                continue;
            QTextBlockFormat spacing;
            spacing.setTopMargin(continuations.contains(block.blockNumber()) ? 0 : gap);
            const QTextBlock next = block.next();
            spacing.setBottomMargin(next.isValid() && continuations.contains(next.blockNumber()) ? 0 : gap);
            QTextCursor cursor(block);
            cursor.mergeBlockFormat(spacing);
        }
    }

    // 3. Text colours and sizes, from the theme.
    if (palette.has("muted")) {
        QTextCharFormat mutedFormat;
        // Halfway between the text and the muted colour: set apart from the
        // body, but still comfortably readable on any theme.
        QColor quoteColor = palette.get("muted");
        if (palette.has("foreground")) {
            const QColor text = palette.get("foreground");
            quoteColor = QColor::fromRgbF((quoteColor.redF() + text.redF()) / 2,
                                          (quoteColor.greenF() + text.greenF()) / 2,
                                          (quoteColor.blueF() + text.blueF()) / 2);
        }
        mutedFormat.setForeground(quoteColor);
        for (const Range &range : quotes)
            mergeChars(doc, range, mutedFormat);
    }
    if (palette.has("accent")) {
        QTextCharFormat linkFormat;
        linkFormat.setForeground(palette.get("accent"));
        for (const Range &range : links)
            mergeChars(doc, range, linkFormat);
    }
    {
        QTextCharFormat codeFormat;
        if (palette.has("raised"))
            codeFormat.setBackground(palette.get("raised"));
        if (palette.has("warning"))
            codeFormat.setForeground(palette.get("warning"));
        for (const Range &range : codeSpans)
            mergeChars(doc, range, codeFormat);
    }
    for (const auto &heading : headings) {
        const int level = heading.second;
        QTextCharFormat headingFormat;
        headingFormat.setFontWeight(QFont::DemiBold);
        if (baseSize > 0)
            headingFormat.setFontPointSize(baseSize * headingScale(level));
        // One colour for the top level, a second for the next, and
        // typography alone below that.
        if (level == 1 && palette.has("accent"))
            headingFormat.setForeground(palette.get("accent"));
        else if (level == 2 && palette.has("accentSecondary"))
        {
            // Pulled a quarter of the way to the text colour, so a pale
            // secondary accent still reads on a light page.
            QColor second = palette.get("accentSecondary");
            if (palette.has("foreground")) {
                const QColor text = palette.get("foreground");
                second = QColor::fromRgbF(second.redF() * 0.75 + text.redF() * 0.25,
                                          second.greenF() * 0.75 + text.greenF() * 0.25,
                                          second.blueF() * 0.75 + text.blueF() * 0.25);
            }
            headingFormat.setForeground(second);
        }
        mergeChars(doc, heading.first, headingFormat);

        QTextCursor cursor(&doc);
        cursor.setPosition(heading.first.position);
        QTextBlockFormat spacing;
        qreal top = 0;
        qreal bottom = 0;
        headingMargins(level, top, bottom);
        spacing.setTopMargin(top);
        spacing.setBottomMargin(bottom);
        cursor.mergeBlockFormat(spacing);
    }

    // Markers of list items and checked tasks take their colour from the
    // block's own format, leaving the item text alone.
    auto colourMarkers = [&doc](const QList<Range> &blocks, const QColor &colour,
                                const QColor &text) {
        for (const Range &range : blocks) {
            QTextCursor cursor(&doc);
            cursor.setPosition(range.position);
            QTextCharFormat marker;
            marker.setForeground(colour);
            cursor.mergeBlockCharFormat(marker);
            if (text.isValid()) {
                QTextCharFormat body;
                body.setForeground(text);
                mergeChars(doc, range, body);
            }
        }
    };
    if (palette.has("accent"))
        colourMarkers(listItems, palette.get("accent"), palette.get("foreground"));
    if (palette.has("positive"))
        colourMarkers(checked, palette.get("positive"), palette.get("foreground"));

    // 4. Tables Markdown produced: theme borders, a raised header row.
    for (QTextFrame *frame : doc.rootFrame()->childFrames()) {
        QTextTable *table = qobject_cast<QTextTable *>(frame);
        if (!table)
            continue;
        QTextTableFormat format = table->format();
        format.setBorder(1);
        format.setCellPadding(5);
        format.setCellSpacing(0);
        format.setBorderStyle(QTextFrameFormat::BorderStyle_Solid);
        if (palette.has("border"))
            format.setBorderBrush(palette.get("border"));
        table->setFormat(format);
        // The header row is set apart by weight alone.
        QTextCharFormat headerText;
        headerText.setFontWeight(QFont::DemiBold);
        for (int column = 0; column < table->columns(); ++column) {
            QTextTableCell cell = table->cellAt(0, column);
            QTextCursor first = cell.firstCursorPosition();
            QTextCursor last = cell.lastCursorPosition();
            first.setPosition(last.position(), QTextCursor::KeepAnchor);
            first.mergeCharFormat(headerText);
        }
    }

    // 5. Boxes and rules, built last, from the end of the document backwards
    //    so that the positions collected above stay valid.
    struct Edit {
        int position;
        int kind; // 0 quote run, 1 code run, 2 rule
        BlockRun run;
    };
    QList<Edit> edits;
    for (const BlockRun &run : quoteRuns)
        edits.append({run.first, 0, run});
    for (const BlockRun &run : codeRuns)
        edits.append({run.first, 1, run});
    std::sort(edits.begin(), edits.end(),
              [](const Edit &a, const Edit &b) { return a.position > b.position; });
    for (const Edit &edit : edits) {
        if (edit.kind == 0 && palette.has("accentSecondary"))
            boxRun(doc, edit.run, Box::Quote, palette.get("accentSecondary"), QColor(), QColor());
        else if (edit.kind == 1 && palette.has("raised") && palette.has("border"))
            boxRun(doc, edit.run, Box::Code, QColor(), palette.get("border"), palette.get("raised"));
    }

    QString html = doc.toHtml();

    // 6. Tidying of Qt's own output. The paragraphs the box edits leave empty
    //    are dropped, and the stock horizontal rule (always drawn in the text
    //    colour) becomes a one-pixel row in the border colour.
    static const QRegularExpression emptyBlock(
        QStringLiteral("<(p|pre) style=\"-qt-paragraph-type:empty;[^\"]*\">(<br />)?</\\1>\n?"));
    // Qt puts an empty paragraph before and after a table nested in a cell;
    // they become one pixel high, so the boxes have no stray bands.
    html.replace(emptyBlock,
                 QStringLiteral("<p style=\"margin-top:0px; margin-bottom:0px; margin-left:0px; margin-right:0px; line-height:1px;\"><span "
                                "style=\"font-size:1px;\">&nbsp;</span></p>\n"));
    if (palette.has("raised")) {
        // A collapsed border around a filled panel is drawn twice by the view.
        const QRegularExpression panel(
            QStringLiteral("<table[^>]*bgcolor=\"%1\"[^>]*>.*?</table>")
                .arg(palette.get("raised").name()),
            QRegularExpression::DotMatchesEverythingOption);
        // The view gives a paragraph no space above it right after a table,
        // and a panel's own margin would be painted in its background, so the
        // gap below a code panel is a spacer line of the same height.
        const QString spacer = QStringLiteral(
            "<p style=\"margin-top:0px; margin-bottom:0px; margin-left:0px; margin-right:0px; "
            "line-height:%1px;\"><span style=\"font-size:1px;\">&nbsp;</span></p>")
                                   .arg(gap);
        QString out;
        qsizetype last = 0;
        for (auto it = panel.globalMatch(html); it.hasNext();) {
            const QRegularExpressionMatch m = it.next();
            out += html.mid(last, m.capturedStart() - last);
            out += m.captured().remove(QStringLiteral("border-collapse:collapse;")) + spacer;
            last = m.capturedEnd();
        }
        html = out + html.mid(last);
    }
    // Space above and below a list, which the view takes from the list's own
    // margins (Qt writes them as zero) and not from its items'.
    html.replace(QStringLiteral("<ul style=\"margin-top: 0px; margin-bottom: 0px;"),
                 QStringLiteral("<ul style=\"margin-top: %1px; margin-bottom: %1px;").arg(gap));
    html.replace(QStringLiteral("<ol style=\"margin-top: 0px; margin-bottom: 0px;"),
                 QStringLiteral("<ol style=\"margin-top: %1px; margin-bottom: %1px;").arg(gap));
    if (palette.has("border")) {
        const QString rule = QStringLiteral(
            "<table width=\"100%\" border=\"0\" cellspacing=\"0\" cellpadding=\"0\" "
            "style=\"margin-top:12px; margin-bottom:12px;\"><tr><td bgcolor=\"%1\">"
            "<p style=\"margin-top:0px; margin-bottom:0px; margin-left:0px; margin-right:0px; line-height:1px;\"><span style=\"font-size:1px;\">&nbsp;</span>"
            "</p></td></tr></table>");
        html.replace(QStringLiteral("<hr />"), rule.arg(palette.get("border").name()));
    }
    return html;
}
