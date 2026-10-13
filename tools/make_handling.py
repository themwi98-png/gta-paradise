"""Builds Burnout-style handling files (crash damage, speed) for the
mods-folder update.rpf.

Rockstar's own files are kept byte for byte except for the damage
multipliers, and written to the paths update.rpf uses:
    common/data/handling.meta                       base-game vehicles
    dlc_patch/<pack>/common/data/handling.meta      each DLC pack's vehicles
(update.rpf's dlc_patch folder overrides a pack's files; where Rockstar
already ships a patch, that patch is the one scaled.)

    python -I tools/make_handling.py <handling_src-dir> <out-dir>

<handling_src-dir> is laid out by tools/build_crash.sh:
    _update/update/update.rpf/common/data/handling.meta
    _update/update/update.rpf/dlc_patch/<pack>/common/data/handling.meta
    <pack>/handling.meta
"""
import re
import sys
from pathlib import Path

# How much harder crashes hit (Rockstar's values average about 1.0 for
# deformation and collision, 1.5 for engine) and how much faster cars go.
SCALE = {
    # Size and depth of dents.
    "fDeformationDamageMult": 6.0,
    # Body health lost per crash.
    "fCollisionDamageMult": 4.0,
    # Engine health lost per crash (smoke, fire, breakdown).
    "fEngineDamageMult": 3.0,
    # Top speed (the gearing's flat-out speed).
    "fInitialDriveMaxFlatVel": 1.35,
    # Engine power: acceleration, and reaching that top speed.
    "fInitialDriveForce": 1.5,
    # Stronger brakes to match.
    "fBrakeForce": 1.25,
    # Grip: peak cornering grip, and grip while sliding (so slides are
    # catchable at speed).
    "fTractionCurveMax": 1.4,
    "fTractionCurveMin": 1.4,
    # Less wheelspin pulling away (the extra engine power spins the tyres).
    "fLowSpeedTractionLossMult": 0.6,
}
# Upper limits; much higher deformation tears the car's physics apart.
CAP = {
    "fDeformationDamageMult": 25.0,
    "fCollisionDamageMult": 10.0,
    "fEngineDamageMult": 8.0,
    "fInitialDriveMaxFlatVel": 500.0,
    "fInitialDriveForce": 3.0,
    "fBrakeForce": 3.0,
    "fTractionCurveMax": 5.0,
    "fTractionCurveMin": 5.0,
    "fLowSpeedTractionLossMult": 5.0,
}

FIELD = re.compile(r'(<(' + "|".join(SCALE) + r')\s+value=")([-0-9.eE+]+)(")')


def scale(src, dst):
    text = src.read_bytes().decode("utf-8")
    n = 0

    def sub(m):
        nonlocal n
        v = float(m.group(3))
        n += 1
        return f"{m.group(1)}{min(v * SCALE[m.group(2)], max(v, CAP[m.group(2)])):.6f}{m.group(4)}"

    text = FIELD.sub(sub, text)
    dst.parent.mkdir(parents=True, exist_ok=True)
    dst.write_bytes(text.encode("utf-8"))
    return n


def main(src, out):
    src, out = Path(src), Path(out)
    upd = src / "_update/update/update.rpf"
    files = [(upd / "common/data/handling.meta", out / "common/data/handling.meta")]
    for d in sorted(src.iterdir()):
        if d.name.startswith("_") or not (d / "handling.meta").is_file():
            continue
        pack = d.name.lower()
        rel = Path("dlc_patch") / pack / "common/data/handling.meta"
        patch = upd / rel
        files.append((patch if patch.is_file() else d / "handling.meta", out / rel))
    total = sum(scale(s, t) for s, t in files)
    print(f"handling: {len(files)} files, {total} values scaled -> {out}")


if __name__ == "__main__":
    main(*sys.argv[1:3])
