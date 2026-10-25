import bpy,math,random,json
from mathutils import Vector
random.seed(88)
ob=bpy.data.objects['Head_v2'];k=ob.data.shape_keys.key_blocks['Portrait likeness']
for v in k.data:
    x,y,z=v.co
    v.co.y-=.11*math.exp(-(x/.17)**2-((z-2.58)/.23)**2)*max(0,min(1,(-y-.6)/.3))
    # slightly inset orbital corners to keep the sclera behind the eyelids
for o in bpy.context.scene.objects:
    if o.name.startswith(('Eye_v2','Iris_v2','Pupil_v2','Cornea_v2')):o.location.y+=.007
for me in [o.data for o in bpy.context.scene.objects if o.name.startswith(('Iris_v2','Pupil_v2','Cornea_v2'))]:
    for p in me.polygons:p.flip()
cornea=bpy.data.materials['Cornea | optical surface'];nt=cornea.node_tree;nt.nodes.clear()
out=nt.nodes.new('ShaderNodeOutputMaterial');mix=nt.nodes.new('ShaderNodeMixShader');tr=nt.nodes.new('ShaderNodeBsdfTransparent');gl=nt.nodes.new('ShaderNodeBsdfGlossy');gl.inputs['Roughness'].default_value=.025;fr=nt.nodes.new('ShaderNodeFresnel');fr.inputs['IOR'].default_value=1.376
nt.links.new(fr.outputs[0],mix.inputs[0]);nt.links.new(tr.outputs[0],mix.inputs[1]);nt.links.new(gl.outputs[0],mix.inputs[2]);nt.links.new(mix.outputs[0],out.inputs[0])
b=next(n for n in bpy.data.materials['Sclera | ivory wet tissue'].node_tree.nodes if n.type=='BSDF_PRINCIPLED');b.inputs['Base Color'].default_value=(.42,.39,.34,1)
ref=bpy.data.images.load('/home/never0n/Pictures/3befb5c3-89cf-4f08-aecb-885665c8eb38.png',check_existing=True);ref.pack()
for name in ['Front reference','Profile reference']:
    uv=ob.data.uv_layers.new(name=name)
    for p in ob.data.polygons:
        for li in p.loop_indices:
            x,y,z=k.data[ob.data.loops[li].vertex_index].co
            if name.startswith('Front'):
                px=130+129*x;py=593-142*z
            else:
                px=658+142*(y+.328)
                py=593-142*z-10*max(0,min(1,(-y-.33)/.8))
            uv.data[li].uv=(px/1536,1-py/1024)
attr=ob.data.color_attributes.new(name='Photo coverage',type='FLOAT_COLOR',domain='POINT')
for a,v in zip(attr.data,k.data):
    x,y,z=v.co
    side=max(0,min(1,(abs(x)-.52)/.21))
    neck=max(0,min(1,(1.98-z)/.20))
    behind=max(0,min(1,(y+.02)/.25))
    a.color=(side,max(neck,behind),0,1)
m=bpy.data.materials.new('Skin v2 | registered diffuse + microstructure');m.use_nodes=True;nt=m.node_tree;ns=nt.nodes;lk=nt.links;bs=next(n for n in ns if n.type=='BSDF_PRINCIPLED')
bs.inputs['Roughness'].default_value=.48;bs.inputs['Subsurface Weight'].default_value=.075;bs.inputs['Subsurface Radius'].default_value=(1,.45,.25);bs.inputs['Subsurface Scale'].default_value=.035
bs.inputs['Specular IOR Level'].default_value=.27
textures=[]
for name in ['Front reference','Profile reference']:
    uv=ns.new('ShaderNodeUVMap');uv.uv_map=name;t=ns.new('ShaderNodeTexImage');t.image=ref;lk.new(uv.outputs[0],t.inputs[0]);textures.append(t)
a=ns.new('ShaderNodeVertexColor');a.layer_name=attr.name;sep=ns.new('ShaderNodeSeparateColor');lk.new(a.outputs[0],sep.inputs[0])
mix=ns.new('ShaderNodeMixRGB');lk.new(sep.outputs[0],mix.inputs[0]);lk.new(textures[0].outputs[0],mix.inputs[1]);lk.new(textures[1].outputs[0],mix.inputs[2])
mix2=ns.new('ShaderNodeMixRGB');lk.new(sep.outputs[1],mix2.inputs[0]);lk.new(mix.outputs[0],mix2.inputs[1]);mix2.inputs[2].default_value=(.39,.23,.145,1)
lk.new(mix2.outputs[0],bs.inputs['Base Color'])
texco=ns.new('ShaderNodeTexCoord');noise=ns.new('ShaderNodeTexNoise');noise.inputs['Scale'].default_value=330;noise.inputs['Detail'].default_value=2;noise.inputs['Roughness'].default_value=.7;lk.new(texco.outputs['Object'],noise.inputs[0])
bump=ns.new('ShaderNodeBump');bump.inputs['Strength'].default_value=.17;bump.inputs['Distance'].default_value=.0025;lk.new(noise.outputs['Fac'],bump.inputs['Height']);lk.new(bump.outputs[0],bs.inputs['Normal'])
rough=ns.new('ShaderNodeMapRange');rough.inputs['To Min'].default_value=.39;rough.inputs['To Max'].default_value=.57;lk.new(noise.outputs['Fac'],rough.inputs['Value']);lk.new(rough.outputs[0],bs.inputs['Roughness'])
ob.data.materials.clear();ob.data.materials.append(m)
s=bpy.context.scene;s.camera.location=(0,-12,2.75);s.camera.rotation_euler=(math.pi/2,0,0);s.camera.data.ortho_scale=3.2;s.render.filepath='/tmp/v2_skin_front.png';bpy.ops.render.render(write_still=True)
s.camera.location=(4,-10,2.95);s.camera.rotation_euler=(Vector((0,-.3,2.75))-s.camera.location).to_track_quat('-Z','Y').to_euler();s.render.filepath='/tmp/v2_skin_threequarter.png';bpy.ops.render.render(write_still=True)
bpy.ops.wm.save_as_mainfile(filepath='/home/never0n/Projects/new_desctop_MMO/assets/characters/reference_portrait/portrait_v2_surface.blend')
