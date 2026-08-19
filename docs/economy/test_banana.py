"""
Contract tests for the Banana Incremental economy.

Written against the Python model; port each to Bevy. The names are the
contract - if one of these fails after a tuning change, a unit type has
silently died or an invariant has drifted.
"""
import math
from collections import Counter
from banana_model import (
    Params, State, KINDS, HARVESTERS, clone, assign, hypothetical,
    worker_cycle, cart_cycle, worker_throughput, cart_throughput, crewed,
    gross, salary, net, cost, projected_delta, available, committed,
    unfunded_salary,
    affordable, offers, simulate, discrete_run, cart_delivery_trace,
    tech_level, m_tech, session_end, world_at,
    m_speed, m_unpack, research_rate)

P = Params()
RUN, END = simulate(P)
FAIL = []


def check(name, cond, detail=""):
    if not cond:
        FAIL.append(name)
    print(f"[{'PASS' if cond else 'FAIL'}] {name}"
          + (f"   -- {detail}" if detail and not cond else ""))


# ─────────────────────────────────────────────────────────────── invariants

# I1 and I2 are deleted, and their tests with them. The pair used to assert that
# net stayed positive after every purchase and that no negative-delta offer was
# ever shown. Neither survives D19: an unfed monkey stops contributing to its
# multiplier, so "net" at a given world state depends on who is currently eating,
# and the gate had no way to name which state it meant. What replaced them is
# I5-prime, below - the player can always pay, out of money that is theirs.


def t_I5_purchase_needs_unencumbered_bananas():
    """A banana a monkey has already earned is not available to spend."""
    s = world_at(RUN, 24)
    held = committed(s, P)
    price = cost("chef", s, P)
    check("I5   a purchase clears against the unencumbered balance",
          affordable("chef", price + held, s, P)
          and not affordable("chef", price + held - 0.01, s, P),
          f"price {price:.1f}, committed {held:.2f}")


def t_I3_rate_is_pure_in_counts():
    """Two worlds with identical counts must report identical rate - if phase
    ever leaks into the rate calculation, this breaks."""
    a = State(W=20, C=5, U=3, X=2, K=4, research=5000)
    b = State(W=20, C=5, U=3, X=2, K=4, research=5000)
    assign(a, P)
    assign(b, P)
    check("I3'  expected rate is a pure function of counts",
          gross(a, P) == gross(b, P))


def t_I3_no_drift_under_incremental_growth():
    s, incremental = State(W=0), 0.0
    for _ in range(150):
        before = gross(s, P)
        s.W += 1
        assign(s, P)
        incremental += gross(s, P) - before
    check("I3'  derived rate == incrementally accumulated rate",
          math.isclose(gross(s, P), incremental, rel_tol=1e-9))


def t_I4_net_identity():
    bad = [e for e in RUN
           if not math.isclose(e["net"], e["gross"] - e["salary"], rel_tol=1e-12)]
    check("I4   net == gross - salary in every observed state", not bad)


# ─────────────────────────────────────────────────────────────── bootstrap

def t_empty_world_is_dead():
    check("BOOT empty world produces nothing (seed grant is mandatory)",
          gross(State(), P) == 0.0)


def t_seeded_world_is_alive():
    s = State(W=P.free_workers)
    assign(s, P)
    check("BOOT seeded world has strictly positive net", net(s, P) > 0,
          f"net={net(s, P):.4f}")


# ──────────────────────────────────────────────────────────── cycle physics

def t_cycle_segments_are_positive_and_finite():
    """Four addends each: travel, pick, unload, and the snack eaten at the stall
    out of the delivery just made. The cart used to have three, which is the
    asymmetry D18 recorded and the cart increment closed."""
    s = State(W=10, C=3, U=2)
    for name, cyc, want in (("worker", worker_cycle(s, P), 4),
                            ("cart", cart_cycle(s, P), 4)):
        check(f"CYC  {name} has {want} positive finite segments",
              len(cyc) == want and all(math.isfinite(x) and x > 0 for x in cyc))


def t_worker_is_travel_bound_cart_is_unload_bound():
    s = State(W=1)
    w, k = worker_cycle(s, P), cart_cycle(s, P)
    check("CYC  worker cycle is travel-dominated", w[0] / sum(w) > 0.6,
          f"{100*w[0]/sum(w):.0f}%")
    check("CYC  cart cycle is unload-dominated", k[2] / sum(k) > 0.4,
          f"{100*k[2]/sum(k):.0f}%")


def t_support_asymmetry_is_real():
    """The whole design rests on chefs and unpackers helping different units."""
    base = State(W=1)
    chefs, unpackers = State(W=1, C=10), State(W=1, U=10)
    wc = worker_throughput(chefs, P) / worker_throughput(base, P)
    kc = cart_throughput(chefs, P) / cart_throughput(base, P)
    wu = worker_throughput(unpackers, P) / worker_throughput(base, P)
    ku = cart_throughput(unpackers, P) / cart_throughput(base, P)
    check("CYC  chefs help workers far more than carts", wc > 3 * (kc - 1) + 1,
          f"worker x{wc:.2f} vs cart x{kc:.2f}")
    check("CYC  unpackers help carts far more than workers", ku > 3 * (wu - 1) + 1,
          f"cart x{ku:.2f} vs worker x{wu:.2f}")


def t_ceiling_is_one_over_pick_time():
    """With travel and unload driven to zero, every harvest method converges
    to the same per-monkey throughput. This bounds the whole game."""
    s = State(W=1, C=10**6, U=10**6)
    ceil = m_tech(s, P) / P.t_pick
    # Every harvester keeps a fixed share of every trip for itself, so they all
    # converge to a flat 5% under the ceiling rather than to the ceiling. The
    # theorem's content survives because the share is constant - and since the
    # cart increment gave carts the same snack, it now bounds every harvest
    # method *equally*, which is what D18 recorded as owed.
    target = ceil * (1 - P.w_snack)
    check("CEIL worker throughput converges to (1-snack) x m_tech/t_pick",
          math.isclose(worker_throughput(s, P), target, rel_tol=1e-3),
          f"{worker_throughput(s, P):.4f} vs {target:.4f}")
    check("CEIL cart per-crew throughput converges to the same ceiling",
          math.isclose(cart_throughput(s, P) / P.k_crew, target, rel_tol=1e-3),
          f"{cart_throughput(s, P)/P.k_crew:.4f} vs {target:.4f}")
    check("CEIL both harvest methods converge to the same number",
          math.isclose(worker_throughput(s, P), cart_throughput(s, P) / P.k_crew,
                       rel_tol=1e-3),
          "the D18 asymmetry is closed")


def t_ceiling_is_not_binding_in_mvp():
    s = world_at(RUN, session_end(RUN)[1])
    ceil = m_tech(s, P) / P.t_pick
    frac = cart_throughput(s, P) / P.k_crew / ceil
    check("CEIL MVP run stays well below the ceiling", frac < 0.8,
          f"carts at {100*frac:.0f}% of ceiling")


# ──────────────────────────────────────────────────────────────── staffing

def t_auto_pull_fills_every_slot():
    bad = [e for e in RUN if e["A"] != min(e["W"], e["K"] * P.k_crew)]
    check("STAF auto-pull keeps every cart slot filled", not bad)


def t_understaffed_cart_is_proportional():
    one = State(W=1, A=1, K=1)
    full = State(W=P.k_crew, A=P.k_crew, K=1)
    part = gross(one, P)
    check("STAF a 1-of-3 crewed cart produces exactly one third",
          math.isclose(part, gross(full, P) / P.k_crew, rel_tol=1e-12),
          f"{part:.4f} vs {gross(full, P)/P.k_crew:.4f}")


def t_understaffed_cart_snapshots_scale_at_cycle_start():
    partial = State(W=1, A=1, K=1)
    full = State(W=P.k_crew, A=P.k_crew, K=1)
    _, realised = discrete_run(partial, P, duration=6000.0)
    same_segments = cart_cycle(partial, P) == cart_cycle(full, P)
    cycle_time = sum(cart_cycle(partial, P))
    initial = cart_delivery_trace(full, P, deliveries=1)
    changed = cart_delivery_trace(
        partial, P, staffing_changes=[(cycle_time / 2, P.k_crew)])
    payloads = [payload for _, payload in changed]
    expected = [P.k_payload / P.k_crew, P.k_payload]
    transition_ok = (
        math.isclose(initial[0][1], P.k_payload, rel_tol=1e-12)
        and all(math.isclose(a, b, rel_tol=1e-12)
                for a, b in zip(payloads, expected)))
    check("STAF crew scale snapshots after spawn assignment and per cycle",
          same_segments
          and math.isclose(realised, gross(partial, P), rel_tol=0.05)
          and transition_ok,
          f"same_segments={same_segments}, realised={realised:.4f}, "
          f"expected={gross(partial, P):.4f}, payloads={payloads}")


def t_crew_never_exceeds_capacity():
    check("STAF crewed count clamps to cart capacity",
          crewed(State(W=100, A=100, K=2), P) == 2 * P.k_crew)


# ─────────────────────────────────────────────────────────── technologists

def t_technologist_delta_is_constant_and_negative():
    """The reason technologists cannot be ranked: their banana delta carries
    no information about the world at all."""
    deltas = {round(projected_delta("technologist", s, P), 9)
              for s in (State(W=1), State(W=20, C=5),
                        State(W=60, C=20, U=15, K=5, A=15))}
    check("TECH banana delta is exactly -salary at every world state",
          deltas == {round(-P.x_salary, 9)}, str(deltas))


def t_technologist_is_never_ranked():
    s = State(W=30, C=5, U=5, X=2, K=3, A=9, research=1e6)
    check("TECH technologist never appears in the ranked offer list",
          all(k != "technologist" for _, k, _, _ in offers(s, P)))


def t_carts_are_gated_behind_research():
    check("TECH carts unavailable at tech level 0",
          not available("cart", State(W=50), P))
    unlocked = State(W=50, research=P.tech_level_base * 1.01)
    check("TECH carts available once the first level lands",
          available("cart", unlocked, P))


def t_chefs_accelerate_research():
    a, b = State(X=3), State(X=3, C=8)
    check("TECH chefs raise the research rate", research_rate(b, P) > research_rate(a, P),
          f"{research_rate(a, P):.2f} -> {research_rate(b, P):.2f}")


# ─────────────────────────────────────────────────────────── spiky delivery

def t_workers_alone_never_dip_from_a_cold_start():
    """The reserve existed to cover a dip. D20 removes the dip instead: a
    worker's meal is reserved out of the delivery that funds it, so nothing
    can spend it in between and the treasury cannot fall below where it stood
    before that delivery landed. From zero bananas, with any workforce."""
    for w in (1, 4, 15, 40):
        s = State(W=w)
        lo, _ = discrete_run(s, P)
        check(f"SPIK {w} workers from zero never go underwater",
              lo >= -1e-9, f"dipped to {lo:.4f}")


def t_support_drains_but_only_at_its_own_wage():
    """Support staff have no delivery of their own, so they *do* draw the
    treasury down between harvests - that is D19's deal, and starving is how it
    resolves. What must not happen is support reaching a worker's reserved
    meal: the dip has to be explained entirely by the support payroll, with
    nothing extra leaking out of the harvest cycle.

    The counterfactual is the test. Deducting the same payroll from a
    worker-only run has to produce the same floor; if the harvest cycle were
    leaking, the real run would be deeper."""
    s = world_at(RUN, 24)
    s.K, s.A = 0, 0                                 # workers and support only
    lo, _ = discrete_run(s, P)

    bare = State(W=s.W)
    bare_lo, _ = discrete_run(bare, P)
    # The longest a worker delivery can be away, which is the whole exposure.
    gap = sum(worker_cycle(s, P)) / max(s.W - s.A, 1)
    payroll = unfunded_salary(s, P) * (gap + sum(worker_cycle(s, P)))

    check("SPIK the dip is the support payroll and nothing more",
          bare_lo - payroll <= lo <= 0.0,
          f"dipped to {lo:.2f}, payroll bound {bare_lo - payroll:.2f}")


def t_the_remaining_dip_is_exactly_what_the_carts_owe():
    """Carts still drain continuously, so they still dip. This is the one
    outstanding cost of the support increment, and it closes when the cart
    increment gives carts the same snack every other harvester has.

    Bounded by the unfunded wage bill across the longest gap between
    deliveries - which is the old reserve formula, now used as a *description*
    of a known divergence rather than as a gate the player pays for."""
    for s in [world_at(RUN, 24), world_at(RUN, 50)]:
        lo, _ = discrete_run(s, P)
        gap = sum(cart_cycle(s, P)) / max(s.K, 1)
        bound = unfunded_salary(s, P) * gap * 2.0
        check(f"SPIK the dip at K={s.K} is bounded by the unfunded wage bill",
              -bound <= lo <= 0.0,
              f"dipped to {lo:.1f}, bound {-bound:.1f}")


def t_reserve_does_not_strangle_pacing():
    ungated, _ = simulate(P, gated=False)
    check("SPIK the reserve gate costs little pacing",
          abs(RUN[-1]["n"] - ungated[-1]["n"]) <= 2,
          f"gated {RUN[-1]['n']} buys vs ungated {ungated[-1]['n']}")


def t_synced_carts_are_materially_worse():
    s = world_at(RUN, 50)
    lo_j, _ = discrete_run(s, P, jitter=True)
    lo_s, _ = discrete_run(s, P, jitter=False)
    check("SPIK synced cart phases deepen the dip several-fold",
          lo_s < 2 * lo_j, f"jittered {lo_j:.1f} vs synced {lo_s:.1f}")


def t_realised_rate_converges_to_expected():
    s = world_at(RUN, 50)
    _, realised = discrete_run(s, P, duration=6000.0)
    check("SPIK realised delivery rate converges to the expected rate",
          math.isclose(realised, gross(s, P), rel_tol=0.05),
          f"realised {realised:.3f} vs expected {gross(s, P):.3f}")


# ───────────────────────────────────────────────────────────────── content

def t_no_unit_type_is_dead():
    mix = Counter(e["kind"] for e in RUN if e["n"] > 0)
    check("LIVE all five unit types are purchased in a full run",
          set(mix) == set(KINDS), f"missing {set(KINDS) - set(mix)}")


def t_chefs_survive_d6_deletion():
    """Regression guard. Exempting carts from the speed multiplier - the old
    D6 - starves chefs of value and removes them from the game entirely."""
    mix = Counter(e["kind"] for e in RUN if e["n"] > 0)
    check("LIVE chefs are purchased (D6-deletion regression guard)",
          mix["chef"] >= 3, f"only {mix['chef']} chefs")


def t_priority_rotates():
    seq = [e["kind"] for e in RUN if e["n"] > 0]
    switches = sum(1 for a, b in zip(seq, seq[1:]) if a != b)
    check("LIVE purchase priority rotates between types",
          switches >= 15, f"{switches} switches")


def t_bottleneck_moves_during_the_run():
    """Amdahl doing its job: the dominant cart segment should change."""
    e, l = cart_cycle(world_at(RUN, 14), P), cart_cycle(world_at(RUN, 50), P)
    check("LIVE the dominant cart segment shifts across the run",
          e.index(max(e)) != l.index(max(l)),
          f"early {e.index(max(e))} late {l.index(max(l))}")


# ──────────────────────────────────────────────────────────── pacing & math

def t_run_length_in_band():
    t_end, n_end = session_end(RUN)
    check("PACE session runs 20-30 minutes at a 15-minute patience horizon",
          20 * 60 <= t_end <= 30 * 60, f"{t_end/60:.1f} min at n={n_end}")


def t_costs_outrun_production():
    early = max(e["payback"] for e in RUN if 0 < e["n"] <= 8)
    late = max(e["payback"] for e in RUN if e["n"] > RUN[-1]["n"] - 8)
    check("PACE payback time grows by orders of magnitude",
          late > 20 * early, f"{early:.0f}s -> {late:.0f}s")


def t_determinism():
    a, _ = simulate(P)
    b, _ = simulate(P)
    check("NUM  simulation is deterministic", [e["t"] for e in a] == [e["t"] for e in b])


def t_f64_headroom():
    ratio = max(e["cost"] for e in RUN) / min(e["net"] for e in RUN if e["net"] > 0)
    check("NUM  treasury/net ratio stays far below 2^53",
          ratio < 2 ** 53 / 1e6, f"{ratio:.2e}")


if __name__ == "__main__":
    for fn in list(globals().values()):
        if callable(fn) and getattr(fn, "__name__", "").startswith("t_"):
            fn()
    print()
    print("ALL PASS" if not FAIL else f"{len(FAIL)} FAILURES: " + ", ".join(FAIL))
