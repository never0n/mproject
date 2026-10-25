"""Polish the existing v3 groom. No loading, rendering, camera edits or saves.

Run with runpy.run_path(...)["polish_groom"](mouth_z=2.17).
Preserves the original material datablocks. Run once per generated groom;
the collection marker makes repeat calls a no-op.
"""
import math
import random

import bpy
from mathutils import Vector
from mathutils.bvhtree import BVHTree
from mathutils.kdtree import KDTree


def smooth(a, b, x):
    t = max(0.0, min(1.0, (x-a)/(b-a)))
    return t*t*(3-2*t)


def unit(v, fallback=(1, 0, 0)):
    return v.normalized() if v.length_squared > 1e-12 else Vector(fallback)


def polish_groom(head_name='Portrait | finished surface', mouth_z=2.17,
                 beard_tip_z=1.65, extra_beard_count=9000, seed=41418):
    rng = random.Random(seed)
    head = bpy.data.objects[head_name]
    hair = next(o for o in bpy.context.scene.objects
                if o.name.startswith('Hair_v3 | coherent') and not o.hide_render)
    beard = next(o for o in bpy.context.scene.objects
                 if o.name.startswith('Beard_v3 | clustered') and not o.hide_render)
    col = hair.users_collection[0]
    if col.get('polished_reference_groom', False):
        print('This v3 groom is already polished; no changes made.')
        return {'already_polished': True}
    ev = head.evaluated_get(bpy.context.evaluated_depsgraph_get())
    mesh = ev.to_mesh()
    mesh.calc_loop_triangles()
    vs = [v.co.copy() for v in mesh.vertices]
    ns = [v.normal.copy() for v in mesh.vertices]
    tris = [tuple(t.vertices) for t in mesh.loop_triangles]
    bvh = BVHTree.FromPolygons(vs, tris, all_triangles=True)
    zoff = max(p.z for p in vs)-4.05

    def material(name, color, rough, spec=.24, anisotropy=.35):
        mat = bpy.data.materials.new(name)
        mat.use_nodes = True
        bs = next(n for n in mat.node_tree.nodes if n.type == 'BSDF_PRINCIPLED')
        bs.inputs['Base Color'].default_value = (*color, 1)
        bs.inputs['Roughness'].default_value = rough
        bs.inputs['Specular IOR Level'].default_value = spec
        if 'Anisotropic IOR Level' in bs.inputs:
            bs.inputs['Anisotropic IOR Level'].default_value = anisotropy
        elif 'Anisotropic' in bs.inputs:
            bs.inputs['Anisotropic'].default_value = anisotropy
        return mat

    # Existing hairs are beveled polygonal tubes, for which a surface BSDF
    # produces substantially more stable highlights than a hair-only BSDF.
    mats = [material('Hair_v3 | stable espresso surface', (.009, .0065, .0045), .48),
            material('Hair_v3 | stable warm fibre variation', (.019, .012, .008), .50),
            material('Hair_v3 | muted silver surface', (.090, .077, .063), .56, .19)]
    curve_objects = [o for o in col.objects if o.type == 'CURVE' and not o.hide_render]
    original_count = sum(len(o.data.splines) for o in curve_objects)
    for obj in curve_objects:
        obj.data.materials.clear()
        for mat in mats:
            obj.data.materials.append(mat)

    # Large coherent lock crests replace some of the uniformly smooth top.
    # Children stay in their existing bundles; roots remain fixed.
    for sp in hair.data.splines:
        root = Vector(sp.points[0].co[:3])
        side = smooth(.37, .70, abs(root.x))
        phase = round(root.x / .13) * 1.07 + round(root.y / .16) * .47
        for j, point in enumerate(sp.points):
            t = j/max(1, len(sp.points)-1)
            p = Vector(point.co[:3])
            hit, n, _, _ = bvh.find_nearest(p)
            if hit is None:
                continue
            envelope = math.sin(math.pi*t) * smooth(0, .10, t)
            p += n * (.019 + .012*side) * math.sin(t*math.tau*1.35+phase) * envelope
            point.co = (*p, 1)

    # Trim the very few low isolated long scalp strands in front of ears.
    # Their original full curves are retained as hidden archival objects.
    trim_archive = []
    for obj in curve_objects:
        if not obj.name.startswith('Hair_v3') or 'flyaway' in obj.name.lower():
            continue
        bad = []
        for sp in obj.data.splines:
            low = [p for p in sp.points
                   if abs(p.co.x) > .63 and p.co.y < -.35 and p.co.z < 2.94+zoff]
            if low and len(low) > max(2, len(sp.points)//8):
                bad.append(sp)
        if bad:
            archive_data = bpy.data.curves.new(obj.name+' | trimmed archive', 'CURVE')
            archive_data.dimensions = '3D'
            for sp in bad:
                copied = archive_data.splines.new('POLY')
                copied.points.add(len(sp.points)-1)
                for dst, src in zip(copied.points, sp.points):
                    dst.co = src.co
                    dst.radius = src.radius
                obj.data.splines.remove(sp)
            archived = bpy.data.objects.new(archive_data.name, archive_data)
            col.objects.link(archived)
            archived.matrix_world = obj.matrix_world.copy()
            archived.hide_render = archived.hide_viewport = True
            trim_archive.append(len(bad))

    # Profile QA revealed a long tapered rat-tail. Compress only the free
    # low back ends into a loose, uneven nape ending near z2.2; preserve
    # roots and retain curls instead of cutting every tip onto one plane.
    for obj in curve_objects:
        if not obj.name.startswith('Hair_v3'):
            continue
        for sp in obj.data.splines:
            root = Vector(sp.points[0].co[:3])
            phase = root.x*23 + root.y*11
            for point in sp.points:
                p = Vector(point.co[:3])
                if p.y > -.17 and p.z < 2.38+zoff:
                    depth = 2.38+zoff-p.z
                    p.z = 2.38+zoff-depth*.32
                    fade = smooth(0, .20, depth)
                    p.y += .035*fade*(1+.40*math.sin(depth*19+phase))
                    p.x += .022*fade*math.sin(depth*22+phase)
                    point.co=(*p,1)

    def boundary(p):
        ax = abs(p.x)
        if p.y < -.83:
            h = 3.57-.10*(min(ax,.75)/.75)**1.6
            h += .045*math.exp(-((ax-.50)/.15)**2)
            h -= .023*math.exp(-(p.x/.10)**2)
        elif p.y < -.48:
            h = 3.48-(p.y+.83)*1.30
        elif p.y < -.10:
            h = 3.025-(p.y+.48)*.21
        else:
            h = 2.945-(p.y+.10)*1.60
        return h+zoff+.009*math.sin(p.x*31+p.y*17)

    cap = next(o for o in col.objects if o.name.startswith('Hair_v3 | softly blended'))
    density = cap.data.color_attributes['Root density']
    for attr, vertex in zip(density.data, cap.data.vertices):
        p = vertex.co
        # Fade from individual root hairs to opaque coverage across 12 mm.
        feather = .12 if p.y < -.68 else .07
        value = smooth(.004, feather, p.z-boundary(p))
        attr.color = (value, value, value, 1)

    def new_curve(name, width):
        data = bpy.data.curves.new(name, 'CURVE')
        data.dimensions = '3D'
        data.resolution_u = 1
        data.bevel_depth = width
        data.bevel_resolution = 0
        for mat in mats:
            data.materials.append(mat)
        obj = bpy.data.objects.new(name, data)
        col.objects.link(obj)
        obj.matrix_world = head.matrix_world.copy()
        return obj

    def strand(obj, points, material_index=0, width=1.0):
        sp = obj.data.splines.new('POLY')
        sp.points.add(len(points)-1)
        sp.material_index = material_index
        for i, (dst, p) in enumerate(zip(sp.points, points)):
            t = i/max(1, len(points)-1)
            dst.co = (*p, 1)
            dst.radius = width*max(.025, (1-t)**.62)

    edge_obj = new_curve('Hair_v3 | delicate irregular hairline roots', .00039)
    edge_vertices = [i for i, p in enumerate(vs)
                     if p.y < -.52 and abs(p.x) < .72
                     and -.015 < p.z-boundary(p) < .11]
    if edge_vertices:
        for unused in range(650):
            vi = rng.choice(edge_vertices)
            p, n = vs[vi], ns[vi]
            tangent = Vector((-.18, .84, .42))
            tangent = unit(tangent-n*tangent.dot(n))
            side = unit(n.cross(tangent))
            jitter = side*rng.uniform(-.010, .010)+tangent*rng.uniform(-.008, .008)
            root, normal, _, _ = bvh.find_nearest(p+jitter)
            if root is None:
                continue
            length = rng.uniform(.032, .12)
            points = []
            for j in range(9):
                t = j/8
                hit, n, _, _ = bvh.find_nearest(root+tangent*length*t)
                points.append(hit+n*(.0012+.005*math.sin(math.pi*t)))
            strand(edge_obj, points, 1 if rng.random()<.12 else 0, rng.uniform(.70, 1))

    # Preserve anchored roots, but lengthen the chin fibres below the skin
    # into the rounded, substantial beard silhouette present in reference.
    central = [sp for sp in beard.data.splines
               if abs(sp.points[0].co.x)<.31 and sp.points[0].co.y<-.55
               and sp.points[0].co.z<2.02]
    if central:
        current_tip = min(p.co.z for sp in central for p in sp.points)
        extension = max(.050, min(.15, current_tip-beard_tip_z))
    else:
        extension = .105
    for sp in beard.data.splines:
        root = Vector(sp.points[0].co[:3])
        chin = 1-smooth(1.88, 2.12, root.z)
        chin *= 1-smooth(.42, .70, abs(root.x))
        chin *= 1-smooth(-.55, -.30, root.y)
        variation = rng.uniform(.80, 1.08)
        for j, point in enumerate(sp.points):
            t = j/max(1, len(sp.points)-1)
            point.co.z -= extension*chin*variation*t**1.25
            point.co.y -= .014*chin*math.sin(math.pi*t*.80)

    # Finer secondary fibres fill actual root gaps and follow their nearest
    # established curl; random submillimetre offsets avoid doubled outlines.
    new_beard = new_curve('Beard_v3 | fine curl undercoat', .00049)
    available = list(beard.data.splines)
    allowed = max(0, 49900-original_count-650)
    extra_beard_count = min(extra_beard_count, allowed)
    for unused in range(extra_beard_count):
        src = rng.choice(available)
        root = Vector(src.points[0].co[:3])
        hit, normal, _, _ = bvh.find_nearest(root)
        side = unit(normal.cross(Vector((0,0,-1))))
        desired = hit+side*rng.uniform(-.011,.011)+Vector((0,0,rng.uniform(-.007,.007)))
        new_root, n, _, _ = bvh.find_nearest(desired)
        if new_root is None:
            continue
        # Respect bare lips when shifting moustache children.
        if (new_root.x/.29)**2+((new_root.z-mouth_z)/.075)**2 < 1.03:
            continue
        delta = new_root-root+n*.001
        length_scale = rng.uniform(.76, 1.10)
        points = []
        phase = rng.uniform(0, math.tau)
        for j, src_point in enumerate(src.points):
            t = j/max(1,len(src.points)-1)
            p = root+(Vector(src_point.co[:3])-root)*length_scale+delta
            p += side*.0017*math.sin(t*9+phase)*math.sin(math.pi*t)
            points.append(p)
        grey = .025 if root.z < 2.03 else .004
        draw = rng.random()
        mi = 2 if draw<grey else (1 if draw<grey+.10 else 0)
        strand(new_beard, points, mi, rng.uniform(.80,1.10))

    # A close, matte root coverage shell gives the dense reference beard its
    # dark body, while retaining the real hairs and a soft cheek boundary.
    roots = [Vector(sp.points[0].co[:3]) for sp in beard.data.splines]
    tree = KDTree(len(roots))
    for i,p in enumerate(roots):
        tree.insert(p,i)
    tree.balance()
    mask = []
    for p in vs:
        _, _, distance = tree.find(p)
        value = 1-smooth(.010, .038, distance)
        value *= .82 if p.z < mouth_z-.12 else .53
        # Extra falloff protects the eyelids, lips and the neck boundary.
        if p.y>-.18 or ((p.x/.29)**2+((p.z-mouth_z)/.078)**2<1.02):
            value=0
        mask.append(value)
    ids=set()
    faces=[]
    for tri in tris:
        if sum(mask[i] for i in tri)/3>.015:
            faces.append(tri)
            ids.update(tri)
    ids=sorted(ids)
    remap={i:j for j,i in enumerate(ids)}
    shell=bpy.data.meshes.new('Beard_v3 | matte roots mesh')
    shell.from_pydata([vs[i]+ns[i]*.0007 for i in ids],[],
                     [[remap[i] for i in tri] for tri in faces])
    shell.update()
    for p in shell.polygons:
        p.use_smooth=True
    attr=shell.color_attributes.new(name='Beard root coverage',type='FLOAT_COLOR',domain='POINT')
    for a,i in zip(attr.data,ids):
        a.color=(mask[i],mask[i],mask[i],1)
    root_mat=material('Beard_v3 | diffuse root body',(.010,.007,.005),.76,.16,0)
    nt=root_mat.node_tree
    bs=next(n for n in nt.nodes if n.type=='BSDF_PRINCIPLED')
    out=next(n for n in nt.nodes if n.type=='OUTPUT_MATERIAL')
    att=nt.nodes.new('ShaderNodeVertexColor');att.layer_name='Beard root coverage'
    trans=nt.nodes.new('ShaderNodeBsdfTransparent')
    mix=nt.nodes.new('ShaderNodeMixShader')
    nt.links.new(att.outputs['Color'],mix.inputs[0])
    nt.links.new(trans.outputs[0],mix.inputs[1])
    nt.links.new(bs.outputs[0],mix.inputs[2])
    nt.links.new(mix.outputs[0],out.inputs['Surface'])
    shell.materials.append(root_mat)
    shell_obj=bpy.data.objects.new('Beard_v3 | soft dense root coverage',shell)
    col.objects.link(shell_obj)
    shell_obj.matrix_world=head.matrix_world.copy()

    ev.to_mesh_clear()
    col['polished_reference_groom']=True
    col['chin_extension_world']=extension
    col['beard_tip_target_z']=beard_tip_z
    new_count=sum(len(o.data.splines) for o in col.objects
                  if o.type=='CURVE' and not o.hide_render)
    print('Polished groom:',{'visible_strands':new_count,'chin_extension':extension,
                            'trimmed_temple_strands':sum(trim_archive)})
    return {'visible_strands':new_count,'chin_extension':extension,
            'trimmed_temple_strands':sum(trim_archive)}


if __name__ == '__main__':
    polish_groom()
