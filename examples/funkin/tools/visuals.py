#!/usr/bin/env python3
"""Flatten upstream stage props and prepare original note/UI art for Scratch."""
import json, math, shutil
from PIL import Image, ImageDraw, ImageFont, ImageOps
from prepare import ASSETS, FILES, Atlas, data, get, sparrow, ROOT
Image.MAX_IMAGE_PIXELS=None

def prop_image(path,prop):
 if path+'/Animation.json' in FILES:return Atlas(path).render(0,4096)
 if path+'.xml' in FILES:
  anims=prop.get('animations',[])
  if anims:return sparrow(path,anims[0].get('prefix',''),0)
  import xml.etree.ElementTree as ET
  first=ET.parse(get('funkin.assets',path+'.xml')).getroot()[0]
  return sparrow(path,first.get('name',''),0)
 if path+'.png' in FILES:return Image.open(get('funkin.assets',path+'.png')).convert('RGBA')
 return None

def background(name):
 stage=data('preload/data/stages/'+name+'.json');roles=stage.get('characters',{});bf=roles.get('bf',{}).get('position',[1000,900]);dad=roles.get('dad',{}).get('position',[350,900]);cx=(bf[0]+dad[0])/2;ground=max(bf[1],dad[1]);scale=240/max(abs(bf[0]-dad[0]),600);left=cx-240/scale;top=ground-282/scale
 out=Image.new('RGBA',(960,720),'#15121d');used=[]
 for prop in sorted(stage.get('props',[]),key=lambda x:x.get('zIndex',0)):
  if prop.get('alpha',1)==0 or prop.get('zIndex',0)>=500:continue
  asset=prop.get('assetPath','');pos=prop.get('position',[0,0]);sz=prop.get('scale',[1,1]);sz=[sz,sz] if isinstance(sz,(int,float)) else sz
  if asset.startswith('#'):
   dimensions=prop.get('size',[1280,720]);im=Image.new('RGBA',tuple(int(v) for v in dimensions),asset)
  else:
   candidates=[asset] if '/images/' in asset else ([asset.replace(':','/images/')] if ':' in asset else [stage.get('directory','shared')+'/images/'+asset,'shared/images/'+asset,'preload/images/'+asset])
   path=next((p for p in candidates if p+'.png' in FILES or p+'/Animation.json' in FILES),None)
   if not path:continue
   try:im=prop_image(path,prop)
   except (ValueError,IndexError) as e:print('Stage prop skipped:',name,path,str(e),flush=True);continue
   if im is None:continue
   used.append(path)
  w=max(1,round(im.width*abs(sz[0])*scale*2));h=max(1,round(im.height*abs(sz[1])*scale*2))
  if w>10000 or h>10000:continue
  im=im.resize((w,h),Image.Resampling.NEAREST if prop.get('isPixel') else Image.Resampling.LANCZOS)
  if sz[0]<0:im=ImageOps.mirror(im)
  if prop.get('alpha',1)<1:im.putalpha(im.getchannel('A').point(lambda v:round(v*prop['alpha'])))
  out.alpha_composite(im,(round((pos[0]-left)*scale*2),round((pos[1]-top)*scale*2)))
 out=out.resize((480,360),Image.Resampling.LANCZOS)
 # A translucent playfield keeps fast notes visible over the original backgrounds.
 shade=Image.new('RGBA',(480,360));d=ImageDraw.Draw(shade);d.rectangle((0,0,480,54),fill=(9,7,17,230));d.rectangle((0,54,480,320),fill=(0,0,0,35));d.rectangle((19,54,203,319),fill=(0,0,0,65));d.rectangle((277,54,461,319),fill=(0,0,0,90));d.rectangle((0,320,480,360),fill=(9,7,17,238));out.alpha_composite(shade);out.save(ASSETS/f'bg-{name}.png')
 print('Stage',name,len(used),'props',flush=True)
 return {'stage':name,'props':used,'player':[120,-17],'opponent':[-120,-17],'girlfriend':[0,0]}

def main():
 fontpath=get('funkin.assets','fonts/vcr.ttf');font=ImageFont.truetype(str(fontpath),16)
 glyphs='ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 .,:!?%/+-()[]<>_\'=&'
 for ch in glyphs:
  im=Image.new('RGBA',(10,19));ImageDraw.Draw(im).text((0,-1),ch,font=font,fill='white',stroke_width=0);im.save(ASSETS/f'g{ord(ch)}.png')
 # Original note frames, including static and pressed receptors.
 import xml.etree.ElementTree as ET
 path='preload/images/NOTE_assets';xml=ET.parse(get('funkin.assets',path+'.xml'));frames=[f.get('name') for f in xml.getroot()]
 (ROOT/'generated/note-frames.json').write_text(json.dumps(frames))
 choices=[['purple instance','blue instance','green instance','red instance'],['arrow static instance 1','arrow static instance 2','arrow static instance 4','arrow static instance 3'],['left confirm','down confirm','up confirm','right confirm']]
 for group,prefixes in enumerate(choices):
  for lane,prefix in enumerate(prefixes):
   im=sparrow(path,prefix,0);box=im.getbbox();im=im.crop(box) if box else im
   im.thumbnail((36,36),Image.Resampling.LANCZOS);canvas=Image.new('RGBA',(42,42));canvas.alpha_composite(im,((42-im.width)//2,(42-im.height)//2));canvas.save(ASSETS/f'note-{group*4+lane}.png')
 # Countdown and judgement art remain original upstream exports.
 for stem in ['ready','set','go']:
  im=Image.open(get('funkin.assets',f'shared/images/ui/countdown/funkin/{stem}.png')).convert('RGBA');im.thumbnail((190,95),Image.Resampling.LANCZOS);im.save(ASSETS/f'ui-{stem}.png')
 for stem in ['sick','good','bad','shit']:
  paths=[p for p in FILES if p.endswith('/'+stem+'.png')];p=next((p for p in paths if '/pixelUI/' not in p),paths[0] if paths else None)
  if p:
   im=Image.open(get('funkin.assets',p)).convert('RGBA');im.thumbnail((95,45),Image.Resampling.LANCZOS);im.save(ASSETS/f'ui-{stem}.png')
 logo=sparrow('preload/images/logoBumpin','logo bumpin',0);logo.thumbnail((300,175),Image.Resampling.LANCZOS);logo.save(ASSETS/'logo.png')
 # Deliberate title layout; live characters and menu labels are stamped on top.
 im=Image.new('RGBA',(480,360),'#f4c5dd');d=ImageDraw.Draw(im)
 d.polygon([(180,0),(480,0),(480,360),(324,360)],fill='#e85c9c');d.polygon([(306,0),(346,0),(480,360),(439,360)],fill='#fa89bb')
 for y in range(-80,430,22):d.line([(160,y),(510,y+100)],fill='#e34890',width=2)
 d.rectangle((0,321,480,360),fill='#171220');im.save(ASSETS/'menu-bg.png')
 for stem,color in [('white','#ffffff'),('black','#100d19'),('pink','#f653a1'),('cyan','#3df7ff'),('green','#66ff33'),('red','#ec2f4b'),('yellow','#f9cf51')]:
  Image.new('RGBA',(2,2),color).save(ASSETS/('dot-'+stem+'.png'))
 for snd in ['scrollMenu','confirmMenu','cancelMenu']:
  shutil.copyfile(get('funkin.assets','preload/sounds/'+snd+'.mp3'),ASSETS/(snd+'.mp3'))
 # The menu loop is stored as one sound; it is stopped before gameplay starts.
 run=None
 import subprocess
 subprocess.run(['ffmpeg','-v','error','-y','-i',str(get('funkin.assets','preload/music/freakyMenu/freakyMenu.mp3')),'-ar','22050','-ac','1','-b:a','64k',str(ASSETS/'menu.mp3')],check=True)
 tracks=json.loads((ROOT/'generated/catalog.json').read_text());provenance=[]
 for name in sorted({t['stage'] for t in tracks}):provenance.append(background(name))
 (ROOT/'generated/stages.json').write_text(json.dumps(provenance,indent=2))
 # Original health icons also provide recognizable character portraits in menus.
 for name in sorted({t[k] for t in tracks for k in ['player','opponent']}):
  candidates=[f'preload/images/icons/icon-{name}.png',f'preload/images/icons/icon-{name.split("-playable")[0]}.png']
  p=next((p for p in candidates if p in FILES),None)
  if p:
   im=Image.open(get('funkin.assets',p)).convert('RGBA');im=im.crop((0,0,min(150,im.width),im.height));im.thumbnail((46,46),Image.Resampling.LANCZOS);im.save(ASSETS/f'icon-{name}.png')
  else:
   im=Image.open(ASSETS/f'char-{name}-0.png');im.thumbnail((46,46),Image.Resampling.LANCZOS);im.save(ASSETS/f'icon-{name}.png')
 print('Visual assets ready',flush=True)
if __name__=='__main__':main()
