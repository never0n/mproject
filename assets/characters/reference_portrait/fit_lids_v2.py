import bpy,math,json,bmesh
from mathutils import Vector
out='/home/never0n/Projects/new_desctop_MMO/assets/characters/reference_portrait'
def lid(p):
    x,y,z=p
    front=max(0,min(1,(-y-.90)/.10))
    w=math.exp(-((abs(x)-.324)/.155)**6)*front
    dz=(-.018*math.exp(-((z-2.992)/.04)**2)+.005*math.exp(-((z-2.899)/.028)**2))*w
    return Vector((x,y,z+dz))
for name in ['Head_v2','Portrait | finished surface']:
    o=bpy.data.objects[name]
    vs=o.data.shape_keys.key_blocks['Portrait likeness'].data if o.data.shape_keys else o.data.vertices
    for v in vs:v.co=lid(v.co)
for o in bpy.context.scene.objects:
    if o.name.startswith(('Eye_v2','Iris_v2','Pupil_v2','Cornea_v2')):o.location.y+=.007
    if o.name.startswith('Lid meniscus'):
        for sp in o.data.splines:
            for p in sp.points:p.co=(*lid(Vector(p.co[:3])),1)
for name in ['01 | Front','02 | Three quarter']:bpy.data.objects[name].data.lens=85
s=bpy.context.scene;s.render.resolution_x=900;s.render.resolution_y=1100;s.cycles.samples=192
s.camera=bpy.data.objects['02 | Three quarter'];s.render.filepath=out+'/v2_three_quarter.png'
bpy.context.preferences.filepaths.save_version=0;bpy.ops.wm.save_as_mainfile(filepath=out+'/reference_portrait_v2.blend')
for name,fn in [('02 | Three quarter','v2_three_quarter.png'),('01 | Front','v2_front.png'),('03 | Profile','v2_profile.png')]:
    s.camera=bpy.data.objects[name];s.render.filepath=out+'/'+fn;bpy.ops.render.render(write_still=True)
s.camera=bpy.data.objects['02 | Three quarter'];s.render.filepath=out+'/v2_three_quarter.png';bpy.ops.wm.save_as_mainfile(filepath=out+'/reference_portrait_v2.blend')
ob=bpy.data.objects['Portrait | finished surface'];bm=bmesh.new();bm.from_mesh(ob.data)
report={'head_vertices':len(ob.data.vertices),'head_faces':len(ob.data.polygons),'head_boundary_edges':sum(e.is_boundary for e in bm.edges),'head_nonmanifold_edges':sum(not e.is_manifold for e in bm.edges),'hair_strands':sum(len(o.data.splines) for o in s.objects if o.type=='CURVE' and o.name.startswith('Hair_v2')),'beard_strands':len(bpy.data.objects['Beard_v2 | dense tapered follicles'].data.splines),'packed_images':[im.name for im in bpy.data.images if im.packed_file]};bm.free()
open(out+'/validation_v2.json','w').write(json.dumps(report,indent=2));print(report)
