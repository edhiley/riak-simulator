//! Prints the step-by-step story for each kind of request, for reviewing the
//! wording. Run: cargo run -p riak-sim-core --example scenarios
use riak_sim_core::sim::{Payload, Sim, Timing};

fn put(sim: &mut Sim, node: Option<usize>, path: &str, body: &str) {
    sim.submit_to(node, "PUT", path, vec![("Content-Type".into(), "text/plain".into())], body.as_bytes().to_vec());
}

fn print(title: &str, sim: &mut Sim) {
    println!("\n=== {title}");
    let mut n = 0;
    for e in sim.take_events() {
        if e.summary {
            println!("  RESULT: {}", e.text);
        } else {
            n += 1;
            println!("  {n:>2}. {}", e.text);
        }
    }
}

fn main() {
    // 5 nodes, 32 partitions: users/alice is on partitions 15, 16, 17 (node1,
    // node2, node3). The seed decides where the load balancer starts.
    let mut sim = Sim::new(5, 32, 1).unwrap();
    put(&mut sim, None, "/buckets/users/keys/alice", "Alice");
    sim.run_until_idle(10_000);
    print("PUT through the load balancer", &mut sim);

    put(&mut sim, Some(4), "/buckets/users/keys/alice", "Alice Smith");
    sim.run_until_idle(10_000);
    print("PUT sent to node5, which holds no copy (forwarded)", &mut sim);

    put(&mut sim, Some(0), "/buckets/users/keys/alice", "Alice S.");
    sim.run_until_idle(10_000);
    print("PUT sent to node1, which holds a copy", &mut sim);

    sim.submit_to(Some(1), "GET", "/buckets/users/keys/alice", vec![], vec![]);
    sim.run_until_idle(10_000);
    print("GET sent to node2", &mut sim);

    sim.submit_to(None, "GET", "/buckets/users/keys/carol", vec![], vec![]);
    sim.run_until_idle(10_000);
    print("GET of a key that does not exist", &mut sim);

    sim.submit_to(None, "DELETE", "/buckets/users/keys/alice", vec![], vec![]);
    sim.run_until_idle(10_000);
    print("DELETE through the load balancer", &mut sim);

    sim.submit_to(None, "GET", "/stats", vec![], vec![]);
    sim.run_until_idle(10_000);
    print("Unsupported request", &mut sim);

    sim.submit_to(Some(2), "GET", "/ping", vec![], vec![]);
    sim.run_until_idle(10_000);
    print("Ping sent to node3", &mut sim);

    // w=3, one replica's write held, then released.
    put(&mut sim, None, "/buckets/users/keys/alice?w=3&dw=3", "Alice W3");
    let held = loop {
        if let Some(m) = sim.snapshot().messages.iter().find(|m| matches!(m.payload, Payload::Put)).map(|m| m.id) {
            sim.hold(m);
            break m;
        }
        sim.step();
    };
    sim.run_until_idle(10_000);
    print("PUT with w=3, one copy's write held (no reply yet)", &mut sim);
    sim.release(held);
    sim.run_until_idle(10_000);
    print("... after the held write is released", &mut sim);

    // node3 holds a copy of alice. Stop it, write, read, then start it again.
    assert!(sim.stop_node(2));
    put(&mut sim, None, "/buckets/users/keys/alice", "Alice (node3 stopped)");
    sim.run_until_idle(10_000);
    print("PUT with node3 stopped (fallback vnode)", &mut sim);
    sim.submit_to(Some(1), "GET", "/buckets/users/keys/alice", vec![], vec![]);
    sim.run_until_idle(10_000);
    print("GET with node3 stopped", &mut sim);
    sim.submit_to(Some(2), "GET", "/ping", vec![], vec![]);
    print("Request sent to node3 while it is stopped", &mut sim);
    assert!(sim.start_node(2));
    sim.run_until_idle(10_000);
    print("node3 starts again: hinted handoff", &mut sim);

    sim.submit_to(None, "POST", "/types/counters/buckets/stats/datatypes/visits", vec![], br#"{"increment": 1}"#.to_vec());
    sim.run_until_idle(10_000);
    print("Counter: increment by 1", &mut sim);
    sim.submit_to(None, "POST", "/types/sets/buckets/fruit/datatypes/basket", vec![], br#"{"add_all": ["apple", "pear"]}"#.to_vec());
    sim.run_until_idle(10_000);
    print("Set: add two elements", &mut sim);
    sim.submit_to(None, "GET", "/types/sets/buckets/fruit/datatypes/basket", vec![], vec![]);
    sim.run_until_idle(10_000);
    print("Set: read", &mut sim);

    sim.set_timing(Timing::REALISTIC);
    put(&mut sim, Some(4), "/buckets/users/keys/bob", "Bob");
    sim.run_until_idle(10_000);
    print("PUT with realistic timing (forwarded)", &mut sim);
}
