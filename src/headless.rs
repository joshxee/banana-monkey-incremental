//! The simulation with no window, stepped one tick at a time.
//!
//! Bevy's own guidance for testing an app is to swap `DefaultPlugins` for
//! `MinimalPlugins`, drive `App::update` by hand, and assert on the world. This
//! is that, with the two pieces the economy needs on top:
//!
//! - `TimeUpdateStrategy::FixedTimesteps(1)`, so every `update` runs exactly
//!   one 20 Hz tick. A worker's trip is then tick 950 for the delivery and
//!   tick 1000 for the meal, and a test can say so.
//! - A recorder for the [`Settled`] messages the simulation writes, so a test
//!   reads what the economy did rather than watching the treasury for a change.
//!
//! Inputs go in the way the presentation puts them in: a hire is a push onto
//! `HireRequests`, a hand-harvest is a `Manual` delivery on the queue. Nothing
//! here fakes a pointer, because nothing in the simulation reads one.
//!
//! A full worker cycle is a thousand ticks and runs in well under a
//! millisecond, so a test can afford to watch several.

use bevy::{
    app::FixedMain,
    ecs::schedule::{ScheduleLabel, SingleThreadedExecutor},
    prelude::*,
    time::TimeUpdateStrategy,
};

use crate::{
    domain::{BANANAS_PER_HARVEST, HarvestCycle, SIM_HZ, UnitKind},
    game::{
        Delivery, DeliveryKind, DeliveryQueue, HireRequests, RestartRequest, Settled, Sim,
        SimulationPlugin,
    },
    persistence::SavedRun,
    scenario::{self, Placement, Scenario},
    worker::Worker,
};

pub(crate) struct Headless {
    app: App,
    ticks: u32,
}

#[derive(Resource, Default)]
struct Recorder(Vec<Settled>);

fn record(mut settled: MessageReader<Settled>, mut recorder: ResMut<Recorder>) {
    recorder.0.extend(settled.read().copied());
}

impl Headless {
    pub(crate) fn scenario(name: &str) -> Self {
        let scenario = scenario::named(name).unwrap_or_else(|| panic!("no scenario `{name}`"));
        Self::from_scenario(&scenario)
    }

    pub(crate) fn from_scenario(scenario: &Scenario) -> Self {
        Self::from_run(scenario.run, scenario.placement)
    }

    pub(crate) fn from_run(run: SavedRun, placement: Placement) -> Self {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::FixedTimesteps(1));
        // The multi-threaded executor costs a few hundred microseconds of
        // thread hand-off per schedule per update, which across a dozen
        // schedules made a 20,000-tick test take eight seconds. The economy
        // is a handful of systems; run them inline.
        for label in [
            Main.intern(),
            First.intern(),
            PreUpdate.intern(),
            RunFixedMainLoop.intern(),
            FixedMain.intern(),
            FixedFirst.intern(),
            FixedPreUpdate.intern(),
            FixedUpdate.intern(),
            FixedPostUpdate.intern(),
            FixedLast.intern(),
            Update.intern(),
            PostUpdate.intern(),
            Last.intern(),
        ] {
            app.edit_schedule(label, |schedule| {
                schedule.set_executor(SingleThreadedExecutor::default());
            });
        }
        scenario::install(&mut app, run, placement);
        app.add_plugins(SimulationPlugin)
            .init_resource::<Recorder>()
            .add_systems(FixedUpdate, record.after(Sim::Settle));
        // Frame zero runs startup and primes the clock; the first fixed tick
        // is the first `tick()`.
        app.update();
        Self { app, ticks: 0 }
    }

    pub(crate) fn fresh() -> Self {
        Self::scenario("fresh")
    }

    /// Advance exactly one simulation tick.
    pub(crate) fn tick(&mut self) {
        self.app.update();
        self.ticks += 1;
    }

    pub(crate) fn ticks(&mut self, count: u32) {
        for _ in 0..count {
            self.tick();
        }
    }

    /// Advance the nearest whole number of ticks to `seconds` of simulated time.
    pub(crate) fn seconds(&mut self, seconds: f64) {
        self.ticks((seconds * SIM_HZ).round() as u32);
    }

    /// Tick until `done` holds, returning how many ticks it took, or `None` if
    /// `limit` ticks pass first. The check runs after each tick.
    pub(crate) fn tick_until(
        &mut self,
        limit: u32,
        mut done: impl FnMut(&mut Self) -> bool,
    ) -> Option<u32> {
        for elapsed in 1..=limit {
            self.tick();
            if done(self) {
                return Some(elapsed);
            }
        }
        None
    }

    /// Ticks stepped since the scenario opened.
    pub(crate) fn elapsed_ticks(&self) -> u32 {
        self.ticks
    }

    /// Ask for a hire, as a shop button press does. Applied on the next tick,
    /// and refused there if the unencumbered treasury cannot cover it.
    pub(crate) fn hire(&mut self, kind: UnitKind) {
        self.app
            .world_mut()
            .resource_mut::<HireRequests>()
            .0
            .push(kind);
    }

    /// Hand-harvest `count` bananas, as `count` drags to the stall would.
    /// Credited on the next tick.
    pub(crate) fn harvest(&mut self, count: u32) {
        let mut queue = self.app.world_mut().resource_mut::<DeliveryQueue>();
        for _ in 0..count {
            queue.entries.push(Delivery {
                amount: BANANAS_PER_HARVEST,
                kind: DeliveryKind::Manual,
            });
        }
    }

    /// Confirm a restart, as the menu does. Applied on the next tick.
    pub(crate) fn restart(&mut self) {
        self.install(SavedRun::default());
    }

    /// Replace the run in flight, as an imported save does.
    pub(crate) fn install(&mut self, run: SavedRun) {
        self.app.world_mut().resource_mut::<RestartRequest>().0 = Some(run);
    }

    /// A resource to change before stepping, for the few contracts that need
    /// to set the world up rather than only read it back.
    pub(crate) fn resource_mut<R: Resource<Mutability = bevy::ecs::component::Mutable>>(
        &mut self,
    ) -> Mut<'_, R> {
        self.app.world_mut().resource_mut::<R>()
    }

    pub(crate) fn resource<R: Resource + Copy>(&self) -> R {
        *self.app.world().resource::<R>()
    }

    pub(crate) fn treasury(&self) -> f64 {
        self.resource::<crate::domain::Treasury>().bananas()
    }

    /// The cycle of every harvester on foot, in query order.
    pub(crate) fn walking(&mut self) -> Vec<HarvestCycle> {
        self.app
            .world_mut()
            .query_filtered::<&HarvestCycle, With<Worker>>()
            .iter(self.app.world())
            .copied()
            .collect()
    }

    pub(crate) fn count<C: Component>(&mut self) -> usize {
        self.app
            .world_mut()
            .query_filtered::<(), With<C>>()
            .iter(self.app.world())
            .count()
    }

    /// Every settlement since the last call, oldest first.
    pub(crate) fn settled(&mut self) -> Vec<Settled> {
        std::mem::take(&mut self.app.world_mut().resource_mut::<Recorder>().0)
    }
}
