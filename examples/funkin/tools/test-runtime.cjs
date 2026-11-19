const assert=require('node:assert/strict');
const path=require('node:path');
const fs=require('node:fs');
const {spawn,execFileSync}=require('node:child_process');
const {chromium}=require('playwright');
const root=path.join(__dirname,'..');
const screenshots=path.join(root,'build/screenshots');fs.mkdirSync(screenshots,{recursive:true});
const fullAudio=process.argv.includes('--full-audio');
if(!fullAudio)execFileSync('python3',[path.join(__dirname,'test-fixture.py')]);
const server=spawn('python3',[path.join(__dirname,'play.py'),'--port','8767'],{stdio:'ignore'});
let browser;
(async()=>{
 for(let i=0;i<60;i++){try{if((await fetch('http://127.0.0.1:8767')).ok)break}catch{}await new Promise(r=>setTimeout(r,100))}
 browser=await chromium.launch({headless:true,args:['--no-sandbox','--autoplay-policy=no-user-gesture-required','--enable-unsafe-swiftshader']});
 const page=await browser.newPage({viewport:{width:1100,height:1000}});
 const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.goto('http://127.0.0.1:8767/'+(fullAudio?'':'?project=Funkin-runtime-test.sb3'));
 await page.click('#launch');await page.waitForFunction(()=>window.ready,null,{timeout:120000});
 if(fullAudio){
  const audio=await page.evaluate(()=>{
   const stage=vm.runtime.getTargetForStage(),bank=stage.sprite.soundBank;
   const times=game.get('segmentTimes').map(Number),ends=new Set(game.get('trackSegmentEnds').map(Number));
   let tested=0,maxSeamError=0,energy=0;
   for(const sound of stage.sprite.sounds){
    if(!/^s[0-9]+$/.test(sound.name))continue;
    const index=Number(sound.name.slice(1));const buffer=bank.soundPlayers[sound.soundId].buffer;
    if(!ends.has(index))maxSeamError=Math.max(maxSeamError,Math.abs(buffer.duration*1000-(times[index]-times[index-1])));
    if(index%101===0){const samples=buffer.getChannelData(0);for(let i=0;i<samples.length;i+=97)energy+=samples[i]*samples[i]}
    tested++;
   }
   return {segments:tested,maxSeamError,energy};
  });
  console.log('Full audio:',JSON.stringify(audio));assert(audio.segments>6500);assert(audio.maxSeamError<10,'audio chunks must meet at their scheduled boundaries');assert(audio.energy>1,'soundtrack must decode to audible PCM');
 }
 const press=async key=>{await page.keyboard.press(key,{delay:90});await page.waitForTimeout(60)};
 const mode=expected=>page.waitForFunction(expected=>game.get('mode')===expected,expected,{timeout:5000});
 const shot=name=>page.screenshot({path:path.join(screenshots,name+'.png')});
 await mode('title');await shot('title');await press('Enter');await mode('menu');await shot('menu');
 await press('ArrowDown');await press('Enter');await mode('freeplay');await shot('freeplay');
 await press('ArrowDown');await press('b');await press('Enter');await mode('play');
 await page.waitForFunction(()=>Number(game.get('songms'))>3500,null,{timeout:15000});
 const playing=await page.evaluate(()=>({score:Number(game.get('score')),misses:Number(game.get('misses')),notes:Number(game.get('noteCount'))}));
 assert(playing.score>0,'botplay must hit real player notes');assert.equal(playing.misses,0);assert(playing.notes>50);await shot('gameplay');
 await press('Escape');await mode('paused');await shot('pause');
 const paused=await page.evaluate(()=>Number(game.get('songms')));await page.waitForTimeout(250);assert.equal(await page.evaluate(()=>Number(game.get('songms'))),paused);
 await press('Enter');await mode('play');await page.waitForTimeout(200);assert(await page.evaluate(()=>Number(game.get('songms')))>paused);
 const results=await page.evaluate(()=>{
  const assert=(condition,message)=>{if(!condition)throw Error(message)};
  vm.runtime.stopAll();
  const call=(name,args=[])=>{game.call(name,args);vm.runtime._step()};
  const num=name=>Number(game.get(name));
  let loaded=0;
  for(let track=1;track<=game.get('trackTitles').length;track++){
   for(let difficulty=1;difficulty<=5;difficulty++){
    const chart=(track-1)*5+difficulty,starts=game.get('chartStarts');
    if(!Number(starts[chart-1]))continue;
    game.set('selectedTrack',track);game.set('difficulty',difficulty);call('loadSong');
    assert(num('chartIndex')===chart,'chart mapping '+chart);
    assert(num('noteCount')===Number(game.get('chartEnds')[chart-1])-Number(starts[chart-1])+1,'note count '+chart);
    assert(game.get('noteTimes').length===num('noteCount'),'loaded chart length '+chart);
    assert(num('segmentIndex')<=num('segmentEnd'),'audio segment range '+chart);
    loaded++;
   }
  }
  // Test real chart input, timing score, sustain completion, drops, and misses.
  const bopeebo=game.get('trackKeys').indexOf('bopeebo')+1;
  game.set('selectedTrack',bopeebo);game.set('difficulty',3);game.set('botplay',0);call('loadSong');
  const times=game.get('noteTimes'),lanes=game.get('noteLanes'),lengths=game.get('noteLengths');
  const first=lanes.findIndex(v=>Number(v)<4);
  game.set('songms',Number(times[first]));game.set('rawms',Number(times[first]));game.set('deltaTime',33);
  game.get('tapped').fill(0);game.get('held').fill(0);game.get('tapped')[Number(lanes[first])]=1;game.get('held')[Number(lanes[first])]=1;
  call('processNotes');assert(num('hits')===1,'real chart key hit');assert(num('score')===500,'PBOT perfect score');
  game.get('tapped').fill(0);call('processNotes');assert(num('hits')===1,'held key must not double-score');
  const sustain=lengths.findIndex((l,i)=>Number(l)>160&&Number(lanes[i])<4);
  // Isolate the chosen hold after verifying the complete chart loaded correctly.
  game.get('noteStates').fill(2);game.get('noteStates')[sustain]=0;game.set('firstNote',sustain+1);
  game.set('songms',Number(times[sustain]));game.set('rawms',Number(times[sustain]));game.get('tapped')[Number(lanes[sustain])]=1;game.get('held').fill(0);game.get('held')[Number(lanes[sustain])]=1;
  call('processNotes');assert(Number(game.get('noteStates')[sustain])===1,'hold head hit');
  game.get('tapped').fill(0);game.set('songms',Number(times[sustain])+100);call('processNotes');assert(Number(game.get('noteStates')[sustain])===1,'hold remains active');
  game.set('songms',Number(times[sustain])+Number(lengths[sustain])+1);call('processNotes');assert(Number(game.get('noteStates')[sustain])===2,'hold completion');
  game.get('noteStates')[sustain]=1;game.get('holdTicks')[sustain]=Number(times[sustain]);game.get('held').fill(0);game.set('songms',Number(times[sustain])+150);const before=num('misses');call('processNotes');assert(num('misses')===before+1,'hold drop counted once');call('processNotes');assert(num('misses')===before+1,'hold drop not repeated');
  call('loadSong');const first2=game.get('noteLanes').findIndex(v=>Number(v)<4);game.set('songms',Number(game.get('noteTimes')[first2])+200);game.set('rawms',num('songms'));game.get('tapped').fill(0);call('processNotes');assert(num('misses')>0,'late note misses');assert(num('combo')===0,'miss resets combo');
  // Story includes every default song and advances directly to the next one.
  game.set('weekIndex',2);game.set('difficulty',2);call('startStory');assert(game.get('storyQueue').length===3,'week 1 tracklist');const queue=[...game.get('storyQueue')];game.set('score',1234);game.set('misses',0);game.set('accuracy',100);call('finishSong');assert(num('storyPosition')===2,'campaign advances');assert(num('selectedTrack')===Number(queue[1]),'campaign loads next song');assert(num('campaignScore')===1234,'campaign tallies score');
  // Failed runs, practice, results, and record gating use the same live engine.
  game.set('storyActive',0);game.set('health',0);game.set('practice',0);game.set('songOrigin',vm.runtime.ioDevices.clock.projectTimer()*1000-500);call('playTick');assert(game.get('mode')==='gameover','zero health fails');
  game.set('practice',1);call('loadSong');game.set('health',0);call('playTick');assert(game.get('mode')==='play','practice prevents failure');
  game.set('practice',0);game.set('botplay',0);game.set('score',4321);game.set('misses',0);game.set('accuracy',100);call('finishSong');assert(game.get('mode')==='results','result state');assert(Number(game.get('highScores')[num('chartIndex')-1])===4321,'ranked high score');
  game.set('botplay',1);game.set('score',9999);call('finishSong');assert(Number(game.get('highScores')[num('chartIndex')-1])===4321,'bot score excluded');
  return {loadedCharts:loaded,notes:game.get('allTimes').length,weeks:game.get('weekTitles').length,tracks:game.get('trackTitles').length};
 });
 console.log(JSON.stringify(results));assert.equal(results.loadedCharts,167);assert.equal(results.tracks,61);assert.equal(results.weeks,10);assert.deepEqual(errors,[]);console.log('PASS: menus, real chart keyboard hits, PBOT scoring, sustains, pause/resume, every chart, story, failure, practice, and high scores.');
})().catch(e=>{console.error(e);process.exitCode=1}).finally(async()=>{if(browser)await browser.close();server.kill()});
