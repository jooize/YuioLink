# Link names — the namespace model

Every link name is one or more EFF-short words in alternating case (`braveOTTER`).
Names are case-insensitive; the casing is only a visual word boundary. The word
list is `core/src/words.txt` — **3,456** curated words (≤6 characters, "anyone can
use this": no slurs, brands, clinical, or hard-to-spell words). All tiers draw from
the whole list, so the `k`-word namespace is `3456^k`.

| Words | Namespace | Entropy |
|------:|----------:|--------:|
| 1 | 3,456 | 11.8 bits |
| 2 | 11.9 M | 23.5 bits |
| 3 | 41.3 B | 35.3 bits |
| 4 | 143 T | 47.0 bits |

## One name, one spelling

A lowercased name does not carry its word boundaries, and the list is not a
uniquely decodable code: twelve words are themselves two words joined (`carpet`
= `car` + `pet`), and words can re-split across a boundary (`cart`+`one` and
`car`+`tone` both spell `cartone`). Left alone, that would let a name drawn for
its 47 bits also be spelled in three words (~1 in 315,000) or two (144 names of
the 143 trillion) — reachable by walking an 11.9-million-name space instead.

So `generate_name` re-rolls any draw with more than one spelling, exactly as it
re-rolls a reserved name. The invariant: **an issued name spells exactly one
word sequence, and it is the tier it was drawn from.** No name is ever reachable
from a cheaper tier. Rejection sampling stays uniform over what it accepts, so
the cost is only what it removes — about 0.05% of four-word names, 0.0007 bits,
against ~0.02 bits of margin in the 47-bit claim.

Uniqueness of the *name* was never at risk (`links.name` is `COLLATE NOCASE
UNIQUE`, so an ambiguous draw was always an insert collision, not a hijack); what
this buys is that word count and guessing cost cannot come apart.

## What the word count is for

Length is **not** sold as privacy except at four words. The dial does two jobs:

- **Unguessability (privacy).** Only a **4-word** name is unguessable enough to stand
  on its own as a secret (~47 bits — a sustained 10⁴ req/s botnet has ~1-in-25,000
  odds over a full 7-day life). So **Secret** and **One-time** links are always 4
  words. A 1–3-word name is never called secret in the UI.
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
  Resolution is never rate-limited: a 3,456-name tier can't be hidden anyway.
