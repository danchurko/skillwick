use serde::{Deserialize, Serialize};
use std::{collections::HashSet, time::Instant};

#[derive(Deserialize)]
pub struct Dataset {
    pub version: u8,
    pub name: String,
    pub cases: Vec<Case>,
}

#[derive(Deserialize)]
pub struct Case {
    pub query: String,
    #[serde(default)]
    pub relevant: Vec<String>,
}

#[derive(Serialize)]
pub struct Report {
    pub version: u8,
    pub dataset: String,
    pub dataset_sha256: String,
    pub corpus_sha256: String,
    pub retriever: String,
    pub corpus_skills: usize,
    pub queries: usize,
    pub judged_queries: usize,
    pub no_match_queries: usize,
    pub recall_at_k: f64,
    pub hit_rate_at_k: f64,
    pub mrr_at_k: f64,
    pub ndcg_at_k: f64,
    pub no_match_accuracy: f64,
    pub query_p50_ms: f64,
    pub query_p95_ms: f64,
    pub misses: Vec<Miss>,
}

#[derive(Serialize)]
pub struct Miss {
    pub query: String,
    pub relevant: Vec<String>,
    pub returned: Vec<String>,
}

pub fn evaluate<F>(
    dataset: &Dataset,
    dataset_sha256: &str,
    corpus_sha256: &str,
    corpus_names: &HashSet<String>,
    corpus_skills: usize,
    k: usize,
    mut rank: F,
) -> Result<Report, String>
where
    F: FnMut(&str, usize) -> Result<Vec<String>, String>,
{
    if dataset.version != 1 {
        return Err(format!(
            "unsupported benchmark dataset version {}",
            dataset.version
        ));
    }
    if dataset.cases.is_empty() {
        return Err("benchmark dataset has no cases".into());
    }
    let missing: HashSet<_> = dataset
        .cases
        .iter()
        .flat_map(|case| &case.relevant)
        .filter(|name| !corpus_names.contains(*name))
        .collect();
    if !missing.is_empty() {
        let mut missing: Vec<_> = missing.into_iter().cloned().collect();
        missing.sort();
        return Err(format!(
            "benchmark labels missing from corpus: {}",
            missing.join(", ")
        ));
    }

    let mut relevant_total = 0;
    let mut recalled = 0;
    let mut reciprocal_rank = 0.0;
    let mut ndcg = 0.0;
    let mut judged = 0;
    let mut no_match = 0;
    let mut correct_no_match = 0;
    let mut hits = 0;
    let mut durations = Vec::with_capacity(dataset.cases.len());
    let mut misses = Vec::new();
    for case in &dataset.cases {
        let start = Instant::now();
        let returned = rank(&case.query, k)?;
        durations.push(start.elapsed());
        if case.relevant.is_empty() {
            no_match += 1;
            correct_no_match += usize::from(returned.is_empty());
            if !returned.is_empty() {
                misses.push(Miss {
                    query: case.query.clone(),
                    relevant: Vec::new(),
                    returned,
                });
            }
            continue;
        }

        judged += 1;
        relevant_total += case.relevant.len();
        let relevant: HashSet<_> = case.relevant.iter().map(String::as_str).collect();
        let ranks: Vec<_> = returned
            .iter()
            .enumerate()
            .filter_map(|(rank, name)| relevant.contains(name.as_str()).then_some(rank + 1))
            .collect();
        recalled += ranks.len();
        if let Some(first) = ranks.first() {
            hits += 1;
            reciprocal_rank += 1.0 / *first as f64;
        } else {
            misses.push(Miss {
                query: case.query.clone(),
                relevant: case.relevant.clone(),
                returned,
            });
        }
        let dcg: f64 = ranks
            .iter()
            .map(|rank| 1.0 / (*rank as f64 + 1.0).log2())
            .sum();
        let ideal: f64 = (1..=case.relevant.len().min(k))
            .map(|rank| 1.0 / (rank as f64 + 1.0).log2())
            .sum();
        ndcg += dcg / ideal;
    }
    durations.sort();
    let percentile =
        |percent: usize| durations[(durations.len() - 1) * percent / 100].as_secs_f64() * 1_000.0;
    Ok(Report {
        version: 1,
        dataset: dataset.name.clone(),
        dataset_sha256: dataset_sha256.into(),
        corpus_sha256: corpus_sha256.into(),
        retriever: "lexical".into(),
        corpus_skills,
        queries: dataset.cases.len(),
        judged_queries: judged,
        no_match_queries: no_match,
        recall_at_k: ratio(recalled, relevant_total),
        hit_rate_at_k: ratio(hits, judged),
        mrr_at_k: if judged == 0 {
            0.0
        } else {
            reciprocal_rank / judged as f64
        },
        ndcg_at_k: if judged == 0 {
            0.0
        } else {
            ndcg / judged as f64
        },
        no_match_accuracy: ratio(correct_no_match, no_match),
        query_p50_ms: percentile(50),
        query_p95_ms: percentile(95),
        misses,
    })
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_standard_rank_metrics() {
        let dataset = Dataset {
            version: 1,
            name: "small".into(),
            cases: vec![
                Case {
                    query: "found second".into(),
                    relevant: vec!["target".into()],
                },
                Case {
                    query: "nothing".into(),
                    relevant: Vec::new(),
                },
            ],
        };
        let names = HashSet::from(["target".into(), "other".into()]);
        let report = evaluate(&dataset, "dataset", "corpus", &names, 2, 5, |query, _| {
            Ok(if query == "found second" {
                vec!["other".into(), "target".into()]
            } else {
                Vec::new()
            })
        })
        .unwrap();
        assert_eq!(report.corpus_skills, 2);
        assert_eq!(report.recall_at_k, 1.0);
        assert_eq!(report.hit_rate_at_k, 1.0);
        assert_eq!(report.mrr_at_k, 0.5);
        assert_eq!(report.no_match_accuracy, 1.0);
        assert!(report.ndcg_at_k > 0.63 && report.ndcg_at_k < 0.64);
    }
}
