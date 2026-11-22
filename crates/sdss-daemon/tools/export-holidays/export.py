#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Export `python-holidays` to sdss JSON (one file per ISO country)."""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

try:
    import holidays
except ImportError:
    print("pip install holidays", file=sys.stderr)
    sys.exit(1)


def export_country(code: str, y0: int, y1: int, out_dir: Path) -> None:
    code = code.upper()
    years = list(range(y0, y1 + 1))
    # Subdivisions list from library
    subdivs = list(holidays.utils.list_supported_countries().get(code, [code]))
    by_year: dict[str, dict] = {}
    for y in years:
        by_subdiv: dict[str, dict] = {}
        for subdiv in subdivs:
            try:
                h = holidays.country_holidays(code, subdiv=subdiv if subdiv != code else None, years=y)
            except Exception:
                continue
            cats: dict[str, list[str]] = {}
            for dt in sorted(h.keys()):
                dstr = dt.isoformat()
                cat = "public"
                if hasattr(h, "categories") and dt in h:
                    # best-effort label
                    cat = "public"
                cats.setdefault(cat, []).append(dstr)
            if cats:
                sk = subdiv if subdiv != code else list(h.subdivisions or [""])[0] if hasattr(h, "subdivisions") else subdiv
                if not sk:
                    sk = ""
                by_subdiv[str(sk)] = {"by_category": cats}
        by_year[str(y)] = {"by_subdiv": by_subdiv}
    payload = {"schema_version": 1, "country": code, "by_year": by_year}
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / f"{code}.json").write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--out", type=Path, default=Path("holidays"))
    p.add_argument("--year-start", type=int, default=2025)
    p.add_argument("--year-end", type=int, default=2028)
    p.add_argument(
        "--all-supported",
        action="store_true",
        help="Export every country code supported by python-holidays (large).",
    )
    p.add_argument("countries", nargs="*", default=["DE"])
    args = p.parse_args()
    codes = args.countries
    if args.all_supported:
        codes = sorted(holidays.utils.list_supported_countries().keys())
    for c in codes:
        export_country(c, args.year_start, args.year_end, args.out)
        print(f"wrote {args.out / (c.upper() + '.json')}")


if __name__ == "__main__":
    main()
