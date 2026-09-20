use crate::bounds::StructuralFilter;
use crate::config::*;
use crate::explicit::minimum_distortion;
use crate::hyperspace::{HausdorffOracle, hyperspace_metric, minimum_hyperspace_distortion_lazy};
use crate::metric::{Metric, family_metric};
use crate::verification::brute_force_minimum_distortion;
use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;

#[test]
fn exact_solver_matches_brute_force_on_small_cases() {
    let cases = [
        (vec![1], vec![3]),
        (vec![2], vec![2, 3, 4]),
        (vec![2, 3, 4], vec![3, 4, 5]),
    ];
    for (left_edges, right_edges) in cases {
        let left = Metric::from_edges(&left_edges).unwrap();
        let right = Metric::from_edges(&right_edges).unwrap();
        let (exact, _) = minimum_distortion(&left, &right, DEFAULT_MAX_RELATION_VERTICES).unwrap();
        let brute = brute_force_minimum_distortion(&left, &right).unwrap();
        assert_eq!(exact, brute);
    }
}

#[test]
fn every_small_three_point_lazy_result_matches_explicit_search() {
    let mut metrics = Vec::new();
    for a in 1..=4 {
        for b in 1..=4 {
            for c in 1..=4 {
                if let Ok(metric) = Metric::from_edges(&[a, b, c]) {
                    metrics.push(metric);
                }
            }
        }
    }
    for left_index in 0..metrics.len() {
        for right_index in left_index..metrics.len() {
            let left = &metrics[left_index];
            let right = &metrics[right_index];
            let exp_left = hyperspace_metric(left, DEFAULT_MAX_HYPERSPACE_POINTS).unwrap();
            let exp_right = hyperspace_metric(right, DEFAULT_MAX_HYPERSPACE_POINTS).unwrap();
            let (explicit, _) =
                minimum_distortion(&exp_left, &exp_right, DEFAULT_MAX_RELATION_VERTICES).unwrap();
            let (generic, _) = minimum_hyperspace_distortion_lazy(
                left,
                right,
                &LazyConfig::default(),
                LazyStrategy::Generic,
                None,
            )
            .unwrap();
            assert_eq!(generic, explicit, "left={left:?}, right={right:?}");
            if left.is_ultrametric() && right.is_ultrametric() {
                let (decomposed, _) = minimum_hyperspace_distortion_lazy(
                    left,
                    right,
                    &LazyConfig::default(),
                    LazyStrategy::Ultrametric,
                    None,
                )
                .unwrap();
                assert_eq!(
                    decomposed, explicit,
                    "ultrametric left={left:?}, right={right:?}"
                );
            }
        }
    }
}

#[test]
fn known_four_point_pair_matches_python_oracle() {
    let left = Metric::from_edges(&KNOWN_LEFT).unwrap();
    let right = Metric::from_edges(&KNOWN_RIGHT).unwrap();
    let (base, _) = minimum_distortion(&left, &right, DEFAULT_MAX_RELATION_VERTICES).unwrap();
    assert_eq!(base, 484);
    assert_eq!(base, brute_force_minimum_distortion(&left, &right).unwrap());

    let exp_left = hyperspace_metric(&left, DEFAULT_MAX_HYPERSPACE_POINTS).unwrap();
    let exp_right = hyperspace_metric(&right, DEFAULT_MAX_HYPERSPACE_POINTS).unwrap();
    let (hyperspace, _) =
        minimum_distortion(&exp_left, &exp_right, DEFAULT_MAX_RELATION_VERTICES).unwrap();
    assert_eq!(hyperspace, 484);
    let (lazy, _) = minimum_hyperspace_distortion_lazy(
        &left,
        &right,
        &LazyConfig::default(),
        LazyStrategy::Generic,
        None,
    )
    .unwrap();
    assert_eq!(lazy, hyperspace);
    let (automatic, automatic_stats) = minimum_hyperspace_distortion_lazy(
        &left,
        &right,
        &LazyConfig::default(),
        LazyStrategy::Auto,
        Some(base),
    )
    .unwrap();
    assert_eq!(automatic, hyperspace);
    assert_eq!(automatic_stats.z3_checks, 0);
}

#[test]
fn dilation_oracle_matches_every_explicit_small_hyperspace_distance() {
    let metrics = [
        Metric::from_edges(&[2, 3, 4]).unwrap(),
        Metric::from_edges(&[2, 4, 8, 4, 8, 8]).unwrap(),
        Metric::from_edges(&KNOWN_LEFT).unwrap(),
    ];
    for base in metrics {
        let explicit = hyperspace_metric(&base, DEFAULT_MAX_HYPERSPACE_POINTS).unwrap();
        let mut oracle = HausdorffOracle::new(&base, 10_000).unwrap();
        for left in 1..=oracle.point_count() as u64 {
            for right in 1..=oracle.point_count() as u64 {
                assert_eq!(
                    oracle.distance(left, right),
                    explicit.get(left as usize - 1, right as usize - 1),
                    "base={base:?}, subsets={left:b},{right:b}"
                );
            }
        }
    }
}

#[test]
fn ultrametric_decomposition_matches_explicit_hyperspace_search() {
    let cases = [
        (
            Metric::from_edges(&[1, 1, 2, 1, 2, 2]).unwrap(),
            Metric::from_edges(&[2, 3, 3, 3, 3, 2]).unwrap(),
        ),
        (
            Metric::from_edges(&[1, 2, 2, 2, 2, 1]).unwrap(),
            Metric::from_edges(&[1, 1, 3, 1, 3, 3]).unwrap(),
        ),
    ];
    for (left, right) in cases {
        assert!(left.is_ultrametric() && right.is_ultrametric());
        let exp_left = hyperspace_metric(&left, DEFAULT_MAX_HYPERSPACE_POINTS).unwrap();
        let exp_right = hyperspace_metric(&right, DEFAULT_MAX_HYPERSPACE_POINTS).unwrap();
        let (explicit, _) =
            minimum_distortion(&exp_left, &exp_right, DEFAULT_MAX_RELATION_VERTICES).unwrap();
        let (decomposed, _) = minimum_hyperspace_distortion_lazy(
            &left,
            &right,
            &LazyConfig::default(),
            LazyStrategy::Ultrametric,
            None,
        )
        .unwrap();
        assert_eq!(decomposed, explicit);
    }
}

#[test]
fn all_four_point_ultrametric_types_through_level_three_match_explicit_search() {
    fn canonical_four_point_key(metric: &Metric) -> Vec<u32> {
        let mut best: Option<Vec<u32>> = None;
        for a in 0..4 {
            for b in 0..4 {
                for c in 0..4 {
                    for d in 0..4 {
                        let permutation = [a, b, c, d];
                        let distinct =
                            (0..4).all(|i| ((i + 1)..4).all(|j| permutation[i] != permutation[j]));
                        if !distinct {
                            continue;
                        }
                        let mut key = Vec::with_capacity(6);
                        for i in 0..4 {
                            for j in (i + 1)..4 {
                                key.push(metric.get(permutation[i], permutation[j]));
                            }
                        }
                        if best.as_ref().is_none_or(|current| key < *current) {
                            best = Some(key);
                        }
                    }
                }
            }
        }
        best.unwrap()
    }

    let mut representatives = BTreeMap::new();
    for encoded in 0_u32..3_u32.pow(6) {
        let mut cursor = encoded;
        let mut edges = [0_u32; 6];
        for edge in &mut edges {
            *edge = cursor % 3 + 1;
            cursor /= 3;
        }
        if let Ok(metric) = Metric::from_edges(&edges)
            && metric.is_ultrametric()
        {
            representatives
                .entry(canonical_four_point_key(&metric))
                .or_insert(metric);
        }
    }
    let metrics: Vec<Metric> = representatives.into_values().collect();
    assert_eq!(metrics.len(), 14);
    for left_index in 0..metrics.len() {
        for right_index in (left_index + 1)..metrics.len() {
            let left = &metrics[left_index];
            let right = &metrics[right_index];
            let exp_left = hyperspace_metric(left, DEFAULT_MAX_HYPERSPACE_POINTS).unwrap();
            let exp_right = hyperspace_metric(right, DEFAULT_MAX_HYPERSPACE_POINTS).unwrap();
            let (explicit, _) =
                minimum_distortion(&exp_left, &exp_right, DEFAULT_MAX_RELATION_VERTICES).unwrap();
            let (decomposed, _) = minimum_hyperspace_distortion_lazy(
                left,
                right,
                &LazyConfig::default(),
                LazyStrategy::Ultrametric,
                None,
            )
            .unwrap();
            assert_eq!(decomposed, explicit, "left={left:?}, right={right:?}");
        }
    }
}

#[test]
fn structural_filter_accepts_certified_hyperspace_optima() {
    let cases = [
        (
            Metric::from_edges(&KNOWN_LEFT).unwrap(),
            Metric::from_edges(&KNOWN_RIGHT).unwrap(),
        ),
        (
            Metric::from_edges(&[1, 1, 2, 1, 2, 2]).unwrap(),
            Metric::from_edges(&[2, 3, 3, 3, 3, 2]).unwrap(),
        ),
    ];
    for (left, right) in cases {
        let exp_left = hyperspace_metric(&left, DEFAULT_MAX_HYPERSPACE_POINTS).unwrap();
        let exp_right = hyperspace_metric(&right, DEFAULT_MAX_HYPERSPACE_POINTS).unwrap();
        let (optimum, _) =
            minimum_distortion(&exp_left, &exp_right, DEFAULT_MAX_RELATION_VERTICES).unwrap();
        assert!(StructuralFilter::new(&left, &right).accepts(optimum));
    }
}

#[test]
fn equilateral_lazy_formula_matches_unequal_explicit_cardinalities() {
    let left = Metric::from_edges(&[2]).unwrap();
    let right = Metric::from_edges(&[3, 3, 3]).unwrap();
    let exp_left = hyperspace_metric(&left, DEFAULT_MAX_HYPERSPACE_POINTS).unwrap();
    let exp_right = hyperspace_metric(&right, DEFAULT_MAX_HYPERSPACE_POINTS).unwrap();
    let (explicit, _) =
        minimum_distortion(&exp_left, &exp_right, DEFAULT_MAX_RELATION_VERTICES).unwrap();
    let (lazy, _) = minimum_hyperspace_distortion_lazy(
        &left,
        &right,
        &LazyConfig::default(),
        LazyStrategy::Auto,
        None,
    )
    .unwrap();
    assert_eq!(lazy, explicit);
}

#[test]
fn order_twenty_identity_uses_no_explicit_hyperspace() {
    let metric = family_metric("comb_ultrametric", 20).unwrap();
    let (distortion, stats) = minimum_hyperspace_distortion_lazy(
        &metric,
        &metric,
        &LazyConfig::default(),
        LazyStrategy::Auto,
        None,
    )
    .unwrap();
    assert_eq!(distortion, 0);
    assert_eq!(stats.left_dilation.distance_queries, 0);
    assert_eq!(stats.right_dilation.distance_queries, 0);
}

#[test]
#[ignore = "requires the optional z3 command; run with --include-ignored"]
fn optional_z3_backend_resolves_a_hard_five_point_threshold() {
    assert!(
        Command::new("z3")
            .arg("-version")
            .output()
            .expect("install Z3 to run this optional test")
            .status
            .success()
    );
    let left = family_metric("rank_generic", 5).unwrap();
    let right = family_metric("dyadic_line", 5).unwrap();
    let (base, _) = minimum_distortion(&left, &right, DEFAULT_MAX_RELATION_VERTICES).unwrap();
    let (hyperspace, stats) = minimum_hyperspace_distortion_lazy(
        &left,
        &right,
        &LazyConfig::default(),
        LazyStrategy::Z3,
        Some(base),
    )
    .unwrap();
    assert_eq!(hyperspace, base);
    assert!(stats.z3_checks > 0);
}

#[test]
fn two_point_hyperspace_is_equilateral() {
    let base = Metric::from_edges(&[7]).unwrap();
    let hyperspace = hyperspace_metric(&base, DEFAULT_MAX_HYPERSPACE_POINTS).unwrap();
    assert_eq!(hyperspace.order, 3);
    assert_eq!(hyperspace.distance_levels(), BTreeSet::from([0, 7]));
}

#[test]
fn every_recorded_family_builds_through_order_twenty() {
    for family in [
        "equilateral",
        "binary_star",
        "dyadic_line",
        "comb_ultrametric",
        "balanced_ultrametric",
        "rank_generic",
    ] {
        for order in 4..=20 {
            assert_eq!(family_metric(family, order).unwrap().order, order);
        }
    }
}

#[test]
fn bounded_explicit_search_matches_exhaustive_correspondences() {
    let mut metrics = vec![
        Metric::from_edges(&[1]).unwrap(),
        Metric::from_edges(&[3]).unwrap(),
    ];
    for a in 1..=3 {
        for b in 1..=3 {
            for c in 1..=3 {
                if let Ok(metric) = Metric::from_edges(&[a, b, c]) {
                    metrics.push(metric);
                }
            }
        }
    }
    for left in &metrics {
        for right in &metrics {
            let (value, _) = minimum_distortion(left, right, 100).unwrap();
            assert_eq!(
                value,
                brute_force_minimum_distortion(left, right).unwrap(),
                "left={left:?}, right={right:?}"
            );
        }
    }
    // Four-point inputs exercise more than bijective correspondences and
    // the recursive solver and its bounds, with an independent oracle.
    for seed in 0_u32..8 {
        let left_edges: Vec<_> = (0..6).map(|bit| 1 + ((seed * 7 + 13) >> bit & 1)).collect();
        let right_edges: Vec<_> = (0..6).map(|bit| 1 + ((seed * 11 + 3) >> bit & 1)).collect();
        let left = Metric::from_edges(&left_edges).unwrap();
        let right = Metric::from_edges(&right_edges).unwrap();
        assert_eq!(
            minimum_distortion(&left, &right, 100).unwrap().0,
            brute_force_minimum_distortion(&left, &right).unwrap()
        );
    }
}

#[test]
fn integer_endpoints_and_resource_errors_remain_distinct() {
    let left = Metric::from_edges(&[u32::MAX]).unwrap();
    let right = Metric::from_edges(&[u32::MAX - 2]).unwrap();
    assert_eq!(minimum_distortion(&left, &right, 100).unwrap().0, 2);
    assert!(
        minimum_distortion(&left, &right, 1)
            .unwrap_err()
            .contains("safety limit")
    );
    let left = Metric::from_edges(&KNOWN_LEFT).unwrap();
    let right = Metric::from_edges(&KNOWN_RIGHT).unwrap();
    for (config, diagnostic) in [
        (
            LazyConfig {
                max_depth: 1,
                ..LazyConfig::default()
            },
            "depth limit",
        ),
        (
            LazyConfig {
                max_search_nodes: 0,
                ..LazyConfig::default()
            },
            "search-node limit",
        ),
        (
            LazyConfig {
                max_candidate_scans: 0,
                ..LazyConfig::default()
            },
            "candidate-scan limit",
        ),
    ] {
        let error =
            minimum_hyperspace_distortion_lazy(&left, &right, &config, LazyStrategy::Generic, None)
                .unwrap_err();
        assert!(error.contains(diagnostic), "{error}");
    }
}

#[test]
fn cache_free_search_and_unequal_ultrametrics_match_explicit_search() {
    let bases = [
        Metric::from_edges(&[2]).unwrap(),
        Metric::from_edges(&[1, 3, 3]).unwrap(),
        Metric::from_edges(&[1, 3, 3, 3, 3, 2]).unwrap(),
    ];
    for left in &bases {
        for right in &bases {
            let le = hyperspace_metric(left, 127).unwrap();
            let re = hyperspace_metric(right, 127).unwrap();
            let expected = minimum_distortion(&le, &re, 20000).unwrap().0;
            let config = LazyConfig {
                max_distance_cache: 0,
                ..LazyConfig::default()
            };
            for strategy in [LazyStrategy::Generic, LazyStrategy::Ultrametric] {
                assert_eq!(
                    minimum_hyperspace_distortion_lazy(left, right, &config, strategy, None)
                        .unwrap()
                        .0,
                    expected
                );
            }
        }
    }
}

#[test]
fn symmetry_reduction_matches_all_binary_four_point_metric_types() {
    let permutations: Vec<_> = (0..4)
        .flat_map(|a| {
            (0..4).flat_map(move |b| (0..4).flat_map(move |c| (0..4).map(move |d| [a, b, c, d])))
        })
        .filter(|p| (0..4).all(|i| (i + 1..4).all(|j| p[i] != p[j])))
        .collect();
    let mut representatives = BTreeMap::new();
    for mask in 0..64 {
        let edges: Vec<_> = (0..6).map(|bit| 1 + ((mask >> bit) & 1)).collect();
        let metric = Metric::from_edges(&edges).unwrap();
        let key = permutations
            .iter()
            .map(|p| {
                let mut edges = Vec::new();
                for i in 0..4 {
                    for j in i + 1..4 {
                        edges.push(metric.get(p[i], p[j]));
                    }
                }
                edges
            })
            .min()
            .unwrap();
        representatives.entry(key).or_insert(metric);
    }
    let metrics: Vec<_> = representatives.into_values().collect();
    assert_eq!(metrics.len(), 11);
    for (i, left) in metrics.iter().enumerate() {
        for right in &metrics[i..] {
            // Relabel one side so the test is not confined to canonical ordering.
            let right = Metric::from_rule(4, |a, b| right.get((a + 1) % 4, (b + 1) % 4)).unwrap();
            let expected = brute_force_minimum_distortion(left, &right).unwrap();
            assert_eq!(minimum_distortion(left, &right, 100).unwrap().0, expected);
        }
    }
}
