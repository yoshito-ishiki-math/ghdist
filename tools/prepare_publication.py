#!/usr/bin/env python3
"""Build an allowlisted, deterministic source package without network access."""
import argparse
from datetime import datetime, timezone
import gzip
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import re
import tarfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
REPOSITORY = "yoshito-ishiki-math/ghdist"
MAX_ARCHIVE_BYTES = 16 * 1024 * 1024
PATTERNS = {
    "absolute local path": re.compile(rb"/(?:Users|home|private|tmp)/[A-Za-z0-9_.-]"),
    "private key": re.compile(rb"-----BEGIN (?:[A-Z0-9]+ )*PRIVATE KEY-----"),
    "GitHub credential": re.compile(rb"(?:gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,})"),
    "API credential": re.compile(rb"\bsk-(?:proj-)?[A-Za-z0-9_-]{32,}"),
    "merge conflict": re.compile(rb"(?m)^(?:<{7}|={7}|>{7})(?:\s|$)"),
}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def encoded(value):
    return (json.dumps(value, indent=2, ensure_ascii=False) + "\n").encode()


def safe_relative(name):
    path = PurePosixPath(name)
    if (
        not name or "\\" in name or path.is_absolute()
        or any(part in {".", "..", ".git", "target", "history", ".publication"} for part in path.parts)
        or path.as_posix() != name
    ):
        raise ValueError(f"invalid public path: {name!r}")
    return path


def inspect_content(name, data, depth=0):
    if name.endswith(".tar.gz"):
        if depth > 2:
            raise ValueError(f"nested archive limit exceeded: {name}")
        with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
            total = 0
            for member in archive:
                safe_relative(member.name)
                if not member.isfile():
                    raise ValueError(f"non-file archive member: {name}::{member.name}")
                total += member.size
                if total > MAX_ARCHIVE_BYTES:
                    raise ValueError(f"archive inspection size limit exceeded: {name}")
                inspect_content(f"{name}::{member.name}", archive.extractfile(member).read(), depth + 1)
        return
    data.decode("utf-8")  # Public files are text or explicitly inspected archives.
    for label, pattern in PATTERNS.items():
        if pattern.search(data):
            # Do not echo the matched value: it may itself be sensitive.
            raise ValueError(f"{label} found in {name}")


def read_allowlist():
    names = [line.strip() for line in (ROOT / "PUBLIC-FILES.txt").read_text().splitlines()
             if line.strip() and not line.lstrip().startswith("#")]
    if len(names) != len(set(names)):
        raise ValueError("duplicate paths in PUBLIC-FILES.txt")
    for name in names:
        safe_relative(name)
    for required in ("Cargo.toml", "Cargo.lock", "LICENSE", "README.md", "PUBLIC-FILES.txt",
                     "PROVENANCE.json", "SOURCE-MANIFEST.json"):
        if required not in names:
            raise ValueError(f"missing required public file: {required}")
    return sorted(names)


def public_provenance(source, files, version):
    if source.get("schema") == "ghdist.public-provenance.v1":
        if source.get("version") != version or source.get("repository") != REPOSITORY:
            raise ValueError("public provenance does not match package metadata")
        return source
    baseline = "benchmarks/baseline-0.1.0.tar.gz"
    return {
        "schema": "ghdist.public-provenance.v1",
        "version": version,
        "repository": REPOSITORY,
        "source_project": source["source_project"],
        "method": "Standalone extraction and subsequent refactoring; public files selected by PUBLIC-FILES.txt.",
        "license_origin": source["license_origin"],
        "original_extraction_sha256": source["sha256"],
        "original_extraction_baseline": {"path": baseline, "sha256": sha(files[baseline])},
        "source_manifest": "SOURCE-MANIFEST.json",
        "private_repository_history_included": False,
    }


def payload():
    files = {}
    for name in read_allowlist():
        path = ROOT / name
        for part in (path, *path.parents):
            if part == ROOT:
                break
            if part.is_symlink():
                raise ValueError(f"public path contains a symlink: {name}")
        if not path.is_file():
            raise ValueError(f"missing public file: {name}")
        files[name] = path.read_bytes()
    package = tomllib.loads(files["Cargo.toml"].decode())["package"]
    version = package["version"]
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+(?:-[A-Za-z0-9.-]+)?", version):
        raise ValueError("unsupported package version")
    if package.get("repository") != f"https://github.com/{REPOSITORY}":
        raise ValueError("Cargo repository does not match the intended public destination")
    files["PROVENANCE.json"] = encoded(public_provenance(json.loads(files["PROVENANCE.json"]), files, version))
    files.pop("SOURCE-MANIFEST.json")
    files["SOURCE-MANIFEST.json"] = encoded({
        "schema": "ghdist.public-source.v1", "version": version,
        "scope": "All allowlisted public files except this manifest itself.",
        "sha256": {name: sha(data) for name, data in sorted(files.items())},
    })
    for name, data in files.items():
        inspect_content(name, data)
    return version, files


def build(output, version, files):
    output.mkdir(parents=True, exist_ok=False)
    tree = output / "ghdist"
    for name, data in sorted(files.items()):
        target = tree / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
    archive_path = output / f"ghdist-{version}.tar.gz"
    with archive_path.open("wb") as raw:
        with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=0) as zipped:
            with tarfile.open(fileobj=zipped, mode="w", format=tarfile.USTAR_FORMAT) as archive:
                for name, data in sorted(files.items()):
                    info = tarfile.TarInfo(f"ghdist-{version}/{name}")
                    info.size, info.mode, info.mtime = len(data), 0o644, 0
                    archive.addfile(info, io.BytesIO(data))
    digest = sha(archive_path.read_bytes())
    (output / "SHA256SUMS").write_text(f"{digest}  {archive_path.name}\n")
    report = {
        "prepared_at": datetime.now(timezone.utc).isoformat(),
        "repository": REPOSITORY, "version": version,
        "public_file_count": len(files), "archive": archive_path.name,
        "archive_sha256": digest, "network_actions_performed": False,
        "excluded": ["local history", "research records", "Git history", "build and generated result files"],
        "transformed": ["PROVENANCE.json", "SOURCE-MANIFEST.json"],
        "checks": ["exact file allowlist", "no symlinks", "text and recursive archive pattern scan"],
    }
    (output / "PREPARATION.json").write_bytes(encoded(report))
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=Path, required=True,
                        help="new directory for the public tree, archive, checksums, and preparation report")
    args = parser.parse_args()
    try:
        if args.output_dir.exists() or args.output_dir.is_symlink():
            raise ValueError("output directory already exists; choose a new path")
        version, files = payload()
        report = build(args.output_dir, version, files)
    except (OSError, ValueError, KeyError, tarfile.TarError) as error:
        parser.exit(1, f"Preparation failed: {error}\n")
    print(f"Prepared {report['public_file_count']} files for {REPOSITORY} v{version}")
    print(f"Source tree: {args.output_dir / 'ghdist'}")
    print(f"Archive SHA-256: {report['archive_sha256']}")


if __name__ == "__main__":
    main()
