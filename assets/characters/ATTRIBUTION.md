# Adventurer character

Adapted from **Animated Defender AKA Brigand**:
https://opengameart.org/content/animated-defender-aka-brigand

- Concept: Konstantin Maystrenko
- Original model: Anthony Myers
- Retopology, rigging and animation: Danimal

Licensed under Creative Commons Attribution 3.0 Unported (CC BY 3.0):
https://creativecommons.org/licenses/by/3.0/

Changes for this project: converted legacy materials to PBR, baked idle/walk/run
clips, removed the polearm to avoid floor clipping during locomotion, validated mesh and exported a skinned glTF binary with embedded textures.
Original authors do not endorse this project. Keep these credits with distributions.

Version 44 runtime adaptations: the visible cuirasses are derived from this
model's torso mesh, with the original skin weights, UVs and textures retained,
normal offsets and material/vertex-colour variants added. These derived
parts retain the same CC BY 3.0 attribution. The procedural swords and staves
are new project geometry; the source GLB itself is unchanged.

Version 45 runtime adaptations: race/gender bone proportions; separated head
and clothing primitives; a fitted short-hair shell derived from the source
head mesh; new procedural ears, tusks and small temple growths. The female
head albedo in adventurer-female-v1.png was generated with the built-in
image_gen tool by editing the original UV atlas. See FEMALE_TEXTURE_PROMPT.md.
The source-derived head/hair meshes and texture keep the CC BY 3.0 attribution.
The original adventurer.glb remains unchanged.

Version 46: the basic axe, hammer, greatsword, greataxe, greatclub, pike, bow
and dagger in appearance/arsenal.rs are new procedural project geometry.
The two-hand grip solver reposes the attributed adventurer skeleton at runtime;
no source animation or GLB was overwritten.

Version 47: named town NPCs reuse this attributed model, idle animation and
the runtime race/gender variants. No new third-party character models were
added. Town workbench props are original procedural project geometry.


Version 49: bandits, marauders and apostates reuse the attributed body, rig and
Idle/Walk/Run clips. Runtime clothing colour variants, fitted masks, helmet,
pauldrons and hood shells are derived from its mesh, UVs and skin weights and
retain CC BY 3.0 attribution. Open folded cloak/coat panels, the kite shield
and nasal guard are new procedural project geometry attached to that rig.
The hit/casting/shield poses use runtime bone adjustments; no source GLB,
texture or authored animation clip was replaced. No new third-party assets
were added. Separate finished enemy bodies, attack/death clips and physical
cloth simulation are not included in this iteration.


Version 50: original project-authored skeletal key poses for enemy dagger
thrusts, axe swings, spell release and collapse are layered onto the attributed
adventurer skeleton. Runtime arm/leg IK, frozen corpse poses and terrain-aware
model placement were added. No source GLB, texture or authored Idle/Walk/Run
clip was replaced; the underlying body and rig retain the attribution above.
These are authored runtime motions, not motion-capture clips or physics ragdolls.


Version 51: new runtime key poses for player one-/two-handed sword, axe,
hammer/club, pike, dagger, bow, unarmed and spell/ability actions reuse the
attributed skeleton and locomotion clips. Existing weapon IK is extended
for action grips. Bow string and nocked arrow are now separate animated
attachments; arrow geometry and cosmetic magic flights are project-authored.
The original GLB, textures and source Idle/Walk/Run animations are unchanged.
No new third-party assets or motion-capture clips were added.


Version 52: the seven named town residents retain the attributed Defender body,
face, skeleton and Idle clip. Their cloned clothing colour variants derive
from the mesh/UVs/weights and retain CC BY 3.0 attribution. Folded apron/coat/
robe panels, buttons, pouches, satchel, small work tools and desk details are
original procedural project geometry. Profession-specific work cycles, arm IK
and conversational head turns are original runtime poses layered on that rig.
The source GLB, textures and imported clips are unchanged. No new third-party
assets or motion-capture clips were added; physical cloth is not simulated.
