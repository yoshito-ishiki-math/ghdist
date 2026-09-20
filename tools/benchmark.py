#!/usr/bin/env python3
"""Compare exact outputs and paired CLI timings; Python standard library only."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import platform
import statistics
import subprocess
import time

CASES = [
    ("known-base", ["--known-pair"]),
    ("known-explicit-hyperspace", ["--known-pair", "--hyperspace", "--engine", "explicit"]),
    ("known-generic-hyperspace", ["--known-pair", "--hyperspace", "--engine", "lazy-generic"]),
    ("ultrametric-four", ["--left-edges", "1,1,2,1,2,2", "--right-edges", "2,3,3,3,3,2", "--hyperspace", "--engine", "ultrametric"]),
    ("ultrametric-six", ["--left-family", "comb_ultrametric", "--right-family", "balanced_ultrametric", "--order", "6", "--hyperspace", "--engine", "ultrametric"]),
    ("identity-twenty", ["--left-family", "comb_ultrametric", "--right-family", "comb_ultrametric", "--order", "20", "--hyperspace"]),
    ("optional-z3-five", ["--left-family", "rank_generic", "--right-family", "dyadic_line", "--order", "5", "--hyperspace", "--engine", "z3", "--compare-base"]),
]


def run(binary, args, timeout):
    started = time.perf_counter_ns()
    process = subprocess.run([str(binary), *args], capture_output=True, text=True, timeout=timeout)
    elapsed_ms = (time.perf_counter_ns() - started) / 1_000_000
    if process.returncode:
        raise RuntimeError(f"{binary.name} {args}: exit {process.returncode}: {process.stderr.strip()}")
    lines = [line for line in process.stdout.splitlines() if line.startswith("EXACT_RESULT ")]
    if len(lines) != 1:
        raise RuntimeError("expected exactly one EXACT_RESULT line")
    fields = dict(word.split("=", 1) for word in lines[0].split()[1:])
    return {"wall_ms": elapsed_ms, "fields": fields}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--repetitions", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30)
    parser.add_argument("--skip-z3", action="store_true")
    args = parser.parse_args()
    if args.repetitions < 1 or args.timeout <= 0:
        parser.error("repetitions and timeout must be positive")
    binaries = {"baseline": args.baseline.resolve(), "candidate": args.candidate.resolve()}
    report = {
        "date": datetime.now(timezone.utc).isoformat(),
        "platform": platform.platform(), "machine": platform.machine(),
        "repetitions": args.repetitions, "timeout_seconds": args.timeout,
        "method": "One warmup per binary per case, then alternating execution order; process wall times include launch overhead. Solver times exclude launch and input parsing. No speed claim outside these cases.",
        "binary_sha256": {key: hashlib.sha256(path.read_bytes()).hexdigest() for key, path in binaries.items()},
        "cases": [],
    }
    for name, command_args in CASES:
        if args.skip_z3 and name.startswith("optional-z3"):
            continue
        for binary in binaries.values():
            run(binary, command_args, args.timeout)
        rows = {key: [] for key in binaries}
        expected = None
        for repetition in range(args.repetitions):
            order = list(binaries) if repetition % 2 == 0 else list(reversed(binaries))
            for key in order:
                sample = run(binaries[key], command_args, args.timeout)
                answer = {field: sample["fields"].get(field) for field in ("layer", "base_orders", "search_orders", "distortion", "d_GH", "base_distortion", "comparison")}
                if expected is not None and answer != expected:
                    raise RuntimeError(f"exact result mismatch for {name}: {answer} != {expected}")
                expected = answer
                rows[key].append(sample)
        wall = {key: statistics.median(sample["wall_ms"] for sample in samples) for key, samples in rows.items()}
        solver = {key: statistics.median(float(sample["fields"]["total_ms"]) for sample in samples) for key, samples in rows.items()}
        row = {"name": name, "args": command_args, "result": expected, "median_wall_ms": wall, "median_solver_ms": solver, "wall_speedup": wall["baseline"] / wall["candidate"], "samples": rows}
        report["cases"].append(row)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print(f"{name}: equal; wall {wall['baseline']:.3f} -> {wall['candidate']:.3f} ms; solver {solver['baseline']:.3f} -> {solver['candidate']:.3f} ms", flush=True)


if __name__ == "__main__":
    main()
