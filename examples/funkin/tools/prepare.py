#!/usr/bin/env python3
"""Build genuine FNF chart tables, audio, and flattened animation assets."""
import argparse, concurrent.futures, json, math, subprocess, wave, xml.etree.ElementTree as ET
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont, ImageOps
from fetch import ROOT, CACHE, lock, get, batch
ASSETS=ROOT/'assets'; ASSETS.mkdir(exist_ok=True)
LOCK=lock(); FILES=LOCK['funkin.assets']['files']
def data(path):return json.loads(get('funkin.assets',path).read_text(encoding='utf-8-sig'))
def run(args):subprocess.run(args,check=True,stdout=subprocess.DEVNULL,stderr=subprocess.PIPE)
def compose(a,b):
 A,B,C,D,E,F=a;g,h,i,j,k,l=b
 return [A*g+C*h,B*g+D*h,A*i+C*j,B*i+D*j,A*k+C*l+E,B*k+D*l+F]
def point(m,x,y):return(m[0]*x+m[2]*y+m[4],m[1]*x+m[3]*y+m[5])
IDENT=[1,0,0,1,0,0]
class Atlas:
 def __init__(self,path):
  self.a=data(path+'/Animation.json');self.symbols={s['SN']:s['TL'] for s in self.a.get('SD',{}).get('S',[])};self.parts={}
  for p in FILES:
   if p.startswith(path+'/spritemap') and p.endswith('.json'):
    d=data(p);image=Image.open(get('funkin.assets',p[:-5]+'.png')).convert('RGBA')
    for e in d['ATLAS']['SPRITES']:
     e=e['SPRITE'];part=image.crop((e['x'],e['y'],e['x']+e['w'],e['y']+e['h']))
     if e.get('rotated'):part=part.transpose(Image.Transpose.ROTATE_90)
     self.parts[str(e['name'])]=part
 def leaves(self,tl,frame,m=IDENT,depth=0):
  if depth>24:return []
  out=[]
  for layer in reversed(tl.get('L',[])):
   frames=layer.get('FR',[]);f=next((f for f in frames if f['I']<=frame<f['I']+f.get('DU',1)),None)
   if f is None:continue
   for e in f.get('E',[]):
    v=e.get('SI',e.get('ASI',{}));mx=compose(m,v.get('MX',IDENT))
    if 'ASI' in e and str(v.get('N')) in self.parts:out.append((self.parts[str(v['N'])],mx))
    elif 'SI' in e and v.get('SN') in self.symbols:
     sub=self.symbols[v['SN']];length=max((z['I']+z.get('DU',1) for l in sub['L'] for z in l.get('FR',[])),default=1)
     sf=v.get('FF',0)+(frame-f['I'] if v.get('LP','LP')!='SF' else 0)
     sf=sf%length if v.get('LP','LP')=='LP' else min(sf,length-1)
     out+=self.leaves(sub,sf,mx,depth+1)
  return out
 def render(self,frame,max_dimension=500):
  leaves=self.leaves(self.a['AN']['TL'],frame)
  if not leaves:raise ValueError('Empty atlas frame')
  corners=[point(m,x,y) for im,m in leaves for x,y in [(0,0),(im.width,0),(0,im.height),(im.width,im.height)]]
  x0=math.floor(min(p[0] for p in corners));y0=math.floor(min(p[1] for p in corners));x1=math.ceil(max(p[0] for p in corners));y1=math.ceil(max(p[1] for p in corners))
  scale=min(1,max_dimension/max(x1-x0,y1-y0));w=max(1,math.ceil((x1-x0)*scale));h=max(1,math.ceil((y1-y0)*scale));out=Image.new('RGBA',(w,h))
  for im,m in leaves:
   a,b,c,d,e,f=m;e=(e-x0)*scale;f=(f-y0)*scale;a*=scale;b*=scale;c*=scale;d*=scale;det=a*d-b*c
   if abs(det)<1e-8:continue
   inv=(d/det,-c/det,(c*f-d*e)/det,-b/det,a/det,(b*e-a*f)/det)
   out.alpha_composite(im.transform((w,h),Image.Transform.AFFINE,inv,Image.Resampling.BICUBIC))
  return out
 def named(self,prefix,phase):
  for layer in self.a['AN']['TL']['L']:
   for f in layer.get('FR',[]):
    if f.get('N','').strip().lower()==prefix.strip().lower():return self.render(f['I']+min(phase*2,max(0,f.get('DU',1)-1)))
  if prefix in self.symbols:
   old=self.a['AN']['TL'];self.a['AN']['TL']=self.symbols[prefix]
   try:return self.render(phase*2)
   finally:self.a['AN']['TL']=old
  raise ValueError('Unknown animation '+prefix)

def resolve(asset):
 lib,path=asset.split(':') if ':' in asset else ('shared',asset)
 return lib+'/images/'+path

def sparrow(path,prefix,phase):
 xml=ET.parse(get('funkin.assets',path+'.xml'));frames=[f for f in xml.getroot() if f.get('name','').lower().startswith(prefix.lower())]
 if not frames:raise ValueError('Missing Sparrow prefix '+prefix)
 f=frames[min(phase*2,len(frames)-1)];im=Image.open(get('funkin.assets',path+'.png')).convert('RGBA');x,y,w,h=[int(f.get(k)) for k in ['x','y','width','height']];im=im.crop((x,y,x+w,y+h))
 if f.get('rotated')=='true':im=im.transpose(Image.Transpose.ROTATE_90)
 fw,fh=int(f.get('frameWidth',im.width)),int(f.get('frameHeight',im.height));out=Image.new('RGBA',(fw,fh));out.alpha_composite(im,(-int(f.get('frameX',0)),-int(f.get('frameY',0))));return out

def character(name):
 meta=data('preload/data/characters/'+name+'.json');animations=meta.get('animations',[]);atlas={};images=[]
 for pose in ['idle','singLEFT','singDOWN','singUP','singRIGHT']:
  anim=next((a for a in animations if a['name']==pose),next((a for a in animations if a['name'] in ['danceLeft','danceRight','idle']),animations[0] if animations else None))
  if anim is None:raise ValueError('No animations for '+name)
  for phase in range(3):
   path=resolve(anim.get('assetPath',meta['assetPath']));prefix=anim.get('prefix',pose)
   if path+'/Animation.json' in FILES:
    if path not in atlas:atlas[path]=Atlas(path)
    try:im=atlas[path].named(prefix,phase)
    except ValueError:
     # A few characters use symbols instead of timeline labels.
     im=atlas[path].named(anim.get('prefix','Idle'),0)
   elif path+'.xml' in FILES:im=sparrow(path,prefix,phase)
   elif path+'.txt' in FILES:
    rows=[r for r in get('funkin.assets',path+'.txt').read_text().splitlines() if r.lower().startswith(prefix.lower())]
    if not rows:raise ValueError('Unknown packed animation '+prefix)
    row=rows[min(phase*2,len(rows)-1)];values=row.split('=',1)[1].strip().split();x,y,w,h=map(int,values[:4]);im=Image.open(get('funkin.assets',path+'.png')).convert('RGBA').crop((x,y,x+w,y+h))
   else:raise ValueError('No asset for '+name+': '+path)
   box=im.getbbox()
   if box:im=im.crop(box)
   if meta.get('flipX'):im=ImageOps.mirror(im)
   images.append(im)
 # A consistent canvas prevents jitter when costumes change.
 maxw=max(i.width for i in images);maxh=max(i.height for i in images);scale=160/maxh
 for i,im in enumerate(images):
  im=im.resize((max(1,round(im.width*scale)),max(1,round(im.height*scale))),Image.Resampling.NEAREST if 'pixel' in name or name.startswith(('senpai','spirit')) else Image.Resampling.LANCZOS)
  canvas=Image.new('RGBA',(max(1,round(maxw*scale)),170));canvas.alpha_composite(im,((canvas.width-im.width)//2,170-im.height));canvas.save(ASSETS/f'char-{name}-{i}.png')
 print('Character',name,flush=True)

def catalog():
 result=[]
 for path in sorted(FILES):
  if not path.startswith('preload/data/songs/') or '-chart' not in path or not path.endswith('.json'):continue
  song=path.split('/')[3];variation=Path(path).stem.split('-chart',1)[1].lstrip('-');meta=data(path.replace('-chart','-metadata'));chart=data(path)
  if not chart.get('notes'):continue
  pd=meta['playData'];chars=pd['characters'];key=song+('-'+variation if variation else '')
  inst='songs/'+song+'/Inst'+('-'+pd['characters'].get('instrumental',variation) if pd['characters'].get('instrumental',variation) else '')+'.mp3'
  if inst not in FILES:raise ValueError('Missing instrumental '+inst)
  voices=[]
  for actor in chars.get('playerVocals',[chars['player']])+chars.get('opponentVocals',[chars['opponent']]):
   aliases=[actor]
   if actor.startswith('bf-'):aliases+=['bf']
   if actor.startswith('pico-'):aliases+=['pico-playable','pico']
   aliases += [{'mom-car':'mom','spooky-dark':'spooky','tankman-bloody':'tankman','senpai-angry':'senpai','monster-christmas':'monster'}.get(actor,actor)]
   for alias in dict.fromkeys(aliases):
    v=f'songs/{song}/Voices-{alias}'+('-'+variation if variation else '')+'.mp3'
    if v in FILES:voices.append(v);break
  # Some charts have combined vocal tracks.
  if not voices:
   v=f'songs/{song}/Voices'+('-'+variation if variation else '')+'.mp3'
   if v in FILES:voices.append(v)
  title=meta['songName'].upper().replace('(FEAT. J-HOPE OF BTS) (CLEAN VER.)','').strip()
  result.append(dict(key=key,song=song,variation=variation,title=title,artist=meta.get('artist',''),chart=path,meta=path.replace('-chart','-metadata'),difficulties=list(chart['notes']),player=chars['player'],opponent=chars['opponent'],girlfriend=chars.get('girlfriend',''),stage=pd.get('stage','mainStage'),audio=[inst]+list(dict.fromkeys(voices))))
 return result

def audio(track):
 folder=ASSETS/'audio'/track['key'];folder.mkdir(parents=True,exist_ok=True);manifest=folder/'segments.json'
 if manifest.exists():
  cached=json.loads(manifest.read_text())
  if cached.get('sources')==track['audio'] and cached.get('encoding')=='ima-adpcm-22050-v1':track.update(cached);return
 for pattern in ['*.mp3','*.wav']:
  for old in folder.glob(pattern):old.unlink()
 inputs=[get('funkin.assets',p) for p in track['audio']];wav=folder/'mix.wav';args=['ffmpeg','-v','error','-y']
 for p in inputs:args+=['-i',str(p)]
 args+=['-filter_complex',f'amix=inputs={len(inputs)}:duration=longest:normalize=0,alimiter=limit=0.95','-ac','1','-ar','22050',str(wav)];run(args)
 with wave.open(str(wav)) as w:
  duration=w.getnframes()/w.getframerate()
 # Scratch's native IMA ADPCM decoder avoids MP3 encoder delay at chunk seams.
 run(['ffmpeg','-v','error','-y','-i',str(wav),'-c:a','adpcm_ima_wav','-block_size','256','-f','segment','-segment_time','1','-segment_list',str(folder/'segments.csv'),'-segment_list_type','csv','-reset_timestamps','1',str(folder/'%04d.wav')])
 import csv
 rows=list(csv.reader((folder/'segments.csv').read_text().splitlines()));starts=[round(float(r[1])*1000,3) for r in rows]
 info={'duration':round(duration*1000),'segments':starts,'sources':track['audio'],'encoding':'ima-adpcm-22050-v1'};manifest.write_text(json.dumps(info));wav.unlink();track.update(info);print('Audio',track['key'],len(starts),'segments',flush=True)

def main():
 parser=argparse.ArgumentParser();parser.add_argument('--base-only',action='store_true');parser.add_argument('--song');args=parser.parse_args();tracks=catalog()
 if args.base_only:tracks=[t for t in tracks if not t['variation']]
 if args.song:tracks=[t for t in tracks if t['song']==args.song]
 batch([('funkin.assets',p) for t in tracks for p in t['audio']])
 with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:list(pool.map(audio,tracks))
 names=sorted({t[k] for t in tracks for k in ['player','opponent','girlfriend'] if t[k]})
 for name in names:
  if not all((ASSETS/f'char-{name}-{i}.png').exists() for i in range(15)):character(name)
 (ROOT/'generated').mkdir(exist_ok=True);(ROOT/'generated/catalog.json').write_text(json.dumps(tracks,indent=2))
 print('Prepared',len(tracks),'tracks',len(names),'characters',flush=True)
if __name__=='__main__':main()
