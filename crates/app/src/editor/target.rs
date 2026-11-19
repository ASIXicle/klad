use super::Editor;
use super::lock::Locked;
use klad_core::doc::{Kind, MASK};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Target {
    Here,
    NewLayer,
    Refused(String),
}

impl Editor {
    pub(super) fn hidden_in_hand(&self) -> Option<String> {
        let doc = &self.scene.bundle.doc;
        let active = &self.scene.active;
        (1..=active.len())
            .filter(|n| active[n - 1] != MASK)
            .filter_map(|n| doc.locate_path(&active[..n]))
            .find(|l| !l.visible)
            .map(|l| l.name.clone())
    }

    pub(super) fn paint_target(&self, erasing: bool) -> Target {
        let Some(layer) = self.scene.active_layer() else {
            return match erasing {
                true => Target::Refused("nothing to erase: pick a paint layer in the layers panel".to_string()),
                false => Target::NewLayer,
            };
        };
        if layer.locked {
            return Target::Refused(Locked(vec![layer.name.clone()]).to_string());
        }
        if let Some(hidden) = self.hidden_in_hand() {
            return Target::Refused(hidden_words(&hidden));
        }
        match (layer.kind(), erasing) {
            (Kind::Paint, _) => Target::Here,
            (_, true) => Target::Refused(format!(
                "{} is a {} layer: the eraser works on paint and pixel layers",
                layer.name,
                layer.kind_name()
            )),
            (_, false) => Target::NewLayer,
        }
    }

    pub(super) fn take_target(&self) -> Option<String> {
        self.hidden_in_hand().map(|hidden| hidden_words(&hidden))
    }
}

pub(super) fn hidden_words(name: &str) -> String {
    format!("{name} is hidden: show it with its eye in the layers panel to work on it")
}
