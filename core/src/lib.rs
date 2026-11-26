//! A simulated Riak KV cluster with no I/O, clock or randomness of its own,
//! so the same inputs always give the same result natively and in WebAssembly.

pub mod crdt;
pub mod leveled;
pub mod ring;
pub mod siblings;
mod sha1;
pub mod sim;

use ring::{hex, ring_fraction, BucketRef, Ring};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ReplicaInfo {
    pub partition: usize,
    /// The partition's index on the 2^160 ring, in decimal, as Riak reports it.
    pub index: String,
    pub node: usize,
}

#[derive(Debug, Serialize)]
pub struct KeyInfo {
    pub bucket: BucketRef,
    pub key: String,
    pub hash: String,
    /// Position of the hash round the ring, 0 up to 1.
    pub position: f64,
    pub preflist: Vec<ReplicaInfo>,
}

#[derive(Debug, Serialize)]
pub struct RingInfo {
    /// How many nodes are in the cluster.
    pub nodes: usize,
    /// The nodes in the cluster, by number from 0.
    pub members: Vec<usize>,
    /// The most nodes a cluster can have; vnode ids depend on it.
    pub max_nodes: usize,
    pub size: usize,
    pub owners: Vec<usize>,
}

pub fn ring_info(ring: &Ring, members: Vec<usize>) -> RingInfo {
    RingInfo { nodes: members.len(), members, max_nodes: sim::MAX_NODES, size: ring.size(), owners: ring.owners.clone() }
}

pub fn key_info(ring: &Ring, bucket: &BucketRef, key: &str, n_val: usize) -> KeyInfo {
    let hash = bucket.hash(key);
    let preflist = ring
        .preflist(&hash, n_val)
        .into_iter()
        .map(|p| ReplicaInfo { partition: p, index: ring.partition_index(p), node: ring.owners[p] })
        .collect();
    KeyInfo { bucket: bucket.clone(), key: key.to_owned(), hash: hex(&hash), position: ring_fraction(&hash), preflist }
}
