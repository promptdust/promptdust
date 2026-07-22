# Brand assets

Source art for the PromptDust app icon.

- `icon-motes.svg` — the **Motes** mark (dust caught in a beam of light), the master vector.
- `icon-motes-1024.png` — 1024×1024 rasterization used to generate the platform icons.

## Regenerate the app icon set

```sh
cd desktop && cargo tauri icon ../brand/icon-motes-1024.png
```

This overwrites `desktop/src-tauri/icons/*` (macOS `.icns`, Windows `.ico`, PNGs,
iOS/Android). Re-rasterize the PNG from the SVG with
`rsvg-convert -w 1024 -h 1024 brand/icon-motes.svg -o brand/icon-motes-1024.png`.

The app UI palette ("Daybreak") lives in `desktop/ui/styles.css`.
