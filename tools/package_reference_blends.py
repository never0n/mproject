"""Turn source collection libraries into standalone Blender documents.

blender --background --factory-startup --python tools/package_reference_blends.py
"""
import bpy
import json
from pathlib import Path
ROOT=Path('/home/never0n/Projects/new_desctop_MMO/assets/environment/reference_city_v1')
records=json.loads((ROOT/'manifest.json').read_text())
report=[]
for record in records:
    filename=ROOT/record['blend']
    bpy.ops.wm.open_mainfile(filepath=str(filename))
    scene=next(s for s in bpy.data.scenes if s.name.startswith(record['id']+' |'))
    bpy.context.window.scene=scene
    asset=bpy.data.objects[record['id']]
    for o in scene.objects:o.select_set(False)
    asset.select_set(True);bpy.context.view_layer.objects.active=asset
    for screen in bpy.data.screens:
        for area in screen.areas:
            if area.type=='VIEW_3D':
                space=area.spaces.active
                allowed={item.identifier for item in space.shading.bl_rna.properties['type'].enum_items}
                assert 'MATERIAL' in allowed;space.shading.type='MATERIAL'
                perspective={item.identifier for item in space.region_3d.bl_rna.properties['view_perspective'].enum_items}
                assert 'CAMERA' in perspective;space.region_3d.view_perspective='CAMERA'
                space.region_3d.view_camera_zoom=5
                space.overlay.show_overlays=False
    bpy.context.preferences.filepaths.save_version=0
    bpy.ops.wm.save_as_mainfile(filepath=str(filename))
    # Reopen the document: validate the actual persisted active scene.
    bpy.ops.wm.open_mainfile(filepath=str(filename))
    assert bpy.context.scene.name.startswith(record['id']+' |')
    meshes=[o for o in bpy.context.scene.objects if o.type=='MESH']
    assert len(meshes)==1 and meshes[0].name==record['id']
    assert bpy.context.scene.camera is not None
    report.append({'index':record['index'],'blend':record['blend'],'active_scene':bpy.context.scene.name,
                   'meshes':len(meshes),'vertices':len(meshes[0].data.vertices)})
    print('BLEND_VERIFIED',record['index'],record['id'],flush=True)
(ROOT/'native_validation.json').write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding='utf8')
