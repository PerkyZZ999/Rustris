#!/usr/bin/python3
"""Private 1,000-image library load and selection responsiveness fixture."""
from native_support import open_game,library,ROOT,Atspi,report,wait,find,visible,press,text,qt,check,db,scalar,nodes,json,os,time,uuid,restart,children,focused_name
import struct,zlib
from pathlib import Path

def chunk(kind,data):return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data))
def png(n):return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',1,1,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(bytes([0,n%256,(n//4)%256,128])))+chunk(b'IEND',b'')
try:
    wait(lambda:find('Search games','text'))
    directory=ROOT/'images';directory.mkdir()
    with db() as connection:
        for number in range(1000):
            image=directory/f'{number:04}.png';image.write_bytes(png(number))
            game,installation,profile=(uuid.uuid4().bytes for _ in range(3))
            connection.execute('INSERT INTO games(id,display_name) VALUES(?,?)',(game,f'Art fixture {number:04}'))
            connection.execute('INSERT INTO installations VALUES(?,?)',(installation,game))
            connection.execute("INSERT INTO profiles(id,installation_id,executable,cwd,runner) VALUES(?,?,?,?,?)",(profile,installation,b'/usr/bin/true',os.fsencode(ROOT),'native'))
            connection.execute('INSERT INTO local_artwork VALUES(?,?)',(game,os.fsencode(image)))
        connection.execute('UPDATE ui_state SET selected_game=?',(game,))
    restart();wait(lambda:find('Art fixture 0999','list item'))
    check('restored-selection-visible-in-list',wait(lambda:visible('Art fixture 0999','list item'),timeout=5) is not None)
    qt('screenshot')
    check('selected-image-visible',wait(lambda:find('Local artwork selected.','label')))
    started=time.monotonic();text('Search games','0999');wait(lambda:(find('Showing 1 of 1,000') or find('Showing 1 of 1000')))
    report['metrics']['filter_ms']=round((time.monotonic()-started)*1000,2)
    text('Search games','');wait(lambda:find('Showing 1,000 of 1,000') or find('Showing 1000 of 1000'))
    check('selection-visible-after-filter-reset',wait(lambda:visible('Art fixture 0999','list item')) is not None)
    search=wait(lambda:find('Search games','text'));assert search.get_component_iface().grab_focus()
    text('Search games','0001');wait(lambda:(find('Showing 1 of 1,000') or find('Showing 1 of 1000')))
    check('hidden-selection-does-not-steal-search-focus',focused_name('Search games') and scalar('SELECT display_name FROM games JOIN ui_state ON games.id=ui_state.selected_game')=='Art fixture 0999')
    text('Search games','');wait(lambda:find('Showing 1,000 of 1,000') or find('Showing 1000 of 1000'))
    check('selection-visible-after-hidden-filter',wait(lambda:visible('Art fixture 0999','list item')) is not None and focused_name('Search games'))
    item=wait(lambda:find('Art fixture 0999','list item'))
    component=item.get_component_iface();assert component is not None and component.grab_focus()
    started=time.monotonic()
    for _ in range(20):qt('key',sequence='Left')  # the grid moves one entry left/right
    report['metrics']['twenty_selection_roundtrip_ms']=round((time.monotonic()-started)*1000,2)
    check('rapid-selection-remains-responsive',wait(lambda:find('Art fixture 0979','label')) and qt('state')['width']>0 and scalar('SELECT display_name FROM games JOIN ui_state ON games.id=ui_state.selected_game')=='Art fixture 0979')
    check('selected-artwork-remains-visible',wait(lambda:find('Local artwork selected.','label')))
    check('latest-selection-uses-correct-image',wait(lambda:(value if (value:=qt('state')).get('artworkSource','').endswith('/0979.png') and value.get('artworkStatus')==1 else None)))
    check('name-order-moves-back-from-last',scalar("SELECT display_name FROM games JOIN ui_state ON games.id=ui_state.selected_game")=='Art fixture 0979')
    image=directory/'0979.png';original=image.read_bytes()
    open_game('Art fixture 0979')
    press('Add favorite');wait(lambda:scalar("SELECT favorite FROM games WHERE display_name='Art fixture 0979'")==1)
    press('Favorites');wait(lambda:find('Showing 1 of 1,000') or find('Showing 1 of 1000'))
    check('favorite-view-keeps-artwork',find('Art fixture 0979','list item') is not None and find('Local artwork selected.','label') is not None)
    restart()
    report['metrics']['restart_selected']=scalar('SELECT display_name FROM games JOIN ui_state ON games.id=ui_state.selected_game')
    report['metrics']['restart_favorite']=scalar("SELECT favorite FROM games WHERE display_name='Art fixture 0979'")
    check('favorite-persists-restart',report['metrics']['restart_favorite']==1 and wait(lambda:find('Remove favorite','button'),timeout=15) is not None and scalar('SELECT path FROM local_artwork JOIN games ON games.id=local_artwork.game_id WHERE display_name=\'Art fixture 0979\'')==os.fsencode(image))
    open_game('Art fixture 0979')
    for _ in range(60):
        if focused_name('Clear artwork'):break
        qt('key',sequence='Tab')
    check('keyboard-focuses-clear',focused_name('Clear artwork'))
    qt('key',sequence='Space');wait(lambda:scalar("SELECT count(*) FROM local_artwork JOIN games ON games.id=local_artwork.game_id WHERE display_name='Art fixture 0979'")==0)
    check('clear-preserves-file',image.read_bytes()==original)
    check('clear-shows-no-artwork',wait(lambda:find('No local artwork selected.','label'),timeout=15) is not None)
    restart();check('cleared-artwork-persists-restart',scalar("SELECT count(*) FROM local_artwork JOIN games ON games.id=local_artwork.game_id WHERE display_name='Art fixture 0979'")==0 and scalar("SELECT favorite FROM games WHERE display_name='Art fixture 0979'")==1 and wait(lambda:find('No local artwork selected.','label'),timeout=15) is not None)
    status=Path(f'/proc/{children[-1].pid}/status').read_text()
    report['metrics']['rss_kib']=int(next(line.split()[1] for line in status.splitlines() if line.startswith('VmRSS:')))
    report['passed']=True
except Exception as error:
    report['passed']=False;report['error']=str(error);report['tree']=[]
    for node in nodes():
        try:report['tree'].append({'name':node.get_name(),'role':node.get_role_name()})
        except Exception:pass
    raise
finally:(ROOT/'report.json').write_text(json.dumps(report,indent=2))
