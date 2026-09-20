use super::generic::{LazyCorrespondenceSolver, hyperspace_point_order};
use super::ultrametric::UltrametricRecursor;
use super::{DilationStats, HausdorffOracle};
use crate::bounds::{
    StructuralFilter, StructuralStats, candidate_thresholds, labelled_correspondence_upper_bound,
    structural_lower_bound,
};
use crate::config::{
    AUTO_Z3_MIN_RELATION_VERTICES, AUTO_Z3_RELATION_VERTICES, LazyConfig, LazyStrategy,
};
use crate::metric::Metric;

#[derive(Default, Debug)]
pub(crate) struct LazyExactStats {
    pub(crate) candidate_thresholds: usize,
    pub(crate) feasibility_checks: u64,
    pub(crate) generic_checks: u64,
    pub(crate) ultrametric_checks: u64,
    pub(crate) ultrametric_fallbacks: u64,
    pub(crate) z3_checks: u64,
    pub(crate) z3_fallbacks: u64,
    pub(crate) z3_variables: usize,
    pub(crate) z3_clauses: u64,
    pub(crate) lazy_search_nodes: u64,
    pub(crate) candidate_scans: u64,
    pub(crate) compatibility_checks: u64,
    pub(crate) lazy_memo_hits: u64,
    pub(crate) maximum_depth: usize,
    pub(crate) ultrametric_recursive_calls: u64,
    pub(crate) ultrametric_assignment_candidates: u64,
    pub(crate) ultrametric_maximum_block_count: usize,
    pub(crate) left_dilation: DilationStats,
    pub(crate) right_dilation: DilationStats,
    pub(crate) structural: StructuralStats,
}

pub(crate) struct LazyHyperspaceEngine<'metric, 'config> {
    pub(crate) left_base: &'metric Metric,
    pub(crate) right_base: &'metric Metric,
    pub(crate) left: HausdorffOracle<'metric>,
    pub(crate) right: HausdorffOracle<'metric>,
    pub(crate) config: &'config LazyConfig,
    pub(crate) strategy: LazyStrategy,
    pub(crate) left_eccentricities: Option<Vec<u32>>,
    pub(crate) right_eccentricities: Option<Vec<u32>>,
    pub(crate) left_point_order: Option<Vec<u64>>,
    pub(crate) right_point_order: Option<Vec<u64>>,
    pub(crate) structural: StructuralFilter<'metric>,
    pub(crate) stats: LazyExactStats,
}

impl<'metric, 'config> LazyHyperspaceEngine<'metric, 'config> {
    pub(crate) fn new(
        left_base: &'metric Metric,
        right_base: &'metric Metric,
        config: &'config LazyConfig,
        strategy: LazyStrategy,
    ) -> Result<Self, String> {
        if strategy == LazyStrategy::Ultrametric
            && (!left_base.is_ultrametric() || !right_base.is_ultrametric())
        {
            return Err("the ultrametric strategy requires two ultrametric bases".to_string());
        }
        let left = HausdorffOracle::new(left_base, config.max_distance_cache)?;
        let right = HausdorffOracle::new(right_base, config.max_distance_cache)?;

        Ok(Self {
            left_base,
            right_base,
            left,
            right,
            config,
            strategy,
            left_eccentricities: None,
            right_eccentricities: None,
            left_point_order: None,
            right_point_order: None,
            structural: StructuralFilter::new(left_base, right_base),
            stats: LazyExactStats::default(),
        })
    }

    pub(crate) fn equilateral_feasible(&self, epsilon: u32) -> Option<bool> {
        let left_distance = self.left_base.is_equilateral()?;
        let right_distance = self.right_base.is_equilateral()?;
        let left_count = self.left.point_count();
        let right_count = self.right.point_count();
        Some(
            left_distance.abs_diff(right_distance) <= epsilon
                && (left_distance <= epsilon || left_count <= right_count)
                && (right_distance <= epsilon || right_count <= left_count),
        )
    }

    fn ensure_eccentricities(&mut self) {
        self.left_eccentricities
            .get_or_insert_with(|| self.left.subset_eccentricities());
        self.right_eccentricities
            .get_or_insert_with(|| self.right.subset_eccentricities());
    }

    pub(crate) fn generic_feasible(&mut self, epsilon: u32) -> Result<bool, String> {
        let required_depth = self.left.point_count().max(self.right.point_count());
        if required_depth > self.config.max_depth {
            return Err(format!(
                "generic lazy search needs at least {required_depth} covering edges, above the depth limit {}",
                self.config.max_depth
            ));
        }
        self.ensure_eccentricities();
        if self.left_point_order.is_none() {
            self.left_point_order = Some(hyperspace_point_order(
                self.left_eccentricities.as_ref().unwrap(),
            ));
        }
        if self.right_point_order.is_none() {
            self.right_point_order = Some(hyperspace_point_order(
                self.right_eccentricities.as_ref().unwrap(),
            ));
        }
        let solver = LazyCorrespondenceSolver::new(
            &mut self.left,
            &mut self.right,
            epsilon,
            self.config,
            self.left_eccentricities.as_ref().unwrap(),
            self.right_eccentricities.as_ref().unwrap(),
            self.left_point_order.as_ref().unwrap(),
            self.right_point_order.as_ref().unwrap(),
        )?;
        let (result, run) = solver.solve()?;
        self.stats.generic_checks += 1;
        self.stats.lazy_search_nodes += run.search_nodes;
        self.stats.candidate_scans += run.candidate_scans;
        self.stats.compatibility_checks += run.compatibility_checks;
        self.stats.lazy_memo_hits += run.memo_hits;
        self.stats.maximum_depth = self.stats.maximum_depth.max(run.maximum_depth);
        Ok(result)
    }

    pub(crate) fn ultrametric_feasible(&mut self, epsilon: u32) -> Result<bool, String> {
        let left_points: Vec<u64> = (1..=self.left.point_count() as u64).collect();
        let right_points: Vec<u64> = (1..=self.right.point_count() as u64).collect();
        let mut recursor = UltrametricRecursor::new(self.config);
        let result = recursor.decide(
            &left_points,
            &right_points,
            &mut self.left,
            &mut self.right,
            epsilon,
            false,
        );
        self.stats.ultrametric_checks += 1;
        self.stats.ultrametric_recursive_calls += recursor.stats.recursive_calls;
        self.stats.ultrametric_assignment_candidates += recursor.stats.assignment_candidates;
        self.stats.ultrametric_maximum_block_count = self
            .stats
            .ultrametric_maximum_block_count
            .max(recursor.stats.maximum_block_count);
        result
    }

    pub(crate) fn z3_feasible(&mut self, epsilon: u32) -> Result<bool, String> {
        self.ensure_eccentricities();
        let (result, stats) = super::z3::feasible(
            &mut self.left,
            &mut self.right,
            self.left_eccentricities.as_ref().unwrap(),
            self.right_eccentricities.as_ref().unwrap(),
            epsilon,
            self.config,
        )?;
        self.stats.z3_checks += stats.checks;
        self.stats.z3_variables = self.stats.z3_variables.max(stats.variables);
        self.stats.z3_clauses = self.stats.z3_clauses.max(stats.clauses);
        Ok(result)
    }

    pub(crate) fn feasible(&mut self, epsilon: u32) -> Result<bool, String> {
        crate::runtime::check()?;
        self.stats.feasibility_checks += 1;
        if !self.structural.accepts(epsilon) {
            return Ok(false);
        }
        if let Some(result) = self.equilateral_feasible(epsilon) {
            return Ok(result);
        }
        if self.strategy == LazyStrategy::Z3 {
            return self.z3_feasible(epsilon);
        }
        let use_ultrametric = self.strategy != LazyStrategy::Generic
            && self.left_base.is_ultrametric()
            && self.right_base.is_ultrametric();
        if use_ultrametric {
            match self.ultrametric_feasible(epsilon) {
                Ok(result) => return Ok(result),
                Err(ultrametric_limit) if self.strategy == LazyStrategy::Auto => {
                    self.stats.ultrametric_fallbacks += 1;
                    return self.generic_feasible(epsilon).map_err(|generic_limit| {
                        format!(
                            "ultrametric decomposition stopped ({ultrametric_limit}); \
                             generic fallback stopped ({generic_limit})"
                        )
                    });
                }
                Err(error) => return Err(error),
            }
        }
        let implicit_relation_vertices = self.left.point_count() * self.right.point_count();
        // Tiny instances are faster in the in-process star-forest solver;
        // medium instances benefit from a bounded exact SAT encoding.
        if self.strategy == LazyStrategy::Auto
            && (AUTO_Z3_MIN_RELATION_VERTICES..=AUTO_Z3_RELATION_VERTICES)
                .contains(&implicit_relation_vertices)
        {
            match self.z3_feasible(epsilon) {
                Ok(result) => return Ok(result),
                Err(_) => self.stats.z3_fallbacks += 1,
            }
        }
        self.generic_feasible(epsilon)
    }

    pub(crate) fn finish(mut self) -> LazyExactStats {
        self.stats.left_dilation = self.left.stats;
        self.stats.right_dilation = self.right.stats;
        self.stats.structural = self.structural.stats;
        self.stats
    }
}

pub(crate) fn minimum_hyperspace_distortion_lazy(
    left: &Metric,
    right: &Metric,
    config: &LazyConfig,
    strategy: LazyStrategy,
    certified_upper_bound: Option<u32>,
) -> Result<(u32, LazyExactStats), String> {
    crate::runtime::check()?;
    let thresholds = candidate_thresholds(left, right);
    let upper_bound =
        certified_upper_bound.unwrap_or_else(|| labelled_correspondence_upper_bound(left, right));
    let lower_bound = structural_lower_bound(left, right);
    let lower_index = thresholds.partition_point(|value| *value < lower_bound);
    let upper_index = thresholds
        .binary_search(&upper_bound)
        .map_err(|_| "the explicit correspondence upper bound is not a candidate threshold")?;
    if lower_index > upper_index {
        return Err(format!(
            "structural lower bound {lower_bound} exceeds correspondence upper bound \
             {upper_bound}"
        ));
    }

    crate::runtime::bounds(
        thresholds[lower_index],
        thresholds[upper_index],
        "structural lower bound",
    );
    crate::runtime::check()?;
    let mut engine = LazyHyperspaceEngine::new(left, right, config, strategy)?;
    engine.stats.candidate_thresholds = thresholds.len();
    if left == right {
        return Ok((0, engine.finish()));
    }
    if !engine.structural.accepts(upper_bound) {
        return Err("structural filter rejected a certified lifted upper bound".to_string());
    }

    let mut lower = lower_index;
    let mut upper = upper_index;
    while lower < upper {
        let middle = lower + (upper - lower) / 2;
        if engine.feasible(thresholds[middle])? {
            upper = middle;
        } else {
            lower = middle + 1;
        }
        crate::runtime::bounds(
            thresholds[lower],
            thresholds[upper],
            &format!("exact infeasibility below distortion {}", thresholds[lower]),
        );
    }
    Ok((thresholds[lower], engine.finish()))
}
