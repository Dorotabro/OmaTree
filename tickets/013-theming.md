# Ticket 013 — Semantic theming and Omarchy integration

## Goal

Give OmaTree a deliberate visual identity on every platform while making it
follow the active Omarchy theme automatically when running under Omarchy.

Introduce one semantic theme layer used by all OmaTree-owned QML UI.

Providers:

1. Omarchy theme, when a valid active Omarchy theme is detected
2. OmaTree Dark, when the system requests a dark color scheme
3. OmaTree Light, when the system requests a light color scheme

There is no manual theme picker in this ticket.

The UI should feel calm, minimal and intentional.

## Architecture

Introduce one theme/palette abstraction.

The rest of the QML application must consume semantic roles, not literal
color values.

Required semantic roles:

- background
- surface
- surfaceRaised
- foreground
- mutedForeground
- accent
- selection
- selectionForeground
- border
- danger
- dangerForeground

Names may vary slightly if there is a compelling implementation reason, but
keep the set small and semantic.

There must be one authoritative palette instance for the running application.

Do not duplicate palette constants across QML files.

## Do not use Qt Labs StyleKit

Do not adopt Qt Labs StyleKit for this ticket.

OmaTree currently targets Qt 6.11 and StyleKit is a Qt Labs / preview API.

Use stable Qt/QML mechanisms and the existing CXX-Qt architecture instead.

## Built-in OmaTree palette

These are OmaTree's own built-in colors.

They are intentionally cool, restrained and slate-like, but are not a direct
copy of a third-party theme.

### OmaTree Dark

    background          #252A33
    surface             #2D3440
    surfaceRaised       #37404D
    foreground          #E6EAF0
    mutedForeground     #AAB4C0
    accent              #82B8C8
    selection           #496E88
    selectionForeground #F4F7FA
    border              #46515F
    danger              #BE6670
    dangerForeground    #FFFFFF

### OmaTree Light

    background          #F2F4F7
    surface             #E7EBF0
    surfaceRaised       #DCE2E8
    foreground          #29313B
    mutedForeground     #65717E
    accent              #557F9D
    selection           #B8D3E0
    selectionForeground #20303D
    border              #C8D0D9
    danger              #B4525C
    dangerForeground    #FFFFFF

Keep these values in one place.

Do not scatter hex literals throughout the UI.

## Generic platform behaviour

Outside Omarchy, follow the operating system's current light/dark preference.

Use Qt's system color-scheme information.

When the OS changes between light and dark while OmaTree is running:

- update to the corresponding built-in OmaTree palette
- update the UI without restart
- do not affect document state or selection

If Qt cannot determine a system color scheme, choose a deterministic fallback.

Prefer OmaTree Dark as the fallback.

Do not add a user preference yet.

## Platform independence

The theme architecture must compile and operate on:

- Linux
- Windows
- macOS

Omarchy-specific detection must remain optional.

On non-Linux platforms, simply use the built-in system-following behaviour.

Do not make the application depend on Omarchy being installed.

## Omarchy detection

On Linux, detect the active Omarchy theme from the canonical active state.

Prefer:

    $XDG_STATE_HOME/omarchy/current/theme/colors.toml

when XDG_STATE_HOME is set.

Otherwise use:

    $HOME/.local/state/omarchy/current/theme/colors.toml

If there is no readable valid Omarchy palette:

- do not treat that as an application error
- fall back to OmaTree Dark/Light based on the system scheme

Do not show an error dialog merely because Omarchy theming is unavailable.

A diagnostic warning to stderr is acceptable for malformed theme data.

## Omarchy parsing

Parse the active `colors.toml`.

Use a real TOML parser rather than ad-hoc string parsing.

Adding one small established TOML parsing dependency is acceptable.

Do not add an Omarchy-specific theme framework/library solely for this ticket.

The integration should remain small and understandable inside OmaTree.

## Omarchy palette mapping

Map Omarchy semantic colors to OmaTree semantic roles.

Prefer these mappings:

    OmaTree role          Omarchy source

    background            background
    surface               dark_background
    surfaceRaised         lighter_background
    foreground            foreground
    mutedForeground       dark_foreground, otherwise muted
    accent                accent
    selection             selection
    selectionForeground   bright_foreground, otherwise foreground
    border                muted
    danger                red
    dangerForeground      bright_foreground, otherwise foreground

Use Omarchy `mode` where available to identify dark/light context.

Do not hard-code individual Omarchy theme names.

Tokyo Night, Catppuccin, Matte Black, custom user themes, etc. should all work
through the same semantic mapping.

## Missing Omarchy keys

Do not crash if optional palette keys are absent.

Use sensible fallback chains.

At minimum, a usable Omarchy palette requires valid:

- background
- foreground

For missing optional roles:

- derive/fall back from another appropriate Omarchy semantic value
- then fall back to the corresponding built-in OmaTree palette role

Do not replace an otherwise valid Omarchy theme completely merely because one
optional color is absent.

Invalid color strings must be handled safely.

## Omarchy live theme changes

When the user changes the Omarchy theme while OmaTree is running, OmaTree
should update automatically without restart.

Omarchy replaces/swaps its active theme directory during theme activation, so
do not rely on watching only one file inode forever.

Watch an appropriate surviving parent/state directory or use another clean
Qt-native mechanism that survives replacement of the theme directory.

Debounce the reload if one theme change produces several filesystem events.

Do not install an Omarchy hook or modify the user's Omarchy configuration.

Prefer existing Qt facilities over adding a background runtime.

Do not add Tokio or an async runtime.

If a filesystem watcher is disproportionately awkward through CXX-Qt, use the
smallest reliable alternative and explain it in the completion report.

## Theme reload safety

A theme update affects presentation only.

It must not:

- modify Notebook data
- change dirty state
- trigger autosave
- change selection
- create checkpoints
- change recovery state

If the newly selected Omarchy theme is temporarily unavailable during its
atomic replacement:

- do not flash an error dialog
- do not crash
- tolerate the short transition
- apply the new palette once it becomes readable

## QML conversion

Replace application-owned hard-coded/default colors with semantic theme roles.

At minimum theme:

### Main window

- window/editor background
- separators
- split-view handle

### Tree pane

- sidebar background
- normal rows
- hover rows
- selected row
- selected text
- ordinary text
- muted/empty-state text
- drag/drop indicator
- invalid drop indicator
- focus indication

### Editor pane

- editor background
- title text
- body text
- text selection
- placeholder/muted text
- focus indication

### Controls

- New note
- New child
- File button/menu
- Recovery button/entry
- Delete
- confirmation/error dialogs owned by OmaTree

Dangerous actions should use `danger` sparingly.

Do not paint the entire UI red merely because a Delete button exists.

## Native dialogs

Native platform file dialogs should remain native.

Do not attempt to recolor an OS-native file picker.

OmaTree-owned Qt Quick dialogs may use the semantic palette.

## Visual direction

The result should feel:

- minimal
- quiet
- compact
- clean
- deliberate

Avoid:

- gradients
- glossy effects
- excessive borders
- giant rounded cards
- oversized controls
- decorative shadows
- excessive use of the accent color

Accent should primarily communicate:

- selection
- focus
- active drag/drop target
- small interactive emphasis

The tree and editor should remain visually dominant.

## Layout polish

Small spacing/padding adjustments are allowed where necessary to make the new
theme feel coherent.

Do not redesign the application structure.

Preserve:

- tree on the left
- editor on the right
- current footer controls
- current dialogs/workflows

This is a theming ticket, not a UI rewrite.

## Fonts

Do not bundle or hard-code a custom font.

Use the platform/Qt application font.

This keeps Windows, macOS, generic Linux and Omarchy integration natural and
avoids font distribution/licensing concerns.

Typography changes may use:

- weight
- size relative to the application default

but not a bundled typeface.

## Theme state and persistence

Theme choice is environmental in this ticket.

Do not store theme preferences in `.omatree` notebook files.

Do not change the SQLite schema.

Do not put visual preferences inside Document or Notebook.

## Tests — built-in palette

Cover at least:

1. Dark palette contains every semantic role
2. Light palette contains every semantic role
3. palette values parse as valid colors
4. dark and light palettes have distinct backgrounds
5. danger and accent roles are defined independently

## Tests — Omarchy parser

Cover at least:

6. valid dark Omarchy colors.toml maps correctly
7. valid light Omarchy colors.toml maps correctly
8. optional missing values use documented fallbacks
9. missing background rejects Omarchy palette
10. missing foreground rejects Omarchy palette
11. malformed color values fail/fallback safely
12. unknown extra TOML keys are ignored
13. custom theme names do not affect parsing
14. no hard-coded theme-name behaviour exists

## Tests — provider selection

Cover at least:

15. valid Omarchy palette wins over generic system palette
16. missing Omarchy state selects built-in system mode
17. invalid Omarchy palette selects built-in system mode
18. system dark selects OmaTree Dark
19. system light selects OmaTree Light
20. unknown system mode selects deterministic fallback

## Tests — live reload

Where practical verify:

21. changing Omarchy colors reloads the semantic palette
22. replacing the active theme directory is detected
23. burst filesystem events produce one effective palette update
24. temporarily missing theme during replacement does not crash
25. returning valid theme applies successfully
26. theme reload changes no Document dirty state

## Runtime verification

Exercise at least:

- built-in dark appearance
- built-in light appearance
- system dark/light switch
- Omarchy theme detection
- switching between two Omarchy themes while OmaTree remains open
- tree selection and hover
- drag/drop valid and invalid feedback
- editor selection
- Delete/recovery dialogs
- native file dialog remains functional
- autosave still works
- Recovery still works
- no QML binding loops or runtime warnings

Take screenshots of at least:

- OmaTree Dark
- OmaTree Light
- one active Omarchy theme

for manual visual inspection.

The screenshots do not need to be committed.

## Verification

Run:

- `cargo fmt --check`
- `cargo clippy --all-targets`
- `cargo test`
- `cargo build`

Perform a clean offscreen launch.

Also run the final build on a real graphical desktop for visual verification,
because visual quality cannot be proven offscreen.

## Out of scope

Do not:

- add a theme picker
- persist theme preference
- ship Nord/Catppuccin/Gruvbox themes
- copy third-party theme assets
- use Qt Labs StyleKit
- bundle fonts
- redesign the application layout
- implement search
- implement Markdown rendering
- change SQLite schema
- add editor settings
- implement custom icon packs
- implement animations beyond subtle existing interaction feedback
- perform unrelated cleanup