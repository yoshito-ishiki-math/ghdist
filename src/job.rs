use crate::bounds::{labelled_correspondence_upper_bound, structural_lower_bound};
use crate::config::{DEFAULT_MAX_HYPERSPACE_POINTS, DEFAULT_MAX_RELATION_VERTICES, LazyConfig};
use crate::input::{Space, normalize};
use crate::metric::Metric;
use crate::witness::{self, Engine, Layer, WitnessLimits};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub(crate) struct Options {
    pub(crate) layer: Layer,
    pub(crate) engine: Engine,
    pub(crate) compare_base: bool,
    pub(crate) config: LazyConfig,
    pub(crate) limits: WitnessLimits,
    pub(crate) time_limit: Option<Duration>,
    pub(crate) progress: bool,
    pub(crate) want_witness: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            layer: Layer::Base,
            engine: Engine::Auto,
            compare_base: false,
            config: LazyConfig::default(),
            limits: WitnessLimits {
                max_pairs: 10_000,
                max_vertices: DEFAULT_MAX_RELATION_VERTICES,
                max_explicit_points: DEFAULT_MAX_HYPERSPACE_POINTS,
            },
            time_limit: None,
            progress: false,
            want_witness: false,
        }
    }
}
pub(crate) struct Job {
    pub(crate) value: Value,
    pub(crate) certificate: Option<Value>,
    pub(crate) incomplete: bool,
}
fn explicit(a: &Metric, b: &Metric, limit: usize) -> Result<(u32, Value), String> {
    let (d, s) = crate::explicit::minimum_distortion(a, b, limit)?;
    Ok((
        d,
        json!({"backend":"explicit","candidate_thresholds":s.candidate_thresholds,"feasibility_checks":s.feasibility_checks,"search_nodes":s.search_nodes,"compatibility_masks_built":s.compatibility_masks_built,"memo_hits":s.memo_hits,"failed_states":s.failed_states}),
    ))
}
fn calculate(
    a: &Metric,
    b: &Metric,
    options: &Options,
    base_bound: Option<u32>,
) -> Result<(u32, Value), String> {
    if options.layer == Layer::Base {
        return explicit(a, b, options.limits.max_vertices);
    }
    if options.engine == Engine::Explicit {
        let left = crate::hyperspace::hyperspace_metric(a, options.limits.max_explicit_points)?;
        let right = crate::hyperspace::hyperspace_metric(b, options.limits.max_explicit_points)?;
        return explicit(&left, &right, options.limits.max_vertices);
    }
    let (d, s) = crate::hyperspace::minimum_hyperspace_distortion_lazy(
        a,
        b,
        &options.config,
        options.engine.strategy(),
        base_bound,
    )?;
    Ok((
        d,
        json!({"backend":options.engine.name(),"candidate_thresholds":s.candidate_thresholds,"feasibility_checks":s.feasibility_checks,
        "generic_checks":s.generic_checks,"ultrametric_checks":s.ultrametric_checks,"ultrametric_fallbacks":s.ultrametric_fallbacks,
        "z3_checks":s.z3_checks,"z3_fallbacks":s.z3_fallbacks,"z3_variables":s.z3_variables,"z3_clauses":s.z3_clauses,
        "search_nodes":s.lazy_search_nodes,"candidate_scans":s.candidate_scans,"compatibility_checks":s.compatibility_checks,
        "memo_hits":s.lazy_memo_hits,"maximum_depth":s.maximum_depth,"ultrametric_calls":s.ultrametric_recursive_calls,
        "ultrametric_assignments":s.ultrametric_assignment_candidates,"ultrametric_max_blocks":s.ultrametric_maximum_block_count,
        "distance_queries":s.left_dilation.distance_queries+s.right_dilation.distance_queries,
        "distance_cache_hits":s.left_dilation.distance_cache_hits+s.right_dilation.distance_cache_hits,
        "structural_rejections":s.structural.lower_bound_rejections+s.structural.cardinality_rejections+s.structural.component_count_rejections+s.structural.component_dominance_rejections}),
    ))
}
pub(crate) fn run(left: &Space, right: &Space, options: &Options) -> Result<Job, String> {
    let (a, b, unit) = normalize(left, right)?;
    if options.layer == Layer::Base && !matches!(options.engine, Engine::Auto | Engine::Explicit) {
        return Err("specialized engines require the hyperspace command".into());
    }
    if options.layer == Layer::Base && options.compare_base {
        return Err("--compare-base requires hyperspace".into());
    }
    if options.engine == Engine::Ultrametric && (!a.is_ultrametric() || !b.is_ultrametric()) {
        return Err("the ultrametric engine requires two ultrametric spaces".into());
    }
    let started = Instant::now();
    let _guard = crate::runtime::begin(
        options.time_limit,
        options.progress,
        options.config.max_search_nodes,
        unit,
    );
    let initial_lower = structural_lower_bound(&a, &b);
    let initial_upper = labelled_correspondence_upper_bound(&a, &b);
    crate::runtime::phase(options.layer.name());
    crate::runtime::bounds(initial_lower, initial_upper, "structural lower bound");
    let mut base_bound = None;
    let mut base_comparison = Value::Null;
    if options.compare_base {
        crate::runtime::phase("base");
        crate::runtime::bounds(initial_lower, initial_upper, "structural lower bound");
        match explicit(&a, &b, options.limits.max_vertices) {
            Ok((d, stats)) => {
                base_bound = Some(d);
                base_comparison = json!({"status":"exact","d_gh":unit.half_multiple(d)?.to_string(),"stats":stats});
            }
            Err(error) => {
                let interval = crate::runtime::interval("base").unwrap();
                base_comparison = json!({"status":"bounded","lower_bound":unit.half_multiple(interval.lower)?.to_string(),"upper_bound":unit.half_multiple(interval.upper)?.to_string(),"reason":error});
            }
        }
    }
    crate::runtime::phase(options.layer.name());
    // A computed base optimum supplies a valid hyperspace upper bound.
    if let Some(bound) = base_bound {
        crate::runtime::bounds(initial_lower, bound, "structural lower bound");
    }
    let outcome = crate::runtime::check().and_then(|()| calculate(&a, &b, options, base_bound));
    let mut stats = Value::Null;
    let (optimum, reason) = match outcome {
        Ok((d, s)) => {
            stats = s;
            crate::runtime::bounds(d, d, "exact threshold search");
            (Some(d), None)
        }
        Err(e) => (None, Some(e)),
    };
    let interval = crate::runtime::interval(options.layer.name()).unwrap();
    let mut value = json!({"schema_version":1,"solver_version":env!("CARGO_PKG_VERSION"),
        "status":if optimum.is_some(){"exact"}else{"bounded"},"layer":options.layer.name(),"engine":options.engine.name(),
        "left":{"name":left.name,"points":left.labels.len()},"right":{"name":right.name,"points":right.labels.len()},
        "inputs":{"left":left.to_json(),"right":right.to_json()},
        "d_gh":optimum.map(|d| unit.half_multiple(d).map(|r|r.to_string())).transpose()?,
        "distortion":optimum.map(|d| unit.multiple(d).map(|r|r.to_string())).transpose()?,
        "lower_bound":unit.half_multiple(interval.lower)?.to_string(),"upper_bound":unit.half_multiple(interval.upper)?.to_string(),
        "lower_bound_evidence":interval.lower_evidence,"normalization_unit":unit.to_string(),"reason":reason,
        "base_comparison":base_comparison,"stats":stats,"certificate_status":"not-requested"});
    if let (Some(base), Some(d)) = (base_bound, optimum) {
        value["base_comparison"]["hyperspace_relation"] = json!(if d == base {
            "equal"
        } else {
            "strictly-smaller"
        });
    }
    let mut certificate = None;
    let mut incomplete = optimum.is_none() || (options.compare_base && base_bound.is_none());
    if options.want_witness {
        crate::runtime::phase("witness");
        crate::runtime::bounds(interval.lower, interval.upper, &interval.lower_evidence);
        let extracted = witness::extract(&a,&b,interval.upper,options.layer,options.engine,&options.config,&options.limits)
            .and_then(|pairs| witness::certificate(left,right,&a,&b,unit,&pairs,options.layer,json!({
                "solver_version":env!("CARGO_PKG_VERSION"),"status":value["status"],"reported_lower_bound":value["lower_bound"],
                "lower_bound_evidence":interval.lower_evidence,"optimality_checked_by_correspondence_verifier":false,
            })));
        match extracted {
            Ok(cert) => {
                value["certificate_status"] = json!("available");
                value["witness_pairs"] = json!(cert["correspondence"].as_array().unwrap().len());
                certificate = Some(cert);
            }
            Err(error) => {
                value["certificate_status"] = json!("incomplete");
                value["certificate_reason"] = json!(error);
                incomplete = true;
            }
        }
    }
    value["elapsed_ms"] = json!(started.elapsed().as_secs_f64() * 1000.0);
    Ok(Job {
        value,
        certificate,
        incomplete,
    })
}
