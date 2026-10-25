import bpy, math, random
from mathutils import Vector
from mathutils.bvhtree import BVHTree
random.seed(81)
ob=bpy.data.objects['Portrait_Head']
offset=ob.location.copy()
for o in bpy.context.scene.objects:
    if o.type=='MESH': o.location-=offset
for o in list(bpy.context.scene.objects):
    if o.name.startswith(('Eye_','Iris_','Pupil_')): bpy.data.objects.remove(o,do_unlink=True)
skin=ob.data.materials[0]
for sign in [-1,1]:
    bpy.ops.mesh.primitive_uv_sphere_add(segments=64,ring_count=32,radius=.143,location=(sign*.324,-1.355,2.946))
    eye=bpy.context.object;eye.name='Eyeball_'+str(sign)
    eye.data.materials.append(skin)
    for name in ['Photo_Front','Photo_Profile','Photo_Back']:
        uv=eye.data.uv_layers.new(name=name)
        for p in eye.data.polygons:
            p.use_smooth=True
            for li in p.loop_indices:
                v=eye.data.vertices[eye.data.loops[li].vertex_index].co+eye.location
                uv.data[li].uv=((130+v.x*129)/1536,1-(175-(v.z-2.94)*137)/1024)
    attr=eye.data.color_attributes.new(name='ProjectionBlend',type='FLOAT_COLOR',domain='POINT')
    for a in attr.data:a.color=(0,0,0,1)
def mat(name,c,r):
    m=bpy.data.materials.new(name);m.use_nodes=True
    b=next(n for n in m.node_tree.nodes if n.type=='BSDF_PRINCIPLED')
    b.inputs['Base Color'].default_value=(*c,1);b.inputs['Roughness'].default_value=r
    return m
hair=mat('Hair | dark warm brown',(.014,.010,.008),.39)
grey=mat('Hair | scattered silver',(.12,.105,.085),.45)
deps=bpy.context.evaluated_depsgraph_get();ev=ob.evaluated_get(deps);me=ev.to_mesh()
vs=[v.co.copy() for v in me.vertices];polys=[tuple(p.vertices) for p in me.polygons]
bvh=BVHTree.FromPolygons(vs,polys)
def hairline(p):
    x,y,z=p
    if y<-.8:return 3.59-.10*min(1,abs(x)/.65)
    if y<-.3:return 3.48-(y+.8)*1.05
    return 2.85-(y+.3)*.80
scalp=[];beard=[];brows=[]
for p in me.polygons:
    c=p.center
    if c.z>hairline(c):scalp.append(p.index)
    x,y,z=c
    if y<-.58 and 1.72<z<2.82:
        lower=z<2.17 and (abs(x)>.21 or z<2.06)
        cheek=abs(x)>.28+(z-2.2)*.75 and z<2.67
        stache=2.27<z<2.48 and abs(x)<.37 and not(abs(x)<.10 and z>2.42)
        if lower or cheek or stache:beard.append(p.index)
    if y<-1.1 and .17<abs(x)<.59 and 3.025<z<3.14:brows.append(p.index)
ids=sorted(set(i for fi in scalp for i in polys[fi]));remap={i:j for j,i in enumerate(ids)}
capverts=[vs[i]+me.vertices[i].normal*.055 for i in ids]
cap=bpy.data.meshes.new('Scalp surface');cap.from_pydata(capverts,[],[[remap[i] for i in polys[fi]] for fi in scalp]);cap.update()
o=bpy.data.objects.new('Hair | continuous scalp',cap);bpy.context.collection.objects.link(o);cap.materials.append(hair)
for p in cap.polygons:p.use_smooth=True
def strands(name,faces,count,width,length):
    cu=bpy.data.curves.new(name,'CURVE');cu.dimensions='3D';cu.resolution_u=1;cu.bevel_depth=width;cu.bevel_resolution=1
    cu.materials.append(hair);cu.materials.append(grey)
    o=bpy.data.objects.new(name,cu);bpy.context.collection.objects.link(o)
    weights=[me.polygons[i].area for i in faces]
    for fi in random.choices(faces,weights=weights,k=count):
        poly=me.polygons[fi];inds=list(poly.vertices);a,b,c=[vs[i] for i in inds[:3]]
        u=random.random();v=random.random()
        if u+v>1:u=1-u;v=1-v
        root=a+(b-a)*u+(c-a)*v;n=poly.normal
        if name.startswith('Scalp'):
            flow=Vector((-.25*root.x,.9,.25 if root.y<-.4 else -.5))
            flow=(flow-n*flow.dot(n)).normalized();ln=length*random.uniform(.65,1.25)
            pts=[]
            for j in range(10):
                t=j/9;p=root+flow*ln*t
                hit,normal,_,_=bvh.find_nearest(p)
                lift=.065+.07*math.sin(math.pi*t)+.012*math.sin(t*9+root.x*17)
                pts.append(hit+normal*lift)
        else:
            flow=Vector((.17*(1 if root.x>0 else -1),-.06,-1))
            if name.startswith('Brows'):flow=Vector((.9*(1 if root.x>0 else -1),0,.15))
            ln=length*random.uniform(.4,1.3)
            pts=[root+n*(.004+.017*math.sin(math.pi*j/4))+flow*ln*j/4+Vector((math.sin(j*1.7+u*10)*.008,0,0))*j/4 for j in range(5)]
        sp=cu.splines.new('POLY');sp.points.add(len(pts)-1)
        sp.material_index=1 if random.random()<(.035 if name.startswith('Scalp') else .09) else 0
        for j,(p,co) in enumerate(zip(sp.points,pts)):
            p.co=(*co,1);p.radius=max(.12,(1-j/len(pts))**.7)
    return o
strands('Scalp | swept individual strands',scalp,14000,.0024,.58)
strands('Beard | individual strands',beard,10000,.0017,.065)
strands('Brows | individual strands',brows,1000,.0015,.045)
ev.to_mesh_clear()
for o in bpy.context.selected_objects:o.select_set(False)
s=bpy.context.scene;s.camera.location=(0,-12,2.65);s.camera.data.ortho_scale=3.7
s.render.filepath='/tmp/head_groom.png';bpy.ops.render.render(write_still=True)
print('Groom completed')
