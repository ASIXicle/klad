use super::testkit::Journey;
use super::{Editor, Tool};
use egui::{Key, Modifiers, vec2};
use egui_kittest::kittest::{NodeT, Queryable};

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
