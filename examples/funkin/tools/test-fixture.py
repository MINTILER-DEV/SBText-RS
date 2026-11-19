"""Keep all engine code/charts/art, omit music decoding for deterministic tests.
Full music playback is checked separately in the browser smoke pass.
"""
import json,zipfile
from pathlib import Path
root=Path(__file__).resolve().parents[1]
with zipfile.ZipFile(root/'build/Funkin.sb3') as src:
 project=json.loads(src.read('project.json'))
 for target in project['targets']:
  target['sounds']=[s for s in target['sounds'] if s['name'] in ['menu','confirmMenu','cancelMenu','scrollMenu']]
 keep={a['md5ext'] for t in project['targets'] for a in t['costumes']+t['sounds']}
 with zipfile.ZipFile(root/'build/Funkin-runtime-test.sb3','w',compression=zipfile.ZIP_DEFLATED) as dst:
  dst.writestr('project.json',json.dumps(project))
  for path in sorted(keep):dst.writestr(path,src.read(path))
