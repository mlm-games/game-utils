# game-utils

Reusable game utility crates extracted from the [`my-ecosystem-bevy`](https://github.com/mlm-games/my-ecosystem-template-bevy) template.

The workspace holds Bevy-agnostic and repame-stack crates (the old Bevy
`game-utils-bevy` crate was removed; Bevy games pin its last working git rev):

| Crate | Description |
| --- | --- |
| [`game-utils`](crates/game-utils/) | Bevy-agnostic: math helpers (`glam`), generic save manager (RON + platform data dir), crash-safe save store (atomic temp+rename writes, `.bak` rotation, corruption quarantine), weighted random / stable sampling, stats aggregation + unlock conditions, achievement registry, i18n (Fluent), multi-profile manager, discovery/codex ledger |
| [`game-utils-repame`](crates/game-utils-repame/) | Repame (repose stack) game-feel utilities over `repame-sim`: sim time scales, hitstop, juice math, pooling, save/i18n/loading |

## game-utils (bevy-agnostic)

- `math_utils` - `MathUtils` with `smooth_damp`, `approach`, `wave` (uses `glam` so the types are the same ones Bevy uses).
- `save` - `SaveManager` persists any `Serialize` data to RON. Data types implement `Versioned` for version migration. Works without Bevy.
- `save_store` - `SaveStore`, a crash-safe file store (temp+rename writes, throttled `.bak` rotation, corruption quarantine + recovery).
- `profiles` - `ProfileManager`, a genre-agnostic multi-profile manager: RON pointer config, per-profile directories, create/clear-with-archive/prune, live switch, and copy-only/verified/atomic legacy migration.
- `codex` - `Codex`/`CodexStore`, a discovery ledger (discovered flag + best value + counter per string id) with optional crash-safe RON persistence.
- `i18n` - `LocaleResources` parses Fluent (`.ftl`) strings into a key/value map for the current locale.

## game-utils-repame

Sim-side game-feel helpers over `repame-sim` (time scales, hitstop,
juice/feel math, pooling, save/i18n/loading resources).

> Retired: the old Bevy `game-utils-bevy` crate (frozen for
> Bevy/`repose-bevy` games) was deleted. Those games pin its last working
> git rev (`rev = "334c1ab2..."` etc. per game lockfile) plus their
> `repose-bevy` rev, so they keep building from git history.

## License

Dual-licensed under either MIT or Apache-2.0, at your option.
