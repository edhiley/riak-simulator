#!/usr/bin/env python3
"""Ask a real Riak node for preflists over Protocol Buffers (message 33).

    validation/pb-preflist.py HOST:PORT FIXTURE

For every row of the hash fixture, compares the fixture's ring-8 preflist with
what the node returns, as partition numbers. The node must have ring_size 8.
Exits non-zero on any mismatch.

Rows for the type `default` are compared with the untyped row for the same
bucket and key: the client APIs treat type `default` as no type, so a request
for it is placed by the hash of the plain bucket, not of {<<"default">>, B}.
Empty keys are skipped; the API rejects them.
"""
import socket, struct, sys

def varint(n):
    out = b''
    while True:
        b, n = n & 0x7f, n >> 7
        if n:
            out += bytes([b | 0x80])
        else:
            return out + bytes([b])

def field(tag, val):
    if isinstance(val, int):
        return varint(tag << 3) + varint(val)
    return varint(tag << 3 | 2) + varint(len(val)) + val

def decode(b):
    i, out = 0, []
    def rv():
        nonlocal i
        n = s = 0
        while True:
            c = b[i]; i += 1; n |= (c & 0x7f) << s; s += 7
            if not c & 0x80:
                return n
    while i < len(b):
        k = rv(); t, w = k >> 3, k & 7
        if w == 0:
            out.append((t, rv()))
        else:
            l = rv(); out.append((t, b[i:i + l])); i += l
    return out

def recv(s, n):
    data = b''
    while len(data) < n:
        data += s.recv(n - len(data))
    return data

host, port = sys.argv[1].split(':')
sock = socket.create_connection((host, int(port)))
inc = 2 ** 160 // 8
bad = rows = 0
lines = [l.rstrip('\n').split('\t') for l in open(sys.argv[2]) if not l.startswith('#')]
untyped = {(b, k): ring8 for t, b, k, _, ring8, _ in lines if not t}
for t, b, k, _, ring8, _ in lines:
    if not k:
        continue
    if t == b'default'.hex():
        ring8 = untyped[(b, k)]
    req = field(1, bytes.fromhex(b)) + field(2, bytes.fromhex(k))
    if t:
        req += field(3, bytes.fromhex(t))
    sock.sendall(struct.pack('>IB', len(req) + 1, 33) + req)
    length, code = struct.unpack('>IB', recv(sock, 5))
    body = recv(sock, length - 1)
    if code != 34:
        print('error', t, b, k, body); bad += 1; continue
    got = [dict(decode(v))[1] for tag, v in decode(body) if tag == 1]
    want = [int(p) // inc for p in ring8.split(',')]
    rows += 1
    if got != want:
        bad += 1
        print('mismatch', bytes.fromhex(t), bytes.fromhex(b), bytes.fromhex(k), 'node', got, 'fixture', want)
print(rows, 'rows,', bad, 'mismatches')
sys.exit(1 if bad else 0)
