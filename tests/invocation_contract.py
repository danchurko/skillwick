#!/usr/bin/env python3
"""Exercise installed CLI contracts using only isolated skill and state fixtures."""
import json
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
