# Testing Plan

This app does not need heavy UI automation first. The highest-value path is:

1. Add pure-function unit tests in [`src/main.rs`](./src/main.rs).
2. Extract and test the Gemini SSE consumer with fixture input.
3. Extract the polish history state model when undo/redo changes again.
4. Keep compositor feel, real microphone quality, and launcher integration as manual checks.

## Phase 1

Add unit tests for:

- `extract_text`
- `escape_xml`
- `default_gemini_thinking_level`
- `gemini_generation_config`
- `history_can_undo`
- `history_can_redo`
- `summarize_output`
- `next_recording_path`
- `ensure_voice_config`
- `load_config`

## Phase 2

Extract:

- `build_gemini_request`
- `consume_sse_stream`

Then add fixture-backed integration tests for:

- normal multi-event SSE stream
- empty stream
- malformed JSON chunk
- cancel mid-stream

## Phase 3

Extract `PolishHistory` transitions into methods and test:

- polish -> undo
- undo -> redo
- redo -> user edit clears history
- polish -> cancel clears history
- polish -> stream error restores original and leaves no history

## Manual Checks To Keep

- streamed polish "feel"
- icon window drag/toggle behavior under Hyprland
- clipboard integration
- real microphone quality and whisper accuracy
- desktop launcher behavior and single-instance locking
