import argparse
from pathlib import Path
p=argparse.ArgumentParser(); p.add_argument('url'); p.add_argument('--destination'); p.add_argument('--yt'); args=p.parse_args()
folder=Path(args.destination)/'sample-source'; folder.mkdir(exist_ok=True)
(folder/'fake-photo [12345678] - 01.jpg').write_bytes(b'fake-photo')
print('Downloaded one fake photo')
