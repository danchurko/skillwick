#!/usr/bin/env python3
"""Exercise installed CLI contracts using only isolated skill and state fixtures."""
import json
import hashlib
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tempfile

binary = str(Path(sys.argv[1]).resolve())
with tempfile.TemporaryDirectory(prefix="skillwick-invocation-") as directory:
    temporary = Path(directory)
    invocation = temporary / "skillwick executable 'é"
    invocation.symlink_to(binary)
    binary = str(invocation)
    root = temporary / "skills with spaces"
    unusual = root / 'quotes " and line\nbreak é'
    unusual.mkdir(parents=True)
    content = '---\nname: example\ndescription: Verify cobalt invocation.\n---\nExact body.\n'
    (unusual / "SKILL.md").write_text(content)
    env = os.environ.copy()
    env.update(HOME=str(temporary / "home"), CODEX_HOME=str(temporary / "codex"),
               CLAUDE_CONFIG_DIR=str(temporary / "claude"), XDG_CONFIG_HOME=str(temporary / "config"),
               XDG_CACHE_HOME=str(temporary / "cache"), XDG_STATE_HOME=str(temporary / "state"))
    def run(*args, code=0):
        result = subprocess.run([binary, *args], env=env, stdin=subprocess.DEVNULL,
                                capture_output=True, text=True, timeout=30)
        assert result.returncode == code, (args, result.returncode, result.stdout, result.stderr)
        return result
    run("init", "--yes", "--agent", "none", "--discovery", "explicit", "--root", str(root))
    rows = json.loads(run("list", "--json").stdout)
    assert rows["version"] == 3 and rows["total"] == 1
    row = rows["results"][0]
    assert Path(row["path"]).read_text() == content
    assert run("read", "--raw", row["id"]).stdout == content
    assert json.loads(run("--json", "read", "example").stdout)["results"][0]["content"] == content
    assert run("read", "example", "missing", code=3).stdout == ""
    assert run("read", "--raw", "example", "example", code=2).stdout == ""
    run("read", "--json", "--raw", "example", code=2)
    run("init", "--agent", "none", code=2)
    run("init", "--yes", "--agent", "none", "--agent", "codex", code=2)
    run("doctor", "--strict", "--require", "example")
    run("doctor", "--require", "missing", code=3)
    run("search", "")
    run("search", "--", "-missing")
    batch = json.loads(run("read", "--json", "example", "example").stdout)
    assert [row["content"] for row in batch["results"]] == [content, content]
    run("search", "cobalt", "--limit", "21", code=2)
    run("unknown", code=2)
    for args in [("find", "cobalt"), ("find", "--limit", "3", "cobalt"),
                 ("search", "cobalt", "--limit", "0"), ("search", "cobalt", "--bad"),
                 ("--json", "instructions"), ("read", "example", "--format", "xml")]:
        run(*args, code=2)
    assert json.loads(run("search", "no_such_skill_987654", "--json").stdout)["results"] == []
    health = json.loads(run("doctor", "--json", "--strict", "--require", "example").stdout)
    assert health["version"] == 3 and health["healthy"] is True
    assert all(isinstance(health[key], list) for key in ("sources", "diagnostics", "required"))
    assert isinstance(health["counts"], dict) and "results" not in health
    assert health["required"][0]["name"] == "example"
    assert health["required"][0]["resolved_id"] == row["id"]
    for shell in ["bash", "zsh", "fish"]:
        assert run("completions", shell).stdout
    command = shlex.join([binary, "read", "example"])
    no_match = shlex.join([binary, "search", "no_such_skill_987654"])
    for shell in ["sh", "bash", "zsh"]:
        executable = shutil.which(shell)
        if not executable:
            continue
        script = f'{command} >/dev/null && {no_match} >/dev/null && printf CHAIN_OK'
        result = subprocess.run([executable, "-c", script], env=env, capture_output=True, text=True)
        assert result.returncode == 0 and result.stdout == "CHAIN_OK", result
        missing = shlex.join([binary, "read", "missing"])
        result = subprocess.run([executable, "-c", f'{missing} && printf SHOULD_NOT_RUN'],
                                env=env, capture_output=True, text=True)
        assert result.returncode == 3 and result.stdout == ""
        result = subprocess.run([executable, "-c", f'{missing}; printf TRAILING_SUCCESS'],
                                env=env, capture_output=True, text=True)
        assert result.returncode == 0 and result.stdout == "TRAILING_SUCCESS"
        assert "skill not found" in result.stderr
        result = subprocess.run([executable, "-c", f'body=$({shlex.join([binary,"read","--raw","example"])}); test -n "$body"'], env=env)
        assert result.returncode == 0
        if shell != "sh":
            result = subprocess.run([executable, "-c", f'set -o pipefail; {command} | python3 -c "pass"'], env=env, capture_output=True)
            assert result.returncode == 0, result.stderr
    rtk = shutil.which("rtk")
    if rtk:
        direct = run("read", "example")
        wrapped = subprocess.run([rtk, binary, "read", "example"], env=env, capture_output=True, text=True)
        assert (wrapped.returncode, wrapped.stdout) == (0, direct.stdout)
    # Selection uses current eligible packages, including copies, conflicts and moves.
    original_id = row["id"]
    copied = root / "verified copy"
    shutil.copytree(unusual, copied)
    grouped = json.loads(run("list", "--json").stdout)
    assert grouped["total"] == 1 and len(grouped["results"][0]["origins"]) == 2
    assert run("read", "--raw", "example").stdout == content
    copied_body = copied / "SKILL.md"
    copied_body.write_text(content + "Different package obligation.\n")
    ambiguous = run("read", "example", code=3)
    assert ambiguous.stdout == "" and "ambiguous" in ambiguous.stderr
    assert str(unusual).replace("\n", "\\n") in ambiguous.stderr and str(copied) in ambiguous.stderr
    assert run("read", "--raw", original_id).stdout == content
    run("read", original_id, "example", code=3)
    shutil.rmtree(copied)
    moved = root / "moved package"
    unusual.rename(moved)
    run("read", original_id, code=3)
    current = json.loads(run("list", "--json").stdout)["results"][0]
    assert current["id"] != original_id and Path(current["base"]).resolve() == moved.resolve()
    assert run("read", "--raw", current["id"]).stdout == content
    body = moved / "SKILL.md"
    changed_content = content + "New live obligation.\n"
    body.write_text(changed_content)
    assert run("read", "--raw", "example").stdout == changed_content
    policy = moved / "agents/openai.yaml"
    policy.parent.mkdir()
    policy.write_text("policy:\n  allow_implicit_invocation: false\n")
    assert json.loads(run("list", "--json").stdout)["results"] == []
    denied = run("read", current["id"], "example", code=3)
    assert denied.stdout == "" and "invocation policy" in denied.stderr
    denied_health = json.loads(run("doctor", "--json", "--require", "example", code=3).stdout)
    assert denied_health["required"][0]["resolved_id"] is None
    assert "invocation policy" in denied_health["required"][0]["diagnostic"]
    policy.unlink()
    assert run("read", "--raw", "example").stdout == changed_content
    before_failure = (temporary / "cache/skillwick/index-v4.sqlite").read_bytes()
    body.chmod(0)
    try:
        if os.access(body, os.R_OK):
            raise AssertionError("unreadable fixture requires an unprivileged test process")
        assert run("read", "example", code=3).stdout == ""
        assert (temporary / "cache/skillwick/index-v4.sqlite").read_bytes() == before_failure
    finally:
        body.chmod(0o600)
    assert run("read", "--raw", "example").stdout == changed_content
    body.unlink()
    assert json.loads(run("list", "--json").stdout)["results"] == []
    assert run("read", current["id"], code=3).stdout == ""
    body.write_text(content)
    assert run("read", "--raw", "example").stdout == content
    # Full bodies survive raw, JSON, default and multi-skill delivery. Package
    # references use the returned base and reading never executes their scripts.
    long_content = content + ("Required obligation: café 日本語 🦀.\n" * 6000)
    body.write_text(long_content)
    references = moved / "references"
    references.mkdir()
    reference = references / "guide.md"
    reference.write_text("Complete supporting reference.\n")
    script = moved / "check.sh"
    sentinel = temporary / "should-not-execute"
    script.write_text("#!/bin/sh\ntouch " + shlex.quote(str(sentinel)) + "\n")
    script.chmod(0o755)
    assert run("read", "--raw", "example").stdout == long_content
    complete = json.loads(run("read", "--json", "example", "example").stdout)
    assert len(complete["results"]) == 2
    for selected in complete["results"]:
        assert selected["content"] == long_content
        assert selected["hash"] == hashlib.sha256(long_content.encode()).hexdigest()
        assert (Path(selected["base"]) / "references/guide.md").read_text() == "Complete supporting reference.\n"
    default = run("read", "example").stdout
    assert default.endswith(long_content) and f'resolved-id: {complete["results"][0]["id"]}\n' in default
    inspected = json.loads(run("inspect", complete["results"][0]["id"], "--json", "--files").stdout)
    assert any(entry["path"] == "references/guide.md" for entry in inspected["package"]["entries"])
    reference.unlink()
    assert not (Path(complete["results"][0]["base"]) / "references/guide.md").exists()
    assert run("read", "--raw", "example").stdout == long_content
    assert not sentinel.exists()
    # Unsupported path encodings fail explicitly instead of manufacturing another path.
    if os.name == "posix":
        bad = os.fsencode(root) + b"/invalid-\xff"
        try:
            os.mkdir(bad)
        except OSError as error:
            if sys.platform != "darwin" or error.errno not in (1, 22, 92):
                raise
            print("Filesystem rejects non-UTF-8 names; Linux exercises scanner rejection")
        else:
            with open(bad + b"/SKILL.md", "wb") as handle:
                handle.write(content.encode())
            assert "UTF-8" in run("list", code=3).stderr
print("Installed invocation contracts passed")
