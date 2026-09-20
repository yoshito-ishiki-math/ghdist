use crate::input::{Rational, Space, normalize, read_json};
use crate::job::{self, Options};
use crate::witness::{self, Engine, Layer};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

const HELP: &str = "ghdist — exact finite Gromov–Hausdorff calculations\n\nUsage:\n  ghdist distance X.json Y.json [options]\n  ghdist hyperspace X.json Y.json [options]\n  ghdist validate X.json [Y.json ...] [--json]\n  ghdist verify correspondence.json [--json] [--time-limit 30s]\n  ghdist batch cases.json [--output results.jsonl] [options]\n\nCommon options:\n  --json                      Machine-readable exact rational results\n  --verbose                   Include search statistics in human output\n  --output PATH               Save JSON (JSONL for batch); never overwrite\n  --progress                  Elapsed time and certified bounds on stderr\n  --time-limit DURATION        Overall solve/witness time, e.g. 500ms, 30s, 2m\n  --engine MODE               auto, explicit, lazy-generic, ultrametric, z3\n  --compare-base              Also compute the base distance (hyperspace only)\n  --witness                   Embed a correspondence certificate in JSON\n  --certificate PATH          Save a correspondence certificate (single pair)\n  --certificate-dir DIR       Save pair-000001.json, ... in batch mode\n  --max-witness-pairs N        Correspondence size limit (default 10000)\n  --max-search-nodes N         Nodes per decision (default 2000000)\n  --max-relation-vertices N    Explicit search limit (default 20000)\n  --max-hyperspace-points N    Explicit hyperspace limit (default 127)\n  --max-lazy-depth N           Generic covering depth (default 4095)\n  --max-candidate-scans N      Generic/Z3 candidate scans (default 100000000)\n  --max-distance-cache N       Cached distances (default 4000000; 0 disables)\n  --mrv-window N              Generic search lookahead (default 8)\n  --max-ultrametric-blocks N   Recursive block width (default 24)\n  --max-ultrametric-assignments N  Block allocations (default 5000000)\n  --max-z3-variables N         SAT variables (default 5000)\n  --max-z3-clauses N           SAT clauses (default 10000000)\n  --z3-timeout-seconds N       Per-call solver timeout (default 30)\n  --hyperspace                Override a batch manifest's mode\n  --help, -h                  Show this help\n  --version                   Show version\n\nInput: {\"name\":\"X\",\"points\":[\"a\",\"b\"],\"distances\":[[0,\"1/3\"],[\"1/3\",0]]}\nA singleton is {\"distances\":[[0]]}. Rational strings and decimal JSON numbers are exact.\nBatch: {\"spaces\":[\"X.json\",\"Y.json\"],\"mode\":\"distance\"}; paths are relative to the manifest.\nExit: 0 success, 1 invalid input/I/O, 2 incomplete search or certificate, 130 interrupted.\nCtrl-C preserves completed bounds and batch records on Unix.\nLegacy --left-edges/--known-pair flags and the old command name remain available.\n";
struct Args {
    command: String,
    paths: Vec<PathBuf>,
    options: Options,
    json: bool,
    verbose: bool,
    output: Option<PathBuf>,
    certificate: Option<PathBuf>,
    certificate_dir: Option<PathBuf>,
    embedded_witness: bool,
    override_layer: bool,
}
fn parse(raw: &[String]) -> Result<Args, String> {
    let command = raw
        .first()
        .ok_or("choose distance, hyperspace, validate, verify, or batch")?
        .clone();
    if !["distance", "hyperspace", "validate", "verify", "batch"].contains(&command.as_str()) {
        return Err(format!("unknown command {command:?}; use ghdist --help"));
    }
    let mut args = Args {
        command,
        paths: Vec::new(),
        options: Options::default(),
        json: false,
        verbose: false,
        output: None,
        certificate: None,
        certificate_dir: None,
        embedded_witness: false,
        override_layer: false,
    };
    if args.command == "hyperspace" {
        args.options.layer = Layer::Hyperspace;
    }
    let mut position = 1;
    let mut positional = false;
    let mut flags = Vec::new();
    while position < raw.len() {
        let argument = &raw[position];
        position += 1;
        if positional || !argument.starts_with('-') {
            args.paths.push(argument.into());
            continue;
        }
        if argument == "--" {
            positional = true;
            continue;
        }
        flags.push(argument.as_str());
        let mut next = || {
            let value = raw
                .get(position)
                .ok_or_else(|| format!("{argument} needs a value"))?;
            position += 1;
            Ok::<&str, String>(value)
        };
        match argument.as_str() {
            "--json" => args.json = true,
            "--verbose" => args.verbose = true,
            "--progress" => args.options.progress = true,
            "--compare-base" => args.options.compare_base = true,
            "--witness" => {
                args.embedded_witness = true;
                args.options.want_witness = true;
            }
            "--output" => args.output = Some(next()?.into()),
            "--certificate" => {
                args.certificate = Some(next()?.into());
                args.options.want_witness = true;
            }
            "--certificate-dir" => {
                args.certificate_dir = Some(next()?.into());
                args.options.want_witness = true;
            }
            "--hyperspace" => {
                args.options.layer = Layer::Hyperspace;
                args.override_layer = true;
            }
            "--engine" => args.options.engine = Engine::parse(next()?)?,
            "--time-limit" => args.options.time_limit = Some(parse_duration(next()?)?),
            "--max-search-nodes" => {
                args.options.config.max_search_nodes = integer(next()?, argument)?
            }
            "--max-candidate-scans" => {
                args.options.config.max_candidate_scans = integer(next()?, argument)?
            }
            "--max-ultrametric-assignments" => {
                args.options.config.max_ultrametric_assignments = integer(next()?, argument)?
            }
            "--max-z3-clauses" => args.options.config.max_z3_clauses = integer(next()?, argument)?,
            "--z3-timeout-seconds" => {
                args.options.config.z3_timeout_seconds = integer(next()?, argument)?
            }
            "--max-hyperspace-points" => {
                args.options.limits.max_explicit_points = integer(next()?, argument)?
            }
            "--max-relation-vertices" => {
                args.options.limits.max_vertices = integer(next()?, argument)?
            }
            "--max-witness-pairs" => args.options.limits.max_pairs = integer(next()?, argument)?,
            "--max-lazy-depth" => args.options.config.max_depth = integer(next()?, argument)?,
            "--max-distance-cache" => {
                args.options.config.max_distance_cache = integer(next()?, argument)?
            }
            "--mrv-window" => args.options.config.mrv_window = integer(next()?, argument)?,
            "--max-ultrametric-blocks" => {
                args.options.config.max_ultrametric_blocks = integer(next()?, argument)?
            }
            "--max-z3-variables" => {
                args.options.config.max_z3_variables = integer(next()?, argument)?
            }
            _ => return Err(format!("unknown option {argument:?}; use ghdist --help")),
        }
    }
    let expected = match args.command.as_str() {
        "distance" | "hyperspace" => 2,
        _ => 1,
    };
    if (args.command == "validate" && args.paths.is_empty())
        || (args.command != "validate" && args.paths.len() != expected)
    {
        return Err(format!("{} needs {} input file(s)", args.command, expected));
    }
    if args.options.config.mrv_window == 0
        || args.options.config.max_ultrametric_blocks == 0
        || args.options.config.z3_timeout_seconds == 0
        || args.options.limits.max_pairs == 0
    {
        return Err("MRV window, ultrametric block limit, Z3 timeout, and witness pair limit must be positive".into());
    }
    if args.command != "batch" && args.certificate_dir.is_some() {
        return Err("--certificate-dir is for batch only".into());
    }
    if args.command == "batch" && args.certificate.is_some() {
        return Err("use --certificate-dir for a batch".into());
    }
    if args.override_layer && args.command != "batch" {
        return Err(
            "--hyperspace is a batch override; use the hyperspace command for a single pair".into(),
        );
    }
    if ["validate", "verify"].contains(&args.command.as_str()) {
        let allowed = if args.command == "validate" {
            vec!["--json", "--verbose", "--output"]
        } else {
            vec![
                "--json",
                "--verbose",
                "--output",
                "--time-limit",
                "--progress",
                "--max-witness-pairs",
            ]
        };
        if let Some(flag) = flags.into_iter().find(|f| !allowed.contains(f)) {
            return Err(format!("{flag} is not used by {}", args.command));
        }
    }
    if args.output.is_some() && args.output == args.certificate {
        return Err("result and certificate must use different paths".into());
    }
    for path in args.output.iter().chain(&args.certificate) {
        ensure_new(path)?;
    }
    Ok(args)
}
fn integer<T: std::str::FromStr>(text: &str, option: &str) -> Result<T, String> {
    text.parse()
        .map_err(|_| format!("{option} needs a nonnegative integer"))
}
fn parse_duration(text: &str) -> Result<Duration, String> {
    let (number, multiplier) = if let Some(v) = text.strip_suffix("ms") {
        (v, 0.001)
    } else if let Some(v) = text.strip_suffix('s') {
        (v, 1.0)
    } else if let Some(v) = text.strip_suffix('m') {
        (v, 60.0)
    } else {
        (text, 1.0)
    };
    let seconds = number.parse::<f64>().map_err(|_| "invalid time limit")? * multiplier;
    Duration::try_from_secs_f64(seconds)
        .map_err(|_| "time limit must be finite and nonnegative".into())
}
fn ensure_new(path: &Path) -> Result<(), String> {
    if path.symlink_metadata().is_ok() {
        return Err(format!(
            "{} already exists; choose a new output path",
            path.display()
        ));
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if !parent.is_dir() {
        return Err(format!(
            "output directory {} does not exist",
            parent.display()
        ));
    }
    Ok(())
}
fn new_file(path: &Path) -> Result<File, String> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))
}
fn save(path: &Path, value: &Value) -> Result<(), String> {
    let mut file = new_file(path)?;
    serde_json::to_writer_pretty(&mut file, value).map_err(|e| e.to_string())?;
    writeln!(file).map_err(|e| e.to_string())?;
    file.flush().map_err(|e| e.to_string())
}
fn emit(value: &Value, json_mode: bool, verbose: bool) -> Result<(), String> {
    let mut stdout = std::io::stdout().lock();
    if json_mode {
        serde_json::to_writer(&mut stdout, value).map_err(|e| e.to_string())?;
        return writeln!(stdout).map_err(|e| e.to_string());
    }
    if let Some(results) = value.as_array() {
        for result in results {
            human(&mut stdout, result, verbose)?;
        }
    } else {
        human(&mut stdout, value, verbose)?;
    }
    Ok(())
}
fn human(out: &mut impl Write, value: &Value, verbose: bool) -> Result<(), String> {
    let text = |key: &str| value[key].as_str().unwrap_or("?");
    let line = match text("status") {
        "exact" => format!(
            "{} ↔ {} [{}]: d_GH = {} (exact)",
            value["left"]["name"].as_str().unwrap_or("X"),
            value["right"]["name"].as_str().unwrap_or("Y"),
            text("layer"),
            text("d_gh")
        ),
        "bounded" => format!(
            "{} ↔ {} [{}]: {} <= d_GH <= {} (incomplete)\n  {}",
            value["left"]["name"].as_str().unwrap_or("X"),
            value["right"]["name"].as_str().unwrap_or("Y"),
            text("layer"),
            text("lower_bound"),
            text("upper_bound"),
            text("reason")
        ),
        "valid" => format!(
            "{}: valid metric, {} point(s), diameter = {}, ultrametric = {}",
            text("name"),
            value["points"],
            text("diameter"),
            value["ultrametric"]
        ),
        "verified-upper-bound" => format!(
            "Correspondence verified: {} pairs, distortion = {}, d_GH <= {}\n  Optimality and the reported lower bound are not verified by this command.",
            value["pairs"],
            text("distortion"),
            text("upper_bound")
        ),
        "incomplete" => format!("Verification incomplete: {}", text("reason")),
        "error" => format!("ERROR: {}", text("message")),
        _ => serde_json::to_string(value).map_err(|e| e.to_string())?,
    };
    writeln!(out, "{line}").map_err(|e| e.to_string())?;
    if value["certificate_status"] == "incomplete" {
        writeln!(
            out,
            "  Correspondence incomplete: {}",
            text("certificate_reason")
        )
        .map_err(|e| e.to_string())?;
    }
    if let Some(path) = value["certificate_file"].as_str() {
        writeln!(out, "  Correspondence: {path}").map_err(|e| e.to_string())?;
    }
    if value["base_comparison"]["status"] == "exact" {
        writeln!(
            out,
            "  Base d_GH = {}; hyperspace {}",
            value["base_comparison"]["d_gh"].as_str().unwrap_or("?"),
            value["base_comparison"]["hyperspace_relation"]
                .as_str()
                .unwrap_or("incomplete")
        )
        .map_err(|e| e.to_string())?;
    }
    if value["base_comparison"]["status"] == "bounded" {
        writeln!(
            out,
            "  Base comparison incomplete: {} <= d_GH <= {} ({})",
            value["base_comparison"]["lower_bound"]
                .as_str()
                .unwrap_or("?"),
            value["base_comparison"]["upper_bound"]
                .as_str()
                .unwrap_or("?"),
            value["base_comparison"]["reason"]
                .as_str()
                .unwrap_or("interrupted")
        )
        .map_err(|e| e.to_string())?;
    }
    if verbose {
        writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(value).map_err(|e| e.to_string())?
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn single(args: &Args) -> Result<u8, String> {
    let left = Space::read(&args.paths[0])?;
    let right = Space::read(&args.paths[1])?;
    let mut result = job::run(&left, &right, &args.options)?;
    if let Some(cert) = result.certificate {
        if let Some(path) = &args.certificate {
            save(path, &cert)?;
            result.value["certificate_file"] = json!(path);
        }
        if args.embedded_witness {
            result.value["certificate"] = cert;
        }
    }
    if let Some(path) = &args.output {
        save(path, &result.value)?;
    }
    emit(&result.value, args.json, args.verbose)?;
    Ok(if crate::runtime::cancelled() {
        130
    } else if result.incomplete {
        2
    } else {
        0
    })
}
fn batch(args: &mut Args) -> Result<u8, String> {
    let manifest = read_json(&args.paths[0])?;
    let object = manifest
        .as_object()
        .ok_or("batch manifest must be an object")?;
    for key in object.keys() {
        if !["spaces", "pairs", "mode"].contains(&key.as_str()) {
            return Err(format!("unknown batch field {key:?}"));
        }
    }
    let mode = match object.get("mode") {
        None => "distance",
        Some(v) => v.as_str().ok_or("batch mode must be a string")?,
    };
    if !["distance", "hyperspace"].contains(&mode) {
        return Err("batch mode must be distance or hyperspace".into());
    }
    if !args.override_layer {
        args.options.layer = if mode == "hyperspace" {
            Layer::Hyperspace
        } else {
            Layer::Base
        };
    }
    if object.contains_key("spaces") == object.contains_key("pairs") {
        return Err("provide exactly one of spaces or pairs in the batch manifest".into());
    }
    let parent = args.paths[0].parent().unwrap_or(Path::new("."));
    let path = |v: &Value| {
        v.as_str()
            .map(|s| parent.join(s))
            .ok_or_else(|| "batch paths must be strings".to_string())
    };
    let pairs: Vec<(PathBuf, PathBuf)> = if let Some(values) = object.get("spaces") {
        let spaces = values
            .as_array()
            .ok_or("spaces must be an array")?
            .iter()
            .map(path)
            .collect::<Result<Vec<_>, _>>()?;
        if spaces.len() < 2 {
            return Err("batch spaces needs at least two files".into());
        }
        let mut pairs = Vec::new();
        for i in 0..spaces.len() {
            for j in i + 1..spaces.len() {
                pairs.push((spaces[i].clone(), spaces[j].clone()));
            }
        }
        pairs
    } else {
        object["pairs"]
            .as_array()
            .ok_or("pairs must be an array")?
            .iter()
            .map(|pair| {
                let pair = pair.as_array().ok_or("each pair must be an array")?;
                if pair.len() != 2 {
                    return Err("each pair must contain two paths".into());
                }
                Ok((path(&pair[0])?, path(&pair[1])?))
            })
            .collect::<Result<Vec<_>, String>>()?
    };
    if pairs.is_empty() {
        return Err("batch has no pairs".into());
    }
    let mut spaces = HashMap::new();
    for (left, right) in &pairs {
        for path in [left, right] {
            spaces
                .entry(path.clone())
                .or_insert_with(|| Space::read(path));
        }
    }
    if let Some(directory) = &args.certificate_dir {
        std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        for index in 1..=pairs.len() {
            ensure_new(&directory.join(format!("pair-{index:06}.json")))?;
        }
    }
    let mut output = args.output.as_deref().map(new_file).transpose()?;
    let mut exit = 0;
    let mut completed = 0;
    for (index, (left_path, right_path)) in pairs.iter().enumerate() {
        if crate::runtime::cancelled() {
            exit = 130;
            break;
        }
        let result = spaces[left_path]
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|left| {
                spaces[right_path]
                    .as_ref()
                    .map_err(Clone::clone)
                    .and_then(|right| job::run(left, right, &args.options))
            });
        let mut value = match result {
            Ok(mut result) => {
                if result.incomplete && exit == 0 {
                    exit = 2;
                }
                if let Some(cert) = result.certificate {
                    if let Some(directory) = &args.certificate_dir {
                        let target = directory.join(format!("pair-{:06}.json", index + 1));
                        save(&target, &cert)?;
                        result.value["certificate_file"] = json!(target);
                    }
                    if args.embedded_witness {
                        result.value["certificate"] = cert;
                    }
                }
                result.value
            }
            Err(error) => {
                exit = 1;
                json!({"schema_version":1,"status":"error","message":error})
            }
        };
        value["pair_index"] = json!(index + 1);
        value["left_file"] = json!(left_path);
        value["right_file"] = json!(right_path);
        if let Some(file) = output.as_mut() {
            serde_json::to_writer(&mut *file, &value).map_err(|e| e.to_string())?;
            writeln!(file).map_err(|e| e.to_string())?;
            file.flush().map_err(|e| e.to_string())?;
        }
        emit(&value, args.json, args.verbose)?;
        completed += 1;
    }
    if crate::runtime::cancelled() {
        exit = 130;
    }
    eprintln!("Batch: {completed}/{} pair(s) recorded.", pairs.len());
    Ok(exit)
}
fn execute(mut args: Args) -> Result<u8, String> {
    crate::runtime::install_interrupt_handler()?;
    match args.command.as_str() {
        "distance" | "hyperspace" => single(&args),
        "batch" => batch(&mut args),
        "validate" => {
            let mut results = Vec::new();
            for path in &args.paths {
                let space = Space::read(path)?;
                let (metric, _, unit) = normalize(&space, &space)?;
                results.push(json!({"schema_version":1,"status":"valid","name":space.name,"points":metric.order,"diameter":unit.multiple(metric.diameter())?.to_string(),"ultrametric":metric.is_ultrametric(),"normalization_unit":unit.to_string()}));
            }
            let value = if results.len() == 1 {
                results.remove(0)
            } else {
                json!(results)
            };
            if let Some(path) = &args.output {
                save(path, &value)?;
            }
            emit(&value, args.json, args.verbose)?;
            Ok(0)
        }
        "verify" => {
            let certificate = read_json(&args.paths[0])?;
            let _guard = crate::runtime::begin(
                args.options.time_limit,
                args.options.progress,
                u64::MAX,
                Rational::new(1, 1)?,
            );
            crate::runtime::phase("verify");
            let value = match witness::verify(&certificate, args.options.limits.max_pairs) {
                Ok(value) => value,
                Err(error) if crate::runtime::check().is_err() => {
                    let value = json!({"schema_version":1,"status":"incomplete","reason":error,"verification_completed":false});
                    if let Some(path) = &args.output {
                        save(path, &value)?;
                    }
                    emit(&value, args.json, args.verbose)?;
                    return Ok(if crate::runtime::cancelled() { 130 } else { 2 });
                }
                Err(error) => return Err(error),
            };
            if let Some(path) = &args.output {
                save(path, &value)?;
            }
            emit(&value, args.json, args.verbose)?;
            Ok(0)
        }
        _ => unreachable!(),
    }
}
pub(crate) fn main() -> ExitCode {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    if raw.is_empty() || raw.iter().any(|v| v == "--help" || v == "-h") {
        print!("{HELP}");
        return ExitCode::SUCCESS;
    }
    if raw == ["--version"] {
        println!("ghdist {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    // Keep old scripts operational through both executable names.
    if raw[0].starts_with('-')
        && raw.iter().any(|v| {
            [
                "--known-pair",
                "--left-edges",
                "--right-edges",
                "--left-family",
                "--right-family",
                "--self-test",
            ]
            .contains(&v.as_str())
        })
    {
        return crate::cli::main();
    }
    let json_mode = raw.iter().any(|v| v == "--json");
    match parse(&raw).and_then(execute) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            let value = json!({"schema_version":1,"status":"error","message":error});
            if json_mode {
                let _ = emit(&value, true, false);
            } else {
                eprintln!("ERROR: {error}");
            }
            ExitCode::from(if crate::runtime::cancelled() { 130 } else { 1 })
        }
    }
}
