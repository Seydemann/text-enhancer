# Hypr Magic

A Rust + GTK4 desktop writing tool. A floating Gemini icon lives on your
screen; click it to open a scratchpad where you draft or dictate text, then
hit **Magic** to stream a polished rewrite from the Gemini API. Designed for
Hyprland but works on any compositor that supports GTK4.

## Dependencies (Arch)

```bash
# Core build and runtime
sudo pacman -S rust gtk4 pipewire

# Voice input (optional — the app runs without it)
# whisper.cpp is AUR-only; use your preferred helper:
paru -S whisper.cpp-git   # or: yay -S whisper.cpp-git
```

On non-Arch distros, install the equivalents: a Rust toolchain, GTK 4
development libraries, PipeWire (which provides `pw-record`), and
optionally whisper.cpp.

## Gemini API key

Get a free key from [Google AI Studio](https://aistudio.google.com/apikey).
The app will launch without one, but the **Magic** button will be inert.

## Build and run

```bash
cd hypr-magic
export GEMINI_API_KEY="your-key-here"
cargo run            # debug build
# — or —
cargo build --release
./target/release/hypr-magic
```

That's it for a basic run. Voice input requires one more variable — see
below.

## Voice input setup

Download a whisper.cpp-compatible GGML model (the small English model is a
good default):

```bash
# Create a directory for models
mkdir -p ~/.local/share/whisper-models

# Download a model (base.en is ~150 MB)
curl -L -o ~/.local/share/whisper-models/ggml-base.en.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin
```

Then point the app at it:

```bash
export WHISPER_MODEL_PATH="$HOME/.local/share/whisper-models/ggml-base.en.bin"
```

## Configuration

All configuration is via environment variables. Defaults are shown below:

| Variable | Default | Purpose |
|---|---|---|
| `GEMINI_API_KEY` | *(none)* | Google AI Studio API key. Required for polish. |
| `GEMINI_MODEL` | `gemini-3.1-flash-lite-preview` | Which Gemini model to call. |
| `GEMINI_THINKING_LEVEL` | `minimal` (flash-lite) / `low` (others) | Thinking budget for Gemini 3.x models. |
| `WHISPER_MODEL_PATH` | *(none)* | Path to a local GGML model file. Required for voice input. |
| `WHISPER_CMD` | `whisper-cpp` | Whisper CLI binary name or path. |
| `WHISPER_LANG` | `en` | Language code passed to whisper.cpp. |
| `PW_RECORD_CMD` | `pw-record` | PipeWire recording command. |

### Persistent configuration

The launch script sources `~/.config/hypr-magic/env` on startup, so you
don't have to export variables in every shell session:

```bash
mkdir -p ~/.config/hypr-magic
cat > ~/.config/hypr-magic/env << 'EOF'
GEMINI_API_KEY=your-key-here
WHISPER_MODEL_PATH=/home/you/.local/share/whisper-models/ggml-base.en.bin
EOF
```

This file uses plain `KEY=value` lines (no `export`, no quotes needed).

## Hyprland window rules

Add these to `~/.config/hypr/hyprland.conf` so the icon floats, stays
pinned across workspaces, and the editor opens as a floating window:

```ini
windowrulev2 = float,title:^(Hypr Magic Icon)$
windowrulev2 = pin,title:^(Hypr Magic Icon)$
windowrulev2 = noborder,title:^(Hypr Magic Icon)$
windowrulev2 = noshadow,title:^(Hypr Magic Icon)$
windowrulev2 = size 64 64,title:^(Hypr Magic Icon)$
windowrulev2 = move 85% 8%,title:^(Hypr Magic Icon)$
windowrulev2 = float,title:^(Hypr Magic)$
```

Then reload: `hyprctl reload`.

## Desktop launcher (wofi / app menus)

The repo includes a `.desktop` entry and a launch script with a
single-instance guard. To install them:

```bash
# Symlink the icon
mkdir -p ~/.local/share/icons/hicolor/scalable/apps
ln -sf "$(pwd)/assets/gemini-logo.svg" \
  ~/.local/share/icons/hicolor/scalable/apps/hypr-magic.svg

# Install the desktop entry
mkdir -p ~/.local/share/applications
cp desktop/hypr-magic.desktop ~/.local/share/applications/

# Update the desktop database so launchers pick it up
update-desktop-database ~/.local/share/applications 2>/dev/null
```

**Important:** both `scripts/launch-hypr-magic.sh` and
`desktop/hypr-magic.desktop` contain a hardcoded path
(`/home/seydemann/text-enhancer/hypr-magic`). If you cloned the repo
elsewhere, update `APP_DIR` in the launch script and the `Exec` line in the
desktop entry to match your location.

## Keyboard shortcuts

| Shortcut | Action |
|---|---|
| `Ctrl+Enter` | Polish text (or cancel if already polishing) |
| `Ctrl+Shift+M` | Start / stop voice recording |
| `Ctrl+Z` | Undo last polish (restore original) |
| `Ctrl+Y` / `Ctrl+Shift+Z` | Redo last polish |
| `Escape` | Hide the editor panel |

## How it works

- **Magic** sends the scratchpad text to Gemini's `streamGenerateContent`
  endpoint inside a `<raw>...</raw>` envelope. Chunks stream into the
  editor as they arrive; the UI stays responsive because the HTTP call runs
  on a background thread.
- **Mic** records 16 kHz mono WAV via `pw-record`, stops on second click,
  then transcribes locally with whisper.cpp on a background thread. The
  transcript is appended to whatever is already in the scratchpad.
- After a successful polish, **Undo** / **Redo** toggle between the
  original and polished text. Any manual edit clears this history.
- The floating icon window is a separate GTK4 `ApplicationWindow` with a
  transparent background and a `WindowHandle` for drag support.
