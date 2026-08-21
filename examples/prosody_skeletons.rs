//! Dump the grammar's POS skeleton inventory, so an offline rig can ask whether
//! meter and grammar fit in one cover budget.
//!
//! `prosody_candidates` dumps finished prose, which only shows the cover words
//! the generator happened to pick. The joint question needs the shape *behind*
//! that choice: which POS slot each position is, what refinement it demands, and
//! how probable the skeleton is — everything `plan_sentence` sees when it draws.
//!
//! Emits TSV: start_symbol, k, probability, then the slots as `Pos` or
//! `Pos[refinement]`, space separated.
//!
//! Run: cargo run --release --example prosody_skeletons [language] [dialect] [k_max]

use glossia::generator::{GenerationMode, SequenceCache};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let language = args.get(1).map(String::as_str).unwrap_or("english");
    let dialect = args.get(2).map(String::as_str).unwrap_or("body");
    let k_max: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(12);

    let cache = SequenceCache::load_with_dialect(
        GenerationMode::Body, language, dialect, k_max, false,
    ).expect("sequence cache");

    println!("start_symbol\tk\tprobability\tslots");
    for start in ["S", "S_N", "S_V", "S_Adj", "S_Adv", "S_Prep", "S_Det"] {
        for k in 1..=k_max {
            let Some(seqs) = cache.get(start, k) else { continue };
            for s in seqs {
                let slots: Vec<String> = s.sequence.iter().zip(s.refinements.iter())
                    .map(|(p, r)| match r {
                        Some(r) => format!("{p:?}[{r}]"),
                        None => format!("{p:?}"),
                    })
                    .collect();
                println!("{start}\t{k}\t{:.10}\t{}", s.probability, slots.join(" "));
            }
        }
    }
}
