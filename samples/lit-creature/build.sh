#!/usr/bin/env bash
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
case "$(uname -s)" in
  Darwin) built=target ;;
  Linux) built=target/linux ;;
  *) built=target/windows ;;
esac
klad="${KLAD:-$root/${KLAD_TARGET_DIR:-$built}/release/klad}"
out="${1:-$here/lit-creature.klad}"
if [ -e "$out" ] && [ "${FORCE:-0}" != 1 ]; then
  echo "$out exists and may hold edits made in the editor; give another path, or FORCE=1 to rebuild it" >&2
  exit 1
fi
rm -rf "$out"
KLAD="$klad" bash "$here/../creature/build.sh" "$out" >/dev/null
"$klad" maps "$out" --on bevel=8 profile=round range=16 ambient=#2c2e3a >/dev/null
height=$("$klad" duplicate "$out" colour | sed -n 's/^added //p')
"$klad" mv "$out" "$height" --above lines >/dev/null
"$klad" set "$out" "$height" name=height map=height >/dev/null
"${PYTHON:-python3}" "$here/heights.py" "$klad" "$out" "$height" >/dev/null
"$klad" set "$out" colour shine=0.4 specular=#fff0d0 glows=#bff2ff >/dev/null
"$klad" apply "$out" >/dev/null <<'OPS'
{"op":"add","layer":{"id":"ball-sketch","kind":"vector","name":"ball sketch","visible":false}}
{"op":"add","layer":{"id":"ball-shadow","kind":"paint","name":"ball shadow","blend":"multiply","opacity":0.35}}
{"op":"add","layer":{"id":"ball","kind":"paint","name":"ball"}}
{"op":"add","layer":{"id":"ball-seams","kind":"paint","name":"ball seams","clip":true}}
{"op":"add","layer":{"id":"ball-grooves","kind":"paint","name":"ball grooves","map":"height","clip":true}}
{"op":"add","node":{"id":"ball-disc","type":"ellipse","cx":800,"cy":752,"rx":70,"ry":70},"in":"ball-sketch"}
{"op":"add","node":{"id":"ball-foot","type":"ellipse","cx":800,"cy":822,"rx":62,"ry":9},"in":"ball-sketch"}
{"op":"fill","layer":"ball","shape":"ball-disc","color":"#d9692e"}
{"op":"fill","layer":"ball-shadow","shape":"ball-foot","color":"#3a2a20"}
OPS
seams=(
  'M 795 683 C 813 720, 813 785, 795 821'
  'M 731 744 C 770 760, 830 760, 869 744'
  'M 752 698 C 780 730, 780 775, 752 806'
  'M 848 698 C 820 730, 820 775, 848 806'
)
for seam in "${seams[@]}"; do
  "$klad" stroke "$out" ball-seams --brush ink --size 4 --color '#2a1a12' --path "$seam" >/dev/null
  "$klad" stroke "$out" ball-grooves --brush soft --size 10 --color '#000000' --path "$seam" >/dev/null
done
"$klad" surface "$out" ball bevel=fit profile=round >/dev/null 2>&1
"$klad" set "$out" ball shine=0.25 specular=#ffe2c8 >/dev/null
"$klad" lights "$out" --add point --at 300,380 --height 170 --color '#ffd9a8' --energy 1.3 --range 900 >/dev/null
"$klad" lights "$out" --add point --at 960,560 --height 70 --color '#9fc4ff' --energy 0.8 --range 520 >/dev/null
"$klad" check "$out" --maps --all
"$klad" render "$out" --frame 0 --lit -o "${out%.klad}-lit.png"
"$klad" render "$out" --frame 0 --map normal --scale 0.5 -o "${out%.klad}-normal.png"
"$klad" hash "$out" --no-cache
"$klad" hash "$out" --map normal
