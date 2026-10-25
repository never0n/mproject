"""Final corrections; run once on the saved v3_groomed.blend checkpoint."""
import bpy, bmesh, math, runpy
from pathlib import Path
from mathutils import Vector

ROOT = Path(__file__).resolve().parent.parent
head = bpy.data.objects['Portrait | finished surface']
scene = bpy.context.scene
if scene.get('portrait_v3_finished'):
    raise RuntimeError('Final corrections are already applied.')

def smooth(a,b,x):
    t=max(0,min(1,(x-a)/(b-a)))
    return t*t*(3-2*t)

def ear_delta(p):
    x,y,z=p
    w=smooth(.63,.73,abs(x))*smooth(-.52,-.35,y)*(1-smooth(-.055,.12,y))
    w*=smooth(2.50,2.57,z)*(1-smooth(3.03,3.29,z))
    return .10*(z-2.52)*w

# Small, smooth pinna correction, checked from both front and profile.
for v in head.data.vertices:
    v.co.z+=ear_delta(v.co)
for o in scene.objects:
    if o.hide_render or not o.name.startswith(('Hair_v3','Beard_v3')):
        continue
    if o.type=='CURVE':
        for sp in o.data.splines:
            dz=ear_delta(sp.points[0].co[:3])
            for p in sp.points:p.co.z+=dz
    elif o.type=='MESH':
        for v in o.data.vertices:v.co.z+=ear_delta(v.co)

# Replace the deformed lower cutoff with a tapered, closed neck continuation.
bm=bmesh.new();bm.from_mesh(head.data)
bmesh.ops.bisect_plane(bm,geom=list(bm.verts)+list(bm.edges)+list(bm.faces),
    dist=.000001,plane_co=(0,0,1.69),plane_no=(0,0,1),clear_inner=True,clear_outer=False)
bound=[e for e in bm.edges if e.is_boundary]
previous=0.0
for z in [1.61,1.53,1.45,1.37,1.29,1.21,1.12]:
    t=smooth(0,.4,1.69-z)
    result=bmesh.ops.extrude_edge_only(bm,edges=bound,use_normal_flip=False)
    for v in [a for a in result['geom'] if isinstance(a,bmesh.types.BMVert)]:
        v.co.x*=(1-.18*t)/(1-.18*previous)
        original_y=(v.co.y+.38-.12*previous)/(1-.18*previous)
        v.co.y=original_y*(1-.18*t)-.38+.12*t
        v.co.z=z
    bound=[e for e in result['geom'] if isinstance(e,bmesh.types.BMEdge)
           and e.is_boundary and all(abs(v.co.z-z)<.0001 for v in e.verts)]
    previous=t
bmesh.ops.holes_fill(bm,edges=bound,sides=0)
neck=[v for v in bm.verts if 1.15<v.co.z<1.78]
for _ in range(4):
    bmesh.ops.smooth_vert(bm,verts=neck,factor=.4,use_axis_x=True,use_axis_y=True,use_axis_z=False)
bmesh.ops.recalc_face_normals(bm,faces=list(bm.faces))
for f in bm.faces:f.smooth=True
assert not any(not e.is_manifold for e in bm.edges)
bm.to_mesh(head.data);bm.free();head.data.update()

for v in head.data.vertices:
    x,y,z=v.co
    if z<2.170 and y<-.95:
        v.co.z+=.0045*math.exp(-((x+.017)/.17)**4-((z-2.166)/.018)**2-((y+1.146)/.075)**2)

runpy.run_path(str(ROOT/'reconstruction/polish_groom_v3.py'))['polish_groom'](
    mouth_z=2.17,beard_tip_z=1.65)

# Uncalibrated ear photographs contain baked shadows. Soften those locally.
attr=head.data.color_attributes.new(name='Ear albedo correction',type='FLOAT_COLOR',domain='POINT')
for a,v in zip(attr.data,head.data.vertices):
    x,y,z=v.co
    w=.76*smooth(.665,.745,abs(x))*smooth(-.53,-.36,y)*(1-smooth(.00,.13,y))
    w*=smooth(2.45,2.57,z)*(1-smooth(3.10,3.30,z))
    a.color=(w,w,w,1)
nt=head.data.materials[0].node_tree
bs=next(n for n in nt.nodes if n.type=='BSDF_PRINCIPLED')
old=bs.inputs['Base Color'].links[0].from_socket
att=nt.nodes.new('ShaderNodeVertexColor');att.layer_name=attr.name
noise=nt.nodes.new('ShaderNodeTexNoise');noise.inputs['Scale'].default_value=35
ramp=nt.nodes.new('ShaderNodeValToRGB')
ramp.color_ramp.elements[0].color=(.325,.160,.105,1)
ramp.color_ramp.elements[1].color=(.42,.215,.143,1)
nt.links.new(noise.outputs['Fac'],ramp.inputs[0])
mix=nt.nodes.new('ShaderNodeMixRGB')
nt.links.new(att.outputs['Color'],mix.inputs[0]);nt.links.new(old,mix.inputs[1])
nt.links.new(ramp.outputs['Color'],mix.inputs[2]);nt.links.new(mix.outputs[0],bs.inputs['Base Color'])

for o in scene.objects:
    if o.name.startswith('Beard_v3 | soft dense'):
        for a in o.data.color_attributes['Beard root coverage'].data:
            c=a.color[0]*.70;a.color=(c,c,c,1)
    if o.type=='CURVE' and o.name.startswith('Hair_v3') and not o.hide_render:
        bad=[sp for sp in o.data.splines if any(abs(p.co.x)>.74 and
             ((p.co.y+.24)/.18)**2+((p.co.z-2.82)/.28)**2<1 for p in sp.points)]
        for sp in bad:o.data.splines.remove(sp)

for name,pos in [('01 | Front',(0,-12,2.65)),('02 | Three quarter',(6.5,-11,2.65)),
                 ('03 | Profile',(12,-.2,2.65))]:
    cam=bpy.data.objects[name];target=(0,-.2 if 'Profile' in name else -.30,2.65)
    cam.location=pos;cam.rotation_euler=(Vector(target)-cam.location).to_track_quat('-Z','Y').to_euler()
    cam.data.type='ORTHO';cam.data.ortho_scale=3.52
bpy.data.objects['Fill_v2'].data.energy=140
scene.camera=bpy.data.objects['02 | Three quarter']
scene.render.resolution_x=1000;scene.render.resolution_y=1250
scene.render.resolution_percentage=100
scene.cycles.samples=256;scene.cycles.adaptive_threshold=.006;scene.cycles.use_denoising=False
scene['portrait_v3_finished']=True
head['reference']='3befb5c3-89cf-4f08-aecb-885665c8eb38.png'
head['method']='Measured front and three-quarter fit; conservative profile refinement. Not a calibrated scan.'
head['editable_surface']=True
bpy.ops.file.pack_all()
bpy.ops.wm.save_as_mainfile(filepath=str(ROOT/'reference_portrait_v3.blend'),compress=True)
print('Saved finished v3:',len(head.data.vertices),'vertices',len(head.data.polygons),'faces')
