"""Reference-matched organic materials for the V2 critter; run via Blender MCP."""
import bpy, os, math
from array import array
from mathutils import Vector
OUT='/home/never0n/Projects/new_desctop_MMO/assets/creatures/rivermoor_textured'
os.makedirs(OUT,exist_ok=True)
s=bpy.context.scene
sculpt=bpy.data.collections['01 • RIVERMOOR sculpt']
anchor=bpy.data.objects.new('Texture coordinates • continuous across sculpt',None)
sculpt.objects.link(anchor)

def material(name,rough=.55):
    m=bpy.data.materials.new(name); m.use_nodes=True
    nt=m.node_tree; bs=next(n for n in nt.nodes if n.type=='BSDF_PRINCIPLED')
    bs.inputs['Roughness'].default_value=rough
    coord=nt.nodes.new('ShaderNodeTexCoord');coord.object=anchor;coord.location=(-1000,0)
    return m,nt,bs,coord.outputs['Object']
def noise(nt,vector,scale,detail=3):
    n=nt.nodes.new('ShaderNodeTexNoise');n.inputs['Scale'].default_value=scale;n.inputs['Detail'].default_value=detail
    nt.links.new(vector,n.inputs['Vector']);return n.outputs['Fac']
def ramp(nt,f,stops,label):
    n=nt.nodes.new('ShaderNodeValToRGB');n.label=label;n.name=label
    r=n.color_ramp
    for i,(p,c) in enumerate(stops):
        e=r.elements[i] if i<2 else r.elements.new(p)
        e.position=p;e.color=(*c,1)
    nt.links.new(f,n.inputs['Fac']);return n.outputs['Color']
def mix(nt,a,b,f=1,mode='MULTIPLY'):
    n=nt.nodes.new('ShaderNodeMixRGB');n.blend_type=mode;n.inputs[0].default_value=f
    nt.links.new(a,n.inputs[1]);nt.links.new(b,n.inputs[2]);return n.outputs[0]
def bump(nt,bs,f,strength=.18,distance=.012):
    n=nt.nodes.new('ShaderNodeBump');n.inputs['Strength'].default_value=strength;n.inputs['Distance'].default_value=distance
    nt.links.new(f,n.inputs['Height']);nt.links.new(n.outputs['Normal'],bs.inputs['Normal'])
def assign(o,m):
    o.data.materials.clear();o.data.materials.append(m)
    for p in o.data.polygons:p.material_index=0
def coords(o):
    return [tuple(o.matrix_world@v.co) for v in o.data.vertices]
def colors(o,rgba,name='Anatomical coloration'):
    attr=o.data.color_attributes.get(name) or o.data.color_attributes.new(name=name,type='FLOAT_COLOR',domain='POINT')
    attr.data.foreach_set('color',rgba)
def blend(c,target,w):return tuple(c[i]*(1-w)+target[i]*w for i in range(3))
def smooth(x):
    x=max(0,min(1,x));return x*x*(3-2*x)

skin,nt,bs,vec=material('SKIN • mottled ochre amphibian',.53)
vc=nt.nodes.new('ShaderNodeVertexColor');vc.layer_name='Anatomical coloration'
marble=ramp(nt,noise(nt,vec,5.8,4),[(.22,(.28,.235,.18)),(.47,(.74,.64,.49)),(.70,(1,.94,.78))],'Uneven tan pigmentation')
pigment=mix(nt,vc.outputs['Color'],marble)
freckles=ramp(nt,noise(nt,vec,83,2),[(.34,(.17,.105,.051)),(.415,(.88,.83,.71)),(.65,(1,1,1))],'Fine brown freckles')
pigment=mix(nt,pigment,freckles,.77)
umber=ramp(nt,noise(nt,vec,11,4),[(.27,(.15,.12,.075)),(.52,(1,1,1))],'Broken umber mottling')
pigment=mix(nt,pigment,umber,.68)
nt.links.new(pigment,bs.inputs['Base Color'])
rough=ramp(nt,noise(nt,vec,23,3),[(.25,(.34,)*3),(.75,(.64,)*3)],'Mixed damp and dry skin')
nt.links.new(rough,bs.inputs['Roughness'])
bs.inputs['Subsurface Weight'].default_value=.055
bs.inputs['Subsurface Radius'].default_value=(.5,.25,.12)
bs.inputs['Coat Weight'].default_value=.15;bs.inputs['Coat Roughness'].default_value=.28
bump(nt,bs,noise(nt,vec,190,2),.23,.009)

skin_objects=[bpy.data.objects['Rivermoor • unified detailed sculpt'],bpy.data.objects['V2 • 5651 rounded irregular skin scales']]
for o in skin_objects:
    rgba=array('f')
    for x,y,z in coords(o):
        c=(.43,.295,.16)
        belly=math.exp(-(x/.49)**4)*smooth((-y-.06)/.35)*smooth((2.85-z)/.4)
        c=blend(c,(.66,.51,.31),belly*.73)
        mott=.65+.35*math.sin(x*14+math.sin(z*10))*math.sin(z*19+y*14)
        dorsal=smooth((y-.2)/.38)*smooth((z-1.05)/.35)
        crown=smooth((z-3.48)/.23)*smooth((y+.52)/.31)
        c=blend(c,(.105,.125,.030),max(dorsal,crown)*mott*.90)
        eyelid=0
        for sign in [-1,1]:
            r=math.sqrt(((x-sign*.49)/.34)**2+((z-3.33)/.36)**2)
            eyelid=max(eyelid,math.exp(-((r-.98)/.32)**2)*smooth((-y-.51)/.25))
        c=blend(c,(.34,.125,.060),eyelid*.89)
        nose=math.exp(-(x/.22)**4-((z-3.25)/.15)**4)*smooth((-y-.76)/.16)
        c=blend(c,(.39,.17,.091),nose*.85)
        lips=math.exp(-((z-2.94)/.054)**2)*math.exp(-(x/.63)**8)*smooth((-y-.69)/.20)
        c=blend(c,(.30,.145,.073),lips*.58)
        rgba.extend((*c,1))
    colors(o,rgba);assign(o,skin)

horn,nt,bs,vec=material('CLAWS • worn horn ivory',.4)
nt.links.new(ramp(nt,noise(nt,vec,25),[(.2,(.10,.054,.024)),(.50,(.36,.255,.12)),(.8,(.69,.55,.32))],'Keratin grain'),bs.inputs['Base Color'])
bump(nt,bs,noise(nt,vec,110),.12,.006)
for o in skin_objects:
    o.data.materials.append(horn)
    for p in o.data.polygons:
        q=o.matrix_world@p.center
        if (q.z<.115 and q.y<-.47) or (.43<q.z<.66 and q.y<-.805 and abs(q.x)>.73):p.material_index=1

moss,nt,bs,vec=material('MOSS • olive green and dry ochre tips',.91)
f=noise(nt,vec,19,4)
nt.links.new(ramp(nt,f,[(.18,(.018,.026,.004)),(.40,(.060,.079,.011)),(.61,(.18,.205,.034)),(.80,(.33,.29,.085))],'Bryophyte color variation'),bs.inputs['Base Color'])
bs.inputs['Subsurface Weight'].default_value=.045
assign(bpy.data.objects['V2 • dense branched moss crown and mantle'],moss)

stem,nt,bs,vec=material('FUNGI • fibrous cream stems and gills',.62)
stretch=nt.nodes.new('ShaderNodeVectorMath');stretch.operation='MULTIPLY';stretch.inputs[1].default_value=(18,18,1.4);nt.links.new(vec,stretch.inputs[0])
fibers=noise(nt,stretch.outputs['Vector'],4,3)
nt.links.new(ramp(nt,fibers,[(.20,(.17,.094,.037)),(.44,(.45,.31,.14)),(.68,(.78,.62,.38))],'Long stem fibres'),bs.inputs['Base Color'])
bump(nt,bs,fibers,.18,.010)
bs.inputs['Subsurface Weight'].default_value=.08
cap,nt,bs,vec=material('FUNGI • speckled buff caps',.41)
base=ramp(nt,noise(nt,vec,12,3),[(.20,(.20,.10,.030)),(.45,(.50,.32,.13)),(.73,(.83,.67,.40))],'Pale irregular cap pigmentation')
spots=ramp(nt,noise(nt,vec,48,2),[(.35,(.10,.046,.012)),(.42,(.35,.23,.09)),(.47,(1,1,1))],'Dark cap speckles')
nt.links.new(mix(nt,base,spots,.87),bs.inputs['Base Color']);bump(nt,bs,noise(nt,vec,115,2),.22,.008)
bs.inputs['Coat Weight'].default_value=.20;bs.inputs['Coat Roughness'].default_value=.27
fungus=bpy.data.objects['Rivermoor • eight gilled mushrooms'];assign(fungus,stem);fungus.data.materials.append(cap)
caps=[(.63,.46,3.15,.29),(-.62,.46,3.05,.25),(.50,.72,2.60,.23),(-.50,.69,2.49,.22),(.08,.85,3.11,.23),(.33,.72,1.96,.17),(-.50,.65,1.75,.15),(.66,.50,3.14,.19)]
for p in fungus.data.polygons:
    q=p.center
    for x,y,z,r in caps:
        if (q.x-x)**2+(q.y-y)**2<(r*1.06)**2 and z-r*.048<q.z<z+r*.78:
            p.material_index=1;break

bristle,nt,bs,vec=material('BRISTLES • dry root fibres',.8)
nt.links.new(ramp(nt,noise(nt,vec,32),[(.25,(.09,.057,.022)),(.75,(.41,.31,.14))],'Root fibre tones'),bs.inputs['Base Color'])
assign(bpy.data.objects['V2 • fine jaw bristles'],bristle)

eye_mat,nt,bs,vec=material('EYES • amber iris radial fibres',.24)
vc=nt.nodes.new('ShaderNodeVertexColor');vc.layer_name='Iris pigment';nt.links.new(vc.outputs['Color'],bs.inputs['Base Color'])
bs.inputs['Coat Weight'].default_value=.55;bs.inputs['Coat Roughness'].default_value=.055
bs.inputs['Specular IOR Level'].default_value=.48
cornea,nt,bs,vec=material('EYES • clear wet corneal layer',.035)
bs.inputs['Base Color'].default_value=(.99,.995,1,1);bs.inputs['Transmission Weight'].default_value=1;bs.inputs['IOR'].default_value=1.38
for eye in [o for o in sculpt.objects if o.name.startswith('V2 • clay eye')]:
    a=coords(eye);cx=sum(p[0] for p in a)/len(a);rgba=array('f')
    for x,y,z in a:
        r=math.sqrt((x-cx)**2+((z-3.33)/1.068)**2);theta=math.atan2((z-3.33)/1.068,x-cx)
        fibres=.5+.18*math.sin(theta*91+math.sin(theta*17)*2+r*130)+.13*math.sin(theta*173-r*72)+.11*math.sin(theta*39+r*80)
        c=(.022,.017,.011)
        if r<.197 and y<-.83:
            limbal=1-.8*math.exp(-((r-.188)/.014)**2)
            c=tuple(q*limbal for q in (.17+.26*fibres,.054+.13*fibres,.009+.029*fibres))
        if r<.089 and y<-.83:c=(.0012,.0008,.0004)
        rgba.extend((*c,1))
    colors(eye,rgba,'Iris pigment');assign(eye,eye_mat)
    bpy.ops.mesh.primitive_uv_sphere_add(segments=80,ring_count=48,location=(cx,-.711,3.33))
    shell=bpy.context.object;shell.name='Eye • transparent wet cornea';shell.scale=(.270,.274,.289)
    for coll in list(shell.users_collection):coll.objects.unlink(shell)
    sculpt.objects.link(shell);shell.data.materials.append(cornea)
    for p in shell.data.polygons:p.use_smooth=True

# Slightly softer neutral studio illumination, preserving warm natural pigments.
bpy.data.objects['Key • large softbox'].data.energy=650
bpy.data.objects['Fill • softbox'].data.energy=220
bpy.data.objects['Rim • softbox'].data.energy=700
s.view_settings.exposure=.05
s.camera=bpy.data.objects['Camera • hero'];s.render.resolution_x=1600;s.render.resolution_y=1600;s.render.resolution_percentage=100;s.cycles.samples=256
s.render.filepath=OUT+'/rivermoor_textured_hero.png'
s['materials_notes']='Reference-inspired spatial skin pigments, freckles, olive moss, buff fungi, amber iris and wet cornea. Procedural textures and point color fields embedded in blend; no external image dependencies.'
# Organize shader nodes for further manual editing.
for m in [skin,horn,moss,stem,cap,bristle,eye_mat,cornea]:
    for i,n in enumerate(m.node_tree.nodes):
        n.location=((i%5)*250-750,-(i//5)*250)
bpy.ops.wm.save_as_mainfile(filepath=OUT+'/rivermoor_textured.blend')
print('Textured model saved',OUT)
