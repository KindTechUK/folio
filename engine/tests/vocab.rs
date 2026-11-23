use engine::{Segment, SkippedLine, Transcript, Vocabulary, Word};

fn w(word: &str, start: f64, end: f64) -> Word {
    Word {
        word: word.into(),
        start,
        end,
        speaker: Some("SPEAKER_00".into()),
        original: None,
    }
}

fn changed(word: &str, start: f64, end: f64, original: &str) -> Word {
    Word {
        original: Some(original.into()),
        ..w(word, start, end)
    }
}

fn transcript(words: Vec<Word>) -> Transcript {
    let text = words
        .iter()
        .map(|w| w.word.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    Transcript {
        segments: vec![Segment {
            start: words[0].start,
            end: words[words.len() - 1].end,
            text,
            speaker: Some("SPEAKER_00".into()),
            words,
        }],
    }
}

fn vocab(text: &str) -> Vocabulary {
    Vocabulary::parse(text).0
}

#[test]
fn parse_reads_replacements_and_terms_and_reports_lines_it_cannot_use() {
    let (v, skipped) = Vocabulary::parse(
        "# Team words\n\nnavio stokes => Navier-Stokes\nKubernetes\n => nothing heard\n",
    );
    assert_eq!(
        v.replacements().collect::<Vec<_>>(),
        vec![("navio stokes", "Navier-Stokes")]
    );
    assert_eq!(v.terms(), ["Kubernetes".to_string()]);
    assert_eq!(
        skipped,
        vec![SkippedLine {
            line: 5,
            text: " => nothing heard".into()
        }]
    );
}

#[test]
fn a_word_is_replaced_whatever_its_case_keeping_its_punctuation() {
    let mut t = transcript(vec![
        w("Ask", 0.0, 0.4),
        w("Siobhan,", 0.4, 1.0),
        w("please.", 1.0, 1.5),
    ]);
    let fired = vocab("siobhan => Shivaun").apply(&mut t);
    assert_eq!(fired, 1);
    assert_eq!(
        t.segments[0].words,
        vec![
            w("Ask", 0.0, 0.4),
            changed("Shivaun,", 0.4, 1.0, "Siobhan,"),
            w("please.", 1.0, 1.5)
        ]
    );
    assert_eq!(t.segments[0].text, "Ask Shivaun, please.");
}

#[test]
fn only_whole_words_are_replaced() {
    let mut t = transcript(vec![w("Leeds", 0.0, 0.5), w("office", 0.5, 1.0)]);
    let fired = vocab("lee => Li").apply(&mut t);
    assert_eq!(fired, 0);
    assert_eq!(
        t.segments[0].words,
        vec![w("Leeds", 0.0, 0.5), w("office", 0.5, 1.0)]
    );
}

#[test]
fn two_heard_words_become_one_spanning_both() {
    let mut t = transcript(vec![
        w("the", 0.0, 0.2),
        w("navio", 0.2, 0.6),
        w("stokes", 0.7, 1.1),
    ]);
    vocab("navio stokes => Navier-Stokes").apply(&mut t);
    assert_eq!(
        t.segments[0].words,
        vec![
            w("the", 0.0, 0.2),
            changed("Navier-Stokes", 0.2, 1.1, "navio stokes")
        ]
    );
}

#[test]
fn one_heard_word_becomes_two_sharing_its_time() {
    let mut t = transcript(vec![w("kindtech", 1.0, 2.0)]);
    vocab("kindtech => Kind Tech").apply(&mut t);
    assert_eq!(
        t.segments[0].words,
        vec![
            changed("Kind", 1.0, 1.5, "kindtech"),
            changed("Tech", 1.5, 2.0, "kindtech")
        ]
    );
}

#[test]
fn a_spelled_acronym_is_joined_only_when_the_vocabulary_has_it() {
    let spelled = || {
        transcript(vec![
            w("the", 0.0, 0.2),
            w("C", 0.2, 0.4),
            w("L", 0.4, 0.6),
            w("I", 0.6, 0.8),
        ])
    };

    let mut t = spelled();
    assert_eq!(vocab("CLI => CLI").apply(&mut t), 1);
    assert_eq!(
        t.segments[0].words,
        vec![w("the", 0.0, 0.2), changed("CLI", 0.2, 0.8, "C L I")]
    );

    let mut t = spelled();
    assert_eq!(vocab("API => API").apply(&mut t), 0);
    assert_eq!(t.segments[0].words, spelled().segments[0].words);
}

#[test]
fn later_rules_see_what_earlier_rules_wrote() {
    let mut t = transcript(vec![w("navio", 0.0, 0.4), w("stokes", 0.4, 0.9)]);
    let fired = vocab("navio => Navier\nnavier stokes => Navier-Stokes").apply(&mut t);
    assert_eq!(fired, 2);
    assert_eq!(
        t.segments[0].words,
        vec![changed("Navier-Stokes", 0.0, 0.9, "navio stokes")]
    );
}
