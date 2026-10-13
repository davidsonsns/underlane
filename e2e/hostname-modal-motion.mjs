// Run with Vite on port 1420. PLAYWRIGHT_MODULE can point to an installed Playwright module.
const { chromium } = await import(process.env.PLAYWRIGHT_MODULE ?? 'playwright');
import assert from 'node:assert/strict';
const browser=await chromium.launch({headless:true});
const errors=[];
const page=await browser.newPage({viewport:{width:1280,height:Number(process.env.HEIGHT??760)},reducedMotion:'no-preference'});
page.setDefaultTimeout(5000);
page.on('pageerror',e=>errors.push(String(e)));
await page.addInitScript((delay)=>{
 window.__loadDelay=delay;
 const hosts=['app.example.com','api.example.com','docs.example.com','checkout.example.com'];
 const routes=hosts.map((hostname,index)=>({index,hostname,service:'http://localhost:'+([4000,3000,4330,4000][index]),path:null}));
 const config={revision:'synthetic-r1',routes,fallback_service:'http_status:404'};
 const state={id:'synthetic',status:'connected',detail:null,hint:null,failure:null,public_url:null,mappings:[...routes.map(r=>({...r,inspected:false,note:null})),{hostname:null,path:null,service:'http_status:404',inspected:false,note:null}],pid:123,metrics_addr:null,connections:4,restarts:0,started_at:Date.now(),inspecting:false,supervised:true,run:1,config_revision:1};
 const tunnel={id:'synthetic',group_id:'group',group_name:'254',name:'example',kind:'config',config_path:'/synthetic/hostname-ui.yml',origin_url:null,inspect:false,autostart:false,loglevel:'info',created_at:Date.now(),sort:0,state,has_token:false};
 const overview={groups:[{id:'group',name:'254',sort:0}],tunnels:[tunnel],cloudflared:{path:'/synthetic/cloudflared',version:'2026.9.1',source:'mock'},settings:{cloudflared_path:null,log_ttl_days:7,max_db_mb:100,max_body_kb:1024,redact_headers:[],redact_query:[],theme:'dark',default_loglevel:'info'},active_count:1,dropped_captures:0,data_dir:'/synthetic',last_launch:null};
 const callbacks={};let next=1;
 window.__hostnameMock={config,overview,calls:[]};
 window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener:()=>{}};
 window.__TAURI_INTERNALS__={metadata:{currentWindow:{label:'dashboard'},currentWebview:{label:'dashboard'}},transformCallback(cb){const n=next++;callbacks[n]=cb;return n},unregisterCallback(n){delete callbacks[n]},async invoke(cmd,args){
  const m=window.__hostnameMock;m.calls.push({cmd,args});
  if(cmd==='overview') return structuredClone(m.overview);
  if(cmd==='load_hostnames') { await new Promise(r=>setTimeout(r,Number(window.__loadDelay??100))); return structuredClone(m.config); }
  if(cmd==='probe_services') return args.services.map(s=>s.startsWith('http_status:')?null:false);
  if(cmd==='check_config_change') return {run:1,config_revision:1,source_path:'/synthetic/hostname-ui.yml',status:'unchanged',message:null};
  if(cmd==='plugin:event|listen') return next++;
  if(cmd.startsWith('plugin:')) return null;
  if(cmd==='login_item_status') return {status:'unsupported',enabled:false,message:null};
  console.warn('Unmocked Tauri command',cmd);return null;
 }};
},Number(process.env.LOAD_DELAY_MS??100));
await page.goto('http://127.0.0.1:1420/#tunnels:synthetic');
await page.getByRole('heading',{name:'Hostnames',exact:true}).waitFor();


const samples = await page.evaluate(async () => {
 const frames=[]; let done=false;
 const sample=()=>{const e=document.querySelector('[role="dialog"]');if(e){const b=e.getBoundingClientRect(), s=getComputedStyle(e);frames.push({y:b.y,height:b.height,opacity:Number(s.opacity),loading:e.innerText.includes('Loading hostnames')});}if(!done)requestAnimationFrame(sample)};
 requestAnimationFrame(sample);
 [...document.querySelectorAll('button')].find(e=>e.textContent.trim()==='Manage hostnames').click();
 await new Promise(r=>setTimeout(r,2000));done=true;return frames;
});
assert.ok(samples.length>5,'Modal frame sampling missed the open');
const ys=samples.filter(f=>f.opacity>0.02).map(f=>f.y);
const movement=Math.max(...ys)-Math.min(...ys);
console.log(JSON.stringify({frames:samples.length,movement,first:samples[0],last:samples.at(-1),loadingFrames:samples.filter(f=>f.loading).length,loadCalls:await page.evaluate(()=>window.__hostnameMock.calls.filter(c=>c.cmd==='load_hostnames').length)},null,2));
await browser.close();
assert.ok(movement<1,`Modal jumped ${movement.toFixed(1)}px while opening`);
const heights=samples.map(f=>f.height);
assert.ok(Math.max(...heights)-Math.min(...heights)<1,'Modal changed height while opening');
assert.ok(samples.every(f=>!f.loading),'Short loading panel flashed before editor');
assert.deepEqual(errors, [], 'Unexpected browser error');
