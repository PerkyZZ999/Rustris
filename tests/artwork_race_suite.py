#!/usr/bin/python3
"""Observe rapid selection across larger images in a private desktop."""
from native_support import open_game,library,ROOT,Atspi,report,wait,find,visible,qt,check,db,scalar,nodes,json,os,time,uuid,restart,children
from pathlib import Path
from PIL import Image
import struct,threading,zlib


def chunk(kind,data):
    return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data))


def solid_png(side,color):
    compressor=zlib.compressobj(6)
    row=b'\0'+bytes(color)*side
    parts=[compressor.compress(row) for _ in range(side)]
    parts.append(compressor.flush())
    return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',side,side,8,2,0,0,0))+chunk(b'IDAT',b''.join(parts))+chunk(b'IEND',b'')


def counters(pid):
    values={}
    for line in Path(f'/proc/{pid}/status').read_text().splitlines():
        if line.startswith(('VmRSS:','VmHWM:','Threads:')):
            key,value,*_=line.split();values[key.rstrip(':')]=int(value)
    return values


def pixel(state):
    qt('screenshot')
    with Image.open(ROOT/'screenshot.png') as image:
        x=round(state['artworkCenterX']*state['dpr'])
        y=round(state['artworkCenterY']*state['dpr'])
        assert 0<=x<image.width and 0<=y<image.height
        return image.convert('RGB').getpixel((x,y))

stop=threading.Event()
samples=[]
thread=None
try:
    directory=ROOT/'race-images';directory.mkdir()
    red=solid_png(3000,(230,30,25));blue=solid_png(3000,(20,60,230))
    assert max(len(red),len(blue))<32*1024*1024
    wait(lambda:find('Search games','text'))
    with db() as connection:
        for number in range(16):
            image=directory/f'{number:04}.png';image.write_bytes(red if number==0 else blue)
            game,installation,profile=(uuid.uuid4().bytes for _ in range(3))
            connection.execute('INSERT INTO games(id,display_name) VALUES(?,?)',(game,f'Race fixture {number:04}'))
            connection.execute('INSERT INTO installations VALUES(?,?)',(installation,game))
            connection.execute("INSERT INTO profiles(id,installation_id,executable,cwd,runner) VALUES(?,?,?,?,?)",(profile,installation,b'/usr/bin/true',os.fsencode(ROOT),'native'))
            connection.execute('INSERT INTO local_artwork VALUES(?,?)',(game,os.fsencode(image)))
        connection.execute('UPDATE ui_state SET selected_game=?',(game,))
    restart();wait(lambda:find('Race fixture 0015','list item'))
    blue_state=wait(lambda:(state if (state:=qt('state')).get('artworkSource','').endswith('/0015.png') and state.get('artworkStatus')==1 else None),timeout=30)
    open_game('Race fixture 0015')
    blue_state=wait(lambda:(state if (state:=qt('state')).get('artworkStatus')==1 else None),timeout=30)
    initial=pixel(blue_state);check('initial-selected-image-pixels',initial[2]>180 and initial[0]<80)
    library()
    item=wait(lambda:find('Race fixture 0015','list item'));assert item.get_component_iface().grab_focus()
    pid=children[-1].pid
    baseline=counters(pid)
    def sample():
        while not stop.wait(.01):
            try:samples.append(counters(pid))
            except (FileNotFoundError,ProcessLookupError):break
    thread=threading.Thread(target=sample);thread.start()
    observed=set()
    states=[]
    previous=None
    superseded_loading=0
    began=time.monotonic()
    for _ in range(15):
        qt('key',sequence='Left')  # one entry back in the grid
        current=qt('state')
        source=current.get('artworkSource','')
        states.append((Path(source).name,current.get('artworkStatus')))
        observed.add(source)
        if previous and previous.get('artworkStatus')==2 and source!=previous.get('artworkSource'):
            superseded_loading+=1
        previous=current
    wait(lambda:scalar('SELECT display_name FROM games JOIN ui_state ON games.id=ui_state.selected_game')=='Race fixture 0000',timeout=30)
    final=wait(lambda:(state if (state:=qt('state')).get('artworkSource','').endswith('/0000.png') and state.get('artworkStatus')==1 else None),timeout=30)
    open_game('Race fixture 0000')
    final=wait(lambda:(state if (state:=qt('state')).get('artworkStatus')==1 else None),timeout=30)
    check('rapid-selection-observed-multiple-sources',len(observed-{''})>1)
    report['metrics']['observed_states']=states
    if superseded_loading:
        check('observed-loading-image-superseded',True)
    color=pixel(final);check('final-selected-image-pixels',color[0]>180 and color[2]<80)
    time.sleep(1)
    settled=qt('state');later=pixel(settled)
    check('final-image-remains-correct-after-one-second',settled.get('artworkSource','').endswith('/0000.png') and settled.get('artworkStatus')==1 and later[0]>180 and later[2]<80)
    check('desktop-remains-responsive',qt('state')['width']>0 and visible('Settings','button') is not None)
    stop.set();thread.join(timeout=5);assert not thread.is_alive()
    report['metrics'].update(image_dimensions='3000x3000 RGB',encoded_bytes={'red':len(red),'blue':len(blue)},baseline=baseline,peak_sampled_rss_kib=max([baseline.get('VmRSS',0)]+[s.get('VmRSS',0) for s in samples]),peak_sampled_threads=max([baseline.get('Threads',0)]+[s.get('Threads',0) for s in samples]),sample_count=len(samples),distinct_sources=len(observed-{''}),superseded_loading=superseded_loading,elapsed_ms=round((time.monotonic()-began)*1000,2),initial_pixel=initial,final_pixel=color,later_pixel=later)
    report['passed']=True
except Exception as error:
    report['passed']=False;report['error']=str(error);report['tree']=[]
    for node in nodes():
        try:report['tree'].append({'name':node.get_name(),'role':node.get_role_name()})
        except Exception:pass
    raise
finally:
    stop.set()
    if thread is not None:thread.join(timeout=5)
    (ROOT/'report.json').write_text(json.dumps(report,indent=2))
