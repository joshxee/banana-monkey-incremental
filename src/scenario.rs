//! Named starting states.
//!
//! A scenario is a run the game could have saved, plus a rule for where its
//! harvesters stand when it opens. The same registry seeds both halves of the
//! test harness: `headless::Headless` steps one through the simulation with no
//! window, and `./play --scenario NAME` opens it for a human with the full
//! presentation. A test and a playtest of the same feature therefore start
//! from the same world, and neither has to hand-harvest its way there.
//!
//! Each entry answers one question a playtester might ask. Add a scenario when
//! a new feature needs a state worth looking at, and give it a `summary` that
//! gives the clock and the pass criterion: that line is the whole of
//! `./play --scenarios`.

// The registry has no caller in a release build, by design: only the
// `test-hooks` launcher and the headless tests resolve a name.
#![cfg_attr(not(feature = "test-hooks"), allow(dead_code))]

use bevy::prelude::*;

use crate::{
    domain::{Carts, Research, Staff, Treasury, Workforce},
    persistence::{SaveMode, SavedRun},
    worker::{RestoreCarts, RestoreWorkers},
};

/// Where a scenario's harvesters are when it opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// Every harvester starts at the stall at phase zero, exactly as a fresh
    /// hire does, and earns from its first trip. This is the placement for
    /// tests that assert exact figures: tick 950 is a delivery, tick 1000 is
    /// a meal.
    AtStall,
    /// Every harvester is dropped somewhere on its route, as a save restore
    /// does, and produces nothing until it completes that partial trip. A
    /// seed makes the drop reproducible; `None` is the real save path.
    Restored { seed: Option<u64> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Scenario {
    pub name: &'static str,
    /// What a playtester should look for. Shown by `./play --scenarios`.
    pub summary: &'static str,
    pub run: SavedRun,
    pub placement: Placement,
}

impl Scenario {
    /// Put this scenario's run into the world, with saving switched off: the
    /// player never earned this state, and it must not overwrite the one they
    /// did.
    pub fn install(&self, app: &mut App) {
        install(app, self.run, self.placement);
        app.insert_resource(SaveMode::Off);
    }
}

/// Put a run into the world, ahead of `SimulationPlugin`.
///
/// The restore budget is the *pool*, not the workforce: crewed monkeys get no
/// avatar, so counting them would leave the budget unspent and the next
/// workers the player buys would spawn as restored ghosts - placed at a random
/// point on the route, no hire flash, and producing nothing for a full cycle.
pub fn install(app: &mut App, run: SavedRun, placement: Placement) {
    let pool = run.workforce.count().saturating_sub(run.carts.crewed());
    let (workers, carts) = match placement {
        Placement::AtStall => (RestoreWorkers::new(0), RestoreCarts::new(0)),
        Placement::Restored { seed: None } => (
            RestoreWorkers::new(pool),
            RestoreCarts::new(run.carts.running()),
        ),
        Placement::Restored { seed: Some(seed) } => (
            RestoreWorkers::with_seed(pool, seed),
            // A different stream from the workers', so a cart is not phased in
            // lockstep with whichever monkey drew the same number.
            RestoreCarts::with_seed(run.carts.running(), seed.rotate_left(32) ^ 0x9e37_79b9),
        ),
    };
    app.insert_resource(run.treasury)
        .insert_resource(run.workforce)
        .insert_resource(run.staff)
        .insert_resource(run.research)
        .insert_resource(run.carts)
        .insert_resource(workers)
        .insert_resource(carts);
}

/// Every scenario, in the order a run reaches them.
///
/// A summary gives the clock and the pass criterion, because that is what a
/// playtester needs: what to watch, when it happens, and what wrong looks
/// like. Restored monkeys' first arrival pays nothing, by design (see
/// `Placement::Restored`), so the summaries of restored scenarios say so.
pub fn all() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "fresh",
            summary: "a new player's first minute: nothing hired, nothing banked; \
                      four hand-harvests should light the WORKER button",
            run: Seed::default().run(),
            placement: Placement::AtStall,
        },
        Scenario {
            name: "one-worker",
            summary: "one monkey walks out of the stall; +5 lands at 47.5 s and -1.5 at \
                      50 s, flat in between, then it sets out again",
            run: Seed {
                bananas: 10.0,
                workers: 1,
                ..Seed::default()
            }
            .run(),
            placement: Placement::AtStall,
        },
        Scenario {
            name: "crew",
            summary: "eight monkeys spread along the route: three lanes, no two on the \
                      same pixel, nearer ones drawn in front; first arrivals pay nothing",
            run: Seed {
                bananas: 60.0,
                workers: 8,
                ..Seed::default()
            }
            .run(),
            placement: Placement::Restored { seed: Some(7) },
        },
        Scenario {
            name: "support",
            summary: "chef, unpacker and technologist at their stations; the cart row \
                      counts research at 1/s and unlocks at about 40 s; first arrivals pay nothing",
            run: Seed {
                bananas: 40.0,
                workers: 6,
                chefs: 2,
                unpackers: 1,
                technologists: 1,
                research: 20.0,
                ..Seed::default()
            }
            .run(),
            placement: Placement::Restored { seed: Some(11) },
        },
        Scenario {
            name: "starving",
            summary: "three chefs, no income: all three grey and pulse from 10 s, the \
                      banner reads HUNGRY 3/3, and the rates stay at zero",
            run: Seed {
                bananas: 0.5,
                chefs: 3,
                ..Seed::default()
            }
            .run(),
            placement: Placement::AtStall,
        },
        Scenario {
            name: "overspent",
            summary: "spent to zero with staff on the payroll: chefs flicker between fed \
                      and hungry as deliveries land, and FARMING dips while they are grey",
            run: Seed {
                bananas: 0.5,
                workers: 6,
                chefs: 3,
                unpackers: 1,
                ..Seed::default()
            }
            .run(),
            placement: Placement::Restored { seed: Some(19) },
        },
        Scenario {
            name: "first-cart",
            summary: "the headline moment: research is one tick from level 1, the CART row \
                      unlocks at once, and buying it takes three monkeys off the route",
            run: Seed {
                bananas: 70.0,
                workers: 12,
                technologists: 1,
                research: 59.0,
                ..Seed::default()
            }
            .run(),
            placement: Placement::Restored { seed: Some(23) },
        },
        Scenario {
            name: "cart-boarding",
            summary: "a cart with one rider waits at the depot; watch at speed 1, the wait \
                      is the feature: the next two monkeys to finish climb aboard",
            run: Seed {
                bananas: 30.0,
                workers: 3,
                technologists: 1,
                research: 100.0,
                carts: 1,
                crewed: 1,
                ..Seed::default()
            }
            .run(),
            placement: Placement::Restored { seed: Some(3) },
        },
        Scenario {
            name: "cart-running",
            summary: "a crewed cart sets out: the pile fills at the grove over 30 s, rides \
                      home full, and drains at the depot; +100 lands at about 94 s",
            run: Seed {
                bananas: 150.0,
                workers: 6,
                technologists: 1,
                research: 100.0,
                carts: 1,
                crewed: 3,
                ..Seed::default()
            }
            .run(),
            placement: Placement::AtStall,
        },
        Scenario {
            name: "rotation",
            summary: "two carts and no unpacker: the shop's rate column should rank UNPACKER \
                      first; buy through it and CHEF should take the lead",
            run: Seed {
                bananas: 30.0,
                workers: 12,
                technologists: 1,
                research: 150.0,
                carts: 2,
                crewed: 6,
                ..Seed::default()
            }
            .run(),
            placement: Placement::Restored { seed: Some(29) },
        },
        Scenario {
            name: "late-game",
            summary: "a crowded field: role badges over the stations, every row unlocked, \
                      cart deliveries landing as +100 lumps; first arrivals pay nothing",
            run: Seed {
                bananas: 800.0,
                workers: 30,
                chefs: 6,
                unpackers: 4,
                technologists: 1,
                research: 400.0,
                carts: 4,
                crewed: 12,
            }
            .run(),
            placement: Placement::Restored { seed: Some(42) },
        },
        Scenario {
            name: "first-drag",
            summary: "the opening frame, and nothing else: watch it at 390x844 and at \
                      844x390. A stranger should be able to point at the banana and at \
                      where it goes within five seconds, without touching anything",
            run: Seed::default().run(),
            placement: Placement::Restored { seed: Some(1) },
        },
        Scenario {
            name: "survey",
            summary: "for the camera: monkeys spread down the whole walk. Drag the \
                      grass - the metre under the cursor should stay under it, and the \
                      whole crowd should slide with the ground and not jitter against \
                      it; first arrivals pay nothing",
            run: Seed {
                bananas: 400.0,
                workers: 18,
                chefs: 3,
                unpackers: 2,
                technologists: 1,
                research: 120.0,
                carts: 1,
                crewed: 3,
            }
            .run(),
            placement: Placement::Restored { seed: Some(37) },
        },
        Scenario {
            name: "rich",
            summary: "every button affordable and nothing hired: for shop and UI work, and \
                      for browser tests that must not grind",
            run: Seed {
                bananas: 100_000.0,
                ..Seed::default()
            }
            .run(),
            placement: Placement::AtStall,
        },
    ]
}

pub fn named(name: &str) -> Option<Scenario> {
    all().into_iter().find(|scenario| scenario.name == name)
}

/// The fields of a save, spelled out. Only the registry above builds these,
/// and every value in it is inside the limits the save loader enforces.
#[derive(Default)]
struct Seed {
    bananas: f64,
    workers: u32,
    chefs: u32,
    unpackers: u32,
    technologists: u32,
    research: f64,
    carts: u32,
    crewed: u32,
}

impl Seed {
    fn run(self) -> SavedRun {
        assert!(
            self.crewed <= self.workers,
            "a scenario cannot crew more monkeys than it hires"
        );
        SavedRun {
            treasury: Treasury::from_saved(self.bananas).expect("scenario bananas are valid"),
            workforce: Workforce::from_saved(self.workers).expect("scenario workforce is valid"),
            staff: Staff::from_saved(self.chefs, self.unpackers, self.technologists)
                .expect("scenario staff are valid"),
            research: Research::from_saved(self.research).expect("scenario research is valid"),
            carts: Carts::from_saved(self.carts, self.crewed).expect("scenario carts are valid"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        CART_TECH_REQUIREMENT, EconomyState, FedStaff, SupportRole, UnitKind, multipliers_for,
        plan_hire,
    };

    #[test]
    fn every_scenario_has_a_unique_kebab_case_name_the_launcher_can_find() {
        let mut seen = std::collections::HashSet::new();
        for scenario in all() {
            assert!(
                scenario
                    .name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '-'),
                "{} is not kebab-case",
                scenario.name
            );
            assert!(
                seen.insert(scenario.name),
                "{} is listed twice",
                scenario.name
            );
            assert_eq!(named(scenario.name).as_ref(), Some(&scenario));
            assert!(!scenario.summary.is_empty());
        }
        assert_eq!(named("nope"), None);
    }

    #[test]
    fn every_scenario_that_owns_a_cart_has_unlocked_it() {
        // A cart in a run whose research has not reached the Cart is a state
        // the game cannot produce, and the shop would show a locked row over
        // a running vehicle.
        for scenario in all() {
            if scenario.run.carts.owned() > 0 {
                assert!(
                    scenario.run.research.level() >= CART_TECH_REQUIREMENT,
                    "{} owns a cart without the research for it",
                    scenario.name
                );
            }
        }
    }

    #[test]
    fn research_only_exists_where_a_technologist_could_have_produced_it() {
        // Research accrues from fed technologists and nothing else, so a
        // research figure with no technologist is a state the game cannot
        // reach, and a reviewer reading the RESEARCH tab would notice.
        for scenario in all() {
            if scenario.run.research.points() > 0.0 {
                assert!(
                    scenario.run.staff.count(SupportRole::Technologist) > 0,
                    "{} has research and no technologist",
                    scenario.name
                );
            }
        }
    }

    #[test]
    fn the_rotation_scenario_really_puts_the_unpacker_first() {
        // The summary makes a claim about the shop's rate column; hold it to it.
        let run = named("rotation").unwrap().run;
        let all_fed = FedStaff {
            chefs: run.staff.count(SupportRole::Chef),
            unpackers: run.staff.count(SupportRole::Unpacker),
            technologists: run.staff.count(SupportRole::Technologist),
        };
        let state = EconomyState {
            workforce: run.workforce,
            carts: run.carts,
            staff: run.staff,
            fed: all_fed,
            research: run.research,
            treasury: run.treasury,
            committed: 0.0,
            multipliers: multipliers_for(all_fed, run.research),
        };
        let gain = |kind| plan_hire(kind, state).gain_per_min;
        let unpacker = gain(UnitKind::Support(SupportRole::Unpacker));
        assert!(unpacker > gain(UnitKind::Support(SupportRole::Chef)));
        assert!(unpacker > gain(UnitKind::Worker));
        assert!(unpacker > gain(UnitKind::Support(SupportRole::Technologist)));
    }

    #[test]
    fn a_scenario_never_writes_the_players_save() {
        let mut app = App::new();
        named("rich").unwrap().install(&mut app);
        assert_eq!(*app.world().resource::<SaveMode>(), SaveMode::Off);
        assert_eq!(app.world().resource::<Treasury>().bananas(), 100_000.0);
    }
}
