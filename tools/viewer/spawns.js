// Spawn editor for the Paradise viewer: no-spawn zones (circles) where GTA
// must not spawn NPCs or vehicles — Los Santos leftovers whose spawn spots
// land inside Paradise's buildings, tunnels or sky. Exported as
// paradise-spawn-blocks-v1 JSON and saved as tools/edits/spawn_blocks.json;
// tools/install_dlc.sh copies it to the game, where paradise.asi switches
// GTA's parked-car generators and scenario spots off inside the zones and
// removes anything else that appears there.
//
// Also shows (after tools/sync_from_game.sh) the F9 marks and the spawns the
// plugin removed (out/marks/paradise_marks.csv, out/marks/paradise_spawns.csv).
//
// Zones are in GTA world coordinates (z up); the viewer is in Burnout
// coordinates (y up), converted with the build's placement.

const STORAGE_KEY = 'paradise-spawn-blocks-v1';
const COLOURS = { zone: 0xff3b6b, mark: 0xffd400, removed: 0x7fd0ff };
const ZONE_HEIGHT = 12;

const emptyEdits = () => ({ format: 'paradise-spawn-blocks-v1', zones: [] });

export function initSpawns({ THREE, scene, camera, panel, pickGround }) {
  panel.innerHTML = `
    <label><input id="sShow" type="checkbox"> Show no-spawn zones</label>
    <label><input id="sEdit" type="checkbox"> Edit zones</label>
    <div class="row"><label style="margin:0">Radius <input id="sRadius" type="number" min="3" max="200" step="1" value="15" style="width:60px"> m</label></div>
    <label><input id="sMarks" type="checkbox" checked> Show F9 marks (yellow)</label>
    <label><input id="sRemoved" type="checkbox" checked> Show removed spawns (blue)</label>
    <div class="row"><button id="sUndo">Undo</button><button id="sExport">Export zones</button><button id="sReset">Reset all</button></div>
    <div id="sStatus" style="color:var(--muted);font-size:11px;margin-top:5px;line-height:1.35"></div>`;
  const $ = (id) => panel.querySelector('#' + id);

  let placement = { offset: [0, 0, 9.9], scale: 1, heading: 0 };
  let edits = emptyEdits();
  let fromFile = false;
  let marks = [];      // [{n, x, y, z}]
  let removed = [];    // [{kind, x, y, z, why}]
  let loaded = false;
  const undo = [];

  try {
    const saved = JSON.parse(localStorage.getItem(STORAGE_KEY) || 'null');
    if (saved && saved.format === 'paradise-spawn-blocks-v1') edits = Object.assign(emptyEdits(), saved);
  } catch (_) {}

  const group = new THREE.Group();
  group.name = 'spawns';
  group.visible = false;
  scene.add(group);
  // Marks and removed spawns have their own layer, shown by their own
  // checkboxes whether or not the zones are.
  const pins = new THREE.Group();
  pins.name = 'marks';
  scene.add(pins);

  // GTA world <-> viewer (Burnout) coordinates.
  function toViewer(x, y, z) {
    const p = placement;
    const s = p.scale || 1, h = (p.heading || 0) * Math.PI / 180;
    const dx = x - p.offset[0], dy = y - p.offset[1];
    const rx = (dx * Math.cos(h) + dy * Math.sin(h)) / s;
    const ry = (-dx * Math.sin(h) + dy * Math.cos(h)) / s;
    return new THREE.Vector3(rx, (z - p.offset[2]) / s, -ry);
  }
  function toGta(v) {
    const p = placement;
    const s = p.scale || 1, h = (p.heading || 0) * Math.PI / 180;
    const x = v.x * s, y = -v.z * s;
    return [x * Math.cos(h) - y * Math.sin(h) + p.offset[0], x * Math.sin(h) + y * Math.cos(h) + p.offset[1], v.y * s + p.offset[2]];
  }

  function status(msg) {
    const n = edits.zones.length;
    $('sStatus').textContent = (msg ? msg + ' ' : '') +
      `${n} no-spawn zone${n === 1 ? '' : 's'} saved in this browser${fromFile ? ' (started from tools/edits/spawn_blocks.json)' : ''}. ` +
      `${marks.length} marks, ${removed.length} removed spawns loaded.` +
      ($('sEdit').checked ? ' Click the map to add a zone; click inside a zone to remove it.' : '');
  }

  function save() {
    try { localStorage.setItem(STORAGE_KEY, JSON.stringify(edits)); } catch (e) { status(`Could not save in this browser: ${e.message}`); }
  }

  // Markers keep the same size on screen at any distance and draw over
  // everything, so they can be found from far away.
  function dot(colour, outline) {
    const c = document.createElement('canvas');
    c.width = c.height = 64;
    const g = c.getContext('2d');
    g.beginPath(); g.arc(32, 32, 26, 0, Math.PI * 2);
    g.fillStyle = '#' + new THREE.Color(colour).getHexString(); g.fill();
    g.lineWidth = 6; g.strokeStyle = outline; g.stroke();
    return new THREE.CanvasTexture(c);
  }
  const markTexture = dot(COLOURS.mark, '#000');
  const removedTexture = dot(COLOURS.removed, '#003050');

  function pin(colour, at, size) {
    const sprite = new THREE.Sprite(new THREE.SpriteMaterial({
      map: colour === COLOURS.mark ? markTexture : removedTexture, depthTest: false, sizeAttenuation: false,
    }));
    sprite.scale.set(size, size, 1);
    sprite.position.copy(at);
    sprite.renderOrder = 5;
    sprite.frustumCulled = false;
    return sprite;
  }

  function label(text, at) {
    const c = document.createElement('canvas');
    c.width = 128; c.height = 64;
    const g = c.getContext('2d');
    g.font = 'bold 44px sans-serif';
    g.lineWidth = 8; g.strokeStyle = '#000'; g.strokeText(text, 8, 48);
    g.fillStyle = '#ffd400'; g.fillText(text, 8, 48);
    const sprite = new THREE.Sprite(new THREE.SpriteMaterial({ map: new THREE.CanvasTexture(c), depthTest: false, sizeAttenuation: false }));
    sprite.scale.set(0.08, 0.04, 1);
    sprite.center.set(-0.15, -0.1);
    sprite.position.copy(at);
    sprite.renderOrder = 6;
    sprite.frustumCulled = false;
    return sprite;
  }

  function redraw() {
    for (const layer of [group, pins]) {
      while (layer.children.length) {
        const c = layer.children.pop();
        c.geometry?.dispose();
        if (c.material?.map && c.material.map !== markTexture && c.material.map !== removedTexture) c.material.map.dispose();
        c.material?.dispose();
      }
    }
    for (const [x, y, z, r] of edits.zones) {
      const m = new THREE.Mesh(
        new THREE.CylinderGeometry(r, r, ZONE_HEIGHT, 48, 1, true),
        new THREE.MeshBasicMaterial({ color: COLOURS.zone, transparent: true, opacity: 0.35, side: THREE.DoubleSide, depthWrite: false }));
      m.position.copy(toViewer(x, y, z));
      m.renderOrder = 4;
      group.add(m);
    }
    if ($('sMarks').checked) {
      for (const k of marks) {
        const at = toViewer(k.x, k.y, k.z);
        pins.add(pin(COLOURS.mark, at, 0.035));
        pins.add(label(String(k.n), at));
      }
    }
    if ($('sRemoved').checked) {
      for (const k of removed) pins.add(pin(COLOURS.removed, toViewer(k.x, k.y, k.z), 0.025));
    }
    status();
  }

  function handleClick(e) {
    if (!$('sEdit').checked) return false;
    const p = pickGround(e);
    if (!p) { status('Nothing under the cursor.'); return true; }
    const [gx, gy, gz] = toGta(p);
    undo.push(JSON.stringify(edits));
    if (undo.length > 200) undo.shift();
    const hit = edits.zones.findIndex(([x, y, , r]) => Math.hypot(gx - x, gy - y) <= r);
    if (hit >= 0) {
      edits.zones.splice(hit, 1);
      status(`Removed a zone at GTA (${gx.toFixed(0)}, ${gy.toFixed(0)}).`);
    } else {
      const r = Math.max(3, Number($('sRadius').value) || 15);
      edits.zones.push([+gx.toFixed(1), +gy.toFixed(1), +gz.toFixed(1), r]);
      status(`Added a ${r} m zone at GTA (${gx.toFixed(0)}, ${gy.toFixed(0)}).`);
    }
    save();
    redraw();
    return true;
  }

  async function csv(url) {
    try {
      const r = await fetch(url, { cache: 'no-store' });
      if (!r.ok) return [];
      const [head, ...lines] = (await r.text()).trim().split(/\r?\n/);
      const cols = head.split(',');
      return lines.map((l) => Object.fromEntries(l.split(',').map((v, i) => [cols[i], v])));
    } catch (_) { return []; }
  }

  async function load() {
    if (loaded) return;
    loaded = true;
    try {
      const view = await (await fetch('../../out/water/water_view.json', { cache: 'no-store' })).json();
      if (view && view.placement) placement = view.placement;
    } catch (_) {}
    if (!edits.zones.length) {
      try {
        const r = await fetch('../edits/spawn_blocks.json', { cache: 'no-store' });
        if (r.ok) {
          const doc = await r.json();
          if (doc && Array.isArray(doc.zones)) { edits = Object.assign(emptyEdits(), doc); fromFile = true; save(); }
        }
      } catch (_) {}
    }
    marks = (await csv('../../out/marks/paradise_marks.csv')).map((r) => ({ n: r.mark, x: +r.x, y: +r.y, z: +r.z }));
    removed = (await csv('../../out/marks/paradise_spawns.csv')).map((r) => ({ kind: r.kind, x: +r.x, y: +r.y, z: +r.z, why: r.reason }));
    redraw();
  }

  $('sShow').addEventListener('change', async () => {
    group.visible = $('sShow').checked;
    if (group.visible) await load();
  });
  $('sEdit').addEventListener('change', async () => {
    if ($('sEdit').checked) { $('sShow').checked = true; group.visible = true; await load(); }
    status();
  });
  $('sMarks').addEventListener('change', async () => { await load(); redraw(); });
  $('sRemoved').addEventListener('change', async () => { await load(); redraw(); });
  $('sUndo').addEventListener('click', () => {
    if (!undo.length) return status('Nothing to undo.');
    edits = JSON.parse(undo.pop());
    save();
    redraw();
  });
  $('sReset').addEventListener('click', () => {
    if (!confirm('Remove every no-spawn zone saved in this browser?')) return;
    undo.push(JSON.stringify(edits));
    edits = emptyEdits();
    save();
    redraw();
  });
  $('sExport').addEventListener('click', () => {
    const blob = new Blob([JSON.stringify(edits, null, 1)], { type: 'application/json' });
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = 'spawn_blocks.json';
    a.click();
    URL.revokeObjectURL(a.href);
    status('Exported. Save it as tools/edits/spawn_blocks.json, then run tools/install_dlc.sh (GTA closed).');
  });
  status();
  // Marks show from the start.
  load();

  return { handleClick, group };
}
