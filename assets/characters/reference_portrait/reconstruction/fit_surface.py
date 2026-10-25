"""Landmark-constrained smooth deformation; no photographic displacement."""
from pathlib import Path
import json
import numpy as np
from scipy.interpolate import RBFInterpolator
from scipy.spatial import cKDTree
P=Path(__file__).resolve().parent
mesh=json.loads((P/'mesh_v2.json').read_text())
V=np.array(mesh['verts'],float)
source=json.loads((P/'model_landmarks_3d.json').read_text())
C=np.array([p['xyz'] for p in source[:468]])
image_points=json.loads((P/'current_model_landmarks.json').read_text())['landmarks']
CXZ=np.array([[(p['x']-256)*3.26/834,2.7+(417-p['y'])*3.26/834] for p in image_points[:468]])
ref=json.loads((P/'reference_landmarks.json').read_text())['views']['photo_front']['landmarks']
depth=json.loads((P/'multiview_depth_targets.json').read_text())['targets']
print('depth keys',depth[0].keys())
T=C.copy()
sx=2*3.26/834
# Reference midpoint of the pupils defines the lateral origin. Keep eye height.
cx=(ref[468]['x']+ref[473]['x'])/2
cz=(ref[468]['y']+ref[473]['y'])/2
for i in range(468):
    T[i,0]=(ref[i]['x']-cx)*sx
    T[i,2]=2.946+(cz-ref[i]['y'])*sx
    d=depth[i]
    if d.get('allow_depth_deformation',False):
        # Use stable multi-view correction conservatively; absolute depth is unobserved.
        target_y=d.get('xyz',[])
        if target_y:T[i,1]=C[i,1]+.65*(target_y[1]-C[i,1])
oval=[10,338,297,332,284,251,389,356,454,323,361,288,397,365,379,378,400,377,152,148,176,149,150,136,172,58,132,93,234,127,162,21,54,103,67,109]
for i in oval:
    if T[i,2]<2.58:
        T[i,0]-=np.sign(T[i,0])*.018
        T[i,2]+=.025
        T[i,1]=C[i,1]
def smooth(a,b,x):
    t=np.clip((x-a)/(b-a),0,1);return t*t*(3-2*t)
def base_delta(v):
    d=np.zeros_like(v);x,y,z=v.T
    head=smooth(2.05,2.50,z)
    d[:,1]=.18*smooth(-.65,.20,y)*head
    ear=smooth(.64,.78,np.abs(x))*(1-smooth(3.15,3.4,z))*smooth(2.3,2.5,z)
    d[:,1]=np.maximum(d[:,1],.15*ear)
    # A longer neck supports the lowered jaw without collapsing the neck opening.
    d[:,2]=-.18*(1-smooth(1.60,2.20,z))
    return d
# Fit a smooth frontal coordinate field. Reusing 3D nearest points at occluding
# contours causes steep gradients; screen correspondences are the observations.
anchors=[];ad=[]
for x in np.linspace(-1.05,1.05,13):
    anchors.extend([[x,4.15],[x,3.93],[x,1.54]])
    ad.extend([[0,0],[0,0],[0,-.15]])
for z in np.linspace(1.7,3.8,18):
    anchors.extend([[-1.05,z],[1.05,z]])
    ad.extend([[0,0],[0,0]])
anchors=np.array(anchors);ad=np.array(ad)
controls=np.vstack([CXZ,anchors]);deltas=np.vstack([T[:,[0,2]]-CXZ,ad])
fit=RBFInterpolator(controls,deltas,kernel='thin_plate_spline',degree=1,smoothing=.005)
# Front/back depth is separately regularized, avoiding anatomically impossible
# spike formation from inconsistent views and eye/mouth cavity intersections.
dy=T[:,1]-C[:,1]
dy=np.clip(dy,-.035,.035)
fit_y=RBFInterpolator(np.vstack([CXZ,anchors]),np.r_[dy,np.zeros(len(anchors))],kernel='thin_plate_spline',degree=1,smoothing=.025)
def deform(v):
    result=[]
    for a in range(0,len(v),4000):
        chunk=v[a:a+4000];p=chunk[:,[0,2]];d=np.zeros_like(chunk)
        faceweight=.30+.70*(1-smooth(-.65,-.15,chunk[:,1]))
        d[:,[0,2]]=fit(p)*faceweight[:,None]
        d[:,1]=fit_y(p)*(1-smooth(-.7,-.3,chunk[:,1]))+base_delta(chunk)[:,1]
        result.append(chunk+d)
    return np.vstack(result)
W=deform(V)
cage=np.array(json.loads((P/'cage_v2.json').read_text()));newcage=deform(cage)
(P/'fitted_mesh_v3.json').write_text(json.dumps({'verts':W.tolist(),'cage':newcage.tolist()}))
pred=CXZ+fit(CXZ)
err=np.linalg.norm(pred-T[:,[0,2]],axis=1)/sx
stats={'median_constraint_error_px':float(np.median(err)),'p90_constraint_error_px':float(np.quantile(err,.9)),'max_displacement_world':float(np.max(np.linalg.norm(W-V,axis=1))),'note':'Deformation constraint residual, not independent likeness validation.'}
(P/'fit_stats.json').write_text(json.dumps(stats,indent=2))
(P/'front_target_points.json').write_text(json.dumps(T.tolist()))
print(stats)
