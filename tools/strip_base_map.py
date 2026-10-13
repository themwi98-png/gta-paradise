"""Removes base-map archives (Los Santos / Blaine County sectors) from the
level's data file lists, so the game never mounts them.

  python strip_base_map.py <list.txt> images.meta gta5.meta <out-dir>

images.meta (update.rpf common/data/levels/gta5/) declares every map archive
before any DLC loads, which is why DLC filesToDisable/filesToInvalidate could
not remove them. Items whose <filename> is in the list are dropped; gta5.meta
loses the permanent .ityp files that live inside dropped archives. Everything
else is copied verbatim. Inputs should be the pristine game files, so running
this again is idempotent.
"""
import os
import re
import sys

ITEM = re.compile(r'[ \t]*<Item>.*?</Item>[ \t]*\r?\n?', re.S)
FILENAME = re.compile(r'<filename>\s*([^<]+?)\s*</filename>')


def load_list(path):
    with open(path, encoding='utf-8') as f:
        return {l.strip().lower() for l in f if l.strip() and not l.lstrip().startswith('#')}


def dropped_dirs(paths):
    # Directories whose archives are dropped: platform:/levels/gta5/_citye/downtown_01/
    return {p.rsplit('/', 1)[0] + '/' for p in paths}


def strip(text, drop):
    removed = []

    def repl(m):
        fm = FILENAME.search(m.group(0))
        if fm and drop(fm.group(1).lower()):
            removed.append(fm.group(1))
            return ''
        return m.group(0)

    return ITEM.sub(repl, text), removed


INNER_ITEM = re.compile(r'[ \t]*<Item>((?:(?!<Item>).)*?)</Item>[ \t]*\r?\n?', re.S)
LS_PATH = re.compile(r'levels/gta5/(_citye|_cityw|_hills)/|levels/gta5/[^/]*_additions[^/]*\.rpf', re.I)


def strip_pack(content, out):
    """Patch DLC packs (patchday*ng, patch2026_01 ...) re-add updated copies
    of Los Santos sectors and map additions. Drops every innermost <Item>
    naming one: the data file entries and their filesToEnable /
    filesToInvalidate references."""
    text = open(content, encoding='utf-8-sig', newline='').read()
    removed = []

    def repl(m):
        body = m.group(1)
        fm = FILENAME.search(body)
        path = (fm.group(1) if fm else body).strip().replace('\\', '/')
        if '<' not in path and LS_PATH.search(path):
            removed.append(path)
            return ''
        return m.group(0)

    text = INNER_ITEM.sub(repl, text)
    open(out, 'w', encoding='utf-8', newline='').write(text)
    print(f'{os.path.basename(content)}: removed {len(removed)} Los Santos item(s)')


def main():
    if sys.argv[1] == 'pack':
        return strip_pack(sys.argv[2], sys.argv[3])
    lst, images, gta5, out = sys.argv[1:5]
    names = load_list(lst)
    dirs = dropped_dirs(names)
    os.makedirs(out, exist_ok=True)

    text = open(images, encoding='utf-8-sig', newline='').read()
    text, removed = strip(text, lambda f: f in names)
    open(os.path.join(out, 'images.meta'), 'w', encoding='utf-8', newline='').write(text)
    print(f'images.meta: removed {len(removed)} archive(s)')

    # Permanent ityps sitting directly in (or under) a dropped sector folder,
    # e.g. _citye/citye_xr.ityp or _citye/downtown_01/..._permanent.ityp.
    area = ('platform:/levels/gta5/_citye/', 'platform:/levels/gta5/_cityw/', 'platform:/levels/gta5/_hills/')
    text = open(gta5, encoding='utf-8-sig', newline='').read()
    text, removed = strip(text, lambda f: f.endswith('.ityp') and f.startswith(area)
                          and any(f.startswith(d) or d.startswith(f.rsplit('/', 1)[0] + '/') for d in dirs))
    # Scenario points (parked cars, bench sitters, smokers ...) are all placed
    # for Los Santos; the manifest lists every region file.
    text, scen = strip(text, lambda f: f.endswith('/sp_manifest.#mt'))
    removed += scen
    open(os.path.join(out, 'gta5.meta'), 'w', encoding='utf-8', newline='').write(text)
    print(f'gta5.meta: removed {len(removed)} item(s): {", ".join(removed)}')


if __name__ == '__main__':
    main()
