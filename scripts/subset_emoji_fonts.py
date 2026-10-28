#!/usr/bin/env python3
"""Cuts egui's emoji fonts down to the symbols the web version shows.

The browser downloads the fonts with the wasm; the full NotoEmoji + emoji-icon-font are ~460 KB
gzipped, of which a few dozen symbols are used. This keeps every non-ASCII character of the
sources of the app (crates/*/src, examples, emu8086.inc) and of egui, plus a few blocks of
arrows and shapes, and writes crates/rustemu-ui/fonts/*-subset.ttf (see src/fonts.rs).

Run it after adding a symbol to the UI (the test `fonts::tests::web_fonts_cover_the_ui` fails
until then):
    python3 -m venv /tmp/ft && /tmp/ft/bin/pip install fonttools
    /tmp/ft/bin/python scripts/subset_emoji_fonts.py
"""
import json
import pathlib
import subprocess

from fontTools import subset
from fontTools.ttLib import TTFont

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "crates/rustemu-ui/fonts"
EGUI_CRATES = {"egui", "epaint", "eframe", "egui-wgpu", "emath", "ecolor"}
# Whole blocks, for symbols in text the app does not know in advance (box drawing, arrows...).
BLOCKS = [(0x2190, 0x21FF), (0x2300, 0x23FF), (0x2500, 0x25FF), (0x2600, 0x26FF), (0x2700, 0x27BF)]


def source_files():
    yield from ROOT.glob("crates/*/src/**/*.rs")
    yield from ROOT.glob("examples/*.asm")
    yield from ROOT.glob("crates/rustemu-asm/inc/*.inc")
    meta = json.loads(subprocess.check_output(["cargo", "metadata", "--format-version", "1"], cwd=ROOT))
    for p in meta["packages"]:
        if p["name"] in EGUI_CRATES:
            yield from pathlib.Path(p["manifest_path"]).parent.glob("src/**/*.rs")


def main():
    chars = set()
    for f in source_files():
        chars.update(c for c in f.read_text(encoding="utf-8", errors="ignore") if ord(c) > 0x7F)
    for lo, hi in BLOCKS:
        chars.update(map(chr, range(lo, hi + 1)))
    meta = json.loads(subprocess.check_output(["cargo", "metadata", "--format-version", "1"], cwd=ROOT))
    fonts_dir = next(
        pathlib.Path(p["manifest_path"]).parent / "fonts"
        for p in meta["packages"]
        if p["name"] == "epaint_default_fonts"
    )
    for name in ["NotoEmoji-Regular", "emoji-icon-font"]:
        font = TTFont(fonts_dir / f"{name}.ttf")
        options = subset.Options()
        options.layout_features = ["*"]
        options.name_IDs = ["*"]
        sub = subset.Subsetter(options)
        sub.populate(unicodes=[ord(c) for c in chars])
        sub.subset(font)
        out = OUT / f"{name}-subset.ttf"
        font.save(out)
        kept = len(font.getBestCmap())
        print(f"{out.relative_to(ROOT)}: {kept} symbols, {out.stat().st_size} bytes")


if __name__ == "__main__":
    main()
