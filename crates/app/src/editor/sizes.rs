use super::export::What;
use super::prefs::Section;
use super::testkit::{Journey, written};
use super::{Editor, Windows};
use crate::commands::Cmd;
use crate::layout::Pane;
use crate::settings::Settings;
use egui::{Key, Modifiers, Pos2, Rect, pos2, vec2};
use egui_kittest::kittest::{By, NodeT, Queryable};
use std::collections::BTreeMap;

const PANES: [(Pane, Cmd); 8] = [
    (Pane::Tool, Cmd::ToolPane),
    (Pane::Brush, Cmd::BrushPane),
    (Pane::Layers, Cmd::LayersPane),
    (Pane::Inspector, Cmd::InspectorPane),
    (Pane::Notes, Cmd::NotesPane),
    (Pane::History, Cmd::HistoryPane),
    (Pane::Preview, Cmd::PreviewPane),
    (Pane::Maps, Cmd::MapsPane),
];

const TOOLS: [(Key, &str); 11] = [
    (Key::B, "brush"),
    (Key::E, "eraser"),
    (Key::G, "fill"),
    (Key::S, "select"),
    (Key::V, "select and move"),
    (Key::A, "reshape"),
    (Key::P, "pen"),
    (Key::U, "shapes"),
    (Key::T, "text"),
    (Key::J, "bones"),
    (Key::N, "notes"),
];

const MENUS: [&str; 9] = [
    "File",
    "Edit",
    "Tools",
    "Layer",
    "Frame",
    "Animation",
    "View",
    "Window",
    "Help",
];

const OPENED: [(Cmd, &str); 14] = [
    (Cmd::BrushWindow, "edit the brush"),
    (Cmd::MessageLog, "message log"),
    (Cmd::OnionSettings, "onion skin settings"),
    (Cmd::Link, "link a drawing here"),
    (Cmd::ClipSettings, "clip settings"),
    (Cmd::ImportPicture, "import a picture"),
    (Cmd::Pixelize, "pixels of a picture"),
    (Cmd::LoadPalette, "load a palette"),
    (Cmd::TileSize, "cut into tiles"),
    (Cmd::About, "about"),
    (Cmd::ReportProblem, "report a problem"),
    (Cmd::AddFont, "add a font"),
    (Cmd::PaintOver, "paint over"),
    (Cmd::FirstSteps, "first steps"),
];

const EXPORTS: [(What, &str); 6] = [
    (What::Image, "export an image"),
    (What::Frames, "export frames"),
    (What::Sheet, "export a sprite sheet"),
    (What::Gif, "export a GIF"),
    (What::Aseprite, "export an Aseprite file"),
    (What::Godot, "export for Godot"),
];

struct Part {
    rect: Rect,
    seen: Rect,
    hint: String,
}

struct Row {
    state: String,
    kind: &'static str,
    count: usize,
    of: usize,
    detail: String,
}

struct Zones {
    strip: Vec<usize>,
    dock: Vec<usize>,
    options: Vec<usize>,
    lower: Vec<usize>,
    titles: Vec<(usize, Pane, bool)>,
    left: f32,
    bottom: f32,
}

fn area(r: Rect) -> f32 {
    r.width().max(0.0) * r.height().max(0.0)
}

fn share(part: &Part, window: Rect) -> f32 {
    match area(part.rect) {
        whole if whole > 0.0 => area(part.seen.intersect(window)) / whole,
        _ => 0.0,
    }
}

fn sideways(part: &Part, window: Rect) -> bool {
    let seen = part.seen.intersect(window);
    seen.height() > 2.0 && seen.width() > 0.0 && seen.width() < part.rect.width() - 2.0
}

fn short(hint: &str) -> String {
    let line: String = hint.split_whitespace().collect::<Vec<_>>().join(" ");
    match line.char_indices().nth(46) {
        Some((at, _)) => format!("{}…", &line[..at]),
        None => line,
    }
}

fn listed(names: &[String]) -> String {
    let mut seen = names.to_vec();
    seen.dedup();
    let more = seen.len().saturating_sub(6);
    let head = seen.iter().take(6).cloned().collect::<Vec<_>>().join("; ");
    match more {
        0 => head,
        n => format!("{head}; and {n} more"),
    }
}

fn parts(j: &Journey) -> Vec<Part> {
    let all: Vec<Part> = crate::theme::hint_parts(&j.h.ctx)
        .into_iter()
        .map(|(rect, seen, hint)| Part { rect, seen, hint })
        .collect();
    let again: Vec<bool> = all
        .iter()
        .enumerate()
        .map(|(i, a)| {
            all[i + 1..]
                .iter()
                .any(|b| b.hint == a.hint && area(a.rect.intersect(b.rect)) > 0.5 * area(a.rect).min(area(b.rect)))
        })
        .collect();
    all.into_iter()
        .zip(again)
        .filter(|(_, again)| !again)
        .map(|(part, _)| part)
        .collect()
}

fn canvas(j: &Journey) -> Rect {
    j.ed().map_or(Rect::NOTHING, |ed| ed.canvas_rect)
}

fn title_of(hint: &str) -> Option<(Pane, bool)> {
    PANES.iter().find_map(|(pane, _)| {
        let title = pane.title();
        match hint {
            h if h == format!("fold {title} to its title") => Some((*pane, false)),
            h if h == format!("open {title}") => Some((*pane, true)),
            _ => None,
        }
    })
}

fn zones(j: &Journey, all: &[Part]) -> Option<Zones> {
    let canvas = canvas(j);
    let titles: Vec<(usize, Pane, bool)> = all
        .iter()
        .enumerate()
        .filter_map(|(i, p)| title_of(&p.hint).map(|(pane, folded)| (i, pane, folded)))
        .collect();
    let first = titles.first().map(|t| t.0)?;
    let left = titles
        .iter()
        .map(|t| all[t.0].rect.left())
        .fold(f32::INFINITY, f32::min)
        - 4.0;
    let end = (first..all.len())
        .find(|i| all[*i].rect.left() < left - 2.0)
        .unwrap_or(all.len());
    let low = (0..first)
        .find(|i| all[*i].rect.top() > canvas.bottom() - 1.0)
        .unwrap_or(0);
    let strip_start = (low..first)
        .find(|i| all[*i].rect.right() <= 50.0 && all[*i].rect.top() < canvas.top() + 10.0)
        .unwrap_or(first);
    Some(Zones {
        strip: (strip_start..first).collect(),
        dock: (first..end).collect(),
        options: (end..all.len())
            .filter(|i| all[*i].rect.center().y < canvas.top() + 1.0 && all[*i].rect.center().x < left)
            .collect(),
        lower: (low..strip_start).collect(),
        titles,
        left,
        bottom: canvas.bottom(),
    })
}

fn overlaps(all: &[Part], which: &[usize], window: Rect) -> Vec<String> {
    let shown: Vec<&Part> = which
        .iter()
        .map(|i| &all[*i])
        .filter(|p| share(p, window) > 0.9 && area(p.rect) > 16.0)
        .collect();
    shown
        .iter()
        .enumerate()
        .flat_map(|(i, a)| shown[i + 1..].iter().map(move |b| (*a, *b)))
        .filter(|(a, b)| {
            let both = area(a.rect.intersect(b.rect));
            let inside = a.rect.expand(1.5).contains_rect(b.rect) || b.rect.expand(1.5).contains_rect(a.rect);
            !inside && both > 16.0 && both > 0.3 * area(a.rect).min(area(b.rect))
        })
        .map(|(a, b)| format!("{} over {}", short(&a.hint), short(&b.hint)))
        .collect()
}

fn reach(j: &mut Journey, at: Option<Pos2>, wanted: &[usize], step: f32) -> Vec<usize> {
    let window = j.window();
    let count = parts(j).len();
    let mut seen: Vec<bool> = vec![false; wanted.len()];
    let mark = |j: &Journey, seen: &mut Vec<bool>| -> Option<f32> {
        let now = parts(j);
        (now.len() == count).then(|| {
            wanted.iter().enumerate().for_each(|(k, i)| {
                seen[k] |= share(&now[*i], window) >= 0.5;
            });
            wanted.iter().map(|i| now[*i].rect.top()).sum()
        })
    };
    let mut last = mark(j, &mut seen);
    let Some(at) = at.filter(|at| window.contains(*at)) else {
        return wanted
            .iter()
            .zip(&seen)
            .filter(|(_, s)| !**s)
            .map(|(i, _)| *i)
            .collect();
    };
    let mut turns = 0;
    let mut still = 0;
    while turns < 240 && still < 2 && seen.iter().any(|s| !*s) {
        j.wheel_at(at, step);
        turns += 1;
        let now = mark(j, &mut seen);
        still = match (now, last) {
            (Some(a), Some(b)) if (a - b).abs() > 0.5 => 0,
            _ => still + 1,
        };
        last = now;
    }
    (0..turns).for_each(|_| j.wheel_at(at, -step));
    j.h.event(egui::Event::PointerGone);
    j.h.run_steps(2);
    wanted
        .iter()
        .zip(&seen)
        .filter(|(_, s)| !**s)
        .map(|(i, _)| *i)
        .collect()
}

fn fresh(j: &mut Journey) {
    if let Some(ed) = j.ed_mut() {
        ed.settings.layout = Settings::default().layout;
        ed.windows = Windows::default();
        ed.palette = None;
        ed.first.open = false;
    }
    j.h.key_press(Key::Escape);
    j.h.run_steps(4);
}

fn shell(j: &mut Journey, rows: &mut Vec<Row>, state: &str) {
    let window = j.window();
    let all = parts(j);
    let canvas = canvas(j);
    rows.push(Row {
        state: state.to_string(),
        kind: "canvas",
        count: (100.0 * area(canvas) / area(window)).round() as usize,
        of: 100,
        detail: format!(
            "the canvas has {:.0} by {:.0} of the window's {:.0} by {:.0} points",
            canvas.width().max(0.0),
            canvas.height().max(0.0),
            window.width(),
            window.height()
        ),
    });
    let Some(z) = zones(j, &all) else {
        rows.push(Row {
            state: state.to_string(),
            kind: "dock",
            count: 1,
            of: 1,
            detail: "no panel title was found".to_string(),
        });
        return;
    };
    let cut = |which: &[usize], least: f32| -> Vec<String> {
        which
            .iter()
            .filter(|i| share(&all[**i], window) < least)
            .map(|i| short(&all[*i].hint))
            .collect()
    };
    let strip = cut(&z.strip, 0.9);
    let middle = z
        .strip
        .iter()
        .map(|i| all[*i].seen.intersect(window))
        .find(|r| area(*r) > 0.0)
        .map(|r| r.center());
    let never = reach(j, middle, &z.strip, 30.0);
    rows.push(Row {
        state: state.to_string(),
        kind: "strip",
        count: strip.len(),
        of: z.strip.len(),
        detail: match strip.is_empty() {
            true => String::new(),
            false => format!(
                "not in sight until the strip is scrolled, which nothing shows: {}; {} of them cannot be scrolled to",
                listed(&strip),
                never.len()
            ),
        },
    });
    let bars: Vec<usize> = z
        .lower
        .iter()
        .copied()
        .filter(|i| {
            let top = all[*i].rect.top();
            top > canvas.bottom() - 1.0 && top < canvas.bottom() + 76.0
        })
        .collect();
    let bar = cut(&bars, 0.9);
    rows.push(Row {
        state: state.to_string(),
        kind: "timeline bar",
        count: bar.len(),
        of: bars.len(),
        detail: listed(&bar),
    });
    let foot: Vec<usize> = z
        .lower
        .iter()
        .copied()
        .filter(|i| all[*i].rect.top() > window.bottom() - 30.0)
        .collect();
    let feet = cut(&foot, 0.9);
    rows.push(Row {
        state: state.to_string(),
        kind: "status line",
        count: feet.len(),
        of: foot.len(),
        detail: listed(&feet),
    });
    z.titles.iter().enumerate().for_each(|(k, (i, pane, folded))| {
        let title = &all[*i];
        let next = z.titles.get(k + 1).map_or(f32::INFINITY, |t| all[t.0].rect.top() - 3.0);
        let body = (next.min(z.bottom) - (title.rect.bottom() + 3.0)).max(0.0);
        let shown = share(title, window) >= 0.9;
        rows.push(Row {
            state: state.to_string(),
            kind: "panel",
            count: usize::from(!shown || (!*folded && body < pane.min_body() - 1.0)),
            of: 1,
            detail: format!(
                "{}: title {}, {}",
                pane.title(),
                if shown { "in sight" } else { "out of sight" },
                match folded {
                    true => "folded".to_string(),
                    false => format!(
                        "{body:.0} points of its body in sight (it asks for {:.0})",
                        pane.min_body()
                    ),
                }
            ),
        });
    });
    let everything: Vec<usize> = (0..all.len()).collect();
    let over = overlaps(&all, &everything, window);
    rows.push(Row {
        state: state.to_string(),
        kind: "overlap",
        count: over.len(),
        of: all.len(),
        detail: listed(&over),
    });
}

fn options(j: &mut Journey, rows: &mut Vec<Row>, state: &str) {
    let window = j.window();
    let all = parts(j);
    let Some(z) = zones(j, &all) else {
        return;
    };
    let out: Vec<String> = z
        .options
        .iter()
        .filter(|i| share(&all[**i], window) < 0.9)
        .map(|i| short(&all[*i].hint))
        .collect();
    let canvas = canvas(j);
    rows.push(Row {
        state: state.to_string(),
        kind: "options",
        count: out.len(),
        of: z.options.len(),
        detail: format!(
            "{}the canvas starts {:.0} points down and is {:.0} high",
            match out.is_empty() {
                true => String::new(),
                false => format!("{}; ", listed(&out)),
            },
            canvas.top(),
            canvas.height().max(0.0)
        ),
    });
    let over = overlaps(&all, &z.options, window);
    if !over.is_empty() {
        rows.push(Row {
            state: state.to_string(),
            kind: "overlap",
            count: over.len(),
            of: z.options.len(),
            detail: listed(&over),
        });
    }
}

fn pane(j: &mut Journey, rows: &mut Vec<Row>, pane: Pane, cmd: Cmd) {
    fresh(j);
    let state = format!("panel: {}", pane.title());
    let window = j.window();
    let (open, folded) = j.ed().map_or((false, false), |ed| {
        let p = ed.settings.layout.pane(pane);
        (p.is_some_and(|p| p.open), p.is_some_and(|p| p.collapsed))
    });
    let how = match (open, folded) {
        (true, false) => "open in a new window's layout",
        (true, true) => {
            let caret = format!("open {}", pane.title());
            let all = parts(j);
            let at = all
                .iter()
                .find(|p| p.hint == caret && share(p, window) >= 0.9)
                .map(|p| p.rect.center());
            match at {
                Some(_) => {
                    j.press_hinted(&caret);
                    "folded in a new window's layout, opened with its arrow"
                }
                None => {
                    if let Some(ed) = j.ed_mut() {
                        let _ = ed.invoke(cmd);
                        let _ = ed.invoke(cmd);
                    }
                    "folded in a new window's layout with its title out of sight, opened with the Window menu twice"
                }
            }
        }
        (false, _) => {
            if let Some(ed) = j.ed_mut() {
                let _ = ed.invoke(cmd);
            }
            "opened from the Window menu"
        }
    };
    j.h.run_steps(4);
    let all = parts(j);
    let Some(z) = zones(j, &all) else {
        rows.push(Row {
            state,
            kind: "panel reach",
            count: 1,
            of: 1,
            detail: format!("{how}; no panel title was found"),
        });
        return;
    };
    let Some(k) = z.titles.iter().position(|t| t.1 == pane) else {
        rows.push(Row {
            state,
            kind: "panel reach",
            count: 1,
            of: 1,
            detail: format!("{how}; the panel is not in the dock"),
        });
        return;
    };
    let (i, _, now_folded) = z.titles[k];
    let title = &all[i];
    let band = title.rect.center().y;
    let until = z.titles.get(k + 1).map_or(z.dock.last().map_or(i, |l| l + 1), |t| t.0);
    let inside: Vec<usize> = (i + 1..until)
        .filter(|n| z.dock.contains(n) && (all[*n].rect.center().y - band).abs() > 16.0)
        .collect();
    let next = z.titles.get(k + 1).map_or(f32::INFINITY, |t| all[t.0].rect.top() - 3.0);
    let body = (next.min(z.bottom) - (title.rect.bottom() + 3.0)).max(0.0);
    let at = (body > 24.0).then(|| {
        pos2(
            (z.left + window.right()) / 2.0,
            title.rect.bottom() + 3.0 + (body / 2.0).min(40.0),
        )
    });
    let lost = reach(j, at, &inside, 30.0);
    let names: Vec<String> = lost.iter().map(|n| short(&all[*n].hint)).collect();
    let folded_others: Vec<&str> = z
        .titles
        .iter()
        .filter(|t| t.2 && t.1 != pane)
        .map(|t| t.1.title())
        .collect();
    rows.push(Row {
        state: state.clone(),
        kind: "panel reach",
        count: lost.len() + usize::from(share(title, window) < 0.9 || now_folded),
        of: inside.len(),
        detail: format!(
            "{how}; title {}; {body:.0} points of its body in sight (it asks for {:.0}); folded beside it: {}{}",
            if share(title, window) >= 0.9 {
                "in sight"
            } else {
                "out of sight"
            },
            pane.min_body(),
            if folded_others.is_empty() {
                "none".to_string()
            } else {
                folded_others.join(", ")
            },
            match names.is_empty() {
                true => String::new(),
                false => format!("; out of reach: {}", listed(&names)),
            }
        ),
    });
    let side: Vec<String> = inside
        .iter()
        .filter(|n| sideways(&all[**n], window))
        .map(|n| short(&all[*n].hint))
        .collect();
    if !side.is_empty() {
        rows.push(Row {
            state,
            kind: "cut sideways",
            count: side.len(),
            of: inside.len(),
            detail: listed(&side),
        });
    }
}

fn counted(all: &[Part]) -> BTreeMap<String, usize> {
    all.iter().fold(BTreeMap::new(), |mut map, p| {
        *map.entry(p.hint.clone()).or_default() += 1;
        map
    })
}

fn added(before: &BTreeMap<String, usize>, all: &[Part]) -> Vec<usize> {
    let mut left = before.clone();
    all.iter()
        .enumerate()
        .rev()
        .filter(|(_, p)| match left.get_mut(&p.hint) {
            Some(n) if *n > 0 => {
                *n -= 1;
                false
            }
            _ => true,
        })
        .map(|(i, _)| i)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

fn floating(j: &mut Journey, rows: &mut Vec<Row>, name: &str, picture: bool, open: impl FnOnce(&mut Editor)) {
    fresh(j);
    let state = format!("window: {name}");
    let window = j.window();
    let before = counted(&parts(j));
    if let Some(ed) = j.ed_mut() {
        open(ed);
    }
    j.h.run_steps(8);
    let all = parts(j);
    let new = added(&before, &all);
    if new.is_empty() {
        rows.push(Row {
            state,
            kind: "window",
            count: 0,
            of: 0,
            detail: "it brought no new control".to_string(),
        });
        return;
    }
    let whole = new.iter().map(|i| all[*i].rect).fold(Rect::NOTHING, Rect::union);
    let sight = new
        .iter()
        .map(|i| all[*i].seen.intersect(window))
        .filter(|r| area(*r) > 0.0)
        .fold(Rect::NOTHING, Rect::union);
    let at = (area(sight) > 0.0).then(|| sight.center());
    if picture {
        j.look(&name.replace(' ', "-"));
    }
    let lost = reach(j, at, &new, 100.0);
    let names: Vec<String> = lost.iter().map(|n| short(&all[*n].hint)).collect();
    let spill = vec2(
        (whole.right() - window.right())
            .max(window.left() - whole.left())
            .max(0.0),
        (whole.bottom() - window.bottom())
            .max(window.top() - whole.top())
            .max(0.0),
    );
    rows.push(Row {
        state: state.clone(),
        kind: "window",
        count: lost.len(),
        of: new.len(),
        detail: format!(
            "its controls span {:.0} by {:.0} points{}{}",
            whole.width(),
            whole.height(),
            match spill == egui::Vec2::ZERO {
                true => String::new(),
                false => format!(
                    ", {:.0} past the side and {:.0} past the top or bottom",
                    spill.x, spill.y
                ),
            },
            match names.is_empty() {
                true => String::new(),
                false => format!("; out of reach: {}", listed(&names)),
            }
        ),
    });
    let side: Vec<String> = new
        .iter()
        .filter(|n| sideways(&all[**n], window))
        .map(|n| short(&all[*n].hint))
        .collect();
    if !side.is_empty() {
        rows.push(Row {
            state: state.clone(),
            kind: "cut sideways",
            count: side.len(),
            of: new.len(),
            detail: listed(&side),
        });
    }
    let over = overlaps(&all, &new, window);
    if !over.is_empty() {
        rows.push(Row {
            state,
            kind: "overlap",
            count: over.len(),
            of: new.len(),
            detail: listed(&over),
        });
    }
}

fn labels(j: &Journey) -> Vec<(String, Rect)> {
    j.h.query_all(By::new().predicate(|n| written(n).is_some()))
        .filter_map(|n| written(&n.accesskit_node()).map(|label| (label, n.rect())))
        .collect()
}

fn menu(j: &mut Journey, rows: &mut Vec<Row>, name: &str, picture: bool) {
    fresh(j);
    let window = j.window();
    let before: Vec<(String, Rect)> = labels(j);
    let bar =
        j.h.query_all_by_label(name)
            .next()
            .map(|n| n.rect())
            .filter(|r| window.contains_rect(*r));
    let state = format!("menu: {name}");
    let Some(bar) = bar else {
        rows.push(Row {
            state,
            kind: "menu",
            count: 1,
            of: 1,
            detail: "its name is not in the window".to_string(),
        });
        return;
    };
    j.h.hover_at(bar.center());
    j.h.run_steps(1);
    j.h.event(egui::Event::PointerButton {
        pos: bar.center(),
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    j.h.run_steps(1);
    j.h.event(egui::Event::PointerButton {
        pos: bar.center(),
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    j.h.run_steps(4);
    let items: Vec<(String, Rect)> = labels(j)
        .into_iter()
        .filter(|(label, rect)| {
            !before
                .iter()
                .any(|(l, r)| l == label && (r.min - rect.min).abs().max_elem() < 0.5)
        })
        .collect();
    let cut: Vec<String> = items
        .iter()
        .filter(|(_, r)| !window.expand(1.0).contains_rect(*r))
        .map(|(label, _)| short(label))
        .collect();
    if picture {
        j.look(&format!("menu-{}", name.to_lowercase()));
    }
    rows.push(Row {
        state,
        kind: "menu",
        count: cut.len(),
        of: items.len(),
        detail: match cut.is_empty() {
            true => String::new(),
            false => format!("past the window's edge: {}", listed(&cut)),
        },
    });
    j.h.key_press(Key::Escape);
    j.h.run_steps(3);
}

fn start(size: egui::Vec2, scale: f32, combo: &str, rows: &mut Vec<Row>) {
    let mut j = Journey::begin_scaled(&format!("sizes-start-{combo}"), size, None, scale);
    j.h.run_steps(6);
    let window = j.window();
    j.look("start-window");
    let all = parts(&j);
    let everything: Vec<usize> = (0..all.len()).collect();
    let lost = reach(&mut j, Some(window.center()), &everything, 100.0);
    let names: Vec<String> = lost.iter().map(|n| short(&all[*n].hint)).collect();
    let named: Vec<(String, Rect)> = labels(&j);
    let create = named
        .iter()
        .find(|(label, _)| label == "Create")
        .is_some_and(|(_, r)| window.contains_rect(*r));
    rows.push(Row {
        state: "the start window".to_string(),
        kind: "window",
        count: lost.len() + usize::from(!create),
        of: all.len(),
        detail: format!(
            "the Create button is {}{}",
            if create { "in the window" } else { "not in the window" },
            match names.is_empty() {
                true => String::new(),
                false => format!("; out of reach: {}", listed(&names)),
            }
        ),
    });
    let over = overlaps(&all, &everything, window);
    if !over.is_empty() {
        rows.push(Row {
            state: "the start window".to_string(),
            kind: "overlap",
            count: over.len(),
            of: all.len(),
            detail: listed(&over),
        });
    }
    j.finish(&[]);
}

fn measure(width: f32, height: f32, scale: f32) {
    let size = vec2(width, height);
    let combo = format!("{width:.0}x{height:.0}-at-{:.0}", scale * 100.0);
    let mut rows: Vec<Row> = Vec::new();
    let mut j = Journey::begin_scaled(&format!("sizes-{combo}"), size, Some("lantern.klad"), scale);
    let every = std::env::var_os("KLAD_SIZES_PICTURES").is_some_and(|v| v == "all");
    let window = j.window();
    j.note(format!(
        "a window of {width:.0} by {height:.0} at interface size {scale:.2}: {:.0} by {:.0} points to lay out in",
        window.width(),
        window.height()
    ));
    fresh(&mut j);
    j.look("new-layout");
    shell(&mut j, &mut rows, "a new window's layout");
    j.pictures(every);
    TOOLS.iter().for_each(|(key, name)| {
        fresh(&mut j);
        j.h.key_press(*key);
        j.h.run_steps(4);
        if every {
            j.look(&format!("tool-{}", name.replace(' ', "-")));
        }
        options(&mut j, &mut rows, &format!("tool: {name}"));
    });
    fresh(&mut j);
    j.h.key_press(Key::V);
    j.h.run_steps(2);
    j.h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    j.h.run_steps(4);
    j.pictures(true);
    j.look("strokes-selected");
    j.pictures(every);
    options(&mut j, &mut rows, "tool: select and move, everything selected");
    j.h.key_press(Key::Escape);
    j.h.key_press(Key::B);
    j.h.run_steps(3);
    PANES.iter().for_each(|(p, cmd)| {
        pane(&mut j, &mut rows, *p, *cmd);
        if every {
            j.look(&format!("panel-{}", p.title().replace(' ', "-").to_lowercase()));
        }
    });
    Section::ALL.into_iter().for_each(|part| {
        floating(&mut j, &mut rows, &format!("settings, {}", part.name()), every, |ed| {
            ed.windows.settings = true;
            ed.settings_section = part;
        });
    });
    OPENED.iter().for_each(|(cmd, name)| {
        floating(&mut j, &mut rows, name, every, |ed| {
            let _ = ed.invoke(*cmd);
        });
    });
    EXPORTS.iter().for_each(|(what, name)| {
        floating(&mut j, &mut rows, name, every || *what == What::Godot, |ed| {
            ed.windows.export = true;
            ed.export.what = *what;
        });
    });
    MENUS.iter().for_each(|name| {
        menu(&mut j, &mut rows, name, every || *name == "File");
    });
    let out = j.out().and_then(|dir| dir.parent().map(|p| p.to_path_buf()));
    j.finish(&[]);
    start(size, scale, &combo, &mut rows);
    let problems: usize = rows.iter().filter(|r| r.kind != "canvas").map(|r| r.count).sum();
    println!("{combo}: {problems} problems in {} rows", rows.len());
    if let Some(out) = out {
        let text: String = rows
            .iter()
            .map(|r| {
                format!(
                    "{combo}\t{}\t{}\t{}\t{}\t{}\n",
                    r.state,
                    r.kind,
                    r.count,
                    r.of,
                    r.detail.replace(['\t', '\n'], " ")
                )
            })
            .collect();
        std::fs::write(out.join(format!("sizes-{combo}.tsv")), text).unwrap();
    }
}

macro_rules! sizes {
    ($($name:ident: $width:expr, $height:expr, $scale:expr;)*) => {
        $(
            #[test]
            #[ignore = "measures the editor at one window size and interface size; run with `just sizes`"]
            fn $name() {
                measure($width, $height, $scale);
            }
        )*
    };
}

sizes! {
    a_small_laptop_at_75: 1280.0, 720.0, 0.75;
    a_small_laptop_at_100: 1280.0, 720.0, 1.0;
    a_small_laptop_at_125: 1280.0, 720.0, 1.25;
    a_small_laptop_at_150: 1280.0, 720.0, 1.5;
    a_laptop_at_75: 1440.0, 900.0, 0.75;
    a_laptop_at_100: 1440.0, 900.0, 1.0;
    a_laptop_at_125: 1440.0, 900.0, 1.25;
    a_laptop_at_150: 1440.0, 900.0, 1.5;
    a_desktop_at_75: 1920.0, 1080.0, 0.75;
    a_desktop_at_100: 1920.0, 1080.0, 1.0;
    a_desktop_at_125: 1920.0, 1080.0, 1.25;
    a_desktop_at_150: 1920.0, 1080.0, 1.5;
}
