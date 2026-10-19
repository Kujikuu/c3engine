# Vendored crates

Upstream crates copied here because their released versions target an older egui than the
engine's. Only their `Cargo.toml` changed (egui `0.37`, `egui-scale` by path); their code is
upstream's, so its warnings are allowed at the crate root.

| Crate | Version | License | Why |
|---|---|---|---|
| egui-snarl | 0.12.0 | MIT OR Apache-2.0 | The Studio's blueprint graph editor (M10). Released for egui 0.36. |
| egui-scale | 0.6.1 | MIT OR Apache-2.0 | A dependency of egui-snarl. Released for egui 0.36. |

Drop them for the released crates once those support the engine's egui.
