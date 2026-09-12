use skillwick::{index, metadata::Metadata, search, sources::Skill};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

fn corpus(size: usize) -> Vec<Skill> {
    (0..size)
        .map(|number| {
            let name = format!("skill-{number:05}");
            let path = PathBuf::from(format!("/bench/{name}/SKILL.md"));
            Skill {
                canonical: path.clone(),
                base: path.parent().unwrap().into(),
                path,
                scope: "global".into(),
                source: "/bench".into(),
                source_kind: "filesystem".into(),
                enabled: true,
                plugin_id: None,
                metadata: Metadata {
                    name,
                    description: format!("Deploy and debug agent runtime number {number}"),
                    keywords: "cloud native service".into(),
                    degraded: false,
                    hash: number.to_string(),
                },
            }
        })
        .collect()
}

fn measure(size: usize) -> (Duration, Duration) {
    let mut db = index::open(Path::new(":memory:")).unwrap();
    let skills = corpus(size);
    let refresh_start = Instant::now();
    index::refresh_kind(&mut db, "filesystem", &skills, true).unwrap();
    let refresh = refresh_start.elapsed();
    let mut samples = Vec::new();
    for _ in 0..200 {
        let start = Instant::now();
        assert_eq!(
            search::query(&db, "deploy agent runtime", 5).unwrap().len(),
            5
        );
        samples.push(start.elapsed());
    }
    samples.sort();
    (refresh, samples[samples.len() * 95 / 100])
}

#[test]
#[ignore = "release performance evidence"]
fn lexical_scale() {
    for size in [1_000, 10_000] {
        let (refresh, p95) = measure(size);
        println!(
            "records={size} refresh_ms={:.2} warm_query_p95_ms={:.2} runs=200",
            refresh.as_secs_f64() * 1000.0,
            p95.as_secs_f64() * 1000.0
        );
        if size == 1_000 {
            assert!(p95 < Duration::from_millis(150));
        }
    }
}
