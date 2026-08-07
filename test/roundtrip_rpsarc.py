#!/usr/bin/env python3
"""Extract, rebuild, compare, and remove temporary results for PSARC fixtures."""

from __future__ import annotations

import argparse
import hashlib
import shutil
import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
DEFAULT_CASES = ROOT / "test" / "psarc"
DEFAULT_RPSARC = ROOT / "target" / "release" / "rpsarc.exe"


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(block)
    return digest.hexdigest()


def first_difference(left: Path, right: Path) -> int | None:
    offset = 0
    with left.open('rb') as a, right.open('rb') as b:
        while True:
            x, y = a.read(1024 * 1024), b.read(1024 * 1024)
            if x == y:
                if not x:
                    return None
                offset += len(x)
                continue
            for index, (xb, yb) in enumerate(zip(x, y)):
                if xb != yb:
                    return offset + index
            return offset + min(len(x), len(y))


def run_case(rpsarc: Path, archive: Path, keep: bool) -> bool:
    temporary = Path(tempfile.mkdtemp(prefix='_roundtrip_', dir=archive.parent))
    extracted, rebuilt = temporary / 'unpacked', temporary / 'rebuilt.psarc'
    try:
        extract = subprocess.run([str(rpsarc), 'x', str(archive), str(extracted)], capture_output=True, text=True)
        if extract.returncode:
            print(f'FAIL extract {archive}:\n{extract.stderr or extract.stdout}')
            return False
        create = subprocess.run([str(rpsarc), 'c', str(extracted / '__manifest.json'), str(rebuilt)], capture_output=True, text=True)
        if create.returncode:
            print(f'FAIL create {archive}:\n{create.stderr or create.stdout}')
            return False
        left, right = sha256(archive), sha256(rebuilt)
        if left == right:
            print(f'PASS {archive.name}  {left}')
            return True
        print(f'FAIL {archive.name}: source={left} rebuilt={right} first_diff={first_difference(archive, rebuilt)}')
        return False
    finally:
        if keep:
            print(f'kept {temporary}')
        else:
            shutil.rmtree(temporary, ignore_errors=True)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument('archives', nargs='*', type=Path, help='archives; default: all fixtures')
    parser.add_argument('--rpsarc', type=Path, default=DEFAULT_RPSARC)
    parser.add_argument('--cases', type=Path, default=DEFAULT_CASES)
    parser.add_argument('--keep', action='store_true', help='keep temporary failed/successful outputs')
    args = parser.parse_args()
    if not args.rpsarc.exists():
        raise SystemExit(f'missing rpsarc: {args.rpsarc}')
    archives = args.archives or sorted(args.cases.rglob('*.psarc'))
    if not archives:
        raise SystemExit('no archives selected')
    failures = sum(not run_case(args.rpsarc, archive, args.keep) for archive in archives)
    raise SystemExit(bool(failures))


if __name__ == '__main__':
    main()
