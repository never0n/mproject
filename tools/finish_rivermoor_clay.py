import bpy, bmesh, math, random, os
from mathutils import Vector
from math import sin, cos, pi
OUT='/home/never0n/Projects/new_desctop_MMO/assets/creatures/rivermoor_clay'
scene=bpy.context.scene; sculpt=bpy.data.collections['01 • RIVERMOOR sculpt']; studio=bpy.data.collections['02 • Studio']
clay=bpy.data.materials['Clay • warm limestone']
source=open('/home/never0n/Projects/new_desctop_MMO/tools/build_rivermoor_clay.py').read()
exec(source[source.index('def put('):source.index('body=[]')])
skin=bpy.data.objects['Rivermoor • unified sculpt']
bridges=[]
for s in [-1,1]:
    points=[(s*(.49+.30*cos(j/48*2*pi)),-.74,3.33+.326*sin(j/48*2*pi)) for j in range(49)]
    bridges.append(tube('Lid to face transition',points,[.071]*49,14,2))
bpy.context.view_layer.objects.active=skin
for mod in list(skin.modifiers): bpy.ops.object.modifier_apply(modifier=mod.name)
skin=join([skin,bpy.data.objects['Rivermoor • folds claws and skin relief']]+bridges,'Rivermoor • unified detailed sculpt')
rem=skin.modifiers.new('Fuse sculpt details','REMESH'); rem.mode='VOXEL'; rem.voxel_size=.012; rem.use_smooth_shade=True
bpy.ops.object.modifier_apply(modifier=rem.name)
sm=skin.modifiers.new('Blend sculpt transitions','SMOOTH'); sm.factor=.55; sm.iterations=3
bpy.ops.object.modifier_apply(modifier=sm.name)
cuts=[]
for s in [-1,1]:
    for k in range(3):
        o=tube('Finger separation',[(s*(.777+k*.084),-.79,.96),(s*(.80+k*.084),-.79,.82),(s*(.843+k*.084),-.82,.67),(s*(.86+k*.084),-.85,.56)],[.009,.014,.020,.026],12,5)
        # Make each cutter extend fully through the depth of the fingers.
        for v in o.data.vertices: v.co.y=-.80+(v.co.y+.80)*6
        cuts.append(o)
cutter=join(cuts,'Temporary finger cutters'); bpy.context.view_layer.objects.active=skin
bo=skin.modifiers.new('Separate long fingers','BOOLEAN'); bo.operation='DIFFERENCE'; bo.solver='EXACT'; bo.object=cutter
bpy.ops.object.modifier_apply(modifier=bo.name); bpy.data.objects.remove(cutter,do_unlink=True)
# Discard skin nodules that do not actually meet the body surface.
bm=bmesh.new(); bm.from_mesh(skin.data); bm.verts.ensure_lookup_table()
seen=set(); groups=[]
for v in bm.verts:
    if v.index in seen: continue
    stack=[v]; seen.add(v.index); group=[]
    while stack:
        cur=stack.pop(); group.append(cur)
        for e in cur.link_edges:
            other=e.other_vert(cur)
            if other.index not in seen:
                seen.add(other.index); stack.append(other)
    groups.append(group)
groups.sort(key=len,reverse=True)
bmesh.ops.delete(bm,geom=[v for group in groups[1:] for v in group],context='VERTS')
bm.to_mesh(skin.data); bm.free()
sub=skin.modifiers.new('Final sculpt finish','SUBSURF'); sub.levels=1; sub.render_levels=1

# Keep the neutral clay grain restrained at final resolution.
m=bpy.data.materials['Clay • warm limestone']; bump=next((n for n in m.node_tree.nodes if n.type=='BUMP'),None)
if bump is None:
    nt=m.node_tree; noise=nt.nodes.new('ShaderNodeTexNoise'); noise.inputs['Scale'].default_value=145; noise.inputs['Detail'].default_value=2.5
    bump=nt.nodes.new('ShaderNodeBump'); bump.inputs['Distance'].default_value=.018
    bs=next(n for n in nt.nodes if n.type=='BSDF_PRINCIPLED')
    nt.links.new(noise.outputs['Fac'],bump.inputs['Height']); nt.links.new(bump.outputs['Normal'],bs.inputs['Normal'])
bump.inputs['Strength'].default_value=.11
scene.cycles.use_denoising=False; scene.cycles.samples=160
scene.render.resolution_x=1600; scene.render.resolution_y=1600
scene.render.filepath=OUT+'/rivermoor_clay_hero.png'
# Additional saved cameras make the sculpt easy to inspect in the blend.
def camera(name,location,scale):
    d=bpy.data.cameras.new(name); o=bpy.data.objects.new(name,d); studio.objects.link(o)
    o.location=location; o.rotation_euler=(Vector((0,.22,1.90))-o.location).to_track_quat('-Z','Y').to_euler()
    d.type='ORTHO'; d.ortho_scale=scale
    return o
camera('Camera • back detail',(6,10,4.4),4.8)
camera('Camera • front',(0,-12,3.7),4.65)
camera('Camera • profile',(-12,0,3.9),4.85)
bpy.ops.object.select_all(action='DESELECT'); skin.select_set(True); bpy.context.view_layer.objects.active=skin
bpy.ops.wm.save_as_mainfile(filepath=OUT+'/rivermoor_clay.blend')
print('Final sculpt',len(skin.data.vertices),'vertices; saved with four cameras')
