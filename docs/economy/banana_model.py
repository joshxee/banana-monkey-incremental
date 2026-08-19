"""
Banana Incremental - consolidated economy model (MVP).

Every design decision from the review is baked in here:

  * production is payload / cycle_time, not count x rate
  * one speed multiplier applies to ALL travel, carts included (D6 deleted)
  * technologists produce no bananas and are never ranked against harvesters
  * harvesters are paid out of the delivery they just made, not continuously
  * support staff are post-paid too, on a bare 10-second shift (D19)
  * a harvester's meal is reserved out of the delivery that funds it (D20), so
    there is no wage reserve and no I1/I2 gate left to model
  * assignment is auto-pull: cart slots are always filled first
    -- NOTE: superseded by D8/D17's deletion; the cart increment owes this file
    a fixed crew of three and a cart snack. Until then `cart_cycle` is three
    addends where the game will have four, a 5% divergence in cart rate.

D19 changes no steady-state rate: a wage of 0.10/s is 0.10/s whether it is
drained continuously or eaten in 1.0-banana lumps every ten seconds. What it
changes is what happens when the larder runs dry, which is a transient this
model does not represent - it assumes every monkey is fed.

The `simulate` function models an OPTIMAL PLAYER. Its technologist policy is a
stand-in for human judgement, not a mechanic the game implements.
"""
from dataclasses import dataclass
import random


# ────────────────────────────────────────────────────────────── parameters

@dataclass
class Params:
    dist: float = 100.0            # metres to the grove, one way

    w_speed: float = 5.0
    w_payload: float = 5.0
    w_salary: float = 0.03
    w_snack: float = 0.05          # share of the trip spent eating at the stall
    w_cost_b: float = 4.0
    w_cost_r: float = 1.15

    k_speed: float = 15.0
    k_payload: float = 200.0
    k_crew: int = 3
    k_salary: float = 0.20
    k_cost_b: float = 70.0
    k_cost_r: float = 1.70
    k_tech_req: int = 1

    t_pick: float = 1.00           # sec per banana, at the grove
    t_unload: float = 0.50         # sec per banana, at the depot

    chef_bonus: float = 0.15       # nutrition -> movement speed
    c_salary: float = 0.10
    c_cost_b: float = 25.0
    c_cost_r: float = 1.30

    unpack_bonus: float = 0.20
    u_salary: float = 0.10
    u_cost_b: float = 30.0
    u_cost_r: float = 1.30

    tech_bonus: float = 0.10       # per tech LEVEL -> pick rate
    x_research: float = 1.0
    x_salary: float = 0.20
    x_cost_b: float = 40.0
    x_cost_r: float = 1.35
    tech_level_base: float = 60.0
    tech_level_growth: float = 2.2

    free_workers: int = 1


@dataclass
class State:
    W: int = 0
    A: int = 0
    C: int = 0
    U: int = 0
    X: int = 0
    K: int = 0
    research: float = 0.0


ATTR = {"worker": "W", "chef": "C", "unpacker": "U",
        "technologist": "X", "cart": "K"}
KINDS = tuple(ATTR)
HARVESTERS = ("worker", "chef", "unpacker", "cart")   # rankable by payback


def clone(s):
    return State(s.W, s.A, s.C, s.U, s.X, s.K, s.research)


# ────────────────────────────────────────────────────────────── multipliers

def m_speed(s, p):
    return 1.0 + s.C * p.chef_bonus


def m_unpack(s, p):
    return 1.0 + s.U * p.unpack_bonus


def tech_level(s, p):
    lvl, need, total = 0, p.tech_level_base, 0.0
    while s.research >= total + need:
        total += need
        lvl += 1
        need *= p.tech_level_growth
    return lvl


def m_tech(s, p):
    return 1.0 + tech_level(s, p) * p.tech_bonus


def research_rate(s, p):
    return s.X * p.x_research * m_speed(s, p)


# ───────────────────────────────────────────────────────────── cycle times

def worker_cycle(s, p):
    """Four addends. The fourth is the snack: the worker stops at the stall and
    eats its wage out of the load it has just handed over.

    A constant SHARE of the trip, not a fixed number of seconds. Fixing it would
    make eating an ever-larger slice of a shortened cycle, so chefs would raise
    the cost of labour per second and partly cancel themselves, and worker
    throughput would converge to payload/t_snack instead of to the pick-rate
    ceiling of §3. As a share it costs a flat 5% of that ceiling and nothing
    more, and `worker_meal` is defined against the whole cycle, so the published
    w_salary stays exactly 0.03/sec at every multiplier.
    """
    work = (2 * p.dist / (p.w_speed * m_speed(s, p)),
            p.w_payload * p.t_pick / m_tech(s, p),
            p.w_payload * p.t_unload / m_unpack(s, p))
    return work + (sum(work) * p.w_snack / (1.0 - p.w_snack),)


def worker_meal(s, p):
    """Bananas one worker eats per round trip."""
    return p.w_salary * sum(worker_cycle(s, p))


def cart_cycle(s, p):
    """Four addends, exactly like `worker_cycle`. Carts get the same speed
    multiplier as everyone else (D6 is deleted) and, since the cart increment,
    the same snack (D19/D20).

    Adding the snack closes the asymmetry D18 recorded as its one outstanding
    cost: workers converged 5% under the §3 ceiling while carts converged on
    it. Both now converge to (1 - snack) x m_tech / t_pick per crew monkey, so
    the ceiling theorem is uniform across harvest methods.

    It also makes the cart solvent by construction. Its meal is 37.9 bananas of
    a 200-banana payload, reserved out of that delivery - without which a cart
    that missed one would freeze holding the payload, and at an empty pool the
    treasury could never climb back to free it.
    """
    work = (2 * p.dist / (p.k_speed * m_speed(s, p)),
            p.k_payload * p.t_pick / (p.k_crew * m_tech(s, p)),
            p.k_payload * p.t_unload / m_unpack(s, p))
    return work + (sum(work) * p.w_snack / (1.0 - p.w_snack),)


def cart_meal(s, p):
    """Bananas one cart eats per round trip, crew included."""
    return p.k_salary * sum(cart_cycle(s, p))


def worker_throughput(s, p):
    return p.w_payload / sum(worker_cycle(s, p))


def cart_throughput(s, p):
    return p.k_payload / sum(cart_cycle(s, p))


def crewed(s, p):
    return min(s.A, s.K * p.k_crew)


# ─────────────────────────────────────────────────────────────── economy

def gross(s, p):
    """Expected rate. Pure function of counts and multipliers - no phase."""
    return ((s.W - s.A) * worker_throughput(s, p)
            + cart_throughput(s, p) * crewed(s, p) / p.k_crew)


def salary(s, p):
    return (s.W * p.w_salary + s.C * p.c_salary + s.U * p.u_salary
            + s.X * p.x_salary + p.k_salary * crewed(s, p) / p.k_crew)


def net(s, p):
    return gross(s, p) - salary(s, p)


def cost(kind, s, p):
    tbl = {"worker": (p.w_cost_b, p.w_cost_r, s.W),
           "chef": (p.c_cost_b, p.c_cost_r, s.C),
           "unpacker": (p.u_cost_b, p.u_cost_r, s.U),
           "technologist": (p.x_cost_b, p.x_cost_r, s.X),
           "cart": (p.k_cost_b, p.k_cost_r, s.K)}
    b, g, n = tbl[kind]
    return b * g ** n


def assign(s, p):
    """Auto-pull. Cart slots are strictly better than the pool at every
    reachable world state, so the optimal policy is simply to fill them."""
    s.A = min(s.W, s.K * p.k_crew)


def hypothetical(kind, s, p):
    h = clone(s)
    setattr(h, ATTR[kind], getattr(h, ATTR[kind]) + 1)
    assign(h, p)
    return h


def projected_delta(kind, s, p):
    return net(hypothetical(kind, s, p), p) - net(s, p)


def available(kind, s, p):
    return tech_level(s, p) >= p.k_tech_req if kind == "cart" else True


def _income_sources(s, p):
    """(gap between this source's deliveries, bananas/sec it contributes)."""
    out = []
    pool = s.W - s.A
    if pool > 0:
        # NET of the meal, not gross. `continuous_salary` already removes pool
        # wages from the drain being reserved against; reporting gross income
        # here applies the same D18 exemption a second time, on the other side
        # of the subtraction, and understates the reserve by exactly
        # 2 * pool * w_salary * gap. Measured at RUN n=84: 46.81 against a
        # correct 57.32.
        out.append((sum(worker_cycle(s, p)) / pool,
                    pool * (worker_throughput(s, p) - p.w_salary)))
    if s.K > 0:
        out.append((sum(cart_cycle(s, p)) / s.K,
                    cart_throughput(s, p) * crewed(s, p) / p.k_crew))
    return out


# `continuous_salary` and `wage_reserve` are DELETED (D15, D19).
#
# The reserve hedged a timing mismatch: wages fell due continuously while income
# arrived in lumps. D18 removed the mismatch for workers and D19 removes it for
# everyone, so there is nothing left to hedge. The reserve's own history is worth
# keeping in mind if a future unit is ever paid continuously again - it took two
# attempts to get right, and the near-miss (a *blended* mean gap) looked correct
# and under-reserved by 38.5 against a measured -66.0 dip.
#
# What replaces it is not a reserve but an encumbrance: see D20. A harvester's
# meal is held on the harvester between delivery and snack, and the shop spends
# `bananas - committed` rather than `bananas`. The quoted price does not move,
# which is the whole point.


def unfunded_salary(s, p):
    """The part of the wage bill that no delivery is holding money for.

    Every harvester's meal is now reserved out of the delivery it just made
    (D20), so neither the pool nor the carts draw on the general pot. What is
    left is the support staff, who have no delivery of their own and eat out of
    surplus (D19).

    Modelled as a continuous drain even though D19 pays support in 1.0-banana
    lumps every ten seconds. The rate is identical and the lumpiness only makes
    the measured dip slightly pessimistic, which is the safe direction.
    """
    return (salary(s, p)
            - (s.W - s.A) * p.w_salary
            - p.k_salary * crewed(s, p) / p.k_crew)


def committed(s, p):
    """Bananas reserved by harvesters against meals they have earned but not yet
    eaten (D20).

    Expected value across the field rather than an instantaneous sum, because
    this model has no per-entity phase: a unit spends `w_snack` of its cycle
    holding its reservation, so on average that fraction of the payroll is
    encumbered at any moment.
    """
    pool = s.W - s.A
    held = (pool * worker_meal(s, p)
            + (crewed(s, p) / p.k_crew) * cart_meal(s, p))
    return held * p.w_snack


def affordable(kind, bananas, s, p):
    """I5-prime, and nothing else.

    I1 (net stays positive) and I2 (only offer what pays) are both gone - see
    the architecture doc's invariant table. What is left is: can the player
    actually pay, out of money that is theirs to spend.
    """
    return bananas - committed(s, p) >= cost(kind, s, p)


# ──────────────────────────────────────────────────── optimal-player model

def offers(s, p):
    """Harvester offers, ranked by payback. Technologists are deliberately
    absent: their payoff is a step function and cannot be ranked here."""
    out = []
    for k in HARVESTERS:
        if not available(k, s, p):
            continue
        d = projected_delta(k, s, p)
        if d > 0:
            out.append((cost(k, s, p) / d, k, cost(k, s, p), d))
    out.sort()
    return out


def technologist_npv(s, p, horizon=600.0):
    """Stand-in for player judgement. NOT a game mechanic - the game shows the
    research track and lets the player decide."""
    h = hypothetical("technologist", s, p)
    if net(h, p) <= 0:
        return None
    dr = research_rate(h, p) - research_rate(s, p)
    if dr <= 0:
        return None
    lvl_cost = p.tech_level_base * p.tech_level_growth ** tech_level(s, p)
    probe = clone(s)
    probe.research = lvl_cost * 1.001 + sum(
        p.tech_level_base * p.tech_level_growth ** i
        for i in range(tech_level(s, p)))
    gain = net(probe, p) - net(s, p)
    for uk in KINDS:
        if not available(uk, s, p) and available(uk, probe, p):
            gain += max(projected_delta(uk, probe, p), 0.0)
    t_unlock = max(lvl_cost - s.research, 0.0) / (research_rate(s, p) + dr)
    equiv = gain * max(horizon - t_unlock, 0.0) / horizon - p.x_salary
    return cost("technologist", s, p) / equiv if equiv > 0 else None


def simulate(p, horizon=60 * 60, patience=150.0, max_buys=500, gated=True):
    s = State(W=p.free_workers)
    assign(s, p)
    bananas, t = 0.0, 0.0
    log = [dict(t=0.0, n=0, kind="start", W=s.W, A=s.A, C=s.C, U=s.U, X=s.X,
                K=s.K, lvl=0, research=0.0, net=net(s, p), gross=gross(s, p),
                salary=salary(s, p), cost=0.0, payback=0.0, wait=0.0)]

    for i in range(1, max_buys + 1):
        cand = list(offers(s, p))
        xp = technologist_npv(s, p)
        if xp is not None:
            cand.append((xp, "technologist", cost("technologist", s, p), 0.0))
        cand.sort()
        if not cand:
            break
        chosen = None
        for payback, kind, c, d in cand:
            # The I1 gate used to sit here and is deleted. An optimal player
            # still declines a purchase that inverts net, but that is judgement,
            # not a rule the game enforces - so it belongs in the policy, which
            # is what `cand` already is.
            gate = c + (committed(s, p) if gated else 0.0)
            if bananas >= gate:
                chosen = (payback, kind, c, 0.0)
                break
            r = net(s, p)
            if r <= 0:
                continue
            wait = (gate - bananas) / r
            if wait <= patience and t + wait <= horizon:
                chosen = (payback, kind, c, wait)
                break
        if chosen is None:
            break
        payback, kind, c, wait = chosen
        if wait > 0.0:
            s.research += research_rate(s, p) * wait
            t += wait
            bananas += net(s, p) * wait
        bananas -= c
        setattr(s, ATTR[kind], getattr(s, ATTR[kind]) + 1)
        assign(s, p)
        log.append(dict(t=t, n=i, kind=kind, W=s.W, A=s.A, C=s.C, U=s.U,
                        X=s.X, K=s.K, lvl=tech_level(s, p), research=s.research,
                        net=net(s, p),
                        gross=gross(s, p), salary=salary(s, p), cost=c,
                        payback=payback, wait=wait))
    return log, s


def session_end(log, patience_payback=900.0):
    """A multi-unit incremental has no stall - there is always something cheap
    left to buy. The session ends when every remaining purchase stops feeling
    worth it, i.e. when payback exceeds the player's patience horizon."""
    hit = next((e for e in log if e["n"] > 0 and e["payback"] > patience_payback), None)
    return (hit or log[-1])["t"], (hit or log[-1])["n"]


def world_at(log, n):
    """Reconstruct the exact world state after purchase n, research included."""
    e = next(x for x in log if x["n"] == n)
    return State(W=e["W"], A=e["A"], C=e["C"], U=e["U"], X=e["X"], K=e["K"],
                 research=e["research"])


# ──────────────────────────────────────────────── discrete-event validator

def discrete_run(s, p, duration=1200.0, dt=0.05, start=0.0,
                 jitter=True, seed=1):
    """Every harvester holds its own phase and delivers a payload at the end
    of its cycle. Returns (deepest treasury dip, realised rate)."""
    rng = random.Random(seed)
    Tw, Tk = sum(worker_cycle(s, p)), sum(cart_cycle(s, p))
    pool = s.W - s.A
    full_carts, partial_crew = divmod(crewed(s, p), p.k_crew)
    cart_payloads = [p.k_payload] * full_carts
    if partial_crew:
        # D8: cycle segments retain their nominal work and duration. The crew
        # fraction sampled at cycle start scales settlement only.
        cart_payloads.append(p.k_payload * partial_crew / p.k_crew)
    wph = [rng.uniform(0, Tw) if jitter else 0.0 for _ in range(pool)]
    kph = [rng.uniform(0, Tk) if jitter else 0.0 for _ in cart_payloads]

    bananas, lo, t, delivered = start, start, 0.0, 0.0
    # Only the units that are not fed by a delivery drain continuously.
    drain = unfunded_salary(s, p)
    # A worker unloads at Tw - w_snack and eats at Tw, so the two events are
    # separated on the clock even though they belong to the same trip.
    unload_at, w_meal = Tw - worker_cycle(s, p)[3], worker_meal(s, p)
    # A cart unloads at Tk - snack and eats at Tk, exactly like a worker.
    k_unload_at, k_meal = Tk - cart_cycle(s, p)[3], cart_meal(s, p)
    while t < duration:
        bananas -= drain * dt
        for i in range(pool):
            was = wph[i]
            wph[i] += dt
            if was < unload_at <= wph[i]:
                bananas += p.w_payload
                delivered += p.w_payload
            if wph[i] >= Tw:
                wph[i] -= Tw
                bananas -= w_meal
        for i, payload in enumerate(cart_payloads):
            was = kph[i]
            kph[i] += dt
            if was < k_unload_at <= kph[i]:
                bananas += payload
                delivered += payload
            if kph[i] >= Tk:
                kph[i] -= Tk
                bananas -= k_meal
        lo = min(lo, bananas)
        t += dt
    return lo, delivered / duration


def cart_delivery_trace(s, p, staffing_changes=(), deliveries=2):
    """D8/D16 transition oracle for one cart.

    The initial crew is assigned before the spawned cycle is initialized.
    Later staffing changes affect the cycle after the next delivery.
    """
    assert s.K == 1
    cycle_time = sum(cart_cycle(s, p))
    staffed = crewed(s, p)
    delivery_scale = staffed / p.k_crew
    changes = iter(sorted(staffing_changes))
    change = next(changes, None)
    trace = []
    now = 0.0

    for _ in range(deliveries):
        cycle_end = now + cycle_time
        while change is not None and change[0] <= cycle_end:
            staffed = min(change[1], p.k_crew)
            change = next(changes, None)
        trace.append((cycle_end, p.k_payload * delivery_scale))
        delivery_scale = staffed / p.k_crew
        now = cycle_end
    return trace
