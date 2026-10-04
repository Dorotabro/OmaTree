pragma Singleton

import QtQuick

// Shared sizes and shapes for OmaTree's own UI: numbers only. Colours belong
// to Theme and are never kept here.
QtObject {
    // Spacing scale.
    readonly property int tiny: 2
    readonly property int small: 4
    readonly property int medium: 8
    readonly property int large: 14

    // The header band of both panes (the search field on the left, the note
    // title on the right), hairline included, so their separators line up.
    readonly property int headerHeight: 44

    // Heights. Rows are compact, but never smaller than a comfortable target.
    readonly property int rowHeight: 26
    readonly property int controlHeight: 30

    // Geometry: flat, with hairlines and barely rounded corners.
    readonly property int hairline: 1
    readonly property int radius: 2
    // Thickness of the selected-row and active-tab indicators.
    readonly property int bar: 2

    // Width of one tree level, which is also the disclosure control's size.
    readonly property int indent: 16
    // Left and right margin of note text, in the editor and the preview.
    readonly property int editorPadding: 22

    // The one animation: a hover colour change you barely notice.
    readonly property int hoverDuration: 80

    // Command-like details (the strip, shortcut hints, mode switch) use the
    // platform's fixed-width font, asked for by generic family and never by
    // name, at a small size.
    readonly property string monoFamily: "monospace"
    readonly property int commandPixelSize: 12

    // Bookkeeping for hover, not a size: changes whenever a menu or dialog
    // opens or closes. See PointerHover.
    property int popupSerial: 0
}
