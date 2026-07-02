# YuioLink

Wieldy ephemeral links — live at [yuio.link](https://yuio.link/).

Every link expires (7 days at most), every link previews before it opens, and
names are short, memorable words like `braveOTTER` instead of random strings.
No permanent links, no accounts, no tracking.

## How it works

- **Names are words.** Drawn from a curated 3,516-word list, shown in
  alternating case (`braveOTTER`) purely for readability — lookups are
  case-insensitive.
- **Three link types.**
  - *Public* — the shortest name currently available (1–3 words depending on
    namespace occupancy and TTL). Guessable by design; guards nothing.
  - *Private* — a four-word name (~47 bits, ~153 trillion combinations). Not
    encryption: protection by sheer improbability of guessing within the
    link's life.
  - *One-Time* — a four-word name that burns on first reveal. If it answers
    410 Gone, someone already opened it — tamper evidence built in.
- **Always-preview.** `GET /:name` never spends a use: you see where a link
  goes (domain only, for one-time links) before continuing. Consuming is a
  POST, so unfurl crawlers can never burn a link.
- **Tombstones.** A spent or withdrawn link answers 410 until it expires; only
  expiry frees a name for reuse. A name can never be silently repurposed
  within its stated life.

Details: [docs/ROUTES.md](docs/ROUTES.md) (endpoints),
[docs/PREVIEW.md](docs/PREVIEW.md) (trust model),
[docs/NAMESPACES.md](docs/NAMESPACES.md) (name allocation).

## Quick use from a terminal

```sh
curl -d url=https://example.com https://yuio.link/create
curl -d url=https://example.com -d ttl=1h -d uses=1 https://yuio.link/create
curl --data-binary @notes.txt https://yuio.link/create   # becomes a Text link
```

## Development

Rust workspace: `core` (names, validation, detection) and `server` (axum +
SQLite). HTML is rendered server-side (maud); the pages work without
JavaScript and enhance with a little vanilla JS.

```sh
nix develop --command cargo test --workspace   # test
nix develop --command cargo run -p yuiolink-server   # http://127.0.0.1:8080/
```

Configuration is environment variables (see `server/src/config.rs`):
`YUIOLINK_BIND`, `YUIOLINK_BASE_URL`, `YUIOLINK_DB`, `YUIOLINK_SECRET`,
`YUIOLINK_MAX_TTL_SECS`, `YUIOLINK_REAP_INTERVAL_SECS`.

## Deployment

CI (`.github/workflows/ci.yml`) gates every push; pushing a `v*` tag builds
the linux/amd64 binary and attaches it to a GitHub release
(`release.yml`). The droplet is provisioned by
`deploy/droplet-user-data.bash` and updates itself pull-based:
`yuiolink-update` fetches the latest release, verifies its SHA-256,
snapshots the database, installs, and health-checks `/healthz` — rolling the
binary back on failure. `yuiolink-update.timer` runs it every 10 minutes (a
no-op when already current), so tagging a release auto-deploys within 10
minutes with no SSH needed; `systemctl start yuiolink-update` also runs it
on demand. Nightly database snapshots run via `yuiolink-backup.timer`.

A `Dockerfile` builds the same server for container use (`/data` volume holds
the SQLite database).

## Versioning

[SemVer 2.0.0](https://semver.org), tags `vX.Y.Z`. The flake reads the
workspace version from `Cargo.toml`.

YuioLink is a project by [jooize](https://github.com/jooize).
