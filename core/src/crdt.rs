//! Riak data types (CRDTs): the counter (riak_dt_pncounter) and the set
//! (riak_dt_orswot). Each update is made by one vnode, the actor. Copies
//! merge without losing an update, so data types never have siblings.
//!
//! Not modelled: maps, registers, flags and HyperLogLog; the opaque context
//! is a simulator encoding, not Riak's binary format; removing an element a
//! copy has not seen is ignored, where Riak returns a precondition error.
use std::collections::BTreeMap;

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DataType {
    Counter,
    Set,
}

impl DataType {
    /// The bucket types the simulator has, as an operator would create them
    /// with `riak admin bucket-type create counters '{"props":{"datatype":"counter"}}'`.
    pub fn of_bucket_type(name: &str) -> Option<DataType> {
        match name {
            "counters" => Some(DataType::Counter),
            "sets" => Some(DataType::Set),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            DataType::Counter => "counter",
            DataType::Set => "set",
        }
    }
}

/// An update from a client.
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    /// Add to a counter; negative to take away.
    Count(i64),
    Add(Vec<String>),
    Remove(Vec<String>),
}

type Dots = BTreeMap<usize, u64>;

#[derive(Clone, Debug, PartialEq)]
pub enum Crdt {
    /// For each actor: the total it added and the total it took away.
    Counter(BTreeMap<usize, (u64, u64)>),
    /// The clock of every add seen, and for each element the adds (dots)
    /// that put it there.
    Set { clock: Dots, entries: BTreeMap<String, Dots> },
}

impl Crdt {
    pub fn new(t: DataType) -> Crdt {
        match t {
            DataType::Counter => Crdt::Counter(BTreeMap::new()),
            DataType::Set => Crdt::Set { clock: BTreeMap::new(), entries: BTreeMap::new() },
        }
    }

    /// Apply an update made by `actor`.
    pub fn apply(&mut self, actor: usize, op: &Op) {
        match (self, op) {
            (Crdt::Counter(counts), Op::Count(n)) => {
                let e = counts.entry(actor).or_default();
                if *n >= 0 {
                    e.0 += *n as u64;
                } else {
                    e.1 += n.unsigned_abs();
                }
            }
            (Crdt::Set { clock, entries }, Op::Add(elems)) => {
                // An add gives the element one new dot, replacing its old ones.
                for e in elems {
                    let c = clock.entry(actor).or_default();
                    *c += 1;
                    entries.insert(e.clone(), BTreeMap::from([(actor, *c)]));
                }
            }
            (Crdt::Set { entries, .. }, Op::Remove(elems)) => {
                for e in elems {
                    entries.remove(e);
                }
            }
            _ => {}
        }
    }

    /// Merge another copy into this one.
    pub fn merge(&mut self, other: &Crdt) {
        match (self, other) {
            (Crdt::Counter(a), Crdt::Counter(b)) => {
                for (actor, (p, n)) in b {
                    let e = a.entry(*actor).or_default();
                    e.0 = e.0.max(*p);
                    e.1 = e.1.max(*n);
                }
            }
            (Crdt::Set { clock: ca, entries: ea }, Crdt::Set { clock: cb, entries: eb }) => {
                let seen = |clock: &Dots, actor: &usize, n: &u64| clock.get(actor).is_some_and(|c| c >= n);
                let mut merged: BTreeMap<String, Dots> = BTreeMap::new();
                let names: std::collections::BTreeSet<&String> = ea.keys().chain(eb.keys()).collect();
                for name in names {
                    let empty = BTreeMap::new();
                    let da = ea.get(name).unwrap_or(&empty);
                    let db = eb.get(name).unwrap_or(&empty);
                    let mut keep = Dots::new();
                    // A dot both copies have stays. A dot only one copy has
                    // stays if the other copy has not seen it; if it has, the
                    // other copy removed the element.
                    for (actor, n) in da {
                        if db.get(actor) == Some(n) || !seen(cb, actor, n) {
                            keep.insert(*actor, *n);
                        }
                    }
                    for (actor, n) in db {
                        if !seen(ca, actor, n) {
                            keep.insert(*actor, *n);
                        }
                    }
                    if !keep.is_empty() {
                        merged.insert(name.clone(), keep);
                    }
                }
                for (actor, n) in cb {
                    let c = ca.entry(*actor).or_default();
                    *c = (*c).max(*n);
                }
                *ea = merged;
            }
            _ => {}
        }
    }

    pub fn data_type(&self) -> DataType {
        match self {
            Crdt::Counter(_) => DataType::Counter,
            Crdt::Set { .. } => DataType::Set,
        }
    }

    pub fn counter_value(&self) -> i64 {
        match self {
            Crdt::Counter(c) => c.values().map(|(p, n)| *p as i64 - *n as i64).sum(),
            _ => 0,
        }
    }

    pub fn set_value(&self) -> Vec<String> {
        match self {
            Crdt::Set { entries, .. } => entries.keys().cloned().collect(),
            _ => vec![],
        }
    }

    /// The value as a person reads it: "5", or "apple, pear".
    pub fn describe(&self) -> String {
        match self {
            Crdt::Counter(_) => self.counter_value().to_string(),
            Crdt::Set { .. } => {
                let v = self.set_value();
                if v.is_empty() { "empty".into() } else { v.join(", ") }
            }
        }
    }

    /// The JSON body of a fetch, as riak_kv_wm_crdt returns it.
    pub fn fetch_json(&self) -> String {
        match self {
            Crdt::Counter(_) => format!("{{\"type\":\"counter\",\"value\":{}}}", self.counter_value()),
            Crdt::Set { clock, .. } => {
                let items: Vec<String> = self.set_value().iter().map(|e| json_string(e)).collect();
                let ctx: String = clock.iter().map(|(a, n)| format!("{a}.{n}")).collect::<Vec<_>>().join("-");
                format!("{{\"type\":\"set\",\"value\":[{}],\"context\":{}}}", items.join(","), json_string(&base64(ctx.as_bytes())))
            }
        }
    }
}

/// Parse an update body: {"increment": 1}, {"decrement": 1}, {"add": "x"},
/// {"add_all": ["x"]}, {"remove": "x"} or {"remove_all": ["x"]}. A
/// "context" field is accepted and not used.
pub fn parse_op(t: DataType, body: &str) -> Result<Op, String> {
    let fields = parse_object(body).ok_or_else(|| "The body must be a JSON object.".to_string())?;
    let mut op = None;
    for (name, value) in fields {
        let next = match (t, name.as_str(), value) {
            (DataType::Counter, "increment", Json::Number(n)) => Op::Count(n),
            (DataType::Counter, "decrement", Json::Number(n)) => Op::Count(-n),
            (DataType::Set, "add", Json::String(s)) => Op::Add(vec![s]),
            (DataType::Set, "add_all", Json::Strings(v)) => Op::Add(v),
            (DataType::Set, "remove", Json::String(s)) => Op::Remove(vec![s]),
            (DataType::Set, "remove_all", Json::Strings(v)) => Op::Remove(v),
            (_, "context", _) => continue,
            (DataType::Counter, f, _) => return Err(format!("A counter takes \"increment\" or \"decrement\" with a whole number, not \"{f}\".")),
            (DataType::Set, f, _) => return Err(format!("A set takes \"add\", \"add_all\", \"remove\" or \"remove_all\", not \"{f}\".")),
        };
        op = Some(next);
    }
    op.ok_or_else(|| "The body has no operation.".to_string())
}

enum Json {
    Number(i64),
    String(String),
    Strings(Vec<String>),
}

/// A small parser for the flat objects that updates use.
fn parse_object(s: &str) -> Option<Vec<(String, Json)>> {
    let mut p = Parser { b: s.as_bytes(), i: 0 };
    p.ws();
    p.eat(b'{')?;
    let mut out = vec![];
    loop {
        p.ws();
        if p.eat(b'}').is_some() {
            break;
        }
        let name = p.string()?;
        p.ws();
        p.eat(b':')?;
        p.ws();
        let value = match p.peek()? {
            b'"' => Json::String(p.string()?),
            b'[' => {
                p.i += 1;
                let mut v = vec![];
                loop {
                    p.ws();
                    if p.eat(b']').is_some() {
                        break;
                    }
                    v.push(p.string()?);
                    p.ws();
                    if p.eat(b',').is_none() {
                        p.ws();
                        p.eat(b']')?;
                        break;
                    }
                }
                Json::Strings(v)
            }
            _ => Json::Number(p.number()?),
        };
        out.push((name, value));
        p.ws();
        if p.eat(b',').is_none() {
            p.ws();
            p.eat(b'}')?;
            break;
        }
    }
    p.ws();
    (p.i == p.b.len()).then_some(out)
}

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }
    fn ws(&mut self) {
        while self.peek().is_some_and(|c| c.is_ascii_whitespace()) {
            self.i += 1;
        }
    }
    fn eat(&mut self, c: u8) -> Option<()> {
        (self.peek()? == c).then(|| self.i += 1)
    }
    fn string(&mut self) -> Option<String> {
        self.eat(b'"')?;
        let mut out = Vec::new();
        loop {
            let c = self.peek()?;
            self.i += 1;
            match c {
                b'"' => return String::from_utf8(out).ok(),
                b'\\' => {
                    let e = self.peek()?;
                    self.i += 1;
                    out.push(match e {
                        b'n' => b'\n',
                        b't' => b'\t',
                        other => other,
                    });
                }
                other => out.push(other),
            }
        }
    }
    fn number(&mut self) -> Option<i64> {
        let start = self.i;
        if self.peek() == Some(b'-') {
            self.i += 1;
        }
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.i += 1;
        }
        std::str::from_utf8(&self.b[start..self.i]).ok()?.parse().ok()
    }
}

fn json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Decode base64 with padding; None if it is not valid.
pub fn unbase64(s: &str) -> Option<Vec<u8>> {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let s = s.as_bytes();
    if s.len() % 4 != 0 {
        return None;
    }
    let mut out = Vec::new();
    for chunk in s.chunks(4) {
        let mut n = 0u32;
        let mut pad = 0;
        for &c in chunk {
            n <<= 6;
            if c == b'=' {
                pad += 1;
            } else {
                n |= T.iter().position(|&t| t == c)? as u32;
            }
        }
        let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&bytes[..3 - pad]);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counter_merge_keeps_each_actors_largest_count() {
        let mut a = Crdt::new(DataType::Counter);
        let mut b = a.clone();
        a.apply(1, &Op::Count(2));
        b.apply(2, &Op::Count(3));
        b.apply(2, &Op::Count(-1));
        let mut m = a.clone();
        m.merge(&b);
        assert_eq!(m.counter_value(), 4);
        m.merge(&a);
        assert_eq!(m.counter_value(), 4, "merging again changes nothing");
    }

    #[test]
    fn set_add_wins_over_a_concurrent_remove_and_a_seen_remove_stays() {
        let mut base = Crdt::new(DataType::Set);
        base.apply(1, &Op::Add(vec!["apple".into()]));
        let mut a = base.clone();
        let mut b = base.clone();
        a.apply(1, &Op::Remove(vec!["apple".into()]));
        b.apply(2, &Op::Add(vec!["apple".into(), "pear".into()]));
        let mut m = a.clone();
        m.merge(&b);
        assert_eq!(m.set_value(), vec!["apple", "pear"], "concurrent add wins");
        // A remove that saw the add wins.
        let mut c = m.clone();
        c.apply(3, &Op::Remove(vec!["pear".into()]));
        let mut n = m.clone();
        n.merge(&c);
        assert_eq!(n.set_value(), vec!["apple"]);
    }

    #[test]
    fn parses_updates() {
        assert_eq!(parse_op(DataType::Counter, r#"{"increment": 5}"#), Ok(Op::Count(5)));
        assert_eq!(parse_op(DataType::Counter, r#"{"decrement":2}"#), Ok(Op::Count(-2)));
        assert_eq!(parse_op(DataType::Set, r#"{"add_all": ["a", "b"], "context": "x"}"#), Ok(Op::Add(vec!["a".into(), "b".into()])));
        assert!(parse_op(DataType::Set, r#"{"increment": 1}"#).is_err());
        assert!(parse_op(DataType::Counter, "nope").is_err());
    }
}
