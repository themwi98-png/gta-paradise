// Water editor for the Paradise viewer: shows the water GTA gets
// (out/water/water_view.json, written by `bpx gta-water`) and lets areas be
// deleted cell by cell. Exported as paradise-water-edits-v1 JSON and saved
// as tools/edits/water_edits.json, the deleted areas are left dry the next
// time the water is built (tools/rebuild_water.sh).
//
// Water and edits are in GTA world coordinates (z up); the viewer is in
// Burnout coordinates (y up), converted with the build's placement.

const STORAGE_KEY = 'paradise-water-edits-v1';
const COLOURS = { sea: 0x1e78ff, lake: 0x19d3c5, deleted: 0xff2a2a, hover: 0xffffff };

const emptyEdits = () => ({ format: 'paradise-water-edits-v1', deleted: [] });

export function initWater({ THREE, scene, camera, panel, pickGround }) {
  panel.innerHTML = `
    <label><input id="wShow" type="checkbox"> Show water</label>
    <label><input id="wThrough" type="checkbox"> See through ground</label>
    <label><input id="wEdit" type="checkbox"> Edit water</label>
    <div class="row"><label style="margin:0">Brush <select id="wBrush"><option value="1">1 cell (16 m)</option><option value="3">3×3</option><option value="5">5×5</option><option value="9">9×9</option></select></label></div>
    <div class="row"><label style="margin:0"><input type="radio" name="wMode" value="delete" checked> Delete</label><label style="margin:0"><input type="radio" name="wMode" value="restore"> Restore</label></div>
    <div class="row"><button id="wUndo">Undo</button><button id="wExport">Export edits</button><button id="wReset">Reset all</button></div>
    <div id="wStatus" style="color:var(--muted);font-size:11px;margin-top:5px;line-height:1.35"></div>`;
  const $ = (id) => panel.querySelector('#' + id);

  let view = null;            // water_view.json
  let edits = emptyEdits();
  let fromFile = false;
  const undo = [];

  try {
    const saved = JSON.parse(localStorage.getItem(STORAGE_KEY) || 'null');
    if (saved && saved.format === 'paradise-water-edits-v1') edits = Object.assign(emptyEdits(), saved);
  } catch (_) {}

  const group = new THREE.Group();
  group.name = 'water';
  group.visible = false;
  scene.add(group);
  let waterMesh = null;
  let deletedMesh = null;

  // GTA world <-> viewer (Burnout) coordinates.
  function toViewer(x, y, z) {
    const p = view.placement;
    const s = p.scale || 1, h = (p.heading || 0) * Math.PI / 180;
    const dx = x - p.offset[0], dy = y - p.offset[1];
    const rx = (dx * Math.cos(h) + dy * Math.sin(h)) / s;
    const ry = (-dx * Math.sin(h) + dy * Math.cos(h)) / s;
    return [rx, (z - p.offset[2]) / s, -ry];
  }
  function toGta(v) {
    const p = view.placement;
    const s = p.scale || 1, h = (p.heading || 0) * Math.PI / 180;
    const x = v.x * s, y = -v.z * s;
    return [x * Math.cos(h) - y * Math.sin(h) + p.offset[0], x * Math.sin(h) + y * Math.cos(h) + p.offset[1], v.y * s + p.offset[2]];
  }

  function status(msg) {
    const n = edits.deleted.length;
    $('wStatus').textContent = (msg ? msg + ' ' : '') +
      (view ? `${view.quads.length} water areas from the last build (${view.removedCells || 0} cells already removed). ` : '') +
      `${n} deleted cell${n === 1 ? '' : 's'} saved in this browser${fromFile ? ' (started from tools/edits/water_edits.json)' : ''}.` +
      ($('wEdit').checked ? ' Click water or ground to ' + mode() + '.' : '');
  }
  const mode = () => panel.querySelector('input[name="wMode"]:checked').value;

  function save() {
    try { localStorage.setItem(STORAGE_KEY, JSON.stringify(edits)); } catch (e) { status(`Could not save in this browser: ${e.message}`); }
  }

  function material(colour, opacity) {
    const m = { transparent: true, opacity, side: THREE.DoubleSide, depthWrite: false, depthTest: !$('wThrough').checked };
    if (colour === undefined) m.vertexColors = true; else m.color = colour;
    return new THREE.MeshBasicMaterial(m);
  }

  // Quads as two triangles each, lifted a little so they don't fight the ground.
  function quadsGeometry(rects, lift) {
    const pos = new Float32Array(rects.length * 18);
    const col = new Float32Array(rects.length * 18);
    rects.forEach((r, i) => {
      const [x0, y0, x1, y1, z] = r;
      const c = [toViewer(x0, y0, z + lift), toViewer(x1, y0, z + lift), toViewer(x1, y1, z + lift), toViewer(x0, y1, z + lift)];
      const tri = [c[0], c[1], c[2], c[0], c[2], c[3]];
      const colour = new THREE.Color(r[5] === 1 ? COLOURS.lake : COLOURS.sea);
      tri.forEach((p, k) => {
        pos.set(p, i * 18 + k * 3);
        col.set([colour.r, colour.g, colour.b], i * 18 + k * 3);
      });
    });
    const g = new THREE.BufferGeometry();
    g.setAttribute('position', new THREE.BufferAttribute(pos, 3));
    g.setAttribute('color', new THREE.BufferAttribute(col, 3));
    return g;
  }

  function redraw() {
    if (!view) return;
    if (waterMesh) { group.remove(waterMesh); waterMesh.geometry.dispose(); waterMesh.material.dispose(); }
    if (deletedMesh) { group.remove(deletedMesh); deletedMesh.geometry.dispose(); deletedMesh.material.dispose(); }
    waterMesh = new THREE.Mesh(quadsGeometry(view.quads, 0.05), material(undefined, 0.45));
    waterMesh.renderOrder = 2;
    group.add(waterMesh);
    const del = edits.deleted.map((r) => [r[0], r[1], r[2], r[3], r[4] ?? view.seaZ, 2]);
    if (del.length) {
      deletedMesh = new THREE.Mesh(quadsGeometry(del, 0.15), material(COLOURS.deleted, 0.6));
      deletedMesh.renderOrder = 3;
      group.add(deletedMesh);
    } else {
      deletedMesh = null;
    }
    status();
  }

  // The water surface height at a GTA point (highest quad there), if any.
  function waterZ(x, y) {
    let z = null;
    for (const q of view.quads) {
      if (x >= q[0] && x <= q[2] && y >= q[1] && y <= q[3] && (z === null || q[4] > z)) z = q[4];
    }
    return z;
  }

  const ray = new THREE.Raycaster();
  function pick(e) {
    ray.setFromCamera(new THREE.Vector2(e.clientX / innerWidth * 2 - 1, -(e.clientY / innerHeight) * 2 + 1), camera);
    const hits = [];
    if (waterMesh) hits.push(...ray.intersectObject(waterMesh));
    if (deletedMesh) hits.push(...ray.intersectObject(deletedMesh));
    const ground = pickGround(e);
    if (ground) hits.push({ distance: ground.distanceTo(ray.ray.origin), point: ground });
    hits.sort((a, b) => a.distance - b.distance);
    return hits.length ? hits[0].point : null;
  }

  function handleClick(e) {
    if (!$('wEdit').checked || !view) return false;
    const p = pick(e);
    if (!p) { status('Nothing under the cursor.'); return true; }
    const [gx, gy] = toGta(p);
    const cell = view.cell, [ox, oy] = view.origin;
    const cx = Math.floor((gx - ox) / cell), cy = Math.floor((gy - oy) / cell);
    const half = (Number($('wBrush').value) - 1) / 2;
    undo.push(JSON.stringify(edits));
    if (undo.length > 200) undo.shift();
    let changed = 0;
    for (let dy = -half; dy <= half; dy++) {
      for (let dx = -half; dx <= half; dx++) {
        const x0 = ox + (cx + dx) * cell, y0 = oy + (cy + dy) * cell;
        const mx = x0 + cell / 2, my = y0 + cell / 2;
        const inside = (r) => mx >= r[0] && mx <= r[2] && my >= r[1] && my <= r[3];
        if (mode() === 'delete') {
          if (edits.deleted.some(inside)) continue;
          const z = waterZ(mx, my);
          edits.deleted.push([x0, y0, x0 + cell, y0 + cell, z ?? view.seaZ]);
          changed++;
        } else {
          const before = edits.deleted.length;
          edits.deleted = edits.deleted.filter((r) => !inside(r));
          changed += before - edits.deleted.length;
        }
      }
    }
    if (!changed) undo.pop();
    save();
    redraw();
    status(`${mode() === 'delete' ? 'Deleted' : 'Restored'} ${changed} cell${changed === 1 ? '' : 's'} at GTA (${gx.toFixed(0)}, ${gy.toFixed(0)}).`);
    return true;
  }

  async function load() {
    if (view) return;
    try {
      view = await (await fetch('../../out/water/water_view.json', { cache: 'no-store' })).json();
    } catch (_) {
      status('Could not load out/water/water_view.json (run tools/rebuild_water.sh, or bpx gta-water).');
      return;
    }
    if (!edits.deleted.length) {
      try {
        const r = await fetch('../edits/water_edits.json', { cache: 'no-store' });
        if (r.ok) {
          const doc = await r.json();
          if (doc && Array.isArray(doc.deleted)) { edits = Object.assign(emptyEdits(), doc); fromFile = true; save(); }
        }
      } catch (_) {}
    }
    redraw();
  }

  $('wShow').addEventListener('change', async () => {
    group.visible = $('wShow').checked;
    if (group.visible) await load();
  });
  $('wEdit').addEventListener('change', async () => {
    if ($('wEdit').checked) { $('wShow').checked = true; group.visible = true; await load(); }
    status();
  });
  $('wThrough').addEventListener('change', redraw);
  panel.querySelectorAll('input[name="wMode"]').forEach((r) => r.addEventListener('change', () => status()));
  $('wUndo').addEventListener('click', () => {
    if (!undo.length) return status('Nothing to undo.');
    edits = JSON.parse(undo.pop());
    save();
    redraw();
  });
  $('wReset').addEventListener('click', () => {
    if (!confirm('Remove every water deletion saved in this browser?')) return;
    undo.push(JSON.stringify(edits));
    edits = emptyEdits();
    save();
    redraw();
  });
  $('wExport').addEventListener('click', () => {
    const blob = new Blob([JSON.stringify(edits, null, 1)], { type: 'application/json' });
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = 'water_edits.json';
    a.click();
    URL.revokeObjectURL(a.href);
    status('Exported. Save it as tools/edits/water_edits.json, then run tools/rebuild_water.sh.');
  });
  status();

  return { handleClick, group };
}
