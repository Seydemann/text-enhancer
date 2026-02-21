# text-enhancer

Two writing-enhancement tools powered by Gemini:

- `hypr-magic/`: Linux desktop utility with a floating Gemini icon and a text panel (`Magic` replaces the full text with polished output).
- `scratch-magic-polish.el`: Emacs utility for `*scratch*` that sends the full buffer and replaces it with polished output.

## Emacs Utility

File: `scratch-magic-polish.el`

What it does:

- runs only in `*scratch*`
- sends full buffer content as `<raw>...</raw>`
- stateless Gemini request each invocation
- replaces full `*scratch*` content on success

Minimal setup in Emacs:

```elisp
(add-to-list 'load-path "/path/to/text-enhancer")
(require 'scratch-magic-polish)

(add-hook
 'lisp-interaction-mode-hook
 (lambda ()
   (when (string= (buffer-name) "*scratch*")
     (local-set-key (kbd "C-c m") #'scratch-magic-polish))))
```

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
