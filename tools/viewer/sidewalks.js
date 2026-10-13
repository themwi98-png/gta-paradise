// Sidewalk editor for the Paradise viewer: draw shapes on the map where the
// pedestrian navmesh's sidewalk (GTA "footpath", where NPCs prefer to walk)
// is taken away or added. Burnout's "pavement" and "cobbles" surfaces become
// sidewalk; "Show sidewalk surfaces" shows only those in the collision
// layers so you can see what's there. Exported as paradise-sidewalk-edits-v1
// JSON, saved as tools/edits/sidewalk_edits.json and applied by
// `bpx gta-navmesh` (tools/rebuild_navmesh.sh).
//
// Shapes are in GTA world coordinates (z up); the viewer is in Burnout
// coordinates (y up), converted with the build's placement.

const STORAGE_KEY = 'paradise-sidewalk-edits-v1';
const COLOURS = { remove: 0xff4040, add: 0x30e060, draft: 0xffffff };
/// Collision surface ids that become sidewalk (cobbles, pavement).
const SIDEWALK_SURFACES = [5, 19];
const LIFT = 0.3;

const emptyEdits = () => ({ format: 'paradise-sidewalk-edits-v1', remove: [], add: [] });

export function initSidewalks({ THREE, scene, panel, pickGround, showOnlySurfaces }) {
  panel.innerHTML = `
    <label><input id="wkShow" type="checkbox"> Show sidewalk edits</label>
    <label><input id="wkSurf" type="checkbox"> Show only sidewalk surfaces (load a collision layer)</label>
    <label><input id="wkEdit" type="checkbox"> Edit sidewalks</label>
    <div class="row"><label style="margin:0"><input type="radio" name="wkMode" value="remove" checked> Remove sidewalk</label><label style="margin:0"><input type="radio" name="wkMode" value="add"> Add sidewalk</label></div>
    <div class="row"><button id="wkFinish">Finish shape</button><button id="wkCancel">Cancel shape</button></div>
    <div class="row"><button id="wkUndo">Undo</button><button id="wkExport">Export edits</button><button id="wkReset">Reset all</button></div>
    <div id="wkStatus" style="color:var(--muted);font-size:11px;margin-top:5px;line-height:1.35"></div>`;
  const $ = (id) => panel.querySelector('#' + id);

  let placement = { offset: [0, 0, 9.9], scale: 1, heading: 0 };
  let edits = emptyEdits();
  let fromFile = false;
  let draft = [];      // GTA points of the shape being drawn
  let loaded = false;
  const undo = [];

  try {
    const saved = JSON.parse(localStorage.getItem(STORAGE_KEY) || 'null');
    if (saved && saved.format === 'paradise-sidewalk-edits-v1') edits = Object.assign(emptyEdits(), saved);
  } catch (_) {}

  const group = new THREE.Group();
  group.name = 'sidewalks';
  group.visible = false;
  scene.add(group);

  function toViewer(x, y, z) {
    const p = placement;
    const s = p.scale || 1, h = (p.heading || 0) * Math.PI / 180;
    const dx = x - p.offset[0], dy = y - p.offset[1];
    const rx = (dx * Math.cos(h) + dy * Math.sin(h)) / s;
    const ry = (-dx * Math.sin(h) + dy * Math.cos(h)) / s;
    return new THREE.Vector3(rx, (z - p.offset[2]) / s + LIFT, -ry);
  }
  function toGta(v) {
    const p = placement;
    const s = p.scale || 1, h = (p.heading || 0) * Math.PI / 180;
    const x = v.x * s, y = -v.z * s;
    return [x * Math.cos(h) - y * Math.sin(h) + p.offset[0], x * Math.sin(h) + y * Math.cos(h) + p.offset[1], v.y * s + p.offset[2]];
  }
  const mode = () => panel.querySelector('input[name="wkMode"]:checked').value;

  function status(msg) {
    $('wkStatus').textContent = (msg ? msg + ' ' : '') +
      `${edits.remove.length} removed, ${edits.add.length} added shape(s) saved in this browser${fromFile ? ' (started from tools/edits/sidewalk_edits.json)' : ''}.` +
      ($('wkEdit').checked ? (draft.length ? ` Drawing (${draft.length} points): click to add points, Finish shape to close it.` : ' Click the map to start a shape; click inside a shape to delete it.') : '');
  }

  function save() {
    try { localStorage.setItem(STORAGE_KEY, JSON.stringify(edits)); } catch (e) { status(`Could not save in this browser: ${e.message}`); }
  }

  function shapeMesh(points, colour) {
    const out = new THREE.Group();
    const pts = points.map((p) => toViewer(p[0], p[1], p[2]));
    const line = new THREE.LineLoop(new THREE.BufferGeometry().setFromPoints(pts), new THREE.LineBasicMaterial({ color: colour, depthTest: false }));
    line.renderOrder = 6;
    out.add(line);
    if (points.length >= 3) {
      const tris = THREE.ShapeUtils.triangulateShape(points.map((p) => new THREE.Vector2(p[0], p[1])), []);
      const g = new THREE.BufferGeometry().setFromPoints(pts);
      g.setIndex(tris.flat());
      const fill = new THREE.Mesh(g, new THREE.MeshBasicMaterial({ color: colour, transparent: true, opacity: 0.35, side: THREE.DoubleSide, depthWrite: false }));
      fill.renderOrder = 5;
      out.add(fill);
    }
    return out;
  }

  function redraw() {
    while (group.children.length) {
      const c = group.children.pop();
      c.traverse((o) => { o.geometry?.dispose(); o.material?.dispose(); });
    }
    for (const s of edits.remove) group.add(shapeMesh(s, COLOURS.remove));
    for (const s of edits.add) group.add(shapeMesh(s, COLOURS.add));
    if (draft.length) {
      group.add(shapeMesh(draft, COLOURS.draft));
      for (const p of draft) {
        const dot = new THREE.Mesh(new THREE.SphereGeometry(0.4, 8, 6), new THREE.MeshBasicMaterial({ color: COLOURS.draft, depthTest: false }));
        dot.position.copy(toViewer(p[0], p[1], p[2]));
        dot.renderOrder = 7;
        group.add(dot);
      }
    }
    status();
  }

  function inside(pt, shape) {
    let c = false;
    for (let i = 0, j = shape.length - 1; i < shape.length; j = i++) {
      const [xi, yi] = shape[i], [xj, yj] = shape[j];
      if ((yi > pt[1]) !== (yj > pt[1]) && pt[0] < (xj - xi) * (pt[1] - yi) / (yj - yi) + xi) c = !c;
    }
    return c;
  }

  function change(fn) {
    undo.push(JSON.stringify(edits));
    if (undo.length > 200) undo.shift();
    fn();
    save();
    redraw();
  }

  function handleClick(e) {
    if (!$('wkEdit').checked) return false;
    const p = pickGround(e);
    if (!p) { status('Nothing under the cursor.'); return true; }
    const g = toGta(p).map((v) => +v.toFixed(2));
    if (!draft.length) {
      for (const key of ['remove', 'add']) {
        const i = edits[key].findIndex((s) => inside(g, s));
        if (i >= 0) {
          change(() => edits[key].splice(i, 1));
          status(`Deleted a "${key}" shape.`);
          return true;
        }
      }
    }
    draft.push(g);
    redraw();
    return true;
  }

  async function load() {
    if (loaded) return;
    loaded = true;
    try {
      const view = await (await fetch('../../out/water/water_view.json', { cache: 'no-store' })).json();
      if (view && view.placement) placement = view.placement;
    } catch (_) {}
    if (!edits.remove.length && !edits.add.length) {
      try {
        const r = await fetch('../edits/sidewalk_edits.json', { cache: 'no-store' });
        if (r.ok) {
          const doc = await r.json();
          if (doc && (Array.isArray(doc.remove) || Array.isArray(doc.add))) { edits = Object.assign(emptyEdits(), doc); fromFile = true; save(); }
        }
      } catch (_) {}
    }
    redraw();
  }

  $('wkShow').addEventListener('change', async () => {
    group.visible = $('wkShow').checked;
    if (group.visible) await load();
  });
  $('wkEdit').addEventListener('change', async () => {
    if ($('wkEdit').checked) { $('wkShow').checked = true; group.visible = true; await load(); }
    else { draft = []; redraw(); }
    status();
  });
  $('wkSurf').addEventListener('change', () => showOnlySurfaces($('wkSurf').checked ? SIDEWALK_SURFACES : null));
  panel.querySelectorAll('input[name="wkMode"]').forEach((r) => r.addEventListener('change', () => status()));
  $('wkFinish').addEventListener('click', () => {
    if (draft.length < 3) return status('A shape needs at least 3 points.');
    const shape = draft;
    draft = [];
    change(() => edits[mode()].push(shape));
    status(`Added a "${mode()}" shape.`);
  });
  $('wkCancel').addEventListener('click', () => { draft = []; redraw(); });
  $('wkUndo').addEventListener('click', () => {
    if (draft.length) { draft.pop(); redraw(); return; }
    if (!undo.length) return status('Nothing to undo.');
    edits = JSON.parse(undo.pop());
    save();
    redraw();
  });
  $('wkReset').addEventListener('click', () => {
    if (!confirm('Remove every sidewalk edit saved in this browser?')) return;
    change(() => { edits = emptyEdits(); });
  });
  $('wkExport').addEventListener('click', () => {
    const blob = new Blob([JSON.stringify(edits, null, 1)], { type: 'application/json' });
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = 'sidewalk_edits.json';
    a.click();
    URL.revokeObjectURL(a.href);
    status('Exported. Save it as tools/edits/sidewalk_edits.json, then run tools/rebuild_navmesh.sh (GTA closed).');
  });
  status();

  return { handleClick, group };
}
