#!/usr/bin/env python3
"""Install the built Island app and per-app pairing key through ftpd.
Uploads are verified before activation. Existing ROMs and keys are preserved.
"""
import datetime
import ftplib
import hashlib
import io
import json
from pathlib import Path
import re
import secrets
import sys

ROOT = Path(__file__).resolve().parent.parent
APP = 'pocket-island'
KEY = '/pocketjs/offload/' + hashlib.sha256(APP.encode()).hexdigest()[:16] + '.key'


def read_optional(ftp, path):
    out = io.BytesIO()
    try:
        ftp.retrbinary('RETR ' + path, out.write)
    except (ftplib.error_perm, ftplib.error_temp) as error:
        message = str(error).lower()
        if 'no such file' in message or 'not found' in message:
            return None
        raise
    return out.getvalue()


def directory(ftp, path):
    try:
        ftp.mkd(path)
    except (ftplib.error_perm, ftplib.error_temp):
        ftp.cwd(path)  # Require an existing accessible directory.


def put_verified(ftp, path, data):
    ftp.storbinary('STOR ' + path, io.BytesIO(data), blocksize=65536)
    if read_optional(ftp, path) != data:
        raise RuntimeError('FTP readback mismatch: ' + path)


def pairing(ftp, host, remote, port):
    key = read_optional(ftp, remote)
    if key is None:
        parent = ''
        for part in remote.strip('/').split('/')[:-1]:
            parent += '/' + part
            directory(ftp, parent)
        key = (secrets.token_hex(32) + '\n').encode()
        put_verified(ftp, remote, key)
    if not re.fullmatch(b'[0-9a-f]{64}', key.strip()):
        raise RuntimeError('Existing pairing key has invalid format; retained: ' + remote)
    local = ROOT / '.pocket/3ds/devices' / f'{host}-{port}.key'
    local.parent.mkdir(parents=True, exist_ok=True)
    local.touch(mode=0o600, exist_ok=True)
    local.chmod(0o600)
    local.write_bytes(key)


def upload(host, data):
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ')
    receipt_dir = ROOT / 'dist/multiplayer'
    receipt_dir.mkdir(parents=True, exist_ok=True)
    with ftplib.FTP() as ftp:
        ftp.connect(host, 5000, timeout=20)
        ftp.login()
        bases = ftp.nlst('/')
        base = next((b for b in bases if b.rstrip('/').split('/')[-1].lower() == '3ds'), '/3ds')
        base = '/' + base.strip('/')
        directory(ftp, base)
        app = base + '/' + APP
        directory(ftp, app)
        target = app + '/pocket-island.3dsx'
        previous = read_optional(ftp, target)
        if previous is not None:
            (receipt_dir / f'{host}-{stamp}-previous.3dsx').write_bytes(previous)
        pending = target + '.upload-' + stamp
        put_verified(ftp, pending, data)
        # Pair before activation so a failed key install leaves the old ROM.
        pairing(ftp, host, KEY, 8741)
        pairing(ftp, host, '/pocketjs/runtime/dev.key', 8131)
        backup = target + '.bak-' + stamp if previous is not None else None
        if backup:
            ftp.rename(target, backup)
        try:
            ftp.rename(pending, target)
        except Exception:
            if backup:
                ftp.rename(backup, target)
            raise
        if read_optional(ftp, target) != data:
            raise RuntimeError('Activated ROM readback mismatch')
        receipt = dict(host=host, path=target, bytes=len(data), sha256=hashlib.sha256(data).hexdigest(),
                       readback='byte-exact', backup=backup, uploaded_at=stamp, paired_ports=[8131, 8741])
        with (receipt_dir / 'uploads.jsonl').open('a') as f:
            f.write(json.dumps(receipt) + '\n')
        print(json.dumps(receipt), flush=True)


if __name__ == '__main__':
    if len(sys.argv) < 2:
        raise SystemExit('Usage: bun island upload <device-ip> [<device-ip> ...]')
    artifact = (ROOT / 'dist/island/release/pocket-island.3dsx').read_bytes()
    for address in sys.argv[1:]:
        upload(address, artifact)
