//! The economy core: pure, deterministic, and free of Bevy beyond the derives
//! that let these types live in the world.
//!
//! Everything here is traceable to `docs/economy/banana-architecture-v2.md` and
//! the parameter table in `docs/economy/banana-whitepaper.md` §8. The reference
//! implementation is `docs/economy/banana_model.py`; where this module and that
//! model disagree, the model is the oracle.

use bevy::prelude::{Component, Resource};

// ─────────────────────────────────────────────────────────────── parameters

/// Bananas a worker carries per round trip.
pub const WORKER_PAYLOAD: f64 = 5.0;
/// Metres per second on foot.
pub const WORKER_SPEED: f64 = 5.0;
/// Metres to the grove, one way.
pub const GROVE_DISTANCE: f64 = 100.0;
/// Seconds per banana, at the grove.
pub const T_PICK: f64 = 1.00;
/// Seconds per banana, at the depot.
pub const T_UNLOAD: f64 = 0.50;
/// Bananas per second, per worker (D2: this is a per-unit figure, and the
/// [`Wage`] component is what the simulation actually sums).
pub const WORKER_WAGE: f64 = 0.03;
pub const WORKER_COST_BASE: f64 = 4.0;
pub const WORKER_COST_GROWTH: f64 = 1.15;

/// Bananas the player earns per manual harvest.
pub const BANANAS_PER_HARVEST: f64 = 1.0;

/// Simulation rate. 20 Hz matches the oracle's `discrete_run(dt=0.05)`, so its
/// convergence contract ports directly, and it keeps the economy independent of
/// browser frame pacing.
pub const SIM_HZ: f64 = 20.0;

pub const MAX_SAFE_BANANAS: f64 = 9_007_199_254_740_991.0;

// ───────────────────────────────────────────────────────────── multipliers

/// D4: every multiplier is additive within its term, `M = 1 + count × bonus`.
///
/// Chefs, Unpackers and Technologists do not exist yet, so all three are 1.0.
/// They are carried explicitly rather than elided so that adding those units is
/// a change of value, not a change of shape.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct Multipliers {
    /// Chefs: shortens travel.
    pub speed: f64,
    /// Technologists: shortens picking.
    pub tech: f64,
    /// Unpackers: shortens unloading.
    pub unpack: f64,
}

impl Default for Multipliers {
    fn default() -> Self {
        Self {
            speed: 1.0,
            tech: 1.0,
            unpack: 1.0,
        }
    }
}

// ─────────────────────────────────────────────────────────── harvest cycle

/// One addend of the harvest cycle. D12: three addends, three support roles,
/// one each. Travel is split into two legs so the round trip can be animated;
/// their durations sum to the doc's single `2d/(v·M_speed)` term.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Segment {
    ToGrove,
    Pick,
    ToDepot,
    Unload,
}

impl Segment {
    pub const ORDER: [Segment; 4] = [
        Segment::ToGrove,
        Segment::Pick,
        Segment::ToDepot,
        Segment::Unload,
    ];

    /// Work in this segment's own units: metres to walk, or nominal bananas to
    /// pick or unload. D13 stores progress as remaining work rather than
    /// elapsed time, so a multiplier bought mid-trip speeds up the remainder of
    /// every journey already in flight without teleporting anyone.
    pub fn nominal_work(self) -> f64 {
        match self {
            Segment::ToGrove | Segment::ToDepot => GROVE_DISTANCE,
            Segment::Pick | Segment::Unload => WORKER_PAYLOAD,
        }
    }

    /// Work consumed per second at the current multipliers.
    pub fn rate(self, multipliers: Multipliers) -> f64 {
        match self {
            Segment::ToGrove | Segment::ToDepot => WORKER_SPEED * multipliers.speed,
            Segment::Pick => multipliers.tech / T_PICK,
            Segment::Unload => multipliers.unpack / T_UNLOAD,
        }
    }

    pub fn duration(self, multipliers: Multipliers) -> f64 {
        self.nominal_work() / self.rate(multipliers)
    }

    pub fn next(self) -> Self {
        match self {
            Segment::ToGrove => Segment::Pick,
            Segment::Pick => Segment::ToDepot,
            Segment::ToDepot => Segment::Unload,
            Segment::Unload => Segment::ToGrove,
        }
    }

    /// True once the worker is loaded: it picked at the grove and has not yet
    /// finished handing the load over at the stall.
    pub fn is_carrying(self) -> bool {
        matches!(self, Segment::ToDepot | Segment::Unload)
    }
}

/// Seconds for one full round trip. Pure in counts and multipliers - no phase
/// appears in it, which is what keeps the projected rate deterministic (I3').
pub fn cycle_time(multipliers: Multipliers) -> f64 {
    Segment::ORDER
        .iter()
        .map(|segment| segment.duration(multipliers))
        .sum()
}

/// Steady-state bananas per second for a single worker.
pub fn worker_throughput(multipliers: Multipliers) -> f64 {
    WORKER_PAYLOAD / cycle_time(multipliers)
}

/// The sole piece of irreducible per-entity state in the simulation (D13).
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct HarvestCycle {
    segment: Segment,
    remaining: f64,
}

impl Default for HarvestCycle {
    fn default() -> Self {
        Self {
            segment: Segment::ToGrove,
            remaining: Segment::ToGrove.nominal_work(),
        }
    }
}

impl HarvestCycle {
    /// A cycle that has already run for `phase` seconds. `phase` is wrapped
    /// into the cycle, so callers may pass any non-negative offset.
    pub fn at_phase(phase: f64, multipliers: Multipliers) -> Self {
        let total = cycle_time(multipliers);
        debug_assert!(phase.is_finite() && phase >= 0.0);
        let mut left = if total > 0.0 { phase % total } else { 0.0 };

        for segment in Segment::ORDER {
            let duration = segment.duration(multipliers);
            if left < duration {
                return Self {
                    segment,
                    remaining: segment.nominal_work() - left * segment.rate(multipliers),
                };
            }
            left -= duration;
        }

        Self::default()
    }

    pub fn segment(self) -> Segment {
        self.segment
    }

    /// How far through the current segment the worker is, in 0.0..=1.0.
    /// Rendering derives position from this, so the avatar is a pure function
    /// of simulation state.
    pub fn segment_fraction(self) -> f64 {
        let nominal = self.segment.nominal_work();
        if nominal <= 0.0 {
            return 1.0;
        }
        (1.0 - self.remaining / nominal).clamp(0.0, 1.0)
    }

    /// Consume `dt` seconds of work, returning the number of completed round
    /// trips (normally 0 or 1; more only if `dt` spans a whole cycle).
    ///
    /// This is a *time budget* rather than a work subtraction, and the leftover
    /// budget is deliberately carried across segment boundaries. Both matter:
    ///
    /// - `remaining -= rate * dt` followed by `remaining <= 0.0` leaves a
    ///   positive binary residual on Pick (9.4e-15) and Unload (1.0e-15) at
    ///   20 Hz, costing a whole extra tick each and making the cycle 47.6 s.
    /// - Discarding the leftover loses `dt/2` per boundary, which is a 0.21%
    ///   throughput error at these parameters and *grows* as multipliers
    ///   shorten the cycle.
    pub fn advance(&mut self, dt: f64, multipliers: Multipliers) -> u32 {
        debug_assert!(dt.is_finite() && dt >= 0.0);
        let mut budget = dt;
        let mut deliveries = 0;
        // A zero or non-finite rate would make `needed` infinite or NaN and the
        // loop non-terminating, so the guard is load-bearing, not defensive.
        let mut guard = 0;

        while budget > 0.0 && guard < 64 {
            guard += 1;
            let rate = self.segment.rate(multipliers);
            if !(rate > 0.0 && rate.is_finite()) {
                debug_assert!(false, "segment rate must be positive and finite");
                break;
            }

            let needed = self.remaining / rate;
            // Repeated subtraction leaves a residual: 0.05 is not exactly
            // representable in binary, so after 100 ticks a 5-banana pick
            // segment has ~9.4e-15 of work left. Without a tolerance that
            // residual costs a whole extra tick at every boundary, turning a
            // 47.5-second cycle into a 47.6-second one. The tolerance is a
            // billionth of the segment's duration - about 20 nanoseconds.
            let tolerance = self.segment.duration(multipliers) * 1e-9;
            if needed <= budget + tolerance {
                budget = (budget - needed).max(0.0);
                if self.segment == Segment::Unload {
                    deliveries += 1;
                }
                self.segment = self.segment.next();
                self.remaining = self.segment.nominal_work();
            } else {
                self.remaining -= rate * budget;
                budget = 0.0;
            }
        }

        deliveries
    }
}

// ────────────────────────────────────────────────────────────────── wages

/// D2: wages are a per-unit component, not a global formula, so tiers tune
/// independently and a future non-harvesting unit cannot be silently omitted
/// from the wage bill.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Wage(pub f64);

/// What a worker delivers per completed cycle. `delivery_scale` is 1.0 for
/// workers; carts will sample a crew fraction here (D8).
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Payload(pub f64);

// ────────────────────────────────────────────────────────────── treasury

/// D1: bananas are a resource, not entities.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct Treasury {
    bananas: f64,
}

impl Default for Treasury {
    fn default() -> Self {
        Self { bananas: 0.0 }
    }
}

impl Treasury {
    /// Rejects saved values that are negative, non-finite, or beyond the f64
    /// integer-safe range. Fractional values are legitimate - wages drain
    /// continuously - so there is deliberately no whole-number check.
    pub fn from_saved(bananas: f64) -> Option<Self> {
        is_valid_banana_count(bananas).then_some(Self { bananas })
    }

    /// The exact balance. This is what gets saved; do not save a rounded value
    /// or every reload silently burns up to a banana.
    pub fn bananas(self) -> f64 {
        self.bananas
    }

    /// One decimal, because at 0.03/s an integer counter only changes every
    /// 33 seconds and reads as a freeze followed by a glitch.
    pub fn display_string(self) -> String {
        format!("{:.1}", self.bananas)
    }

    pub fn credit(&mut self, amount: f64) {
        debug_assert!(amount.is_finite() && amount >= 0.0);
        self.bananas = (self.bananas + amount).min(MAX_SAFE_BANANAS);
    }

    /// Wages are charged even when they cannot be covered. Clamping the
    /// treasury at zero instead would turn unpayable wages into free bananas,
    /// which is a measurable pacing gift and an exploit: spending down to
    /// exactly zero would buy a wage holiday until the next delivery. The
    /// purchase gate ([`HirePlan`]) is what keeps the balance out of the red.
    pub fn charge(&mut self, amount: f64) {
        debug_assert!(amount.is_finite() && amount >= 0.0);
        self.bananas -= amount;
    }

    pub fn restart(&mut self) {
        self.bananas = 0.0;
    }
}

pub fn is_valid_banana_count(value: f64) -> bool {
    value.is_finite() && (0.0..=MAX_SAFE_BANANAS).contains(&value)
}

// ───────────────────────────────────────────────────────────── workforce

/// No reachable economy comes near this, and a save that claims it has been
/// tampered with. The bound matters for two reasons: the avatar spawner loops
/// `current..count` in a single system run, and `next_cost` raises 1.15 to this
/// power, which overflows f64 to infinity around n = 5000 - long before a u32
/// would overflow. A player cannot pass ~253 workers in any case, since the
/// price crosses the integer-safe banana ceiling there.
pub const MAX_WORKERS: u32 = 1_000;

#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Workforce {
    hired: u32,
}

impl Workforce {
    /// Validated like [`Treasury::from_saved`], because local storage is
    /// player-writable: an unchecked count spawns that many entities in one
    /// tick and is re-persisted, so the tab never recovers.
    pub fn from_saved(hired: u32) -> Option<Self> {
        (hired <= MAX_WORKERS).then_some(Self { hired })
    }

    pub fn count(self) -> u32 {
        self.hired
    }

    /// Geometric per type, `cost = b·g^n`, exactly as the oracle computes it.
    /// Deliberately not rounded: rounding up is a 15% premium on the third
    /// worker and an 8-11% drag on early pacing, and the shop shows one decimal
    /// anyway, so there is nothing to gain by it.
    ///
    /// `n` is the number of workers *owned*. This implementation seeds no free
    /// worker (unlike `Params.free_workers = 1`), so the first purchase costs
    /// `WORKER_COST_BASE`. The ladder itself is unshifted - see whitepaper §8.
    pub fn next_cost(self) -> f64 {
        WORKER_COST_BASE * WORKER_COST_GROWTH.powi(self.hired as i32)
    }

    pub fn hire(&mut self) {
        self.hired = (self.hired + 1).min(MAX_WORKERS);
    }

    pub fn restart(&mut self) {
        self.hired = 0;
    }
}

/// Reset every piece of run state together, so the two cannot drift apart.
pub fn restart_run(treasury: &mut Treasury, workforce: &mut Workforce) {
    treasury.restart();
    workforce.restart();
}

// ──────────────────────────────────────────────────────────────── economy

/// I4: gross, wages and net are visible simultaneously, and all three are
/// derived from world state every tick rather than cached (I3').
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq)]
pub struct EconomySnapshot {
    /// Steady-state expected rate, `Σ payload / cycle_time`. The realised rate
    /// depends on phase; over any short window the two disagree, which is why
    /// the readout is labelled an average.
    pub gross_per_sec: f64,
    pub wages_per_sec: f64,
    pub net_per_sec: f64,
}

impl EconomySnapshot {
    pub fn project(workers: u32, multipliers: Multipliers) -> Self {
        let gross_per_sec = workers as f64 * worker_throughput(multipliers);
        let wages_per_sec = workers as f64 * WORKER_WAGE;
        Self {
            gross_per_sec,
            wages_per_sec,
            net_per_sec: gross_per_sec - wages_per_sec,
        }
    }
}

/// Expected seconds between deliveries anywhere in the economy, `1 / Σ(nᵢ/Tᵢ)`.
pub fn mean_delivery_gap(workers: u32, multipliers: Multipliers) -> f64 {
    if workers == 0 {
        return f64::INFINITY;
    }
    cycle_time(multipliers) / workers as f64
}

/// I5 / D15, generalised.
///
/// D15's published formula is cart-specific and returns zero here, because it
/// hard-codes "the lumpy income is carts, the continuous income is the pool".
/// That is false with no carts at all: the pool delivers once every 47.5
/// seconds, and without a reserve the treasury goes underwater.
///
/// The general form - see the amended D15 and `banana_model.py::wage_reserve` -
/// reserves against the *largest* gap between deliveries and credits only the
/// income arriving inside it:
///
/// ```text
/// gap     = maxᵢ (Tᵢ / nᵢ)
/// covered = Σ { rateᵢ : Tᵢ / nᵢ < gap }
/// reserve = 2 × max(0, wages − covered) × gap
/// ```
///
/// Workers are the only source here, so `covered` is zero and the whole thing
/// collapses to `2 × 0.03W × 47.5/W` - a constant 2.85 bananas, independent of
/// W. When a second unit type lands, port the general form rather than
/// extending this one, and do *not* reach for a blended mean gap
/// (`1 / Σ(nᵢ/Tᵢ)`): it agrees here and under-reserves by nearly half once
/// carts exist.
///
/// Measured, this halves the worst treasury dip for a 1.8% pacing cost.
pub fn wage_reserve(workers_after: u32, multipliers: Multipliers) -> f64 {
    if workers_after == 0 {
        return 0.0;
    }
    let wages = workers_after as f64 * WORKER_WAGE;
    2.0 * wages * mean_delivery_gap(workers_after, multipliers)
}

/// Everything the shop needs to render, and the single authority on whether a
/// hire is legal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HirePlan {
    pub cost: f64,
    pub reserve: f64,
    /// Bananas the player actually needs on hand: `cost + reserve`.
    pub required: f64,
    /// Change in net rate if the hire goes through.
    pub net_delta: f64,
    pub affordable: bool,
}

/// I1 (net stays strictly positive) and I5 (cost plus a wage reserve). Both
/// gates are trivially satisfied by workers - a worker's net delta is
/// +0.0753/s at every world state - but they are written in the doc's shape now
/// so that later units are a value change rather than a rewrite of every call
/// site.
pub fn plan_hire(workforce: Workforce, treasury: Treasury, multipliers: Multipliers) -> HirePlan {
    let workers_after = workforce.count() + 1;
    let cost = workforce.next_cost();
    let reserve = wage_reserve(workers_after, multipliers);
    let required = cost + reserve;

    let before = EconomySnapshot::project(workforce.count(), multipliers);
    let after = EconomySnapshot::project(workers_after, multipliers);

    HirePlan {
        cost,
        reserve,
        required,
        net_delta: after.net_per_sec - before.net_per_sec,
        affordable: after.net_per_sec > 0.0 && treasury.bananas() >= required,
    }
}

// ───────────────────────────────────────────────────────────────── jitter

/// A deterministic xorshift64*, so that a burst of hires desynchronises without
/// making the game unreproducible for tests or visual snapshots.
///
/// Draw exactly once per entity at spawn time, in command order. Bevy's query
/// iteration order is not stable across archetype moves, so drawing while
/// iterating would make the determinism claim false the first time an entity is
/// despawned.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Jitter {
    state: u64,
}

const JITTER_SEED: u64 = 0x9E37_79B9_7F4A_7C15;

impl Default for Jitter {
    fn default() -> Self {
        Self { state: JITTER_SEED }
    }
}

impl Jitter {
    pub fn restart(&mut self) {
        *self = Self::default();
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform in 0.0..1.0.
    pub fn unit(&mut self) -> f64 {
        // 53 bits: exactly the f64 mantissa, so the mapping is uniform.
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// A starting cycle phase for a newly hired worker, in seconds.
    ///
    /// D16 randomises the initial phase so that workers bought in one burst do
    /// not deliver in lockstep.
    ///
    /// Uniform in time over the **whole** cycle - not over the outbound leg,
    /// and not over segments. Both narrower choices are tempting and both are
    /// wrong:
    ///
    /// - Sampling a segment and then a position within it puts 25% of workers
    ///   in Unload, which is 5.3% of the cycle, and biases first-cycle income
    ///   upward by about 25%. It self-corrects after one cycle, so only a
    ///   first-minute test ever catches it.
    /// - Confining the window to the outbound leg would guarantee a new worker
    ///   appears empty-handed and already walking, which reads better. But it
    ///   leaves a 27.5-second stretch of every cycle in which a burst-bought
    ///   cohort delivers nothing, and the wage reserve is derived from a mean
    ///   gap of `T/W`. Measured, the treasury goes underwater from the fourth
    ///   worker on: the dip is `0.03 × W × 27.5`, which passes 2.85 at W = 4
    ///   and keeps growing, while the full-cycle window holds the dip at a
    ///   constant 1.425 for every W.
    ///
    /// The cost is that a hire can appear mid-route, so the purchase needs its
    /// own visible cue rather than relying on a monkey walking out of the
    /// stall - see the highlight in `worker::spawn_missing_workers`.
    pub fn spawn_phase(&mut self, multipliers: Multipliers) -> f64 {
        self.unit() * cycle_time(multipliers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIM_DT: f64 = 1.0 / SIM_HZ;

    fn base() -> Multipliers {
        Multipliers::default()
    }

    // ───────────────────────────────────────────────────────── cycle shape

    #[test]
    fn segment_durations_match_the_whitepaper() {
        let m = base();

        assert_eq!(Segment::ToGrove.duration(m), 20.0);
        assert_eq!(Segment::Pick.duration(m), 5.0);
        assert_eq!(Segment::ToDepot.duration(m), 20.0);
        assert_eq!(Segment::Unload.duration(m), 2.5);
        assert_eq!(cycle_time(m), 47.5);
        // Travel is 84% of a worker's life; that asymmetry is what makes Chefs
        // worth buying later (whitepaper §2).
        assert!((worker_throughput(m) - 5.0 / 47.5).abs() < 1e-15);
    }

    #[test]
    fn a_round_trip_delivers_on_tick_950_not_951() {
        // 400 + 100 + 400 + 50 ticks at 20 Hz. Delivery must land on the tick
        // that exactly completes the work, which is what the inclusive
        // `needed <= budget` comparison buys.
        let m = base();
        let mut cycle = HarvestCycle::default();
        let mut delivered_on = None;

        for tick in 1..=1_000 {
            if cycle.advance(SIM_DT, m) > 0 && delivered_on.is_none() {
                delivered_on = Some(tick);
            }
        }

        assert_eq!(delivered_on, Some(950));
    }

    #[test]
    fn segment_boundaries_do_not_cost_a_tick() {
        // The naive `remaining <= 0.0` test overshoots Pick and Unload by one
        // tick each on binary residual alone, which would make the cycle 47.6s.
        let m = base();
        let mut cycle = HarvestCycle::default();
        let mut boundaries = Vec::new();

        for tick in 1..=950 {
            let before = cycle.segment();
            cycle.advance(SIM_DT, m);
            if cycle.segment() != before {
                boundaries.push((before, tick));
            }
        }

        assert_eq!(
            boundaries,
            vec![
                (Segment::ToGrove, 400),
                (Segment::Pick, 500),
                (Segment::ToDepot, 900),
                (Segment::Unload, 950),
            ]
        );
    }

    #[test]
    fn leftover_budget_carries_across_segment_boundaries() {
        // Discarding the leftover would lose dt/2 per boundary: 0.1s per cycle,
        // a 0.21% throughput error that grows as multipliers shorten the cycle.
        let m = base();
        let mut cycle = HarvestCycle::default();
        let mut deliveries = 0;

        for _ in 0..20_000 {
            deliveries += cycle.advance(SIM_DT, m);
        }

        // 1000 seconds / 47.5 = 21.05 cycles.
        assert_eq!(deliveries, 21);
    }

    #[test]
    fn realised_rate_converges_to_the_projected_rate() {
        let m = base();
        for workers in 1..=10u32 {
            let mut cycles: Vec<_> = (0..workers)
                .map(|index| HarvestCycle::at_phase(index as f64 * 4.3, m))
                .collect();
            let mut delivered = 0.0;

            for _ in 0..(600.0 * SIM_HZ) as u32 {
                for cycle in &mut cycles {
                    delivered += cycle.advance(SIM_DT, m) as f64 * WORKER_PAYLOAD;
                }
            }

            let realised = delivered / 600.0;
            let projected = EconomySnapshot::project(workers, m).gross_per_sec;
            assert!(
                (realised - projected).abs() / projected < 0.05,
                "workers={workers} realised={realised} projected={projected}"
            );
        }
    }

    #[test]
    fn a_multiplier_bought_mid_trip_speeds_up_the_remainder() {
        // D13's whole point: remaining work is invariant under a multiplier
        // change, so nobody teleports and nobody loses ground.
        let mut cycle = HarvestCycle::default();
        cycle.advance(10.0, base());
        assert_eq!(cycle.segment_fraction(), 0.5);

        let chefs = Multipliers {
            speed: 2.0,
            ..base()
        };
        cycle.advance(5.0, chefs);

        assert_eq!(cycle.segment(), Segment::Pick);
        assert_eq!(cycle.segment_fraction(), 0.0);
    }

    // ─────────────────────────────────────────────────────────────── phase

    #[test]
    fn at_phase_lands_in_the_right_segment() {
        let m = base();

        assert_eq!(HarvestCycle::at_phase(0.0, m), HarvestCycle::default());
        assert_eq!(HarvestCycle::at_phase(10.0, m).segment(), Segment::ToGrove);
        assert_eq!(HarvestCycle::at_phase(10.0, m).segment_fraction(), 0.5);
        assert_eq!(HarvestCycle::at_phase(22.0, m).segment(), Segment::Pick);
        assert_eq!(HarvestCycle::at_phase(30.0, m).segment(), Segment::ToDepot);
        assert_eq!(HarvestCycle::at_phase(46.0, m).segment(), Segment::Unload);
        // Phases wrap rather than falling off the end.
        assert_eq!(HarvestCycle::at_phase(47.5, m), HarvestCycle::default());
    }

    #[test]
    fn a_phase_offset_delays_delivery_by_exactly_that_offset() {
        let m = base();
        let mut cycle = HarvestCycle::at_phase(7.5, m);
        let mut delivered_on = None;

        for tick in 1..=1_000 {
            if cycle.advance(SIM_DT, m) > 0 && delivered_on.is_none() {
                delivered_on = Some(tick);
            }
        }

        // 47.5 - 7.5 = 40.0 s = 800 ticks.
        assert_eq!(delivered_on, Some(800));
    }

    #[test]
    fn spawn_phase_is_uniform_in_time_across_the_whole_cycle() {
        let m = base();
        let mut jitter = Jitter::default();
        let mut sum = 0.0;
        let mut in_segment = [0u32; 4];
        let samples = 40_000;

        for _ in 0..samples {
            let phase = jitter.spawn_phase(m);
            assert!((0.0..cycle_time(m)).contains(&phase));
            let segment = HarvestCycle::at_phase(phase, m).segment();
            in_segment[Segment::ORDER.iter().position(|s| *s == segment).unwrap()] += 1;
            sum += phase;
        }

        // Uniform in time over 0..47.5 has mean 23.75.
        let mean = sum / samples as f64;
        assert!((mean - 23.75).abs() < 0.5, "mean={mean}");

        // Each segment must be hit in proportion to its *duration*, not its
        // count. Sampling a segment first would give every segment 25%, which
        // would put a quarter of new workers in a phase worth 5.3% of a cycle.
        for (index, segment) in Segment::ORDER.iter().enumerate() {
            let share = in_segment[index] as f64 / samples as f64;
            let expected = segment.duration(m) / cycle_time(m);
            assert!(
                (share - expected).abs() < 0.01,
                "{segment:?}: {share} vs {expected}"
            );
        }
    }

    #[test]
    fn jitter_is_reproducible_and_resets() {
        let mut jitter = Jitter::default();
        let first: Vec<f64> = (0..8).map(|_| jitter.unit()).collect();
        jitter.restart();
        let second: Vec<f64> = (0..8).map(|_| jitter.unit()).collect();

        assert_eq!(first, second);
        assert!(first.windows(2).any(|pair| pair[0] != pair[1]));
        assert!(first.iter().all(|value| (0.0..1.0).contains(value)));
    }

    // ──────────────────────────────────────────────────────────── treasury

    #[test]
    fn manual_harvest_credits_exactly_one_banana() {
        let mut treasury = Treasury::default();

        treasury.credit(BANANAS_PER_HARVEST);

        assert_eq!(treasury.bananas(), 1.0);
        assert_eq!(treasury.display_string(), "1.0");
    }

    #[test]
    fn credit_saturates_at_the_integer_safe_ceiling() {
        let mut treasury = Treasury::from_saved(MAX_SAFE_BANANAS).unwrap();

        treasury.credit(WORKER_PAYLOAD);

        assert_eq!(treasury.bananas(), MAX_SAFE_BANANAS);
    }

    #[test]
    fn wages_are_charged_even_when_they_cannot_be_covered() {
        // Clamping at zero would hand out free bananas; the purchase gate, not
        // the drain, is what keeps a player solvent.
        let mut treasury = Treasury::default();

        treasury.charge(1.5);

        assert_eq!(treasury.bananas(), -1.5);
    }

    #[test]
    fn saved_count_must_be_nonnegative_finite_and_safe_but_may_be_fractional() {
        assert!(Treasury::from_saved(42.0).is_some());
        // Wages drain continuously, so a fractional save is legitimate.
        assert!(Treasury::from_saved(1.5).is_some());
        assert!(Treasury::from_saved(-1.0).is_none());
        assert!(Treasury::from_saved(f64::INFINITY).is_none());
        assert!(Treasury::from_saved(f64::NAN).is_none());
        assert!(Treasury::from_saved(MAX_SAFE_BANANAS + 2.0).is_none());
    }

    #[test]
    fn a_saved_worker_count_must_be_within_reach() {
        assert_eq!(Workforce::from_saved(7).map(Workforce::count), Some(7));
        assert_eq!(
            Workforce::from_saved(MAX_WORKERS).map(Workforce::count),
            Some(MAX_WORKERS)
        );
        // A tampered save would otherwise spawn four billion entities in one
        // tick, and the cost ladder's `powi` would overflow to infinity.
        assert_eq!(Workforce::from_saved(MAX_WORKERS + 1), None);
        assert_eq!(Workforce::from_saved(u32::MAX), None);
        // The cap has to keep the geometric ladder finite - `1.15^100_000` is
        // infinity - and has to sit above anything a player can reach.
        assert!(
            Workforce::from_saved(MAX_WORKERS)
                .unwrap()
                .next_cost()
                .is_finite()
        );
        let last_affordable = (0..MAX_WORKERS)
            .take_while(|n| Workforce::from_saved(*n).unwrap().next_cost() <= MAX_SAFE_BANANAS)
            .count() as u32;
        assert!(
            last_affordable < MAX_WORKERS,
            "the cap is below the last affordable worker ({last_affordable})"
        );

        // Hiring saturates rather than wrapping past the cap.
        let mut at_cap = Workforce::from_saved(MAX_WORKERS).unwrap();
        at_cap.hire();
        assert_eq!(at_cap.count(), MAX_WORKERS);
    }

    #[test]
    fn restart_clears_treasury_and_workforce_together() {
        let mut treasury = Treasury::from_saved(12.0).unwrap();
        let mut workforce = Workforce::from_saved(4).unwrap();

        restart_run(&mut treasury, &mut workforce);

        assert_eq!(treasury, Treasury::default());
        assert_eq!(workforce, Workforce::default());
    }

    // ─────────────────────────────────────────────────── costs and gating

    #[test]
    fn cost_ladder_is_geometric_and_unrounded() {
        // Matches the oracle's `cost("worker", ...)` with n = workers owned.
        let expected = [
            4.0,
            4.6,
            5.29,
            6.083_499_999_999_999,
            6.996_024_999_999_999,
            8.045_428_749_999_998,
        ];

        for (hired, want) in expected.iter().enumerate() {
            let workforce = Workforce::from_saved(hired as u32).unwrap();
            assert!(
                (workforce.next_cost() - want).abs() < 1e-12,
                "hired={hired} got={} want={want}",
                workforce.next_cost()
            );
        }
    }

    #[test]
    fn cost_ladder_matches_the_closed_form_across_the_reachable_range() {
        for hired in 0..30u32 {
            let workforce = Workforce::from_saved(hired).unwrap();
            let want = WORKER_COST_BASE * WORKER_COST_GROWTH.powf(hired as f64);
            assert!((workforce.next_cost() - want).abs() < 1e-12);
        }
    }

    #[test]
    fn the_wage_reserve_is_constant_for_a_worker_only_economy() {
        let m = base();
        // 2 × (W × 0.03) × (47.5 / W) = 2.85, independent of W.
        for workers_after in 1..=25u32 {
            assert!((wage_reserve(workers_after, m) - 2.85).abs() < 1e-12);
        }
        assert_eq!(wage_reserve(0, m), 0.0);
    }

    #[test]
    fn the_first_hire_needs_cost_plus_reserve() {
        let m = base();
        let workforce = Workforce::default();

        let at_cost = plan_hire(workforce, Treasury::from_saved(4.0).unwrap(), m);
        assert_eq!(at_cost.cost, 4.0);
        assert_eq!(at_cost.reserve, 2.85);
        assert_eq!(at_cost.required, 6.85);
        assert!(!at_cost.affordable);

        let at_required = plan_hire(workforce, Treasury::from_saved(6.85).unwrap(), m);
        assert!(at_required.affordable);
    }

    #[test]
    fn hiring_always_raises_net_so_invariant_i1_holds() {
        let m = base();
        let per_worker = worker_throughput(m) - WORKER_WAGE;
        assert!(per_worker > 0.0);

        for hired in 0..40u32 {
            let plan = plan_hire(
                Workforce::from_saved(hired).unwrap(),
                Treasury::from_saved(MAX_SAFE_BANANAS).unwrap(),
                m,
            );
            assert!((plan.net_delta - per_worker).abs() < 1e-12);
            assert!(plan.affordable);
        }
    }

    #[test]
    fn snapshot_reports_gross_wages_and_net_together() {
        let snapshot = EconomySnapshot::project(1, base());

        assert!((snapshot.gross_per_sec - 0.105_263_157_894_736_84).abs() < 1e-15);
        assert!((snapshot.wages_per_sec - 0.03).abs() < 1e-15);
        assert!((snapshot.net_per_sec - 0.075_263_157_894_736_84).abs() < 1e-15);
        assert_eq!(
            EconomySnapshot::project(0, base()),
            EconomySnapshot::default()
        );
    }

    // ─────────────────────────────────────────── the settled trajectory

    #[test]
    fn one_worker_from_zero_follows_the_expected_trajectory() {
        // B(t) = 5·floor(t/47.5) - 0.03t, sampled at 20 Hz with deliveries
        // credited before wages are charged. The checkpoints are the values a
        // player would read off the counter.
        let m = base();
        let mut treasury = Treasury::default();
        let mut cycle = HarvestCycle::default();
        let mut seen = Vec::new();
        let checkpoints = [400u32, 900, 949, 950, 1900, 2400];

        for tick in 1..=2_400u32 {
            let deliveries = cycle.advance(SIM_DT, m);
            treasury.credit(deliveries as f64 * WORKER_PAYLOAD);
            treasury.charge(WORKER_WAGE * SIM_DT);
            if checkpoints.contains(&tick) {
                seen.push((tick, (treasury.bananas() * 1e4).round() / 1e4));
            }
        }

        assert_eq!(
            seen,
            vec![
                (400, -0.6),    // reached the grove
                (900, -1.35),   // back at the stall
                (949, -1.4235), // the deepest the balance ever goes
                (950, 3.575),   // first delivery
                (1900, 7.15),   // second delivery
                (2400, 6.4),    // t = 120 s
            ]
        );
        // A zero-clamped treasury would read 7.825 here. The 1.425 gap is the
        // clamp's entire signature: bananas conjured from unpaid wages.
        assert!((treasury.bananas() - 6.4).abs() < 1e-9);
    }

    #[test]
    fn the_reserve_keeps_a_solvent_player_out_of_the_red() {
        // Buy the moment the gate allows it and the balance never crosses zero.
        let m = base();
        let mut treasury = Treasury::default();
        let mut workforce = Workforce::default();
        let mut jitter = Jitter::default();
        let mut cycles: Vec<HarvestCycle> = Vec::new();
        let mut worst = f64::INFINITY;

        // Seed the run the way a player does: by hand, up to the first gate.
        treasury.credit(7.0);

        for _ in 0..(1_200.0 * SIM_HZ) as u32 {
            let plan = plan_hire(workforce, treasury, m);
            if plan.affordable {
                treasury.charge(plan.cost);
                workforce.hire();
                cycles.push(HarvestCycle::at_phase(jitter.spawn_phase(m), m));
            }

            let mut delivered = 0.0;
            for cycle in &mut cycles {
                delivered += cycle.advance(SIM_DT, m) as f64 * WORKER_PAYLOAD;
            }
            treasury.credit(delivered);
            treasury.charge(workforce.count() as f64 * WORKER_WAGE * SIM_DT);
            worst = worst.min(treasury.bananas());
        }

        assert!(worst >= 0.0, "treasury dipped to {worst}");
        assert!(workforce.count() >= 8, "only {} hired", workforce.count());
    }
}
