# kurva

High-performance, modular raster-to-vector engine written in Rust, engineered for sharp cusp preservation and gapless topological segmentation.

`kurva` converts raster artwork (PNG/JPG) into compact, production-ready SVG paths. It targets the failure modes that keep vectorizers unusable for professional work: blunted corners on emblems and mascots, staircase noise that inflates path node counts, and hairline seams between adjacent color regions.

## Project Status

Version 0.1.0. The workspace and the frozen `kurva-types` contracts are implemented; the five pipeline stages currently expose validated stubs with co-located unit tests. Thresholds, contracts, and error taxonomies are stable and evolve only by explicit revision.

## Core Pillars

### Cusp Preservation

Corner cusps are isolated from smooth tangents by angular dot-product classification before any curve fitting occurs. A vertex whose interior angle falls below the configured threshold is emitted as a hard segment boundary, so emblem beaks, claws, shield points, and geometric typography retain discontinuities in tangent instead of melting into rounded blobs.

### Strict Intermediate Representation

Every stage consumes and produces one explicitly declared type from `kurva-types`, and data flows in a single direction. There is no shared mutable state and no cyclic dependency between crates: `kurva-types` has no internal dependencies, and each stage crate can be tested against deterministic JSON fixtures without running its predecessors.

### Topology-Aware Layer Stacking

Adjacent color regions are serialized as a stacked layer order with computed boundary overlap instead of independent traced fields. This removes the microscopic white gaps that appear where two quantized regions meet once the SVG is rasterized or printed.

### Pure Rust and Zero Unsafe

The entire engine is native Rust with no `unsafe` blocks, no C/C++ runtime bindings, and no external color or vision services. Library crates return typed `thiserror` enums and avoid panicking failure paths, which keeps the binary portable and deterministic across Windows, Linux, and macOS.

## Architecture Pipeline

Data flows strictly downward. Each box is one crate; the label between boxes is the only type crossing that boundary.

```text
[Raw Bitmap (PNG/JPG)]
          |
          v
+-------------------------+
|       kurva-color       |  CIELAB quantization (K-Means / Median Cut)
+------------+------------+
             | IndexedRaster
             v
+-------------------------+
|      kurva-contour      |  Border following, closed loop extraction
+------------+------------+
             | RawContourList
             v
+-------------------------+
|        kurva-poly       |  Ramer-Douglas-Peucker, cusp classification
+------------+------------+
             | SimplifiedPolyline
             v
+-------------------------+
|        kurva-fit        |  Constrained Schneider cubic Bezier fitting
+------------+------------+
             | VectorPathList
             v
+-------------------------+
|        kurva-svg        |  Gapless serializer, W3C SVG emitter
+------------+------------+
             |
             v
[Optimized SVG Output]
```

## Workspace Crate Layout

| Crate | Stage | Responsibility | Contract |
| --- | --- | --- | --- |
| `kurva-types` | Contracts | Frozen geometry and IR definitions shared by every stage. No internal dependencies. | Type definitions only |
| `kurva-color` | 1 | Quantizes raw pixels into a perceptually bounded palette (target: CIELAB, K-Means). | `&[u8]` + dimensions -> `IndexedRaster` |
| `kurva-contour` | 2 | Traces closed boundaries per palette index and classifies outer loops versus holes. | `IndexedRaster` -> `RawContourList` |
| `kurva-poly` | 3 | Removes staircase artifacts with RDP and marks sharp cusps against smooth joints. | `RawContourList` -> `Vec<SimplifiedPolyline>` |
| `kurva-fit` | 4 | Fits constrained cubic Bezier segments, splitting recursively past the error tolerance. | `&[SimplifiedPolyline]` -> `VectorPathList` |
| `kurva-svg` | 5 | Serializes paths with rounded coordinates and gap-free layer stacking. | `VectorPathList` + palette -> SVG document |
| `kurva-cli` | Orchestrator | Binds the stages in sequence behind a one-shot command line interface. | Input raster path -> output SVG path |

## Building and Development

Requirements: Rust 1.75 or newer, 2021 Edition. No system libraries, no Python, no native toolchain beyond the Rust MSVC or GNU target.

```bash
# Release build of every workspace member
cargo build --workspace --release

# Full test suite (unit tests are co-located with their implementation)
cargo test --workspace

# Lint gate; zero warnings tolerated
cargo clippy --workspace --all-targets -- -D warnings

# Formatting gate
cargo fmt --all -- --check
```

Verification is one-shot only. Continuous watchers are intentionally excluded from the development protocol; see `agent-rules.md` for the full engineering directive set, including the 250-line source file budget and the strict merge abort gate.

Running the orchestrator:

```bash
cargo run --release -p kurva-cli
```

## Repository Layout

```text
kurva/
|-- Cargo.toml            workspace manifest and shared dependency versions
|-- crates/               one directory per pipeline stage
|-- fixtures/             deterministic JSON fixtures for integration tests
|-- prd.md                product requirements and acceptance criteria
`-- agent-rules.md        engineering conventions enforced on every change
```

## License

Dual-licensed under either of:

- MIT License, see [`LICENSE-MIT`](LICENSE-MIT)
- Apache License, Version 2.0, see [`LICENSE-APACHE`](LICENSE-APACHE)

Choose whichever license suits your integration. Both texts are shipped at the repository root, and the workspace crates declare `MIT OR Apache-2.0`.

Copyright (c) 2026 annaqibz01.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project by you shall be dual-licensed as above, without additional terms or conditions.
