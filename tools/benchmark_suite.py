#!/usr/bin/env python3
"""Time the same original test cases in two compiled Rust test executables."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import platform
import re
import statistics
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline', type=Path, required=True)
    parser.add_argument('--candidate', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--repetitions', type=int, default=3)
    parser.add_argument('--timeout', type=float, default=60)
    args = parser.parse_args()
    if args.repetitions < 1 or args.timeout <= 0:
        parser.error('repetitions and timeout must be positive')
    # This comparison includes the original optional Z3 test in both binaries.
    subprocess.run(['z3', '-version'], check=True, capture_output=True)
    cases = (Path(__file__).resolve().parents[1] / 'benchmarks/common-tests.txt').read_text().splitlines()
    binaries = {'baseline': args.baseline.resolve(), 'candidate': args.candidate.resolve()}
    command_args = ['--exact', *cases, '--include-ignored', '--test-threads', '1']
    report = {
        'date': datetime.now(timezone.utc).isoformat(), 'platform': platform.platform(),
        'method': 'Same 12 original tests, including Z3; single test thread; alternating order; process wall time; no warmup.',
        'args': command_args, 'repetitions': args.repetitions,
        'binary_sha256': {k: hashlib.sha256(v.read_bytes()).hexdigest() for k,v in binaries.items()},
        'samples': {key: [] for key in binaries},
    }
    for repetition in range(args.repetitions):
        order = list(binaries) if repetition % 2 == 0 else list(reversed(binaries))
        for side in order:
            started = time.perf_counter()
            process = subprocess.run([str(binaries[side]), *command_args], text=True, capture_output=True, timeout=args.timeout, check=True)
            elapsed = time.perf_counter() - started
            count = re.search(r'test result: ok\. (\d+) passed; 0 failed; 0 ignored;', process.stdout)
            if not count or int(count[1]) != len(cases):
                raise RuntimeError(f'wrong test count: {process.stdout}')
            report['samples'][side].append({'wall_seconds': elapsed, 'stdout': process.stdout})
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(json.dumps(report, indent=2) + '\n')
            print(f'{side} run {repetition + 1}: {len(cases)} passed in {elapsed:.3f}s', flush=True)
    report['median_seconds'] = {k: statistics.median(x['wall_seconds'] for x in v) for k,v in report['samples'].items()}
    args.output.write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    main()
