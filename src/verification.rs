use crate::config::*;
use crate::explicit::minimum_distortion;
use crate::hyperspace::{hyperspace_metric, minimum_hyperspace_distortion_lazy};
use crate::metric::Metric;

pub(crate) fn brute_force_minimum_distortion(left: &Metric, right: &Metric) -> Result<u32, String> {
    let vertex_count = left.order * right.order;
    if vertex_count > 20 {
        return Err("brute-force check is limited to 20 relation vertices".to_string());
    }
    let mut best = u32::MAX;
    for relation in 1_u64..(1_u64 << vertex_count) {
        let mut rows = 0_u64;
        let mut columns = 0_u64;
        for vertex in 0..vertex_count {
            if relation & (1_u64 << vertex) != 0 {
                rows |= 1_u64 << (vertex / right.order);
                columns |= 1_u64 << (vertex % right.order);
            }
        }
        if rows.count_ones() as usize != left.order || columns.count_ones() as usize != right.order
        {
            continue;
        }
        let mut distortion = 0;
        for first in 0..vertex_count {
            if relation & (1_u64 << first) == 0 {
                continue;
            }
            let i = first / right.order;
            let j = first % right.order;
            for second in 0..vertex_count {
                if relation & (1_u64 << second) == 0 {
                    continue;
                }
                let k = second / right.order;
                let ell = second % right.order;
                distortion = distortion.max(left.get(i, k).abs_diff(right.get(j, ell)));
            }
        }
        best = best.min(distortion);
    }
    Ok(best)
}

pub(crate) fn run_self_test() -> Result<(), String> {
    let small_cases = [
        (vec![1], vec![3]),
        (vec![2], vec![2, 3, 4]),
        (vec![2, 3, 4], vec![3, 4, 5]),
    ];
    for (index, (left_edges, right_edges)) in small_cases.iter().enumerate() {
        let left = Metric::from_edges(left_edges)?;
        let right = Metric::from_edges(right_edges)?;
        let (exact, _) = minimum_distortion(&left, &right, DEFAULT_MAX_RELATION_VERTICES)?;
        let brute = brute_force_minimum_distortion(&left, &right)?;
        if exact != brute {
            return Err(format!(
                "self-test case {index} differs: exact={exact}, brute={brute}"
            ));
        }
    }

    let left = Metric::from_edges(&KNOWN_LEFT)?;
    let right = Metric::from_edges(&KNOWN_RIGHT)?;
    let (base, _) = minimum_distortion(&left, &right, DEFAULT_MAX_RELATION_VERTICES)?;
    let brute = brute_force_minimum_distortion(&left, &right)?;
    if base != 484 || base != brute {
        return Err(format!(
            "known base pair differs: exact={base}, brute={brute}, expected=484"
        ));
    }
    let exp_left = hyperspace_metric(&left, DEFAULT_MAX_HYPERSPACE_POINTS)?;
    let exp_right = hyperspace_metric(&right, DEFAULT_MAX_HYPERSPACE_POINTS)?;
    let (hyperspace, _) = minimum_distortion(&exp_left, &exp_right, DEFAULT_MAX_RELATION_VERTICES)?;
    if hyperspace != 484 {
        return Err(format!(
            "known hyperspace pair differs: exact={hyperspace}, expected=484"
        ));
    }
    let config = LazyConfig::default();
    let (lazy_hyperspace, _) =
        minimum_hyperspace_distortion_lazy(&left, &right, &config, LazyStrategy::Generic, None)?;
    if lazy_hyperspace != hyperspace {
        return Err(format!(
            "lazy known-pair result differs: lazy={lazy_hyperspace}, explicit={hyperspace}"
        ));
    }

    let ultrametric_left = Metric::from_edges(&[1, 1, 2, 1, 2, 2])?;
    let ultrametric_right = Metric::from_edges(&[2, 3, 3, 3, 3, 2])?;
    let (ultrametric_hyperspace, _) = minimum_hyperspace_distortion_lazy(
        &ultrametric_left,
        &ultrametric_right,
        &config,
        LazyStrategy::Ultrametric,
        None,
    )?;
    if ultrametric_hyperspace != 2 {
        return Err(format!(
            "ultrametric decomposition differs: exact={ultrametric_hyperspace}, expected=2"
        ));
    }
    println!(
        "SELF_TEST_OK small_cases={} known_base={} known_hyperspace={} \
         lazy_hyperspace={} ultrametric_hyperspace={}",
        small_cases.len(),
        base,
        hyperspace,
        lazy_hyperspace,
        ultrametric_hyperspace,
    );
    Ok(())
}
