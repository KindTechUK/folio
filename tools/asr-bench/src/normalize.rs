//! The Open ASR Leaderboard's English text normaliser, ported from
//! `normalizer/normalizer.py` at commit 2aaa07a (see `NOTICE`).
//!
//! Behaviour is pinned by `tests/oasr_parity.rs` against outputs of the Python
//! original. Known divergence: numeric detection is ASCII-only, where Python's
//! `\d` also matches other scripts' digits; bench references are English.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use num_bigint::BigInt;
use num_integer::Integer;
use num_traits::{One, Zero};
use regex::{Captures, Regex};
use unicode_general_category::{get_general_category, GeneralCategory as G};
use unicode_normalization::UnicodeNormalization;

const SPELLING: &str = include_str!("../data/english_spelling.tsv");
const NAMES: &str = include_str!("../data/english_names.tsv");
const COMPOUNDS: &str = include_str!("../data/english_compounds.tsv");
const REPLACERS: &str = include_str!("../data/english_replacers.tsv");
const FILLERS: &str = include_str!("../data/english_fillers.txt");

fn tsv(text: &'static str) -> Vec<(&'static str, &'static str)> {
    text.lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.split_once('\t').expect("tab-separated pair"))
        .collect()
}

/// A Python `re.sub` replacement (`\1`) as a `regex` replacement (`${1}`).
fn py_replacement(rep: &str) -> String {
    let escaped = rep.replace('$', "$$");
    Regex::new(r"\\(\d+)").unwrap().replace_all(&escaped, "$${${1}}").into_owned()
}

fn re(p: &str) -> Regex {
    Regex::new(p).unwrap_or_else(|e| panic!("bad pattern {p}: {e}"))
}

/// Non-ASCII letters that NFKD does not decompose.
fn additional_diacritic(c: char) -> Option<&'static str> {
    Some(match c {
        'œ' => "oe",
        'Œ' => "OE",
        'ø' => "o",
        'Ø' => "O",
        'æ' => "ae",
        'Æ' => "AE",
        'ß' => "ss",
        'ẞ' => "SS",
        'đ' => "d",
        'Đ' => "D",
        'ð' => "d",
        'Ð' => "D",
        'þ' => "th",
        'Þ' => "th",
        'ł' => "l",
        'Ł' => "L",
        _ => return None,
    })
}

/// Unicode general category starts with M, S or P.
fn is_mark_symbol_punct(c: char) -> bool {
    matches!(
        get_general_category(c),
        G::NonspacingMark
            | G::SpacingMark
            | G::EnclosingMark
            | G::MathSymbol
            | G::CurrencySymbol
            | G::ModifierSymbol
            | G::OtherSymbol
            | G::ConnectorPunctuation
            | G::DashPunctuation
            | G::OpenPunctuation
            | G::ClosePunctuation
            | G::InitialPunctuation
            | G::FinalPunctuation
            | G::OtherPunctuation
    )
}

/// `remove_symbols_and_diacritics(s, keep)`.
pub fn remove_symbols_and_diacritics(s: &str, keep: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.nfkd() {
        if keep.contains(c) {
            out.push(c);
        } else if let Some(r) = additional_diacritic(c) {
            out.push_str(r);
        } else if get_general_category(c) == G::NonspacingMark {
        } else if is_mark_symbol_punct(c) {
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}

// ---------------------------------------------------------------- numbers

/// A pending number: Python keeps it as an `int` or a `str`.
#[derive(Clone, Debug)]
enum Val {
    Int(BigInt),
    Str(String),
}

impl Val {
    fn to_str(&self) -> String {
        match self {
            Val::Int(i) => i.to_string(),
            Val::Str(s) => s.clone(),
        }
    }
    /// `str(value or "")`: a zero int is falsy.
    fn or_empty(v: &Option<Val>) -> String {
        match v {
            None => String::new(),
            Some(Val::Int(i)) if i.is_zero() => String::new(),
            Some(v) => v.to_str(),
        }
    }
}

/// `fractions.Fraction(s)` for the plain decimals the normaliser builds,
/// reduced. `None` where Python raises `ValueError`.
fn to_fraction(v: &Val) -> Option<(BigInt, BigInt)> {
    let s = match v {
        Val::Int(i) => return Some((i.clone(), BigInt::one())),
        Val::Str(s) => s.trim(),
    };
    let (neg, body) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let (int_part, frac_part) = match body.split_once('.') {
        Some((a, b)) => (a, b),
        None => (body, ""),
    };
    let digits = |p: &str| p.bytes().all(|b| b.is_ascii_digit());
    if !digits(int_part) || !digits(frac_part) || (int_part.is_empty() && frac_part.is_empty()) {
        return None;
    }
    if int_part.is_empty() && !body.starts_with('.') {
        return None;
    }
    let all: String = format!("{int_part}{frac_part}");
    let mut num: BigInt = if all.is_empty() { BigInt::zero() } else { all.parse().ok()? };
    if neg {
        num = -num;
    }
    let den = num_traits::pow(BigInt::from(10u32), frac_part.len());
    let g = num.gcd(&den);
    Some((num / &g, den / g))
}

struct Numbers {
    zeros: HashSet<String>,
    ones: HashMap<String, u32>,
    ones_suffixed: HashMap<String, (u32, &'static str)>,
    tens: HashMap<String, u32>,
    tens_suffixed: HashMap<String, (u32, &'static str)>,
    multipliers: HashMap<String, BigInt>,
    multipliers_suffixed: HashMap<String, (BigInt, &'static str)>,
    decimals: HashSet<String>,
    preceding_prefixers: HashMap<&'static str, &'static str>,
    following_prefixers: HashMap<&'static str, &'static str>,
    prefixes: HashSet<char>,
    specials: HashSet<&'static str>,
    words: HashSet<String>,
    numeric: Regex,
    digits: Regex,
    and_a_half: Regex,
    letter_digit: Regex,
    digit_letter: Regex,
    suffix_space: Regex,
    combine_cents: Regex,
    extract_cents: Regex,
    one: Regex,
}

const ONES: [&str; 19] = [
    "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven",
    "twelve", "thirteen", "fourteen", "fifteen", "sixteen", "seventeen", "eighteen", "nineteen",
];

impl Numbers {
    fn new() -> Self {
        let zeros: HashSet<String> = ["o", "oh", "zero"].map(String::from).into();
        let ones: HashMap<String, u32> =
            ONES.iter().enumerate().map(|(i, n)| (n.to_string(), i as u32 + 1)).collect();
        let mut ones_suffixed: HashMap<String, (u32, &'static str)> = HashMap::new();
        for (name, &v) in &ones {
            let plural = if name == "six" { "sixes".to_string() } else { format!("{name}s") };
            ones_suffixed.insert(plural, (v, "s"));
        }
        for (name, v, s) in [
            ("zeroth", 0, "th"),
            ("first", 1, "st"),
            ("second", 2, "nd"),
            ("third", 3, "rd"),
            ("fifth", 5, "th"),
            ("twelfth", 12, "th"),
        ] {
            ones_suffixed.insert(name.to_string(), (v, s));
        }
        for (name, &v) in &ones {
            if v > 3 && v != 5 && v != 12 {
                let ord = format!("{name}{}", if name.ends_with('t') { "h" } else { "th" });
                ones_suffixed.insert(ord, (v, "th"));
            }
        }
        let tens: HashMap<String, u32> = [
            ("twenty", 20),
            ("thirty", 30),
            ("forty", 40),
            ("fifty", 50),
            ("sixty", 60),
            ("seventy", 70),
            ("eighty", 80),
            ("ninety", 90),
        ]
        .into_iter()
        .map(|(n, v)| (n.to_string(), v))
        .collect();
        let mut tens_suffixed = HashMap::new();
        for (name, &v) in &tens {
            tens_suffixed.insert(name.replace('y', "ies"), (v, "s"));
            tens_suffixed.insert(name.replace('y', "ieth"), (v, "th"));
        }
        let mult_names = [
            "hundred", "thousand", "million", "billion", "trillion", "quadrillion", "quintillion",
            "sextillion", "septillion", "octillion", "nonillion", "decillion",
        ];
        let mut multipliers = HashMap::new();
        for (k, name) in mult_names.iter().enumerate() {
            let v = if k == 0 {
                BigInt::from(100u32)
            } else {
                num_traits::pow(BigInt::from(1000u32), k)
            };
            multipliers.insert(name.to_string(), v);
        }
        let mut multipliers_suffixed = HashMap::new();
        for (name, v) in &multipliers {
            multipliers_suffixed.insert(format!("{name}s"), (v.clone(), "s"));
            multipliers_suffixed.insert(format!("{name}th"), (v.clone(), "th"));
        }
        let decimals: HashSet<String> =
            ones.keys().chain(tens.keys()).chain(zeros.iter()).cloned().collect();
        let preceding_prefixers: HashMap<_, _> =
            [("minus", "-"), ("negative", "-"), ("plus", "+"), ("positive", "+")].into();
        let following_prefixers: HashMap<_, _> = [
            ("pound", "£"),
            ("pounds", "£"),
            ("euro", "€"),
            ("euros", "€"),
            ("dollar", "$"),
            ("dollars", "$"),
            ("cent", "¢"),
            ("cents", "¢"),
        ]
        .into();
        let prefixes: HashSet<char> = preceding_prefixers
            .values()
            .chain(following_prefixers.values())
            .map(|s| s.chars().next().unwrap())
            .collect();
        let specials: HashSet<&str> = ["and", "double", "triple", "point"].into();
        let mut words: HashSet<String> = HashSet::new();
        words.extend(zeros.iter().cloned());
        words.extend(ones.keys().cloned());
        words.extend(ones_suffixed.keys().cloned());
        words.extend(tens.keys().cloned());
        words.extend(tens_suffixed.keys().cloned());
        words.extend(multipliers.keys().cloned());
        words.extend(multipliers_suffixed.keys().cloned());
        words.extend(preceding_prefixers.keys().map(|s| s.to_string()));
        words.extend(following_prefixers.keys().map(|s| s.to_string()));
        words.extend(["per", "percent"].map(String::from));
        words.extend(specials.iter().map(|s| s.to_string()));
        Numbers {
            zeros,
            ones,
            ones_suffixed,
            tens,
            tens_suffixed,
            multipliers,
            multipliers_suffixed,
            decimals,
            preceding_prefixers,
            following_prefixers,
            prefixes,
            specials,
            words,
            numeric: re(r"^[0-9]+(\.[0-9]+)?$"),
            digits: re(r"^[0-9]+$"),
            and_a_half: re(r"\band\s+a\s+half\b"),
            letter_digit: re(r"([a-z])([0-9])"),
            digit_letter: re(r"([0-9])([a-z])"),
            suffix_space: re(r"([0-9])\s+(st|nd|rd|th|s)\b"),
            combine_cents: re(r"([€£$])([0-9]+) (?:and )?¢([0-9]{1,2})\b"),
            extract_cents: re(r"[€£$]0.([0-9]{1,2})\b"),
            one: re(r"\b1(s?)\b"),
        }
    }

    fn is_digit_token(&self, t: Option<&str>) -> bool {
        t.is_some_and(|t| {
            self.digits.is_match(t)
                || self.zeros.contains(t)
                || self.ones.contains_key(t)
                || self.tens.contains_key(t)
        })
    }

    fn process_words(&self, words: &[&str]) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut prefix: Option<String> = None;
        let mut value: Option<Val> = None;
        let mut skip = false;

        // `output(result)`: prefix it, reset the pending number and prefix.
        fn emit(out: &mut Vec<String>, prefix: &mut Option<String>, value: &mut Option<Val>, r: String) {
            let r = match prefix.take() {
                Some(p) => p + &r,
                None => r,
            };
            *value = None;
            out.push(r);
        }

        for (i, &current) in words.iter().enumerate() {
            let prev = if i > 0 { Some(words[i - 1]) } else { None };
            let next = words.get(i + 1).copied();
            if skip {
                skip = false;
                continue;
            }
            let next_is_numeric = next.is_some_and(|n| self.numeric.is_match(n));
            let first = current.chars().next().unwrap();
            let has_prefix = self.prefixes.contains(&first);
            let without_prefix = if has_prefix { &current[first.len_utf8()..] } else { current };
            let in_words = |w: Option<&str>| w.is_some_and(|w| self.words.contains(w));
            let prev_in_ones = prev.is_some_and(|p| self.ones.contains_key(p));
            let prev_in_tens = prev.is_some_and(|p| self.tens.contains_key(p));

            if self.numeric.is_match(without_prefix) {
                let f = to_fraction(&Val::Str(without_prefix.to_string()))
                    .expect("numeric token parses as a fraction");
                if let Some(v) = &value {
                    if let Val::Str(s) = v {
                        if s.ends_with('.') {
                            value = Some(Val::Str(format!("{s}{current}")));
                            continue;
                        }
                    }
                    let v = v.to_str();
                    emit(&mut out, &mut prefix, &mut value, v);
                }
                if has_prefix {
                    prefix = Some(first.to_string());
                }
                value = Some(if f.1.is_one() {
                    Val::Int(f.0)
                } else {
                    Val::Str(without_prefix.to_string())
                });
            } else if !self.words.contains(current) {
                if let Some(v) = &value {
                    let v = v.to_str();
                    emit(&mut out, &mut prefix, &mut value, v);
                }
                emit(&mut out, &mut prefix, &mut value, current.to_string());
            } else if self.zeros.contains(current) {
                let next2 = words.get(i + 2).copied();
                let in_number =
                    self.is_digit_token(next) && (value.is_some() || self.is_digit_token(next2));
                if current == "oh" && !in_number {
                    if let Some(v) = &value {
                        let v = v.to_str();
                        emit(&mut out, &mut prefix, &mut value, v);
                    }
                    emit(&mut out, &mut prefix, &mut value, current.to_string());
                } else {
                    value = Some(Val::Str(Val::or_empty(&value) + "0"));
                }
            } else if let Some(&ones) = self.ones.get(current) {
                value = Some(match value.take() {
                    None => Val::Int(BigInt::from(ones)),
                    Some(v) if matches!(v, Val::Str(_)) || prev_in_ones => {
                        let s = v.to_str();
                        if prev_in_tens && ones < 10 {
                            Val::Str(drop_last(&s) + &ones.to_string())
                        } else {
                            Val::Str(s + &ones.to_string())
                        }
                    }
                    Some(Val::Int(n)) => {
                        let m = if ones < 10 { 10u32 } else { 100u32 };
                        if (&n % m).is_zero() {
                            Val::Int(n + ones)
                        } else {
                            Val::Str(format!("{n}{ones}"))
                        }
                    }
                    Some(Val::Str(_)) => unreachable!(),
                });
            } else if let Some(&(ones, suffix)) = self.ones_suffixed.get(current) {
                let r = match value.clone() {
                    None => format!("{ones}{suffix}"),
                    Some(v) if matches!(v, Val::Str(_)) || prev_in_ones => {
                        let s = v.to_str();
                        if prev_in_tens && ones < 10 {
                            format!("{}{ones}{suffix}", drop_last(&s))
                        } else {
                            format!("{s}{ones}{suffix}")
                        }
                    }
                    Some(Val::Int(n)) => {
                        let m = if ones < 10 { 10u32 } else { 100u32 };
                        if (&n % m).is_zero() {
                            format!("{}{suffix}", n + ones)
                        } else {
                            format!("{n}{ones}{suffix}")
                        }
                    }
                    Some(Val::Str(_)) => unreachable!(),
                };
                emit(&mut out, &mut prefix, &mut value, r);
                value = None;
            } else if let Some(&tens) = self.tens.get(current) {
                value = Some(match value.take() {
                    None => Val::Int(BigInt::from(tens)),
                    Some(Val::Str(s)) => Val::Str(format!("{s}{tens}")),
                    Some(Val::Int(n)) => {
                        if (&n % 100u32).is_zero() {
                            Val::Int(n + tens)
                        } else {
                            Val::Str(format!("{n}{tens}"))
                        }
                    }
                });
            } else if let Some(&(tens, suffix)) = self.tens_suffixed.get(current) {
                let r = match value.clone() {
                    None => format!("{tens}{suffix}"),
                    Some(Val::Str(s)) => format!("{s}{tens}{suffix}"),
                    Some(Val::Int(n)) => {
                        if (&n % 100u32).is_zero() {
                            format!("{}{suffix}", n + tens)
                        } else {
                            format!("{n}{tens}{suffix}")
                        }
                    }
                };
                emit(&mut out, &mut prefix, &mut value, r);
            } else if let Some(mult) = self.multipliers.get(current) {
                match value.clone() {
                    None => value = Some(Val::Int(mult.clone())),
                    Some(v) if matches!(&v, Val::Str(_)) || matches!(&v, Val::Int(n) if n.is_zero()) => {
                        match to_fraction(&v).map(|(n, d)| reduce(n * mult, d)) {
                            Some((n, d)) if d.is_one() => value = Some(Val::Int(n)),
                            _ => {
                                emit(&mut out, &mut prefix, &mut value, v.to_str());
                                value = Some(Val::Int(mult.clone()));
                            }
                        }
                    }
                    Some(Val::Int(n)) => {
                        let before = &n / 1000u32 * 1000u32;
                        let residual = &n % 1000u32;
                        value = Some(Val::Int(before + residual * mult));
                    }
                    Some(Val::Str(_)) => unreachable!(),
                }
            } else if let Some((mult, suffix)) = self.multipliers_suffixed.get(current) {
                match value.clone() {
                    None => emit(&mut out, &mut prefix, &mut value, format!("{mult}{suffix}")),
                    Some(Val::Str(s)) => {
                        match to_fraction(&Val::Str(s.clone())).map(|(n, d)| reduce(n * mult, d)) {
                            Some((n, d)) if d.is_one() => {
                                emit(&mut out, &mut prefix, &mut value, format!("{n}{suffix}"))
                            }
                            _ => {
                                emit(&mut out, &mut prefix, &mut value, s);
                                emit(&mut out, &mut prefix, &mut value, format!("{mult}{suffix}"));
                            }
                        }
                    }
                    Some(Val::Int(n)) => {
                        let before = &n / 1000u32 * 1000u32;
                        let residual = &n % 1000u32;
                        let v = before + residual * mult;
                        emit(&mut out, &mut prefix, &mut value, format!("{v}{suffix}"));
                    }
                }
                value = None;
            } else if let Some(&sym) = self.preceding_prefixers.get(current) {
                if let Some(v) = &value {
                    let v = v.to_str();
                    emit(&mut out, &mut prefix, &mut value, v);
                }
                if in_words(next) || next_is_numeric {
                    prefix = Some(sym.to_string());
                } else {
                    emit(&mut out, &mut prefix, &mut value, current.to_string());
                }
            } else if let Some(&sym) = self.following_prefixers.get(current) {
                if let Some(v) = &value {
                    prefix = Some(sym.to_string());
                    let v = v.to_str();
                    emit(&mut out, &mut prefix, &mut value, v);
                } else {
                    emit(&mut out, &mut prefix, &mut value, current.to_string());
                }
            } else if current == "per" || current == "percent" {
                if let Some(v) = &value {
                    let v = v.to_str();
                    if current == "per" {
                        if next == Some("cent") {
                            emit(&mut out, &mut prefix, &mut value, format!("{v}%"));
                            skip = true;
                        } else {
                            emit(&mut out, &mut prefix, &mut value, v);
                            emit(&mut out, &mut prefix, &mut value, current.to_string());
                        }
                    } else {
                        emit(&mut out, &mut prefix, &mut value, format!("{v}%"));
                    }
                } else {
                    emit(&mut out, &mut prefix, &mut value, current.to_string());
                }
            } else if self.specials.contains(current) {
                if !in_words(next) && !next_is_numeric {
                    if let Some(v) = &value {
                        let v = v.to_str();
                        emit(&mut out, &mut prefix, &mut value, v);
                    }
                    emit(&mut out, &mut prefix, &mut value, current.to_string());
                } else if current == "and" {
                    if !prev.is_some_and(|p| self.multipliers.contains_key(p)) {
                        if let Some(v) = &value {
                            let v = v.to_str();
                            emit(&mut out, &mut prefix, &mut value, v);
                        }
                        emit(&mut out, &mut prefix, &mut value, current.to_string());
                    }
                } else if current == "double" || current == "triple" {
                    let n = next.unwrap_or("");
                    if self.ones.contains_key(n) || self.zeros.contains(n) {
                        let repeats = if current == "double" { 2 } else { 3 };
                        let ones = self.ones.get(n).copied().unwrap_or(0);
                        value = Some(Val::Str(Val::or_empty(&value) + &ones.to_string().repeat(repeats)));
                        skip = true;
                    } else {
                        if let Some(v) = &value {
                            let v = v.to_str();
                            emit(&mut out, &mut prefix, &mut value, v);
                        }
                        emit(&mut out, &mut prefix, &mut value, current.to_string());
                    }
                } else if current == "point" {
                    if next.is_some_and(|n| self.decimals.contains(n)) || next_is_numeric {
                        value = Some(Val::Str(Val::or_empty(&value) + "."));
                    }
                }
            } else {
                unreachable!("unexpected token: {current}");
            }
        }
        if let Some(v) = &value {
            let v = v.to_str();
            emit(&mut out, &mut prefix, &mut value, v);
        }
        out
    }

    fn preprocess(&self, s: &str) -> String {
        let segments: Vec<&str> = self.and_a_half.split(s).collect();
        let mut results: Vec<String> = Vec::new();
        for (i, seg) in segments.iter().enumerate() {
            if seg.trim().is_empty() {
                continue;
            }
            results.push(seg.to_string());
            if i != segments.len() - 1 {
                let last = seg.split_whitespace().last().unwrap_or("");
                if self.decimals.contains(last) || self.multipliers.contains_key(last) {
                    results.push("point five".into());
                } else {
                    results.push("and a half".into());
                }
            }
        }
        let s = results.join(" ");
        let s = self.letter_digit.replace_all(&s, "$1 $2");
        let s = self.digit_letter.replace_all(&s, "$1 $2");
        self.suffix_space.replace_all(&s, "$1$2").into_owned()
    }

    fn postprocess(&self, s: &str) -> String {
        let s = self.combine_cents.replace_all(s, |c: &Captures| {
            let cents: u64 = c[3].parse().unwrap();
            format!("{}{}.{cents:02}", &c[1], &c[2])
        });
        let s = self.extract_cents.replace_all(&s, |c: &Captures| {
            format!("¢{}", c[1].parse::<u64>().unwrap())
        });
        self.one.replace_all(&s, "one$1").into_owned()
    }

    fn normalize(&self, s: &str) -> String {
        let s = self.preprocess(s);
        let words: Vec<&str> = s.split_whitespace().collect();
        let s = self.process_words(&words).join(" ");
        self.postprocess(&s)
    }
}

fn drop_last(s: &str) -> String {
    let mut c = s.chars();
    c.next_back();
    c.as_str().to_string()
}

fn reduce(n: BigInt, d: BigInt) -> (BigInt, BigInt) {
    let g = n.gcd(&d);
    if g.is_zero() {
        return (n, d);
    }
    (n / &g, d / g)
}

// ---------------------------------------------------------------- text

/// `EnglishTextNormalizer`.
pub struct EnglishTextNormalizer {
    brackets: Regex,
    parens: Regex,
    fillers: Regex,
    space_apostrophe: Regex,
    replacers: Vec<(Regex, String)>,
    digit_commas: Regex,
    periods: Regex,
    compounds: Vec<(Regex, String)>,
    numbers: Numbers,
    spelling: HashMap<&'static str, &'static str>,
    names: HashMap<&'static str, &'static str>,
    symbol_before: Regex,
    percent_after: Regex,
    spaces: Regex,
}

impl EnglishTextNormalizer {
    pub fn new() -> Self {
        let pairs = |t| {
            tsv(t).into_iter().map(|(p, r)| (re(p), py_replacement(r))).collect::<Vec<_>>()
        };
        EnglishTextNormalizer {
            brackets: re(r"[<\[][^>\]]*[>\]]"),
            parens: re(r"\(([^)]+?)\)"),
            fillers: re(FILLERS.trim_end()),
            space_apostrophe: re(r"\s+'"),
            replacers: pairs(REPLACERS),
            digit_commas: re(r"(\d),(\d)"),
            periods: re(r"\.([^0-9]|$)"),
            compounds: pairs(COMPOUNDS),
            numbers: Numbers::new(),
            spelling: tsv(SPELLING).into_iter().collect(),
            names: tsv(NAMES).into_iter().collect(),
            symbol_before: re(r"[.$¢€£]([^0-9])"),
            percent_after: re(r"([^0-9])%"),
            spaces: re(r"\s+"),
        }
    }

    /// The normalised text (single-spaced, possibly with edge spaces).
    pub fn normalize(&self, s: &str) -> String {
        let s = s.to_lowercase();
        let s = self.brackets.replace_all(&s, "");
        let s = self.parens.replace_all(&s, "");
        let s = self.fillers.replace_all(&s, "");
        let mut s = self.space_apostrophe.replace_all(&s, "'").into_owned();
        for (p, r) in &self.replacers {
            s = p.replace_all(&s, r.as_str()).into_owned();
        }
        let s = self.digit_commas.replace_all(&s, "${1}${2}");
        let s = self.periods.replace_all(&s, " ${1}");
        let mut s = remove_symbols_and_diacritics(&s, ".%$¢€£");
        for (p, r) in &self.compounds {
            s = p.replace_all(&s, r.as_str()).into_owned();
        }
        let s = self.numbers.normalize(&s);
        let s = map_words(&s, &self.spelling);
        let s = map_words(&s, &self.names);
        let s = acronyms(&s);
        let s = self.symbol_before.replace_all(&s, " ${1}");
        let s = self.percent_after.replace_all(&s, "${1} ");
        self.spaces.replace_all(&s, " ").into_owned()
    }

    /// The leaderboard's normalised words.
    pub fn words(&self, s: &str) -> Vec<String> {
        self.normalize(s).split_whitespace().map(String::from).collect()
    }
}

impl Default for EnglishTextNormalizer {
    fn default() -> Self {
        Self::new()
    }
}

/// One shared normaliser (building it compiles ~40 regexes).
pub fn english() -> &'static EnglishTextNormalizer {
    static N: OnceLock<EnglishTextNormalizer> = OnceLock::new();
    N.get_or_init(EnglishTextNormalizer::new)
}

fn map_words(s: &str, map: &HashMap<&'static str, &'static str>) -> String {
    s.split_whitespace().map(|w| *map.get(w).unwrap_or(&w)).collect::<Vec<_>>().join(" ")
}

/// `EnglishAcronymNormalizer`: runs of single-character tokens join ("b b c" -> "bbc");
/// a run holding "a" or "i" needs 3+ tokens.
fn acronyms(s: &str) -> String {
    let words: Vec<&str> = s.split_whitespace().collect();
    let single = |w: &str| {
        let mut c = w.chars();
        matches!((c.next(), c.next()), (Some(ch), None) if ch.is_alphanumeric())
    };
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < words.len() {
        if single(words[i]) {
            let mut j = i + 1;
            while j < words.len() && single(words[j]) {
                j += 1;
            }
            let run = &words[i..j];
            let min_run = if run.iter().any(|c| *c == "a" || *c == "i") { 3 } else { 2 };
            if run.len() >= min_run {
                out.push(run.concat());
            } else {
                out.extend(run.iter().map(|w| w.to_string()));
            }
            i = j;
        } else {
            out.push(words[i].to_string());
            i += 1;
        }
    }
    out.join(" ")
}
