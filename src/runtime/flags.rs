use std::collections::BTreeSet;
use std::sync::Arc;

pub(super) fn flags_arc(set: &BTreeSet<String>) -> Option<Arc<Vec<String>>> {
    if set.is_empty() {
        None
    } else {
        Some(Arc::new(set.iter().cloned().collect()))
    }
}

pub(super) fn merge_response_flags(
    global: &[String],
    route: &[String],
    variant: Option<&Vec<String>>,
) -> Vec<String> {
    let mut combined = Vec::new();
    dedup_extend(&mut combined, global);
    dedup_extend(&mut combined, route);
    if let Some(extra) = variant {
        dedup_extend(&mut combined, extra);
    }
    combined
}

fn dedup_extend(target: &mut Vec<String>, source: &[String]) {
    for value in source {
        if !target.iter().any(|existing| existing == value) {
            target.push(value.clone());
        }
    }
}
