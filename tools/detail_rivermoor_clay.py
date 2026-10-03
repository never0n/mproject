import bpy, math, random, os
from mathutils import Vector
from math import sin, cos, pi
random.seed(38)
OUT='/home/never0n/Projects/new_desctop_MMO/assets/creatures/rivermoor_clay'
scene=bpy.context.scene
sculpt=bpy.data.collections['01 • RIVERMOOR sculpt']
studio=bpy.data.collections['02 • Studio']
clay=bpy.data.materials['Clay • warm limestone']
eyeclay=bpy.data.materials['Clay • polished eyeballs']
skin=bpy.data.objects['Rivermoor • unified sculpt']
source=open('/home/never0n/Projects/new_desctop_MMO/tools/build_rivermoor_clay.py').read()
exec(source[source.index('def put('):source.index('body=[]')])

# Carved nostrils and mouth; sculpted eyelids cover the socket boundaries.
cut=[]
for s in [-1,1]:
    cut.append(ell('Eye recess',(s*.49,-.70,3.33),(.292,.325,.315)))
    nost=ell('Nostril recess',(s*.125,-.927,3.235),(.066,.13,.074)); nost.rotation_euler[1]=s*-.32; cut.append(nost)
mouth=[(-.57,-.811,2.925),(-.43,-.939,2.943),(-.23,-1.003,2.966),(0,-1.022,2.974),(.23,-1.003,2.966),(.43,-.939,2.943),(.57,-.811,2.925)]
cut.append(tube('Mouth incision',mouth,[.022]*7,12,6))
cutter=join(cut,'Temporary sculpt cutters')
bpy.context.view_layer.objects.active=skin
bo=skin.modifiers.new('Carve face','BOOLEAN'); bo.operation='DIFFERENCE'; bo.solver='EXACT'; bo.object=cutter
# Keep high-resolution subdivision last for soft carved edges.
bpy.ops.object.modifier_move_up(modifier=bo.name)
bpy.ops.object.modifier_apply(modifier=bo.name)
bpy.data.objects.remove(cutter,do_unlink=True)
detail=[]
for s in [-1,1]:
    ell('Eye • sculpted blank',(s*.49,-.711,3.33),(.264,.264,.282),eyeclay,64,40)
    pts=[]; rr=[]
    for j in range(49):
        a=j/48*2*pi
        pts.append((s*(.49+.274*cos(a)),-.815-.018*sin(a),3.33+.302*sin(a)))
        rr.append(.039+.032*max(0,sin(a)))
    detail.append(tube('Thick eyelid',pts,rr,12,2))
    for row in range(2):
        pts=[]
        for j in range(16):
            a=pi*1.07+j/15*pi*.86
            pts.append((s*(.49+(.306+row*.023)*cos(a)),-.75+row*.018,3.33+(.337+row*.022)*sin(a)))
        detail.append(tube('Under-eye skin fold',pts,[.013]*16,8,2))
    detail.append(tube('Expressive brow',[(s*.19,-.69,3.54),(s*.31,-.77,3.65),(s*.50,-.73,3.68),(s*.72,-.58,3.55)],[.048,.073,.07,.013],16))
    detail.append(tube('Ear inner fold',[(s*.70,-.17,3.30),(s*.85,-.19,3.38),(s*1.03,-.107,3.49),(s*1.38,.018,3.58)],[.033,.048,.030,.003],12))
    detail.append(tube('Cheek crease',[(s*.69,-.62,3.15),(s*.69,-.68,3.04),(s*.57,-.78,2.88),(s*.38,-.80,2.82)],[.014,.023,.018,.002],10))
    detail.append(tube('Nose wing',[(s*.06,-.933,3.30),(s*.15,-.942,3.33),(s*.212,-.88,3.245)],[.021,.033,.016],12))
    for j in range(3):
        x=s*(.22+j*.09)
        detail.append(tube('Throat tendon',[(x,-.54,2.78),(x*.74,-.565,2.58),(x*.53,-.55,2.36),(x*.56,-.43,2.17)],[.021,.024,.018,.002],10))
    for j in range(4):
        z=1.43+j*.17
        detail.append(tube('Abdominal fold',[(s*.06,-.318,z),(s*.24,-.302,z+.014),(s*.40,-.238,z+.09)],[.009,.026,.004],10))
    for j in range(3):
        detail.append(tube('Elbow fold',[(s*.83,-.32,1.69-j*.051),(s*.92,-.352,1.66-j*.05),(s*1.02,-.30,1.65-j*.04)],[.005,.02,.003],10))
    for k in range(4):
        x=s*(.735+k*.084); z=1.01-abs(k-1.3)*.015
        detail.append(tube('Finger tendon',[(x,-.755,1.19),(x,-.778,1.01),(x+s*.025,-.808,z-.17)],[.011,.013,.004],8))
        detail.append(tube('Finger claw',[(x+s*.075,-.782,z-.34),(x+s*.093,-.851,z-.43),(x+s*.081,-.895,z-.49)],[.032,.020,.001],12))
        for j in range(2):
            xx=x+s*(.03+j*.03); zz=z-.17-j*.12
            detail.append(tube('Finger knuckle crease',[(xx-.027,-.80,zz),(xx,-.817,zz+.006),(xx+.027,-.80,zz)],[.004,.009,.004],8,3))
        tx=s*(.70+(k-1.5)*.14)+s*.035; ty=-.65+abs(k-1.5)*.06
        detail.append(tube('Toe claw',[(tx,ty+.025,.10),(tx,ty-.055,.072),(tx,ty-.11,.035)],[.041,.028,.001],12))
    detail.append(tube('Thumb claw',[(s*.62,-.85,.89),(s*.65,-.90,.81),(s*.69,-.92,.77)],[.032,.02,.001],10))
detail.append(tube('Lower lip',[(x,y+.006,z-.041) for x,y,z in mouth],[.004,.026,.032,.037,.032,.026,.004],14))

# Irregular, low relief skin granules; all are clay geometry, no colour map.
for i in range(245):
    a=random.uniform(0,2*pi); t=random.uniform(.23,1.9)
    x=.815*sin(t)*cos(a); y=-.16+.55*sin(t)*sin(a); z=3.14+.646*cos(t)
    if y<-.42 and z<3.67: continue
    r=random.uniform(.012,.039)
    detail.append(ell('Crown skin nodule',(x,y,z),(r,r*.85,r*.68),seg=12,rings=8))
for i in range(170):
    a=random.uniform(-.30,pi+.30); z=random.uniform(1.27,2.75)
    width=.52*math.sqrt(max(.12,1-((z-1.96)/1.04)**2))
    x=width*cos(a); y=.19+.445*sin(a)
    r=random.uniform(.019,.058)
    detail.append(ell('Back pebble scale',(x,y,z),(r,r*.6,r*1.2),seg=12,rings=8))
for s in [-1,1]:
    for i in range(32):
        z=random.uniform(1.36,2.18); x=s*(.81+.09*sin((z-1.36)*5)); y=random.uniform(-.25,-.08)
        r=random.uniform(.013,.032)
        detail.append(ell('Arm scale',(x,y,z),(r,r*.7,r*1.2),seg=12,rings=8))
    for i in range(32):
        x=s*random.uniform(.1,.54); z=random.uniform(3.006,3.10)
        y=-.66-.397*math.sqrt(max(.01,1-(x/.65)**2-((z-3.035)/.235)**2))
        r=random.uniform(.008,.018)
        detail.append(ell('Muzzle pore rim',(x,y,z),(r,r*.55,r),seg=10,rings=6))
join(detail,'Rivermoor • folds claws and skin relief')

# Mushrooms grow from the shoulder blades and down the dorsal ridge.
fungi=[]
def mushroom(base,h,r,lean,idx):
    b=Vector(base); top=b+Vector((lean[0],lean[1],h))
    fungi.append(tube('Mushroom %02d • stem'%idx,[b,b+Vector((lean[0]*.22,lean[1]*.25,h*.48)),top],[r*.21,r*.15,r*.19],14,7))
    vs=[]; fs=[]; n=48
    prof=[(0,.65),(.16,.64),(.38,.55),(.64,.37),(.88,.10),(1,0),(.94,-.085),(.65,-.085),(.30,-.08),(0,-.06)]
    for k,(rad,z) in enumerate(prof):
        for j in range(n):
            a=2*pi*j/n
            scallop=1+.037*sin(7*a+idx)+.025*sin(11*a)
            vs.append(tuple(top+Vector((r*rad*cos(a)*scallop,r*rad*sin(a)*scallop,r*(z+.018*rad*sin(5*a+idx))))))
        if k:
            for j in range(n): fs.append(((k-1)*n+j,(k-1)*n+(j+1)%n,k*n+(j+1)%n,k*n+j))
    cap=mesh('Mushroom %02d • bell cap'%idx,vs,fs)
    sub=cap.modifiers.new('Smooth cap','SUBSURF'); sub.levels=1
    bpy.context.view_layer.objects.active=cap; bpy.ops.object.modifier_apply(modifier=sub.name); fungi.append(cap)
    for j in range(22):
        a=j/22*2*pi
        p1=top+Vector((r*.19*cos(a),r*.19*sin(a),-r*.08))
        p2=top+Vector((r*.55*cos(a),r*.55*sin(a),-r*.11))
        p3=top+Vector((r*.93*cos(a),r*.93*sin(a),-r*.07))
        fungi.append(tube('Radial mushroom gill',[p1,p2,p3],[.002,.005,.001],6,3))
    for j in range(15):
        a=random.uniform(0,2*pi); rr=random.uniform(.2,.83)
        zz=.65*(1-rr**1.7)
        p=top+Vector((r*rr*cos(a),r*rr*sin(a),r*zz))
        sz=random.uniform(.014,.025)
        fungi.append(ell('Cap fleck',p,(sz,sz,.008),seg=10,rings=6))

for idx,args in enumerate([
    ((.45,.43,2.43),.72,.29,(.18,.03)),
    ((-.43,.44,2.47),.58,.25,(-.19,.02)),
    ((.26,.60,2.18),.42,.23,(.24,.12)),
    ((-.30,.63,2.02),.47,.22,(-.20,.06)),
    ((.03,.65,2.64),.47,.23,(.05,.20)),
    ((.14,.62,1.65),.31,.17,(.19,.10)),
    ((-.35,.55,1.47),.28,.15,(-.15,.1)),
    ((.40,.49,2.79),.35,.19,(.26,.01)),
]): mushroom(*args,idx)
join(fungi,'Rivermoor • eight gilled mushrooms')

# Sparse leaf-like moss and dorsal thornlets give an organic silhouette.
growth=[]
for i in range(90):
    a=random.uniform(.12,pi-.12); z=random.uniform(1.6,2.85)
    x=.50*cos(a); y=.20+.42*sin(a)
    length=random.uniform(.07,.18)
    p=Vector((x,y,z)); q=p+Vector((random.uniform(-.07,.07),.065,length))
    growth.append(tube('Moss tendril',[p,p.lerp(q,.6)+Vector((.018,0,0)),q],[.017,.011,.001],7,3))
for i in range(15):
    a=i/14*pi
    x=.76*cos(a); z=3.17+.60*sin(a); y=.04
    growth.append(tube('Crown thornlet',[(x,y,z),(x*1.04,y+.025,z+.065),(x*1.05,y+.015,z+.11)],[.025,.012,.001],9))
for i in range(15):
    t=i/14; x=-.13-1.6*t; y=.96+.78*sin(t*pi*.67); z=.88-.57*sin(t*pi*.7)
    growth.append(tube('Tail ridge',[(x,y,z),(x-.022,y+.03,z+.095*(1-t)+.023)],[.034*(1-t)+.008,.001],8,3))
join(growth,'Rivermoor • moss and dorsal spines')

# The source sheet is packed in the project for later sculpt refinement.
ref=bpy.data.images.load('/home/never0n/Pictures/minimob.png',check_existing=True); ref.pack()
scene['reference']='minimob.png • supplied character turnaround'
scene['asset_notes']='Clay study. Sculpt geometry with separate eyes, surface relief and mushrooms. No rig or game retopology.'
bpy.ops.object.select_all(action='DESELECT'); skin.select_set(True); bpy.context.view_layer.objects.active=skin
for area in bpy.context.screen.areas:
    if area.type=='VIEW_3D': area.spaces.active.overlay.show_overlays=False
bpy.ops.wm.save_as_mainfile(filepath=OUT+'/rivermoor_clay.blend')
print('Detailed sculpt saved:',len(sculpt.objects),'objects')
