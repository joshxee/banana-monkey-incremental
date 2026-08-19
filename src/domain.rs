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
/// Bananas per second, per worker (D2: this is a per-unit figure, and
/// [`CycleSpec::wage`] is what each worker's meal is actually derived from).
pub const WORKER_WAGE: f64 = 0.03;
/// Share of a worker's round trip spent eating at the stall, immediately after
/// unloading. At the base parameters this is 2.5 s of a 50 s cycle.
///
/// This segment is the reason the signing fee needs no wage reserve: a worker is
/// only ever fed out of a delivery it has just made, and its meal is a fraction
/// of that delivery, so paying it can never take the treasury below where it
/// stood before the delivery landed.
///
/// A *fraction* rather than a fixed number of seconds. Fixing it would make
/// eating an ever-larger share of a shortened cycle, so Chefs would raise the
/// cost of labour per second and partly undo their own benefit, and worker
/// throughput would converge to `payload / t_snack` instead of to the pick-rate
/// ceiling the whole design rests on (whitepaper §3).
pub const SNACK_FRACTION: f64 = 0.05;
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

/// Everything that distinguishes one harvester from another, in one place.
///
/// D2 put wages on a per-unit component so tiers tune independently; this is
/// that decision widened to every parameter the cycle reads. A harvester tier is
/// then a `const` here rather than a branch inside the cycle, and
/// [`HarvestCycle`] never learns which kind of unit it is driving.
///
/// `crew` is the one term that is not simply "the worker number, but bigger": a
/// cart's crew picks in parallel, so it divides the picking time only. Travel
/// and unloading are properties of the vehicle, not of how many monkeys are
/// aboard - which is exactly the asymmetry that makes Chefs and Unpackers pull
/// in different directions (whitepaper §5).
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct CycleSpec {
    /// Bananas delivered per completed round trip.
    pub payload: f64,
    /// Metres per second, before `M_speed`.
    pub speed: f64,
    /// Metres to the grove, one way.
    pub distance: f64,
    /// Seconds per banana, at the grove.
    pub t_pick: f64,
    /// Seconds per banana, at the depot.
    pub t_unload: f64,
    /// Share of the round trip spent eating, immediately after unloading.
    pub snack_fraction: f64,
    /// Bananas per second. The meal is derived from this and the cycle time, so
    /// this figure stays true at every multiplier.
    pub wage: f64,
    /// Monkeys picking in parallel. 1 on foot.
    pub crew: f64,
}

impl CycleSpec {
    pub const WORKER: Self = Self {
        payload: WORKER_PAYLOAD,
        speed: WORKER_SPEED,
        distance: GROVE_DISTANCE,
        t_pick: T_PICK,
        t_unload: T_UNLOAD,
        snack_fraction: SNACK_FRACTION,
        wage: WORKER_WAGE,
        crew: 1.0,
    };
}

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
///
/// [`Segment::Snack`] is the fourth addend and has no support role: it is where
/// the worker is paid. Putting the meal *after* the unload, rather than draining
/// wages continuously, is what makes the economy self-funding - see
/// [`HarvestCycle::advance`].
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Segment {
    ToGrove,
    Pick,
    ToDepot,
    Unload,
    Snack,
}

impl Segment {
    pub const ORDER: [Segment; 5] = [
        Segment::ToGrove,
        Segment::Pick,
        Segment::ToDepot,
        Segment::Unload,
        Segment::Snack,
    ];

    /// Work in this segment's own units: metres to walk, or nominal bananas to
    /// pick, unload or eat. D13 stores progress as remaining work rather than
    /// elapsed time, so a multiplier bought mid-trip speeds up the remainder of
    /// every journey already in flight without teleporting anyone.
    pub fn nominal_work(self, spec: CycleSpec, multipliers: Multipliers) -> f64 {
        match self {
            Segment::ToGrove | Segment::ToDepot => spec.distance,
            Segment::Pick | Segment::Unload => spec.payload,
            Segment::Snack => meal(spec, multipliers),
        }
    }

    /// Work consumed per second at the current multipliers.
    pub fn rate(self, spec: CycleSpec, multipliers: Multipliers) -> f64 {
        match self {
            Segment::ToGrove | Segment::ToDepot => spec.speed * multipliers.speed,
            // The crew picks in parallel, so it divides the picking time. It
            // deliberately does not touch travel or unloading: a full cart
            // walks and is emptied at the same rate as an empty one.
            Segment::Pick => spec.crew * multipliers.tech / spec.t_pick,
            Segment::Unload => multipliers.unpack / spec.t_unload,
            Segment::Snack => {
                meal(spec, multipliers) / Segment::Snack.duration(spec, multipliers)
            }
        }
    }

    pub fn duration(self, spec: CycleSpec, multipliers: Multipliers) -> f64 {
        match self {
            // Taken from the cycle rather than derived from work over rate. A
            // snack's nominal work is the meal, and the meal is defined against
            // the cycle time, so deriving this one the usual way would recurse.
            Segment::Snack => cycle_time(spec, multipliers) - work_time(spec, multipliers),
            _ => self.nominal_work(spec, multipliers) / self.rate(spec, multipliers),
        }
    }

    pub fn next(self) -> Self {
        match self {
            Segment::ToGrove => Segment::Pick,
            Segment::Pick => Segment::ToDepot,
            Segment::ToDepot => Segment::Unload,
            Segment::Unload => Segment::Snack,
            Segment::Snack => Segment::ToGrove,
        }
    }

    /// True while the worker has a banana in hand: the one it picked at the
    /// grove, and then the one it keeps back to eat. Presentation only.
    pub fn holds_banana(self) -> bool {
        matches!(self, Segment::ToDepot | Segment::Unload | Segment::Snack)
    }

    /// The worker is standing still, at one end of the route or the other.
    pub fn is_walking(self) -> bool {
        matches!(self, Segment::ToGrove | Segment::ToDepot)
    }
}

/// Seconds a worker spends actually working: the doc's three addends, with
/// travel split into two legs.
pub fn work_time(spec: CycleSpec, multipliers: Multipliers) -> f64 {
    Segment::ORDER
        .iter()
        .filter(|segment| !matches!(segment, Segment::Snack))
        .map(|segment| segment.duration(spec, multipliers))
        .sum()
}

/// Seconds for one full round trip, eating included. Pure in counts and
/// multipliers - no phase appears in it, which is what keeps the projected rate
/// deterministic (I3').
///
/// Inflating the working time by the feeding share, rather than summing every
/// segment, is what keeps [`Segment::Snack`] a constant fraction of the trip
/// without `duration` recursing into itself.
pub fn cycle_time(spec: CycleSpec, multipliers: Multipliers) -> f64 {
    work_time(spec, multipliers) / (1.0 - spec.snack_fraction)
}

/// Bananas one worker eats per round trip.
///
/// Derived from the per-second wage rather than fixed, so that the published
/// `0.03 /s` stays true at every multiplier. A Chef that halves the cycle halves
/// the meal with it; if the meal were a constant, buying Chefs would silently
/// double the cost of labour and undo their own benefit.
pub fn meal(spec: CycleSpec, multipliers: Multipliers) -> f64 {
    spec.wage * cycle_time(spec, multipliers)
}

/// Steady-state bananas per second for a single harvester.
pub fn throughput(spec: CycleSpec, multipliers: Multipliers) -> f64 {
    spec.payload / cycle_time(spec, multipliers)
}

/// Steady-state bananas per second for a single worker on foot.
pub fn worker_throughput(multipliers: Multipliers) -> f64 {
    throughput(CycleSpec::WORKER, multipliers)
}

/// What a single worker earns and eats per round trip. Passed into
/// [`HarvestCycle::advance`] rather than read from constants, so that the cycle
/// stays driven by the entity's own [`Payload`] and [`Wage`] components (D2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CycleTerms {
    pub payload: f64,
    pub meal: f64,
}

impl CycleTerms {
    pub fn new(spec: CycleSpec, multipliers: Multipliers) -> Self {
        Self {
            payload: spec.payload,
            meal: meal(spec, multipliers),
        }
    }
}

/// Bananas that moved during one call to [`HarvestCycle::advance`].
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct CycleOutput {
    pub delivered: f64,
    pub eaten: f64,
}

/// The sole piece of irreducible per-entity state in the simulation (D13).
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct HarvestCycle {
    segment: Segment,
    remaining: f64,
}

impl HarvestCycle {
    /// A unit standing at the stall, about to walk out. There is deliberately
    /// no `Default`: the outbound leg's remaining work is the route length,
    /// which is a property of the unit's [`CycleSpec`] and not a constant.
    pub fn starting(spec: CycleSpec) -> Self {
        Self {
            segment: Segment::ToGrove,
            // Multiplier-independent for the outbound leg: it is a distance.
            remaining: spec.distance,
        }
    }

    /// Construct a cycle at an elapsed-time phase without advancing it.
    /// Advancing here would emit delivery or meal output while merely placing
    /// an avatar on the route.
    pub fn from_phase(phase: f64, spec: CycleSpec, multipliers: Multipliers) -> Self {
        let cycle = cycle_time(spec, multipliers);
        let mut remaining_phase = phase.rem_euclid(cycle);

        for segment in Segment::ORDER {
            let duration = segment.duration(spec, multipliers);
            if remaining_phase < duration {
                let fraction = (remaining_phase / duration).clamp(0.0, 1.0);
                return Self {
                    segment,
                    remaining: segment.nominal_work(spec, multipliers) * (1.0 - fraction),
                };
            }
            remaining_phase -= duration;
        }

        // Floating-point subtraction can land exactly on the exclusive upper
        // bound. Treat that boundary as phase zero.
        Self::starting(spec)
    }

    pub fn segment(self) -> Segment {
        self.segment
    }

    /// How far through the current segment the worker is, in 0.0..=1.0.
    /// Rendering derives position from this, so the avatar is a pure function
    /// of simulation state.
    pub fn segment_fraction(self, spec: CycleSpec, multipliers: Multipliers) -> f64 {
        let nominal = self.segment.nominal_work(spec, multipliers);
        if nominal <= 0.0 {
            return 1.0;
        }
        (1.0 - self.remaining / nominal).clamp(0.0, 1.0)
    }

    /// A worker that has finished eating everything it can afford and is waiting
    /// for the larder to refill. Its cycle is frozen until then.
    pub fn is_hungry(self) -> bool {
        self.segment == Segment::Snack && self.remaining <= 0.0
    }

    /// Consume `dt` seconds of work, reporting what the worker delivered and ate.
    ///
    /// `larder` is the bananas available to be eaten *right now*: the treasury at
    /// the top of the tick, plus anything delivered earlier in the same tick.
    /// It is credited on delivery and debited on eating, so the caller can
    /// settle a whole tick's worth of workers and know the treasury cannot go
    /// negative. A worker that cannot afford its meal stalls at the stall rather
    /// than eating on credit - unpaid wages are never forgiven, because
    /// forgiving them would make spending down to zero a free wage holiday.
    ///
    /// This is a *time budget* rather than a work subtraction, and the leftover
    /// budget is deliberately carried across segment boundaries. Both matter:
    ///
    /// - `remaining -= rate * dt` followed by `remaining <= 0.0` leaves a
    ///   positive binary residual on Pick (9.4e-15) and Unload (1.0e-15) at
    ///   20 Hz, costing a whole extra tick each and stretching the cycle.
    /// - Discarding the leftover loses `dt/2` per boundary, which is a 0.2%
    ///   throughput error at these parameters and *grows* as multipliers
    ///   shorten the cycle.
    pub fn advance(
        &mut self,
        dt: f64,
        spec: CycleSpec,
        multipliers: Multipliers,
        terms: CycleTerms,
        larder: &mut f64,
    ) -> CycleOutput {
        debug_assert!(dt.is_finite() && dt >= 0.0);
        let mut budget = dt;
        let mut output = CycleOutput::default();
        // A zero or non-finite rate would make `needed` infinite or NaN and the
        // loop non-terminating, so the guard is load-bearing, not defensive.
        let mut guard = 0;

        while budget > 0.0 && guard < 64 {
            guard += 1;
            let rate = self.segment.rate(spec, multipliers);
            if !(rate > 0.0 && rate.is_finite()) {
                debug_assert!(false, "segment rate must be positive and finite");
                break;
            }

            let needed = self.remaining / rate;
            // Repeated subtraction leaves a residual: 0.05 is not exactly
            // representable in binary, so after 100 ticks a 5-banana pick
            // segment has ~9.4e-15 of work left. Without a tolerance that
            // residual costs a whole extra tick at every boundary. The
            // tolerance is a billionth of the segment's duration - about 20
            // nanoseconds.
            let tolerance = self.segment.duration(spec, multipliers) * 1e-9;
            if needed > budget + tolerance {
                self.remaining -= rate * budget;
                break;
            }

            match self.segment {
                Segment::Unload => {
                    output.delivered += terms.payload;
                    *larder += terms.payload;
                }
                Segment::Snack => {
                    // Exactly, with no slack. `tolerance` above is in seconds
                    // and belongs to the time budget; reusing it here would
                    // compare a duration against a quantity of bananas and let
                    // a worker eat a couple of nano-bananas it does not have -
                    // enough to take the treasury negative and trip the
                    // `debug_assert` in `Treasury::charge`. A worker that
                    // misses its meal by 2e-9 simply waits one more tick.
                    if *larder < terms.meal {
                        // Nothing to eat. Hold the worker here - not on the
                        // next leg - so the debt is still owed when food
                        // arrives, and so the idle sprite reads as hunger.
                        self.remaining = 0.0;
                        break;
                    }
                    output.eaten += terms.meal;
                    *larder -= terms.meal;
                }
                _ => {}
            }

            budget = (budget - needed).max(0.0);
            self.segment = self.segment.next();
            self.remaining = self.segment.nominal_work(spec, multipliers);
        }

        // The guard is a safety net against a zero rate, not a work limit. If
        // it is ever the thing that ends the loop, segments are completing
        // faster than 64 per tick and the simulation is silently dropping
        // work - a throughput cliff that would appear only once support
        // multipliers shorten the cycle below a few milliseconds.
        debug_assert!(guard < 64, "segment budget loop hit its iteration guard");

        output
    }
}

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

    /// Callers must not spend what is not there. Two rules keep that true and
    /// between them make the balance structurally non-negative: a hire is gated
    /// on [`HirePlan::affordable`], and a meal is gated on the larder inside
    /// [`HarvestCycle::advance`].
    ///
    /// The `debug_assert` is the real check - it fails the build's tests on any
    /// caller that overdraws. The `max(0.0)` behind it is a release-only last
    /// resort, chosen over letting the balance go negative because a negative
    /// banana count on screen is a worse failure than a rounding-sized one that
    /// is silently absorbed. It should be unreachable; if it ever fires in
    /// practice, the bug is in the gate, not here.
    pub fn charge(&mut self, amount: f64) {
        debug_assert!(amount.is_finite() && amount >= 0.0);
        debug_assert!(
            amount <= self.bananas + 1e-9,
            "charged {amount} against a balance of {}",
            self.bananas
        );
        self.bananas = (self.bananas - amount).max(0.0);
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
    /// Steady-state expected rate, `Σ payload / cycle_time`, counting only
    /// workers that are actually working. The realised rate depends on phase;
    /// over any short window the two disagree, which is why the readout is
    /// labelled an average.
    pub gross_per_sec: f64,
    pub wages_per_sec: f64,
    pub net_per_sec: f64,
    pub workers: u32,
    /// Workers stalled at the stall with a meal they cannot afford.
    pub stalled: u32,
}

impl EconomySnapshot {
    /// `stalled` workers are excluded from *both* sides. A starving worker
    /// harvests nothing, and it is not eating either - that is the whole point
    /// of the stall - so counting it in the wage bill would be as wrong as
    /// counting it in production.
    ///
    /// Excluding it at all is the fix for a readout that used to lie: the
    /// projection was a pure function of the worker *count*, so a wholly
    /// stalled workforce reported `+6.0/min` indefinitely while producing
    /// nothing, and no number on screen explained why the pile had stopped
    /// growing. That is the same complaint that started this redesign.
    pub fn project(workers: u32, stalled: u32, multipliers: Multipliers) -> Self {
        let working = workers.saturating_sub(stalled) as f64;
        let gross_per_sec = working * worker_throughput(multipliers);
        let wages_per_sec = working * WORKER_WAGE;
        Self {
            gross_per_sec,
            wages_per_sec,
            net_per_sec: gross_per_sec - wages_per_sec,
            workers,
            stalled: stalled.min(workers),
        }
    }
}

/// Everything the shop needs to render, and the single authority on whether a
/// hire is legal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HirePlan {
    /// The signing fee, and the whole of what the player must have on hand.
    pub cost: f64,
    /// Bananas this worker will eat per round trip, for the shop to explain
    /// itself with. Not a gate: it is paid out of the delivery it follows.
    pub meal: f64,
    /// Change in net rate if the hire goes through.
    pub net_delta: f64,
    pub affordable: bool,
}

/// I1 (net stays strictly positive) and I5 (the player can pay).
///
/// I5 used to add a wage reserve on top of the fee, because wages drained
/// continuously and a fresh hire spent 50 seconds costing bananas before it
/// earned any. Feeding a worker out of the delivery it just made removes that
/// exposure entirely, so the fee *is* the requirement: `cost + reserve` was both
/// a misleading price and a solution to a problem the cycle no longer has.
///
/// The I1 gate is retained even though a worker's net delta is +0.07/s at every
/// world state, so that later units are a value change rather than a rewrite of
/// every call site.
pub fn plan_hire(workforce: Workforce, treasury: Treasury, multipliers: Multipliers) -> HirePlan {
    let cost = workforce.next_cost();

    // Hypothetical steady state, so no stalls: this is "what would this hire be
    // worth", not "what is happening right now".
    let before = EconomySnapshot::project(workforce.count(), 0, multipliers);
    let after = EconomySnapshot::project(workforce.count() + 1, 0, multipliers);

    HirePlan {
        cost,
        meal: meal(CycleSpec::WORKER, multipliers),
        net_delta: after.net_per_sec - before.net_per_sec,
        affordable: after.net_per_sec > 0.0 && treasury.bananas() >= cost,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIM_DT: f64 = 1.0 / SIM_HZ;
    /// Enough that a test which is not about hunger never trips over it.
    const FULL_LARDER: f64 = 1e9;

    fn base() -> Multipliers {
        Multipliers::default()
    }

    const SPEC: CycleSpec = CycleSpec::WORKER;

    fn terms(multipliers: Multipliers) -> CycleTerms {
        CycleTerms::new(SPEC, multipliers)
    }

    fn fresh() -> HarvestCycle {
        HarvestCycle::starting(SPEC)
    }

    /// One tick against a bottomless larder.
    fn tick(cycle: &mut HarvestCycle, multipliers: Multipliers) -> CycleOutput {
        let mut larder = FULL_LARDER;
        cycle.advance(SIM_DT, SPEC, multipliers, terms(multipliers), &mut larder)
    }

    /// Wind a fresh cycle forward to an arbitrary phase, the way the simulation
    /// would: there is no way to construct one mid-route out of thin air.
    fn at_phase(phase: f64, multipliers: Multipliers) -> HarvestCycle {
        let mut cycle = fresh();
        let mut larder = FULL_LARDER;
        cycle.advance(phase, SPEC, multipliers, terms(multipliers), &mut larder);
        cycle
    }

    // ───────────────────────────────────────────────────────── cycle shape

    #[test]
    fn segment_durations_match_the_whitepaper() {
        let m = base();

        assert_eq!(Segment::ToGrove.duration(SPEC, m), 20.0);
        assert_eq!(Segment::Pick.duration(SPEC, m), 5.0);
        assert_eq!(Segment::ToDepot.duration(SPEC, m), 20.0);
        assert_eq!(Segment::Unload.duration(SPEC, m), 2.5);
        assert_eq!(Segment::Snack.duration(SPEC, m), 2.5);
        assert_eq!(work_time(SPEC, m), 47.5);
        assert_eq!(cycle_time(SPEC, m), 50.0);
        // Every segment must still add up to the trip, even though the cycle is
        // derived from the working time rather than summed from the parts.
        let summed: f64 = Segment::ORDER.iter().map(|s| s.duration(SPEC, m)).sum();
        assert!((summed - cycle_time(SPEC, m)).abs() < 1e-12);
        // Travel is 80% of a worker's life; that asymmetry is what makes Chefs
        // worth buying later (whitepaper §2).
        assert_eq!(worker_throughput(m), 0.1);
        // A worker eats 1.5 of the 5 it brings home, which is the published
        // 0.03/s expressed per trip instead of per second.
        assert_eq!(meal(SPEC, m), 1.5);
        assert!((meal(SPEC, m) / cycle_time(SPEC, m) - WORKER_WAGE).abs() < 1e-15);
    }

    #[test]
    fn the_meal_tracks_the_cycle_so_the_wage_rate_is_multiplier_invariant() {
        // A constant meal would make Chefs raise the cost of labour per second
        // and partly undo their own benefit. Deriving it from the cycle keeps
        // the published 0.03/s true at every world state.
        for speed in [1.0, 1.15, 2.0, 7.5] {
            let m = Multipliers {
                speed,
                ..Multipliers::default()
            };
            assert!(
                (meal(SPEC, m) / cycle_time(SPEC, m) - WORKER_WAGE).abs() < 1e-15,
                "{speed}"
            );
            // And eating stays the same share of the trip, so a shortened cycle
            // does not turn into a life spent at the stall.
            assert!(
                (Segment::Snack.duration(SPEC, m) / cycle_time(SPEC, m) - SNACK_FRACTION).abs() < 1e-15,
                "{speed}"
            );
        }
    }

    #[test]
    fn throughput_still_converges_to_the_pick_rate_ceiling() {
        // Whitepaper §3: with travel and unloading driven to zero, every harvest
        // method converges to the same per-monkey ceiling. Feeding costs a flat
        // 5% of it and nothing more - which is exactly why the snack is a share
        // of the trip and not a fixed 2.5 seconds.
        let m = Multipliers {
            speed: 1e6,
            unpack: 1e6,
            ..Multipliers::default()
        };
        let ceiling = m.tech / T_PICK;

        let realised = worker_throughput(m);
        assert!(
            (realised / ceiling - (1.0 - SNACK_FRACTION)).abs() < 1e-4,
            "{realised} vs {ceiling}"
        );
    }

    #[test]
    fn a_round_trip_delivers_on_tick_950_and_eats_on_tick_1000() {
        // 400 + 100 + 400 + 50 + 50 ticks at 20 Hz. Both events must land on
        // the tick that exactly completes the work, which is what the inclusive
        // `needed <= budget` comparison buys.
        let m = base();
        let mut cycle = fresh();
        let (mut delivered_on, mut ate_on) = (None, None);

        for t in 1..=1_050 {
            let output = tick(&mut cycle, m);
            if output.delivered > 0.0 && delivered_on.is_none() {
                delivered_on = Some(t);
            }
            if output.eaten > 0.0 && ate_on.is_none() {
                ate_on = Some(t);
            }
        }

        assert_eq!(delivered_on, Some(950));
        assert_eq!(ate_on, Some(1_000));
    }

    #[test]
    fn segment_boundaries_do_not_cost_a_tick() {
        // The naive `remaining <= 0.0` test overshoots Pick, Unload and Snack
        // by one tick each on binary residual alone.
        let m = base();
        let mut cycle = fresh();
        let mut boundaries = Vec::new();

        for t in 1..=1_000 {
            let before = cycle.segment();
            tick(&mut cycle, m);
            if cycle.segment() != before {
                boundaries.push((before, t));
            }
        }

        assert_eq!(
            boundaries,
            vec![
                (Segment::ToGrove, 400),
                (Segment::Pick, 500),
                (Segment::ToDepot, 900),
                (Segment::Unload, 950),
                (Segment::Snack, 1_000),
            ]
        );
    }

    #[test]
    fn leftover_budget_carries_across_segment_boundaries() {
        // Discarding the leftover would lose dt/2 per boundary: 0.125s per
        // cycle, a throughput error that grows as multipliers shorten it.
        let m = base();
        let mut cycle = fresh();
        let mut delivered = 0.0;

        for _ in 0..20_000 {
            delivered += tick(&mut cycle, m).delivered;
        }

        // 1000 seconds / 50 = exactly 20 cycles.
        assert_eq!(delivered, 20.0 * WORKER_PAYLOAD);
    }

    #[test]
    fn realised_rate_converges_to_the_projected_rate() {
        let m = base();
        for workers in 1..=10u32 {
            let mut cycles: Vec<_> = (0..workers)
                .map(|index| at_phase(index as f64 * 4.3, m))
                .collect();
            let mut delivered = 0.0;

            for _ in 0..(600.0 * SIM_HZ) as u32 {
                for cycle in &mut cycles {
                    delivered += tick(cycle, m).delivered;
                }
            }

            let realised = delivered / 600.0;
            let projected = EconomySnapshot::project(workers, 0, m).gross_per_sec;
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
        let mut cycle = fresh();
        let mut larder = FULL_LARDER;
        cycle.advance(10.0, SPEC, base(), terms(base()), &mut larder);
        assert_eq!(cycle.segment_fraction(SPEC, base()), 0.5);

        let chefs = Multipliers {
            speed: 2.0,
            ..base()
        };
        cycle.advance(5.0, SPEC, chefs, terms(chefs), &mut larder);

        assert_eq!(cycle.segment(), Segment::Pick);
        assert_eq!(cycle.segment_fraction(SPEC, chefs), 0.0);
    }

    #[test]
    fn winding_a_cycle_forward_lands_in_the_right_segment() {
        let m = base();

        assert_eq!(at_phase(0.0, m), fresh());
        assert_eq!(at_phase(10.0, m).segment(), Segment::ToGrove);
        assert_eq!(at_phase(10.0, m).segment_fraction(SPEC, m), 0.5);
        assert_eq!(at_phase(22.0, m).segment(), Segment::Pick);
        assert_eq!(at_phase(30.0, m).segment(), Segment::ToDepot);
        assert_eq!(at_phase(46.0, m).segment(), Segment::Unload);
        assert_eq!(at_phase(48.0, m).segment(), Segment::Snack);
        assert_eq!(at_phase(50.0, m), fresh());
    }

    #[test]
    fn a_phase_offset_delays_delivery_by_exactly_that_offset() {
        let m = base();
        let mut cycle = at_phase(7.5, m);
        let mut delivered_on = None;

        for t in 1..=1_000 {
            if tick(&mut cycle, m).delivered > 0.0 && delivered_on.is_none() {
                delivered_on = Some(t);
            }
        }

        // 47.5 - 7.5 = 40.0 s = 800 ticks.
        assert_eq!(delivered_on, Some(800));
    }

    #[test]
    fn every_worker_starts_at_the_stall_facing_the_grove() {
        // A new monkey always walks out of the stall, which is the purchase's
        // visible consequence. Only restored workers receive a phase.
        let fresh = fresh();

        assert_eq!(fresh.segment(), Segment::ToGrove);
        assert_eq!(fresh.segment_fraction(SPEC, base()), 0.0);
        assert!(!fresh.is_hungry());
    }

    #[test]
    fn phase_constructor_uses_elapsed_time_segment_boundaries() {
        let m = base();

        assert_eq!(HarvestCycle::from_phase(0.0, SPEC, m), fresh());
        assert_eq!(HarvestCycle::from_phase(20.0, SPEC, m).segment(), Segment::Pick);
        assert_eq!(HarvestCycle::from_phase(25.0, SPEC, m).segment(), Segment::ToDepot);
        assert_eq!(HarvestCycle::from_phase(45.0, SPEC, m).segment(), Segment::Unload);
        assert_eq!(HarvestCycle::from_phase(47.5, SPEC, m).segment(), Segment::Snack);
        assert_eq!(HarvestCycle::from_phase(50.0, SPEC, m), fresh());

        let halfway_home = HarvestCycle::from_phase(35.0, SPEC, m);
        assert_eq!(halfway_home.segment(), Segment::ToDepot);
        assert_eq!(halfway_home.segment_fraction(SPEC, m), 0.5);
        assert!(halfway_home.segment().holds_banana());
        assert!(!halfway_home.is_hungry());
    }

    // ──────────────────────────────────────────────────────────── feeding

    #[test]
    fn a_worker_is_fed_out_of_the_delivery_it_just_made() {
        // The invariant the whole redesign rests on: within one cycle the
        // credit strictly precedes the debit and strictly exceeds it, so the
        // larder cannot be lower after a cycle than it was before.
        let m = base();
        let mut cycle = fresh();
        let mut larder = 0.0;
        let mut worst = f64::INFINITY;

        for _ in 0..1_000 {
            cycle.advance(SIM_DT, SPEC, m, terms(m), &mut larder);
            worst = worst.min(larder);
        }

        assert!(worst >= 0.0, "larder dipped to {worst}");
        assert!((larder - (WORKER_PAYLOAD - meal(SPEC, m))).abs() < 1e-12);
    }

    #[test]
    fn a_worker_with_nothing_to_eat_stalls_instead_of_eating_on_credit() {
        let m = base();
        let mut cycle = fresh();
        let mut larder = 0.0;

        // Run to the moment the delivery lands, then have the player spend it.
        for _ in 0..950 {
            cycle.advance(SIM_DT, SPEC, m, terms(m), &mut larder);
        }
        assert_eq!(cycle.segment(), Segment::Snack);
        assert_eq!(larder, WORKER_PAYLOAD);
        larder = 0.0;

        // Two full cycles' worth of ticks with an empty larder.
        for _ in 0..2_000 {
            let output = cycle.advance(SIM_DT, SPEC, m, terms(m), &mut larder);
            assert_eq!(output, CycleOutput::default());
        }
        assert!(cycle.is_hungry());
        assert_eq!(larder, 0.0);

        // Feeding it clears exactly the meal that was owed - the debt was
        // deferred, never forgiven, so starving is a penalty and not a wage
        // holiday - and the worker goes straight back to work.
        larder = meal(SPEC, m);
        let output = cycle.advance(SIM_DT, SPEC, m, terms(m), &mut larder);
        assert_eq!(output.eaten, meal(SPEC, m));
        assert_eq!(larder, 0.0);
        assert_eq!(cycle.segment(), Segment::ToGrove);
        assert!(!cycle.is_hungry());
    }

    #[test]
    fn a_crowd_shares_one_larder_without_overdrawing_it() {
        // Deliveries made earlier in a tick are edible later in the same tick,
        // and no combination of workers can take the larder below zero.
        let m = base();
        let mut cycles: Vec<_> = (0..12).map(|i| at_phase(i as f64 * 3.7, m)).collect();
        let mut larder = 0.5;
        let mut worst = f64::INFINITY;

        for _ in 0..(600.0 * SIM_HZ) as u32 {
            for cycle in &mut cycles {
                cycle.advance(SIM_DT, SPEC, m, terms(m), &mut larder);
            }
            worst = worst.min(larder);
        }

        assert!(worst >= 0.0, "larder dipped to {worst}");
        assert!(
            larder > 400.0,
            "12 workers should net ~42/min, got {larder}"
        );
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
    fn saved_count_must_be_nonnegative_finite_and_safe_but_may_be_fractional() {
        assert!(Treasury::from_saved(42.0).is_some());
        // Meals are fractional, so a fractional save is legitimate.
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

    // ─────────────────────────────────────────── costs and gating

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
    fn the_signing_fee_is_the_whole_requirement() {
        // The shop used to ask for the fee plus a 2.85 wage reserve while
        // showing only the fee, which read as a bug. Post-paid meals make the
        // reserve unnecessary, so the price on the button is the price.
        let m = base();
        let workforce = Workforce::default();

        let plan = plan_hire(workforce, Treasury::from_saved(4.0).unwrap(), m);
        assert_eq!(plan.cost, 4.0);
        assert_eq!(plan.meal, 1.5);
        assert!(plan.affordable);

        let short = plan_hire(workforce, Treasury::from_saved(3.9).unwrap(), m);
        assert!(!short.affordable);
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
        let snapshot = EconomySnapshot::project(1, 0, base());

        assert!((snapshot.gross_per_sec - 0.1).abs() < 1e-15);
        assert!((snapshot.wages_per_sec - 0.03).abs() < 1e-15);
        assert!((snapshot.net_per_sec - 0.07).abs() < 1e-15);
        assert_eq!(
            EconomySnapshot::project(0, 0, base()),
            EconomySnapshot::default()
        );

        // A stalled workforce reports what it is actually doing: nothing, and
        // eating nothing while it does it.
        let starving = EconomySnapshot::project(4, 4, base());
        assert_eq!(starving.gross_per_sec, 0.0);
        assert_eq!(starving.wages_per_sec, 0.0);
        assert_eq!(starving.net_per_sec, 0.0);
        assert_eq!(starving.stalled, 4);

        // And a partly stalled one reports the fed fraction.
        let half = EconomySnapshot::project(4, 2, base());
        assert!((half.gross_per_sec - 0.2).abs() < 1e-15);
        assert!((half.wages_per_sec - 0.06).abs() < 1e-15);
    }

    // ─────────────────────────────────────────── the settled trajectory

    #[test]
    fn one_worker_from_zero_follows_the_expected_trajectory() {
        // The player spends their last banana on the hire, so the run starts at
        // exactly zero. The counter holds flat for a trip, jumps a whole
        // payload, then visibly gives 1.5 of it back - the rhythm the economy
        // is meant to read as.
        let m = base();
        let mut treasury = Treasury::default();
        let mut cycle = fresh();
        let mut seen = Vec::new();
        let checkpoints = [400u32, 949, 950, 999, 1_000, 2_000];

        for t in 1..=2_000u32 {
            let mut larder = treasury.bananas();
            let output = cycle.advance(SIM_DT, SPEC, m, terms(m), &mut larder);
            treasury.credit(output.delivered);
            treasury.charge(output.eaten);
            if checkpoints.contains(&t) {
                seen.push((t, (treasury.bananas() * 1e4).round() / 1e4));
            }
        }

        assert_eq!(
            seen,
            vec![
                (400, 0.0),   // reached the grove, nothing spent getting there
                (949, 0.0),   // still nothing: the trip itself is free
                (950, 5.0),   // delivery
                (999, 5.0),   // eating, but the meal settles on the last tick
                (1_000, 3.5), // snack paid for out of the delivery
                (2_000, 7.0), // t = 100 s, two full cycles
            ]
        );
        // The old continuously-drained model read -1.4235 at its worst and only
        // climbed out at the first delivery. This one never goes below zero at
        // all, which is what lets the shop quote a bare signing fee.
    }

    #[test]
    fn buying_the_moment_the_shop_allows_it_never_goes_underwater() {
        let m = base();
        let mut treasury = Treasury::default();
        let mut workforce = Workforce::default();
        let mut cycles: Vec<HarvestCycle> = Vec::new();
        let mut worst = f64::INFINITY;

        // Seed the run the way a player does: by hand, up to the first gate.
        treasury.credit(4.0);

        for _ in 0..(1_200.0 * SIM_HZ) as u32 {
            let plan = plan_hire(workforce, treasury, m);
            if plan.affordable {
                treasury.charge(plan.cost);
                workforce.hire();
                cycles.push(fresh());
            }

            let mut larder = treasury.bananas();
            let (mut delivered, mut eaten) = (0.0, 0.0);
            for cycle in &mut cycles {
                let output = cycle.advance(SIM_DT, SPEC, m, terms(m), &mut larder);
                delivered += output.delivered;
                eaten += output.eaten;
            }
            treasury.credit(delivered);
            treasury.charge(eaten);
            worst = worst.min(treasury.bananas());
        }

        assert!(worst >= 0.0, "treasury dipped to {worst}");
        // Spending every banana the instant it arrives is now a viable play,
        // and it still reaches a real workforce inside twenty minutes.
        assert!(workforce.count() >= 8, "only {} hired", workforce.count());
    }
}
