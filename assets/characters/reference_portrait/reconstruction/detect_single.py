"""Run MediaPipe on one rendering; used for objective before/after anchors."""
import argparse
from pathlib import Path
import json
import mediapipe as mp
import numpy as np
from PIL import Image,ImageDraw

parser=argparse.ArgumentParser()
parser.add_argument('image')
parser.add_argument('--out',default='current_model_landmarks')
a=parser.parse_args()
root=Path(__file__).resolve().parent
im=Image.open(a.image).convert('RGB')
options=mp.tasks.vision.FaceLandmarkerOptions(
 base_options=mp.tasks.BaseOptions(model_asset_path=str(root/'face_landmarker.task')),
 running_mode=mp.tasks.vision.RunningMode.IMAGE,
 num_faces=1,min_face_detection_confidence=.2,min_face_presence_confidence=.2,
 output_facial_transformation_matrixes=True)
with mp.tasks.vision.FaceLandmarker.create_from_options(options) as detector:
 result=detector.detect(mp.Image(image_format=mp.ImageFormat.SRGB,data=np.asarray(im)))
out={'source':str(Path(a.image).resolve()),'image_size':im.size,'landmarks':[]}
if result.face_landmarks:
 out['landmarks']=[{'id':i,'x':p.x*im.width,'y':p.y*im.height,'z':p.z*im.width} for i,p in enumerate(result.face_landmarks[0])]
 out['transformation']=result.facial_transformation_matrixes[0].tolist()
 draw=ImageDraw.Draw(im)
 ids=[1,4,6,9,10,13,14,17,33,61,93,98,127,133,152,159,168,172,195,234,263,291,323,327,356,362,386,397,454,468,473]
 for p in out['landmarks']:
  x,y=p['x'],p['y'];draw.ellipse((x-1,y-1,x+1,y+1),fill=(50,250,190))
  if p['id'] in ids:
   draw.ellipse((x-2,y-2,x+2,y+2),fill=(255,60,30));draw.text((x+3,y-6),str(p['id']),fill=(255,235,40),stroke_width=1,stroke_fill=(0,0,0))
 im.save(root/f'{a.out}.png')
(root/f'{a.out}.json').write_text(json.dumps(out,indent=2))
print('Detected',len(out['landmarks']),'landmarks in',a.image)
