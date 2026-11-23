---
name: verify
description: Verify a Folio change in the real desktop app (Tauri dev) on macOS without taking keyboard focus from the user. Use when checking transcription output, the editor, saved sessions or banners at runtime.
---

# Verify Folio in the real app, without keystrokes

The user is usually working in other apps. **Never send keystrokes**: macOS keeps their app in front, so typed text lands in whatever they're using. Drive Folio only through Accessibility `AXPress` on buttons, and capture the window by id. The native file picker needs keystrokes, so reach transcription through **crash recovery** instead of Import.

## Launch

```sh
osascript -e 'tell application "System Events" to get name of first process whose frontmost is true'  # remember it
cd app && KINDTECH_MODELS_DIR=$HOME/folio-bench-models npm run tauri dev   # background; the binary is target/debug/kindtech-transcribe-app
```

The new window takes focus once it appears. Give it back straight away with `osascript -e 'tell application "<remembered app>" to activate'`.

## Transcribe a file with no picker

Data folder: `~/Library/Application Support/uk.kindtech.folio/`. Any `.wav` in `library/recordings/` that no saved session uses is offered for recovery **at launch**, so copy it there before launching:

```sh
cp ~/folio-bench-pack/audio/ami-sdm/AMI_EN2002b_sdm_FEO070_0027547_0029410.wav \
   "$HOME/Library/Application Support/uk.kindtech.folio/library/recordings/Recording verify.wav"
```

Recovery runs the same `transcribe` command as Import, vocabulary file included, using the detect-speakers setting (off by default). Put a test `vocabulary.txt` in the data folder first if needed.

The offer lists every planted file and stays on screen after one is recovered. To get several transcriptions in one launch (for example before and after editing `vocabulary.txt`), plant several copies under different names.

## Press buttons in the background

The web view exposes its buttons to Accessibility only once it's asked to. Turn that on, then press by label. Just after launch the page may not have loaded yet, so retry the pair a few times while the press returns `not found`:

```swift
// axon.swift: swiftc axon.swift -o axon && ./axon
import ApplicationServices; import AppKit
for a in NSWorkspace.shared.runningApplications where (a.executableURL?.lastPathComponent ?? "").contains("kindtech") {
  AXUIElementSetAttributeValue(AXUIElementCreateApplication(a.processIdentifier), "AXManualAccessibility" as CFString, kCFBooleanTrue)
}
```

```applescript
-- press.applescript "Recover transcript": no `set frontmost`
on run argv
  tell application "System Events" to tell (first process whose name contains "kindtech")
    repeat with e in (entire contents of window 1)
      try
        if role of e is "AXButton" and ((name of e) as text) starts with (item 1 of argv) then
          perform action "AXPress" of e
          return "clicked"
        end if
      end try
    end repeat
  end tell
  return "not found"
end run
```

## Capture

- **Window id:** `CGWindowListCopyWindowInfo`, owner containing `kindtech` and window name `Folio`. Then `screencapture -x -o -l <id> out.png` works while the window is behind other apps.
- **Saved session:** `library/<uuid>.json`, with `editorState.sentences[].words[]` holding `text`, `start`, `end`, `speaker` and `original`.
- **Backend logs** (for example vocabulary lines it skipped) go to the `tauri dev` output.

## Clean up

Delete the test session JSON, the planted `.wav`, any test `vocabulary.txt`, then `pkill -f target/debug/kindtech-transcribe-app`. Runs under 60 s add an entry to `import-speed.json` that is ignored.
