//! Public configured-search boundary, independent of CLI dispatch.
use skillwick::{
    config::{Config, Reranker, RerankerBackend},
    index,
    metadata::{InvocationPolicy, Metadata},
    reranker, search,
    sources::Skill,
};
use std::path::{Path, PathBuf};

fn database() -> rusqlite::Connection {
    let mut db = index::open(Path::new(":memory:")).unwrap();
    let records: Vec<_> = [
        (
            "sqlite-maintenance",
            "Maintain SQLite databases and tune queries.",
        ),
        ("sqlite-guide", "SQLite database query reference."),
    ]
    .iter()
    .enumerate()
    .map(|(i, (name, description))| Skill {
        path: PathBuf::from(format!("/{name}/SKILL.md")),
        canonical: PathBuf::from(format!("/{name}/SKILL.md")),
        base: PathBuf::from(format!("/{name}")),
        scope: "global".into(),
        source: format!("/{name}"),
        source_kind: "filesystem".into(),
        enabled: true,
        plugin_id: None,
        source_fingerprint: "fixture".into(),
        roots: vec![],
        metadata: Metadata {
            name: (*name).into(),
            description: (*description).into(),
            keywords: String::new(),
            degraded: false,
            hash: format!("fixture-{i}"),
            invocation_policy: InvocationPolicy::Discoverable,
            policy_diagnostic: None,
        },
    })
    .collect();
    index::refresh_kind(&mut db, "filesystem", &records, true).unwrap();
    db
}

#[test]
fn configured_library_preserves_lexical_results_and_reports_fallback() {
    let db = database();
    let baseline = search::query(&db, "SQLite", 2, None).unwrap();
    for backend in [
        RerankerBackend::None,
        RerankerBackend::Tinybert,
        RerankerBackend::Jev,
    ] {
        let settings = Reranker {
            backend,
            runtime: (backend != RerankerBackend::None)
                .then(|| PathBuf::from("/nonexistent-skillwick-runtime")),
        };
        let outcome = reranker::search(&db, "SQLite", 2, None, &settings).unwrap();
        assert_eq!(
            serde_json::to_value(&outcome.rows).unwrap(),
            serde_json::to_value(&baseline).unwrap()
        );
        assert_eq!(
            outcome.diagnostic.is_some(),
            backend != RerankerBackend::None
        );
        let empty = reranker::search(&db, "unmatched", 2, None, &settings).unwrap();
        assert!(empty.rows.is_empty());
        assert!(empty.diagnostic.is_none());
    }
    assert!(reranker::search(&db, "SQLite", 21, None, &Reranker::default()).is_err());
}

#[test]
#[ignore = "explicit live verification against a setup-prepared local configuration"]
fn prepared_backend_library_search() {
    let config_path =
        std::env::var("SKILLWICK_LIVE_CONFIG").expect("explicit verification configuration");
    let settings = Config::load(Path::new(&config_path)).unwrap();
    assert_ne!(settings.reranker.backend, RerankerBackend::None);
    let cwd = std::env::current_dir().unwrap();
    let snapshot = skillwick::inventory::reconcile(&settings, &cwd, "search").unwrap();
    let profile: serde_json::Value =
        serde_json::from_str(include_str!("../benchmarks/profile-v1.json")).unwrap();
    let mut rankings = Vec::new();
    for case in profile["heldout"]["cases"].as_array().unwrap() {
        for (index, query) in case["queries"].as_array().unwrap().iter().enumerate() {
            let query = query.as_str().unwrap();
            let started = std::time::Instant::now();
            let outcome = reranker::search(
                &snapshot.db,
                query,
                5,
                Some(&snapshot.roots),
                &settings.reranker,
            )
            .unwrap();
            let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
            assert!(
                outcome.diagnostic.is_none(),
                "backend failed: {:?}",
                outcome.diagnostic
            );
            let lexical = search::query(&snapshot.db, query, 20, Some(&snapshot.roots)).unwrap();
            assert_eq!(outcome.rows.len(), lexical.len().min(5));
            for row in &outcome.rows {
                let original = lexical
                    .iter()
                    .find(|candidate| candidate.id == row.id)
                    .unwrap();
                assert_eq!(
                    serde_json::to_value(row).unwrap(),
                    serde_json::to_value(original).unwrap(),
                    "reranking changed candidate metadata"
                );
            }
            rankings.push(serde_json::json!({
                "id": format!("{}:{}", case["id"].as_str().unwrap(), index + 1),
                "query": query,
                "relevant": case["relevant"],
                "ranked": outcome.rows.iter().map(|row| &row.name).collect::<Vec<_>>(),
                "diagnostic": outcome.diagnostic,
                "metadata_preserved": true,
                "latency_ms": elapsed_ms,
            }));
        }
    }
    assert_eq!(rankings.len(), 105);
    if let Ok(path) = std::env::var("SKILLWICK_LIVE_RECEIPT") {
        let receipt = serde_json::json!({
            "backend": settings.reranker.backend,
            "profile_version": profile["version"],
            "rankings": rankings,
            "metadata_preserved": true,
            "status": "passed",
        });
        std::fs::write(path, serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();
    }
}
