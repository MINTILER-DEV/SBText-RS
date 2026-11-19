#!/usr/bin/env python3
"""Serve the local Scratch player. No deployment or external server is required."""
import argparse, http.server, json, mimetypes
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
VENDOR={
 'vm.js':'scratch-vm/dist/web/scratch-vm.js',
 'render.js':'scratch-render/dist/web/scratch-render.js',
 'storage.js':'scratch-storage/dist/web/scratch-storage.js',
 'audio.js':'scratch-audio/dist.js',
}
class Handler(http.server.SimpleHTTPRequestHandler):
 def __init__(self,*args,**kwargs):super().__init__(*args,directory=str(ROOT/'build'),**kwargs)
 def do_GET(self):
  if self.path.split('?')[0] in ['/','/index.html']:
   path=ROOT/'tools/player.html'
  elif self.path.startswith('/vendor/'):
   key=self.path.split('?')[0].rsplit('/',1)[1]
   if key not in VENDOR:self.send_error(404);return
   path=ROOT/'node_modules'/VENDOR[key]
  else:return super().do_GET()
  if not path.is_file():self.send_error(404,'Run npm install in examples/funkin first');return
  self.send_response(200);self.send_header('Content-Type',mimetypes.guess_type(path.name)[0] or 'application/javascript');self.send_header('Content-Length',str(path.stat().st_size));self.end_headers()
  with path.open('rb') as f:
   import shutil
   shutil.copyfileobj(f,self.wfile)
 def log_message(self,*args):pass
if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('--port',type=int,default=8765);args=p.parse_args()
 print(f'Play FNF: http://localhost:{args.port} (Ctrl-C to stop)',flush=True)
 http.server.ThreadingHTTPServer(('127.0.0.1',args.port),Handler).serve_forever()
