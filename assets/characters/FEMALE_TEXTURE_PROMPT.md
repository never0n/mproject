# Female face texture, v1

Generated with the built-in image_gen tool on 2026-09-14. Edit target: the
embedded albedo texture of adventurer.glb (CC BY 3.0, see ATTRIBUTION.md).
Saved unchanged as assets/characters/adventurer-female-v1.png. Only the
head/neck submesh samples this atlas; clothing keeps its original texture.

## Prompt

Use case: precise-object-edit. Asset type: 1024x1024 UV albedo texture atlas for an existing rigged medieval RPG character. Input image is the edit target, NOT a concept-art reference. Keep the atlas layout pixel-aligned: same canvas, exact positions of eyes, nose, mouth, ears, top scalp, neck, every UV island and its boundaries. Edit ONLY the head/scalp/face/neck island at the top centre (roughly x=230..760, y=6..365). Replace the older bearded male face with an adult female adventurer's realistic face: clean-shaven natural skin, softer brow and jaw shading, subtle lips, neutral expression, warm neutral complexion, no makeup, no glamorization. Remove ALL beard/moustache/sideburn shading from the cheeks, chin and upper lip; paint coherent skin there. Keep dark brown hair only at the outer temple/scalp regions, following the original UV mapping, and preserve the exact central mouth and eye positions. Texture remains an unwrapped distorted UV island, NOT a front-view portrait: preserve the original stretched silhouette and landmarks. Keep EVERY other atlas region unchanged, including armor, gloves, boots, leather, eyeball islands and flat gray background. Do not add clothing or accessories. Flat diffuse illumination only; no captions, no grid, no perspective, no 3D render. This atlas will be sampled by the existing head mesh; alignment is the highest priority.
