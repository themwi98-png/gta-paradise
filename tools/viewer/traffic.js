// Traffic editor for the Paradise viewer: Burnout traffic lanes and parked
// cars (out/traffic/traffic.json, from `bpx traffic-json`), edited by hand and
// exported as paradise-traffic-edits-v1 JSON. Saved as
// tools/edits/traffic_edits.json, the edits are applied by bpx before GTA's
// roads and parked cars are built (crates/bpx/src/edits.rs mirrors the rules
// here: reverse first, then shift to the right of the driving direction).
//
// Traffic levels: each lane has Burnout's own traffic flow (vehicles per
// minute, 0-9; 0 = no cars start there), which becomes how much GTA traffic
// starts on that road. "Colour by traffic" shows it; "Set traffic" changes
// it for the selection, and "Select area" selects every lane in a box.
//
// Everything is in Burnout coordinates (y up), like the rest of the viewer.

const STORAGE_KEY = 'paradise-traffic-edits-v1';
const LIFT = 0.4;           // draw lanes this far above the road (m)
const CHEVRON_EVERY = 12;   // arrow spacing along lanes (m)
const PICK_PX = 12;         // click distance for selecting (pixels)
const SHIFT_STEP = 0.25;    // sideways nudge (m)
const DRAWN_TRAFFIC = 5;    // traffic level of a drawn lane (matches bpx)
const MAX_TRAFFIC = 9;      // busiest Burnout lane (colour scale)

const COLOURS = {
  lane: 0xffd23f, reversed: 0xff8a00, shifted: 0xffe98f, deleted: 0x7a2525,
  selected: 0x00e5ff, drawn: 0x3fff7a, connection: 0xff40ff,
  parked: 0x9ecbff, parkedDeleted: 0xb03030, parkedNew: 0x3fff7a, draft: 0xffffff,
};

const emptyEdits = () => ({ format: 'paradise-traffic-edits-v1', lanes: {}, connections: [], newLanes: [], parked: {}, newParked: [] });

// Right of the driving direction (Burnout x/z): GTA maps (x, y, z) to (x, -z, y).
function rightOf(tx, tz) {
  const l = Math.hypot(tx, tz) || 1;
  return [-tz / l, tx / l];
}

export function initTraffic({ THREE, scene, camera, renderer, panel, info, pickGround }) {
  panel.innerHTML = `
    <label><input id="tShow" type="checkbox"> Show traffic</label>
    <label><input id="tTop" type="checkbox" checked> Draw on top</label>
    <label><input id="tEdit" type="checkbox"> Edit traffic</label>
    <div class="row"><button id="tReverse">Reverse</button><button id="tDelete">Delete</button><button id="tRestore">Restore</button></div>
    <div class="row"><button id="tLeft">◀ Shift</button><button id="tRight">Shift ▶</button><button id="tConnect">Connect A→B</button></div>
    <div class="row"><input id="tSpeed" type="number" min="5" max="250" step="5" placeholder="km/h" style="width:70px"><button id="tSetSpeed">Set speed</button></div>
    <label><input id="tHeat" type="checkbox"> Colour by traffic (grey none, blue quiet … red busy)</label>
    <div class="row"><input id="tBusy" type="number" min="0" max="15" step="1" placeholder="0-9" style="width:70px"><button id="tSetBusy">Set traffic</button><button id="tArea">Select area</button></div>
    <div class="row"><button id="tDraw">Draw lane</button><button id="tFinish">Finish lane</button></div>
    <div class="row"><button id="tAddPark">Add parked car</button><button id="tMovePark">Move car</button></div>
    <div class="row"><button id="tRotL">↺ 15°</button><button id="tRotR">↻ 15°</button></div>
    <div class="row"><button id="tUndo">Undo</button><button id="tClearSel">Clear selection</button></div>
    <div class="row"><button id="tExport">Export edits</button><button id="tReset">Reset all</button></div>
    <div id="tStatus" style="color:var(--muted);font-size:11px;margin-top:5px;line-height:1.35"></div>`;
  const $ = (id) => panel.querySelector('#' + id);

  let data = null;            // { lanes, parked } from traffic.json
  let laneById = new Map();
  let parkedById = new Map();
  let edits = emptyEdits();
  let undo = [];
  let mode = 'select';        // select | draw | addPark | movePark | area
  let areaStart = null;       // first corner of an area selection
  let draft = [];             // points of the lane being drawn
  const selection = [];       // ordered keys: lane:<id>, new:<i>, conn:<i>, park:<id>, np:<i>

  try {
    const saved = JSON.parse(localStorage.getItem(STORAGE_KEY) || 'null');
    if (saved && saved.format === 'paradise-traffic-edits-v1') edits = Object.assign(emptyEdits(), saved);
  } catch (_) {}

  const group = new THREE.Group();
  group.name = 'traffic';
  group.visible = false;
  scene.add(group);

  function status(msg) {
    const n = Object.keys(edits.lanes).length + edits.connections.length + edits.newLanes.length + Object.keys(edits.parked).length + edits.newParked.length;
    const modeText = { select: '', draw: ` · drawing a lane (${draft.length} points; click the ground, then Finish)`, addPark: ' · click the ground to place a parked car', movePark: ' · click the ground to move the selected car', area: areaStart ? ' · click the opposite corner of the area (Shift: add to selection)' : ' · click one corner of the area' }[mode];
    $('tStatus').textContent = (msg ? msg + ' ' : '') + `${selection.length} selected · ${n} edits saved in this browser${modeText}`;
  }

  function save() {
    try { localStorage.setItem(STORAGE_KEY, JSON.stringify(edits)); } catch (e) { status(`Could not save edits in this browser: ${e.message}`); }
  }

  function change(fn) {
    undo.push(JSON.stringify(edits));
    if (undo.length > 200) undo.shift();
    fn();
    save();
    redraw();
  }

  // --- geometry with edits applied (mirrors bpx) ---
  function lanePoints(lane) {
    const e = edits.lanes[lane.id] || {};
    const n = lane.pts.length / 3;
    let pts = [];
    for (let k = 0; k < n; k++) pts.push([lane.pts[3 * k], lane.pts[3 * k + 1], lane.pts[3 * k + 2]]);
    if (e.reverse) pts.reverse();
    if (e.shift) {
      const src = pts.map((p) => p.slice());
      pts = src.map((p, k) => {
        const [a, b] = k + 1 < n ? [src[k], src[k + 1]] : k > 0 ? [src[k - 1], src[k]] : [src[k], src[k]];
        const [rx, rz] = rightOf(b[0] - a[0], b[2] - a[2]);
        return [p[0] + rx * e.shift, p[1], p[2] + rz * e.shift];
      });
    }
    return pts;
  }
  function parkedPose(car) {
    const e = edits.parked[car.id] || {};
    return { pos: e.pos || car.pos, dir: e.dir || car.dir, deleted: !!e.delete };
  }
  const laneTraffic = (lane) => (edits.lanes[lane.id] || {}).traffic ?? lane.traffic ?? DRAWN_TRAFFIC;
  function trafficColour(v) {
    if (!v) return 0x6a6a6a;
    const t = Math.min(v, MAX_TRAFFIC) / MAX_TRAFFIC;
    return new THREE.Color().setHSL(0.66 * (1 - t), 1, 0.5).getHex();
  }
  function connectionPoints(c) {
    const a = laneById.get(c.from), b = laneById.get(c.to);
    if (!a || !b) return null;
    const pa = lanePoints(a), pb = lanePoints(b);
    return [pa[pa.length - 1], pb[0]];
  }

  // --- drawing ---
  function addPolyline(pos, col, pts, colour, arrows) {
    const c = new THREE.Color(colour);
    const push = (p) => { pos.push(p[0], p[1] + LIFT, p[2]); col.push(c.r, c.g, c.b); };
    for (let k = 0; k + 1 < pts.length; k++) { push(pts[k]); push(pts[k + 1]); }
    if (!arrows) return;
    let run = CHEVRON_EVERY * 0.5;
    for (let k = 0; k + 1 < pts.length; k++) {
      const a = pts[k], b = pts[k + 1];
      const dx = b[0] - a[0], dz = b[2] - a[2], len = Math.hypot(dx, dz);
      if (len < 1e-3) continue;
      const ux = dx / len, uz = dz / len;
      while (run <= len) {
        const t = run / len;
        const tip = [a[0] + dx * t, a[1] + (b[1] - a[1]) * t, a[2] + dz * t];
        for (const side of [-1, 1]) {
          push(tip);
          push([tip[0] - ux * 1.6 - uz * side * 1.0, tip[1], tip[2] - uz * 1.6 + ux * side * 1.0]);
        }
        run += CHEVRON_EVERY;
      }
      run -= len;
    }
  }
  function addCar(pos, col, pose, colour) {
    const [x, y, z] = pose.pos, [fx, fz] = pose.dir;
    const l = Math.hypot(fx, fz) || 1, ux = fx / l, uz = fz / l;
    const corner = (a, b) => [x + ux * a - uz * b, y, z + uz * a + ux * b];
    const box = [corner(2.3, 1), corner(2.3, -1), corner(-2.3, -1), corner(-2.3, 1), corner(2.3, 1)];
    addPolyline(pos, col, box, colour, false);
    addPolyline(pos, col, [[x, y, z], corner(3.2, 0)], colour, false);
  }

  function redraw() {
    for (const o of [...group.children]) { group.remove(o); o.geometry.dispose(); o.material.dispose(); }
    if (!data) return;
    const pos = [], col = [];
    const sel = new Set(selection);
    for (const lane of data.lanes) {
      const e = edits.lanes[lane.id] || {};
      const heat = $('tHeat').checked;
      const colour = sel.has('lane:' + lane.id) ? COLOURS.selected : e.delete ? COLOURS.deleted : heat ? trafficColour(laneTraffic(lane)) : e.reverse ? COLOURS.reversed : e.shift || e.speed || e.traffic != null ? COLOURS.shifted : COLOURS.lane;
      addPolyline(pos, col, lanePoints(lane), colour, !e.delete);
    }
    edits.connections.forEach((c, i) => {
      const pts = connectionPoints(c);
      if (pts) addPolyline(pos, col, pts, sel.has('conn:' + i) ? COLOURS.selected : COLOURS.connection, true);
    });
    edits.newLanes.forEach((l, i) => addPolyline(pos, col, l.points, sel.has('new:' + i) ? COLOURS.selected : $('tHeat').checked ? trafficColour(l.traffic ?? DRAWN_TRAFFIC) : COLOURS.drawn, true));
    if (draft.length) addPolyline(pos, col, draft.length > 1 ? draft : [draft[0], draft[0]], COLOURS.draft, true);
    for (const car of data.parked) {
      const p = parkedPose(car);
      addCar(pos, col, p, sel.has('park:' + car.id) ? COLOURS.selected : p.deleted ? COLOURS.parkedDeleted : COLOURS.parked);
    }
    edits.newParked.forEach((car, i) => addCar(pos, col, car, sel.has('np:' + i) ? COLOURS.selected : COLOURS.parkedNew));
    const g = new THREE.BufferGeometry();
    g.setAttribute('position', new THREE.Float32BufferAttribute(pos, 3));
    g.setAttribute('color', new THREE.Float32BufferAttribute(col, 3));
    const m = new THREE.LineBasicMaterial({ vertexColors: true, depthTest: !$('tTop').checked, transparent: true });
    const lines = new THREE.LineSegments(g, m);
    lines.renderOrder = 10;
    group.add(lines);
    status();
  }

  // --- picking (screen space) ---
  function toScreen(p) {
    const v = new THREE.Vector3(p[0], p[1] + LIFT, p[2]).project(camera);
    if (v.z > 1) return null;
    const r = renderer.domElement.getBoundingClientRect();
    return [(v.x + 1) / 2 * r.width + r.left, (1 - v.y) / 2 * r.height + r.top];
  }
  function segDist(m, a, b) {
    const dx = b[0] - a[0], dy = b[1] - a[1], l2 = dx * dx + dy * dy;
    const t = l2 ? Math.max(0, Math.min(1, ((m[0] - a[0]) * dx + (m[1] - a[1]) * dy) / l2)) : 0;
    return Math.hypot(m[0] - a[0] - dx * t, m[1] - a[1] - dy * t);
  }
  function polyDist(m, pts) {
    let best = Infinity, prev = null;
    for (const p of pts) {
      const s = toScreen(p);
      if (s && prev) best = Math.min(best, segDist(m, prev, s));
      prev = s;
    }
    return best;
  }
  function pick(e) {
    const m = [e.clientX, e.clientY];
    let best = [PICK_PX, null];
    const consider = (key, d) => { if (d < best[0]) best = [d, key]; };
    for (const car of data.parked) { const s = toScreen(parkedPose(car).pos); if (s) consider('park:' + car.id, Math.hypot(m[0] - s[0], m[1] - s[1]) * 0.8); }
    edits.newParked.forEach((car, i) => { const s = toScreen(car.pos); if (s) consider('np:' + i, Math.hypot(m[0] - s[0], m[1] - s[1]) * 0.8); });
    edits.newLanes.forEach((l, i) => consider('new:' + i, polyDist(m, l.points)));
    edits.connections.forEach((c, i) => { const pts = connectionPoints(c); if (pts) consider('conn:' + i, polyDist(m, pts)); });
    for (const lane of data.lanes) consider('lane:' + lane.id, polyDist(m, lanePoints(lane)));
    return best[1];
  }

  function describe(key) {
    const [kind, id] = [key.slice(0, key.indexOf(':')), key.slice(key.indexOf(':') + 1)];
    if (kind === 'lane') {
      const lane = laneById.get(id), e = edits.lanes[id] || {};
      const pts = lanePoints(lane);
      let len = 0;
      for (let k = 0; k + 1 < pts.length; k++) len += Math.hypot(pts[k + 1][0] - pts[k][0], pts[k + 1][2] - pts[k][2]);
      const speed = e.speed ?? lane.speed;
      const flags = ['delete', 'reverse'].filter((f) => e[f]).concat(e.shift ? [`shift ${e.shift > 0 ? '+' : ''}${e.shift} m`] : []);
      return `lane ${id} · ${Math.round(speed * 3.6)} km/h · traffic ${laneTraffic(lane)}${e.traffic != null ? ` (Burnout ${lane.traffic})` : ''} · ${len.toFixed(0)} m · width ${lane.width} m · leads to ${lane.next.join(', ') || '—'}` + (flags.length ? ` · edited: ${flags.join(', ')}` : '');
    }
    if (kind === 'park') { const p = parkedPose(parkedById.get(id)); return `parked car ${id} at ${p.pos.map((v) => v.toFixed(1)).join(', ')}` + (p.deleted ? ' · deleted' : ''); }
    if (kind === 'np') return `new parked car ${id}`;
    if (kind === 'new') return `drawn lane ${id} · ${edits.newLanes[+id].points.length} points · traffic ${edits.newLanes[+id].traffic ?? DRAWN_TRAFFIC}`;
    if (kind === 'conn') { const c = edits.connections[+id]; return `connection ${c.from} → ${c.to}`; }
    return key;
  }

  function showInfo() {
    if (!selection.length) { info.style.display = 'none'; return; }
    info.textContent = selection.slice(-6).map(describe).join('\n') + (selection.length > 6 ? `\n… and ${selection.length - 6} more` : '');
    info.style.display = 'block';
  }

  // --- click handling (called by the viewer for clicks, not drags) ---
  function handleClick(e) {
    if (!$('tEdit').checked || !data) return false;
    if (mode === 'area') {
      const p = pickGround(e);
      if (!p) { status('No ground there: load the units (visual or collision) under the spot first.'); return true; }
      if (!areaStart) { areaStart = [p.x, p.y, p.z]; status(); return true; }
      const [x0, x1] = [Math.min(areaStart[0], p.x), Math.max(areaStart[0], p.x)];
      const [z0, z1] = [Math.min(areaStart[2], p.z), Math.max(areaStart[2], p.z)];
      const inside = (pts) => pts.some((q) => q[0] >= x0 && q[0] <= x1 && q[2] >= z0 && q[2] <= z1);
      if (!e.shiftKey && !e.ctrlKey) selection.length = 0;
      const add = (key) => { if (!selection.includes(key)) selection.push(key); };
      for (const lane of data.lanes) if (!(edits.lanes[lane.id] || {}).delete && inside(lanePoints(lane))) add('lane:' + lane.id);
      edits.newLanes.forEach((l, i) => { if (inside(l.points)) add('new:' + i); });
      mode = 'select';
      areaStart = null;
      redraw();
      showInfo();
      status(`Selected ${selection.length} lanes in the area.`);
      return true;
    }
    if (mode === 'draw' || mode === 'addPark' || mode === 'movePark') {
      const p = pickGround(e);
      if (!p) { status('No ground there: load the units (visual or collision) under the spot first.'); return true; }
      const pt = [+p.x.toFixed(2), +p.y.toFixed(2), +p.z.toFixed(2)];
      if (mode === 'draw') { draft.push(pt); redraw(); return true; }
      if (mode === 'addPark') {
        change(() => edits.newParked.push({ pos: pt, dir: nearestLaneDir(pt) }));
        mode = 'select';
        return true;
      }
      const target = selection.find((k) => k.startsWith('park:') || k.startsWith('np:'));
      if (!target) { mode = 'select'; status('Select a parked car first.'); return true; }
      change(() => {
        if (target.startsWith('np:')) edits.newParked[+target.slice(3)].pos = pt;
        else edits.parked[target.slice(5)] = Object.assign(edits.parked[target.slice(5)] || {}, { pos: pt });
      });
      mode = 'select';
      return true;
    }
    const key = pick(e);
    if (!e.shiftKey && !e.ctrlKey) selection.length = 0;
    if (key) {
      const i = selection.indexOf(key);
      if (i >= 0) selection.splice(i, 1); else selection.push(key);
    }
    redraw();
    showInfo();
    return true;
  }

  function nearestLaneDir(pt) {
    let best = [Infinity, [1, 0]];
    for (const lane of data.lanes) {
      const pts = lanePoints(lane);
      for (let k = 0; k + 1 < pts.length; k++) {
        const d = Math.hypot(pts[k][0] - pt[0], pts[k][2] - pt[2]);
        if (d < best[0]) best = [d, [pts[k + 1][0] - pts[k][0], pts[k + 1][2] - pts[k][2]]];
      }
    }
    const [dx, dz] = best[1], l = Math.hypot(dx, dz) || 1;
    return [+(dx / l).toFixed(3), +(dz / l).toFixed(3)];
  }

  // --- actions ---
  const selected = (kind) => selection.filter((k) => k.startsWith(kind + ':')).map((k) => k.slice(kind.length + 1));
  const laneEdit = (id) => (edits.lanes[id] = edits.lanes[id] || {});
  function tidy(id) { const e = edits.lanes[id]; if (e && !e.delete && !e.reverse && !e.shift && e.speed == null && e.traffic == null) delete edits.lanes[id]; }

  $('tShow').onchange = async () => {
    group.visible = $('tShow').checked;
    if (group.visible && !data) {
      status('Loading traffic…');
      try {
        data = await (await fetch('../../out/traffic/traffic.json')).json();
      } catch (err) {
        status('Could not load out/traffic/traffic.json (run: bpx traffic-json <game-dir> out/traffic/traffic.json).');
        return;
      }
      laneById = new Map(data.lanes.map((l) => [l.id, l]));
      parkedById = new Map(data.parked.map((c) => [c.id, c]));
      // With nothing edited in this browser yet, start from the edits saved
      // in the project (tools/edits/traffic_edits.json, applied in game).
      const empty = JSON.stringify(edits) === JSON.stringify(emptyEdits());
      if (empty) {
        try {
          const r = await fetch('../edits/traffic_edits.json', { cache: 'no-store' });
          if (r.ok) { edits = Object.assign(emptyEdits(), await r.json()); save(); }
        } catch (_) {}
      }
    }
    redraw();
  };
  $('tTop').onchange = redraw;
  $('tEdit').onchange = () => {
    if ($('tEdit').checked && !$('tShow').checked) { $('tShow').checked = true; $('tShow').onchange(); }
    if (!$('tEdit').checked) { mode = 'select'; draft = []; selection.length = 0; redraw(); showInfo(); }
    status($('tEdit').checked ? 'Click a lane or parked car to select it; Shift+click adds to the selection.' : 'Traffic editing off.');
  };
  $('tReverse').onclick = () => change(() => { for (const id of selected('lane')) { const e = laneEdit(id); e.reverse = !e.reverse; tidy(id); } });
  $('tDelete').onclick = () => change(() => {
    for (const id of selected('lane')) laneEdit(id).delete = true;
    for (const id of selected('park')) edits.parked[id] = Object.assign(edits.parked[id] || {}, { delete: true });
    const drop = (list, kind) => { const idx = new Set(selected(kind).map(Number)); return list.filter((_, i) => !idx.has(i)); };
    edits.newLanes = drop(edits.newLanes, 'new');
    edits.connections = drop(edits.connections, 'conn');
    edits.newParked = drop(edits.newParked, 'np');
    selection.length = 0;
  });
  $('tRestore').onclick = () => change(() => {
    for (const id of selected('lane')) delete edits.lanes[id];
    for (const id of selected('park')) delete edits.parked[id];
  });
  const shift = (d) => change(() => { for (const id of selected('lane')) { const e = laneEdit(id); e.shift = Math.round(((e.shift || 0) + d) * 100) / 100; if (!e.shift) delete e.shift; tidy(id); } });
  $('tLeft').onclick = () => shift(-SHIFT_STEP);
  $('tRight').onclick = () => shift(SHIFT_STEP);
  $('tConnect').onclick = () => {
    const lanes = selection.filter((k) => k.startsWith('lane:')).map((k) => k.slice(5));
    if (lanes.length !== 2) { status('Select exactly two lanes: first the one to leave, then (Shift+click) the one to join.'); return; }
    change(() => edits.connections.push({ from: lanes[0], to: lanes[1] }));
  };
  $('tSetSpeed').onclick = () => {
    const kmh = +$('tSpeed').value;
    if (!(kmh > 0)) { status('Enter a speed in km/h first.'); return; }
    change(() => {
      for (const id of selected('lane')) laneEdit(id).speed = Math.round(kmh / 3.6 * 10) / 10;
      for (const i of selected('new')) edits.newLanes[+i].speed = Math.round(kmh / 3.6 * 10) / 10;
    });
  };
  $('tHeat').onchange = redraw;
  $('tSetBusy').onclick = () => {
    const raw = $('tBusy').value;
    const v = Math.round(+raw);
    if (raw === '' || !(v >= 0 && v <= 15)) { status("Enter a traffic level first: 0 (no cars start there) to 9 (Burnout's busiest)."); return; }
    if (!selection.some((k) => k.startsWith('lane:') || k.startsWith('new:'))) { status('Select lanes first (click, Shift+click, or Select area).'); return; }
    change(() => {
      for (const id of selected('lane')) {
        const lane = laneById.get(id);
        if (lane && lane.traffic === v) { delete laneEdit(id).traffic; tidy(id); } else laneEdit(id).traffic = v;
      }
      for (const i of selected('new')) edits.newLanes[+i].traffic = v;
    });
  };
  $('tArea').onclick = () => { if (!$('tEdit').checked) { status('Tick “Edit traffic” first.'); return; } mode = 'area'; areaStart = null; status(); };
  $('tDraw').onclick = () => { if (!$('tEdit').checked) return; mode = 'draw'; draft = []; selection.length = 0; redraw(); };
  const finish = () => {
    if (mode !== 'draw') return;
    const pts = draft;
    mode = 'select';
    draft = [];
    if (pts.length < 2) { redraw(); status('A lane needs at least two points.'); return; }
    change(() => edits.newLanes.push({ points: pts, width: 4.5, speed: 13.4, traffic: DRAWN_TRAFFIC }));
  };
  $('tFinish').onclick = finish;
  $('tAddPark').onclick = () => { if (!$('tEdit').checked) return; mode = 'addPark'; status(); };
  $('tMovePark').onclick = () => {
    if (!selection.some((k) => k.startsWith('park:') || k.startsWith('np:'))) { status('Select a parked car first.'); return; }
    mode = 'movePark';
    status();
  };
  const rotate = (deg) => change(() => {
    const r = deg * Math.PI / 180;
    const turn = ([x, z]) => [+(x * Math.cos(r) - z * Math.sin(r)).toFixed(3), +(x * Math.sin(r) + z * Math.cos(r)).toFixed(3)];
    for (const id of selected('park')) edits.parked[id] = Object.assign(edits.parked[id] || {}, { dir: turn(parkedPose(parkedById.get(id)).dir) });
    for (const i of selected('np')) edits.newParked[+i].dir = turn(edits.newParked[+i].dir);
  });
  $('tRotL').onclick = () => rotate(15);
  $('tRotR').onclick = () => rotate(-15);
  $('tUndo').onclick = () => {
    const prev = undo.pop();
    if (!prev) { status('Nothing to undo.'); return; }
    edits = JSON.parse(prev);
    selection.length = 0;
    save();
    redraw();
    showInfo();
  };
  $('tClearSel').onclick = () => { selection.length = 0; redraw(); showInfo(); };
  $('tReset').onclick = () => { if (confirm('Remove all traffic edits saved in this browser?')) change(() => { edits = emptyEdits(); selection.length = 0; }); };
  $('tExport').onclick = () => {
    const blob = new Blob([JSON.stringify(edits, null, 1)], { type: 'application/json' });
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = 'paradise-traffic-edits.json';
    a.click();
    setTimeout(() => URL.revokeObjectURL(a.href), 0);
    status('Exported. Save it as tools/edits/traffic_edits.json, then run tools/rebuild_traffic.sh.');
  };
  addEventListener('keydown', (e) => {
    if (!$('tEdit').checked || e.target.tagName === 'INPUT') return;
    if (e.key === 'Enter') finish();
    if (e.key === 'Escape') { mode = 'select'; draft = []; areaStart = null; redraw(); }
    if (e.key === 'Backspace' && mode === 'draw') { draft.pop(); redraw(); }
    if ((e.key === 'Delete') && selection.length) $('tDelete').onclick();
    if (e.key === 'r' || e.key === 'R') $('tReverse').onclick();
  });

  status('Tick “Show traffic” to load the lanes.');
  return { handleClick, group };
}
