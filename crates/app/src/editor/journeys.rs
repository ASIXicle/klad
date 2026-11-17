use super::testkit::{Journey, Rows};
use super::{Editor, Tool};
use egui::{Key, Modifiers, vec2};
use egui_kittest::kittest::{NodeT, Queryable};
use klad_core::stroke::Mark;

const WINDOW: egui::Vec2 = vec2(1440.0, 900.0);
const CMD: Modifiers = Modifiers::COMMAND;
const CMD_SHIFT: Modifiers = Modifiers {
    alt: false,
    ctrl: false,
    shift: true,
    mac_cmd: false,
    command: true,
};

fn marks(j: &Journey) -> usize {
    j.ed().map_or(0, |ed| ed.scene.active_marks().len())
}

fn zoom(j: &Journey) -> f32 {
    j.ed().and_then(|ed| ed.view).map_or(0.0, |v| v.zoom)
}

fn centre(j: &Journey) -> egui::Vec2 {
    j.ed().and_then(|ed| ed.view).map_or(egui::Vec2::ZERO, |v| v.center)
}

fn last_brush(ed: &Editor) -> String {
    ed.scene
        .active_marks()
        .last()
        .and_then(|m| m.as_stroke())
        .map_or_else(String::new, |s| s.brush.clone())
}

fn last_colour(ed: &Editor) -> [u16; 4] {
    ed.scene
        .active_marks()
        .last()
        .and_then(|m| m.as_stroke())
        .map_or([0; 4], |s| s.color)
}

fn pngs(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|x| x == "png"))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn the_first_ten_minutes_of_someone_who_draws() {
    let mut j = Journey::begin("first-ten-minutes-draw", WINDOW, None);

    j.act("klad starts");
    j.look("start");
    let can_start = j.shown("Name") && j.shown("Create");
    j.expect("start-offers-new", can_start, "the start window offers a new drawing");

    j.act("make a drawing called mushroom, typing the name into the field as it is");
    j.type_into("Name", "mushroom");
    j.click("Create");
    j.look("new-drawing");
    let open = j.ed().is_some();
    j.expect("new-opens", open, "the new drawing opens in the editor");
    if !open {
        return j.finish(&[]);
    }
    let named = j.folder.join("mushroom.klad").exists();
    j.expect("name-typed-replaces", named, "the drawing is called mushroom, as typed");
    let brush = j.ed().is_some_and(|ed| ed.tool == Tool::Brush);
    j.expect("starts-on-brush", brush, "the brush is the tool in hand");

    j.act("paint three strokes");
    j.drag(&[(300.0, 300.0), (500.0, 220.0), (700.0, 320.0)]);
    j.drag(&[(320.0, 420.0), (680.0, 430.0)]);
    j.drag(&[(500.0, 240.0), (500.0, 600.0)]);
    j.look("three-strokes");
    j.expect("strokes-land", marks(&j) == 3, "three strokes are on the drawing");

    j.act("pick red by typing its hex, then paint");
    j.retype("hex", "#d03030");
    j.drag(&[(900.0, 300.0), (1200.0, 700.0)]);
    j.look("red-stroke");
    let red = j.ed().map(last_colour).is_some_and(|c| c[0] > 40000 && c[1] < 20000);
    j.expect(
        "hex-sets-colour",
        red,
        "the stroke painted after typing a hex colour is that colour",
    );

    j.act("pick the red stroke with V and drag it");
    j.key(Key::V);
    let before = j
        .ed()
        .and_then(|ed| ed.scene.active_marks().last().and_then(|m| m.bounds()));
    j.drag(&[(1050.0, 500.0), (1250.0, 560.0)]);
    j.look("moved");
    let after = j
        .ed()
        .and_then(|ed| ed.scene.active_marks().last().and_then(|m| m.bounds()));
    j.note(format!("the red stroke's box went from {before:?} to {after:?}"));
    let moved = matches!((before, after), (Some(a), Some(b)) if (b[0] - a[0]).abs() > 50.0);
    j.expect(
        "v-drags-a-stroke",
        moved,
        "V picks the stroke under the pointer and drags it",
    );

    j.act("cut the picked stroke with cmd X");
    let before = marks(&j);
    j.chord(CMD, Key::X);
    j.look("cut");
    j.expect("cmd-x-cuts", marks(&j) + 1 == before, "cmd X cuts what is picked");

    j.act("delete the picked stroke with Backspace, then undo");
    let before = marks(&j);
    j.key(Key::Backspace);
    j.look("deleted");
    j.expect(
        "backspace-deletes",
        marks(&j) + 1 == before,
        "Backspace deletes what is picked",
    );
    j.chord(CMD, Key::Z);
    j.look("delete-undone");
    j.expect(
        "undo-brings-back",
        marks(&j) == before,
        "cmd Z brings the deleted stroke back",
    );
    j.key(Key::Escape);
    j.key(Key::B);

    j.act("pick a colour by clicking on the colour wheel, then paint");
    let before = j.ed().map(|ed| ed.color);
    j.click_inside("colour wheel", 0.5, 0.06);
    j.drag(&[(900.0, 800.0), (1300.0, 900.0)]);
    j.look("wheel-colour");
    let changed = j.ed().map(|ed| ed.color) != before;
    j.expect(
        "wheel-sets-colour",
        changed,
        "a click on the colour wheel changes the colour",
    );

    j.act("make the brush bigger with ] three times, then paint");
    let size = j.ed().map_or(0.0, |ed| ed.size);
    (0..3).for_each(|_| j.key(Key::CloseBracket));
    j.drag(&[(300.0, 900.0), (700.0, 1000.0)]);
    j.look("bigger-brush");
    let grew = j.ed().is_some_and(|ed| ed.size > size);
    j.expect("bracket-sizes-brush", grew, "] makes the brush bigger");

    j.act("erase across the first strokes with E");
    let painted = marks(&j);
    j.key(Key::E);
    j.drag(&[(400.0, 200.0), (600.0, 650.0)]);
    j.look("erased");
    let erased = j.ed().map(last_brush).unwrap_or_default();
    j.note(format!("the mark it made uses the brush \"{erased}\""));
    j.expect(
        "e-erases",
        marks(&j) == painted + 1 && erased.contains("eraser"),
        "E erases",
    );

    j.act("go back to the brush with B and paint");
    j.key(Key::B);
    j.drag(&[(1300.0, 300.0), (1700.0, 500.0)]);
    j.look("brush-again");
    let back = j.ed().map(last_brush).unwrap_or_default();
    j.note(format!("the mark it made uses the brush \"{back}\""));
    j.expect(
        "b-paints-again",
        !back.contains("eraser"),
        "B brings the brush back after the eraser",
    );

    j.act("undo twice with cmd Z, redo once with cmd shift Z");
    let before = marks(&j);
    j.chord(CMD, Key::Z);
    j.chord(CMD, Key::Z);
    j.look("undone");
    j.expect(
        "cmd-z-undoes",
        marks(&j) + 2 == before,
        "cmd Z twice takes two strokes away",
    );
    j.chord(CMD_SHIFT, Key::Z);
    j.look("redone");
    j.expect(
        "cmd-shift-z-redoes",
        marks(&j) + 1 == before,
        "cmd shift Z brings one back",
    );

    j.act("redo with cmd Y, the Windows habit");
    j.chord(CMD, Key::Y);
    j.look("redo-y");
    j.expect("cmd-y-redoes", marks(&j) == before, "cmd Y redoes");

    j.act("zoom in with cmd +");
    let fit = zoom(&j);
    j.chord(CMD, Key::Plus);
    j.chord(CMD, Key::Equals);
    j.look("cmd-plus");
    j.expect("cmd-plus-zooms", zoom(&j) > fit, "cmd + zooms the canvas in");

    j.act("zoom out with cmd -");
    j.chord(CMD, Key::Minus);
    j.look("cmd-minus");
    j.expect("cmd-minus-zooms", zoom(&j) < fit, "cmd - zooms the canvas out");

    j.act("zoom with cmd and scroll, then fit with cmd 0");
    j.wheel(1000.0, 1000.0, vec2(0.0, 120.0), CMD);
    j.look("cmd-scroll");
    let zoomed = zoom(&j);
    j.expect(
        "cmd-scroll-zooms",
        (zoomed - fit).abs() > 1e-3,
        "cmd and scroll zooms the canvas",
    );
    j.chord(CMD, Key::Num0);
    j.look("cmd-zero");
    j.expect(
        "cmd-zero-fits",
        (zoom(&j) - fit).abs() < 1e-3,
        "cmd 0 fits the canvas in the window",
    );

    j.act("zoom with a pinch, pan with a two-finger scroll, fit with F");
    let z = zoom(&j);
    j.pinch(1000.0, 1000.0, 1.5);
    j.look("pinched");
    j.expect("pinch-zooms", zoom(&j) > z, "a pinch zooms");
    let c = centre(&j);
    j.wheel(1000.0, 1000.0, vec2(-60.0, -40.0), Modifiers::NONE);
    j.look("scrolled");
    j.expect("scroll-pans", centre(&j) != c, "a two-finger scroll pans");
    j.key(Key::F);
    j.look("fit");
    j.expect(
        "f-fits",
        (zoom(&j) - fit).abs() < 1e-3,
        "F fits the canvas in the window",
    );

    j.act("right click on the canvas");
    j.tap_with(1000.0, 1000.0, egui::PointerButton::Secondary, Modifiers::NONE);
    j.look("right-click");
    j.expect(
        "right-click-menu",
        j.shown("Move to new layer") && j.shown("Select all"),
        "a right click on the canvas opens a menu",
    );
    j.key(Key::Escape);

    j.act("save with cmd S");
    let heard = j.ed().and_then(|ed| ed.messages.latest().map(|m| m.id));
    j.chord(CMD, Key::S);
    j.look("saved");
    let told = j.ed().and_then(|ed| ed.messages.latest().map(|m| m.id)) != heard;
    j.expect(
        "cmd-s-answers",
        told,
        "cmd S gets an answer, such as that klad saves as you go",
    );

    j.act("add a layer from the Layer menu and paint on it");
    let layers = j.ed().map_or(0, |ed| ed.scene.bundle.doc.layers.len());
    j.click("Layer");
    j.click("New paint layer");
    j.look("new-layer");
    let added = j.ed().map_or(0, |ed| ed.scene.bundle.doc.layers.len()) == layers + 1;
    j.expect("layer-menu-adds", added, "Layer > New paint layer adds a layer");
    j.drag(&[(1400.0, 1500.0), (1800.0, 1700.0)]);
    j.look("painted-on-new-layer");
    j.expect(
        "new-layer-takes-paint",
        marks(&j) == 1,
        "the next stroke lands on the new layer",
    );
    let rows =
        j.h.query_all_by_label("paint 1")
            .chain(j.h.query_all_by_label("paint 2"))
            .filter(|n| n.rect().left() > 1040.0 && n.rect().bottom() < 540.0)
            .count();
    j.note(format!("layer rows in sight in the Layers panel: {rows}"));
    j.expect(
        "layers-panel-shows-layers",
        rows == 2,
        "the Layers panel shows both paint layers in a 1440 by 900 window",
    );

    j.act("export a PNG from File > Export > Image");
    j.menu("File", "Export", "Image… cmd E");
    j.look("export-window");
    j.click("Export");
    j.look("exported");
    let written: Vec<String> = pngs(&j.folder)
        .iter()
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .collect();
    j.note(format!("PNG files next to the drawing: {written:?}"));
    j.expect(
        "export-writes-png",
        written.len() == 1,
        "export writes one PNG next to the drawing",
    );
    let internal = j.ed().is_some_and(|ed| {
        ed.messages
            .latest()
            .is_some_and(|m| m.text.contains(" ms") || m.text.contains("dithered"))
    });
    j.expect(
        "export-says-it-plainly",
        !internal,
        "the export message says where the file is without render times",
    );
    j.key(Key::Escape);

    j.act("press + and then - without cmd, as someone reaching for zoom would");
    let frames = j.ed().map_or(0, |ed| ed.scene.bundle.doc.timeline.frames);
    j.key(Key::Equals);
    j.look("plain-plus");
    let after = j.ed().map_or(0, |ed| ed.scene.bundle.doc.timeline.frames);
    j.note(format!("the timeline was {frames} frames long and is now {after}"));
    j.expect(
        "plain-plus-leaves-the-drawing",
        after == frames,
        "+ without cmd does not change the drawing",
    );
    j.key(Key::Minus);
    j.look("plain-minus");
    let back = j.ed().map_or(0, |ed| ed.scene.bundle.doc.timeline.frames);
    j.note(format!("after - the timeline is {back} frames long"));
    j.expect("minus-takes-plus-back", back == frames, "- takes back what + did");

    j.act("type a new hex and press Return while the drawing has several frames");
    j.retype("hex", "#208040");
    j.look("return-in-a-field");
    let playing = j.ed().is_some_and(|ed| ed.timeline.player.is_some());
    j.expect(
        "return-in-a-field-stays-there",
        !playing,
        "Return in a text field only confirms the field and does not start playback",
    );
    if playing {
        j.key(Key::Enter);
    }

    j.act("close the drawing with cmd W and open it again from the recent list");
    let total = j.ed().map_or(0, |ed| ed.scene.total_marks());
    j.chord(CMD, Key::W);
    j.look("closed");
    j.expect(
        "close-goes-to-start",
        j.ed().is_none(),
        "cmd W goes back to the start window",
    );
    j.click_containing("mushroom");
    j.look("reopened");
    let kept = j.ed().map_or(0, |ed| ed.scene.total_marks());
    j.note(format!("{total} marks before closing, {kept} after opening it again"));
    j.expect(
        "work-is-kept",
        j.ed().is_some() && kept == total,
        "the drawing opens again from the recent list with everything in it",
    );

    j.finish(&[
        "cmd-x-cuts",
        "cmd-y-redoes",
        "cmd-plus-zooms",
        "cmd-minus-zooms",
        "cmd-zero-fits",
        "layers-panel-shows-layers",
    ]);
}

fn spot(j: &Journey, pick: impl Fn(&klad_core::stroke::Mark) -> bool) -> Option<(u32, f32, f32)> {
    j.ed().and_then(|ed| {
        ed.scene.active_marks().iter().filter(|m| pick(m)).find_map(|m| {
            let stroke = m.as_stroke()?;
            let (x, y) = stroke.samples.get(stroke.samples.len() / 2)?.pos();
            Some((m.id(), x, y))
        })
    })
}

fn bounds_of(j: &Journey, id: u32) -> Option<[f32; 4]> {
    j.ed().and_then(|ed| {
        ed.scene
            .active_marks()
            .iter()
            .find(|m| m.id() == id)
            .and_then(|m| m.bounds())
    })
}

fn colour_of(j: &Journey, id: u32) -> Option<[u16; 4]> {
    j.ed().and_then(|ed| {
        ed.scene
            .active_marks()
            .iter()
            .find(|m| m.id() == id)
            .and_then(|m| m.as_stroke())
            .map(|s| s.color)
    })
}

fn picked(j: &Journey) -> usize {
    j.ed().and_then(|ed| ed.picked.as_ref()).map_or(0, |p| p.ids.len())
}

#[test]
fn the_first_ten_minutes_of_someone_who_edits_what_an_agent_painted() {
    let mut j = Journey::begin("first-ten-minutes-edit", WINDOW, Some("lantern.klad"));

    j.act("open the lantern an agent painted");
    j.look("opened");
    let open = j.ed().is_some();
    j.expect("drawing-opens", open, "the drawing opens");
    if !open {
        return j.finish(&[]);
    }

    j.act("go to the lines layer by clicking its name");
    j.click("lines");
    j.look("lines-layer");
    let on_lines = j
        .ed()
        .and_then(|ed| ed.scene.active_layer().map(|l| l.name.clone()))
        .is_some_and(|n| n == "lines");
    j.expect(
        "layer-click-activates",
        on_lines,
        "a click on a layer's name makes it the layer in hand",
    );
    let Some((id, x, y)) = spot(&j, |m| m.as_stroke().is_some_and(|s| s.samples.len() > 8)) else {
        j.note("no stroke to work on");
        return j.finish(&[]);
    };

    j.act("click a line with the tool in hand, as someone who wants to select it");
    let before = j.ed().map_or(0, |ed| ed.scene.total_marks());
    let tool = j.ed().map(|ed| format!("{:?}", ed.tool)).unwrap_or_default();
    j.tap(x, y);
    j.look("first-click");
    let after = j.ed().map_or(0, |ed| ed.scene.total_marks());
    j.note(format!(
        "the tool in hand was {tool}; the drawing had {before} marks and has {after}"
    ));
    j.expect(
        "first-click-leaves-the-drawing",
        after == before,
        "a first click on someone else's drawing does not paint on it",
    );
    if after != before {
        j.chord(CMD, Key::Z);
    }

    j.act("pick that line with V");
    j.key(Key::V);
    j.tap(x, y);
    j.look("picked");
    j.expect(
        "v-click-picks",
        picked(&j) == 1,
        "V and a click pick the line under the pointer",
    );
    let id = j
        .ed()
        .and_then(|ed| ed.picked.as_ref().and_then(|p| p.ids.first().copied()))
        .unwrap_or(id);

    j.act("give it another colour by choosing the colour while it is picked");
    let was = colour_of(&j, id);
    j.retype("hex", "#d03030");
    j.look("colour-while-picked");
    j.expect(
        "colour-applies-to-selection",
        colour_of(&j, id) != was,
        "choosing a colour recolours what is picked",
    );
    let playing = j.ed().is_some_and(|ed| ed.timeline.player.is_some());
    j.expect(
        "return-in-a-field-stays-there",
        !playing,
        "Return in a text field only confirms the field and does not start playback",
    );
    if playing {
        j.key(Key::Enter);
    }

    j.act("recolour it from Edit > Strokes > Recolour");
    let sub = j.menu("Edit", "Strokes", "Recolour");
    j.look("recoloured");
    j.expect(
        "recolour-command-works",
        sub && colour_of(&j, id) != was,
        "Edit > Strokes > Recolour recolours it",
    );

    j.act("nudge it with the arrow keys");
    let was = bounds_of(&j, id);
    j.key(Key::ArrowRight);
    j.chord(Modifiers::SHIFT, Key::ArrowDown);
    j.look("nudged");
    let now = bounds_of(&j, id);
    j.note(format!("its box went from {was:?} to {now:?}"));
    let nudged =
        matches!((was, now), (Some(a), Some(b)) if (b[0] - a[0] - 1.0).abs() < 0.6 && (b[1] - a[1] - 10.0).abs() < 0.6);
    j.expect("arrows-nudge", nudged, "the arrows move it 1 px, and 10 px with shift");

    j.act("drag it somewhere else");
    let was = bounds_of(&j, id);
    let from = was.map_or((x, y), |b| ((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0));
    let (px, py) = spot(&j, |m| m.id() == id).map_or(from, |(_, x, y)| (x, y));
    j.drag(&[(px, py), (px + 120.0, py - 80.0)]);
    j.look("dragged");
    let now = bounds_of(&j, id);
    let dragged = matches!((was, now), (Some(a), Some(b)) if (b[0] - a[0]).abs() > 60.0);
    j.expect("drag-moves", dragged, "a drag moves the picked line");

    j.act("copy it with cmd C and paste it with cmd V");
    let before = marks(&j);
    j.chord(CMD, Key::C);
    j.chord(CMD, Key::V);
    j.look("pasted");
    j.expect("copy-paste", marks(&j) == before + 1, "cmd C then cmd V makes a copy");

    j.act("duplicate with cmd D, then delete the copy with Delete");
    let before = marks(&j);
    j.chord(CMD, Key::D);
    j.look("duplicated");
    j.expect(
        "cmd-d-duplicates",
        marks(&j) == before + 1,
        "cmd D duplicates what is picked",
    );
    j.key(Key::Delete);
    j.look("deleted");
    j.expect("delete-deletes", marks(&j) == before, "Delete removes what is picked");

    j.act("select everything with cmd A, then nothing with Esc");
    j.chord(CMD, Key::A);
    j.look("all-picked");
    let all = picked(&j);
    j.expect("cmd-a-picks-all", all > 1, "cmd A picks every stroke on the layer");
    j.key(Key::Escape);
    j.look("none-picked");
    j.expect("esc-clears", picked(&j) == 0, "Esc clears the selection");

    j.act("drag a loop around part of the drawing to pick several lines");
    j.drag(&[
        (300.0, 300.0),
        (760.0, 300.0),
        (760.0, 700.0),
        (300.0, 700.0),
        (300.0, 310.0),
    ]);
    j.look("looped");
    j.note(format!("{} lines picked by the loop", picked(&j)));
    j.expect(
        "loop-picks",
        picked(&j) >= 1,
        "a loop dragged on the canvas picks the lines inside",
    );
    j.key(Key::Escape);

    j.act("hide the lines layer from Layer > Show, and show it again");
    j.click("Layer");
    j.click("Show");
    j.look("hidden");
    let hidden = j.ed().and_then(|ed| ed.scene.active_layer().map(|l| !l.visible));
    j.expect("layer-hides", hidden == Some(true), "Layer > Show hides the layer");
    j.click("Layer");
    j.click("Show");

    j.act("undo five times and redo five times");
    let before = j.ed().map_or(0, |ed| ed.scene.total_marks());
    (0..5).for_each(|_| j.chord(CMD, Key::Z));
    j.look("undone");
    (0..5).for_each(|_| j.chord(CMD_SHIFT, Key::Z));
    j.look("redone");
    let after = j.ed().map_or(0, |ed| ed.scene.total_marks());
    j.expect(
        "undo-redo-round-trip",
        after == before,
        "five undos and five redos end where they began",
    );

    j.finish(&["first-click-leaves-the-drawing", "colour-applies-to-selection"]);
}

#[test]
fn the_first_ten_minutes_of_someone_who_makes_pixel_art() {
    let mut j = Journey::begin("first-ten-minutes-pixel", WINDOW, None);

    j.act("klad starts; choose pixel art");
    j.click("Pixel art");
    j.look("pixel-start");

    j.act("make a sprite called slime");
    j.retype("Name", "slime");
    j.click("Create");
    j.look("new-sprite");
    let open = j.ed().is_some();
    j.expect("sprite-opens", open, "the new sprite opens in the editor");
    if !open {
        return j.finish(&[]);
    }
    let (w, h) = j.ed().map_or((0, 0), |ed| ed.scene.size());
    j.note(format!("the sprite is {w} by {h}"));
    let pixel = j
        .ed()
        .and_then(|ed| ed.scene.active_layer().map(|l| l.pixel()))
        .unwrap_or(false);
    j.expect("starts-on-a-pixel-layer", pixel, "the layer in hand is a pixel layer");

    j.act("draw a few pixels");
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    j.drag(&[(cx - 6.0, cy), (cx + 6.0, cy)]);
    j.drag(&[(cx, cy - 6.0), (cx, cy + 6.0)]);
    j.look("pixels");
    j.expect("pixels-land", marks(&j) >= 1, "dragging draws pixels");
    let ring = j.ed().and_then(|ed| ed.ring());
    let pencil = j.ed().map_or(0, |ed| ed.pixel_size);
    j.note(format!(
        "the pencil is {pencil} px wide; the brush ring around the pointer: {ring:?}"
    ));
    j.expect(
        "pixel-pointer-is-the-pencil",
        ring.is_none(),
        "on a pixel layer the pointer outlines the pencil's pixels, with no brush ring",
    );

    j.act("pick a green by its hex and draw with it");
    let swatches = j.ed().map_or(0, |ed| ed.scene.bundle.doc.palette.len());
    j.retype("hex", "#40a060");
    let playing = j.ed().is_some_and(|ed| ed.timeline.player.is_some());
    if playing {
        j.key(Key::Enter);
    }
    j.drag(&[(cx - 10.0, cy + 10.0), (cx + 10.0, cy + 10.0)]);
    j.look("green-pixels");
    let now = j.ed().map_or(0, |ed| ed.scene.bundle.doc.palette.len());
    j.note(format!("the palette had {swatches} colours and has {now}"));
    j.expect(
        "new-colour-joins-palette",
        now == swatches + 1,
        "a colour drawn with joins the palette",
    );

    j.act("erase a few pixels with E, then fill an area with G");
    j.key(Key::E);
    j.drag(&[(cx - 3.0, cy), (cx + 3.0, cy)]);
    j.look("erased");
    j.key(Key::G);
    j.tap(cx - 20.0, cy - 20.0);
    j.look("filled");
    let tool = j.ed().map(|ed| ed.tool);
    j.expect(
        "g-fills",
        tool == Some(Tool::Fill) && marks(&j) >= 2,
        "G fills the area that is clicked",
    );
    j.chord(CMD, Key::Z);

    j.act("zoom with a pinch");
    j.key(Key::B);
    j.pinch(cx, cy, 1.6);
    j.look("pinched");
    let px = zoom(&j) * j.h.ctx.pixels_per_point();
    j.note(format!("a pixel of the sprite is {px:.2} screen pixels wide"));
    j.expect(
        "zoom-in-whole-steps",
        (px - px.round()).abs() < 1e-3,
        "the zoom stays on whole steps",
    );

    j.act("select some pixels by dragging a box with S and nudge them");
    j.key(Key::S);
    j.drag(&[(cx - 8.0, cy - 8.0), (cx + 8.0, cy + 8.0)]);
    j.look("boxed");
    j.note(format!("{} marks picked", picked(&j)));
    j.expect("box-picks-pixels", picked(&j) >= 1, "a box dragged with S picks pixels");
    j.key(Key::ArrowRight);
    j.look("nudged");
    j.key(Key::Escape);

    j.act("look in the Frame menu for a new frame");
    j.click("Frame");
    j.look("frame-menu");
    j.key(Key::Escape);

    j.act("add a frame after this one with shift K and draw on it");
    j.key(Key::B);
    let frames = j.ed().map_or(0, |ed| ed.scene.bundle.doc.timeline.frames);
    j.chord(Modifiers::SHIFT, Key::K);
    j.look("second-frame");
    let now = j.ed().map_or(0, |ed| ed.scene.bundle.doc.timeline.frames);
    j.note(format!("the timeline was {frames} frames long and is now {now}"));
    j.expect(
        "shift-k-adds-a-frame",
        now > frames,
        "shift K adds a frame after this one",
    );
    j.drag(&[(cx - 6.0, cy - 12.0), (cx + 6.0, cy - 12.0)]);
    j.look("drawn-on-second-frame");

    j.act("play with the space bar, then with Return");
    j.key(Key::Space);
    j.look("space");
    let space = j.ed().is_some_and(|ed| ed.timeline.player.is_some());
    j.expect("space-plays", space, "the space bar plays the animation");
    if !space {
        j.key(Key::Enter);
    }
    j.look("playing");
    let playing = j.ed().is_some_and(|ed| ed.timeline.player.is_some());
    j.expect("return-plays", playing, "Return plays the animation");
    j.key(Key::Enter);

    j.act("export a GIF from File > Export > GIF");
    j.menu("File", "Export", "GIF…");
    j.look("export-gif");
    j.click("Export");
    j.look("exported-gif");
    let gifs: Vec<(String, u16, u16)> = std::fs::read_dir(&j.folder)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|x| x == "gif"))
                .filter_map(|p| {
                    let bytes = std::fs::read(&p).ok()?;
                    let side = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
                    Some((p.file_name()?.to_string_lossy().to_string(), side(6), side(8)))
                })
                .collect()
        })
        .unwrap_or_default();
    j.note(format!("GIF files next to the drawing, with their sizes: {gifs:?}"));
    j.expect(
        "gif-exports",
        gifs.len() == 1,
        "export writes one GIF next to the drawing",
    );

    j.finish(&["space-plays"]);
}

#[test]
fn what_a_cold_first_session_tripped_on() {
    let mut j = Journey::begin("cold-session", WINDOW, None);

    j.act("klad starts; make a drawing");
    j.type_into("Name", "house");
    j.click("Create");
    j.look("new-drawing");
    if j.ed().is_none() {
        return j.finish(&[]);
    }

    j.act("click once on the canvas with the brush, without dragging");
    j.tap(600.0, 600.0);
    j.look("dot");
    let dot = j
        .ed()
        .and_then(|ed| ed.scene.active_marks().last().and_then(|m| m.bounds()));
    j.note(format!("the click left a mark with the box {dot:?}"));
    j.expect(
        "a-click-paints-a-dot",
        dot.is_some_and(|b| b[2] - b[0] > 2.0),
        "one click with the brush leaves a dot as wide as the brush",
    );

    j.act("hold alt over the paint, as someone who wants to pick its colour");
    j.hover_with(600.0, 600.0, Modifiers::ALT);
    j.look("alt-held");
    j.let_go();

    j.act("double click the layer's name and type a new one");
    j.double_click("paint 1");
    j.look("rename-started");
    j.type_text("roof");
    j.key(Key::Enter);
    j.look("renamed");
    let name = j
        .ed()
        .and_then(|ed| ed.scene.active_layer().map(|l| l.name.clone()))
        .unwrap_or_default();
    j.note(format!("the layer is now called \"{name}\""));
    j.expect(
        "double-click-renames-by-typing",
        name == "roof",
        "a double click on a layer's name selects it, so typing replaces it",
    );

    j.act("look for other ways to mix a colour for a shadow");
    j.click("Lab");
    j.look("colour-lab");
    let lab =
        j.h.query_all_by_role_and_label(egui::accesskit::Role::Slider, "L")
            .count()
            == 1
            && j.h
                .query_all_by_role_and_label(egui::accesskit::Role::Slider, "b")
                .count()
                == 1;
    j.expect(
        "colour-models",
        lab,
        "the Colour panel mixes in HSL, RGB and Lab as well as on the wheel",
    );
    j.click("HSV");

    j.act("draw a roof, select its strokes and right click to split them off");
    j.drag(&[(300.0, 500.0), (800.0, 200.0)]);
    j.drag(&[(800.0, 200.0), (1300.0, 500.0)]);
    let layers = j.ed().map_or(0, |ed| ed.scene.bundle.doc.all_layers().len());
    j.key(Key::S);
    j.chord(CMD, Key::A);
    j.tap_with(1500.0, 1500.0, egui::PointerButton::Secondary, Modifiers::NONE);
    j.look("right-click-with-a-selection");
    j.click("Move to new layer");
    j.look("moved-to-new-layer");
    let split = j.ed().is_some_and(|ed| {
        ed.scene.bundle.doc.all_layers().len() == layers + 1
            && ed.scene.active_layer().is_some_and(|l| l.name == "roof part")
            && ed.scene.active_marks().len() == 3
    });
    j.expect(
        "right-click-moves-to-new-layer",
        split,
        "right click > Move to new layer puts the selected strokes on a layer of their own",
    );

    j.act("recolour them with the button at the selection");
    let was = j.ed().and_then(|ed| ed.scene.active_marks().first().map(|m| m.color()));
    j.click("the colour of the selected strokes");
    j.look("recolour-picker");
    let picker = j.h.query_all_by_label("colour wheel").count() == 2;
    j.expect(
        "recolour-opens-a-picker",
        picker && j.ed().and_then(|ed| ed.scene.active_marks().first().map(|m| m.color())) == was,
        "the recolour button opens a colour picker at the selection and changes nothing before a choice",
    );
    j.key(Key::Escape);

    j.act("draw a rectangle with U while the paint layer is the active one");
    j.key(Key::U);
    j.drag(&[(400.0, 700.0), (1200.0, 1500.0)]);
    j.look("shape-on-a-paint-layer");
    let shape = j.ed().is_some_and(|ed| {
        ed.scene
            .active_layer()
            .is_some_and(|l| l.kind() == klad_core::doc::Kind::Vector && l.cels()[0].nodes.len() == 1)
    });
    j.expect(
        "shape-makes-its-vector-layer",
        shape,
        "a shape drawn while a paint layer is active lands on a new vector layer",
    );

    j.act("copy the rectangle with cmd C and paste it with cmd V");
    j.key(Key::V);
    j.tap(800.0, 1100.0);
    j.chord(CMD, Key::C);
    j.chord(CMD, Key::V);
    j.look("shape-pasted");
    let pasted = j
        .ed()
        .is_some_and(|ed| ed.scene.active_layer().is_some_and(|l| l.cels()[0].nodes.len() == 2));
    j.expect("shape-copy-paste", pasted, "cmd C and cmd V copy a vector shape");
    j.key(Key::Escape);

    j.act("turn the view with two fingers, then put it back");
    j.twist(1000.0, 1000.0, 23.0, Modifiers::NONE);
    j.twist(1000.0, 1000.0, 4.0, Modifiers::SHIFT);
    j.look("turned");
    let angle = |j: &Journey| j.ed().and_then(|ed| ed.view).map_or(0.0, |v| v.angle.to_degrees());
    j.expect(
        "shift-snaps-the-twist",
        (angle(&j) - 30.0).abs() < 0.01,
        "holding shift while twisting snaps the view to 15 degree steps",
    );
    j.click_containing("upright");
    j.look("upright");
    j.expect(
        "turned-view-goes-back",
        angle(&j).abs() < 0.01,
        "a button on the canvas puts a turned view upright again",
    );

    j.act("press I on the canvas, as someone reaching for an eyedropper");
    let cels = |j: &Journey| {
        j.ed()
            .map_or(0, |ed| ed.scene.active_layer().map_or(0, |l| l.cels().len()))
    };
    let frames = |j: &Journey| j.ed().map_or(0, |ed| ed.scene.bundle.doc.timeline.frames);
    let (was_cels, was_frames) = (cels(&j), frames(&j));
    j.key(Key::I);
    j.look("i-on-the-canvas");
    j.expect(
        "frame-keys-wait-for-the-timeline",
        (cels(&j), frames(&j)) == (was_cels, was_frames) && j.heard().contains("timeline key"),
        "I changes no timing while the canvas is in use, and says where it works",
    );

    j.act("make three drawings, select every frame in the timeline and press cmd D");
    j.click("roof");
    j.key(Key::B);
    j.chord(Modifiers::SHIFT, Key::K);
    j.drag(&[(300.0, 300.0), (500.0, 300.0)]);
    j.chord(Modifiers::SHIFT, Key::K);
    j.drag(&[(300.0, 400.0), (500.0, 400.0)]);
    j.look("three-drawings");
    let length = frames(&j);
    let drawings = cels(&j);
    j.drag_frames("roof", 1, 1);
    j.chord(CMD, Key::A);
    j.look("frames-selected");
    j.chord(CMD, Key::D);
    j.look("frames-repeated");
    j.note(format!(
        "{drawings} drawings on {length} frames became {} drawings on {} frames",
        cels(&j),
        frames(&j)
    ));
    j.expect(
        "cmd-d-repeats-selected-frames",
        cels(&j) == drawings * 2 && frames(&j) == length * 2,
        "cmd D with frames selected in the timeline repeats all of them after themselves",
    );

    j.act("press + and then - on a frame in the timeline");
    j.drag_frames("roof", 1, 1);
    let holds = |j: &Journey| -> Vec<(u32, Option<u32>)> {
        j.ed().map_or(Vec::new(), |ed| {
            ed.scene
                .bundle
                .doc
                .all_layers()
                .iter()
                .flat_map(|l| l.cels().iter().map(|c| (c.frame, c.hold)))
                .collect()
        })
    };
    let (was_holds, was_frames) = (holds(&j), frames(&j));
    j.key(Key::Equals);
    j.look("plus-on-a-frame");
    let longer = frames(&j);
    j.key(Key::Minus);
    j.look("minus-on-a-frame");
    j.note(format!(
        "the timeline went from {was_frames} frames to {longer} and back to {}",
        frames(&j)
    ));
    j.expect(
        "plus-and-minus-undo-each-other",
        longer == was_frames + 1 && frames(&j) == was_frames && holds(&j) == was_holds,
        "on a frame in the timeline + makes the drawing one frame longer and - takes exactly that back",
    );

    j.act("save with cmd S");
    j.chord(CMD, Key::S);
    j.look("saved");
    j.expect(
        "cmd-s-says-it-is-saved",
        j.heard().contains("nothing to save") && j.heard().contains("house.klad"),
        "cmd S says the drawing is saved as you go, and where it is",
    );

    j.act("export the picture from File > Export > Image, with a name");
    j.menu("File", "Export", "Image… cmd E");
    j.type_into("save as", "roof-shot");
    j.look("export-image");
    let middle =
        j.h.query_all_by_label_contains("Export image")
            .find(|n| n.accesskit_node().role() == egui::accesskit::Role::Window)
            .map(|n| n.rect().center());
    j.expect(
        "export-opens-in-the-middle",
        middle.is_some_and(|c| (c - egui::pos2(WINDOW.x / 2.0, WINDOW.y / 2.0)).length() < 4.0),
        "the export window opens in the middle of the window",
    );
    j.click("Export");
    j.look("exported");
    let written = pngs(&j.folder)
        .iter()
        .any(|p| p.file_name().is_some_and(|n| n == "roof-shot.png"));
    j.expect(
        "export-takes-a-name-and-says-where",
        written && j.heard().starts_with("exported to ") && j.heard().ends_with("roof-shot.png"),
        "the export is written under the name typed and the message says where it is",
    );
    j.menu("File", "Export", "Godot…");
    j.look("export-godot");
    j.expect(
        "godot-has-its-own-window",
        j.shown("Godot folder") && !j.shown("16 bits per channel"),
        "what Godot needs is in its own export window",
    );

    j.finish(&[]);
}

fn layer_marks(j: &Journey, layer: &str) -> Vec<Mark> {
    j.ed().map_or_else(Vec::new, |ed| {
        ed.scene
            .bundle
            .doc
            .find_layer(layer)
            .ok()
            .and_then(|l| l.content_at(ed.scene.frame))
            .map(|c| ed.scene.marks_of(&c.id).to_vec())
            .unwrap_or_default()
    })
}

fn mark_of(j: &Journey, layer: &str, id: u32) -> Option<Mark> {
    layer_marks(j, layer).into_iter().find(|m| m.id() == id)
}

fn free_spot(marks: &[Mark], id: u32) -> Option<(f32, f32)> {
    let own = marks.iter().find(|m| m.id() == id)?.as_stroke()?;
    let others: Vec<(f32, f32)> = marks
        .iter()
        .filter(|m| m.id() != id)
        .filter_map(|m| m.as_stroke())
        .flat_map(|s| s.samples.iter().step_by(3).map(|p| p.pos()))
        .collect();
    let count = own.samples.len();
    own.samples
        .iter()
        .skip(count / 5)
        .take((count * 3 / 5).max(1))
        .map(|p| p.pos())
        .map(|(x, y)| {
            let near = others
                .iter()
                .map(|(ox, oy)| (ox - x).hypot(oy - y))
                .fold(f32::MAX, f32::min);
            (near, x, y)
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, x, y)| (x, y))
}

fn picked_ids(j: &Journey) -> Vec<u32> {
    j.ed()
        .and_then(|ed| ed.picked.as_ref())
        .map_or_else(Vec::new, |p| p.ids.clone())
}

fn active_name(j: &Journey) -> String {
    j.ed()
        .and_then(|ed| ed.scene.active_layer().map(|l| l.name.clone()))
        .unwrap_or_else(|| "no layer".into())
}

fn hex_of(mark: &Mark) -> String {
    klad_core::color::to_hex(mark.color())
}

fn box_of(marks: &[Mark], ids: &[u32]) -> Option<[f32; 4]> {
    marks
        .iter()
        .filter(|m| ids.contains(&m.id()))
        .filter_map(|m| m.bounds())
        .reduce(|a, b| [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])])
}

fn handle_corners(j: &Journey) -> Option<[egui::Pos2; 4]> {
    let (_, corners) = j.ed()?.handles()?;
    Some(corners.map(|c| j.at(c.x, c.y)))
}

fn near(a: Option<[f32; 4]>, b: Option<[f32; 4]>, by: f32) -> bool {
    matches!((a, b), (Some(a), Some(b)) if a.iter().zip(b).all(|(a, b)| (a - b).abs() <= by))
}

#[test]
fn someone_changes_one_stroke_and_several_in_what_an_agent_painted() {
    let mut j = Journey::begin("edit-strokes", WINDOW, Some("lantern.klad"));

    j.act("open the lantern an agent painted and take the select tool");
    j.key(Key::V);
    j.look("opened");
    if j.ed().is_none() {
        return j.finish(&[]);
    }
    let lines = layer_marks(&j, "lines");
    let blades: Vec<(u32, f32, f32)> = lines
        .iter()
        .filter(|m| m.bounds().is_some_and(|b| b[2] < 330.0 && b[1] > 1100.0))
        .filter_map(|m| free_spot(&lines, m.id()).map(|(x, y)| (m.id(), x, y)))
        .take(3)
        .collect();
    j.note(format!("the blades of grass left of the stone: {blades:?}"));
    if blades.len() < 3 {
        return j.finish(&[]);
    }

    j.act("zoom in on the grass left of the stone, to work on single blades");
    j.pinch(268.0, 1160.0, 2.0);
    j.pinch(268.0, 1160.0, 2.0);
    j.look("zoomed");
    let spot = j.at(268.0, 1160.0);
    let in_view = j.ed().is_some_and(|ed| ed.canvas_rect.contains(spot));
    j.note(format!("zoom is {:.2}", zoom(&j)));
    j.expect(
        "zoom-keeps-the-spot",
        in_view,
        "after zooming in on the grass it is still in view",
    );

    j.act("click one blade");
    j.tap(blades[0].1, blades[0].2);
    j.look("one-blade");
    j.note(format!("picked {:?} on {}", picked_ids(&j), active_name(&j)));
    j.expect(
        "click-picks-the-blade",
        picked_ids(&j) == [blades[0].0],
        "a click picks the blade under the pointer",
    );

    j.act("shift click two more blades");
    j.tap_with(blades[1].1, blades[1].2, egui::PointerButton::Primary, Modifiers::SHIFT);
    j.tap_with(blades[2].1, blades[2].2, egui::PointerButton::Primary, Modifiers::SHIFT);
    j.look("three-blades");
    j.note(format!("picked {:?}", picked_ids(&j)));
    j.expect(
        "shift-click-adds",
        picked_ids(&j).len() == 3,
        "shift click adds to the selection",
    );

    j.act("shift click the second one again, to take it out");
    j.tap_with(blades[1].1, blades[1].2, egui::PointerButton::Primary, Modifiers::SHIFT);
    j.look("two-blades");
    let two = picked_ids(&j);
    j.note(format!("picked {two:?}"));
    j.expect(
        "shift-click-takes-out",
        two.len() == 2 && !two.contains(&blades[1].0),
        "shift click on a picked stroke takes it out of the selection",
    );

    j.act("make the two thinner with the minus in the bar at the selection, twice");
    let sizes = |j: &Journey, ids: &[u32]| -> Vec<f32> {
        layer_marks(j, "lines")
            .iter()
            .filter(|m| ids.contains(&m.id()))
            .filter_map(|m| m.as_stroke().map(|s| s.size))
            .collect()
    };
    let was = sizes(&j, &two);
    j.press_hinted("make the selected strokes thinner");
    j.press_hinted("make the selected strokes thinner");
    j.look("thinner");
    let now = sizes(&j, &two);
    j.note(format!("their sizes went from {was:?} to {now:?}"));
    j.expect(
        "thinner-thins",
        was.len() == 2 && was.iter().zip(&now).all(|(a, b)| (b / a - 0.64).abs() < 0.02),
        "each click on minus makes the selected strokes a fifth thinner",
    );

    j.act("give them the stone colour from the palette, through the bar's colour button");
    let opened = j.press_hinted("the colour of the selected strokes");
    j.look("picker");
    j.expect(
        "colour-button-opens-a-picker",
        opened && j.shown("use the current colour"),
        "the bar's colour button opens a picker",
    );
    let popup =
        j.h.query_all_by_label("use the current colour")
            .next()
            .map(|n| n.rect())
            .zip(
                j.h.query_all_by_label("the colour of the selected strokes")
                    .find(|n| n.accesskit_node().role() == egui::accesskit::Role::Label)
                    .map(|n| n.rect()),
            )
            .map(|(low, title)| low.union(title));
    let selection = handle_corners(&j).map(|c| egui::Rect::from_points(&c));
    j.note(format!(
        "the picker is at {popup:?}, the selection's box at {selection:?}"
    ));
    j.expect(
        "the-picker-leaves-the-selection-in-sight",
        !matches!((popup, selection), (Some(p), Some(s)) if p.intersects(s)),
        "the picker opens beside what it recolours, so the change can be seen",
    );
    j.click_leftmost("#6b7584");
    j.look("recoloured");
    let colours: Vec<String> = layer_marks(&j, "lines")
        .iter()
        .filter(|m| two.contains(&m.id()))
        .map(hex_of)
        .collect();
    j.note(format!("their colours are {colours:?}"));
    j.expect(
        "palette-colour-recolours-several",
        colours.len() == 2 && colours.iter().all(|c| c == "#6b7584"),
        "a palette colour in the picker recolours every selected stroke",
    );

    j.act("close the picker with Esc, to go on with the two blades");
    j.key(Key::Escape);
    j.look("picker-closed");
    j.note(format!("picked {:?}", picked_ids(&j)));
    j.expect(
        "esc-closes-the-picker-first",
        !j.shown("use the current colour") && picked_ids(&j).len() == 2,
        "Esc closes the picker and keeps the selection",
    );
    if picked_ids(&j).len() != 2 {
        j.tap(blades[0].1, blades[0].2);
        j.tap_with(blades[2].1, blades[2].2, egui::PointerButton::Primary, Modifiers::SHIFT);
        j.note(format!("picked the two again: {:?}", picked_ids(&j)));
    }

    j.act("turn them a little by the knob above their box");
    let before_turn = box_of(&layer_marks(&j, "lines"), &two);
    let knob_of = |c: [egui::Pos2; 4]| {
        let top = c[0].lerp(c[1], 0.5);
        let centre = (c[0].to_vec2() + c[1].to_vec2() + c[2].to_vec2() + c[3].to_vec2()) / 4.0;
        top + (top.to_vec2() - centre).normalized() * 30.0
    };
    if let Some(c) = handle_corners(&j) {
        let knob = knob_of(c);
        j.drag_screen(
            knob,
            knob + vec2(40.0, 10.0),
            "the knob above the selection's box to the right",
        );
    } else {
        j.note("no box around the selection to turn it by");
    }
    j.look("turned");
    let after_turn = box_of(&layer_marks(&j, "lines"), &two);
    j.note(format!("their box went from {before_turn:?} to {after_turn:?}"));
    j.expect(
        "knob-turns-under-the-bar",
        !near(before_turn, after_turn, 0.5),
        "the knob turns the selection, also when the bar sits above the box",
    );

    j.act("the bar is on the knob: scroll the grass up to the middle of the canvas and try the knob again");
    let low = j.at(268.0, 1160.0).y;
    j.wheel(268.0, 1160.0, vec2(0.0, -220.0), Modifiers::NONE);
    if j.at(268.0, 1160.0).y >= low {
        j.wheel(268.0, 1160.0, vec2(0.0, 440.0), Modifiers::NONE);
    }
    j.note(format!(
        "the grass went from y {low:.0} to y {:.0} on screen",
        j.at(268.0, 1160.0).y
    ));
    if let Some(c) = handle_corners(&j) {
        let knob = knob_of(c);
        j.drag_screen(
            knob,
            knob + vec2(40.0, 10.0),
            "the knob above the selection's box to the right",
        );
    }
    j.look("turned-with-room");
    let after_turn = box_of(&layer_marks(&j, "lines"), &two);
    j.note(format!("their box went from {before_turn:?} to {after_turn:?}"));
    j.expect(
        "knob-turns",
        !near(before_turn, after_turn, 0.5) && sizes(&j, &two) == now,
        "the knob turns the selection and leaves the strokes as thick as they were",
    );

    j.act("make them bigger by a corner of the box");
    if let Some(c) = handle_corners(&j) {
        let centre = (c[0].to_vec2() + c[1].to_vec2() + c[2].to_vec2() + c[3].to_vec2()) / 4.0;
        let out = (c[0].to_vec2() - centre).normalized() * 40.0;
        j.drag_screen(c[0], c[0] + out, "the top left corner of the box outwards");
    }
    j.look("scaled");
    let after_scale = box_of(&layer_marks(&j, "lines"), &two);
    let grew = matches!((after_turn, after_scale), (Some(a), Some(b)) if (b[2] - b[0]) > (a[2] - a[0]) * 1.1);
    j.note(format!(
        "their box is now {after_scale:?}, their sizes {:?}",
        sizes(&j, &two)
    ));
    j.expect("corner-scales", grew, "a corner of the box scales the selection");

    j.act("take the turn and the scale back with cmd Z, twice");
    j.chord(CMD, Key::Z);
    j.chord(CMD, Key::Z);
    j.look("turn-and-scale-undone");
    let back = box_of(&layer_marks(&j, "lines"), &two);
    j.expect(
        "undo-takes-back-the-transforms",
        near(before_turn, back, 0.5),
        "two undos put the blades back where they were before the turn",
    );
    j.key(Key::Escape);

    j.act("see the whole lantern again with F and click the pale body of the lamp, to change its colour");
    j.key(Key::F);
    j.tap(560.0, 650.0);
    j.look("clicked-the-body");
    let got = j.ed().map_or_else(String::new, |ed| {
        format!(
            "strokes or fills {:?}, shape {:?}, on {}",
            ed.picked.as_ref().map(|p| p.ids.clone()),
            ed.selection,
            active_name(&j)
        )
    });
    j.note(format!("the click took: {got}"));
    let body = mark_of(&j, "flats", 6);
    j.expect(
        "click-on-a-flat-colour-picks-it",
        active_name(&j) == "flats" && picked_ids(&j) == [6],
        "a click on the lamp's pale body picks that flat colour and goes to its layer",
    );

    j.act("give the body a warmer colour by typing a hex in the picker");
    j.press_hinted("the colour of the selected strokes");
    j.retype_leftmost("hex", "#e8c890");
    j.look("body-recoloured");
    let now_body = mark_of(&j, "flats", 6).map(|m| hex_of(&m));
    j.note(format!(
        "the body went from {:?} to {now_body:?}",
        body.as_ref().map(hex_of)
    ));
    j.expect(
        "hex-in-the-picker-recolours-a-fill",
        now_body.as_deref() == Some("#e8c890"),
        "a hex typed in the picker recolours the selected fill",
    );
    j.key(Key::Escape);
    j.key(Key::Escape);

    j.act("reshape the outline of the stone: A, click the line, drag one of its points up");
    j.click("lines");
    j.key(Key::A);
    let stone = free_spot(&layer_marks(&j, "lines"), 6);
    let was_stone = mark_of(&j, "lines", 6).and_then(|m| m.bounds());
    if let Some((x, y)) = stone {
        j.tap(x, y);
    }
    j.look("points");
    let anchors: Vec<[f32; 2]> = j
        .ed()
        .and_then(|ed| ed.shaped.as_ref())
        .map(|s| {
            klad_core::pathedit::anchors(&s.subs)
                .iter()
                .map(|(_, a)| a.at)
                .collect()
        })
        .unwrap_or_default();
    j.note(format!("the line shows {} points", anchors.len()));
    j.expect(
        "a-shows-the-points-of-a-stroke",
        anchors.len() > 2,
        "A and a click show the points of a brush stroke",
    );
    if let Some(top) = anchors.iter().min_by(|a, b| a[1].total_cmp(&b[1])) {
        let from = j.at(top[0], top[1]);
        j.drag_screen(
            from,
            from - vec2(0.0, 14.0),
            "the topmost point of the stone's outline 14 px up",
        );
        j.wait("the reshaped line", |ed| {
            ed.point_drag.is_none() && ed.path_commit.is_none()
        });
    }
    j.look("reshaped");
    let now_stone = mark_of(&j, "lines", 6).and_then(|m| m.bounds());
    j.note(format!("the outline's box went from {was_stone:?} to {now_stone:?}"));
    j.expect(
        "a-point-drag-reshapes-the-stroke",
        matches!((was_stone, now_stone), (Some(a), Some(b)) if b[1] < a[1] - 20.0),
        "dragging a point reshapes the stroke",
    );
    j.key(Key::Escape);

    j.act("put the grass right of the stone on a layer of its own: zoom in, S, and a loop around it");
    j.pinch(750.0, 1150.0, 2.0);
    j.key(Key::S);
    let count_before = layer_marks(&j, "lines").len();
    let all_before = j.ed().map_or(0, |ed| ed.scene.total_marks());
    let around = [
        (694.0, 1108.0),
        (806.0, 1108.0),
        (806.0, 1212.0),
        (694.0, 1212.0),
        (694.0, 1112.0),
    ];
    j.drag(&around);
    j.look("looped-the-grass");
    let grass: Vec<u32> = (28..=34).collect();
    let cut = |j: &Journey| {
        j.ed()
            .and_then(|ed| ed.picked.as_ref())
            .is_some_and(|p| p.cut.is_some())
    };
    let elsewhere = |j: &Journey| -> Vec<(String, usize)> {
        j.ed()
            .and_then(|ed| ed.picked.as_ref())
            .map(|p| p.more.iter().map(|part| (part.layer.clone(), part.ids.len())).collect())
            .unwrap_or_default()
    };
    j.note(format!(
        "the loop took {:?} on lines{}, and on other layers {:?}; the grass there is {grass:?}",
        picked_ids(&j),
        if cut(&j) {
            " with the parts of other strokes inside it"
        } else {
            ""
        },
        elsewhere(&j)
    ));
    j.expect(
        "a-loop-takes-the-grass",
        grass.iter().all(|id| picked_ids(&j).contains(id)),
        "a loop around the grass takes every blade",
    );

    j.act("the stone's edge came along: untick a lasso cuts strokes at its edge and loop again");
    j.key(Key::Escape);
    j.click("a lasso cuts strokes at its edge");
    j.drag(&around);
    j.look("looped-whole-strokes");
    let shading = elsewhere(&j);
    let taken = picked_ids(&j).len() + shading.iter().map(|(_, n)| n).sum::<usize>();
    j.note(format!(
        "the loop took {:?} on lines and on other layers {shading:?}: {taken} marks",
        picked_ids(&j)
    ));
    let mut whole = picked_ids(&j);
    whole.sort_unstable();
    j.expect(
        "a-loop-without-the-cut-takes-whole-strokes",
        whole == grass && !cut(&j),
        "with the cut off, the loop takes the strokes wholly inside it and nothing else on this layer",
    );

    j.act("move them: cmd X, a new paint layer, cmd V");
    let grass_box = box_of(&layer_marks(&j, "lines"), &grass);
    j.chord(CMD, Key::X);
    j.look("cut");
    let cut_away = layer_marks(&j, "lines").len() + grass.len() == count_before;
    j.expect("cmd-x-cuts", cut_away, "cmd X cuts what is picked");

    j.act("no cut: copy with cmd C, Delete, then a new paint layer and cmd V");
    j.chord(CMD, Key::C);
    j.key(Key::Delete);
    j.click("Layer");
    j.click("New paint layer");
    j.chord(CMD, Key::V);
    j.look("pasted-after-delete");
    let new_layer = active_name(&j);
    let all_now = j.ed().map_or(0, |ed| ed.scene.total_marks());
    j.note(format!(
        "the layer in hand is {new_layer} with {} marks; the drawing had {all_before} marks and has {all_now}",
        layer_marks(&j, &new_layer).len()
    ));
    j.expect(
        "copy-delete-paste-keeps-the-strokes",
        all_now == all_before,
        "what was copied can still be pasted after the originals are deleted",
    );

    j.act("take that back and do it the other way round: copy, a new layer, paste, and delete the originals after");
    (0..4).for_each(|_| {
        if j.ed().map_or(0, |ed| ed.scene.total_marks()) < all_before {
            j.chord(CMD, Key::Z);
        }
    });
    if active_name(&j) != "lines" {
        j.chord(CMD, Key::Z);
        j.click("lines");
    }
    j.key(Key::S);
    j.drag(&around);
    j.chord(CMD, Key::C);
    j.click("Layer");
    j.click("New paint layer");
    j.chord(CMD, Key::V);
    j.look("pasted-on-new-layer");
    let new_layer = active_name(&j);
    let pasted: Vec<Mark> = layer_marks(&j, &new_layer);
    let pasted_box = pasted
        .iter()
        .filter_map(|m| m.bounds())
        .reduce(|a, b| [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]);
    j.note(format!(
        "the layer in hand is {new_layer} with {} marks in {pasted_box:?}; the grass was in {grass_box:?}; lines has {} marks (it had {count_before}); the drawing has {} marks (it had {all_before})",
        pasted.len(),
        layer_marks(&j, "lines").len(),
        j.ed().map_or(0, |ed| ed.scene.total_marks())
    ));
    j.expect(
        "paste-lands-on-the-layer-in-hand",
        new_layer != "lines" && pasted.len() >= grass.len() && layer_marks(&j, "lines").len() == count_before,
        "strokes copied from several layers are pasted on the layer in hand",
    );

    j.act("the glow in the lamp's window is a shape: see everything with F, click it with V");
    j.key(Key::F);
    j.key(Key::V);
    j.click("all layers");
    j.tap(650.0, 655.0);
    j.look("clicked-the-glow");
    let shape = j.ed().and_then(|ed| ed.selection.clone());
    j.note(format!(
        "picked the shape {shape:?}; the layer in hand is {}; strokes picked {:?}",
        active_name(&j),
        picked_ids(&j)
    ));
    j.expect(
        "click-picks-a-shape-among-paint",
        shape.is_some(),
        "a click on the glow picks the shape",
    );
    j.expect(
        "picking-a-shape-goes-to-its-layer",
        active_name(&j) == "light",
        "the layer of the picked shape becomes the layer in hand, as it does for a stroke",
    );

    j.act("type a size for the glow: look for its number fields in Layer settings");
    let fields = ["x", "y", "rotate", "sx", "sy"];
    let seen = |j: &Journey| -> Vec<&str> { fields.iter().copied().filter(|f| j.in_sight(f)).collect() };
    j.look("number-fields");
    j.note(format!("number fields in sight: {:?}", seen(&j)));
    j.expect(
        "shape-number-fields-in-sight",
        seen(&j).len() == fields.len(),
        "the picked shape's position, turn and scale fields are in sight",
    );
    j.press_hinted("fold Colour to its title");
    j.press_hinted("fold Brush to its title");
    j.look("number-fields-after-folding");
    j.note(format!(
        "with Colour and Brush folded, number fields in sight: {:?}",
        seen(&j)
    ));
    j.expect(
        "shape-number-fields-after-folding",
        seen(&j).len() == fields.len(),
        "with two panels folded the fields are in sight",
    );

    j.act("make the glow warmer with the bar's fill button, typing a hex");
    let fill_of = |j: &Journey, id: &str| -> Option<String> {
        j.ed().and_then(|ed| match ed.scene.bundle.doc.item(id) {
            Some(klad_core::doc::Item::Node(_, _, node)) => Some(format!("{:?}", node.fill)),
            _ => None,
        })
    };
    let was_fill = shape.as_deref().and_then(|id| fill_of(&j, id));
    j.press_hinted("the shape's fill");
    j.look("glow-picker");
    j.retype_leftmost("hex", "#ffd9a0");
    j.look("glow-warmer");
    let now_fill = shape.as_deref().and_then(|id| fill_of(&j, id));
    j.note(format!("its fill went from {was_fill:?} to {now_fill:?}"));
    j.expect(
        "shape-fill-recolours",
        shape.is_some() && was_fill != now_fill,
        "the bar's fill button recolours the shape",
    );
    let soft = |fill: &Option<String>| {
        fill.as_ref()
            .is_some_and(|f| f.contains("Radial") || f.contains("Linear"))
    };
    j.expect(
        "recolouring-a-gradient-keeps-it-a-gradient",
        !soft(&was_fill) || soft(&now_fill),
        "a soft glow stays a soft glow when it is given another colour",
    );

    j.act("that made a flat disc of it: take it back with cmd Z");
    j.chord(CMD, Key::Z);
    j.look("one-undo");
    let after_one = shape.as_deref().and_then(|id| fill_of(&j, id));
    j.expect(
        "one-recolour-is-one-undo",
        after_one == was_fill,
        "one cmd Z takes back one recolour",
    );
    if after_one != was_fill {
        j.chord(CMD, Key::Z);
        j.look("second-undo");
        let after_two = shape.as_deref().and_then(|id| fill_of(&j, id));
        j.note(format!(
            "after a second cmd Z the glow is {}",
            if after_two == was_fill {
                "as it was"
            } else {
                "still changed"
            }
        ));
    }

    j.act("delete the glow and bring it back");
    j.key(Key::Escape);
    j.key(Key::Escape);
    j.tap(650.0, 655.0);
    j.key(Key::Delete);
    j.look("glow-deleted");
    let gone = shape.as_deref().is_some_and(|id| fill_of(&j, id).is_none());
    j.expect("delete-removes-the-shape", gone, "Delete removes the picked shape");
    j.chord(CMD, Key::Z);
    j.look("glow-back");
    let again = shape.as_deref().and_then(|id| fill_of(&j, id));
    j.expect(
        "undo-brings-the-shape-back",
        again.is_some() && again == was_fill,
        "cmd Z brings the shape back as it was",
    );

    j.finish(&[
        "cmd-x-cuts",
        "copy-delete-paste-keeps-the-strokes",
        "esc-closes-the-picker-first",
        "knob-turns-under-the-bar",
        "one-recolour-is-one-undo",
        "paste-lands-on-the-layer-in-hand",
        "picking-a-shape-goes-to-its-layer",
        "recolouring-a-gradient-keeps-it-a-gradient",
        "shape-number-fields-after-folding",
        "shape-number-fields-in-sight",
        "the-picker-leaves-the-selection-in-sight",
    ]);
}

fn tree(j: &Journey) -> Vec<String> {
    j.ed().map_or_else(Vec::new, |ed| {
        ed.layer_rows(super::layers::Where::Panel)
            .iter()
            .filter(|r| !r.mask)
            .map(|r| format!("{}{}", "  ".repeat(r.depth), r.name))
            .collect()
    })
}

fn names(j: &Journey) -> Vec<String> {
    tree(j).iter().map(|n| n.trim().to_string()).collect()
}

fn layer_box(j: &Journey, layer: &str) -> Option<[f32; 4]> {
    layer_marks(j, layer)
        .iter()
        .filter_map(|m| m.bounds())
        .reduce(|a, b| [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])])
}

fn shifted(a: Option<[f32; 4]>, b: Option<[f32; 4]>) -> Option<(f32, f32)> {
    match (a, b) {
        (Some(a), Some(b)) => Some((b[0] - a[0], b[1] - a[1])),
        _ => None,
    }
}

#[test]
fn someone_rearranges_the_layers_of_an_agents_drawing_and_transforms_parts() {
    let mut j = Journey::begin("edit-layers", WINDOW, Some("lantern.klad"));

    j.act("open the lantern and look for its layers in the Layers panel");
    j.look("opened");
    if j.ed().is_none() {
        return j.finish(&[]);
    }
    let all = names(&j);
    let seen = j.rows_in_sight(&all, Rows::Panel);
    j.note(format!("the drawing's layers, top first: {all:?}"));
    j.note(format!(
        "in sight in the Layers panel: {seen:?}; in the timeline: {:?}",
        j.rows_in_sight(&all, Rows::Timeline)
    ));
    j.expect(
        "layers-panel-shows-the-layers",
        seen.len() == all.len(),
        "the Layers panel shows the drawing's layers",
    );

    j.act("make room for them: fold Colour and Brush");
    j.press_hinted("fold Colour to its title");
    j.press_hinted("fold Brush to its title");
    j.look("two-folded");
    let seen = j.rows_in_sight(&all, Rows::Panel);
    j.note(format!("in sight in the Layers panel: {seen:?}"));
    j.expect(
        "layers-panel-grows-when-others-fold",
        seen.len() == all.len(),
        "with Colour and Brush folded the Layers panel shows every layer",
    );

    j.act("still cut off: drag the edge between Layers and Layer settings down");
    if let Some(title) = j.spot_of("fold Layer settings to its title") {
        let grip = egui::pos2(title.center().x + 120.0, title.center().y - 13.0);
        j.drag_screen(
            grip,
            grip + vec2(0.0, 170.0),
            "the top edge of the Layer settings title 170 px down",
        );
    }
    j.look("layers-taller");
    let seen = j.rows_in_sight(&all, Rows::Panel);
    j.note(format!("in sight in the Layers panel: {seen:?}"));
    j.expect(
        "the-edge-between-panels-drags",
        seen.len() == all.len(),
        "dragging the edge under the Layers panel makes room for every layer",
    );
    j.press_hinted("fold Layer settings to its title");
    j.look("three-folded");
    let seen = j.rows_in_sight(&all, Rows::Panel);
    j.note(format!(
        "with Layer settings folded as well, in sight in the Layers panel: {seen:?}"
    ));
    j.expect(
        "layers-panel-takes-the-room-of-a-folded-neighbour",
        seen.len() == all.len(),
        "with the panel under it folded too, the Layers panel shows every layer",
    );
    j.press_hinted("open Layer settings");
    let rows = Rows::Timeline;
    j.note("the timeline shows every layer: working with its rows from here");

    j.act("put the halo in front of the lines: take its row by the name and drag it above lines");
    let before = j.picture();
    j.drag_row_from("halo", "lines", 0.1, rows, true);
    j.look("halo-dragged-by-name");
    j.note(format!("the layers are now {:?}; it said: {}", names(&j), j.heard()));
    let by_name = names(&j) != all;
    j.expect(
        "a-row-drags-by-its-name",
        by_name,
        "a row taken by its name can be dragged",
    );
    if by_name {
        j.chord(CMD, Key::Z);
    }

    j.act("take the row beside its name and drag it above lines");
    j.drag_row("halo", "lines", 0.1, rows);
    j.look("halo-above-lines");
    let order = names(&j);
    j.note(format!("the layers are now {order:?}"));
    let above = order
        .iter()
        .position(|n| n == "halo")
        .zip(order.iter().position(|n| n == "lines"))
        .is_some_and(|(halo, lines)| halo + 1 == lines);
    j.expect("a-dragged-row-moves", above, "a row dragged above another lands there");
    let shown = j.picture();
    j.expect(
        "moving-a-layer-changes-the-picture",
        shown != before,
        "the picture shows the new order",
    );
    j.chord(CMD, Key::Z);
    j.look("halo-back");
    let shown = j.picture();
    j.expect(
        "undo-puts-the-row-back",
        names(&j) == all && shown == before,
        "cmd Z puts the layer back where it was",
    );

    j.act("put the two shading layers in a group: click shade, the group button, then drag rim into the group");
    j.click("shade");
    j.press_hinted("put the layer in a new group");
    j.look("shade-grouped");
    let grouped = tree(&j);
    j.note(format!("the layers are now {grouped:?}"));
    let group = names(&j).into_iter().find(|n| !all.contains(n)).unwrap_or_default();
    j.expect(
        "group-button-wraps-the-layer",
        !group.is_empty() && grouped.iter().any(|n| n == "  shade"),
        "the button puts the layer in a new group",
    );
    let shown = j.picture();
    j.expect(
        "grouping-keeps-the-picture",
        shown == before,
        "putting a layer in a group leaves the picture as it was",
    );
    j.drag_row("rim", &group, 0.5, rows);
    j.look("rim-in-group");
    let grouped = tree(&j);
    j.note(format!("the layers are now {grouped:?}"));
    j.expect(
        "a-row-dropped-on-a-group-goes-in",
        grouped.iter().any(|n| n == "  rim"),
        "a row dropped on the middle of a group goes into it",
    );
    let shown = j.picture();
    j.expect(
        "moving-into-the-group-keeps-the-picture",
        shown == before,
        "the shading still shows as it did, now from inside the group",
    );

    j.act("name the group: double click its name, type shading, Return");
    j.double_click(&group);
    j.type_text("shading");
    j.key(Key::Enter);
    j.look("group-named");
    j.expect(
        "double-click-renames",
        names(&j).iter().any(|n| n == "shading"),
        "a double click on the name renames the group",
    );
    let group = match names(&j).iter().any(|n| n == "shading") {
        true => "shading".to_string(),
        false => group,
    };

    j.act("hide the group with its eye, and show it again");
    let with = j.picture();
    j.press_hinted(&format!("show or hide {group}"));
    j.look("group-hidden");
    let shown = j.picture();
    j.expect(
        "the-eye-hides-a-group",
        shown != with,
        "the eye hides everything in the group",
    );
    j.press_hinted(&format!("show or hide {group}"));
    j.look("group-shown");
    let shown = j.picture();
    j.expect("the-eye-shows-it-again", shown == with, "and shows it again as it was");

    j.act("make the halo fainter: click halo, drag its opacity down, then choose another blend mode");
    j.click("halo");
    let layer = |j: &Journey, name: &str| j.ed().and_then(|ed| ed.scene.bundle.doc.find_layer(name).ok().cloned());
    let was = layer(&j, "halo");
    let field =
        j.h.query_all_by_label("opacity")
            .map(|n| n.rect())
            .find(|r| j.in_sight("opacity") && r.center().y < 200.0);
    match field {
        Some(r) => j.drag_screen(
            r.center(),
            r.center() - vec2(60.0, 0.0),
            "the opacity number 60 px to the left",
        ),
        None => j.note("no opacity field in sight"),
    }
    j.look("halo-fainter");
    let now = layer(&j, "halo");
    j.note(format!(
        "the halo's opacity went from {:?} to {:?}",
        was.as_ref().map(|l| l.opacity),
        now.as_ref().map(|l| l.opacity)
    ));
    j.expect(
        "opacity-drags",
        matches!((&was, &now), (Some(a), Some(b)) if b.opacity < a.opacity),
        "dragging the opacity number makes the layer fainter",
    );
    j.click("how the active layer mixes with what is below it");
    j.click("add");
    j.look("halo-add");
    let blended = layer(&j, "halo");
    j.expect(
        "blend-mode-changes",
        matches!((&now, &blended), (Some(a), Some(b)) if a.blend != b.blend),
        "the list beside the opacity changes the blend mode",
    );

    j.act("move the lantern to the left: go to lines, cmd T, drag inside the box");
    j.click("lines");
    j.chord(CMD, Key::T);
    j.look("transform-lines");
    let said = j.heard();
    j.note(format!("it said: {said}"));
    let lines_before = layer_box(&j, "lines");
    let flats_before = layer_box(&j, "flats");
    if let Some(c) = handle_corners(&j) {
        let middle = (c[0].to_vec2() + c[2].to_vec2()) / 2.0;
        j.drag_screen(
            middle.to_pos2(),
            middle.to_pos2() - vec2(60.0, 0.0),
            "from the middle of the layer's box 60 px to the left",
        );
    } else {
        j.note("no box around the layer");
    }
    j.look("lines-moved");
    let moved = shifted(lines_before, layer_box(&j, "lines"));
    let left = shifted(flats_before, layer_box(&j, "flats"));
    j.note(format!("lines moved by {moved:?}, flats by {left:?}"));
    j.expect(
        "cmd-t-moves-the-layer",
        moved.is_some_and(|(dx, _)| dx < -100.0),
        "cmd T and a drag inside the box move the layer",
    );
    j.expect(
        "the-lantern-moves-as-one",
        left == moved,
        "the colour under the lines moves with them",
    );
    j.key(Key::Escape);
    j.chord(CMD, Key::Z);

    j.act("the lantern is on several layers: put them all in one group, to move them together");
    j.click("flats");
    j.press_hinted("put the layer in a new group");
    let whole = names(&j)
        .into_iter()
        .find(|n| !all.contains(n) && *n != group)
        .unwrap_or_default();
    let dragged = ["lines", group.as_str(), "light", "halo"]
        .into_iter()
        .filter(|name| j.drag_row(name, &whole, 0.5, rows))
        .count();
    j.look("all-in-one-group");
    let grouped = tree(&j);
    j.note(format!("{dragged} rows dragged; the layers are now {grouped:?}"));
    let inside = ["lines", "light", "halo", "flats"]
        .iter()
        .all(|n| grouped.iter().any(|g| g.trim() == *n && g.starts_with("  ")));
    j.expect(
        "several-layers-go-into-one-group",
        inside,
        "five rows dragged one by one end up in the group",
    );

    j.act("move the whole lantern: click the group, cmd T, drag inside the box");
    j.click(&whole);
    j.chord(CMD, Key::T);
    j.look("transform-group");
    let lines_before = layer_box(&j, "lines");
    let flats_before = layer_box(&j, "flats");
    let shade_before = layer_box(&j, "shade");
    let pictured = j.picture();
    if let Some(c) = handle_corners(&j) {
        let middle = (c[0].to_vec2() + c[2].to_vec2()) / 2.0;
        j.drag_screen(
            middle.to_pos2(),
            middle.to_pos2() - vec2(60.0, 0.0),
            "from the middle of the group's box 60 px to the left",
        );
    } else {
        j.note("no box around the group");
    }
    j.look("group-moved");
    let by = [
        shifted(lines_before, layer_box(&j, "lines")),
        shifted(flats_before, layer_box(&j, "flats")),
        shifted(shade_before, layer_box(&j, "shade")),
    ];
    j.note(format!("lines, flats and shade moved by {by:?}"));
    j.expect(
        "cmd-t-on-a-group-moves-everything-in-it",
        by[0].is_some_and(|(dx, _)| dx < -100.0)
            && by.iter().all(
                |b| matches!((b, by[0]), (Some(b), Some(a)) if (b.0 - a.0).abs() < 0.5 && (b.1 - a.1).abs() < 0.5),
            ),
        "cmd T on a group moves every layer in it by the same amount",
    );

    j.act("make it smaller by a corner, then turn it by dragging outside the box");
    if let Some(c) = handle_corners(&j) {
        let centre = (c[0].to_vec2() + c[2].to_vec2()) / 2.0;
        let inward = (centre - c[2].to_vec2()).normalized() * 50.0;
        j.drag_screen(c[2], c[2] + inward, "the bottom right corner 50 px inwards");
    }
    j.look("group-smaller");
    let small = layer_box(&j, "lines");
    let width = |b: Option<[f32; 4]>| b.map_or(0.0, |b| b[2] - b[0]);
    j.note(format!("the box of lines is {small:?}"));
    j.expect(
        "a-corner-scales-the-group",
        width(small) < width(lines_before) * 0.95,
        "a corner of the box scales the group",
    );
    if let Some(c) = handle_corners(&j) {
        let centre = (c[0].to_vec2() + c[2].to_vec2()) / 2.0;
        let out = c[1] + (c[1].to_vec2() - centre).normalized() * 28.0;
        j.drag_screen(
            out,
            out + vec2(0.0, 60.0),
            "from just outside the top right corner 60 px down",
        );
    }
    j.look("group-dragged-outside");
    let turned = layer_box(&j, "lines");
    let still = j.ed().is_some_and(|ed| ed.shifting.is_some());
    j.note(format!(
        "the box of lines is {turned:?}; the transform is {}; it said: {}",
        if still { "still on" } else { "over" },
        j.heard()
    ));
    j.expect(
        "outside-the-box-turns-as-the-message-says",
        !near(small, turned, 1.0),
        "a drag from outside the box turns the group, as the message says",
    );

    j.act("that ended the transform: cmd T again and turn it by the knob above the box");
    if !still {
        j.chord(CMD, Key::T);
    }
    if let Some(c) = handle_corners(&j) {
        let top = c[0].lerp(c[1], 0.5);
        let centre = (c[0].to_vec2() + c[2].to_vec2()) / 2.0;
        let knob = top + (top.to_vec2() - centre).normalized() * 30.0;
        j.drag_screen(knob, knob + vec2(50.0, 10.0), "the knob above the box to the right");
    }
    j.look("group-turned");
    let turned = layer_box(&j, "lines");
    j.note(format!("the box of lines is {turned:?}"));
    j.expect(
        "the-knob-turns-the-group",
        !near(small, turned, 1.0),
        "the knob above the box turns the group",
    );
    j.key(Key::Escape);
    j.look("transform-done");
    j.chord(CMD, Key::Z);
    j.chord(CMD, Key::Z);
    j.chord(CMD, Key::Z);
    j.look("transforms-undone");
    let shown = j.picture();
    j.expect(
        "three-undos-take-the-transforms-back",
        near(lines_before, layer_box(&j, "lines"), 0.5) && shown == pictured,
        "three undos put the lantern back where it was",
    );

    j.act("throw the agent's hidden sketch away: click sketch, the bin, and take it back");
    if !j.click("sketch") {
        j.press_hinted(&format!("fold or open the group {group}"));
        j.press_hinted(&format!("fold or open the group {whole}"));
        j.note("folded the two groups in the timeline to bring the sketch's row in sight");
        j.click("sketch");
    }
    j.note(format!("the layer in hand is {}", active_name(&j)));
    j.press_hinted("delete the layer");
    j.look("sketch-deleted");
    j.note(format!("it said: {}", j.heard()));
    j.expect(
        "the-bin-deletes-the-layer",
        !names(&j).iter().any(|n| n == "sketch"),
        "the bin deletes the layer in hand",
    );
    j.chord(CMD, Key::Z);
    j.look("sketch-back");
    j.expect(
        "undo-brings-the-layer-back",
        names(&j).iter().any(|n| n == "sketch"),
        "cmd Z brings the layer back",
    );

    j.act("make a second halo: click halo and the duplicate button");
    j.click("halo");
    let count = names(&j).len();
    j.press_hinted("duplicate the layer");
    j.look("halo-twice");
    let after = tree(&j);
    j.note(format!(
        "the layers are now {after:?}; the layer in hand is {}",
        active_name(&j)
    ));
    j.expect(
        "duplicate-copies-the-layer",
        names(&j).len() == count + 1,
        "the duplicate button makes a copy of the layer",
    );

    j.finish(&[
        "a-row-drags-by-its-name",
        "grouping-keeps-the-picture",
        "layers-panel-grows-when-others-fold",
        "layers-panel-shows-the-layers",
        "moving-into-the-group-keeps-the-picture",
        "outside-the-box-turns-as-the-message-says",
        "the-edge-between-panels-drags",
        "the-lantern-moves-as-one",
    ]);
}

fn count_on(j: &Journey, layer: &str) -> usize {
    layer_marks(j, layer).len()
}

fn history(j: &mut Journey, take: usize) -> Vec<String> {
    j.ed_mut().map_or_else(Vec::new, |ed| {
        ed.history_rows()
            .iter()
            .take(take)
            .map(|r| format!("{}{} {}", if r.undone { "undone " } else { "" }, r.who, r.what))
            .collect()
    })
}

#[test]
fn someone_works_on_a_drawing_while_their_agent_edits_it() {
    let mut j = Journey::begin("edit-with-agent", WINDOW, Some("lantern.klad"));

    j.act("open the lantern and copy its path for the agent, from the File menu");
    j.look("opened");
    if j.ed().is_none() {
        return j.finish(&[]);
    }
    let drawing = j.drawing();
    j.click("File");
    let found = j.click("Copy path");
    j.look("path-copied");
    let copied = j.copied();
    j.note(format!(
        "on the clipboard: {:?}; it said: {}",
        copied
            .iter()
            .map(|c| c.replace(&drawing, "<the drawing's path>"))
            .collect::<Vec<_>>(),
        j.heard().replace(&drawing, "<the drawing's path>")
    ));
    j.expect(
        "copy-path-says-what-it-copied",
        found && j.heard().contains(&drawing),
        "File > Copy path says that it copied the drawing's path",
    );

    j.act("pin a note for the agent on the lamp: N, a click, the words, pin it");
    j.key(Key::N);
    j.tap(650.0, 640.0);
    j.look("note-open");
    j.type_text("make this glow warmer");
    j.click("pin it");
    j.look("note-pinned");
    let pins = j.ed().map_or(0, |ed| ed.pins.len());
    j.expect("a-note-pins", pins == 1, "the note is pinned on the canvas");
    let (_, read) = j.agent(&["notes", &drawing]);
    j.expect(
        "the-agent-reads-the-note",
        read.contains("make this glow warmer"),
        "klad notes gives the agent the note",
    );

    j.act("select two lines for the agent to look at: V, a click, a shift click");
    j.key(Key::V);
    let lines = layer_marks(&j, "lines");
    let spots: Vec<(u32, f32, f32)> = [4u32, 6]
        .iter()
        .filter_map(|id| free_spot(&lines, *id).map(|(x, y)| (*id, x, y)))
        .collect();
    if spots.len() == 2 {
        j.tap(spots[0].1, spots[0].2);
        j.tap_with(spots[1].1, spots[1].2, egui::PointerButton::Primary, Modifiers::SHIFT);
    }
    j.look("two-selected");
    let held = picked_ids(&j);
    j.note(format!("picked {held:?} on {}", active_name(&j)));
    let (_, read) = j.agent(&["selection", &drawing]);
    j.expect(
        "the-agent-reads-the-selection",
        held.len() == 2 && held.iter().all(|id| read.contains(&format!("s{id}"))),
        "klad selection gives the agent the two selected lines",
    );

    j.act("the agent paints a red line while the two lines are still selected");
    let before = count_on(&j, "lines");
    let last = j.heard();
    j.agent(&[
        "stroke",
        &drawing,
        "lines",
        "--brush",
        "ink",
        "--size",
        "14",
        "--color",
        "#d03030",
        "--path",
        "M 300 330 L 460 300",
    ]);
    let arrived = j.wait("the agent's line in the editor", |ed| {
        ed.scene
            .bundle
            .doc
            .find_layer("lines")
            .ok()
            .and_then(|l| l.content_at(ed.scene.frame))
            .is_some_and(|c| ed.scene.marks_of(&c.id).len() > 34)
    });
    j.look("agent-painted");
    let told = j.heard();
    j.note(format!(
        "lines has {} marks (it had {before}); the last message is: {told}",
        count_on(&j, "lines")
    ));
    j.expect(
        "the-agents-stroke-shows",
        arrived && count_on(&j, "lines") == before + 1,
        "the agent's line shows in the open editor",
    );
    j.expect(
        "the-selection-survives-an-agent-edit",
        picked_ids(&j) == held,
        "what was selected stays selected when the agent paints elsewhere",
    );
    j.expect(
        "an-agent-edit-is-announced",
        told != last,
        "a word on screen says the agent changed the drawing",
    );

    j.act("the agent marks the note as done");
    let id = j.ed().and_then(|ed| ed.pins.first().map(|n| n.id)).unwrap_or_default();
    j.agent(&["notes", &drawing, "--done", &id.to_string()]);
    let done = j.wait("the note marked done", |ed| ed.pins.iter().any(|n| n.done));
    j.look("note-done");
    j.expect(
        "the-pin-turns-done",
        done,
        "the pin on the canvas shows that the agent handled it",
    );

    j.act("paint a line yourself with B, then the agent paints another, then press cmd Z to take your own line back");
    j.key(Key::Escape);
    j.key(Key::B);
    j.click("lines");
    j.drag(&[(300.0, 420.0), (460.0, 400.0)]);
    j.settle();
    let mine = layer_marks(&j, "lines").last().map(|m| m.id());
    j.agent(&[
        "stroke",
        &drawing,
        "lines",
        "--color",
        "#3060c0",
        "--size",
        "14",
        "--path",
        "M 300 500 L 460 480",
    ]);
    j.wait("the agent's second line", |ed| {
        ed.scene
            .bundle
            .doc
            .find_layer("lines")
            .ok()
            .and_then(|l| l.content_at(ed.scene.frame))
            .is_some_and(|c| ed.scene.marks_of(&c.id).len() > 36)
    });
    j.look("both-painted");
    let theirs = layer_marks(&j, "lines").last().map(|m| m.id());
    j.chord(CMD, Key::Z);
    j.look("one-undo");
    let left: Vec<u32> = layer_marks(&j, "lines").iter().map(|m| m.id()).collect();
    let said = j.heard();
    j.note(format!(
        "my line is {mine:?}, the agent's {theirs:?}; after cmd Z the layer still has mine: {}, the agent's: {}; it said: {said}",
        mine.is_some_and(|id| left.contains(&id)),
        theirs.is_some_and(|id| left.contains(&id))
    ));
    j.expect(
        "undo-says-whose-step-it-took",
        said.contains("agent"),
        "cmd Z says that the step it took back was the agent's",
    );
    j.chord(CMD_SHIFT, Key::Z);

    j.act("look for the History panel, which says who did what");
    let title = j.hint_in_sight("open History") || j.hint_in_sight("fold History to its title");
    j.look("looking-for-history");
    j.expect(
        "history-panel-is-in-the-window",
        title,
        "the History panel, or its title, is in the window",
    );

    j.act("it is not there: try Window > History");
    j.click("Window");
    j.click("History");
    j.look("window-history");
    let listed = |j: &Journey| j.in_sight_containing("agent  stroke") && j.in_sight_containing("you  stroke");
    j.note(format!(
        "the history is {}; the panel is {}; it said: {}",
        if listed(&j) { "in sight" } else { "not in sight" },
        match j
            .ed()
            .map(|ed| ed.settings.layout.is_open(crate::layout::Pane::History))
        {
            Some(true) => "in the dock",
            _ => "closed",
        },
        j.heard()
    ));
    j.expect(
        "window-menu-brings-up-history-at-once",
        listed(&j),
        "Window > History shows the history",
    );

    j.act("nothing came up: choose Window > History once more");
    j.click("Window");
    j.click("History");
    j.look("history-open");
    let rows = history(&mut j, 6);
    j.note(format!("the history, newest first: {rows:?}; it said: {}", j.heard()));
    j.expect(
        "making-room-shows-the-history",
        listed(&j),
        "the room klad makes for the History panel shows its steps",
    );

    j.act("only its buttons show: fold Layer settings as well");
    j.press_hinted("fold Layer settings to its title");
    j.look("history-with-three-folded");
    j.expect(
        "history-shows-with-three-panels-folded",
        listed(&j),
        "with Colour, Brush and Layer settings folded the History panel shows its steps",
    );

    j.act("one step shows, cut off: fold Layers too");
    j.press_hinted("fold Layers to its title");
    j.look("history-with-room");
    j.expect(
        "history-names-you-and-the-agent",
        listed(&j),
        "the History panel names the agent's steps and yours",
    );
    j.press_hinted("open Layers");

    j.act("draw a long line while the agent paints in the middle of it");
    let before = count_on(&j, "lines");
    j.pen_down(300.0, 900.0);
    j.pen_move(360.0, 880.0);
    j.pen_move(420.0, 900.0);
    j.agent(&[
        "stroke",
        &drawing,
        "lines",
        "--color",
        "#30a050",
        "--size",
        "14",
        "--path",
        "M 300 980 L 460 960",
    ]);
    j.pen_move(480.0, 880.0);
    j.pen_move(540.0, 900.0);
    j.pen_up(540.0, 900.0);
    j.wait("both lines", |ed| {
        ed.scene
            .bundle
            .doc
            .find_layer("lines")
            .ok()
            .and_then(|l| l.content_at(ed.scene.frame))
            .is_some_and(|c| ed.scene.marks_of(&c.id).len() >= 39)
    });
    j.look("drawn-through-an-agent-edit");
    j.note(format!(
        "lines had {before} marks and has {}; the last message is: {}",
        count_on(&j, "lines"),
        j.heard()
    ));
    j.expect(
        "a-stroke-drawn-through-an-agent-edit-keeps-both",
        count_on(&j, "lines") == before + 2,
        "the line being drawn and the agent's line are both kept",
    );

    j.act("select a line with V; the agent deletes that very line; then press Delete");
    j.key(Key::V);
    let target = free_spot(&layer_marks(&j, "lines"), 8);
    if let Some((x, y)) = target {
        j.tap(x, y);
    }
    j.look("picked-for-the-agent");
    let picked_one = picked_ids(&j);
    j.note(format!("picked {picked_one:?}"));
    let cel = j.ed().and_then(|ed| ed.scene.active_cel()).unwrap_or_default();
    let address = picked_one.first().map(|id| format!("{cel}:s{id}")).unwrap_or_default();
    let before = count_on(&j, "lines");
    j.agent(&["rm", &drawing, &address]);
    j.wait("the line gone", |ed| {
        ed.scene
            .bundle
            .doc
            .find_layer("lines")
            .ok()
            .and_then(|l| l.content_at(ed.scene.frame))
            .is_some_and(|c| ed.scene.marks_of(&c.id).len() < 39)
    });
    j.look("agent-deleted-my-selection");
    j.note(format!(
        "lines had {before} marks and has {}; picked {:?}; the last message is: {}",
        count_on(&j, "lines"),
        picked_ids(&j),
        j.heard()
    ));
    j.expect(
        "a-selection-the-agent-removed-is-dropped",
        picked_ids(&j).is_empty(),
        "a line the agent deleted is no longer selected",
    );
    j.key(Key::Delete);
    j.look("delete-after");
    j.note(format!("Delete said: {}", j.heard()));
    j.expect(
        "delete-after-the-agent-removed-it-is-calm",
        count_on(&j, "lines") == before - 1,
        "Delete after that removes nothing else",
    );

    j.act("make a layer of your own and paint on it; the agent removes that layer");
    j.click("Layer");
    j.click("New paint layer");
    j.key(Key::B);
    j.drag(&[(820.0, 300.0), (900.0, 400.0)]);
    j.look("my-layer");
    let own = active_name(&j);
    let own_id = j.ed().and_then(|ed| ed.scene.active_id()).unwrap_or_default();
    j.agent(&["rm", &drawing, &own_id]);
    j.wait("the layer gone", |ed| ed.scene.bundle.doc.find_layer(&own_id).is_err());
    j.look("agent-removed-my-layer");
    j.note(format!(
        "the layer in hand was {own}; now it is {}; the last message is: {}",
        active_name(&j),
        j.heard()
    ));
    j.expect(
        "a-layer-the-agent-removed-leaves-a-layer-in-hand",
        active_name(&j) != own && active_name(&j) != "no layer",
        "after the agent removed the layer in hand, another layer is in hand",
    );
    let total = j.ed().map_or(0, |ed| ed.scene.total_marks());
    j.drag(&[(820.0, 500.0), (900.0, 600.0)]);
    j.look("painted-after");
    j.note(format!(
        "a stroke after that: the drawing had {total} marks and has {}, on {}; it said: {}",
        j.ed().map_or(0, |ed| ed.scene.total_marks()),
        active_name(&j),
        j.heard()
    ));
    j.expect(
        "painting-goes-on-after-the-layer-is-gone",
        j.ed().map_or(0, |ed| ed.scene.total_marks()) == total + 1,
        "the next stroke lands on the layer now in hand",
    );

    j.act("the agent undoes the last step, which was yours");
    let total = j.ed().map_or(0, |ed| ed.scene.total_marks());
    j.agent(&["undo", &drawing]);
    j.wait("the undo", |ed| ed.scene.total_marks() < total);
    j.look("agent-undid-mine");
    j.note(format!(
        "the drawing had {total} marks and has {}; the last message is: {}",
        j.ed().map_or(0, |ed| ed.scene.total_marks()),
        j.heard()
    ));
    j.chord(CMD_SHIFT, Key::Z);
    j.look("redone");
    j.expect(
        "redo-brings-back-what-the-agent-undid",
        j.ed().map_or(0, |ed| ed.scene.total_marks()) == total,
        "cmd shift Z brings back the stroke the agent's undo took",
    );

    j.act("paint right after the agent writes, before the editor has shown its line");
    let total = j.ed().map_or(0, |ed| ed.scene.total_marks());
    j.agent(&[
        "stroke",
        &drawing,
        "lines",
        "--color",
        "#a040a0",
        "--size",
        "14",
        "--path",
        "M 300 1040 L 460 1020",
    ]);
    j.drag(&[(820.0, 700.0), (900.0, 800.0)]);
    j.wait("both strokes", |ed| ed.scene.total_marks() >= total + 2);
    j.look("painted-right-after");
    j.note(format!(
        "the drawing had {total} marks and has {}; the last message is: {}",
        j.ed().map_or(0, |ed| ed.scene.total_marks()),
        j.heard()
    ));
    j.expect(
        "a-stroke-right-after-the-agents-write-keeps-both",
        j.ed().map_or(0, |ed| ed.scene.total_marks()) == total + 2,
        "the agent's line and the stroke painted right after it are both kept",
    );

    j.finish(&[
        "an-agent-edit-is-announced",
        "history-panel-is-in-the-window",
        "history-shows-with-three-panels-folded",
        "making-room-shows-the-history",
        "undo-says-whose-step-it-took",
        "window-menu-brings-up-history-at-once",
    ]);
}

const GODOT_CHECK: &str = r#"extends SceneTree

func _init() -> void:
	var packed: PackedScene = load("res://art/SCENE.tscn")
	if packed == null:
		print("FAILED the scene did not load")
		quit(1)
		return
	var node: Node = packed.instantiate()
	var sprite: AnimatedSprite2D = node as AnimatedSprite2D
	if sprite == null or sprite.sprite_frames == null:
		print("FAILED the scene's root is a ", node.get_class(), " without frames")
		node.free()
		quit(1)
		return
	var frames: SpriteFrames = sprite.sprite_frames
	for name in frames.get_animation_names():
		var length: float = 0.0
		for i in frames.get_frame_count(name):
			length += frames.get_frame_duration(name, i)
		print("ANIMATION ", name, " frames ", length, " fps ", frames.get_animation_speed(name), " loops ", frames.get_animation_loop(name))
	print("ROOT ", node.get_class(), " named ", node.name, " plays ", sprite.animation)
	node.free()
	quit(0)
"#;

fn godot(j: &mut Journey, project: &std::path::Path, scene: &str) -> Option<String> {
    let godot = std::env::var_os("KLAD_GODOT").filter(|g| !g.is_empty())?;
    std::fs::write(
        project.join("project.godot"),
        "config_version=5\n\n[application]\nconfig/name=\"check\"\n",
    )
    .ok()?;
    std::fs::write(project.join("check.gd"), GODOT_CHECK.replace("SCENE", scene)).ok()?;
    let run = |args: &[&str]| {
        std::process::Command::new(&godot)
            .arg("--headless")
            .arg("--path")
            .arg(project)
            .args(args)
            .output()
            .map(|out| String::from_utf8_lossy(&out.stdout).to_string())
            .unwrap_or_default()
    };
    run(&["--import"]);
    let said = run(&["--script", "res://check.gd"]);
    j.note("opened the folder as a Godot project and loaded the scene there");
    said.lines()
        .filter(|l| l.starts_with("ANIMATION") || l.starts_with("ROOT") || l.starts_with("FAILED"))
        .for_each(|l| j.note(format!("Godot: {l}")));
    Some(said)
}

fn files_in(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().to_string()))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

fn uids(dir: &std::path::Path) -> Vec<String> {
    files_in(dir)
        .iter()
        .filter(|n| n.ends_with(".tscn") || n.ends_with(".tres"))
        .flat_map(|n| {
            std::fs::read_to_string(dir.join(n))
                .unwrap_or_default()
                .split("uid=\"")
                .skip(1)
                .filter_map(|rest| rest.split('"').next().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn someone_exports_an_agents_animation_for_a_game() {
    let mut j = Journey::begin("export-for-a-game", WINDOW, Some("creature.klad"));

    j.act("open the creature an agent animated and choose File > Export > Godot…");
    j.look("opened");
    if j.ed().is_none() {
        return j.finish(&[]);
    }
    let home = j.folder.clone();
    let opened = j.menu("File", "Export", "Godot…");
    j.look("godot-window");
    j.expect(
        "the-godot-window-opens",
        opened && j.shown("Godot folder"),
        "File > Export > Godot… opens its window",
    );
    j.expect(
        "export-says-what-it-needs-first",
        j.shown("choose the Godot folder first"),
        "the window says that it needs a folder before it can export",
    );
    j.note(format!(
        "a Choose… button for the folder is {}",
        if j.shown("Choose…") {
            "there"
        } else {
            "not there in this build: the folder is typed"
        }
    ));

    j.act("give it a folder in the game, game/art, and export the animated sprite");
    j.type_into("Godot folder", "game/art");
    j.look("folder-typed");
    let place = j.shown("goes into");
    j.expect(
        "the-window-says-where-it-will-go",
        place,
        "the window says where the files will go",
    );
    j.click("Export");
    j.look("exported");
    let art = home.join("game").join("art");
    let written = files_in(&art);
    j.note(format!(
        "in game/art: {written:?}; it said: {}",
        j.heard().replace(&home.display().to_string(), "")
    ));
    j.expect(
        "the-sprite-is-written",
        ["creature.png", "creature.tres", "creature.tscn"]
            .iter()
            .all(|f| written.iter().any(|w| w == f)),
        "the sheet, the SpriteFrames and the scene are in the folder",
    );
    j.expect(
        "the-export-says-where-it-went",
        j.heard().contains("game/art") || j.heard().contains("game\\art"),
        "the message names the folder the files went to",
    );

    j.act("load it in Godot");
    match godot(&mut j, &home.join("game"), "creature") {
        Some(said) => {
            j.expect(
                "godot-loads-the-scene",
                said.contains("ROOT AnimatedSprite2D"),
                "Godot loads the scene as an animated sprite",
            );
            let clip = |name: &str, frames: u32| {
                said.lines().any(|l| {
                    l.starts_with(&format!("ANIMATION {name} frames {frames}"))
                        && l.contains("fps 12")
                        && l.contains("loops true")
                })
            };
            j.expect(
                "godot-has-both-clips",
                clip("idle", 12) && clip("walk", 16),
                "the sprite has the idle and walk animations at their length and speed",
            );
        }
        None => j.note("Godot was not given (KLAD_GODOT): the files were not loaded there"),
    }

    j.act("change the drawing and send it again with File > Export > Godot sprite, same folder");
    let ids = uids(&art);
    let sheet = std::fs::read(art.join("creature.png")).unwrap_or_default();
    let shut = |j: &mut Journey, window: &str| {
        let hint = format!("close {} {window}", crate::theme::icon::EXPORT);
        if j.hint_in_sight(&hint) {
            j.press_hinted(&hint);
        }
    };
    j.key(Key::Escape);
    j.look("after-esc");
    let open = j.shown("Godot folder");
    j.note(format!(
        "after Esc the Godot window is {}",
        if open { "still open" } else { "closed" }
    ));
    shut(&mut j, "Export for Godot");
    j.click("lines");
    j.key(Key::B);
    j.drag(&[(300.0, 300.0), (420.0, 360.0)]);
    j.settle();
    let again = j.menu("File", "Export", "Godot sprite, same folder");
    j.look("exported-again");
    j.note(format!(
        "it said: {}",
        j.heard().replace(&home.display().to_string(), "")
    ));
    let now = std::fs::read(art.join("creature.png")).unwrap_or_default();
    j.expect(
        "export-again-writes-the-change",
        again && !sheet.is_empty() && now != sheet,
        "the second export writes the changed picture",
    );
    j.note(format!("the ids in the files were {ids:?} and are {:?}", uids(&art)));
    j.expect(
        "export-again-keeps-godots-ids",
        !ids.is_empty() && ids.iter().all(|id| uids(&art).contains(id)),
        "the second export keeps the ids Godot knows the files by",
    );
    j.chord(CMD, Key::Z);

    j.act("a sprite sheet of the walk alone: File > Export > Sprite sheet…, the walk button, a folder, Export");
    j.menu("File", "Export", "Sprite sheet…");
    j.look("sheet-window");
    j.click("walk");
    j.retype("folder", "game/sheets");
    j.look("sheet-ready");
    j.click("Export");
    j.look("sheet-exported");
    let sheets = files_in(&home.join("game").join("sheets"));
    j.note(format!(
        "in game/sheets: {sheets:?}; it said: {}",
        j.heard().replace(&home.display().to_string(), "")
    ));
    j.expect(
        "a-sheet-with-its-json",
        sheets.iter().any(|f| f.ends_with(".png")) && sheets.iter().any(|f| f.ends_with(".json")),
        "the sheet and its JSON file are written",
    );
    let json = sheets
        .iter()
        .find(|f| f.ends_with(".json"))
        .and_then(|f| std::fs::read_to_string(home.join("game").join("sheets").join(f)).ok())
        .unwrap_or_default();
    j.expect(
        "the-sheet-holds-the-clip-asked-for",
        json.contains("\"walk\"") && !json.contains("\"idle\""),
        "the JSON names the walk and not the idle",
    );
    shut(&mut j, "Export sprite sheet");

    j.act("numbered frames of everything: File > Export > Frames…, a folder, Export");
    j.menu("File", "Export", "Frames…");
    j.look("frames-window");
    let field = |j: &Journey, label: &str| -> String {
        j.h.query_all_by_label(label)
            .filter(|n| n.accesskit_node().role() == egui::accesskit::Role::TextInput)
            .filter_map(|n| n.accesskit_node().value())
            .next()
            .unwrap_or_default()
    };
    let carried = (field(&j, "frames"), field(&j, "folder"));
    j.note(format!(
        "the window opens with frames \"{}\" and folder \"{}\"",
        carried.0, carried.1
    ));
    j.expect(
        "a-new-export-window-starts-with-all-frames",
        carried.0 == "all",
        "another kind of export starts with all frames, not with the clip chosen for the last one",
    );
    j.retype("frames", "all");
    j.retype("folder", "game/frames");
    j.look("frames-ready");
    j.click("Export");
    j.look("frames-exported");
    let frames_home = home.join("game").join("frames");
    let inside: Vec<String> = files_in(&frames_home)
        .iter()
        .flat_map(|name| {
            let sub = frames_home.join(name);
            match sub.is_dir() {
                true => files_in(&sub).into_iter().map(|f| format!("{name}/{f}")).collect(),
                false => vec![name.clone()],
            }
        })
        .collect();
    j.note(format!(
        "in game/frames: {} files, the first {:?}, the last {:?}; it said: {}",
        inside.len(),
        inside.first(),
        inside.last(),
        j.heard().replace(&home.display().to_string(), "")
    ));
    j.expect(
        "a-png-for-every-frame",
        inside.iter().filter(|f| f.ends_with(".png")).count() == 28,
        "the 28 frames are written as 28 numbered pictures",
    );
    shut(&mut j, "Export frames");

    j.act("the shadow alone as a picture: File > Export > Image…, the layer, a name, Export");
    j.menu("File", "Export", "Image… cmd E");
    j.look("image-window");
    j.click("export the whole image or one layer or group");
    j.click_leftmost("shadow");
    j.retype("folder", "game");
    j.retype("save as", "shadow");
    j.look("image-ready");
    let size = j.h.query_all_by_label_contains(" px").count();
    j.note(format!("lines on screen that give a size in px: {size}"));
    j.click("Export");
    j.look("image-exported");
    let game = files_in(&home.join("game"));
    j.note(format!(
        "in game: {game:?}; it said: {}",
        j.heard().replace(&home.display().to_string(), "")
    ));
    j.expect(
        "one-layer-as-a-picture",
        game.iter().any(|f| f.starts_with("shadow") && f.ends_with(".png")),
        "the layer is written under the name given",
    );
    shut(&mut j, "Export image");

    j.act("the sheet again, later: File > Export > Sprite sheet… and Export, without looking at the fields");
    let picture = std::fs::read(home.join("game").join("shadow.png")).unwrap_or_default();
    j.menu("File", "Export", "Sprite sheet…");
    j.look("sheet-window-again");
    let carried = (field(&j, "save as"), field(&j, "folder"), field(&j, "frames"));
    j.note(format!(
        "the window opens with save as \"{}\", folder \"{}\", frames \"{}\"",
        carried.0, carried.1, carried.2
    ));
    j.click("Export");
    j.look("sheet-over-the-picture");
    let after = std::fs::read(home.join("game").join("shadow.png")).unwrap_or_default();
    j.note(format!(
        "in game: {:?}; it said: {}",
        files_in(&home.join("game")),
        j.heard().replace(&home.display().to_string(), "")
    ));
    j.expect(
        "a-later-export-leaves-the-earlier-file",
        !picture.is_empty() && picture == after,
        "the picture exported before is still the picture, not overwritten by the sheet",
    );
    shut(&mut j, "Export sprite sheet");

    j.act("a folder that cannot be made, inside a file: File > Export > Image… into creature.klad/doc.json/out");
    j.menu("File", "Export", "Image… cmd E");
    j.retype("folder", "creature.klad/doc.json/out");
    j.click("Export");
    j.look("export-failed");
    let said = j.heard();
    j.note(format!("it said: {said}"));
    j.expect(
        "a-failed-export-says-so",
        !said.is_empty() && !said.starts_with("exported"),
        "an export that cannot be written says so",
    );
    j.expect(
        "a-failed-export-speaks-plainly",
        !said.contains("os error") && !said.contains("klad export") && !said.contains("--"),
        "the message is in plain words, without the system's error text or a command line",
    );
    shut(&mut j, "Export image");

    j.finish(&[
        "a-failed-export-speaks-plainly",
        "a-later-export-leaves-the-earlier-file",
        "a-new-export-window-starts-with-all-frames",
    ]);
}

fn start_error(j: &Journey) -> String {
    j.h.state()
        .start
        .as_ref()
        .and_then(|s| s.error.clone())
        .unwrap_or_default()
}

fn on_disk(path: &std::path::Path) -> Option<usize> {
    let bundle = klad_core::bundle::Bundle::open(path).ok()?;
    Some(
        bundle
            .doc
            .all_layers()
            .iter()
            .flat_map(|l| {
                l.cels()
                    .iter()
                    .map(|c| (l.id.clone(), c.id.clone()))
                    .collect::<Vec<_>>()
            })
            .filter_map(|(layer, cel)| bundle.read_marks(&layer, &cel).ok())
            .map(|marks| marks.len())
            .sum(),
    )
}

fn new_drawing(j: &mut Journey, name: &str) -> bool {
    j.click("Size");
    j.click("1024 x 1024  sprite");
    j.retype("Name", name);
    if j.ed().is_none() {
        j.click("Create");
    }
    j.settle();
    j.ed().is_some()
}

#[test]
fn someone_keeps_their_drawings_in_order() {
    let mut j = Journey::begin("files", WINDOW, None);
    let home = j.folder.clone();
    let shown = |text: String| text.replace(&format!("{}/", home.display()), "");

    j.act("klad starts: make a drawing called poster, paint two strokes, close it with cmd W");
    let made = new_drawing(&mut j, "poster");
    j.expect("a-drawing-is-made", made, "the new drawing opens");
    if !made {
        return j.finish(&[]);
    }
    j.drag(&[(200.0, 300.0), (500.0, 320.0)]);
    j.drag(&[(220.0, 420.0), (520.0, 400.0)]);
    j.look("poster");
    j.chord(CMD, Key::W);
    j.look("closed");
    j.expect(
        "a-closed-drawing-is-in-recent",
        j.ed().is_none() && j.shown("poster"),
        "after cmd W the drawing is among the recent ones",
    );
    j.expect(
        "closing-kept-the-strokes",
        on_disk(&home.join("poster.klad")) == Some(2),
        "the two strokes are in the drawing on disk",
    );

    j.act("make another drawing with the same name");
    j.retype("Name", "poster");
    j.look("same-name");
    let said = shown(start_error(&j));
    j.note(format!("it said: {said}"));
    j.expect(
        "a-name-in-use-is-refused",
        j.ed().is_none() && said.contains("already exists"),
        "a second drawing of the same name is refused, and the first is left alone",
    );

    j.act("call it sketchbook, paint a stroke, close it");
    let made = new_drawing(&mut j, "sketchbook");
    j.drag(&[(300.0, 600.0), (700.0, 620.0)]);
    j.look("sketchbook");
    j.chord(CMD, Key::W);
    j.look("two-recent");
    let place = |j: &Journey, name: &str| j.h.query_all_by_label(name).next().map(|n| n.rect().left());
    let order = (place(&j, "sketchbook"), place(&j, "poster"));
    j.note(format!(
        "in the recent row, sketchbook is at x {:?} and poster at x {:?}",
        order.0, order.1
    ));
    j.expect(
        "recent-lists-the-newest-first",
        made && matches!(order, (Some(a), Some(b)) if a < b),
        "the drawing closed last is the first of the recent ones",
    );

    j.act("open poster again from the recent row");
    j.click("poster");
    j.look("poster-again");
    j.expect(
        "recent-reopens-a-drawing",
        j.ed()
            .is_some_and(|ed| ed.scene.bundle.root.ends_with("poster.klad") && ed.scene.total_marks() == 2),
        "a click on its card opens the drawing as it was left",
    );

    j.act("go to the other drawing without closing this one: File > Open recent > sketchbook");
    let folder = home
        .file_name()
        .map_or_else(String::new, |f| f.to_string_lossy().to_string());
    let switched = j.menu("File", "Open recent", &format!("sketchbook  ({folder})"));
    j.look("switched");
    j.expect(
        "open-recent-switches-drawings",
        switched
            && j.ed()
                .is_some_and(|ed| ed.scene.bundle.root.ends_with("sketchbook.klad")),
        "File > Open recent switches to another drawing",
    );

    j.act("look at all of them over the open drawing, and go back: File > Open recent > All recent drawings…");
    j.menu("File", "Open recent", "All recent drawings…");
    j.look("recent-over-the-drawing");
    j.expect(
        "recent-shows-over-the-drawing",
        j.shown("Back to this drawing"),
        "the recent drawings show over the open one, with a way back",
    );
    j.click("Back to this drawing");
    j.look("back");
    j.expect(
        "back-keeps-the-drawing",
        j.ed()
            .is_some_and(|ed| ed.scene.bundle.root.ends_with("sketchbook.klad"))
            && !j.shown("Back to this drawing"),
        "Back to this drawing leaves the drawing open",
    );
    j.chord(CMD, Key::W);

    j.act("in the file browser, rename poster to poster final; then look at klad's recent row");
    std::fs::rename(home.join("poster.klad"), home.join("poster final.klad")).unwrap();
    j.note("renamed the folder poster.klad to poster final.klad outside klad");
    j.look("renamed-outside");
    j.expect(
        "a-renamed-drawing-shows-as-moved",
        j.shown("moved or deleted"),
        "the recent row says that poster was moved or deleted",
    );

    j.act("open it where it is now: paste its path under Open");
    let moved = home.join("poster final.klad").display().to_string();
    j.type_into("Or paste its path", &moved);
    j.click("Open");
    j.look("opened-by-path");
    j.expect(
        "a-pasted-path-opens",
        j.ed()
            .is_some_and(|ed| ed.scene.bundle.root.ends_with("poster final.klad") && ed.scene.total_marks() == 2),
        "the drawing opens from its new place with its strokes",
    );
    j.chord(CMD, Key::W);
    j.look("recent-after-the-move");
    let stale = j.shown("Remove poster");
    j.note(format!(
        "the old card is {}",
        if stale {
            "still there, with a Remove button"
        } else {
            "gone"
        }
    ));
    if stale {
        j.click("Remove poster");
        j.look("old-card-removed");
        j.expect(
            "remove-forgets-the-old-card",
            !j.shown("moved or deleted"),
            "Remove takes the old card out of the row",
        );
    }

    j.act("open sketchbook, paint, then rename its folder in the file browser while it is open, and paint again");
    j.click("sketchbook");
    j.settle();
    j.drag(&[(300.0, 200.0), (700.0, 220.0)]);
    j.settle();
    let before = j.ed().map_or(0, |ed| ed.scene.total_marks());
    let renamed = std::fs::rename(home.join("sketchbook.klad"), home.join("sketchbook moved.klad")).is_ok();
    j.note(match renamed {
        true => "renamed the folder sketchbook.klad to sketchbook moved.klad outside klad",
        false => "the system refused to rename the folder of an open drawing: nothing to see here",
    });
    j.h.run_steps(6);
    j.look("moved-under-the-editor");
    j.note(format!("after the rename it said: {}", shown(j.heard())));
    j.drag(&[(300.0, 300.0), (700.0, 320.0)]);
    j.look("painted-after-the-move");
    let said = shown(j.heard());
    let shows = j.ed().map_or(0, |ed| ed.scene.total_marks());
    let there = on_disk(&home.join("sketchbook moved.klad"));
    let old = on_disk(&home.join("sketchbook.klad"));
    j.note(format!(
        "the editor shows {shows} strokes (it had {before}); on disk sketchbook moved.klad has {there:?} and sketchbook.klad has {old:?}; in the folder: {:?}; it said: {said}",
        files_in(&home)
    ));
    let inside = |name: &str| -> Vec<String> {
        let root = home.join(name);
        let mut found: Vec<String> = files_in(&root)
            .into_iter()
            .filter(|f| f != ".cache")
            .flat_map(|f| match root.join(&f).is_dir() {
                true => files_in(&root.join(&f))
                    .into_iter()
                    .map(|g| format!("{f}/{g}"))
                    .collect(),
                false => vec![f],
            })
            .collect();
        found.sort();
        found
    };
    j.note(format!(
        "sketchbook moved.klad holds {:?}; the sketchbook.klad that came back holds {:?}",
        inside("sketchbook moved.klad"),
        inside("sketchbook.klad")
    ));
    if renamed {
        j.expect(
            "a-stroke-after-the-move-is-kept-or-refused-aloud",
            there == Some(shows) || (shows == before && !said.is_empty()),
            "the stroke painted after the move is in the renamed drawing, or the editor says that it could not keep it",
        );
        j.expect(
            "the-move-leaves-no-second-drawing",
            old.is_none() && !home.join("sketchbook.klad").exists(),
            "no second drawing appears at the old place",
        );
    }
    j.chord(CMD, Key::W);
    j.look("closed-after-the-move");

    j.act("put an older copy back under the editor, as a sync tool or git would");
    let third = new_drawing(&mut j, "roll");
    j.drag(&[(200.0, 200.0), (600.0, 220.0)]);
    j.settle();
    let roll = home.join("roll.klad");
    let backup = home.join("backup of roll");
    super::testkit::copy_dir(&roll, &backup);
    j.note("kept a copy of the drawing with one stroke");
    j.drag(&[(200.0, 400.0), (600.0, 420.0)]);
    j.drag(&[(200.0, 600.0), (600.0, 620.0)]);
    j.look("three-strokes");
    super::testkit::copy_dir(&backup, &roll);
    j.note("put that copy back over the drawing's files");
    j.wait("the older copy in the editor", |ed| ed.scene.total_marks() == 1);
    j.look("older-copy-back");
    let shows = j.ed().map_or(0, |ed| ed.scene.total_marks());
    j.note(format!(
        "the editor shows {shows} strokes, the files hold {:?}; it said: {}",
        on_disk(&roll),
        shown(j.heard())
    ));
    j.expect(
        "the-editor-follows-the-files",
        third && on_disk(&roll) == Some(shows),
        "the editor shows what the files hold after they were put back",
    );
    j.drag(&[(200.0, 800.0), (600.0, 820.0)]);
    j.look("painted-on-the-older-copy");
    let shows = j.ed().map_or(0, |ed| ed.scene.total_marks());
    j.note(format!(
        "after one more stroke the editor shows {shows}, the files hold {:?}; it said: {}",
        on_disk(&roll),
        shown(j.heard())
    ));
    j.expect(
        "painting-goes-on-from-the-older-copy",
        on_disk(&roll) == Some(shows),
        "the next stroke is saved on top of what the files hold",
    );
    j.chord(CMD, Key::S);
    j.look("cmd-s");
    j.note(format!("cmd S said: {}", shown(j.heard())));
    j.chord(CMD, Key::W);

    j.act("a drawing whose file is damaged: open it from the recent row");
    std::fs::write(roll.join("doc.json"), "{ \"version\": ").unwrap();
    j.note("cut the drawing's doc.json short outside klad");
    j.click("roll");
    j.look("damaged");
    let said = shown(start_error(&j));
    j.note(format!("it said: {said}"));
    j.expect(
        "a-damaged-drawing-is-not-opened",
        j.ed().is_none() && !said.is_empty(),
        "klad stays in the start window and says something",
    );
    j.expect(
        "a-damaged-drawing-is-explained",
        !said.contains("EOF") && !said.contains("line 1") && !said.contains("column"),
        "the message says in plain words that the drawing is damaged and what to do",
    );

    j.act("give Open the path of a picture");
    let (_, rendered) = j.agent(&["render", &home.join("poster final.klad").display().to_string()]);
    let picture = rendered
        .lines()
        .find(|l| l.trim().ends_with(".png"))
        .map(|l| l.trim().to_string())
        .unwrap_or_default();
    j.retype("Or paste its path", &format!("{}/{picture}", home.display()));
    j.click("Open");
    j.look("picture-to-open");
    let said = shown(start_error(&j));
    j.note(format!("it said: {said}"));
    j.expect(
        "a-picture-given-to-open-says-how-to-bring-it-in",
        said.contains("Import") || said.contains("import") || said.contains("drop"),
        "the refusal says how a picture comes into klad",
    );

    let moved_open: &[&str] = match renamed {
        true => &[
            "a-stroke-after-the-move-is-kept-or-refused-aloud",
            "the-move-leaves-no-second-drawing",
        ],
        false => &[],
    };
    let known: Vec<&str> = [
        "a-damaged-drawing-is-explained",
        "a-picture-given-to-open-says-how-to-bring-it-in",
    ]
    .iter()
    .chain(moved_open)
    .copied()
    .collect();
    j.finish(&known);
}

fn finder_offers(query: &str) -> Vec<&'static str> {
    crate::commands::rank(query, &[])
        .into_iter()
        .filter(|c| !crate::commands::DEVELOPER.contains(c))
        .take(3)
        .map(|c| c.spec().name)
        .collect()
}

#[test]
fn someone_looks_for_help_and_finds_their_way_back() {
    let mut j = Journey::begin("help-and-recovery", WINDOW, Some("lantern.klad"));

    j.act("open the lantern and look at Help > First steps");
    j.look("opened");
    if j.ed().is_none() {
        return j.finish(&[]);
    }
    j.click("Help");
    let found = j.click_containing("First steps");
    j.look("first-steps");
    j.expect(
        "first-steps-open",
        found && j.shown("copy a request"),
        "Help > First steps opens its window",
    );
    j.press_hinted(&format!("close {} First steps", crate::theme::icon::INFO));

    j.act("look for a command by typing what it does: cmd K, export");
    j.chord(CMD, Key::K);
    let field = format!("{} Find a command", crate::theme::icon::SEARCH);
    j.type_into(&field, "export");
    j.look("finder-export");
    let offers = finder_offers("export");
    j.note(format!("for \"export\" the finder offers first: {offers:?}"));
    j.expect(
        "the-finder-finds-a-command-by-its-name",
        offers.first().is_some_and(|name| name.contains("export")),
        "typing export offers the export commands",
    );

    j.act("type the words people bring from other apps");
    let words = [
        "resize",
        "canvas size",
        "crop",
        "merge",
        "save as",
        "zoom in",
        "deselect",
        "preferences",
        "shortcuts",
        "rotate",
        "mirror",
        "transparency",
        "new layer",
        "rename",
        "help",
    ];
    let offered: Vec<(&str, Vec<&str>)> = words.iter().map(|w| (*w, finder_offers(w))).collect();
    offered
        .iter()
        .for_each(|(word, offers)| j.note(format!("\"{word}\": {offers:?}")));
    j.key(Key::Escape);
    j.chord(CMD, Key::K);
    j.type_into(&field, "canvas size");
    j.look("finder-canvas-size");
    j.expect(
        "the-finder-says-when-it-has-nothing",
        j.shown("nothing matches"),
        "a word no command answers to says that nothing matches",
    );
    let fitting = [
        ("deselect", "clear the selection"),
        ("preferences", "settings"),
        ("shortcuts", "every key"),
        ("rotate", "turn"),
        ("help", "first steps"),
    ];
    let missed: Vec<&str> = fitting
        .iter()
        .filter(|(word, name)| {
            !crate::commands::rank(word, &[])
                .iter()
                .take(5)
                .any(|c| c.spec().name.contains(name))
        })
        .map(|(word, _)| *word)
        .collect();
    j.note(format!(
        "words for things klad has under another name that do not lead to them: {missed:?}"
    ));
    j.expect(
        "the-finder-knows-the-words-people-bring",
        missed.is_empty(),
        "a usual word for something klad has leads to it",
    );

    j.act("type deselect and press Return, with two lines selected");
    j.key(Key::Escape);
    j.key(Key::V);
    let lines = layer_marks(&j, "lines");
    if let (Some(a), Some(b)) = (free_spot(&lines, 4), free_spot(&lines, 6)) {
        j.tap(a.0, a.1);
        j.tap_with(b.0, b.1, egui::PointerButton::Primary, Modifiers::SHIFT);
    }
    let before = count_on(&j, "lines");
    let held = picked_ids(&j).len();
    j.chord(CMD, Key::K);
    j.type_into(&field, "deselect");
    j.look("finder-deselect");
    j.key(Key::Enter);
    j.look("deselect-run");
    j.note(format!(
        "{held} lines were selected and lines had {before} marks; now {} are selected and it has {}; it said: {}",
        picked_ids(&j).len(),
        count_on(&j, "lines"),
        j.heard()
    ));
    j.expect(
        "deselect-in-the-finder-does-not-delete",
        count_on(&j, "lines") == before,
        "typing deselect and Return does not delete what is selected",
    );
    if count_on(&j, "lines") != before {
        j.chord(CMD, Key::Z);
    }
    j.key(Key::Escape);

    j.act("look up the keys: Help > Keyboard shortcuts, and type undo in its field");
    j.click("Help");
    let keys = j.click_containing("Keyboard shortcuts");
    j.look("keys");
    let list = j.h.ctx.memory(|m| m.area_rect(egui::Id::unique("palette")));
    j.note(format!(
        "the list of every key is the command finder with nothing typed; its window is {:.0} px high",
        list.map_or(0.0, |r| r.height())
    ));
    j.expect(
        "the-key-list-opens",
        keys && j.shown("every key"),
        "Help > Keyboard shortcuts opens the list of keys",
    );
    j.expect(
        "the-key-list-shows-its-rows",
        list.is_some_and(|r| r.height() > 300.0),
        "the list shows more than a few of its two hundred rows at a time",
    );
    j.key(Key::Escape);

    j.act("fold a panel, then put everything back from the settings: cmd comma, Layout, reset the layout");
    j.press_hinted("fold Colour to its title");
    j.chord(CMD, Key::Comma);
    j.look("settings");
    j.click_containing(" Layout");
    j.look("settings-layout");
    j.click("reset the layout");
    j.look("layout-reset");
    let unfolded = j
        .ed()
        .and_then(|ed| ed.settings.layout.pane(crate::layout::Pane::Tool).map(|p| !p.collapsed));
    j.expect(
        "reset-the-layout-unfolds",
        unfolded == Some(true),
        "reset the layout opens the folded panel again",
    );
    j.key(Key::Escape);

    j.act("close the Layers panel by accident and get it back from the Window menu");
    j.press_hinted("close Layers; cmd K brings it back");
    j.look("layers-closed");
    j.click("Window");
    j.click("Layers");
    j.look("layers-back");
    j.expect(
        "the-window-menu-brings-a-panel-back",
        j.ed()
            .is_some_and(|ed| ed.settings.layout.is_open(crate::layout::Pane::Layers)),
        "Window > Layers brings the closed panel back",
    );

    j.act("hide everything with Window > Hide panels, then look for the way back");
    j.click("Window");
    j.click_containing("Hide panels");
    j.look("panels-hidden");
    let bar = j.in_sight("Window");
    j.note(format!(
        "with the panels hidden the menu bar is {}; the status line says: {}",
        if bar { "still there" } else { "gone" },
        j.heard()
    ));
    j.expect(
        "hidden-panels-leave-a-way-back-in-sight",
        bar || j.shown("Tab"),
        "something on screen says how to get the panels back",
    );
    if bar {
        j.click("Window");
        j.click_containing("Hide panels");
    } else {
        j.chord(CMD, Key::K);
        j.type_into(&field, "hide the panels");
        j.key(Key::Enter);
    }
    j.look("panels-back");
    j.expect(
        "the-panels-come-back",
        j.in_sight("Window") && j.ed().is_some_and(|ed| !ed.settings.layout.hidden),
        "the panels come back",
    );

    j.act("lose the drawing: zoom far in and scroll away, then press F");
    j.pinch(512.0, 640.0, 8.0);
    j.wheel(512.0, 640.0, vec2(0.0, -4000.0), Modifiers::NONE);
    j.look("lost");
    let middle = j.at(512.0, 640.0);
    let lost = j.ed().is_some_and(|ed| !ed.canvas_rect.contains(middle));
    j.note(format!(
        "the middle of the drawing is {} the canvas area, zoom {:.2}",
        if lost { "outside" } else { "inside" },
        zoom(&j)
    ));
    j.key(Key::F);
    j.look("found-again");
    let middle = j.at(512.0, 640.0);
    j.expect(
        "f-brings-the-drawing-back",
        j.ed().is_some_and(|ed| ed.canvas_rect.contains(middle)) && zoom(&j) < 1.0,
        "F shows the whole drawing again",
    );

    j.act("make three mistakes, then read them back in Window > Message log");
    j.key(Key::Escape);
    j.key(Key::Delete);
    j.chord(CMD, Key::V);
    j.key(Key::ArrowLeft);
    j.look("mistakes");
    j.click("Window");
    j.click_containing("Message log");
    j.look("message-log");
    let kept = ["select a shape or strokes with V first", "nothing is copied yet"]
        .iter()
        .all(|text| j.h.query_all_by_label_contains(text).count() > 0);
    j.expect(
        "the-message-log-keeps-what-was-said",
        kept,
        "the message log lists what klad said",
    );
    j.press_hinted(&format!("close {} Messages", crate::theme::icon::LOG));

    j.act("something goes wrong: an export into a folder that cannot be made; then Help > Report a problem…");
    j.menu("File", "Export", "Image… cmd E");
    j.retype("folder", "lantern.klad/doc.json/out");
    j.click("Export");
    j.settle();
    let error = j.heard();
    j.press_hinted(&format!("close {} Export image", crate::theme::icon::EXPORT));
    j.click("Help");
    let report = j.click_containing("Report a problem");
    j.look("report");
    let holds = j.h.query_all_by_label_contains("could not be made").count() > 0
        || j.h
            .query_all(
                egui_kittest::kittest::By::new()
                    .predicate(|n| n.value().is_some_and(|v| v.contains("could not be made"))),
            )
            .count()
            > 0;
    j.note(format!("the error was: {error}"));
    j.expect(
        "the-report-holds-the-error",
        report && holds,
        "the report holds the error that was just shown",
    );
    j.click("email it to support");
    j.look("report-emailed");
    let sent = j.ed().and_then(|ed| ed.opened.last().cloned()).unwrap_or_default();
    j.note(format!("it opened: {}", sent.chars().take(80).collect::<String>()));
    j.expect(
        "the-report-goes-to-an-email",
        sent.starts_with("mailto:"),
        "email it to support opens an email",
    );
    j.press_hinted(&format!("close {} Report a problem", crate::theme::icon::LOG));

    j.act("the other ways to reach help: Help > Guides, What's new, Contact support, About");
    let links: Vec<String> = ["Guides", "What's new", "Contact support"]
        .iter()
        .map(|item| {
            j.click("Help");
            j.click_containing(item);
            j.ed().and_then(|ed| ed.opened.last().cloned()).unwrap_or_default()
        })
        .collect();
    j.note(format!(
        "they open: {:?}",
        links
            .iter()
            .map(|l| l.chars().take(60).collect::<String>())
            .collect::<Vec<_>>()
    ));
    j.expect(
        "help-links-lead-somewhere",
        links[0].starts_with("https://") && links[1].starts_with("https://") && links[2].starts_with("mailto:"),
        "Guides and What's new open the site, Contact support an email",
    );
    j.click("Help");
    j.click_containing("About klad");
    j.look("about");
    j.expect(
        "about-shows-the-version",
        j.h.query_all_by_label_contains(crate::VERSION).count() > 0,
        "About shows the version",
    );
    j.press_hinted(&format!("close {} About klad", crate::theme::icon::INFO));

    j.act("leave and come back: go to flats with V, close with cmd W, open it again from the recent row");
    j.click("flats");
    j.key(Key::V);
    j.pinch(512.0, 640.0, 2.0);
    let left = (active_name(&j), j.ed().map(|ed| format!("{:?}", ed.tool)), zoom(&j));
    j.chord(CMD, Key::W);
    j.look("closed");
    j.click("lantern");
    j.look("reopened");
    let back = (active_name(&j), j.ed().map(|ed| format!("{:?}", ed.tool)), zoom(&j));
    j.note(format!(
        "left on {} with {:?} at zoom {:.2}; came back on {} with {:?} at zoom {:.2}",
        left.0, left.1, left.2, back.0, back.1, back.2
    ));
    j.expect(
        "reopening-opens-the-drawing",
        j.ed().is_some(),
        "the drawing opens again from the recent row",
    );

    j.finish(&[
        "deselect-in-the-finder-does-not-delete",
        "hidden-panels-leave-a-way-back-in-sight",
        "the-finder-knows-the-words-people-bring",
        "the-key-list-shows-its-rows",
    ]);
}
