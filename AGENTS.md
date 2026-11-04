# Ganbate development

Ganbate is a general-purpose game engine. Keep application assets, game protocols, rules, fixtures, and adapters in their owning game repositories. Samples and tests here must be original, genre-neutral or synthetic. Never upload material from private game repositories.

Write engine code in Rust with zero third-party Cargo dependencies. Bind directly to documented platform APIs. Write shaders in Ganbate's WGSL subset for `ganbate-shader`; keep game styles in Look files, not engine constants. Keep unsafe FFI behind a narrow interface with explicit lifetime and synchronization rules. Use the offscreen path for regression tests and the presented path for the player.

Before a change lands, exercise its normal entry point, run formatting, clippy, unit tests, and relevant driver tests. Distinguish a compile check from a hardware run. Record which platform and driver produced evidence. Preserve unrelated work and upstream history. Game migrations must be reversible per platform.
