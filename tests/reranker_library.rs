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
    let query = "Make p95 latency and data freshness explicit success criteria for the design";
    let outcome = reranker::search(
        &snapshot.db,
        query,
        5,
        Some(&snapshot.roots),
        &settings.reranker,
    )
    .unwrap();
    assert!(
        outcome.diagnostic.is_none(),
        "backend failed: {:?}",
        outcome.diagnostic
    );
    assert!(!outcome.rows.is_empty());
    if let Ok(path) = std::env::var("SKILLWICK_LIVE_RECEIPT") {
        std::fs::write(path, serde_json::to_vec_pretty(&serde_json::json!({"backend": settings.reranker.backend, "query":query,"names":outcome.rows.iter().map(|v| &v.name).collect::<Vec<_>>(),"ids":outcome.rows.iter().map(|v| &v.id).collect::<Vec<_>>(),"diagnostic":outcome.diagnostic,"status":"passed"})).unwrap()).unwrap();
    }
}
