# Stroke data

The JSON files in this directory are stroke-order data from
[hanzi-writer-data](https://github.com/chanind/hanzi-writer-data), which is
derived from [Make Me a Hanzi](https://github.com/skishore/makemeahanzi).

The graphics are distributed under the Arphic Public License (APL). See
`ARPHICPL.TXT` in this directory for the full license text.

Files are named by Unicode code point: `4f60.json` is 你. Each file contains
`strokes` (SVG path outlines) and `medians` (centre lines used for animation
and handwriting validation), in the Make Me a Hanzi coordinate system:
x in 0..1024, y in -124..900, y increasing upward.
