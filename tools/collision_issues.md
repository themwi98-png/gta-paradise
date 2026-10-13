# Collision issues found in game

Reported spots to fix in the collision pass (GTA coordinates, map at
`--offset 0,0,9.9`). Marks come from `paradise_marks.csv` (F9 in game).

| # | GTA position | Heading | Symptom (user) | Notes |
|---|---|---|---|---|
| 1 | 1421.47, 759.44, 15.98 | 36 | Upward slope not in the real road geometry; car left floating above the road | On the seam between Burnout units 269/270; generated (gen_) concrete present on both sides |
| 2 | 2615.02, 676.04, 10.96 | 278 | Car suddenly hovering ~30 cm off the ground, beneath the railway (not touching it) | Overlay shows two floor layers: road z 9.73 and concrete/metal/rail-gravel z 10.53 over at least 12 m; unit 144 |
