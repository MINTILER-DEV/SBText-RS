#!/usr/bin/env python3
"""Generate SBText tables and compile the complete project with sbtext-rs."""
import argparse, collections, json, subprocess, sys, zipfile
from pathlib import Path
from PIL import Image
from fetch import ROOT, CACHE
BUILD=ROOT/'build';BUILD.mkdir(exist_ok=True)
DIFFICULTIES=['easy','normal','hard','erect','nightmare']

def lit(value):return json.dumps(value,ensure_ascii=True,separators=(',',':'))
def declaration(name,values):return f'list {name} = ['+', '.join(lit(v) for v in values)+']\n'

def generate():
 tracks=json.loads((ROOT/'generated/catalog.json').read_text());levels=[]
 for week in ['tutorial']+[f'week{i}' for i in range(1,8)]+['weekend1','sserafim']:
  path=CACHE/'funkin.assets/preload/data/levels'/(week+'.json')
  if path.exists():levels.append(json.loads(path.read_text()))
 order=[s for w in levels for s in w['songs']];tracks.sort(key=lambda t:(order.index(t['song']) if t['song'] in order else 999,t['variation']))
 table=collections.defaultdict(list)
 for t in tracks:
  table['trackShortTitles'].append(t['title'][:28]);table['trackHudTitles'].append(t['title'][:35]);table['trackTitles'].append(t['title']);table['trackKeys'].append(t['key']);table['trackPlayers'].append(t['player']);table['trackOpponents'].append(t['opponent']);table['trackGirlfriends'].append(t['girlfriend']);table['trackStages'].append(t['stage']);table['trackDurations'].append(t['duration']);table['defaultDifficulties'].append(2 if 'normal' in t['difficulties'] else DIFFICULTIES.index(t['difficulties'][0])+1)
  chart=json.loads((CACHE/'funkin.assets'/t['chart']).read_text());meta=json.loads((CACHE/'funkin.assets'/t['meta']).read_text())
  for diff in DIFFICULTIES:
   if diff not in chart['notes']:
    table['chartStarts'].append(0);table['chartEnds'].append(0);table['chartSpeeds'].append(1);table['highScores'].append(0);continue
   notes=sorted(chart['notes'][diff],key=lambda n:(n['t'],n['d']))
   if any(not 0<=n['d']<=7 for n in notes):raise ValueError('Unsupported lane in '+t['key'])
   table['chartStarts'].append(len(table['allTimes'])+1)
   for note in notes:
    table['allTimes'].append(round(note['t'],3));table['allLanes'].append(note['d']);table['allLengths'].append(round(note.get('l',0),3))
   table['chartEnds'].append(len(table['allTimes']));table['chartSpeeds'].append(chart['scrollSpeed'].get(diff,1));table['highScores'].append(0)
  table['trackSegmentStarts'].append(len(table['segmentTimes'])+1);table['segmentTimes']+=t['segments'];table['trackSegmentEnds'].append(len(table['segmentTimes']))
  table['trackBpmStarts'].append(len(table['bpmTimes'])+1);last=0;beat=0;bpm=meta['timeChanges'][0]['bpm']
  for change in sorted(meta['timeChanges'],key=lambda c:c['t']):
   beat+=(change['t']-last)*bpm/60000;bpm=change['bpm'];last=change['t'];table['bpmTimes'].append(change['t']);table['bpmValues'].append(bpm);table['bpmBeats'].append(round(beat,5))
  table['trackBpmEnds'].append(len(table['bpmTimes']))
  table['trackEventStarts'].append(len(table['eventTimes'])+1)
  for event in sorted(chart.get('events',[]),key=lambda e:e['t']):
   if event['e'] not in ['FocusCamera','ScrollSpeed']:continue
   value=event.get('v',{});kind=1 if event['e']=='FocusCamera' else 2
   if kind==1:
    value=value.get('char',0) if isinstance(value,dict) else value
    values=[float(value),0,0]
   else:values=[float(value.get('scroll',1)),float(value.get('duration',0)),int(value.get('absolute',False))]
   table['eventTimes'].append(round(event['t'],3));table['eventKinds'].append(kind)
   for j,v in enumerate(values):table['eventValue'+str(j)].append(v)
  table['trackEventEnds'].append(len(table['eventTimes']))
 for level in levels:
  included=[next((i+1 for i,t in enumerate(tracks) if t['song']==s and not t['variation']),None) for s in level['songs']]
  if not included or any(v is None for v in included):continue
  table['weekTitles'].append(level['name'].upper());table['weekStarts'].append(len(table['weekTracks'])+1);table['weekTracks']+=included;table['weekEnds'].append(len(table['weekTracks']))
 table['difficultyNames']=[s.upper() for s in DIFFICULTIES];table['toggleNames']=['OFF','ON']
 data=''.join(declaration(k,v) for k,v in table.items())
 sounds=[];segment=1
 for t in tracks:
  for p in sorted((ROOT/'assets/audio'/t['key']).glob('[0-9][0-9][0-9][0-9].wav')):
   sounds.append(f'sound {lit("../"+str(p.relative_to(ROOT)))} as "s{segment}"');segment+=1
 for s in ['menu','scrollMenu','confirmMenu','cancelMenu']:sounds.append(f'sound "../assets/{s}.mp3" as "{s}"')
 costumes=[]
 for path in sorted((ROOT/'assets').glob('*.png')):
  with Image.open(path) as im:w,h=im.size
  metadata={'rotationCenterX':w/2,'rotationCenterY':h/2,'bitmapResolution':1}
  costumes.append(f'costume {lit("../"+str(path.relative_to(ROOT)))} as {lit(path.stem)} metadata {lit(lit(metadata))}')
 glyphchars='ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 .,:!?%/+-()[]<>_\'=&'
 glyphs=declaration('glyphChars',list(glyphchars))+declaration('glyphCostumes',['g'+str(ord(c)) for c in glyphchars])
 engine=(ROOT/'src/engine.sbtext').read_text().replace('@@DATA@@',data).replace('@@SOUNDS@@','\n'.join(sounds))
 renderer=(ROOT/'src/renderer.sbtext').read_text().replace('@@COSTUMES@@','\n'.join(costumes)).replace('@@GLYPHS@@',glyphs)
 out=BUILD/'funkin.sbtext';out.write_text('# Generated by tools/build.py. Edit src/ for engine changes.\n'+engine+'\n'+renderer)
 manifest={'tracks':len(tracks),'charts':sum(1 for n in table['chartStarts'] if n),'notes':len(table['allTimes']),'audioSegments':len(table['segmentTimes']),'costumes':len(costumes),'weeks':len(table['weekTitles']),'source':'funkin.sbtext','compiler':'sbtext-rs','upstream':{k:v['commit'] for k,v in json.loads((ROOT/'upstream.lock.json').read_text()).items()}}
 (BUILD/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n');(BUILD/'catalog.json').write_text(json.dumps(tracks,indent=2));print(json.dumps(manifest,indent=2),flush=True)
 return out

def validate(archive):
 with zipfile.ZipFile(archive) as z:
  p=json.loads(z.read('project.json'));files=set(z.namelist());blocks=0
  for t in p['targets']:
   ids=set(t['blocks'])
   for key,b in t['blocks'].items():
    for field in ['next','parent']:
     if b.get(field) and b[field] not in ids:raise ValueError(f'Dangling block {key}.{field}')
    for inp in b.get('inputs',{}).values():
     for val in inp[1:]:
      if isinstance(val,str) and val not in ids:raise ValueError(f'Dangling input {key}: {val}')
   for a in t['costumes']+t['sounds']:
    if a['md5ext'] not in files:raise ValueError('Missing asset '+a['md5ext'])
   blocks+=len(ids)
  if p['extensions']!=['pen']:raise ValueError('Expected native Pen extension only')
 print(f'Validated {blocks} blocks and {len(files)-1} embedded assets; no missing references.',flush=True)

if __name__=='__main__':
 parser=argparse.ArgumentParser();parser.add_argument('--compiler',default=str(ROOT.parents[1]/'target/release/sbtext-rs'));parser.add_argument('--generate-only',action='store_true');args=parser.parse_args();source=generate()
 if not args.generate_only:
  subprocess.run([args.compiler,str(source),str(BUILD/'Funkin.sb3'),'--no-svg-scale'],check=True);validate(BUILD/'Funkin.sb3')
