# Routes

The authoritative route reference, matching `server/src/web.rs::router`.
Model in one line: **GET previews, POST spends** (Post/Redirect/Get), so
crawlers and prefetchers can never spend a use.

There is no `POST /:name/go`. Following an unlimited redirect is a plain
`<a href>` on the preview page: nothing is spent (an unlimited link has no use
to spend), and a link — unlike a form — has nothing for the CSP's
`form-action 'self'` to block at the redirect hop. Whether the destination may
be emitted as an `href` at all is decided at **render time**, in
`views::is_linkable`, on both the preview page and the revealed page; a scheme
off `DEFAULT_ALLOWED_SCHEMES` is printed and given no control.

## Pages

| Route | Method | Behavior |
|-------|--------|----------|
| `/` | GET | Landing page: the create form (works without JS). |
| `/` | POST | No-JS create (form-encoded). Renders a server-side result page. Rate-limited. |
| `/:name` | GET | The always-preview resolver. Spends **no** use. A live redirect (or one-time Text) renders the interstitial; unlimited Text renders immediately; a visitor with a valid `yl_reveal` cookie gets the revealed view here; spent/withdrawn is **410 Gone**; expired/unknown is **404**. A trailing `+` is accepted and ignored. |
| `/:name/reveal` | POST | Spend a **one-time** link's use (redirect or Text): `uses`+1, set the path-scoped `yl_reveal` HMAC cookie (~10 min), 303 back to `/:name`, which renders the revealed view. That render redacts the row **and** expires the cookie (`Max-Age=0`), so the capability lives exactly one request. |
| `/:name/card.png` | GET | The og:image share card (redirects only). Spends no use; `Cache-Control: max-age=3600`. |
| `/help` | GET | The usage page: why links expire, what the three types and two kinds are for, worked scenarios, and the `curl` endpoint (printed with this instance's base URL, so a copied command targets the host being read). Static — touches no database. |
| `/healthz` | GET | Deploy/update health probe. Touches the database, so a failed migration reads as unhealthy. |
| `/stats` | GET | Public aggregate counters: live links, created (by type and kind), opened, previewed, revealed, expired, and the last 7 UTC days. Reads the `stats` table, which holds nothing but `(day, metric, count)` — no IP, user agent, referrer, link name, or destination, and no per-event row. Degrades to zeroes rather than 500ing. |
| `/wordlist.txt` | GET | The curated 3,456-word name list as plain text (linked from the landing page's Privacy/Security disclosure — the namespace is public by design). |
| `/static/app.css`, `/static/app.js`, `/static/text.js`, `/static/preview.js` | GET | Embedded assets; `Cache-Control: public, max-age=3600`. |

## Terminal convenience

| Route | Method | Behavior |
|-------|--------|----------|
| `/create` | POST | `curl -d url=<url> [-d ttl=10m\|2h\|3d] [-d uses=1] https://yuio.link/create` → the short URL as plain text (JSON with `Accept: application/json`). Kind is auto-detected; `--data-binary @file` becomes a Text link. Rate-limited. No delete token is issued. |

`ttl` and `uses` are peeled off the **end** of the body, so an unencoded URL
keeps its own `?a=1&b=2` query as long as they come last. That leniency cannot
be perfect — an unencoded URL whose own last parameter is called `ttl` would be
read as the option — so a value that arrives under a field name (`url=`,
`text=`, `content=`) is **percent-decoded once**, which makes
`curl --data-urlencode url=…` the unambiguous way to send anything with an `&`,
an `=`, or a space in it. `%XX` only: a `+` is left alone, because in an
unencoded URL it is usually a character somebody typed and curl writes a space
as `%20` and a plus as `%2B` anyway. A body with **no** field name is
`--data-binary @file` — raw bytes for a Text link, never decoded, because a log
full of `%` is not a form.

## REST API (`/api/v0`, same-origin, no CORS)

| Route | Method | Behavior |
|-------|--------|----------|
| `/api/v0/links` | POST | Create (JSON: `kind`, `content`, `ttl_seconds?`, `max_uses?` (only `1`), `secret?`). `201 Created` + `Location` + a one-time `delete_token`. Rate-limited. |
| `/api/v0/links/:name` | GET | Read without spending. For a **one-time** link this returns **metadata only** — no `target`/`content` — because disclosing the payload without spending the use would defeat the burn-after-read tamper evidence. Unlimited links include their `target`/`content`. |
| `/api/v0/links/:name` | DELETE | Withdraw, authorized by `Authorization: Bearer <delete_token>`. `204`; the name stays reserved as a 410 tombstone until expiry. Wrong/missing token or unknown name are both `404` (reveals nothing). |
| `/api/v0/openapi.yaml` | GET | The OpenAPI 3.1 description (embedded from `server/openapi.yaml`, so the served spec matches the binary). |

That description carries its own version in `info.version`, which is
deliberately **not** the crate version: OAS 3.1 defines the field as the version
of the OpenAPI document, "distinct from the OpenAPI Specification version or the
API implementation version" ([issue #3872] settled the ambiguity — a
components-only document can be shared by several APIs, so the field cannot mean
the API's version). It moves when `server/openapi.yaml` changes, at its own
semver pace, and stays put across releases that leave the document alone.

[issue #3872]: https://github.com/OAI/OpenAPI-Specification/issues/3872

Validation does not fail fast: a `400` reports **every** offending field at
once — `error` is the joined summary string, `errors` an array of
`{ "field", "message" }`. The no-JS form and `/create` render the same batch
as one message per line.

Unknown JSON fields are **rejected**, not ignored (`additionalProperties:
false`), and each one comes back named in that same batch. A dropped field is
one the caller believes took effect: someone sending the pre-0.8 `private`, or
a typo for `secret`, would otherwise be handed a short guessable name and told
nothing.

## Rate limiting

Creation only (the three create surfaces above): per-client token bucket,
burst 10, one create per 6 s sustained; over the limit is an immediate `429`.
Resolution is never rate-limited or slowed — latency is not throughput;
volumetric abuse is the upstream CDN's job.

## Why no CSRF tokens on the consume POSTs

The name **is** the capability: anyone who knows it can POST `/:name/reveal`
directly, so a cross-site auto-submitting form gives an attacker nothing they
could not already do with the name. `SameSite=Lax` additionally protects the
reveal cookie, and the cookie is expired on the very response that uses it.
