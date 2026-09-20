use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Metric {
    pub(crate) order: usize,
    pub(crate) entries: Vec<u32>,
}

impl Metric {
    pub(crate) fn from_edges(edges: &[u32]) -> Result<Self, String> {
        let order = infer_order(edges.len())?;
        let mut entries = vec![0; order * order];
        let mut cursor = 0;
        for i in 0..order {
            for j in (i + 1)..order {
                let value = edges[cursor];
                if value == 0 {
                    return Err(format!("edge {i}-{j} has zero length"));
                }
                entries[i * order + j] = value;
                entries[j * order + i] = value;
                cursor += 1;
            }
        }
        let metric = Self { order, entries };
        metric.validate()?;
        Ok(metric)
    }

    pub(crate) fn get(&self, i: usize, j: usize) -> u32 {
        self.entries[i * self.order + j]
    }

    pub(crate) fn from_rule(
        order: usize,
        rule: impl Fn(usize, usize) -> u32,
    ) -> Result<Self, String> {
        let mut entries = vec![0; order * order];
        for i in 0..order {
            for j in (i + 1)..order {
                let value = rule(i, j);
                entries[i * order + j] = value;
                entries[j * order + i] = value;
            }
        }
        let metric = Self { order, entries };
        metric.validate()?;
        Ok(metric)
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.order == 0 || self.order.checked_mul(self.order) != Some(self.entries.len()) {
            return Err("metric matrix must be nonempty and square".into());
        }
        for i in 0..self.order {
            if self.get(i, i) != 0 {
                return Err(format!("diagonal entry {i},{i} is not zero"));
            }
            for j in 0..self.order {
                if self.get(i, j) != self.get(j, i) {
                    return Err(format!("entries {i},{j} and {j},{i} differ"));
                }
                if i != j && self.get(i, j) == 0 {
                    return Err(format!("entry {i},{j} is not positive"));
                }
                for k in 0..self.order {
                    if self.get(i, j) > self.get(i, k).saturating_add(self.get(k, j)) {
                        return Err(format!("triangle inequality fails at {i},{k},{j}"));
                    }
                }
            }
        }
        Ok(())
    }

    pub(crate) fn distance_levels(&self) -> BTreeSet<u32> {
        self.entries.iter().copied().collect()
    }

    /// Representatives of classes whose transpositions are isometries.
    /// Two points are twins if distances to every other point agree.
    pub(crate) fn twin_classes(&self) -> Vec<usize> {
        let mut representatives = Vec::new();
        let mut classes = Vec::with_capacity(self.order);
        for point in 0..self.order {
            let representative = representatives.iter().copied().find(|other| {
                (0..self.order).all(|third| {
                    third == point
                        || third == *other
                        || self.get(point, third) == self.get(*other, third)
                })
            });
            let representative = representative.unwrap_or_else(|| {
                representatives.push(point);
                point
            });
            classes.push(representative);
        }
        classes
    }

    pub(crate) fn diameter(&self) -> u32 {
        self.entries.iter().copied().max().unwrap_or(0)
    }

    pub(crate) fn radius(&self) -> u32 {
        (0..self.order)
            .map(|i| (0..self.order).map(|j| self.get(i, j)).max().unwrap_or(0))
            .min()
            .unwrap_or(0)
    }

    pub(crate) fn minimum_positive_distance(&self) -> u32 {
        self.entries
            .iter()
            .copied()
            .filter(|value| *value > 0)
            .min()
            .unwrap_or(u32::MAX)
    }

    pub(crate) fn is_ultrametric(&self) -> bool {
        (0..self.order).all(|i| {
            (0..self.order).all(|j| {
                (0..self.order).all(|k| self.get(i, j) <= self.get(i, k).max(self.get(k, j)))
            })
        })
    }

    pub(crate) fn is_equilateral(&self) -> Option<u32> {
        let mut positive = self.entries.iter().copied().filter(|value| *value > 0);
        let first = positive.next()?;
        positive.all(|value| value == first).then_some(first)
    }

    pub(crate) fn component_sizes_leq(&self, threshold: u64) -> Vec<usize> {
        let mut parent: Vec<usize> = (0..self.order).collect();

        fn find(parent: &mut [usize], mut point: usize) -> usize {
            while parent[point] != point {
                parent[point] = parent[parent[point]];
                point = parent[point];
            }
            point
        }

        for i in 0..self.order {
            for j in (i + 1)..self.order {
                if u64::from(self.get(i, j)) <= threshold {
                    let left = find(&mut parent, i);
                    let right = find(&mut parent, j);
                    if left != right {
                        parent[right] = left;
                    }
                }
            }
        }
        let mut counts = BTreeMap::new();
        for point in 0..self.order {
            let root = find(&mut parent, point);
            *counts.entry(root).or_insert(0_usize) += 1;
        }
        let mut result: Vec<usize> = counts.into_values().collect();
        result.sort_unstable();
        result
    }

    pub(crate) fn open_component_labels(&self, threshold: u32) -> Vec<usize> {
        let mut labels = vec![usize::MAX; self.order];
        let mut next_label = 0;
        for seed in 0..self.order {
            if labels[seed] != usize::MAX {
                continue;
            }
            let mut stack = vec![seed];
            labels[seed] = next_label;
            while let Some(point) = stack.pop() {
                for (neighbor, label) in labels.iter_mut().enumerate() {
                    if *label == usize::MAX && self.get(point, neighbor) < threshold {
                        *label = next_label;
                        stack.push(neighbor);
                    }
                }
            }
            next_label += 1;
        }
        labels
    }

    pub(crate) fn mst_merge_spectrum(&self) -> Vec<u32> {
        if self.order <= 1 {
            return Vec::new();
        }
        let mut in_tree = vec![false; self.order];
        let mut best = vec![u32::MAX; self.order];
        best[0] = 0;
        let mut result = Vec::with_capacity(self.order - 1);
        for step in 0..self.order {
            let point = (0..self.order)
                .filter(|i| !in_tree[*i])
                .min_by_key(|i| best[*i])
                .expect("a finite complete metric graph stays connected");
            in_tree[point] = true;
            if step > 0 {
                result.push(best[point]);
            }
            for neighbor in 0..self.order {
                if !in_tree[neighbor] {
                    best[neighbor] = best[neighbor].min(self.get(point, neighbor));
                }
            }
        }
        result.sort_unstable();
        result
    }
}

pub(crate) fn edge_rank(order: usize, i: usize, j: usize) -> usize {
    i * (2 * order - i - 1) / 2 + (j - i)
}

pub(crate) fn family_metric(name: &str, order: usize) -> Result<Metric, String> {
    if !(4..=20).contains(&order) {
        return Err(format!(
            "catalog families are recorded only for orders 4 through 20, received {order}"
        ));
    }
    let normalized = name.replace('-', "_");
    match normalized.as_str() {
        "equilateral" => Metric::from_rule(order, |_i, _j| 1),
        "binary_star" => Metric::from_rule(order, |i, j| (1_u32 << i) + (1_u32 << j)),
        "dyadic_line" => Metric::from_rule(order, |i, j| (1_u32 << i).abs_diff(1_u32 << j)),
        "comb_ultrametric" => Metric::from_rule(order, |_i, j| 1_u32 << j),
        "balanced_ultrametric" => Metric::from_rule(order, |i, j| {
            1_u32 << (usize::BITS - (i ^ j).leading_zeros())
        }),
        "rank_generic" => Metric::from_rule(order, |i, j| 1000 + edge_rank(order, i, j) as u32),
        _ => Err(format!(
            "unknown family {name:?}; choose equilateral, binary_star, \
             dyadic_line, comb_ultrametric, balanced_ultrametric, or rank_generic"
        )),
    }
}

pub(crate) fn infer_order(edge_count: usize) -> Result<usize, String> {
    for order in 2_usize..=1_000_000 {
        match order.checked_mul(order - 1).map(|value| value / 2) {
            Some(count) if count == edge_count => return Ok(order),
            Some(count) if count > edge_count => break,
            Some(_) => {}
            None => break,
        }
    }
    Err(format!(
        "{edge_count} entries are not the edge count n(n-1)/2 of a finite space"
    ))
}
