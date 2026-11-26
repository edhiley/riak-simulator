//! Leveled, the backend inside each vnode, as in OpenRiak 3.4.1
//! (`leveled_bookie`, `leveled_inker`, `leveled_penciller`, and their clerks).
//!
//! The model keeps the structure and the rules, not the bytes:
//! - Each write gets a sequence number. The Inker appends the object to the
//!   active journal file. The Bookie puts the key and its sequence number in
//!   its ledger cache.
//! - When the ledger cache is full, the Bookie gives it to the Penciller,
//!   which keeps it in memory (the L0 cache).
//! - When the Penciller's memory is full, it writes it out as one level 0 SST
//!   file. The Penciller's clerk merges that file into level 1. A level with
//!   more files than its limit has one file, chosen at random, merged into
//!   the level below (`leveled_pmanifest:mergefile_selector/3`).
//! - Journal files roll at an object count. Then the Inker's clerk scores
//!   the older files by the share of objects still current and compacts the
//!   best run (`leveled_iclerk:score_run/2`).
//!
//! Every count is the real default divided by `SCALE`, so a few hundred keys
//! show several levels. Merges and compaction take no simulated time.
use serde::Serialize;
use std::collections::{BTreeMap, VecDeque};

/// Real sizes are this many times larger.
pub const SCALE: usize = 1000;
/// `leveled.cache_size`, 2000: keys the Bookie holds before it gives them to
/// the Penciller.
pub const CACHE_SIZE: usize = 2;
/// `leveled.penciller_cache_size`, 20000: keys the Penciller holds in memory
/// before it writes a level 0 file.
pub const PENCILLER_CACHE: usize = 20;
/// Keys in one SST file: 256 slots of 128 keys, 32768.
pub const SST_KEYS: usize = 33;
/// `leveled.journal_objectcount`, 200000: objects in a journal file before
/// it rolls.
pub const JOURNAL_OBJECTS: usize = 200;
/// Files each level may hold before one is merged down (LEVEL_SCALEFACTOR).
/// Not scaled: the number of files is what makes the shape.
pub const LEVEL_FILES: [usize; 8] = [0, 4, 16, 64, 384, 2304, 18432, usize::MAX];
/// `leveled.max_run_length`, `singlefile_compactionpercentage` and
/// `maxrunlength_compactionpercentage`.
pub const MAX_RUN_LENGTH: usize = 4;
pub const SINGLE_FILE_TARGET: f64 = 25.0;
pub const MAX_RUN_TARGET: f64 = 75.0;
/// Activity kept for the zoom view.
const EVENTS_KEPT: usize = 120;

/// A ledger key: the bucket (with its type) and the key. The ledger is in
/// this order, so one bucket's keys are together.
pub type Key = (String, String);

#[derive(Clone, Debug)]
struct Sst {
    id: u64,
    keys: BTreeMap<Key, u64>,
}

impl Sst {
    fn first(&self) -> &Key {
        self.keys.keys().next().unwrap()
    }
    fn last(&self) -> &Key {
        self.keys.keys().next_back().unwrap()
    }
    fn covers(&self, k: &Key) -> bool {
        self.first() <= k && k <= self.last()
    }
}

#[derive(Clone, Debug)]
struct JFile {
    id: u64,
    entries: Vec<(Key, u64)>,
}

/// One step of work, for the zoom view.
#[derive(Clone, Debug, Serialize)]
pub struct LEvent {
    pub at: u64,
    /// Who does it: bookie, inker, iclerk, penciller, pclerk.
    pub who: &'static str,
    /// What: journal, cache, push, l0, merge, roll, compact, head, get, fold.
    pub kind: &'static str,
    pub text: String,
    /// Why leveled does it this way.
    pub why: &'static str,
    /// The level a ledger step is at, if any.
    pub level: Option<usize>,
}

/// Why each kind of step happens the way it does.
pub fn why(kind: &str) -> &'static str {
    match kind {
        "journal" => "Appending in arrival order is one sequential write, the fastest kind of disk write. The value is written once and never moved by a merge: merges move only the small keys.",
        "cache" => "Keys wait in memory so the Penciller gets them in batches, not one at a time. A read for a key written a moment ago is answered from here.",
        "push" => "Handing over a batch keeps the Bookie quick: it never waits for the ledger to sort or write.",
        "l0" => "Sorting in memory and writing a whole file at once turns many small random writes into one sequential write.",
        "merge" => "Files in a level below 0 never overlap, so a read checks at most one file a level. Merging down keeps the upper levels small, so recent keys are found quickly.",
        "roll" => "A journal file is never changed after it rolls, so it can be compacted later without stopping writes.",
        "compact" => "Replaced values waste disk. Rewriting only files that are mostly waste keeps the cost low, and a run of up to 4 files lets one rewrite clear several of them.",
        "head" => "The ledger holds each object's metadata as well as its key, so a HEAD reads no value. Riak's reads send HEAD to every copy and fetch the value from one.",
        "get" => "The ledger says which journal entry holds the value, so the Inker reads exactly that entry.",
        "fold" => "The ledger is sorted by bucket and key, so one bucket's keys are together and the rest are skipped.",
        _ => "",
    }
}

#[derive(Clone, Debug, Default)]
pub struct Leveled {
    seq: u64,
    /// The Bookie's ledger cache.
    cache: Vec<(Key, u64)>,
    /// The Penciller's memory (the L0 cache).
    pmem: BTreeMap<Key, u64>,
    /// levels[0] holds at most one file; each lower level holds files in
    /// key order that do not overlap.
    levels: Vec<Vec<Sst>>,
    /// Journal files, oldest first. The last is the active file.
    journal: Vec<JFile>,
    /// The newest sequence number of each key.
    latest: BTreeMap<Key, u64>,
    next_file: u64,
    events: VecDeque<LEvent>,
    /// Counters for the view.
    merges: u64,
    compactions: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct SstView {
    pub id: u64,
    pub first: String,
    pub last: String,
    pub keys: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct JournalView {
    pub id: u64,
    pub objects: usize,
    pub live: usize,
    pub active: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct LeveledView {
    pub scale: usize,
    pub seq: u64,
    pub cache: Vec<String>,
    pub cache_size: usize,
    pub pmem: usize,
    pub pmem_size: usize,
    pub sst_keys: usize,
    pub journal_objects: usize,
    pub level_files: Vec<usize>,
    pub levels: Vec<Vec<SstView>>,
    pub journal: Vec<JournalView>,
    pub merges: u64,
    pub compactions: u64,
    pub events: Vec<LEvent>,
}

fn show(k: &Key) -> String {
    format!("{}/{}", k.0, k.1)
}

impl Leveled {
    fn event(&mut self, at: u64, who: &'static str, kind: &'static str, text: String, level: Option<usize>) {
        self.events.push_back(LEvent { at, who, kind, text, why: why(kind), level });
        while self.events.len() > EVENTS_KEPT {
            self.events.pop_front();
        }
    }

    fn file_id(&mut self) -> u64 {
        self.next_file += 1;
        self.next_file
    }

    pub fn is_empty(&self) -> bool {
        self.seq == 0
    }

    /// A put or a delete (a tombstone is a put too). `pick` is a random
    /// number, for the merge file choice.
    pub fn put(&mut self, at: u64, key: Key, tombstone: bool, mut pick: impl FnMut(usize) -> usize) {
        self.seq += 1;
        let seq = self.seq;
        self.latest.insert(key.clone(), seq);
        // The Inker first: the object goes in the journal.
        if self.journal.last().is_none_or(|j| j.entries.len() >= JOURNAL_OBJECTS) {
            self.roll(at);
        }
        let jid = self.journal.last().unwrap().id;
        self.journal.last_mut().unwrap().entries.push((key.clone(), seq));
        let what = if tombstone { "a tombstone for" } else { "the object" };
        self.event(at, "inker", "journal", format!("The Inker appends {what} {} to journal file {jid}, as number {seq}.", show(&key)), None);
        // Then the Bookie: the key goes in its ledger cache.
        self.cache.push((key.clone(), seq));
        self.event(at, "bookie", "cache", format!("The Bookie notes {} (number {seq}) in its ledger cache.", show(&key)), None);
        if self.cache.len() >= CACHE_SIZE {
            let n = self.cache.len();
            for (k, s) in self.cache.drain(..) {
                self.pmem.insert(k, s);
            }
            self.event(at, "bookie", "push", format!("The ledger cache is full, so the Bookie gives its {n} keys to the Penciller, which keeps them in memory."), None);
        }
        if self.pmem.len() >= PENCILLER_CACHE && self.levels.first().is_none_or(|l| l.is_empty()) {
            let keys = std::mem::take(&mut self.pmem);
            let n = keys.len();
            let id = self.file_id();
            if self.levels.is_empty() {
                self.levels.push(vec![]);
            }
            self.levels[0].push(Sst { id, keys });
            self.event(at, "penciller", "l0", format!("The Penciller's memory is full, so it writes its {n} keys, in key order, to level 0 file {id}."), Some(0));
            self.work(at, &mut pick);
        }
    }

    /// The Penciller's clerk: merge until no level is over its limit.
    fn work(&mut self, at: u64, pick: &mut impl FnMut(usize) -> usize) {
        loop {
            let level = (0..self.levels.len()).find(|&i| {
                let limit = LEVEL_FILES.get(i).copied().unwrap_or(usize::MAX);
                self.levels[i].len() > limit
            });
            let Some(i) = level else { return };
            let f = pick(self.levels[i].len());
            let file = self.levels[i].remove(f);
            self.merge(at, file, i);
        }
    }

    /// Merge one file into the level below: the file's keys and the keys of
    /// every file there that overlaps it, newest number first, written out
    /// as new files of at most SST_KEYS keys.
    fn merge(&mut self, at: u64, file: Sst, from: usize) {
        let into = from + 1;
        if self.levels.len() <= into {
            self.levels.push(vec![]);
        }
        let (lo, hi) = (file.first().clone(), file.last().clone());
        let (over, keep): (Vec<Sst>, Vec<Sst>) = std::mem::take(&mut self.levels[into]).into_iter().partition(|s| !(s.last() < &lo || s.first() > &hi));
        let mut keys: BTreeMap<Key, u64> = BTreeMap::new();
        for s in &over {
            keys.extend(s.keys.iter().map(|(k, v)| (k.clone(), *v)));
        }
        for (k, v) in file.keys {
            let e = keys.entry(k).or_insert(v);
            *e = (*e).max(v);
        }
        let total = keys.len();
        let mut out: Vec<Sst> = vec![];
        let mut chunk = BTreeMap::new();
        for (k, v) in keys {
            chunk.insert(k, v);
            if chunk.len() == SST_KEYS {
                let id = self.file_id();
                out.push(Sst { id, keys: std::mem::take(&mut chunk) });
            }
        }
        if !chunk.is_empty() {
            let id = self.file_id();
            out.push(Sst { id, keys: chunk });
        }
        let made = out.len();
        let mut level = keep;
        level.extend(out);
        level.sort_by(|a, b| a.first().cmp(b.first()));
        self.levels[into] = level;
        self.merges += 1;
        let with = match over.len() {
            0 => "No file there overlaps its keys".to_string(),
            1 => "1 file there overlaps its keys".to_string(),
            n => format!("{n} files there overlap its keys"),
        };
        self.event(
            at,
            "pclerk",
            "merge",
            format!("The Penciller's clerk merges file {} from level {from} into level {into}. {with}. It writes the {total} keys in key order to {made} file{}, keeping the newest number of each key.", file.id, if made == 1 { "" } else { "s" }),
            Some(into),
        );
    }

    /// The active journal file is full: start a new one, then let the
    /// Inker's clerk look for files to compact.
    fn roll(&mut self, at: u64) {
        let id = self.file_id();
        let had = !self.journal.is_empty();
        self.journal.push(JFile { id, entries: vec![] });
        if had {
            self.event(at, "inker", "roll", format!("The active journal file is full, so the Inker starts journal file {id}."), None);
            self.compact(at);
        }
    }

    /// Share of a journal file's objects that are still current, in percent.
    fn score(&self, f: &JFile) -> f64 {
        if f.entries.is_empty() {
            return 100.0;
        }
        let live = f.entries.iter().filter(|(k, s)| self.latest.get(k) == Some(s)).count();
        100.0 * live as f64 / f.entries.len() as f64
    }

    /// `leveled_iclerk`: score every run of up to MAX_RUN_LENGTH older files
    /// against a target that rises from SINGLE_FILE_TARGET for one file to
    /// MAX_RUN_TARGET for the longest run; compact the best run if it beats
    /// its target.
    fn compact(&mut self, at: u64) {
        let n = self.journal.len().saturating_sub(1);
        if n == 0 {
            return;
        }
        let scores: Vec<f64> = self.journal[..n].iter().map(|f| self.score(f)).collect();
        let max = MAX_RUN_LENGTH.min(n);
        let incr = if max <= 1 { 0.0 } else { (MAX_RUN_TARGET - SINGLE_FILE_TARGET) / (MAX_RUN_LENGTH - 1) as f64 };
        let mut best: Option<(usize, usize, f64)> = None;
        for len in (1..=max).rev() {
            for start in 0..=(n - len) {
                let avg = scores[start..start + len].iter().sum::<f64>() / len as f64;
                let s = SINGLE_FILE_TARGET + incr * (len - 1) as f64 - avg;
                if best.is_none_or(|(_, _, b)| s > b) {
                    best = Some((start, len, s));
                }
            }
        }
        let Some((start, len, s)) = best else { return };
        if s <= 0.0 {
            self.event(at, "iclerk", "compact", "The Inker's clerk scores the older journal files. Too few of their objects are out of date, so it compacts nothing yet.".into(), None);
            return;
        }
        let run: Vec<JFile> = self.journal.drain(start..start + len).collect();
        let before: usize = run.iter().map(|f| f.entries.len()).sum();
        let live: Vec<(Key, u64)> = run.iter().flat_map(|f| f.entries.iter().filter(|(k, s)| self.latest.get(k) == Some(s)).cloned()).collect();
        let kept = live.len();
        let id = self.file_id();
        self.journal.insert(start, JFile { id, entries: live });
        self.compactions += 1;
        let names = run.iter().map(|f| f.id.to_string()).collect::<Vec<_>>().join(", ");
        self.event(
            at,
            "iclerk",
            "compact",
            format!("The Inker's clerk compacts journal file{} {names}: it keeps the {kept} of {before} objects that are still current and writes them to file {id}. Newer writes replaced the others.", if len == 1 { "" } else { "s" }),
            None,
        );
    }

    /// Where a key is found: the ledger cache, the Penciller's memory, or a
    /// level. Returns the steps taken, and records them.
    fn find(&self, k: &Key) -> (Vec<String>, Option<(String, Option<usize>, u64)>) {
        let mut steps = vec![];
        if let Some((_, s)) = self.cache.iter().rev().find(|(x, _)| x == k) {
            return (steps, Some(("the Bookie's ledger cache".into(), None, *s)));
        }
        steps.push("the ledger cache".to_string());
        if let Some(s) = self.pmem.get(k) {
            return (steps, Some(("the Penciller's memory".into(), None, *s)));
        }
        steps.push("the Penciller's memory".to_string());
        for (i, level) in self.levels.iter().enumerate() {
            for f in level.iter().filter(|f| f.covers(k)) {
                if let Some(s) = f.keys.get(k) {
                    return (steps, Some((format!("file {} on level {i}", f.id), Some(i), *s)));
                }
                steps.push(format!("file {} on level {i}", f.id));
            }
        }
        (steps, None)
    }

    /// A HEAD: the ledger only. A GET also reads the value from the journal.
    pub fn read(&mut self, at: u64, key: &Key, get: bool) {
        let (steps, found) = self.find(key);
        let checked = if steps.is_empty() { String::new() } else { format!(" It checks {}.", steps.join(", then ")) };
        let (kind, who) = if get { ("get", "bookie") } else { ("head", "bookie") };
        match found {
            Some((place, level, seq)) => {
                let text = if get {
                    let jid = self.journal.iter().find(|f| f.entries.iter().any(|(k, s)| k == key && *s == seq)).map(|f| f.id);
                    let j = jid.map(|j| format!(" Then the Inker reads the value of number {seq} from journal file {j}.")).unwrap_or_default();
                    format!("The Bookie looks up {} in the ledger.{checked} It finds number {seq} in {place}.{j}", show(key))
                } else {
                    format!("The Bookie looks up {} in the ledger for a HEAD.{checked} It finds number {seq} in {place}. A HEAD needs only the ledger: the value stays in the journal.", show(key))
                };
                self.event(at, who, kind, text, level);
            }
            None => {
                self.event(at, who, kind, format!("The Bookie looks up {} in the ledger.{checked} It is not there: not found.", show(key)), None);
            }
        }
    }

    /// Keys in one bucket, for list keys: the ledger is in bucket order, so
    /// a fold reads only that bucket's range (`book_keylist`).
    pub fn bucket_keys(&self, bucket: &str) -> usize {
        self.latest.keys().filter(|(b, _)| b == bucket).count()
    }

    pub fn fold(&mut self, at: u64, bucket: &str, keys: usize, all: usize) {
        self.event(
            at,
            "penciller",
            "fold",
            format!("A key fold for bucket {bucket}. The ledger is in bucket and key order, so the fold reads only this bucket's range: {keys} of the {all} keys this vnode holds."),
            None,
        );
    }

    pub fn view(&self) -> LeveledView {
        let mut levels: Vec<Vec<SstView>> = self
            .levels
            .iter()
            .map(|l| l.iter().map(|f| SstView { id: f.id, first: show(f.first()), last: show(f.last()), keys: f.keys.len() }).collect())
            .collect();
        while levels.len() < 3 {
            levels.push(vec![]);
        }
        let active = self.journal.last().map(|j| j.id);
        LeveledView {
            scale: SCALE,
            seq: self.seq,
            cache: self.cache.iter().map(|(k, _)| show(k)).collect(),
            cache_size: CACHE_SIZE,
            pmem: self.pmem.len(),
            pmem_size: PENCILLER_CACHE,
            sst_keys: SST_KEYS,
            journal_objects: JOURNAL_OBJECTS,
            level_files: LEVEL_FILES.iter().take(levels.len()).map(|&n| n.min(1_000_000)).collect(),
            levels,
            journal: self
                .journal
                .iter()
                .map(|f| JournalView {
                    id: f.id,
                    objects: f.entries.len(),
                    live: f.entries.iter().filter(|(k, s)| self.latest.get(k) == Some(s)).count(),
                    active: Some(f.id) == active,
                })
                .collect(),
            merges: self.merges,
            compactions: self.compactions,
            events: self.events.iter().cloned().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(i: usize) -> Key {
        ("data".into(), format!("k{i:04}"))
    }

    #[test]
    fn writes_fill_the_levels_in_key_order() {
        let mut l = Leveled::default();
        let mut r = 7usize;
        for i in 0..400 {
            l.put(i as u64, k(i), false, |n| {
                r = r.wrapping_mul(31).wrapping_add(11);
                r % n
            });
        }
        let v = l.view();
        // Level 1 is within its limit after the work, and lower levels exist.
        assert!(v.levels[1].len() <= LEVEL_FILES[1]);
        assert!(v.levels.len() >= 3 && !v.levels[2].is_empty());
        // Files in a level below 0 do not overlap and are in key order.
        for level in &v.levels[1..] {
            for w in level.windows(2) {
                assert!(w[0].last < w[1].first, "{} / {}", w[0].last, w[1].first);
            }
        }
        // Every key is somewhere, once, in the newest version.
        let total: usize = v.levels.iter().flatten().map(|f| f.keys).sum::<usize>() + l.pmem.len() + l.cache.len();
        assert_eq!(total, 400);
        assert!(v.merges > 0);
    }

    #[test]
    fn rewrites_make_the_inker_compact() {
        let mut l = Leveled::default();
        // The same 20 keys again and again: old journal objects go out of date.
        for i in 0..1000 {
            l.put(i as u64, k(i % 20), false, |_| 0);
        }
        let v = l.view();
        assert!(v.compactions > 0);
        // Compaction keeps only current objects in the files it writes.
        let objects: usize = v.journal.iter().map(|j| j.objects).sum();
        assert!(objects < 1000);
    }

    #[test]
    fn a_read_finds_the_newest_number() {
        let mut l = Leveled::default();
        for i in 0..100 {
            l.put(i as u64, k(i % 30), false, |_| 0);
        }
        l.read(200, &k(5), true);
        let e = l.view().events.last().cloned().unwrap();
        // k5 was written as numbers 6, 36, 66 and 96.
        assert!(e.text.contains("number 96"), "{}", e.text);
    }
}
