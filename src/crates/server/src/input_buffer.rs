//! Per-player input jitter buffer: one client command per 60 Hz server tick.
//!
//! The client predicts exactly one simulation step per command it sends, so
//! the server simulates each command once, in order. Arrival jitter is
//! absorbed by a small queue: a tick with nothing queued repeats the last
//! held input (one-shot presses removed), and the late command is simply
//! used on a following tick. A standing backlog (the client's clock running a
//! little fast, or a hitch that sent a burst) is trimmed only when the queue
//! never drained during a whole window, one command at a time; a delayed
//! burst beyond MAX_DEPTH is cut at once. A trimmed command's one-shot
//! actions (throw, buy, deploy, suicide) and press edges (jump, fire,
//! interact) carry into the next simulated command, so no press is lost.
use std::collections::VecDeque;
use peakrunner_core::sim::{merge_one_shots, without_one_shots, Command};

/// Most commands held (133 ms at 60 Hz); a burst beyond it is cut at once.
pub const MAX_DEPTH: usize = 8;
/// Ticks over which the smallest queue depth is watched before trimming.
pub const WINDOW: u32 = 60;

#[derive(Clone, Debug, Default)]
pub struct InputBuffer {
    queue: VecDeque<Command>,
    /// The last simulated command without its one-shots, repeated when empty.
    held: Command,
    /// Presses from trimmed commands, added to the next simulated one.
    carry: Option<Command>,
    /// Smallest depth seen at the start of a tick in this window.
    low: usize,
    ticks: u32,
    /// Commands simulated and trimmed, for tests and measurement.
    pub simulated: u64,
    pub trimmed: u64,
}

/// `into` with the presses of a command that will never be simulated.
fn keep_presses(mut into: Command, dropped: &Command) -> Command {
    into = merge_one_shots(into, dropped);
    // A carried trigger press keeps the view it was aimed at (sim::lagcomp).
    if dropped.fire && !into.fire { (into.view, into.ahead) = (dropped.view, dropped.ahead); }
    into.jump |= dropped.jump;
    into.fire |= dropped.fire;
    into.interact |= dropped.interact;
    into
}

impl InputBuffer {
    pub fn new() -> Self { Self { low: usize::MAX, ..Self::default() } }

    /// Queue a command (already validated and newer than every queued one).
    pub fn push(&mut self, command: Command) {
        while self.queue.len() >= MAX_DEPTH { self.trim(); }
        self.queue.push_back(command);
    }

    /// Commands waiting.
    pub fn queued(&self) -> usize { self.queue.len() }

    fn trim(&mut self) {
        let Some(dropped) = self.queue.pop_front() else { return };
        self.trimmed += 1;
        self.carry = Some(match self.carry { Some(carry) => keep_presses(dropped, &carry), None => dropped });
    }

    /// The command to simulate this tick: the next queued one, or the held
    /// input repeated (None only before any command has arrived).
    pub fn next_command(&mut self) -> Option<Command> {
        self.low = self.low.min(self.queue.len());
        self.ticks += 1;
        if self.ticks >= WINDOW {
            // Two or more queued at the start of every tick of the window:
            // one of them is standing latency, not jitter protection.
            if self.low >= 2 { self.trim(); }
            self.low = usize::MAX;
            self.ticks = 0;
        }
        match self.queue.pop_front() {
            Some(mut command) => {
                if let Some(carry) = self.carry.take() { command = keep_presses(command, &carry); }
                self.held = without_one_shots(command);
                self.simulated += 1;
                Some(command)
            }
            None => (self.held.seq > 0).then_some(self.held),
        }
    }

    /// Forget everything (a map change resets every player).
    pub fn clear(&mut self) { *self = Self::new(); }
}

#[cfg(test)]
mod tests {
    use super::*;
    use peakrunner_core::{sim::{Match, Phase, World, MAX_PLAYERS}, terrain::MapId};

    fn cmd(seq: u64) -> Command { Command { seq, move_z: 1.0, ..Command::default() } }

    #[test]
    fn one_command_per_tick_in_order_and_held_input_when_late() {
        let mut b = InputBuffer::new();
        assert!(b.next_command().is_none(), "nothing before the first command");
        b.push(cmd(1)); b.push(cmd(2));
        assert_eq!(b.next_command().unwrap().seq, 1);
        assert_eq!(b.next_command().unwrap().seq, 2);
        // Late: the held input repeats, with no one-shot actions.
        let late = b.next_command().unwrap();
        assert_eq!((late.seq, late.move_z), (2, 1.0));
        b.push(Command { seq: 3, throw: 1, throw_strength: 0.5, buy: 2, ..cmd(3) });
        let press = b.next_command().unwrap();
        assert_eq!((press.seq, press.throw, press.buy), (3, 1, 2));
        let held = b.next_command().unwrap();
        assert_eq!((held.seq, held.throw, held.buy), (3, 0, 0), "a press acts once");
        assert_eq!((b.simulated, b.trimmed), (3, 0));
    }

    #[test]
    fn a_burst_is_cut_and_its_presses_carry_into_the_next_command() {
        let mut b = InputBuffer::new();
        b.push(Command { seq: 1, throw: 2, throw_strength: 0.9, ..cmd(1) });
        b.push(Command { seq: 2, jump: true, ..cmd(2) });
        b.push(Command { seq: 3, fire: true, view: 4321, ..cmd(3) });
        for seq in 4..=11 { b.push(cmd(seq)); }
        assert_eq!(b.queued(), MAX_DEPTH);
        let next = b.next_command().unwrap();
        assert_eq!(next.seq, 4);
        assert_eq!((next.throw, next.jump, next.fire), (2, true, true), "trimmed presses are not lost");
        assert_eq!(next.view, 4321, "a carried trigger press keeps its view");
        assert!((next.throw_strength - 0.9).abs() < 1e-6);
        let after = b.next_command().unwrap();
        assert_eq!((after.seq, after.throw, after.jump, after.fire), (5, 0, false, false));
        assert_eq!(b.trimmed, 3);
    }

    #[test]
    fn a_standing_backlog_drains_but_jitter_is_kept() {
        // Four commands of standing backlog, then one arriving per tick: the
        // backlog is trimmed until each tick finds just its own command.
        let mut b = InputBuffer::new();
        let mut seq = 0;
        for _ in 0..4 { seq += 1; b.push(cmd(seq)); }
        for _ in 0..(WINDOW * 6) { seq += 1; b.push(cmd(seq)); assert_eq!(b.next_command().unwrap().seq, seq - b.queued() as u64); }
        assert_eq!((b.queued(), b.trimmed), (0, 4), "backlog trimmed away");
        // Alternating 0 and 2 arrivals per tick: the queue drains to empty
        // every other tick, so nothing is trimmed and nothing repeats.
        let simulated = b.simulated;
        for t in 0..(WINDOW * 6) {
            if t % 2 == 0 { seq += 1; b.push(cmd(seq)); seq += 1; b.push(cmd(seq)); }
            b.next_command();
        }
        assert_eq!(b.trimmed, 4);
        assert_eq!(b.simulated - simulated, WINDOW as u64 * 6);
    }

    /// Model of the input path under jitter, against the real shared sim and
    /// the client's predict/replay loop (online.rs): commands are created at
    /// 60 Hz plus frame jitter and arrive after 30 ms plus network jitter
    /// (in datagrams carrying the last three commands, as clients send them);
    /// the server simulates one buffered command per tick and sends a
    /// snapshot every third tick. `legacy` samples only the newest queued
    /// command per tick instead (the server before this buffer), for
    /// comparison. Returns (share of commands simulated, share of snapshots
    /// correcting the local player by more than 5 cm, mean input->ack
    /// latency in ms).
    pub(crate) fn jitter_model(frame_jitter: f64, net_jitter: f64, drift: f64, legacy: bool) -> (f64, f64, f64) {
        let step_ms = 1000.0 / 60.0; let lat = 30.0;
        let mut rng = 0x1234_5678u64;
        let mut rnd = || { rng ^= rng << 13; rng ^= rng >> 7; rng ^= rng << 17; (rng % 10_000) as f64 / 10_000.0 };
        let mut m = Match::new(MapId::OldHoller);
        let me = m.join(1, "Me").unwrap(); m.join(2, "Other").unwrap();
        m.phase = Phase::Playing;
        m.step(&[]);
        let mut client = World::new(); client.apply_snapshot(&m.snapshot(), 1);
        let n = 1800u64;
        let period = step_ms * (1.0 - drift);
        let cmds: Vec<(f64, f64, Command)> = (1..=n).map(|k| {
            let created = k as f64 * period + rnd() * frame_jitter;
            let c = Command { seq: k, move_z: 1.0, move_x: if (k / 90) % 2 == 0 { 0.6 } else { -0.6 },
                yaw: (k as f32 * 0.015).rem_euclid(std::f32::consts::TAU), jet: (k / 40) % 3 == 0, jump: k % 50 < 3, ..Command::default() };
            (created, created + lat + rnd() * net_jitter, c)
        }).collect();
        let mut arrivals: Vec<(f64, Command)> = cmds.iter().map(|(_, a, c)| (*a, *c)).collect();
        arrivals.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut pending: VecDeque<Command> = VecDeque::new();
        let mut snaps: VecDeque<(f64, peakrunner_core::sim::Snapshot)> = VecDeque::new();
        let mut sent_at = std::collections::HashMap::new();
        let mut buffer = InputBuffer::new();
        let (mut queue, mut current, mut legacy_simulated) = (VecDeque::new(), Command::default(), 0u64);
        let (mut ci, mut ai, mut last_seq, mut last_in) = (0usize, 0usize, 0u64, 0.0f64);
        let (mut big, mut count, mut lat_sum, mut lat_n) = (0u32, 0u32, 0.0f64, 0u32);
        let mut tick_t = step_ms + lat + 8.0;
        let end = n as f64 * period;
        loop {
            let tc = cmds.get(ci).map_or(f64::MAX, |c| c.0);
            let ts = snaps.front().map_or(f64::MAX, |s| s.0);
            if tc >= end && tick_t >= end + 300.0 { break; }
            if tc <= tick_t && tc <= ts {
                let c = cmds[ci].2; client.predict_command(c); pending.push_back(c); sent_at.insert(c.seq, tc); ci += 1;
            } else if ts <= tick_t {
                let (at, s) = snaps.pop_front().unwrap();
                let before = client.players[client.player_id].pos;
                client.apply_snapshot(&s, 1);
                let ack = s.acks[me];
                if let Some(t) = sent_at.get(&ack) { lat_sum += at - t; lat_n += 1; }
                while pending.front().is_some_and(|c| c.seq <= ack) { pending.pop_front(); }
                for c in &pending { client.predict_command(*c); }
                let e = before.distance(client.players[client.player_id].pos);
                if at > 1000.0 && at < end { count += 1; if e > 0.05 { big += 1; } }
            } else {
                while ai < arrivals.len() && arrivals[ai].0 <= tick_t {
                    // Each input datagram carries its command and the two
                    // before it (packets::inputs), so reordering loses none.
                    let newest = arrivals[ai].1.seq; ai += 1;
                    for seq in newest.saturating_sub(2).max(1)..=newest {
                        if seq <= last_seq { continue; }
                        let c = cmds[seq as usize - 1].2;
                        last_seq = c.seq; last_in = tick_t;
                        if legacy { queue.push_back(c); } else { buffer.push(c); }
                    }
                }
                let mut commands = vec![None; MAX_PLAYERS];
                let next = if legacy {
                    let fresh = !queue.is_empty();
                    let mut merged = queue.back().copied().unwrap_or(current);
                    for c in queue.drain(..) { merged = merge_one_shots(merged, &c); }
                    current = without_one_shots(merged);
                    if fresh { legacy_simulated += 1; }
                    Some(if fresh { merged } else { current })
                } else { buffer.next_command() };
                if tick_t - last_in <= 250.0 { commands[me] = next; }
                m.step(&commands);
                if m.tick.is_multiple_of(3) { snaps.push_back((tick_t + lat, m.snapshot())); }
                tick_t += step_ms;
            }
        }
        let simulated = if legacy { legacy_simulated } else { buffer.simulated };
        (simulated as f64 / n as f64, big as f64 / count.max(1) as f64, lat_sum / lat_n.max(1) as f64)
    }

    #[test]
    fn under_jitter_nearly_every_command_is_simulated_and_corrections_are_rare() {
        for (fj, nj) in [(0.0, 0.0), (7.0, 0.0), (0.0, 8.0), (7.0, 15.0), (10.0, 30.0)] {
            for legacy in [true, false] {
                let (simulated, corrected, latency) = jitter_model(fj, nj, 0.0, legacy);
                eprintln!("{} frame jitter {fj:>4.1} ms, net jitter {nj:>4.1} ms: {:5.1}% of commands simulated, {:4.1}% of snapshots correct > 5 cm, input->ack {latency:.1} ms",
                    if legacy { "newest-sample (before)" } else { "jitter buffer (after) " }, simulated * 100.0, corrected * 100.0);
                if !legacy {
                    assert!(simulated > 0.99, "{fj}/{nj}: {simulated}");
                    assert!(corrected < 0.03, "{fj}/{nj}: {corrected}");
                }
            }
        }
        // A client clock 0.5% fast: the backlog is trimmed, latency stays bounded.
        let (simulated, corrected, latency) = jitter_model(7.0, 8.0, 0.005, false);
        eprintln!("0.5% fast client: {:.1}% simulated, {:.1}% corrected, input->ack {latency:.1} ms", simulated * 100.0, corrected * 100.0);
        assert!(simulated > 0.98 && latency < 120.0, "{simulated} {latency}");
    }
}
