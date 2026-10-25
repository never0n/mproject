"""Regularized, approximate weak-perspective multiview depth fitting.

The sheet is uncalibrated and may be visually synthesized. This deliberately
preserves a depth prior and reports sensitivity instead of claiming a scan.
"""
from pathlib import Path
import json
import numpy as np
from scipy.optimize import least_squares
from scipy.spatial.transform import Rotation
from scipy.sparse import lil_matrix

ROOT=Path(__file__).resolve().parent
ref=json.loads((ROOT/'reference_landmarks.json').read_text())
cur=json.loads((ROOT/'model_landmarks_3d.json').read_text())
N=468
front=np.array([[p['x'],p['y']] for p in ref['views']['photo_front']['landmarks'][:N]])
scale=2*3.26/834
X=(front[:,0]-128)*scale
Z=2.7+(208.5-front[:,1])*scale
current=np.array([p['xyz'] for p in cur[:N]])
canonical=[];faces=[]
for line in (ROOT/'canonical_face_model.obj').read_text().splitlines():
 if line.startswith('v '):canonical.append([float(v) for v in line.split()[1:4]])
 if line.startswith('f '):faces.append([int(v.split('/')[0])-1 for v in line.split()[1:]])
canonical=np.array(canonical)
mirror=np.argmin(np.sum((canonical[:,None,:]-canonical[None,:,:]*np.array([-1,1,1]))**2,axis=2),axis=1)
pairs=[(i,int(j)) for i,j in enumerate(mirror) if j>i]
edges=set()
for face in faces:
 for a,b in zip(face,face[1:]+face[:1]):edges.add(tuple(sorted((a,b))))
neighbors=[[] for i in range(N)]
for a,b in edges:neighbors[a].append(b);neighbors[b].append(a)
credible=np.array([p['mode']=='ray' for p in cur[:N]]) & (np.abs(X)<.50) & (Z>2.2)
A=np.column_stack([-canonical[:,2],np.ones(N)])
fit=least_squares(lambda a:(A@a-current[:,1])[credible],np.array([.1,-.5]),loss='soft_l1',f_scale=.05)
canonical_y=A@fit.x
prior=np.where(credible,.75*current[:,1]+.25*canonical_y,canonical_y)
for i,j in pairs:prior[i]=prior[j]=(prior[i]+prior[j])/2
names=['photo_three_quarter_left','photo_three_quarter_right']
R0=np.array(ref['views']['photo_front']['transformation'])[:3,:3]
rots=[np.array(ref['views'][k]['transformation'])[:3,:3]@R0.T for k in names]
observed=np.array([[[p['x'],p['y']] for p in ref['views'][k]['landmarks'][:N]] for k in names])
# Exclude facial silhouette: geometric contour points change identity with view.
faceoval={10,338,297,332,284,251,389,356,454,323,361,288,397,365,379,378,400,377,152,148,176,149,150,136,172,58,132,93,234,127,162,21,54,103,67,109}
weights=np.ones((2,N))
for i in range(N):
 if i in faceoval:weights[:,i]=.05
 if Z[i]<2.15:weights[:,i]*=.25
 if abs(X[i])>.52:weights[:,i]*=.25
 # Far sides in the 3/4 views are partly hallucinated by the detector.
 if X[i]<-.25:weights[0,i]*=.55
 if X[i]>.25:weights[1,i]*=.55
camera0=[]
for v,R in enumerate(rots):
 P=np.column_stack([X,Z-2.7,-prior])@R.T
 P=P[:,:2]*np.array([1,-1])
 ww=weights[v]
 pp=(P*ww[:,None]).sum(0)/ww.sum();qq=(observed[v]*ww[:,None]).sum(0)/ww.sum()
 s=np.sum(ww[:,None]*(P-pp)*(observed[v]-qq))/np.sum(ww[:,None]*(P-pp)**2)
 camera0.extend([0,0,0,np.log(s),*(qq-s*pp)])
camera0=np.array(camera0)

def projection(y,camera,v):
 R=Rotation.from_rotvec(camera[:3]).as_matrix()@rots[v]
 xyz=np.column_stack([X,Z-2.7,-y])
 uv=(xyz@R.T)[:,:2]*np.array([1,-1])
 return np.exp(camera[3])*uv+camera[4:6]

def solve(depth_sigma):
 def residual(p):
  y=p[:N];c=p[N:].reshape(2,6);dy=y-prior
  out=[((projection(y,c[v],v)-observed[v])*np.sqrt(weights[v])[:,None]/2).ravel() for v in range(2)]
  out.append(dy/depth_sigma)
  out.append(np.array([(y[i]-y[j])/.025 for i,j in pairs]))
  out.append(np.array([(dy[i]-np.mean(dy[n]))/.055 for i,n in enumerate(neighbors)]))
  out.append((c[:,:3]/np.deg2rad([5,5,4])).ravel())
  out.append(((c[:,3]-camera0.reshape(2,6)[:,3])/.07).ravel())
  # Gauge: global Y depth is unknowable; keep a stable inner-eye/brow average.
  out.append(np.array([np.mean(dy[[33,133,263,362,9,168]])/.003]))
  return np.concatenate(out)
 p0=np.r_[prior,camera0]
 M=len(residual(p0));sparsity=lil_matrix((M,len(p0)),dtype=int)
 for v in range(2):
  for i in range(N):
   sparsity[v*2*N+2*i:v*2*N+2*i+2,i]=1
   sparsity[v*2*N+2*i:v*2*N+2*i+2,N+v*6:N+(v+1)*6]=1
 row=4*N
 for i in range(N):sparsity[row+i,i]=1
 row+=N
 for n,(i,j) in enumerate(pairs):sparsity[row+n,i]=1;sparsity[row+n,j]=1
 row+=len(pairs)
 for i,ns in enumerate(neighbors):sparsity[row+i,[i,*ns]]=1
 row+=N
 sparsity[row:row+8,N:]=1;row+=8
 sparsity[row,[33,133,263,362,9,168]]=1
 fit=least_squares(residual,p0,jac_sparsity=sparsity.tocsr(),loss='soft_l1',f_scale=1.5,max_nfev=250,ftol=1e-7)
 y=fit.x[:N];cams=fit.x[N:].reshape(2,6)
 raw=[np.linalg.norm(projection(y,cams[v],v)-observed[v],axis=1) for v in range(2)]
 return y,cams,np.array(raw),{'cost':fit.cost,'nfev':fit.nfev,'success':fit.success}

results=[solve(s) for s in [.05,.10,.20]]
y,cams,error,meta=results[1]
sensitivity=np.max(np.array([r[0] for r in results]),axis=0)-np.min(np.array([r[0] for r in results]),axis=0)
out={'method':'regularized weak-perspective fitting, approximate only','warning':'noncalibrated sheet; depths depend on prior; silhouettes/beard are not skin measurements','world_from_front':{'x':'(u-128)*2*3.26/834','z':'2.7+(208.5-v)*2*3.26/834'},'prior_canonical_affine':fit.x.tolist(),'solver':meta,'cameras':[],'targets':[]}
for v,c in enumerate(cams):
 out['cameras'].append({'name':names[v],'scale_pixels_per_unit':float(np.exp(c[3])),'delta_rotation_deg':np.rad2deg(c[:3]).tolist(),'translation':c[4:].tolist(),'median_reprojection_error_px':float(np.median(error[v])),'p90_reprojection_error_px':float(np.percentile(error[v],90))})
for i in range(N):
 confidence=float(np.clip(np.exp(-sensitivity[i]/.06)*np.exp(-np.mean(error[:,i])/6)*np.mean(weights[:,i]),0,1))
 out['targets'].append({'id':i,'xyz':[float(X[i]),float(y[i]),float(Z[i])],'current_xyz':current[i].tolist(),'delta_y':float(y[i]-current[i,1]),'prior_y':float(prior[i]),'sensitivity_y':float(sensitivity[i]),'reprojection_error_px':error[:,i].tolist(),'confidence':confidence,'allow_depth_deformation':bool(confidence>.3 and i not in faceoval and Z[i]>2.15)})
(ROOT/'multiview_depth_targets.json').write_text(json.dumps(out,indent=2))
print(json.dumps({k:out[k] for k in ['solver','cameras']},indent=2))
for i in [4,1,6,168,9,195,98,327,33,133,263,362,13,14,17,61,291,152]:
 t=out['targets'][i];print(i,'Y',round(t['xyz'][1],4),'dy',round(t['delta_y'],4),'sens',round(t['sensitivity_y'],4),'conf',round(t['confidence'],3),'resid',np.round(t['reprojection_error_px'],2))
