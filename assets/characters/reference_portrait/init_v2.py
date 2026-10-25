import bpy, math, json
from mathutils import Vector
bpy.ops.wm.read_factory_settings(use_empty=True)
s=bpy.context.scene;s.name='Portrait_v2'
base=json.load(open('/home/never0n/Projects/new_desctop_MMO/assets/characters/reference_portrait/base_cc0.json'))
me=bpy.data.meshes.new('Anatomical quads');me.from_pydata(base['verts'],[],base['faces']);me.update()
ob=bpy.data.objects.new('Head_v2',me);s.collection.objects.link(ob)
for p in me.polygons:p.use_smooth=True
ob.shape_key_add(name='Anatomical base');ob.shape_key_add(name='Portrait likeness').value=1
mod=ob.modifiers.new('Sculpt surface','SUBSURF');mod.levels=2;mod.render_levels=2
m=bpy.data.materials.new('Sculpt neutral');m.use_nodes=True;b=next(n for n in m.node_tree.nodes if n.type=='BSDF_PRINCIPLED');b.inputs['Base Color'].default_value=(.25,.25,.25,1);b.inputs['Roughness'].default_value=.65;me.materials.append(m)
s.render.engine='CYCLES';s.cycles.samples=48;s.cycles.use_denoising=False;s.render.resolution_x=650;s.render.resolution_y=800;s.render.resolution_percentage=100
w=bpy.data.worlds.new('Studio');w.use_nodes=True;s.world=w;bg=next(n for n in w.node_tree.nodes if n.type=='BACKGROUND');bg.inputs[0].default_value=(.12,.12,.12,1);bg.inputs[1].default_value=.35
c=bpy.data.cameras.new('Portrait Camera');cam=bpy.data.objects.new('Portrait Camera',c);s.collection.objects.link(cam);s.camera=cam;c.type='ORTHO';c.ortho_scale=3.2;cam.location=(0,-12,2.75);cam.rotation_euler=(math.pi/2,0,0)
for name,loc,power,size in [('Key_v2',(-3,-5,6),450,4),('Fill_v2',(3,-4,3),140,3),('Rim_v2',(2,2,5),220,3)]:
    d=bpy.data.lights.new(name,'AREA');d.energy=power;d.shape='DISK';d.size=size;o=bpy.data.objects.new(name,d);s.collection.objects.link(o);o.location=loc;o.rotation_euler=(Vector((0,-.2,2.7))-o.location).to_track_quat('-Z','Y').to_euler()
exec(compile(open('/home/never0n/Projects/new_desctop_MMO/assets/characters/reference_portrait/refine_v2.py').read(),'refine_v2.py','exec'))
bpy.ops.wm.save_as_mainfile(filepath='/home/never0n/Projects/new_desctop_MMO/assets/characters/reference_portrait/portrait_v2_work.blend')
