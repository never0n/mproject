"""Reference portrait groom, Blender 4.x/5.x.

Import this file and call build_groom() AFTER fitting the presentation mesh.
No file opening, saving or rendering occurs here. All coordinates are local
to the fitted presentation mesh; generated objects inherit its transform.
This intentionally uses portable POLY curves rather than a particle system.
"""
from pathlib import Path
import math
import random
import bisect

import bpy
from mathutils import Vector
from mathutils.bvhtree import BVHTree


ASSET_ROOT = Path(__file__).resolve().parent.parent
TAU = math.tau


def smooth(a, b, x):
    t = max(0.0, min(1.0, (x - a) / (b - a)))
    return t * t * (3.0 - 2.0 * t)


def unit(v, fallback=(0, 0, -1)):
    return v.normalized() if v.length_squared > 1e-12 else Vector(fallback)


def build_groom(head_name='Portrait | finished surface', mouth_z=2.16,
                chin_z=1.67, rebuild_beard=True, seed=93471,
                guide_count=760, children_per_guide=16,
                undercoat_count=7200, beard_count=17000,
                hairline_front_z=3.57, density_scale=1.0):
    """Create ~36,000 fine strands, preserving all previous grooms hidden.

    mouth_z is the fitted mouth seam; chin_z is the bare chin surface.
    density_scale can be lowered for previews, but changes strand count only.
    Returns a small dictionary of output objects, count and fitting settings.
    """
    planned_count = (guide_count * children_per_guide + undercoat_count + 280
                     + (beard_count if rebuild_beard else 0)) * density_scale
    if density_scale <= 0 or planned_count > 50000:
        raise ValueError('Groom density must be positive and keep the total under 50,000 strands')
    rng = random.Random(seed)
    ob = bpy.data.objects[head_name]
    deps = bpy.context.evaluated_depsgraph_get()
    ev = ob.evaluated_get(deps)
    mesh = ev.to_mesh()
    mesh.calc_loop_triangles()
    verts = [v.co.copy() for v in mesh.vertices]
    normals = [v.normal.copy() for v in mesh.vertices]
    triangles = [tuple(t.vertices) for t in mesh.loop_triangles]
    bvh = BVHTree.FromPolygons(verts, triangles, all_triangles=True)
    top = max(v.z for v in verts)
    zoff = top - 4.05
    strand_count = 0

    # Retire previous attempts reversibly. Eyebrows remain available.
    prefixes = ('Hair_v2', 'Scalp |', 'Hair | continuous scalp', 'Hair_v3')
    if rebuild_beard:
        prefixes += ('Beard_v2', 'Beard_v3')
    for old in bpy.context.scene.objects:
        if old.name.startswith(prefixes):
            old.hide_render = True
            old.hide_viewport = True
    for old in bpy.data.collections:
        if old.name.startswith('Groom v3 |'):
            old.hide_render = True
            old.hide_viewport = True
    collection = bpy.data.collections.new('Groom v3 | reference waves')
    bpy.context.scene.collection.children.link(collection)

    def hairline(p):
        x, y, z = p
        ax = abs(x)
        # Frontal widow's peak is restrained; lateral temporal recession is
        # deeper than the centre. Back boundary reaches below the ear.
        if y < -.83:
            h = hairline_front_z - .10 * (min(ax, .75) / .75) ** 1.6
            h += .045 * math.exp(-((ax - .50) / .15) ** 2)
            h -= .023 * math.exp(-(x / .10) ** 2)
        elif y < -.48:
            h = 3.48 - (y + .83) * 1.30
        elif y < -.10:
            h = 3.025 - (y + .48) * .21
        else:
            h = 2.945 - (y + .10) * 1.60
        # Low frequency irregularity only: never a saw-tooth boundary.
        return h + zoff + .009 * math.sin(x * 31 + y * 17)

    def ear_exclusion(p):
        ax = abs(p.x)
        # Ear geometry extends lateral to the scalp. Do not grow hairs on
        # helix, antihelix or lobule, including triangles behind the ear.
        if ax > .705 and -.53 < p.y < -.055 and p.z < 3.16 + zoff:
            return 0.0
        return 1.0

    def scalp_mask(p):
        if p.z < 2.48 + zoff:
            return 0.0
        return smooth(-.007, .060, p.z - hairline(p)) * ear_exclusion(p)

    def beard_mask(p):
        x, y, z = p
        ax = abs(x)
        if y > -.18 or (ax > .705 and y > -.50 and z > 2.42):
            return 0.0
        neckline = chin_z + .025 + .71 * max(0, y + 1.04)
        lower = smooth(neckline - .025, neckline + .045, z)
        top_cheek = mouth_z + .055 + .60 * ax
        sideburn = smooth(.55, .72, ax) * smooth(-.88, -.56, y)
        top_cheek += .36 * sideburn
        field = 1.0 - smooth(top_cheek - .025, top_cheek + .055, z)
        # Smooth elliptical exclusion surrounds the actual lips.
        lip_ellipse = (x / .30) ** 2 + ((z - mouth_z) / .081) ** 2
        field *= smooth(.94, 1.28, lip_ellipse)
        # Keep the skin gap beneath the lower lip, retaining the soul patch.
        bare_below_lip = (1.0 - smooth(.20, .27, ax))
        bare_below_lip *= smooth(mouth_z - .17, mouth_z - .105, z)
        bare_below_lip *= 1.0 - smooth(mouth_z - .065, mouth_z - .035, z)
        field *= 1.0 - .96 * bare_below_lip
        stache = (1.0 - smooth(.285, .365, ax))
        stache *= smooth(mouth_z + .033, mouth_z + .075, z)
        stache *= 1.0 - smooth(mouth_z + .235, mouth_z + .28, z)
        if ax < .09:
            stache *= 1.0 - smooth(mouth_z + .18, mouth_z + .225, z)
        soul = (1.0 - smooth(.075, .125, ax))
        soul *= smooth(mouth_z - .235, mouth_z - .19, z)
        soul *= 1.0 - smooth(mouth_z - .115, mouth_z - .080, z)
        return lower * max(field, stache, soul)

    def diffuse_material(name, color, roughness):
        m = bpy.data.materials.new(name)
        m.use_nodes = True
        bs = next(n for n in m.node_tree.nodes if n.type == 'BSDF_PRINCIPLED')
        bs.inputs['Base Color'].default_value = (*color, 1)
        bs.inputs['Roughness'].default_value = roughness
        bs.inputs['Specular IOR Level'].default_value = .25
        return m

    # Actual hair BSDF: longitudinal and azimuthal roughness avoid a plastic
    # solid helmet highlight, while restrained melanin variation supplies
    # warm brown fibres visible under a soft studio key.
    def hair_material(name, melanin, tint, roughness):
        m = bpy.data.materials.new(name)
        m.use_nodes = True
        nt = m.node_tree
        nt.nodes.clear()
        out = nt.nodes.new('ShaderNodeOutputMaterial')
        bs = nt.nodes.new('ShaderNodeBsdfHairPrincipled')
        try:
            bs.parametrization = 'MELANIN'
        except (TypeError, AttributeError):
            pass
        settings = {'Melanin': melanin, 'Melanin Redness': .35,
                    'Tint': (*tint, 1), 'Roughness': roughness,
                    'Radial Roughness': .40, 'Coat': .06,
                    'IOR': 1.55, 'Random Color': .12,
                    'Random Roughness': .10}
        for key, value in settings.items():
            if key in bs.inputs:
                bs.inputs[key].default_value = value
        nt.links.new(bs.outputs[0], out.inputs['Surface'])
        return m

    hair_mats = [
        hair_material('Hair_v3 | deep espresso fibres', .86, (.78, .65, .53), .34),
        hair_material('Hair_v3 | brown fibre variation', .74, (.70, .56, .43), .40),
        hair_material('Hair_v3 | restrained silver fibres', .18, (.60, .56, .50), .44),
    ]

    def curve_object(name, radius):
        data = bpy.data.curves.new(name, 'CURVE')
        data.dimensions = '3D'
        data.resolution_u = 1
        data.bevel_depth = radius
        data.bevel_resolution = 0
        data.use_fill_caps = False
        for mat in hair_mats:
            data.materials.append(mat)
        obj = bpy.data.objects.new(name, data)
        collection.objects.link(obj)
        obj.matrix_world = ob.matrix_world.copy()
        return obj

    def add_strand(obj, points, grey=.007, width=1.0):
        nonlocal strand_count
        sp = obj.data.splines.new('POLY')
        sp.points.add(len(points) - 1)
        value = rng.random()
        sp.material_index = 2 if value < grey else (1 if value < grey + .16 else 0)
        # Three-way bevel is thin enough to remain a fibre, not a tube.
        radius_scale = width * rng.uniform(.77, 1.14)
        for i, (point, co) in enumerate(zip(sp.points, points)):
            t = i / max(1, len(points) - 1)
            point.co = (*co, 1.0)
            point.radius = radius_scale * max(.025, (1.0 - t) ** .62)
        strand_count += 1

    def weighted_surface(mask, threshold=.015):
        selected, weights = [], []
        cumulative = 0.0
        for index, tri in enumerate(triangles):
            a, b, c = (verts[i] for i in tri)
            mid = (a + b + c) / 3.0
            coverage = mask(mid)
            if coverage > threshold:
                area = (b - a).cross(c - a).length * .5
                cumulative += area * coverage
                selected.append(index)
                weights.append(cumulative)
        if not selected:
            raise RuntimeError('No root triangles match the fitted groom mask')
        return selected, weights

    scalp_faces, scalp_weights = weighted_surface(scalp_mask)

    def sample_root(faces, weights, mask):
        for unused in range(30):
            tri = triangles[faces[bisect.bisect_left(weights, rng.random() * weights[-1])]]
            a, b, c = (verts[i] for i in tri)
            u, v = rng.random(), rng.random()
            if u + v > 1:
                u, v = 1 - u, 1 - v
            point = a + (b - a) * u + (c - a) * v
            if mask(point) >= rng.random():
                normal = unit(normals[tri[0]] * (1-u-v) + normals[tri[1]] * u + normals[tri[2]] * v)
                return point, normal
        return None

    # A 0.1 mm root shell closes microscopic sampling gaps without making
    # a volumetric hair helmet. The boundary fades using the true mask.
    scalp_ids = set()
    cap_faces = []
    for tri in triangles:
        if sum(scalp_mask(verts[i]) for i in tri) / 3 > .006:
            scalp_ids.update(tri)
            cap_faces.append(tri)
    scalp_ids = sorted(scalp_ids)
    remap = {i: k for k, i in enumerate(scalp_ids)}
    cap_mesh = bpy.data.meshes.new('Hair_v3 | root shell mesh')
    cap_mesh.from_pydata([verts[i] + normals[i] * .001 for i in scalp_ids], [],
                        [[remap[i] for i in tri] for tri in cap_faces])
    cap_mesh.update()
    attr = cap_mesh.color_attributes.new(name='Root density', type='FLOAT_COLOR', domain='POINT')
    for point, index in zip(attr.data, scalp_ids):
        density = scalp_mask(verts[index])
        point.color = (density, density, density, 1)
    for face in cap_mesh.polygons:
        face.use_smooth = True
    cap_mat = diffuse_material('Hair_v3 | soft dark roots', (.006, .004, .0028), .67)
    nt = cap_mat.node_tree
    bs = next(n for n in nt.nodes if n.type == 'BSDF_PRINCIPLED')
    out = next(n for n in nt.nodes if n.type == 'OUTPUT_MATERIAL')
    vertex = nt.nodes.new('ShaderNodeVertexColor')
    vertex.layer_name = 'Root density'
    trans = nt.nodes.new('ShaderNodeBsdfTransparent')
    mix = nt.nodes.new('ShaderNodeMixShader')
    nt.links.new(vertex.outputs['Color'], mix.inputs[0])
    nt.links.new(trans.outputs[0], mix.inputs[1])
    nt.links.new(bs.outputs[0], mix.inputs[2])
    nt.links.new(mix.outputs[0], out.inputs['Surface'])
    cap_mesh.materials.append(cap_mat)
    cap_obj = bpy.data.objects.new('Hair_v3 | softly blended roots', cap_mesh)
    collection.objects.link(cap_obj)
    cap_obj.matrix_world = ob.matrix_world.copy()

    def flow_at(point, normal):
        sign = 1 if point.x > 0 else -1
        side = smooth(.35, .69, abs(point.x))
        back = smooth(-.28, .26, point.y)
        # Front locks sweep up and back; the sides wrap around ears, then
        # turn down into the wavy nape. Slight asymmetric top sweep.
        desired = Vector((-.26 * (1-side) + sign * .055 * side,
                          .96 - .30 * back,
                          .30 * (1-side) * (1-back) - .40 * side - .77 * back))
        return unit(desired - normal * desired.dot(normal))

    def make_guide(origin, n0, length, phase, steps=40, flyaway=False):
        point = origin.copy()
        guide = []
        side = smooth(.32, .68, abs(origin.x))
        back = smooth(-.16, .22, origin.y)
        # Locks are coherent S curves; phase shared among children and
        # related to root position, rather than uniform straight strips.
        wavelength = rng.uniform(.40, .62) if side > .4 else rng.uniform(.58, .85)
        amplitude = rng.uniform(.032, .061) * (.80 + .40 * side)
        volume = rng.uniform(.060, .115) * (1.0 - .30 * back)
        last_tangent = flow_at(origin, n0)
        for j in range(steps + 1):
            t = j / steps
            hit, normal, _, distance = bvh.find_nearest(point)
            if hit is None:
                break
            tangent = flow_at(hit, normal)
            lateral = unit(normal.cross(tangent), (1, 0, 0))
            distance_along = length * t
            wave_phase = TAU * distance_along / wavelength + phase
            wave_direction = .34 * math.sin(wave_phase) * side
            tangent = unit(tangent + lateral * wave_direction)
            # Avoid tight discontinuous flips near the back/crown.
            tangent = unit(tangent * .72 + last_tangent * .28)
            root_fade = smooth(0, .13, t)
            envelope = math.sin(math.pi * min(.995, t)) ** .72
            lift = .0014 + root_fade * (.022 + volume * envelope)
            lift += amplitude * .45 * math.sin(wave_phase) * root_fade
            if flyaway:
                lift += .040 * envelope
            lateral_wave = amplitude * math.sin(wave_phase) * root_fade
            # Loose, lifted nape ends curl off the skin instead of being
            # projected back into a straight solid edge.
            curl = smooth(.65, 1.0, t) * back
            lift += .060 * curl * (1 + math.sin(wave_phase * 1.4))
            lateral_wave += .040 * curl * math.cos(wave_phase * 1.4)
            guide.append((hit, normal, tangent, lateral, max(.0012, lift), lateral_wave))
            point = hit + tangent * (length / steps)
            last_tangent = tangent
        return guide

    main_obj = curve_object('Hair_v3 | coherent swept wavy locks', .00057)
    guide_count = max(1, int(guide_count * density_scale))
    for unused in range(guide_count):
        sampled = sample_root(scalp_faces, scalp_weights, scalp_mask)
        if sampled is None:
            continue
        origin, normal = sampled
        side = smooth(.35, .70, abs(origin.x))
        back = smooth(-.12, .25, origin.y)
        length = rng.uniform(.89, 1.40) * (1 - .23 * side - .28 * back)
        phase = origin.z * 13 + origin.y * 9 + abs(origin.x) * 3 + rng.uniform(-.38, .38)
        guide = make_guide(origin, normal, length, phase)
        if len(guide) < 3:
            continue
        lock_width = rng.uniform(.020, .043)
        for child in range(children_per_guide):
            across = rng.uniform(-lock_width, lock_width)
            along = rng.uniform(-.018, .018)
            root_guess = origin + guide[0][3] * across + guide[0][2] * along
            root_hit, root_normal, _, _ = bvh.find_nearest(root_guess)
            if root_hit is None or scalp_mask(root_hit) < .03:
                continue
            root_delta = root_hit - origin
            child_phase = rng.uniform(0, TAU)
            points = []
            length_fraction = rng.uniform(.82, 1.0)
            end = max(3, int(len(guide) * length_fraction))
            for j, (hit, n, tangent, lateral, lift, wave) in enumerate(guide[:end]):
                t = j / max(1, end - 1)
                spread = 1 - .40 * smooth(.12, .9, t)
                micro = .0016 * math.sin(t * 19 + child_phase) * math.sin(math.pi * t)
                point = hit + n * (lift + micro) + lateral * wave
                point += root_delta * spread
                points.append(point)
            points[0] = root_hit + root_normal * .0013
            add_strand(main_obj, points, grey=.006)

    under_obj = curve_object('Hair_v3 | fine flowing undercoat', .00048)
    for unused in range(int(undercoat_count * density_scale)):
        sampled = sample_root(scalp_faces, scalp_weights, scalp_mask)
        if sampled is None:
            continue
        origin, normal = sampled
        length = rng.uniform(.15, .34)
        phase = origin.z * 13 + origin.y * 9
        guide = make_guide(origin, normal, length, phase, steps=9)
        points = [p + n * min(.022, lift * .24) + lateral * wave * .25
                  for p, n, tangent, lateral, lift, wave in guide]
        if len(points) > 2:
            points[0] = origin + normal * .0011
            add_strand(under_obj, points, grey=.003)

    fly_obj = curve_object('Hair_v3 | soft silhouette and hairline flyaways', .00040)
    for unused in range(int(280 * density_scale)):
        sampled = sample_root(scalp_faces, scalp_weights, scalp_mask)
        if sampled is None:
            continue
        origin, normal = sampled
        guide = make_guide(origin, normal, rng.uniform(.45, .95), rng.uniform(0, TAU),
                           steps=24, flyaway=True)
        points = [p + n * lift + lateral * wave for p, n, tangent, lateral, lift, wave in guide]
        if len(points) > 2:
            add_strand(fly_obj, points, grey=.012, width=.78)

    beard_obj = None
    if rebuild_beard:
        beard_faces, beard_weights = weighted_surface(beard_mask)
        beard_obj = curve_object('Beard_v3 | clustered short curls and moustache', .00063)
        for unused in range(int(beard_count * density_scale)):
            sampled = sample_root(beard_faces, beard_weights, beard_mask)
            if sampled is None:
                continue
            origin, normal = sampled
            ax = abs(origin.x)
            sign = 1 if origin.x > 0 else -1
            stache = origin.z > mouth_z + .035 and ax < .37
            chin = 1 - smooth(chin_z + .19, chin_z + .43, origin.z)
            cheek_boundary = beard_mask(origin)
            length = rng.uniform(.043, .081) * (1 + .37 * chin)
            if stache:
                length = rng.uniform(.035, .068)
            length *= .60 + .40 * cheek_boundary
            tangent = Vector((sign * .46 if stache else origin.x * .15,
                              -.10, -.70 if stache else -1.0))
            tangent -= normal * tangent.dot(normal)
            tangent = unit(tangent)
            side = unit(normal.cross(tangent), (1, 0, 0))
            # Neighbouring hairs share a low-frequency curl field, while
            # individual phases and length prevent turf-like repetition.
            phase = origin.x * 43 + origin.z * 37 + rng.uniform(-.7, .7)
            curl = rng.uniform(.0028, .0065) * (1.35 if chin > .5 else 1)
            points = []
            for j in range(10):
                t = j / 9
                base = origin + tangent * length * t
                hit, n, _, _ = bvh.find_nearest(base)
                if hit is None:
                    hit, n = base, normal
                lift = .0012 + (.005 + .011 * chin) * math.sin(math.pi * t) + .004 * t
                p = hit + n * lift
                p += side * curl * math.sin(t * 7.2 + phase) * smooth(0, .25, t)
                if t > .65:
                    p += n * curl * .8 * math.sin(t * 8 + phase) * smooth(.65, 1, t)
                points.append(p)
            grey = .055 if chin > .4 else (.012 if stache else .007)
            add_strand(beard_obj, points, grey=grey, width=rng.uniform(.75, 1.2))

    ev.to_mesh_clear()
    settings = {'strand_count': strand_count, 'head': ob.name,
                'mouth_z': mouth_z, 'chin_z': chin_z,
                'hairline_front_z': hairline_front_z,
                'top_calibration_offset': zoff,
                'root_shell_offset': .001, 'seed': seed,
                'source_reference': '3befb5c3-89cf-4f08-aecb-885665c8eb38.png'}
    for key, value in settings.items():
        collection[key] = value
    print('Reference groom v3:', settings)
    return {'collection': collection, 'hair': main_obj, 'undercoat': under_obj,
            'flyaways': fly_obj, 'beard': beard_obj, 'settings': settings}


if __name__ == '__main__':
    build_groom()
