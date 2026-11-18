#!/usr/bin/env python3
"""Generate the HSK 3.0 2025 character table used by the deck builder.

The 2025 syllabus publishes recognition characters with levels, but not a
single citation reading per character.  This script derives one:

  * characters already in the hand-checked 2021 table keep that reading;
  * every other character uses the most frequent reading in the 2025 words at
    or below the character's level;
  * a small override table fixes neutral-tone and polyphonic cases.

Output format, one character per line:

    <char> <numbered pinyin> <level> [# alternatives: ...]

Run:

    python tools/build_hsk30_chars.py \
      --words-csv /tmp/hsk30-2025.csv \
      --chars-tsv /tmp/hanzi_recognition.tsv \
      --curated tools/hsk30-1-3-chars.txt \
      --output tools/hsk30-2025-chars.txt
"""

from __future__ import annotations

import argparse
import collections
import csv
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parents[1]
CURATED = ROOT / "tools" / "hsk30-1-3-chars.txt"
OUTPUT = ROOT / "tools" / "hsk30-2025-chars.txt"

CJK = re.compile(r"[\u4e00-\u9fff]")

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

# The automatic rules are right for these, but they are neutral-tone or
# polyphonic characters where the HSK usage is unambiguous.
OVERRIDES = {
    # Sentence particle; not the interjection yā.
    "呀": "ya5",
    # Neutral-tone syllables in one common word; keep the citation tone.
    "朵": "duo3",  # 耳朵 ěr duo
    "膊": "bo2",   # 胳膊 gē bo
    "傅": "fu4",   # 师傅 shī fu
    "呼": "hu1",   # 招呼 zhāo hu
    "户": "hu4",   # 窗户 chuāng hu
    "闹": "nao4",  # 热闹 rè nao
    "泼": "po1",   # 活泼 huó po
    "戚": "qi1",   # 亲戚 qīn qi
    "拾": "shi2",  # 收拾 shōu shi
    "嗽": "sou4",  # 咳嗽 ké sou
    "萄": "tao2",  # 葡萄 pú tao
    "扮": "ban4",  # 打扮 dǎ ban
    "腐": "fu3",   # 豆腐 dòu fu
    "糊": "hu2",   # 糊涂 hú tu
    "辑": "ji2",   # 编辑 biān jí
    "璃": "li2",   # 玻璃 bō li
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


def load_curated(path: pathlib.Path) -> dict[str, str]:
    readings: dict[str, str] = {}
    if not path.exists():
        return readings
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split()
        readings[parts[0]] = to_numbered(parts[1].lower())
    return readings


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--words-csv", type=pathlib.Path, required=True)
    parser.add_argument("--chars-tsv", type=pathlib.Path, required=True)
    parser.add_argument("--curated", type=pathlib.Path, default=CURATED)
    parser.add_argument("--output", type=pathlib.Path, default=OUTPUT)
    args = parser.parse_args()

    rows = list(
        csv.DictReader(args.words_csv.open(encoding="utf-8-sig"))
    )
    char_rows = list(
        csv.DictReader(args.chars_tsv.open(encoding="utf-8-sig"), delimiter="\t")
    )

    levels = {
        row["word"]: level_rank(row["examLevelId"].replace("HSK", ""))
        for row in char_rows
    }
    curated = load_curated(args.curated)

    readings: dict[str, collections.Counter[str]] = collections.defaultdict(
        collections.Counter
    )
    by_level: dict[int, dict[str, collections.Counter[str]]] = collections.defaultdict(
        lambda: collections.defaultdict(collections.Counter)
    )
    for row in rows:
        level = level_rank(row["level"])
        word = clean_word(row["simplified"])
        syllables = row["pinyin"].split()
        if len(word) != len(syllables):
            continue
        for char, syllable in zip(word, syllables):
            numbered = to_numbered(syllable.lower())
            readings[char][numbered] += 1
            by_level[level][char][numbered] += 1

    def pick(char: str) -> tuple[str | None, list[tuple[str, int]]]:
        level = levels[char]
        counts: collections.Counter[str] = collections.Counter()
        for step in range(1, level + 1):
            counts.update(by_level[step].get(char, {}))
        if not counts:
            counts = readings[char]
        if not counts:
            return None, []
        ranked = counts.most_common()
        if len(ranked) > 1 and ranked[0][1] == ranked[1][1]:
            ranked.sort(key=lambda item: (-item[1], -readings[char][item[0]], item[0]))
        return ranked[0][0], ranked

    lines: list[str] = []
    ambiguous: list[tuple[str, str, list[tuple[str, int]]]] = []
    missing: list[str] = []
    for row in char_rows:
        char = row["word"]
        level = row["examLevelId"].replace("HSK", "")
        ranked: list[tuple[str, int]] = []
        if char in OVERRIDES:
            pinyin = OVERRIDES[char]
        elif char in curated:
            pinyin = curated[char]
        else:
            pinyin, ranked = pick(char)
            if pinyin is None:
                missing.append(char)
                pinyin = "?"
            elif len(ranked) > 1:
                ambiguous.append((char, level, ranked))

        note = ""
        if len(ranked) > 1:
            note = " # alternatives: " + ", ".join(
                f"{reading}({count})" for reading, count in ranked[1:3]
            )
        lines.append(f"{char} {pinyin} {level}{note}")

    args.output.write_text(
        "# HSK 3.0 (2025 syllabus) recognition characters, 3,088 characters.\n"
        "# Format: <char> <numbered pinyin> <level> [# alternatives]\n"
        "# Generated by tools/build_hsk30_chars.py; see its docstring for the\n"
        "# reading rules. Levels follow the 2025 recognition character list.\n"
        + "\n".join(lines)
        + "\n",
        encoding="utf-8",
    )

    total = collections.Counter(row["examLevelId"] for row in char_rows)
    print(f"characters: {len(char_rows)} {dict(total)}")
    print(f"curated readings reused: {sum(1 for c in levels if c in curated)}")
    print(f"generated with alternatives: {len(ambiguous)}")
    if missing:
        print(f"missing readings: {len(missing)}: {', '.join(missing)}")
    print(f"wrote {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
