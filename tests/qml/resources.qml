import QtQuick
import org.omatree

// Scenario 8: Preview never loads a remote or local resource. The Rust side
// listens on 127.0.0.1 and checks afterwards that nothing connected.
Base {
    function run() {
        const remote = "http://127.0.0.1:" + params.port;
        const body = "![remote](" + remote + "/remote.png)\n\n" + "![local](" + url(params.local) + ")\n\n" + "<img src=\"" + remote + "/raw.png\" onerror=\"boom()\">\n\n" + "<script>boom()</script>\n\n" + "[a link](" + remote + "/link)\n\n" + "text after\n";
        mk(null, "Hostile", body);
        check("save", saveAs(tmp("resources.omatree")) === "");
        select(find("Hostile"));

        tree.forceActiveFocus();
        shortcut("Ctrl+E");
        until("Preview is on", () => editorPane.previewing && previewField() !== null);
        until("it rendered", () => previewField().getText(0, previewField().length).indexOf("text after") >= 0);

        const html = previewField().text;
        check("no image element in the rendered output", html.indexOf("<img") < 0);
        check("no script element", html.toLowerCase().indexOf("<script") < 0);
        check("the images became their alt text", previewField().getText(0, previewField().length).indexOf("[remote]") >= 0);

        // Give any (wrongly started) load time to reach the listener.
        wait(800);
        check("the source is unchanged", notebook.body(find("Hostile")) === body);
        check("previewing changed nothing else", !notebook.dirty);

        // A link is only ever opened by a click, and only http(s)/mailto.
        check("javascript: links are refused", !Markdown.isSafeLink("javascript:boom()"));
        check("file: links are refused", !Markdown.isSafeLink(url(params.local)));
        check("https links are allowed", Markdown.isSafeLink("https://example.invalid/"));
    }
}
