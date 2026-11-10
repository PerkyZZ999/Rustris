#!/usr/bin/python3
"""Observe a high-dimension compressed PNG in a private desktop."""
from native_support import ROOT,report,wait,find,press,qt,check,db,scalar,nodes,json,time,restart,children
from pathlib import Path
import os,struct,zlib

def chunk(kind,data):return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data))
def solid(side,color):
    compressor=zlib.compressobj(6)
    row=b'\0'+bytes(color)*side
    payload=b''.join(compressor.compress(row) for _ in range(side))+compressor.flush()
    return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',side,side,8,2,0,0,0))+chunk(b'IDAT',payload)+chunk(b'IEND',b'')
try:
    (ROOT/'bounded.png').write_bytes(solid(2000,(20,60,230)))
    wait(lambda:find('Search games','text'))
    capped=qt('decode-artwork',path='bounded.png',width=4000,height=4000)
    report['metrics']['capped']=capped
    check('decoded-output-stays-within-edge',0<capped['decodedWidth']<=256 and 0<capped['decodedHeight']<=256 and capped['decodedBytes']<=256*256*4)
    thumb=qt('decode-artwork',path='bounded.png',width=144,height=180)
    check('decoded-thumbnail-follows-request',0<thumb['decodedWidth']<=144 and 0<thumb['decodedHeight']<=180)
    cancelled=qt('cancel-artwork',path='bounded.png')
    check('cancelled-response-finishes-without-texture',cancelled['cancelFinished'] and not cancelled['cancelHasTexture'])
    cancelled=qt('cancel-decoded-artwork',path='bounded.png')
    check('cancelled-decoded-response-finishes-without-texture',cancelled['cancelFinished'] and not cancelled['cancelHasTexture'])
    side=9000;row=b'\0'+bytes(side*3);compressor=zlib.compressobj(6)
    parts=[]
    for _ in range(side):parts.append(compressor.compress(row))
    parts.append(compressor.flush())
    image=ROOT/'large-dimensions.png'
    image.write_bytes(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',side,side,8,2,0,0,0))+chunk(b'IDAT',b''.join(parts))+chunk(b'IEND',b''))
    report['metrics']['encoded_bytes']=image.stat().st_size
    check('compressed-fixture-under-input-limit',image.stat().st_size<32*1024*1024)
    wait(lambda:find('Search games','text'));press('Add game')
    from native_support import text
    text('Game name','Large image fixture');text('Executable path','/usr/bin/true');press('Save profile');wait(lambda:scalar('SELECT count(*) FROM games')==1)
    with db() as connection:
        game=connection.execute('SELECT id FROM games').fetchone()[0]
        connection.execute('INSERT INTO local_artwork VALUES(?,?)',(game,os.fsencode(image)))
    restart();wait(lambda:find('Large image fixture','list item'))
    latency=[];end=time.monotonic()+30;observed=None
    while time.monotonic()<end:
        started=time.monotonic();value=qt('state');latency.append((time.monotonic()-started)*1000)
        if value.get('artworkSource','').endswith('/large-dimensions.png') and value.get('artworkStatus') in (1,3):observed=value;break
    assert observed is not None,'Image did not settle within the observation period'
    report['metrics']['image_status']='ready' if observed['artworkStatus']==1 else 'error'
    check('allocation-limit-rejects-large-image',observed['artworkStatus']==3 and 'exceeds the current allocation limit of 64 megabytes' in (ROOT/'restart-0.log').read_text(errors='replace'))
    report['metrics']['max_qt_roundtrip_ms']=round(max(latency),2)
    status=Path(f'/proc/{children[-1].pid}/status').read_text()
    report['metrics']['rss_kib']=int(next(line.split()[1] for line in status.splitlines() if line.startswith('VmRSS:')))
    press('Settings');wait(lambda:find('Opaque panels','check box'));press('Done')
    check('large-image-settles-and-controls-respond',True)
    report['passed']=True
except Exception as error:
    report['passed']=False;report['error']=str(error);report['tree']=[]
    for node in nodes():
        try:report['tree'].append({'name':node.get_name(),'role':node.get_role_name()})
        except Exception:pass
    raise
finally:(ROOT/'report.json').write_text(json.dumps(report,indent=2))
