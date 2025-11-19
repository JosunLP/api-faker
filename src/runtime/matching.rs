use std::collections::{BTreeMap, BTreeSet};

use super::builder::RouteRuntime;

impl RouteRuntime {
    pub fn same_query_signature(&self, other: &RouteRuntime) -> bool {
        if self.error_trigger.as_deref() != other.error_trigger.as_deref() {
            return false;
        }

        match (&self.request_flags, &other.request_flags) {
            (None, None) => {}
            (Some(a), Some(b)) if a.as_slice() == b.as_slice() => {}
            _ => return false,
        }

        match (&self.query_params, &other.query_params) {
            (None, None) => true,
            (Some(a), Some(b)) => a.as_slice() == b.as_slice(),
            _ => false,
        }
    }

    pub fn matches_query(&self, query: Option<&BTreeMap<String, Vec<String>>>) -> bool {
        match &self.query_params {
            None => true,
            Some(expected) => {
                let actual = match query {
                    Some(map) => map,
                    None => return false,
                };
                expected.iter().all(|(key, value)| {
                    actual
                        .get(key)
                        .map(|vals| vals.iter().any(|candidate| candidate == value))
                        .unwrap_or(false)
                })
            }
        }
    }

    pub fn matches_request_flags(&self, flags: Option<&BTreeSet<String>>) -> bool {
        match &self.request_flags {
            None => true,
            Some(expected) => {
                let provided = match flags {
                    Some(set) => set,
                    None => return false,
                };
                expected.iter().all(|flag| provided.contains(flag))
            }
        }
    }

    pub fn specificity_rank(&self) -> u8 {
        let mut rank = 0;
        if self.query_params.is_some() {
            rank += 1;
        }
        if self
            .request_flags
            .as_ref()
            .map(|flags| !flags.is_empty())
            .unwrap_or(false)
        {
            rank += 1;
        }
        rank
    }
}
