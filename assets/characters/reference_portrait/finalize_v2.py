import bpy,math,random
from mathutils import Vector
random.seed(707)
# Re-seat every visible eye layer on the same spherical surface. This removes
# the enlarged iris intersection with the lower lid and the exposed white ring.
for o in bpy.context.scene.objects:
    if not o.name.startswith(('Iris_v2','Pupil_v2','Cornea_v2')):continue
    sign=-1 if o.name.endswith('-1') else 1;cx=sign*.324;cz=2.946
    for v in o.data.vertices:
        dx=v.co.x-cx;dz=v.co.z-cz
        if o.name.startswith('Pupil'):
            dx*=1.25;dz*=1.25;v.co.x=cx+dx;v.co.z=cz+dz
        r=math.sqrt(dx*dx+dz*dz)
        lift=.0008
        if o.name.startswith('Cornea'):lift=.003*max(0,1-(r/.085)**2)
        if o.name.startswith('Pupil'):lift=.001
        v.co.y=-.891-math.sqrt(max(.00001,.145**2-r*r))-lift
def smooth(a,b,x):
    t=max(0,min(1,(x-a)/(b-a)));return t*t*(3-2*t)
def mask(p):
    x,y,z=p;ax=abs(x)
    if ax>.71 and y>-.48 and z>2.40:return 0
    neckline=1.82+.64*max(0,y+1.05)
    jaw=smooth(neckline,neckline+.07,z)*(1-smooth(-.28,-.12,y))
    top=2.20+.60*ax
    face=1-smooth(top-.035,top+.045,z)
    lip=(1-smooth(.28,.34,ax))*smooth(2.105,2.15,z)*(1-smooth(2.27,2.29,z))
    face*=1-lip
    stache=(1-smooth(.30,.36,ax))*smooth(2.29,2.33,z)*(1-smooth(2.43,2.47,z))
    if ax<.10:stache*=1-smooth(2.40,2.44,z)
    soul=(1-smooth(.09,.13,ax))*smooth(2.07,2.11,z)*(1-smooth(2.15,2.19,z))
    return min(1,max(face,stache,soul))*jaw
for name in ['Head_v2','Portrait | finished surface']:
    o=bpy.data.objects[name];vs=o.data.shape_keys.key_blocks['Portrait likeness'].data if o.data.shape_keys else o.data.vertices
    attr=o.data.color_attributes['Follicle coverage']
    for a,v in zip(attr.data,vs):
        c=list(a.color);c[1]=mask(v.co);a.color=c
cu=bpy.data.objects['Beard_v2 | dense tapered follicles'].data
for sp in list(cu.splines):
    p=Vector(sp.points[0].co[:3])
    if mask(p)<random.random()*.75:cu.splines.remove(sp)
cu=bpy.data.objects['Brows_v2 | directional fine hairs'].data
for sp in cu.splines:
    for p in sp.points:p.co.z-=.015
s=bpy.context.scene;s.cycles.samples=96;s.render.resolution_x=700;s.render.resolution_y=800
s.camera.location=(0,-12,2.97);s.camera.rotation_euler=(math.pi/2,0,0);s.camera.data.ortho_scale=1.85
s.render.filepath='/tmp/v2_eye_check.png';bpy.ops.render.render(write_still=True)
s.camera.location=(4.5,-10,2.92);s.camera.rotation_euler=(Vector((0,-.3,2.75))-s.camera.location).to_track_quat('-Z','Y').to_euler();s.camera.data.ortho_scale=3.15
s.render.resolution_x=850;s.render.resolution_y=1050;s.render.filepath='/tmp/v2_final_check.png';bpy.ops.render.render(write_still=True)
bpy.ops.wm.save_as_mainfile(filepath='/home/never0n/Projects/new_desctop_MMO/assets/characters/reference_portrait/portrait_v2_final_work.blend')
