import re, sys
from PIL import Image, ImageDraw
col = {'g':(96,150,60),'d':(140,110,70),'a':(210,190,130),'s':(225,210,150),'w':(90,150,200),'W':(30,70,140),'f':(30,80,30),'n':(240,240,240)}
own = {0:(40,90,255),1:(230,40,40),2:(250,150,0),3:(160,40,200),255:(250,250,0)}
def draw(path, out, scale=6):
    t = open(path).read()
    rows = re.findall(r'^\s+"([gdaswWfn]+)",$', t, re.M)
    if not rows: return
    n = len(rows[0]); rows = [r for r in rows if len(r)==n][:n]
    im = Image.new('RGB', (n*scale, n*scale)); d = ImageDraw.Draw(im)
    for y,r in enumerate(rows):
        for x,c in enumerate(r):
            d.rectangle([x*scale,y*scale,x*scale+scale-1,y*scale+scale-1], fill=col[c])
    for m in re.finditer(r'\(kind: "([^"]+)", owner: (\d+), at: \((-?\d+), (-?\d+)\)', t):
        k,o,x,y = m.group(1), int(m.group(2)), int(m.group(3)), int(m.group(4))
        c = own.get(o,(255,255,255))
        d.rectangle([x*scale,y*scale,x*scale+scale*2-1,y*scale+scale*2-1], outline=(0,0,0), fill=c)
    for m in re.finditer(r'from: \((\d+), (\d+)\), to: \((\d+), (\d+)\)', t):
        x0,y0,x1,y1 = map(int, m.groups())
        d.rectangle([x0*scale,y0*scale,(x1+1)*scale,(y1+1)*scale], outline=(255,0,255))
    im.save(out)
for p in sys.argv[2:]:
    name = p.rsplit('/',1)[1].replace('.ron','')
    draw(p, f"{sys.argv[1]}/top-{name}.png", 5 if 'marathon' in p or 'plataea' in p else 6)
