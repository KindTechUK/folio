# Spike S6: crash-safe recording

> Status: **PASS** (Mac; the hands-on kill of the real app and a Windows run are still to do) · Recorded 2026-11-12.
> Plan: [`next-phase-spikes.md`](next-phase-spikes.md#s6-crash-safe-recording).
> Test: `recorder::tests::a_recording_killed_mid_write_is_recovered_with_all_the_audio_that_reached_the_disk`
> in `app/src-tauri/src/recorder.rs`.

## Question

Does a recording survive the app being killed? Is recovery offered on restart? Does deletion
still remove everything?

## Verdict

**Pass.** Each criterion:

| Criterion | Result |
|---|---|
| At most the last chunk is lost | After recovery, at most the 8 KiB write buffer is lost: **256 ms worst case**, 52 ms in every measured kill. Without recovery, up to 5 s would be lost. |
| Recovery is offered on restart | At launch, Folio lists every recording that no saved transcript uses, with **Recover transcript** and **Delete**. |
| No files are left after deletion | Delete removes the WAV, both notes files (saved and half-written) and the deletion approval. A recovered recording is an ordinary transcript, and "Delete transcript and audio" removes it as before. |

**The plan's rolling 5–10 s chunks were not needed.** P1 writes one WAV, and every 5 s it rewrites the
header and forces the data to disk. Recovery then rewrites the header to cover all the audio on disk, so
the 5 s cadence no longer limits what is kept.

## What a crash leaves, and what recovery does

`RecordingFile` writes 16-bit samples through a `BufWriter` (8 KiB). Every 5 s it flushes the buffer,
rewrites the RIFF and data sizes, and calls `sync_data`. Between those points the buffer still spills to
the file each time it fills, but the header isn't updated. After a kill, the file therefore holds:

- a header that covers audio up to the last 5 s point (all that a WAV reader would play);
- after it, up to 5 s more audio that did reach the disk;
- missing: only what was still in the 8 KiB buffer.

`recover_recording` (`repair_wav_header`) finds the `data` chunk, trims the file to whole samples, and
rewrites both sizes from the file length. It then calls `sync_all`, and the existing transcription runs
on the repaired file.

## Environment

- Apple M5 Pro, macOS 27.2, APFS, internal SSD. Rust 1.99.1.
- The child process writes through the app's own `RecordingFile`, 100 ms (1,600 samples) at a time.
  That is the same path the capture thread uses, minus the microphone.

## Commands

```bash
cd app/src-tauri
cargo test --lib recorder::tests::a_recording_killed -- --nocapture
# 20 kills:
B=$(ls -t target/debug/deps/kindtech_transcribe_app_lib-* | grep -v '\.d$' | head -1)
for i in $(seq 20); do
  $B --exact recorder::tests::a_recording_killed_mid_write_is_recovered_with_all_the_audio_that_reached_the_disk --nocapture | grep "S6 kill"
done
```

The test runs its own binary again as a child, which records until it is killed. The parent kills it
with `SIGKILL` (on Windows, `TerminateProcess`) about 2.1 s after a header update, then reads the WAV
before and after recovery.

## Raw numbers

20 of 20 kills gave the same result:

| Written by the child (at least) | Covered by the header after the kill | After recovery | Lost after recovery |
|---|---|---|---|
| 113,600 samples (7.10 s) | 80,000 (5.00 s) | 112,768 (7.05 s) | 832 samples (52 ms) |

112,768 = 80,000 + 8 × 4,096: every full buffer that reached the file was kept. The audio recovered
matched what was written, sample by sample.

## Power loss

This can't be tested by pulling the plug on a laptop, so the analysis is:

- **macOS:** Rust's `File::sync_data` and `sync_all` call `fcntl(F_FULLFSYNC)` on Apple platforms (Rust 1.99.1
  `library/std/src/sys/fs/unix.rs`), which also flushes the drive's write cache. So what had been
  synced at the last 5 s point survives power loss. Audio written since then may or may not be on disk;
  recovery keeps whatever is.
- **Windows:** `sync_data` calls `FlushFileBuffers`, which flushes the drive cache too. Same result.

So the loss on power failure is at most 5 s, and usually less. No code change was needed.

## Findings beyond the pass criteria

- **A recording without a transcript isn't always a crash.** Closing Folio while recording keeps the
  finished WAV (by design). Removing a transcript from Transcripts without its audio also leaves the WAV.
  Folio can't tell these cases apart, so the notice says the recording "has no transcript" and that
  "Folio may have closed while it was recording", rather than claiming a crash.
- **Notes come back too.** Recovery reads `<recording>.notes.txt`, or the half-written `.tmp` if that is
  all there is, and saves the notes in the recovered transcript. The notes file is removed after that
  first save, as after Stop.
- **Live text is not recovered.** It existed only in memory. Recovery transcribes the WAV again with the
  file path, which takes about as long as importing it. The speaker setting comes from the Import page.

## Still to do

1. **The real app, by hand:** start a recording, type a note, `kill -9` the app, relaunch, then Recover.
   Do it again with Delete, and list `recordings/` to check it's empty.
2. **The same on Windows** (Task Manager "End task" and `taskkill /F`), on NTFS.

## What changes in the PRD

- P1's testing decision 2 now says "under 0.3 s" instead of "the last chunk". The assurance-pack line now
  covers the recording file and its recovery, rather than chunks.
- `docs/assurance/data-at-rest.md` describes the recovery notice, what Recover and Delete do, and that
  an unrecovered recording stays in the folder until the user chooses.
