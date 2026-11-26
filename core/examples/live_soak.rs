//! Diagnose live mode: bursts of writes and reads while switches cycle
//! through the nodes, as the browser test does. Prints what is left.
use riak_sim_core::sim::{Sim, Timing};

fn main() {
    let mut sim = Sim::new(5, 32, 3).unwrap();
    sim.set_timing(Timing::REALISTIC);
    let mut key = 1;
    for i in 1..=120u64 {
        if i % 3 == 0 {
            sim.submit_to(None, "GET", &format!("/buckets/users/keys/user{key}"), vec![], vec![]);
        } else {
            for _ in 0..9 {
                sim.submit_to(None, "PUT", &format!("/buckets/users/keys/user{key}"), vec![("Content-Type".into(), "text/plain".into())], b"v".to_vec());
                key = key % 9 + 1;
                let t = sim.now() + 150;
                sim.advance_to(t, 100_000);
            }
        }
        if i % 10 == 0 {
            let n = ((i / 10) % 5) as usize;
            if sim.is_down(n) { sim.start_node(n); } else { sim.stop_node(n); }
        }
        let t = sim.now() + 1000;
        sim.advance_to(t, 100_000);
    }
    for n in 0..5 { sim.start_node(n); }
    let ran = sim.run_until_idle(2_000_000);
    println!("actions after start: {ran}, idle: {}", sim.next_time().is_none());
    let snap = sim.snapshot();
    for v in snap.vnodes.iter().filter(|v| v.fallback) {
        println!("fallback {} on node{} keys {} busy {} mailbox {}", v.partition, v.owner + 1, v.keys, v.busy.is_some(), v.mailbox.len());
    }
    println!("messages {}", snap.messages.len());
    for m in snap.messages.iter().take(10) { println!("  {:?} -> {:?} {}", m.from, m.to, m.label); }
    drop(snap);
    let ev = sim.take_events();
    for e in ev.iter().rev().take(8).rev() { println!("EV {}: {}", e.req, &e.text[..e.text.len().min(200)]); }
}
