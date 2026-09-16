# Architecture

Skillwick is one Rust CLI with a disposable SQLite FTS5 index. Package installers
own skills; host metadata determines plugin eligibility; installed files own
instruction content; calling agents decide what guidance to use.

```text
configuration + cwd + bounded host metadata
                 │
                 ▼
          applicable source roots
                 │
                 ▼
        scan, parse, fingerprint
                 │
                 ▼
      complete scoped SQLite snapshot
                 │
                 ▼
     exact lookup or lexical retrieval
                 │
                 ▼
    verified-copy grouping / live read
                 │
                 ▼
       human, raw, or JSON output
```

## Ownership

- `config` owns strict configuration, environment paths, and project associations.
- `discovery` resolves eligible shared, Codex, and Claude sources. Its bounded
  Codex subprocess lists installed plugin metadata; it never starts an agent
  server or installs, updates, authenticates, or executes a package.
- `sources` scans only resolved roots, rejects unauthorized escapes and unsupported
  path encodings, and parses bounded instructions and invocation policy.
- `metadata` parses and fingerprints the same instruction bytes.
- `inventory` coordinates complete reconciliation and cache locking. Failures
  retain the previous publication; stale inventory is not a successful result.
- `index` owns scoped SQLite records and atomic publication.
- `search` owns deterministic FTS5 ranking, exact resolution, and grouping after
  applicability/policy filtering and before result limits.
- `package` owns bounded inspection and complete package fingerprints. Incomplete
  fingerprints never establish that copies are identical.
- `integration` owns setup plans, locks, ownership receipts, and recovery.
- `cli`, `doctor`, and `output` own invocation, diagnostics, and rendering.

Selected content is revalidated and read from the live file. SQLite never stores
an authoritative copy of instructions. JSON preserves exact values; terminal
rendering escapes control characters separately. Lexical search is the only
production retrieval path. No model runtime, prompt hook, or watcher is required.
