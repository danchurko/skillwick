use serde::Deserialize;
use skillwick::{index, metadata::Metadata, search, sources::Skill};
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct Case {
    query: String,
    relevant: Vec<String>,
}

fn skill(name: &str, description: &str, keywords: &str) -> Skill {
    let path = PathBuf::from(format!("/fixtures/{name}/SKILL.md"));
    Skill {
        canonical: path.clone(),
        base: path.parent().unwrap().into(),
        path,
        scope: "global".into(),
        source: "/fixtures".into(),
        source_kind: "filesystem".into(),
        enabled: true,
        plugin_id: None,
        metadata: Metadata {
            name: name.into(),
            description: description.into(),
            keywords: keywords.into(),
            degraded: false,
            hash: "fixture".into(),
        },
    }
}

#[test]
fn labelled_recall_at_five_and_false_positives() {
    let cases: Vec<Case> = serde_json::from_str(include_str!("fixtures/evaluation.json")).unwrap();
    assert!(cases.len() >= 40);
    let skills = [
        skill(
            "aws-agentcore",
            "Deploy and debug managed AWS agent runtimes and AgentCore services",
            "MCP TypeScript generative",
        ),
        skill(
            "rust-cli",
            "Build and test a native Rust command line terminal application",
            "cargo clap CLI C++ C# .NET Node.js packaging tools",
        ),
        skill(
            "macos-release",
            "Package cross architecture native macOS Apple release artifacts",
            "arm64 x86_64 notarize checksums archives Mac distribution",
        ),
        skill(
            "codex-skills",
            "Configure Codex installed agent skills and global AGENTS instructions",
            "catalogue app server skills list setup refresh",
        ),
        skill(
            "sqlite",
            "Build a transactional local SQLite FTS5 full text search index",
            "BM25 busy database escape query metadata Unicode café",
        ),
        skill(
            "homebrew",
            "Write Homebrew formulas and brew tap binary installers",
            "checksum archive formula arm intel URLs no home edits",
        ),
    ];
    let mut db = index::open(Path::new(":memory:")).unwrap();
    index::refresh_kind(&mut db, "filesystem", &skills, true).unwrap();
    let mut labelled = 0;
    let mut recalled = 0;
    let mut irrelevant = 0;
    let mut output_bytes = 0;
    for case in cases {
        let results = search::query(&db, &case.query, 5).unwrap();
        output_bytes += results
            .iter()
            .map(|row| row.id.len() + row.description.len())
            .sum::<usize>();
        if case.relevant.is_empty() {
            if !results.is_empty() {
                println!(
                    "irrelevant query {:?}: {:?}",
                    case.query,
                    results.iter().map(|row| &row.name).collect::<Vec<_>>()
                );
            }
            irrelevant += results.len();
            continue;
        }
        labelled += 1;
        if case
            .relevant
            .iter()
            .any(|name| results.iter().any(|row| &row.name == name))
        {
            recalled += 1;
        }
    }
    let recall = recalled as f64 / labelled as f64;
    println!("cases=40 labelled={labelled} recall_at_5={recall:.3} irrelevant_suggestions={irrelevant} output_bytes={output_bytes}");
    assert!(recall >= 0.95, "Recall@5 was {recall:.3}");
    assert_eq!(irrelevant, 0);
}
