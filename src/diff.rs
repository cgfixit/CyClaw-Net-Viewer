use std::collections::{HashMap, HashSet};

use crate::snapshot::{Dir, Endpoint, EndpointKey};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Highlight {
    NewIn,
    NewOut,
    Changed,
    Deleted,
    None,
}

#[derive(Clone, Debug)]
pub struct Row {
    pub endpoint: Endpoint,
    pub highlight: Highlight,
    /// Remaining visible deleted ticks, including the current snapshot.
    pub linger: u8,
}

/// Deleted rows appear on exactly two refreshes after their last live snapshot:
/// first absent refresh: 2, second: 1, third: removed. Repaints do not age rows.
pub fn diff(prev: &[Row], next: &[Endpoint]) -> Vec<Row> {
    let live_prev: HashMap<&EndpointKey, &Row> = prev
        .iter()
        .filter(|r| r.highlight != Highlight::Deleted)
        .map(|r| (&r.endpoint.key, r))
        .collect();

    let mut seen: HashSet<&EndpointKey> = HashSet::new();
    let mut out = Vec::with_capacity(next.len() + 8);

    for e in next {
        seen.insert(&e.key);
        let highlight = match live_prev.get(&e.key) {
            None => match e.dir {
                Dir::In | Dir::Listen => Highlight::NewIn,
                Dir::Out => Highlight::NewOut,
                Dir::Unknown => Highlight::None,
            },
            Some(old) if old.endpoint.state != e.state => Highlight::Changed,
            Some(_) => Highlight::None,
        };
        out.push(Row {
            endpoint: e.clone(),
            highlight,
            linger: 0,
        });
    }

    for r in prev {
        if seen.contains(&r.endpoint.key) {
            continue;
        }
        if r.highlight == Highlight::Deleted {
            let linger = r.linger.saturating_sub(1);
            if linger > 0 {
                let mut d = r.clone();
                d.linger = linger;
                out.push(d);
            }
        } else {
            let mut d = r.clone();
            d.highlight = Highlight::Deleted;
            d.linger = 2;
            out.push(d);
        }
    }
    out
}
