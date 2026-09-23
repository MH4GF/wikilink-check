#!/usr/bin/env python3
"""Compare wikilink-check against Obsidian's own unresolved-link list.

Obsidian's CLI (`obsidian unresolved verbose format=json`, Obsidian 1.12+) reports the
metadata cache's unresolved links as {link, count, sources}. This script runs both tools on
the same vault and prints the symmetric difference of (source, link) pairs, which is the
development oracle for the resolution rules. Obsidian must be running with the vault open.

Usage:
    scripts/oracle-diff.py VAULT [--oracle saved.json] [--bin path/to/wikilink-check]
"""

import argparse
import collections
import json
import os
import subprocess
import sys


def load_oracle(vault, path):
    if path:
        with open(path, encoding="utf-8") as f:
            data = json.load(f)
    else:
        out = subprocess.run(
            ["obsidian", "unresolved", "verbose", "format=json"],
            cwd=vault, check=True, capture_output=True, text=True,
        ).stdout
        data = json.loads(out)
    pairs = set()
    for e in data:
        for src in e["sources"].split(", "):
            pairs.add((src.strip(), e["link"]))
    return pairs


def load_ours(vault, binary):
    out = subprocess.run(
        [binary, vault, "--format", "json", "--no-git", "--top", "0"],
        check=True, capture_output=True, text=True,
    ).stdout
    data = json.loads(out)
    pairs = set()
    for l in data["links"]:
        target = l["target"]
        if target.endswith(".md"):
            target = target[:-3]
        pairs.add((l["source"], target))
    return pairs


def show(title, pairs, limit):
    print(f"\n{title}: {len(pairs)}")
    by_dir = collections.Counter(s.split("/")[0] for s, _ in pairs)
    if by_dir:
        print("  by source dir:", dict(by_dir.most_common(8)))
    for src, link in sorted(pairs)[:limit]:
        print(f"  {src}  ->  {link}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("vault")
    ap.add_argument("--oracle", help="saved output of `obsidian unresolved verbose format=json`")
    ap.add_argument("--bin", default=os.path.join(os.path.dirname(__file__), "..", "target", "debug", "wikilink-check"))
    ap.add_argument("--limit", type=int, default=40)
    args = ap.parse_args()

    oracle = load_oracle(args.vault, args.oracle)
    ours = load_ours(args.vault, args.bin)
    print(f"oracle pairs: {len(oracle)}   ours: {len(ours)}   common: {len(oracle & ours)}")
    show("only in oracle (we resolved something Obsidian did not)", oracle - ours, args.limit)
    show("only in ours (we failed to resolve something Obsidian did)", ours - oracle, args.limit)
    return 0 if oracle == ours else 1


if __name__ == "__main__":
    sys.exit(main())
