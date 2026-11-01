# iOS Unified Memory and Mobile Rendering Plan

## Goal

Treat iPhone memory as one constrained CPU/GPU budget. Apple silicon uses unified physical memory; GPU buffers, texture resources, chunk meshes, decoded assets, and ordinary application allocations compete for the same device memory. The client cannot manufacture RAM or override iOS process limits. It can avoid over-reserving memory, stream resources, reuse allocations, and shed optional work before the OS terminates the app.

## Stage 1 — conservative GPU pool (implemented in experiment branch)

- Keep the existing desktop VRAM policy unchanged.
- On iOS, cap the terrain chunk pool at 192 buckets and use a 64-bucket minimum.
- Budget from the reported shared device heap at a conservative 5% rather than applying the desktop 25% VRAM fraction.
- Log the selected shared-memory budget at startup.

This limits the initial terrain reservation to roughly 108 MiB with the current bucket layout, instead of allowing the desktop 2,048-bucket ceiling to reserve over a gigabyte. Actual values depend on `PackedVertex` layout and device-reported heaps.

## Stage 2 — adaptive unified-memory controller

Introduce a `MobileResourceBudget` with explicit classes:

1. **Critical:** active chunk meshes, visible textures, player/entity assets, frame targets.
2. **Warm cache:** nearby chunk meshes, recently used textures, decoded resource-pack assets.
3. **Evictable:** distant chunk meshes, old decoded images, shader intermediates, unused atlas variants.

The controller should combine device class, app-owned allocation estimates, current render distance, and OS memory-pressure notifications. It should expose a small number of pressure tiers (`Normal`, `Constrained`, `Critical`) and change budgets with hysteresis so it does not oscillate every frame.

On pressure:

- Stop speculative chunk prefetch first.
- Evict distant chunk mesh allocations only after their GPU frame fences have retired.
- Drop least-recently-used decoded texture caches and rebuild them on demand.
- Reduce optional render targets and shader quality before touching visible world data.
- Keep a small emergency reserve for networking, UI, and world-state updates.

Do not use `device_local_bytes` as a proxy for free RAM. It describes a heap, not current free memory. Add a platform adapter for supported iOS memory-pressure signals and process footprint reporting before enabling runtime pressure reactions.

## Stage 3 — Fast Graphics profile

Fast mode should be a coherent render budget, not merely a color preset:

- Disable built-in post grading and spectator full-frame post chains.
- Disable particles, weather, clouds, and entity shadows.
- Prefer lower-cost water and sky paths; keep block/entity visibility and gameplay logic intact.
- Reduce speculative chunk work and cache residency under pressure.
- Keep render distance user-controlled, but adapt background chunk preparation to the available budget.

Avoid hiding nearby entities or changing server/game simulation behavior as a performance shortcut.

## Shader feature roadmap

The existing built-in shader is a lightweight full-screen color grade with a five-sample highlight bloom approximation. The shader-pack renderer is a separate path and already has terrain/entity/sky/water program stages and shadow-pass plumbing. Neither fact means all advanced effects below are complete.

Recommended implementation order for mobile:

1. **Water surface:** time-driven low-amplitude wave normals, Fresnel-weighted sky reflection, depth/fog-based transparency, shoreline foam, and a low-resolution reflection target. Start with one or two taps, not full ray tracing.
2. **Lighting and shadows:** one sun/moon shadow cascade first, with PCF filtering and a reduced-resolution shadow map; add colored block-light contribution in the terrain shading path.
3. **Atmosphere:** analytic sky scattering and inexpensive sun shafts; reserve true volumetric cloud marching for high quality only.
4. **Material response:** metallic/roughness inputs and normal maps where resource packs provide them; POM only on capable devices and close surfaces.
5. **Post effects:** separable bloom at half resolution, optional SSAO at reduced resolution, and depth-of-field only when a usable depth target is available.

Each effect needs a quality tier, a memory budget, and a fallback path. Avoid claiming realistic reflections, volumetrics, PBR, or ray tracing until their render targets, shader inputs, and device tests exist.

## Validation

- Run Rust tests and iOS-target Clippy for each change.
- Build an unsigned IPA from the experiment branch separately from the main release branch.
- On-device capture: peak resident footprint, chunk-pool allocation, frame-time percentiles, thermal state, and memory-pressure behavior across Fancy/Fast.
- Test shader variants at several resolutions and verify that Fast mode never allocates optional post targets.
