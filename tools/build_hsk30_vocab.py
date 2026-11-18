#!/usr/bin/env python3
"""Build the HSK 3.0 (2025 syllabus) deck.

Sources:
  * ``tools/hsk30-2025-chars.txt``      recognition characters, readings, levels
  * HSK 3.0 2025 word list (CSV)        words, pinyin, levels, glosses
  * CC-CEDICT                           glosses and traditional forms for characters
  * ``tools/legacy-ids.json``           entry IDs from the 2021 deck

Run:

    python tools/build_hsk30_vocab.py \
      --words-csv /tmp/hsk30-2025.csv \
      --cedict /tmp/cedict.txt \
      --levels 1-5

The output ``data/vocab.json`` is embedded in the app.  Custom groups and
mastery live in SQLite, so re-running this script never touches them.  IDs
from the previous deck are reused by simplified form, which keeps existing
mastery when words move between levels.
"""

from __future__ import annotations

import argparse
import collections
import csv
import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
CHARS = ROOT / "tools" / "hsk30-2025-chars.txt"
LEGACY_IDS = ROOT / "tools" / "legacy-ids.json"
OUTPUT = ROOT / "data" / "vocab.json"

CJK = re.compile(r"[\u4e00-\u9fff]")
CEDICT_LINE = re.compile(r"^(\S+)\s+(\S+)\s+\[([^\]]+)\]\s+/(.*)/\s*$")
NUMBERED_SYLLABLE = re.compile(r"^[a-z]+[1-5]$")
SKIP_DEF = re.compile(
    r"^(variant of|old variant of|erhua variant of|erhua form of|used in|see |"
    r"also written|also pr\.|Japanese variant|abbr\. for|CL:|Taiwan pr\.)",
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


def clean_word(raw: str) -> str:
    word = str(raw).split("|")[0]
    word = re.sub(r"（[^）]*）", "", word)
    word = re.sub(r"\([^)]*\)", "", word)
    word = re.sub(r"\d+$", "", word)
    return "".join(CJK.findall(word))


def level_rank(level: str) -> int:
    return 7 if level == "7-9" else int(level)


def parse_levels(text: str) -> list[int]:
    levels: set[int] = set()
    for part in text.split(","):
        part = part.strip()
        if not part:
            continue
        if "-" in part:
            start, end = part.split("-", 1)
            levels.update(range(int(start), int(end) + 1))
        else:
            levels.add(int(part))
    return sorted(levels)


def cap_meaning(text: str, limit: int = 70) -> str:
    text = " ".join(text.split())
    if len(text) <= limit:
        return text
    cut = text.rfind(" ", 0, limit - 3)
    if cut < 30:
        cut = limit - 1
    result = text[:cut].rstrip(" ;,")
    # Do not leave an unfinished parenthetical at the cut.
    while result.count("(") > result.count(")"):
        result = result[: result.rfind("(")].rstrip(" ;,")
    return result + "…"


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


def normalize_pinyin(pinyin: str) -> tuple[str, ...]:
    return tuple(
        part.lower().replace("u:", "v").replace("ü", "v") for part in pinyin.split()
    )


def clean_defs(defs: list[str]) -> list[str]:
    cleaned: list[str] = []
    for text in defs:
        if re.match(r"^surname\s+[A-Z]", text):
            continue
        if SKIP_DEF.match(text):
            continue
        text = MARKER_PAREN.sub("", text).strip()
        text = re.sub(r"\s*\(CL:[^)]*\)", "", text)
        text = re.sub(r"[;, ]+Taiwan pr\..*$", "", text, flags=re.IGNORECASE)
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


def with_erhua_note(meaning: str) -> str:
    """Mark erhua forms without naming the base word (no quiz hints)."""
    if meaning == "—" or "(erhua form)" in meaning:
        return meaning
    base = meaning.rstrip("…").strip()
    return cap_meaning(f"{cap_meaning(base, 55)} (erhua form)")


def strip_leading_note(text: str) -> str:
    """Drop a leading usage note when a real gloss follows it."""
    text = " ".join(text.split())
    if not text.startswith("("):
        return text
    depth = 0
    for index, char in enumerate(text):
        if char == "(":
            depth += 1
        elif char == ")":
            depth -= 1
            if depth == 0:
                rest = text[index + 1 :].strip(" ;,")
                return rest or text
    return text


WORD_MEANING_OVERRIDES = {
    # CEDICT only has a usage note for this one.
    "大厦": "large building; mansion; high-rise",
}

CHAR_MEANING_OVERRIDES = {
    # CEDICT's first usable entry is a proper name or a reference note.
    "巴": "bus (in 大巴); suffix (in 尾巴, 嘴巴)",
    "啡": "coffee (in 咖啡)",
    "构": "to construct; to form; structure",
    "葡": "grape (in 葡萄)",
    "圾": "garbage; rubbish (in 垃圾)",
    "垃": "garbage; rubbish (in 垃圾)",
    "奈": "helpless; to bear; how can one help",
    "璃": "glass; glaze",
    "筑": "to build; to construct",
    "戚": "relatives; kin; grief",
}


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
    for entry in candidates:
        if clean_defs(entry[1]):
            return entry[0], entry[1], entry[2]
    return candidates[0][0], candidates[0][1], candidates[0][2]


class IdAllocator:
    """Reuse IDs from the 2021 deck, then generate stable pinyin IDs."""

    def __init__(self, legacy: dict[str, str]) -> None:
        self.legacy = legacy
        self.used: set[str] = set(legacy.values())
        self.reused = 0
        self.generated = 0

    def for_entry(self, simplified: str, pinyin: str, is_word: bool) -> str:
        legacy = self.legacy.get(simplified)
        if legacy is not None:
            self.reused += 1
            return legacy

        base = pinyin.replace(" ", "-") if is_word else pinyin
        candidate = base
        if candidate in self.used:
            suffix = (
                "".join(f"{ord(char):x}" for char in simplified)
                if is_word
                else f"{ord(simplified):x}"
            )
            candidate = f"{base}-{suffix}"
            index = 2
            while candidate in self.used:
                candidate = f"{base}-{suffix}-{index}"
                index += 1
        self.used.add(candidate)
        self.generated += 1
        return candidate


def read_char_table(
    path: pathlib.Path, levels: set[int]
) -> tuple[list[tuple[str, str, int]], list[str]]:
    rows: list[tuple[str, str, int]] = []
    problems: list[str] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.split("#", 1)[0].strip()
        if not line:
            continue
        parts = line.split()
        if len(parts) < 3:
            problems.append(f"bad character row: {line}")
            continue
        char, pinyin, raw_level = parts[0], parts[1], parts[2]
        level = level_rank(raw_level)
        if level not in levels:
            continue
        if not NUMBERED_SYLLABLE.fullmatch(pinyin):
            problems.append(f"bad character reading: {line}")
            continue
        rows.append((char, pinyin, level))
    return rows, problems


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--words-csv", type=pathlib.Path, required=True)
    parser.add_argument("--chars", type=pathlib.Path, default=CHARS)
    parser.add_argument("--cedict", type=pathlib.Path, required=True)
    parser.add_argument("--legacy-ids", type=pathlib.Path, default=LEGACY_IDS)
    parser.add_argument("--levels", default="1-5")
    parser.add_argument("--version", type=int, default=9)
    parser.add_argument("--output", type=pathlib.Path, default=OUTPUT)
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()

    levels = parse_levels(args.levels)
    if not levels:
        print("no levels selected", file=sys.stderr)
        return 1

    cedict = load_cedict(args.cedict)
    legacy = (
        json.loads(args.legacy_ids.read_text(encoding="utf-8"))
        if args.legacy_ids.exists()
        else {}
    )
    allocator = IdAllocator(legacy)

    char_rows, char_problems = read_char_table(args.chars, set(levels))
    if char_problems:
        for problem in char_problems:
            print(f"warning: {problem}", file=sys.stderr)

    word_rows = list(csv.DictReader(args.words_csv.open(encoding="utf-8-sig")))

    # Single-character words can supply a gloss when CEDICT has no useful one
    # for the character itself.
    single_defs: dict[str, tuple[str, str]] = {}
    for row in word_rows:
        level = level_rank(row["level"])
        if level not in levels:
            continue
        word = clean_word(row["simplified"])
        if len(word) == 1 and word not in single_defs:
            single_defs[word] = (
                row["definitions"].strip(),
                row["traditional"].strip() or word,
            )

    char_entries = []
    char_ids_by_level: dict[int, list[str]] = {level: [] for level in levels}
    for char, pinyin, level in char_rows:
        chosen = pick_cedict(char, tuple(pinyin.split()), cedict)
        if chosen:
            _pinyin, defs, traditional = chosen
            meaning = variant_meaning(defs, cedict, char)
        else:
            traditional = ""
            meaning = "—"

        if meaning == "—" and char in single_defs:
            fallback_meaning, fallback_traditional = single_defs[char]
            meaning = cap_meaning(fallback_meaning)
            traditional = traditional or fallback_traditional
        meaning = CHAR_MEANING_OVERRIDES.get(char, meaning)

        entry = {
            "id": allocator.for_entry(char, pinyin, False),
            "simplified": char,
            "traditional": traditional or char,
            "pinyin": pinyin,
            "meaning": meaning,
            "hsk": level,
        }
        char_entries.append(entry)
        char_ids_by_level[level].append(entry["id"])

    word_entries = []
    word_ids_by_level: dict[int, list[str]] = {level: [] for level in levels}
    seen_words: set[str] = set()
    skipped: list[str] = []
    bad_pinyin: list[str] = []
    for row in word_rows:
        level = level_rank(row["level"])
        if level not in levels:
            continue
        word = clean_word(row["simplified"])
        if len(word) < 2 or word in seen_words:
            continue
        seen_words.add(word)

        syllables = [to_numbered(syllable.lower()) for syllable in row["pinyin"].split()]
        if not syllables or any(
            not NUMBERED_SYLLABLE.fullmatch(syllable) for syllable in syllables
        ):
            bad_pinyin.append(f"{word}: {row['pinyin']!r}")
            continue

        pinyin = " ".join(syllables)
        meaning = WORD_MEANING_OVERRIDES.get(
            word, strip_leading_note(row["definitions"].strip())
        )
        meaning = cap_meaning(meaning)
        if syllables[-1] == "r5":
            meaning = with_erhua_note(meaning)

        entry = {
            "id": allocator.for_entry(word, pinyin, True),
            "simplified": word,
            "traditional": row["traditional"].strip() or word,
            "pinyin": pinyin,
            "meaning": meaning,
            "hsk": level,
        }
        word_entries.append(entry)
        word_ids_by_level[level].append(entry["id"])

    if bad_pinyin:
        print(
            "error: could not convert pinyin for: " + ", ".join(bad_pinyin),
            file=sys.stderr,
        )
        return 1
    if skipped:
        print(f"skipped words: {len(skipped)}: {', '.join(skipped)}", file=sys.stderr)

    def level_name(level: int) -> str:
        return "7–9" if level == 7 else str(level)

    first = levels[0]
    last = levels[-1]
    combined_name = (
        f"Level {level_name(first)}"
        if first == last
        else f"Levels {level_name(first)}–{level_name(last)}"
    )

    groups = []
    for level in levels:
        groups.append(
            {
                "id": f"hsk30-level-{level}",
                "name": f"HSK 3.0 · Level {level_name(level)}",
                "entries": char_ids_by_level[level],
            }
        )
        groups.append(
            {
                "id": f"hsk30-level-{level}-words",
                "name": f"HSK 3.0 · Level {level_name(level)} words",
                "entries": word_ids_by_level[level],
            }
        )
    groups.append(
        {
            "id": "hsk30-all",
            "name": f"HSK 3.0 · {combined_name}",
            "entries": [entry["id"] for entry in char_entries],
        }
    )
    groups.append(
        {
            "id": "hsk30-all-words",
            "name": f"HSK 3.0 · Words {level_name(first)}–{level_name(last)}",
            "entries": [entry["id"] for entry in word_entries],
        }
    )

    entries = char_entries + word_entries
    vocab = {
        "version": args.version,
        "name": f"HSK 3.0 · {combined_name}",
        "groups": groups,
        "entries": entries,
    }

    ids = [entry["id"] for entry in entries]
    simplified = [entry["simplified"] for entry in entries]
    if len(ids) != len(set(ids)):
        print("error: duplicate entry IDs", file=sys.stderr)
        return 1
    if len(simplified) != len(set(simplified)):
        print("error: duplicate simplified forms", file=sys.stderr)
        return 1
    known = set(ids)
    for group in groups:
        unknown = [entry_id for entry_id in group["entries"] if entry_id not in known]
        if unknown:
            print(
                f"error: group {group['id']} references {len(unknown)} unknown entries",
                file=sys.stderr,
            )
            return 1

    meanings = [entry["meaning"] for entry in entries]
    print(f"characters: {len(char_entries)}")
    for level in levels:
        print(
            f"  level {level_name(level)}: {len(char_ids_by_level[level])} chars, "
            f"{len(word_ids_by_level[level])} words"
        )
    print(f"words: {len(word_entries)}")
    print(f"entries: {len(entries)}, groups: {len(groups)}")
    print(f"IDs reused: {allocator.reused}, generated: {allocator.generated}")
    print(f"distinct meanings: {len(set(meanings))}")
    print(f"longest meaning: {max(len(meaning) for meaning in meanings)} chars")
    print(f"missing glosses: {sum(1 for meaning in meanings if meaning == '—')}")
    bare_refs = [
        entry
        for entry in entries
        if entry["meaning"].startswith(
            (
                "variant of",
                "old variant of",
                "erhua variant of",
                "erhua form of",
                "see ",
                "used in",
            )
        )
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
