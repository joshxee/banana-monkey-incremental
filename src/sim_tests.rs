//! Contract tests for the economy, run headless through [`Headless`].
//!
//! Each test names the decision in `docs/banana-architecture-v2.md` it holds
//! to. They replace the browser tests that used to wait out real harvest
//! cycles: a thousand ticks here is a millisecond, and the figures are exact,
//! so a test can say "tick 950" rather than "within fifteen seconds".

use bevy::prelude::*;

use crate::{
    domain::{
        CART_CREW, Carts, Committed, CycleSpec, EconomySnapshot, FedStaff, Multipliers, Research,
        Segment, Staff, SupportRole, Treasury, UnitKind, Workforce, cart_price, cycle_time, meal,
        spendable, work_time,
    },
    game::DeliveryKind,
    headless::Headless,
    persistence::SavedRun,
    scenario::{self, Placement},
    support::SupportUnit,
    worker::{Boarding, Cart, RestoredCycle, Worker},
};

/// A fresh hire walks 20 s, picks 5 s, walks 20 s, unloads 2.5 s: tick 950.
const DELIVERY_TICK: u32 = 950;
/// And eats 2.5 s after that: tick 1000, the end of its 50 s cycle.
const MEAL_TICK: u32 = 1000;
const PAYLOAD: f64 = 5.0;
const MEAL: f64 = 1.5;

fn run(bananas: f64, workers: u32) -> SavedRun {
    SavedRun {
        treasury: Treasury::from_saved(bananas).unwrap(),
        workforce: Workforce::from_saved(workers).unwrap(),
        ..SavedRun::default()
    }
}

// ─────────────────────────────────────────────────────────────── harness

#[test]
fn each_update_is_exactly_one_fixed_tick() {
    let mut sim = Headless::fresh();
    assert_eq!(sim.resource::<Time<Fixed>>().elapsed_secs_f64(), 0.0);
    sim.tick();
    assert_eq!(sim.resource::<Time<Fixed>>().elapsed_secs_f64(), 0.05);
    sim.ticks(19);
    assert_eq!(sim.resource::<Time<Fixed>>().elapsed_secs_f64(), 1.0);
    assert_eq!(sim.elapsed_ticks(), 20);
}

#[test]
fn every_registered_scenario_opens_and_runs() {
    for scenario in scenario::all() {
        let mut sim = Headless::from_scenario(&scenario);
        sim.ticks(200);
        let run = scenario.run;
        assert_eq!(
            sim.resource::<Workforce>(),
            run.workforce,
            "{}",
            scenario.name
        );
        // Every hired monkey is somewhere: on the route, or aboard a cart.
        let walking = sim.count::<Worker>() as u32;
        assert_eq!(
            walking + run.carts.crewed(),
            run.workforce.count(),
            "{}: pool + crew != workforce",
            scenario.name
        );
        assert_eq!(
            sim.count::<Cart>() as u32,
            run.carts.owned(),
            "{}",
            scenario.name
        );
        assert_eq!(
            sim.count::<SupportUnit>() as u32,
            run.staff.total(),
            "{}",
            scenario.name
        );
        assert!(sim.treasury() >= 0.0, "{}", scenario.name);
    }
}

#[test]
fn a_seeded_scenario_is_the_same_run_every_time() {
    let mut first = Headless::scenario("late-game");
    let mut second = Headless::scenario("late-game");
    first.ticks(3_000);
    second.ticks(3_000);

    assert_eq!(first.treasury().to_bits(), second.treasury().to_bits());
    assert_eq!(first.resource::<Research>(), second.resource::<Research>());
    assert_eq!(first.settled(), second.settled());
    let segments = |sim: &mut Headless| -> Vec<Segment> {
        sim.walking().iter().map(|cycle| cycle.segment()).collect()
    };
    assert_eq!(segments(&mut first), segments(&mut second));
}

// ──────────────────────────────────────────────────────── the worker (D18)

#[test]
fn a_fresh_hire_delivers_at_tick_950_and_eats_at_tick_1000() {
    let mut sim = Headless::scenario("one-worker");
    let start = sim.treasury();

    let delivered = sim.tick_until(2_000, |sim| sim.treasury() != start);
    assert_eq!(delivered, Some(DELIVERY_TICK));
    assert_eq!(sim.treasury(), start + PAYLOAD);
    let settled = sim.settled();
    assert_eq!(settled.len(), 1);
    assert_eq!(settled[0].delivery.kind, DeliveryKind::Worker);
    assert_eq!(settled[0].delivery.amount, PAYLOAD);

    let after_delivery = sim.treasury();
    let ate = sim.tick_until(2_000, |sim| sim.treasury() != after_delivery);
    assert_eq!(ate.map(|t| t + DELIVERY_TICK), Some(MEAL_TICK));
    assert_eq!(sim.treasury(), start + PAYLOAD - MEAL);
    let settled = sim.settled();
    assert_eq!(settled.len(), 1);
    assert_eq!(settled[0].delivery.kind, DeliveryKind::Snack);
    assert_eq!(settled[0].delivery.amount, MEAL);
}

#[test]
fn the_swarm_never_reaches_the_economy() {
    // D24 and D27. Every swarm offset - the fraction wobble that re-times where
    // a monkey is *drawn* along the walk, the corridor spread, the along-route
    // scatter, the standing ring - is presentation and only presentation. The
    // one construction `map` exists to prevent is a drawn path and a cycle time
    // that are two different journeys, and the way that would show up here is a
    // crowd delivering on a different tick from a monkey walking alone.
    //
    // The wobble is the dangerous one, because a sine bulge genuinely changes
    // how far along the route a monkey appears at a given moment. It is chosen
    // to vanish at both ends for exactly this reason;
    // `worker::tests::the_swarm_never_moves_an_arrival` pins the arithmetic and
    // this pins the consequence.
    let mut alone = Headless::scenario("one-worker");
    let start = alone.treasury();
    let solo = alone.tick_until(2_000, |sim| sim.treasury() != start);
    assert_eq!(solo, Some(DELIVERY_TICK));

    // Sixty monkeys, every one with a different wobble, spread and scatter,
    // all setting out together from the stall.
    let mut crowd = Headless::from_run(run(600.0, 60), Placement::AtStall);
    let start = crowd.treasury();
    let together = crowd.tick_until(2_000, |sim| sim.treasury() != start);
    assert_eq!(
        together, solo,
        "a crowd delivers on a different tick from a monkey walking alone"
    );
    // And on that tick every one of them arrives, because the swarm changed
    // where they are drawn and not how far any of them walked.
    assert_eq!(crowd.treasury(), start + PAYLOAD * 60.0);
}

#[test]
fn a_worker_visits_every_segment_in_order_and_only_carries_on_the_way_back() {
    let mut sim = Headless::scenario("one-worker");
    let mut visits: Vec<(Segment, u32)> = Vec::new();
    // The worker spawns on tick one and is advanced on it, so this samples the
    // segment *after* each of the cycle's thousand ticks.
    for _ in 0..MEAL_TICK {
        sim.tick();
        let segment = sim.walking()[0].segment();
        match visits.last_mut() {
            Some((last, count)) if *last == segment => *count += 1,
            _ => visits.push((segment, 1)),
        }
        // Empty on the way out and while picking; loaded from the pick to the
        // end of the snack, because the snack *is* the banana.
        let should_carry = matches!(segment, Segment::ToDepot | Segment::Unload | Segment::Snack);
        assert_eq!(segment.holds_banana(), should_carry, "{segment:?}");
    }

    // Travel 20 s each way, pick 5 s, unload 2.5 s, snack 2.5 s, at 20 Hz. A
    // segment flips on the tick that completes it, so the 400th tick of the
    // outbound walk already reads as picking, and the thousandth as the next
    // trip's first step.
    assert_eq!(
        visits,
        vec![
            (Segment::ToGrove, 399),
            (Segment::Pick, 100),
            (Segment::ToDepot, 400),
            (Segment::Unload, 50),
            (Segment::Snack, 50),
            (Segment::ToGrove, 1),
        ]
    );
}

#[test]
fn the_treasury_never_dips_below_where_the_hire_left_it() {
    // D18: the credit precedes the debit within a cycle and exceeds it, so a
    // trip costs nothing and the balance never falls below where it stood
    // before the delivery that funded the meal. From ten rather than zero,
    // because `Treasury::charge` clamps at zero and a floor of zero would
    // pass an overdraw.
    let mut sim = Headless::from_run(run(10.0, 4), Placement::AtStall);
    let mut low = f64::INFINITY;
    for _ in 0..(MEAL_TICK * 3) {
        sim.tick();
        low = low.min(sim.treasury());
    }
    assert_eq!(low, 10.0);
    // Four workers in lockstep deliver on one tick and eat on one tick, so
    // the floor is where the balance stood before the first delivery of the
    // cohort, and every snack in the cohort must leave it there or above.
    let mut floor = 10.0;
    let mut delivering = false;
    for entry in sim.settled() {
        match entry.delivery.kind {
            DeliveryKind::Worker => {
                if !delivering {
                    floor = entry.balance - entry.delivery.amount;
                    delivering = true;
                }
            }
            DeliveryKind::Snack => {
                delivering = false;
                assert!(entry.balance >= floor, "{entry:?} below {floor}");
            }
            _ => unreachable!("only workers in this economy"),
        }
    }
}

#[test]
fn the_readout_rate_is_the_rate_the_world_actually_produces() {
    // I3': `net_per_sec` is a projection from counts, and over whole cycles it
    // is what the treasury gains, to the banana.
    let mut sim = Headless::from_run(run(100.0, 4), Placement::AtStall);
    sim.tick();
    let snapshot = sim.resource::<EconomySnapshot>();
    assert_eq!(snapshot.gross_per_sec, 4.0 * PAYLOAD / 50.0);
    assert_eq!(snapshot.wages_per_sec, 4.0 * 0.03);
    assert!(snapshot.net_per_sec > 0.0);

    let start = sim.treasury();
    let cycles = 20;
    sim.ticks(MEAL_TICK * cycles);
    let realised = sim.treasury() - start;
    let projected = snapshot.net_per_sec * f64::from(cycles) * 50.0;
    // Every realised quantity is binary-exact; only `0.28 x 1000` rounds.
    assert_eq!(realised, 280.0);
    assert!(
        (realised - projected).abs() < 1e-9,
        "realised {realised} vs projected {projected}"
    );

    // And the ledger is a ledger: each balance is the one before it plus or
    // minus that settlement, and the last one is the treasury now.
    let settled = sim.settled();
    assert_eq!(
        settled.len(),
        4 * 2 * cycles as usize,
        "a delivery and a meal per trip"
    );
    let mut running = start;
    for entry in &settled {
        running = if entry.delivery.kind.is_income() {
            running + entry.delivery.amount
        } else {
            running - entry.delivery.amount
        };
        assert!(
            (entry.balance - running).abs() < 1e-9,
            "{entry:?} vs {running}"
        );
    }
    assert_eq!(settled.last().unwrap().balance, sim.treasury());
}

// ────────────────────────────────────────────────────────── the shop (D20)

#[test]
fn the_price_on_the_button_is_the_whole_requirement() {
    let mut sim = Headless::fresh();
    sim.harvest(3);
    sim.tick();
    assert_eq!(sim.treasury(), 3.0);
    sim.hire(UnitKind::Worker);
    sim.tick();
    assert_eq!(
        sim.resource::<Workforce>().count(),
        0,
        "three is short of four"
    );
    assert_eq!(sim.treasury(), 3.0);

    sim.harvest(1);
    sim.tick();
    sim.hire(UnitKind::Worker);
    sim.tick();
    assert_eq!(sim.resource::<Workforce>().count(), 1);
    // Exactly the quoted price, with nothing reserved on top.
    assert_eq!(sim.treasury(), 0.0);
    assert_eq!(sim.count::<Worker>(), 1);
    assert_eq!(sim.walking()[0].segment(), Segment::ToGrove);
}

#[test]
fn two_requests_between_ticks_buy_two_monkeys() {
    let mut sim = Headless::scenario("rich");
    sim.hire(UnitKind::Worker);
    sim.hire(UnitKind::Worker);
    sim.tick();
    assert_eq!(sim.resource::<Workforce>().count(), 2);
    assert_eq!(sim.count::<Worker>(), 2);
}

#[test]
fn a_banana_a_monkey_has_earned_is_not_yours_to_spend() {
    // I5' and D20: after a delivery the treasury holds the meal the worker
    // reserved out of it. The price does not move, but an encumbered balance
    // cannot cover it.
    let mut sim = Headless::from_run(run(0.5, 1), Placement::AtStall);
    sim.ticks(DELIVERY_TICK + 1);
    assert_eq!(sim.treasury(), 5.5);
    assert_eq!(sim.resource::<Committed>().0, MEAL);
    let price = sim.resource::<Workforce>().next_cost();
    assert!(price < sim.treasury(), "the balance covers the price");
    assert!(
        price > sim.treasury() - MEAL,
        "the spendable balance does not"
    );

    sim.hire(UnitKind::Worker);
    sim.tick();
    assert_eq!(sim.resource::<Workforce>().count(), 1, "refused");
    assert_eq!(sim.treasury(), 5.5);

    // Once the meal is eaten nothing is reserved, and the shop can sell.
    sim.harvest(1);
    sim.ticks(MEAL_TICK - DELIVERY_TICK);
    assert_eq!(sim.resource::<Committed>().0, 0.0);
    assert_eq!(sim.treasury(), 5.0);
    sim.hire(UnitKind::Worker);
    sim.tick();
    assert_eq!(sim.resource::<Workforce>().count(), 2);
}

// ────────────────────────────────────────────────────── support staff (D19)

#[test]
fn an_unfed_chef_drops_out_of_the_speed_multiplier_until_it_is_fed() {
    let mut sim = Headless::scenario("starving");
    sim.tick();
    assert_eq!(sim.resource::<FedStaff>().chefs, 3, "hired fed");
    assert!(sim.resource::<Multipliers>().speed > 1.0);

    // Three meals fall due at the end of the first 10 s shift - every first
    // monkey of a role shares phase zero - and half a banana feeds none. The
    // fed count is read one tick later, after this tick's meal.
    let starved = sim.tick_until(400, |sim| sim.resource::<FedStaff>().chefs == 0);
    assert_eq!(starved, Some(200));
    assert_eq!(sim.resource::<Multipliers>().speed, 1.0);
    assert_eq!(sim.treasury(), 0.5, "a hungry monkey draws nothing");

    // Food arrives; a hungry monkey retries every tick, so the next tick
    // feeds as many as the larder holds.
    sim.harvest(1);
    sim.tick();
    sim.tick();
    assert_eq!(sim.resource::<FedStaff>().chefs, 1);
    assert!(sim.resource::<Multipliers>().speed > 1.0);
    assert_eq!(sim.treasury(), 0.5);
}

#[test]
fn under_scarcity_the_unpacker_eats_before_the_chef_before_the_technologist() {
    // Three bananas: the unpacker's meal is 1, the chef's is 1 and the
    // technologist's is 2, so the correct order feeds the first two and
    // leaves 1.0, while unpacker-technologist-chef would feed the first and
    // the third and leave nothing. One banana could not tell those apart.
    let scarce = SavedRun {
        treasury: Treasury::from_saved(3.0).unwrap(),
        staff: Staff::from_saved(1, 1, 1).unwrap(),
        ..SavedRun::default()
    };
    let mut sim = Headless::from_run(scarce, Placement::AtStall);
    // Each role's first monkey has the same phase, so all three meals fall
    // due on the same tick.
    sim.seconds(10.0);
    sim.tick();
    let fed = sim.resource::<FedStaff>();
    assert_eq!(
        (fed.unpackers, fed.chefs, fed.technologists),
        (1, 1, 0),
        "{fed:?}"
    );
    assert_eq!(sim.treasury(), 1.0);
    let wages: Vec<f64> = sim
        .settled()
        .iter()
        .filter(|s| s.delivery.kind == DeliveryKind::Wage)
        .map(|s| s.delivery.amount)
        .collect();
    assert_eq!(
        wages,
        vec![SupportRole::Unpacker.meal(), SupportRole::Chef.meal()]
    );
}

#[test]
fn a_technologist_accrues_research_and_unlocks_the_cart() {
    let mut sim = Headless::scenario("rich");
    sim.hire(UnitKind::Cart);
    sim.tick();
    assert_eq!(sim.resource::<Carts>().owned(), 0, "locked");

    sim.hire(UnitKind::Support(SupportRole::Technologist));
    sim.tick();
    // A new technologist starts fed and is spawned before the multipliers are
    // read, so research is credited on the hire tick itself: 1.0/s x 50 ms.
    assert_eq!(sim.resource::<Research>().points(), 0.05);
    // Sixty seconds at 1/s is 1200 credits of 0.05 - which sum in binary to
    // 59.99999999999873, one tick short of 60. The level lands on tick 1200
    // after the hire rather than 1199: D13's residual, in research, and a
    // 50 ms delay on an unlock is the whole cost of not adding a tolerance.
    let unlocked = sim.tick_until(2_000, |sim| sim.resource::<Research>().level() >= 1);
    assert_eq!(unlocked, Some(1200));

    sim.hire(UnitKind::Cart);
    sim.tick();
    assert_eq!(sim.resource::<Carts>().owned(), 1);
}

// ──────────────────────────────────────────────────────────── the cart (D23)

#[test]
fn a_cart_takes_the_next_workers_to_finish_their_trip_and_then_runs() {
    // Three spare workers on the route, so the cart costs its base price and
    // hires nobody. It sits at the depot boarding until the three arrive.
    let unlocked = SavedRun {
        treasury: Treasury::from_saved(200.0).unwrap(),
        workforce: Workforce::from_saved(3).unwrap(),
        research: Research::from_saved(100.0).unwrap(),
        ..SavedRun::default()
    };
    let mut sim = Headless::from_run(unlocked, Placement::AtStall);
    sim.tick();
    sim.hire(UnitKind::Cart);
    sim.tick();
    let carts = sim.resource::<Carts>();
    assert_eq!(carts.owned(), 1);
    assert_eq!(carts.crewed(), 0);
    assert_eq!(sim.treasury(), 200.0 - 70.0);
    assert_eq!(sim.resource::<Workforce>().count(), 3, "nobody hired");
    assert_eq!(sim.count::<Boarding>(), 1, "waiting at the depot");
    assert_eq!(sim.count::<Worker>(), 3, "still walking");

    // They board at the snack, the one point they stand still owing a meal
    // they have reserved - which they settle on the way aboard. At M_tech 1.1
    // the pick is 91 ticks, so unload ends on tick 941 and the three board on
    // tick 942 - two of which were stepped before this count started.
    let boarded = sim.tick_until(2_000, |sim| sim.count::<Boarding>() == 0);
    assert_eq!(boarded, Some(940));
    let carts = sim.resource::<Carts>();
    assert_eq!(carts.crewed(), CART_CREW);
    assert_eq!(carts.running(), 1);
    assert_eq!(sim.count::<Worker>(), 0, "the pool is empty");
    let snacks: Vec<f64> = sim
        .settled()
        .iter()
        .filter(|s| s.delivery.kind == DeliveryKind::Snack)
        .map(|s| s.delivery.amount)
        .collect();
    // Not the bare 1.5: this run has a research level, which shortens the
    // pick and with it the trip the meal is a fixed share of.
    let multipliers = sim.resource::<Multipliers>();
    let worker_meal = meal(CycleSpec::WORKER, multipliers);
    assert!(worker_meal < MEAL);
    assert_eq!(snacks, vec![worker_meal; 3], "three meals paid on boarding");
    assert_eq!(sim.resource::<Committed>().0, 0.0);

    // One cart cycle later, a hundred bananas land at once and the cart eats
    // its share out of them. The delivery is the tick that completes the
    // unload, and the cart was already advanced on the launch tick that
    // `boarded` counted, hence the one less.
    let spec = CycleSpec::CART;
    let work_ticks = (work_time(spec, multipliers) * 20.0).ceil() as u32;
    let cycle_ticks = (cycle_time(spec, multipliers) * 20.0).ceil() as u32;
    let landed = sim.tick_until(cycle_ticks, |sim| {
        sim.settled()
            .iter()
            .any(|s| s.delivery.kind == DeliveryKind::Cart && s.delivery.amount == 100.0)
    });
    assert_eq!(landed, Some(work_ticks - 1));
    let ate = sim.tick_until(cycle_ticks, |sim| {
        sim.settled().iter().any(|s| {
            s.delivery.kind == DeliveryKind::Snack && s.delivery.amount == meal(spec, multipliers)
        })
    });
    assert_eq!(ate, Some(cycle_ticks - work_ticks));
}

#[test]
fn buying_a_cart_short_of_crew_hires_the_difference_and_boards_them_at_once() {
    let short = SavedRun {
        treasury: Treasury::from_saved(1_000.0).unwrap(),
        workforce: Workforce::from_saved(1).unwrap(),
        research: Research::from_saved(100.0).unwrap(),
        ..SavedRun::default()
    };
    let mut sim = Headless::from_run(short, Placement::AtStall);
    sim.tick();
    let before = sim.treasury();
    let quoted = cart_price(sim.resource::<Workforce>(), sim.resource::<Carts>());
    sim.hire(UnitKind::Cart);
    sim.tick();

    let carts = sim.resource::<Carts>();
    assert_eq!(
        sim.resource::<Workforce>().count(),
        3,
        "two hired with the cart"
    );
    assert_eq!(carts.crewed(), 2, "and both aboard already");
    assert_eq!(sim.count::<Worker>(), 1, "the original keeps walking");
    // 70 for the cart, plus the next two rungs of the worker ladder: one
    // monkey is already on the payroll, so the crew costs rungs two and three.
    assert!((quoted - (70.0 + 4.0 * 1.15 + 4.0 * 1.15 * 1.15)).abs() < 1e-9);
    assert_eq!(before - sim.treasury(), quoted, "the quoted price, exactly");
    assert_eq!(sim.count::<Boarding>(), 1);

    // The original boards when its own trip ends: tick 942, less the two
    // stepped so far, as in the test above.
    let launched = sim.tick_until(2_000, |sim| sim.count::<Boarding>() == 0);
    assert_eq!(launched, Some(940));
    assert_eq!(sim.count::<Worker>(), 0);
}

#[test]
fn a_cart_at_an_empty_pool_reserves_its_meal_and_the_shop_respects_it() {
    // D20's motivating case: the cart is the only harvester, its meal is
    // reserved out of the hundred it just delivered, and the encumbered
    // balance cannot cover a purchase the raw balance could. Five hungry
    // chefs, so that a price sits between the two figures.
    let alone = SavedRun {
        treasury: Treasury::from_saved(0.0).unwrap(),
        workforce: Workforce::from_saved(3).unwrap(),
        staff: Staff::from_saved(5, 0, 1).unwrap(),
        research: Research::from_saved(100.0).unwrap(),
        carts: Carts::from_saved(1, 3).unwrap(),
    };
    let mut sim = Headless::from_run(alone, Placement::AtStall);
    let landed = sim.tick_until(3_000, |sim| {
        sim.settled()
            .iter()
            .any(|s| s.delivery.kind == DeliveryKind::Cart)
    });
    assert!(landed.is_some());
    assert_eq!(sim.count::<Worker>(), 0, "everyone is aboard");
    // The meal is priced at the multipliers in force when it was reserved:
    // the technologist's level, and no chef fed.
    let cart_meal = meal(CycleSpec::CART, sim.resource::<Multipliers>());
    assert_eq!(sim.resource::<Committed>().0, cart_meal);

    let treasury = sim.resource::<Treasury>();
    let price = sim.resource::<Staff>().next_cost(SupportRole::Chef);
    let free = spendable(treasury, cart_meal);
    assert!(
        free < price && price < treasury.bananas(),
        "{free} < {price} < {}",
        treasury.bananas()
    );
    sim.hire(UnitKind::Support(SupportRole::Chef));
    sim.tick();
    assert_eq!(
        sim.resource::<Staff>().count(SupportRole::Chef),
        5,
        "refused"
    );

    // Then the cart eats exactly what it reserved, and the reservation clears.
    let ate = sim.tick_until(3_000, |sim| {
        sim.settled()
            .iter()
            .any(|s| s.delivery.kind == DeliveryKind::Snack && s.delivery.amount == cart_meal)
    });
    assert!(ate.is_some());
    assert_eq!(sim.resource::<Committed>().0, 0.0);
}

#[test]
fn a_meal_is_priced_against_the_delivery_that_funds_it() {
    // D18: the snack is the figure locked in at delivery. A chef hired between
    // the delivery and the snack shortens the *next* trip's meal, not this one.
    let mut sim = Headless::from_run(run(30.0, 1), Placement::AtStall);
    sim.ticks(DELIVERY_TICK + 1);
    assert_eq!(sim.resource::<Committed>().0, MEAL);
    sim.hire(UnitKind::Support(SupportRole::Chef));
    sim.tick();
    assert_eq!(sim.resource::<Staff>().count(SupportRole::Chef), 1);
    let shortened = meal(CycleSpec::WORKER, sim.resource::<Multipliers>());
    assert!(shortened < MEAL);

    let snack = sim.tick_until(200, |sim| {
        sim.settled()
            .iter()
            .any(|s| s.delivery.kind == DeliveryKind::Snack)
    });
    assert!(snack.is_some());
    let _ = sim.settled();
    // And the trip after it is priced at the new multipliers.
    sim.tick_until(2_000, |sim| {
        sim.settled()
            .iter()
            .any(|s| s.delivery.kind == DeliveryKind::Snack && s.delivery.amount == shortened)
    })
    .expect("the next meal is the shortened one");
}

#[test]
fn a_chef_hired_mid_walk_shortens_the_rest_of_the_trip() {
    // D13: progress is remaining work, so a multiplier bought mid-segment
    // speeds up the rest of it without teleporting anyone. Ten seconds out at
    // 3 m/s leaves 30 m; at 3.45 m/s that is 173.9 ticks, so the walk ends on
    // tick 374 rather than 400 - the chef spawns and is counted on the very
    // tick the purchase lands.
    let mut sim = Headless::from_run(run(100.0, 1), Placement::AtStall);
    sim.ticks(200);
    assert_eq!(sim.walking()[0].segment(), Segment::ToGrove);
    sim.hire(UnitKind::Support(SupportRole::Chef));
    let picking = sim.tick_until(400, |sim| sim.walking()[0].segment() == Segment::Pick);
    assert_eq!(picking, Some(174));
}

#[test]
fn the_rate_is_a_pure_function_of_counts_not_of_phase() {
    // I3': two worlds with the same counts report the same rate, wherever
    // their monkeys happen to be standing.
    let at_stall = Headless::from_run(run(100.0, 4), Placement::AtStall);
    let restored = Headless::from_run(run(100.0, 4), Placement::Restored { seed: Some(9) });
    for mut sim in [at_stall, restored] {
        sim.tick();
        assert_eq!(
            sim.resource::<EconomySnapshot>(),
            EconomySnapshot::project(
                4,
                Carts::default(),
                Staff::default(),
                FedStaff::default(),
                Multipliers::default()
            )
        );
    }
}

#[test]
fn a_restored_cart_produces_nothing_until_it_sets_out_afresh() {
    // The cart has the same placement guard as a worker and a far bigger
    // reason for it: a cart restored onto its return leg is drawn carrying a
    // hundred bananas it never picked.
    let crewed = SavedRun {
        treasury: Treasury::from_saved(0.0).unwrap(),
        workforce: Workforce::from_saved(3).unwrap(),
        staff: Staff::from_saved(0, 0, 1).unwrap(),
        research: Research::from_saved(100.0).unwrap(),
        carts: Carts::from_saved(1, 3).unwrap(),
    };
    let mut guarded_past_spawn = 0;
    for seed in 0..8 {
        let mut sim = Headless::from_run(crewed, Placement::Restored { seed: Some(seed) });
        let released = sim
            .tick_until(3_000, |sim| {
                sim.count::<Cart>() == 1 && sim.count::<RestoredCycle>() == 0
            })
            .expect("the marker comes off within a cycle");
        if released > 1 {
            guarded_past_spawn += 1;
        }
        let income: Vec<_> = sim
            .settled()
            .into_iter()
            .filter(|s| s.delivery.kind.is_income())
            .collect();
        assert!(income.is_empty(), "seed {seed}: {income:?}");

        let first = sim
            .tick_until(3_000, |sim| {
                sim.settled()
                    .iter()
                    .any(|s| s.delivery.kind == DeliveryKind::Cart && s.delivery.amount == 100.0)
            })
            .expect("a real trip after that");
        let multipliers = sim.resource::<Multipliers>();
        assert!(first <= (work_time(CycleSpec::CART, multipliers) * 20.0).ceil() as u32);
    }
    assert!(
        guarded_past_spawn > 0,
        "every seed landed on the grove road"
    );
}

// ─────────────────────────────────────────────────── restore and restart

#[test]
fn a_restored_worker_produces_nothing_until_it_sets_out_afresh() {
    // The save does not keep cycle phase; a restored monkey is dropped
    // somewhere on the route, and that placement must not create income: a
    // monkey restored onto the return leg is drawn carrying a banana it never
    // picked. The guard lifts the moment it next sets out for the grove.
    let mut guarded_past_spawn = 0;
    for seed in 0..8 {
        let mut sim = Headless::from_run(run(0.0, 1), Placement::Restored { seed: Some(seed) });
        let released = sim
            .tick_until(MEAL_TICK, |sim| {
                sim.count::<Worker>() == 1 && sim.count::<RestoredCycle>() == 0
            })
            .expect("the marker comes off within a cycle");
        if released > 1 {
            guarded_past_spawn += 1;
        }
        assert!(
            sim.settled().is_empty(),
            "seed {seed}: settled while restored"
        );
        assert_eq!(sim.treasury(), 0.0, "seed {seed}");

        // From there it is an ordinary worker: a full trip, a full payload.
        let first = sim
            .tick_until(MEAL_TICK, |sim| sim.treasury() != 0.0)
            .expect("a real trip after that");
        assert!(first <= DELIVERY_TICK, "seed {seed}: {first}");
        assert_eq!(sim.treasury(), PAYLOAD, "seed {seed}");
    }
    // The seeds have to have exercised the guard, not just the release.
    assert!(
        guarded_past_spawn > 0,
        "every seed landed on the grove road"
    );
}

#[test]
fn restart_returns_the_world_to_nothing_mid_flight() {
    let mut sim = Headless::scenario("late-game");
    sim.ticks(500);
    assert!(sim.count::<Worker>() > 0);
    sim.harvest(1);
    sim.restart();
    sim.tick();

    assert_eq!(sim.treasury(), 0.0, "the queued harvest went with it");
    assert_eq!(sim.resource::<Workforce>().count(), 0);
    assert_eq!(sim.resource::<Carts>().owned(), 0);
    assert_eq!(sim.resource::<Staff>().total(), 0);
    assert_eq!(sim.resource::<Research>().points(), 0.0);
    assert_eq!(sim.count::<Worker>(), 0);
    assert_eq!(sim.count::<Cart>(), 0);
    assert_eq!(sim.count::<SupportUnit>(), 0);
    assert_eq!(sim.resource::<Committed>().0, 0.0);
    assert_eq!(sim.resource::<EconomySnapshot>().wages_per_sec, 0.0);
    let _ = sim.settled();

    // And the next hire is a fresh one at the stall, not a restored ghost.
    sim.harvest(4);
    sim.tick();
    sim.hire(UnitKind::Worker);
    sim.tick();
    assert_eq!(sim.count::<Worker>(), 1);
    assert_eq!(sim.count::<RestoredCycle>(), 0);
    assert_eq!(sim.walking()[0].segment(), Segment::ToGrove);
}
