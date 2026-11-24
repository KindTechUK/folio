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

The web view exposes its buttons to Accessibility only once it's asked to. Use the Accessibility API directly; System Events' `window 1` can report no window, for example when Folio opens on a second display. This switches the web view's tree on, then presses the first button whose title starts with the argument:

```swift
// axpress.swift: swiftc -O axpress.swift -o axpress && ./axpress "Recover transcript"
import ApplicationServices; import AppKit
let target = CommandLine.arguments[1]
guard let app = NSWorkspace.shared.runningApplications.first(where: { ($0.executableURL?.lastPathComponent ?? "").contains("kindtech") }) else { print("no app"); exit(1) }
let root = AXUIElementCreateApplication(app.processIdentifier)
AXUIElementSetAttributeValue(root, "AXManualAccessibility" as CFString, kCFBooleanTrue)
func attr(_ e: AXUIElement, _ n: String) -> AnyObject? { var v: AnyObject?; AXUIElementCopyAttributeValue(e, n as CFString, &v); return v }
func walk(_ e: AXUIElement, _ depth: Int) -> Bool {
  if depth > 60 { return false }
  if (attr(e, "AXRole") as? String) == "AXButton",
     ((attr(e, "AXTitle") as? String) ?? (attr(e, "AXDescription") as? String) ?? "").hasPrefix(target) {
    print("pressed", AXUIElementPerformAction(e, "AXPress" as CFString).rawValue); return true
  }
  return ((attr(e, "AXChildren") as? [AXUIElement]) ?? []).contains { walk($0, depth + 1) }
}
for w in (attr(root, "AXWindows") as? [AXUIElement]) ?? [] { if walk(w, 0) { exit(0) } }
print("not found")
```

Retry while it prints `not found`: the first `tauri dev` run after a Rust change builds for minutes, and the page loads after the window appears. `pressed 0` means it worked; `-25204` means the button was disabled because another transcription was running.

## Capture

- **Window id:** `CGWindowListCopyWindowInfo`, owner containing `kindtech` and window name `Folio`. Then `screencapture -x -o -l <id> out.png` works while the window is behind other apps.
- **Saved session:** `library/<uuid>.json`, with `editorState.sentences[].words[]` holding `text`, `start`, `end`, `speaker` and `original`.
- **Backend logs** (for example vocabulary lines it skipped) go to the `tauri dev` output.

## Clean up

Delete the test session JSON, the planted `.wav`, any test `vocabulary.txt`, then `pkill -f target/debug/kindtech-transcribe-app`. Runs under 60 s add an entry to `import-speed.json` that is ignored.
