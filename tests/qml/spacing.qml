import QtQuick
import org.omatree

// Markdown Preview spacing: a blank line is a visible paragraph gap, a hard
// line break is not, and blocks do not touch. Measured in the same run, in
// units of the preview's own line height, never in absolute pixels.
Base {
    property var note: null

    // Shows `body` in Preview and returns what it looks like.
    function show(body, mark) {
        notebook.setBody(note, body);
        until("preview shows " + mark, () => editorPane.previewSource === body && previewField().getText(0, previewField().length).indexOf(mark) >= 0);
        wait(60);
        const p = previewField();
        const text = p.getText(0, p.length);
        return {
            "height": p.contentHeight,
            "lineHeight": p.positionToRectangle(0).height,
            "y": mark => p.positionToRectangle(text.indexOf(mark)).y,
            "html": Markdown.render(body, editorPane.previewColors)
        };
    }

    // The margin (px) of the first paragraph after / before `marker` in html.
    function marginAfter(html, marker, which) {
        const at = html.indexOf(marker);
        const match = /<p style="[^"]*margin-top:(\d+)px; margin-bottom:(\d+)px/.exec(html.slice(at));
        return match ? parseInt(match[which === "top" ? 1 : 2]) : -1;
    }
    function lastMarginBefore(html, marker) {
        const before = html.slice(0, html.indexOf(marker));
        const pattern = /<p style="[^"]*margin-top:(\d+)px; margin-bottom:(\d+)px[^>]*>(?!<span style="font-size:1px)/g;
        let last = -1;
        for (let found = pattern.exec(before); found; found = pattern.exec(before))
            last = parseInt(found[2]);
        return last;
    }

    function run() {
        note = mk(null, "Note", "x");
        check("save", saveAs(tmp("spacing.omatree")) === "");
        select(find("Note"));
        tree.forceActiveFocus();
        shortcut("Ctrl+E");
        until("Preview is on", () => editorPane.previewing && previewField() !== null);
        const checkpoints = notebook.checkpointCount();

        const para = "one\n\ntwo";
        const spaces = "one  \ntwo";
        const backslash = "one\\\ntwo";
        const soft = "one\ntwo";
        const many = "one\n\n\n\n\ntwo";

        const p = show(para, "two");
        const lh = p.lineHeight;
        check("a line height was measured", lh > 4);
        const gapPara = p.y("two") - p.y("one") - lh;
        check("a blank line gives a clearly visible gap", gapPara >= 0.3 * lh && gapPara <= 1.5 * lh);

        const s = show(spaces, "two");
        const gapSpaces = s.y("two") - s.y("one") - s.lineHeight;
        check("two trailing spaces: two consecutive lines", gapSpaces >= -1 && gapSpaces <= 2);
        check("a paragraph is measurably taller than a hard break", p.height - s.height >= 0.3 * lh);

        const b = show(backslash, "two");
        const gapBackslash = b.y("two") - b.y("one") - b.lineHeight;
        check("backslash hard break: two consecutive lines", gapBackslash >= -1 && gapBackslash <= 2);
        check("same height as the two-space break", Math.abs(b.height - s.height) <= 2);

        const o = show(soft, "one");
        check("a single newline stays one paragraph (one line)", o.height < s.height && o.html.indexOf("one two") >= 0);

        const m = show(many, "two");
        check("several blank lines are no bigger than one", Math.abs(m.height - p.height) <= 2);

        // Code next to prose.
        const afterCode = show("lead\n\n```rust\nfn main() {}\n```\n\ntrail\n", "trail");
        // Below a code panel the gap is a spacer line (the view ignores the
        // margin of a paragraph that follows a table).
        const spacer = /<\/table>\s*<p style="[^"]*line-height:(\d+)px/.exec(afterCode.html);
        const gapOut = spacer ? parseInt(spacer[1]) : -1;
        const gapIn = lastMarginBefore(afterCode.html, "<table");
        check("prose after a code panel has a gap", gapOut >= 0.4 * lh);
        check("prose before a code panel has a gap", gapIn >= 0.4 * lh);
        check("trail is below the code, not on it", afterCode.y("trail") > afterCode.y("fn main") + lh);

        // Lists: space above and below, through the list's own margins.
        const list = show("before\n\n- a\n- b\n\nafter\n", "after");
        const listMargins = /<ul style="margin-top: (\d+)px; margin-bottom: (\d+)px/.exec(list.html);
        check("a list has space above and below", listMargins !== null && parseInt(listMargins[1]) >= 0.4 * lh && parseInt(listMargins[2]) >= 0.4 * lh);

        // Nothing above touched the note, and Preview is a view.
        shortcut("Ctrl+E");
        until("back to Edit", () => !editorPane.previewing);
        check("the raw source is exactly what was set", notebook.body(note) === "before\n\n- a\n- b\n\nafter\n");
        check("no checkpoint for previewing", notebook.checkpointCount() === checkpoints);
        for (const source of [para, spaces, backslash, soft, many]) {
            bodyField().text = source;
            shortcut("Ctrl+E");
            until("preview on", () => editorPane.previewing);
            shortcut("Ctrl+E");
            until("preview off", () => !editorPane.previewing);
            check("source byte-identical after Preview: " + JSON.stringify(source), notebook.body(note) === source && bodyField().text === source);
        }
    }
}
