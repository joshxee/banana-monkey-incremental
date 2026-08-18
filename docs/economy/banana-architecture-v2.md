# Banana Incremental — Text-Only MVP Architecture (v2)

**Engine:** Bevy (Rust), ECS-first, headless/text renderer
**Scope:** Establish the simulation core. No visuals, no persistence, no prestige.

Supersedes v1. The economy is no longer a flat rate per unit; it is a harvest
cycle with separately attackable segments. Every change below is traceable to a
measured result in the companion white paper.

---

## 1. Core Loop

Monkeys walk to a grove, pick bananas, walk back, and unload them. That round
trip is the whole game.

Bananas pay wages and buy more monkeys. Some monkeys harvest. Others make the
harvesters better at exactly one part of the trip — Chefs feed them so they walk
faster, Unpackers clear the depot so they unload faster, Technologists research
better picking technique and unlock the Net Cart.

Because each support role shortens one segment of the cycle and no other, no
support role stays the best purchase for long. The player's job is to find the
current bottleneck and pay to remove it, then find the next one.

---

## 2. Invariants

| # | Invariant |
|---|---|
| I1 | Net banana rate is strictly positive after every legal purchase. |
| I2 | No harvester is *offered* unless its projected net delta is positive. |
| I3′ | Production **rate** is derived from world state every tick and never cached. Harvest **progress** is per-entity state, stored as remaining work. The treasury is credited on delivery. |
| I4 | The player can always see gross rate, wage rate, and net rate simultaneously. |
| I5 | A purchase requires the treasury to cover its cost *plus* a wage reserve sized to the gap between cart deliveries. |

I3′ is the load-bearing one, and it is weaker than v1's I3 by necessity. A
monkey halfway to the grove is *somewhere*, and that position is not derivable
from component counts. What survives is the part that mattered: the rate is
still a pure function of counts and multipliers, so nothing can silently drift
out of sync with the world.

I1 is stronger than v1's. Non-producing units draw wages, so "not negative" is
not enough — a purchase that drives net to zero strands the run.

I5 is new and exists because cart income is lumpy while wages are continuous.
See D15.

---

## 3. Decision Log

**D1 — Bananas are a resource, not entities.**
`f64` in a `Treasury` resource. Counts stay far below $2^{53}$ in the MVP, but
the type should not need revisiting when prestige arrives.

**D2 — Wages are a per-unit component, not a global formula.**
Tiers tune independently without touching systems.

**D3 — Five unit types.**
Worker Monkey (harvests on foot), Net Cart (harvests with a crew of three),
Chef (speed), Unpacker (unload rate), Technologist (research).

**D4 — All multipliers are additive within their term.**
`M = 1 + count × bonus`. Multiplicative stacking is unbalanceable at this stage.
Revisit post-MVP.

**D5 — Augments target a cycle segment, not an entity.**
Support units declare which *term* of the harvest cycle they shorten. No entity
relationships, no dangling references, and a new vehicle tier inherits correct
support sensitivity without a design decision.

**D6 — *Deleted.***
v1 exempted carts from the chef bonus to manufacture a tradeoff. Measured, that
rule removes Chefs from the game entirely: zero purchased across a full run,
because carts absorb workers into crews and leave no pool for chefs to improve.
One speed multiplier now applies to every travel segment, carts included. The
tradeoff survives on its own — see §5.

**D7 — Worker assignment is modelled by presence of a component.**
An unassigned worker has no `AssignedTo`. The pool is the query
`With<Worker>, Without<AssignedTo>`.

**D8 — Structures may be under-staffed and produce proportionally, and their
wages scale the same way.**
A 1-of-3 crewed cart produces at ⅓ *and costs ⅓ of its wage*. Production-only
scaling makes a retired vehicle a permanent tax, which matters more once
vehicles are a ladder. Every cart performs the nominal full-cart segment work;
its crew fraction is sampled when a cycle starts and scales only the delivered
payload. Assignment changes therefore affect its next cycle. This keeps every
segment duration unchanged and makes production exactly proportional.

**D9 — No intermediate aggregation resource.**
Multipliers are locals inside the production system. Reintroduce when a second
augment targets the same term and the map stops being trivial.

**D10 — `AssignedTo` is the only record of staffing.**
`Staffing` carries `required` only. Assigned counts are tallied by query.

**D11 — Offers project rates, and only for harvesters.**
Each harvester's offer carries cost, projected net delta, and payback seconds.
Technologists are excluded — see D14.

**D12 — Production is `payload / cycle_time`.**
```
T = travel/M_speed + pick/M_tech + unload/M_unpack
```
Three addends, three support roles, one each. This is the whole design.

**D13 — Cycle progress is stored as remaining work, not elapsed time.**
Metres left to walk, nominal bananas of work left to pick or unload, and the
cycle's delivery scale; each tick consumes remaining work at the current
multiplier's rate. The nominal work equals literal payload at full staffing. At
partial staffing D8 scales settlement, not segment work. Buying a Chef then
correctly speeds up the rest of every trip already in flight without teleporting
anyone down the road, and buying an Unpacker helps carts already queued at the
depot. It also collapses three segments into one uniform representation.
The UI labels gross and net as steady-state averages and shows each cart's
in-flight payload, so a staffing change cannot make the next delivery look
incorrect.

*Implementation note (Worker Monkey, 2026-08-18).* A tick must be spent as a
**time budget**, not as `remaining -= rate × dt` followed by `remaining <= 0`.
The naive form fails twice, and both failures are silent:

- `1/20` is not exactly representable in binary, so repeated subtraction leaves
  a *positive* residual — 9.4e-15 on a 5-banana pick segment, 1.0e-15 on
  unload. Each residual costs a whole extra tick, turning the 47.5-second
  worker cycle into a 47.6-second one.
- Discarding the budget left over at a segment boundary loses `dt/2` per
  boundary, a 0.21% throughput error at these parameters. Because the loss is
  absolute, it grows as a *fraction* when multipliers shorten the cycle: 0.28%
  with ten Chefs, 0.43% at `M_speed = 2.5`. Buying support would quietly make
  the implementation less accurate.

So: consume `needed = remaining / rate` out of a per-tick budget, carry the
remainder across the boundary, and compare with a tolerance of a billionth of
the segment's duration. A bounded iteration guard is load-bearing rather than
defensive — a zero rate makes `needed` infinite and the loop never terminates.

**D14 — Technologists are never ranked against harvesters.**
Their banana delta is exactly $-\text{wage}$ at every possible world state — it
carries no information. Ranking them requires net-present-value reasoning over
an invented horizon. The research track gets its own readout in the full
product; in the MVP the Technologist simply appears in the shop with a research
figure and no payback number.

**D15 — Purchases are gated on a wage reserve.**
```
reserve = 2 × max(0, wages − pool_income) × cart_cycle / cart_count
```
Only the wages that carts are covering are at risk, and only for one delivery
gap. Costs 0.3 minutes across a 24-minute session.

*Amended (Worker Monkey, 2026-08-18) — the formula returns zero for a cart-free
economy, and that is wrong.* It hard-codes "the lumpy source is carts, the
continuous one is the pool", so with `K = 0` it reserves nothing. But a lone
worker delivers once every 47.5 seconds, which is not continuous by any
reading, and the treasury goes underwater without a reserve.

The fix is to identify the lumpiest source by *measurement* rather than by
name: reserve against the largest gap between deliveries, and credit only the
income that keeps arriving inside it.

```
gap     = maxᵢ (Tᵢ / nᵢ)                 over every source in the field
covered = Σ { rateᵢ : Tᵢ / nᵢ < gap }
reserve = 2 × max(0, wages − covered) × gap
```

This reduces to the published cart form whenever carts are the lumpiest source,
and to a constant **2.85 bananas** for a worker-only economy — `2 × 0.03W ×
47.5/W`, independent of `W`. Measured over 30 purchases and 40 seeds: median
worst dip −3.36 → −1.09, minimum −6.54 → −3.04, for 1.8% of pacing. The first
hire therefore needs 6.85 bananas rather than 4. All 39 contract assertions
still pass.

One near miss is worth recording, because it looks correct and is not. Using a
*blended* mean gap, `1 / Σ(nᵢ/Tᵢ)`, also collapses to 2.85 for workers — but it
under-reserves badly once carts exist (38.5 against a measured −66.0 dip at
`K = 4`), because frequent five-banana pool deliveries drag the average gap down
without doing anything to cover wages across the two-hundred-banana cart gap.
Averaging delivery *frequency* is the wrong statistic when income is both lumpy
and heterogeneous.

The alternative considered and rejected was clamping the treasury at zero.
Unpayable wages then become *free bananas*: a 12.5% pacing gift concentrated in
the first twelve purchases, and an exploit with no on-screen tell, since
spending down to exactly zero buys a wage holiday until the next delivery. At
`W ≈ 7` the forgiven amount exceeds the price of the monkey that caused it. Debt
is allowed to happen instead; it is rare and small, exactly as §6 intends.

**D16 — Cart cycle phase is randomised after assignment on spawn.**
Carts bought in one burst otherwise synchronise. Measured, that deepens the
treasury dip from −38 to −255 in the same economy. One line; prevents a
punishment the player cannot see or diagnose. A new cart remains pending until
assignment completes, then samples its crew fraction and initial phase together.

*Extended to workers (Worker Monkey, 2026-08-18), and the window is not
negotiable.* The phase must be uniform in **time across the whole cycle**. Two
narrower windows are tempting and both are wrong:

- Sampling a segment and then a position inside it puts 25% of new workers in
  Unload, which is 5.3% of the cycle, biasing first-cycle income upward by ~25%.
  It self-corrects after one cycle, so only a first-minute test catches it.
- Confining the window to the outbound leg — attractive because a worker would
  then always appear empty-handed and walking, rather than materialising
  mid-route — leaves 27.5 seconds of every cycle in which a burst-bought cohort
  delivers nothing. D15's amended reserve is derived from a mean gap of `T/W`,
  so the dip becomes `0.03 × W × 27.5`, which passes the 2.85 reserve at `W = 4`
  and grows without bound. The full-cycle window holds the dip at 1.425 for
  every `W`.

The presentation cost is real and is paid elsewhere: because a hire can appear
anywhere on the route, the purchase needs its own visible cue. The
implementation flashes the new worker gold for 0.6 s.

**D17 — Assignment is auto-pull.**
Cart slots beat the pool by 230–280% at every reachable world state, so the
optimal policy is always "fill every slot, overflow to the pool." Manual
assignment can only reproduce that with extra clicks or produce a worse result.
D7 and D10's representation stays; the player-facing choice is removed.

---

## 4. Data Model

### Resources

```rust
#[derive(Resource)] struct Treasury { bananas: f64 }
#[derive(Resource)] struct Research { points: f64, level: u32 }

#[derive(Resource, Default)]
struct EconomySnapshot {
    gross_per_sec:  f64,   // steady-state expected rate: Σ payload / cycle_time
    wages_per_sec:  f64,
    net_per_sec:    f64,
    wage_reserve:   f64,
}

#[derive(Resource)] struct UnitCosts { /* base + growth per type */ }

#[derive(Resource, Default)]
struct PurchaseOptions { offers: Vec<Offer> }

struct Offer {
    kind: UnitKind,
    cost: f64,
    projected_net_delta: f64,
    payback_secs: f64,
    affordable: bool,          // treasury ≥ cost + reserve  AND  net stays > 0
    cycle_effect: CycleDelta,  // powers the shop info panel
}
```

### Components

```rust
#[derive(Component)] struct Worker;
#[derive(Component)] struct Chef;
#[derive(Component)] struct Unpacker;
#[derive(Component)] struct Technologist;
#[derive(Component)] struct Structure;

#[derive(Component)] struct Wage(f64);
#[derive(Component)] struct Payload(f64);
#[derive(Component)] struct Speed(f64);

#[derive(Component)]
struct Augment { target: Segment, bonus: f64 }   // Travel | Pick | Unload

#[derive(Component)] struct Staffing { required: u32 }
#[derive(Component)] struct AssignedTo(Entity);

#[derive(Component)]
struct CycleProgress {
    segment: Segment,
    remaining: f64,
    delivery_scale: f64, // 1.0 for workers; cart crew fraction sampled at cycle start
}
```

`Wage` is shared across all five archetypes. `Payload` and `Speed` sit on
harvesters only. `CycleProgress` is the sole piece of irreducible per-entity
state in the simulation. Settlement credits `Payload × delivery_scale`.

---

## 5. Where the tradeoff lives

v1 tried to make carts interesting with a rule. The cycle model makes them
interesting with arithmetic.

| Cycle | travel | pick / load | unload |
|---|---|---|---|
| Worker on foot | **84%** | 11% | 5% |
| Net Cart, crew of 3 | 7% | 37% | **56%** |

A monkey on foot spends most of its life walking. A cart barely travels at all
and instead sits at the depot being emptied. So ten Chefs raise worker
throughput by 102% and cart throughput by 5%; ten Unpackers do the reverse, 4%
and 59%.

Nobody decided that. It follows from payload and speed, and it will keep
following from them when a tier-2 vehicle with a longer route turns out to be
more chef-sensitive than the cart was.

The decision is not "cart or pool?" — D17 settles that. It is *which segment of
which cycle am I paying to shorten?*, and the answer changes as you buy. In a
measured session the dominant cart segment moves from unload to load partway
through, at which point Unpackers quietly stop being the obvious buy.

---

## 6. Component Diagram

```mermaid
graph TD
    subgraph Resources
        T[Treasury]
        R[Research<br/>points / level]
        E[EconomySnapshot<br/>gross / wages / net / reserve]
        O[PurchaseOptions<br/>cost + delta + payback]
    end

    subgraph "Worker Monkey"
        W1[Worker]
        W2[Wage]
        W3[Payload + Speed]
        W4[CycleProgress]
        W5["AssignedTo (optional)"]
    end

    subgraph "Net Cart"
        S1[Structure]
        S2[Wage]
        S3[Payload + Speed]
        S4[CycleProgress]
        S5[Staffing]
    end

    subgraph Support
        C1["Chef · Augment(Travel)"]
        U1["Unpacker · Augment(Unload)"]
        X1["Technologist → Research"]
    end

    W5 -.crews.-> S1
    C1 -.shortens travel.-> W4
    C1 -.shortens travel.-> S4
    U1 -.shortens unload.-> W4
    U1 -.shortens unload.-> S4
    X1 -.levels raise.-> R
    R -.shortens pick.-> W4
    R -.shortens pick.-> S4
    C1 -.feeds researchers.-> X1
```

Every augment arrow terminates on a cycle segment. Chefs reach both harvesters
because both of them walk — that is D6's deletion, drawn.

---

## 7. System Schedule

```mermaid
graph LR
    P[1. Purchase<br/>drain treasury<br/>spawn pending] --> A[2. Assign<br/>fill cart slots<br/>overflow to pool]
    A --> I[3. Initialize<br/>sample crew fraction<br/>jitter new cart phase]
    I --> M[4. Multipliers<br/>speed / unpack / tech<br/>as locals]
    M --> AD[5. Advance Cycles<br/>consume remaining work<br/>emit Delivered events]
    AD --> SE[6. Settle<br/>treasury += deliveries<br/>treasury -= wages × dt]
    SE --> SN[7. Snapshot<br/>steady-state rate<br/>wages, net, reserve]
    SN --> OF[8. Project Offers<br/>delta + payback<br/>+ cycle effect]
    OF --> RE[9. Render<br/>snapshot + offers<br/>in-flight payload]
```

Stages 4, 7 and 8 are pure functions of world state. Stages 3 and 5 are the only
writers of `CycleProgress`; stage 6 is the only writer of `Treasury`. Stage 5
samples current staffing only when beginning the next cycle.

### Tick Math

```
M_speed   = 1 + chefs      × chef_bonus
M_unpack  = 1 + unpackers  × unpack_bonus
M_tech    = 1 + tech_level × tech_bonus

T_worker  = 2d/(v_w·M_speed) + q·t_pick/M_tech       + q·t_unload/M_unpack
T_cart    = 2d/(v_k·M_speed) + Q·t_pick/(r·M_tech)   + Q·t_unload/M_unpack

gross     = pool × q/T_worker + carts × Q/T_cart × staffed/r
wages     = Σ Wage, cart wages scaled by staffed/r
net       = gross − wages
reserve   = 2 × max(0, wages − pool_income) × T_cart / carts
```

Offer projection reuses the same expression with one unit hypothetically added,
and reports the diff of every segment it changed — which is exactly what the
shop's info panel renders.

---

## 8. MVP Feature Cut

**In:** five unit types; harvest cycles with per-entity progress; escalating
per-type costs; research levels gating the Net Cart; auto-pull crewing;
live gross/wages/net/reserve readout; per-offer payback and cycle-effect diff;
fixed-step tick loop with decoupled text refresh.

**Out:** save/load, prestige, offline progress, population cap, additional
vehicle tiers, retiring or reassigning technologists, a persistent research
progress bar, achievements, any rendering beyond stdout.

The research progress bar is deliberately deferred rather than cut — see §10.

---

## 9. Resolved Questions

1. **Cost growth curve** — per type. Worker 1.15, Chef 1.30, Unpacker 1.30,
   Technologist 1.35, Cart 1.70. These are the primary balance levers.
2. **Manual vs auto-pull assignment** — auto-pull (D17). Manual assignment also
   breaks offer projection: an unstaffed cart produces nothing, so its delta is
   $-\text{wage}$ and I2 would never offer a cart at all.
3. **Cart purchasable with an empty pool** — no, and it needs no special case.
   Projected delta is negative and I2 suppresses it.
4. **Fixed timestep** — sim at 20 Hz, text at 4 Hz. Every contract test depends
   on determinism.

## 10. Known Gaps

**The Technologist has no pull.** D14 removes it from the ranked list, and the
MVP has no research progress bar. A player following payback order will never
buy one, never unlock the Net Cart, and never see the second half of the game.
In a text MVP where all five units are visible at once this is survivable; in
the full product the research bar with its named next unlock is what closes it.
Flagged here so it is not rediscovered as a bug.

**Retired Technologists are permanent overhead.** With capped research they end
the session drawing wages for nothing — six of them, 1.2 bananas/sec against a
15-banana gross. Tolerable now; under a population cap they are six monkeys not
harvesting, which is a different problem. Retraining is the v2 fix.
