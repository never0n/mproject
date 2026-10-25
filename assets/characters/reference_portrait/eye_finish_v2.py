import bpy,math,json,random
from mathutils import Vector
random.seed(4)
pixels=json.load(open('/home/never0n/Projects/new_desctop_MMO/assets/characters/reference_portrait/iris_source.json'))
def linear(v):
    v=v/255;return v/12.92 if v<.04045 else ((v+.055)/1.055)**2.4
for o in bpy.context.scene.objects:
    if not o.name.startswith(('Iris_v2','Pupil_v2','Cornea_v2')):continue
    sign=-1 if o.name.endswith('-1') else 1;cx=sign*.324;cz=2.946
    attr=o.data.color_attributes.get('Iris')
    for i,v in enumerate(o.data.vertices):
        dx=v.co.x-cx;dz=v.co.z-cz;r=math.sqrt(dx*dx+dz*dz)
        if o.name.startswith('Iris'):nr=.023+(.065-.023)*(r-.023*1.23)/((.068-.023)*1.23)
        elif o.name.startswith('Pupil'):nr=r*.0235/(.0235*1.25)
        else:nr=r*.066/(.069*1.23)
        if r>1e-8:dx*=nr/r;dz*=nr/r
        lift=.0008
        if o.name.startswith('Cornea'):lift=.003*max(0,1-(nr/.066)**2)
        if o.name.startswith('Pupil'):lift=.0012
        v.co=(cx+dx,-.891-math.sqrt(max(.0001,.145**2-nr*nr))-lift,cz+dz+.003)
        if attr:
            px=88.5+20.5*dx/.065;py=898+abs(20.5*dz/.065)
            rgb=pixels[max(0,min(49,int(py-873)))][max(0,min(51,int(px-63)))]
            attr.data[i].color=(*[linear(c) for c in rgb],1)
# Move the chin surface and its follicles together, preserving their attachment.
def deform(p):
    x,y,z=p
    f=max(0,min(1,(-y-.5)/.35))*math.exp(-((z-1.99)/.23)**2)
    return Vector((x,y-.085*f,z))
for name in ['Head_v2','Portrait | finished surface']:
    o=bpy.data.objects[name]
    if o.data.shape_keys:
        for v in o.data.shape_keys.key_blocks['Portrait likeness'].data:v.co=deform(v.co)
    else:
        for v in o.data.vertices:v.co=deform(v.co)
for sp in bpy.data.objects['Beard_v2 | dense tapered follicles'].data.splines:
    for p in sp.points:p.co=(*deform(Vector(p.co[:3])),1)
# Trim the combed paths at a natural nape instead of continuing onto the neck.
for o in bpy.context.scene.objects:
    if o.type!='CURVE' or not o.name.startswith('Hair_v2'):continue
    for sp in list(o.data.splines):
        threshold=2.13+random.random()*.13;stop=None
        for j,p in enumerate(sp.points):
            if p.co.z<threshold and p.co.y>-.10:stop=j;break
        if stop==0:o.data.splines.remove(sp);continue
        if stop is not None:
            end=sp.points[stop-1].co.copy()
            for j,p in enumerate(sp.points):
                if j>=stop:p.co=end;p.radius=.001
                elif j>stop-4:p.radius*=max(.03,(stop-j)/4)
bpy.data.lights['Key_v2'].size=2.4
bpy.data.lights['Fill_v2'].energy=95
bpy.data.lights['Fill_v2'].size=3
s=bpy.context.scene;s.cycles.samples=96;s.render.resolution_x=700;s.render.resolution_y=800
s.camera.location=(0,-12,2.96);s.camera.rotation_euler=(math.pi/2,0,0);s.camera.data.ortho_scale=1.9
s.render.filepath='/tmp/v2_eye_real.png';bpy.ops.render.render(write_still=True)
s.camera.location=(4.5,-10,2.92);s.camera.rotation_euler=(Vector((0,-.3,2.75))-s.camera.location).to_track_quat('-Z','Y').to_euler();s.camera.data.ortho_scale=3.15
s.render.resolution_x=850;s.render.resolution_y=1050;s.render.filepath='/tmp/v2_hero_real.png';bpy.ops.render.render(write_still=True)
bpy.ops.wm.save_as_mainfile(filepath='/home/never0n/Projects/new_desctop_MMO/assets/characters/reference_portrait/portrait_v2_ready.blend')
