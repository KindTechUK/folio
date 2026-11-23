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
//! word is ignored for matching and kept in the output; punctuation written in
//! the heard side (`c++`) must be there. A match never spans two speakers or
//! punctuation between its words, so it can't join two sentences. A spelled
//! acronym ("C L I") is joined only when an entry asks for it, because the
//! transcript is the record. Every word a replacement writes keeps what the
//! model wrote in [`Word::original`], so each change can be shown and audited.

use crate::{Transcript, Word};

#[derive(Debug, Clone, PartialEq)]
struct Replacement {
    heard: String,
    written: String,
    /// `heard` split into lowercase words. Punctuation in them is matched too.
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
                    let heard_words: Vec<String> =
                        heard.split_whitespace().map(str::to_lowercase).collect();
                    let has_word = heard_words.iter().any(|w| !core(w).is_empty());
                    if !has_word || written.is_empty() {
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
            // Words a rewrite wrote share an id, so a later rule records their
            // source once, while repeated words the model wrote stay separate.
            let mut ids: Vec<usize> = (0..seg.words.len()).collect();
            let mut next_id = ids.len();
            for rule in &self.replacements {
                let mut i = 0;
                while i < seg.words.len() {
                    let Some(m) = rule.match_at(&seg.words, i) else {
                        i += 1;
                        continue;
                    };
                    let end = m.end;
                    match rule.rewrite(&seg.words[i..end], &ids[i..end], &m) {
                        Some(new) => {
                            let len = new.len();
                            seg.words.splice(i..end, new);
                            ids.splice(i..end, std::iter::repeat_n(next_id, len));
                            next_id += 1;
                            fired += 1;
                            i += len;
                        }
                        None => i = end,
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

/// A match of words `[start, end)`, with the punctuation outside it.
struct Match<'a> {
    end: usize,
    lead: &'a str,
    trail: &'a str,
}

impl Replacement {
    /// The match starting at `words[i]`, if any. A match never spans two
    /// speakers, and never has punctuation inside it, so it can't join two
    /// sentences.
    fn match_at<'a>(&self, words: &'a [Word], i: usize) -> Option<Match<'a>> {
        let m = self
            .literal_at(words, i)
            .or_else(|| self.spelled_at(words, i))?;
        let speaker = &words[i].speaker;
        words[i..m.end]
            .iter()
            .all(|w| &w.speaker == speaker)
            .then_some(m)
    }

    /// The heard words, one transcript word each.
    fn literal_at<'a>(&self, words: &'a [Word], i: usize) -> Option<Match<'a>> {
        let n = self.heard_words.len();
        let span = words.get(i..i + n)?;
        let mut m = Match {
            end: i + n,
            lead: "",
            trail: "",
        };
        for (k, (w, heard)) in span.iter().zip(&self.heard_words).enumerate() {
            let (lead, trail) = around(&w.word, heard)?;
            if (k > 0 && !lead.is_empty()) || (k < n - 1 && !trail.is_empty()) {
                return None;
            }
            if k == 0 {
                m.lead = lead;
            }
            if k == n - 1 {
                m.trail = trail;
            }
        }
        Some(m)
    }

    /// The heard acronym spelled differently: `C L I` heard and `CLI`
    /// transcribed, or the reverse.
    fn spelled_at<'a>(&self, words: &'a [Word], i: usize) -> Option<Match<'a>> {
        let spelled = self.spelled.as_deref()?;
        if let Some((lead, trail)) = around(&words[i].word, spelled) {
            return Some(Match {
                end: i + 1,
                lead,
                trail,
            });
        }
        // A run of single letters, less a prose "a" or "I" at either end.
        if !is_letter(&words[i]) {
            return None;
        }
        let starts_run = i == 0
            || !is_letter(&words[i - 1])
            || (is_prose_letter(&words[i - 1]) && (i < 2 || !is_letter(&words[i - 2])));
        if !starts_run {
            return None;
        }
        let run = words[i..].iter().take_while(|w| is_letter(w)).count();
        let spells = |k: usize| {
            words[i..i + k]
                .iter()
                .map(|w| core(&w.word).to_lowercase())
                .collect::<String>()
                == spelled
        };
        let k = if run >= 2 && spells(run) {
            run
        } else if run >= 3 && is_prose_letter(&words[i + run - 1]) && spells(run - 1) {
            run - 1
        } else {
            return None;
        };
        let span = &words[i..i + k];
        if span[1..].iter().any(|w| !edges(&w.word).0.is_empty())
            || span[..k - 1].iter().any(|w| !edges(&w.word).1.is_empty())
        {
            return None;
        }
        Some(Match {
            end: i + k,
            lead: edges(&span[0].word).0,
            trail: edges(&span[k - 1].word).1,
        })
    }

    /// The words that replace `matched`, or `None` if they'd be unchanged.
    /// `ids` says which matched words one earlier rewrite wrote.
    fn rewrite(&self, matched: &[Word], ids: &[usize], m: &Match) -> Option<Vec<Word>> {
        let first = &matched[0];
        let last = &matched[matched.len() - 1];
        let written: Vec<&str> = self.written.split_whitespace().collect();
        let n = written.len();
        let step = (last.end - first.start) / n as f64;
        let new: Vec<Word> = written
            .iter()
            .enumerate()
            .map(|(k, w)| {
                let mut word = w.to_string();
                if k == 0 {
                    word.insert_str(0, m.lead);
                }
                if k == n - 1 {
                    word.push_str(m.trail);
                }
                // One word for one: each keeps its own time.
                let (start, end) = if n == matched.len() {
                    (matched[k].start, matched[k].end)
                } else if k == n - 1 {
                    (round3(first.start + k as f64 * step), last.end)
                } else {
                    (
                        round3(first.start + k as f64 * step),
                        round3(first.start + (k + 1) as f64 * step),
                    )
                };
                Word {
                    word,
                    start,
                    end,
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
        let mut last_id = None;
        for (w, &id) in matched.iter().zip(ids) {
            if last_id != Some(id) {
                sources.push(w.original.as_deref().unwrap_or(&w.word));
                last_id = Some(id);
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

/// The punctuation before and after a word.
fn edges(word: &str) -> (&str, &str) {
    let lead = word.len() - word.trim_start_matches(not_word).len();
    let trail = word.trim_end_matches(not_word).len().max(lead);
    (&word[..lead], &word[trail..])
}

/// When `word` is `heard` (any case) with only punctuation around it, that
/// punctuation. Punctuation written in `heard` must be there too.
fn around<'a>(word: &'a str, heard: &str) -> Option<(&'a str, &'a str)> {
    let lead_max = edges(word).0.len();
    word.char_indices()
        .map(|(b, _)| b)
        .take_while(|&b| b <= lead_max)
        .find_map(|b| {
            let rest = strip_prefix_ignoring_case(&word[b..], heard)?;
            rest.chars().all(not_word).then(|| (&word[..b], rest))
        })
}

fn strip_prefix_ignoring_case<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let mut chars = s.char_indices();
    for p in prefix.chars() {
        let (_, c) = chars.next()?;
        if !c.to_lowercase().eq(p.to_lowercase()) {
            return None;
        }
    }
    Some(chars.next().map_or("", |(b, _)| &s[b..]))
}

/// A single letter, perhaps with punctuation around it.
fn is_letter(w: &Word) -> bool {
    let c = core(&w.word);
    c.chars().count() == 1 && c.chars().all(char::is_alphabetic)
}

/// "a" or "I": a single letter that's usually a word of its own.
fn is_prose_letter(w: &Word) -> bool {
    matches!(core(&w.word), "a" | "I")
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
