# Screenshots

Images used by the root `README.md`. Everything lives in the repository; the README never links to an
external image host. Never add an image that is not a real capture of OmaTree.

## Files present

| File | Shows | Size |
|---|---|---|
| `omatree-dark-kanagawa-editor.png` | Dark theme (Kanagawa palette): tree with colour tags, table, emoji, slash menu, header colour picker | 941 × 508 |
| `omatree-light-editor.png` | Light theme: tree, bold/italic text, emoji, header colour picker with a tag selected | 941 × 508 |
| `omatree-dark-notebook-menu.png` | Dark theme with the notebook menu open | 941 × 508 |

The three files were supplied by the maintainer and renamed here (they were `screenshot01`–`03.png`).
Which operating system and which desktop each was captured on is **not recorded in the files**; the README
captions therefore name only the theme. If `omatree-dark-kanagawa-editor.png` was captured on Omarchy, the
README can call it "OmaTree on Omarchy"; that is the maintainer's to confirm.

## Not yet supplied

No capture exists for these, so the README has **no reference to them**. When a file is added, add the
reference to the README in the same change.

| Wanted file name | What it should show |
|---|---|
| `omatree-omarchy-hero.png` | The primary screenshot, on Omarchy (Hyprland), a saved notebook, theme switching visible if possible |
| `omatree-windows.png` | The application on Windows 10/11 (only once a Windows build of the egui version exists) |
| `omatree-macos.png` | The application on macOS (only once a macOS build of the egui version is distributed) |

## Image specification

- **Format:** PNG, 8-bit, no transparency needed. Strip metadata. No animated GIF in the README.
- **Dimensions:** window contents only, with no desktop, bar or other windows around it. Capture at 1× or 2× scale and
  scale the final image to **1600 × 900 px** (16:9) for the hero shot and **1280 × 720 px** for the others.
  The current 941 × 508 files are acceptable placeholders but look soft on high-DPI displays.
- **Cropping:** crop to the application window. The window may include its own title bar on Windows and macOS (it
  shows that the application is native); on Linux crop to the window content. Keep the same crop and window size
  across the dark and light shots so they compare side by side.
- **Size budget:** hero ≤ 400 KB, every other image ≤ 250 KB, whole directory ≤ 2 MB. Reduce with `oxipng -o4 --strip all`
  or `pngquant --quality 85-95` when over budget. Do not trade readability for size.
- **Content:** use a notebook with made-up content (never a real notebook), saved to a file so the status bar does not read
  "Untitled" and "Unsaved changes", no half-typed slash menu unless it is the point of the shot, and the six colour tags visible.
- **Naming:** `omatree-<theme-or-platform>-<subject>.png`, lowercase, hyphens.

## Retake suggestions for the existing images (optional)

`omatree-dark-kanagawa-editor.png` and `omatree-light-editor.png` show "Untitled" and "● Unsaved changes" in the
status bar and a half-typed `/emo` slash menu. Saving the notebook before the capture gives a calmer image.
