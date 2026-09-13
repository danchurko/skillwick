#!/usr/bin/env python3
# /// script
# requires-python = ">=3.14"
# dependencies = ["tiktoken==0.14.0"]
# ///
"""Estimate discovery text tokens from native prompt captures and real searches."""

import argparse
import hashlib
import json
from pathlib import Path
import platform
import re
import statistics
import subprocess

import tiktoken


def check_corpus(binary, expected):
    result = subprocess.run(
        [binary, "--json", "list"], check=True, capture_output=True, text=True, timeout=30
    )
    actual = json.loads(result.stdout)
    if actual != expected:
        raise ValueError("live corpus differs from the captured inventory; recapture before measuring")


def catalogue(path):
    messages = json.loads(path.read_text())
    text = "\n".join(
        part.get("text", "")
        for message in messages
        for part in message.get("content", [])
    )
    blocks = re.findall(r"<skills_instructions>.*?</skills_instructions>", text, re.S)
    if len(blocks) != 1:
        raise ValueError(f"{path}: expected one native skills block")
    names = [
        line[2:].split(": ", 1)[0]
        for line in blocks[0].splitlines()
        if line.startswith("- ") and "(file: " in line
    ]
    if not names or len(names) != len(set(names)):
        raise ValueError(f"{path}: missing or ambiguous skill entries")
    return blocks[0], set(names)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--native", type=Path, required=True)
    parser.add_argument("--configured-native", type=Path, required=True)
    parser.add_argument("--hidden", type=Path, required=True)
    parser.add_argument("--corpus", type=Path, required=True)
    parser.add_argument("--dataset", type=Path, required=True)
    parser.add_argument("--instructions", type=Path, default=Path("assets/skillwick/SKILLWICK.md"))
    parser.add_argument("--binary", default="skillwick")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    corpus = json.loads(args.corpus.read_text())
    check_corpus(args.binary, corpus)
    hidden = json.loads(args.hidden.read_text())
    if any(
        "<skills_instructions>" in part.get("text", "")
        for message in hidden
        for part in message.get("content", [])
    ):
        raise ValueError("hidden prompt still contains a native skills catalogue")
    expected = {row["name"] for row in corpus["results"] if row["enabled"]}
    full, full_names = catalogue(args.native)
    configured, configured_names = catalogue(args.configured_native)
    if not full_names <= expected:
        raise ValueError("native catalogue contains names absent from the corpus; recapture both")
    if not configured_names <= full_names:
        raise ValueError("configured catalogue is not a subset of the full catalogue")
    encoding = tiktoken.get_encoding("o200k_base")

    def count(text):
        return len(encoding.encode(text, disallowed_special=()))

    instructions = args.instructions.read_text()
    cases = json.loads(args.dataset.read_text())["cases"]
    if not cases:
        raise ValueError("dataset has no cases")
    payloads = []
    for case in cases:
        result = subprocess.run(
            [args.binary, "search", case["query"], "--limit", "5"],
            check=True, capture_output=True, text=True, timeout=30,
        )
        payloads.append(count(instructions + "\n" + case["query"] + "\n" + result.stdout))
    check_corpus(args.binary, corpus)
    mean = statistics.mean(payloads)
    report = {
        "version": 1,
        "measurement": "estimated discovery text tokens; not provider usage or total workflow savings",
        "encoding": "o200k_base",
        "tiktoken_version": tiktoken.__version__,
        "platform": platform.platform(),
        "binary_version": subprocess.run(
            [args.binary, "--version"], check=True, capture_output=True, text=True, timeout=30
        ).stdout.strip(),
        "corpus_skills": len(expected),
        "full_catalogue_skills": len(full_names),
        "inventory_skills_not_rendered": len(expected - full_names),
        "configured_catalogue_skills": len(configured_names),
        "full_catalogue_tokens": count(full),
        "configured_catalogue_tokens": count(configured),
        "instructions_tokens": count(instructions),
        "queries": len(cases),
        "queries_per_case": 1,
        "candidate_limit": 5,
        "skillwick_discovery_tokens_mean": mean,
        "skillwick_discovery_tokens_median": statistics.median(payloads),
        "skillwick_discovery_tokens_p95": sorted(payloads)[(len(payloads) - 1) * 95 // 100],
        "estimated_reduction_vs_full_percent": 100 * (1 - mean / count(full)),
        "estimated_reduction_vs_configured_percent": 100 * (1 - mean / count(configured)),
        "dataset_sha256": hashlib.sha256(args.dataset.read_bytes()).hexdigest(),
        "corpus_sha256": hashlib.sha256(args.corpus.read_bytes()).hexdigest(),
        "instructions_sha256": hashlib.sha256(args.instructions.read_bytes()).hexdigest(),
        "full_catalogue_sha256": hashlib.sha256(full.encode()).hexdigest(),
        "configured_catalogue_sha256": hashlib.sha256(configured.encode()).hexdigest(),
        "limitations": [
            "Tokenizer estimate; target model tokenization and message framing may differ.",
            "Catalogue baseline excludes Skillwick instructions; discovery includes instructions, query, and search output.",
            "Selected instruction bodies, common context, tool schemas, reasoning, and output are excluded.",
            "One search per case; delegated and multi-query workflows require separate measurement.",
            "Raw and cached input prices are not estimated.",
        ],
    }
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
