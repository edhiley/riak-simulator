//! WebAssembly wrapper around the simulation core. Values cross the boundary
//! as JSON strings, which keeps the interface small and the core free of
//! wasm-bindgen. Ids and the seed are 32-bit here so JavaScript sees plain numbers.

use riak_sim_core::{key_info, ring::BucketRef, ring_info, sim};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Sim {
    sim: sim::Sim,
}

fn json<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_string(v).unwrap()
}

#[wasm_bindgen]
impl Sim {
    #[wasm_bindgen(constructor)]
    pub fn new(nodes: usize, ring_size: usize, seed: u32) -> Result<Sim, String> {
        Ok(Sim { sim: sim::Sim::new(nodes, ring_size, seed as u64)? })
    }

    /// `{nodes, size, owners}`
    pub fn ring(&self) -> String {
        json(&ring_info(self.sim.ring(), self.sim.members()))
    }

    /// Where a key is stored. An empty `bucket_type` means none.
    pub fn key_info(&self, bucket_type: &str, bucket: &str, key: &str) -> String {
        let b = BucketRef::new(Some(bucket_type).filter(|t| !t.is_empty()), bucket);
        json(&key_info(self.sim.ring(), &b, key, sim::N_VAL))
    }

    /// Send an HTTP request to node `node` (from 0), or to the load balancer
    /// if `node` is negative. `headers_json` is a list of `[name, value]`
    /// pairs. Returns the request id.
    pub fn submit(&mut self, node: i32, method: &str, path: &str, headers_json: &str, body: &[u8]) -> Result<u32, String> {
        let headers: Vec<(String, String)> = serde_json::from_str(headers_json).map_err(|e| e.to_string())?;
        let node = usize::try_from(node).ok();
        Ok(self.sim.submit_to(node, method, path, headers, body.to_vec()) as u32)
    }

    /// Send a request after `delay` microseconds of simulated time, as one
    /// of many clients would.
    pub fn submit_after(&mut self, delay: f64, node: i32, method: &str, path: &str, headers_json: &str, body: &[u8]) -> Result<u32, String> {
        let headers: Vec<(String, String)> = serde_json::from_str(headers_json).map_err(|e| e.to_string())?;
        let node = usize::try_from(node).ok();
        Ok(self.sim.submit_after(delay.max(0.0) as u64, node, method, path, headers, body.to_vec()) as u32)
    }

    pub fn now(&self) -> f64 {
        self.sim.now() as f64
    }

    /// Next scheduled time, or -1 if nothing is scheduled.
    pub fn next_time(&self) -> f64 {
        self.sim.next_time().map_or(-1.0, |t| t as f64)
    }

    pub fn step(&mut self) -> bool {
        self.sim.step()
    }

    pub fn advance_to(&mut self, until: f64, max_actions: usize) -> usize {
        self.sim.advance_to(until.max(0.0) as u64, max_actions)
    }

    pub fn hold(&mut self, msg: u32) -> bool {
        self.sim.hold(msg as u64)
    }

    pub fn release(&mut self, msg: u32) -> bool {
        self.sim.release(msg as u64)
    }

    pub fn slow(&mut self, msg: u32, factor: u32) -> bool {
        self.sim.slow(msg as u64, factor)
    }

    /// Stop a node (from 0). Only when no request is running.
    pub fn stop_node(&mut self, node: u32) -> bool {
        self.sim.stop_node(node as usize)
    }

    /// Start a stopped node. Returns -1 if it cannot start now, 0 if it
    /// starts with nothing to hand off, or the id of the hinted handoff.
    pub fn start_node(&mut self, node: u32) -> i32 {
        if !self.sim.start_node(node as usize) {
            return -1;
        }
        self.sim.last_handoff().map(|id| id as i32).unwrap_or(0)
    }

    /// `riak admin cluster join|leave|plan|commit`. `node` is the node the
    /// command runs on; `target` the node it names, or -1. Returns JSON:
    /// {"ok": bool, "text": string, "id": number | null}.
    pub fn cluster(&mut self, node: u32, action: &str, target: i32) -> String {
        let n = node as usize;
        let t = if target < 0 { n } else { target as usize };
        let (ok, text, id) = match action {
            "join" => match self.sim.stage_join(n) {
                Ok(s) => (true, s, None),
                Err(e) => (false, e, None),
            },
            "leave" => match self.sim.stage_leave(t) {
                Ok(s) => (true, s, None),
                Err(e) => (false, e, None),
            },
            "plan" => (true, self.sim.plan(), None),
            "commit" => match self.sim.commit() {
                Ok(id) => (true, "Cluster changes committed".to_string(), id),
                Err(e) => (false, e, None),
            },
            other => (false, format!("Unknown cluster command: {other}"), None),
        };
        serde_json::json!({ "ok": ok, "text": text, "id": id }).to_string()
    }

    /// Set the cluster-wide quorum defaults. Returns an error, or "".
    pub fn set_quorum(&mut self, r: u32, w: u32, dw: u32, pr: u32, pw: u32) -> String {
        let q = riak_sim_core::sim::Quorum { r: r as usize, w: w as usize, dw: dw as usize, pr: pr as usize, pw: pw as usize };
        self.sim.set_quorum(q).err().unwrap_or_default()
    }

    /// No request is running, so nodes can be stopped or started.
    pub fn quiet(&self) -> bool {
        self.sim.quiet()
    }

    /// Use realistic timing (true) or even timing (false) from now on.
    pub fn set_realistic_timing(&mut self, realistic: bool) {
        self.sim.set_timing(if realistic { sim::Timing::REALISTIC } else { sim::Timing::EVEN });
    }

    /// `{now, messages, vnodes, requests, idle}`
    pub fn snapshot(&self) -> String {
        json(&self.sim.snapshot())
    }

    /// The leveled store of one vnode, as JSON, or null for an unknown id.
    pub fn leveled(&self, vnode: usize) -> String {
        json(&self.sim.leveled(vnode))
    }

    /// Messages that arrived since the last call, as JSON.
    pub fn take_hops(&mut self) -> String {
        serde_json::to_string(&self.sim.take_hops()).unwrap()
    }

    pub fn take_events(&mut self) -> String {
        json(&self.sim.take_events())
    }

    pub fn take_responses(&mut self) -> String {
        json(&self.sim.take_responses())
    }

    /// Every outside input so far, with the time it was applied.
    pub fn inputs(&self) -> String {
        json(&self.sim.inputs())
    }
}
