"""Reject accidental publishing of a different source version."""
from pathlib import Path
import re
import sys
import tomllib

version = sys.argv[1]
if not re.fullmatch(r'\d+\.\d+\.\d+', version):
    raise SystemExit('Android releases require a stable major.minor.patch version.')
source = Path(__file__).resolve().parents[1] / 'server/Cargo.toml'
actual = tomllib.loads(source.read_text())['package']['version']
if version != actual:
    raise SystemExit(f'Source version {actual} does not match requested release {version}.')
major, minor, patch = map(int, version.split('.'))
if minor >= 1000 or patch >= 1000 or not 0 < major * 1000000 + minor * 1000 + patch <= 2100000000:
    raise SystemExit('Version cannot be represented as an Android versionCode.')
