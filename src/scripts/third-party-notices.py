#!/usr/bin/env python3
"""Write assets/package-docs/THIRD-PARTY-NOTICES.txt: every crate compiled into
a shipped program (game, server, launcher; native and browser builds), its
license, and the license texts its package carries, plus the Rust standard
library, the fonts and the MinGW runtime that Windows builds link statically.

Usage, from src/: python3 scripts/third-party-notices.py [--check]
--check exits 1 when the committed file is out of date (the release checks it).
Needs the crates downloaded (any cargo build does that) and, for the MinGW
text, Homebrew's mingw-w64.
"""
import glob, hashlib, json, re, subprocess, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / 'assets/package-docs/THIRD-PARTY-NOTICES.txt'
SHIPPED = {'peakrunner', 'peakrunner-server', 'peakrunner-launcher'}
TARGETS = ['aarch64-apple-darwin', 'x86_64-pc-windows-gnu', 'x86_64-unknown-linux-gnu', 'wasm32-unknown-unknown']
TEXT = re.compile(r'^(licen[cs]e|copying|notice|copyright|unlicense|ofl|ufl)', re.I)
RUST_TEXTS = ['MIT.txt', 'Apache-2.0.txt', 'LLVM-exception.txt', 'Unicode-3.0.txt', 'GCC-exception-3.1.txt']


def crates():
    """name version -> package, for every non-workspace crate reachable by
    normal (not build or dev) dependencies from a shipped program."""
    found = {}
    for target in TARGETS:
        meta = json.loads(subprocess.run(
            ['cargo', 'metadata', '--format-version', '1', '--locked', '--filter-platform', target],
            cwd=ROOT, check=True, capture_output=True, text=True).stdout)
        packages = {p['id']: p for p in meta['packages']}
        nodes = {n['id']: n for n in meta['resolve']['nodes']}
        members = set(meta['workspace_members'])
        todo = [i for i in members if packages[i]['name'] in SHIPPED]
        seen = set()
        while todo:
            i = todo.pop()
            if i in seen:
                continue
            seen.add(i)
            todo += [d['pkg'] for d in nodes[i]['deps'] if any(k['kind'] is None for k in d['dep_kinds'])]
        for i in seen - members:
            p = packages[i]
            found[f"{p['name']} {p['version']}"] = p
    return dict(sorted(found.items()))


def texts(package):
    """License files at the package root and one level down (fonts/)."""
    root = Path(package['manifest_path']).parent
    files = [f for f in root.iterdir() if f.is_file() and TEXT.match(f.name)]
    for sub in sorted(d for d in root.iterdir() if d.is_dir()):
        files += [f for f in sub.iterdir() if f.is_file() and (TEXT.match(f.name) or
                  (f.suffix == '.txt' and any(sub.glob(f.stem + '.[ot]tf'))) or 'license' in f.name.lower())]
    return [(str(f.relative_to(root)), f.read_text(errors='replace').strip()) for f in sorted(set(files))]


def toolchain_texts():
    sysroot = subprocess.run(['rustc', '--print', 'sysroot'], check=True, capture_output=True, text=True).stdout.strip()
    rust = [(name, Path(sysroot, 'share/doc/rust/licenses', name).read_text().strip()) for name in RUST_TEXTS]
    mingw = sorted(glob.glob('/opt/homebrew/Cellar/mingw-w64/*/COPYING'))
    if not mingw:
        sys.exit('third-party-notices: no mingw-w64 COPYING found (brew install mingw-w64)')
    return rust, Path(mingw[-1]).read_text().strip()


def render():
    packages = crates()
    rust, mingw = toolchain_texts()
    lines = ['PeakRunner third-party notices', '',
             'PeakRunner (see LICENSE) includes the third-party components below. Each is',
             'used under its own license; the license texts follow the list. They are',
             'provided "as is", without warranty, as their licenses state.', '',
             'Components', '----------', '',
             'Rust standard library: MIT OR Apache-2.0 (https://github.com/rust-lang/rust)',
             'MinGW-w64 runtime, linked into the Windows builds: ZPL-2.1 and others (text below);',
             '  libgcc under GPL-3.0 with the GCC Runtime Library Exception (text below)']
    for key, p in packages.items():
        source = p.get('repository') or f"https://crates.io/crates/{p['name']}"
        lines.append(f"{key}: {p.get('license') or 'see its license file'} ({source})")
    # Identical texts are printed once, with every component that carries them.
    blocks = {}
    def add(owner, name, text):
        digest = hashlib.sha256(text.encode()).hexdigest()
        blocks.setdefault(digest, [name, text, []])[2].append(owner)
    for name, text in rust:
        add('Rust standard library', name, text)
    add('MinGW-w64 runtime', 'COPYING', mingw)
    missing = []
    for key, p in packages.items():
        found = texts(p)
        if not found:
            authors = ', '.join(a.split(' <')[0] for a in p.get('authors') or []) or 'its authors'
            missing.append(f"{key}: {p.get('license')}, copyright {authors}")
        for name, text in found:
            add(key, name, text)
    lines += ['', 'License texts', '=============']
    for name, text, owners in blocks.values():
        lines += ['', '-' * 78, f"{name}, for: {', '.join(owners)}", '-' * 78, '', text]
    if missing:
        lines += ['', '-' * 78, 'Components whose packages carry no license file. Each is used under the',
                  'standard text of its license: MIT.txt and Apache-2.0.txt above, and',
                  'https://spdx.org/licenses/ for the others.', '-' * 78, ''] + missing
    return '\n'.join(lines) + '\n'


if __name__ == '__main__':
    text = render()
    if '--check' in sys.argv:
        if not OUT.exists() or OUT.read_text() != text:
            sys.exit('third-party-notices: assets/package-docs/THIRD-PARTY-NOTICES.txt is out of date; '
                     'run python3 scripts/third-party-notices.py')
        print('third-party notices: up to date')
    else:
        OUT.write_text(text)
        print(f'wrote {OUT.relative_to(ROOT)} ({len(text) // 1024} KiB)')
