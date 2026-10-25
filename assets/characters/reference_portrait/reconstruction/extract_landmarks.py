"""Extract reference geometry anchors using official MediaPipe Face Landmarker.

This produces estimates, not a metrically accurate scan. Full profiles and
beard-covered jawlines must be checked manually against the source sheet.
"""
from pathlib import Path
import json
import numpy as np
from PIL import Image, ImageDraw, ImageFont
import mediapipe as mp

ROOT = Path(__file__).resolve().parent
SOURCE = Path('/home/never0n/Pictures/3befb5c3-89cf-4f08-aecb-885665c8eb38.png')
sheet = Image.open(SOURCE).convert('RGB')
cols = [0, 256, 511, 772, 1032, 1287, 1536]
rows = [(0, 416, 'photo'), (418, 804, 'clay')]
names = ['front', 'three_quarter_left', 'left', 'back', 'right', 'three_quarter_right']
key_ids = [1,4,5,6,9,10,13,14,17,19,33,37,39,46,52,61,67,70,78,93,94,98,105,107,127,133,144,145,148,149,150,151,152,153,154,155,157,158,159,160,161,162,163,168,172,176,191,195,197,234,263,267,269,276,282,291,297,300,308,323,327,334,336,356,362,373,374,377,378,379,380,381,382,384,385,386,387,388,389,390,397,400,411,454,468,473]
options = mp.tasks.vision.FaceLandmarkerOptions(
    base_options=mp.tasks.BaseOptions(model_asset_path=str(ROOT/'face_landmarker.task')),
    running_mode=mp.tasks.vision.RunningMode.IMAGE,
    num_faces=1,
    min_face_detection_confidence=0.25,
    min_face_presence_confidence=0.25,
    output_facial_transformation_matrixes=True,
)
out = {'source':str(SOURCE), 'source_size':sheet.size, 'model':'Official MediaPipe Face Landmarker float16 latest (478 estimates)', 'views':{}}
with mp.tasks.vision.FaceLandmarker.create_from_options(options) as detector:
    for top,bottom,kind in rows:
        for i,name in enumerate(names):
            if name=='back': continue
            box=(cols[i],top,cols[i+1],bottom)
            crop=sheet.crop(box)
            arr=np.asarray(crop.resize((crop.width*2,crop.height*2)))
            result=detector.detect(mp.Image(image_format=mp.ImageFormat.SRGB,data=arr))
            view={'crop_box':box,'crop_size':crop.size,'landmarks':[]}
            if result.face_landmarks:
                view['landmarks']=[{'id':j,'x':p.x*crop.width,'y':p.y*crop.height,'z':p.z*crop.width,'sheet_x':p.x*crop.width+box[0],'sheet_y':p.y*crop.height+box[1]} for j,p in enumerate(result.face_landmarks[0])]
                view['transformation']=result.facial_transformation_matrixes[0].tolist()
                ann=crop.resize((crop.width*3,crop.height*3))
                d=ImageDraw.Draw(ann)
                for p in view['landmarks']:
                    x,y=p['x']*3,p['y']*3
                    d.ellipse((x-1,y-1,x+1,y+1),fill=(50,250,190))
                    if p['id'] in key_ids:
                        d.ellipse((x-2,y-2,x+2,y+2),fill=(255,60,30))
                        d.text((x+3,y-6),str(p['id']),fill=(255,235,40),stroke_width=1,stroke_fill=(0,0,0))
                ann.save(ROOT/f'landmarks_{kind}_{name}.png')
            out['views'][f'{kind}_{name}']=view
            print(kind,name,len(view['landmarks']),flush=True)
(ROOT/'reference_landmarks.json').write_text(json.dumps(out,indent=2))
print('Saved',ROOT/'reference_landmarks.json',flush=True)
