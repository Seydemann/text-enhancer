# text-enhancer

Two writing-enhancement tools powered by Gemini:

- `hypr-magic/`: Linux desktop utility with a floating Gemini icon and a text panel (`Magic` replaces the full text with polished output).
- Emacs utility (`scratch-magic-polish.el`): source of truth lives in your dotfiles repo.

## Emacs Utility

Canonical file:

- <https://github.com/Seydemann/dotfiles/blob/CachyOS/emacs/lisp/scratch-magic-polish.el>

What it does:

- runs only in `*scratch*`
- sends full buffer content as `<raw>...</raw>`
- stateless Gemini request each invocation
- replaces full `*scratch*` content on success

Set your API key (either):

- env var: `GEMINI_API_KEY`
- Emacs variable: `(setq scratch-magic-api-key "YOUR_KEY")`

## Linux Desktop Utility

See `hypr-magic/README.md` for run/setup details.

Quick run:

```bash
cd hypr-magic
export GEMINI_API_KEY="YOUR_KEY"
cargo run
```
