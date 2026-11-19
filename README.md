# Kimbo – Placement (Windows, Rust)

Assistant de placement pour la Canopée du Kimbo (Dofus), version native Windows.

- Interface : egui / eframe, rendu **wgpu** : DirectX 12, repli automatique sur OpenGL
- Fenêtre **toujours au-dessus** des autres applications
- GPU intégré privilégié (`LowPower`), mémoire privilégiée (`MemoryHints::MemoryUsage`),
  `desired_maximum_frame_latency = 1`
- Purge de la mémoire inutilisée (`EmptyWorkingSet`) 1,5 s après le démarrage puis à chaque perte de focus

## Compiler en local (Windows)

    rustup target add x86_64-pc-windows-msvc
    cargo build --release

Le `.exe` se trouve dans `target/release/kimbo-placement.exe` (icône intégrée depuis `assets/icon.ico`).

## GitHub Actions

`.github/workflows/build.yml` compile le `.exe` à chaque push / pull request et le publie comme artefact
(onglet *Actions* → run → *Artifacts*). Un tag `vX.Y.Z` crée en plus une Release avec le `.exe`.
