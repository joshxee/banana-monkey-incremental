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

/// Bananas a full cart carries per round trip.
///
/// The first cart is deliberately a frequent, moderate delivery rather than
/// a huge delivery that spends most of its cycle being unloaded.
pub const CART_PAYLOAD: f64 = 100.0;
/// Metres per second. Three times a monkey on foot, which is why a cart barely
/// travels and instead spends its life being emptied (whitepaper §5).
pub const CART_SPEED: f64 = 15.0;
/// Bananas per second for the whole vehicle, crew included. Its three monkeys
/// stop drawing their individual worker wage while they are aboard.
pub const CART_WAGE: f64 = 0.20;
/// Monkeys a cart needs, and the only number of monkeys a cart ever has.
pub const CART_CREW: u32 = 3;
pub const CART_COST_BASE: f64 = 70.0;
pub const CART_COST_GROWTH: f64 = 1.70;
/// No reachable economy comes near this; it bounds the spawn loop and keeps
/// `1.70^n` finite.
pub const MAX_CARTS: u32 = 500;

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
    /// The Net Cart. Same state machine, different constants - which is the
    /// entire reason this struct exists.
    ///
    /// `crew: 3` divides the picking time only: three monkeys pick in parallel,
    /// but the vehicle walks and is emptied at one rate whoever is aboard. That
    /// asymmetry is not a rule anybody wrote; it falls out of payload and speed,
    /// and it is what makes Chefs a worker's buy and Unpackers a cart's
    /// (whitepaper §5).
    pub const CART: Self = Self {
        payload: CART_PAYLOAD,
        speed: CART_SPEED,
        distance: GROVE_DISTANCE,
        t_pick: T_PICK,
        t_unload: T_UNLOAD,
        snack_fraction: SNACK_FRACTION,
        wage: CART_WAGE,
        crew: CART_CREW as f64,
    };

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

/// D4: every multiplier is additive within its term, `M = 1 + count × bonus`,
/// and only *fed* monkeys count towards it.
///
/// Derived every tick by [`multipliers_for`], never set by hand: speed from
/// Chefs, unpack from Unpackers, and tech from the research *level* rather than
/// from the Technologists who bought it.
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
            Segment::Snack => meal(spec, multipliers) / Segment::Snack.duration(spec, multipliers),
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

/// What a single harvester earns and eats per round trip. Passed into
/// [`HarvestCycle::advance`] rather than read from constants, so that the cycle
/// stays driven by the entity's own [`CycleSpec`] (D2).
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
    /// Bananas set aside at the moment of delivery to pay for the snack that
    /// immediately follows it. Zero everywhere except between Unload and Snack.
    ///
    /// D18 argued that a harvester is solvent because "the credit strictly
    /// precedes the debit within a cycle and strictly exceeds it". That holds
    /// only if nothing else can spend the credit in between - and support staff,
    /// who draw on the same larder but deliver nothing of their own, can. The
    /// gap is small (2.5 s of a worker's 50 s trip) but it is real: measured,
    /// workers stalled 8.4% of the time after a spend-to-zero shock at the
    /// whitepaper's end-state support bill.
    ///
    /// Reserving the meal out of the delivery it is owed against closes the gap
    /// by construction rather than by gate, and it is what makes the cart
    /// possible at all: a cart's meal is about 20.4 bananas, so a cart that missed one
    /// would freeze holding a 100-banana payload, and at an empty pool the
    /// treasury could never climb back to free it.
    earmarked: f64,
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
            earmarked: 0.0,
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
                    // A restored unit is placed, not paid: it earns nothing on
                    // its first partial cycle, so it owes nothing either.
                    earmarked: 0.0,
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

    /// Bananas this unit has reserved against its own next meal. Summed across
    /// the field, this is the slice of the treasury that support staff and the
    /// shop must both leave alone.
    pub fn earmarked(self) -> f64 {
        self.earmarked
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
                    // The whole payload is delivered and credited, so the
                    // counter still reads the arithmetic D18 wanted (+5, then
                    // -1.5). Only the *spendable* share enters the larder; the
                    // meal is reserved on the unit until it eats it.
                    output.delivered += terms.payload;
                    self.earmarked = terms.meal.min(terms.payload);
                    *larder += terms.payload - self.earmarked;
                }
                Segment::Snack => {
                    // Eaten out of this unit's own reservation, never out of the
                    // shared larder, so no amount of spending elsewhere can
                    // stall a harvester that has just delivered. The meal is the
                    // figure locked in at delivery rather than the one implied
                    // by the current multipliers: it is paid for by that
                    // delivery, so it is priced against it.
                    output.eaten += self.earmarked;
                    self.earmarked = 0.0;
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

// ─────────────────────────────────────────────────────────────── support

/// Seconds between one support monkey's meals.
///
/// Support staff have no round trip to be paid out of, so unlike a harvester
/// their meal needs a clock of its own. A bare period rather than anything
/// derived from the harvest cycle: `meal / period` is then `wage` exactly, with
/// no multiplier in either factor, so the published 0.10 and 0.20 per second
/// stay true at every world state. This is the same property D18 needed a
/// *fraction* to obtain for workers, and here it comes for free - a shortened
/// harvest cycle does not shorten a chef's shift.
///
/// Short on purpose. Nothing funds this meal at the moment it falls due, so the
/// period sets how big a lump lands on the larder: at 10 s a chef eats 1.0 and
/// recovers from an empty larder in under two seconds at a modest pool, where a
/// worker-length 50 s period would make it 5.0 and roughly five times as long.
/// Starvation frequency is set by how hard the player is spending; the period
/// only sets how much it hurts.
pub const SUPPORT_MEAL_PERIOD: f64 = 10.0;

/// Stride between successive support monkeys' shift phases, as a share of the
/// period.
///
/// The golden ratio, because it is the stride that spreads N points around a
/// circle most evenly for *every* N without ever repeating. The first attempt
/// divided the period by the sprite budget, which gave three phases: monkeys 0,
/// 3 and 6 ate on exactly the same tick, so at the whitepaper's twenty-one-strong
/// end state the drain arrived as three lumps per ten seconds rather than as the
/// smooth 0.10/s it was supposed to buy - and the *economy* changed whenever the
/// crowd budget did.
pub const SUPPORT_PHASE_STRIDE: f64 = 0.618_033_988_749_895;

/// Pick multiplier added per research *level* - not per Technologist.
///
/// The Technologist is the one unit whose output is not a multiplier but a
/// currency. That indirection is the whole reason D14 refuses to rank it
/// against harvesters, and it is why a second Technologist is worth so much
/// less than the first: the ladder below grows 2.2x a level while a marginal
/// researcher only scales the rate by `(X+1)/X`.
pub const TECH_BONUS_PER_LEVEL: f64 = 0.10;
/// Research points one fed Technologist produces per second, before `M_speed`.
pub const RESEARCH_PER_TECHNOLOGIST: f64 = 1.0;
/// Research level `n` costs `60 x 2.2^n`.
pub const RESEARCH_LEVEL_BASE: f64 = 60.0;
pub const RESEARCH_LEVEL_GROWTH: f64 = 2.2;
/// Research level the Cart is gated behind. The Cart itself arrives in the next
/// increment; the shop already reads this to decide whether its row is locked.
pub const CART_TECH_REQUIREMENT: u32 = 1;

/// Travel multiplier added per fed Chef.
pub const CHEF_BONUS: f64 = 0.15;
/// Unload multiplier added per fed Unpacker.
pub const UNPACK_BONUS: f64 = 0.20;

/// The three monkeys who never touch a banana tree.
///
/// D5: a support unit declares which *term* of the harvest cycle it shortens,
/// not which entity it helps. That is why a cart bought later inherits the right
/// support sensitivity without anybody deciding it should.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SupportRole {
    Chef,
    Unpacker,
    Technologist,
}

impl SupportRole {
    /// Declaration order, which is also the order the shop lists them in.
    pub const ALL: [SupportRole; 3] = [
        SupportRole::Chef,
        SupportRole::Unpacker,
        SupportRole::Technologist,
    ];

    /// The order the larder is offered around when it cannot cover everyone.
    ///
    /// Deliberately *not* [`SupportRole::ALL`]. Feeding by declaration order
    /// spends the last banana on the least valuable monkey: measured, an
    /// Unpacker's marginal contribution is 3.6x a Chef's at the whitepaper's end
    /// state and 5.9x at twelve minutes, because by then the cart's unload
    /// segment dominates everything else. A Technologist eats last because its
    /// output is research, which nothing in flight depends on.
    ///
    /// Fixed rather than recomputed from marginal value: a fixed order is
    /// deterministic and cheap, and starvation is meant to be the rare edge of
    /// the economy rather than a state worth optimising inside.
    pub const FEEDING_ORDER: [SupportRole; 3] = [
        SupportRole::Unpacker,
        SupportRole::Chef,
        SupportRole::Technologist,
    ];

    /// Bananas per second (whitepaper §8).
    pub fn wage(self) -> f64 {
        match self {
            SupportRole::Chef | SupportRole::Unpacker => 0.10,
            SupportRole::Technologist => 0.20,
        }
    }

    /// Bananas eaten per shift. Derived from the wage so the wage is the truth.
    pub fn meal(self) -> f64 {
        self.wage() * SUPPORT_MEAL_PERIOD
    }

    pub fn cost_base(self) -> f64 {
        match self {
            SupportRole::Chef => 25.0,
            SupportRole::Unpacker => 30.0,
            SupportRole::Technologist => 40.0,
        }
    }

    pub fn cost_growth(self) -> f64 {
        match self {
            SupportRole::Chef | SupportRole::Unpacker => 1.30,
            SupportRole::Technologist => 1.35,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            SupportRole::Chef => "CHEF",
            SupportRole::Unpacker => "UNPACKER",
            SupportRole::Technologist => "TECHNOLOGIST",
        }
    }
}

/// No reachable economy comes near this; it exists for the same reason
/// [`MAX_WORKERS`] does - a tampered save otherwise spawns that many entities in
/// a single tick and is then re-persisted.
pub const MAX_SUPPORT: u32 = 1_000;

/// How many of each support role have been hired.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Staff {
    chefs: u32,
    unpackers: u32,
    technologists: u32,
}

impl Staff {
    pub fn from_saved(chefs: u32, unpackers: u32, technologists: u32) -> Option<Self> {
        (chefs <= MAX_SUPPORT && unpackers <= MAX_SUPPORT && technologists <= MAX_SUPPORT)
            .then_some(Self {
                chefs,
                unpackers,
                technologists,
            })
    }

    pub fn count(self, role: SupportRole) -> u32 {
        match role {
            SupportRole::Chef => self.chefs,
            SupportRole::Unpacker => self.unpackers,
            SupportRole::Technologist => self.technologists,
        }
    }

    pub fn total(self) -> u32 {
        SupportRole::ALL.iter().map(|role| self.count(*role)).sum()
    }

    /// Geometric per type, `cost = b·g^n`, exactly as [`Workforce::next_cost`]
    /// and the oracle compute it.
    pub fn next_cost(self, role: SupportRole) -> f64 {
        role.cost_base() * role.cost_growth().powi(self.count(role) as i32)
    }

    pub fn hire(&mut self, role: SupportRole) {
        let slot = match role {
            SupportRole::Chef => &mut self.chefs,
            SupportRole::Unpacker => &mut self.unpackers,
            SupportRole::Technologist => &mut self.technologists,
        };
        *slot = (*slot + 1).min(MAX_SUPPORT);
    }

    pub fn restart(&mut self) {
        *self = Self::default();
    }
}

/// How many of each role are currently fed, and therefore working.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FedStaff {
    pub chefs: u32,
    pub unpackers: u32,
    pub technologists: u32,
}

impl FedStaff {
    pub fn count(self, role: SupportRole) -> u32 {
        match role {
            SupportRole::Chef => self.chefs,
            SupportRole::Unpacker => self.unpackers,
            SupportRole::Technologist => self.technologists,
        }
    }

    pub fn add(&mut self, role: SupportRole) {
        match role {
            SupportRole::Chef => self.chefs += 1,
            SupportRole::Unpacker => self.unpackers += 1,
            SupportRole::Technologist => self.technologists += 1,
        }
    }

    pub fn total(self) -> u32 {
        self.chefs + self.unpackers + self.technologists
    }
}

/// One support monkey's shift clock: work for [`SUPPORT_MEAL_PERIOD`], then eat.
///
/// Per-entity rather than a single counter per role, because units starve
/// *independently* - the larder can cover two chefs and not the third - and a
/// resource-level accumulator that modelled that would be a hand-rolled ECS.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct SupportCycle {
    /// Seconds of shift left before the next meal falls due.
    remaining: f64,
    fed: bool,
}

impl SupportCycle {
    /// A newly hired monkey starts fed and works a full shift before its first
    /// meal falls due - the same post-paid deal a worker gets on its first trip.
    ///
    /// `phase` staggers the shift so that N monkeys of a role do not all eat on
    /// the same tick. Derived from the hire index by the caller, exactly like
    /// `worker::Lane`, so it is a deterministic offset and not D16's jitter.
    pub fn starting(phase: f64) -> Self {
        Self {
            remaining: (SUPPORT_MEAL_PERIOD - phase.rem_euclid(SUPPORT_MEAL_PERIOD))
                .clamp(0.0, SUPPORT_MEAL_PERIOD),
            fed: true,
        }
    }

    /// Unpaid, and therefore idle: it contributes nothing to its multiplier and
    /// draws nothing from the larder until it is fed.
    pub fn is_hungry(self) -> bool {
        !self.fed
    }

    /// Spend `dt` seconds of shift, eating when one falls due. Returns what it
    /// ate, which is either `meal` or nothing at all.
    ///
    /// A time budget with a carried remainder, for the reasons D13's
    /// implementation note gives: `remaining -= dt` two hundred times over
    /// accumulates a binary residual, and a shift that takes 201 ticks instead
    /// of 100 puts the realised wage a fraction of a percent under the published
    /// one. Work is measured in seconds here, so the rate is exactly 1.
    pub fn advance(&mut self, dt: f64, meal: f64, larder: &mut f64) -> f64 {
        debug_assert!(dt.is_finite() && dt >= 0.0);
        let mut budget = dt;
        let mut eaten = 0.0;
        let mut guard = 0;

        while budget > 0.0 && guard < 64 {
            guard += 1;
            let needed = self.remaining;
            const TOLERANCE: f64 = SUPPORT_MEAL_PERIOD * 1e-9;
            if needed > budget + TOLERANCE {
                self.remaining -= budget;
                break;
            }

            // Exactly, with no slack - the tolerance above is in seconds and
            // belongs to the budget, not to a quantity of bananas.
            if *larder < meal {
                // Hold the clock at zero rather than restarting it. Resetting
                // would hand an unpaid monkey a free shift, which is the "wage
                // holiday" D18 refuses to allow.
                self.remaining = 0.0;
                self.fed = false;
                break;
            }

            *larder -= meal;
            eaten += meal;
            self.fed = true;
            budget = (budget - needed).max(0.0);
            self.remaining = SUPPORT_MEAL_PERIOD;
        }

        debug_assert!(guard < 64, "support shift loop hit its iteration guard");
        eaten
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

/// Reset every piece of run state together, so they cannot drift apart. Every
/// new resource that survives a run belongs here and nowhere else.
pub fn restart_run(
    treasury: &mut Treasury,
    workforce: &mut Workforce,
    carts: &mut Carts,
    staff: &mut Staff,
    research: &mut Research,
) {
    treasury.restart();
    workforce.restart();
    carts.restart();
    staff.restart();
    research.restart();
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
    pub workers: u32,
    /// Support monkeys hired, across all three roles.
    pub staff: u32,
    /// Support monkeys who could not be fed, and are therefore contributing
    /// nothing to their multiplier.
    ///
    /// Harvesters no longer appear here and cannot: since their meal is
    /// reserved out of the delivery that funds it, nothing else can spend it.
    /// Hunger is now exactly the condition of a monkey who depends on somebody
    /// else's surplus, which is what support staff are.
    pub hungry: u32,
}

impl EconomySnapshot {
    /// Unfed support is excluded from *both* sides: an idle chef shortens
    /// nobody's trip, and it is not eating either, so counting it in the wage
    /// bill would be as wrong as counting its bonus in the rate.
    ///
    /// That symmetry is the fix for a readout that used to lie. The projection
    /// was once a pure function of head*count*, so a workforce that had stopped
    /// working still reported `+6.0/min` while the pile sat still, and no number
    /// on screen explained why.
    pub fn project(
        workers: u32,
        carts: Carts,
        staff: Staff,
        fed: FedStaff,
        multipliers: Multipliers,
    ) -> Self {
        // Crewed monkeys have left the route, so they neither harvest on foot
        // nor draw a worker's wage - the cart pays for them out of its own
        // delivery. A monkey waiting in a half-crewed cart is in neither pool:
        // it is not harvesting, and nothing is feeding it. That is the price of
        // boarding, and it is bounded by one worker cycle.
        let pool = workers.saturating_sub(carts.crewed()) as f64;
        let running = carts.running() as f64;

        let gross_per_sec = pool * worker_throughput(multipliers)
            + running * throughput(CycleSpec::CART, multipliers);
        let support_wages: f64 = SupportRole::ALL
            .iter()
            .map(|role| fed.count(*role) as f64 * role.wage())
            .sum();
        let wages_per_sec = pool * WORKER_WAGE + running * CART_WAGE + support_wages;
        Self {
            gross_per_sec,
            wages_per_sec,
            net_per_sec: gross_per_sec - wages_per_sec,
            workers,
            staff: staff.total(),
            hungry: staff.total().saturating_sub(fed.total()),
        }
    }
}

/// The bananas the player and the support staff may actually draw on: the
/// balance, less every meal a harvester has already reserved against a delivery
/// it has just made.
///
/// Reserving is what makes the economy solvent by construction rather than by
/// gate, but it only works if *everything* that can spend respects the
/// reservation. Support staff do so through the larder; the shop does so
/// through here. Without this the player could buy a chef with a cart's
/// 20.4-banana meal and freeze the cart holding a 100-banana payload - and at an
/// empty pool the treasury could never climb back to free it.
pub fn spendable(treasury: Treasury, committed: f64) -> f64 {
    (treasury.bananas() - committed).max(0.0)
}

/// Carts owned, and how many monkeys are aboard them.
///
/// D8 and D17 are both deleted here. A cart is crewed by exactly three monkeys
/// or it does not run: buying one takes three spare workers, and if the player
/// has fewer the price includes hiring the difference. There is no partial
/// staffing, no sampled crew fraction and no assignment policy - the question
/// "who crews this cart" is asked once, at purchase, and answered by boarding.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Carts {
    owned: u32,
    /// Monkeys aboard, across every cart. At most one cart is ever partly
    /// crewed, because they fill in purchase order.
    crewed: u32,
}

impl Carts {
    pub fn from_saved(owned: u32, crewed: u32) -> Option<Self> {
        // A crew that exceeds the berths it could sit in is a tampered save.
        (owned <= MAX_CARTS && crewed <= owned * CART_CREW).then_some(Self { owned, crewed })
    }

    pub fn owned(self) -> u32 {
        self.owned
    }

    /// Monkeys aboard, and therefore out of the walking pool.
    pub fn crewed(self) -> u32 {
        self.crewed
    }

    /// Carts with a full crew. Only these run - an empty box at the depot
    /// produces nothing and, crucially, is never advanced: a cycle with a crew
    /// of zero has a picking rate of zero, which would divide by nothing.
    pub fn running(self) -> u32 {
        self.crewed / CART_CREW
    }

    /// Berths still waiting to be filled on the cart currently boarding.
    pub fn berths_open(self) -> u32 {
        self.owned * CART_CREW - self.crewed
    }

    pub fn next_cost(self) -> f64 {
        CART_COST_BASE * CART_COST_GROWTH.powi(self.owned as i32)
    }

    pub fn buy(&mut self, boarding_now: u32) {
        self.owned = (self.owned + 1).min(MAX_CARTS);
        self.crewed = (self.crewed + boarding_now).min(self.owned * CART_CREW);
    }

    /// One more monkey has climbed aboard.
    pub fn board(&mut self) {
        self.crewed = (self.crewed + 1).min(self.owned * CART_CREW);
    }

    pub fn restart(&mut self) {
        *self = Self::default();
    }
}

/// Cumulative research, and the levels it has bought.
///
/// Only the points are stored. The level is *derived* from them, rather than
/// carried alongside, because two fields that must agree eventually will not:
/// a tampered save or a rebalanced `RESEARCH_LEVEL_GROWTH` would leave a level
/// its points do not justify, and nothing would notice.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq)]
pub struct Research {
    points: f64,
}

impl Research {
    pub fn from_saved(points: f64) -> Option<Self> {
        is_valid_banana_count(points).then_some(Self { points })
    }

    pub fn points(self) -> f64 {
        self.points
    }

    /// Levels completed. `60 x 2.2^n` per level, summed.
    pub fn level(self) -> u32 {
        let mut level = 0;
        let mut spent = 0.0;
        loop {
            let next = spent + Self::level_cost(level);
            if self.points < next || level >= MAX_RESEARCH_LEVEL {
                return level;
            }
            spent = next;
            level += 1;
        }
    }

    /// Points into the current level, and what it needs. The shop renders this
    /// as the Cart's unlock progress.
    #[allow(dead_code)]
    pub fn progress(self) -> (f64, f64) {
        let level = self.level();
        let spent: f64 = (0..level).map(Self::level_cost).sum();
        (self.points - spent, Self::level_cost(level))
    }

    pub fn level_cost(level: u32) -> f64 {
        RESEARCH_LEVEL_BASE * RESEARCH_LEVEL_GROWTH.powi(level as i32)
    }

    pub fn credit(&mut self, amount: f64) {
        debug_assert!(amount.is_finite() && amount >= 0.0);
        self.points = (self.points + amount).min(MAX_SAFE_BANANAS);
    }

    pub fn restart(&mut self) {
        self.points = 0.0;
    }
}

/// Far beyond anything reachable, and low enough that `2.2^n` stays finite -
/// it overflows f64 around n = 450.
pub const MAX_RESEARCH_LEVEL: u32 = 64;

/// Research points per second, which only *fed* technologists produce.
///
/// Scaled by `M_speed`: chefs feed the researchers too (whitepaper §8). It is
/// the one place a support unit boosts another support unit, and it is what
/// keeps Chefs relevant during the research phase, when the walking pool is at
/// its smallest.
pub fn research_per_sec(fed: FedStaff, multipliers: Multipliers) -> f64 {
    fed.technologists as f64 * RESEARCH_PER_TECHNOLOGIST * multipliers.speed
}

/// Bananas currently reserved by harvesters against meals they have earned but
/// not yet eaten. Summed from the world every tick, never accumulated (I3').
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq)]
pub struct Committed(pub f64);

/// Which purchasable unit an offer is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitKind {
    Worker,
    Support(SupportRole),
    Cart,
}

/// Everything the shop needs to render one row, and the single authority on
/// whether that purchase is legal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HirePlan {
    /// The signing fee, and the whole of what the player must have on hand.
    pub cost: f64,
    /// Bananas this unit will eat per meal, for the shop to explain itself
    /// with. Never a gate: it is paid out of a delivery, or out of surplus.
    pub meal: f64,
    /// Seconds between meals - a round trip for a harvester, a shift for
    /// support. The shop needs it to label `meal` honestly, since the two units
    /// eat on entirely different clocks.
    pub meal_period: f64,
    /// Bananas per minute the economy gains if this purchase goes through,
    /// **net of what the new unit itself eats**, evaluated against the world as
    /// it stands right now. D11's `projected_net_delta`, per minute.
    ///
    /// Net rather than gross, and the difference is the whole value of the
    /// column. A sixth Chef at nine workers raises production by 5.5/min and
    /// eats 6.0/min: gross it reads `+5.5` and looks like a good buy, net it
    /// reads `-0.5` and is one. Showing gross here would reproduce, in a live
    /// number, exactly the misleading-price failure D18 was written to remove.
    ///
    /// D11 projected rates for harvesters only, because a support unit's own
    /// banana delta is exactly `-wage` and carries no information. This is the
    /// other half of that expression: not what the unit produces, but what it
    /// makes everybody else produce, less its own keep.
    pub gain_per_min: f64,
    pub affordable: bool,
}

/// I5 (the player can pay), and nothing else.
///
/// I5 used to add a wage reserve on top of the fee, because wages drained
/// continuously and a fresh hire spent 50 seconds costing bananas before it
/// earned any. Post-paid meals removed that exposure, so the fee *is* the
/// requirement: `cost + reserve` was both a misleading price and a solution to a
/// problem the cycle no longer has.
///
/// **I1 is deliberately gone.** It required net to stay strictly positive after
/// every purchase, on the grounds that a purchase driving net to zero strands
/// the run. Two things retired it. Nothing strands any more - an unpayable
/// support monkey goes idle and stops eating, so the economy recovers by itself.
/// And once idleness feeds back into the multipliers, I1 stopped being
/// well-defined: at the whitepaper's end state net is -1.44/s with every monkey
/// fed and +0.59/s with the technologists idle, so the gate's answer depended on
/// a multiplier state it never named.
/// Everything a purchase decision reads, in one value.
///
/// Seven parameters that always travel together, and a caller that passed six
/// of them would be pricing against a world that does not exist. Bundling also
/// keeps the shop and the simulation honest with each other: both build one of
/// these from the same resources, so the price on the button and the price the
/// tick charges cannot drift.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EconomyState {
    pub workforce: Workforce,
    pub carts: Carts,
    pub staff: Staff,
    pub fed: FedStaff,
    pub research: Research,
    pub treasury: Treasury,
    /// Bananas harvesters have reserved against meals they have earned.
    pub committed: f64,
    pub multipliers: Multipliers,
}

pub fn plan_hire(kind: UnitKind, state: EconomyState) -> HirePlan {
    let EconomyState {
        workforce,
        carts,
        staff,
        research,
        treasury,
        committed,
        multipliers,
        // Live fed-ness is what the *snapshot* reports; an offer is priced
        // against a fed world. See below.
        fed: _,
    } = state;
    // Hypothetical steady state with everything fed: this is "what would this
    // purchase be worth", not "what is happening right now". A shop that priced
    // a chef against a currently-starving kitchen would quote a number that
    // buying the chef immediately falsifies - which is why `state.fed`, the
    // live count, is deliberately not read here.
    let all_fed = FedStaff {
        chefs: staff.count(SupportRole::Chef),
        unpackers: staff.count(SupportRole::Unpacker),
        technologists: staff.count(SupportRole::Technologist),
    };
    let before = EconomySnapshot::project(workforce.count(), carts, staff, all_fed, multipliers);

    let (cost, meal, meal_period, after) = match kind {
        UnitKind::Worker => (
            workforce.next_cost(),
            meal(CycleSpec::WORKER, multipliers),
            cycle_time(CycleSpec::WORKER, multipliers),
            EconomySnapshot::project(workforce.count() + 1, carts, staff, all_fed, multipliers),
        ),
        UnitKind::Cart => {
            let hires = cart_crew_shortfall(workforce, carts);
            let mut hired = workforce;
            for _ in 0..hires {
                hired.hire();
            }
            let mut bought = carts;
            // The monkeys bought *with* the cart board at once: they are already
            // standing at the stall, so there is nothing for them to walk back
            // from. That is what makes "pay more, start sooner" a real trade
            // rather than a pure penalty.
            bought.buy(hires);
            (
                cart_price(workforce, carts),
                meal(CycleSpec::CART, multipliers),
                cycle_time(CycleSpec::CART, multipliers),
                // Projected at full crew: this is what the purchase is worth
                // once it is running, not what the boarding gap costs. The
                // boarding wait is real and shown on the cart itself.
                EconomySnapshot::project(
                    hired.count(),
                    Carts {
                        owned: bought.owned(),
                        crewed: bought.owned() * CART_CREW,
                    },
                    staff,
                    all_fed,
                    multipliers,
                ),
            )
        }
        UnitKind::Support(role) => {
            let mut hired = staff;
            hired.hire(role);
            let mut fed_after = all_fed;
            fed_after.add(role);
            (
                staff.next_cost(role),
                role.meal(),
                SUPPORT_MEAL_PERIOD,
                EconomySnapshot::project(
                    workforce.count(),
                    carts,
                    hired,
                    fed_after,
                    // A support hire changes the multipliers, which is the
                    // whole point of buying one - so the projection has to be
                    // taken against the multipliers it would create, not the
                    // ones standing now. Research is unchanged by a hire: a new
                    // Technologist raises the research *rate*, and the level it
                    // eventually buys is a later event, not this purchase.
                    multipliers_for(fed_after, research),
                ),
            )
        }
    };

    // The one hard gate left in the economy. Everything else is "can you pay";
    // this is "does this exist yet".
    let unlocked = kind != UnitKind::Cart || research.level() >= CART_TECH_REQUIREMENT;
    HirePlan {
        cost,
        meal,
        meal_period,
        gain_per_min: (after.net_per_sec - before.net_per_sec) * 60.0,
        affordable: unlocked && spendable(treasury, committed) >= cost,
    }
}

/// How many workers the player is short of a full cart crew.
///
/// Spare means "not already promised to a berth", not "not yet aboard". Counting
/// only the monkeys who have physically boarded lets a second cart bought during
/// the first one's boarding window see the same three spare workers twice: six
/// berths, three monkeys, and a cart that can never launch - while the shop
/// quotes it a bare 70 and projects the gain of a running vehicle.
pub fn cart_crew_shortfall(workforce: Workforce, carts: Carts) -> u32 {
    let promised = carts.owned() * CART_CREW;
    let spare = workforce.count().saturating_sub(promised);
    CART_CREW.saturating_sub(spare.min(CART_CREW))
}

/// What a cart actually costs, including any crew the player does not have.
///
/// A cart cannot run under-crewed, so a shop that quoted 70 and then refused
/// the sale for want of monkeys would be the same lie D18 removed. Instead the
/// price absorbs the difference: the button says what the player will pay.
///
/// Worth knowing, and worth showing somewhere eventually: the bundled workers
/// go on the geometric ladder, so buying three at once raises the *next*
/// worker's price by 1.15³ ≈ 52%.
pub fn cart_price(workforce: Workforce, carts: Carts) -> f64 {
    let mut price = carts.next_cost();
    let mut ladder = workforce;
    for _ in 0..cart_crew_shortfall(workforce, carts) {
        price += ladder.next_cost();
        ladder.hire();
    }
    price
}

/// D4: every multiplier is additive within its term, `M = 1 + count × bonus`,
/// and only monkeys that are actually fed count towards it.
pub fn multipliers_for(fed: FedStaff, research: Research) -> Multipliers {
    Multipliers {
        speed: 1.0 + fed.chefs as f64 * CHEF_BONUS,
        unpack: 1.0 + fed.unpackers as f64 * UNPACK_BONUS,
        // Per level, not per head: a Technologist's output is research, and
        // research is what moves the multiplier.
        tech: 1.0 + research.level() as f64 * TECH_BONUS_PER_LEVEL,
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

    /// An economy with `workers` workers, `staff` support, and nothing owed.
    fn world(workers: u32, staff: Staff, bananas: f64, m: Multipliers) -> EconomyState {
        EconomyState {
            workforce: Workforce::from_saved(workers).unwrap(),
            carts: Carts::default(),
            staff,
            fed: FedStaff::default(),
            research: Research::default(),
            treasury: Treasury::from_saved(bananas).unwrap(),
            committed: 0.0,
            multipliers: m,
        }
    }

    fn worker_plan(workers: u32, bananas: f64, m: Multipliers) -> HirePlan {
        plan_hire(
            UnitKind::Worker,
            world(workers, Staff::default(), bananas, m),
        )
    }

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
                (Segment::Snack.duration(SPEC, m) / cycle_time(SPEC, m) - SNACK_FRACTION).abs()
                    < 1e-15,
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
            let projected = EconomySnapshot::project(
                workers,
                Carts::default(),
                Staff::default(),
                FedStaff::default(),
                m,
            )
            .gross_per_sec;
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
        assert_eq!(fresh.earmarked(), 0.0);
    }

    #[test]
    fn phase_constructor_uses_elapsed_time_segment_boundaries() {
        let m = base();

        assert_eq!(HarvestCycle::from_phase(0.0, SPEC, m), fresh());
        assert_eq!(
            HarvestCycle::from_phase(20.0, SPEC, m).segment(),
            Segment::Pick
        );
        assert_eq!(
            HarvestCycle::from_phase(25.0, SPEC, m).segment(),
            Segment::ToDepot
        );
        assert_eq!(
            HarvestCycle::from_phase(45.0, SPEC, m).segment(),
            Segment::Unload
        );
        assert_eq!(
            HarvestCycle::from_phase(47.5, SPEC, m).segment(),
            Segment::Snack
        );
        assert_eq!(HarvestCycle::from_phase(50.0, SPEC, m), fresh());

        let halfway_home = HarvestCycle::from_phase(35.0, SPEC, m);
        assert_eq!(halfway_home.segment(), Segment::ToDepot);
        assert_eq!(halfway_home.segment_fraction(SPEC, m), 0.5);
        assert!(halfway_home.segment().holds_banana());
        // Placed, not paid: a restored unit owes nothing on its first partial
        // cycle because it earned nothing on it.
        assert_eq!(halfway_home.earmarked(), 0.0);
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
    fn a_meal_is_reserved_at_delivery_so_nothing_else_can_spend_it() {
        // D18 argued a harvester is solvent because the credit precedes the
        // debit within its cycle. That only holds if nothing else can spend the
        // credit in the 2.5 s in between - and support staff, who draw on the
        // same larder and deliver nothing, can. Reserving the meal out of the
        // delivery closes the gap by construction.
        let m = base();
        let mut cycle = fresh();
        let mut larder = 0.0;

        // Run to the tick the delivery lands on.
        for _ in 0..950 {
            cycle.advance(SIM_DT, SPEC, m, terms(m), &mut larder);
        }
        assert_eq!(cycle.segment(), Segment::Snack);
        // Only the spendable share reached the larder; the meal is held back.
        assert!((larder - (WORKER_PAYLOAD - meal(SPEC, m))).abs() < 1e-12);
        assert!((cycle.earmarked() - meal(SPEC, m)).abs() < 1e-12);

        // Now have every other claimant on the economy take everything there
        // is. The reservation is not in the larder, so it cannot be taken.
        larder = 0.0;

        let mut ate = 0.0;
        for _ in 0..60 {
            ate += cycle.advance(SIM_DT, SPEC, m, terms(m), &mut larder).eaten;
        }

        assert!((ate - meal(SPEC, m)).abs() < 1e-12, "ate {ate}");
        assert_eq!(cycle.earmarked(), 0.0);
        // And it went straight back to work rather than stalling at the stall.
        assert_eq!(cycle.segment(), Segment::ToGrove);
    }

    #[test]
    fn a_reservation_never_exceeds_the_delivery_that_funds_it() {
        // The structural-solvency claim in one line: a unit can always afford
        // its own meal, at every multiplier and for every tier. If this ever
        // fails, the treasury can go negative and the cart can freeze holding a
        // full payload.
        for spec in [CycleSpec::WORKER, CycleSpec::CART] {
            for speed in [1.0, 1.15, 2.5, 10.0] {
                for unpack in [1.0, 2.0, 5.0] {
                    let m = Multipliers {
                        speed,
                        unpack,
                        ..Multipliers::default()
                    };
                    assert!(
                        meal(spec, m) < spec.payload,
                        "meal {} >= payload {} at {m:?}",
                        meal(spec, m),
                        spec.payload
                    );
                }
            }
        }
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
    fn restart_clears_every_piece_of_run_state_together() {
        let mut treasury = Treasury::from_saved(12.0).unwrap();
        let mut workforce = Workforce::from_saved(4).unwrap();

        let mut staff = Staff::from_saved(2, 3, 1).unwrap();
        let mut research = Research::from_saved(500.0).unwrap();
        let mut carts = Carts::from_saved(2, 6).unwrap();

        restart_run(
            &mut treasury,
            &mut workforce,
            &mut carts,
            &mut staff,
            &mut research,
        );

        assert_eq!(treasury, Treasury::default());
        assert_eq!(workforce, Workforce::default());
        assert_eq!(staff, Staff::default());
        assert_eq!(research, Research::default());
        assert_eq!(carts, Carts::default());
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

        let plan = worker_plan(0, 4.0, m);
        assert_eq!(plan.cost, 4.0);
        assert_eq!(plan.meal, 1.5);
        assert!(plan.affordable);

        assert!(!worker_plan(0, 3.9, m).affordable);
    }

    #[test]
    fn a_reserved_meal_is_not_spendable_in_the_shop() {
        // The other half of the reservation. Support staff respect it through
        // the larder; the shop has to respect it here, or the player can buy a
        // chef with a meal a monkey has already earned - and at an empty pool a
        // cart that missed its 37.9-banana meal could never be freed.
        let m = base();
        let plan = |committed| {
            plan_hire(
                UnitKind::Worker,
                EconomyState {
                    committed,
                    ..world(0, Staff::default(), 4.0, m)
                },
            )
        };

        assert!(plan(0.0).affordable);
        // 1.5 of those four bananas belong to a monkey that has just delivered.
        assert!(!plan(1.5).affordable);
        // The quoted price does not move - it is the *balance* that is
        // encumbered, not the fee. Quoting a higher price is the lie D18 came
        // from in the first place.
        assert_eq!(plan(1.5).cost, 4.0);
    }

    #[test]
    fn a_worker_is_worth_its_own_throughput_and_a_chef_is_worth_more_at_first() {
        // The GAIN column's contract. A worker adds exactly what it harvests; a
        // support monkey adds what it makes everybody *else* harvest, which is
        // zero on an empty field and grows with the workforce it multiplies.
        let m = base();

        // Net of its own meal: +6.0/min harvested, -1.8/min eaten.
        let worker = worker_plan(8, MAX_SAFE_BANANAS, m);
        assert!(
            (worker.gain_per_min - (worker_throughput(m) - WORKER_WAGE) * 60.0).abs() < 1e-9,
            "{}",
            worker.gain_per_min
        );
        assert!(
            (worker.gain_per_min - 4.2).abs() < 1e-9,
            "{}",
            worker.gain_per_min
        );

        let chef_of = |workers| {
            plan_hire(
                UnitKind::Support(SupportRole::Chef),
                world(workers, Staff::default(), MAX_SAFE_BANANAS, m),
            )
            .gain_per_min
        };

        // Nobody to feed, and it still eats: a Chef on an empty field is
        // legibly, numerically a bad buy.
        assert!((chef_of(0) + SupportRole::Chef.wage() * 60.0).abs() < 1e-9);
        assert!(chef_of(0) < 0.0);
        // And it scales with the pool it is multiplying, crossing into
        // profitable somewhere in between - which is the bottleneck moving.
        assert!(chef_of(8) > chef_of(4));
        assert!(chef_of(16) > 0.0);
        // A chef eats on a shift, not on a trip, and the shop has to say so.
        let chef = plan_hire(
            UnitKind::Support(SupportRole::Chef),
            world(8, Staff::default(), MAX_SAFE_BANANAS, m),
        );
        assert_eq!(chef.cost, 25.0);
        assert_eq!(chef.meal, 1.0);
        assert_eq!(chef.meal_period, SUPPORT_MEAL_PERIOD);
    }

    #[test]
    fn snapshot_reports_gross_wages_and_net_together() {
        let snapshot = EconomySnapshot::project(
            1,
            Carts::default(),
            Staff::default(),
            FedStaff::default(),
            base(),
        );

        assert!((snapshot.gross_per_sec - 0.1).abs() < 1e-15);
        assert!((snapshot.wages_per_sec - 0.03).abs() < 1e-15);
        assert!((snapshot.net_per_sec - 0.07).abs() < 1e-15);
        assert_eq!(
            EconomySnapshot::project(
                0,
                Carts::default(),
                Staff::default(),
                FedStaff::default(),
                base()
            ),
            EconomySnapshot::default()
        );

        // Idle support is excluded from both sides: it shortens nobody's trip
        // and it is not eating either. Counting its wage while dropping its
        // bonus would be exactly the asymmetric lie the readout used to tell.
        let staff = Staff::from_saved(2, 0, 0).unwrap();
        let starving =
            EconomySnapshot::project(4, Carts::default(), staff, FedStaff::default(), base());
        assert!((starving.wages_per_sec - 4.0 * WORKER_WAGE).abs() < 1e-15);
        assert_eq!(starving.hungry, 2);
        assert_eq!(starving.staff, 2);

        let fed = FedStaff {
            chefs: 2,
            ..FedStaff::default()
        };
        let paid = EconomySnapshot::project(4, Carts::default(), staff, fed, base());
        assert!((paid.wages_per_sec - (4.0 * WORKER_WAGE + 0.2)).abs() < 1e-15);
        assert_eq!(paid.hungry, 0);
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
        // Spending every banana the instant it arrives is a viable play, and it
        // must stay solvent - including once support staff are drawing on the
        // same larder without delivering anything into it.
        let mut treasury = Treasury::default();
        let mut workforce = Workforce::default();
        let mut staff = Staff::default();
        let mut cycles: Vec<HarvestCycle> = Vec::new();
        let mut shifts: Vec<(SupportRole, SupportCycle)> = Vec::new();
        let mut multipliers;
        let mut worst = f64::INFINITY;

        // Seed the run the way a player does: by hand, up to the first gate.
        treasury.credit(4.0);

        for tick in 0..(1_800.0 * SIM_HZ) as u32 {
            let committed: f64 = cycles.iter().map(|cycle| cycle.earmarked()).sum();
            let fed = FedStaff {
                chefs: shifts
                    .iter()
                    .filter(|(role, cycle)| *role == SupportRole::Chef && !cycle.is_hungry())
                    .count() as u32,
                unpackers: shifts
                    .iter()
                    .filter(|(role, cycle)| *role == SupportRole::Unpacker && !cycle.is_hungry())
                    .count() as u32,
                technologists: 0,
            };
            multipliers = multipliers_for(fed, Research::default());

            // Buy whatever is affordable, alternating so support actually gets
            // bought rather than being priced out by a cheaper worker forever.
            let kind = if tick % 3 == 0 {
                UnitKind::Support(if tick % 2 == 0 {
                    SupportRole::Chef
                } else {
                    SupportRole::Unpacker
                })
            } else {
                UnitKind::Worker
            };
            let plan = plan_hire(
                kind,
                EconomyState {
                    workforce,
                    carts: Carts::default(),
                    staff,
                    fed,
                    research: Research::default(),
                    treasury,
                    committed,
                    multipliers,
                },
            );
            if plan.affordable {
                treasury.charge(plan.cost);
                match kind {
                    // The solvency sweep is deliberately cart-free: a cart's
                    // meal is reserved out of its own 100-banana delivery by the
                    // same mechanism, and mixing them in would only re-test it.
                    UnitKind::Cart => unreachable!("this sweep buys no carts"),
                    UnitKind::Worker => {
                        workforce.hire();
                        cycles.push(HarvestCycle::starting(CycleSpec::WORKER));
                    }
                    UnitKind::Support(role) => {
                        staff.hire(role);
                        shifts.push((role, SupportCycle::starting(shifts.len() as f64 * 3.3)));
                    }
                }
            }

            let mut larder = treasury.bananas() - committed;
            let (mut delivered, mut eaten) = (0.0, 0.0);
            for cycle in &mut cycles {
                let output = cycle.advance(
                    SIM_DT,
                    CycleSpec::WORKER,
                    multipliers,
                    CycleTerms::new(CycleSpec::WORKER, multipliers),
                    &mut larder,
                );
                delivered += output.delivered;
                eaten += output.eaten;
            }
            // Support eats last, and only out of what is left unreserved.
            for role in SupportRole::FEEDING_ORDER {
                for (unit, shift) in &mut shifts {
                    if *unit == role {
                        eaten += shift.advance(SIM_DT, role.meal(), &mut larder);
                    }
                }
            }

            treasury.credit(delivered);
            treasury.charge(eaten);
            worst = worst.min(treasury.bananas());
        }

        assert!(worst >= 0.0, "treasury dipped to {worst}");
        assert!(workforce.count() >= 8, "only {} hired", workforce.count());
        assert!(staff.total() >= 1, "no support hired");
    }

    #[test]
    fn the_cart_cycle_matches_the_whitepaper() {
        let m = base();
        let cart = CycleSpec::CART;

        // Cart balance: 100 bananas, crew of 3, 15 m/s.
        assert!((Segment::ToGrove.duration(cart, m) * 2.0 - 13.333_333_333_333_334).abs() < 1e-9);
        assert!((Segment::Pick.duration(cart, m) - 33.333_333_333_333_336).abs() < 1e-9);
        assert!((Segment::Unload.duration(cart, m) - 50.0).abs() < 1e-9);
        assert!((work_time(cart, m) - 96.666_666_666_666_67).abs() < 1e-9);
        // The snack is a uniform inflation of all three, so the segment shares
        // §5 reports are untouched by it.
        assert!((cycle_time(cart, m) - 101.754_385_964_912_3).abs() < 1e-9);
        assert!((meal(cart, m) - 20.350_877_192_982_46).abs() < 1e-9);

        // A cart barely travels and instead sits at the depot being emptied,
        // which is the whole reason Chefs are a worker's buy and Unpackers a
        // cart's. Nobody wrote that rule; it follows from payload and speed.
        let share = |segment: Segment| segment.duration(cart, m) / work_time(cart, m);
        assert!(share(Segment::Unload) > 0.55);
        assert!(share(Segment::ToGrove) * 2.0 < 0.08);
    }

    #[test]
    fn chefs_are_for_walkers_and_unpackers_are_for_carts() {
        // §5 in two numbers. Ten of each, against each harvester.
        let m = base();
        let chefs = multipliers_for(
            FedStaff {
                chefs: 10,
                ..FedStaff::default()
            },
            Research::default(),
        );
        let unpackers = multipliers_for(
            FedStaff {
                unpackers: 10,
                ..FedStaff::default()
            },
            Research::default(),
        );

        let gain = |spec, after| throughput(spec, after) / throughput(spec, m) - 1.0;

        assert!(
            gain(CycleSpec::WORKER, chefs) > 1.0,
            "chefs should double a walker"
        );
        assert!(gain(CycleSpec::CART, chefs) < 0.10);
        assert!(gain(CycleSpec::CART, unpackers) > 0.50);
        assert!(gain(CycleSpec::WORKER, unpackers) < 0.10);
    }

    #[test]
    fn a_cart_converges_to_the_same_per_monkey_ceiling_as_a_walker() {
        // Whitepaper §3: with travel and unloading driven to zero, every harvest
        // method converges to the same per-monkey ceiling, and feeding costs a
        // flat 5% of it. D18 left this asymmetric - workers paid a snack and
        // carts did not - and giving the cart the same snack closes it.
        let m = Multipliers {
            speed: 1e6,
            unpack: 1e6,
            ..Multipliers::default()
        };
        let ceiling = m.tech / T_PICK;

        let walker = throughput(CycleSpec::WORKER, m);
        let per_crew = throughput(CycleSpec::CART, m) / CART_CREW as f64;

        assert!((walker / ceiling - (1.0 - SNACK_FRACTION)).abs() < 1e-4);
        assert!((per_crew / ceiling - (1.0 - SNACK_FRACTION)).abs() < 1e-4);
        assert!((walker - per_crew).abs() < 1e-4, "{walker} vs {per_crew}");
    }

    #[test]
    fn a_cart_costs_its_missing_crew_and_never_rewinds_the_worker_ladder() {
        let carts = Carts::default();

        // No spare monkeys: the price is the cart plus three hires off the
        // ladder. A shop that quoted 70 and then refused the sale for want of
        // monkeys would be the same lie D18 removed.
        let broke = Workforce::default();
        assert_eq!(cart_crew_shortfall(broke, carts), 3);
        let bundled = 70.0 + 4.0 + 4.0 * 1.15 + 4.0 * 1.15 * 1.15;
        assert!((cart_price(broke, carts) - bundled).abs() < 1e-12);

        // Three spare monkeys: just the cart.
        let staffed = Workforce::from_saved(3).unwrap();
        assert_eq!(cart_crew_shortfall(staffed, carts), 0);
        assert_eq!(cart_price(staffed, carts), 70.0);

        // Crewing does not give the worker ladder back. If it did, buying a
        // cart would make the next worker 1.15³ cheaper - a discount for
        // spending seventy bananas, which inverts the whole ladder.
        let mut owned = Workforce::from_saved(8).unwrap();
        let before = owned.next_cost();
        let mut crewed = Carts::default();
        crewed.buy(0);
        crewed.board();
        crewed.board();
        crewed.board();
        assert_eq!(crewed.crewed(), 3);
        assert_eq!(owned.next_cost(), before);
        // And a bundled purchase advances it, exactly as three separate hires
        // would have.
        owned.hire();
        assert!(owned.next_cost() > before);
    }

    #[test]
    fn a_second_cart_bought_during_boarding_still_pays_for_its_own_crew() {
        // `crewed()` is monkeys aboard; berths are monkeys *promised*. Counting
        // only the boarded ones let a cart bought inside the first one's
        // boarding window see the same three spare workers twice - six berths,
        // three monkeys, and a cart that could never launch, quoted at a bare
        // 70 and projected as if it were running.
        let workforce = Workforce::from_saved(3).unwrap();
        let mut carts = Carts::default();

        assert_eq!(cart_crew_shortfall(workforce, carts), 0);
        assert_eq!(cart_price(workforce, carts), 70.0);
        carts.buy(0);

        // Nobody has boarded yet, and the three spare monkeys are spoken for.
        assert_eq!(carts.crewed(), 0);
        assert_eq!(cart_crew_shortfall(workforce, carts), 3);
        let bundled = 70.0 * CART_COST_GROWTH
            + 4.0 * WORKER_COST_GROWTH.powi(3)
            + 4.0 * WORKER_COST_GROWTH.powi(4)
            + 4.0 * WORKER_COST_GROWTH.powi(5);
        assert!((cart_price(workforce, carts) - bundled).abs() < 1e-12);

        // And the berths stay accounted for once they actually fill.
        for _ in 0..CART_CREW {
            carts.board();
        }
        assert_eq!(carts.berths_open(), 0);
        assert_eq!(cart_crew_shortfall(workforce, carts), 3);
    }

    #[test]
    fn a_cart_is_locked_until_the_first_research_level() {
        let m = base();
        let rich = Treasury::from_saved(MAX_SAFE_BANANAS).unwrap();
        let plan = |research| {
            plan_hire(
                UnitKind::Cart,
                EconomyState {
                    workforce: Workforce::from_saved(8).unwrap(),
                    carts: Carts::default(),
                    staff: Staff::default(),
                    fed: FedStaff::default(),
                    research,
                    treasury: rich,
                    committed: 0.0,
                    multipliers: m,
                },
            )
        };

        assert!(!plan(Research::default()).affordable);
        assert!(!plan(Research::from_saved(59.0).unwrap()).affordable);
        assert!(plan(Research::from_saved(60.0).unwrap()).affordable);
        // The gate is the only hard lock left; the price is unaffected by it.
        assert_eq!(plan(Research::default()).cost, 70.0);
    }

    #[test]
    fn crewing_moves_a_monkey_from_the_pool_onto_a_cart() {
        let m = base();
        let staff = Staff::default();
        let fed = FedStaff::default();

        let pool_only = EconomySnapshot::project(9, Carts::default(), staff, fed, m);
        let mut crewed = Carts::default();
        crewed.buy(0);
        for _ in 0..CART_CREW {
            crewed.board();
        }
        let with_cart = EconomySnapshot::project(9, crewed, staff, fed, m);

        // Three monkeys left the route, so the pool's contribution drops by
        // three walkers' worth and the cart's is added in their place.
        let walker = worker_throughput(m);
        let expected = pool_only.gross_per_sec - 3.0 * walker + throughput(CycleSpec::CART, m);
        assert!((with_cart.gross_per_sec - expected).abs() < 1e-12);

        // A cart costs 0.20/s flat, and its crew stop drawing 0.03 each.
        let expected_wages = 6.0 * WORKER_WAGE + CART_WAGE;
        assert!((with_cart.wages_per_sec - expected_wages).abs() < 1e-12);
        // Which is a decisive win - the whole reason D17 called cart slots
        // strictly better than the pool.
        assert!(with_cart.net_per_sec > pool_only.net_per_sec);

        // A half-boarded cart produces nothing and its waiting crew is out of
        // the pool: boarding costs a trip, and that is its whole cost.
        let mut boarding = Carts::default();
        boarding.buy(0);
        boarding.board();
        let mid = EconomySnapshot::project(9, boarding, staff, fed, m);
        assert_eq!(boarding.running(), 0);
        assert!((mid.gross_per_sec - 8.0 * walker).abs() < 1e-12);
        assert!((mid.wages_per_sec - 8.0 * WORKER_WAGE).abs() < 1e-12);
    }

    #[test]
    fn the_research_ladder_is_geometric_and_the_level_is_derived_from_it() {
        // Level n costs 60 x 2.2^n, and the level is a function of cumulative
        // points rather than a second stored field that could disagree.
        assert_eq!(Research::level_cost(0), 60.0);
        assert!((Research::level_cost(1) - 132.0).abs() < 1e-12);
        assert!((Research::level_cost(2) - 290.4).abs() < 1e-12);

        assert_eq!(Research::from_saved(0.0).unwrap().level(), 0);
        assert_eq!(Research::from_saved(59.9).unwrap().level(), 0);
        assert_eq!(Research::from_saved(60.0).unwrap().level(), 1);
        assert_eq!(Research::from_saved(191.9).unwrap().level(), 1);
        assert_eq!(Research::from_saved(192.0).unwrap().level(), 2);

        // Progress into the current level is what the shop renders as the
        // Cart's unlock counter.
        let (into, needed) = Research::from_saved(90.0).unwrap().progress();
        assert!((into - 30.0).abs() < 1e-12);
        assert!((needed - 132.0).abs() < 1e-12);

        // An absurd save cannot spin the derivation forever. In practice the
        // ladder bounds itself long before the guard does - 60 x 2.2^41 already
        // exceeds the integer-safe banana ceiling - so the guard is the backstop
        // for a future rebalance, not the thing doing the work today.
        let absurd = Research::from_saved(MAX_SAFE_BANANAS).unwrap().level();
        assert!(absurd <= MAX_RESEARCH_LEVEL, "{absurd}");
        assert!(Research::level_cost(absurd).is_finite());
    }

    #[test]
    fn a_research_level_shortens_picking_and_the_cart_is_gated_on_the_first() {
        let fed = FedStaff::default();
        let base = multipliers_for(fed, Research::default());
        assert_eq!(base.tech, 1.0);

        let one = multipliers_for(fed, Research::from_saved(60.0).unwrap());
        assert!((one.tech - 1.1).abs() < 1e-12);
        // The gate the Cart's shop row reads.
        assert_eq!(
            Research::from_saved(60.0).unwrap().level(),
            CART_TECH_REQUIREMENT
        );
        assert!(Research::from_saved(59.0).unwrap().level() < CART_TECH_REQUIREMENT);

        // Picking is the term it shortens, and only that term.
        let spec = CycleSpec::WORKER;
        assert!(Segment::Pick.duration(spec, one) < Segment::Pick.duration(spec, base));
        assert_eq!(
            Segment::ToGrove.duration(spec, one),
            Segment::ToGrove.duration(spec, base)
        );
        assert_eq!(
            Segment::Unload.duration(spec, one),
            Segment::Unload.duration(spec, base)
        );
    }

    #[test]
    fn only_fed_technologists_research_and_chefs_make_them_faster() {
        // Whitepaper §8: research scales with M_speed - chefs feed the
        // researchers too. It is the one place a support unit boosts another,
        // and it is what keeps Chefs worth owning while the pool is smallest.
        let hired = FedStaff {
            technologists: 3,
            ..FedStaff::default()
        };
        let base = Multipliers::default();
        assert_eq!(research_per_sec(hired, base), 3.0);

        let with_chefs = multipliers_for(
            FedStaff {
                chefs: 2,
                technologists: 3,
                ..FedStaff::default()
            },
            Research::default(),
        );
        assert!((with_chefs.speed - 1.3).abs() < 1e-12);
        assert!((research_per_sec(hired, with_chefs) - 3.9).abs() < 1e-12);

        // A starving researcher researches nothing.
        assert_eq!(research_per_sec(FedStaff::default(), with_chefs), 0.0);
    }

    #[test]
    fn the_larder_is_the_treasury_less_what_is_already_reserved() {
        // The blocker this pins. The reservation only survives if *everything*
        // that draws on the treasury nets it out - including the next tick,
        // which re-derives the larder from a balance that now contains the meal
        // `settle` just credited. Get this wrong and support eats reserved
        // bananas ~98% of the time it matters.
        let m = base();
        let spec = CycleSpec::WORKER;
        let mut treasury = Treasury::default();
        let mut cycle = HarvestCycle::starting(spec);
        let mut chef = SupportCycle::starting(0.0);
        let mut worst = f64::INFINITY;
        let mut forgiven = 0.0;

        for _ in 0..(600.0 * SIM_HZ) as u32 {
            // Exactly what `advance_cycles` does.
            let committed = cycle.earmarked();
            let mut larder = (treasury.bananas() - committed).max(0.0);

            let out = cycle.advance(SIM_DT, spec, m, CycleTerms::new(spec, m), &mut larder);
            let eaten = chef.advance(SIM_DT, SupportRole::Chef.meal(), &mut larder);

            treasury.credit(out.delivered);
            // `settle` charges unconditionally; anything it cannot cover is a
            // wage silently forgiven, which is the failure mode.
            let owed = out.eaten + eaten;
            forgiven += (owed - treasury.bananas()).max(0.0);
            treasury.charge(owed.min(treasury.bananas()));
            worst = worst.min(treasury.bananas());
        }

        assert!(worst >= 0.0, "treasury dipped to {worst}");
        assert_eq!(forgiven, 0.0, "{forgiven} bananas of wages were forgiven");
    }

    #[test]
    fn support_eats_exactly_its_wage_per_second_over_a_long_run() {
        // The clock must not drift. `remaining -= dt` two hundred times leaves a
        // binary residual, and a shift that takes 201 ticks instead of 200 puts
        // the realised wage a fraction of a percent under the published one -
        // silently, and only visible as an economy that is slightly cheaper to
        // run than the shop claims.
        for role in SupportRole::ALL {
            let mut shift = SupportCycle::starting(0.0);
            let mut larder = 1e9;
            let seconds = 10_000.0;

            let mut eaten = 0.0;
            for _ in 0..(seconds * SIM_HZ) as u32 {
                eaten += shift.advance(SIM_DT, role.meal(), &mut larder);
            }

            let want = role.wage() * seconds;
            // Tight on purpose. The failure this names - one extra tick per
            // shift from float drift - costs about five bananas over this run,
            // and a tolerance of a whole meal would have let a drift two
            // hundred thousand times smaller than that through unnoticed.
            assert!(
                (eaten - want).abs() < role.meal() * 1e-9,
                "{role:?} ate {eaten}, expected {want}"
            );
        }
    }

    #[test]
    fn an_unfed_support_monkey_holds_its_debt_instead_of_getting_a_free_shift() {
        // Resetting the clock on a failed meal would hand an idle monkey a free
        // ten seconds, which is the "wage holiday" D18 refuses to allow: spend
        // to exactly zero and your staff work for nothing.
        let role = SupportRole::Chef;
        let mut shift = SupportCycle::starting(0.0);
        let mut larder = 0.0;

        // A full shift with an empty larder.
        for _ in 0..(SUPPORT_MEAL_PERIOD * SIM_HZ) as u32 {
            assert_eq!(shift.advance(SIM_DT, role.meal(), &mut larder), 0.0);
        }
        assert!(shift.is_hungry());

        // Another full shift changes nothing: the debt is still owed.
        for _ in 0..(SUPPORT_MEAL_PERIOD * SIM_HZ) as u32 {
            assert_eq!(shift.advance(SIM_DT, role.meal(), &mut larder), 0.0);
        }
        assert!(shift.is_hungry());

        // Feed it and it goes straight back to work, having paid in full.
        larder = role.meal();
        assert_eq!(shift.advance(SIM_DT, role.meal(), &mut larder), role.meal());
        assert_eq!(larder, 0.0);
        assert!(!shift.is_hungry());
    }

    #[test]
    fn the_larder_feeds_unpackers_before_chefs_before_technologists() {
        // Under scarcity a fixed type order spends the last banana on the least
        // valuable monkey unless the order is chosen. An Unpacker's marginal
        // contribution is measured at 3.6x a Chef's at the end state; research
        // is worth nothing to anything currently in flight, so it eats last.
        assert_eq!(
            SupportRole::FEEDING_ORDER,
            [
                SupportRole::Unpacker,
                SupportRole::Chef,
                SupportRole::Technologist
            ]
        );

        // One chef, one unpacker, and food for exactly one of them.
        let mut chef = SupportCycle::starting(0.0);
        let mut unpacker = SupportCycle::starting(0.0);
        let mut larder = SupportRole::Unpacker.meal();

        for _ in 0..(SUPPORT_MEAL_PERIOD * SIM_HZ) as u32 + 2 {
            for role in SupportRole::FEEDING_ORDER {
                match role {
                    SupportRole::Unpacker => {
                        unpacker.advance(SIM_DT, role.meal(), &mut larder);
                    }
                    SupportRole::Chef => {
                        chef.advance(SIM_DT, role.meal(), &mut larder);
                    }
                    SupportRole::Technologist => {}
                }
            }
        }

        assert!(!unpacker.is_hungry(), "the unpacker should have eaten");
        assert!(chef.is_hungry(), "the chef should have gone without");
    }
}
