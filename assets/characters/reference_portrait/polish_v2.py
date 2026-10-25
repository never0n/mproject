import bpy,math,bmesh
from mathutils import Vector
ob=bpy.data.objects['Head_v2'];key=ob.data.shape_keys.key_blocks['Portrait likeness']
def smooth(a,b,x):
    t=max(0,min(1,(x-a)/(b-a)));return t*t*(3-2*t)
def ear(p):
    return abs(p.x)>.71 and p.y>-.48 and p.z>2.40
for a,v in zip(ob.data.color_attributes['Follicle coverage'].data,key.data):
    x,y,z=v.co;h,b,sk,al=a.color
    if ear(v.co):b=0
    neckline=1.82+.64*max(0,y+1.05)
    sk=max(sk,(1-smooth(neckline-.05,neckline+.10,z))*smooth(-1.0,-.65,y))
    a.color=(h,b,sk,al)
beard=bpy.data.objects['Beard_v2 | dense tapered follicles'].data
for sp in list(beard.splines):
    if ear(Vector(sp.points[0].co[:3])):beard.splines.remove(sp)
brows=bpy.data.objects['Brows_v2 | directional fine hairs'].data;brows.bevel_depth=.00052
for sp in brows.splines:
    root=sp.points[0].co.copy()
    for p in sp.points:p.co=root+(p.co-root)*.65
bs=next(n for n in bpy.data.materials['Sclera | ivory wet tissue'].node_tree.nodes if n.type=='BSDF_PRINCIPLED');bs.inputs['Base Color'].default_value=(.20,.175,.15,1);bs.inputs['Specular IOR Level'].default_value=.24
bs=next(n for n in bpy.data.materials['Pupil | light absorption'].node_tree.nodes if n.type=='BSDF_PRINCIPLED');bs.inputs['Specular IOR Level'].default_value=0
bs=next(n for n in bpy.data.materials['Iris | hazel brown radial fibres'].node_tree.nodes if n.type=='BSDF_PRINCIPLED');bs.inputs['Specular IOR Level'].default_value=.10;bs.inputs['Roughness'].default_value=.38
nt=bpy.data.materials['Cornea | optical surface'].node_tree;mix=next(n for n in nt.nodes if n.type=='MIX_SHADER');prev=mix.inputs[0].links[0].from_socket;mult=nt.nodes.new('ShaderNodeMath');mult.operation='MULTIPLY';mult.inputs[1].default_value=.55;nt.links.new(prev,mult.inputs[0]);nt.links.new(mult.outputs[0],mix.inputs[0])
for n in nt.nodes:
    if n.type=='BSDF_GLOSSY':n.inputs['Roughness'].default_value=.05
# Subtle asymmetry from the supplied frontal photograph.
for v in key.data:
    x,y,z=v.co;front=smooth(-.4,-.9,y) if False else 1-smooth(-.9,-.4,y)
    v.co.x+=.035*math.exp(-(x/.20)**2-((z-2.58)/.21)**2)*front
    v.co.x+=.014*math.exp(-(x/.30)**4-((z-2.23)/.10)**2)*front
    v.co.y+=.009*(math.exp(-((x-.055)/.025)**2)+math.exp(-((x+.055)/.025)**2))*math.exp(-((z-3.19)/.12)**4)*front
uv=ob.data.uv_layers['Front reference']
for p in ob.data.polygons:
    for li in p.loop_indices:
        x,y,z=key.data[ob.data.loops[li].vertex_index].co;uv.data[li].uv=((130+129*x)/1536,1-(593-142*z)/1024)
# Keep an editable cage, and make a second, clean capped presentation surface.
source=bpy.data.collections.new('Sculpt source | hidden');bpy.context.scene.collection.children.link(source)
for co in list(ob.users_collection):co.objects.unlink(ob)
source.objects.link(ob)
deps=bpy.context.evaluated_depsgraph_get();me=bpy.data.meshes.new_from_object(ob.evaluated_get(deps),depsgraph=deps)
present=bpy.data.objects.new('Portrait | finished surface',me);bpy.context.scene.collection.objects.link(present)
bm=bmesh.new();bm.from_mesh(me);bmesh.ops.bisect_plane(bm,geom=list(bm.verts)+list(bm.edges)+list(bm.faces),dist=.00001,plane_co=(0,0,1.59),plane_no=(0,0,1),clear_inner=True)
edges=[e for e in bm.edges if e.is_boundary and all(abs(v.co.z-1.59)<.001 for v in e.verts)];bmesh.ops.holes_fill(bm,edges=edges,sides=0);bm.to_mesh(me);bm.free()
source.hide_render=True;source.hide_viewport=True
s=bpy.context.scene;s.unit_settings.system='METRIC';s.unit_settings.scale_length=.1
s.render.resolution_x=850;s.render.resolution_y=1050;s.cycles.samples=128
s.camera.location=(0,-12,2.75);s.camera.rotation_euler=(math.pi/2,0,0);s.camera.data.ortho_scale=3.15
s.render.filepath='/tmp/v2_polished_front.png';bpy.ops.render.render(write_still=True)
s.camera.location=(4.5,-10,2.92);s.camera.rotation_euler=(Vector((0,-.3,2.75))-s.camera.location).to_track_quat('-Z','Y').to_euler();s.render.filepath='/tmp/v2_polished_threequarter.png';bpy.ops.render.render(write_still=True)
s.camera.location=(12,-.30,2.75);s.camera.rotation_euler=(Vector((0,-.30,2.75))-s.camera.location).to_track_quat('-Z','Y').to_euler();s.render.filepath='/tmp/v2_polished_profile.png';bpy.ops.render.render(write_still=True)
bpy.ops.wm.save_as_mainfile(filepath='/home/never0n/Projects/new_desctop_MMO/assets/characters/reference_portrait/portrait_v2_polished.blend')
