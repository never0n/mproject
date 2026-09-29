"""Native Blender asset library reconstructed from the user's object sheet.

Run inside Blender, then call setup(), build_asset(1) ... build_asset(20), finalize().
All geometry is local-space, meters, Z up, front facing -Y. No external services.
The presentation uses scaled collection instances; the source assets keep meter scale.
"""
import bpy
import math
import random
import json
import bmesh
from pathlib import Path
from mathutils import Vector

ROOT = Path('/home/never0n/Projects/new_desctop_MMO/assets/environment/reference_city_v1')
TAU = math.tau
ASSETS = [
 ('gatehouse','Крепостные ворота'), ('curtain_wall','Крепостная стена'),
 ('watchtower','Сторожевая башня'), ('merchant_house','Дом торговца'),
 ('small_house','Малый фахверковый дом'), ('blacksmith_forge','Кузница'),
 ('anvil','Наковальня'), ('blacksmith_sign','Вывеска кузнеца'),
 ('fountain','Фонтан'), ('market_stall','Рыночный прилавок с навесом'),
 ('street_lantern','Уличный фонарь'), ('wall_lantern','Настенный фонарь'),
 ('royal_banner','Королевское знамя'), ('wooden_crates','Деревянные ящики'),
 ('merchant_counter','Торговый прилавок'), ('square_flower_planter','Квадратная клумба'),
 ('long_flower_planter','Длинная клумба'), ('deciduous_tree','Лиственное дерево'),
 ('stone_iron_fence','Каменная ограда'), ('cobblestone_patch','Брусчатка'),
]
MATS = {}
COLLECTIONS = {}
RECORDS = []
SCENE = None

def enum(obj, prop, wanted):
    values = {i.identifier for i in obj.bl_rna.properties[prop].enum_items}
    if wanted not in values:
        raise ValueError(f'{prop}: {wanted} not in {values}')
    setattr(obj, prop, wanted)

def material(name, rgb, rough=.72, metal=0, texture=None, emission=0):
    m = bpy.data.materials.new('RC_' + name)
    m.use_nodes = True
    m.diffuse_color = (*rgb, 1)
    bs = next(n for n in m.node_tree.nodes if n.type == 'BSDF_PRINCIPLED')
    bs.inputs['Base Color'].default_value = (*rgb, 1)
    bs.inputs['Roughness'].default_value = rough
    bs.inputs['Metallic'].default_value = metal
    if emission:
        bs.inputs['Emission Color'].default_value = (*rgb, 1)
        bs.inputs['Emission Strength'].default_value = emission
    if texture:
        nd = m.node_tree.nodes; lk = m.node_tree.links
        coords = nd.new('ShaderNodeTexCoord')
        noise = nd.new('ShaderNodeTexNoise')
        noise.inputs['Scale'].default_value = 5 if texture == 'wood' else 24
        noise.inputs['Detail'].default_value = 3
        if texture == 'wood':
            vec = nd.new('ShaderNodeVectorMath')
            enum(vec, 'operation', 'MULTIPLY')
            vec.inputs[1].default_value = (3, 14, .4)
            lk.new(coords.outputs['Object'], vec.inputs[0])
            lk.new(vec.outputs['Vector'], noise.inputs['Vector'])
        else:
            lk.new(coords.outputs['Object'], noise.inputs['Vector'])
        ramp = nd.new('ShaderNodeValToRGB')
        ramp.color_ramp.elements[0].position = .16
        ramp.color_ramp.elements[0].color = (*(v * .65 for v in rgb), 1)
        ramp.color_ramp.elements[1].position = .84
        ramp.color_ramp.elements[1].color = (*(min(1, v * 1.18) for v in rgb), 1)
        lk.new(noise.outputs['Fac'], ramp.inputs['Fac'])
        lk.new(ramp.outputs['Color'], bs.inputs['Base Color'])
        bump = nd.new('ShaderNodeBump')
        bump.inputs['Strength'].default_value = .2
        bump.inputs['Distance'].default_value = .025 if texture == 'stone' else .009
        lk.new(noise.outputs['Fac'], bump.inputs['Height'])
        lk.new(bump.outputs['Normal'], bs.inputs['Normal'])
    MATS[name] = m
    return m

def make_materials():
    for i, rgb in enumerate([(.61,.55,.43),(.72,.66,.54),(.67,.61,.5),(.79,.72,.61)]):
        material(f'stone{i}',rgb,texture='stone')
    material('mortar',(.31,.29,.25),texture='stone')
    material('plaster',(.8,.73,.57),texture='stone')
    for i,rgb in enumerate([(.20,.10,.043),(.31,.17,.075),(.39,.23,.11),(.25,.135,.058)]):
        material(f'wood{i}',rgb,texture='wood')
    for i,rgb in enumerate([(.065,.11,.17),(.09,.15,.23),(.12,.18,.26),(.085,.13,.20)]):
        material(f'slate{i}',rgb,texture='stone')
    material('iron',(.075,.09,.1),.34,.78)
    material('steel',(.25,.29,.32),.3,.84)
    material('gold',(.68,.40,.07),.32,.7)
    material('blue',(.022,.08,.31),.8,texture='cloth')
    material('cream',(.89,.86,.73),.86,texture='cloth')
    material('shadow',(.024,.026,.021))
    material('glass',(.20,.31,.32),.22,.25)
    material('amber',(.95,.43,.065),.28,emission=1.8)
    material('water',(.10,.40,.51),.13,.22)
    bs=next(n for n in MATS['water'].node_tree.nodes if n.type=='BSDF_PRINCIPLED')
    bs.inputs['Transmission Weight'].default_value=.24
    material('terracotta',(.43,.20,.095),texture='stone')
    material('pottery',(.56,.42,.26),texture='stone')
    material('sack',(.55,.44,.28),texture='cloth')
    material('soil',(.085,.06,.026),texture='stone')
    material('coal',(.035,.028,.02),texture='stone')
    material('fire',(.95,.15,.007),.6,emission=2.8)
    material('flame',(.99,.55,.04),.6,emission=4)
    for i,rgb in enumerate([(.11,.23,.025),(.22,.34,.04),(.065,.16,.022),(.31,.43,.063)]):
        material(f'leaf{i}',rgb,.9)
    material('yellow',(.95,.59,.025),.74)
    material('purple',(.40,.08,.63),.76)
    material('white',(.95,.92,.83),.7)
    material('red',(.57,.07,.025),.66)
    material('ground',(.78,.76,.7),.88)
    material('ink',(.075,.10,.14),.8)

class Mesh:
    def __init__(self, seed=1):
        self.v=[];self.f=[];self.mi=[];self.smooth=[]
        self.keys=list(MATS);self.rng=random.Random(seed)
    def add(self, verts, faces, mat, smooth=False):
        offset=len(self.v);self.v.extend(tuple(v) for v in verts)
        self.f.extend(tuple(offset+i for i in f) for f in faces)
        self.mi.extend([self.keys.index(mat)]*len(faces))
        self.smooth.extend([smooth]*len(faces))
    def variation(self, key, n=4):
        return key+str(self.rng.randrange(n))
    def box(self,c,size,mat,axes=None):
        x,y,z=(s/2 for s in size)
        vs=[(-x,-y,-z),(x,-y,-z),(x,y,-z),(-x,y,-z),(-x,-y,z),(x,-y,z),(x,y,z),(-x,y,z)]
        if axes:
            vs=[Vector(c)+axes[0]*p[0]+axes[1]*p[1]+axes[2]*p[2] for p in vs]
        else:vs=[(c[0]+a,c[1]+b,c[2]+d) for a,b,d in vs]
        self.add(vs,[(0,3,2,1),(0,1,5,4),(1,2,6,5),(2,3,7,6),(3,0,4,7),(4,5,6,7)],mat)
    def beam(self,a,b,w,d,mat):
        a,b=Vector(a),Vector(b);z=(b-a).normalized()
        x=z.cross(Vector((0,1,0)) if abs(z.y)<.98 else Vector((1,0,0))).normalized()
        y=z.cross(x).normalized()
        self.box((a+b)*.5,(w,d,(b-a).length),mat,(x,y,z))
    def panel(self, verts, delta, mat):
        n=len(verts);v=[Vector(p) for p in verts];dd=Vector(delta)
        self.add(v+[p+dd for p in v],[tuple(reversed(range(n))),tuple(range(n,2*n))]+[(i,(i+1)%n,(i+1)%n+n,i+n) for i in range(n)],mat)
    def cyl(self,a,b,r,mat,n=16,r2=None):
        if r2 is None:r2=r
        a,b=Vector(a),Vector(b);z=(b-a).normalized()
        x=z.cross(Vector((0,1,0)) if abs(z.y)<.98 else Vector((1,0,0))).normalized();y=z.cross(x)
        vs=[p+rr*(math.cos(i*TAU/n)*x+math.sin(i*TAU/n)*y) for p,rr in [(a,r),(b,r2)] for i in range(n)]
        self.add(vs,[(i,(i+1)%n,(i+1)%n+n,i+n) for i in range(n)],mat,True)
        self.add(vs,[tuple(reversed(range(n))),tuple(range(n,n*2))],mat)
    def sphere(self,c,scale,mat,n=12,rings=8):
        vs=[]
        for j in range(1,rings):
            t=math.pi*j/rings
            for i in range(n):
                a=TAU*i/n;vs.append((c[0]+scale[0]*math.sin(t)*math.cos(a),c[1]+scale[1]*math.sin(t)*math.sin(a),c[2]+scale[2]*math.cos(t)))
        vs.extend([(c[0],c[1],c[2]+scale[2]),(c[0],c[1],c[2]-scale[2])]);top=len(vs)-2;bot=top+1
        fs=[(j*n+i,j*n+(i+1)%n,(j+1)*n+(i+1)%n,(j+1)*n+i) for j in range(rings-2) for i in range(n)]
        fs.extend((top,i,(i+1)%n) for i in range(n));fs.extend((bot,(rings-2)*n+(i+1)%n,(rings-2)*n+i) for i in range(n))
        self.add(vs,fs,mat,True)
    def lathe(self,c,profile,mat,n=32):
        vs=[(c[0]+r*math.cos(TAU*i/n),c[1]+r*math.sin(TAU*i/n),c[2]+z) for r,z in profile for i in range(n)]
        fs=[(j*n+i,j*n+(i+1)%n,(j+1)*n+(i+1)%n,(j+1)*n+i) for j in range(len(profile)-1) for i in range(n)]
        self.add(vs,fs,mat,True)
    def line(self,pts,r,mat,n=8):
        for a,b in zip(pts,pts[1:]):
            if (Vector(a)-Vector(b)).length>1e-5:self.cyl(a,b,r,mat,n)
    def ring(self,c,r,t,mat,n=32):
        self.lathe(c,[(r+t*math.cos(i*TAU/8),t*math.sin(i*TAU/8)) for i in range(9)],mat,n)
    def finish(self,name,collection,bevel=.015):
        mesh=bpy.data.meshes.new(name+'_Mesh');mesh.from_pydata(self.v,[],self.f);mesh.update()
        for key in self.keys:mesh.materials.append(MATS[key])
        for p,idx,sm in zip(mesh.polygons,self.mi,self.smooth):p.material_index=idx;p.use_smooth=sm
        # Remove unused slots before interchange export.
        used=sorted(set(self.mi));remap={old:new for new,old in enumerate(used)}
        mats=[mesh.materials[i] for i in used];indices=[remap[p.material_index] for p in mesh.polygons]
        mesh.materials.clear()
        for m in mats:mesh.materials.append(m)
        for p,idx in zip(mesh.polygons,indices):p.material_index=idx
        bm=bmesh.new();bm.from_mesh(mesh)
        bmesh.ops.recalc_face_normals(bm,faces=list(bm.faces));bm.to_mesh(mesh);bm.free()
        obj=bpy.data.objects.new(name,mesh);collection.objects.link(obj)
        if bevel:
            mod=obj.modifiers.new('Soft worked edges','BEVEL');mod.width=bevel;mod.segments=2
            enum(mod,'limit_method','ANGLE');mod.angle_limit=.52
        obj['source_sheet']='reference_sheet.png';obj['units']='meters';obj['front']='-Y'
        return obj

def masonry(m,c,w,d,h,block=.62,row=.34):
    m.box((c[0],c[1],c[2]+h/2),(w-.04,d-.04,h-.025),'mortar')
    rows=max(1,round(h/row));rh=h/rows
    for j in range(rows):
        offset=block*.5 if j%2 else 0
        x=-w/2-offset
        while x<w/2:
            a=max(x,-w/2);b=min(x+block,w/2)
            if b-a>.04:
                m.box((c[0]+(a+b)/2,c[1],c[2]+(j+.5)*rh),(b-a-.018,d,rh-.017),m.variation('stone'))
            x+=block

def round_masonry(m,c,r,h,row=.38,n=16):
    rows=round(h/row);rh=h/rows
    m.cyl(c,(c[0],c[1],c[2]+h),r*.98,'mortar',n)
    for j in range(rows):
        for i in range(n):
            a=TAU*(i+(j%2)*.5)/n;da=TAU/n*.475;z=c[2]+j*rh+.012
            vs=[(c[0]+rr*math.cos(t),c[1]+rr*math.sin(t),zz) for zz in [z,z+rh-.024] for rr,t in [(r-.16,a-da),(r,a-da),(r,a+da),(r-.16,a+da)]]
            m.add(vs,[(0,3,2,1),(0,1,5,4),(1,2,6,5),(2,3,7,6),(3,0,4,7),(4,5,6,7)],m.variation('stone'))

def pointed_arch(m,x,y,z,r,rise,depth,band=.27,mat='stone1',wall_top=None):
    pts=[]
    for i in range(17):
        t=-1+2*i/16
        pts.append((x+r*t,y,z+rise*(1-abs(t)**1.55)))
    for a,b in zip(pts,pts[1:]):
        if wall_top is not None:
            m.panel([a,b,(b[0],y,wall_top),(a[0],y,wall_top)],(0,depth,0),m.variation('stone'))
        else:
            v=(Vector(b)-Vector(a)).normalized();normal=Vector((-v.z,0,v.x))
            m.panel([a,b,Vector(b)+normal*band,Vector(a)+normal*band],(0,depth,0),mat)

def spire(m,c,r,h):
    x,y,z=c
    m.lathe(c,[(r*.97,0),(r*1.08,.10),(r*1.1,.17),(r,.23)],'stone1',16)
    m.cyl((x,y,z+.23),(x,y,z+.23+h),r*.998,'slate0',16,.015)
    rows=max(10,int(h/.3));n=16
    for j in range(rows):
        z0=z+.23+h*j/rows;z1=z+.23+h*(j+1)/rows+.02
        r0=r*(1-j/rows)+.04;r1=r*(1-(j+1)/rows)+.04
        for i in range(n):
            a=TAU*i/n+.012;b=TAU*(i+1)/n-.012
            m.panel([(x+r0*math.cos(a),y+r0*math.sin(a),z0),(x+r0*math.cos(b),y+r0*math.sin(b),z0),(x+r1*math.cos(b),y+r1*math.sin(b),z1),(x+r1*math.cos(a),y+r1*math.sin(a),z1)],(0,0,-.025),m.variation('slate'))
    m.cyl((x,y,z+h+.20),(x,y,z+h+.65),.045,'gold',10,.01)
    m.sphere((x,y,z+h+.39),(.085,.085,.085),'gold')

def tower(m,x=0,y=0,z=0,r=1.1,h=6.5,roof=3.7,mini=True):
    round_masonry(m,(x,y,z+.25),r,h)
    for zz,rr,hh in [(z,r*1.13,.25),(z+.35,r*1.05,.13),(z+h*.43,r*1.04,.15),(z+h-.15,r*1.13,.24),(z+h+.16,r*1.14,.2)]:
        m.lathe((x,y,zz),[(rr,0),(rr,hh),(rr*.96,hh+.09)],'stone1',16)
    for level in [.29,.67,.91]:
        for a in [-math.pi/2,0,math.pi/2,math.pi]:
            radial=Vector((math.cos(a),math.sin(a),0));tangent=Vector((-math.sin(a),math.cos(a),0));vertical=Vector((0,0,1))
            center=Vector((x,y,z+h*level))+radial*(r+.012)
            m.box(center,(.19,.035,.64),'shadow',(tangent,radial,vertical))
            for sign in [-1,1]:m.box(center+tangent*.14*sign,(.075,.065,.74),'stone1',(tangent,radial,vertical))
            m.box(center+vertical*.39,(.37,.07,.10),'stone2',(tangent,radial,vertical))
    spire(m,(x,y,z+h+.35),r*1.11,roof)
    if mini:
        for a in [0,math.pi/2,math.pi,math.pi*1.5]:
            xx=x+math.cos(a)*r*.93;yy=y+math.sin(a)*r*.93
            m.cyl((xx,yy,z+h-.6),(xx,yy,z+h+.6),.17,'stone1',8)
            spire(m,(xx,yy,z+h+.54),.24,roof*.40)

def banner(m,x,y,z,w=1,h=2.5):
    # Draped, pointed cloth with a separate gold border and raised heraldic mark.
    nx=16;ny=18;vs=[]
    for j in range(ny+1):
        t=j/ny
        for i in range(nx+1):
            u=i/nx;bottom=h*(1-.22*abs(2*u-1))
            vs.append((x+(u-.5)*w,y+.045*math.sin(u*TAU*2.3)*(.35+t)+.08*math.sin(t*math.pi),z-t*bottom))
    m.add(vs,[(j*(nx+1)+i,j*(nx+1)+i+1,(j+1)*(nx+1)+i+1,(j+1)*(nx+1)+i) for j in range(ny) for i in range(nx)],'blue')
    def pt(u,t):return (x+(u-.5)*w,y-.025+.045*math.sin(u*TAU*2.3)*(.35+t)+.08*math.sin(t*math.pi),z-t*h*(1-.22*abs(2*u-1)))
    for u in [0,1]:m.line([pt(u,j/ny) for j in range(ny+1)],.018*w,'gold')
    m.line([pt(i/nx,1) for i in range(nx+1)],.022*w,'gold')
    m.cyl((x-w*.66,y,z+.10),(x+w*.66,y,z+.1),w*.037,'gold')
    for s in [-1,1]:m.sphere((x+s*w*.68,y,z+.1),(w*.064,w*.064,w*.064),'gold')
    # Gold pixels from the reference become a welded, extruded relief mesh.
    heraldic_relief(m,x,y-.11,z-h*.43,w)

def build_gatehouse(m):
    for x in [-3.8,3.8]:tower(m,x,0,r=1.13,h=6.7,roof=3.9)
    for x in [-2.24,2.24]:masonry(m,(x,0,0),1.65,1.9,5.2)
    pointed_arch(m,0,-.95,2.5,1.43,1.52,1.9,wall_top=5.5)
    pointed_arch(m,0,-1.10,2.5,1.43,1.52,.16,.33)
    for x in [-1.61,1.61]:masonry(m,(x,-1.05,0),.30,.23,2.53,.3,.29)
    for j in range(4):
        z=4.15+j*.33
        masonry(m,(0,0,z),5.45,1.95,.31,.64,.31)
    m.box((0,0,5.57),(5.8,2.1,.21),'stone1')
    for x in [-2.55,-1.7,-.85,0,.85,1.7,2.55]:m.box((x,0,5.92),(.52,1.95,.52),'stone2')
    for x in [i*.27 for i in range(-5,6)]:
        edge=2.5+1.52*(1-(abs(x)/1.43)**1.55)
        if edge>3.13:
            m.cyl((x,-.78,3.10),(x,-.78,edge+.09),.038,'iron',8)
            m.cyl((x,-.78,3.0),(x,-.78,3.12),.003,'iron',8,.068)
    for z in [3.39,3.70,3.98]:m.box((0,-.80,z),(2.7 if z<3.4 else 1.75,.095,.065),'iron')
    for x in [-3.8,3.8]:banner(m,x,-1.44,5.15,1.16,3.12)
    m.box((0,-1.1,5.3),(.7,.15,.69),'gold')

def build_wall(m):
    masonry(m,(0,0,0),6.8,.8,3.5)
    m.box((0,0,3.53),(7,1.02,.20),'stone1')
    for x in [-3.2,-2.4,-1.6,-.8,0,.8,1.6,2.4,3.2]:masonry(m,(x,0,3.63),.48,.9,.5,.48,.25)
    for x in [-3.15,0,3.15]:
        for j in range(9):
            z=j*.39;d=.95-.055*j
            m.box((x,-.31-d*.25,z+.19),(.64-.025*j,d,.37),m.variation('stone'))
        m.box((x,-.30,3.8),(.7,1,.25),'stone1')

def gable_roof(m,c,w,d,eave,rise,tile=.32):
    x,y,z=c;half=w/2
    for side in [-1,1]:
        m.panel([(x,y-d/2,z+eave+rise),(x+side*half,y-d/2,z+eave),(x+side*half,y+d/2,z+eave),(x,y+d/2,z+eave+rise)],(0,0,-.10),'wood0')
        rows=max(4,round(math.hypot(half,rise)/tile));cols=max(5,round(d/tile))
        for j in range(rows):
            a=j/rows;b=min(1,(j+1.18)/rows)
            for i in range(cols):
                ya=y-d/2+i*d/cols+.008;yb=ya+d/cols-.016
                m.panel([(x+side*half*a,ya,z+eave+rise*(1-a)+.027),(x+side*half*b,ya,z+eave+rise*(1-b)+.027),(x+side*half*b,yb,z+eave+rise*(1-b)+.027),(x+side*half*a,yb,z+eave+rise*(1-a)+.027)],(0,0,-.035),m.variation('slate'))
        for yy in [-d/2,d/2]:m.beam((x,y+yy,z+eave+rise+.07),(x+side*half,y+yy,z+eave+.02),.13,.17,'wood1')
        m.beam((x+side*half,y-d/2,z+eave),(x+side*half,y+d/2,z+eave),.16,.18,'wood1')
    m.cyl((x,y-d/2-.06,z+eave+rise+.08),(x,y+d/2+.06,z+eave+rise+.08),.1,'slate2',10)

def window(m,x,y,z,w=.8,h=1.0):
    m.box((x,y,z),(w+.15,.08,h+.17),'wood0')
    m.box((x,y-.055,z),(w,.035,h),'glass')
    for dx in [-w*.5,0,w*.5]:m.box((x+dx,y-.09,z),(.055,.07,h+.1),'wood2')
    for dz in [-h*.5,0,h*.5]:m.box((x,y-.1,z+dz),(w+.1,.075,.055),'wood2')
    m.box((x,y-.16,z-h*.5-.12),(w+.3,.32,.13),'wood1')
    for sign in [-1,1]:
        for i in range(2):m.box((x+sign*(w*.5+.1+i*.115),y+.025,z),(.105,.09,h+.04),'wood1')
        for dz in [-.3,.3]:m.box((x+sign*(w*.5+.16),y-.04,z+dz*h),(.24,.06,.07),'iron')

def house(m,large=True):
    w=4.7 if large else 3.4;d=3.65 if large else 2.9;h=4.45 if large else 2.65;rise=2.10 if large else 1.95
    masonry(m,(0,0,0),w+.18,d+.14,.48)
    m.box((0,0,h/2+.24),(w,d,h-.48),'plaster')
    for yy in [-d/2,d/2]:
        m.panel([(-w/2,yy,h),(w/2,yy,h),(0,yy,h+rise)],(0,.09 if yy<0 else -.09,0),'plaster')
        for x in [-w/2,-w/4,0,w/4,w/2]:m.box((x,yy,h/2+.12),(.16,.18,h-.2),'wood0')
        for z in [.55,h*.5 if large else .6,h]:m.box((0,yy,z),(w+.14,.2,.19),'wood0')
        for s in [-1,1]:
            m.beam((0,yy,h+rise),(s*w/2,yy,h),.16,.20,'wood0')
            m.beam((s*w*.23,yy,h),(s*w*.23,yy,h+rise*.52),.13,.20,'wood1')
        m.beam((0,yy,h),(0,yy,h+rise),.17,.21,'wood0')
        if large:
            for s in [-1,1]:m.beam((s*w*.48,yy,h*.5+.1),(s*w*.26,yy,h-.1),.14,.20,'wood0')
    for xx in [-w/2,w/2]:
        for yy in [-d/2,-d/6,d/6,d/2]:m.box((xx,yy,h/2),(.19,.15,h),'wood0')
        for zz in [.55,h*.5,h]:m.box((xx,0,zz),(.2,d,.18),'wood0')
        for a,b in [(-d/2,-d/6),(d/6,d/2)]:m.beam((xx,a,h*.51),(xx,b,h*.92),.12,.12,'wood0')
    front=-d/2-.14
    # Boarded doorway and frame.
    doorx=-.68 if large else -.52
    m.box((doorx,front,.98),(1.02,.10,1.8),'wood0')
    for i in range(6):m.box((doorx-.45+i*.18,front-.08,.98),(.17,.08,1.74),m.variation('wood'))
    for z in [.47,1.53]:m.box((doorx,front-.14,z),(.96,.06,.09),'iron')
    for x in [doorx-.58,doorx+.58]:m.box((x,front,1.03),(.16,.24,2.05),'wood1')
    m.box((doorx,front,2.05),(1.30,.24,.18),'wood1')
    m.sphere((doorx+.3,front-.20,1),(.05,.04,.05),'gold')
    for i in range(2):m.box((doorx,front-.34-i*.16,.11-i*.045),(1.5,.63+i*.28,.16),'stone1')
    window(m,w*.26,front,1.30,.81,.94)
    if large:
        window(m,-1.3,front,3.25,.84,1.18);window(m,1.26,front,3.25,.84,1.18)
        window(m,0,front,h+rise*.38,.65,.82)
        # Full-width balcony, rails, balusters and supports.
        balcony_y=front-.54;balcony_z=2.49
        for i in range(16):m.box((-w*.47+i*w*.94/15,balcony_y,balcony_z),(w*.94/16-.01,1.08,.11),'wood1')
        for xx in [-w*.47,0,w*.47]:
            m.beam((xx,front,1.6),(xx,front-1.03,balcony_z-.1),.16,.16,'wood0')
        rail_y=front-1.05
        for zz in [balcony_z+.2,balcony_z+.86]:m.box((0,rail_y,zz),(w*.99,.14,.12),'wood1')
        for i in range(11):m.box((-w*.46+i*w*.092,rail_y,balcony_z+.52),(.065,.08,.69),'wood2')
        for xx in [-w*.47,w*.47]:m.box((xx,balcony_y,balcony_z+.86),(.14,1.1,.13),'wood1')
    else:
        window(m,0,front,h+rise*.4,.61,.72)
        for xx in [-w*.47,w*.47]:m.box((xx,front-.78,1.04),(.16,.18,2.08),'wood1')
        for i in range(14):
            xx=-w*.54+i*w*1.08/13
            m.panel([(xx,front+.1,2.56),(xx+.23,front+.1,2.56),(xx+.23,front-1.13,2.09),(xx,front-1.13,2.09)],(0,0,-.05),m.variation('wood'))
    gable_roof(m,(0,0,0),w+.65,d+.6,h+.12,rise)
    masonry(m,(-w*.3,d*.23,h+.5),.5,.53,rise+.25,.28,.27)
    m.box((-w*.3,d*.23,h+rise+.78),(.63,.66,.14),'stone1')
    if large:
        crate(m,(w*.42,front-.3,0),.55)
        for xx in [-1.20,1.22]:planter(m,(xx,front-1.07,2.84),1.10,.32,.23,21)
    else:
        planter(m,(w*.43,front-.38,.02),.57,.55,.42,24)
        lantern_head(m,(-1.18,front-.23,1.46),.49)

def crate(m,c,s=1):
    x,y,z=c;w=s;d=s*.85;h=s*.87
    m.box((x,y,z+h/2),(w-.06,d-.06,h-.04),'shadow')
    for yy in [-d/2,d/2]:
        for i in range(5):m.box((x-w/2+(i+.5)*w/5,y+yy,z+h/2),(w/5-.01,.07,h),m.variation('wood'))
        for zz in [.07,h-.07]:m.box((x,y+yy*1.07,z+zz),(w+.05,.085,.11),'wood1')
        m.beam((x-w*.41,y+yy*1.16,z+.13),(x+w*.41,y+yy*1.16,z+h-.13),.10,.075,'wood2')
    for xx in [-w/2,w/2]:
        for i in range(5):m.box((x+xx,y-d/2+(i+.5)*d/5,z+h/2),(.07,d/5-.009,h),m.variation('wood'))
        for zz in [.07,h-.07]:m.box((x+xx*1.07,y,z+zz),(.085,d+.04,.11),'wood1')
    for i in range(5):m.box((x-w/2+(i+.5)*w/5,y,z+h),(w/5-.012,d,.06),m.variation('wood'))
    for xx in [-w*.37,w*.37]:
        m.box((x+xx,y,z+h+.036),(.08,d+.06,.03),'iron')
        for yy in [-d/2-.05,d/2+.05]:
            m.box((x+xx,y+yy,z+h-.15),(.09,.026,.30),'iron')
            for zz in [h-.06,h-.23]:m.sphere((x+xx,y+yy*1.035,z+zz),(.022,.015,.022),'steel',8,4)

def setup():
    global SCENE
    if bpy.data.scenes.get('Reference City | 20 objects'):
        raise RuntimeError('Library scene already exists; use the loaded module to continue.')
    ROOT.mkdir(parents=True,exist_ok=True)
    for folder in ['blend','glb','previews']:(ROOT/folder).mkdir(exist_ok=True)
    make_materials()
    SCENE=bpy.data.scenes.new('Reference City | 20 objects')
    enum(SCENE.unit_settings,'system','METRIC');SCENE.unit_settings.scale_length=1
    SCENE['reference_order']='5 columns x 4 rows, left to right then top to bottom'
    SCENE['source_geometry']='Meter scale source collections; presentation instances fitted to equal cells.'
    bpy.context.window.scene=SCENE
    # Deliberately leave the pre-existing user scene intact.
    setup_lighting(SCENE)
    print('Initialized new library scene; pre-existing scene preserved.')

def anvil_shape(m,c,s=1):
    x,y,z=c
    m.box((x,y,z+.12*s),(.67*s,.40*s,.19*s),'iron')
    m.panel([(x-.28*s,y-.15*s,z+.17*s),(x+.28*s,y-.15*s,z+.17*s),(x+.16*s,y-.15*s,z+.40*s),(x-.15*s,y-.15*s,z+.40*s)],(0,.3*s,0),'iron')
    m.box((x+.10*s,y,z+.43*s),(.72*s,.36*s,.16*s),'steel')
    m.cyl((x-.24*s,y,z+.44*s),(x-.88*s,y,z+.46*s),.17*s,'steel',20,.008*s)
    m.box((x+.37*s,y+.07*s,z+.515*s),(.07*s,.07*s,.005*s),'shadow')

def stump(m,c,r=.42,h=.78):
    x,y,z=c;n=14
    m.cyl(c,(x,y,z+h),r,'wood1',n,r*.94)
    for i in range(n):
        a=i*TAU/n;rr=r*1.004
        m.line([(x+rr*math.cos(a),y+rr*math.sin(a),z+.04),(x+rr*.97*math.cos(a+.012),y+rr*.97*math.sin(a+.012),z+h-.04)],.012,'wood0')
    for zz in [.1,h-.1]:
        m.lathe((x,y,z+zz),[(r*1.02,-.03),(r*1.02,.03)],'iron',n)
        for i in range(0,n,2):
            a=i*TAU/n;m.sphere((x+r*1.04*math.cos(a),y+r*1.04*math.sin(a),z+zz),(.02,.02,.02),'steel',8,4)
    for rr in [r*.3,r*.58,r*.82]:m.ring((x,y,z+h+.008),rr,.007,'wood0')

def build_anvil(m):
    stump(m,(0,0,0),.47,.82);anvil_shape(m,(.08,0,.84),1.1)

def build_blacksmith_sign(m):
    m.box((-.78,0,1.4),(.18,.22,2.8),'wood1')
    m.box((-.03,0,2.65),(1.65,.24,.18),'wood1')
    m.beam((-.72,0,2.13),(-.12,0,2.58),.105,.15,'wood2')
    for x in [-.76,.66]:
        m.box((x,-.02,2.65),(.12,.30,.25),'iron')
        m.sphere((x,-.18,2.65),(.034,.018,.034),'steel',8,4)
    for x in [-.43,.49]:
        for j in range(4):
            zz=2.43-j*.095
            pts=[(x+.032*math.cos(i*TAU/12),-.045+(j%2)*.022*math.cos(i*TAU/12),zz+.06*math.sin(i*TAU/12)) for i in range(13)]
            m.line(pts,.012,'iron')
    for i in range(5):m.box((-.54+i*.25,0,1.48),(.241,.14,1.16),m.variation('wood'))
    for z in [.88,2.08]:m.box((-.04,-.04,z),(1.39,.20,.15),'wood0')
    for x in [-.66,.58]:m.box((x,-.04,1.48),(.13,.20,1.33),'wood1')
    outline=[(-.49,.23),(-.34,.12),(-.08,.06),(-.10,-.14),(-.26,-.27),(.34,-.27),(.24,-.14),(.21,.05),(.39,.13),(.41,.23)]
    m.panel([(x-.03,-.14,1.52+z) for x,z in outline],(0,.04,0),'iron')
    for x in [-.66,.58]:
        for z in [.88,2.08]:m.sphere((x,-.16,z),(.038,.023,.038),'steel',8,4)

def build_blacksmith_forge(m):
    # Open-front masonry hearth with a dark chamber and chimney.
    masonry(m,(-.55,.40,0),2.15,1.4,.65)
    for x in [-1.48,.39]:masonry(m,(x,.43,.65),.33,1.37,1.36,.34,.27)
    masonry(m,(-.55,1.01,.65),1.85,.25,1.45,.43,.27)
    pointed_arch(m,-.55,-.27,1.32,.76,.70,1.29,wall_top=2.4)
    pointed_arch(m,-.55,-.35,1.32,.76,.7,.17,.20,'stone2')
    masonry(m,(-.55,.43,2.32),1.15,1.04,1.22,.42,.28)
    masonry(m,(-.55,.43,3.5),.70,.73,1.18,.37,.25)
    m.box((-.55,.43,4.75),(.88,.90,.2),'stone1')
    m.box((-.55,.43,4.87),(.54,.58,.026),'shadow')
    m.box((-.55,.10,.77),(1.58,1.16,.13),'coal')
    for i in range(27):
        xx=-.55+m.rng.uniform(-.66,.66);yy=m.rng.uniform(-.38,.45)
        m.sphere((xx,yy,.85),(.075,.08,.065),'coal' if i%3 else 'fire',8,5)
        if i%3==0:
            m.lathe((xx,yy,.88),[(.07,0),(.10,.10),(.05,.3),(.002,.48)],'flame',8)
    # Open workshop shelter, blue roof on massive timber posts.
    for x in [-1.95,1.9]:
        for y in [-1.15,1.08]:
            m.box((x,y,1.40),(.19,.20,2.80),'wood0')
            m.box((x,y,.12),(.29,.29,.24),'stone1')
            m.beam((x,y,2.2),(x+(-.55 if x>0 else .55),y,2.78),.12,.14,'wood1')
    gable_roof(m,(0,-.06,0),4.45,2.9,2.85,1.1,.27)
    for y in [-1.21,1.09]:m.box((0,y,2.77),(4.2,.18,.2),'wood1')
    stump(m,(-1.38,-1.38,0),.33,.57);anvil_shape(m,(-1.35,-1.38,.59),.67)
    crate(m,(1.15,-.77,0),.9)
    for i in range(5):m.box((.70+i*.18,-.6,1.02),(.17,.82,.075),'wood2')
    for x in [.7,1.4]:
        m.cyl((x,-.61,1.05),(x,-.61,1.49),.023,'wood0',8)
        m.box((x,-.61,1.50),(.16,.085,.09),'steel')
    # Tongs and horseshoe on the front right support.
    for dx in [-.05,.05]:m.line([(1.88+dx,-1.28,1.3),(1.90+dx*.5,-1.28,1.73),(1.86+dx,-1.28,1.91)],.018,'iron')

def pot(m,c,r=.18,h=.36,mat='terracotta'):
    m.lathe(c,[(0,0),(r*.62,0),(r*.9,h*.08),(r,h*.44),(r*.84,h*.70),(r*.54,h*.83),(r*.55,h*.96),(r*.68,h),(r*.69,h*1.07),(r*.49,h*1.07),(r*.48,h*.84),(r*.70,h*.63),(r*.76,h*.13),(0,h*.13)],mat,20)

def sack(m,c,r=.24,h=.48):
    x,y,z=c
    m.sphere((x,y,z+h*.41),(r,r*.8,h*.42),'sack',14,10)
    m.lathe((x,y,z),[(r*.52,h*.69),(r*.50,h*.9),(r*.72,h),(r*.69,h*1.04),(r*.49,h*.94)],'sack',18)
    m.cyl((x,y,z+h*.9),(x,y,z+h*.92),r*.47,'soil',18)
    for i in range(19):
        a=m.rng.uniform(0,TAU);rr=m.rng.uniform(0,r*.42)
        m.sphere((x+rr*math.cos(a),y+rr*math.sin(a),z+h*.93),(.034,.027,.025),'cream',8,4)

def basket(m,c,r=.24,h=.22,produce=True):
    x,y,z=c
    m.lathe(c,[(0,0),(r*.8,0),(r,h),(r*1.04,h+.02),(r*.96,h+.04),(r*.90,h*.91),(r*.73,.03),(0,.03)],'wood2',24)
    for j in range(6):m.ring((x,y,z+.03+j*h/6),r*(.82+.18*j/6),.012,'sack',24)
    for i in range(20):
        a=i*TAU/20;m.line([(x+r*.83*math.cos(a),y+r*.83*math.sin(a),z+.02),(x+r*.97*math.cos(a),y+r*.97*math.sin(a),z+h)],.009,'sack')
    if produce:
        for i in range(12):
            a=m.rng.uniform(0,TAU);rr=m.rng.uniform(0,r*.72)
            m.sphere((x+rr*math.cos(a),y+rr*math.sin(a),z+h*.9),(.048,.046,.052),'red' if i%3 else 'yellow',10,6)

def counter(m,c,w=2.4,d=.8,h=.92):
    x,y,z=c
    for yy in [-d/2,d/2]:
        for i in range(10):m.box((x-w/2+(i+.5)*w/10,y+yy,z+h/2),(w/10-.012,.07,h),m.variation('wood'))
        for zz in [.1,h-.1]:m.box((x,y+yy*1.07,z+zz),(w+.06,.08,.13),'wood1')
        for i in [-1,1]:m.beam((x+i*.07,y+yy*1.15,z+.17),(x+i*(w*.46),y+yy*1.15,z+h-.17),.09,.065,'wood2')
    for xx in [-w/2,w/2]:m.box((x+xx,y,z+h/2),(.08,d,h),'wood1')
    for i in range(5):m.box((x,y-d*.59+(i+.5)*d*1.18/5,z+h),(w+.2,d*1.18/5-.008,.10),m.variation('wood'))
    for xx in [-w*.45,w*.45]:
        for yy in [-d*.44,d*.44]:m.box((x+xx,y+yy,z+.1),(.12,.12,.26),'wood0')

def build_market_stall(m):
    w=3.15;d=1.75
    for x in [-w/2,w/2]:
        for y in [-.83,.79]:
            top=2.90 if y>0 else 2.51
            m.box((x,y,top/2),(.13,.14,top),'wood1')
            m.beam((x,y,top-.44),(x+(-.37 if x>0 else .37),y,top-.07),.085,.085,'wood2')
    for y,z in [(.80,2.90),(-.99,2.50)]:m.box((0,y,z),(w+.23,.13,.13),'wood0')
    # Individually curved strips of fabric, with scalloped valance.
    stripes=12
    for i in range(stripes):
        xa=-w/2-.18+i*(w+.36)/stripes;xb=xa+(w+.36)/stripes
        mat='blue' if i%2==0 else 'cream';verts=[]
        for j in range(13):
            t=j/12;y=.90-t*2.05;z=2.97-.41*t-.10*math.sin(t*math.pi)
            verts.extend([(xa,y,z),(xb,y,z)])
        m.add(verts,[(j*2,j*2+1,j*2+3,j*2+2) for j in range(12)],mat)
        scallop=[(xa,-1.15,2.56),(xb,-1.15,2.56),(xb,-1.15,2.34)]
        scallop.extend((xb-(xb-xa)*j/8,-1.15,2.34-.08*math.sin(j*math.pi/8)) for j in range(1,9))
        m.panel(scallop,(0,.022,0),mat)
    counter(m,(0,-.58,0),3,.72,.92)
    for z in [.35,1.12,1.76]:
        m.box((0,.73,z),(3.0,.39,.09),'wood1')
        for i in range(7):
            x=-1.25+i*.41
            pot(m,(x,.71,z+.06),.11+m.rng.random()*.05,.19+m.rng.random()*.13,'pottery' if i%2 else 'terracotta')
    for i in range(5):
        x=-1.13+i*.56
        if i%2:pot(m,(x,-.55,.98),.14,.32)
        else:basket(m,(x,-.54,.98),.21,.19)
    sack(m,(1.39,-1.05,0),.23,.58);crate(m,(-1.62,-.7,0),.48)

def lantern_head(m,c,s=1):
    x,y,z=c
    # Four tapered glowing window panels within an iron cage.
    r=.22*s;zt=z+.61*s
    for side in range(4):
        a=math.pi/4+side*math.pi/2;b=a+math.pi/2
        pts=[(x+r*.76*math.cos(a),y+r*.76*math.sin(a),z),(x+r*.76*math.cos(b),y+r*.76*math.sin(b),z),(x+r*math.cos(b),y+r*math.sin(b),zt),(x+r*math.cos(a),y+r*math.sin(a),zt)]
        m.panel(pts,(0,0,-.005*s),'amber')
        m.beam(pts[0],pts[3],.034*s,.034*s,'iron')
        m.beam((Vector(pts[0])+Vector(pts[1]))*.5,(Vector(pts[2])+Vector(pts[3]))*.5,.025*s,.025*s,'iron')
        m.beam(Vector(pts[0])*.4+Vector(pts[3])*.6,Vector(pts[1])*.4+Vector(pts[2])*.6,.024*s,.024*s,'iron')
    for zz,rr in [(z-.03*s,r*.86),(zt,r*1.15)]:m.cyl((x,y,zz),(x,y,zz+.06*s),rr,'iron',4)
    m.lathe((x,y,zt+.04*s),[(r*1.34,0),(r*1.39,.04*s),(r*.72,.14*s),(.04*s,.30*s),(.055*s,.33*s),(.004*s,.52*s)],'iron',4)
    m.lathe((x,y,z-.19*s),[(.025*s,0),(.065*s,.06*s),(.08*s,.10*s),(r*.85,.16*s)],'iron',8)
    m.sphere((x,y,zt+.37*s),(.047*s,.047*s,.05*s),'gold',10,6)

def build_street_lantern(m):
    m.box((0,0,.07),(.45,.45,.14),'iron')
    m.box((0,0,.18),(.36,.36,.12),'iron')
    m.lathe((0,0,.24),[(.17,0),(.14,.18),(.10,.3),(.062,1.92),(.1,1.98),(.11,2.03),(.075,2.10)],'iron',12)
    for z in [.55,2.06,2.30]:m.ring((0,0,z),.08,.018,'gold',16)
    lantern_head(m,(0,0,2.45),1.15)

def build_wall_lantern(m):
    m.box((0,.22,.83),(.28,.16,1.66),'wood1')
    for z in [.19,1.47]:m.sphere((0,.125,z),(.04,.026,.04),'iron',10,6)
    m.beam((0,.12,1.35),(0,-.68,1.35),.065,.065,'iron')
    m.line([(0,.12,.78),(0,-.10,.85),(0,-.22,1.01),(0,-.19,1.13),(0,-.09,1.14)],.032,'iron')
    lantern_head(m,(0,-.68,.25),1.05)
    m.cyl((0,-.68,1.21),(0,-.68,1.42),.031,'iron',12)

def build_royal_banner(m):
    banner(m,0,0,2.9,1.4,2.8)

def build_wooden_crates(m):
    crate(m,(-.50,0,0),.95);crate(m,(.52,.10,0),.92);crate(m,(0,.08,.87),.85)

def build_merchant_counter(m):
    counter(m,(0,0,0),2.6,.84,.9)
    pot(m,(-.98,.09,.97),.21,.49)
    basket(m,(-.42,-.04,.97),.21,.22)
    basket(m,(.12,.05,.97),.24,.27)
    pot(m,(.66,.12,.97),.18,.32,'pottery')
    sack(m,(1.02,.04,.97),.20,.43)
    sack(m,(1.50,.0,0),.27,.62)
    # Cloth draped over the counter edge, with blue woven bands.
    for i in range(12):
        xx=.25+i*.035;z0=.995;y0=-.12
        m.panel([(xx,y0,z0),(xx+.034,y0,z0),(xx+.034,-.53,z0),(xx,-.53,z0)],(0,0,-.008),'blue' if i in [1,2,9] else 'cream')
        m.panel([(xx,-.535,z0),(xx+.034,-.535,z0),(xx+.034,-.55,.46),(xx,-.55,.46)],(0,.008,0),'blue' if i in [1,2,9] else 'cream')

def foliage_leaf(m,c,length,width,angle,mat):
    x,y,z=c;dx=math.cos(angle);dy=math.sin(angle)
    vs=[(x,y,z),(x+dx*length*.46-dy*width,y+dy*length*.46+dx*width,z+.025),
        (x+dx*length,y+dy*length,z+.08),(x+dx*length*.46+dy*width,y+dy*length*.46-dx*width,z+.025),
        (x+dx*length*.47,y+dy*length*.47,z+.085)]
    m.add(vs,[(0,1,4),(1,2,4),(2,3,4),(3,0,4)],mat)

def flower(m,c,s=.25,color='yellow'):
    x,y,z=c;h=s*m.rng.uniform(.8,1.6)
    m.cyl((x,y,z),(x+.035*s,y,z+h),.012*s,'leaf2',6)
    for j in range(3):foliage_leaf(m,(x,y,z+h*j*.27),s*.7,s*.17,j*2.4+m.rng.random(),'leaf0')
    zz=z+h;r=s*.29
    for i in range(7):
        a=i*TAU/7;dx=math.cos(a);dy=math.sin(a)
        vs=[(x,y,zz),(x+dx*r*.7-dy*r*.31,y+dy*r*.7+dx*r*.31,zz+.034),
            (x+dx*r*1.23,y+dy*r*1.23,zz+.019),
            (x+dx*r*.7+dy*r*.31,y+dy*r*.7-dx*r*.31,zz+.034)]
        m.add(vs,[(0,1,2),(0,2,3)],color)
    m.sphere((x,y,zz+.016),(r*.26,r*.26,r*.16),'yellow' if color!='yellow' else 'terracotta',8,4)

def planter(m,c,w=1,d=1,h=.65,flowers=48):
    x,y,z=c
    m.box((x,y,z+h*.78),(w-.06,d-.06,h*.35),'soil')
    for yy in [-d/2,d/2]:
        for j in range(4):m.box((x,y+yy,z+(j+.5)*h/4),(w,.07,h/4-.012),m.variation('wood'))
        for xx in [-w*.43,w*.43]:
            m.box((x+xx,y+yy*1.05,z+h/2),(.10,.09,h+.06),'wood0')
            for zz in [.10,h-.10]:m.sphere((x+xx,y+yy*1.14,z+zz),(.022,.018,.022),'iron',8,4)
    for xx in [-w/2,w/2]:
        for j in range(4):m.box((x+xx,y,z+(j+.5)*h/4),(.07,d,h/4-.012),m.variation('wood'))
    for yy in [-d/2,d/2]:m.box((x,y+yy,z+h),(w+.14,.115,.08),'wood1')
    for xx in [-w/2,w/2]:m.box((x+xx,y,z+h),(.115,d+.1,.08),'wood1')
    for i in range(flowers):
        xx=x+m.rng.uniform(-w*.46,w*.46);yy=y+m.rng.uniform(-d*.46,d*.46)
        flower(m,(xx,yy,z+h*.96),m.rng.uniform(.18,.36),['yellow','purple','white'][i%3])
    for i in range(flowers*3):
        xx=x+m.rng.uniform(-w*.53,w*.53);yy=y+m.rng.uniform(-d*.53,d*.53)
        foliage_leaf(m,(xx,yy,z+h+m.rng.uniform(.0,.16)),m.rng.uniform(.13,.30),.055,m.rng.uniform(0,TAU),m.variation('leaf'))

def build_square_flower_planter(m):
    planter(m,(0,0,0),1,1,.7,70)

def build_long_flower_planter(m):
    planter(m,(0,0,0),2.3,.62,.40,95)

def build_fountain(m):
    # Polygonal steps and genuinely hollow bowls; no humanoid statue.
    m.lathe((0,0,0),[(0,0),(2.35,0),(2.35,.16),(2.20,.20),(2.2,.35),(2.0,.35)],'stone2',32)
    m.lathe((0,0,.30),[(1.88,0),(2.0,.07),(2.07,.47),(2.15,.51),(2.15,.62),(1.90,.62),(1.89,.50),(1.83,.11),(1.70,.07)],'stone1',32)
    m.cyl((0,0,.48),(0,0,.52),1.88,'water',64)
    for i in range(24):
        a=i*TAU/24;r=2.068
        m.line([(r*math.cos(a),r*math.sin(a),.39),(r*math.cos(a),r*math.sin(a),.79)],.01,'mortar')
    m.lathe((0,0,.51),[(.53,0),(.57,.15),(.46,.22),(.40,.62),(.34,.81),(.48,1.02),(.61,1.07)],'stone1',32)
    for z,r in [(1.60,1.17),(2.57,.74)]:
        m.lathe((0,0,z),[(.24,-.10),(r*.70,.0),(r*.93,.18),(r,.33),(r*1.06,.37),(r*1.06,.46),(r*.94,.46),(r*.92,.34),(r*.66,.13),(.23,.04)],'stone2',48)
        m.cyl((0,0,z+.35),(0,0,z+.38),r*.93,'water',48)
        for i in range(10 if r>1 else 8):
            a=i*TAU/(10 if r>1 else 8)+.13
            start=Vector((r*math.cos(a),r*math.sin(a),z+.38))
            pts=[]
            for j in range(13):
                t=j/12;rr=r+.12*t+.08*math.sin(t*math.pi)
                pts.append((rr*math.cos(a),rr*math.sin(a),z+.38-t*(1.43 if r>1 else .97)))
            for k in [-1,0,1]:m.line([(px+.016*k,py+.013*k,pz) for px,py,pz in pts],.017,'water',6)
            m.ring((pts[-1][0],pts[-1][1],pts[-1][2]+.02),.10,.013,'water',16)
    m.lathe((0,0,1.95),[(.29,0),(.34,.1),(.23,.24),(.21,.46),(.30,.66)],'stone1',32)
    m.lathe((0,0,2.96),[(.22,0),(.22,.16),(.14,.22),(.18,.34),(.09,.46),(.025,.64),(0,.91)],'stone2',24)
    for a in [0,math.pi/2,math.pi,math.pi*1.5]:
        x=math.cos(a)*2.07;y=math.sin(a)*2.07
        m.box((x,y,.53),(.34,.34,1.04),'stone1');m.box((x,y,1.08),(.43,.43,.11),'stone2')
    for x in [-1.72,1.72]:planter(m,(x,-1.76,.08),1.02,.52,.25,24)

def build_deciduous_tree(m):
    # Branched trunk and thousands of small leaf blades; no spherical canopy blobs.
    m.lathe((0,0,0),[(.40,0),(.28,.34),(.21,1.1),(.17,1.75),(.12,2.4),(.04,3.2)],'wood0',14)
    for i in range(8):
        a=i*TAU/8;m.cyl((math.cos(a)*.62,math.sin(a)*.62,.02),(math.cos(a)*.14,math.sin(a)*.14,.40),.035,'wood0',8,.13)
    clusters=[]
    for i in range(16):
        a=i*2.399;rr=m.rng.uniform(.72,1.6);z=m.rng.uniform(2.20,3.72)
        tip=Vector((math.cos(a)*rr,math.sin(a)*rr,z));start=Vector((0,0,1.2+i*.09))
        middle=start*.3+tip*.7-Vector((0,0,.24))
        m.cyl(start,middle,.095,'wood0',10,.048);m.cyl(middle,tip,.048,'wood1',8,.012)
        for j in range(3):
            aa=a+j*2.1;end=tip+Vector((math.cos(aa)*.38,math.sin(aa)*.38,.25+m.rng.random()*.2))
            m.cyl(tip,end,.018,'wood1',7,.004);clusters.append(end)
    clusters.append(Vector((0,0,3.9)))
    for center in clusters:
        for i in range(85):
            u=m.rng.uniform(-1,1);a=m.rng.uniform(0,TAU);r=m.rng.random()**(1/3)
            p=center+Vector((math.sqrt(1-u*u)*math.cos(a)*r*.57,math.sqrt(1-u*u)*math.sin(a)*r*.57,u*r*.48))
            foliage_leaf(m,p,m.rng.uniform(.10,.21),m.rng.uniform(.045,.075),m.rng.uniform(0,TAU),m.variation('leaf'))
    m.lathe((0,0,0),[(0,0),(.85,0),(.76,.035),(0,.04)],'soil',24)
    for i in range(95):
        a=m.rng.uniform(0,TAU);r=m.rng.uniform(.28,.85)
        foliage_leaf(m,(r*math.cos(a),r*math.sin(a),.025),.12,.023,a,m.variation('leaf'))

def build_stone_iron_fence(m):
    masonry(m,(0,0,0),2.65,.42,.25,.44,.25)
    for x in [-1.2,1.2]:
        masonry(m,(x,0,.25),.38,.40,1.38,.38,.23)
        m.box((x,0,.23),(.5,.5,.18),'stone1')
        m.box((x,0,1.64),(.50,.5,.13),'stone1')
        m.cyl((x,0,1.70),(x,0,1.87),.36,'stone2',4,.16)
    for z in [.51,1.14]:m.box((0,0,z),(2.12,.075,.075),'iron')
    for i in range(9):
        x=-.93+i*.233;m.cyl((x,0,.35),(x,0,1.41),.026,'iron',8)
        m.cyl((x,0,1.37),(x,0,1.62),.067,'iron',4,.002)
        m.sphere((x,0,1.22),(.05,.05,.05),'iron',8,5)

def build_cobblestone_patch(m):
    w=2.8;d=2.8
    m.box((0,0,.055),(w,d,.11),'mortar')
    for j in range(10):
        row=d/10;offset=.175 if j%2 else 0;x=-w/2-offset
        while x<w/2:
            xx=max(-w/2,x);xr=min(w/2,x+.35)
            if xr-xx>.035:
                h=m.rng.uniform(.135,.18);center=((xx+xr)/2,-d/2+(j+.5)*row,h/2)
                theta=m.rng.uniform(-.025,.025)
                m.box(center,(xr-xx-.025,row-.025,h),m.variation('stone'),(Vector((math.cos(theta),math.sin(theta),0)),Vector((-math.sin(theta),math.cos(theta),0)),Vector((0,0,1))))
            x+=.35



def setup_lighting(scene):
    world=bpy.data.worlds.new(scene.name+'_World');world.use_nodes=True;scene.world=world
    bg=next(n for n in world.node_tree.nodes if n.type=='BACKGROUND')
    bg.inputs['Color'].default_value=(.74,.79,.86,1);bg.inputs['Strength'].default_value=.45
    try:scene.render.engine='BLENDER_EEVEE'
    except TypeError:scene.render.engine='BLENDER_EEVEE_NEXT'
    if hasattr(scene,'eevee'):
        scene.eevee.use_gtao=True;scene.eevee.gtao_distance=3;scene.eevee.gtao_factor=1.25
        scene.eevee.use_soft_shadows=True;scene.eevee.taa_render_samples=64
    scene.render.resolution_x=1800;scene.render.resolution_y=1800;scene.render.resolution_percentage=100
    enum(scene.render.image_settings,'file_format','PNG');enum(scene.render.image_settings,'color_mode','RGBA')
    scene.render.film_transparent=True

def light(scene,name,loc,power,size,target=(0,0,2)):
    dat=bpy.data.lights.new(name,'AREA');dat.energy=power;dat.shape='DISK';dat.size=size
    obj=bpy.data.objects.new(name,dat);scene.collection.objects.link(obj);obj.location=loc
    obj.rotation_euler=(Vector(target)-obj.location).to_track_quat('-Z','Y').to_euler()
    return obj

def camera(scene,center,size):
    dat=bpy.data.cameras.new(scene.name+'_Camera');enum(dat,'type','ORTHO');dat.ortho_scale=size*1.45
    obj=bpy.data.objects.new(scene.name+'_Camera',dat);scene.collection.objects.link(obj)
    obj.location=Vector(center)+Vector((size*.74,-size*1.35,size*.76))
    obj.rotation_euler=(Vector(center)-obj.location).to_track_quat('-Z','Y').to_euler();scene.camera=obj
    return obj

def focus(scene,center,size):
    bpy.context.window.scene=scene
    for screen in bpy.data.screens:
        for area in screen.areas:
            if area.type=='VIEW_3D':
                sp=area.spaces.active;enum(sp.shading,'type','MATERIAL')
                sp.overlay.show_overlays=False
                sp.region_3d.view_distance=size*2.05
                sp.region_3d.view_location=center
                sp.region_3d.view_rotation=(Vector((.74,-1.35,.76))).to_track_quat('Z','Y')
                enum(sp.region_3d,'view_perspective','ORTHO')

def build_asset(number):
    global RECORDS
    if number in COLLECTIONS:raise ValueError(f'Already built {number}')
    slug,title=ASSETS[number-1];name=f'{number:02d}_{slug}'
    collection=bpy.data.collections.new(name);collection.use_fake_user=True
    m=Mesh(7300+number)
    builders={1:build_gatehouse,2:build_wall,3:lambda m:tower(m,r=1.0,h=5.4,roof=3.6),
              4:lambda m:house(m,True),5:lambda m:house(m,False)}
    if number in builders:builders[number](m)
    else:globals()['build_'+slug](m)
    obj=m.finish(name,collection,.012 if number<=6 else .007)
    obj['sheet_index']=number;obj['name_ru']=title
    collection.asset_mark();collection.asset_data.description=f'{number:02d}. {title}. Reconstructed from reference; meter scale.'
    COLLECTIONS[number]=collection
    bounds=[Vector(v) for v in obj.bound_box]
    minimum=Vector(tuple(min(v[i] for v in bounds) for i in range(3)))
    maximum=Vector(tuple(max(v[i] for v in bounds) for i in range(3)))
    dimensions=maximum-minimum;center=(minimum+maximum)*.5;size=max(dimensions)
    # Every standalone .blend has a real scene with editable source geometry and a camera.
    single=bpy.data.scenes.new(name+' | '+title);single.collection.children.link(collection)
    enum(single.unit_settings,'system','METRIC');setup_lighting(single)
    camera(single,center,size)
    light(single,name+'_Key',center+Vector((-size,-size,size*1.7)),size*size*70,size)
    light(single,name+'_Fill',center+Vector((size,size*.3,size)),size*size*35,size*.8)
    single.render.resolution_x=1000;single.render.resolution_y=1000
    single.render.filepath=str(ROOT/'previews'/f'{name}.png')
    bpy.data.libraries.write(str(ROOT/'blend'/f'{name}.blend'),{single},fake_user=True,compress=True)
    # Fitted collection instances form the ordered presentation, while originals stay in meters.
    instance=bpy.data.objects.new(name+' | '+title,None);enum(instance,'instance_type','COLLECTION');instance.instance_collection=collection
    SCENE.collection.objects.link(instance)
    scale=min(6.6/max(dimensions.x,dimensions.y),6.8/dimensions.z)
    instance.scale=(scale,)*3;instance.rotation_euler.z=-.25
    instance.location=(((number-1)%5-2)*9,(1.5-(number-1)//5)*11,-minimum.z*scale)
    instance['source_meters']=list(dimensions);instance['sheet_index']=number
    RECORDS.append({'index':number,'id':name,'name_ru':title,'size_meters':[round(v,3) for v in dimensions],
                    'vertices':len(obj.data.vertices),'faces':len(obj.data.polygons),'blend':f'blend/{name}.blend',
                    'glb':f'glb/{name}.glb','preview':f'previews/{name}.png'})
    (ROOT/'manifest.json').write_text(json.dumps(RECORDS,ensure_ascii=False,indent=2),encoding='utf8')
    focus(single,center,size)
    print(json.dumps(RECORDS[-1],ensure_ascii=False))
    return obj

def heraldic_relief(m,x,y,z,w):
    data=json.loads((ROOT/'heraldry_geometry.json').read_text())
    cells={tuple(p) for p in data['gold_cells']};unit=w*.009
    verts=[];faces=[];lookup={}
    def vertex(i,j,back):
        key=(i,j,back)
        if key not in lookup:
            lookup[key]=len(verts)
            verts.append((x+(i-data['width']/2)*unit,y+back*w*.011,z+(data['height']/2-j)*unit))
        return lookup[key]
    for i,j in sorted(cells):
        p=[(i,j),(i+1,j),(i+1,j+1),(i,j+1)]
        a=[vertex(ii,jj,0) for ii,jj in p];b=[vertex(ii,jj,1) for ii,jj in p]
        faces.extend([tuple(a),tuple(reversed(b))])
        for edge,neighbor in enumerate([(i,j-1),(i+1,j),(i,j+1),(i-1,j)]):
            if neighbor not in cells:
                nxt=(edge+1)%4;faces.append((a[edge],b[edge],b[nxt],a[nxt]))
    m.add(verts,faces,'gold')

def rebuild_geometry(number):
    slug,title=ASSETS[number-1];name=f'{number:02d}_{slug}'
    old=bpy.data.objects[name];oldmesh=old.data;m=Mesh(7300+number)
    builders={1:build_gatehouse,2:build_wall,3:lambda m:tower(m,r=1,h=5.4,roof=3.6),
              4:lambda m:house(m,True),5:lambda m:house(m,False)}
    if number in builders:builders[number](m)
    else:globals()['build_'+slug](m)
    fresh=m.finish('_replacement',COLLECTIONS[number],0);old.data=fresh.data
    bpy.data.objects.remove(fresh,do_unlink=True);bpy.data.meshes.remove(oldmesh)
    old.data.name=name+'_Mesh'
    print('Updated',name)

def finalize():
    if sorted(COLLECTIONS)!=list(range(1,21)):raise RuntimeError('All 20 assets are required')
    for record in RECORDS:
        number=record['index'];name=record['id'];obj=bpy.data.objects[name]
        bm=bmesh.new();bm.from_mesh(obj.data);bmesh.ops.recalc_face_normals(bm,faces=list(bm.faces));bm.to_mesh(obj.data);bm.free()
        obj.data.update();obj.data.calc_loop_triangles()
        assert all(math.isfinite(value) for v in obj.data.vertices for value in v.co)
        bounds=[Vector(v) for v in obj.bound_box]
        lo=Vector(tuple(min(v[i] for v in bounds) for i in range(3)))
        hi=Vector(tuple(max(v[i] for v in bounds) for i in range(3)))
        dim=hi-lo;center=(lo+hi)*.5;size=max(dim)
        record.update(vertices=len(obj.data.vertices),faces=len(obj.data.polygons),triangles=len(obj.data.loop_triangles),size_meters=[round(v,3) for v in dim])
        source=next(s for s in bpy.data.scenes if s.name.startswith(name+' |'))
        source.camera.data.ortho_scale=size*1.29
        source.camera.location=center+Vector((size*.74,-size*1.35,size*.76))
        source.camera.rotation_euler=(center-source.camera.location).to_track_quat('-Z','Y').to_euler()
        # The standalone files open on their own asset, with material preview and camera.
        focus(source,center,size)
        bpy.data.libraries.write(str(ROOT/record['blend']),{source},fake_user=True,compress=True)
        instance=next(o for o in SCENE.objects if o.get('sheet_index')==number)
        factor=min(6.6/max(dim.x,dim.y),6.8/dim.z);instance.scale=(factor,)*3;instance.location.z=-lo.z*factor
        instance['source_meters']=list(dim)
    (ROOT/'manifest.json').write_text(json.dumps(RECORDS,ensure_ascii=False,indent=2),encoding='utf8')
    if SCENE.camera is None:
        gallery_camera=camera(SCENE,(0,1,1.5),40)
        gallery_camera.location=(0,-48,60)
        gallery_camera.rotation_euler=(Vector((0,1,1.5))-gallery_camera.location).to_track_quat('-Z','Y').to_euler()
        gallery_camera.data.ortho_scale=47
        light(SCENE,'Gallery_Key',(-25,-15,36),18000,22,(0,0,1))
        light(SCENE,'Gallery_Fill',(25,12,28),10000,20,(0,0,1))
        for record in RECORDS:
            n=record['index'];x=((n-1)%5-2)*9;y=(1.5-(n-1)//5)*11
            curve=bpy.data.curves.new(f'{n:02d}_Label','FONT');curve.body=f"{n:02d}  {record['id'][3:].replace('_',' ')}"
            enum(curve,'align_x','CENTER');curve.size=.36;curve.extrude=.0
            obj=bpy.data.objects.new(f'{n:02d}_Label',curve);SCENE.collection.objects.link(obj)
            obj.location=(x,y-3.1,.02);obj.data.materials.append(MATS['ink'])
            obj.rotation_euler=SCENE.camera.rotation_euler
        floor_mesh=Mesh();floor_mesh.box((0,0,-.08),(116,114,.1),'ground');floor_mesh.finish('Gallery_Surface',SCENE.collection,0)
    SCENE.render.film_transparent=False
    SCENE.render.resolution_x=2400;SCENE.render.resolution_y=2100
    SCENE.render.filepath=str(ROOT/'previews'/'00_full_library.png')
    bpy.context.window.scene=SCENE
    for screen in bpy.data.screens:
        for area in screen.areas:
            if area.type=='VIEW_3D':
                sp=area.spaces.active;enum(sp.shading,'type','MATERIAL');sp.overlay.show_overlays=False
                sp.region_3d.view_rotation=SCENE.camera.rotation_euler.to_quaternion()
                sp.region_3d.view_location=(0,1,1.5);sp.region_3d.view_distance=54
                enum(sp.region_3d,'view_perspective','ORTHO')
    bpy.ops.wm.save_as_mainfile(filepath=str(ROOT/'reference_city_library.blend'))
    print('SAVED',ROOT/'reference_city_library.blend')
    print('TOTAL',len(RECORDS),'assets',sum(r['triangles'] for r in RECORDS),'base triangles')

if __name__=='__main__':
    setup()
    for number in range(1,21):build_asset(number)
    finalize()
