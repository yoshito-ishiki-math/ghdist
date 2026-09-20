use crate::input::{Rational, Space};
use crate::job::{self, Options};
use crate::witness::{self, Engine, Layer};
use serde_json::{Value, json};
use std::time::Duration;

fn two(distance: Value) -> Space {
    Space::from_json(
        &json!({"points":["a","b"],"distances":[[0,distance],[distance,0]]}),
        "two",
    )
    .unwrap()
}
fn point() -> Space {
    Space::from_json(&json!({"distances":[[0]]}), "point").unwrap()
}
fn from_edges(edges: &[u32], order: usize) -> Space {
    let metric = crate::metric::Metric::from_edges(edges).unwrap();
    assert_eq!(metric.order, order);
    let distances: Vec<Vec<_>> = metric
        .entries
        .chunks(order)
        .map(|row| row.to_vec())
        .collect();
    Space::from_json(&json!({"distances":distances}), "test").unwrap()
}
fn roundtrip(left: &Space, right: &Space, options: Options, expected: &str) {
    let job = job::run(
        left,
        right,
        &Options {
            want_witness: true,
            ..options
        },
    )
    .unwrap();
    assert!(!job.incomplete, "{}", job.value);
    assert_eq!(job.value["d_gh"], expected);
    let cert = job.certificate.unwrap();
    let checked = witness::verify(&cert, 10000).unwrap();
    assert_eq!(checked["upper_bound"], expected);
    assert_eq!(checked["optimality_verified"], false);
}
#[test]
fn rational_numbers_are_exact_and_scaling_is_shared_between_inputs() {
    for (text, expected) in [
        ("0.125", "1/8"),
        ("2/6", "1/3"),
        ("1.2e-3", "3/2500"),
        ("100e2", "10000"),
        ("0e1000000", "0"),
    ] {
        assert_eq!(Rational::parse(text).unwrap().to_string(), expected);
    }
    for bad in ["1/0", "-1", "NaN", "1/2/3", "1e99999"] {
        assert!(Rational::parse(bad).is_err(), "{bad}");
    }
    for layer in [Layer::Base, Layer::Hyperspace] {
        roundtrip(
            &two(json!("1/3")),
            &two(json!(0.5)),
            Options {
                layer,
                ..Options::default()
            },
            "1/12",
        );
        roundtrip(
            &point(),
            &two(json!("5/7")),
            Options {
                layer,
                ..Options::default()
            },
            "5/14",
        );
        roundtrip(
            &point(),
            &point(),
            Options {
                layer,
                ..Options::default()
            },
            "0",
        );
    }
    // This JSON integer cannot be represented exactly as an f64.
    let large: Value = serde_json::from_str("9007199254740993").unwrap();
    roundtrip(
        &point(),
        &two(large),
        Options::default(),
        "9007199254740993/2",
    );
}
#[test]
fn invalid_matrices_and_arithmetic_overflow_are_errors_not_panics() {
    for value in [
        json!({"distances":[]}),
        json!({"distances":[[0,1],[1]]}),
        json!({"distances":[[0,1],[2,0]]}),
        json!({"distances":[[0,0],[0,0]]}),
        json!({"points":["x","x"],"distances":[[0,1],[1,0]]}),
        json!({"distances":[[0,1,3],[1,0,1],[3,1,0]]}),
        json!({"matrix":[[0]]}),
    ] {
        assert!(Space::from_json(&value, "invalid").is_err());
    }
    let left = two(json!("4294967296"));
    let right = two(json!(1));
    assert!(
        job::run(&left, &right, &Options::default())
            .err()
            .unwrap()
            .contains("u32")
    );
}
#[test]
fn every_in_process_engine_exports_an_independently_checked_correspondence() {
    let left = from_edges(&crate::config::KNOWN_LEFT, 4);
    let right = from_edges(&crate::config::KNOWN_RIGHT, 4);
    roundtrip(&left, &right, Options::default(), "242");
    for engine in [Engine::Explicit, Engine::Generic, Engine::Auto] {
        roundtrip(
            &left,
            &right,
            Options {
                layer: Layer::Hyperspace,
                engine,
                ..Options::default()
            },
            "242",
        );
    }
    let left = from_edges(&[1, 3, 3, 3, 3, 2], 4);
    let right = from_edges(&[2, 3, 3, 3, 3, 1], 4);
    for engine in [Engine::Ultrametric, Engine::Generic, Engine::Explicit] {
        roundtrip(
            &left,
            &right,
            Options {
                layer: Layer::Hyperspace,
                engine,
                ..Options::default()
            },
            "0",
        );
    }
}
#[test]
#[ignore = "requires optional z3; run with --include-ignored"]
fn z3_models_export_valid_hyperspace_correspondences() {
    let left = from_edges(&crate::config::KNOWN_LEFT, 4);
    let right = from_edges(&crate::config::KNOWN_RIGHT, 4);
    roundtrip(
        &left,
        &right,
        Options {
            layer: Layer::Hyperspace,
            engine: Engine::Z3,
            ..Options::default()
        },
        "242",
    );
}
#[test]
fn interrupted_calculation_keeps_an_interval_and_no_exact_distance() {
    let left = from_edges(&crate::config::KNOWN_LEFT, 4);
    let right = from_edges(&crate::config::KNOWN_RIGHT, 4);
    for layer in [Layer::Base, Layer::Hyperspace] {
        for options in [
            Options {
                layer,
                time_limit: Some(Duration::ZERO),
                ..Options::default()
            },
            Options {
                layer,
                config: crate::config::LazyConfig {
                    max_search_nodes: 0,
                    ..Default::default()
                },
                ..Options::default()
            },
        ] {
            let job = job::run(&left, &right, &options).unwrap();
            assert_eq!(job.value["status"], "bounded", "{}", job.value);
            assert!(job.value["d_gh"].is_null());
            assert!(job.incomplete);
            let lo = Rational::parse(job.value["lower_bound"].as_str().unwrap()).unwrap();
            let hi = Rational::parse(job.value["upper_bound"].as_str().unwrap()).unwrap();
            assert!(lo.num <= 242 * lo.den && hi.num >= 242 * hi.den);
        }
    }
}
#[test]
fn witness_limit_does_not_erase_an_exact_distance() {
    let mut options = Options {
        want_witness: true,
        ..Options::default()
    };
    options.limits.max_pairs = 1;
    let job = job::run(&two(json!(2)), &two(json!(6)), &options).unwrap();
    assert_eq!(job.value["d_gh"], "2");
    assert_eq!(job.value["certificate_status"], "incomplete");
    assert!(job.incomplete && job.certificate.is_none());
}
#[test]
fn certificate_verifier_detects_tampering_without_trusting_optimality_metadata() {
    let job = job::run(
        &two(json!(2)),
        &two(json!(6)),
        &Options {
            want_witness: true,
            ..Options::default()
        },
    )
    .unwrap();
    let certificate = job.certificate.unwrap();
    let mut bad = certificate.clone();
    bad["upper_bound"] = json!("1");
    assert!(witness::verify(&bad, 100).is_err());
    let mut bad = certificate.clone();
    bad["correspondence"].as_array_mut().unwrap().pop();
    assert!(witness::verify(&bad, 100).is_err());
    let mut bad = certificate.clone();
    bad["correspondence"][0]["left"] = json!(["unknown"]);
    assert!(witness::verify(&bad, 100).is_err());
    let mut metadata = certificate;
    metadata["search_report"]["reported_lower_bound"] = json!("999999");
    assert_eq!(
        witness::verify(&metadata, 100).unwrap()["optimality_verified"],
        false
    );
}

#[test]
fn ultrametric_witnesses_match_explicit_optima_in_small_cases() {
    let mut spaces = vec![point(), two(json!(2))];
    for a in 1..=3 {
        for b in 1..=3 {
            for c in 1..=3 {
                if let Ok(metric) = crate::metric::Metric::from_edges(&[a, b, c])
                    && metric.is_ultrametric()
                {
                    spaces.push(from_edges(&[a, b, c], 3));
                }
            }
        }
    }
    for left in &spaces {
        for right in &spaces {
            let reference = job::run(
                left,
                right,
                &Options {
                    layer: Layer::Hyperspace,
                    engine: Engine::Explicit,
                    ..Options::default()
                },
            )
            .unwrap();
            roundtrip(
                left,
                right,
                Options {
                    layer: Layer::Hyperspace,
                    engine: Engine::Ultrametric,
                    ..Options::default()
                },
                reference.value["d_gh"].as_str().unwrap(),
            );
        }
    }
}
#[test]
fn unfinished_base_comparison_is_reported_even_if_hyperspace_finishes() {
    let left = from_edges(&crate::config::KNOWN_LEFT, 4);
    let right = from_edges(&crate::config::KNOWN_RIGHT, 4);
    let mut options = Options {
        layer: Layer::Hyperspace,
        compare_base: true,
        ..Options::default()
    };
    options.limits.max_vertices = 1;
    let result = job::run(&left, &right, &options).unwrap();
    assert_eq!(result.value["status"], "exact");
    assert_eq!(result.value["base_comparison"]["status"], "bounded");
    assert!(result.incomplete);
}
