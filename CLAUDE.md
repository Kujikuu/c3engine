# C3Engine

General-purpose Rust game engine + studio (TQ C3-engine style). Conquer is the first game.

**Read before working:** `docs/VISION.md`, `docs/ARCHITECTURE.md`, `docs/ROADMAP.md`,
`docs/DECISIONS.md`, `docs/FORMATS.md`. Keep them updated: tick roadmap items,
append decisions, document formats as you implement them.

## Rules
- UI/UX matters: clear hierarchy, hints, shortcuts, legends, empty/loading states. Verify UI changes with screenshots.
- Clean code: focused modules, doc comments on public items, `cargo fmt` + `cargo clippy --workspace --all-targets` with zero warnings.
- Studio layout: `studio/src/{main,app,panels,map_view,theme}.rs`. Reusable widgets go in `theme.rs`, the overlay palette in `map_view::colors`.
- Nothing in `engine/` or `studio/` references Conquer; game code goes in `games/`.
- Every format parser needs a round-trip test on real files from
  `D:\Conquer2\Tools\eosource\client5065` (skip gracefully if absent).
- Build: `cargo build` · Test: `cargo test` · Lint: `cargo clippy --all-targets`.
- Toolchain: stable-x86_64-pc-windows-msvc. If `cargo` isn't on PATH, add `%USERPROFILE%\.cargo\bin`.
