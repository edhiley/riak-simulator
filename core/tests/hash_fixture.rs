//! Key placement checked against a real OpenRiak 3.4.1 node.
//!
//! `fixtures/hash-real-3.4.1.tsv` was produced by `validation/make-hash-fixture.sh`:
//! the node's own `riak_core_util:chash_key/2` and `chash:successors/2` for
//! 1,530 bucket/key pairs over default, typed and explicitly `default`-typed
//! buckets, at ring sizes 8 and 64.

use riak_sim_core::ring::{hex, key_hash, Ring};

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

#[test]
fn hashes_and_preflists_match_real_node() {
    let text = include_str!("fixtures/hash-real-3.4.1.tsv");
    let ring8 = Ring::new(8, 1).unwrap();
    let ring64 = Ring::new(64, 1).unwrap();
    let mut rows = 0;
    for line in text.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = line.split('\t').collect();
        let t = unhex(f[0]);
        let (b, k) = (unhex(f[1]), unhex(f[2]));
        // Raw hash of exactly the term given, including {<<"default">>, B}.
        let h = key_hash((!f[0].is_empty()).then_some(&t[..]), &b, &k);
        assert_eq!(hex(&h), f[3], "hash of {f:?}");
        for (ring, want) in [(&ring8, f[4]), (&ring64, f[5])] {
            let got: Vec<String> = ring.preflist(&h, 3).iter().map(|&p| ring.partition_index(p)).collect();
            assert_eq!(got.join(","), want, "preflist at ring {} for {f:?}", ring.size());
        }
        rows += 1;
    }
    assert_eq!(rows, 1530);
}
