# Hypr Magic (Linux / Hyprland)

Rust desktop utility with:

- floating icon window
- click icon to toggle text panel
- local voice dictation via `pw-record` + `whisper.cpp`
- streamed Gemini polish (`<raw>...</raw>` stateless payload)
- single-level undo/redo after successful polish
- appends dictated text and replaces full text only when polishing

## Stack

- Rust
- GTK4 (`gtk4-rs`)
- PipeWire `pw-record`
- local `whisper.cpp` CLI
- Gemini `streamGenerateContent` API

## Prerequisites

- Rust toolchain
- GTK4 dev libs
- PipeWire tools (`pw-record`)
- `whisper.cpp` installed and reachable via `WHISPER_CMD` or `PATH`
- a local Whisper model file
- `GEMINI_API_KEY` only if you want the `Magic` polish button

## Run

```bash
cd hypr-magic
export WHISPER_MODEL_PATH="/path/to/ggml-base.en.bin"
cargo run
```

Optional Gemini polish:

```bash
export GEMINI_API_KEY="YOUR_KEY"
```

Current default polish model:

```bash
export GEMINI_MODEL="gemini-3.1-flash-lite-preview"
export GEMINI_THINKING_LEVEL="minimal"
```

Release build/run:

```bash
cargo build --release
./target/release/hypr-magic
```

Optional Gemini override:

```bash
export GEMINI_MODEL="gemini-3.1-flash-lite-preview"
export GEMINI_THINKING_LEVEL="minimal"
```

Optional voice overrides:

```bash
export WHISPER_CMD="whisper-cpp"
export WHISPER_LANG="en"
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
- `Magic` reuses a single HTTP client and wakes the UI on the next idle cycle instead of waiting on a fixed timer.
- `Magic` streams Gemini output into the editor as chunks arrive instead of waiting for the full response.
- `Magic` turns into `Cancel` while polishing, and `Undo` / `Redo` provide a subtle single-level toggle between the original and polished text after a successful run.
- Voice transcription runs on a background worker thread using the same pattern.
- `Mic` starts/stops recording, shows a live duration counter, then appends the local transcript to the text area.
- API keys and model paths are intentionally read from env vars, not stored in code/config.
- Icon logo file: `assets/gemini-logo.svg` (Gemini mark).

## Shortcuts

- `Ctrl+Enter`: trigger `Magic` (or cancel if a polish is running)
- `Ctrl+Shift+M`: start/stop `Mic`
- `Ctrl+Z`: undo the last successful polish
- `Ctrl+Y` or `Ctrl+Shift+Z`: redo the last successful polish
- `Escape`: hide the editor panel

## Wofi Launcher

Installed files:

- `~/.local/share/applications/hypr-magic.desktop`
- `~/.local/share/icons/hicolor/scalable/apps/hypr-magic.svg`
- `~/.config/hypr-magic/env` (launcher API key env file)

The desktop launcher executes:

- `/home/seydemann/text-enhancer/hypr-magic/scripts/launch-hypr-magic.sh`
- launcher enforces single-instance via a lock file (`$XDG_RUNTIME_DIR/hypr-magic.lock`)
