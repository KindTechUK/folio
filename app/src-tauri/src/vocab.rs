//! The person's vocabulary file (`vocabulary.txt` in the app data folder),
//! applied to every finished transcript. See `engine::vocab` for the format.
//! There's no editor for it yet; IT can push the file with the installer.

use tauri::{AppHandle, Manager};

/// Load the vocabulary. Fail-soft: a missing file is an empty vocabulary, and
/// an unreadable one is logged and treated as empty, never failing a transcript.
pub(crate) fn load(app: &AppHandle) -> engine::Vocabulary {
    let Ok(dir) = app.path().app_data_dir() else {
        return engine::Vocabulary::default();
    };
    let path = dir.join("vocabulary.txt");
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return engine::Vocabulary::default(),
        Err(e) => {
            eprintln!("vocabulary: can't read {}: {e}", path.display());
            return engine::Vocabulary::default();
        }
    };
    let (vocab, skipped) = engine::Vocabulary::parse(&text);
    for s in skipped {
        eprintln!("vocabulary: skipped line {}: {}", s.line, s.text);
    }
    vocab
}

/// Apply the vocabulary file to a finished transcript.
pub(crate) fn apply(app: &AppHandle, transcript: &mut engine::Transcript) {
    load(app).apply(transcript);
}
