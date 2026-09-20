//! Correspondence extraction and a verifier independent of the search engines.
use crate::config::{LazyConfig, LazyStrategy};
use crate::hyperspace::{HausdorffOracle, hyperspace_metric};
use crate::input::{Rational, Space, normalize};
use crate::metric::Metric;
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Layer {
    Base,
    Hyperspace,
}
impl Layer {
    pub(crate) fn name(self) -> &'static str {
        if self == Self::Base {
            "base"
        } else {
            "hyperspace"
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Engine {
    Auto,
    Explicit,
    Generic,
    Ultrametric,
    Z3,
}
impl Engine {
    pub(crate) fn parse(text: &str) -> Result<Self, String> {
        match text {
            "auto" | "lazy" => Ok(Self::Auto),
            "explicit" => Ok(Self::Explicit),
            "lazy-generic" => Ok(Self::Generic),
            "ultrametric" => Ok(Self::Ultrametric),
            "z3" => Ok(Self::Z3),
            _ => Err(format!("unknown engine {text:?}")),
        }
    }
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Explicit => "explicit",
            Self::Generic => "lazy-generic",
            Self::Ultrametric => "ultrametric",
            Self::Z3 => "z3",
        }
    }
    pub(crate) fn strategy(self) -> LazyStrategy {
        match self {
            Self::Auto | Self::Explicit => LazyStrategy::Auto,
            Self::Generic => LazyStrategy::Generic,
            Self::Ultrametric => LazyStrategy::Ultrametric,
            Self::Z3 => LazyStrategy::Z3,
        }
    }
}
#[derive(Clone)]
pub(crate) struct WitnessLimits {
    pub(crate) max_pairs: usize,
    pub(crate) max_vertices: usize,
    pub(crate) max_explicit_points: usize,
}
type Pairs = Vec<(u64, u64)>;
fn labelled(left: usize, right: usize) -> Pairs {
    (0..left.max(right))
        .map(|i| ((i % left) as u64, (i % right) as u64))
        .collect()
}

pub(crate) fn extract(
    left: &Metric,
    right: &Metric,
    epsilon: u32,
    layer: Layer,
    engine: Engine,
    config: &LazyConfig,
    limits: &WitnessLimits,
) -> Result<Pairs, String> {
    crate::runtime::check()?;
    let (n, m) = point_counts(left, right, layer)?;
    if n.max(m) > limits.max_pairs {
        return Err(format!(
            "a correspondence needs at least {} pairs, above --max-witness-pairs {}",
            n.max(m),
            limits.max_pairs
        ));
    }
    let base_pairs = labelled(left.order, right.order);
    let base_dis = pair_distortion(left, right, &base_pairs, Layer::Base)?;
    let pairs = if base_dis <= epsilon {
        if layer == Layer::Base {
            base_pairs
        } else {
            lift(&base_pairs, left.order, right.order, limits.max_pairs)?
        }
    } else if layer == Layer::Base {
        crate::explicit::correspondence(left, right, epsilon, limits.max_vertices)?
            .into_iter()
            .map(|(a, b)| (a as u64, b as u64))
            .collect()
    } else if engine == Engine::Explicit {
        let a = hyperspace_metric(left, limits.max_explicit_points)?;
        let b = hyperspace_metric(right, limits.max_explicit_points)?;
        crate::explicit::correspondence(&a, &b, epsilon, limits.max_vertices)?
            .into_iter()
            .map(|(a, b)| (a as u64 + 1, b as u64 + 1))
            .collect()
    } else {
        let mut a = HausdorffOracle::new(left, config.max_distance_cache)?;
        let mut b = HausdorffOracle::new(right, config.max_distance_cache)?;
        if engine == Engine::Ultrametric
            || (engine == Engine::Auto && left.is_ultrametric() && right.is_ultrametric())
        {
            match crate::hyperspace::ultrametric::correspondence(
                &mut a,
                &mut b,
                epsilon,
                config,
                limits.max_pairs,
            ) {
                Ok(pairs) => {
                    return checked_pairs(left, right, pairs, epsilon, layer, limits.max_pairs);
                }
                Err(error) if engine == Engine::Ultrametric => return Err(error),
                Err(_) => {
                    crate::runtime::check()?;
                }
            }
        }
        let ae = a.subset_eccentricities();
        let be = b.subset_eccentricities();
        if engine == Engine::Z3
            || (engine == Engine::Auto
                && (crate::config::AUTO_Z3_MIN_RELATION_VERTICES
                    ..=crate::config::AUTO_Z3_RELATION_VERTICES)
                    .contains(&(n * m)))
        {
            match crate::hyperspace::z3::feasible(&mut a, &mut b, &ae, &be, epsilon, config) {
                Ok((true, stats)) => {
                    return checked_pairs(
                        left,
                        right,
                        stats
                            .witness
                            .into_iter()
                            .map(|p| (p.left, p.right))
                            .collect(),
                        epsilon,
                        layer,
                        limits.max_pairs,
                    );
                }
                Ok((false, _)) => {
                    return Err(
                        "solver rejected the reported bound during witness extraction".into(),
                    );
                }
                Err(error) if engine == Engine::Z3 => return Err(error),
                Err(_) => {
                    crate::runtime::check()?;
                }
            }
        }
        let ao = crate::hyperspace::generic::hyperspace_point_order(&ae);
        let bo = crate::hyperspace::generic::hyperspace_point_order(&be);
        let solver = crate::hyperspace::generic::LazyCorrespondenceSolver::new(
            &mut a, &mut b, epsilon, config, &ae, &be, &ao, &bo,
        )?;
        let (feasible, _, pairs) = solver.solve_with_witness()?;
        if !feasible {
            return Err("no witness at the reported bound".into());
        }
        pairs.into_iter().map(|p| (p.left, p.right)).collect()
    };
    checked_pairs(left, right, pairs, epsilon, layer, limits.max_pairs)
}
fn checked_pairs(
    left: &Metric,
    right: &Metric,
    pairs: Pairs,
    epsilon: u32,
    layer: Layer,
    max_pairs: usize,
) -> Result<Pairs, String> {
    let pairs = minimal_cover(pairs);
    if pairs.len() > max_pairs {
        return Err("witness pair limit reached".into());
    }
    verify_coverage(left, right, &pairs, layer)?;
    if pair_distortion(left, right, &pairs, layer)? > epsilon {
        return Err("extracted correspondence exceeds the reported bound".into());
    }
    Ok(pairs)
}
fn minimal_cover(pairs: Pairs) -> Pairs {
    let mut pairs: Pairs = pairs
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut a = HashMap::new();
    let mut b = HashMap::new();
    for (left, right) in &pairs {
        *a.entry(*left).or_insert(0_usize) += 1;
        *b.entry(*right).or_insert(0_usize) += 1;
    }
    pairs.retain(|(left, right)| {
        if a[left] > 1 && b[right] > 1 {
            *a.get_mut(left).unwrap() -= 1;
            *b.get_mut(right).unwrap() -= 1;
            false
        } else {
            true
        }
    });
    pairs
}
fn lift(base: &[(u64, u64)], left: usize, right: usize, max_pairs: usize) -> Result<Pairs, String> {
    let mut images = vec![0_u64; left];
    let mut preimages = vec![0_u64; right];
    for &(a, b) in base {
        images[a as usize] |= 1 << b;
        preimages[b as usize] |= 1 << a;
    }
    let image = |mask: u64, neighbors: &[u64]| -> u64 {
        neighbors
            .iter()
            .enumerate()
            .filter(|(i, _)| mask & (1 << i) != 0)
            .fold(0, |acc, (_, value)| acc | value)
    };
    let mut pairs = BTreeSet::new();
    for mask in 1..(1_u64 << left) {
        if mask.is_multiple_of(1024) {
            crate::runtime::check()?;
        }
        pairs.insert((mask, image(mask, &images)));
        if pairs.len() > max_pairs {
            return Err("lifted witness exceeds the pair limit".into());
        }
    }
    for mask in 1..(1_u64 << right) {
        if mask.is_multiple_of(1024) {
            crate::runtime::check()?;
        }
        pairs.insert((image(mask, &preimages), mask));
        if pairs.len() > max_pairs {
            return Err("lifted witness exceeds the pair limit".into());
        }
    }
    Ok(pairs.into_iter().collect())
}
fn point_counts(left: &Metric, right: &Metric, layer: Layer) -> Result<(usize, usize), String> {
    if layer == Layer::Base {
        return Ok((left.order, right.order));
    }
    if left.order > 20 || right.order > 20 {
        return Err("hyperspace witnesses support at most 20 base points".into());
    }
    Ok(((1 << left.order) - 1, (1 << right.order) - 1))
}
fn verify_coverage(
    left: &Metric,
    right: &Metric,
    pairs: &[(u64, u64)],
    layer: Layer,
) -> Result<(), String> {
    let (n, m) = point_counts(left, right, layer)?;
    if pairs.len() < n.max(m) {
        return Err("correspondence does not cover all points".into());
    }
    let mut a = vec![false; n];
    let mut b = vec![false; m];
    let offset = u64::from(layer == Layer::Hyperspace);
    for &(i, j) in pairs {
        let i = i
            .checked_sub(offset)
            .and_then(|x| usize::try_from(x).ok())
            .ok_or("invalid left point")?;
        let j = j
            .checked_sub(offset)
            .and_then(|x| usize::try_from(x).ok())
            .ok_or("invalid right point")?;
        *a.get_mut(i).ok_or("left point is out of range")? = true;
        *b.get_mut(j).ok_or("right point is out of range")? = true;
    }
    if a.contains(&false) || b.contains(&false) {
        return Err("correspondence is not surjective on both sides".into());
    }
    Ok(())
}
// Deliberately use the defining max-min formula, not the solver's Hausdorff oracle.
fn distance(metric: &Metric, a: u64, b: u64, layer: Layer) -> u32 {
    if layer == Layer::Base {
        return metric.get(a as usize, b as usize);
    }
    let directed = |source: u64, target: u64| -> u32 {
        (0..metric.order)
            .filter(|i| source & (1 << i) != 0)
            .map(|i| {
                (0..metric.order)
                    .filter(|j| target & (1 << j) != 0)
                    .map(|j| metric.get(i, j))
                    .min()
                    .unwrap()
            })
            .max()
            .unwrap()
    };
    directed(a, b).max(directed(b, a))
}
fn pair_distortion(
    left: &Metric,
    right: &Metric,
    pairs: &[(u64, u64)],
    layer: Layer,
) -> Result<u32, String> {
    let mut result = 0;
    for (i, &(a, b)) in pairs.iter().enumerate() {
        crate::runtime::check()?;
        for &(c, d) in &pairs[i + 1..] {
            result = result.max(distance(left, a, c, layer).abs_diff(distance(right, b, d, layer)));
        }
    }
    Ok(result)
}
fn names(space: &Space, point: u64, layer: Layer) -> Vec<String> {
    if layer == Layer::Base {
        vec![space.labels[point as usize].clone()]
    } else {
        space
            .labels
            .iter()
            .enumerate()
            .filter(|(i, _)| point & (1 << i) != 0)
            .map(|(_, label)| label.clone())
            .collect()
    }
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn certificate(
    left: &Space,
    right: &Space,
    a: &Metric,
    b: &Metric,
    unit: Rational,
    pairs: &[(u64, u64)],
    layer: Layer,
    report: Value,
) -> Result<Value, String> {
    let distortion = pair_distortion(a, b, pairs, layer)?;
    let relation: Vec<_> = pairs
        .iter()
        .map(|&(i, j)| json!({"left":names(left,i,layer),"right":names(right,j,layer)}))
        .collect();
    Ok(
        json!({"schema":"ghdist.correspondence.v1","layer":layer.name(),"left":left.to_json(),"right":right.to_json(),"correspondence":relation,"distortion":unit.multiple(distortion)?.to_string(),"upper_bound":unit.half_multiple(distortion)?.to_string(),"search_report":report}),
    )
}
pub(crate) fn verify(value: &Value, max_pairs: usize) -> Result<Value, String> {
    if value["schema"] != "ghdist.correspondence.v1" {
        return Err("unknown correspondence schema".into());
    }
    let layer = match value["layer"].as_str() {
        Some("base") => Layer::Base,
        Some("hyperspace") => Layer::Hyperspace,
        _ => return Err("invalid correspondence layer".into()),
    };
    let left = Space::from_json(&value["left"], "X")?;
    let right = Space::from_json(&value["right"], "Y")?;
    let (a, b, unit) = normalize(&left, &right)?;
    let rows = value["correspondence"]
        .as_array()
        .ok_or("correspondence must be an array")?;
    if rows.len() > max_pairs {
        return Err("certificate exceeds --max-witness-pairs".into());
    }
    let decode = |v: &Value, space: &Space| -> Result<u64, String> {
        let labels = v
            .as_array()
            .ok_or("each endpoint must be an array of point names")?;
        if labels.is_empty() || (layer == Layer::Base && labels.len() != 1) {
            return Err("invalid correspondence endpoint size".into());
        }
        let mut indices = BTreeSet::new();
        for name in labels {
            let name = name.as_str().ok_or("endpoint names must be strings")?;
            let index = space
                .labels
                .iter()
                .position(|x| x == name)
                .ok_or_else(|| format!("unknown point {name:?}"))?;
            if !indices.insert(index) {
                return Err("a subset endpoint repeats a point".into());
            }
        }
        if layer == Layer::Base {
            Ok(*indices.first().unwrap() as u64)
        } else {
            if space.labels.len() > 20 {
                return Err("hyperspace certificate exceeds 20 base points".into());
            }
            Ok(indices.into_iter().fold(0, |mask, i| mask | (1 << i)))
        }
    };
    let pairs: Pairs = rows
        .iter()
        .map(|v| Ok((decode(&v["left"], &left)?, decode(&v["right"], &right)?)))
        .collect::<Result<_, String>>()?;
    verify_coverage(&a, &b, &pairs, layer)?;
    let distortion = pair_distortion(&a, &b, &pairs, layer)?;
    let actual = unit.multiple(distortion)?;
    let upper = unit.half_multiple(distortion)?;
    if Rational::parse(
        value["distortion"]
            .as_str()
            .ok_or("missing exact distortion")?,
    )? != actual
        || Rational::parse(
            value["upper_bound"]
                .as_str()
                .ok_or("missing exact upper bound")?,
        )? != upper
    {
        return Err("declared bounds do not match the correspondence distortion".into());
    }
    Ok(
        json!({"schema_version":1,"status":"verified-upper-bound","layer":layer.name(),"pairs":pairs.len(),"distortion":actual.to_string(),"upper_bound":upper.to_string(),"optimality_verified":false,"note":"Coverage and distortion verified from the distance matrices. The lower bound and optimality reported by the search are not verified by this command."}),
    )
}
