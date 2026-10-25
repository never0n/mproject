import bpy,math,random,json,os
from mathutils import Vector
from mathutils.bvhtree import BVHTree
random.seed(146)
out='/home/never0n/Projects/new_desctop_MMO/assets/characters/reference_portrait'
for o in bpy.context.scene.objects:
    if o.name.startswith(('Iris_v2','Pupil_v2','Cornea_v2')):
        for v in o.data.vertices:v.co.z-=.003
    if o.name.startswith(('Eye_v2','Iris_v2','Pupil_v2','Cornea_v2')):o.location.y+=.004
# Thin tear meniscus derived from the actual globe/lid intersection.
ob=bpy.data.objects['Portrait | finished surface'];me=ob.data
bvh=BVHTree.FromPolygons([v.co for v in me.vertices],[tuple(p.vertices) for p in me.polygons])
m=bpy.data.materials.new('Lid | tear meniscus');m.use_nodes=True;b=next(n for n in m.node_tree.nodes if n.type=='BSDF_PRINCIPLED');b.inputs['Base Color'].default_value=(.18,.085,.055,1);b.inputs['Roughness'].default_value=.21
for sign in [-1,1]:
    pts=[];cx=sign*.324;cz=2.946;cy=-.880
    for j in range(100):
        angle=math.pi+math.pi*j/99
        last=None
        for rr in range(1,146):
            r=rr*.001;dx=r*math.cos(angle);dz=r*math.sin(angle)
            yy=cy-math.sqrt(max(.000001,.145**2-r*r))
            hit,normal,index,dist=bvh.ray_cast(Vector((cx+dx,-2,cz+dz)),Vector((0,1,0)),4)
            if hit is not None and hit.y<yy:
                if last is not None:pts.append(last)
                break
            last=Vector((cx+dx,yy-.0015,cz+dz))
    cu=bpy.data.curves.new('Tear meniscus','CURVE');cu.dimensions='3D';cu.bevel_depth=.0014;cu.bevel_resolution=2;cu.materials.append(m)
    if pts:
        sp=cu.splines.new('POLY');sp.points.add(len(pts)-1)
        for p,v in zip(sp.points,pts):p.co=(*v,1)
    o=bpy.data.objects.new('Lid meniscus '+str(sign),cu);bpy.context.collection.objects.link(o)
# Three saved portrait cameras, with a perspective presentation view.
s=bpy.context.scene
def camera(name,loc,target,lens=100,ortho=None):
    d=bpy.data.cameras.new(name);o=bpy.data.objects.new(name,d);s.collection.objects.link(o);o.location=loc;o.rotation_euler=(Vector(target)-o.location).to_track_quat('-Z','Y').to_euler();d.lens=lens
    if ortho is not None:d.type='ORTHO';d.ortho_scale=ortho
    return o
front=camera('01 | Front',(0,-7.5,2.77),(0,-.3,2.77),100)
hero=camera('02 | Three quarter',(3.7,-7.0,2.93),(0,-.3,2.76),105)
profile=camera('03 | Profile',(12,-.3,2.77),(0,-.3,2.77),ortho=3.2)
s.render.resolution_x=1050;s.render.resolution_y=1300;s.render.resolution_percentage=100;s.cycles.samples=256;s.cycles.use_denoising=False
s.camera=hero
s['reference']='User reference sheet 3befb5c3-89cf-4f08-aecb-885665c8eb38.png, packed in this file.'
s['status']='Reference-based head study v2. Approximate likeness; photo-projected diffuse retains source lighting; static portrait, no facial rig.'
s['units']='1 Blender unit = 10 cm; metric scene scale 0.1.'
ob['source']='MakeHuman CC0 basemesh, locally available human_base.obj; proportion fitting and procedural grooming created for this reference.'
for im in bpy.data.images:
    if im.source=='FILE':
        try:im.pack()
        except RuntimeError:pass
for o in bpy.context.selected_objects:o.select_set(False)
ob.select_set(True);bpy.context.view_layer.objects.active=ob
for screen in bpy.data.screens:
    for area in screen.areas:
        if area.type=='VIEW_3D':
            area.spaces.active.region_3d.view_perspective='CAMERA'
            area.spaces.active.shading.type='MATERIAL'
            area.spaces.active.overlay.show_overlays=False
bpy.context.preferences.filepaths.save_version=0
s.render.filepath=out+'/v2_three_quarter.png'
bpy.ops.wm.save_as_mainfile(filepath=out+'/reference_portrait_v2.blend')
for cam,name in [(hero,'v2_three_quarter.png'),(front,'v2_front.png'),(profile,'v2_profile.png')]:
    s.camera=cam;s.render.filepath=out+'/'+name;bpy.ops.render.render(write_still=True)
s.camera=hero;s.render.filepath=out+'/v2_three_quarter.png';bpy.ops.wm.save_as_mainfile(filepath=out+'/reference_portrait_v2.blend')
report={'head_vertices':len(ob.data.vertices),'head_faces':len(ob.data.polygons),'hair_strands':sum(len(o.data.splines) for o in s.objects if o.type=='CURVE' and o.name.startswith('Hair_v2')),'beard_strands':len(bpy.data.objects['Beard_v2 | dense tapered follicles'].data.splines),'packed_images':[im.name for im in bpy.data.images if im.packed_file]}
open(out+'/validation_v2.json','w').write(json.dumps(report,indent=2));print(report)
