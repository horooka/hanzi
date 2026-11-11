#!/usr/bin/env python3
"""Build the HSK 3.0 deck: 899 characters + ~1650 multi-character words.

Sources:
  * ``tools/hsk30-1-3-chars.txt``             character readings and levels
  * HSK 3.0 word lists (levels 1-3)            words and readings
  * CC-CEDICT                                  English glosses and traditional forms

Run:

    python tools/build_hsk30_vocab.py \
      --words-csv /tmp/hsk30.csv \
      --cedict /tmp/cedict.txt

The output ``data/vocab.json`` (version 4) is embedded in the app. Custom lists
are stored in SQLite, not here, so re-running this script never touches them.
"""

from __future__ import annotations

import argparse
import csv
import json
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parents[1]
CHARS = ROOT / "tools" / "hsk30-1-3-chars.txt"
OUTPUT = ROOT / "data" / "vocab.json"

CJK = re.compile(r"[\u4e00-\u9fff]")
CEDICT_LINE = re.compile(r"^(\S+)\s+(\S+)\s+\[([^\]]+)\]\s+/(.*)/\s*$")
SKIP_DEF = re.compile(
    r"^(variant of|old variant of|erhua variant of|erhua form of|used in|see |also written|"
    r"also pr\.|Japanese variant|abbr\. for|CL:)",
    re.IGNORECASE,
)
MARKER_PAREN = re.compile(
    r"\((?:bound form|literary|archaic|in data tables|Taiwan pr\.[^)]*|old|coll\.)\)\s*",
    re.IGNORECASE,
)
VARIANT = re.compile(
    r"^(old variant of|variant of|erhua variant of|erhua form of)\s+([^\[]+)\[",
    re.IGNORECASE,
)
REFERENCE = re.compile(r"^(used in|see)\s+([^\[]+)\[", re.IGNORECASE)


def cap_meaning(text: str, limit: int = 70) -> str:
    if len(text) <= limit:
        return text
    cut = text.rfind(" ", 0, limit - 3)
    return text[: cut if cut > 30 else limit - 1] + "…"


def variant_meaning(
    defs: list[str],
    cedict: dict[str, list[tuple[tuple[str, ...], list[str], str, bool]]],
    word: str,
) -> str:
    """Resolve a reference to the base word's meaning.

    The relation itself is not appended: in quiz/write prompts a note such as
    "used in 漂亮" works as an unwanted hint.
    """
    cleaned = clean_defs(defs)
    if cleaned:
        meaning = short_meaning(cleaned)
        self_reference = re.compile(
            rf"\s*\((?:old )?variant of {re.escape(word)}\)"
        )
        return self_reference.sub("", meaning).strip()

    for text in defs:
        match = VARIANT.match(text)
        if not match:
            continue
        base = match.group(2).strip().split("|")[-1].strip()
        if not base or base == word:
            continue
        entries = cedict.get(base)
        if not entries:
            continue
        base_defs = clean_defs(entries[0][1])
        if base_defs:
            return short_meaning(base_defs)

    # "used in X" / "see X" references carry no meaning of their own.
    for text in defs:
        match = REFERENCE.match(text)
        if not match:
            continue
        base = match.group(2).strip().split("|")[-1].strip()
        if not base or base == word:
            continue
        entries = cedict.get(base)
        if not entries:
            continue
        base_defs = clean_defs(entries[0][1])
        if base_defs:
            return short_meaning(base_defs)

    return "—"

# Accented vowel -> (base vowel, tone digit); ü is written v in numbered pinyin.
TONE_MAP = {
    "ā": ("a", 1), "á": ("a", 2), "ǎ": ("a", 3), "à": ("a", 4),
    "ē": ("e", 1), "é": ("e", 2), "ě": ("e", 3), "è": ("e", 4),
    "ī": ("i", 1), "í": ("i", 2), "ǐ": ("i", 3), "ì": ("i", 4),
    "ō": ("o", 1), "ó": ("o", 2), "ǒ": ("o", 3), "ò": ("o", 4),
    "ū": ("u", 1), "ú": ("u", 2), "ǔ": ("u", 3), "ù": ("u", 4),
    "ǖ": ("v", 1), "ǘ": ("v", 2), "ǚ": ("v", 3), "ǜ": ("v", 4),
    "ü": ("v", 0),
}

# Words missing from CC-CEDICT (or with mismatched pinyin): manual fallback.
WORD_OVERRIDES = {
    "车上": ("che1 shang4", "on the vehicle; in the car", "車上"),
    "是不是": ("shi4 bu5 shi4", "is it or not; is it?", "是不是"),
    "不太": ("bu4 tai4", "not very; not too", "不太"),
    "不一会儿": ("bu4 yi1 hui4 r5", "in a moment; shortly", "不一會兒"),
    "见过": ("jian4 guo5", "have seen (before)", "見過"),
    "送到": ("song4 dao4", "deliver to; send to", "送到"),
    "这时候": ("zhe4 shi2 hou5", "at this time; now", "這時候"),
    "放到": ("fang4 dao4", "put onto; put in", "放到"),
    "能不能": ("neng2 bu5 neng2", "can or cannot; is that possible?", "能不能"),
}


def to_numbered(syllable: str) -> str:
    tone = 5
    out = []
    for char in syllable:
        if char in TONE_MAP:
            base, digit = TONE_MAP[char]
            out.append(base)
            if digit:
                tone = digit
        else:
            out.append(char)
    return "".join(out) + str(tone)


def normalize_pinyin(pinyin: str) -> tuple[str, ...]:
    return tuple(
        part.lower().replace("u:", "v").replace("ü", "v") for part in pinyin.split()
    )


def load_cedict(
    path: pathlib.Path,
) -> dict[str, list[tuple[tuple[str, ...], list[str], str, bool]]]:
    by_word: dict[str, list[tuple[tuple[str, ...], list[str], str, bool]]] = {}
    with path.open(encoding="utf-8") as handle:
        for line in handle:
            if line.startswith("#"):
                continue
            match = CEDICT_LINE.match(line)
            if not match:
                continue
            traditional, simplified, pinyin, raw_defs = match.groups()
            defs = [part.strip() for part in raw_defs.split("/") if part.strip()]
            proper = any(part[:1].isupper() for part in pinyin.split())
            by_word.setdefault(simplified, []).append(
                (normalize_pinyin(pinyin), defs, traditional, proper)
            )
    return by_word


def clean_defs(defs: list[str]) -> list[str]:
    cleaned: list[str] = []
    for text in defs:
        if re.match(r"^surname\s+[A-Z]", text):
            continue
        if SKIP_DEF.match(text):
            continue
        text = MARKER_PAREN.sub("", text).strip()
        text = re.sub(r"\{[^}]*\}", "", text)
        text = re.sub(r"\[[^\]]*\]", "", text)
        text = re.sub(r"[\u4e00-\u9fff]+\|([\u4e00-\u9fff]+)", r"\1", text)
        text = re.sub(r"\s{2,}", " ", text).strip()
        text = text.rstrip(" ;,")
        if text and text not in cleaned:
            cleaned.append(text)
    return cleaned


def short_meaning(defs: list[str], limit: int = 70) -> str:
    cleaned = clean_defs(defs) or defs
    meaning = "; ".join(cleaned[:2]) if cleaned else "—"
    return cap_meaning(meaning, limit)


def with_erhua_note(meaning: str) -> str:
    """Mark erhua forms without naming the base word (no quiz hints)."""
    if meaning == "—":
        return meaning
    base = meaning.rstrip("…").strip()
    return cap_meaning(f"{cap_meaning(base, 55)} (erhua form)")


def pick_cedict(
    word: str,
    syllables: tuple[str, ...],
    cedict: dict[str, list[tuple[tuple[str, ...], list[str], str, bool]]],
) -> tuple[tuple[str, ...], list[str], str] | None:
    entries = cedict.get(word, [])
    if not entries:
        return None
    matches = [entry for entry in entries if entry[0] == syllables]
    matches.sort(key=lambda entry: entry[3])  # prefer common words over names
    candidates = matches or entries
    # CEDICT sometimes lists a "variant of" entry before the real one. Prefer
    # any entry that yields a usable gloss.
    for entry in candidates:
        if clean_defs(entry[1]):
            return entry[0], entry[1], entry[2]
    return candidates[0][0], candidates[0][1], candidates[0][2]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--chars", type=pathlib.Path, default=CHARS)
    parser.add_argument("--words-csv", type=pathlib.Path, required=True)
    parser.add_argument("--cedict", type=pathlib.Path, required=True)
    parser.add_argument("--output", type=pathlib.Path, default=OUTPUT)
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()

    cedict = load_cedict(args.cedict)

    # --- characters -----------------------------------------------------
    char_rows = []
    for line in args.chars.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split()
        char_rows.append((parts[0], parts[1], int(parts[2])))

    char_entries = []
    char_ids_by_level: dict[int, list[str]] = {1: [], 2: [], 3: []}
    used_char_ids: dict[str, int] = {}
    for char, accented, level in char_rows:
        numbered = to_numbered(accented)
        used_char_ids[numbered] = used_char_ids.get(numbered, 0) + 1
        entry_id = (
            numbered
            if used_char_ids[numbered] == 1
            else f"{numbered}-{ord(char):x}"
        )
        chosen = pick_cedict(char, normalize_pinyin(numbered), cedict)
        if chosen:
            _pinyin, defs, traditional = chosen
        else:
            defs, traditional = [], char
        char_entries.append(
            {
                "id": entry_id,
                "simplified": char,
                "traditional": traditional,
                "pinyin": numbered,
                "meaning": variant_meaning(defs, cedict, char),
                "hsk": level,
            }
        )
        char_ids_by_level[level].append(entry_id)

    # --- words ----------------------------------------------------------
    rows = [
        row
        for row in csv.DictReader(args.words_csv.open(encoding="utf-8-sig"))
        if row["Level"] in ("1", "2", "3")
    ]

    def clean_word(raw: str) -> str:
        word = raw.split("|")[0]
        word = re.sub(r"（[^）]*）", "", word)
        word = re.sub(r"\d+$", "", word)
        return "".join(CJK.findall(word))

    word_entries = []
    word_ids_by_level: dict[int, list[str]] = {1: [], 2: [], 3: []}
    seen_words: set[str] = set()
    used_word_ids: dict[str, int] = {}
    skipped: list[str] = []
    for row in rows:
        word = clean_word(row["Simplified"])
        if len(word) < 2 or word in seen_words:
            continue
        seen_words.add(word)
        level = int(row["Level"])

        bracket = re.search(r"\[([^\]]+)\]", row["CEDICT"])
        syllables = (
            normalize_pinyin(bracket.group(1)) if bracket else ()
        )
        traditional = ""
        if len(syllables) != len(word) or not syllables:
            override = WORD_OVERRIDES.get(word)
            if not override:
                skipped.append(word)
                continue
            pinyin, meaning, traditional = override
            syllables = tuple(pinyin.split())
        else:
            chosen = pick_cedict(word, syllables, cedict)
            if not chosen:
                override = WORD_OVERRIDES.get(word)
                if not override:
                    skipped.append(word)
                    continue
                pinyin, meaning, traditional = override
                syllables = tuple(pinyin.split())
            else:
                _pinyin, defs, traditional = chosen
                meaning = variant_meaning(defs, cedict, word)

        if syllables and syllables[-1] == "r5":
            meaning = with_erhua_note(meaning)

        pinyin_key = "-".join(syllables)
        used_word_ids[pinyin_key] = used_word_ids.get(pinyin_key, 0) + 1
        entry_id = (
            pinyin_key
            if used_word_ids[pinyin_key] == 1
            else f"{pinyin_key}-{''.join(f'{ord(c):x}' for c in word)}"
        )
        word_entries.append(
            {
                "id": entry_id,
                "simplified": word,
                "traditional": traditional or word,
                "pinyin": " ".join(syllables),
                "meaning": meaning,
                "hsk": level,
            }
        )
        word_ids_by_level[level].append(entry_id)

    groups = []
    for level in (1, 2, 3):
        groups.append(
            {
                "id": f"hsk30-level-{level}",
                "name": f"HSK 3.0 · Level {level}",
                "entries": char_ids_by_level[level],
            }
        )
        groups.append(
            {
                "id": f"hsk30-level-{level}-words",
                "name": f"HSK 3.0 · Level {level} words",
                "entries": word_ids_by_level[level],
            }
        )
    groups.append(
        {
            "id": "hsk30-all",
            "name": "HSK 3.0 · Levels 1–3",
            "entries": [entry["id"] for entry in char_entries],
        }
    )
    groups.append(
        {
            "id": "hsk30-all-words",
            "name": "HSK 3.0 · Words 1–3",
            "entries": [entry["id"] for entry in word_entries],
        }
    )

    entries = char_entries + word_entries
    # Bump this whenever the deck content changes: the app only reseeds its
    # SQLite copy when the version differs.
    vocab = {
        "version": 8,
        "name": "HSK 3.0 · Levels 1–3",
        "groups": groups,
        "entries": entries,
    }

    meanings = [entry["meaning"] for entry in entries]
    print(f"characters: {len(char_entries)}")
    for level in (1, 2, 3):
        print(f"  level {level}: {len(char_ids_by_level[level])} chars, "
              f"{len(word_ids_by_level[level])} words")
    print(f"words: {len(word_entries)} (skipped {len(skipped)}: {', '.join(skipped)})")
    print(f"entries: {len(entries)}, groups: {len(groups)}")
    print(f"distinct meanings: {len(set(meanings))}")
    print(f"longest meaning: {max(len(m) for m in meanings)} chars")
    print(f"missing glosses: {sum(1 for m in meanings if m == '—')}")
    bare_refs = [
        entry for entry in entries
        if entry["meaning"].startswith(("variant of", "old variant of", "erhua variant of", "erhua form of", "see ", "used in"))
    ]
    if bare_refs:
        print(f"reference-only meanings: {len(bare_refs)}")
        for entry in bare_refs[:10]:
            print(f"  {entry['simplified']}: {entry['meaning']}")

    if args.dry_run:
        print("dry run: nothing written")
        return 0

    args.output.write_text(
        json.dumps(vocab, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    print(f"wrote {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
