#!/usr/bin/python3
"""A stored image replaced by a FIFO must fail without blocking the next image."""
from native_support import open_game, library, ROOT,report,wait,find,qt,check,db,scalar,nodes,json,os,time,uuid,restart
from PIL import Image
import errno,struct,zlib


def chunk(kind,data):
    return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data))


def png(color):
    side=160
    compressor=zlib.compressobj(6)
    row=b'\0'+bytes(color)*side
    payload=b''.join(compressor.compress(row) for _ in range(side))+compressor.flush()
    return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',side,side,8,2,0,0,0))+chunk(b'IDAT',payload)+chunk(b'IEND',b'')


def state_for(name,status=None):
    value=qt('state')
    return value if value.get('artworkSource','').endswith('/'+name) and (status is None or value.get('artworkStatus')==status) else None


def pixel(state):
    qt('screenshot')
    with Image.open(ROOT/'screenshot.png') as image:
        x=round(state['artworkCenterX']*state['dpr']);y=round(state['artworkCenterY']*state['dpr'])
        assert 0<=x<image.width and 0<=y<image.height
        return image.convert('RGB').getpixel((x,y))


target=ROOT/'target.png'
fifo_created=False
try:
    target.write_bytes(png((230,30,25)))
    safe=ROOT/'safe.png';safe.write_bytes(png((20,60,230)))
    wait(lambda:find('Search games','text'))
    with db() as connection:
        for label,image in [('Swap fixture A',target),('Swap fixture B',safe)]:
            game,installation,profile=(uuid.uuid4().bytes for _ in range(3))
            connection.execute('INSERT INTO games(id,display_name) VALUES(?,?)',(game,label))
            connection.execute('INSERT INTO installations VALUES(?,?)',(installation,game))
            connection.execute("INSERT INTO profiles(id,installation_id,executable,cwd,runner) VALUES(?,?,?,?,?)",(profile,installation,b'/usr/bin/true',os.fsencode(ROOT),'native'))
            connection.execute('INSERT INTO local_artwork VALUES(?,?)',(game,os.fsencode(image)))
            if label.endswith('A'):first=game
        connection.execute('UPDATE ui_state SET selected_game=?',(first,))
    restart();wait(lambda:find('Swap fixture A','list item'))
    wait(lambda:state_for('target.png',1),timeout=20)
    item=wait(lambda:find('Swap fixture A','list item'));assert item.get_component_iface().grab_focus()
    qt('key',sequence='Right');wait(lambda:scalar('SELECT display_name FROM games JOIN ui_state ON games.id=ui_state.selected_game')=='Swap fixture B')
    wait(lambda:state_for('safe.png',1),timeout=20)
    os.replace(target,ROOT/'target-original.png');os.mkfifo(target,0o600);fifo_created=True
    qt('key',sequence='Left')
    replaced=wait(lambda:state_for('target.png',3),timeout=5)
    report['metrics']['replaced_source_status']=replaced.get('artworkStatus')
    try:
        descriptor=os.open(target,os.O_WRONLY|os.O_NONBLOCK);os.close(descriptor)
        raise AssertionError('replaced artwork path still has a reader')
    except OSError as error:
        if error.errno!=errno.ENXIO:raise
    check('replaced-path-is-not-a-blocking-reader',True)
    qt('key',sequence='Right')
    wait(lambda:scalar('SELECT display_name FROM games JOIN ui_state ON games.id=ui_state.selected_game')=='Swap fixture B')
    started=time.monotonic()
    safe=wait(lambda:state_for('safe.png',1),timeout=5)
    report['metrics']['safe_reload_ms']=round((time.monotonic()-started)*1000,2)
    open_game('Swap fixture B')
    safe=wait(lambda:state_for('safe.png',1),timeout=5)
    color=pixel(safe)
    report['metrics']['safe_pixel']=list(color)
    check('safe-image-loads-after-replaced-path',color[2]>180 and color[0]<80)
    report['passed']=True
except Exception as error:
    report['passed']=False;report['error']=str(error);report['tree']=[]
    for node in nodes():
        try:report['tree'].append({'name':node.get_name(),'role':node.get_role_name()})
        except Exception:pass
    raise
finally:
    if fifo_created:
        try:
            descriptor=os.open(target,os.O_WRONLY|os.O_NONBLOCK)
            os.close(descriptor)
            report['metrics']['fifo_reader_released']=True
        except OSError as error:
            if error.errno!=errno.ENXIO:report['cleanup_error']=str(error)
    (ROOT/'report.json').write_text(json.dumps(report,indent=2))
