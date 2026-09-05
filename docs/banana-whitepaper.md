# Bottlenecks as Game Design
### The mathematics of a harvest-cycle incremental

*Companion to the Banana Incremental MVP architecture. All figures are measured
from the reference model in `banana_model.py`; the contract suite in
`test_banana.py` asserts every claim made here.*

---

## Abstract

Conventional incremental games model production as `count × rate × multiplier`.
Costs grow exponentially, production grows polynomially, and the game is the
seesaw between them.

We replace the flat rate with a **harvest cycle** — travel, pick, travel,
unload — and give each support unit exactly one segment to shorten. The result
is that Amdahl's law does the balancing. Every support role acquires automatic
diminishing returns without a tuned relevance window, purchase priority rotates
on its own, and the tradeoff between harvest methods emerges from payload and
speed rather than from a rule.

We also report three failures the model produced along the way, because each is
a general trap rather than a tuning accident: a rule that deleted a unit type,
a unit type that marginal projection cannot price at all, and a wall that turned
out not to exist.

---

## 1. The model

Let a harvester carry payload $q$ over distance $d$ at speed $v$, picking at
$t_{\text{pick}}$ seconds per banana and unloading at $t_{\text{unload}}$.

$$T \;=\; \underbrace{\frac{2d}{v\,M_{\text{speed}}}}_{\text{travel}} \;+\; \underbrace{\frac{q\,t_{\text{pick}}}{M_{\text{tech}}}}_{\text{pick}} \;+\; \underbrace{\frac{q\,t_{\text{unload}}}{M_{\text{unpack}}}}_{\text{unload}}, \qquad \text{throughput} = \frac{q}{T}$$

Three multipliers, three addends, one each:

| Role | Fiction | Term |
|---|---|---|
| Chef | feeds the monkeys, so they move faster | travel |
| Technologist | researches better picking technique | pick |
| Unpacker | clears the depot | unload |

$$M_{\text{speed}} = 1 + C\beta_c, \quad M_{\text{unpack}} = 1 + U\beta_u, \quad M_{\text{tech}} = 1 + L\beta_t$$

Costs remain geometric per type, $\text{cost} = b\,g^{\,n}$, so the classical
seesaw is intact. What changes is the shape of the production side.

### 1.1 Why one-term-each matters

Shortening a single addend has bounded effect on the sum. As
$M_{\text{speed}} \to \infty$ the travel term vanishes and throughput
approaches $q/(\text{pick} + \text{unload})$ — not infinity.

This is Amdahl's law, and it is the entire design. **Every support role has
built-in diminishing returns, and the returns diminish precisely as that role's
term stops dominating the cycle.** No relevance windows to tune, no unlock
thresholds to hand-place. When Chefs stop mattering it is because monkeys are no
longer mostly walking, which is both true and legible.

---

## 2. Emergent asymmetry

The two harvest methods have different cycle shapes, and everything follows.

| | travel | pick / load | unload | throughput |
|---|---|---|---|---|
| **Worker**, payload 5, 5 m/s | 40.0 s (**84%**) | 5.0 s (11%) | 2.5 s (5%) | 0.105 /s |
| **Cart**, payload 100, crew 3, 15 m/s | 13.3 s (13%) | 33.3 s (33%) | 50.0 s (**49%**) | 0.983 /s |

A monkey on foot spends its life walking. A cart barely travels and instead
sits at the depot being emptied — it is fast and capacious and *slow to handle*,
exactly as specified. Consequently:

| | worker throughput | cart throughput |
|---|---|---|
| +10 Chefs | 0.105 → 0.213 (**+102%**) | 1.111 → 1.163 (+5%) |
| +10 Unpackers | 0.105 → 0.109 (+4%) | 1.111 → 1.765 (**+59%**) |

Chefs are for monkeys on foot. Unpackers are for carts. Nobody wrote that rule;
it is a consequence of `payload / speed`. A future vehicle with a longer route
will be more chef-sensitive automatically, and the designer's only job is to
choose payload, crew, and speed.

**This replaces a rule with a fact, and §5.1 shows why that was necessary.**

---

## 3. The ceiling theorem

**Claim.** With $t_{\text{pick}}$ fixed, every harvest method converges to the
same throughput per monkey, namely $M_{\text{tech}}/t_{\text{pick}}$.

**Proof sketch.** As $M_{\text{speed}}, M_{\text{unpack}} \to \infty$, the
travel and unload terms vanish and $T \to q\,t_{\text{pick}}/M_{\text{tech}}$.
Throughput $q/T \to M_{\text{tech}}/t_{\text{pick}}$, independent of $q$, $v$,
and crew size. Payload cancels because pick time scales with payload. $\square$

Measured at $M_{\text{tech}} = 1$: worker **0.9999**/s, cart **1.0000**/s per
crew monkey. Both equal $1/t_{\text{pick}}$.

Three consequences, in ascending order of importance.

**Bigger vehicles buy a constant, never an asymptote.** A tier-4 truck hauling
ten thousand bananas has the same ceiling as a monkey with a basket. Vehicle
tiers are pacing devices, not growth engines.

**Total production caps at $W \cdot M_{\text{tech}} / t_{\text{pick}}$.** Since
$W$ is bounded logarithmically by exponential costs, and $M_{\text{tech}}$ grows
logarithmically in research invested, the cycle model is a materially weaker
growth engine than the flat model it replaced. That is a real cost, paid for the
decision space in §2.

**It is not binding in the MVP.** At session end the worker sits at 13% of
ceiling and the cart at 52%. Six-fold headroom on foot, two-fold on carts. The
theorem is a design constraint for tier 3 and for the prestige system, not a
present concern — and it tells us precisely where the population cap and town
size will need to do their work.

---

## 4. Measured session

Optimal play, defined as always buying the shortest-payback offer.

A multi-unit incremental has no stall — there is always something cheap left to
buy — so we define the session as ending when the best available purchase
exceeds a fifteen-minute payback.

**24.2 minutes, 52 purchases, 37 changes of purchase type.** Ending state: 26
workers (8 in the pool, 18 crewing), 6 Chefs, 9 Unpackers, 6 Technologists, 6
Carts, tech level 5. Gross 15.35/s against 4.68/s in wages.

| | |
|---|---|
| **0 – 4.3 min** | Bare foraging. One monkey, then a dozen, all on foot. |
| **4.3 min** | First Technologist. No banana yield; the player is buying an unlock on faith. |
| **8.3 min** | Tech level 1 lands. **First Net Cart.** Three monkeys leave the pool. |
| **11.2 min** | Carts are 57% unload-bound and choking. **First Unpacker.** |
| **15.9 min** | The pool has regrown enough that walking is the binding cost again. **First Chef** — 16 minutes into the game. |
| **15.9 – 24.2 min** | All five rotating. The dominant cart segment crosses from unload to load, and Unpackers quietly stop being the obvious buy. |

Payback inflates from 93 s to 931 s across the session and to 6,077 s if the run
is continued to ninety-six purchases — a 37-fold degradation. Exponential costs
outrun cycle-bound production exactly as the classical model predicts, but
smoothly, without a cliff.

The fact worth dwelling on is that **Chefs do not appear for sixteen minutes.**
Nothing gates them. They are simply not the bottleneck until they are.

---

## 5. Three failures worth publishing

### 5.1 A rule that deleted a unit type

The original design stipulated that Chefs boost the worker pool only, so that
carts would be "a genuine tradeoff rather than a strict upgrade."

Measured, that rule removes Chefs from the game. Zero purchased, in a full run.

The mechanism is a local optimum. Carts absorb workers into crews, so the pool
stays small; if Chefs improve only the pool, their marginal value never clears
their wage; so they are never offered; so the pool never becomes worth
improving. Under the exemption the session finishes with **four** unit types and
21 fewer purchases.

Deleting the rule and letting one speed multiplier apply to all travel restores
Chefs — and the intended tradeoff survives anyway, because §2 produces it from
arithmetic. The direct effect of Chefs on carts is only +5%, but that 5% is the
difference between a viable purchase and dead content.

**General form:** a rule that exempts unit A from support B, in a game where A
consumes the resource that makes B valuable, does not create a tradeoff. It
creates a trap. Prefer facts that emerge from quantities over rules that
override them.

### 5.2 A unit type that marginal projection cannot price

Technologists produce no bananas and draw a wage. Their projected net delta is
$-0.2000$/s at $W{=}1$, at $W{=}20$ with 5 Chefs, and at $W{=}60$ with 20 Chefs,
15 Unpackers and 5 Carts. Not approximately — identically, at every reachable
world state. The number carries no information about the world.

Under an offer system that shows only positive-delta purchases, Technologists
are never offered; carts are gated behind research; two of five unit types
vanish silently, and the offer list gives no signal that anything is missing.

Four repairs were attempted:

| Repair | Outcome |
|---|---|
| Offer if any tracked resource has positive delta | Buys one into a one-monkey economy; net inverts; run dies |
| Price research at $\text{gain}/L_{\text{next}}$ | Banana-equivalent 0.02/s against a 0.20/s wage. Still never offered |
| Add the option value of what the level unlocks | Still negative |
| Net present value over an explicit horizon $H$ | Works |

The third failure is dimensional, and it is the interesting one. A tech level
delivers a *permanent rate*; dividing that by a *one-off* research cost is not a
meaningful quantity. Only NPV — the unlock is forever, discounted by how long
you wait for it — has the right units.

**General form:** marginal projection prices rates. An unlock is a step
function. No amount of patching the projector will make it see a step; you need
a second valuation path, and that path needs a horizon $H$ that is a design
parameter rather than a derivable constant.

The design response is to stop trying. Research gets its own readout and its own
decision. "Fund research or buy three more monkeys?" is a *good* question
precisely because the game will not answer it, and auto-ranking it would hand
the player the answer while inventing a number to do so.

### 5.3 A wall that did not exist

An early instrument reported that runs stalled at 24 minutes. They did not. The
simulated player, on finding the top-ranked offer unaffordable within its
patience, gave up — rather than buying the second-ranked offer, which was
fifteen seconds away.

With the bug fixed, runs continue past ninety-six purchases and an hour. The
"wall" was an artifact of a policy no human would follow.

This has a design consequence beyond the fix. **In a single-unit incremental the
wall is a stall; in a multi-unit one it is inflation.** There is always something
cheap left, so progress never stops — it just stops being worth the attention.
Session length must therefore be defined against a payback threshold rather than
an affordability threshold, and that threshold is a statement about player
patience, not about the economy.

Two smaller corrections came from the same audit. A diagnostic that stubbed
`research = 1e9` was silently running at tech level 21 and reporting cart cycles
that no session ever sees. And a claim that cheap Technologists cratered the run
turned out to be the same fall-through bug wearing a different hat: with the
policy corrected, technologist price merely shifts when the research track opens
— from 4.3 to 12.1 minutes across a 4.5× price range — and changes nothing else.

*We report these because the instrument was wrong three times and the model was
right three times, which is the usual ratio and worth stating out loud.*

---

## 6. Lumpy income

A cart delivers 100 bananas every ~102 seconds. Wages drain continuously. The
treasury therefore dips between deliveries even when net rate is comfortably
positive.

> **Superseded for workers, §6.1.** Harvesters are now fed out of the delivery
> they have just made, so a worker contributes no dip at all and needs no
> reserve. The analysis below still governs every unit whose wage is drained
> continuously, which today is the support staff and the cart crews.

Measured worst dips over twenty minutes from a zero start: −27 at three carts,
−36 at five, −38 at six. The dip does not grow with the economy, because wages
rise while cart cycles shorten and the two roughly cancel.

**Desynchronisation is doing most of the work.** With cart phases randomised on
spawn, deliveries interleave. With every cart bought in one burst they align:
the same economies dip −150, −216 and −255 — four to seven times worse, purely
because of purchase timing the player cannot see. Randomising the initial phase
is one line and prevents an invisible punishment for saving up.

The reserve that covers the rest:

$$\text{reserve} \;=\; 2 \cdot \max(0,\ \text{wages} - \text{pool income}) \cdot \frac{T_{\text{cart}}}{K}$$

Only the wages that carts are covering are at risk — pool workers deliver every
twenty to forty seconds, which is continuous enough — and only for one delivery
gap, which is $T_{\text{cart}}/K$ and not $T_{\text{cart}}$. The factor of two
absorbs the variance in a gap that is, after all, a random variable.

A first attempt reserved total wages over a full cart cycle. At fourteen workers
and one cart that is a 138-banana reserve against a 6.8-banana risk, and it
collapsed the session from 52 purchases to 16. The corrected form costs **0.3
minutes** across a 24-minute session, with no change to purchase count or unit
mix.

Residual exposure is a few tens of bananas. This is by design: debt is possible,
rare, and small.

### 6.1 Post-paid meals, and the death of the worker reserve

The reserve above is a hedge against a timing mismatch: wages fall due
continuously while income arrives in lumps. For workers the mismatch was total.
A fresh hire spent a full cycle costing bananas before earning any, so I5 had to
demand a 2.85-banana reserve on top of a 4.0 signing fee — a shop that quoted one
price and enforced another, which reads as a bug however it is explained.

**Removing the mismatch is strictly better than reserving against it.** A worker
now spends the last 5% of its trip eating at the stall, immediately after
unloading, and that meal *is* its wage:

$$\text{meal} \;=\; w_{\text{salary}} \cdot T_{\text{worker}} \;=\; 1.5 \ \text{bananas of the } 5 \text{ it just delivered}$$

Three properties follow, and between them they replace the reserve entirely:

1. **Solvency is structural**, by *two* mechanisms, and it is worth being
   precise about which does the work. Within a cycle the credit strictly
   precedes the debit and strictly exceeds it, so an undisturbed worker cannot
   take the treasury below where it stood before its delivery landed. But a
   purchase *can* land in the 47.5→50 s window between a delivery and the meal
   it funds, since `plan_hire` gates on the fee alone — and there the ordering
   argument does not save you. What does is the larder gate: the worker cannot
   afford its meal, and stalls. Measured worst dip over twenty minutes:
   **0.000 at W = 1, 4, 10 and 25**, against −1.42 under the drained model. The
   signing fee can therefore be the whole requirement.
2. **The wage rate is unchanged.** Both the meal and the eating time are defined
   against the cycle, so $w_{\text{salary}}$ stays exactly 0.03/s at every
   multiplier. Nothing downstream — payback ranking, chef viability, §5.1 —
   moves. (Per second of *cycle*: a stalled worker's cycle stretches, so its
   realised wage rate falls below 0.03/s while it starves. That is intended — it
   is the punishment — but the invariant is conditional on a fed workforce.)
3. **The counter reads as arithmetic.** +5 on delivery, −1.5 two seconds later,
   flat in between. The drained model instead ticked imperceptibly downward for
   47 seconds and then jumped; players read the first half as a freeze and the
   second as a glitch.

The cost is 5% of throughput, taken as a *share* of the trip rather than a fixed
2.5 seconds. That distinction is load-bearing. A fixed snack would become an
ever-larger slice of a shortened cycle, so chefs would raise the cost of labour
per second and partly cancel themselves, and worker throughput would converge to
$q/t_{\text{snack}}$ instead of to the pick-rate ceiling of §3 — the bound the
whole game rests on. As a share it costs a flat 5% of that ceiling and nothing
more, and the theorem survives with a constant factor:

$$\lim \text{throughput} \;=\; (1 - f_{\text{snack}}) \cdot \frac{M_{\text{tech}}}{t_{\text{pick}}}$$

**Owed, never forgiven — within a session.** A worker that cannot afford its
meal stalls at the stall rather than eating on credit, and resumes the instant
food arrives. Reloading is the exception: cycle phase is not persisted, so a
starving worker's debt dies with the page. Not worth save-scumming for — a
reload also discards in-flight progress worth about half a payload per healthy
worker — but a real hole, recorded in `persistence.rs`. This matters: clamping the treasury at zero instead would turn unpayable wages into
free bananas and make spending down to zero a wage holiday. Stalling is a
penalty, it is visible — the sprite greys out and production stops — and it is
the only place in the economy where overspending has a consequence the player
can see.

**Carts owe the same treatment.** Their crews still drain continuously, so
`wage_reserve` still fires for them and §6's analysis still governs. Until the
cart increment adopts a snack, workers converge 5% under the §3 ceiling and
carts converge on it, which is the one asymmetry this change introduces.

---

## 7. Determinism and the two rates

Per-entity cycle progress means production is no longer a pure function of
component counts. What remains pure is the *expected* rate,
$\sum q_i / T_i$ — no phase appears in it — and that is the quantity offers,
projections and the readout all consume.

The *realised* rate depends on phase. Over 100 simulated minutes the two agree
to within 0.4% (14.781 vs 14.841 bananas/sec), but over any thirty-second window
they disagree substantially, because that is what a spike is.

The practical consequences are small and worth stating plainly: label the
readout as an average or players will report the treasury freezing as a bug;
store progress as *remaining work* rather than elapsed time, so that a Chef
bought mid-trip speeds up the remainder of every journey in flight without
teleporting anyone; and keep the treasury as the only accumulator, credited on
delivery.

---

## 8. Parameters

| Unit | payload | speed | wage | cost base | cost growth | augment |
|---|---|---|---|---|---|---|
| Worker Monkey | 5 | 5 m/s | 0.03 (post-paid, §6.1) | 4 | 1.15 | — |
| Cart | 100 (crew 3) | 15 m/s | 0.20 | 70 | 1.70 | — |
| Chef | — | — | 0.10 | 25 | 1.30 | travel +0.15 |
| Unpacker | — | — | 0.10 | 30 | 1.30 | unload +0.20 |
| Technologist | — | — | 0.20 | 40 | 1.35 | pick +0.10/level |

Grove distance 100 m. $t_{\text{pick}} = 1.00$ s/banana,
$t_{\text{unload}} = 0.50$ s/banana. A worker spends a further
$f_{\text{snack}} = 5\%$ of each trip eating at the stall (§6.1), which makes
the round trip 50.0 s: 40 travel, 5 picking, 2.5 unloading, 2.5 eating. Gross
0.100/s per worker, wages 0.030/s, net **0.070/s**. Research level $n$ costs $60 \times 2.2^n$;
one Technologist yields 1.0 research/sec, scaled by $M_{\text{speed}}$ — chefs
feed the researchers too. Net Cart requires tech level 1. Seed: one free worker.

### 8.1 Where the shipped game differs at $t = 0$

`banana_model.py` seeds one free worker (`free_workers = 1`); the Rust
implementation seeds **none**. The game opens as a manual clicker, and buying
the first monkey is the moment automation begins — a tutorial beat the model has
no reason to represent.

This is a difference in $t = 0$ semantics, not in balance. Cost is indexed on
workers *owned*, so the ladder is unshifted: the oracle's first *purchase* is
its second worker at $4 \times 1.15^1$, and the game's first purchase is its
first worker at $4 \times 1.15^0$. Every subsequent price agrees. The whole
downstream session is the oracle's, displaced by the time it takes to hand-pick
the opening 4.0 bananas.

Two consequences worth stating so they are not rediscovered as bugs:

**The first delivery is 47.5 seconds after the first purchase**, every time. The
seed worker exists in the model partly because it hides this; without it, the
wait is the price of making the purchase the tutorial. The implementation answers
it with presentation — the monkey walks out of the stall on the click, carries a
banana home, and arrives loudly — rather than by changing a parameter.

Fresh hires deliberately start at phase zero. This keeps the purchase's most
legible consequence: a new monkey walks out of the stall. When loading saved
data, the existing workforce receives random elapsed-time phases so a resume
does not reset every monkey to the stall. This is presentation-only because the
save format does not claim to preserve offline cycle progress. The geometric
cost ladder still staggers hires on its own. Every numeric lever here is expensive:
halving the grove distance invalidates D17's measured 230–280% cart advantage,
doubling worker speed cuts the Chef effect from +102% to +52% and undermines
§5.1, and $t_{\text{pick}}$ sets the ceiling in §3 outright.

**Manual clicking dominates the automated economy for roughly the first dozen
purchases.** At one drag per second the player earns 1.0/s against a worker's
net 0.070/s. That is ordinary for the genre, but it means the first monkey is
sold on "it works while you are not clicking", not on rate.

Prices are charged exactly as $b\,g^{\,n}$ and displayed to one decimal.
Rounding them up to whole bananas was considered and rejected: it is a 15%
premium on the third worker, an 8–11% drag on time-to-Nth-monkey, and it moves
the measured session from 24.2 to 25.3 minutes, all for cosmetics.

---

## 9. Contract tests

Thirty-nine assertions, all passing. Four deserve mention because they guard
findings rather than arithmetic.

**`chefs are purchased`** is the §5.1 regression guard. Any change that starves
Chefs of value — a new rule, a wage tweak, a cart buff — trips it before a human
would notice a missing unit.

**`banana delta is exactly −wage at every world state`** pins §5.2. If this ever
fails, Technologists have acquired a marginal banana value and the case for
excluding them from ranking needs revisiting.

**`both harvest methods converge to $M_{\text{tech}}/t_{\text{pick}}$`** asserts
the ceiling theorem against the implementation, so a future vehicle tier that
appears to escape it is caught immediately — because it would mean a bug, not a
breakthrough.

**`the dominant cart segment shifts across the run`** asserts that Amdahl is
actually rotating. It is the difference between a game with a bottleneck and a
game with *a moving* bottleneck, and it is the one property that, if lost,
quietly reduces the design to picking the biggest number.

---

## 10. What generalises

For a vehicle tier $n$ with output $c_n$, crew $r_n$ and wage $s_n$, the crossover
against foot harvesting sits at

$$C^*_n \;=\; \frac{1}{\beta_c}\left(\frac{c_n - s_n}{r_n\,w} - 1\right)$$

so pick the chef count at which tier $n{+}1$ should take over, then solve for
$c_n$. But the deeper guidance from this exercise is smaller and less formal:

**Give each support role exactly one term.** Diminishing returns, rotation, and
relevance windows all come free, and you stop hand-tuning them.

**Let asymmetries emerge from quantities.** §2's tradeoff is more robust than
§5.1's rule, and it extends to units that do not exist yet.

**Watch for units whose payoff is a step.** They are invisible to any marginal
system, and the correct response is usually to give them their own decision
rather than to force them into a ranking that cannot hold them.

**And check whether the wall is real.** Ours was not, twice.

---

*Reference model: `banana_model.py`. Contract suite: `test_banana.py` — 39
assertions covering invariants, cycle physics, the ceiling, staffing,
technologist valuation, spiky delivery, content liveness, pacing and numerics.
Every figure in this paper is reproducible from those two files.*
