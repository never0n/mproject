"""Export and render the generated Blender library, without changing the UI session.

blender -b assets/environment/reference_city_v1/reference_city_library.blend \
    --python tools/render_reference_city.py
"""
import bpy
import json
import struct
import sys
from pathlib import Path

ROOT=Path(bpy.data.filepath).parent
records=json.loads((ROOT/'manifest.json').read_text())
reuse_previews='--reuse-previews' in sys.argv
results=[]
for record in records:
    name=record['id']
    scene=next(s for s in bpy.data.scenes if s.name.startswith(name+' |'))
    bpy.context.window.scene=scene
    for obj in scene.objects:obj.select_set(False)
    obj=bpy.data.objects[name];obj.select_set(True);bpy.context.view_layer.objects.active=obj
    assert obj.type=='MESH' and len(obj.data.vertices)>0
    assert obj.location.length<1e-8 and tuple(obj.scale)==(1.0,1.0,1.0)
    out=ROOT/record['glb']
    bpy.ops.export_scene.gltf(filepath=str(out),export_format='GLB',use_selection=True,use_active_scene=True,
        export_apply=True,export_materials='EXPORT',export_extras=True,export_yup=True,
        export_animations=False,export_normals=True,export_texcoords=True)
    data=out.read_bytes()
    magic,version,length=struct.unpack_from('<4sII',data)
    assert magic==b'glTF' and version==2 and length==len(data)
    json_len,kind=struct.unpack_from('<II',data,12)
    document=json.loads(data[20:20+json_len])
    assert len(document.get('meshes',[]))==1 and document.get('materials')
    assert len(document.get('scenes',[]))==1
    assert sum('mesh' in n for n in document.get('nodes',[]))==1
    assert not document.get('cameras')
    for buffer in document.get('buffers',[]):assert 'uri' not in buffer
    scene.render.resolution_x=800;scene.render.resolution_y=800
    scene.render.resolution_percentage=100
    if hasattr(scene,'eevee'):scene.eevee.taa_render_samples=48
    scene.render.filepath=str(ROOT/record['preview'])
    if not reuse_previews or not (ROOT/record['preview']).exists():
        bpy.ops.render.render(write_still=True,scene=scene.name)
    results.append({'index':record['index'],'file':record['glb'],'bytes':len(data),
                    'mesh_count':len(document['meshes']),'materials':len(document['materials']),
                    'preview':record['preview']})
    print('ASSET_COMPLETE',record['index'],name,flush=True)

scene=bpy.data.scenes['Reference City | 20 objects'];bpy.context.window.scene=scene
if hasattr(scene,'eevee'):scene.eevee.taa_render_samples=64
scene.render.filepath=str(ROOT/'previews'/'00_full_library.png')
if not reuse_previews or not (ROOT/'previews'/'00_full_library.png').exists():
    bpy.ops.render.render(write_still=True,scene=scene.name)
(ROOT/'validation.json').write_text(json.dumps({'asset_count':len(results),'assets':results,
    'checks':['20 ordered assets','nonempty meshes','meter scale source transforms',
              'GLB headers and embedded buffers','one mesh and one scene per GLB','material payloads','no exported cameras',
              'individual and overview renders']},ensure_ascii=False,indent=2),encoding='utf8')
print('COMPLETE: 20 GLB assets, 20 individual PNG previews, 1 overview.',flush=True)
