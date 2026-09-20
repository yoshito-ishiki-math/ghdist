use crate::bitset::BitSet;
use crate::bounds::{
    candidate_thresholds, labelled_correspondence_upper_bound, structural_lower_bound,
};
use crate::metric::Metric;
use std::collections::HashSet;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct FailedState {
    pub(crate) candidates: BitSet,
    pub(crate) covered_rows: BitSet,
    pub(crate) covered_columns: BitSet,
}

#[derive(Default, Debug)]
pub(crate) struct FeasibilityStats {
    pub(crate) nodes: u64,
    pub(crate) compatibility_masks_built: u64,
    pub(crate) memo_hits: u64,
    pub(crate) failed_states: usize,
}

type ExplicitWitnessResult = (bool, FeasibilityStats, Vec<(usize, usize)>);

pub(crate) struct CorrespondenceSolver<'a> {
    pub(crate) left: &'a Metric,
    pub(crate) right: &'a Metric,
    left_twins: &'a [usize],
    right_twins: &'a [usize],
    pub(crate) epsilon: u32,
    pub(crate) vertex_count: usize,
    pub(crate) row_masks: Vec<BitSet>,
    pub(crate) column_masks: Vec<BitSet>,
    pub(crate) compatibility: Vec<Option<BitSet>>,
    pub(crate) failed: HashSet<FailedState>,
    pub(crate) stats: FeasibilityStats,
    selected: Vec<(usize, usize)>,
}

impl<'a> CorrespondenceSolver<'a> {
    pub(crate) fn new(
        left: &'a Metric,
        right: &'a Metric,
        epsilon: u32,
        left_twins: &'a [usize],
        right_twins: &'a [usize],
    ) -> Result<Self, String> {
        let vertex_count = left
            .order
            .checked_mul(right.order)
            .ok_or_else(|| "relation-vertex count overflowed usize".to_string())?;
        let mut row_masks = vec![BitSet::empty(vertex_count); left.order];
        let mut column_masks = vec![BitSet::empty(vertex_count); right.order];
        for (i, row_mask) in row_masks.iter_mut().enumerate() {
            for (j, column_mask) in column_masks.iter_mut().enumerate() {
                let vertex = i * right.order + j;
                row_mask.set(vertex);
                column_mask.set(vertex);
            }
        }
        Ok(Self {
            left,
            right,
            left_twins,
            right_twins,
            epsilon,
            vertex_count,
            row_masks,
            column_masks,
            compatibility: vec![None; vertex_count],
            failed: HashSet::new(),
            stats: FeasibilityStats::default(),
            selected: Vec::new(),
        })
    }

    pub(crate) fn solve(self) -> Result<(bool, FeasibilityStats), String> {
        let (result, stats, _) = self.solve_with_witness()?;
        Ok((result, stats))
    }

    pub(crate) fn solve_with_witness(mut self) -> Result<ExplicitWitnessResult, String> {
        let mut candidates = BitSet::full(self.vertex_count);
        let eccentricities = |metric: &Metric| -> Vec<u32> {
            (0..metric.order)
                .map(|point| {
                    (0..metric.order)
                        .map(|other| metric.get(point, other))
                        .max()
                        .unwrap_or(0)
                })
                .collect()
        };
        let left_eccentricities = eccentricities(self.left);
        let right_eccentricities = eccentricities(self.right);
        // Surjectivity of a correspondence implies that related points have
        // eccentricities differing by at most its distortion.
        for (i, left) in left_eccentricities.iter().enumerate() {
            for (j, right) in right_eccentricities.iter().enumerate() {
                if left.abs_diff(*right) > self.epsilon {
                    candidates.clear(i * self.right.order + j);
                }
            }
        }
        let covered_rows = BitSet::empty(self.left.order);
        let covered_columns = BitSet::empty(self.right.order);
        let result = self.search(candidates, covered_rows, covered_columns)?;
        self.stats.failed_states = self.failed.len();
        Ok((result, self.stats, self.selected))
    }

    fn compatibility_mask(&mut self, vertex: usize) -> &BitSet {
        self.compatibility[vertex].get_or_insert_with(|| {
            let i = vertex / self.right.order;
            let j = vertex % self.right.order;
            let mut mask = BitSet::empty(self.vertex_count);
            for k in 0..self.left.order {
                for ell in 0..self.right.order {
                    if self.left.get(i, k).abs_diff(self.right.get(j, ell)) <= self.epsilon {
                        mask.set(k * self.right.order + ell);
                    }
                }
            }
            self.stats.compatibility_masks_built += 1;
            mask
        })
    }

    pub(crate) fn search(
        &mut self,
        candidates: BitSet,
        covered_rows: BitSet,
        covered_columns: BitSet,
    ) -> Result<bool, String> {
        self.stats.nodes += 1;
        crate::runtime::node(self.stats.nodes)?;
        if covered_rows.count() == self.left.order as u64
            && covered_columns.count() == self.right.order as u64
        {
            return Ok(true);
        }

        let state = FailedState {
            candidates: candidates.clone(),
            covered_rows: covered_rows.clone(),
            covered_columns: covered_columns.clone(),
        };
        if self.failed.contains(&state) {
            self.stats.memo_hits += 1;
            return Ok(false);
        }

        let mut option_count = u64::MAX;
        let mut options = None;
        for row in 0..self.left.order {
            if covered_rows.contains(row) {
                continue;
            }
            let count = candidates.intersection_count(&self.row_masks[row]);
            if count == 0 {
                self.failed.insert(state);
                return Ok(false);
            }
            if count < option_count {
                option_count = count;
                options = Some((&self.row_masks[row], true));
            }
        }
        for column in 0..self.right.order {
            if covered_columns.contains(column) {
                continue;
            }
            let count = candidates.intersection_count(&self.column_masks[column]);
            if count == 0 {
                self.failed.insert(state);
                return Ok(false);
            }
            if count < option_count {
                option_count = count;
                options = Some((&self.column_masks[column], false));
            }
        }

        let (mask, fixed_row) = options.expect("an uncovered row or column must remain");
        let mut options = candidates.intersection(mask);
        let mut tried_twins = vec![
            false;
            if fixed_row {
                self.right.order
            } else {
                self.left.order
            }
        ];
        while let Some(vertex) = options.pop_first() {
            let i = vertex / self.right.order;
            let j = vertex % self.right.order;
            let (covered, representative) = if fixed_row {
                (covered_columns.contains(j), self.right_twins[j])
            } else {
                (covered_rows.contains(i), self.left_twins[i])
            };
            // Swapping two unused twins fixes every selected pair and is an
            // isometry, so their branches have identical feasibility. Covered
            // points are never identified by this reduction.
            if !covered {
                if tried_twins[representative] {
                    continue;
                }
                tried_twins[representative] = true;
            }
            let compatibility = self.compatibility_mask(vertex);
            let next_candidates = candidates.intersection(compatibility);
            let mut next_rows = covered_rows.clone();
            let mut next_columns = covered_columns.clone();
            next_rows.set(i);
            next_columns.set(j);
            self.selected.push((i, j));
            if self.search(next_candidates, next_rows, next_columns)? {
                return Ok(true);
            }
            self.selected.pop();
        }
        self.failed.insert(state);
        Ok(false)
    }
}

#[derive(Default, Debug)]
pub(crate) struct ExactStats {
    pub(crate) candidate_thresholds: usize,
    pub(crate) feasibility_checks: u64,
    pub(crate) search_nodes: u64,
    pub(crate) compatibility_masks_built: u64,
    pub(crate) memo_hits: u64,
    pub(crate) failed_states: u64,
}

pub(crate) fn minimum_distortion(
    left: &Metric,
    right: &Metric,
    max_relation_vertices: usize,
) -> Result<(u32, ExactStats), String> {
    let relation_vertices = left
        .order
        .checked_mul(right.order)
        .ok_or_else(|| "relation-vertex count overflowed usize".to_string())?;
    if relation_vertices > max_relation_vertices {
        return Err(format!(
            "refusing {relation_vertices} relation vertices; safety limit is \
             {max_relation_vertices} (override with --max-relation-vertices)"
        ));
    }
    crate::runtime::check()?;
    let thresholds = candidate_thresholds(left, right);
    let mut stats = ExactStats {
        candidate_thresholds: thresholds.len(),
        ..ExactStats::default()
    };
    // Both bounds are for distortion (twice d_GH). The upper bound is
    // witnessed by a labelled correspondence; no search is needed there.
    let lower_bound = structural_lower_bound(left, right);
    let upper_bound = labelled_correspondence_upper_bound(left, right);
    let mut lower = thresholds.partition_point(|value| *value < lower_bound);
    let mut upper = thresholds
        .binary_search(&upper_bound)
        .map_err(|_| "correspondence upper bound is not a candidate threshold")?;
    if lower > upper {
        return Err("structural lower bound exceeds correspondence upper bound".into());
    }
    crate::runtime::bounds(
        thresholds[lower],
        thresholds[upper],
        "structural lower bound",
    );
    crate::runtime::check()?;
    if lower == upper {
        return Ok((thresholds[lower], stats));
    }
    let left_twins = left.twin_classes();
    let right_twins = right.twin_classes();
    while lower < upper {
        let middle = lower + (upper - lower) / 2;
        let solver =
            CorrespondenceSolver::new(left, right, thresholds[middle], &left_twins, &right_twins)?;
        let (feasible, run) = solver.solve()?;
        stats.feasibility_checks += 1;
        stats.search_nodes += run.nodes;
        stats.compatibility_masks_built += run.compatibility_masks_built;
        stats.memo_hits += run.memo_hits;
        stats.failed_states += run.failed_states as u64;
        if feasible {
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
    Ok((thresholds[lower], stats))
}

pub(crate) fn correspondence(
    left: &Metric,
    right: &Metric,
    epsilon: u32,
    max_vertices: usize,
) -> Result<Vec<(usize, usize)>, String> {
    if left
        .order
        .checked_mul(right.order)
        .is_none_or(|n| n > max_vertices)
    {
        return Err("correspondence extraction exceeds the relation-vertex limit".into());
    }
    let left_twins = left.twin_classes();
    let right_twins = right.twin_classes();
    let solver = CorrespondenceSolver::new(left, right, epsilon, &left_twins, &right_twins)?;
    let (feasible, _, pairs) = solver.solve_with_witness()?;
    if !feasible {
        return Err("no correspondence exists at the reported upper bound".into());
    }
    Ok(pairs)
}
