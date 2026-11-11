# tools

## Vocabulary deck

`data/vocab.json` is generated from the HSK 3.0 sources:

```sh
python tools/build_hsk30_vocab.py \
  --words-csv /tmp/hsk30.csv \
  --cedict /tmp/cedict.txt
```

Download the sources with:

```sh
curl -L -o /tmp/hsk30.csv \
  https://raw.githubusercontent.com/ivankra/hsk30/master/hsk30.csv
curl -L -o /tmp/cedict.txt.gz \
  https://www.mdbg.net/chinese/export/cedict/cedict_1_0_ts_utf-8_mdbg.txt.gz
gzip -dc /tmp/cedict.txt.gz > /tmp/cedict.txt
```

- 899 characters (from `tools/hsk30-1-3-chars.txt`)
- 1657 multi-character words (HSK 3.0 levels 1–3, first occurrence per word)
- 8 groups: characters/words per level plus two combined groups
- English glosses from CC-CEDICT (`data/LICENSE-CC-CEDICT`)

Custom groups are stored in SQLite, not in `data/vocab.json`, so regenerating
the deck never touches them.

There is no audio or TTS pipeline in the repository; the deck is text-only.

## Strokes

`fetch_stroke_data.py` downloads stroke JSON for every simplified character in
`data/vocab.json` from hanzi-writer-data, storing it under `data/strokes/` by
Unicode code point. It writes the attribution note as well.

## Icons

`render_icons.sh` regenerates the PNG icon fallbacks from
`data/icons/hicolor/scalable/apps/io.github.horooka.hanzi.svg` using
`rsvg-convert`. The PNGs matter on systems without a gdk-pixbuf SVG loader.
