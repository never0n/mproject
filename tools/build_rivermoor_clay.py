"""River moor critter sculpt, executed in the live Blender session via MCP."""
import bpy, math, random, os
from mathutils import Vector
from math import sin, cos, pi

OUT = '/home/never0n/Projects/new_desctop_MMO/assets/creatures/rivermoor_clay'
os.makedirs(OUT, exist_ok=True)
random.seed(38)
scene = bpy.data.scenes.new('Rivermoor • Clay studio')
bpy.context.window.scene = scene
sculpt = bpy.data.collections.new('01 • RIVERMOOR sculpt')
scene.collection.children.link(sculpt)
studio = bpy.data.collections.new('02 • Studio')
scene.collection.children.link(studio)

def mat(name, color, roughness=.72):
    m=bpy.data.materials.new(name); m.use_nodes=True
    n=next(n for n in m.node_tree.nodes if n.type=='BSDF_PRINCIPLED')
    n.inputs['Base Color'].default_value=(*color,1)
    n.inputs['Roughness'].default_value=roughness
    m.diffuse_color=(*color,1)
    return m
clay=mat('Clay • warm limestone',(.46,.405,.335))
eyeclay=mat('Clay • polished eyeballs',(.46,.405,.335),.3)
ground=mat('Studio • warm graphite',(.16,.175,.185),.84)

def put(o, material=clay, collection=sculpt):
    for c in list(o.users_collection): c.objects.unlink(o)
    collection.objects.link(o)
    if o.type=='MESH':
        o.data.materials.append(material)
        for p in o.data.polygons: p.use_smooth=True
    return o

def mesh(name, vs, fs, material=clay):
    me=bpy.data.meshes.new(name); me.from_pydata(vs,[],fs); me.update()
    o=bpy.data.objects.new(name,me); sculpt.objects.link(o)
    me.materials.append(material)
    for p in me.polygons: p.use_smooth=True
    return o

def ell(name,loc,scale,material=clay,seg=40,rings=28):
    bpy.ops.mesh.primitive_uv_sphere_add(segments=seg,ring_count=rings,location=loc)
    o=bpy.context.object; o.name=name; o.scale=scale
    bpy.ops.object.transform_apply(location=False,rotation=False,scale=True)
    return put(o,material)

def tube(name,points,radii,sides=12,steps=6,material=clay):
    # Catmull-Rom interpolation and a parallel transported cross section.
    ps=[Vector(p) for p in points]; samples=[]; rs=[]
    for i in range(len(ps)-1):
        a,b,c,d=ps[max(0,i-1)],ps[i],ps[i+1],ps[min(len(ps)-1,i+2)]
        for j in range(steps):
            t=j/steps
            samples.append(.5*((2*b)+(-a+c)*t+(2*a-5*b+4*c-d)*t*t+(-a+3*b-3*c+d)*t*t*t))
            rs.append(radii[i]*(1-t)+radii[i+1]*t)
    samples.append(ps[-1]); rs.append(radii[-1]); vs=[]; fs=[]
    for i,p in enumerate(samples):
        tangent=(samples[min(i+1,len(samples)-1)]-samples[max(i-1,0)]).normalized()
        u=tangent.cross(Vector((0,1,0)))
        if u.length<.01: u=tangent.cross(Vector((1,0,0)))
        u.normalize(); v=tangent.cross(u).normalized()
        for k in range(sides):
            q=p+rs[i]*(u*cos(k*2*pi/sides)+v*sin(k*2*pi/sides)); vs.append(tuple(q))
        if i:
            for k in range(sides):
                a=(i-1)*sides+k; b=(i-1)*sides+(k+1)%sides
                fs.append((a,b,b+sides,a+sides))
    fs.append(tuple(reversed(range(sides))))
    fs.append(tuple((len(samples)-1)*sides+k for k in range(sides)))
    return mesh(name,vs,fs,material)

def join(obs,name):
    bpy.ops.object.select_all(action='DESELECT')
    for o in obs: o.select_set(True)
    bpy.context.view_layer.objects.active=obs[0]; bpy.ops.object.join()
    obs[0].name=name; return obs[0]

body=[]
body.append(ell('Torso',(0,.16,1.74),(.60,.46,.90)))
body.append(ell('Hunched shoulders',(0,.22,2.28),(.64,.46,.60)))
body.append(ell('Forward throat',(0,-.17,2.52),(.43,.39,.64)))
body.append(ell('Cranium',(0,-.16,3.14),(.82,.55,.65)))
body.append(ell('Lower jaw',(0,-.52,2.86),(.64,.40,.24)))
body.append(ell('Broad amphibian muzzle',(0,-.64,3.035),(.65,.40,.235)))
body.append(ell('Nose bridge',(0,-.69,3.25),(.265,.28,.25)))
for s in [-1,1]:
    body.append(ell('Cheek',(s*.59,-.38,3.00),(.27,.34,.32)))
    body.append(ell('Eye socket',(s*.48,-.45,3.32),(.37,.33,.40)))
    body.append(tube('Arm',[(s*.50,.02,2.32),(s*.74,-.01,1.93),(s*.92,-.20,1.65),(s*.89,-.46,1.30),(s*.85,-.61,1.10)],[.24,.20,.17,.125,.12],18))
    body.append(ell('Palm',(s*.86,-.64,1.06),(.19,.125,.235)))
    body.append(ell('Haunch',(s*.47,.20,1.15),(.34,.38,.55)))
    body.append(tube('Bent shin',[(s*.49,.13,1.23),(s*.68,-.17,.79),(s*.68,.09,.43),(s*.70,-.02,.22)],[.30,.22,.13,.135],18))
    body.append(ell('Foot',(s*.70,-.21,.16),(.25,.36,.135)))
    for k in range(4):
        x=s*(.70+(k-1.5)*.14)
        body.append(tube('Toe',[(s*.7,-.22,.17),(x,-.40,.14),(x+s*.035,-.65+abs(k-1.5)*.06,.09)],[.10,.07,.035]))
    for k in range(4):
        x=s*(.735+k*.084); z=1.01-abs(k-1.3)*.015
        body.append(tube('Long finger',[(x,-.69,z),(x+s*.025,-.76,z-.19),(x+s*.075,-.77,z-.34),(x+s*.085,-.83,z-.40)],[.061,.053,.037,.017]))
    body.append(tube('Opposing thumb',[(s*.73,-.62,1.15),(s*.65,-.75,1.03),(s*.62,-.85,.88),(s*.65,-.88,.83)],[.082,.063,.04,.02]))
body.append(tube('Long curling tail',[(0,.48,1.28),(-.09,.87,1.00),(-.29,1.24,.67),(-.63,1.61,.39),(-1.15,1.79,.26),(-1.65,1.70,.34),(-1.94,1.49,.53)],[.27,.25,.185,.13,.087,.045,.004],24,10))

for s in [-1,1]:
    # Flared ears with a thick leading edge and a soft drooping lower fold.
    profile=[(.59,.0,3.30,.21,.28),(.83,.015,3.39,.19,.29),(1.03,.04,3.49,.14,.23),(1.27,.05,3.58,.065,.12),(1.53,.07,3.59,.004,.004)]
    vs=[]; fs=[]; n=24
    for i,(x,y,z,ry,rz) in enumerate(profile):
        for j in range(n):
            t=2*pi*j/n; vs.append((s*x,y+ry*cos(t),z+rz*sin(t)))
        if i:
            for j in range(n): fs.append(((i-1)*n+j,(i-1)*n+(j+1)%n,i*n+(j+1)%n,i*n+j))
    fs.extend([tuple(reversed(range(n))),tuple((len(profile)-1)*n+j for j in range(n))])
    o=mesh('Swept pointed ear',vs,fs); sub=o.modifiers.new('Organic ear','SUBSURF'); sub.levels=2
    bpy.context.view_layer.objects.active=o; bpy.ops.object.modifier_apply(modifier=sub.name); body.append(o)

skin=join(body,'Rivermoor • unified sculpt')
rem=skin.modifiers.new('Sculpt union','REMESH'); rem.mode='VOXEL'; rem.voxel_size=.021; rem.use_smooth_shade=True
bpy.context.view_layer.objects.active=skin; bpy.ops.object.modifier_apply(modifier=rem.name)
sm=skin.modifiers.new('Relax sculpt','SMOOTH'); sm.factor=.65; sm.iterations=5; bpy.ops.object.modifier_apply(modifier=sm.name)
sub=skin.modifiers.new('Surface finish','SUBSURF'); sub.levels=1; sub.render_levels=1
print('Core sculpt:',len(skin.data.vertices),'vertices')

# Studio camera, saved with a useful working viewport.
def aim(o,point): o.rotation_euler=(Vector(point)-o.location).to_track_quat('-Z','Y').to_euler()
camdata=bpy.data.cameras.new('Hero 3-quarter'); cam=bpy.data.objects.new('Camera • hero',camdata); studio.objects.link(cam)
cam.location=(-7.6,-10,4.6); aim(cam,(-.05,.20,1.90)); camdata.type='ORTHO'; camdata.ortho_scale=4.85; scene.camera=cam
bpy.ops.mesh.primitive_plane_add(size=200)
floor=put(bpy.context.object,ground,studio); floor.name='Infinite studio floor'; floor.location.z=-.025
def light(name,loc,power,size):
    d=bpy.data.lights.new(name,'AREA'); d.energy=power; d.size=size
    o=bpy.data.objects.new(name,d); studio.objects.link(o); o.location=loc; aim(o,(0,0,1.8))
light('Key • large softbox',(-3,-4,7),650,4)
light('Fill • softbox',(4,-1,4.5),350,3)
light('Rim • softbox',(-2,4,6),850,3)
scene.world=bpy.data.worlds.new('Studio ambient'); scene.world.use_nodes=True
bg=next(n for n in scene.world.node_tree.nodes if n.type=='BACKGROUND'); bg.inputs[0].default_value=(.20,.22,.25,1); bg.inputs[1].default_value=.35
scene.render.resolution_x=1500; scene.render.resolution_y=1500; scene.render.resolution_percentage=100
scene.render.image_settings.file_format='PNG'
try: scene.render.engine='CYCLES'
except TypeError: pass
if scene.render.engine=='CYCLES':
    scene.cycles.samples=48; scene.cycles.use_denoising=False
for area in bpy.context.screen.areas:
    if area.type=='VIEW_3D':
        area.spaces.active.region_3d.view_perspective='CAMERA'
scene.render.filepath=OUT+'/rivermoor_clay_hero.png'
print('Core and studio ready')
