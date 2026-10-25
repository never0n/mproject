import bpy, math, random, json
from mathutils import Vector
random.seed(1029)
ob=bpy.data.objects['Head_v2']
key=ob.data.shape_keys.key_blocks['Portrait likeness']
for base,v in zip(ob.data.shape_keys.key_blocks[0].data,key.data):
    x,y,z=base.co
    front=max(0,min(1,(-y-.4)/.65))
    # Register silhouette and anatomical landmarks against front/profile views.
    y*=.70
    y-=.045*math.exp(-(x/.15)**2-((z-2.85)/.28)**2)*front
    x*=1+.10*math.exp(-((z-2.05)/.27)**2)
    x*=1+.09*math.exp(-((z-2.24)/.14)**2-(x/.35)**4)*front
    if abs(x)>.73:x=math.copysign(.73+(abs(x)-.73)*.6,x)
    eye=math.exp(-((abs(x)-.323)/.19)**4-((z-2.946)/.14)**4)*front
    z+=(z-2.946)*.36*eye
    # Stronger brow ridge, nasolabial transition and slightly fuller lower lip.
    y-=.025*math.exp(-((abs(x)-.33)/.29)**4-((z-3.075)/.105)**2)*front
    y-=.015*math.exp(-(x/.27)**4-((z-2.20)/.045)**2)*front
    if z<1.54:z=1.54
    v.co=(x,y,z)
def material(name,color,rough):
    m=bpy.data.materials.new(name);m.use_nodes=True
    b=next(n for n in m.node_tree.nodes if n.type=='BSDF_PRINCIPLED')
    b.inputs['Base Color'].default_value=(*color,1);b.inputs['Roughness'].default_value=rough
    return m
sclera=material('Sclera | ivory wet tissue',(.58,.54,.48),.20)
b=next(n for n in sclera.node_tree.nodes if n.type=='BSDF_PRINCIPLED');b.inputs['Subsurface Weight'].default_value=.04
iris=material('Iris | hazel brown radial fibres',(.07,.028,.009),.29)
nt=iris.node_tree;b=next(n for n in nt.nodes if n.type=='BSDF_PRINCIPLED');a=nt.nodes.new('ShaderNodeVertexColor');a.layer_name='Iris';nt.links.new(a.outputs[0],b.inputs['Base Color'])
black=material('Pupil | light absorption',(.0004,.0003,.0002),.26)
cornea=material('Cornea | optical surface',(.98,.98,.98),.015)
b=next(n for n in cornea.node_tree.nodes if n.type=='BSDF_PRINCIPLED');b.inputs['Transmission Weight'].default_value=1;b.inputs['IOR'].default_value=1.376
for sign in [-1,1]:
    center=Vector((sign*.324,-.891,2.946));radius=.145
    bpy.ops.mesh.primitive_uv_sphere_add(segments=96,ring_count=64,radius=radius,location=center)
    eye=bpy.context.object;eye.name='Eye_v2_'+str(sign);eye.data.materials.append(sclera)
    for p in eye.data.polygons:p.use_smooth=True
    # Iris sits immediately below the curved cornea, with a real circular pupil.
    vs=[];fs=[];colors=[];segments=192;rings=25
    strand=[random.uniform(.35,1.25) for _ in range(segments)]
    for i in range(rings):
        r=.023+(.068-.023)*i/(rings-1)
        for j in range(segments):
            a=math.tau*j/segments
            yy=-math.sqrt(radius*radius-r*r)-.001
            vs.append(tuple(center+Vector((r*math.cos(a),yy,r*math.sin(a)))))
            t=i/(rings-1);lum=strand[j]*(.6+.25*math.sin(t*37+j*.2)+.15*random.random())
            edge=1-.8*max(0,(t-.85)/.15)
            colors.append((.11*lum*edge,.047*lum*edge,.015*lum*edge,1))
    for i in range(rings-1):
        for j in range(segments):
            a=i*segments+j;c=i*segments+(j+1)%segments;fs.append((a,c,c+segments,a+segments))
    me=bpy.data.meshes.new('Iris microstructure');me.from_pydata(vs,[],fs);o=bpy.data.objects.new('Iris_v2_'+str(sign),me);bpy.context.collection.objects.link(o);me.materials.append(iris)
    attr=me.color_attributes.new(name='Iris',type='FLOAT_COLOR',domain='POINT')
    for a,c in zip(attr.data,colors):a.color=c
    for p in me.polygons:p.use_smooth=True
    for name,rad,mat,lift in [('Pupil',.0235,black,.0015),('Cornea',.069,cornea,.008)]:
        vv=[tuple(center+Vector((0,-radius-lift,0)))];ff=[]
        for i in range(1,17):
            r=rad*i/16
            for j in range(segments):
                a=j*math.tau/segments;yy=-math.sqrt(radius*radius-r*r)-lift*max(0,1-(r/rad)**2)
                vv.append(tuple(center+Vector((r*math.cos(a),yy,r*math.sin(a)))))
        for j in range(segments):ff.append((0,1+j,1+(j+1)%segments))
        for i in range(15):
            for j in range(segments):
                a=1+i*segments+j;c=1+i*segments+(j+1)%segments;ff.append((a,c,c+segments,a+segments))
        me=bpy.data.meshes.new(name);me.from_pydata(vv,[],ff);o=bpy.data.objects.new(name+'_v2_'+str(sign),me);bpy.context.collection.objects.link(o);me.materials.append(mat)
        for p in me.polygons:p.use_smooth=True
s=bpy.context.scene;s.camera.location=(0,-12,2.75);s.camera.rotation_euler=(math.pi/2,0,0);s.camera.data.ortho_scale=3.2
s.render.filepath='/tmp/v2_fitted_front.png';bpy.ops.render.render(write_still=True)
s.camera.location=(12,-.38,2.75);s.camera.rotation_euler=(Vector((0,-.38,2.75))-s.camera.location).to_track_quat('-Z','Y').to_euler();s.render.filepath='/tmp/v2_fitted_profile.png';bpy.ops.render.render(write_still=True)
print('Likeness proportions and optical eyes built')
