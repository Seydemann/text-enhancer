# Hypr Magic (Linux / Hyprland)

Rust desktop utility with:

- floating icon window
- click icon to toggle text panel
- full-text Gemini polish (`<raw>...</raw>` stateless payload)
- replaces entire panel text with polished output

## Stack

- Rust
- GTK4 (`gtk4-rs`)
- Gemini `generateContent` API

## Prerequisites

- Rust toolchain
- GTK4 dev libs
- `GEMINI_API_KEY` environment variable

## Run

```bash
cd hypr-magic
export GEMINI_API_KEY="YOUR_KEY"
cargo run
```

Optional model override:

```bash
export GEMINI_MODEL="gemini-3-flash-preview"
```

## Hyprland Behavior

Add rules so the icon stays floating and pinned across workspaces.

Example `~/.config/hypr/hyprland.conf` snippet:

```ini
windowrulev2 = float,title:^(Hypr Magic Icon)$
windowrulev2 = pin,title:^(Hypr Magic Icon)$
windowrulev2 = noborder,title:^(Hypr Magic Icon)$
windowrulev2 = noshadow,title:^(Hypr Magic Icon)$
windowrulev2 = size 64 64,title:^(Hypr Magic Icon)$
windowrulev2 = move 85% 8%,title:^(Hypr Magic Icon)$
windowrulev2 = float,title:^(Hypr Magic)$
```

Then reload Hyprland config.

## Notes

- Gemini request runs on a background worker thread (UI stays responsive).
- API key is intentionally read from env vars, not stored in code/config.
- Icon logo file: `assets/gemini-logo.svg` (Gemini mark).
