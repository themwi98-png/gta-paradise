"""Writes a self-contained HTML tool for lining up Burnout's district grid
over the GTA-space road network and coastline by hand."""
import sys, struct, base64, io, json
import numpy as np
from PIL import Image

npy, grid_bin, lanes_csv, out = sys.argv[1:5]
g = np.load(npy)
names = ['Ocean View','West Acres','Twin Bridges','Big Surf Beach','Eastern Shore','Hillside Pass','Heartbreak Hills','Rockridge Cliffs','South Bay','Park Vale','Paradise Wharf','Crystal Summit','Lone Peaks','Sunset Valley','Downtown','River City','Motor City','Waterfront','Paradise Keys Bridge','North Beach','Midtown','South Coast',"Perren's Point"]
pal = np.random.RandomState(7).randint(60, 230, (256, 4)).astype(np.uint8); pal[:, 3] = 255; pal[255] = (0, 0, 0, 0)
def png(arr):
    b = io.BytesIO(); Image.fromarray(arr).save(b, 'PNG'); return base64.b64encode(b.getvalue()).decode()
# Two layers placed independently: mainland (0-17) and the island group
# (Paradise Keys Bridge + Big Surf Island districts, 18-22), which Burnout
# paints at a different scale.
ISLAND = list(range(18, 23))
img = pal[g].copy(); img[np.isin(g, ISLAND)] = 0
dist_png = png(img)
img = pal[g].copy(); img[~np.isin(g, ISLAND)] = 0
isl_png = png(img)
labels = []
for i, n in enumerate(names):
    ys, xs = np.nonzero(g == i)
    if len(xs): labels.append([n, float(xs.mean() + .5), float(ys.mean() + .5), 1 if i in ISLAND else 0])

raw = open(grid_bin, 'rb').read()
gw, gh = struct.unpack_from('<II', raw); ox, oy = struct.unpack_from('<ff', raw, 8)
cls = np.frombuffer(raw, dtype=np.uint8, offset=16).reshape(gh, gw)
land = np.zeros((gh, gw, 4), np.uint8); land[cls >= 1] = (70, 110, 70, 255); land[cls == 2] = (60, 120, 200, 255)
land_png = png(land[::-1])  # row 0 = north

a = np.loadtxt(lanes_csv, delimiter=',', skiprows=1)
key = a[:, 0] * 1000 + a[:, 1]
lines = []
for k in np.unique(key):
    p = a[key == k][:, 3:5]
    lines.append([round(float(v), 0) for v in p[::2].ravel()] + [round(float(v), 0) for v in p[-1]])

html = """<!doctype html><html><head><meta charset="utf-8"><title>District Aligner</title>
<style>
:root{--bg:#111;--fg:#eee;--panel:#222}
body{margin:0;background:var(--bg);color:var(--fg);font:14px system-ui;display:flex;height:100vh}
#side{width:300px;padding:12px;background:var(--panel);overflow:auto}
#side label{display:block;margin:8px 0 2px}
#side input[type=number]{width:120px}
canvas{flex:1;cursor:grab;display:block}
#out{width:100%;height:60px;font:12px monospace}
button{margin:4px 0}
</style></head><body>
<div id="side">
<h3>District Aligner</h3>
<p>Pick a layer, then drag it (or arrow keys, Shift = bigger steps; [ and ] scale) until it sits over Paradise's roads and coast. Mouse wheel zooms, right-drag pans. Then copy the line at the bottom back to Claude.</p>
<label><input type="radio" name="layer" value="m" checked> Mainland districts (0-17)</label>
<label><input type="radio" name="layer" value="i"> Island + bridge districts (18-22)</label>
<label>Cell width (m, east-west) — keys [ ]</label><input id="cell" type="number" step="0.05">
<label>Cell height (m, north-south) — keys - =</label><input id="cellY" type="number" step="0.05">
<label>Grid west edge x (m)</label><input id="x0" type="number" step="1">
<label>Grid north edge y (m)</label><input id="y0" type="number" step="1">
<label><input id="fr" type="checkbox"> Flip rows (north/south)</label>
<label><input id="fc" type="checkbox"> Flip columns (east/west)</label>
<label>District opacity</label><input id="op" type="range" min="0" max="1" step="0.05" value="0.55">
<label><input id="lb" type="checkbox" checked> Show names</label>

<p>Result:</p><textarea id="out" readonly></textarea>
<button onclick="navigator.clipboard.writeText(document.getElementById('out').value)">Copy</button>
</div>
<canvas id="c"></canvas>
<script>
const ISL="data:image/png;base64,__ISL__";
const DIST="data:image/png;base64,__DIST__", LAND="data:image/png;base64,__LAND__";
const LINES=__LINES__, LABELS=__LABELS__, GW=__W__, GH=__H__;
const LAND_X0=__LX0__, LAND_Y1=__LY1__, LAND_W=__LW__, LAND_H=__LH__;
const L={m:{cell:26.6667,cellY:26.6667,x0:-3920,y0:4000,fr:false,fc:false},i:{cell:26.6667,cellY:26.6667,x0:-3920,y0:4000,fr:false,fc:false}};
try{const s=JSON.parse(localStorage.getItem('districtAlign2')||'{}');if(s.m)Object.assign(L.m,s.m);if(s.i)Object.assign(L.i,s.i)}catch(e){}
let P=L.m;
for(const r of document.querySelectorAll('input[name=layer]'))r.onchange=()=>{P=L[r.value];r.blur();sync()};
const c=document.getElementById('c'),ctx=c.getContext('2d');
const di=new Image(),ii=new Image(),li=new Image();di.src=DIST;ii.src=ISL;li.src=LAND;
let view={cx:800,cy:900,s:0.09};
const ids=['cell','cellY','x0','y0','fr','fc'];
function sync(){for(const k of ids){const e=document.getElementById(k);if(e.type==='checkbox')e.checked=P[k];else e.value=(+P[k]).toFixed(k.startsWith('cell')?3:0)}
 const f=q=>`cellX=${(+q.cell).toFixed(3)} cellY=${(+(q.cellY??q.cell)).toFixed(3)} x0=${(+q.x0).toFixed(0)} y0=${(+q.y0).toFixed(0)} flipRows=${q.fr} flipCols=${q.fc}`;
 document.getElementById('out').value=`districts mainland ${f(L.m)} | island ${f(L.i)}`;
 try{localStorage.setItem('districtAlign2',JSON.stringify(L))}catch(e){};draw()}
for(const k of ids)document.getElementById(k).addEventListener('input',e=>{P[k]=e.target.type==='checkbox'?e.target.checked:+e.target.value;sync()});
document.getElementById('op').oninput=draw;document.getElementById('lb').oninput=draw;
const W2S=(x,y)=>[(x-view.cx)*view.s+c.width/2,(view.cy-y)*view.s+c.height/2];
function draw(){c.width=c.clientWidth;c.height=c.clientHeight;ctx.fillStyle='#0d1830';ctx.fillRect(0,0,c.width,c.height);
 let [a,b]=W2S(LAND_X0,LAND_Y1);ctx.imageSmoothingEnabled=false;ctx.drawImage(li,a,b,LAND_W*view.s,LAND_H*view.s);
 for(const [q,im] of [[L.m,di],[L.i,ii]]){ctx.save();ctx.globalAlpha=+document.getElementById('op').value*(q===P?1:0.7);
  [a,b]=W2S(q.x0,q.y0);ctx.translate(a,b);const sw=GW*q.cell*view.s,sh=GH*(q.cellY??q.cell)*view.s;
  ctx.translate(q.fc?sw:0,q.fr?sh:0);ctx.scale(q.fc?-1:1,q.fr?-1:1);ctx.drawImage(im,0,0,sw,sh);ctx.restore()}
 ctx.strokeStyle='#ffe14a';ctx.lineWidth=1;ctx.beginPath();
 for(const l of LINES){for(let i=0;i<l.length;i+=2){const [x,y]=W2S(l[i],l[i+1]);i?ctx.lineTo(x,y):ctx.moveTo(x,y)}}ctx.stroke();
 if(document.getElementById('lb').checked){ctx.fillStyle='#fff';ctx.font='12px system-ui';for(const [n,col,row,isl] of LABELS){const q=isl?L.i:L.m;
  const cc=q.fc?GW-col:col,rr=q.fr?GH-row:row;const [x,y]=W2S(q.x0+cc*q.cell,q.y0-rr*(q.cellY??q.cell));ctx.fillText(n,x-20,y)}}}
let drag=null;c.onmousedown=e=>{drag={x:e.clientX,y:e.clientY,x0:P.x0,y0:P.y0,pan:e.button!==0||e.altKey}};
window.onmouseup=()=>drag=null;
window.onmousemove=e=>{if(!drag)return;const dx=(e.clientX-drag.x)/view.s,dy=(e.clientY-drag.y)/view.s;
 if(drag.pan){view.cx-=dx;view.cy+=dy;drag.x=e.clientX;drag.y=e.clientY;draw()}else{P.x0=drag.x0+dx;P.y0=drag.y0-dy;sync()}};
c.oncontextmenu=e=>e.preventDefault();
c.onwheel=e=>{e.preventDefault();view.s*=e.deltaY<0?1.15:1/1.15;draw()};
window.onkeydown=e=>{if(e.target.tagName==='INPUT')return;const st=e.shiftKey?100:10;
 if(e.key==='ArrowLeft')P.x0-=st;else if(e.key==='ArrowRight')P.x0+=st;else if(e.key==='ArrowUp')P.y0+=st;else if(e.key==='ArrowDown')P.y0-=st;
 else if(e.key==='[')P.cell-=e.shiftKey?0.5:0.05;else if(e.key===']')P.cell+=e.shiftKey?0.5:0.05;else if(e.key==='-')P.cellY=(P.cellY??P.cell)-(e.shiftKey?0.5:0.05);else if(e.key==='=')P.cellY=(P.cellY??P.cell)+(e.shiftKey?0.5:0.05);else return;e.preventDefault();sync()};
window.onresize=draw;di.onload=ii.onload=li.onload=draw;sync();
</script></body></html>"""
lw, lh = gw * 16.0, gh * 16.0
html = (html.replace('__ISL__', isl_png).replace('__DIST__', dist_png).replace('__LAND__', land_png).replace('__LINES__', json.dumps(lines, separators=(',', ':')))
        .replace('__LABELS__', json.dumps(labels)).replace('__W__', str(g.shape[1])).replace('__H__', str(g.shape[0]))
        .replace('__LX0__', str(ox)).replace('__LY1__', str(oy + lh)).replace('__LW__', str(lw)).replace('__LH__', str(lh)))
open(out, 'w', encoding='utf-8').write(html)
print('wrote', out, len(html) // 1024, 'KB')
