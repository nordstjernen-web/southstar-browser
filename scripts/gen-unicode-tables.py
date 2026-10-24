#!/usr/bin/env python3
"""Generate rust/unicode/src/tables.rs from the Unicode Character Database.

Usage: gen-unicode-tables.py UCD_DIR [OUT]

UCD_DIR is one Unicode version laid out as on unicode.org's
Public/<version>/ucd/: UnicodeData.txt, LineBreak.txt, Scripts.txt,
EastAsianWidth.txt, SpecialCasing.txt and PropertyValueAliases.txt. The
tables are the properties text layout reads: general category, line break
class, script, East Asian width, canonical combining class, the simple and
special case mappings and the canonical decompositions. Scripts keep the
numbering of GLib's GUnicodeScript, which the layout code stores, and new
scripts are appended after it. The output is run through rustfmt when it is
on the PATH.
"""
import os
import re
import shutil
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
UCD = sys.argv[1] if len(sys.argv) > 1 else '/usr/share/unicode'
OUT = sys.argv[2] if len(sys.argv) > 2 else \
    os.path.join(ROOT, 'rust', 'unicode', 'src', 'tables.rs')

MAX = 0x110000

CATEGORIES = ['Cc', 'Cf', 'Cn', 'Co', 'Cs', 'Ll', 'Lm', 'Lo', 'Lt', 'Lu',
              'Mc', 'Me', 'Mn', 'Nd', 'Nl', 'No', 'Pc', 'Pd', 'Pe', 'Pf',
              'Pi', 'Po', 'Ps', 'Sc', 'Sk', 'Sm', 'So', 'Zl', 'Zp', 'Zs']

BREAKS = ['BK', 'CR', 'LF', 'CM', 'SG', 'ZW', 'IN', 'GL', 'CB', 'SP', 'BA',
          'BB', 'B2', 'HY', 'NS', 'OP', 'CL', 'QU', 'EX', 'ID', 'NU', 'IS',
          'SY', 'AL', 'PR', 'PO', 'SA', 'AI', 'XX', 'NL', 'WJ', 'JL', 'JV',
          'JT', 'H2', 'H3', 'CP', 'CJ', 'HL', 'RI', 'EB', 'EM', 'ZWJ', 'AK',
          'AP', 'AS', 'VF', 'VI', 'HH']

SCRIPTS = [
    'Zyyy', 'Zinh', 'Arab', 'Armn', 'Beng', 'Bopo', 'Cher', 'Copt', 'Cyrl',
    'Dsrt', 'Deva', 'Ethi', 'Geor', 'Goth', 'Grek', 'Gujr', 'Guru', 'Hani',
    'Hang', 'Hebr', 'Hira', 'Knda', 'Kana', 'Khmr', 'Laoo', 'Latn', 'Mlym',
    'Mong', 'Mymr', 'Ogam', 'Ital', 'Orya', 'Runr', 'Sinh', 'Syrc', 'Taml',
    'Telu', 'Thaa', 'Thai', 'Tibt', 'Cans', 'Yiii', 'Tglg', 'Hano', 'Buhd',
    'Tagb', 'Brai', 'Cprt', 'Limb', 'Osma', 'Shaw', 'Linb', 'Tale', 'Ugar',
    'Talu', 'Bugi', 'Glag', 'Tfng', 'Sylo', 'Xpeo', 'Khar', 'Zzzz', 'Bali',
    'Xsux', 'Phnx', 'Phag', 'Nkoo', 'Kali', 'Lepc', 'Rjng', 'Sund', 'Saur',
    'Cham', 'Olck', 'Vaii', 'Cari', 'Lyci', 'Lydi', 'Avst', 'Bamu', 'Egyp',
    'Armi', 'Phli', 'Prti', 'Java', 'Kthi', 'Lisu', 'Mtei', 'Sarb', 'Orkh',
    'Samr', 'Lana', 'Tavt', 'Batk', 'Brah', 'Mand', 'Cakm', 'Merc', 'Mero',
    'Plrd', 'Shrd', 'Sora', 'Takr', 'Bass', 'Aghb', 'Dupl', 'Elba', 'Gran',
    'Khoj', 'Sind', 'Lina', 'Mahj', 'Mani', 'Mend', 'Modi', 'Mroo', 'Nbat',
    'Narb', 'Perm', 'Hmng', 'Palm', 'Pauc', 'Phlp', 'Sidd', 'Tirh', 'Wara',
    'Ahom', 'Hluw', 'Hatr', 'Mult', 'Hung', 'Sgnw', 'Adlm', 'Bhks', 'Marc',
    'Newa', 'Osge', 'Tang', 'Gonm', 'Nshu', 'Soyo', 'Zanb', 'Dogr', 'Gong',
    'Rohg', 'Maka', 'Medf', 'Sogo', 'Sogd', 'Elym', 'Nand', 'Hmnp', 'Wcho',
    'Chrs', 'Diak', 'Kits', 'Yezi', 'Cpmn', 'Ougr', 'Tnsa', 'Toto', 'Vith',
    'Zmth', 'Kawi', 'Nagm', 'Todr', 'Gara', 'Tutg', 'Sunu', 'Gukh', 'Krai',
    'Onao', 'Sidt', 'Tols', 'Tayo', 'Berf',
]

WIDE_UNASSIGNED = [(0x3400, 0x4dbf), (0x4e00, 0x9fff), (0xf900, 0xfaff),
                   (0x20000, 0x2fffd), (0x30000, 0x3fffd)]


def ucd_file(name):
    path = os.path.join(UCD, name)
    if os.path.exists(path):
        return path
    sys.exit('missing %s under %s' % (name, UCD))


def records(path, missing=False):
    for line in open(path, encoding='utf-8'):
        if missing:
            m = re.match(r'#\s*@missing:\s*([0-9A-F.]+)\s*;\s*(\w+)', line)
            if not m:
                continue
            fields = [m.group(1), m.group(2)]
        else:
            line = line.split('#', 1)[0].strip()
            if not line:
                continue
            fields = [x.strip() for x in line.split(';')]
        rng = fields[0]
        if '..' in rng:
            lo, hi = [int(x, 16) for x in rng.split('..')]
        else:
            lo = hi = int(rng, 16)
        yield lo, hi, fields[1:]


def version():
    for line in open(ucd_file('Scripts.txt'), encoding='utf-8'):
        m = re.match(r'#\s*Scripts-([0-9.]+)\.txt', line)
        if m:
            return m.group(1)
    return 'unknown'


def unicode_data():
    rows = {}
    first = None
    for line in open(ucd_file('UnicodeData.txt'), encoding='utf-8'):
        f = line.rstrip('\n').split(';')
        cp = int(f[0], 16)
        if f[1].endswith(', First>'):
            first = cp
            continue
        if f[1].endswith(', Last>'):
            for c in range(first, cp + 1):
                rows[c] = f
            continue
        rows[cp] = f
    return rows


def category_table(rows):
    value = [CATEGORIES.index('Cn')] * MAX
    for cp, f in rows.items():
        value[cp] = CATEGORIES.index(f[2])
    return value


def break_table():
    names = {}
    for line in open(ucd_file('PropertyValueAliases.txt'), encoding='utf-8'):
        f = [x.strip() for x in line.split('#', 1)[0].split(';')]
        if len(f) >= 3 and f[0] == 'lb':
            names[f[2]] = f[1]
            names[f[1]] = f[1]
    value = [BREAKS.index('XX')] * MAX
    path = ucd_file('LineBreak.txt')
    for lo, hi, fields in records(path, missing=True):
        short = names.get(fields[0], fields[0])
        for cp in range(lo, hi + 1):
            value[cp] = BREAKS.index(short)
    for lo, hi, fields in records(path):
        for cp in range(lo, hi + 1):
            value[cp] = BREAKS.index(fields[0])
    return value


def script_table():
    names = {}
    for line in open(ucd_file('PropertyValueAliases.txt'), encoding='utf-8'):
        f = [x.strip() for x in line.split('#', 1)[0].split(';')]
        if len(f) >= 3 and f[0] == 'sc':
            names[f[2]] = f[1]
    scripts = list(SCRIPTS)
    value = [scripts.index('Zzzz')] * MAX
    for lo, hi, fields in records(ucd_file('Scripts.txt')):
        tag = names[fields[0]]
        if tag not in scripts:
            scripts.append(tag)
        for cp in range(lo, hi + 1):
            value[cp] = scripts.index(tag)
    return value, scripts


def wide_table(categories):
    value = [False] * MAX
    for lo, hi, fields in records(ucd_file('EastAsianWidth.txt')):
        if fields[0] in ('W', 'F'):
            for cp in range(lo, hi + 1):
                value[cp] = True
    unassigned = CATEGORIES.index('Cn')
    for lo, hi in WIDE_UNASSIGNED:
        for cp in range(lo, hi + 1):
            if categories[cp] == unassigned:
                value[cp] = True
    return value


def combining_table(rows):
    value = [0] * MAX
    for cp, f in rows.items():
        value[cp] = int(f[3])
    return value


def case_tables(rows):
    upper, lower, title = {}, {}, {}
    for cp, f in rows.items():
        if f[12]:
            upper[cp] = int(f[12], 16)
        if f[13]:
            lower[cp] = int(f[13], 16)
        if f[14]:
            title[cp] = int(f[14], 16)
    special = {}
    for lo, _, fields in records(ucd_file('SpecialCasing.txt')):
        if len(fields) > 4 and fields[3].strip():
            continue
        lower_full = [int(x, 16) for x in fields[0].split()]
        title_full = [int(x, 16) for x in fields[1].split()]
        upper_full = [int(x, 16) for x in fields[2].split()]
        special[lo] = (lower_full, title_full, upper_full)
    return upper, lower, title, special


def decomposition_table(rows):
    out = {}
    for cp, f in rows.items():
        parts = f[5].split()
        if parts and not parts[0].startswith('<'):
            values = [int(x, 16) for x in parts]
            out[cp] = (values[0], values[1] if len(values) > 1 else 0)
    return out


def ranges(values):
    starts, out = [], []
    previous = None
    for cp, v in enumerate(values):
        if v != previous:
            starts.append(cp)
            out.append(v)
            previous = v
    return starts, out


def array(name, kind, items, fmt):
    body = ', '.join(fmt(x) for x in items)
    return 'pub static %s: [%s; %d] = [%s];\n' % (name, kind, len(items),
                                                   body)


def mapping(name, table, out, hexa):
    keys = sorted(table)
    out.append(array(name + '_KEYS', 'u32', keys, hexa))
    out.append(array(name + '_VALUES', 'u32', [table[k] for k in keys], hexa))


def main():
    rows = unicode_data()
    categories = category_table(rows)
    scripts_values, scripts = script_table()
    upper, lower, title, special = case_tables(rows)
    decomposition = decomposition_table(rows)
    hexa = lambda x: '0x%x' % x
    out = []
    out.append('//! Southstar — the Unicode %s character properties text '
               'layout reads, generated by scripts/gen-unicode-tables.py.\n'
               % version())
    out.append('//! Copyright 2026 Andreas Røsdal\n')
    out.append('//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR '
               'GPL-3.0-or-later\n\n')
    for name, values in (('CATEGORY', categories),
                         ('LINE_BREAK', break_table()),
                         ('SCRIPT', scripts_values),
                         ('COMBINING_CLASS', combining_table(rows))):
        starts, vals = ranges(values)
        out.append(array(name + '_STARTS', 'u32', starts, hexa))
        out.append(array(name + '_VALUES', 'u8', vals, str))
    wide_starts, wide_values = ranges(wide_table(categories))
    out.append(array('WIDE_STARTS', 'u32', wide_starts, hexa))
    out.append('pub const WIDE_FIRST: bool = %s;\n'
               % ('true' if wide_values[0] else 'false'))
    out.append(array('SCRIPT_TAGS', '[u8; 4]', scripts,
                     lambda t: '*b"%s"' % t))
    mapping('UPPER', upper, out, hexa)
    mapping('LOWER', lower, out, hexa)
    titlecase = CATEGORIES.index('Lt')
    title_table = [(cp, upper.get(cp, 0), lower.get(cp, 0))
                   for cp in range(MAX) if categories[cp] == titlecase]
    out.append(array('TITLE_TABLE', '[u32; 3]', title_table,
                     lambda e: '[%s]' % ', '.join(hexa(x) for x in e)))
    keys = sorted(special)
    pool = []
    index = []
    for k in keys:
        entry = []
        for part in special[k]:
            entry.append(len(pool) << 8 | len(part))
            pool.extend(part)
        index.append(entry)
    out.append(array('SPECIAL_KEYS', 'u32', keys, hexa))
    out.append(array('SPECIAL_VALUES', '[u32; 3]', index,
                     lambda e: '[%s]' % ', '.join(hexa(x) for x in e)))
    out.append(array('SPECIAL_POOL', 'u32', pool, hexa))
    keys = sorted(decomposition)
    out.append(array('DECOMPOSITION_KEYS', 'u32', keys, hexa))
    out.append(array('DECOMPOSITION_VALUES', '(u32, u32)',
                     [decomposition[k] for k in keys],
                     lambda v: '(%s, %s)' % (hexa(v[0]), hexa(v[1]))))
    with open(OUT, 'w', encoding='utf-8') as f:
        f.write(''.join(out))
    if shutil.which('rustfmt'):
        subprocess.run(['rustfmt', '--edition', '2024', OUT], check=True)


main()
