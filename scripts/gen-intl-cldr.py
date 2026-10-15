#!/usr/bin/env python3
"""Generate rust/js-intl/src/cldr.rs from the CLDR JSON distribution.

Usage: gen-intl-cldr.py CLDR_JSON_DIR [OUT]

CLDR_JSON_DIR holds these files from unicode-org/cldr-json (cldr-json/):
cldr-core/supplemental/{aliases,likelySubtags,calendarPreferenceData,
timeData,weekData}.json, cldr-core/{coverageLevels,scriptMetadata}.json and
cldr-bcp47/bcp47/{calendar,collation,measure,number,timezone,transform}.json,
all flattened into the one directory (bcp47 files prefixed "bcp47-").

The tables are what rust/js-intl needs for ECMA-402 without ICU: the alias
rules of UTS #35 Annex C, the likely subtags of the languages CLDR covers, of
every two-letter language and of every "und" entry, the -u- and -t- value aliases, the region preferences
behind Intl.Locale's getCalendars, getHourCycles, getWeekInfo and
getTimeZones, the writing direction of each script, and the languages
reported as available. Each table is a string of space-separated entries
that rust/js-intl/src/cldr.rs's readers index on first use.
"""
import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SRC = sys.argv[1] if len(sys.argv) > 1 else '.'
OUT = sys.argv[2] if len(sys.argv) > 2 else \
    os.path.join(ROOT, 'rust', 'js-intl', 'src', 'cldr.rs')

LANGUAGE = re.compile(r'^([a-z]{2,3}|[a-z]{5,8})$')
SCRIPT = re.compile(r'^[A-Z][a-z]{3}$')
REGION = re.compile(r'^([A-Z]{2}|[0-9]{3})$')
VARIANT = re.compile(r'^([a-z0-9]{5,8}|[0-9][a-z0-9]{3})$')
UVALUE = re.compile(r'^[a-z0-9]{3,8}(-[a-z0-9]{3,8})*$')

EXTRA_LANGUAGES = ['aae', 'jbo', 'hak', 'hsn', 'gan', 'wuu', 'nan', 'hyw',
                   'ssy', 'gss', 'sfb', 'vgt', 'tlh', 'lzh', 'cmn']

CALENDAR_NAMES = {'gregorian': 'gregory',
                  'ethiopic-amete-alem': 'ethioaa'}


def load(name):
    with open(os.path.join(SRC, name), encoding='utf-8') as handle:
        return json.load(handle)


def is_language_id(tag):
    parts = tag.split('-')
    if not LANGUAGE.match(parts[0]):
        return False
    rest = parts[1:]
    if rest and SCRIPT.match(rest[0]):
        rest = rest[1:]
    if rest and REGION.match(rest[0]):
        rest = rest[1:]
    return all(VARIANT.match(part) for part in rest)


def language_aliases(alias):
    entries = []
    for key, value in alias['languageAlias'].items():
        replacement = value['_replacement']
        if not is_language_id(key) or '-x-' in replacement:
            continue
        entries.append(f'{key}={replacement}')
    for key, value in alias['scriptAlias'].items():
        entries.append(f'und-{key}=und-{value["_replacement"]}')
    for key, value in alias['variantAlias'].items():
        entries.append(f'und-{key}=und-{value["_replacement"]}')
    return sorted(entries)


def territory_aliases(alias):
    entries = []
    for key, value in alias['territoryAlias'].items():
        if REGION.match(key):
            regions = value['_replacement'].split()
            entries.append(f'{key}={",".join(regions)}')
    return sorted(entries)


def subdivision_aliases(alias):
    entries = []
    for key, value in alias['subdivisionAlias'].items():
        first = value['_replacement'].split()[0]
        if REGION.match(first):
            first = first.lower() + 'zzzz'
        entries.append(f'{key}={first}')
    return sorted(entries)


def keyword_aliases(files, extension):
    entries = set()
    for name in files:
        keywords = load(name)['keyword'].get(extension, {})
        for key, types in keywords.items():
            names = {t for t in types if not t.startswith('_')}
            for type_name, info in types.items():
                if type_name.startswith('_') or not isinstance(info, dict):
                    continue
                preferred = info.get('_preferred')
                if preferred:
                    entries.add(f'{key}:{type_name}={preferred}')
                    continue
                for alias in info.get('_alias', '').split():
                    alias = alias.lower()
                    if alias in names or not UVALUE.match(alias):
                        continue
                    entries.add(f'{key}:{alias}={type_name}')
    return sorted(entries)


def missing_subtags(key, value):
    parts = key.split('-')
    language, script, region = value.split('-')
    filled = []
    if parts[0] == 'und':
        filled.append(language)
    if not any(SCRIPT.match(part) for part in parts[1:]):
        filled.append(script)
    if not any(REGION.match(part) for part in parts[1:]):
        filled.append(region)
    return '-'.join(filled)


def likely_subtags(likely, coverage):
    languages = set(key.split('-')[0] for key in coverage)
    languages.update(EXTRA_LANGUAGES)
    languages.update(key for key in likely if len(key) == 2)
    for key, value in likely.items():
        if key.startswith('und') or '-' in key:
            languages.add(value.split('-')[0])
            languages.add(key.split('-')[0])
    entries = []
    for key, value in likely.items():
        language = key.split('-')[0]
        if key.startswith('und') or '-' in key or language in languages:
            entries.append(f'{key}={missing_subtags(key, value)}')
    return sorted(entries)


def grouped(mapping):
    groups = {}
    for key, value in mapping.items():
        groups.setdefault(value, []).append(key)
    return sorted(f'{value}:{",".join(sorted(keys))}'
                  for value, keys in groups.items())


def calendar_preferences(data):
    mapping = {}
    for region, calendars in data.items():
        names = [CALENDAR_NAMES.get(name, name) for name in calendars]
        mapping[region] = ','.join(names)
    return grouped(mapping)


def hour_cycles(data):
    mapping = {}
    for key, value in data.items():
        preferred = {'h': 'h12', 'H': 'h23', 'K': 'h11', 'k': 'h24'}
        cycle = preferred[value['_preferred']]
        if key == '001' or (cycle == 'h23' and '-' not in key):
            continue
        mapping[key] = cycle
    return grouped(mapping)


def first_days(week):
    mapping = {key: value for key, value in week['firstDay'].items()
               if REGION.match(key) and value != 'mon'}
    return grouped(mapping)


def weekends(week):
    regions = set(week['weekendStart']) | set(week['weekendEnd'])
    mapping = {}
    for region in regions:
        if not REGION.match(region) or region == '001':
            continue
        start = week['weekendStart'].get(region, 'sat')
        end = week['weekendEnd'].get(region, start)
        if (start, end) != ('sat', 'sun'):
            mapping[region] = f'{start}-{end}'
    return grouped(mapping)


def time_zones(data):
    zones = data['keyword']['u']['tz']
    regions = {}
    for name, info in zones.items():
        if name.startswith('_') or info.get('_deprecated'):
            continue
        aliases = info.get('_alias', '').split()
        if not aliases or name in ('unk', 'gmt'):
            continue
        primary = info.get('_iana', aliases[0])
        if primary == 'Etc/UTC':
            primary = 'UTC'
        elif '/' not in primary:
            continue
        region = name[:2].upper()
        if primary.startswith('Etc/') or primary == 'UTC':
            region = ''
        regions.setdefault(region, []).append(primary)
    return sorted(f'{region}:{",".join(sorted(names))}'
                  for region, names in regions.items())


def scripts(metadata, direction):
    return sorted(code for code, info in metadata.items()
                  if info['rtl'] == direction)


def available(coverage, likely):
    languages = set()
    scripts_seen = set()
    for key in coverage:
        parts = key.split('-')
        if parts[0] == 'und' or not LANGUAGE.match(parts[0]):
            continue
        languages.add(parts[0])
        if len(parts) > 1 and SCRIPT.match(parts[1]):
            scripts_seen.add(f'{parts[0]}-{parts[1]}')
    for language in languages:
        maximal = likely.get(language)
        if maximal:
            scripts_seen.add('-'.join(maximal.split('-')[:2]))
    return sorted(languages), sorted(scripts_seen)


def wrapped(text, width=92):
    lines = []
    while len(text) > width:
        cut = max(text.rfind(' ', 0, width), text.rfind(',', 0, width))
        lines.append(text[:cut + 1])
        text = text[cut + 1:]
    lines.append(text)
    return lines


def rust_string(name, entries):
    body = '\\\n'.join(wrapped(' '.join(entries)))
    return f'pub(crate) const {name}: &str = "\\\n{body}";\n'


def main():
    alias = load('aliases.json')['supplemental']['metadata']['alias']
    likely = load('likelySubtags.json')['supplemental']['likelySubtags']
    coverage = load('coverageLevels.json')['effectiveCoverageLevels']
    calendars = load('calendarPreferenceData.json')
    calendars = calendars['supplemental']['calendarPreferenceData']
    time_data = load('timeData.json')['supplemental']['timeData']
    week = load('weekData.json')['supplemental']['weekData']
    metadata = load('scriptMetadata.json')['scriptMetadata']
    unicode_files = ['bcp47-calendar.json', 'bcp47-collation.json',
                     'bcp47-measure.json', 'bcp47-number.json',
                     'bcp47-timezone.json']
    languages = language_aliases(alias)
    available_languages, available_scripts = available(coverage, likely)
    tables = [
        ('LANGUAGE_ALIASES', languages),
        ('TERRITORY_ALIASES', territory_aliases(alias)),
        ('SUBDIVISION_ALIASES', subdivision_aliases(alias)),
        ('UNICODE_VALUE_ALIASES', keyword_aliases(unicode_files, 'u')),
        ('TRANSFORM_VALUE_ALIASES',
         keyword_aliases(['bcp47-transform.json'], 't')),
        ('LIKELY_SUBTAGS', likely_subtags(likely, coverage)),
        ('CALENDAR_PREFERENCES', calendar_preferences(calendars)),
        ('HOUR_CYCLES', hour_cycles(time_data)),
        ('FIRST_DAYS', first_days(week)),
        ('WEEKENDS', weekends(week)),
        ('TIME_ZONES', time_zones(load('bcp47-timezone.json'))),
        ('RTL_SCRIPTS', scripts(metadata, 'YES')),
        ('LTR_SCRIPTS', scripts(metadata, 'NO')),
        ('AVAILABLE_LANGUAGES', available_languages),
        ('AVAILABLE_SCRIPTS', available_scripts),
    ]
    out = ['//! Southstar — CLDR locale data for Intl, generated by '
           'scripts/gen-intl-cldr.py.\n',
           '//! Copyright 2026 Andreas Røsdal\n',
           '//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR '
           'GPL-3.0-or-later\n']
    for name, entries in tables:
        out.append('\n')
        out.append(rust_string(name, entries))
    with open(OUT, 'w', encoding='utf-8') as handle:
        handle.write(''.join(out))


main()
