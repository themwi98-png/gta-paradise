# GTA Paradise — how to

Step-by-step instructions for editing the map and getting changes into the
game. For what the mod does, see [FEATURES.md](FEATURES.md).


## The map viewer / editor

Open http://localhost:8765/tools/viewer/index.html (the viewer's local server
must be running; ask Claude to start it if the page doesn't load).

- Pick map units in the **Units** box and **Load**; tick the layers you want
  (Visual, Collision stock / extra).
- Every editor keeps your edits in the browser until you **Export**. Exports
  go to your Downloads folder — move the file into
  `gta-paradise/tools/edits/` with the name given below, replacing the old one.
- Editors load the saved project file when they start, so saved edits always
  show, and exports include them.

### Collision (delete pieces)

1. Load the **Collision (extra)** layer.
2. Tick **Edit extra collision**. Tick **Include native collision** to also
   select Burnout's own pieces (otherwise only the magenta generated ones).
3. Click pieces to select them, **Delete selected**.
4. **Export edits** → save as `tools/edits/collision_edits.json`.
5. Rebuild: `bash gta-paradise/tools/rebuild_collision.sh`
   (also rebuilds water and the pedestrian navmesh).

### Traffic (lanes and parked cars)

Lanes show as yellow lines with arrows in the driving direction. Each lane is
one of Burnout's lane sections (usually junction to junction); **Delete**
removes a whole section — to shorten one, delete it and draw a new lane over
the part to keep.

1. Load the units around the spot, then **Traffic** panel → **Show traffic**
   and **Edit traffic**.
2. Click a lane to select it (cyan; the info box shows id, length, speed and
   what it leads to). Shift+click adds to the selection.
3. Tools for the selection: **Reverse**, **Delete** / **Restore**,
   **◀ Shift / Shift ▶** (0.25 m sideways), **Set speed** (km/h; the info box
   shows GTA's speed band: slow under 40, normal 40–65, fast 65–97, fastest
   above, and traffic only drives differently when the band changes).
   **Set traffic** (0–9, Burnout's scale: how much traffic starts on the
   lane; 0 = none). **Colour by traffic** shows every lane's level (grey
   none, blue quiet … red busy); **Select area** then two clicks on the
   ground selects every lane in that box.
4. **Connect A→B**: click lane A, Shift+click lane B, Connect — a short link
   from A's end to B's start.
5. **Draw lane**: click points in the driving direction, then **Finish lane**
   (green). The first point snaps to the *end* of a lane within 3 m (traffic
   flows in from it); the last point snaps to the *start* of a lane within
   3 m (traffic flows on into it). New lanes are 48 km/h — change with Set
   speed. Long straight bits don't need extra points over hills or
   bridges: the build lays drawn lanes onto the road surface.
6. Parked vehicles (click anywhere on one to select it): **Delete**,
   **Move car** (then click the new spot) or **↺ / ↻ 15°**; **Add parked car**
   then click the spot. **Copy cars** / **Paste cars** (Ctrl+C / Ctrl+V):
   each paste lands 3 m to the side of the last. Type a vehicle model (e.g.
   `maverick`, `luxor`, `dinghy`, `blazer`) and **Set vehicle** to make the
   selected spots always that vehicle (orange; empty box = any car again), or
   type it before **Add parked car**. Boats go on water; aircraft need room.
7. **Export edits** → `tools/edits/traffic_edits.json`.
8. Rebuild: `bash gta-paradise/tools/rebuild_traffic.sh`

Example — shorten a lane and send it somewhere else: select it and note where
it starts, **Delete** it, **Draw lane** from the end of the lane that fed into
it, along the part you keep, to the start of the lane it should join, then
**Finish lane**.

### Water (delete flooded squares)

1. **Water** panel → **Edit water**; pick a brush size, click squares to
   delete (switch to Restore to bring them back).
2. **Export edits** → `tools/edits/water_edits.json`.
3. Rebuild: `bash gta-paradise/tools/rebuild_water.sh`

### Spawns (no-spawn zones)

For Los Santos leftovers spawning in the wrong place (inside buildings,
tunnels…). Mid-air spawns are removed automatically.

1. In game, press **F9** at each problem spot. Close GTA.
2. Bring your marks and the plugin's removal log into the viewer:
   ```bash
   bash gta-paradise/tools/sync_from_game.sh
   ```
   (yellow numbered pins = F9 marks, blue pins = spawns the plugin removed)
3. **Spawns** panel → **Edit zones**; set the radius, click to add a zone,
   click inside a zone to remove it.
4. **Export zones** → `tools/edits/spawn_blocks.json`.
5. Install: `bash gta-paradise/tools/install_dlc.sh`

### Sidewalks (where pedestrians walk)

Sidewalk (GTA "footpath") is where NPCs prefer to walk. It comes from
Burnout's pavement and cobbles surfaces.

1. Load a collision layer and tick **Show only sidewalk surfaces** to see the
   current sidewalks (untick to show everything again).
2. **Sidewalks** panel → **Edit sidewalks**; choose **Remove sidewalk** or
   **Add sidewalk**.
3. Click points around the area (any shape), then **Finish shape**. Click
   inside a finished shape to delete it. **Undo** removes the last point
   while drawing.
   - Remove: sidewalk inside becomes plain walkable ground.
   - Add: ground and road inside become sidewalk.
4. **Export edits** → `tools/edits/sidewalk_edits.json`.
5. Rebuild: `bash gta-paradise/tools/rebuild_navmesh.sh`

### Props (which GTA prop replaces which Burnout prop)

Edit `tools/props_map.csv` (Burnout name, GTA prop name), then
`bash gta-paradise/tools/rebuild_props.sh`. Easiest: F9 a bad prop (and,
ideally, an example of what it should be) and tell Claude.

---

## Settings while playing

`paradise.ini` in the GTA folder is re-read every few seconds — edit it with
the game running. A reinstall overwrites it with the project's copy
(`gta-paradise/tools/paradise.ini`), so tell Claude any values you want kept.
All settings are listed in [FEATURES.md](FEATURES.md#settings-paradiseini-in-the-gta-folder).


## Other scripts

| Command | Does |
|---|---|
| `bash gta-paradise/tools/install_dlc.sh` | Re-installs everything already built (plugin, settings, spawn zones…) |
| `bash gta-paradise/tools/build_crash.sh` | Rebuilds car handling (damage, speed, grip) |
| `VIS_UNITS=all bash gta-paradise/tools/build_dlc.sh tools/collision/paradise_extra.pcx --offset 0,0,9.9` | Rebuilds everything (slow) |
