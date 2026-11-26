//! Consistent hashing as Riak does it: keys hashed onto a 160-bit ring that is
//! cut into equal partitions, each owned by one node.

use crate::sha1::sha1;
use serde::Serialize;

/// Where a bucket/key lands. `bucket_type` is `None` for an untyped bucket.
/// The client APIs treat type `default` as no type, so callers normalise it
/// before hashing (see `BucketRef::new`).
pub fn key_hash(bucket_type: Option<&[u8]>, bucket: &[u8], key: &[u8]) -> [u8; 20] {
    sha1(&bkey_term(bucket_type, bucket, key))
}

/// `term_to_binary({Bucket, Key})`, where Bucket is a binary or, for a typed
/// bucket, the tuple `{Type, Bucket}`. This is what Riak's default key hash
/// function (`chash_std_keyfun`) hashes.
fn bkey_term(bucket_type: Option<&[u8]>, bucket: &[u8], key: &[u8]) -> Vec<u8> {
    fn binary(out: &mut Vec<u8>, b: &[u8]) {
        out.push(109); // BINARY_EXT
        out.extend_from_slice(&(b.len() as u32).to_be_bytes());
        out.extend_from_slice(b);
    }
    let mut out = vec![131, 104, 2]; // version, SMALL_TUPLE_EXT, arity 2
    if let Some(t) = bucket_type {
        out.extend_from_slice(&[104, 2]);
        binary(&mut out, t);
    }
    binary(&mut out, bucket);
    binary(&mut out, key);
    out
}

/// A bucket as the client APIs name it, with type `default` folded into "no type".
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct BucketRef {
    pub bucket_type: Option<String>,
    pub bucket: String,
}

impl BucketRef {
    pub fn new(bucket_type: Option<&str>, bucket: &str) -> Self {
        BucketRef {
            bucket_type: bucket_type.filter(|t| *t != "default").map(str::to_owned),
            bucket: bucket.to_owned(),
        }
    }

    pub fn hash(&self, key: &str) -> [u8; 20] {
        key_hash(self.bucket_type.as_deref().map(str::as_bytes), self.bucket.as_bytes(), key.as_bytes())
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Ring {
    /// Owner (node number, from 0) of each partition, in ring order.
    pub owners: Vec<usize>,
}

impl Ring {
    /// A fresh ring of `size` partitions (a power of two from 8 to 1024)
    /// over `nodes`. Every node gets an equal share, as near as possible, and
    /// any 3 neighbouring partitions are on 3 different nodes where that is
    /// possible, so a key's 3 copies are on 3 nodes. Riak's claim algorithm
    /// has the same aim (target_n_val); the exact owners can differ.
    pub fn new(size: usize, nodes: usize) -> Result<Ring, String> {
        if !size.is_power_of_two() || !(8..=1024).contains(&size) {
            return Err(format!("ring_size must be a power of two from 8 to 1024, not {size}"));
        }
        if nodes == 0 {
            return Err("a cluster needs at least one node".into());
        }
        Ok(Ring { owners: claim(size, nodes, SPREAD) })
    }

    pub fn size(&self) -> usize {
        self.owners.len()
    }

    /// A fresh ring's owners, for checks.
    pub fn fresh(size: usize, nodes: usize) -> Vec<usize> {
        claim(size, nodes, SPREAD)
    }

    /// The partition whose range contains `hash`: the top bits of the hash.
    pub fn partition_of(&self, hash: &[u8; 20]) -> usize {
        let top = u32::from_be_bytes([hash[0], hash[1], hash[2], hash[3]]);
        let bits = self.size().trailing_zeros();
        (top >> (32 - bits)) as usize
    }

    /// The `n` partitions a key is stored on, in order: the partitions
    /// following the one its hash falls in, wrapping round. A partition's
    /// index marks the start of its range, so a key is owned by the next one.
    pub fn preflist(&self, hash: &[u8; 20], n: usize) -> Vec<usize> {
        let first = self.partition_of(hash) + 1;
        (0..n.min(self.size())).map(|i| (first + i) % self.size()).collect()
    }

    /// A partition's index as Riak reports it: its start on the 2^160 ring, in decimal.
    pub fn partition_index(&self, partition: usize) -> String {
        let mut bytes = [0u8; 20];
        let shift = 160 - self.size().trailing_zeros() as usize;
        // partition << shift, as a 160-bit big-endian number
        let value = partition as u128;
        for bit in 0..128 {
            if value >> bit & 1 == 1 {
                let pos = bit + shift;
                if pos < 160 {
                    bytes[19 - pos / 8] |= 1 << (pos % 8);
                }
            }
        }
        to_decimal(&bytes)
    }
}

/// Neighbouring partitions that should be on different nodes: n_val.
const SPREAD: usize = 3;

/// Give partitions to nodes. Tries to keep every `spread` neighbouring
/// partitions, round the ring, on different nodes, with equal shares. Falls
/// back to dealing in turn if that is not possible (for example 3 nodes and
/// a ring size that 3 does not divide).
fn claim(size: usize, nodes: usize, spread: usize) -> Vec<usize> {
    let in_turn: Vec<usize> = (0..size).map(|p| p % nodes).collect();
    if nodes < spread {
        return in_turn;
    }
    let cap = size.div_ceil(nodes);
    let mut owners = vec![usize::MAX; size];
    let mut counts = vec![0; nodes];
    let mut budget = 200_000usize;

    fn ok(owners: &[usize], size: usize, spread: usize, p: usize, n: usize) -> bool {
        // Partitions before p, and (near the end) partitions after the wrap.
        (1..spread).all(|d| {
            let before = owners[(p + size - d) % size];
            let after = owners[(p + d) % size];
            before != n && after != n
        })
    }

    fn place(
        p: usize,
        size: usize,
        nodes: usize,
        spread: usize,
        cap: usize,
        owners: &mut Vec<usize>,
        counts: &mut Vec<usize>,
        budget: &mut usize,
    ) -> bool {
        if p == size {
            return true;
        }
        if *budget == 0 {
            return false;
        }
        *budget -= 1;
        // Prefer the node that dealing in turn would pick, then the least used.
        let mut order: Vec<usize> = (0..nodes).collect();
        order.sort_by_key(|&n| (counts[n], (n + nodes - p % nodes) % nodes));
        for n in order {
            if counts[n] < cap && ok(owners, size, spread, p, n) {
                owners[p] = n;
                counts[n] += 1;
                if place(p + 1, size, nodes, spread, cap, owners, counts, budget) {
                    return true;
                }
                owners[p] = usize::MAX;
                counts[n] -= 1;
            }
        }
        false
    }

    if place(0, size, nodes, spread, cap, &mut owners, &mut counts, &mut budget) {
        owners
    } else {
        in_turn
    }
}

/// True if node `n` at partition `p` keeps any 3 neighbouring partitions
/// on different nodes.
fn spread_ok(owners: &[usize], p: usize, n: usize) -> bool {
    let size = owners.len();
    (1..SPREAD).all(|d| owners[(p + size - d) % size] != n && owners[(p + d) % size] != n)
}

/// New owners after `new` joins: the nodes with the most partitions give
/// some to the new node until every node has an equal share. Only what must
/// move moves. This is the simulator's own claim, not Riak's; it has the
/// same aims (an equal share, and 3 neighbouring partitions on 3 nodes).
pub fn claim_join(owners: &[usize], members: &[usize]) -> Vec<usize> {
    rebalance(owners.to_vec(), members)
}

/// New owners after `leaver` leaves: each of its partitions goes to a node
/// with the fewest, where that keeps neighbours on different nodes.
pub fn claim_leave(owners: &[usize], remaining: &[usize], leaver: usize) -> Vec<usize> {
    let mut owners = owners.to_vec();
    let size = owners.len();
    for p in 0..size {
        if owners[p] != leaver {
            continue;
        }
        let count = |o: &[usize], n: usize| o.iter().filter(|&&x| x == n).count();
        let mut order: Vec<usize> = remaining.to_vec();
        order.sort_by_key(|&n| (count(&owners, n), n));
        let pick = order.iter().copied().find(|&n| spread_ok(&owners, p, n)).unwrap_or(order[0]);
        owners[p] = pick;
    }
    rebalance(owners, remaining)
}

/// Move partitions from the nodes with the most to the nodes with the
/// fewest until no two nodes differ by more than one, keeping neighbours on
/// different nodes where possible.
fn rebalance(mut owners: Vec<usize>, members: &[usize]) -> Vec<usize> {
    let size = owners.len();
    for _ in 0..size * 4 {
        let count = |o: &[usize], n: usize| o.iter().filter(|&&x| x == n).count();
        let mut by: Vec<usize> = members.to_vec();
        by.sort_by_key(|&n| (count(&owners, n), n));
        let (low, high) = (by[0], by[by.len() - 1]);
        if count(&owners, high) <= count(&owners, low) + 1 {
            break;
        }
        // Prefer a move from the fullest node to the emptiest that keeps the
        // spread; then any move between nodes that differ by two or more.
        let mut moved = false;
        'search: for &d in by.iter().rev() {
            for &r in by.iter() {
                if count(&owners, d) < count(&owners, r) + 2 {
                    continue;
                }
                for p in 0..size {
                    if owners[p] == d && spread_ok(&owners, p, r) {
                        owners[p] = r;
                        moved = true;
                        break 'search;
                    }
                }
            }
        }
        if !moved {
            // No move keeps the spread: balance the share anyway.
            let p = (0..size).find(|&p| owners[p] == high).unwrap();
            owners[p] = low;
        }
    }
    let owners = repair(owners);
    if spread_holds(&owners) {
        return owners;
    }
    // Last resort: a fresh ring of this shape, with its nodes named so that
    // as many partitions as possible keep their owner.
    let fresh = claim(size, members.len(), SPREAD);
    if !spread_holds(&fresh) {
        return owners;
    }
    best_relabel(&fresh, &owners, members)
}

/// Name the nodes of `fresh` (0..m) after `members` so that it agrees with
/// `current` on the most partitions. Tries every naming: m is at most 8.
fn best_relabel(fresh: &[usize], current: &[usize], members: &[usize]) -> Vec<usize> {
    let m = members.len();
    // agree[i][j]: partitions where fresh has i and current has members[j].
    let mut agree = vec![vec![0usize; m]; m];
    for (f, c) in fresh.iter().zip(current) {
        if let Some(j) = members.iter().position(|x| x == c) {
            agree[*f][j] += 1;
        }
    }
    let mut perm: Vec<usize> = (0..m).collect();
    let mut best = (0, perm.clone());
    let score = |p: &[usize]| (0..m).map(|i| agree[i][p[i]]).sum::<usize>();
    // Heap's algorithm.
    let mut c = vec![0; m];
    best.0 = score(&perm);
    let mut i = 0;
    while i < m {
        if c[i] < i {
            if i % 2 == 0 { perm.swap(0, i) } else { perm.swap(c[i], i) }
            let s = score(&perm);
            if s > best.0 {
                best = (s, perm.clone());
            }
            c[i] += 1;
            i = 0;
        } else {
            c[i] = 0;
            i += 1;
        }
    }
    fresh.iter().map(|&f| members[best.1[f]]).collect()
}

/// Swap the owners of two partitions while that removes clashes (3
/// neighbouring partitions not on 3 nodes). A swap keeps every node's share.
fn repair(mut owners: Vec<usize>) -> Vec<usize> {
    let size = owners.len();
    let bad = |o: &[usize], i: usize| !spread_ok(o, i % size, o[i % size]);
    // Clashes in the neighbourhoods of p and q.
    let near = |o: &[usize], p: usize, q: usize| {
        let mut idx: Vec<usize> = (0..2 * SPREAD - 1).flat_map(|d| [(p + size + d + 1 - SPREAD) % size, (q + size + d + 1 - SPREAD) % size]).collect();
        idx.sort();
        idx.dedup();
        idx.iter().filter(|&&i| bad(o, i)).count()
    };
    for _ in 0..size * 2 {
        let Some(p) = (0..size).find(|&p| bad(&owners, p)) else { break };
        let mut fixed = false;
        for q in 0..size {
            if owners[q] == owners[p] {
                continue;
            }
            let before = near(&owners, p, q);
            owners.swap(p, q);
            if near(&owners, p, q) < before {
                fixed = true;
                break;
            }
            owners.swap(p, q);
        }
        if !fixed {
            break;
        }
    }
    owners
}

/// True if any 3 neighbouring partitions are on 3 different nodes.
pub fn spread_holds(owners: &[usize]) -> bool {
    (0..owners.len()).all(|p| spread_ok(owners, p, owners[p]))
}

/// Position of a hash round the ring, from 0 up to 1, for drawing.
pub fn ring_fraction(hash: &[u8; 20]) -> f64 {
    let top = u64::from_be_bytes(hash[..8].try_into().unwrap());
    (top >> 11) as f64 / (1u64 << 53) as f64
}

fn to_decimal(be: &[u8]) -> String {
    let mut n = be.to_vec();
    let mut digits = Vec::new();
    while n.iter().any(|&b| b != 0) {
        let mut rem = 0u32;
        for b in n.iter_mut() {
            let cur = rem << 8 | *b as u32;
            *b = (cur / 10) as u8;
            rem = cur % 10;
        }
        digits.push(b'0' + rem as u8);
    }
    if digits.is_empty() {
        return "0".into();
    }
    digits.reverse();
    String::from_utf8(digits).unwrap()
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::Ring;

    #[test]
    fn neighbouring_partitions_are_on_different_nodes() {
        for nodes in 3..=8 {
            for size in [8, 16, 32, 64, 128] {
                let owners = Ring::new(size, nodes).unwrap().owners;
                // With 3 nodes this is only possible when 3 divides the ring size.
                if nodes == 3 && size % 3 != 0 {
                    continue;
                }
                for p in 0..size {
                    let w = [owners[p], owners[(p + 1) % size], owners[(p + 2) % size]];
                    assert!(w[0] != w[1] && w[1] != w[2] && w[0] != w[2], "nodes {nodes}, size {size}, at {p}: {owners:?}");
                }
                let mut counts = vec![0; nodes];
                owners.iter().for_each(|&o| counts[o] += 1);
                assert!(counts.iter().max().unwrap() - counts.iter().min().unwrap() <= 1, "unequal shares {counts:?}");
            }
        }
    }
}
