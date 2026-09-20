use super::HausdorffOracle;
use crate::config::LazyConfig;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct UltrametricMemoKey {
    pub(crate) orientation_swapped: bool,
    pub(crate) epsilon: u32,
    pub(crate) left_points: Vec<u64>,
    pub(crate) right_points: Vec<u64>,
}

#[derive(Default, Debug)]
pub(crate) struct UltrametricStats {
    pub(crate) recursive_calls: u64,
    pub(crate) memo_hits: u64,
    pub(crate) partitions_built: u64,
    pub(crate) assignment_candidates: u64,
    pub(crate) maximum_block_count: usize,
}

enum WitnessPlan {
    Collapsed,
    Swapped,
    Blocks(Vec<(Vec<u64>, Vec<u64>)>),
}

pub(crate) struct UltrametricRecursor<'config> {
    pub(crate) config: &'config LazyConfig,
    pub(crate) memo: HashMap<UltrametricMemoKey, bool>,
    pub(crate) stats: UltrametricStats,
    capture: bool,
    plans: HashMap<UltrametricMemoKey, WitnessPlan>,
}

impl<'config> UltrametricRecursor<'config> {
    pub(crate) fn new(config: &'config LazyConfig) -> Self {
        Self {
            config,
            memo: HashMap::new(),
            stats: UltrametricStats::default(),
            capture: false,
            plans: HashMap::new(),
        }
    }

    pub(crate) fn subspace_diameter(points: &[u64], oracle: &mut HausdorffOracle<'_>) -> u32 {
        let first = points[0];
        points
            .iter()
            .map(|point| oracle.distance(first, *point))
            .max()
            .unwrap_or(0)
    }

    pub(crate) fn union_blocks(blocks: &[Vec<u64>], mask: u64) -> Vec<u64> {
        let mut points = Vec::new();
        for (index, block) in blocks.iter().enumerate() {
            if mask & (1_u64 << index) != 0 {
                points.extend(block);
            }
        }
        points.sort_unstable();
        points
    }

    pub(crate) fn decide(
        &mut self,
        left_points: &[u64],
        right_points: &[u64],
        left_oracle: &mut HausdorffOracle<'_>,
        right_oracle: &mut HausdorffOracle<'_>,
        epsilon: u32,
        orientation_swapped: bool,
    ) -> Result<bool, String> {
        crate::runtime::check()?;
        self.stats.recursive_calls += 1;
        if self.stats.recursive_calls > self.config.max_search_nodes {
            return Err(format!(
                "ultrametric recursion-call limit {} reached",
                self.config.max_search_nodes
            ));
        }
        let memoizable = self.capture || left_points.len() + right_points.len() <= 4_096;
        let key = memoizable.then(|| UltrametricMemoKey {
            orientation_swapped,
            epsilon,
            left_points: left_points.to_vec(),
            right_points: right_points.to_vec(),
        });
        if let Some(cached) = key.as_ref().and_then(|value| self.memo.get(value)) {
            self.stats.memo_hits += 1;
            return Ok(*cached);
        }

        let left_diameter = Self::subspace_diameter(left_points, left_oracle);
        let right_diameter = Self::subspace_diameter(right_points, right_oracle);
        let mut plan = None;
        let result = if left_diameter.abs_diff(right_diameter) > epsilon {
            false
        } else if left_diameter.max(right_diameter) <= epsilon {
            plan = Some(WitnessPlan::Collapsed);
            true
        } else if right_diameter <= epsilon {
            let result = self.decide(
                right_points,
                left_points,
                right_oracle,
                left_oracle,
                epsilon,
                !orientation_swapped,
            )?;
            if result {
                plan = Some(WitnessPlan::Swapped);
            }
            result
        } else {
            let left_threshold = right_diameter - epsilon;
            // Distinct top blocks on the right are exactly right_diameter
            // apart.  An epsilon-correspondence cannot send one open
            // (< right_diameter - epsilon) block on the left to two of them.
            // For ultrametrics the converse gluing is exact because all
            // inter-block distances are constant at the relevant level.
            let left_blocks = left_oracle.partition_by_open_support(left_points, left_threshold);
            let mut right_blocks =
                right_oracle.partition_by_open_support(right_points, right_diameter);
            self.stats.partitions_built += 2;
            self.stats.maximum_block_count = self
                .stats
                .maximum_block_count
                .max(left_blocks.len())
                .max(right_blocks.len());

            if left_blocks.len() < right_blocks.len() {
                false
            } else {
                if left_blocks.len() > self.config.max_ultrametric_blocks || left_blocks.len() >= 64
                {
                    return Err(format!(
                        "ultrametric decomposition produced {} source blocks, above the \
                         limit {}",
                        left_blocks.len(),
                        self.config.max_ultrametric_blocks
                    ));
                }
                right_blocks.sort_unstable_by_key(|block| usize::MAX - block.len());
                let full = (1_u64 << left_blocks.len()) - 1;
                let mut local_cache = vec![HashMap::new(); right_blocks.len()];
                let mut solution = Vec::new();
                let result = self.assign_blocks(
                    0,
                    full,
                    &left_blocks,
                    &right_blocks,
                    left_oracle,
                    right_oracle,
                    epsilon,
                    orientation_swapped,
                    &mut local_cache,
                    &mut HashSet::new(),
                    &mut solution,
                )?;
                if result && self.capture {
                    plan = Some(WitnessPlan::Blocks(
                        solution
                            .into_iter()
                            .zip(&right_blocks)
                            .map(|(mask, target)| {
                                (Self::union_blocks(&left_blocks, mask), target.clone())
                            })
                            .collect(),
                    ));
                }
                result
            }
        };
        if let Some(key) = key {
            if result && self.capture {
                self.plans
                    .insert(key.clone(), plan.ok_or("missing ultrametric witness plan")?);
            }
            self.memo.insert(key, result);
        }
        Ok(result)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn assign_blocks(
        &mut self,
        target_index: usize,
        remaining: u64,
        left_blocks: &[Vec<u64>],
        right_blocks: &[Vec<u64>],
        left_oracle: &mut HausdorffOracle<'_>,
        right_oracle: &mut HausdorffOracle<'_>,
        epsilon: u32,
        orientation_swapped: bool,
        local_cache: &mut [HashMap<u64, bool>],
        failed_assignments: &mut HashSet<(usize, u64)>,
        solution: &mut Vec<u64>,
    ) -> Result<bool, String> {
        crate::runtime::check()?;
        if target_index == right_blocks.len() {
            return Ok(remaining == 0);
        }
        if failed_assignments.contains(&(target_index, remaining)) {
            return Ok(false);
        }
        let targets_left = right_blocks.len() - target_index;
        if (remaining.count_ones() as usize) < targets_left {
            return Ok(false);
        }

        let singletons_only = remaining.count_ones() as usize == targets_left;
        // If only one target remains it must receive every source block.
        // If counts agree each target must receive exactly one source block.
        let mut singleton_choices = remaining;
        let mut subset = if singletons_only {
            remaining & remaining.wrapping_neg()
        } else {
            remaining
        };
        while subset != 0 {
            self.stats.assignment_candidates += 1;
            if self.stats.assignment_candidates.is_multiple_of(1024) {
                crate::runtime::check()?;
            }
            if self.stats.assignment_candidates > self.config.max_ultrametric_assignments {
                return Err(format!(
                    "ultrametric block-assignment limit {} reached",
                    self.config.max_ultrametric_assignments
                ));
            }
            let leftover = remaining ^ subset;
            if leftover.count_ones() as usize + 1 >= targets_left {
                let feasible = if let Some(value) = local_cache[target_index].get(&subset) {
                    *value
                } else {
                    let source_points = Self::union_blocks(left_blocks, subset);
                    let value = self.decide(
                        &source_points,
                        &right_blocks[target_index],
                        left_oracle,
                        right_oracle,
                        epsilon,
                        orientation_swapped,
                    )?;
                    local_cache[target_index].insert(subset, value);
                    value
                };
                if feasible {
                    solution.push(subset);
                    if self.assign_blocks(
                        target_index + 1,
                        leftover,
                        left_blocks,
                        right_blocks,
                        left_oracle,
                        right_oracle,
                        epsilon,
                        orientation_swapped,
                        local_cache,
                        failed_assignments,
                        solution,
                    )? {
                        return Ok(true);
                    }
                    solution.pop();
                }
            }
            if targets_left == 1 {
                break;
            }
            if singletons_only {
                singleton_choices &= singleton_choices - 1;
                subset = singleton_choices & singleton_choices.wrapping_neg();
            } else {
                subset = (subset - 1) & remaining;
            }
        }
        failed_assignments.insert((target_index, remaining));
        Ok(false)
    }
    fn collect_witness(
        &self,
        key: &UltrametricMemoKey,
        pairs: &mut Vec<(u64, u64)>,
        max_pairs: usize,
    ) -> Result<(), String> {
        crate::runtime::check()?;
        match self.plans.get(key).ok_or("missing cached witness plan")? {
            WitnessPlan::Collapsed => {
                for i in 0..key.left_points.len().max(key.right_points.len()) {
                    if pairs.len() >= max_pairs {
                        return Err("witness pair limit reached".into());
                    }
                    let a = key.left_points[i % key.left_points.len()];
                    let b = key.right_points[i % key.right_points.len()];
                    pairs.push(if key.orientation_swapped {
                        (b, a)
                    } else {
                        (a, b)
                    });
                }
            }
            WitnessPlan::Swapped => self.collect_witness(
                &UltrametricMemoKey {
                    orientation_swapped: !key.orientation_swapped,
                    epsilon: key.epsilon,
                    left_points: key.right_points.clone(),
                    right_points: key.left_points.clone(),
                },
                pairs,
                max_pairs,
            )?,
            WitnessPlan::Blocks(blocks) => {
                for (left, right) in blocks {
                    self.collect_witness(
                        &UltrametricMemoKey {
                            orientation_swapped: key.orientation_swapped,
                            epsilon: key.epsilon,
                            left_points: left.clone(),
                            right_points: right.clone(),
                        },
                        pairs,
                        max_pairs,
                    )?;
                }
            }
        }
        Ok(())
    }
}

pub(crate) fn correspondence(
    left: &mut HausdorffOracle<'_>,
    right: &mut HausdorffOracle<'_>,
    epsilon: u32,
    config: &LazyConfig,
    max_pairs: usize,
) -> Result<Vec<(u64, u64)>, String> {
    if left.point_count().max(right.point_count()) > max_pairs {
        return Err("witness pair limit is smaller than a hyperspace".into());
    }
    let key = UltrametricMemoKey {
        orientation_swapped: false,
        epsilon,
        left_points: (1..=left.point_count() as u64).collect(),
        right_points: (1..=right.point_count() as u64).collect(),
    };
    let mut recursor = UltrametricRecursor::new(config);
    recursor.capture = true;
    if !recursor.decide(
        &key.left_points,
        &key.right_points,
        left,
        right,
        epsilon,
        false,
    )? {
        return Err("no ultrametric witness at the reported bound".into());
    }
    let mut pairs = Vec::new();
    recursor.collect_witness(&key, &mut pairs, max_pairs)?;
    Ok(pairs)
}
