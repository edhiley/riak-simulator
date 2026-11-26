//! Prints the event trace of a PUT sent to a node with no replica, then a GET.
use riak_sim_core::sim::Sim;

fn main() {
    let mut sim = Sim::new(5, 32, 1).unwrap();
    sim.submit(4, "PUT", "/buckets/users/keys/alice", vec![("Content-Type".into(), "text/plain".into())], b"Alice".to_vec());
    sim.run_until_idle(10_000);
    sim.submit(1, "GET", "/buckets/users/keys/alice", vec![], vec![]);
    sim.run_until_idle(10_000);
    for e in sim.take_events() {
        println!("{:>7.3} ms  #{}  {}", e.at as f64 / 1000.0, e.req, e.text);
    }
}
