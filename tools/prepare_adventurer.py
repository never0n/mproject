"""Blender 4.0: --background --factory-startup --disable-autoexec Defender.blend --python tools/prepare_adventurer.py -- OUTPUT.glb
Requires NumPy in Blender's Python environment. See assets/characters/ATTRIBUTION.md.
"""
import sys, os, bpy
output=os.path.abspath(sys.argv[sys.argv.index('--')+1])
mesh=bpy.data.objects['Defender']
import bpy
for m in mesh.data.materials:
 m.use_nodes=True
 nodes=m.node_tree.nodes; nodes.clear()
 out=nodes.new('ShaderNodeOutputMaterial');bs=nodes.new('ShaderNodeBsdfPrincipled');m.node_tree.links.new(bs.outputs['BSDF'],out.inputs['Surface'])
 tex=nodes.new('ShaderNodeTexImage');tex.image=bpy.data.images['DoneWeps.png' if 'Weapon' in m.name else 'Done3.png'];m.node_tree.links.new(tex.outputs['Color'],bs.inputs['Base Color'])
 bs.inputs['Roughness'].default_value=.65;bs.inputs['Metallic'].default_value=.25
 normaltex=nodes.new('ShaderNodeTexImage');normaltex.image=bpy.data.images['DoneWeps_NM2.png' if 'Weapon' in m.name else 'Done2_NMoverlay2.png'];normaltex.image.colorspace_settings.name='Non-Color';n=nodes.new('ShaderNodeNormalMap');m.node_tree.links.new(normaltex.outputs['Color'],n.inputs['Color']);m.node_tree.links.new(n.outputs['Normal'],bs.inputs['Normal'])

rig=bpy.data.objects['Defender_Ske']; mesh=bpy.data.objects['Defender']; scene=bpy.context.scene
rig.animation_data.action=None
for t in rig.animation_data.nla_tracks:t.mute=False
samples={}
for name,frames in [('Idle',range(143,166)),('Walk',range(3,63)),('Run',range(64,84))]:
 samples[name]=[]
 for f in frames:
  scene.frame_set(f); row={}
  for p in rig.pose.bones:
   mat=p.bone.matrix_local.inverted() @ p.matrix if not p.parent else p.bone.matrix_local.inverted() @ p.parent.bone.matrix_local @ p.parent.matrix.inverted() @ p.matrix
   row[p.name]=mat.decompose()
  samples[name].append(row)
rig.animation_data_clear()
for a in list(bpy.data.actions): bpy.data.actions.remove(a)
# The source polearm clips through the floor in locomotion clips. Keep the adventurer unarmed.
import bmesh
bm=bmesh.new(); bm.from_mesh(mesh.data)
weapon_slots={i for i,m in enumerate(mesh.data.materials) if 'Weapon' in m.name}
bmesh.ops.delete(bm, geom=[f for f in bm.faces if f.material_index in weapon_slots], context='FACES')
bm.to_mesh(mesh.data); bm.free()
mesh.data.validate()
for p in rig.pose.bones:
 for c in list(p.constraints):p.constraints.remove(c)
 p.rotation_mode='QUATERNION'
for o in list(bpy.data.objects):
 if o not in [rig,mesh]:bpy.data.objects.remove(o,do_unlink=True)
rig.animation_data_create()
for name,rows in samples.items():
 a=bpy.data.actions.new(name);rig.animation_data.action=a
 for f,row in enumerate(rows,1):
  for namebone,(loc,rot,scale) in row.items():
   p=rig.pose.bones[namebone];p.location=loc;p.rotation_quaternion=rot;p.scale=scale
   for prop in ['location','rotation_quaternion','scale']:p.keyframe_insert(prop,frame=f,group=p.name)
 for fc in a.fcurves:
  for k in fc.keyframe_points:k.interpolation='LINEAR'
 rig.animation_data.action=None
 t=rig.animation_data.nla_tracks.new();t.name=name;t.strips.new(name,1,a)
for t in rig.animation_data.nla_tracks:t.mute=t.name!='Idle'
scene.frame_set(1)
bpy.ops.object.select_all(action='DESELECT');rig.select_set(True);mesh.select_set(True);bpy.context.view_layer.objects.active=rig
for t in rig.animation_data.nla_tracks:t.mute=False
bpy.ops.export_scene.gltf(filepath=output,export_format='GLB',use_selection=True,export_animations=True,export_nla_strips=True,export_force_sampling=True)
