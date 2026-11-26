//! Nodes joining and leaving: partitions move, nothing is lost.

use riak_sim_core::ring::{spread_holds, Ring};
use riak_sim_core::sim::Sim;

fn idle(sim: &mut Sim) {
    sim.run_until_idle(2_000_000);
    assert!(sim.next_time().is_none(), "simulation did not go idle");
}

fn put(sim: &mut Sim, key: &str, value: &str) {
    sim.submit_to(None, "PUT", &format!("/buckets/b/keys/{key}"), vec![("Content-Type".into(), "text/plain".into())], value.as_bytes().to_vec());
}

/// Read every key with r=3 and check its value.
fn check_all(sim: &mut Sim, expect: &[(String, String)]) {
    sim.take_responses();
    let ids: Vec<(u64, &String)> = expect.iter().map(|(k, v)| (sim.submit_to(None, "GET", &format!("/buckets/b/keys/{k}?r=3"), vec![], vec![]), v)).collect();
    idle(sim);
    let replies = sim.take_responses();
    for (id, v) in ids {
        let r = replies.iter().find(|r| r.req == id).unwrap();
        assert_eq!((r.status, &r.body), (200, v), "request {id}");
    }
}

fn join(sim: &mut Sim, n: usize) {
    sim.stage_join(n).unwrap();
    sim.commit().unwrap();
}

fn leave(sim: &mut Sim, n: usize) {
    sim.stage_leave(n).unwrap();
    sim.commit().unwrap();
}

fn balanced(sim: &Sim) -> bool {
    let counts: Vec<usize> = sim.members().iter().map(|m| sim.ring().owners.iter().filter(|o| *o == m).count()).collect();
    counts.iter().max().unwrap() - counts.iter().min().unwrap() <= 1
}

fn no_moving(sim: &Sim) -> bool {
    sim.snapshot().vnodes.iter().all(|v| !v.moving && !v.fallback)
}

#[test]
fn join_then_leave_keeps_every_key() {
    let mut sim = Sim::new(5, 32, 1).unwrap();
    let mut keys = vec![];
    for i in 0..40 {
        put(&mut sim, &format!("k{i}"), &format!("v{i}"));
        keys.push((format!("k{i}"), format!("v{i}")));
    }
    idle(&mut sim);
    let before = sim.ring().owners.clone();
    join(&mut sim, 5);
    idle(&mut sim);
    assert_eq!(sim.members(), vec![0, 1, 2, 3, 4, 5]);
    assert!(balanced(&sim) && no_moving(&sim));
    let moved = before.iter().zip(&sim.ring().owners).filter(|(a, b)| a != b).count();
    assert!(moved <= 6, "only the new node's share moves, not {moved}");
    check_all(&mut sim, &keys);
    leave(&mut sim, 1);
    idle(&mut sim);
    assert_eq!(sim.members(), vec![0, 2, 3, 4, 5]);
    assert!(!sim.ring().owners.contains(&1));
    assert!(balanced(&sim) && no_moving(&sim));
    check_all(&mut sim, &keys);
}

#[test]
fn writes_during_moves_are_not_lost() {
    let mut sim = Sim::new(4, 32, 2).unwrap();
    let mut latest = std::collections::BTreeMap::new();
    for i in 0..20 {
        put(&mut sim, &format!("k{i}"), "old");
        latest.insert(format!("k{i}"), "old".to_string());
    }
    idle(&mut sim);
    join(&mut sim, 4);
    // Keep writing while partitions move.
    for round in 0..40 {
        let k = format!("k{}", round % 20);
        let v = format!("new{round}");
        put(&mut sim, &k, &v);
        latest.insert(k, v);
        for _ in 0..30 {
            sim.step();
        }
        // Let each write finish before the next one to the same key.
        if round % 20 == 19 {
            idle(&mut sim);
        }
    }
    idle(&mut sim);
    assert!(no_moving(&sim));
    let expect: Vec<(String, String)> = latest.into_iter().collect();
    check_all(&mut sim, &expect);
}

#[test]
fn a_counter_keeps_its_count_across_a_move() {
    let mut sim = Sim::new(3, 16, 3).unwrap();
    let path = "/types/counters/buckets/s/datatypes/c";
    for _ in 0..5 {
        sim.submit_to(None, "POST", path, vec![], br#"{"increment": 1}"#.to_vec());
        idle(&mut sim);
    }
    join(&mut sim, 3);
    for _ in 0..5 {
        sim.submit_to(None, "POST", path, vec![], br#"{"increment": 1}"#.to_vec());
        for _ in 0..20 {
            sim.step();
        }
    }
    idle(&mut sim);
    sim.take_responses();
    let id = sim.submit_to(None, "GET", &format!("{path}?r=3"), vec![], vec![]);
    idle(&mut sim);
    let r = sim.take_responses().into_iter().find(|r| r.req == id).unwrap();
    assert_eq!(r.body, r#"{"type":"counter","value":10}"#);
}

#[test]
fn a_node_stopping_mid_move_holds_the_move_until_it_starts() {
    let mut sim = Sim::new(5, 32, 4).unwrap();
    let keys: Vec<(String, String)> = (0..20).map(|i| (format!("k{i}"), format!("v{i}"))).collect();
    for (k, v) in &keys {
        put(&mut sim, k, v);
    }
    idle(&mut sim);
    join(&mut sim, 5);
    for _ in 0..5 {
        sim.step();
    }
    assert!(sim.stop_node(5));
    idle(&mut sim);
    assert!(sim.snapshot().ring_version > 0);
    assert!(sim.start_node(5));
    idle(&mut sim);
    assert!(balanced(&sim) && no_moving(&sim));
    check_all(&mut sim, &keys);
}

/// A leaving node that keeps copies for a stopped node cannot go until it
/// has handed them back.
#[test]
fn a_leaver_with_fallback_copies_waits_for_them_to_go_home() {
    let mut sim = Sim::new(5, 32, 5).unwrap();
    assert!(sim.stop_node(0));
    let keys: Vec<(String, String)> = (0..30).map(|i| (format!("k{i}"), format!("v{i}"))).collect();
    for (k, v) in &keys {
        put(&mut sim, k, v);
    }
    idle(&mut sim);
    // The node that hosts the most fallback copies leaves.
    let snap = sim.snapshot();
    let host = (1..5).max_by_key(|n| snap.vnodes.iter().filter(|v| v.fallback && v.owner == *n).map(|v| v.keys).sum::<usize>()).unwrap();
    drop(snap);
    leave(&mut sim, host);
    idle(&mut sim);
    assert!(sim.members().contains(&host), "left while holding copies for a stopped node");
    assert!(sim.start_node(0));
    idle(&mut sim);
    assert!(!sim.members().contains(&host));
    assert!(no_moving(&sim));
    check_all(&mut sim, &keys);
}

#[test]
fn staging_rules() {
    let mut sim = Sim::new(3, 16, 6).unwrap();
    assert!(sim.stage_join(1).is_err(), "already a member");
    assert!(sim.stop_node(2));
    assert!(sim.stage_leave(2).is_err(), "stopped");
    assert!(sim.commit().is_err(), "nothing staged");
    assert!(sim.plan().contains("no staged"));
    sim.stage_join(3).unwrap();
    assert!(sim.plan().contains("join           'node4'"));
    sim.commit().unwrap();
    sim.stage_leave(0).unwrap();
    assert!(sim.commit().is_err(), "moves still running");
}

/// After joins and leaves, any 3 neighbouring partitions are on 3 nodes
/// whenever a fresh ring of that shape has that property.
#[test]
fn spread_holds_after_changes() {
    for size in [16, 32, 64] {
        let mut sim = Sim::new(4, size, 7).unwrap();
        let mut rng = 99u64;
        for _ in 0..12 {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let members = sim.members();
            let free: Vec<usize> = (0..8).filter(|n| !members.contains(n)).collect();
            if (rng >> 33) % 2 == 0 && !free.is_empty() {
                join(&mut sim, free[(rng >> 40) as usize % free.len()]);
            } else if members.len() > 3 {
                leave(&mut sim, members[(rng >> 40) as usize % members.len()]);
            }
            idle(&mut sim);
            let m = sim.members().len();
            assert!(balanced(&sim), "size {size}, {m} nodes");
            if spread_holds(&Ring::fresh(size, m)) {
                assert!(spread_holds(&sim.ring().owners), "size {size}, {m} nodes: {:?}", sim.ring().owners);
            }
        }
    }
}

#[test]
fn traffic_with_joins_leaves_and_stops() {
    let mut sim = Sim::new(4, 32, 11).unwrap();
    let mut rng = 7u64;
    let mut next = move || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };
    let mut latest = std::collections::BTreeMap::new();
    for round in 0..300 {
        let k = format!("k{}", next() % 25);
        let v = format!("v{round}");
        put(&mut sim, &k, &v);
        latest.insert(k, v);
        match round % 50 {
            10 => {
                let free: Vec<usize> = (0..8).filter(|n| !sim.members().contains(n)).collect();
                if sim.stage_join(free[0]).is_ok() {
                    let _ = sim.commit();
                }
            }
            30 => {
                let m = sim.members();
                let n = m[(next() % m.len() as u64) as usize];
                if sim.stage_leave(n).is_ok() && sim.commit().is_err() {
                    // Moves still running: try later.
                }
            }
            40 => {
                let m = sim.members();
                let n = m[(next() % m.len() as u64) as usize];
                sim.stop_node(n);
            }
            45 => {
                for n in sim.members() {
                    sim.start_node(n);
                }
            }
            _ => {}
        }
        // One write to a key at a time, so the latest value is well defined.
        idle(&mut sim);
    }
    let _ = sim.commit();
    for n in sim.members() {
        sim.start_node(n);
    }
    idle(&mut sim);
    assert!(no_moving(&sim));
    let expect: Vec<(String, String)> = latest.into_iter().collect();
    check_all(&mut sim, &expect);
}

#[test]
fn a_join_with_data_moves_quickly_and_resends_only_changes() {
    let mut sim = Sim::new(5, 32, 1).unwrap();
    for i in 0..1000 {
        put(&mut sim, &format!("k{i}"), "v");
    }
    idle(&mut sim);
    let start = sim.now();
    join(&mut sim, 5);
    // Writes keep coming while the partitions move.
    for i in 0..200 {
        put(&mut sim, &format!("k{i}"), "w");
    }
    idle(&mut sim);
    assert!(balanced(&sim) && no_moving(&sim));
    // About 30 objects for each moving partition, at 20 µs each, 2 at a time
    // for each node: well under 20 ms of simulated time.
    assert!(sim.now() - start < 20_000, "the join took {} µs", sim.now() - start);
}
