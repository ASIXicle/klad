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
out="${1:-$here/lantern.klad}"
if [ -e "$out" ] && [ "${FORCE:-0}" != 1 ]; then
  echo "$out exists and may hold edits made in the editor; give another path, or FORCE=1 to rebuild it" >&2
  exit 1
fi
rm -rf "$out"
"$klad" new "$out" --width 1024 --height 1280 --layers 0 >/dev/null
"${PYTHON:-python3}" "$here/../lantern/generate.py" | "$klad" apply "$out" >/dev/null
"$klad" maps "$out" --on bevel=12 profile=round body=1 ambient=#20222c shininess=32 >/dev/null
"$klad" set "$out" light emissive=true >/dev/null
"$klad" set "$out" flats shine=0.35 specular=#cfe8ff >/dev/null
"$klad" apply "$out" >/dev/null <<'OPS'
{"op":"add","layer":{"id":"window-glow","kind":"vector","name":"window glow","map":"emission"}}
{"op":"add","node":{"type":"ellipse","cx":650,"cy":655,"rx":48,"ry":68,"fill":"#bff2ff"},"in":"window-glow"}
{"op":"add","layer":{"id":"stone-lift","kind":"vector","name":"stone lift","map":"height"}}
{"op":"add","node":{"type":"ellipse","cx":520,"cy":1140,"rx":150,"ry":40,"fill":"#ffffff80"},"in":"stone-lift"}
OPS
"$klad" lights "$out" --add point --at 650,655 --height 140 --color '#bff2ff' --energy 1.4 --range 900 >/dev/null
"$klad" lights "$out" --add directional --at -1,-1 --height 0.45 --color '#8090c0' --energy 0.35 >/dev/null
"$klad" check "$out" --maps
"$klad" render "$out" --lit -o "${out%.klad}-lit.png"
"$klad" render "$out" --map normal --scale 0.5 -o "${out%.klad}-normal.png"
"$klad" export "$out" --godot "${out%.klad}-godot" --occluders --shaders occlusion >/dev/null
"$klad" hash "$out" --no-cache
"$klad" hash "$out" --map normal
"$klad" hash "$out" --map specular
"$klad" hash "$out" --map emission
