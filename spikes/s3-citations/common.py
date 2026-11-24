"""Shared helpers for spike S3 (copied from S2): transcript format, prompt rendering, output schemas.

Transcript JSON (one file per transcript, in data/):
  {"id", "title", "template", "source", "speakers": {code: label},
   "lines": [{"id": "L0001", "start": s, "end": s, "speaker": code, "text": str}],
   "facts": [...], "traps": [...]}          # facts/traps only on synthetic files

S3 change: each line is rendered with its start time, `L<n> [mm:ss] <speaker>: text`, as the spike plan
asks ("lines numbered and time-stamped"). The model still cites line numbers (an integer array, enforced by
the JSON schema); after generation the harness maps each cited line to that line's {start, end} and writes
the planned product shape {section, sentence, cites: [{start, end}]} per sentence (`span_note`).
"""
import json
import pathlib

HERE = pathlib.Path(__file__).resolve().parent
DATA = HERE / "data"
TEMPLATES = HERE / "templates"
RESULTS = HERE / "results"


def load_transcript(path):
    return json.loads(pathlib.Path(path).read_text())


def load_template(name):
    return json.loads((TEMPLATES / f"{name}.json").read_text())


def line_num(line_id):
    """'L0042' -> 42."""
    return int(line_id.lstrip("L"))


def fmt_ts(seconds):
    """Seconds -> 'mm:ss' (minutes are not wrapped into hours: 61 min -> '61:05')."""
    s = int(seconds)
    return f"{s // 60:02d}:{s % 60:02d}"


def render_line(l):
    return f"L{line_num(l['id'])} [{fmt_ts(l['start'])}] {l['speaker']}: {l['text']}"


def render_lines(lines):
    return "\n".join(render_line(l) for l in lines)


def duration_min(lines):
    return (lines[-1]["end"] - lines[0]["start"]) / 60 if lines else 0.0


def sections(lines, minutes=10.0):
    """Split lines into consecutive ~`minutes` sections (by start time)."""
    out, cur, t0 = [], [], None
    for l in lines:
        if t0 is None:
            t0 = l["start"]
        if cur and l["start"] - t0 >= minutes * 60:
            out.append(cur)
            cur, t0 = [], l["start"]
        cur.append(l)
    if cur:
        out.append(cur)
    return out


# ---- output schemas (enforced via llama-server json_schema) ----
# Bounds matter: without maxItems/maxLength a small model can loop inside the grammar (the first
# smoke run generated 3,300+ tokens of citations for a 15-minute meeting).


def cited(max_len, max_cites):
    return {
        "type": "object",
        "properties": {
            "text": {"type": "string", "maxLength": max_len},
            "cites": {"type": "array", "items": {"type": "integer"}, "minItems": 1, "maxItems": max_cites},
        },
        "required": ["text", "cites"],
    }


def note_schema(tpl):
    titles = [s["title"] for s in tpl["sections"]]
    return {
        "type": "object",
        "properties": {
            "sections": {
                "type": "array",
                "minItems": len(titles),
                "maxItems": len(titles),
                "items": {
                    "type": "object",
                    "properties": {
                        "title": {"type": "string", "enum": titles},
                        "sentences": {"type": "array", "items": cited(300, 6), "maxItems": 10},
                    },
                    "required": ["title", "sentences"],
                },
            }
        },
        "required": ["sections"],
    }


def facts_schema(tpl):
    """Map output: facts tagged with the note section they belong to, so a note can also be assembled
    directly from the facts with no final LLM rewrite (the "sectioned" strategy)."""
    item = cited(180, 5)
    item["properties"]["section"] = {"type": "string",
                                     "enum": [s["title"] for s in tpl["sections"]] + ["Other"]}
    item["required"] = ["section", "text", "cites"]
    return {
        "type": "object",
        "properties": {"facts": {"type": "array", "items": item, "maxItems": 30}},
        "required": ["facts"],
    }


def sectioned_note(tpl, facts):
    """Assemble a note from section-tagged facts, in transcript order, without an LLM."""
    return {"sections": [
        {"title": s["title"],
         "sentences": [{"text": f["text"], "cites": f["cites"]} for f in facts
                       if f.get("section") == s["title"]]}
        for s in tpl["sections"]]}


# ---- prompts ----

SYSTEM = (
    "You write accurate professional records from conversation transcripts. "
    "The transcript is machine-generated from speech and may contain errors, fillers and small talk. "
    "Rules: (1) Only state what the transcript supports. Never add facts, diagnoses, decisions or advice "
    "that were not said. (2) Keep negations, uncertainty and hypotheticals exactly as spoken: 'denied X' "
    "is not 'X'; 'if X happened' is not 'X happened'; something a person heard from someone else is "
    "reported as such. (3) When a speaker corrects themselves, use the corrected value. (4) Attribute "
    "views to the person who expressed them. (5) Record disclosures of harm, abuse or risk accurately "
    "and in plain words; do not soften or omit them. (6) Every sentence must cite the transcript line "
    "numbers (the n in 'Ln') that support it, not the [mm:ss] times."
)


def speakers_block(t):
    return "\n".join(f"{k} = {v}" for k, v in t["speakers"].items())


def template_block(tpl):
    return "\n".join(f"- {s['title']}: {s['instruction']}" for s in tpl["sections"])


def single_pass_messages(t, tpl, lines):
    user = (
        f"Write a {tpl['name']} from the transcript below.\n\n"
        f"Speakers:\n{speakers_block(t)}\n\n"
        f"Sections (use these titles, in this order):\n{template_block(tpl)}\n\n"
        f"Return JSON: sections -> sentences, each with 'cites' = supporting line numbers.\n\n"
        f"TRANSCRIPT\n{render_lines(lines)}"
    )
    return [{"role": "system", "content": SYSTEM}, {"role": "user", "content": user}]


def map_messages(t, tpl, section_lines, idx, total):
    user = (
        f"This is part {idx + 1} of {total} of a longer conversation. The final record will be a "
        f"{tpl['name']} with these sections:\n{template_block(tpl)}\n\n"
        f"Speakers:\n{speakers_block(t)}\n\n"
        "List every fact from THIS part that could belong in the record: who said what, dates, "
        "numbers, names, needs, risks, concerns, disclosures, wishes, decisions, advice and actions. "
        "One fact per item, each citing its supporting line numbers and tagged with the record "
        "section it belongs to. Skip pure small talk.\n\n"
        f"TRANSCRIPT PART\n{render_lines(section_lines)}"
    )
    return [{"role": "system", "content": SYSTEM}, {"role": "user", "content": user}]


def reduce_messages(t, tpl, facts):
    fact_lines = "\n".join(
        f"- {f['text']} [cites: {', '.join(str(c) for c in f.get('cites', []))}]" for f in facts
    )
    user = (
        f"Write a {tpl['name']} from these facts, extracted in order from a conversation.\n\n"
        f"Speakers:\n{speakers_block(t)}\n\n"
        f"Sections (use these titles, in this order):\n{template_block(tpl)}\n\n"
        "Use only these facts. Keep each fact's line-number citations on the sentences you write "
        "from it. If facts conflict, prefer the later one if it is a correction.\n\n"
        f"FACTS\n{fact_lines}"
    )
    return [{"role": "system", "content": SYSTEM}, {"role": "user", "content": user}]


def salvage_json(content, list_key):
    """Recover the complete items from a JSON object whose `list_key` array was cut off by max_tokens."""
    try:
        return json.loads(content), True
    except Exception:
        pass
    cut = content.rfind("}")
    while cut > 0:
        try:
            return json.loads(content[: cut + 1] + "]}"), False
        except Exception:
            cut = content.rfind("}", 0, cut)
    return None, False


# ---- S3: sentences and time-span citations ----

def note_sentence_list(note):
    """Every note sentence in note order, with a stable id S001, S002, ... used by packets, verdicts and flags."""
    out = []
    for sec in (note or {}).get("sections", []):
        for s in sec.get("sentences", []):
            out.append({"sid": f"S{len(out) + 1:03d}", "section": sec.get("title", ""),
                        "text": s.get("text", ""), "cites": list(s.get("cites", []))})
    return out


def span_note(note, lines):
    """The planned product shape: one {section, sentence, cites: [{start, end}]} per sentence.

    `lines` keeps the cited line numbers; a cite to a line that doesn't exist gets no span and is listed
    in `invalid_cites`."""
    by_num = {line_num(l["id"]): l for l in lines}
    out = []
    for s in note_sentence_list(note):
        valid = [c for c in s["cites"] if c in by_num]
        rec = {"id": s["sid"], "section": s["section"], "sentence": s["text"],
               "cites": [{"start": by_num[c]["start"], "end": by_num[c]["end"]} for c in valid],
               "lines": s["cites"]}
        bad = [c for c in s["cites"] if c not in by_num]
        if bad:
            rec["invalid_cites"] = bad
        out.append(rec)
    return out
