// How big a bot server's snapshots get (JSON relay and compressed datagrams).
use peakrunner_core::{sim::{Match, MAX_PLAYERS}, terrain::MapId};
fn main() {
    let mut m = Match::new(MapId::OldHoller);
    m.bots = 16;
    let none = vec![None; MAX_PLAYERS];
    let mut worst = (0, 0usize, 0usize);
    for t in 0..(60 * 120) {
        m.step(&none);
        if t % 60 == 0 {
            if t == 60 * 5 { m.join(1, "Probe").unwrap(); }
            let s = m.snapshot_for(0);
            let json = serde_json::to_vec(&s).unwrap().len();
            let raw = postcard::to_allocvec(&s).unwrap();
            let lz = lz4_flex::compress_prepend_size(&raw).len();
            if json > worst.1 { worst = (t / 60, json, lz); }
            if t % 600 == 0 { println!("t={}s json {} B, compressed {} B, discs {} smoke {} expl {} feed {}", t/60, json, lz, s.discs.len(), s.smoke.len(), s.explosions.len(), s.feed.len()); }
        }
    }
    println!("worst at {}s: json {} B, compressed {} B", worst.0, worst.1, worst.2);
}
