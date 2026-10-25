"""Second sculpt pass: anchored scales, wrinkles, eyes and moss geometry."""
import bpy, math, random, os
from mathutils import Vector, noise
from mathutils.bvhtree import BVHTree
from math import sin,cos,pi
random.seed(721)
OUT='/home/never0n/Projects/new_desctop_MMO/assets/creatures/rivermoor_clay'
scene=bpy.context.scene
sculpt=bpy.data.collections['01 • RIVERMOOR sculpt']; studio=bpy.data.collections['02 • Studio']
clay=bpy.data.materials['Clay • warm limestone']; eyeclay=bpy.data.materials['Clay • polished eyeballs']
source=open('/home/never0n/Projects/new_desctop_MMO/tools/build_rivermoor_clay.py').read()
exec(source[source.index('def put('):source.index('body=[]')])
skin=bpy.data.objects['Rivermoor • unified detailed sculpt']
bpy.context.view_layer.objects.active=skin
for m in list(skin.modifiers): bpy.ops.object.modifier_apply(modifier=m.name)

# A directional, multi-scale displacement is actual mesh detail.
normals=[v.normal.copy() for v in skin.data.vertices]
for v,n in zip(skin.data.vertices,normals):
    p=skin.matrix_world@v.co
    eyelid=min(((p.x-s*.49)/.38)**2+((p.z-3.33)/.40)**2 for s in (-1,1))<1.2 and p.y<-.65
    grain=.0032 if eyelid else .006
    d=grain*(noise.noise(p*31)+.45*noise.noise(p*83))
    if p.z<2.8: d+=.006*noise.noise(Vector((p.x*23,p.y*23,p.z*10)))
    v.co+=n*d
skin.data.update()
deps=bpy.context.evaluated_depsgraph_get()
bvh=BVHTree.FromObject(skin,deps)
M=skin.matrix_world; MI=M.inverted()
def hit(origin,direction):
    loc,n,idx,dist=bvh.ray_cast(MI@Vector(origin),Vector(direction),12)
    return (M@loc,n) if loc is not None else (None,None)
def front(x,z): return hit((x,-4,z),(0,1,0))
def back(x,z): return hit((x,4,z),(0,-1,0))

class Batch:
    def __init__(self): self.v=[]; self.f=[]
    def surface(self,name): return mesh(name,self.v,self.f)
    def strand(self,pts,radii,sides=6):
        pts=[Vector(p) for p in pts]; start=len(self.v)
        for i,p in enumerate(pts):
            t=(pts[min(i+1,len(pts)-1)]-pts[max(0,i-1)]).normalized()
            u=t.cross(Vector((0,1,0)))
            if u.length<.05: u=t.cross(Vector((1,0,0)))
            u.normalize(); w=t.cross(u).normalized()
            for j in range(sides): self.v.append(tuple(p+radii[i]*(u*cos(j*2*pi/sides)+w*sin(j*2*pi/sides))))
            if i:
                for j in range(sides):
                    a=start+(i-1)*sides+j; b=start+(i-1)*sides+(j+1)%sides
                    self.f.append((a,b,b+sides,a+sides))
        self.f.append(tuple(start+j for j in reversed(range(sides))))
        self.f.append(tuple(start+(len(pts)-1)*sides+j for j in range(sides)))
    def pebble(self,p,n,r,h,stretch=1):
        # Low irregular polygonal plates with fully buried skirts.
        p=Vector(p); n=Vector(n).normalized(); u=n.cross(Vector((0,0,1)))
        if u.length<.1: u=n.cross(Vector((0,1,0)))
        u.normalize(); w=n.cross(u); rot=random.uniform(0,2*pi)
        u,w=u*cos(rot)+w*sin(rot),-u*sin(rot)+w*cos(rot)
        start=len(self.v); N=9; shape=[random.uniform(.79,1.17) for i in range(N)]
        for k,(rad,z) in enumerate([(1,-.002),(.88,.20),(.62,.72),(.26,1.0),(0,1.03)]):
            for j in range(N):
                a=j*2*pi/N
                q=p+u*(cos(a)*r*rad*shape[j])+w*(sin(a)*r*rad*shape[j]*stretch)+n*(h*z)
                self.v.append(tuple(q))
            if k:
                for j in range(N):
                    a=start+(k-1)*N+j; b=start+(k-1)*N+(j+1)%N; self.f.append((a,b,b+N,a+N))

scales=Batch(); occupied=set(); samples=[]
indices=list(range(len(skin.data.vertices))); random.shuffle(indices)
for idx in indices:
    v=skin.data.vertices[idx]; p=M@v.co; n=v.normal
    if p.z<.16 or (p.y<-.55 and p.z<.7): continue
    eyezone=p.y<-.60 and min(((p.x-s*.49)/.37)**2+((p.z-3.33)/.40)**2 for s in [-1,1])<1.13
    face=p.y<-.64 and p.z>2.82
    spacing=.023 if face else .049
    if eyezone: spacing=.032
    cell=tuple(int(c/spacing) for c in p)
    key=(round(spacing,3),*cell)
    if key in occupied: continue
    occupied.add(key)
    if random.random()>.32: continue
    if noise.noise(p*5)<-.3 and not face: continue
    r=random.uniform(.008,.018) if face else random.uniform(.019,.041)
    h=r*random.uniform(.16,.46)
    if eyezone: r*=.66; h*=.4
    if p.y<-.2 and 1.3<p.z<2.25 and abs(p.x)<.43:
        r*=1.24; h*=.5
    scales.pebble(p-n*.001,n,r,h,random.uniform(.7,1.4))
    samples.append((p.copy(),n.copy()))
    if len(samples)>=7000: break
scales.surface('V2 • 7000 irregular skin scales')
print('Anchored skin plates',len(samples))

folds=Batch()
def wrinkle(xzs,r=.008,reverse=False):
    pts=[]; pieces=[]
    for x,z in xzs:
        p,n=(back(x,z) if reverse else front(x,z))
        if p is not None:
            q=p-n*r*.20
            if pts and (q-pts[-1]).length>.045:
                pieces.append(pts); pts=[]
            pts.append(q)
        else:
            pieces.append(pts); pts=[]
    pieces.append(pts)
    for pts in pieces:
        if len(pts)>2:
            rr=[r*.78*(.12+.88*max(0,sin(pi*i/(len(pts)-1)))**.6) for i in range(len(pts))]
            folds.strand(pts,rr,7)

for s in [-1,1]:
    # Nested wrinkles below and outside each eye, interrupted instead of perfect rings.
    for row in range(4):
        pts=[]
        for j in range(40):
            a=pi*(1.06+.83*j/39)
            rx=.297+row*.023; rz=.318+row*.018
            pts.append((s*(.49+rx*cos(a)),3.33+rz*sin(a)+.003*sin(j*.8+row)))
        wrinkle(pts,.006+row*.0008)
    for k in range(32):
        a=2*pi*k/32; pts=[]
        for j in range(8):
            rr=.302+j*.007
            pts.append((s*(.49+rr*cos(a)),3.33+rr*1.06*sin(a)))
        wrinkle(pts,.0035)
    for row in range(7):
        wrinkle([(s*(.26+.065*j),3.73-row*.027-.032*sin(j*pi/10)) for j in range(10)],.006)
    # Fans of branching cheek creases.
    for row in range(8):
        wrinkle([(s*(.31+.032*j),2.86+row*.026+.075*sin(j*pi/25)) for j in range(12)],.0065)
    # Many short, irregular transverse compression folds on both elbows and knees.
    for z0,width,cx in [(1.63,.14,.90),(.77,.15,.68),(1.09,.15,.86)]:
        for row in range(8):
            wrinkle([(s*(cx+width*(j/15*2-1)),z0+row*.021+.012*cos(j/15*pi*2+row*.3)) for j in range(16)],.006)
    # Broken long tendon folds rather than a few thick wires.
    for row in range(7):
        wrinkle([(s*(.11+row*.037+.018*sin(j*.3)),2.30+j*.022) for j in range(22)],.005)
    for k in range(4):
        for row in range(7):
            x=s*(.78+k*.084+row*.004); z=.70+row*.045
            wrinkle([(x+s*(j-4)*.007,z+.007*cos(j*.6)) for j in range(9)],.004)
    # Belly scales and loose diagonal skin folds.
    for row in range(12):
        wrinkle([(s*(.03+j*.026),1.20+row*.09+.02*sin(j*.4+row)) for j in range(15)],.005)
    for row in range(13):
        wrinkle([(s*(.13+j*.024),1.37+row*.092+.024*sin(j*.5+row)) for j in range(15)],.007,True)
folds.surface('V2 • eyelid wrinkles tendons and joint folds')

# Sculpt irises and cupped pupils in the same clay, with no painted eye textures.
for eye in [o for o in sculpt.objects if o.name.startswith('Eye •')]:
    eye.hide_render=True; eye.hide_set(True)
    cx=eye.location.x; cy=-.711; cz=3.33; vs=[]; fs=[]; N=128; rows=65
    for i in range(rows):
        a=pi*i/(rows-1); radial=.264*sin(a)
        for j in range(N):
            t=j*2*pi/N
            x=cx+radial*cos(t); z=cz+radial*1.068*sin(t); y=cy-.264*cos(a)
            if a<pi/2:
                # Deep but smooth pupil bowl; radial striations on the iris annulus.
                if radial<.085: y+=.068*(1-(radial/.085)**2)**2
                if .085<radial<.19:
                    w=sin(pi*(radial-.085)/.105)
                    y-=.005*w*(.5+.5*sin(t*59+radial*90))
                y-=.006*math.exp(-((radial-.189)/.007)**2)
                y-=.004*math.exp(-((radial-.085)/.005)**2)
            vs.append((x,y,z))
        if i:
            for j in range(N): fs.append(((i-1)*N+j,(i-1)*N+(j+1)%N,i*N+(j+1)%N,i*N+j))
    mesh('V2 • clay eye with iris and recessed pupil',vs,fs,eyeclay)

# A surface-anchored moss mat, with tiny branched fronds rather than isolated spikes.
moss=Batch(); roots=[]; count=0
for idx in indices:
    v=skin.data.vertices[idx]; p=M@v.co; n=v.normal
    crown=p.z>3.63 and p.y>-.42
    dorsal=1.25<p.z<2.94 and p.y>.38 and n.y>.15
    if not (crown or dorsal): continue
    if random.random()>.009: continue
    if any((p-q).length<.047 for q in roots[-120:]): continue
    roots.append(p.copy()); count+=1
    # Each tuft has 4–8 bending stems with paired leaflets.
    for stem in range(random.randint(4,8)):
        u=n.cross(Vector((0,0,1)))
        if u.length<.1: u=n.cross(Vector((0,1,0)))
        u.normalize(); w=n.cross(u)
        a=random.uniform(0,2*pi); side=u*cos(a)+w*sin(a)
        length=random.uniform(.045,.125)
        base=p+side*random.uniform(0,.022)-n*.005
        tip=base+n*length+side*length*.7
        pts=[base,base.lerp(tip,.35)+n*.009,base.lerp(tip,.72),tip]
        moss.strand(pts,[.0045,.0036,.0022,.0004],5)
        for k in range(1,5):
            start=base.lerp(tip,k/6)
            for sign in (-1,1):
                end=start+u*sign*length*.23+n*length*.13+side*length*.06
                moss.strand([start,start.lerp(end,.6)+n*.004,end],[.0025,.003,.0003],5)
    if count>=420: break
moss.surface('V2 • dense branched moss crown and mantle')
print('Moss tufts',count)

# Short, tapered bristles around the jaw and sides, rooted to the skin.
bristles=Batch()
for i in range(180):
    s=random.choice([-1,1]); x=s*random.uniform(.34,.74); z=random.uniform(2.78,3.2)
    p,n=front(x,z)
    if p is None: continue
    length=random.uniform(.023,.08); tip=p+n*length+Vector((s*.018,0,-.025))
    bristles.strand([p-n*.003,p.lerp(tip,.6),tip],[.003,.0016,.0002],5)
bristles.surface('V2 • fine jaw bristles')

# Real ridges and irregular cap relief on the existing mushrooms.
fungus=bpy.data.objects['Rivermoor • eight gilled mushrooms']
fungus_normals=[v.normal.copy() for v in fungus.data.vertices]
for v,n in zip(fungus.data.vertices,fungus_normals):
    p=fungus.matrix_world@v.co
    v.co+=n*(.006*noise.noise(p*27)+.003*noise.noise(p*91))
fungus.data.update()

# Side-lit neutral clay makes the new relief readable.
nt=clay.node_tree
bump=next((n for n in nt.nodes if n.type=='BUMP'),None)
if bump: bump.inputs['Strength'].default_value=.26; bump.inputs['Distance'].default_value=.016
bs=next(n for n in nt.nodes if n.type=='BSDF_PRINCIPLED'); bs.inputs['Roughness'].default_value=.77
bpy.data.objects['Key • large softbox'].data.energy=500; bpy.data.objects['Key • large softbox'].data.size=2.8
bpy.data.objects['Fill • softbox'].data.energy=125
bpy.data.objects['Rim • softbox'].data.energy=650
scene.view_settings.exposure=-.35
scene.camera=bpy.data.objects['Camera • hero']
scene.render.resolution_x=1600; scene.render.resolution_y=1600; scene.cycles.samples=96
scene.render.filepath=OUT+'/rivermoor_clay_v2_hero.png'
scene['detail_pass']='V2: geometric skin scales, joint wrinkles, iris and pupil relief, dense branched moss and jaw bristles.'
bpy.ops.object.select_all(action='DESELECT'); skin.select_set(True); bpy.context.view_layer.objects.active=skin
bpy.ops.wm.save_as_mainfile(filepath=OUT+'/rivermoor_clay_v2.blend')
print('Saved detail pass',len(skin.data.vertices),'base vertices')
