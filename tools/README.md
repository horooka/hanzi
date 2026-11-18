# tools

## Vocabulary deck

`data/vocab.json` is the HSK 3.0 (2025 syllabus) deck for levels 1–5. It is
generated from the 2025 vocabulary list, the recognition-character list, and
CC-CEDICT:

```sh
python tools/build_hsk30_chars.py \
  --words-csv /tmp/hsk30-2025.csv \
  --chars-tsv /tmp/hanzi_recognition.tsv

python tools/build_hsk30_vocab.py \
  --words-csv /tmp/hsk30-2025.csv \
  --cedict /tmp/cedict.txt \
  --levels 1-5
```

Download the sources with:

```sh
curl -L -o /tmp/hsk30-2025.csv \
  https://raw.githubusercontent.com/bezlant/hsk-3-vocabulary/main/hsk-3.0-vocabulary.csv
curl -L -o /tmp/hanzi_recognition.tsv \
  https://raw.githubusercontent.com/chelsea6502/hsk-2025-data/master/hanzi_recognition.tsv
curl -L -o /tmp/cedict.txt.gz \
  https://www.mdbg.net/chinese/export/cedict/cedict_1_0_ts_utf-8_mdbg.txt.gz
gzip -dc /tmp/cedict.txt.gz > /tmp/cedict.txt
```

- 1,527 recognition characters (levels 1–5)
- 2,770 multi-character words (levels 1–5, first occurrence per word)
- 4,297 entries in 12 groups: characters/words per level plus two combined groups
- Single-character words are already covered by the character groups, so they
  are not duplicated in the word groups
- Character readings are generated into `tools/hsk30-2025-chars.txt`; the
  hand-checked 2021 table `tools/hsk30-1-3-chars.txt` supplies the readings
  for the characters it covers
- `tools/legacy-ids.json` maps simplified forms to the entry IDs from the
  previous deck. It is used only to keep existing mastery when words move
  between levels; do not regenerate it after the migration

English glosses come from CC-CEDICT (`data/LICENSE-CC-CEDICT`) and the cleaned
2025 vocabulary list (`data/LICENSE-HSK-2025`). Custom groups and mastery live
in SQLite, not in `data/vocab.json`, so regenerating the deck never touches
them; IDs are reused by simplified form.

Levels 6 and 7–9 are the next planned phases. The character table already
contains all 3,088 recognition characters; their readings for 6–9 still need a
review pass, and the word/level selection and groups need extending.

There is no audio or TTS pipeline in the repository; the deck is text-only.

## Strokes

`fetch_stroke_data.py` downloads stroke JSON for every simplified character in
`data/vocab.json` from hanzi-writer-data, storing it under `data/strokes/` by
Unicode code point. It writes the attribution note as well.

## Icons

`render_icons.sh` regenerates the PNG icon fallbacks from
`data/icons/hicolor/scalable/apps/io.github.horooka.hanzi.svg` using
`rsvg-convert`. The PNGs matter on systems without a gdk-pixbuf SVG loader.
