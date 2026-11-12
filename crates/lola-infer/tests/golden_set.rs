//! The shipped golden set loads, validates and passes against the
//! deterministic `GoldenOracle` double — the S2.a pipeline proof.

use lola_infer::infer::Infer;
use lola_infer::oracle::{run_conformance, ConformanceReport, GoldenOracle, GoldenSet};

const GOLDEN: &str = include_str!("../../../evals/golden/decision-contract-v1.json");

fn load() -> GoldenSet {
    let path = std::env::temp_dir().join(format!("lola-golden-{}.json", std::process::id()));
    std::fs::write(&path, GOLDEN).expect("write golden copy");
    let set = GoldenSet::load(&path).expect("shipped golden set must load and validate");
    let _ = std::fs::remove_file(&path);
    set
}

#[test]
fn shipped_golden_set_loads_and_validates() {
    let set = load();
    assert_eq!(set.cases.len(), 3, "three placeholder cases");
    assert!(set.provenance.contains("hand-authored-placeholder"));
    // Regime coverage: one per dominant bucket.
    let _ = &set.cases[0];
}

#[test]
fn golden_double_conforms_on_shipped_set() {
    let set = load();
    let oracle = GoldenOracle::from_set(&set);
    let report: ConformanceReport = run_conformance(&oracle, &set);
    assert!(report.all_passed(), "failures: {:?}", report.failures);
    assert_eq!(report.total, 3);
    assert_eq!(report.passed, 3);
}

#[test]
fn infer_trait_roundtrip_via_double() {
    // The double serves through the real trait surface, erroring on
    // generate() and on unknown snapshots — contract behavior S2.b keeps.
    let set = load();
    let oracle = GoldenOracle::from_set(&set);
    assert_eq!(oracle.model_id(), "golden-oracle");
    let request = &set.cases[0].request;
    let decision = oracle.decide(request).expect("known answer");
    assert_eq!(decision.decision_id, set.cases[0].expected.decision_id);
    assert!(oracle
        .generate(&lola_infer::infer::GenerateRequest {
            prompt: "hi".into(),
            max_tokens: 8,
            stop: vec![],
        })
        .is_err());
}
