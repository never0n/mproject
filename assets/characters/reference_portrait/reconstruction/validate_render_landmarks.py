"""Compare independently detected rendered landmarks with source landmarks."""
from pathlib import Path
import json
import numpy as np

ROOT=Path(__file__).resolve().parent
ref=json.loads((ROOT/'reference_landmarks.json').read_text())['views']['photo_front']['landmarks']
before=json.loads((ROOT/'current_model_landmarks.json').read_text())['landmarks']
after=json.loads((ROOT/'fitted_model_landmarks.json').read_text())['landmarks']
P=lambda points:np.array([[p['x'],p['y']] for p in points])
R,B,A=map(P,[ref,before,after])
left_eye=[33,7,163,144,145,153,154,155,133,173,157,158,159,160,161,246]
right_eye=[263,249,390,373,374,380,381,382,362,398,384,385,386,387,388,466]
lips=[61,146,91,181,84,17,314,405,321,375,291,308,324,318,402,317,14,87,178,88,95,78,191,80,81,82,13,312,311,310,415,185,40,39,37,0,267,269,270,409]
nose=[1,2,4,5,6,19,94,97,98,99,115,129,168,195,196,197,209,218,219,220,235,275,278,294,305,326,327,328,344,354,419,420,429,438,439,440,455]
groups={'eye_contours':left_eye+right_eye,'nose':nose,'mouth':lips}
core=sorted(set(sum(groups.values(),[])))
groups={'core_face':core,**groups}
mean_a=A[core].mean(axis=0);mean_r=R[core].mean(axis=0)
aa=A[core]-mean_a;rr=R[core]-mean_r
scale=float(np.sum(aa*rr)/np.sum(aa*aa));translation=mean_r-scale*mean_a
mapped=[B*scale+translation,A*scale+translation]
result={'method':'MediaPipe detected independently on clay renders; after-render core aligned to reference with uniform scale and translation only; identical transform then applied to before-render','reference':'photo_front','reference_image_size':[256,416],'alignment':{'scale':scale,'translation':translation.tolist(),'fitted_to':'after render','indices':core},'groups':{},'per_landmark':[]}
errors=[np.linalg.norm(x-R,axis=1) for x in mapped]
for name,ids in groups.items():
 vals={tag:{'mean_source_px':float(np.mean(e[ids])),'median_source_px':float(np.median(e[ids])),'p90_source_px':float(np.percentile(e[ids],90))} for tag,e in zip(['before','after'],errors)}
 vals['indices']=ids
 result['groups'][name]=vals
 print(name,vals)
for i in core:result['per_landmark'].append({'id':i,'reference_xy':R[i].tolist(),'before_xy':mapped[0][i].tolist(),'after_xy':mapped[1][i].tolist(),'before_error_px':float(errors[0][i]),'after_error_px':float(errors[1][i])})
(ROOT/'render_landmark_validation.json').write_text(json.dumps(result,indent=2))
print('alignment',result['alignment'])
