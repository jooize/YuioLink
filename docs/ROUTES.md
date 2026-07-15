# Routes

The authoritative route reference, matching `server/src/web.rs::router`.
Model in one line: **GET previews, POST consumes** (Post/Redirect/Get), so
crawlers and prefetchers can never spend a use.

## Pages

| Route | Method | Behavior |
|-------|--------|----------|
| `/` | GET | Landing page: the create form (works without JS). |
| `/` | POST | No-JS create (form-encoded). Renders a server-side result page. Rate-limited. |
| `/:name` | GET | The always-preview resolver. Spends **no** use. A live redirect (or limited Text) renders the interstitial; unlimited Text renders immediately (counts a hit); a visitor with a valid `yl_reveal` cookie gets the revealed view here; spent/withdrawn is **410 Gone**; expired/unknown is **404**. A trailing `+` is accepted and ignored. |
| `/:name/go` | POST | Consume an **unlimited redirect**: hits+1, 303 to the destination. |
| `/:name/reveal` | POST | Consume a **limited** link (redirect or Text): hits+1, set the path-scoped `yl_reveal` HMAC cookie (~10 min), 303 back to `/:name`, which renders the revealed view. Refresh/back re-renders without re-consuming. |
| `/:name/card.png` | GET | The og:image share card (redirects only). Spends no use; `Cache-Control: max-age=3600`. |
| `/healthz` | GET | Deploy/update health probe. Touches the database, so a failed migration reads as unhealthy. |
| `/wordlist.txt` | GET | The curated 3,516-word name list as plain text (linked from the landing page's Privacy/Security disclosure — the namespace is public by design). |
| `/static/app.css`, `/static/app.js`, `/static/text.js` | GET | Embedded assets; `Cache-Control: public, max-age=3600`. |

## Terminal convenience

| Route | Method | Behavior |
|-------|--------|----------|
| `/create` | POST | `curl -d url=<url> [-d ttl=10m\|2h\|3d] [-d uses=1] https://yuio.link/create` → the short URL as plain text (JSON with `Accept: application/json`). Kind is auto-detected; `--data-binary @file` becomes a Text link. Rate-limited. No delete token is issued. |

## REST API (`/api/v1`, same-origin, no CORS)

| Route | Method | Behavior |
|-------|--------|----------|
| `/api/v1/links` | POST | Create (JSON: `kind`, `content`, `ttl_seconds?`, `max_uses?` (only `1`), `private?`). `201 Created` + `Location` + a one-time `delete_token`. Rate-limited. |
| `/api/v1/links/:name` | GET | Read without consuming. For a **limited** (single-use) link this returns **metadata only** — no `target`/`content` — because disclosing the payload without spending the use would defeat the burn-after-read tamper evidence. Unlimited links include their `target`/`content`. |
| `/api/v1/links/:name` | DELETE | Withdraw, authorized by `Authorization: Bearer <delete_token>`. `204`; the name stays reserved as a 410 tombstone until expiry. Wrong/missing token or unknown name are both `404` (reveals nothing). |
| `/api/v1/openapi.yaml` | GET | The OpenAPI 3.1 description (embedded from `server/openapi.yaml`, so the served spec matches the binary). |

Validation does not fail fast: a `400` reports **every** offending field at
once — `error` is the joined summary string, `errors` an array of
`{ "field", "message" }`. The no-JS form and `/create` render the same batch
as one message per line.

## Rate limiting

Creation only (the three create surfaces above): per-client token bucket,
burst 10, one create per 6 s sustained; over the limit is an immediate `429`.
Resolution is never rate-limited or slowed — latency is not throughput;
volumetric abuse is the upstream CDN's job.

## Why no CSRF tokens on the consume POSTs

The name **is** the capability: anyone who knows it can POST `/:name/reveal`
directly, so a cross-site auto-submitting form gives an attacker nothing they
could not already do with the name. `SameSite=Lax` additionally protects the
reveal cookie.
