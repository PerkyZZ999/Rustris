#!/usr/bin/python3
"""Local artwork selection through the private desktop and native accessibility."""
from native_support import open_game,library,ROOT,SCALE,report,wait,find,visible,press,text,qt,check,db,scalar,nodes,json,Atspi,focused_name,restart,walk
import base64,os

try:
    image=ROOT/'home'/'art.png'

    import struct,zlib
    def chunk(kind,data):return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data))
    png=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',1,1,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(b'\0\xff\0\0'))+chunk(b'IEND',b'')
    image.write_bytes(png)
    wait(lambda:find('Search games','text'))
    press('Add game');text('Game name','Artwork fixture');text('Executable path','/usr/bin/true');press('Save profile');wait(lambda:find('Artwork fixture','list item'))
    for _ in range(50):
        if focused_name('Choose artwork…'):break
        qt('key',sequence='Tab')
    check('keyboard-focuses-artwork',focused_name('Choose artwork…'))
    qt('key',sequence='Space');wait(lambda:find('Parent folder','button'))
    qt('key',sequence='Escape')
    check('picker-cancel-does-not-save',scalar('SELECT count(*) FROM local_artwork')==0)
    zebra=ROOT/'home'/'zebra.png';zebra.write_bytes(png)
    for _ in range(40):
        if focused_name('Choose artwork…'):break
        qt('key',sequence='Tab')
    qt('key',sequence='Space');wait(lambda:find('Parent folder','button'))
    for _ in range(40):
        if focused_name('art.png'):break
        qt('key',sequence='Tab')
    check('picker-keyboard-reaches-file',focused_name('art.png'))
    qt('key',sequence='Tab')
    check('picker-keyboard-moves-to-next-file',focused_name('zebra.png'))
    qt('key',sequence='Shift+Tab')
    check('picker-keyboard-returns-to-previous-file',focused_name('art.png'))
    qt('key',sequence='Space')
    wait(lambda:scalar('SELECT path FROM local_artwork')==os.fsencode(image))
    check('picker-keyboard-selects-file',wait(lambda:find('Local artwork selected.','label')))
    for index in range(40):
        (ROOT/'home'/f'zz{index:02}.png').write_bytes(png)
    press('Choose artwork…');wait(lambda:find('Parent folder','button'))
    for _ in range(100):
        if focused_name('zz39.png'):break
        qt('key',sequence='Tab')
    check('picker-keyboard-reaches-offscreen-file',focused_name('zz39.png'))
    qt('key',sequence='Shift+Tab')
    check('picker-keyboard-moves-back-after-scroll',focused_name('zz38.png'))
    qt('key',sequence='Tab')
    check('picker-keyboard-returns-to-last-file',focused_name('zz39.png'))
    qt('key',sequence='Tab')
    check('picker-keyboard-leaves-list-at-end',focused_name('Cancel'))
    qt('key',sequence='Shift+Tab')
    check('picker-keyboard-enters-list-from-end',focused_name('zz39.png'))
    for _ in range(100):
        if focused_name('art.png'):break
        qt('key',sequence='Shift+Tab')
    check('picker-keyboard-reaches-first-file-in-reverse',focused_name('art.png'))
    for _ in range(100):
        if focused_name('zz39.png'):break
        qt('key',sequence='Tab')
    check('picker-keyboard-reaches-last-file-again',focused_name('zz39.png'))
    qt('key',sequence='Space')
    last_image=ROOT/'home'/'zz39.png'
    check('picker-keyboard-selects-offscreen-file',wait(lambda:scalar('SELECT path FROM local_artwork')==os.fsencode(last_image)))
    press('Choose artwork…');wait(lambda:find('Parent folder','button'))
    for _ in range(40):
        if focused_name('art.png'):break
        qt('key',sequence='Tab')
    check('picker-reopen-starts-at-first-file',focused_name('art.png'))
    qt('key',sequence='Space')
    wait(lambda:scalar('SELECT path FROM local_artwork')==os.fsencode(image))
    press('Choose artwork…');wait(lambda:find('Parent folder','button'))
    qt('key',sequence='Escape')
    check('picker-cancel-preserves-selected-file',scalar('SELECT path FROM local_artwork')==os.fsencode(image))
    nested=ROOT/'home'/'nested';nested.mkdir();(nested/'nested.png').write_bytes(png)
    press('Choose artwork…');wait(lambda:find('Parent folder','button'))
    for _ in range(20):
        if focused_name('Folder nested'):break
        qt('key',sequence='Tab')
    check('picker-keyboard-focuses-folder',focused_name('Folder nested'))
    qt('key',sequence='Space');wait(lambda:find('nested.png','button'))
    for _ in range(20):
        if focused_name('nested.png'):break
        qt('key',sequence='Tab')
    check('picker-keyboard-enters-folder',focused_name('nested.png'))
    qt('key',sequence='Escape')
    restart();check('artwork-persists-restart',wait(lambda:find('Local artwork selected.','label')))
    image.unlink();restart()
    missing=qt('state');report['metrics']['missing_artwork']=missing
    check('missing-image-placeholder',missing.get('artworkStatus')==3 and wait(lambda:find('Artwork unavailable or unreadable. The original selection is preserved.','label')))
    image.write_bytes(b'corrupt');restart();check('corrupt-image-placeholder',wait(lambda:find('Artwork unavailable or unreadable. The original selection is preserved.','label')))
    open_game('Artwork fixture')
    for _ in range(50):
        if focused_name('Clear artwork'):break
        qt('key',sequence='Tab')
    check('keyboard-focuses-clear',focused_name('Clear artwork'))
    qt('key',sequence='Space');wait(lambda:scalar('SELECT count(*) FROM local_artwork')==0)
    check('clear-preserves-external-image',image.read_bytes()==b'corrupt')
    with db() as connection:
        game=connection.execute('SELECT id FROM games').fetchone()[0]
        connection.execute('INSERT INTO local_artwork(game_id,path) VALUES(?,?)',(game,b'/missing/art_\xff.png'))
    restart()
    check('non-utf8-reference-is-preserved-with-placeholder',wait(lambda:find('Artwork unavailable or unreadable. The original selection is preserved.','label')) and scalar('SELECT path FROM local_artwork')==b'/missing/art_\xff.png')
    open_game('Artwork fixture')
    for _ in range(50):
        if focused_name('Enter artwork path…'):break
        qt('key',sequence='Tab')
    check('keyboard-focuses-path-entry',focused_name('Enter artwork path…'))
    qt('key',sequence='Space');wait(lambda:find('Artwork path','text'))
    check('path-form-initial-focus',wait(lambda:focused_name('Artwork path')))
    qt('key',sequence='X');qt('close');wait(lambda:visible('Keep editing','button'))
    press('Keep editing');check('dirty-path-close-cancel-preserves-form',visible('Save artwork path','button'))
    text('Artwork path','relative.png');press('Save artwork path')
    check('relative-path-keeps-form-open',wait(lambda:find('Choose an absolute local artwork path.','alert')) and visible('Save artwork path','button'))
    text('Artwork path',r'/missing/new_\xff.png');press('Artwork path uses exact Unix bytes','check box')
    for _ in range(10):
        if focused_name('Save artwork path'):break
        qt('key',sequence='Tab')
    check('keyboard-focuses-path-save',focused_name('Save artwork path'))
    qt('key',sequence='Space');wait(lambda:scalar('SELECT path FROM local_artwork')==b'/missing/new_\xff.png')
    check('exact-byte-path-saved-and-form-closed',not visible('Save artwork path','button'))
    check('path-close-returns-focus-to-opener',wait(lambda:focused_name('Enter artwork path…')))
    image.write_bytes(png)
    for _ in range(50):
        if focused_name('Enter artwork path…'):break
        qt('key',sequence='Tab')
    assert focused_name('Enter artwork path…')
    qt('key',sequence='Space');wait(lambda:focused_name('Artwork path'))
    qt('type-ascii',text=str(image))
    assert Atspi.Text.get_text(find('Artwork path','text'),0,-1)==str(image)
    for _ in range(10):
        if focused_name('Save artwork path'):break
        qt('key',sequence='Tab')
    assert focused_name('Save artwork path')
    qt('key',sequence='Space')
    wait(lambda:scalar('SELECT path FROM local_artwork')==os.fsencode(image))
    check('keyboard-only-artwork-path-selection',wait(lambda:find('Local artwork selected.','label')))
    report['passed']=True
except Exception as error:
    report['passed']=False;report['error']=str(error);report['tree']=[]
    for node in nodes():
        try:report['tree'].append({'name':node.get_name(),'role':node.get_role_name()})
        except Exception:pass
    raise
finally:
    (ROOT/'report.json').write_text(json.dumps(report,indent=2))
