use super::Editor;
use klad_core::ops::{ClipEdit, Op};
use klad_core::timing::CelAction;

fn plural(n: u32, one: &str, many: &str) -> String {
    match n {
        1 => format!("1 {one}"),
        _ => format!("{n} {many}"),
    }
}

impl Editor {
    pub(super) fn layer_name(&self, id: &str) -> String {
        self.scene
            .bundle
            .doc
            .layer(id)
            .map_or_else(|| "the layer".to_string(), |l| l.name.clone())
    }

    fn op_words(&self, op: &Op) -> Option<String> {
        match op {
            Op::AddLayer { layer, .. } => Some(match layer["name"].as_str() {
                Some(name) => format!("added the layer {name}"),
                None => format!("added a {} layer", layer["kind"].as_str().unwrap_or("new")),
            }),
            Op::Frames(spec) => Some(format!(
                "{} {} at frame {}",
                if spec.insert { "inserted" } else { "removed" },
                plural(spec.count, "frame", "frames"),
                spec.at
            )),
            Op::Duplicate { layer } => Some(format!("duplicated {}", self.layer_name(layer))),
            Op::Cel(spec) => Some(match &spec.action {
                CelAction::New { layer } => format!(
                    "new drawing on {} at frame {}",
                    self.layer_name(layer),
                    spec.frame.unwrap_or(self.scene.frame)
                ),
                CelAction::Dup { .. } => "duplicated the drawing".to_string(),
                CelAction::Link { .. } => "the frame shows the same drawing now".to_string(),
                CelAction::Unlink { .. } => "the frame has a drawing of its own now".to_string(),
                CelAction::Move { .. } => format!("moved the drawing to frame {}", spec.frame.unwrap_or(0)),
            }),
            Op::Clip(ClipEdit::Add(clip)) => Some(format!("added the clip {}", clip.name)),
            Op::Clip(ClipEdit::Remove { name }) => Some(format!("removed the clip {name}")),
            Op::Timeline {
                frames: Some(frames), ..
            } => Some(format!("the timeline is {} long", plural(*frames, "frame", "frames"))),
            _ => None,
        }
    }

    pub(super) fn edit_words(&self, ops: &[Op]) -> Option<String> {
        let parts: Vec<String> = ops
            .iter()
            .filter_map(|op| self.op_words(op))
            .fold(Vec::new(), |mut acc, w| {
                if acc.last() != Some(&w) {
                    acc.push(w);
                }
                acc
            });
        (!parts.is_empty()).then(|| parts.join("; "))
    }

    pub(super) fn job_words(&self, text: &str) -> String {
        self.plain(&text.replace(" cel ", " "))
    }

    pub(super) fn refusal_words(&self, why: &str) -> String {
        let bare = match why.strip_prefix("op ") {
            Some(rest) => rest.split_once(": ").map_or(why, |(_, tail)| tail),
            None => why,
        };
        self.plain(bare)
    }

    pub(super) fn step_words(&self, summary: &str) -> String {
        let verbs = [
            ("rm ", "delete "),
            ("mv ", "move "),
            ("set ", "change "),
            ("add cel to ", "add a drawing to "),
            ("add layer ", "add a layer: "),
        ];
        let said = summary
            .split(", ")
            .map(|part| {
                verbs
                    .iter()
                    .find_map(|(from, to)| part.strip_prefix(from).map(|rest| format!("{to}{rest}")))
                    .unwrap_or_else(|| part.to_string())
            })
            .collect::<Vec<_>>()
            .join(", ");
        self.plain(&said)
    }
}
