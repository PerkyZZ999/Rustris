#!/usr/bin/env python3
"""Send one agent-cu primitive to an owned isolated desktop; return its real output."""
import json
from pathlib import Path
import sys
import time
import uuid
root=Path(sys.argv[1]).resolve()
request={'id':str(uuid.uuid4()),'argv':sys.argv[2:]}
temp=root/'request.tmp';temp.write_text(json.dumps(request));temp.rename(root/'request.json')
deadline=time.monotonic()+25
while time.monotonic()<deadline:
    path=root/'response.json'
    if path.exists():
        result=json.loads(path.read_text())
        if result['id']==request['id']:
            print(result['stdout'],end='')
            if result['stderr']:print(result['stderr'],file=sys.stderr,end='')
            raise SystemExit(result['code'])
    time.sleep(0.05)
raise SystemExit('Private desktop command timed out')
