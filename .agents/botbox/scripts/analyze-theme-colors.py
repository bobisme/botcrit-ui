#!/usr/bin/env python3
"""Analyze raw ANSI snapshots against theme seed-derived expected colors."""

import argparse
import json
import math
import os
import re
import sys
from dataclasses import dataclass
from pathlib import Path


@dataclass
class Rgb:
    r: int
    g: int
    b: int

    @staticmethod
    def from_hex(h: str) -> "Rgb":
        h = h.lstrip("#")
        return Rgb(int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16))

    def lerp(self, other: "Rgb", t: float) -> "Rgb":
        return Rgb(
            int(self.r + (other.r - self.r) * t),
            int(self.g + (other.g - self.g) * t),
            int(self.b + (other.b - self.b) * t),
        )

    def blend_over(self, bg: "Rgb", alpha: float) -> "Rgb":
        """Porter-Duff source-over: src * alpha + bg * (1 - alpha)"""
        return Rgb(
            int(self.r * alpha + bg.r * (1 - alpha)),
            int(self.g * alpha + bg.g * (1 - alpha)),
            int(self.b * alpha + bg.b * (1 - alpha)),
        )

    def distance(self, other: "Rgb") -> float:
        return math.sqrt(
            (self.r - other.r) ** 2
            + (self.g - other.g) ** 2
            + (self.b - other.b) ** 2
        )

    def hex(self) -> str:
        return f"#{self.r:02x}{self.g:02x}{self.b:02x}"

    def __hash__(self):
        return hash((self.r, self.g, self.b))

    def __eq__(self, other):
        return self.r == other.r and self.g == other.g and self.b == other.b


def derive_expected_colors(seeds: dict, overrides: dict | None) -> dict[str, Rgb]:
    """Derive all expected theme colors from seeds, matching from_seeds() logic."""
    bg = Rgb.from_hex(seeds["background"])
    fg = Rgb.from_hex(seeds["foreground"])
    primary = Rgb.from_hex(seeds["primary"])
    muted = Rgb.from_hex(seeds["muted"])
    success = Rgb.from_hex(seeds["success"])
    warning = Rgb.from_hex(seeds["warning"])
    error = Rgb.from_hex(seeds["error"])

    expected = {
        "background": bg,
        "foreground": fg,
        "primary": primary,
        "muted": muted,
        "success": success,
        "warning": warning,
        "error": error,
        "panel_bg": bg.lerp(fg, 0.05),
        "selection_bg": primary.blend_over(bg, 0.25),
        "border": bg.lerp(fg, 0.15),
        "diff.added": success,
        "diff.removed": error,
        "diff.context": fg.lerp(muted, 0.15),
        "diff.hunk_header": muted,
        "diff.highlight_added": success.lerp(primary, 0.3),
        "diff.highlight_removed": error.lerp(fg, 0.15),
        "diff.added_bg": success.blend_over(bg, 0.08),
        "diff.removed_bg": error.blend_over(bg, 0.08),
        "diff.context_bg": bg,
        "diff.line_number": muted,
        "diff.added_ln_bg": success.blend_over(bg, 0.05),
        "diff.removed_ln_bg": error.blend_over(bg, 0.05),
    }

    # Apply overrides
    override_map = {
        "panelBg": "panel_bg",
        "selectionBg": "selection_bg",
        "diffAdded": "diff.added",
        "diffRemoved": "diff.removed",
        "diffHighlightAdded": "diff.highlight_added",
        "diffHighlightRemoved": "diff.highlight_removed",
        "diffAddedBg": "diff.added_bg",
        "diffRemovedBg": "diff.removed_bg",
    }
    if overrides:
        for json_key, internal_key in override_map.items():
            if json_key in overrides and overrides[json_key]:
                expected[internal_key] = Rgb.from_hex(overrides[json_key])
        # Syntax overrides — add them as expected colors too
        for key in [
            "syntaxKeyword",
            "syntaxFunction",
            "syntaxTypeName",
            "syntaxString",
            "syntaxNumber",
            "syntaxComment",
            "syntaxOperator",
            "syntaxPunctuation",
            "syntaxVariable",
            "syntaxConstant",
            "syntaxAttribute",
        ]:
            if key in overrides and overrides[key]:
                expected[f"syntax.{key}"] = Rgb.from_hex(overrides[key])

    return expected


def extract_rendered_colors(raw_path: str) -> tuple[set[Rgb], set[Rgb]]:
    """Extract unique fg and bg RGB colors from a raw ANSI snapshot."""
    with open(raw_path, "r") as f:
        data = f.read()

    fg_colors = set()
    bg_colors = set()
    for m in re.finditer(r"(38|48);2;(\d+);(\d+);(\d+)", data):
        kind = m.group(1)
        rgb = Rgb(int(m.group(2)), int(m.group(3)), int(m.group(4)))
        if kind == "38":
            fg_colors.add(rgb)
        else:
            bg_colors.add(rgb)

    return fg_colors, bg_colors


def find_nearest(color: Rgb, candidates: set[Rgb]) -> tuple[Rgb | None, float]:
    """Find the nearest candidate color by Euclidean distance."""
    best = None
    best_dist = float("inf")
    for c in candidates:
        d = color.distance(c)
        if d < best_dist:
            best_dist = d
            best = c
    return best, best_dist


def analyze_theme(
    theme_name: str,
    theme_path: str,
    snapshot_path: str,
    threshold: float,
) -> tuple[bool, list[str]]:
    """Analyze one theme. Returns (passed, messages)."""
    messages = []

    # Load theme JSON
    with open(theme_path) as f:
        theme_data = json.load(f)

    if "seeds" not in theme_data:
        messages.append(f"  SKIP: legacy format (no seeds)")
        return True, messages

    seeds = theme_data["seeds"]
    overrides = theme_data.get("overrides")
    expected = derive_expected_colors(seeds, overrides)

    # Load snapshot
    if not os.path.exists(snapshot_path):
        messages.append(f"  FAIL: snapshot not found")
        return False, messages

    fg_rendered, bg_rendered = extract_rendered_colors(snapshot_path)
    all_rendered = fg_rendered | bg_rendered

    if not all_rendered:
        messages.append(f"  FAIL: no colors found in snapshot (app may have crashed)")
        return False, messages

    # Core colors that MUST appear in any diff view
    core_keys = {
        "background",
        "foreground",
        "panel_bg",
        "muted",
        "diff.context_bg",
    }
    # Colors that SHOULD appear but depend on diff content
    expected_keys = {
        "diff.added_bg",
        "diff.removed_bg",
        "diff.added",
        "diff.removed",
        "diff.context",
        "diff.line_number",
        "success",
        "error",
    }
    # Everything else is optional (syntax, selection, border, highlights)

    # Separate bg-expected and fg-expected colors
    bg_expected_keys = {
        k
        for k in expected
        if any(
            x in k
            for x in [
                "background",
                "panel_bg",
                "added_bg",
                "removed_bg",
                "context_bg",
                "ln_bg",
                "selection_bg",
            ]
        )
    }

    passed = True
    found = 0
    close_count = 0
    missing_core = 0
    missing_optional = 0

    for name, color in sorted(expected.items()):
        # Match bg colors against bg_rendered, fg colors against fg_rendered
        if name in bg_expected_keys:
            nearest, dist = find_nearest(color, bg_rendered)
        else:
            nearest, dist = find_nearest(color, fg_rendered)

        # Also check all rendered as fallback
        if dist > threshold:
            nearest_all, dist_all = find_nearest(color, all_rendered)
            if dist_all < dist:
                nearest, dist = nearest_all, dist_all

        is_core = name in core_keys
        is_expected = name in expected_keys
        severity = "CORE" if is_core else ("WANT" if is_expected else "INFO")

        if dist == 0:
            found += 1
        elif dist <= threshold:
            close_count += 1
            if dist > 5:  # Only report non-trivial rounding
                messages.append(
                    f"  CLOSE [{severity}]: {name} expected={color.hex()} "
                    f"nearest={nearest.hex() if nearest else '?'} dist={dist:.1f}"
                )
        else:
            if is_core:
                missing_core += 1
                passed = False
                messages.append(
                    f"  MISS  [CORE]: {name} expected={color.hex()} "
                    f"nearest={nearest.hex() if nearest else '?'} dist={dist:.1f}"
                )
            elif is_expected:
                missing_optional += 1
                messages.append(
                    f"  MISS  [WANT]: {name} expected={color.hex()} "
                    f"nearest={nearest.hex() if nearest else '?'} dist={dist:.1f}"
                )
            else:
                missing_optional += 1
                messages.append(
                    f"  MISS  [INFO]: {name} expected={color.hex()} "
                    f"nearest={nearest.hex() if nearest else '?'} dist={dist:.1f}"
                )

    total = len(expected)
    status = "PASS" if passed else "FAIL"
    messages.insert(
        0,
        f"  {status}: {found}/{total} exact, {close_count} close, "
        f"{missing_core} core missing, {missing_optional} optional missing",
    )

    return passed, messages


def main():
    parser = argparse.ArgumentParser(description="Analyze theme color snapshots")
    parser.add_argument("--themes-dir", required=True, help="Path to themes/ directory")
    parser.add_argument(
        "--snapshots-dir", required=True, help="Path to raw snapshot files"
    )
    parser.add_argument(
        "--threshold",
        type=float,
        default=30.0,
        help="Max Euclidean distance for 'close' match",
    )
    args = parser.parse_args()

    themes_dir = Path(args.themes_dir)
    snapshots_dir = Path(args.snapshots_dir)

    theme_files = sorted(themes_dir.glob("*.json"))
    if not theme_files:
        print(f"No theme files found in {themes_dir}")
        sys.exit(1)

    total_pass = 0
    total_fail = 0

    for theme_path in theme_files:
        theme_name = theme_path.stem
        snapshot_path = snapshots_dir / f"{theme_name}.raw"

        print(f"{theme_name}:")
        ok, messages = analyze_theme(
            theme_name,
            str(theme_path),
            str(snapshot_path),
            args.threshold,
        )
        for msg in messages:
            print(msg)
        print()

        if ok:
            total_pass += 1
        else:
            total_fail += 1

    print(f"=== Summary: {total_pass} passed, {total_fail} failed ===")
    sys.exit(0 if total_fail == 0 else 1)


if __name__ == "__main__":
    main()
