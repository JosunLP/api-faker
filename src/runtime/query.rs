use std::collections::BTreeMap;
use std::sync::Arc;

pub(super) fn map_query_arc(map: &BTreeMap<String, String>) -> Option<Arc<Vec<(String, String)>>> {
    if map.is_empty() {
        None
    } else {
        Some(Arc::new(
            map.iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<Vec<_>>(),
        ))
    }
}
