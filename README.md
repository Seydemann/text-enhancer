# text-enhancer

Writing-enhancement tools powered by Gemini.

## Hypr Magic — Linux desktop utility

A Rust + GTK4 floating scratchpad with streamed Gemini polish and local
voice dictation. Built for Hyprland but works on any GTK4-capable
compositor.

See [`hypr-magic/README.md`](hypr-magic/README.md) for full setup and
usage.

Quick start (Arch):

```bash
sudo pacman -S rust gtk4 pipewire
cd hypr-magic
export GEMINI_API_KEY="your-key-here"
cargo run
```

## Emacs utility — scratch-magic-polish

Canonical file lives in the
[dotfiles repo](https://github.com/Seydemann/dotfiles/blob/CachyOS/emacs/lisp/scratch-magic-polish.el).

Sends the `*scratch*` buffer to Gemini for a stateless polish and replaces
the buffer content on success.

Set your API key via env var (`GEMINI_API_KEY`) or Emacs variable
(`(setq scratch-magic-api-key "YOUR_KEY")`).
