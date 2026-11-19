const path=require('node:path');
require('esbuild').buildSync({entryPoints:[require.resolve('scratch-audio/src/index.js')],outfile:path.join(__dirname,'../build/audio.js'),bundle:true,platform:'browser',format:'iife',globalName:'ScratchAudio',minify:true});
require('node:fs').copyFileSync(path.join(path.dirname(require.resolve('scratch-svg-renderer')),'../web/scratch-svg-renderer.js'),path.join(__dirname,'../build/svg.js'));
