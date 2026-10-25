import shutil
import stat
import sys
import zipfile
from pathlib import Path

source, output = map(Path, sys.argv[1:3])
main_name = sys.argv[3]
output.mkdir(parents=True, exist_ok=True)
nested = output / 'package' / 'nested' / 'char'
nested.mkdir(parents=True, exist_ok=True)
files = sorted(source.glob('*.iff'))
assert files and (source / main_name).is_file()
for file in files:
    shutil.copy2(file, nested / file.name)
(nested / 'readme.txt').write_text('Unrelated files must not be staged.', encoding='ascii')
with zipfile.ZipFile(output / 'player.zip', 'w', zipfile.ZIP_DEFLATED) as archive:
    for file in nested.iterdir():
        archive.write(file, file.relative_to(output / 'package').as_posix())
main = (source / main_name).read_bytes()
cases = {
    'traversal': [('nested/../' + main_name, main)],
    'duplicate': [('a/' + main_name, main), ('b/' + main_name.upper(), main)],
    'multiple': [(main_name, main), ('png99999.iff', main)],
    'invalid-main': [(main_name, b'not an iff')],
}
for name, members in cases.items():
    with zipfile.ZipFile(output / (name + '.zip'), 'w') as archive:
        for member, data in members:
            archive.writestr(member, data)
with zipfile.ZipFile(output / 'link.zip', 'w') as archive:
    entry = zipfile.ZipInfo(main_name)
    entry.create_system = 3
    entry.external_attr = (stat.S_IFLNK | 0o777) << 16
    archive.writestr(entry, main)
