# Link names — the namespace model

Every link name is one or more EFF-short words in alternating case (`braveOTTER`).
Names are case-insensitive; the casing is only a visual word boundary. The word
list is `core/src/words.txt` — **3,517** curated words (≤6 characters, "anyone can
use this": no slurs, brands, clinical, or hard-to-spell words). All tiers draw from
the whole list, so the `k`-word namespace is `3517^k`.

| Words | Namespace | Entropy |
|------:|----------:|--------:|
| 1 | 3,517 | 11.8 bits |
| 2 | 12.4 M | 23.6 bits |
| 3 | 43.5 B | 35.3 bits |
| 4 | 153 T | 47.1 bits |

## What the word count is for

Length is **not** sold as privacy except at four words. The dial does two jobs:

- **Unguessability (privacy).** Only a **4-word** name is unguessable enough to stand
  on its own as a secret (~47 bits — a sustained 10⁴ req/s botnet has ~1-in-25,000
  odds over a full 7-day life). So **Private** and **One-time** links are always 4
  words. A 1–3-word name is never called private in the UI.
- **Availability.** Public links guard nothing, so their length is chosen purely to
  keep short names *available*: the shortest tier that is not over-subscribed.

## Public allocation: occupancy + TTL

A public link gets the **shortest tier whose live occupancy is under a ceiling that
depends on its TTL**. Shorter-lived links recycle their names quickly, so they get
priority on the scarce short tiers; longer-lived links yield to a longer name sooner.

TTL bands and the 1-word-tier occupancy at which each escalates:

| 1w occupancy | ≤1h TTL | ≤2d TTL | ≤7d TTL |
|---|---|---|---|
| < 40% | 1w | 1w | 1w |
| 40–60% | 1w | 1w | 2w |
| 60–90% | 1w | 2w | 2w |
| ≥ 90% | 2w | 2w | 2w |

The same shape governs 2w→3w and 3w→4w (which need billions of live links to ever
trigger, so public names top out at three in practice; the code still escalates to
four if it must). When occupancy bumps a public link above one word, the result page
shows a note explaining the short names are in demand.

Occupancy is the live count per tier (`words` column), recomputed by the reaper each
sweep and read by the create path. Between refreshes, the per-create
grow-on-collision valve still resolves any tier that filled in the meantime.

## Defenses (deliberately *not* per-request rate limits)

- **Privacy needs no rate limit** — 4-word entropy is self-sufficient.
- **Volumetric DoS** belongs **upstream** (a CDN); a 1-vCPU box can't absorb a flood
  in-app, and per-request latency injection doesn't slow a concurrent attacker.
- **1-word namespace squatting** is bounded by short TTLs (names churn back fast),
  the occupancy ladder (heavy creation just lengthens everyone's names, never denies
  one), and — when needed — a **create-path** rate limit (fast 429, never a delay).
  Resolution is never rate-limited: a 3,517-name tier can't be hidden anyway.
