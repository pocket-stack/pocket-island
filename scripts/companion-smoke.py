"""Exercise the production daemon with two isolated TCP device fixtures.

This checks the host transport/process/authority path. It is not a hardware
or rendering test. Fixture keys and listening sockets are owned by this run.
"""
import socket, struct, subprocess, threading, time, os, json
from pathlib import Path
root = Path(__file__).resolve().parent.parent
(root / 'dist/multiplayer').mkdir(parents=True, exist_ok=True)
hosts = ['127.0.0.1', '127.0.0.1']
keys = ['a1' * 32, 'b2' * 32]
paths = []
endpoints = []
hash = 14695981039346656037
for name in ['src/motion.rs', 'src/lib.rs', 'src/net.rs', 'assets/layout.rs', 'assets/mira.p3m']:
    for b in (root / name).read_bytes():
        hash = (hash ^ b) * 1099511628211 & (1 << 64) - 1

def encode(kind, payload):
    return (b'PI' + bytes([1, kind]) + payload).hex().encode()

def send(s, p):
    s.sendall(struct.pack('>I', len(p)) + p)

def exact(s, n):
    out = b''
    while len(out) < n:
        data = s.recv(n - len(out))
        if not data:
            raise EOFError()
        out += data
    return out

def packet(s):
    size = struct.unpack('>I', exact(s, 4))[0]
    assert 0 < size <= 4096
    return bytes.fromhex(exact(s, size).decode())
listeners = []
connected = []
proc = None
try:
    for h, k in zip(hosts, keys):
        s = socket.socket()
        s.bind((h, 0))
        s.listen(1)
        s.settimeout(60)
        listeners.append(s)
        port = s.getsockname()[1]
        endpoints.append(f'{h}:{port}')
        p = root / '.pocket/3ds/devices' / f'{h}-{port}.key'
        assert not p.exists()
        p.parent.mkdir(parents=True, exist_ok=True)
        paths.append(p)
        p.write_text(k + '\n')
        p.chmod(384)
    log = (root / 'dist/multiplayer/companion-smoke.log').open('w')
    proc = subprocess.Popen(['bun', 'scripts/companion.ts', *endpoints], cwd=root, stdout=log, stderr=log)
    epochs = []
    starts = []
    latest = [None, None]
    for i, listener in enumerate(listeners):
        s, _ = listener.accept()
        s.settimeout(10)
        connected.append(s)
        assert exact(s, 64).decode() == keys[i]
        send(s, encode(1, struct.pack('<IQ', 1, hash)))
        welcome = packet(s)
        assert welcome[:4] == b'PI\x01\x02'
        nonce, epoch, player, tick = struct.unpack_from('<IQBQ', welcome, 4)
        assert nonce == 1 and player == i + 1
        epochs.append(epoch)
        starts.append(struct.unpack_from('<fff', welcome, 33))

    def receive(i):
        try:
            while True:
                p = packet(connected[i])
                if p[:4] == b'PI\x01\x04':
                    latest[i] = p
        except (OSError, EOFError):
            pass
    threads = [threading.Thread(target=receive, args=(i,), daemon=True) for i in range(2)]
    for t in threads:
        t.start()
    for seq in range(1, 61):
        for i in range(2):
            cmd = struct.pack('<hhBB', 0, -10000 if i == 0 else 10000, 0, i)
            send(connected[i], encode(3, struct.pack('<QQB', epochs[i], seq, 1) + cmd))
        time.sleep(1 / 30)
    time.sleep(0.4)
    receipts = []
    for i, p in enumerate(latest):
        assert p is not None
        epoch, tick, ack = struct.unpack_from('<QQQ', p, 4)
        own = struct.unpack_from('<fff', p, 36)
        remote = struct.unpack_from('<fff', p, 75)
        other = struct.unpack_from('<fff', latest[1 - i], 36)
        assert epoch == epochs[i] and ack == 60
        assert abs(own[2] - starts[i][2]) > 0.5
        assert all((abs(x - y) < 1e-06 for x, y in zip(remote, other)))
        receipts.append({'player': i + 1, 'ack': ack, 'position': own, 'remote': remote})
    connected[0].shutdown(socket.SHUT_RDWR)
    connected[0].close()
    threads[0].join(1)
    s, _ = listeners[0].accept()
    s.settimeout(10)
    assert exact(s, 64).decode() == keys[0]
    send(s, encode(1, struct.pack('<IQ', 2, hash)))
    welcome = packet(s)
    assert welcome[:4] == b'PI\x01\x02'
    nonce, epoch, player, tick = struct.unpack_from('<IQBQ', welcome, 4)
    assert nonce == 2 and epoch != epochs[0] and (player == 1)
    assert struct.unpack_from('<fff', welcome, 33) == receipts[0]['position']
    s.close()
    result = {'passed': True, 'transport': 'two loopback TCP peers through production Bun daemon and Rust authority', 'movement': receipts, 'reconnectPreservesPosition': True, 'hardware': False}
    (root / 'dist/multiplayer/companion-smoke.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result))
finally:
    if proc:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
    for s in connected + listeners:
        s.close()
    for p in paths:
        p.unlink(missing_ok=True)
