import json
import subprocess
import sys

klad, bundle, layer = sys.argv[1:4]

HEIGHTS = {
    "#5f7d4cff": "#b0b0b0",
    "#6b5643ff": "#808080",
    "#20263aff": "#4c4c4c",
    "#bff2ffff": "#a8a8a8",
    "#8dbb6aff": "#a0a0a0",
    "#45372cff": "#808080",
}


def drawing(frame):
    out = subprocess.run(
        [klad, "strokes", bundle, layer, "--frame", str(frame), "--json"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    data = json.loads(out)
    return data["cel"], data["fills"]


frames = int(
    subprocess.run([klad, "timeline", bundle], check=True, capture_output=True, text=True).stdout.split()[0]
)
cels = dict(drawing(frame) for frame in range(frames))
marks = [f for fills in cels.values() for f in fills]
unknown = sorted({f["color"] for f in marks} - HEIGHTS.keys())
if unknown:
    sys.exit(f"no height for {unknown}")
ops = [{"op": "set", "id": f["address"], "color": HEIGHTS[f["color"]]} for f in marks]
subprocess.run([klad, "apply", bundle], input="\n".join(json.dumps(op) for op in ops), check=True, text=True)
