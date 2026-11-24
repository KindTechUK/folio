"""Dump the Open ASR Leaderboard English scorer's data tables and golden outputs.

Run with -I. argv[1]: the oasr package's parent dir (v2t's utils/asr_grid), argv[2]: output dir.
"""
import json, sys, random
sys.path.insert(0, sys.argv[1])
from oasr.normalizer import EnglishTextNormalizer
from oasr.english_abbreviations import english_spelling_normalizer, english_name_normalizer, english_compound_normalizer
from kaldialign import edit_distance

out = sys.argv[2]
n = EnglishTextNormalizer()

def tsv(path, rows):
    with open(path, "w", encoding="utf-8") as f:
        for a, b in rows:
            assert "\t" not in a and "\t" not in b and "\n" not in a and "\n" not in b
            f.write(f"{a}\t{b}\n")

tsv(f"{out}/english_spelling.tsv", english_spelling_normalizer.items())
tsv(f"{out}/english_names.tsv", english_name_normalizer.items())
tsv(f"{out}/english_compounds.tsv", english_compound_normalizer.items())
tsv(f"{out}/english_replacers.tsv", n.replacers.items())
with open(f"{out}/english_fillers.txt", "w", encoding="utf-8") as f:
    f.write(n.ignore_patterns + "\n")

texts = [
    "", "Hello, World!", "  multiple   spaces\there\nnewline ",
    "Um, I think, uh, we should go. Hmm. Mm-hmm. Ah-ha! Uhhh, errr, tsk.",
    "I won't, can't, let's, ain't, y'all wanna gotta gonna i'ma imma woulda coulda shoulda ma'am",
    "Mr Smith and Mrs Jones met Dr Who and St Paul, Prof X, Capt Y, Lt Z, Jr, Sr, Esq.",
    "He'd been there. She's been there. It'd gone. He's gone. We'd done it. She's got it.",
    "They're here, it's fine, that's it, I'd go, we'll see, I've been, I'm here, don't, isn't",
    "The meeting is at 2:00 AM, or 11:00, or 3:30 pm.",
    "twenty five", "one hundred and twenty three", "two thousand and nineteen", "nineteen ninety nine",
    "one oh one", "four oh four error", "oh no", "oh oh", "oh seven nine eight", "nineteen oh five",
    "zero point five", "three point one four one five nine", "0 point 5", "point five",
    "two and a half", "a half", "one and a half million", "and a half",
    "first second third fourth fifth twelfth twentieth twenty first one hundredth",
    "the 1960s and 70s, the 274th, 32nd and 1st", "sixes and sevens", "twenties", "thousands of hundreds",
    "$20 million", "twenty dollars", "twenty dollars and seven cents", "$2 and ¢7", "$0.07", "£5.50", "fifty pence",
    "ten percent", "ten per cent", "fifty %", "5%", "minus five", "negative three degrees", "plus two", "positive",
    "double seven", "triple zero", "double o seven", "double trouble",
    "one", "ones", "1", "1s", "number one", "the ones who know", "one two three four",
    "1,000,000 people", "1,2,3", "3.14", "192.168.0.1", "version 2.0.1", "a.b.c", "end.", "U.S.A.", "e.g. this",
    "B B C news", "5 G network", "a big cat", "i a m", "the f b i and c i a", "x ray t shirt wi fi e mail",
    "et cetera", "al right", "all right", "how ever", "o k", "ooooh", "ohhh", "a m and p m",
    "colour flavour centre theatre organise analyse travelling", "OK okay kay etcetera",
    "Alan Allan Micheal Phillip Tracey Stacey", "[noise] hello <unk> world (laughs) there [inaudible]",
    "café naïve façade Œuvre Straße þorn łódź æsir", "don ' t", "rock 'n' roll", "fifty-fifty", "well-known",
    "It's 10:00 o'clock", "a thousand", "a million dollars", "million", "hundred", "two hundred thousand",
    "three million two hundred thousand five hundred and six", "one billion and one", "twelve hundred",
    "fifteen hundred and seventy", "seven eleven", "eleven twelve", "ninety nine point nine nine percent",
    "the year two thousand", "ten to eleven", "four by four", "one on one", "covid nineteen", "mp3", "b2b",
    "24/7", "50/50", "1/2", "3rd", "21st century", "the 90's", "in '99", "100km", "5kg", "10am", "4pm",
    "Q3 2026 results were $1.2 billion, up 15% year over year.",
    "We, uh, we reported it to Lambeth council twice, and nobody came out.",
    "Section twenty one notice, HHSRS, H H S R S inspection, the housing ombudsman.",
]
norm = [{"text": t, "words": n(t).split()} for t in texts]

pairs = [
    ("tonight", "to night"), ("to night", "tonight"), ("white paper", "whitepaper"), ("whitepaper", "white paper"),
    ("a b c", "abc"), ("the cat sat", "the cat sat"), ("the cat sat", "the dog sat"), ("the cat sat", "cat sat"),
    ("the cat sat", "the big cat sat on"), ("", "hello"), ("hello", ""), ("", ""),
    ("any one of them", "anyone of them"), ("neo platonists", "neoplatonists"), ("some where else", "somewhere else"),
    ("Twenty-five dollars", "$25"), ("it is ten percent", "it's 10%"), ("Mister Smith", "Mr. Smith"),
    ("I can not go", "I can't go"), ("okay so", "OK so"), ("colour", "color"),
    ("we reported it to lambeth council twice", "we reported it to lamberth council twice and nobody came"),
    ("a b c d e", "e d c b a"), ("x y z", "xy z"), ("xyz", "x yz"), ("ab ab ab", "ababab"), ("ababab", "ab ab ab"),
    ("one two three", "123"), ("hot dog stand", "hotdog stand"), ("hotdogstand", "hot dog stand"),
]
rng = random.Random(7)
vocab = "the a of and to in is it that was for on are with as he she they we you this be at by not or".split() + ["to", "night", "tonight", "any", "one", "anyone", "white", "paper", "whitepaper"]
for _ in range(60):
    r = " ".join(rng.choice(vocab) for _ in range(rng.randint(0, 25)))
    h = " ".join(rng.choice(vocab) for _ in range(rng.randint(0, 25)))
    pairs.append((r, h))
for _ in range(3):
    r = " ".join(rng.choice(vocab) for _ in range(400))
    h = " ".join(rng.choice(vocab) for _ in range(rng.randint(350, 450)))
    pairs.append((r, h))
align = []
for r, h in pairs:
    rw, hw = n(r).split(), n(h).split()
    d = edit_distance(tuple(rw), tuple(hw), merge_compounds=True)
    align.append({"ref": r, "hyp": h, "ref_words": rw, "hyp_words": hw,
                  "ins": d["ins"], "del": d["del"], "sub": d["sub"], "total": d["total"]})

with open(f"{out}/oasr_golden.json", "w", encoding="utf-8") as f:
    json.dump({"source": "open_asr_leaderboard normalizer @ 2aaa07a5ea7c5b18ea53c511426d2079b35d1200 (via voice2text utils/asr_grid/oasr), kaldialign 0.12.0 edit_distance(merge_compounds=True)",
               "normalise": norm, "align": align}, f, ensure_ascii=False, indent=1)
print(len(norm), len(align))
