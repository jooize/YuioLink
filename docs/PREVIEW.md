# Link preview, trust model, and share cards — implementation plan

Status: **designed, not yet built** (2026-06-23). Visual reference:
`design/preview-and-cards.html`. Background/decisions: see the `preview-and-trust-design`
and `encryption-threat-model` auto-memories. This document is the spec to implement from.

## Goal

YuioLink names are short, case-insensitive words that **recycle** after a link expires.
A blind redirect therefore can't be trusted (the same name may point somewhere new later,
and prefetch bots can burn one-time links). The fix is a mandatory **preview interstitial**
plus a **tombstone trust model**, so a recipient can always see where a link goes and a
link can never be silently repurposed within its life.

Scope for v1: **plaintext redirect + text links only**. Encryption is dropped (see memory);
a "Secret" type is deferred. Do not build crypto, the encrypt toggle, or a Secret type.

---

## 1. Behavioural model

- **Every redirect shows an interstitial first.** `GET /:name` returns a 200 preview page
  and spends **no** use. Non-bypassable by the creator (a creator who could skip it is the
  phishing case).
- **Unlimited text opens immediately** (no interstitial — there is no external destination
  to vet). **Limited/one-time text** gets the interstitial (to gate the use).
- **Consuming is always a POST that 303-redirects** (Post/Redirect/Get). This keeps the
  back button clean (no "resubmit form?") and means link-unfurl crawlers — which only GET —
  cannot spend a use.
- **Buttons by action:** blue = **Reveal** (shows destination/text, you stay on YuioLink);
  amber (dark text) = **Continue/Go** (you leave to the external site).

### Flows

Unlimited redirect (one step):
```
GET  /:name            -> interstitial: full syntax-highlighted URL + amber "Continue to <domain>"
POST /:name/go         -> consume (hits+1) -> 303 to the destination
```

Limited / one-time redirect (two steps — the full URL is gated behind a use):
```
GET  /:name            -> interstitial: DOMAIN ONLY + blue "Reveal Destination"
POST /:name/reveal     -> consume (hits+1) -> 303 to GET /:name/revealed?t=<token>
GET  /:name/revealed?t -> verify token -> full URL + amber "Continue to <domain>"
                          ("Continue" is a plain link to the destination; going is free,
                          the use was already spent at reveal)
```

Unlimited text (no interstitial):
```
GET  /:name            -> render the text immediately (inert <pre>); counts a hit
```

Limited / one-time text:
```
GET  /:name            -> interstitial: "A text snippet" + blue "Reveal Text"
POST /:name/reveal     -> consume -> 303 to GET /:name/revealed?t=<token> -> renders the text
```

Why reveal must consume: domain-only for a limited link means the **exact destination is
the gated resource**. If reveal didn't consume, anyone (or a script) could read the full URL
without spending a use, making the limit meaningless. Reveal is a POST so crawlers can't
trigger it.

---

## 2. Trust model: tombstone + immutability

Guarantee: **a link's destination is immutable, and its name is reserved until expiry.**
What the preview shows is what the link is, for its whole stated life. It can degrade to
"gone", but never silently become a **different** live destination.

HTTP semantics for `GET /:name` (and the POST consume endpoints):

| State | Condition | Response |
|-------|-----------|----------|
| Live | not past `expires_at`, uses left | 200 interstitial (or immediate text) / 303 on consume |
| Used up | `hits >= max_uses`, not expired | **410 Gone** (tombstone page) |
| Withdrawn | deleted by creator, not expired | **410 Gone** (tombstone page) |
| Expired / never existed / recycled | reaped or unknown name | **404 Not Found** |

- **410 vs 404 matters:** 410 says "this was a real link, now spent/withdrawn" (this is the
  "someone already opened it" signal for one-time links); 404 says "nothing here". Don't
  conflate them.
- **Tombstone = don't hard-delete early.** Exhausted links already persist as rows until the
  reaper deletes them at expiry, so they are tombstones for free. **Withdraw (creator
  delete) must stop resolving but NOT free the name** — mark the row instead of deleting it.
- **Only the clock frees a name.** `reap_expired` (DELETE WHERE expires_at <= now) stays as
  is; that is the single path that recycles a name. After expiry, always-preview protects
  the next clicker.
- **Immutability** is already true (content is set at insert, never updated). Keep it that
  way — do not add a destination-edit path.

### DB changes (`server/src/db.rs`)

- Add column `withdrawn INTEGER NOT NULL DEFAULT 0` to `links` (migration).
- `delete_link`: change from `DELETE` to `UPDATE links SET withdrawn = 1 WHERE name = ? AND
  delete_token = ?` (keeps the name reserved as a tombstone until expiry).
- `LIVE_PREDICATE`: add `AND withdrawn = 0`.
- Add `get_link_any(name)` — SELECT regardless of LIVE/withdrawn — so the resolver can
  classify 410 vs 404 and the revealed page can read content from a tombstone row.
- `consume_link` stays (UPDATE hits+1 WHERE LIVE RETURNING). It already returns None for
  dead links; the caller then uses `get_link_any` to choose 410 vs 404.
- `reap_expired` unchanged.

---

## 3. Routes (`server/src/main.rs`)

Replace the current `/:name` handling:
```
.route("/:name", get(web::resolve))                 // -> interstitial / immediate text / 410 / 404
.route("/:name/go", post(web::go))                  // unlimited redirect: consume + 303
.route("/:name/reveal", post(web::reveal))          // limited redirect/text: consume + 303 to revealed
.route("/:name/revealed", get(web::revealed))       // token-gated revealed view
```
- **Drop the old `+` preview convention** (`name.strip_suffix('+')` in `resolve`). `GET /:name`
  is now the preview.
- API under `/api/v1`: read still does not consume. **Remove the open `CorsLayer`** (decided
  2026-06-23) so the API is **same-origin** — the "host your own browser frontend" rationale
  died with encryption. See Open Question on gating limited destinations (CORS does not solve
  it for non-browser clients).

### Revealed-view token

`POST /:name/reveal` consumes one use, then mints a signed, short-TTL token and 303s to
`GET /:name/revealed?t=<token>`. The token authorises **re-rendering without re-consuming**
(so refresh/back is safe). Recommended: stateless **HMAC** token = `base64(name | exp) .
hmac_sha256(secret, name | exp)`, TTL ~10 min. `revealed` verifies the token, then reads the
row via `get_link_any` (the tombstone row still holds the content) and renders:
- redirect -> full URL + amber "Continue to <domain>" (plain link to the destination)
- text -> the text in an inert `<pre>`

Add an HMAC secret to config (`YUIOLINK_SECRET`, random per-process default if unset; note
that an unset/rotating secret invalidates outstanding reveal tokens, which is acceptable).

---

## 4. Views (`server/src/views.rs`)

New / changed:
- `interstitial_page` — source link, blue down-arrow, destination. Redirect-unlimited: full
  **syntax-highlighted** URL (registrable domain highlighted). Redirect-limited: domain only.
  Text-limited: "A text snippet". Meta = **relative expiry only** (no created/hits/uses); a
  neutral pill for limited (`Limited Use` / `Opens Once`); the recycling caution with
  **"Always check the destination."** on its own line. Button: amber Continue (unlimited) or
  blue Reveal (limited/text). Masthead `<h1>` links home (drop "Back to YuioLink").
- `revealed_page` — full URL + amber Continue (redirect) or the text (text).
- `gone_page` (410) — "This link has been used or withdrawn" tombstone, expiry-aware,
  prominent "Create a new link" home CTA.
- `not_found_page` (404) — "This link has expired or never existed — links on YuioLink are
  ephemeral", home CTA. Frame expiry as by-design, not a crash.
- Keep `text_view_page` (unlimited text, immediate). Keep generic 500 `error_page` terse.
- **Remove** `encrypted_redirect_page` and `encrypted_text_page` (encryption dropped).

### Syntax-highlighted URL + IDN handling

Render the URL in parts (scheme / `://` / subdomain / **registrable domain** / path / query),
delimiters in accent, registrable domain highlighted. For the host:
- Decode punycode with the `idna` crate (UTS #46).
- Classify with `unicode-security` (UTS #39): **single-script** label (incl. Latin +
  diacritics, all-Cyrillic, CJK, ...) = legit -> show decoded Unicode, **no warning**.
  **Mixed-script or confusable** = deceptive -> show the **punycode** form in the URL, a red
  (`--danger`) panel ("Lookalike domain. Domain uses special characters that can deceptively
  imitate another name." + `displays as <decoded>` / `real address <xn--...>`), and button
  text **"Continue Anyway"** (do not print the deceptive domain on the button).
- Crates: `idna`, `unicode-security` (+ maybe `unicode-script`).

---

## 5. Share cards (Open Graph + og:image)

On the interstitial `GET /:name` response `<head>`, emit (plaintext links only, and the
card **always shows the domain** — decided, for trustworthiness):
- `og:site_name` = YuioLink
- `og:title` = e.g. "Redirect to example.com" / "One-time link to acme.co"
- `og:description` = prose, e.g. "Ephemeral redirect that expires Jun 29, 2026 and may
  change after." (one-time: "Single-use redirect that expires ...")
- `og:image` = absolute URL to the card endpoint; `twitter:card` = summary_large_image
- `theme-color` = brand blue (Discord left-rail)

og:image endpoint: `GET /:name/card.png` — render the shared card (brand, kicker
"Ephemeral redirect"/"One-time redirect", destination domain, foot "expires <date, year> ·
may change after"). **No use consumed** (crawlers fetch it). Build an SVG from a template
and rasterise to PNG with `resvg` + `tiny-skia`; serve `image/png`. Consider caching the
PNG per name until expiry. The visual spec is the `.ogcard` design in the mockup. Cards are
identical across platforms — iMessage shows image + title + source domain (`yuio.link`);
Discord shows the theme-color rail + plain title + prose description + image; we only feed
the og tags, the chrome is platform-controlled.

---

## 6. CSS

Port the mockup's interstitial styles into `server/static/app.css` (the `pv-*` classes,
`.btn--go` amber button with dark text, the `.pv-idn` red lookalike panel, the neutral
`.pv-badge`). The `.ogcard` styles are for the **server-side SVG template**, not app.css.
Remove `crypto.js` / `redirect.js` references (encryption gone); keep `text.js` (copy).

---

## 7. Implementation order

1. **DB**: `withdrawn` column + migration; `LIVE_PREDICATE += withdrawn = 0`; `get_link_any`;
   `delete_link` -> tombstone UPDATE. Reaper unchanged.
2. **Resolve refactor**: `GET /:name` classifies live / exhausted / withdrawn / expired /
   missing -> interstitial / immediate-text / 410 / 404. Drop `+` convention.
3. **Interstitial view** (redirect unlimited full-URL, limited domain-only, text-limited) +
   IDN detection + syntax highlighting.
4. **Consume endpoints**: `POST /:name/go` (unlimited) and `POST /:name/reveal` (limited) +
   signed reveal token + `GET /:name/revealed`.
5. **404 / 410 pages** (friendly, expiry-aware); split from generic 500.
6. **OG tags** on the interstitial + **og:image** endpoint (resvg).
7. **Port CSS** to app.css; drop encryption views/assets.
8. **Tests** (see below).

---

## 8. Tests

- Resolve classification: live -> 200 interstitial; exhausted -> 410; withdrawn -> 410;
  expired/missing -> 404.
- Preview spends no use; `POST /go` and `POST /reveal` each spend exactly one; one-time link
  is gone (410) after one consume.
- PRG: consume returns 303; revealed page is a GET and re-rendering it does not consume.
- Reveal token: valid renders; tampered/expired token -> 404/expired.
- Withdraw (API delete) -> 410 and the name cannot be re-registered until expiry.
- IDN: mixed-script host -> warning + punycode; single-script IDN -> decoded, no warning;
  ASCII -> no warning.
- Crawler simulation: a GET to `/:name` and `/:name/card.png` never changes `hits`.

---

## 9. Open questions / decisions to confirm during build

- **API limited-destination leak.** `GET /api/v1/links/:name` returns the full destination
  without consuming, bypassing the domain-only reveal gate for limited links. Closing CORS
  does NOT fix this (CORS only stops browser cross-origin calls; curl/servers are unaffected).
  Decide: (a) accept it, or (b) gate/limit the API too (e.g. domain-only for limited links).
- **HMAC secret lifecycle** for reveal tokens (persisted vs per-process random).
- **og:image rasterisation** dependency weight (`resvg`) and PNG caching strategy.
- **Hits semantics**: a hit is counted at `go` (unlimited) or at `reveal` (limited). One use
  per access either way — document it in user-facing copy if needed.
