#!/usr/bin/env python3
"""Fetch stroke-order data for the demo deck.

Data comes from hanzi-writer-data / Make Me a Hanzi:

    https://github.com/chanind/hanzi-writer-data
    https://github.com/skishore/makemeahanzi

The graphics are derived from Arphic fonts and distributed under the Arphic
Public License. See `data/strokes/README.md` after running this script.

Files are stored by Unicode code point (`4f60.json` for 你) so paths stay
ASCII-only and easy to look up from Rust.

    ./tools/fetch_stroke_data.py [--force]

Requires network access.
"""

from __future__ import annotations

import json
import pathlib
import sys
import urllib.parse
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[1]
VOCAB = ROOT / "data" / "vocab.json"
OUT = ROOT / "data" / "strokes"
BASE = "https://raw.githubusercontent.com/chanind/hanzi-writer-data/master/data"

ATTRIBUTION = """# Stroke data

The JSON files in this directory are stroke-order data from
[hanzi-writer-data](https://github.com/chanind/hanzi-writer-data), which is
derived from [Make Me a Hanzi](https://github.com/skishore/makemeahanzi).

The graphics are distributed under the Arphic Public License (APL). See
`ARPHICPL.TXT` in this directory for the full license text.

Files are named by Unicode code point: `4f60.json` is 你. Each file contains
`strokes` (SVG path outlines) and `medians` (centre lines used for animation
and handwriting validation), in the Make Me a Hanzi coordinate system:
x in 0..1024, y in -124..900, y increasing upward.
"""

BASE_URL = "https://raw.githubusercontent.com/chanind/hanzi-writer-data/master"
LICENSE_URL = f"{BASE_URL}/ARPHICPL.TXT"


def characters() -> list[str]:
    vocab = json.loads(VOCAB.read_text(encoding="utf-8"))
    seen: set[str] = set()
    ordered: list[str] = []
    for entry in vocab["entries"]:
        for char in entry["simplified"]:
            if char not in seen:
                seen.add(char)
                ordered.append(char)
    return ordered


def fetch(char: str) -> dict:
    url = f"{BASE}/{urllib.parse.quote(char)}.json"
    with urllib.request.urlopen(url, timeout=30) as response:
        return json.loads(response.read().decode("utf-8"))


def main() -> int:
    force = "--force" in sys.argv[1:]
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / "README.md").write_text(ATTRIBUTION, encoding="utf-8")

    license_path = OUT / "ARPHICPL.TXT"
    if not license_path.exists():
        with urllib.request.urlopen(LICENSE_URL, timeout=30) as response:
            license_path.write_bytes(response.read())

    chars = characters()
    fetched = 0
    skipped = 0

    for char in chars:
        path = OUT / f"{ord(char):x}.json"
        if path.exists() and not force:
            skipped += 1
            continue

        try:
            data = fetch(char)
        except Exception as error:  # noqa: BLE001 - report and keep going
            print(f"skip {char} (U+{ord(char):04X}): {error}", file=sys.stderr)
            continue

        strokes = data.get("strokes", [])
        medians = data.get("medians", [])
        if not strokes or len(strokes) != len(medians):
            print(f"skip {char} (U+{ord(char):04X}): bad stroke data", file=sys.stderr)
            continue

        path.write_text(
            json.dumps(
                {
                    "strokes": strokes,
                    "medians": medians,
                    "radStrokes": data.get("radStrokes", []),
                },
                ensure_ascii=False,
                separators=(",", ":"),
            ),
            encoding="utf-8",
        )
        fetched += 1
        print(f"{char} U+{ord(char):04X} -> {path.relative_to(ROOT)} ({len(strokes)} strokes)")

    print(f"{fetched} fetched, {skipped} already present, {len(chars)} characters")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
