"""Shared helpers for spike S2: transcript format, prompt rendering, output schemas.

Transcript JSON (one file per transcript, in data/):
  {"id", "title", "template", "source", "speakers": {code: label},
   "lines": [{"id": "L0001", "start": s, "end": s, "speaker": code, "text": str}],
   "facts": [...], "traps": [...]}          # facts/traps only on synthetic files

The model never sees timestamps. Each line is rendered as `L<n> <speaker>: text`,
and the model cites line numbers. The harness maps a cited line to its [start, end]
time span, which is how the product anchors citations (see the PRD, "Source-linked
verification").
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


def render_lines(lines):
    return "\n".join(f"L{line_num(l['id'])} {l['speaker']}: {l['text']}" for l in lines)


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
    "numbers (the n in 'Ln') that support it."
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


# ---- reduce v2 (after the screen showed v1 copying facts in order until the per-section cap) ----

def note_schema_v2(tpl):
    s = note_schema(tpl)
    s["properties"]["sections"]["items"]["properties"]["sentences"] = {
        "type": "array", "items": cited(350, 8), "maxItems": 14}
    return s


def reduce_messages_v2(t, tpl, facts):
    groups = []
    for sec in tpl["sections"] + [{"title": "Other"}]:
        fs = [f for f in facts if f.get("section") == sec["title"]]
        if fs:
            groups.append(f"## {sec['title']}\n" + "\n".join(
                f"- {f['text']} [{', '.join(str(c) for c in f.get('cites', []))}]" for f in fs))
    user = (
        f"Write a concise {tpl['name']} from the facts below. They were extracted from a whole "
        "conversation and grouped by the section they belong to.\n\n"
        f"Speakers:\n{speakers_block(t)}\n\n"
        f"Sections (use these titles, in this order):\n{template_block(tpl)}\n\n"
        "For each section, merge related facts into clear sentences that cover the WHOLE conversation, "
        "from start to end. Keep specifics: dates, amounts, names, medications, who does what by when. "
        "Keep every risk, disclosure, denial and wish. Leave out procedural chatter. Put the numbers "
        "from [..] into 'cites', not into the text. Use facts from 'Other' only if they matter. If facts "
        "conflict, use the later one when it is a correction.\n\n"
        "FACTS\n" + "\n\n".join(groups)
    )
    return [{"role": "system", "content": SYSTEM}, {"role": "user", "content": user}]
