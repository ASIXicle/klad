use klad_core::doc::{Document, Layer};
use klad_core::ops::{Op, apply_all};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Locked(pub Vec<String>);

impl std::fmt::Display for Locked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0.as_slice() {
            [one] => write!(
                f,
                "{one} is locked: unlock it with the lock in the layers panel to change it"
            ),
            many => write!(
                f,
                "{} are locked: unlock them with the lock in the layers panel to change them",
                and_list(many)
            ),
        }
    }
}

impl std::error::Error for Locked {}

fn and_list(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

fn drawing(layer: &Layer) -> Value {
    let cels: Vec<Value> = layer
        .cels()
        .iter()
        .map(|cel| {
            let mut value = serde_json::to_value(cel).unwrap_or_default();
            if let Some(fields) = value.as_object_mut() {
                fields.remove("frame");
                fields.remove("hold");
            }
            value
        })
        .collect();
    serde_json::json!({
        "cels": cels,
        "adjust": layer.adjustment(),
        "bones": layer.bones,
        "ik": layer.ik,
        "rig": layer.rig,
    })
}

fn timing(layer: &Layer) -> Vec<(String, u32, Option<u32>)> {
    layer.cels().iter().map(|c| (c.id.clone(), c.frame, c.hold)).collect()
}

fn named(op: &Op) -> Vec<String> {
    match op {
        Op::Frames(spec) => spec.layers.clone().unwrap_or_default(),
        Op::Retime(spec) => spec.layers.clone().unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn whole_timeline(ops: &[Op]) -> bool {
    ops.iter()
        .all(|op| matches!(op, Op::Frames(spec) if spec.layers.is_none()))
}

pub(super) fn refuse(doc: &Document, ops: &[Op]) -> Result<(), Locked> {
    let locked: Vec<&Layer> = doc.all_layers().into_iter().filter(|l| l.locked).collect();
    if locked.is_empty() {
        return Ok(());
    }
    let Ok((next, applied)) = apply_all(doc, ops) else {
        return Ok(());
    };
    let named: Vec<String> = ops.iter().flat_map(named).collect();
    let whole = whole_timeline(ops);
    let hit: Vec<String> = locked
        .into_iter()
        .filter(|layer| {
            let marked = applied
                .marks
                .iter()
                .any(|p| p.layer == layer.id && !p.changes.is_empty());
            let changed = next
                .layer(&layer.id)
                .is_some_and(|after| drawing(after) != drawing(layer) || (!whole && timing(after) != timing(layer)));
            marked || changed || named.contains(&layer.id)
        })
        .map(|layer| layer.name.clone())
        .collect();
    match hit.is_empty() {
        true => Ok(()),
        false => Err(Locked(hit)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use klad_core::ops::parse_lines;

    fn doc() -> Document {
        let mut doc = Document::new(64, 64);
        doc.timeline.frames = 4;
        doc.layers.push(Layer::paint("l1", "ground", "c1"));
        doc.layers.push(Layer::paint("l2", "lines", "c2"));
        doc.layers[1].locked = true;
        doc.next_id = 10;
        doc
    }

    fn ops(lines: &str) -> Vec<Op> {
        parse_lines(lines).unwrap()
    }

    #[test]
    fn what_changes_a_locked_layer_is_refused_and_named() {
        let doc = doc();
        let refused = |lines: &str| refuse(&doc, &ops(lines)).err().map(|e| e.to_string());
        let message = Some("lines is locked: unlock it with the lock in the layers panel to change it".to_string());
        assert_eq!(refused(r#"{"op":"stroke","layer":"l2","path":"M1 1 L9 9"}"#), message);
        assert_eq!(refused(r#"{"op":"cel","new":"l2","frame":2}"#), message);
        assert_eq!(refused(r#"{"op":"set","id":"c2","hold":2}"#), message);
        assert_eq!(
            refused(r#"{"op":"frames","insert":0,"count":1,"layers":["l2"]}"#),
            message
        );
        assert_eq!(refused(r#"{"op":"stroke","layer":"l1","path":"M1 1 L9 9"}"#), None);
        assert_eq!(refused(r#"{"op":"set","id":"l2","locked":false}"#), None);
        assert_eq!(refused(r#"{"op":"set","id":"l2","name":"ink","visible":false}"#), None);
        assert_eq!(refused(r#"{"op":"frames","insert":0,"count":1}"#), None);
        assert_eq!(refused(r#"{"op":"rm","id":"l2"}"#), None);
    }

    #[test]
    fn several_locked_layers_are_named_together() {
        let mut doc = doc();
        doc.layers[0].locked = true;
        let e = refuse(
            &doc,
            &ops(r#"{"op":"frames","insert":0,"count":1,"layers":["l1","l2"]}"#),
        )
        .unwrap_err();
        assert_eq!(
            e.to_string(),
            "ground and lines are locked: unlock them with the lock in the layers panel to change them"
        );
    }
}
