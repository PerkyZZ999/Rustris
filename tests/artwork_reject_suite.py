#!/usr/bin/python3
"""Truncated and wrong-type artwork stays a placeholder without blocking the library."""
from native_support import open_game, library, ROOT,report,wait,find,press,qt,check,db,scalar,nodes,json,os,uuid,restart
import struct,zlib


def chunk(kind,data):
    return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data))


def png():
    return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',1,1,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(b'\0\xff\0\0'))+chunk(b'IEND',b'')


def artwork_path(name):
    with db() as connection:
        return connection.execute('SELECT local_artwork.path FROM local_artwork JOIN games ON games.id=local_artwork.game_id WHERE games.display_name=?',(name,)).fetchone()[0]


def rejected(name,source_name):
    state=wait(lambda:(value if (value:=qt('state')).get('artworkSource','').endswith('/'+source_name) and value.get('artworkStatus')==3 else None),timeout=15)
    check(name+'-placeholder',find('Artwork unavailable or unreadable. The original selection is preserved.','label') is not None and state.get('artworkStatus')==3)
    return state


try:
    directory=ROOT/'rejected';directory.mkdir()
    truncated=directory/'truncated.png';truncated.write_bytes(png()[:24])
    text=directory/'notes.txt';text.write_bytes(b'hello artwork\n')
    pdf=directory/'poster.pdf';pdf.write_bytes(b'%PDF-1.7\nnot an image\n')
    cases=(('Artwork pdf',pdf),('Artwork text',text),('Artwork truncated',truncated))
    wait(lambda:find('Search games','text'))
    with db() as connection:
        first=None
        for label,image in cases:
            game,installation,profile=(uuid.uuid4().bytes for _ in range(3))
            if first is None:first=game
            connection.execute('INSERT INTO games(id,display_name) VALUES(?,?)',(game,label))
            connection.execute('INSERT INTO installations VALUES(?,?)',(installation,game))
            connection.execute("INSERT INTO profiles(id,installation_id,executable,cwd,runner) VALUES(?,?,?,?,?)",(profile,installation,b'/usr/bin/true',os.fsencode(ROOT),'native'))
            connection.execute('INSERT INTO local_artwork VALUES(?,?)',(game,os.fsencode(image)))
        connection.execute('UPDATE ui_state SET selected_game=?',(first,))
    restart();wait(lambda:find('Artwork pdf','list item'))
    rows=[node.get_name() for node in nodes() if node.get_role_name()=='list item']
    check('three-games-follow-name-order',rows==['Artwork pdf','Artwork text','Artwork truncated'])
    stored={label:artwork_path(label) for label,_ in cases}
    rejected('wrong-type-pdf','poster.pdf')
    item=wait(lambda:find('Artwork pdf','list item'));assert item.get_component_iface().grab_focus()
    qt('key',sequence='Right');wait(lambda:scalar('SELECT display_name FROM games JOIN ui_state ON games.id=ui_state.selected_game')=='Artwork text')
    rejected('wrong-type-text','notes.txt')
    item=wait(lambda:find('Artwork text','list item'));assert item.get_component_iface().grab_focus()
    qt('key',sequence='Right');wait(lambda:scalar('SELECT display_name FROM games JOIN ui_state ON games.id=ui_state.selected_game')=='Artwork truncated')
    rejected('truncated-png','truncated.png')
    check('rejected-paths-unchanged',all(artwork_path(label)==stored[label] for label,_ in cases))
    press('Settings');wait(lambda:find('Opaque panels','check box'));press('Done')
    check('library-usable-after-rejected-artwork',find('Search games','text') is not None and scalar('SELECT count(*) FROM games')==3)
    report['passed']=True
except Exception as error:
    report['passed']=False;report['error']=str(error);report['tree']=[]
    for node in nodes():
        try:report['tree'].append({'name':node.get_name(),'role':node.get_role_name()})
        except Exception:pass
    raise
finally:(ROOT/'report.json').write_text(json.dumps(report,indent=2))
