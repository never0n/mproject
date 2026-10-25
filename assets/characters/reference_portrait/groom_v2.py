import bpy, math, random
from mathutils import Vector
from mathutils.bvhtree import BVHTree
random.seed(440)
ob=bpy.data.objects['Head_v2'];key=ob.data.shape_keys.key_blocks['Portrait likeness']
def smooth(a,b,x):
    t=max(0,min(1,(x-a)/(b-a)));return t*t*(3-2*t)
def hairline(p):
    x,y,z=p;ax=abs(x)
    if y<-.84:return 3.61-.20*(ax/.74)**2
    if y<-.53:return 3.48-(y+.84)*1.45
    if y<-.12:return 3.05-(y+.53)*.24
    return 2.94-(y+.12)*2.0
def hairmask(p):
    x,y,z=p
    # Ear opening and tapered temple growth.
    if abs(x)>.66 and -.48<y<-.13 and z<3.15:return 0
    return smooth(-.012,.035,z-hairline(p))
def beardmask(p):
    x,y,z=p;ax=abs(x)
    neckline=1.82+.64*max(0,y+1.05)
    jaw=smooth(neckline,neckline+.07,z)*(1-smooth(-.28,-.12,y))
    top=2.20+.74*ax
    face=1-smooth(top-.025,top+.035,z)
    lip=1-smooth(.28,.34,ax)
    lip*=smooth(2.105,2.15,z)*(1-smooth(2.27,2.29,z))
    face*=1-lip
    stache=(1-smooth(.30,.36,ax))*smooth(2.29,2.33,z)*(1-smooth(2.43,2.47,z))
    if ax<.10:stache*=1-smooth(2.40,2.44,z)
    soul=(1-smooth(.09,.13,ax))*smooth(2.07,2.11,z)*(1-smooth(2.15,2.19,z))
    return min(1,max(face,stache,soul))*jaw
# Increase brown irises to the reference diameter and remove the surprised white rim.
for o in bpy.context.scene.objects:
    if o.name.startswith(('Iris_v2','Cornea_v2')):
        sign=-1 if o.name.endswith('-1') else 1;cx=sign*.324;cz=2.946
        for v in o.data.vertices:
            dx=v.co.x-cx;dz=v.co.z-cz;v.co.x=cx+dx*1.23;v.co.z=cz+dz*1.23
for a,v in zip(ob.data.color_attributes['Photo coverage'].data,key.data):
    x,y,z=v.co;side=a.color[0]
    neck=1-smooth(1.72,1.91,z)
    behind=smooth(-.16,.13,y)
    a.color=(side,max(neck,behind),0,1)
coverage=ob.data.color_attributes.new(name='Follicle coverage',type='FLOAT_COLOR',domain='POINT')
for a,v in zip(coverage.data,key.data):
    x,y,z=v.co
    skin=smooth(-.55,-.18,y)*(1-smooth(2.48,2.76,z))
    a.color=(hairmask(v.co),beardmask(v.co),skin,1)
mat=ob.data.materials[0];nt=mat.node_tree;bs=next(n for n in nt.nodes if n.type=='BSDF_PRINCIPLED');prev=bs.inputs['Base Color'].links[0].from_socket
att=nt.nodes.new('ShaderNodeVertexColor');att.layer_name=coverage.name;sep=nt.nodes.new('ShaderNodeSeparateColor');nt.links.new(att.outputs[0],sep.inputs[0])
for channel,col,strength in [(2,(.35,.195,.12,1),1),(1,(.018,.012,.008,1),.67),(0,(.009,.006,.004,1),1)]:
    f=nt.nodes.new('ShaderNodeMath');f.operation='MULTIPLY';nt.links.new(sep.outputs[channel],f.inputs[0]);f.inputs[1].default_value=strength
    mix=nt.nodes.new('ShaderNodeMixRGB');nt.links.new(f.outputs[0],mix.inputs[0]);nt.links.new(prev,mix.inputs[1]);mix.inputs[2].default_value=col;prev=mix.outputs[0]
nt.links.new(prev,bs.inputs['Base Color'])
def hairmat(name,color,rough):
    m=bpy.data.materials.new(name);m.use_nodes=True;b=next(n for n in m.node_tree.nodes if n.type=='BSDF_PRINCIPLED');b.inputs['Base Color'].default_value=(*color,1);b.inputs['Roughness'].default_value=rough;b.inputs['Specular IOR Level'].default_value=.32
    return m
mats=[hairmat('Hair | espresso',(.010,.007,.0045),.40),hairmat('Hair | chestnut',(.021,.013,.008),.43),hairmat('Hair | muted silver',(.105,.093,.078),.46)]
deps=bpy.context.evaluated_depsgraph_get();ev=ob.evaluated_get(deps);me=ev.to_mesh();vs=[v.co.copy() for v in me.vertices];polys=[tuple(p.vertices) for p in me.polygons];bvh=BVHTree.FromPolygons(vs,polys)
scalp=[p.index for p in me.polygons if hairmask(p.center)>.2]
beard=[p.index for p in me.polygons if beardmask(p.center)>.1]
brow=[p.index for p in me.polygons if -.99>p.center.y and .15<abs(p.center.x)<.6 and abs(p.center.z-(3.08+.035*math.sin((abs(p.center.x)-.15)*7)))<.035]
def root(fi):
    p=me.polygons[fi];a,b,c=[vs[i] for i in p.vertices[:3]];u=random.random();v=random.random()
    if u+v>1:u=1-u;v=1-v
    return a+(b-a)*u+(c-a)*v,p.normal.copy()
def curve(name,width):
    cu=bpy.data.curves.new(name,'CURVE');cu.dimensions='3D';cu.resolution_u=1;cu.bevel_depth=width;cu.bevel_resolution=0
    for m in mats:cu.materials.append(m)
    o=bpy.data.objects.new(name,cu);bpy.context.collection.objects.link(o);return cu
def spline(cu,points,grey=.01):
    sp=cu.splines.new('POLY');sp.points.add(len(points)-1);r=random.random();sp.material_index=2 if r<grey else (1 if r<.17 else 0)
    for i,(p,co) in enumerate(zip(sp.points,points)):
        p.co=(*co,1);p.radius=max(.08,(1-i/(len(points)-.1))**.48)*random.uniform(.85,1.1)
    return sp
cu=curve('Hair_v2 | layered swept waves',.00095)
weights=[me.polygons[i].area for i in scalp]
for fi in random.choices(scalp,weights=weights,k=2200):
    origin,n=root(fi)
    if hairmask(origin)<random.random():continue
    length=random.uniform(.55,1.03);step=length/28
    p=origin.copy();guide=[];phase=random.uniform(0,math.tau)
    for j in range(29):
        t=j/28;hit,n,_,_=bvh.find_nearest(p)
        flow=Vector((-.36,.9,.12 if hit.y<-.30 else -.7))
        if abs(hit.x)>.62:flow.x=-.08*hit.x;flow.z=-.25
        flow-=n*flow.dot(n)
        if flow.length<.1:flow=Vector((0,0,-1))
        flow.normalize();lateral=n.cross(flow).normalized()
        lift=.003+(.065 if origin.z>3.4 else .025)*math.sin(math.pi*t)**.8
        wave=lateral*(.012*math.sin(t*math.tau*1.3+phase)*math.sin(math.pi*t))
        guide.append((hit+n*lift+wave,n,lateral))
        p=hit+flow*step
    for child in range(10):
        ox=random.uniform(-.025,.025);oz=random.uniform(-.012,.012);points=[]
        for j,(p,n,lat) in enumerate(guide):
            t=j/28;spread=.95-.3*t
            points.append(p+lat*ox*spread+n*oz*math.sin(math.pi*t))
        spline(cu,points,.012)
# Short undercoat follows the scalp and closes the gaps between longer locks.
short=curve('Hair_v2 | fine continuous undercoat',.00085)
for fi in random.choices(scalp,weights=weights,k=17000):
    p,n=root(fi)
    if hairmask(p)<random.random():continue
    flow=Vector((-.30,.9,-.15 if p.y<-.35 else -.75));flow=(flow-n*flow.dot(n)).normalized()
    ln=random.uniform(.13,.32);points=[]
    for j in range(7):
        t=j/6;hit,normal,_,_=bvh.find_nearest(p+flow*ln*t)
        points.append(hit+normal*(.002+.008*math.sin(math.pi*t)))
    spline(short,points,.009)
cu=curve('Beard_v2 | dense tapered follicles',.0008)
weights=[me.polygons[i].area*beardmask(me.polygons[i].center) for i in beard]
for fi in random.choices(beard,weights=weights,k=30000):
    p,n=root(fi)
    if beardmask(p)<random.random():continue
    x,y,z=p;ax=abs(x)
    stache=z>2.29 and ax<.36
    length=random.uniform(.012,.027) if stache else random.uniform(.020,.046)
    if z<2.05:length*=1.20
    flow=Vector((.30*(1 if x>0 else -1) if stache else .1*x,-.12,-.75 if stache else -1)).normalized()
    phase=random.uniform(0,math.tau);side=n.cross(flow).normalized();points=[]
    for j in range(7):
        t=j/6
        points.append(p+flow*length*t+n*(.002+.012*math.sin(math.pi*t))+side*.0025*math.sin(t*8+phase)*t)
    spline(cu,points,.025 if z<2.14 else .004)
cu=curve('Brows_v2 | directional fine hairs',.00072)
if brow:
    for fi in random.choices(brow,weights=[me.polygons[i].area for i in brow],k=550):
        p,n=root(fi);sign=1 if p.x>0 else -1;flow=Vector((sign*.8,0,.25)).normalized();ln=random.uniform(.018,.045)
        spline(cu,[p+n*.003+flow*ln*t+n*.003*math.sin(math.pi*t) for t in [0,.25,.5,.75,1]],.002)
ev.to_mesh_clear()
s=bpy.context.scene;s.cycles.samples=64;s.render.filepath='/tmp/v2_groom_threequarter.png';bpy.ops.render.render(write_still=True)
s.camera.location=(0,-12,2.75);s.camera.rotation_euler=(math.pi/2,0,0);s.render.filepath='/tmp/v2_groom_front.png';bpy.ops.render.render(write_still=True)
bpy.ops.wm.save_as_mainfile(filepath='/home/never0n/Projects/new_desctop_MMO/assets/characters/reference_portrait/portrait_v2_groom.blend')
