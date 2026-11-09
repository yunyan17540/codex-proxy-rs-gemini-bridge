"""Restore the pinned host binary; build the repaired Google module from included source."""
import argparse
import hashlib
from io import BytesIO
from pathlib import Path
import urllib.request
from zipfile import ZipFile

ASSETS = [
    ('https://github.com/router-for-me/CLIProxyAPI/releases/download/v8.0.60/CLIProxyAPI_8.0.60_windows_amd64.zip',
     'c81a2c51aeab819c6e4840ad62f8d8f4143706b9fa7682f4a769683e0044fff1',
     'cli-proxy-api.exe', 'engine/cli-proxy-api.exe'),
]

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--proxy',default='http://127.0.0.1:7890',help='HTTP proxy; pass an empty string for direct')
    args=parser.parse_args()
    opener=urllib.request.build_opener(urllib.request.ProxyHandler({'http':args.proxy,'https':args.proxy} if args.proxy else {}))
    root=Path(__file__).resolve().parent
    for url,digest,entry,target in ASSETS:
        with opener.open(url,timeout=60) as response:
            content=response.read(64*1024*1024+1)
        if len(content)>64*1024*1024 or hashlib.sha256(content).hexdigest()!=digest:
            raise RuntimeError('Official archive SHA-256 mismatch: '+entry)
        with ZipFile(BytesIO(content)) as archive:
            # Read only the expected entry; never extract paths supplied by an archive.
            info=archive.getinfo(entry)
            if info.file_size>128*1024*1024: raise RuntimeError('Unexpected binary size')
            data=archive.read(info)
        output=root/target
        output.parent.mkdir(parents=True,exist_ok=True)
        output.write_bytes(data)
        print('Verified and restored '+target)

if __name__=='__main__':main()
