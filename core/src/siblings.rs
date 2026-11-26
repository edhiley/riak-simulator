//! Siblings: the values of one key that no write has replaced, in a bucket
//! with allow_mult (every created bucket type; not the default type).
//!
//! The rules follow riak-emu's engine, which is checked against a real
//! OpenRiak 3.4.1 node, extended from one actor to many (Riak's dotted
//! version vectors). Each value carries a dot: the vnode that wrote it and
//! that vnode's count. A write brings the vclock the client read, its
//! context; it replaces the values the context has seen and keeps the
//! others. A write with no context replaces nothing, so it adds a sibling.
//! Two copies merge by keeping every value that the other copy has not seen.
use std::collections::BTreeMap;

pub type Clock = BTreeMap<usize, u64>;

#[derive(Clone, Debug, PartialEq)]
pub struct Sib {
    /// The vnode that wrote this value, and its count.
    pub dot: (usize, u64),
    pub value: Vec<u8>,
    pub content_type: String,
    /// Riak's name for one sibling, used with ?vtag=.
    pub vtag: String,
    /// A tombstone: the record of a delete.
    pub deleted: bool,
}

/// The clock has seen this dot.
pub fn covered(clock: &Clock, dot: (usize, u64)) -> bool {
    clock.get(&dot.0).is_some_and(|n| *n >= dot.1)
}

/// The values kept after a write with context `ctx`: those it has not seen.
pub fn unseen(siblings: &[Sib], ctx: Option<&Clock>) -> Vec<Sib> {
    match ctx {
        Some(c) => siblings.iter().filter(|s| !covered(c, s.dot)).cloned().collect(),
        None => siblings.to_vec(),
    }
}

/// Merge two copies of a key: keep a value both have, and a value one has
/// that the other has not seen. A value one has seen and dropped was
/// replaced, so it goes.
pub fn merge(a: &[Sib], ca: &Clock, b: &[Sib], cb: &Clock) -> Vec<Sib> {
    let mut out: Vec<Sib> = Vec::new();
    for s in a {
        if b.iter().any(|t| t.dot == s.dot) || !covered(cb, s.dot) {
            out.push(s.clone());
        }
    }
    for s in b {
        if !out.iter().any(|t| t.dot == s.dot) && !covered(ca, s.dot) {
            out.push(s.clone());
        }
    }
    // A real node returns siblings ordered by value.
    out.sort_by(|x, y| x.value.cmp(&y.value).then(x.dot.cmp(&y.dot)));
    out
}

/// The vclock as the client sees it in X-Riak-Vclock. Riak sends an opaque
/// binary term; the simulator sends its own opaque text.
pub fn encode(clock: &Clock) -> String {
    let text: String = clock.iter().map(|(a, n)| format!("{a}.{n}")).collect::<Vec<_>>().join("-");
    crate::crdt::base64(text.as_bytes())
}

pub fn decode(s: &str) -> Option<Clock> {
    let bytes = crate::crdt::unbase64(s.trim())?;
    let text = String::from_utf8(bytes).ok()?;
    let mut clock = Clock::new();
    for part in text.split('-').filter(|p| !p.is_empty()) {
        let (a, n) = part.split_once('.')?;
        clock.insert(a.parse().ok()?, n.parse().ok()?);
    }
    Some(clock)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sib(a: usize, n: u64, v: &str) -> Sib {
        Sib { dot: (a, n), value: v.as_bytes().to_vec(), content_type: "text/plain".into(), vtag: format!("t{a}{n}"), deleted: false }
    }

    #[test]
    fn a_write_without_context_adds_a_sibling_and_with_context_replaces() {
        let first = vec![sib(1, 1, "milk")];
        // Blind write: keeps milk.
        assert_eq!(unseen(&first, None).len(), 1);
        // Write with the clock that saw milk: replaces it.
        let ctx = Clock::from([(1, 1)]);
        assert!(unseen(&first, Some(&ctx)).is_empty());
    }

    #[test]
    fn merge_keeps_concurrent_values_and_drops_replaced_ones() {
        // Copy A took "milk" at vnode 1; copy B took "bread" at vnode 2.
        let a = vec![sib(1, 1, "milk")];
        let b = vec![sib(2, 1, "bread")];
        let (ca, cb) = (Clock::from([(1, 1)]), Clock::from([(2, 1)]));
        let m = merge(&a, &ca, &b, &cb);
        assert_eq!(m.len(), 2);
        // C saw both and wrote "milk, bread": merging with A drops milk.
        let c = vec![sib(3, 1, "milk, bread")];
        let cc = Clock::from([(1, 1), (2, 1), (3, 1)]);
        assert_eq!(merge(&a, &ca, &c, &cc), c);
    }

    #[test]
    fn vclock_round_trip() {
        let c = Clock::from([(3, 2), (40, 7)]);
        assert_eq!(decode(&encode(&c)), Some(c));
        assert_eq!(decode("not base64!"), None);
    }
}
