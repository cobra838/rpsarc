#!/usr/bin/env python3

from __future__ import annotations

import argparse
import subprocess
from pathlib import Path

from roundtrip_rpsarc import DEFAULT_RPSARC, run_case


SEARCH_ROOTS = (
    # Path(r'J:\Game\shadps4\CUSA01130\arc'),
    # Path(r'D:\Game\ShadPS4\CUSA03745'),
    Path(r'J:\Psark test\BCES01097-[The ICO & Shadow of the Colossus Collection]'),
)


def find_archives() -> list[Path]:
    archives = []
    for root in SEARCH_ROOTS:
        if not root.is_dir():
            print(f'SKIP missing directory: {root}')
            continue
        found = sorted(path for path in root.rglob('*.psarc') if path.is_file())
        print(f'FOUND {len(found):5} archives: {root}')
        archives.extend(found)
    return archives


def inspect_case(rpsarc: Path, archive: Path) -> bool:
    result = subprocess.run([str(rpsarc), 'i', str(archive)], capture_output=True, text=True)
    if result.returncode:
        print(f'FAIL inspect {archive}:\n{result.stderr or result.stdout}')
        return False
    print(f'INSPECT {archive}\n{result.stdout.rstrip()}')
    return True


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument('--rpsarc', type=Path, default=DEFAULT_RPSARC)
    parser.add_argument('--keep', action='store_true', help='keep temporary failed/successful outputs')
    parser.add_argument('--inspect', action='store_true', help='inspect archives without rebuilding them')
    args = parser.parse_args()
    if not args.rpsarc.exists():
        raise SystemExit(f'missing rpsarc: {args.rpsarc}')

    archives = find_archives()
    if not archives:
        raise SystemExit('no archives found')

    if args.inspect:
        failures = sum(not inspect_case(args.rpsarc, archive) for archive in archives)
        print(f'inspected {len(archives)} archives: {len(archives) - failures} passed, {failures} failed')
    else:
        failures = sum(not run_case(args.rpsarc, archive, args.keep) for archive in archives)
        print(f'checked {len(archives)} archives: {len(archives) - failures} passed, {failures} failed')
    raise SystemExit(bool(failures))


if __name__ == '__main__':
    main()
