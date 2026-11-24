//! Word edit distance where joining adjacent words to match one word costs
//! nothing ("to night" against "tonight"), as the Open ASR Leaderboard scores.
//!
//! A port of kaldialign v0.12.0 `LevenshteinEditDistanceCompound` (unit costs),
//! with the same transition order and tie-breaking, so counts match exactly
//! (`tests/oasr_parity.rs`). kaldialign keeps the whole table; a compound can
//! only reach back as many rows as the longest hypothesis word has bytes, so
//! this keeps that many rows and scores hour-long transcripts in little memory.

/// Error counts for one reference/hypothesis pair.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Edits {
    pub ins: u32,
    pub del: u32,
    pub sub: u32,
}

impl Edits {
    pub fn total(&self) -> u32 {
        self.ins + self.del + self.sub
    }
}

#[derive(Clone, Copy, Default)]
struct Cell {
    ins: u32,
    del: u32,
    sub: u32,
    total: u32,
    cost: u32,
}

/// `kaldialign.edit_distance(ref, hyp, merge_compounds=True)`.
pub fn edit_distance<S: AsRef<str>>(reference: &[S], hyp: &[S]) -> Edits {
    let r: Vec<&str> = reference.iter().map(|s| s.as_ref()).collect();
    let h: Vec<&str> = hyp.iter().map(|s| s.as_ref()).collect();
    let (m, n) = (r.len(), h.len());
    // Rows i-k with k <= len(hyp word) are reachable; keep that many plus one.
    let depth = h.iter().map(|w| w.len()).max().unwrap_or(0) + 2;
    let ring = depth.min(m + 1).max(1);
    let mut rows: Vec<Vec<Cell>> = vec![vec![Cell::default(); n + 1]; ring];
    for (j, c) in rows[0].iter_mut().enumerate() {
        let j = j as u32;
        *c = Cell { ins: j, total: j, cost: j, ..Default::default() };
    }
    for i in 1..=m {
        let (cur_idx, prev_idx) = (i % ring, (i - 1) % ring);
        let mut cur = std::mem::take(&mut rows[cur_idx]);
        if cur.len() != n + 1 {
            cur = vec![Cell::default(); n + 1];
        }
        let iu = i as u32;
        cur[0] = Cell { del: iu, total: iu, cost: iu, ..Default::default() };
        for j in 1..=n {
            let prev = &rows[prev_idx];
            let same = r[i - 1] == h[j - 1];
            let sub_err = prev[j - 1].cost + u32::from(!same);
            let del_err = prev[j].cost + 1;
            let ins_err = cur[j - 1].cost + 1;
            let mut c = if sub_err < ins_err && sub_err < del_err {
                let mut c = prev[j - 1];
                if !same {
                    c.sub += 1;
                    c.total += 1;
                }
                c.cost = sub_err;
                c
            } else if del_err < ins_err {
                let mut c = prev[j];
                c.del += 1;
                c.total += 1;
                c.cost = del_err;
                c
            } else {
                let mut c = cur[j - 1];
                c.ins += 1;
                c.total += 1;
                c.cost = ins_err;
                c
            };
            // k reference words -> one hypothesis word
            let mut merged = r[i - 1].to_string();
            for k in 2..=i {
                merged.insert_str(0, r[i - k]);
                if merged.len() > h[j - 1].len() {
                    break;
                }
                if merged == h[j - 1] {
                    let from = rows[(i - k) % ring][j - 1];
                    if from.cost < c.cost {
                        c = from;
                    }
                }
            }
            // k hypothesis words -> one reference word
            let mut merged = h[j - 1].to_string();
            for k in 2..=j {
                merged.insert_str(0, h[j - k]);
                if merged.len() > r[i - 1].len() {
                    break;
                }
                if merged == r[i - 1] {
                    let from = rows[prev_idx][j - k];
                    if from.cost < c.cost {
                        c = from;
                    }
                }
            }
            cur[j] = c;
        }
        rows[cur_idx] = cur;
    }
    let last = rows[m % ring][n];
    Edits { ins: last.ins, del: last.del, sub: last.sub }
}
