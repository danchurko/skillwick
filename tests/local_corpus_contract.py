#!/usr/bin/env python3
"""The read-only corpus proof covers instruction and package support changes."""

import importlib.util
from pathlib import Path
import tempfile

spec = importlib.util.spec_from_file_location(
    "corpus", Path(__file__).resolve().parents[1] / "scripts/verify-local-corpus.py"
)
corpus = importlib.util.module_from_spec(spec)
spec.loader.exec_module(corpus)

with tempfile.TemporaryDirectory(prefix="skillwick-corpus-contract-") as directory:
    root = Path(directory)
    package = root / "skill"
    package.mkdir()
    (package / "SKILL.md").write_text("fixture")
    support = package / "run.sh"
    support.write_text("first")

    def snapshot():
        records = {}
        corpus.walk_root(records, str(root))
        return records

    before = snapshot()
    support.write_text("second")
    assert snapshot() != before, "support content must be protected"
    before = snapshot()
    support.chmod(support.stat().st_mode ^ 0o100)
    assert snapshot() != before, "executable mode must be protected"
    before = snapshot()
    support.unlink()
    assert snapshot() != before, "support deletion must be protected"

for backend in ("none", "tinybert", "jev"):
    reranker = {"backend": backend}
    if backend != "none":
        reranker["runtime"] = "/private/tmp/prepared-runtime"
    config = corpus.normalize_config({"version": 1, "reranker": reranker}, Path("config.toml"))
    assert "reranker" not in config, "corpus proof must keep temporary configs lexical-only"
for reranker in ({"backend": "invalid"}, {"backend": "jev"},
                 {"backend": "none", "runtime": "/tmp/runtime"},
                 {"backend": "jev", "runtime": "relative"}, {"secret": "invalid"}):
    try:
        corpus.normalize_config({"version": 1, "reranker": reranker}, Path("config.toml"))
    except corpus.CorpusError:
        pass
    else:
        raise AssertionError("invalid reranker accepted by corpus verifier")

print("Local corpus preservation contract passed")
