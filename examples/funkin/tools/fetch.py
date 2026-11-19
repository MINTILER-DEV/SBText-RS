#!/usr/bin/env python3
"""Fetch pinned upstream inputs; cache paths preserve their original repositories."""
import concurrent.futures, hashlib, json, time, urllib.request, urllib.parse
from pathlib import Path
ROOT = Path(__file__).resolve().parents[1]
CACHE = ROOT / 'cache'
REPOS = ['Funkin', 'funkin.assets', 'funkin.art']

def request(url):
    for attempt in range(5):
        try:
            with urllib.request.urlopen(urllib.request.Request(url, headers={'User-Agent':'SBText-Funkin-Importer'}), timeout=90) as r:
                return r.read()
        except Exception:
            if attempt == 4: raise
            time.sleep(2 ** attempt)

def lock():
    path = ROOT / 'upstream.lock.json'
    if path.exists(): return json.loads(path.read_text())
    data = {}
    for repo in REPOS:
        sha = json.loads(request(f'https://api.github.com/repos/FunkinCrew/{repo}/commits/main'))['sha']
        tree = json.loads(request(f'https://api.github.com/repos/FunkinCrew/{repo}/git/trees/{sha}?recursive=1'))
        if tree.get('truncated'): raise RuntimeError('Truncated upstream tree')
        data[repo] = {'commit':sha,'url':f'https://github.com/FunkinCrew/{repo}', 'files':{x['path']:x['sha'] for x in tree['tree'] if x['type']=='blob'}}
    path.write_text(json.dumps(data,indent=2)+'\n')
    return data
LOCK = None

def get(repo, path):
    global LOCK
    if LOCK is None: LOCK = lock()
    out = CACHE / repo / path
    expected = LOCK[repo]['files'][path]
    if out.exists():
        b = out.read_bytes()
        if hashlib.sha1(b'blob '+str(len(b)).encode()+b'\0'+b).hexdigest()==expected: return out
    b = request(f"https://raw.githubusercontent.com/FunkinCrew/{repo}/{LOCK[repo]['commit']}/{urllib.parse.quote(path)}")
    if hashlib.sha1(b'blob '+str(len(b)).encode()+b'\0'+b).hexdigest()!=expected: raise RuntimeError(f'Integrity failure: {repo}/{path}')
    out.parent.mkdir(parents=True,exist_ok=True)
    out.write_bytes(b)
    return out

def batch(items):
    with concurrent.futures.ThreadPoolExecutor(max_workers=10) as pool:
        return list(pool.map(lambda x:get(*x),items))

if __name__=='__main__':
    LOCK = lock()
    items = [('Funkin','source/funkin/Conductor.hx'),('Funkin','source/funkin/util/Constants.hx'),('Funkin','source/funkin/play/scoring/Scoring.hx')]
    for repo in REPOS: items += [(repo,p) for p in LOCK[repo]['files'] if p in ['LICENSE.md','README.md','ChartFormat-README.txt']]
    items += [('funkin.assets',p) for p in LOCK['funkin.assets']['files'] if p.startswith('preload/data/') and (('/songs/' in p and ('-chart' in p or '-metadata' in p)) or '/characters/' in p or '/levels/' in p or '/stages/' in p)]
    # Keep references to primary art assets as well as runtime exports.
    items += [('funkin.art',p) for p in LOCK['funkin.art']['files'] if p.startswith('discord/icon-')]
    batch(items)
    print(f'Fetched {len(items)} pinned reference files', flush=True)
