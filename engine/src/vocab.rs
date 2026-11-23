//! Custom vocabulary: voice2text's dictionary format, applied as fixed word
//! replacements after transcription.
//!
//! ```text
//! # comment
//! navio stokes => Navier-Stokes     (a replacement)
//! Kubernetes                        (a term: kept for hotwords, not applied)
//! ```
//!
//! Replacements match whole words, case-insensitively, in file order, so a
//! later rule sees an earlier rule's output. Punctuation around a transcript
//! word is ignored for matching and kept in the output. A spelled acronym
//! ("C L I") is joined only when an entry asks for it, because the transcript
//! is the record. Every word a replacement writes keeps what the model wrote
//! in [`Word::original`], so each change can be shown and audited.

use crate::{Transcript, Word};

#[derive(Debug, Clone, PartialEq)]
struct Replacement {
    heard: String,
    written: String,
    /// `heard` split into lowercase words, punctuation trimmed.
    heard_words: Vec<String>,
    /// `heard` as one lowercase run of letters when it can be spelled out
    /// ("cli" for `CLI` or `C L I`).
    spelled: Option<String>,
}

/// A parsed vocabulary file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Vocabulary {
    replacements: Vec<Replacement>,
    terms: Vec<String>,
}

/// A line [`Vocabulary::parse`] couldn't use.
#[derive(Debug, Clone, PartialEq)]
pub struct SkippedLine {
    /// 1-based line number.
    pub line: usize,
    pub text: String,
}

impl Vocabulary {
    /// Parse a vocabulary file. Lines it can't use are returned, never fatal.
    pub fn parse(text: &str) -> (Vocabulary, Vec<SkippedLine>) {
        let mut vocab = Vocabulary::default();
        let mut skipped = Vec::new();
        for (n, raw) in text.lines().enumerate() {
            let line = raw.trim().trim_start_matches('\u{feff}').trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            match line.split_once("=>") {
                Some((heard, written)) => {
                    let (heard, written) = (heard.trim(), written.trim());
                    let heard_words: Vec<String> = heard
                        .split_whitespace()
                        .map(core)
                        .filter(|w| !w.is_empty())
                        .map(|w| w.to_lowercase())
                        .collect();
                    if heard_words.is_empty() || written.is_empty() {
                        skipped.push(SkippedLine {
                            line: n + 1,
                            text: raw.to_string(),
                        });
                        continue;
                    }
                    let spelled = spelled_form(&heard_words);
                    vocab.replacements.push(Replacement {
                        heard: heard.to_string(),
                        written: written.to_string(),
                        heard_words,
                        spelled,
                    });
                }
                None => vocab.terms.push(line.to_string()),
            }
        }
        (vocab, skipped)
    }

    /// `(heard, written)` for each replacement, in file order.
    pub fn replacements(&self) -> impl Iterator<Item = (&str, &str)> {
        self.replacements
            .iter()
            .map(|r| (r.heard.as_str(), r.written.as_str()))
    }

    /// Lines that name a term rather than a replacement.
    pub fn terms(&self) -> &[String] {
        &self.terms
    }

    pub fn is_empty(&self) -> bool {
        self.replacements.is_empty() && self.terms.is_empty()
    }

    /// Apply the replacements to `transcript`; returns how many fired.
    pub fn apply(&self, transcript: &mut Transcript) -> usize {
        let mut fired = 0;
        for seg in &mut transcript.segments {
            let before = fired;
            for rule in &self.replacements {
                let mut i = 0;
                while i < seg.words.len() {
                    match rule.match_at(&seg.words, i) {
                        Some(end) => match rule.rewrite(&seg.words[i..end]) {
                            Some(new) => {
                                let len = new.len();
                                seg.words.splice(i..end, new);
                                fired += 1;
                                i += len;
                            }
                            None => i = end,
                        },
                        None => i += 1,
                    }
                }
            }
            if fired > before {
                seg.text = seg
                    .words
                    .iter()
                    .map(|w| w.word.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
            }
        }
        fired
    }
}

impl Replacement {
    /// End (exclusive) of a match starting at `words[i]`.
    fn match_at(&self, words: &[Word], i: usize) -> Option<usize> {
        let n = self.heard_words.len();
        if i + n <= words.len()
            && words[i..i + n]
                .iter()
                .zip(&self.heard_words)
                .all(|(w, h)| core(&w.word).to_lowercase() == *h)
        {
            return Some(i + n);
        }
        let spelled = self.spelled.as_deref()?;
        // `C L I` heard, `CLI` transcribed.
        if core(&words[i].word).to_lowercase() == spelled {
            return Some(i + 1);
        }
        // `CLI` heard, `C L I` transcribed: the whole run of single letters.
        let letter = |w: &Word| {
            let c = core(&w.word);
            c.chars().count() == 1 && c.chars().all(char::is_alphabetic)
        };
        if i > 0 && letter(&words[i - 1]) {
            return None;
        }
        let run = words[i..].iter().take_while(|w| letter(w)).count();
        let joined: String = words[i..i + run]
            .iter()
            .map(|w| core(&w.word).to_lowercase())
            .collect();
        (run >= 2 && joined == spelled).then_some(i + run)
    }

    /// The words that replace `matched`, or `None` if they'd be unchanged.
    fn rewrite(&self, matched: &[Word]) -> Option<Vec<Word>> {
        let first = &matched[0];
        let last = &matched[matched.len() - 1];
        let lead = &first.word[..first.word.len() - first.word.trim_start_matches(not_word).len()];
        let trail = &last.word[last.word.trim_end_matches(not_word).len()..];
        let written: Vec<&str> = self.written.split_whitespace().collect();
        let n = written.len();
        let step = (last.end - first.start) / n as f64;
        let new: Vec<Word> = written
            .iter()
            .enumerate()
            .map(|(k, w)| {
                let mut word = w.to_string();
                if k == 0 {
                    word.insert_str(0, lead);
                }
                if k == n - 1 {
                    word.push_str(trail);
                }
                Word {
                    word,
                    start: round3(first.start + k as f64 * step),
                    end: if k == n - 1 {
                        last.end
                    } else {
                        round3(first.start + (k + 1) as f64 * step)
                    },
                    speaker: first.speaker.clone(),
                    original: None,
                }
            })
            .collect();
        if new
            .iter()
            .map(|w| &w.word)
            .eq(matched.iter().map(|w| &w.word))
        {
            return None;
        }
        let mut sources: Vec<&str> = Vec::new();
        for w in matched {
            let s = w.original.as_deref().unwrap_or(&w.word);
            if sources.last() != Some(&s) {
                sources.push(s);
            }
        }
        let original = sources.join(" ");
        Some(
            new.into_iter()
                .map(|w| Word {
                    original: Some(original.clone()),
                    ..w
                })
                .collect(),
        )
    }
}

fn not_word(c: char) -> bool {
    !c.is_alphanumeric()
}

/// A word without the punctuation around it.
fn core(word: &str) -> &str {
    word.trim_matches(not_word)
}

/// `cli` for `["cli"]` or `["c", "l", "i"]`; `None` when it can't be spelled.
fn spelled_form(heard_words: &[String]) -> Option<String> {
    let letters = |w: &String| w.chars().all(char::is_alphabetic);
    let single = heard_words.len() == 1 && letters(&heard_words[0]);
    let spaced = heard_words.len() >= 2
        && heard_words
            .iter()
            .all(|w| w.chars().count() == 1 && letters(w));
    (single || spaced).then(|| heard_words.concat())
}

fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}
